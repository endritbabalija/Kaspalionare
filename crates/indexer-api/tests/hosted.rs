//! F6 part C: what the hosted API adds for testnet-10 (written before the server code):
//! per-client limits (a client cannot pick its own bucket with a header, unless the peer is
//! the local reverse proxy), the anchor-proof cache in front of the node (one build at a
//! time, results kept, a slow build never cancelled by a client), `/health`, the new flags,
//! and the per-client limit on a real socket (the peer address reaches the guard).
//!
//! F6 security review: one client cannot starve another during a draw (S1), the follower's
//! witness (S4: a lying node cannot write the store), bounded bucket work, `/health` detail only
//! on loopback, and their flags.
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
use indexer_api::follow::{Agreement, ChainView, DIVERGED, Witness, agree, apply_witnessed, host_of};
use indexer_api::limits::{BUCKETS_MAX, CachedAnchor, ClientIp, ClientLimiter, InFlight, Peer, bucket_of, client_of};
use indexer_api::node_indexer::{ChainBatch, ChainBlock, Indexer};
use kaspa_hashes::Hash;
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

// ------------------------------------------------------------------ F6 security review (S1, S4, notes)

/// S1: one client firing many anchor-proof requests while the proof is being built (a draw) cannot
/// starve another client: its requests in flight are capped (the rest `busy` at once), the waits
/// hold permits of the anchor pool only (the global pool here is 4, smaller than the client's 8
/// waits), so the other client's status and its own anchor-proof request are served. Negative
/// control: with no per-client cap (and an anchor pool of 8) the same flood takes every anchor
/// slot and the other client's anchor-proof request is refused `busy`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_client_cannot_starve_another_during_a_draw() {
    let f = fixture("hosted-starve");
    let a_peer = Some(peer("192.0.2.50"));
    let b_peer = Some(peer("198.51.100.77"));
    let scenario = |inflight: usize, anchor_waits: usize| {
        let mut cfg = f.cfg.clone();
        cfg.max_requests = 4;
        cfg.per_client = Some((100_000, 100_000));
        cfg.per_client_inflight = inflight;
        cfg.max_anchor_waits = anchor_waits;
        let slow = Arc::new(Mock { builds: 0.into(), delay: Duration::from_millis(2_500), answer: Ok(proof_bytes()) });
        cfg.anchor = Some(Arc::new(CachedAnchor::new(slow, Duration::from_secs(60), Duration::from_secs(1), Duration::from_millis(1_500))));
        app(&cfg)
    };
    for (label, inflight, anchor_waits, b_served) in [("capped", 8, 32, true), ("negative control: no per-client cap", 10_000, 8, false)] {
        let a = scenario(inflight, anchor_waits);
        let flood: Vec<_> = (0..40)
            .map(|_| {
                let a = a.clone();
                tokio::spawn(async move { status_of(&a, "/v1/rounds/2/anchor-proof", a_peer, None).await })
            })
            .collect();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let (s, _, body) = status_of(&a, "/v1/status", b_peer, None).await;
        assert_eq!(s, 200, "{label}: another client's status while the flood waits: {body}");
        let (s, _, body) = status_of(&a, "/v1/rounds/2/anchor-proof", b_peer, None).await;
        let b_busy = s == 503 && parse_error(&body).map(|e| e.error.code) == Ok(ApiErrorCode::Busy);
        assert_eq!(!b_busy, b_served, "{label}: another client's anchor-proof request: {s} {body}");
        let mut waited = 0;
        for h in flood {
            let (s, _, body) = h.await.unwrap();
            if !(s == 503 && body.contains("\"busy\"")) {
                waited += 1;
            }
        }
        println!("{label}: the flooding client got {waited} of 40 requests through, the other client served: {b_served}");
        assert!(waited <= inflight.min(anchor_waits), "{label}: {waited} waits");
    }
}

/// The per-client in-flight cap: `cap` at once per client (IPv6 by /64), released when a request ends.
#[test]
fn in_flight_cap_per_client() {
    let f = InFlight::new(2);
    let a: IpAddr = "2001:db8:5:6::1".parse().unwrap();
    let a2: IpAddr = "2001:db8:5:6::2".parse().unwrap();
    let g1 = f.enter(a).unwrap();
    let g2 = f.enter(a2).unwrap();
    assert!(f.enter(a).is_none(), "a third of one /64");
    assert!(f.enter("203.0.113.1".parse().unwrap()).is_some(), "another client");
    drop(g1);
    assert_eq!(f.running(a), 1);
    assert!(f.enter(a).is_some());
    drop(g2);
}

/// Note (bucket memory): at most `BUCKETS_MAX` buckets, the sweep at most once a second (one sweep
/// for 70,000 new addresses in the same instant), new addresses past the cap share one bucket;
/// a later sweep frees the idle buckets (negative control: without it the map would stay full).
#[test]
fn bucket_memory_and_work_stay_bounded() {
    let l = ClientLimiter::new(2, 1);
    let t0 = Instant::now();
    let ip = |i: u32| IpAddr::V4(std::net::Ipv4Addr::from(0x0A00_0000 + i));
    for i in 0..BUCKETS_MAX as u32 {
        assert!(l.allow_at(ip(i), t0));
    }
    let over: Vec<bool> = (0..5_000u32).map(|i| l.allow_at(ip(BUCKETS_MAX as u32 + i), t0)).collect();
    assert!(l.clients() <= BUCKETS_MAX + 1, "{} buckets", l.clients());
    assert_eq!(over.iter().filter(|x| **x).count(), 2, "past the cap new addresses share one bucket of burst 2");
    assert_eq!(l.sweeps(), 1, "one sweep in the same second, however many new addresses");
    assert!(l.allow_at(ip(1), t0 + Duration::from_millis(1_500)), "a known client keeps its bucket");
    // After the refill time (2 s at 1/s) the idle buckets are swept on the next new address.
    assert!(l.allow_at(ip(9_999_999), t0 + Duration::from_secs(3)));
    assert_eq!(l.sweeps(), 2);
    assert!(l.clients() < 10, "{} buckets left after the sweep", l.clients());
}

/// Note (/health): a remote client (or one behind the proxy) gets only `{"ok":...}`; the detail with
/// the node's error (which can name paths and the node's URL) only on loopback without a forwarded
/// client. Control: the detail does carry the error string the public answer leaves out.
#[tokio::test]
async fn health_detail_only_on_loopback() {
    let f = fixture("hosted-health-public");
    let fo = Arc::new(FollowHealth::new(Duration::from_secs(60)));
    fo.set_nodes("wss://node.example/kaspa/testnet-10/wrpc/borsh?token=SECRET".into(), Some("wss://witness.example/w".into()));
    fo.err("C:\\srv\\secret-store\\index-log.jsonl: cannot find header".into(), true);
    let snap = indexer_api::store::Follower::open(&f.cfg.store, &f.cfg.bundle, f.cfg.cov).unwrap().snapshot();
    for mode in [ClientIp::Peer, ClientIp::Forwarded] {
        let mut cfg = f.cfg.clone();
        cfg.client_ip = mode;
        let mut st = indexer_api::AppState::new(snap.clone(), &cfg);
        st.follow = Some(fo.clone());
        let r = indexer_api::router(st, &[]).unwrap();
        let (s, _, b) = status_of(&r, "/health", Some(peer("203.0.113.9")), None).await;
        assert_eq!((s, b.as_str()), (503, r#"{"ok":false}"#), "{mode:?}: a remote client");
        let (s, _, b) = status_of(&r, "/health", Some(peer("127.0.0.1")), Some("203.0.113.9")).await;
        assert_eq!((s, b.as_str()), (503, r#"{"ok":false}"#), "{mode:?}: a client behind the local proxy");
        let (s, _, b) = status_of(&r, "/health", Some(peer("127.0.0.1")), None).await;
        assert_eq!(s, 503);
        assert!(b.contains("secret-store") && b.contains("SECRET") && b.contains("witness.example"), "{mode:?}: the local detail: {b}");
    }
}

/// F6 part E: `/v1/status`'s `error` is the fixed word `STATUS_STORE_ERROR` for a remote client and for
/// one behind the local proxy (the store's error text can name the server's paths); the text itself only
/// on loopback without a forwarded client (as `/health`). Negative control: the local answer carries the
/// path the public one hides, and a store without an error answers `null` to everyone.
#[tokio::test]
async fn status_error_text_only_on_loopback() {
    let f = fixture("hosted-status-error");
    let mut snap = indexer_api::store::Follower::open(&f.cfg.store, &f.cfg.bundle, f.cfg.cov).unwrap().snapshot();
    let clean = snap.clone();
    snap.error = Some("log line 9 (byte 4242): C:\\srv\\secret-store\\index-log.jsonl: txid mismatch".into());
    let field = |b: &str| serde_json::from_str::<serde_json::Value>(b).unwrap()["error"].clone();
    for mode in [ClientIp::Peer, ClientIp::Forwarded] {
        let mut cfg = f.cfg.clone();
        cfg.client_ip = mode;
        let r = indexer_api::router(indexer_api::AppState::new(snap.clone(), &cfg), &[]).unwrap();
        for (who, p, xff) in [("a remote client", Some(peer("203.0.113.9")), None), ("a client behind the local proxy", Some(peer("127.0.0.1")), Some("203.0.113.9"))] {
            let (s, _, b) = status_of(&r, "/v1/status", p, xff).await;
            assert_eq!(s, 200, "{mode:?}: {who}");
            assert_eq!(field(&b), serde_json::json!(indexer_api::routes::STATUS_STORE_ERROR), "{mode:?}: {who}: {b}");
            assert!(!b.contains("secret-store") && !b.contains("4242"), "{mode:?}: {who}: {b}");
            // The page's parser still reads the answer (the field stays a string).
            assert!(web::indexer::parse_status(&b).is_ok(), "{mode:?}: {who}: {b}");
        }
        let (_, _, b) = status_of(&r, "/v1/status", Some(peer("127.0.0.1")), None).await;
        assert!(field(&b).as_str().is_some_and(|e| e.contains("secret-store") && e.contains("4242")), "{mode:?}: the local detail: {b}");
        let r = indexer_api::router(indexer_api::AppState::new(clean.clone(), &cfg), &[]).unwrap();
        let (_, _, b) = status_of(&r, "/v1/status", Some(peer("203.0.113.9")), None).await;
        assert_eq!(field(&b), serde_json::Value::Null, "{mode:?}: no error: {b}");
    }
}

fn empty_block(byte: u8, daa: u64) -> ChainBlock {
    ChainBlock { hash: Hash::from_bytes([byte; 32]), daa, txs: vec![] }
}

fn view(blocks: &[(u8, &[u8])]) -> ChainView {
    ChainView {
        removed: vec![],
        added: blocks.iter().map(|(h, txs)| (Hash::from_bytes([*h; 32]), txs.iter().map(|t| (Hash::from_bytes([*t; 32]), vec![vec![*t]])).collect())).collect(),
    }
}

/// S4 `agree`: both nodes' common prefix is taken; a tip fork or a witness behind takes less; a
/// removal one node reports and the other not takes nothing; one chain block with different
/// transactions is a divergence.
#[test]
fn the_witness_confirms_the_common_prefix_only() {
    let p = view(&[(1, &[]), (2, &[7]), (3, &[])]);
    assert_eq!(agree(&p, &p), Agreement::Take(3));
    assert_eq!(agree(&p, &view(&[(1, &[]), (2, &[7])])), Agreement::Take(2), "the witness is behind");
    assert_eq!(agree(&p, &view(&[(1, &[]), (2, &[7]), (3, &[]), (4, &[])])), Agreement::Take(3), "the witness is ahead");
    assert_eq!(agree(&p, &view(&[(1, &[]), (9, &[])])), Agreement::Take(1), "a tip fork");
    assert_eq!(agree(&p, &view(&[(9, &[])])), Agreement::Take(0), "another chain");
    assert!(matches!(agree(&p, &view(&[(1, &[]), (2, &[8]), (3, &[])])), Agreement::Diverged(_)), "chain block 2 with another transaction");
    assert!(matches!(agree(&p, &view(&[(1, &[]), (2, &[]), (3, &[])])), Agreement::Diverged(_)), "chain block 2 without its transaction");
    let mut sig = view(&[(1, &[]), (2, &[7]), (3, &[])]);
    sig.added[1].1[0].1 = vec![vec![0xEE]];
    assert!(matches!(agree(&p, &sig), Agreement::Diverged(_)), "the same transaction ID with another signature script");
    let mut rm = p.clone();
    rm.removed = vec![Hash::from_bytes([5; 32])];
    assert_eq!(agree(&rm, &p), Agreement::Take(0), "a removal the witness does not report");
    assert_eq!(agree(&rm, &rm), Agreement::Take(3));
}

/// S4 on the real indexer over a store: a lying followed node's chain blocks the witness does not
/// confirm are never persisted (the store's position and files unchanged); a divergence persists
/// nothing and is the 503-at-once error; the witness's agreement persists them. Negative control:
/// without a witness the same lie is persisted (what one bad node did before).
#[test]
fn a_lying_node_cannot_write_the_store_past_the_witness() {
    let f = fixture("hosted-witness");
    let (_, _, t, genesis) = indexer_api::store::read_bundle(&f.cfg.bundle, f.cfg.cov).unwrap();
    let open = || Indexer::open(Some(&f.dir), t.clone(), f.cfg.cov, Some(genesis.clone()), Hash::from_bytes([0; 32]), 0).unwrap();
    let mut x = open();
    let (last, daa) = (x.last, x.last_daa);
    let snap = || std::fs::read_to_string(f.dir.join(indexer_api::node_indexer::SNAPSHOT_FILE)).unwrap_or_default();
    let log = || std::fs::read_to_string(f.dir.join(indexer_api::node_indexer::LOG_FILE)).unwrap();
    let (snap0, log0) = (snap(), log());
    let lie = ChainBatch { removed: vec![], added: vec![empty_block(0xA1, daa + 1), empty_block(0xA2, daa + 2)] };
    // The witness reports another chain from the same block: nothing is taken.
    let (done, more) = apply_witnessed(&mut x, lie.clone(), Some(&view(&[(0xB1, &[])]))).unwrap();
    assert_eq!((done.blocks, done.unconfirmed, more), (0, 2, false));
    assert_eq!((x.last, snap(), log()), (last, snap0.clone(), log0.clone()), "nothing persisted");
    // The same block with other transactions: a divergence, nothing persisted.
    let e = apply_witnessed(&mut x, lie.clone(), Some(&view(&[(0xA1, &[3])]))).unwrap_err();
    assert!(e.starts_with(DIVERGED), "{e}");
    assert_eq!((x.last, snap(), log()), (last, snap0.clone(), log0.clone()), "nothing persisted");
    // Agreement on the first block only: that block is persisted, the second waits.
    let (done, _) = apply_witnessed(&mut x, lie.clone(), Some(&view(&[(0xA1, &[]), (0xB2, &[])]))).unwrap();
    assert_eq!((done.blocks, done.unconfirmed), (1, 1));
    assert_eq!(x.last, Hash::from_bytes([0xA1; 32]));
    assert_eq!(open().last, Hash::from_bytes([0xA1; 32]), "persisted: a restart resumes there");
    // Negative control: no witness, a fresh copy of the store: the lie is persisted.
    let g = fixture("hosted-witness-control");
    let mut y = Indexer::open(Some(&g.dir), t.clone(), g.cfg.cov, Some(genesis.clone()), Hash::from_bytes([0; 32]), 0).unwrap();
    apply_witnessed(&mut y, lie, None).unwrap();
    assert_eq!(y.last, Hash::from_bytes([0xA2; 32]), "one node alone writes what it reports");
}

/// The S1 and S4 flags: `--per-client-inflight`, `--max-anchor-waits`, `--witness` (only with
/// `--node`, never the followed node itself, refused on a mainnet port).
#[test]
fn security_review_flags() {
    let cov = "ab".repeat(32);
    let base = ["--store", "x", "--cov", cov.as_str()];
    let with = |more: &[&str]| parse(&[&base[..], more].concat());
    let d = with(&[]).unwrap();
    assert_eq!((d.per_client_inflight, d.max_anchor_waits, d.max_requests), (16, 32, 64));
    let c = with(&["--per-client-inflight", "4", "--max-anchor-waits", "8"]).unwrap();
    assert_eq!((c.per_client_inflight, c.max_anchor_waits), (4, 8));
    assert_eq!(with(&["--node", "resolver"]).unwrap().follow.unwrap().witness, Witness::Unset);
    assert_eq!(with(&["--node", "resolver", "--witness", "resolver"]).unwrap().follow.unwrap().witness, Witness::Resolver);
    assert_eq!(with(&["--node", "resolver", "--witness", "off"]).unwrap().follow.unwrap().witness, Witness::Off);
    let u = "wss://w.example/kaspa/testnet-10/wrpc/borsh";
    assert_eq!(with(&["--node", "wss://n.example/x", "--witness", u]).unwrap().follow.unwrap().witness, Witness::Url(u.into()));
    assert!(with(&["--witness", "resolver"]).is_err(), "--witness without --node");
    assert!(with(&["--node", u, "--witness", u]).is_err(), "the followed node as its own witness");
    assert!(with(&["--node", "resolver", "--witness", "wss://x:17110"]).is_err(), "a mainnet port");
    assert!(with(&["--node", "resolver", "--witness", "http://x"]).is_err());
    assert_eq!(host_of("wss://Quark-10.kaspa.red/kaspa/testnet-10/wrpc/borsh"), "quark-10.kaspa.red");
}

/// The deploy files (F6 security review notes): `indexer-api.env.example` parses as the server's settings (with
/// a covenant ID filled in) and sets a witness; every base image in the Dockerfile and compose.yaml is pinned by
/// digest; the Caddyfile caps request headers. Negative control: an unpinned image line is found.
#[test]
fn deploy_files_parse_and_pin_their_images() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy");
    let env_text = std::fs::read_to_string(dir.join("indexer-api.env.example")).unwrap();
    let mut vars: std::collections::HashMap<String, String> = env_text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect();
    vars.insert("INDEXER_API_COV".into(), "ab".repeat(32));
    let c = indexer_api::config::parse(&[], |k| vars.get(k).cloned(), |_, _| Ok(false)).unwrap();
    let fo = c.follow.expect("the example follows a node");
    assert_eq!((fo.node, fo.witness), (None, Witness::Resolver), "the example follows the resolver with a witness");
    assert_eq!((c.client_ip, c.per_client_inflight), (ClientIp::Forwarded, 16));
    let unpinned = |text: &str, key: &str| -> Vec<String> {
        text.lines().map(str::trim).filter(|l| l.starts_with(key)).filter(|l| !l.contains("@sha256:") && !l.contains(" scratch ")).map(String::from).collect()
    };
    let docker = std::fs::read_to_string(dir.join("Dockerfile")).unwrap();
    let compose = std::fs::read_to_string(dir.join("compose.yaml")).unwrap();
    assert_eq!(unpinned(&docker, "FROM "), Vec::<String>::new(), "Dockerfile base images by digest");
    assert_eq!(unpinned(&compose, "image: caddy"), Vec::<String>::new(), "compose.yaml's Caddy by digest");
    assert_eq!(unpinned("FROM rust:1.94.0-bookworm AS build", "FROM ").len(), 1, "negative control: an unpinned line is found");
    let caddy = std::fs::read_to_string(dir.join("Caddyfile")).unwrap();
    assert!(caddy.contains("max_header_size 16KB"), "the Caddyfile caps request headers");
}
