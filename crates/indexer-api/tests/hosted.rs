//! F6 part C: what the hosted API adds for testnet-10 (written before the server code):
//! per-client limits (a client cannot pick its own bucket with a header, unless the peer is
//! the local reverse proxy), the anchor-proof cache in front of the node (one build at a
//! time, results kept, a slow build never cancelled by a client), `/health`, the new flags,
//! and the per-client limit on a real socket (the peer address reaches the guard).
//!
//! Run: `cargo test -j 4 -p indexer-api --test hosted`. The one-process mode against a real
//! node is `simnet follow-check` (crates/simnet, `dev.rs` `follow_check`).

mod common;

use std::future::Future;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::body::Body;
use common::*;
use indexer_api::limits::{CachedAnchor, ClientIp, ClientLimiter, Peer, bucket_of, client_of};
use indexer_api::{AnchorProvider, FollowHealth};
use web::indexer::*;

fn peer(ip: &str) -> Peer {
    Peer(SocketAddr::new(ip.parse().unwrap(), 50_000))
}

fn req_from(uri: &str, p: Option<Peer>, xff: Option<&str>) -> axum::http::Request<Body> {
    let mut b = axum::http::Request::get(uri);
    if let Some(x) = xff {
        b = b.header("x-forwarded-for", x);
    }
    let mut r = b.body(Body::empty()).unwrap();
    if let Some(p) = p {
        r.extensions_mut().insert(p);
    }
    r
}

async fn status_of(a: &axum::Router, uri: &str, p: Option<Peer>, xff: Option<&str>) -> (u16, axum::http::HeaderMap, String) {
    request(a, req_from(uri, p, xff)).await
}

/// The bucket: `burst` at once, then refilled at `rate` a second; IPv6 by /64; v4-mapped as v4.
#[test]
fn token_bucket_and_keys() {
    let l = ClientLimiter::new(3, 2);
    let ip: IpAddr = "203.0.113.7".parse().unwrap();
    let t0 = Instant::now();
    assert!((0..3).all(|_| l.allow_at(ip, t0)), "the burst");
    assert!(!l.allow_at(ip, t0), "over the burst");
    assert!(!l.allow_at(ip, t0 + Duration::from_millis(400)), "0.8 token after 0.4 s");
    assert!(l.allow_at(ip, t0 + Duration::from_millis(600)), "a token after 0.5 s at 2/s");
    assert!(l.allow_at("203.0.113.8".parse().unwrap(), t0), "another client has its own bucket");
    let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
    let b: IpAddr = "2001:db8:1:2:bbbb::9".parse().unwrap();
    let c: IpAddr = "2001:db8:1:3::1".parse().unwrap();
    assert_eq!(bucket_of(a), bucket_of(b), "one /64, one bucket");
    assert_ne!(bucket_of(a), bucket_of(c));
    assert_eq!(bucket_of("::ffff:203.0.113.7".parse().unwrap()), ip, "v4-mapped is its v4 address");
}

/// Which address names the client: the peer; behind a local proxy (`forwarded`) the last
/// X-Forwarded-For entry, only when the peer is loopback.
#[test]
fn client_address_rules() {
    let mut h = axum::http::HeaderMap::new();
    h.insert("x-forwarded-for", "1.1.1.1, 198.51.100.4".parse().unwrap());
    let ip = |s: &str| Some(s.parse::<IpAddr>().unwrap());
    assert_eq!(client_of(Some(peer("192.0.2.1")), &h, ClientIp::Peer), ip("192.0.2.1"), "peer mode ignores the header");
    assert_eq!(client_of(Some(peer("127.0.0.1")), &h, ClientIp::Peer), ip("127.0.0.1"));
    assert_eq!(client_of(Some(peer("127.0.0.1")), &h, ClientIp::Forwarded), ip("198.51.100.4"), "the proxy's appended entry, not the client's own");
    assert_eq!(client_of(Some(peer("::1")), &h, ClientIp::Forwarded), ip("198.51.100.4"));
    assert_eq!(client_of(Some(peer("192.0.2.1")), &h, ClientIp::Forwarded), ip("192.0.2.1"), "a header from a non-local peer is ignored");
    let mut bad = axum::http::HeaderMap::new();
    bad.insert("x-forwarded-for", "not-an-ip".parse().unwrap());
    assert_eq!(client_of(Some(peer("127.0.0.1")), &bad, ClientIp::Forwarded), ip("127.0.0.1"), "an unparsable entry: the peer");
    assert_eq!(client_of(None, &h, ClientIp::Forwarded), None, "no peer, no limit");
}

/// The guard: over the limit is `busy` (503) with Retry-After; other clients are served; a
/// spoofed header from a remote peer does not give a fresh bucket; behind the local proxy each
/// forwarded client has its own; the limit off serves everything (negative control).
#[tokio::test]
async fn per_client_limit_in_the_router() {
    let f = fixture("hosted-limit");
    let mut cfg = f.cfg.clone();
    cfg.per_client = Some((3, 1));
    let a = app(&cfg);
    let p1 = Some(peer("192.0.2.10"));
    for i in 0..3 {
        assert_eq!(status_of(&a, "/v1/status", p1, None).await.0, 200, "request {i}");
    }
    let (s, h, b) = status_of(&a, "/v1/status", p1, None).await;
    assert_eq!(s, 503, "{b}");
    assert_eq!(parse_error(&b).unwrap().error.code, ApiErrorCode::Busy);
    assert_eq!(h["retry-after"], "1");
    for x in ["5.5.5.5", "6.6.6.6", "7.7.7.7"] {
        assert_eq!(status_of(&a, "/v1/status", p1, Some(x)).await.0, 503, "a remote peer's X-Forwarded-For {x} is not its key");
    }
    assert_eq!(status_of(&a, "/v1/status", Some(peer("192.0.2.11")), None).await.0, 200, "another client");
    assert_eq!(status_of(&a, "/v1/status", None, None).await.0, 200, "no peer: not limited here");

    cfg.client_ip = ClientIp::Forwarded;
    let b = app(&cfg);
    let lo = Some(peer("127.0.0.1"));
    for _ in 0..3 {
        assert_eq!(status_of(&b, "/v1/rounds", lo, Some("198.51.100.1")).await.0, 200);
    }
    assert_eq!(status_of(&b, "/v1/rounds", lo, Some("198.51.100.1")).await.0, 503, "forwarded client 1 over its limit");
    assert_eq!(status_of(&b, "/v1/rounds", lo, Some("198.51.100.2")).await.0, 200, "forwarded client 2 has its own bucket");
    let remote = Some(peer("192.0.2.20"));
    for _ in 0..3 {
        assert_eq!(status_of(&b, "/v1/rounds", remote, Some("198.51.100.9")).await.0, 200);
    }
    for x in ["198.51.100.3", "198.51.100.4"] {
        assert_eq!(status_of(&b, "/v1/rounds", remote, Some(x)).await.0, 503, "forwarded mode: a remote peer's own X-Forwarded-For {x} is not its key");
    }

    cfg.per_client = None;
    let c = app(&cfg);
    for _ in 0..20 {
        assert_eq!(status_of(&c, "/v1/status", p1, None).await.0, 200, "negative control: no limit configured");
    }
}

/// A provider that counts its builds, takes `delay`, and answers `answer`.
struct Mock {
    builds: AtomicU64,
    delay: Duration,
    answer: Result<Vec<u8>, String>,
}

impl AnchorProvider for Mock {
    fn proof(&self, _round: i64, _d: u64, _m: u64) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + '_>> {
        Box::pin(async move {
            self.builds.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.answer.clone()
        })
    }
}

fn proof_bytes() -> Vec<u8> {
    (0..harness::ANCHOR_LEN).map(|i| i as u8).collect()
}

/// The cache in front of the node, through the endpoint (round 2 is closing in the fixture):
/// 16 concurrent requests make one build and get one proof; within the TTL no new build; a
/// failure is kept only for its short TTL; a build slower than the wait answers "being built"
/// and is not cancelled: the next request gets its proof without another build.
#[tokio::test]
async fn anchor_cache_bounds_node_work() {
    let f = fixture("hosted-anchor");
    let mock = Arc::new(Mock { builds: 0.into(), delay: Duration::from_millis(200), answer: Ok(proof_bytes()) });
    let cache = Arc::new(CachedAnchor::new(mock.clone(), Duration::from_secs(60), Duration::from_millis(300), Duration::from_secs(5)));
    let mut cfg = f.cfg.clone();
    cfg.anchor = Some(cache.clone());
    let a = app(&cfg);
    let hs: Vec<_> = (0..16).map(|_| {
        let a = a.clone();
        tokio::spawn(async move { get(&a, "/v1/rounds/2/anchor-proof").await })
    }).collect();
    let mut bodies = vec![];
    for h in hs {
        let (s, _, b) = h.await.unwrap();
        assert_eq!(s, 200, "{b}");
        bodies.push(b);
    }
    assert!(bodies.iter().all(|b| *b == bodies[0]));
    let p = parse_anchor_proof(&bodies[0]).unwrap();
    assert_eq!((p.source, p.anchor_proof.len()), (ProofSource::Node, 2 * harness::ANCHOR_LEN));
    assert_eq!(mock.builds.load(Ordering::SeqCst), 1, "16 concurrent requests, one node build");
    for _ in 0..5 {
        assert_eq!(get(&a, "/v1/rounds/2/anchor-proof").await.0, 200);
    }
    assert_eq!(mock.builds.load(Ordering::SeqCst), 1, "kept within its TTL");
    // Rounds without a closing state never reach the provider.
    assert_eq!(get(&a, "/v1/rounds/3/anchor-proof").await.0, 404);
    assert_eq!(get(&a, "/v1/rounds/1/anchor-proof").await.0, 200, "round 1's proof is its draw's");
    assert_eq!(mock.builds.load(Ordering::SeqCst), 1);

    // A failing node: kept for the error TTL only.
    let bad = Arc::new(Mock { builds: 0.into(), delay: Duration::from_millis(10), answer: Err("node down".into()) });
    let mut cfg2 = f.cfg.clone();
    cfg2.anchor = Some(Arc::new(CachedAnchor::new(bad.clone(), Duration::from_secs(60), Duration::from_millis(300), Duration::from_secs(5))));
    let b = app(&cfg2);
    for _ in 0..5 {
        let (s, _, body) = get(&b, "/v1/rounds/2/anchor-proof").await;
        assert_eq!(s, 404);
        assert_eq!(parse_error(&body).unwrap().error.code, ApiErrorCode::AnchorUnavailable);
    }
    assert_eq!(bad.builds.load(Ordering::SeqCst), 1, "a failure is kept for its TTL");
    tokio::time::sleep(Duration::from_millis(400)).await;
    get(&b, "/v1/rounds/2/anchor-proof").await;
    assert_eq!(bad.builds.load(Ordering::SeqCst), 2, "and built again after it");

    // A build slower than the wait.
    let slow = Arc::new(Mock { builds: 0.into(), delay: Duration::from_millis(800), answer: Ok(proof_bytes()) });
    let mut cfg3 = f.cfg.clone();
    cfg3.anchor = Some(Arc::new(CachedAnchor::new(slow.clone(), Duration::from_secs(60), Duration::from_secs(1), Duration::from_millis(200))));
    let c = app(&cfg3);
    let (s, _, body) = get(&c, "/v1/rounds/2/anchor-proof").await;
    assert_eq!(s, 404);
    assert!(parse_error(&body).unwrap().error.detail.contains("being built"), "{body}");
    tokio::time::sleep(Duration::from_millis(900)).await;
    let (s, _, body) = get(&c, "/v1/rounds/2/anchor-proof").await;
    assert_eq!(s, 200, "{body}");
    assert_eq!(slow.builds.load(Ordering::SeqCst), 1, "the first build finished although its client gave up");
}

/// `/health`: 200 for a store that applies; in the one-process mode 503 until the node was
/// read, 200 after, 503 again once the last good read is older than the limit.
#[tokio::test]
async fn health() {
    let f = fixture("hosted-health");
    let a = app(&f.cfg);
    let (s, h, b) = get(&a, "/health").await;
    assert_eq!((s, h["content-type"].to_str().unwrap()), (200, "application/json"), "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!((v["ok"].as_bool(), v["following"].as_bool()), (Some(true), Some(false)));

    let fo = Arc::new(FollowHealth::new(Duration::from_millis(300)));
    let snap = indexer_api::store::Follower::open(&f.cfg.store, &f.cfg.bundle, f.cfg.cov).unwrap().snapshot();
    let mut st = indexer_api::AppState::new(snap, &f.cfg);
    st.follow = Some(fo.clone());
    let r = indexer_api::router(st, &[]).unwrap();
    assert_eq!(get(&r, "/health").await.0, 200, "within the limit after start");
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (s, _, b) = get(&r, "/health").await;
    assert_eq!(s, 503, "never read the node: {b}");
    assert!(b.contains("no successful poll yet"), "{b}");
    fo.err("get_virtual_chain_from_block_v2: timed out".into(), false);
    let (_, _, b) = get(&r, "/health").await;
    assert!(b.contains("timed out"), "the reason: {b}");
    fo.ok(1_000);
    assert_eq!(get(&r, "/health").await.0, 200, "after a good poll");
    fo.err("cannot find header e7df…; below its retention root".into(), true);
    let (s, _, b) = get(&r, "/health").await;
    assert_eq!(s, 503, "a pruned resume block is 503 at once, not after the staleness limit: {b}");
    assert!(b.contains("cannot continue") && b.contains("retention root"), "{b}");
    fo.err("connection reset by peer".into(), false);
    assert_eq!(get(&r, "/health").await.0, 200, "a retryable error within the limit stays 200");
    fo.ok(1_000);
    let (s, _, b) = get(&r, "/health").await;
    assert_eq!(s, 200, "{b}");
    assert!(b.contains("\"nodeReadMsAgo\""), "{b}");
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(get(&r, "/health").await.0, 503, "stale again");
}

fn parse(args: &[&str]) -> Result<indexer_api::Config, String> {
    let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    indexer_api::config::parse(&a, |_| None, |_, _| Ok(false))
}

/// The new flags: the node URL (refused on a mainnet port or a non-WebSocket scheme), the
/// resolver, `--start` only with `--node`, the per-client limit and its `off`, the client
/// address mode, and the defaults (a per-client limit on, peer addresses, no follower).
#[test]
fn flags() {
    let cov = "ab".repeat(32);
    let base = ["--store", "x", "--cov", cov.as_str()];
    let with = |more: &[&str]| parse(&[&base[..], more].concat());
    let d = with(&[]).unwrap();
    assert_eq!((d.per_client, d.client_ip, d.follow.is_none()), (Some((120, 20)), ClientIp::Peer, true));
    let r = with(&["--node", "resolver"]).unwrap().follow.unwrap();
    assert_eq!((r.node, r.start, r.every), (None, None, Duration::from_secs(10)));
    let n = with(&["--node", "wss://node.example/kaspa/testnet-10/wrpc/borsh", "--start", &"cd".repeat(32), "--node-poll-ms", "5000"]).unwrap().follow.unwrap();
    assert_eq!((n.node.as_deref(), n.start.is_some(), n.every), (Some("wss://node.example/kaspa/testnet-10/wrpc/borsh"), true, Duration::from_secs(5)));
    for bad in ["ws://127.0.0.1:17110", "wss://x:16110", "wss://mainnet.example/wrpc", "http://node.example", "node.example"] {
        assert!(with(&["--node", bad]).is_err(), "{bad} accepted");
    }
    assert!(with(&["--start", &"cd".repeat(32)]).err().unwrap().contains("--start needs --node"));
    assert!(with(&["--node", "resolver", "--start", "zz"]).is_err());
    assert_eq!(with(&["--per-client-rate", "off"]).unwrap().per_client, None);
    assert_eq!(with(&["--per-client-burst", "30", "--per-client-rate", "5"]).unwrap().per_client, Some((30, 5)));
    assert_eq!(with(&["--client-ip", "forwarded"]).unwrap().client_ip, ClientIp::Forwarded);
    assert!(with(&["--client-ip", "header"]).is_err());
    assert!(with(&["--per-client-rate", "0"]).is_err());
    assert_eq!(with(&["--stale-ms", "5000"]).unwrap().stale, Duration::from_secs(5));
    let e = indexer_api::config::parse(&base.map(String::from), |k| (k == "INDEXER_API_NODE").then(|| "resolver".to_string()), |_, _| Ok(false)).unwrap();
    assert!(e.follow.is_some(), "INDEXER_API_NODE");
}

fn raw_get(addr: SocketAddr, path: &str) -> String {
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

/// On a real socket the peer address reaches the guard: a burst of 2 serves two requests and
/// refuses the third from the same address.
#[test]
fn per_client_limit_on_a_real_socket() {
    let f = fixture("hosted-socket");
    let mut cfg = f.cfg.clone();
    cfg.per_client = Some((2, 1));
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let addr = listener.local_addr().unwrap();
    let st = indexer_api::open(&cfg).unwrap();
    let router = indexer_api::router(st, &[]).unwrap();
    rt.spawn(async move { indexer_api::serve(listener, router, &cfg).await });
    assert!(raw_get(addr, "/v1/status").starts_with("HTTP/1.1 200"));
    assert!(raw_get(addr, "/v1/status").starts_with("HTTP/1.1 200"));
    let r = raw_get(addr, "/v1/status");
    assert!(r.starts_with("HTTP/1.1 503") && r.to_ascii_lowercase().contains("retry-after: 1") && r.contains("\"busy\""), "{r}");
}
