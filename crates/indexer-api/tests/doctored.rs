//! Spec frontend 8, "Untrusted API": an API that serves a wrong state, a wrong tree, a
//! missing purchase, an extra purchase, a wrong winning number or a wrong anchor proof makes
//! the page refuse, with the mismatch message ("The indexer's copy does not match the
//! chain."). Each doctored response goes through the page's own checks (`web::indexer`, the
//! code the wasm runs) against the chain (the fixture's ledger as the public node). The
//! honest server is the positive control: every check passes on its answers.
//!
//! Run: `cargo test -j 2 -p indexer-api --features doctored --test doctored`.

mod common;

use common::*;
use indexer_api::doctor::{ALL, Mode};
use lottery::web::Code;
use web::convert::Net;
use web::indexer::*;

const NET: Net = Net::Testnet10;

fn doctored(f: &Fixture, m: Option<Mode>) -> axum::Router {
    let mut cfg = f.cfg.clone();
    cfg.doctor = m;
    app(&cfg)
}

/// The round's responses as the page fetches them, and the node's entries for its objects.
async fn round_input(f: &Fixture, a: &axum::Router, r: u32) -> RoundCheckInput {
    let (s, _, b) = get(a, &format!("/v1/rounds/{r}")).await;
    assert_eq!(s, 200, "round {r}: {b}");
    let round = parse_round(&b).unwrap();
    let (s, _, b) = get(a, &format!("/v1/rounds/{r}/purchases")).await;
    assert_eq!(s, 200, "round {r} purchases: {b}");
    let purchases = Some(parse_purchases(&b).unwrap());
    let (s, _, b) = get(a, &format!("/v1/rounds/{r}/anchor-proof")).await;
    let anchor_proof = (s == 200).then(|| parse_anchor_proof(&b).unwrap());
    let utxos = node_utxos(&f.ch, &round.objects);
    RoundCheckInput { covenant_id: cov_hex(&f.ch), round, utxos, purchases, anchor_proof, anchor_block: None }
}

async fn tickets_input(f: &Fixture, a: &axum::Router) -> ObjectsCheckInput {
    let payout = lottery::state::Payout::from_fields(f.ch.tickets[0].1.payout_class, &f.ch.tickets[0].1.payout_key);
    let (s, _, b) = get(a, &format!("/v1/tickets?payout={}", addr(&payout))).await;
    assert_eq!(s, 200, "{b}");
    let t = parse_tickets(&b).unwrap();
    let utxos = node_utxos(&f.ch, &t.tickets);
    ObjectsCheckInput { covenant_id: cov_hex(&f.ch), objects: t.tickets, utxos }
}

async fn objects_input(f: &Fixture, a: &axum::Router, uri: &str) -> ObjectsCheckInput {
    let (s, _, b) = get(a, uri).await;
    assert_eq!(s, 200, "{uri}: {b}");
    let o = parse_objects(&b).unwrap();
    let utxos = node_utxos(&f.ch, &o.objects);
    ObjectsCheckInput { covenant_id: cov_hex(&f.ch), objects: o.objects, utxos }
}

fn refused(what: &str, r: lottery::web::Result<impl std::fmt::Debug>) {
    match r {
        Err(e) => {
            assert_eq!(e.code, Code::ChainMismatch, "{what}: refused with {} ({}), not chain_mismatch", e.code, e.detail);
            assert_eq!(e.code.message(), "The indexer's copy does not match the chain.");
            println!("  {what}: chain_mismatch ({})", e.detail);
        }
        Ok(v) => panic!("{what}: the doctored response passed the page's check: {v:?}"),
    }
}

/// The positive control: the honest server's every answer passes the page's checks, with
/// the statuses the chain shows (round 1 drawn, 2 closing, 3 open), the roots rebuilt, and
/// round 1's numbers final from its anchor proof.
#[tokio::test]
async fn honest_answers_pass_every_check() {
    let f = fixture("honest");
    let a = doctored(&f, None);
    for (r, want) in [(1, RoundStatus::Drawn), (2, RoundStatus::Closing), (3, RoundStatus::Open)] {
        let i = round_input(&f, &a, r).await;
        let c = check_round(NET, &i).unwrap_or_else(|e| panic!("round {r}: {e}"));
        assert_eq!((c.status, c.status_checked), (want, true), "round {r}");
        assert!(c.roots.is_some() && c.purchases.is_some(), "round {r}: purchases checked against the chain's roots");
        if r == 1 {
            let w = c.winning.expect("round 1's numbers from its proof");
            assert_eq!(w.status, web::dto::NumbersStatus::Final);
        }
        println!("round {r}: {:?}, {} objects, roots {}", c.status, c.objects.len(), c.roots.as_ref().map(|x| x.roots.len()).unwrap_or(0));
    }
    let t = check_objects(NET, &tickets_input(&f, &a).await).unwrap();
    assert!(!t.is_empty());
    for uri in ["/v1/vault".to_string(), format!("/v1/positions?owner={}", addr(&depositor()))] {
        let o = check_objects(NET, &objects_input(&f, &a, &uri).await).unwrap();
        assert!(!o.is_empty(), "{uri}");
    }
    let (_, _, bundle) = get(&a, "/v1/instance").await;
    let hash = web::convert::hex(&bundle_hash(bundle.as_bytes()));
    let inst = check_instance(NET, &InstanceCheckInput { bundle, covenant_id: cov_hex(&f.ch), bundle_hash: Some(hash) }).unwrap();
    assert_eq!(inst.k, 4);
}

/// Every doctored mode, on every response it touches, is refused with `chain_mismatch`; and
/// each doctored response really differs from the honest one (the doctor did lie).
#[tokio::test]
async fn every_doctored_answer_is_refused() {
    let f = fixture("doctored");
    let honest = doctored(&f, None);
    for m in ALL {
        println!("mode {}", m.name());
        let a = doctored(&f, Some(m));
        let mut lied = 0;
        for r in [1u32, 2, 3] {
            let good = round_input(&f, &honest, r).await;
            let bad = round_input(&f, &a, r).await;
            let differs = serde_json::to_string(&good.round).unwrap() != serde_json::to_string(&bad.round).unwrap()
                || good.purchases != bad.purchases
                || good.anchor_proof != bad.anchor_proof;
            if differs {
                lied += 1;
                refused(&format!("{} round {r}", m.name()), check_round(NET, &bad));
            } else {
                check_round(NET, &bad).unwrap_or_else(|e| panic!("{} round {r}: unchanged answer refused: {e}", m.name()));
            }
        }
        if m == Mode::WrongState {
            refused("wrong_state tickets", check_objects(NET, &tickets_input(&f, &a).await));
            refused("wrong_state vault", check_objects(NET, &objects_input(&f, &a, "/v1/vault").await));
            let uri = format!("/v1/positions?owner={}", addr(&depositor()));
            refused("wrong_state positions", check_objects(NET, &objects_input(&f, &a, &uri).await));
        }
        let want = match m {
            Mode::WrongState | Mode::WrongTree | Mode::MissingPurchase | Mode::ExtraPurchase => 3,
            Mode::WrongWinning | Mode::WrongAnchorProof => 1,
        };
        assert_eq!(lied, want, "{}: rounds whose answer the doctor changed", m.name());
    }
}

/// The builders refuse doctored purchases too (the page never gets as far as a transaction):
/// a buy on round 3's shard 2 from its doctored purchase list, and a claim of round 1 from
/// its doctored purchase lists, are `tree_mismatch` (the F1 builder check; spec 6.3).
#[tokio::test]
async fn builders_refuse_doctored_purchases() {
    let f = fixture("builders");
    for m in [Mode::WrongTree, Mode::MissingPurchase, Mode::ExtraPurchase] {
        let a = doctored(&f, Some(m));
        // round 3's shard 2 (the only one with purchases in round 3).
        let (_, _, b) = get(&a, "/v1/rounds/3/shards/2").await;
        let shard_list = parse_shard(&b).unwrap();
        let (_, _, b) = get(&a, "/v1/rounds/3").await;
        let round = parse_round(&b).unwrap();
        let utxos = node_utxos(&f.ch, &round.objects);
        let objs = check_objects(NET, &ObjectsCheckInput { covenant_id: cov_hex(&f.ch), objects: round.objects.clone(), utxos }).unwrap();
        let shard = objs.iter().find(|o| matches!(&o.view, web::views::StateView::Shard(s) if s.shard_index == "2")).expect("shard 2").object.clone();
        let i = buy_input(&f, shard, shard_list.purchases.clone());
        let e = web::api::build_buy(NET, &i).expect_err("doctored purchases built a buy");
        assert!(matches!(e.code, Code::TreeMismatch | Code::PurchaseInvalid), "{}: buy refused with {e}", m.name());
        println!("  {} buy: {} ({})", m.name(), e.code, e.detail);
    }
}

fn buy_input(f: &Fixture, shard: web::dto::CovUtxo, purchases: String) -> web::dto::BuyInput {
    // A wallet coin as a node would show it (the tree check comes before any coin is spent).
    let spk = lottery::buy::funding_spk();
    let coin = web::dto::Utxo {
        transaction_id: "aa".repeat(32),
        index: 0,
        amount: (100 * lottery::buy::KAS).to_string(),
        script_public_key: web::convert::spk_hex(&spk),
        block_daa_score: "1000".into(),
        is_coinbase: false,
        covenant_id: None,
    };
    let change = NET.address(&spk).unwrap();
    web::dto::BuyInput {
        covenant_id: cov_hex(&f.ch),
        shard,
        purchases,
        bundle: vec![web::dto::Combination { main: [1, 2, 3, 4, 5], bonus: 1 }],
        payout: addr(&depositor()),
        referrer: None,
        funds: web::dto::Funds { coins: vec![coin], change, fee_rate: 100 },
    }
}

/// F5 (J6 "Verify in this browser"): `verifyRound` names each lie as its own item. On the honest
/// server no item is a mismatch (round 1's numbers match its pools); under each doctored mode the
/// items the lie touches, and only those, are mismatches: every object's state (wrong_state), the
/// one shard tree the doctor changed (wrong_tree, missing_purchase, extra_purchase), the winning
/// numbers (wrong_winning, wrong_anchor_proof).
#[tokio::test]
async fn verify_names_each_lie_as_its_own_item() {
    use web::rounds::{VerifyKind as K, VerifyResult as V, verify_round};
    let f = fixture("verify");
    let honest = doctored(&f, None);
    for r in [1u32, 2, 3] {
        let v = verify_round(NET, &round_input(&f, &honest, r).await).unwrap();
        let shown: Vec<_> = v.items.iter().map(|x| (x.kind, x.result, x.shard)).collect();
        println!("honest round {r}: {shown:?}");
        assert!(v.consistent, "honest round {r}: {:?}", v.items.iter().filter(|x| x.result == V::Mismatch).collect::<Vec<_>>());
        assert!(v.items.iter().any(|x| x.kind == K::Status && x.result == V::Match), "round {r}: the status is the chain's");
        assert_eq!(v.items.iter().filter(|x| x.kind == K::Tree && x.result == V::Match).count(), 4, "round {r}: k = 4 trees rebuilt");
        if r == 1 {
            assert!(v.items.iter().any(|x| x.kind == K::Numbers && x.result == V::Match), "round 1's numbers recomputed from its proof");
            assert!(v.items.iter().any(|x| x.kind == K::Anchor && x.result == V::NotCheckable), "no node block given: X.hash not checkable");
        }
    }
    for m in ALL {
        let a = doctored(&f, Some(m));
        for r in [1u32, 2, 3] {
            let bad = round_input(&f, &a, r).await;
            let good = round_input(&f, &honest, r).await;
            let lied = serde_json::to_string(&good.round).unwrap() != serde_json::to_string(&bad.round).unwrap() || good.purchases != bad.purchases || good.anchor_proof != bad.anchor_proof;
            let v = verify_round(NET, &bad).unwrap();
            let wrong: Vec<_> = v.items.iter().filter(|x| x.result == V::Mismatch).map(|x| (x.kind, x.shard)).collect();
            println!("  {} round {r}: mismatches {wrong:?}", m.name());
            if !lied {
                assert!(wrong.is_empty(), "{} round {r}: an unchanged answer shows a mismatch", m.name());
                continue;
            }
            assert!(!v.consistent);
            match m {
                Mode::WrongState => {
                    assert!(wrong.iter().all(|(k, _)| *k == K::State), "{wrong:?}");
                    assert_eq!(wrong.len(), bad.round.objects.len(), "every object's state");
                }
                Mode::WrongTree | Mode::MissingPurchase | Mode::ExtraPurchase => {
                    assert_eq!(wrong.len(), 1, "{wrong:?}");
                    assert_eq!(wrong[0].0, K::Tree);
                    let n = wrong[0].1.unwrap() as usize;
                    assert_ne!(good.purchases.as_ref().unwrap().shards[n], bad.purchases.as_ref().unwrap().shards[n], "the shard the doctor changed");
                }
                Mode::WrongWinning | Mode::WrongAnchorProof => assert_eq!(wrong, vec![(K::Numbers, None)], "{}", m.name()),
            }
        }
    }
}
