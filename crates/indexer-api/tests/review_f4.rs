//! F4 review S1: "void" only where the chain does not contradict it. The page showed a round void
//! on the indexer's word alone. `checkIndexerRound` now says `voidSupported` only when the
//! indexer says void, the live drawer it serves is past the round (the round was drawn or
//! voided), and no live object of the round that a void cannot leave was served (its marker, a
//! pool, its open controller or a shard). A settlement record alone does not tell a draw from a
//! void (a void with pending vault requests writes one too), so it no longer makes a void
//! response a mismatch.
//!
//! What the chain cannot show (recorded for the owner, handoff "F4 review and fixes"): an indexer
//! that hides a drawn round's pools and says void passes this check; the page then refuses the
//! void only from its own copy (a live marker, or a kept round result).
//!
//! Run: `cargo test -j 2 -p indexer-api --test review_f4 -- --nocapture`.

mod common;

use common::*;
use lottery::web::Code;
use web::convert::Net;
use web::indexer::*;
use web::views::RoleName;

const NET: Net = Net::Testnet10;

async fn round(f: &Fixture, r: u32) -> ApiRound {
    let (s, _, b) = get(&app(&f.cfg), &format!("/v1/rounds/{r}")).await;
    assert_eq!(s, 200, "{b}");
    parse_round(&b).unwrap()
}

fn check(f: &Fixture, round: &ApiRound) -> lottery::web::Result<CheckedRound> {
    let utxos = node_utxos(&f.ch, &round.objects);
    check_round(NET, &RoundCheckInput { covenant_id: cov_hex(&f.ch), round: round.clone(), utxos, purchases: None, anchor_proof: None, anchor_block: None })
}

fn void_of(a: &ApiRound, keep: impl Fn(&ApiObject) -> bool) -> ApiRound {
    let mut v = a.clone();
    v.status = RoundStatus::Void;
    v.objects.retain(keep);
    // A consistent lie: the draw record says void too.
    if let Some(d) = &mut v.draw {
        d.void = true;
        d.winning = None;
        d.pools.clear();
    }
    v
}

#[tokio::test]
async fn void_is_supported_only_when_no_live_object_contradicts_it() {
    let f = fixture("review-f4-s1");
    // Round 1 is drawn: honest, no void.
    let r1 = round(&f, 1).await;
    let c = check(&f, &r1).unwrap();
    assert_eq!((c.status, c.void_supported), (RoundStatus::Drawn, false));
    // Said void with its pools served: refused.
    let e = check(&f, &void_of(&r1, |_| true)).unwrap_err();
    assert_eq!(e.code, Code::ChainMismatch);
    println!("round 1 void with its pools: refused: {}", e.detail);
    // Said void with the drawer (round 2, past round 1) and the record, the pools hidden: the
    // chain cannot contradict it (the residual, see the header).
    let hidden = void_of(&r1, |o| o.role != RoleName::Pool);
    let c = check(&f, &hidden).unwrap();
    assert!(c.void_supported, "{c:?}");
    println!("round 1 void with its pools hidden: voidSupported (the indexer can hide pools)");
    // The drawer left out too: nothing shows the round resolved, so no void.
    let c = check(&f, &void_of(&r1, |o| o.role != RoleName::Pool && o.role != RoleName::Drawer)).unwrap();
    assert!(!c.void_supported);
    // Round 2 is closing: void with its marker served is refused; with it hidden, the drawer
    // (round 2, not past it) shows the round unresolved: no void.
    let r2 = round(&f, 2).await;
    assert_eq!(check(&f, &void_of(&r2, |_| true)).unwrap_err().code, Code::ChainMismatch);
    let c = check(&f, &void_of(&r2, |o| o.role != RoleName::Marker)).unwrap();
    assert!(!c.void_supported, "{c:?}");
    // Round 3 is open: its controller refuses a void.
    let r3 = round(&f, 3).await;
    assert_eq!(check(&f, &void_of(&r3, |o| o.role != RoleName::Shard)).unwrap_err().code, Code::ChainMismatch);
}
