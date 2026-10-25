//! The HTTP server: the router, its guards, and the accept loop.
//!
//! Server basics (F2):
//! - **Read-only.** GET (and HEAD, and the CORS preflight) only; any other method is
//!   `method_not_allowed`. No request body is ever read.
//! - **Version prefix** `/v1/`; anything else is `not_found`. Every error is a typed JSON
//!   body (`web::indexer::ApiErrorBody`).
//! - **CORS** for a static site on another origin: GET and HEAD from the configured origins
//!   (default: any origin; the data is public and no credentials are accepted).
//! - **Limits:** each response kind has a size cap (`web::indexer::LIMITS`, the same caps
//!   the page enforces); at most `max_requests` requests run at once (`busy` above that);
//!   each request has a deadline (`timeout`); each connection has a header-read timeout
//!   and a total lifetime; at most `max_connections` connections are open.
//! - **No keys, no transactions, no writes.** The only inputs are the store directory and
//!   the bundle file, read-only.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get;
use tokio::sync::Semaphore;
use tower_http::cors::{AllowOrigin, CorsLayer};
use web::indexer::ApiErrorCode;

use crate::config::Config;
use crate::routes;
use crate::store::{Follower, Snapshot};

/// A source of anchor proofs before the draw (spec 6.1 fallback): only a process that
/// follows a node (the simnet dev run, an indexer with a node) can build one. `round`, D and
/// the marker's creation DAA score identify what to build.
pub trait AnchorProvider: Send + Sync + 'static {
    fn proof(&self, round: i64, d: u64, marker_daa: u64) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + '_>>;
}

/// What every handler reads.
#[derive(Clone)]
pub struct AppState {
    snap: Arc<RwLock<Arc<Snapshot>>>,
    pub anchor: Option<Arc<dyn AnchorProvider>>,
    busy: Arc<Semaphore>,
    timeout: Duration,
    /// TEST-ONLY (feature `doctored`): the wrong answers to serve.
    #[cfg(feature = "doctored")]
    pub doctor: Option<crate::doctor::Mode>,
}

impl AppState {
    pub fn new(snap: Snapshot, cfg: &Config) -> Self {
        AppState {
            snap: Arc::new(RwLock::new(Arc::new(snap))),
            anchor: cfg.anchor.clone(),
            busy: Arc::new(Semaphore::new(cfg.max_requests)),
            timeout: cfg.request_timeout,
            #[cfg(feature = "doctored")]
            doctor: cfg.doctor,
        }
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snap.read().map(|s| s.clone()).unwrap_or_else(|p| p.into_inner().clone())
    }

    pub fn replace(&self, s: Snapshot) {
        match self.snap.write() {
            Ok(mut w) => *w = Arc::new(s),
            Err(p) => *p.into_inner() = Arc::new(s),
        }
    }
}

/// The per-request guards: the method, the concurrency limit and the deadline.
async fn guard(State(st): State<AppState>, req: Request, next: Next) -> Response {
    if !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return routes::error(ApiErrorCode::MethodNotAllowed, format!("{} is not served: the indexer API is read-only", req.method()));
    }
    let Ok(_permit) = st.busy.clone().try_acquire_owned() else {
        return routes::error(ApiErrorCode::Busy, "too many requests at once");
    };
    #[cfg_attr(not(feature = "doctored"), allow(unused_mut))]
    let mut r = match tokio::time::timeout(st.timeout, next.run(req)).await {
        Ok(r) => r,
        Err(_) => return routes::error(ApiErrorCode::Timeout, format!("over {} ms", st.timeout.as_millis())),
    };
    #[cfg(feature = "doctored")]
    if let Some(m) = st.doctor {
        r.headers_mut().insert("x-indexer-doctored", HeaderValue::from_static(m.name()));
    }
    r
}

/// The v1 router over `st`, with CORS for `origins` (empty: any origin).
pub fn router(st: AppState, origins: &[String]) -> Result<Router, String> {
    let allow = if origins.is_empty() {
        AllowOrigin::any()
    } else {
        let v = origins.iter().map(|o| HeaderValue::from_str(o).map_err(|e| format!("--allow-origin {o}: {e}"))).collect::<Result<Vec<_>, _>>()?;
        AllowOrigin::list(v)
    };
    let cors = CorsLayer::new().allow_origin(allow).allow_methods([Method::GET, Method::HEAD]).max_age(Duration::from_secs(86_400));
    Ok(Router::new()
        .route("/v1/status", get(routes::status))
        .route("/v1/instance", get(routes::instance))
        .route("/v1/rounds", get(routes::rounds))
        .route("/v1/rounds/{r}", get(routes::round_one))
        .route("/v1/rounds/{r}/purchases", get(routes::purchases))
        .route("/v1/rounds/{r}/shards/{i}", get(routes::shard))
        .route("/v1/rounds/{r}/anchor-proof", get(routes::anchor_proof))
        .route("/v1/tickets", get(routes::tickets))
        .route("/v1/vault", get(routes::vault))
        .route("/v1/positions", get(routes::positions))
        .route("/v1/requests", get(routes::requests))
        .fallback(routes::not_found)
        .method_not_allowed_fallback(routes::method_not_allowed)
        .layer(middleware::from_fn_with_state(st.clone(), guard))
        .layer(cors)
        .with_state(st))
}

/// Open the store and start following it (a thread polling the log every `cfg.poll`).
pub fn open(cfg: &Config) -> Result<AppState, String> {
    let mut f = Follower::open(&cfg.store, &cfg.bundle, cfg.cov)?;
    let st = AppState::new(f.snapshot(), cfg);
    let (shared, poll) = (st.clone(), cfg.poll);
    std::thread::Builder::new()
        .name("indexer-api-follow".into())
        .spawn(move || {
            loop {
                std::thread::sleep(poll);
                match f.poll() {
                    Ok(true) => shared.replace(f.snapshot()),
                    Ok(false) => {}
                    Err(e) => {
                        let mut s = (*shared.snapshot()).clone();
                        s.error = Some(e);
                        shared.replace(s);
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(st)
}

/// Serve `router` on `listener` until the process ends: each connection gets a header-read
/// timeout and a total lifetime, and at most `max_connections` are open (more are closed at
/// once).
pub async fn serve(listener: tokio::net::TcpListener, router: Router, cfg: &Config) -> Result<(), String> {
    use hyper_util::rt::{TokioIo, TokioTimer};
    use hyper_util::service::TowerToHyperService;
    let conns = Arc::new(Semaphore::new(cfg.max_connections));
    let (header_timeout, lifetime) = (cfg.header_timeout, cfg.connection_lifetime);
    loop {
        let (stream, _peer): (tokio::net::TcpStream, SocketAddr) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                eprintln!("indexer-api: accept: {e}");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let Ok(permit) = conns.clone().try_acquire_owned() else {
            drop(stream);
            continue;
        };
        let svc = TowerToHyperService::new(router.clone());
        tokio::spawn(async move {
            let conn = hyper::server::conn::http1::Builder::new()
                .timer(TokioTimer::new())
                .header_read_timeout(header_timeout)
                .keep_alive(true)
                .serve_connection(TokioIo::new(stream), svc);
            let _ = tokio::time::timeout(lifetime, conn).await;
            drop(permit);
        });
    }
}

/// Open the store, bind and serve (the binaries' and an embedding process's entry point).
pub async fn run(cfg: Config) -> Result<(), String> {
    let st = open(&cfg)?;
    let s = st.snapshot();
    let router = router(st, &cfg.allow_origins)?;
    let listener = tokio::net::TcpListener::bind(cfg.bind).await.map_err(|e| format!("bind {}: {e}", cfg.bind))?;
    println!(
        "indexer-api: http://{}/v1/ for {} instance {} ({} log lines, {} rounds, {} live objects{})",
        listener.local_addr().map_err(|e| e.to_string())?,
        s.net.name(),
        s.cov,
        s.log_lines,
        s.rounds.len(),
        s.live.len(),
        s.error.as_ref().map(|e| format!("; ERROR {e}")).unwrap_or_default()
    );
    serve(listener, router, &cfg).await
}
