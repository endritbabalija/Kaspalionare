//! The fixture of the indexer API tests: a three-round game on lottery's own test chain
//! (`lottery/tests/common`, compiled in here; every transaction built by the builders and run
//! in the engine), written as an M8 store (`index-log.jsonl` + `bundle.json`), and the
//! chain's ledger as the "public node" the page reads entries from.
//!
//! The game (K = 4, `chain5`): round 1 has buys on three shards (a jackpot holding and a
//! lower-tier holding for its seed, plus non-winning ones and a referral), a deposit request,
//! a close, a draw (pools for both held tiers), a settled request (a position) and a claim of
//! the lower tier; round 2 has buys and a close (closing: waiting for its draw); round 3 has
//! buys (open). So every status a page shows is present at once.

#![allow(dead_code)]

#[path = "../../../lottery/tests/common/mod.rs"]
pub mod lc;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use harness::{Hash, TransactionOutpoint};
use http_body_util::BodyExt;
use lottery::buy::{Final, KAS, OwnerKey, funding_spk};
use lottery::claim::{combo_for_tier, combo_matching};
use lottery::combo::Combo;
use lottery::genesis::{GENESIS_FUNDING, GenesisCase};
use lottery::ledger::Ledger;
use lottery::state::{Payout, TIER_JACKPOT, TIER_LOWER0};
use lottery::store::{self, IndexLog, IndexLogLine};
use tower::ServiceExt;
use web::convert::{Net, utxo_dto};
use web::dto::Utxo;
use web::indexer::ApiObject;

use lc::m2::Chain;
use lc::m5::chain5;

pub use indexer_api::{AppState, Config};

fn win6(ch: &Chain, seed: &[u8; 32]) -> [u8; 6] {
    let w = ch.winning(seed);
    [w.main[0], w.main[1], w.main[2], w.main[3], w.main[4], w.bonus]
}

fn buy(ch: &mut Chain, i: usize, bundle: Vec<Combo>, with_ref: bool, daa: u64) {
    let price = ch.controller.1.price as u64;
    let (mut c, ops) = ch.buy_case_funded(i, bundle.len() as u64 * price + 10 * KAS);
    c.bundle = bundle;
    c.with_ref = with_ref;
    ch.buy(i, &c, ops, daa);
}

fn ticket_of(ch: &Chain, round: i64, bundle0: [u8; 6]) -> usize {
    ch.tickets.iter().position(|t| t.1.round == round && t.1.bundle[..6] == bundle0).expect("ticket tracked")
}

pub fn depositor() -> Payout {
    OwnerKey::Schnorr([0x41; 32]).payout()
}

/// The game (see the module doc).
pub fn game() -> Chain {
    let mut ch = chain5();
    // ---- Round 1: drawn.
    let seed1 = [0x11u8; 32];
    let w1 = win6(&ch, &seed1);
    let m1 = ch.controller.1.m;
    let jack = combo_for_tier(&w1, TIER_JACKPOT, m1, 0);
    let low = combo_for_tier(&w1, TIER_LOWER0, m1, 0);
    let nowin = |n| combo_matching(&w1, 0, false, m1, n);
    buy(&mut ch, 0, vec![jack, low], false, 310_000);
    buy(&mut ch, 1, vec![nowin(1)], true, 320_000);
    buy(&mut ch, 0, vec![nowin(2), nowin(3)], false, 330_000);
    buy(&mut ch, 2, vec![nowin(4)], false, 340_000);
    let base = ch.controller.1.deadline as u64 - 60_000;
    ch.deposit_now(&depositor(), 20 * KAS as i64, base);
    ch.close_now();
    ch.draw_now(seed1);
    let tj = ticket_of(&ch, 1, jack.bytes());
    let pl = ch.pool_of(1, TIER_LOWER0);
    let (c, ops) = ch.claim_case(tj, pl, 1, 1_099_000);
    ch.claim(&c, ops, 1_100_000);
    let now = ch.controller.1.deadline as u64 - 50_000;
    ch.settle_now(0, now);
    // ---- Round 2: closing.
    let m2 = ch.controller.1.m;
    let w2 = win6(&ch, &[0x22; 32]);
    let base = ch.controller.1.deadline as u64 - 60_000;
    buy(&mut ch, 1, vec![combo_matching(&w2, 0, false, m2, 5)], false, base - 10_000);
    buy(&mut ch, 3, vec![combo_matching(&w2, 0, false, m2, 6), combo_matching(&w2, 0, false, m2, 7)], true, base - 9_000);
    ch.close_now();
    // ---- Round 3: open.
    let m3 = ch.controller.1.m;
    let w3 = win6(&ch, &[0x33; 32]);
    let base = ch.controller.1.deadline as u64 - 60_000;
    buy(&mut ch, 2, vec![combo_matching(&w3, 0, false, m3, 8)], false, base);
    buy(&mut ch, 2, vec![combo_matching(&w3, 0, false, m3, 9)], false, base + 100);
    assert!(!ch.fabricated, "nothing fabricated");
    ch
}

/// The chain's launch bundle (`bundle/1`): the genesis case rebuilt exactly as `chain5`
/// builds it (same ledger funding, same edit), the chain's genesis transaction.
pub fn bundle_of(ch: &Chain) -> lottery::bundle::Bundle {
    let mut l = Ledger::new();
    let op = l.fund(GENESIS_FUNDING, funding_spk(), 100);
    let e = l.get(&op).unwrap().clone();
    let mut g = GenesisCase::new_with(4, vec![(op, e)], &ch.t);
    g.controller.st = [400, 300, 300, 0, 0];
    let f = Final { tx: ch.log[0].tx.clone(), entries: vec![], units: vec![], sim: None };
    let live = lottery::bundle::genesis_live(&g, &f, 200);
    let b = lottery::bundle::Bundle::new(&g, &f, live, None).unwrap();
    let v = lottery::bundle::verify(&b).expect("the fixture bundle verifies");
    assert_eq!(v.cov, ch.cov, "the fixture bundle is the chain's instance");
    b
}

/// A fresh store directory for `name`.
pub fn fresh_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("indexer-api-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The log of the first `n` accepted transactions of `ch` (all with `None`).
pub fn log_text(ch: &Chain, n: Option<usize>) -> String {
    let lines: Vec<IndexLogLine> =
        ch.log.iter().take(n.unwrap_or(ch.log.len())).enumerate().map(|(i, a)| IndexLogLine::add(i as u64, Hash::from_bytes([(i % 250) as u8 + 1; 32]), a.daa, a)).collect();
    IndexLog { cov: ch.cov, lines, reanchors: vec![] }.to_jsonl().unwrap()
}

/// The store of `ch` in `dir`: the whole log (or its first `n` transactions) and the bundle.
pub fn write_store(ch: &Chain, dir: &Path, n: Option<usize>) {
    std::fs::write(dir.join("index-log.jsonl"), log_text(ch, n)).unwrap();
    std::fs::write(dir.join("bundle.json"), store::write(&bundle_of(ch), store::BUNDLE).unwrap()).unwrap();
}

/// The fixture: the game, its store in a fresh directory, a configuration for it.
pub struct Fixture {
    pub ch: Chain,
    pub dir: PathBuf,
    pub cfg: Config,
}

pub fn fixture(name: &str) -> Fixture {
    let ch = game();
    let dir = fresh_dir(name);
    write_store(&ch, &dir, None);
    let cfg = Config::new(dir.clone(), ch.cov);
    Fixture { ch, dir, cfg }
}

/// The app over the store (no follow thread: the snapshot of the store as it is).
pub fn app(cfg: &Config) -> axum::Router {
    let f = indexer_api::store::Follower::open(&cfg.store, &cfg.bundle, cfg.cov).expect("store opens");
    indexer_api::router(AppState::new(f.snapshot(), cfg), &cfg.allow_origins).unwrap()
}

/// GET `uri` on the router: (status, headers, body).
pub async fn get(app: &axum::Router, uri: &str) -> (u16, axum::http::HeaderMap, String) {
    request(app, axum::http::Request::get(uri).body(Body::empty()).unwrap()).await
}

pub async fn request(app: &axum::Router, req: axum::http::Request<Body>) -> (u16, axum::http::HeaderMap, String) {
    let r = app.clone().oneshot(req).await.unwrap();
    let status = r.status().as_u16();
    let headers = r.headers().clone();
    let b = r.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(b.to_vec()).unwrap())
}

/// The public node's entries (the chain's ledger) for the objects' outpoints; spent ones are
/// simply absent, as a node returns nothing for them.
pub fn node_utxos(ch: &Chain, objects: &[ApiObject]) -> Vec<Utxo> {
    objects
        .iter()
        .filter_map(|o| {
            let id = Hash::from_bytes(web::convert::hash32("txid", &o.transaction_id).unwrap());
            let op = TransactionOutpoint::new(id, o.index);
            ch.ledger.get(&op).map(|e| utxo_dto(&op, e))
        })
        .collect()
}

pub fn addr(p: &Payout) -> String {
    Net::Testnet10.address(&p.spk()).unwrap()
}

pub fn cov_hex(ch: &Chain) -> String {
    web::convert::hex(&ch.cov.as_bytes())
}

pub fn shared<T>(x: T) -> Arc<T> {
    Arc::new(x)
}
