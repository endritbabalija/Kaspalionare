//! F4 review regressions (the reviewer's probe `review_f4_probe.rs`, inverted, and the fixes'
//! own tests). Each guards money or data:
//! - M1 `a_return_is_offered_only_when_no_unclaimed_index_can_still_be_claimed`: design 7.5 (a
//!   return forfeits the unclaimed winning indices): path a after the claim deadline with the pool
//!   live, path b from T + D_RET (D102), an unknown result, an unknown tier; offered for a ticket
//!   whose wins are all claimed, and in a void.
//! - M2 `a_win_whose_pool_the_copy_does_not_list_is_unknown_never_expired` (the probe):
//!   "expired" only for a listed pool the chain shows absorbed; and
//!   `with_the_marker_kept_a_drawn_copy_lists_every_pool_of_the_draw`: `round_pools` from the
//!   marker (`game::pool_set`), `mergeCopy` (never a stage back, never fewer pools), the record
//!   check of a backup.
//! - S3 `back_with_last_claim_only_when_every_unclaimed_index_is_claimable`.
//! - S4 `a_closed_copy_whose_marker_is_spent_is_incomplete`.
//! - Notes: `an_f3_record_is_left_out_of_a_backup_not_fatal`, `limits_agree`.
//!
//! Run: `cargo test -j 2 -p web --release --test f4_review -- --nocapture`.

mod common;

use common::{Input, input, live_utxo, marker_for, stored_object};
use lottery::index::{Role, State};
use lottery::state::PoolState;
use lottery::web::Code;
use lottery::web::fixtures::{calls, claim_calls};
use lottery::web::verify;
use web::backup::{self, BackupReadInput, BackupWriteInput, CopyCheckInput, CopyStatus, MergeCopyInput};
use web::convert::{Net, unhex};
use web::dto::*;
use web::tickets::{self, CardStage, ComboStatus, ReturnPathName, TicketCard, TicketCardInput};
use web::webstore::{RoundCopyStage, StoredObject, StoredResult, StoredRound, StoredTicket};

const N: Net = Net::Testnet10;

fn claim(name: &str) -> ClaimInput {
    let mut all: Vec<_> = calls().unwrap().into_iter().map(|(n, c, _)| (n, c)).collect();
    all.extend(claim_calls().unwrap().into_iter().map(|(n, c, _)| (n, c)));
    let (_, c) = all.into_iter().find(|(n, _)| n == name).unwrap();
    match input(&c) {
        Input::Claim(i) => i,
        _ => unreachable!(),
    }
}

fn pool_state(c: &CovUtxo) -> PoolState {
    match verify::decode_state(Role::Pool, &unhex("state", &c.state).unwrap()).unwrap() {
        State::Pool(p) => p,
        _ => unreachable!(),
    }
}

fn win_of(p: &PoolState) -> Combination {
    let w = p.win;
    Combination { main: [w[0], w[1], w[2], w[3], w[4]], bonus: w[5] }
}

struct Card<'a> {
    i: &'a ClaimInput,
    ticket: CovUtxo,
    pools: Vec<CovUtxo>,
    winning: Option<Combination>,
    absorbed: Vec<u8>,
    complete: bool,
    void: bool,
    now: i64,
}

impl<'a> Card<'a> {
    fn new(i: &'a ClaimInput, now: i64) -> Self {
        Self { i, ticket: i.ticket.clone(), pools: i.pools.clone(), winning: None, absorbed: vec![], complete: false, void: false, now }
    }
    fn get(&self) -> TicketCard {
        tickets::ticket_card(N, &TicketCardInput {
            covenant_id: self.i.covenant_id.clone(),
            ticket: self.ticket.clone(),
            pools: self.pools.clone(),
            absorbed: self.absorbed.clone(),
            complete: self.complete,
            purchases: Some(self.i.purchases.clone()),
            winning: self.winning,
            claim_deadline: None,
            void: self.void,
            virtual_daa_score: self.now.to_string(),
        })
        .unwrap()
    }
}

/// The ticket after its PAY claim of index 0 (claimed, then a non-winning index): the claim's
/// successor output as a live UTXO.
fn claimed_successor(i: &ClaimInput) -> CovUtxo {
    let built = web::api::build_claim(N, i).unwrap();
    let f = tickets::claim_figures(N, &tickets::ClaimFiguresInput {
        covenant_id: i.covenant_id.clone(),
        ticket: i.ticket.clone(),
        pools: i.pools.clone(),
        purchases: i.purchases.clone(),
        index: 0,
        built: built.clone(),
    })
    .unwrap();
    let next = f.ticket_next.expect("PAY leaves a successor");
    let s = verify::decode_state(Role::Ticket, &unhex("state", &next.state).unwrap()).unwrap();
    live_utxo(&i.covenant_id, &s, 0x5c, 1, built.summary.outputs[1].value.parse().unwrap())
}

fn deadlines(i: &ClaimInput) -> (i64, i64) {
    let p = pool_state(&i.pools[0]);
    let t = match verify::decode_state(Role::Ticket, &unhex("state", &i.ticket.state).unwrap()).unwrap() {
        State::Ticket(t) => t,
        _ => unreachable!(),
    };
    (p.deadline, t.deadline + lottery::game::D_RET)
}

#[test]
fn a_return_is_offered_only_when_no_unclaimed_index_can_still_be_claimed() {
    let i = claim("claim k=4");
    let (deadline, alone) = deadlines(&i);
    // Path a: past the claim deadline, the pool still live, index 0 claimable (the probe's P5).
    let c = Card::new(&i, deadline + 100).get();
    assert_eq!(c.combos[0].status, ComboStatus::Claimable);
    assert!(c.returnable.is_none(), "path a while index 0 is claimable would forfeit its prize");
    // Path b (D102: from T + D_RET, possibly before a late close's claim deadline), index 0 claimable.
    let c = Card::new(&i, alone + 1).get();
    assert!(c.returnable.is_none(), "path b while index 0 is claimable would forfeit its prize");
    // The result unknown to the page (no copy of the draw): nothing offered.
    let mut x = Card::new(&i, alone + 1);
    x.pools = vec![];
    let c = x.get();
    assert_eq!(c.stage, CardStage::Open);
    assert!(c.returnable.is_none(), "an unknown result may hold a win");
    // Drawn, the winning tier's pool not shown: unknown, nothing offered.
    x.winning = Some(win_of(&pool_state(&i.pools[0])));
    let c = x.get();
    assert_eq!(c.combos[0].status, ComboStatus::Unknown);
    assert!(c.returnable.is_none(), "an unknown tier may be claimable");
    // Every win claimed (the PAY successor: claimed, no win): both paths offered.
    let mut y = Card::new(&i, deadline + 100);
    y.ticket = claimed_successor(&i);
    let c = y.get();
    assert_eq!((c.combos[0].status, c.combos[1].status), (ComboStatus::Claimed, ComboStatus::NoWin));
    assert_eq!(c.returnable.as_ref().map(|r| r.path), Some(ReturnPathName::WithPool));
    y.now = alone + 1;
    assert_eq!(y.get().returnable.map(|r| r.path), Some(ReturnPathName::Alone));
    // The win expired (its pool absorbed): offered.
    let mut z = Card::new(&i, alone + 1);
    z.pools = vec![];
    z.winning = Some(win_of(&pool_state(&i.pools[0])));
    z.absorbed = vec![pool_state(&i.pools[0]).tier];
    let c = z.get();
    assert_eq!(c.combos[0].status, ComboStatus::Expired);
    assert_eq!(c.returnable.map(|r| r.path), Some(ReturnPathName::Alone));
    // A void: nothing to claim, path b offered.
    let mut v = Card::new(&i, alone + 1);
    v.pools = vec![];
    v.void = true;
    assert_eq!(v.get().returnable.map(|r| r.path), Some(ReturnPathName::Alone));
}

#[test]
fn back_with_last_claim_only_when_every_unclaimed_index_is_claimable() {
    // [claimable, no win]: index 1 can never be claimed, so no claim is FINAL; the value comes
    // back by return_deposit() (the probe's P4: the claim is PAY).
    let i = claim("claim k=4");
    let c = Card::new(&i, 1).get();
    assert_eq!(c.combos[0].claim.as_ref().map(|x| x.mode), Some(tickets::ClaimModeName::Pay));
    assert!(!c.back_with_last_claim);
    // [claimable, claimed]: the claim is FINAL and pays the value back.
    let f = claim("claim final");
    let c = Card::new(&f, 1).get();
    assert_eq!(c.combos.iter().find(|x| x.status == ComboStatus::Claimable).and_then(|x| x.claim.as_ref()).map(|x| x.mode), Some(tickets::ClaimModeName::Final));
    assert!(c.back_with_last_claim);
    // [claimed, no win] and [unknown, ...]: no.
    let mut y = Card::new(&i, 1);
    y.ticket = claimed_successor(&i);
    assert!(!y.get().back_with_last_claim);
    let mut u = Card::new(&f, 1);
    u.pools = vec![];
    u.winning = Some(win_of(&pool_state(&f.pools[0])));
    assert!(!u.get().back_with_last_claim);
}

/// A drawn copy of the `claim k=4` round listing `pools`, and its ticket's record.
fn drawn_copy(i: &ClaimInput, pools: Vec<StoredObject>) -> StoredRound {
    let p = pool_state(&i.pools[0]);
    let roots = web::api::check_roots(&RootsInput { covenant_id: i.covenant_id.clone(), against: i.pools[0].clone(), purchases: i.purchases.clone() }).unwrap();
    StoredRound {
        key: format!("{}:{}", i.covenant_id, p.round),
        covenant_id: i.covenant_id.clone(),
        network: "testnet-10".into(),
        round: p.round.to_string(),
        m: p.m as u32,
        k: p.k as u32,
        stage: RoundCopyStage::Drawn,
        purchases: i.purchases.clone(),
        roots: Some(roots.roots),
        against: pools.first().cloned(),
        result: Some(StoredResult { winning: win_of(&p), claim_deadline: p.deadline.to_string(), pools }),
        marker: None,
        checked_ms: "1".into(),
    }
}

fn own_pool(i: &ClaimInput) -> StoredObject {
    stored_object(&State::Pool(pool_state(&i.pools[0])))
}

/// Another pool of the same round (the next tier), live: what an indexer can serve alone.
fn other_pool(i: &ClaimInput) -> (StoredObject, CovUtxo) {
    let mut o = pool_state(&i.pools[0]);
    o.tier += 1;
    let s = State::Pool(o);
    (stored_object(&s), live_utxo(&i.covenant_id, &s, 0x77, 0, 3 * lottery::buy::KAS))
}

fn check(i: &ClaimInput, r: &StoredRound, live: Vec<Utxo>, now: i64) -> lottery::web::Result<backup::CopyCheck> {
    backup::check_copy(N, &CopyCheckInput { covenant_id: i.covenant_id.clone(), round: r.clone(), live, virtual_daa_score: Some(now.to_string()) })
}

fn card_of(i: &ClaimInput, r: &StoredRound, c: &backup::CopyCheck, now: i64) -> TicketCard {
    tickets::ticket_card(N, &TicketCardInput {
        covenant_id: i.covenant_id.clone(),
        ticket: i.ticket.clone(),
        pools: c.pools.clone(),
        absorbed: c.absorbed.clone(),
        complete: c.complete,
        purchases: Some(r.purchases.clone()),
        winning: r.result.as_ref().map(|x| x.winning),
        claim_deadline: r.result.as_ref().map(|x| x.claim_deadline.clone()),
        void: false,
        virtual_daa_score: now.to_string(),
    })
    .unwrap()
}

#[test]
fn a_win_whose_pool_the_copy_does_not_list_is_unknown_never_expired() {
    let i = claim("claim k=4");
    let (deadline, _) = deadlines(&i);
    let tier = pool_state(&i.pools[0]).tier;
    let (other, other_live) = other_pool(&i);
    // The probe: the copy lists only another tier's pool (the indexer served one pool); the node
    // holds both. The record check and checkCopy pass (verified against the other pool) ...
    let r = drawn_copy(&i, vec![other.clone()]);
    backup::check_records(N, &i.covenant_id, &[], std::slice::from_ref(&r), &[]).expect("a subset is a valid record without the marker");
    let live = vec![other_live.utxo.clone(), i.pools[0].utxo.clone()];
    let c = check(&i, &r, live.clone(), 1).unwrap();
    assert_eq!(c.status, CopyStatus::Verified);
    assert!(!c.complete, "without the marker the copy cannot show it lists every pool");
    // ... but the win whose pool it does not list is unknown, never expired.
    let card = card_of(&i, &r, &c, 1);
    assert_eq!(card.combos[0].tier, Some(tier));
    assert_eq!(card.combos[0].status, ComboStatus::Unknown, "the live tier-{tier} pool is not in the copy");
    assert!(card.returnable.is_none() && !card.back_with_last_claim);
    // Control: with the pool listed, the same win is claimable.
    let full = drawn_copy(&i, vec![own_pool(&i), other.clone()]);
    let c = check(&i, &full, live.clone(), 1).unwrap();
    assert_eq!(card_of(&i, &full, &c, 1).combos[0].status, ComboStatus::Claimable);
    // Listed and not live before its claim deadline: unknown (the node's answer is incomplete).
    let c = check(&i, &full, vec![other_live.utxo.clone()], deadline - 1).unwrap();
    assert_eq!(card_of(&i, &full, &c, deadline - 1).combos[0].status, ComboStatus::Unknown);
    // Listed and not live after its claim deadline: absorbed, so expired.
    let c = check(&i, &full, vec![other_live.utxo.clone()], deadline + 1).unwrap();
    assert_eq!(c.absorbed, vec![tier]);
    assert_eq!(card_of(&i, &full, &c, deadline + 1).combos[0].status, ComboStatus::Expired);
    // Nothing live: gone after the deadline, incomplete before it.
    assert_eq!(check(&i, &full, vec![], deadline + 1).unwrap().status, CopyStatus::Gone);
    assert_eq!(check(&i, &full, vec![], deadline - 1).unwrap().status, CopyStatus::Incomplete);
    // The probe's final control: an unlisted tier with no absorption evidence is never expired,
    // even past the deadline.
    let c = check(&i, &r, vec![other_live.utxo.clone()], deadline + 1).unwrap();
    assert_eq!(card_of(&i, &r, &c, deadline + 1).combos[0].status, ComboStatus::Unknown);
}

/// The `claim k=4` round's marker, the pool set its draw created and those pools as live UTXOs.
fn marked(i: &ClaimInput) -> (StoredObject, Vec<StoredObject>, Vec<Utxo>) {
    let p = pool_state(&i.pools[0]);
    let mk = marker_for(&p);
    let lists: Vec<Vec<u8>> = i.purchases.iter().map(|x| unhex("p", x).unwrap()).collect();
    let set = backup::round_pools(&mk, &p, &lists).unwrap();
    let objs: Vec<StoredObject> = set.iter().map(|x| stored_object(&State::Pool(x.clone()))).collect();
    let live: Vec<Utxo> = set
        .iter()
        .enumerate()
        .map(|(n, x)| if x.tier == p.tier { i.pools[0].utxo.clone() } else { live_utxo(&i.covenant_id, &State::Pool(x.clone()), 0x60 + n as u8, n as u32, 2 * lottery::buy::KAS).utxo })
        .collect();
    (stored_object(&State::Marker(mk)), objs, live)
}

#[test]
fn with_the_marker_kept_a_drawn_copy_lists_every_pool_of_the_draw() {
    let i = claim("claim k=4");
    let p = pool_state(&i.pools[0]);
    let (marker, set, live) = marked(&i);
    let tiers: Vec<u8> = set.iter().map(|o| pool_state(&CovUtxo { utxo: live[0].clone(), state: o.state.clone() }).tier).collect();
    println!("the draw's pools: tiers {tiers:?}");
    assert_eq!(set.len(), 5, "the five lower tiers; nobody holds the winning numbers");
    assert!(set.contains(&own_pool(&i)));
    // The closed copy (checked against the live marker).
    let mut closed = drawn_copy(&i, vec![]);
    closed.stage = RoundCopyStage::Closed;
    closed.result = None;
    closed.against = Some(marker.clone());
    let merge = |prev: Option<&StoredRound>, next: &StoredRound| backup::merge_copy(N, &MergeCopyInput { covenant_id: i.covenant_id.clone(), prev: prev.cloned(), next: next.clone() });
    let kept = merge(None, &closed).unwrap();
    assert!(kept.changed);
    // The indexer serves the drawn round with one pool only: the kept marker completes the set.
    let thin = drawn_copy(&i, vec![own_pool(&i)]);
    let m = merge(Some(&kept.round), &thin).unwrap();
    assert_eq!(m.round.marker.as_ref(), Some(&marker));
    let pools = &m.round.result.as_ref().unwrap().pools;
    assert_eq!(pools.len(), 5);
    assert!(set.iter().all(|o| pools.contains(o)));
    // The record check of a backup takes it, and refuses the same copy with a pool dropped.
    backup::check_records(N, &i.covenant_id, &[], std::slice::from_ref(&m.round), &[]).unwrap();
    let mut short = m.round.clone();
    short.result.as_mut().unwrap().pools.pop();
    let e = backup::check_records(N, &i.covenant_id, &[], std::slice::from_ref(&short), &[]).unwrap_err();
    assert_eq!(e.code, Code::BackupInvalid, "{e}");
    // checkCopy: complete; a winning tier without a pool of the set is "no pool", not unknown.
    let c = check(&i, &m.round, live.clone(), 1).unwrap();
    assert_eq!((c.status, c.complete, c.pools.len()), (CopyStatus::Verified, true, 5));
    assert_eq!(card_of(&i, &m.round, &c, 1).combos[0].status, ComboStatus::Claimable);
    let nopool = tickets::ticket_card(N, &TicketCardInput { complete: true, pools: vec![], ..card_input(&i, &m.round) }).unwrap();
    assert_eq!(nopool.combos[0].status, ComboStatus::NoPool);
    // Never fewer pools: a later copy listing one pool keeps the five; a stage back keeps the drawn copy.
    let again = merge(Some(&m.round), &thin).unwrap();
    assert!(!again.changed);
    assert_eq!(again.round, m.round);
    let back = merge(Some(&m.round), &closed).unwrap();
    assert_eq!((back.round.stage, back.changed), (RoundCopyStage::Drawn, false));
    // Without the marker the pools are the union of what each copy listed.
    let (other, _) = other_pool(&i);
    let two = merge(Some(&drawn_copy(&i, vec![own_pool(&i), other.clone()])), &thin).unwrap();
    assert_eq!(two.round.result.as_ref().unwrap().pools.len(), 2, "never fewer pools than the kept copy");
    // A marker that does not explain the served pool (another slice share) is refused.
    let mut lie = marker_for(&p);
    lie.st[0] = 900;
    let mut bad = closed.clone();
    bad.against = Some(stored_object(&State::Marker(lie)));
    let kb = merge(None, &bad).unwrap();
    assert_eq!(merge(Some(&kb.round), &thin).unwrap_err().code, Code::CopyMismatch);
    // Frozen lists that differ between two copies are refused.
    let mut lists = thin.clone();
    let n = lists.purchases.iter().position(|l| l.len() >= 12).unwrap();
    lists.purchases[n] = lists.purchases[n][12..].to_string();
    assert!(merge(Some(&m.round), &lists).is_err());
}

fn card_input(i: &ClaimInput, r: &StoredRound) -> TicketCardInput {
    TicketCardInput {
        covenant_id: i.covenant_id.clone(),
        ticket: i.ticket.clone(),
        pools: i.pools.clone(),
        absorbed: vec![],
        complete: false,
        purchases: Some(r.purchases.clone()),
        winning: r.result.as_ref().map(|x| x.winning),
        claim_deadline: None,
        void: false,
        virtual_daa_score: "1".into(),
    }
}

#[test]
fn a_closed_copy_whose_marker_is_spent_is_incomplete() {
    let i = claim("claim k=4");
    let (marker, _, _) = marked(&i);
    let mut closed = drawn_copy(&i, vec![]);
    closed.stage = RoundCopyStage::Closed;
    closed.result = None;
    closed.against = Some(marker.clone());
    let mk = verify::decode_state(Role::Marker, &unhex("state", &marker.state).unwrap()).unwrap();
    let live = live_utxo(&i.covenant_id, &mk, 0x4d, 0, 5 * lottery::buy::KAS).utxo;
    // The probe's P3: the marker spent (the round drawn or void) is incomplete, never "complete".
    assert_eq!(check(&i, &closed, vec![], 1).unwrap().status, CopyStatus::Incomplete);
    // Control: the marker live, the lists against its frozen roots.
    assert_eq!(check(&i, &closed, vec![live], 1).unwrap().status, CopyStatus::Verified);
    // An F3 record (closed or drawn without the object it was checked against) is incomplete.
    let mut f3 = closed.clone();
    f3.against = None;
    assert_eq!(check(&i, &f3, vec![], 1).unwrap().status, CopyStatus::Incomplete);
    let mut f3d = drawn_copy(&i, vec![]);
    f3d.result = None;
    assert_eq!(check(&i, &f3d, vec![], 1).unwrap().status, CopyStatus::Incomplete);
}

#[test]
fn an_f3_record_is_left_out_of_a_backup_not_fatal() {
    let i = claim("claim k=4");
    let good = drawn_copy(&i, vec![own_pool(&i)]);
    let mut f3 = drawn_copy(&i, vec![]);
    f3.key = format!("{}:9", i.covenant_id);
    f3.round = "9".into();
    f3.result = None;
    let t: StoredTicket = StoredTicket {
        key: format!("{}:1", "ee".repeat(32)),
        covenant_id: i.covenant_id.clone(),
        network: "testnet-10".into(),
        round: good.round.clone(),
        shard_index: Some(0),
        ticket: i.ticket.clone(),
        bundle: tickets::ticket_card(N, &card_input(&i, &good)).unwrap().combos.iter().map(|c| c.combination).collect(),
        payout: tickets::ticket_card(N, &card_input(&i, &good)).unwrap().payout,
        buy: None,
        ended: None,
        pending: Some(web::webstore::StoredAction {
            kind: web::webstore::ActionKind::Claim,
            index: Some(0),
            signed: Signed { transaction: "{}".into(), id: "ab".repeat(32) },
            successor: None,
            successor_value: None,
            paid: None,
            saved_ms: "1".into(),
        }),
        saved_ms: "1".into(),
    };
    let w = backup::backup_write(N, &BackupWriteInput { covenant_id: i.covenant_id.clone(), created_ms: "1".into(), tickets: vec![t.clone()], rounds: vec![good.clone(), f3.clone()], pending: vec![] }).unwrap();
    assert_eq!(w.skipped, vec![f3.key.clone()], "the bad record is left out and named; the rest is written");
    assert!(!w.text.contains('\n'), "compact");
    let back = backup::backup_read(N, &BackupReadInput { covenant_id: i.covenant_id.clone(), text: w.text.clone() }).unwrap();
    assert_eq!(back.rounds, vec![good]);
    assert!(back.tickets[0].pending.is_none(), "a pending claim is not exported");
    assert_eq!(back.tickets[0].key, t.key);
}

#[test]
fn limits_agree() {
    let l = web::indexer::LIMITS;
    assert_eq!(l.backup as usize, backup::BACKUP_MAX);
    assert_eq!(l.worker_args as usize, backup::WORKER_ARGS_MAX);
    assert_eq!(l.node_objects, web::indexer::OBJECTS_MAX);
    assert!(backup::RECORDS_MAX >= web::indexer::OBJECTS_MAX as usize, "the page reads the node in batches of OBJECTS_MAX");
}
