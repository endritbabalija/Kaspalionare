//! F2/F3 review regressions (the reviewer's probe `review_f3_probe.rs`, inverted): what
//! `checkIndexerRound` may return as checked when the indexer serves only part of a round.
//!
//! - M1: for an open round, the purchase lists come back as checked only when every one of the k
//!   lists was compared with a live shard read from the node (and k with round r's live
//!   controller); the roots are keyed by shard index. Before the fix, an indexer that left out
//!   the shards whose lists it lied about got all k lists returned as checked, and
//!   `web/src/store/roundCopy.ts` stored them in `webstore/1`.
//! - S4: a live controller of round r means round r is open, whatever the indexer says; an open
//!   round served without its shards could pass as drawn (`statusChecked: false`, and the round
//!   card showed the label anyway).
//!
//! Run: `cargo test -j 2 -p indexer-api --test review_f3 -- --nocapture`.

mod common;

use common::*;
use lottery::web::Code;
use web::convert::Net;
use web::indexer::*;
use web::views::{RoleName, StateView};

const NET: Net = Net::Testnet10;
/// One combination (1 2 3 4 5, bonus 1) that nobody bought on shard 0.
const POISON: &str = "010203040501";

async fn inputs(f: &Fixture) -> (ApiRound, ApiPurchases) {
    let a = app(&f.cfg);
    let (s, _, b) = get(&a, "/v1/rounds/3").await;
    assert_eq!(s, 200, "{b}");
    let round = parse_round(&b).unwrap();
    let (s, _, b) = get(&a, "/v1/rounds/3/purchases").await;
    assert_eq!(s, 200, "{b}");
    (round, parse_purchases(&b).unwrap())
}

fn check(f: &Fixture, round: &ApiRound, p: Option<&ApiPurchases>) -> lottery::web::Result<CheckedRound> {
    let utxos = node_utxos(&f.ch, &round.objects);
    check_round(NET, &RoundCheckInput { covenant_id: cov_hex(&f.ch), round: round.clone(), utxos, purchases: p.cloned(), anchor_proof: None, anchor_block: None })
}

fn shard_index(f: &Fixture, o: &ApiObject) -> Option<String> {
    let utxos = node_utxos(&f.ch, std::slice::from_ref(o));
    let c = check_objects(NET, &ObjectsCheckInput { covenant_id: cov_hex(&f.ch), objects: vec![o.clone()], utxos }).ok()?;
    match &c[0].view {
        StateView::Shard(s) => Some(s.shard_index.clone()),
        _ => None,
    }
}

fn refused(r: lottery::web::Result<CheckedRound>, what: &str) -> String {
    let e = r.expect_err(what);
    assert_eq!(e.code, Code::ChainMismatch, "{what}: {e}");
    e.detail
}

#[tokio::test]
async fn open_round_lists_are_checked_for_every_shard_or_not_returned() {
    let f = fixture("review-f3-m1");
    let (round, honest) = inputs(&f).await;
    assert_eq!(honest.shards[0], "", "fixture: shard 0 of round 3 has no purchases");
    let mut poisoned = honest.clone();
    poisoned.shards[0] = POISON.into();

    // Every object served: the honest lists pass, all k of them, roots keyed by shard index.
    let c = check(&f, &round, Some(&honest)).unwrap();
    assert_eq!((c.status, c.status_checked), (RoundStatus::Open, true));
    let roots = c.roots.as_ref().expect("roots").roots.clone();
    assert_eq!(roots.len(), c.k as usize);
    assert_eq!(c.purchases.as_ref().map(Vec::len), Some(c.k as usize));
    for o in &c.objects {
        if let StateView::Shard(s) = &o.view {
            let i: usize = s.shard_index.parse().unwrap();
            assert_eq!(roots[i], s.root, "roots[{i}] is shard {i}'s root");
        }
    }
    // Served in another order, the roots still come out by shard index.
    let mut shuffled = round.clone();
    shuffled.objects.reverse();
    assert_eq!(check(&f, &shuffled, Some(&honest)).unwrap().roots.unwrap().roots, roots);

    // Control: every shard served, the poisoned list is refused.
    println!("control (all served): refused: {}", refused(check(&f, &round, Some(&poisoned)), "poisoned, all served"));

    // The lie: only shard 2 served (with the controller and params). Neither the poisoned lists
    // nor the honest ones come back as checked.
    let mut thin = round.clone();
    thin.objects.retain(|o| o.role != RoleName::Shard || shard_index(&f, o).as_deref() == Some("2"));
    assert!(thin.objects.len() < round.objects.len());
    println!("thin, poisoned: refused: {}", refused(check(&f, &thin, Some(&poisoned)), "poisoned, shard 0 omitted"));
    println!("thin, honest: refused: {}", refused(check(&f, &thin, Some(&honest)), "honest, shards omitted"));
    // Without purchases the thin round is still a checked open round (the controller shows it).
    let c = check(&f, &thin, None).unwrap();
    assert_eq!((c.status, c.status_checked, c.purchases.is_none(), c.roots.is_none()), (RoundStatus::Open, true, true, true));

    // A shard served twice in place of another.
    let mut twice = round.clone();
    let s2 = round.objects.iter().find(|o| shard_index(&f, o).as_deref() == Some("2")).unwrap().clone();
    let s0 = twice.objects.iter().position(|o| shard_index(&f, o).as_deref() == Some("0")).unwrap();
    twice.objects[s0] = s2;
    println!("shard 2 twice, no shard 0: refused: {}", refused(check(&f, &twice, Some(&honest)), "a shard twice"));

    // The controller left out: k and m are then the indexer's word, so no list is returned.
    let mut headless = round.clone();
    headless.objects.retain(|o| o.role != RoleName::Controller);
    println!("no controller: refused: {}", refused(check(&f, &headless, Some(&honest)), "no controller"));
}

#[tokio::test]
async fn open_round_cannot_pass_as_drawn_or_closing() {
    let f = fixture("review-f3-s4");
    let (round, _) = inputs(&f).await;
    for (status, shards) in [(RoundStatus::Drawn, false), (RoundStatus::Closing, false), (RoundStatus::Void, false), (RoundStatus::Drawn, true)] {
        let mut lie = round.clone();
        if !shards {
            lie.objects.retain(|o| o.role != RoleName::Shard);
        }
        lie.status = status;
        let d = refused(check(&f, &lie, None), &format!("{status:?}, shards served: {shards}"));
        println!("{status:?} (shards served: {shards}): refused: {d}");
    }
    // The controller alone shows the round open, checked.
    let mut bare = round.clone();
    bare.objects.retain(|o| o.role != RoleName::Shard);
    let c = check(&f, &bare, None).unwrap();
    assert_eq!((c.status, c.status_checked), (RoundStatus::Open, true));
}
