//! The indexer API's endpoints and server basics (spec frontend 6.3; F2): every endpoint's
//! body parses as its `web::indexer` type (the page's parser, unknown fields refused), the
//! instance is the bundle file byte for byte, typed errors, GET only, CORS, following a
//! growing log, a log line that does not apply, and the connection guards on a real socket.
//!
//! Run: `cargo test -j 2 -p indexer-api --test api`.

mod common;

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use axum::body::Body;
use common::*;
use lottery::state::Payout;
use web::indexer::*;

/// Every endpoint, as the page reads it.
#[tokio::test]
async fn endpoints_serve_their_types() {
    let f = fixture("endpoints");
    let a = app(&f.cfg);
    let (s, h, b) = get(&a, "/v1/status").await;
    assert_eq!((s, h["content-type"].to_str().unwrap()), (200, "application/json"));
    let st = parse_status(&b).unwrap();
    assert_eq!((st.network.as_str(), st.covenant_id.as_str(), st.log_lines.as_str()), ("testnet-10", cov_hex(&f.ch).as_str(), f.ch.log.len().to_string().as_str()));
    assert_eq!(st.current_round.as_deref(), Some("3"));
    assert!(st.error.is_none() && st.gaps.is_empty());

    let (s, _, b) = get(&a, "/v1/instance").await;
    assert_eq!(s, 200);
    assert_eq!(b.as_bytes(), std::fs::read(f.dir.join("bundle.json")).unwrap().as_slice(), "the bundle, byte for byte");

    let (_, _, b) = get(&a, "/v1/rounds").await;
    let rs = parse_rounds(&b).unwrap();
    let st: Vec<(String, RoundStatus)> = rs.rounds.iter().map(|r| (r.round.clone(), r.status)).collect();
    assert_eq!(st, [("3".into(), RoundStatus::Open), ("2".into(), RoundStatus::Closing), ("1".into(), RoundStatus::Drawn)]);
    assert_eq!(rs.next, None);
    let (_, _, b) = get(&a, "/v1/rounds?before=3").await;
    assert_eq!(parse_rounds(&b).unwrap().rounds.len(), 2);

    for r in 1..=3 {
        let (s, _, b) = get(&a, &format!("/v1/rounds/{r}")).await;
        assert_eq!(s, 200, "{b}");
        let x = parse_round(&b).unwrap();
        assert!(!x.objects.is_empty(), "round {r} has live objects");
        let (s, _, b) = get(&a, &format!("/v1/rounds/{r}/purchases")).await;
        assert_eq!(s, 200, "{b}");
        let p = parse_purchases(&b).unwrap();
        assert_eq!(p.shards.len(), 4);
        let total: usize = p.shards.iter().map(|h| h.len() / 12).sum();
        assert_eq!(total as u32, x.combinations, "round {r}: every combination served");
        for i in 0..4 {
            let (s, _, b) = get(&a, &format!("/v1/rounds/{r}/shards/{i}")).await;
            assert_eq!(s, 200, "{b}");
            let sh = parse_shard(&b).unwrap();
            assert_eq!(sh.purchases, p.shards[i], "round {r} shard {i}");
            assert_eq!(sh.count as usize, sh.purchases.len() / 12);
        }
    }
    let (_, _, b) = get(&a, "/v1/rounds/1").await;
    let r1 = parse_round(&b).unwrap();
    let d = r1.draw.as_ref().expect("round 1 drawn");
    assert!(!d.void && d.winning.is_some() && !d.pools.is_empty());
    assert!(r1.close.as_ref().is_some_and(|c| c.frozen_roots.len() == 4));
    assert!(r1.deadline.is_some() && r1.anchor_daa_score.is_some());
    let (s, _, b) = get(&a, "/v1/rounds/1/anchor-proof").await;
    assert_eq!(s, 200, "{b}");
    let ap = parse_anchor_proof(&b).unwrap();
    assert_eq!((ap.anchor_proof.len(), ap.source), (544, ProofSource::Draw), "272 bytes from the draw");

    let t = &f.ch.tickets[0].1;
    let payout = addr(&Payout::from_fields(t.payout_class, &t.payout_key));
    let (s, _, b) = get(&a, &format!("/v1/tickets?payout={payout}")).await;
    assert_eq!(s, 200, "{b}");
    let tk = parse_tickets(&b).unwrap();
    assert_eq!(tk.tickets.len(), f.ch.tickets.iter().filter(|x| x.1.payout_key == t.payout_key).count(), "every live ticket of that payout");
    assert!(tk.bought.len() >= tk.tickets.len());
    // Percent-encoded, as a browser's URLSearchParams sends it.
    let (s, _, _) = get(&a, &format!("/v1/tickets?payout={}", payout.replace(':', "%3A"))).await;
    assert_eq!(s, 200);

    let (_, _, b) = get(&a, "/v1/vault").await;
    let v = parse_objects(&b).unwrap();
    assert!(v.objects.iter().any(|o| o.role == web::views::RoleName::Vault));
    let dep = addr(&depositor());
    let (_, _, b) = get(&a, &format!("/v1/positions?owner={dep}")).await;
    let p = parse_objects(&b).unwrap();
    assert_eq!(p.objects.len(), 1, "the settled deposit's position");
    let (s, _, b) = get(&a, &format!("/v1/requests?owner={dep}")).await;
    assert_eq!(s, 200, "{b}");
    assert!(parse_objects(&b).unwrap().objects.is_empty(), "the request was settled");
}

/// Typed errors; GET only; no write endpoint.
#[tokio::test]
async fn errors_are_typed_and_writes_refused() {
    let f = fixture("errors");
    let a = app(&f.cfg);
    let mainnet = "kaspa:qypr7ayn2rzkc8xpx5xt8yqvq5zy5ftx6uxylvrkqvtlmddmy7jw3sqddcvj4pd";
    for (uri, status, code) in [
        ("/v1/nope", 404, ApiErrorCode::NotFound),
        ("/v2/status", 404, ApiErrorCode::NotFound),
        ("/v1/rounds/0", 400, ApiErrorCode::BadRequest),
        ("/v1/rounds/01", 400, ApiErrorCode::BadRequest),
        ("/v1/rounds/x", 400, ApiErrorCode::BadRequest),
        ("/v1/rounds/9", 404, ApiErrorCode::RoundUnknown),
        ("/v1/rounds/1/shards/4", 404, ApiErrorCode::ShardUnknown),
        ("/v1/rounds/1/shards/01", 404, ApiErrorCode::ShardUnknown),
        ("/v1/rounds/3/anchor-proof", 404, ApiErrorCode::AnchorUnavailable),
        ("/v1/rounds/2/anchor-proof", 404, ApiErrorCode::AnchorUnavailable),
        ("/v1/rounds?before=x", 400, ApiErrorCode::BadRequest),
        ("/v1/tickets", 400, ApiErrorCode::BadRequest),
        ("/v1/tickets?payout=kaspatest:nope", 400, ApiErrorCode::BadRequest),
        ("/v1/positions?owner=x&owner=y", 400, ApiErrorCode::BadRequest),
        ("/v1/requests?who=x", 400, ApiErrorCode::BadRequest),
    ] {
        let (s, h, b) = get(&a, uri).await;
        assert_eq!(s, status, "{uri}: {b}");
        assert_eq!(h["content-type"], "application/json", "{uri}");
        let e = parse_error(&b).unwrap_or_else(|e| panic!("{uri}: {e}: {b}"));
        assert_eq!(e.error.code, code, "{uri}: {b}");
        assert_eq!(e.error.message, code.message());
    }
    let (s, _, b) = get(&a, &format!("/v1/tickets?payout={mainnet}")).await;
    assert_eq!((s, parse_error(&b).unwrap().error.code), (400, ApiErrorCode::AddressNetwork), "a mainnet address: {b}");
    for m in ["POST", "PUT", "DELETE", "PATCH"] {
        for uri in ["/v1/status", "/v1/rounds/1", "/v1/nope"] {
            let req = axum::http::Request::builder().method(m).uri(uri).body(Body::from("{}")).unwrap();
            let (s, _, b) = request(&a, req).await;
            assert_eq!((s, parse_error(&b).unwrap().error.code), (405, ApiErrorCode::MethodNotAllowed), "{m} {uri}: {b}");
        }
    }
    let head = axum::http::Request::head("/v1/status").body(Body::empty()).unwrap();
    assert_eq!(request(&a, head).await.0, 200);
}

/// CORS for a static site on another origin: any origin by default; with a list, only the
/// listed ones; GET and HEAD only.
#[tokio::test]
async fn cors_for_a_static_site() {
    let f = fixture("cors");
    let a = app(&f.cfg);
    let req = axum::http::Request::get("/v1/status").header("origin", "https://kaspalionare.example").body(Body::empty()).unwrap();
    let (_, h, _) = request(&a, req).await;
    assert_eq!(h["access-control-allow-origin"], "*");
    let pre = || {
        axum::http::Request::builder()
            .method("OPTIONS")
            .uri("/v1/rounds/1")
            .header("origin", "https://kaspalionare.example")
            .header("access-control-request-method", "GET")
            .body(Body::empty())
            .unwrap()
    };
    let (s, h, _) = request(&a, pre()).await;
    assert_eq!(s, 200);
    let methods = h["access-control-allow-methods"].to_str().unwrap().to_string();
    assert!(methods.contains("GET") && !methods.contains("POST"), "{methods}");
    let mut cfg = f.cfg.clone();
    cfg.allow_origins = vec!["https://kaspalionare.example".into()];
    let b = app(&cfg);
    let (_, h, _) = request(&b, pre()).await;
    assert_eq!(h["access-control-allow-origin"], "https://kaspalionare.example");
    let other = axum::http::Request::get("/v1/status").header("origin", "https://evil.example").body(Body::empty()).unwrap();
    let (_, h, _) = request(&b, other).await;
    assert!(h.get("access-control-allow-origin").is_none(), "an unlisted origin gets no CORS grant");
}

/// The follower: a log that grows is followed (a partial last line waits for its end), a
/// rewritten log is read again, and a line that does not apply stops it there with the
/// error reported while the last good state stays served.
#[test]
fn follows_a_growing_log() {
    let ch = game();
    let dir = fresh_dir("follow");
    write_store(&ch, &dir, Some(3));
    let mut fo = indexer_api::store::Follower::open(&dir, &dir.join("bundle.json"), ch.cov).unwrap();
    assert_eq!(fo.snapshot().log_lines, 3);
    let full = log_text(&ch, None);
    let part = log_text(&ch, Some(3));
    // Half of the next line: nothing applied yet.
    let next_end = part.len() + full[part.len()..].find('\n').unwrap() + 1;
    std::fs::write(dir.join("index-log.jsonl"), &full[..part.len() + (next_end - part.len()) / 2]).unwrap();
    assert!(!fo.poll().unwrap() || fo.snapshot().log_lines == 3);
    assert_eq!(fo.snapshot().log_lines, 3, "a partial line waits");
    std::fs::write(dir.join("index-log.jsonl"), &full).unwrap();
    assert!(fo.poll().unwrap());
    let s = fo.snapshot();
    assert_eq!((s.log_lines, s.rounds.len(), s.error.clone()), (ch.log.len() as u64, 3, None));
    // A line that does not apply (its transaction id is not its transaction's).
    let mut v: serde_json::Value = serde_json::from_str(full.lines().last().unwrap()).unwrap();
    v["seq"] = serde_json::json!(ch.log.len());
    v["txid"] = serde_json::json!("ab".repeat(32));
    std::fs::write(dir.join("index-log.jsonl"), format!("{full}{}\n", serde_json::to_string(&v).unwrap())).unwrap();
    assert!(fo.poll().unwrap());
    let s = fo.snapshot();
    assert_eq!(s.log_lines, ch.log.len() as u64, "the last good state stays");
    assert!(s.error.as_deref().is_some_and(|e| e.contains("txid")), "{:?}", s.error);
    // Rewritten shorter: read again from the start.
    std::fs::write(dir.join("index-log.jsonl"), &part).unwrap();
    assert!(fo.poll().unwrap());
    assert_eq!((fo.snapshot().log_lines, fo.snapshot().error), (3, None));
}

/// A store of another instance, or with a bundle of another, is refused at start.
#[test]
fn refuses_another_instance() {
    let ch = game();
    let dir = fresh_dir("other");
    write_store(&ch, &dir, None);
    let other = harness::Hash::from_bytes([9; 32]);
    let e = indexer_api::store::Follower::open(&dir, &dir.join("bundle.json"), other).err().unwrap();
    assert!(e.contains("not the configured"), "{e}");
}

fn raw_get(addr: std::net::SocketAddr, path: &str) -> String {
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

/// On a real socket: the server answers, and a connection that never sends its headers is
/// closed after the header-read timeout.
#[test]
fn real_socket_and_header_timeout() {
    let f = fixture("socket");
    let mut cfg = f.cfg.clone();
    cfg.header_timeout = Duration::from_millis(500);
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let addr = listener.local_addr().unwrap();
    let st = indexer_api::open(&cfg).unwrap();
    let router = indexer_api::router(st, &[]).unwrap();
    rt.spawn(async move { indexer_api::serve(listener, router, &cfg).await });
    let r = raw_get(addr, "/v1/status");
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    let r = raw_get(addr, "/v1/rounds/1");
    assert!(r.starts_with("HTTP/1.1 200") && r.contains("\"status\":\"drawn\""), "{}", &r[..r.len().min(300)]);
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let t0 = Instant::now();
    let mut buf = [0u8; 64];
    let n = s.read(&mut buf).unwrap_or(0);
    let waited = t0.elapsed();
    assert!(waited >= Duration::from_millis(400) && waited < Duration::from_secs(5), "closed after {waited:?} ({n} bytes)");
    println!("an idle connection without headers was closed after {waited:?}");
}
