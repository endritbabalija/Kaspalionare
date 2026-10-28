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
//! - **Per-client limits** (F6 part C, `limits`): a token bucket per client address (the
//!   peer, or behind a local reverse proxy its `X-Forwarded-For`); over it, `busy`. F6 security
//!   review S1: at most `per_client_inflight` requests of one client at once (below the global
//!   `max_requests`), and an anchor-proof request waits for its build on a permit of its own
//!   pool (`max_anchor_waits`), never on one of the global permits: a client holding many slow
//!   anchor-proof requests during a draw leaves the other endpoints and other clients served.
//! - **Health** (F6 part C): `GET /health`, 200 or 503, for a process supervisor or an uptime
//!   check; outside `/v1/` (the page never reads it). F6 security review: a remote client gets
//!   only `{"ok":...}`; the detail (store and node errors, which can name paths and the node's
//!   URL) only on loopback without a forwarded client, and in the log.
//!   F6 part E: `/v1/status`'s `error` is the fixed word `store_error` for the same clients;
//!   the store's error text only on loopback.
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
use crate::limits::{ClientIp, ClientLimiter, InFlight, Peer, client_of};
use crate::routes;
use crate::store::{Follower, Snapshot};

/// A source of anchor proofs before the draw (spec 6.1 fallback): only a process that
/// follows a node (the simnet dev run, an indexer with a node) can build one. `round`, D and
/// the marker's creation DAA score identify what to build.
pub trait AnchorProvider: Send + Sync + 'static {
    fn proof(&self, round: i64, d: u64, marker_daa: u64) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + '_>>;
}

/// The follower's liveness in the one-process mode (`follow`): the last successful poll of
/// the node, and the last error since.
pub struct FollowHealth {
    last_ok_ms: std::sync::atomic::AtomicU64,
    last_daa: std::sync::atomic::AtomicU64,
    error: std::sync::Mutex<Option<String>>,
    /// The last error is one a retry cannot fix (a pruned or unknown resume block, an index error).
    fatal: std::sync::atomic::AtomicBool,
    started_ms: u64,
    stale: Duration,
    /// The followed node and the witness (shown only in `/health`'s local detail).
    nodes: std::sync::Mutex<(Option<String>, Option<String>)>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl FollowHealth {
    pub fn new(stale: Duration) -> Self {
        FollowHealth { last_ok_ms: 0.into(), last_daa: 0.into(), error: Default::default(), fatal: false.into(), started_ms: now_ms(), stale, nodes: Default::default() }
    }
    pub fn set_nodes(&self, node: String, witness: Option<String>) {
        if let Ok(mut g) = self.nodes.lock() {
            *g = (Some(node), witness);
        }
    }
    /// (the followed node, the witness).
    pub fn nodes(&self) -> (Option<String>, Option<String>) {
        self.nodes.lock().map(|g| g.clone()).unwrap_or_default()
    }
    pub fn ok(&self, daa: u64) {
        self.last_ok_ms.store(now_ms(), std::sync::atomic::Ordering::SeqCst);
        self.last_daa.store(daa, std::sync::atomic::Ordering::SeqCst);
        if let Ok(mut e) = self.error.lock() {
            *e = None;
        }
        self.fatal.store(false, std::sync::atomic::Ordering::SeqCst);
    }
    /// A failed poll; `fatal`: retrying cannot fix it (`/health` is 503 at once, not only
    /// after the staleness limit).
    pub fn err(&self, e: String, fatal: bool) {
        if let Ok(mut g) = self.error.lock() {
            *g = Some(e);
        }
        self.fatal.store(fatal, std::sync::atomic::Ordering::SeqCst);
    }
    /// (healthy, the reason when not, ms since the last good poll).
    pub fn state_at(&self, now: u64) -> (bool, Option<String>, Option<u64>) {
        let last = self.last_ok_ms.load(std::sync::atomic::Ordering::SeqCst);
        let ago = (last > 0).then(|| now.saturating_sub(last));
        let err = self.error.lock().ok().and_then(|g| g.clone());
        let since = ago.unwrap_or(now.saturating_sub(self.started_ms));
        if self.fatal.load(std::sync::atomic::Ordering::SeqCst) {
            return (false, Some(format!("the follower cannot continue without a fix: {}", err.unwrap_or_default())), ago);
        }
        if since > self.stale.as_millis() as u64 {
            let why = err.unwrap_or_else(|| "no successful poll yet".into());
            return (false, Some(format!("the node was last read {} s ago: {why}", since / 1000)), ago);
        }
        (true, err, ago)
    }
}

/// What every handler reads.
#[derive(Clone)]
pub struct AppState {
    snap: Arc<RwLock<Arc<Snapshot>>>,
    pub anchor: Option<Arc<dyn AnchorProvider>>,
    busy: Arc<Semaphore>,
    /// Anchor-proof requests' own permits (S1: their waits never hold a global one).
    anchor_busy: Arc<Semaphore>,
    timeout: Duration,
    /// Per-client token buckets (`None`: off).
    pub limiter: Option<Arc<ClientLimiter>>,
    /// Per-client requests in flight (on with the per-client limit).
    pub inflight: Option<Arc<InFlight>>,
    pub client_ip: ClientIp,
    /// The one-process mode's follower (`None` in the store-only mode).
    pub follow: Option<Arc<FollowHealth>>,
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
            anchor_busy: Arc::new(Semaphore::new(cfg.max_anchor_waits)),
            timeout: cfg.request_timeout,
            limiter: cfg.per_client.map(|(burst, rate)| Arc::new(ClientLimiter::new(burst, rate))),
            inflight: cfg.per_client.map(|_| InFlight::new(cfg.per_client_inflight)),
            client_ip: cfg.client_ip,
            follow: None,
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
    let client = client_of(req.extensions().get::<Peer>().copied(), req.headers(), st.client_ip);
    let busy_client = || {
        let mut r = routes::error(ApiErrorCode::Busy, "too many requests from this client; retry in a second");
        r.headers_mut().insert(axum::http::header::RETRY_AFTER, HeaderValue::from_static("1"));
        r
    };
    if let (Some(l), Some(ip)) = (&st.limiter, client) {
        if !l.allow(ip) {
            return busy_client();
        }
    }
    let _running = match (&st.inflight, client) {
        (Some(f), Some(ip)) => match f.enter(ip) {
            Some(g) => Some(g),
            None => return busy_client(),
        },
        _ => None,
    };
    let p = req.uri().path();
    let anchor = p.starts_with("/v1/rounds/") && p.ends_with("/anchor-proof");
    let pool = if anchor { &st.anchor_busy } else { &st.busy };
    let Ok(_permit) = pool.clone().try_acquire_owned() else {
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
        .route("/health", get(routes::health))
        .route("/v1/status", get(routes::status))
        .route("/v1/instance", get(routes::instance))
        .route("/v1/rounds", get(routes::rounds))
        .route("/v1/rounds/{r}", get(routes::round_one))
        .route("/v1/rounds/{r}/purchases", get(routes::purchases))
        .route("/v1/rounds/{r}/shards/{i}", get(routes::shard))
        .route("/v1/rounds/{r}/anchor-proof", get(routes::anchor_proof))
        .route("/v1/tickets", get(routes::tickets))
        .route("/v1/vault", get(routes::vault))
        .route("/v1/pools", get(routes::pools))
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
                        if s.error.as_deref() != Some(e.as_str()) {
                            eprintln!("indexer-api: the store does not apply: {e}");
                        }
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
        let (stream, peer): (tokio::net::TcpStream, SocketAddr) = match listener.accept().await {
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
        let svc = TowerToHyperService::new(router.clone().layer(axum::Extension(Peer(peer))));
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

/// The binary's entry point: the one-process mode with `--node` (`follow`), else the
/// store-only server (`run`).
pub async fn main_with(cfg: Config) -> Result<(), String> {
    match cfg.follow.clone() {
        Some(fo) => crate::follow::run_following(cfg, fo, crate::follow::shutdown_signal()).await,
        None => {
            tokio::select! {
                r = run(cfg) => r,
                _ = crate::follow::shutdown_signal() => { println!("indexer-api: stop signal; exiting"); Ok(()) }
            }
        }
    }
}

/// Open the store, bind and serve (the store-only binary's and an embedding process's entry point).
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
