//! M8 `lottery::index` tests (spec M8 A6, IX1-IX9), written before the module.
//!
//! S2a: IX1 (replay of a 3-round local game), IX4 (a missing buy marks the round
//! incomplete), IX5 (reorg), IX7 (classify every shape) against `lottery::index`.
//!
//! S2b: IX2 (`ClaimCase::from_store`, `DrawCase::from_store`, byte-identical to the
//! builders), IX3 (a claim built from a tampered index is refused on chain at the K-leaf
//! check), IX4's `from_store` refusal, IX6 (`index/1`, `wallet/1`, `campaign/1`,
//! `bundle/1` round trips and refusals), IX8 (`bundle::verify`), IX9 (`fee_for`).

mod common;

use std::collections::BTreeMap;

use common::m2::*;
use common::m4::{Case, first_failing_check4};
use common::m5::{chain5, deployer, refresh5_bases, settle_bases};
use harness::{Hash, TransactionOutpoint};
use lottery::anchor::AnchorProof;
use lottery::bundle::{self, Bundle};
use lottery::buy::{BuyCase, KAS, OwnerKey, Templates, b_max, fee_for, funding_spk};
use lottery::claim::{ClaimCase, ReturnCase, combo_for_tier, combo_matching};
use lottery::store::{self, Campaign, CheckJson, ClaimStore, Funding, IndexLog, IndexLogLine, IndexSnapshot, Payer, StepJson, TxRef, Wallet};
use lottery::close::CloseCase;
use lottery::combo::Combo;
use lottery::draw::{DrawCase, RoundResult};
use lottery::genesis::{GENESIS_FUNDING, GenesisCase, covenant_id};
use lottery::index::{Accepted, Event, Rebuild, Role, State, classify};
use lottery::ledger::Ledger;
use lottery::params::SetParamsCase;
use lottery::state::{Payout, TIER_JACKPOT, TIER_LOWER0, TIER_SECOND};
use lottery::vault::{DepositCase, SettleCase, WithdrawCase};

fn key(n: u8) -> OwnerKey {
    OwnerKey::Schnorr([0x40 + n; 32])
}

fn win6(ch: &Chain, seed: &[u8; 32]) -> [u8; 6] {
    let w = ch.winning(seed);
    [w.main[0], w.main[1], w.main[2], w.main[3], w.main[4], w.bonus]
}

/// Buy `bundle` on shard `i` at `daa` (funding b x price + 10 KAS), with or without a
/// referral output.
fn buy(ch: &mut Chain, i: usize, bundle: Vec<Combo>, with_ref: bool, daa: u64) {
    let price = ch.controller.1.price as u64;
    let (mut c, ops) = ch.buy_case_funded(i, bundle.len() as u64 * price + 10 * KAS);
    c.bundle = bundle;
    c.with_ref = with_ref;
    ch.buy(i, &c, ops, daa);
}

/// Settle every live request whose round has a live record (as m5_vault `settle_all`).
fn settle_all(ch: &mut Chain, now: u64) {
    let mut n = 0;
    while let Some(qi) = ch.requests.iter().position(|q| ch.records.iter().any(|r| r.1.round == q.1.round)) {
        ch.settle_now(qi, now + n);
        n += 10;
    }
}

/// The index of the live ticket created by the `n`-th buy of round `r` (in `tickets`).
fn ticket_of(ch: &Chain, round: i64, bundle0: [u8; 6]) -> usize {
    ch.tickets.iter().position(|t| t.1.round == round && t.1.bundle[..6] == bundle0).expect("ticket tracked")
}

/// The draws of the IX1 game: (round, case, transaction).
type Draws = Vec<(i64, DrawCase, lottery::buy::Final)>;

/// IX1's local game (spec M8 A6): three rounds on `chain5()` (K = 4, lower tiers
/// (400, 300, 300, 0, 0)) with an admin change (`set_params` in round 1, live in round 2),
/// an m change at round 1's close, deposits and withdrawals in rounds 1 and 2 (the
/// deployer's 95 % withdrawal is capped and carried), a referral, claims of a jackpot (PAY),
/// a lower tier (FINAL) and a round-2 holding, a ticket refresh, every request settled,
/// a `return_deposit_with_pool` on an expired round-1 pool, round 3 voided late with the
/// expired round-1 pools absorbed, and a `return_deposit_after`. Every step goes through the
/// builders and the engine (`Chain`); nothing is fabricated.
fn game() -> (Chain, Draws, Vec<State>) {
    let mut ch = chain5();
    let gstates = genesis_states(&ch);
    let mut draws: Draws = vec![];
    let m1 = ch.controller.1.m;
    // ---- Round 1.
    let seed1 = [0x11u8; 32];
    let w1 = win6(&ch, &seed1);
    let mut sp = SetParamsCase::new();
    sp.s_r = 500;
    ch.set_params(&mut sp, 300_000);
    let jack = combo_for_tier(&w1, TIER_JACKPOT, m1, 0);
    let low = combo_for_tier(&w1, TIER_LOWER0, m1, 0);
    let nowin = |n| combo_matching(&w1, 0, false, m1, n);
    buy(&mut ch, 0, vec![jack, low], false, 310_000);
    buy(&mut ch, 1, vec![nowin(1)], true, 320_000);
    buy(&mut ch, 0, vec![nowin(2), nowin(3), nowin(4)], false, 330_000);
    buy(&mut ch, 2, vec![nowin(5)], false, 340_000);
    let base = ch.controller.1.deadline as u64 - 60_000;
    ch.deposit_now(&key(1).payout(), 50 * KAS as i64, base);
    ch.deposit_now(&key(2).payout(), 20 * KAS as i64, base + 100);
    let s0 = ch.positions[0].1.shares;
    let (c, ops) = ch.withdraw_case(0, s0 / 100 * 95, deployer(), base + 5_000);
    ch.withdraw(&c, ops, base + 5_000);
    ch.close_now();
    let (c, f) = ch.draw_now(seed1);
    draws.push((1, c, f));
    // Claims of round 1: the jackpot (index 0, PAY), then the lower tier (index 1, FINAL).
    let tj = ticket_of(&ch, 1, jack.bytes());
    let pj = ch.pool_of(1, TIER_JACKPOT);
    let (c, ops) = ch.claim_case(tj, pj, 0, 1_099_000);
    ch.claim(&c, ops, 1_100_000);
    let tj = ticket_of(&ch, 1, jack.bytes());
    let pl = ch.pool_of(1, TIER_LOWER0);
    let (c, ops) = ch.claim_case(tj, pl, 1, 1_109_000);
    ch.claim(&c, ops, 1_110_000);
    // A refresh of an idle round-1 ticket (D_IDLE after its creation).
    let ti = ticket_of(&ch, 1, nowin(5).bytes());
    let (c, ops) = ch.refresh_ticket_case(ti, 1_200_000);
    let lock = c.lock_time;
    ch.refresh(&c, ops, lock);
    let now = ch.controller.1.deadline as u64 - 50_000;
    settle_all(&mut ch, now);
    // ---- Round 2 (m changed at round 1's close; s_R = 500 from set_params).
    let m2 = ch.controller.1.m;
    assert_ne!(m2, m1, "IX1: round 1's close changes m");
    assert_eq!(ch.controller.1.s_r, 500, "IX1: the admin change is live in round 2");
    let seed2 = [0x22u8; 32];
    let w2 = win6(&ch, &seed2);
    let second = combo_for_tier(&w2, TIER_SECOND, m2, 0);
    let low2 = combo_for_tier(&w2, TIER_LOWER0, m2, 1);
    let base = ch.controller.1.deadline as u64 - 60_000;
    buy(&mut ch, 1, vec![second], false, base - 10_000);
    buy(&mut ch, 3, vec![low2, combo_matching(&w2, 0, false, m2, 7)], true, base - 9_000);
    ch.deposit_now(&key(3).payout(), 15 * KAS as i64, base);
    let p1 = ch.positions.iter().position(|p| Payout::from_fields(p.1.owner_class, &p.1.owner_key) == key(1).payout());
    if let Some(p1) = p1 {
        let half = ch.positions[p1].1.shares / 2;
        let (c, ops) = ch.withdraw_case(p1, half, key(1), base + 5_000);
        ch.withdraw(&c, ops, base + 5_000);
    }
    ch.close_now();
    let (c, f) = ch.draw_now(seed2);
    draws.push((2, c, f));
    let now2 = ch.controller.1.deadline as u64 - 70_000;
    let ts = ticket_of(&ch, 2, second.bytes());
    let ps = ch.pool_of(2, TIER_SECOND);
    let (c, ops) = ch.claim_case(ts, ps, 0, now2 - 1_000);
    ch.claim(&c, ops, now2);
    let now = ch.controller.1.deadline as u64 - 50_000;
    settle_all(&mut ch, now);
    // ---- Round 3: one buy, close; then the late void that absorbs round 1's pools.
    let base = ch.controller.1.deadline as u64 - 60_000;
    let w3 = win6(&ch, &[0x33; 32]);
    let t3 = combo_matching(&w3, 0, false, ch.controller.1.m, 3);
    buy(&mut ch, 2, vec![t3], false, base);
    ch.close_now();
    // return_deposit_with_pool: a non-winning round-1 ticket with the expired round-1
    // lower-tier pool, at the pool's deadline; the pool is recreated unchanged.
    let tr = ticket_of(&ch, 1, nowin(1).bytes());
    let pl = ch.pool_of(1, TIER_LOWER0);
    let dl = ch.pools[pl].1.deadline as u64;
    let (c, ops) = ch.return_with_pool_case(tr, pl, dl - 1_000);
    assert_eq!(c.lock_time, dl);
    ch.return_deposit(&c, ops, dl);
    // The void of round 3 (draw window passed), absorbing every live round-1 pool.
    let absorb: Vec<usize> = ch.pools.iter().enumerate().filter(|(_, p)| p.1.round == 1).map(|(i, _)| i).collect();
    assert!(!absorb.is_empty());
    let d3 = ch.anchor_d();
    let (c, ops) = ch.void_case(&absorb, (d3 + lottery::game::DRAW_WINDOW + 1_000) as u64);
    let lock = c.lock_time;
    let f = ch.draw(&c, ops, lock + 10);
    draws.push((3, c, f));
    settle_all(&mut ch, lock + 1_000);
    // return_deposit_after of the round-3 ticket at T + D_RET.
    let t3i = ticket_of(&ch, 3, t3.bytes());
    let lock = (ch.tickets[t3i].1.deadline + lottery::game::D_RET) as u64;
    let (c, ops) = ch.return_after_case(t3i, lock - 1_000);
    assert_eq!(c.lock_time, lock);
    ch.return_deposit(&c, ops, lock);
    assert!(!ch.fabricated, "IX1: nothing fabricated");
    (ch, draws, gstates)
}

/// The genesis outputs' states of a fresh chain (output order, D50), as a launch bundle
/// gives them (S2a choice 3).
fn genesis_states(ch: &Chain) -> Vec<State> {
    let mut v = vec![State::Controller(ch.controller.1.clone()), State::Params(ch.params.1.clone())];
    v.extend(ch.shards.iter().map(|s| State::Shard(s.1.clone())));
    v.extend([State::Vault(ch.vault.1.clone()), State::Drawer(ch.drawer.1.clone()), State::Position(ch.positions[0].1.clone())]);
    v
}

/// A sortable outpoint key (txid bytes, index).
type Op = ([u8; 32], u32);
fn opk(op: &TransactionOutpoint) -> Op {
    (op.transaction_id.as_bytes(), op.index)
}

/// The live UTXO set the chain's own trackers describe: outpoint -> (role, state, value).
fn chain_live(ch: &Chain) -> BTreeMap<Op, (Role, State, u64)> {
    let mut m = BTreeMap::new();
    let v = |op: &TransactionOutpoint| ch.ledger.get(op).expect("tracked UTXO in the ledger").amount;
    let mut add = |op: TransactionOutpoint, s: State| {
        let r = s.role();
        assert!(m.insert(opk(&op), (r, s, v(&op))).is_none(), "duplicate outpoint");
    };
    add(ch.controller.0, State::Controller(ch.controller.1.clone()));
    add(ch.params.0, State::Params(ch.params.1.clone()));
    for s in &ch.shards {
        add(s.0, State::Shard(s.1.clone()));
    }
    if let Some((op, mk)) = &ch.marker {
        add(*op, State::Marker(mk.clone()));
    }
    add(ch.vault.0, State::Vault(ch.vault.1.clone()));
    add(ch.drawer.0, State::Drawer(ch.drawer.1.clone()));
    for p in &ch.pools {
        add(p.0, State::Pool(p.1.clone()));
    }
    for t in &ch.tickets {
        add(t.0, State::Ticket(t.1.clone()));
    }
    for p in &ch.positions {
        add(p.0, State::Position(p.1.clone()));
    }
    for q in &ch.requests {
        add(q.0, State::Request(q.1.clone()));
    }
    for r in &ch.records {
        add(r.0, State::Record(r.1.clone()));
    }
    m
}

fn index_live(ix: &Rebuild) -> BTreeMap<Op, (Role, State, u64)> {
    ix.live().values().map(|l| (opk(&l.outpoint), (l.role, l.state.clone().expect("every live state known"), l.value))).collect()
}

fn replay(ch: &Chain, log: &[Accepted]) -> Rebuild {
    replay_with(ch, log, None)
}

fn replay_with(ch: &Chain, log: &[Accepted], genesis: Option<Vec<State>>) -> Rebuild {
    let mut ix = Rebuild::new(ch.t.clone(), ch.cov, genesis);
    for (n, a) in log.iter().enumerate() {
        match ix.apply(a) {
            Ok(Some(_)) => {}
            Ok(None) => panic!("log entry {n}: not recognized as the instance's"),
            Err(e) => panic!("log entry {n}: {e}"),
        }
    }
    ix
}

// ---------------------------------------------------------------- IX1

/// IX1: the 3-round local game replayed through `classify` + `Rebuild`: every rebuilt shard
/// root equals the frozen root; the rounds (buys, m, close, frozen roots), the round results
/// and the live UTXO set (outpoint, role, state, value) equal the builder's own state.
#[test]
fn ix1_replay_three_round_game() {
    let (ch, draws, _) = game();
    let ix = replay(&ch, &ch.log);
    let names: Vec<&str> = ch.log.iter().map(|a| classify(&a.tx, &ch.t, ch.cov).unwrap().name()).collect();
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for n in &names {
        *count.entry(n).or_default() += 1;
    }
    println!("IX1 replayed {} transactions: {count:?}", names.len());
    for shape in [
        "genesis",
        "buy",
        "set_params",
        "close",
        "draw",
        "void",
        "claim",
        "refresh",
        "request_deposit",
        "request_withdraw",
        "settle",
        "return_deposit_with_pool",
        "return_deposit_after",
    ] {
        assert!(count.contains_key(shape), "IX1: the game has no {shape}");
    }
    assert_eq!(ix.genesis_txid(), Some(ch.log[0].tx.id()));
    // Rounds 1-3 are closed and complete; the frozen roots are the chain's trees' roots.
    for (r, c, f) in &draws {
        let round = ix.round(*r).unwrap_or_else(|| panic!("round {r} indexed"));
        assert_eq!(round.incomplete, None, "round {r}");
        let close = round.close.as_ref().expect("closed");
        let trees = ch.drawn_trees.get(r).expect("chain trees");
        let roots: Vec<[u8; 32]> = trees.iter().map(|t| t.root()).collect();
        assert_eq!(close.frozen_roots, roots, "round {r}: frozen roots");
        let rebuilt: Vec<[u8; 32]> = ix.trees(*r).unwrap().iter().map(|t| t.root()).collect();
        assert_eq!(rebuilt, roots, "round {r}: every rebuilt shard root equals the frozen root");
        assert_eq!(Some(close.m_next), ix.round(r + 1).and_then(|n| n.m), "round {r}: m'");
        // The round result equals the builder's own record of the draw.
        let x = c.expected();
        let draw = round.draw.as_ref().expect("drawn");
        assert_eq!(draw.txid, f.tx.id());
        assert_eq!(draw.result, RoundResult::new(c, &x, f.tx.id()), "round {r}: round result");
        assert_eq!(draw.result.void, *r == 3);
    }
    // Buys per round (the chain's tickets of every round, claimed or not).
    let buys: Vec<usize> = (1..=3).map(|r| ix.round(r).unwrap().buys.len()).collect();
    assert_eq!(buys, vec![4, 2, 1]);
    // Round 2's referral at the new s_R = 500 bp: r = floor(2 x 1 KAS x 500 / 10^4).
    assert_eq!(ix.round(2).unwrap().buys[1].referral, 2 * KAS * 500 / 10_000);
    assert_eq!(ix.round(1).unwrap().buys[1].referral, KAS * 1_000 / 10_000, "round 1 still pays at s_R = 1,000 bp");
    // The live UTXO set equals the chain's.
    let want = chain_live(&ch);
    let got = index_live(&ix);
    let missing: Vec<_> = want.keys().filter(|k| !got.contains_key(k)).collect();
    let extra: Vec<_> = got.keys().filter(|k| !want.contains_key(k)).collect();
    assert!(missing.is_empty() && extra.is_empty(), "IX1 live set: missing {missing:?}, extra {extra:?}");
    for (op, w) in &want {
        assert_eq!(&got[op], w, "IX1 live UTXO {op:?}");
    }
    println!("IX1 live UTXOs: {} (all states rebuilt and equal)", got.len());
}

/// IX1 (negative control: the rebuilt states are checked). Replaying the IX1 game, each
/// transition is first offered with its first covenant-bound output's value raised by one
/// sompi, and each with outputs' scripts (state) swapped where two are bound: every one is
/// refused with an error and leaves the index unchanged; then the real transaction applies.
#[test]
fn ix1_rebuilt_mismatch_is_an_error() {
    let (ch, _, _) = game();
    let mut ix = Rebuild::new(ch.t.clone(), ch.cov, None);
    ix.apply(&ch.log[0]).unwrap();
    let (mut values, mut scripts) = (0, 0);
    for a in &ch.log[1..] {
        let bound: Vec<usize> = a.tx.outputs.iter().enumerate().filter(|(_, o)| o.covenant.is_some()).map(|(i, _)| i).collect();
        let name = classify(&a.tx, &ch.t, ch.cov).unwrap().name();
        let before = (ix.log().len(), ix.live().len());
        if let Some(&o) = bound.first() {
            let mut bad = a.clone();
            bad.tx.outputs[o].value += 1;
            let e = ix.apply(&bad).err().unwrap_or_else(|| panic!("{name}: output {o} value + 1 accepted"));
            // Usually "value"; a deposit derives D from vault' value, so there the rebuilt
            // vault' state no longer hashes to the output script.
            assert!(e.contains("value") || e.contains("does not hash"), "{name}: {e}");
            values += 1;
        }
        if bound.len() >= 2 {
            let mut bad = a.clone();
            let (x, y) = (bound[0], bound[1]);
            let spk = bad.tx.outputs[x].script_public_key.clone();
            bad.tx.outputs[x].script_public_key = bad.tx.outputs[y].script_public_key.clone();
            bad.tx.outputs[y].script_public_key = spk;
            assert!(ix.apply(&bad).is_err(), "{name}: swapped output scripts accepted");
            scripts += 1;
        }
        assert_eq!((ix.log().len(), ix.live().len()), before, "{name}: a refused transaction changed the index");
        ix.apply(a).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    println!("IX1 negative control: {values} value changes and {scripts} script swaps refused");
    assert!(values >= 25 && scripts >= 10);
}

/// IX1 (genesis states): the genesis outputs' states given to `Rebuild::new` are checked
/// against the genesis outputs (P2SH); one changed field is refused.
#[test]
fn ix1_genesis_states_checked() {
    let mut l = Ledger::new();
    let op = l.fund(GENESIS_FUNDING, funding_spk(), 100);
    let e = l.get(&op).unwrap().clone();
    let g = GenesisCase::new(4, vec![(op, e)]);
    let f = g.build().finish();
    let cov = covenant_id(&f);
    let t = Templates::get(b_max());
    let mut states = vec![State::Controller(g.controller.clone()), State::Params(g.params.clone())];
    states.extend(g.shards.iter().map(|s| State::Shard(s.clone())));
    states.extend([State::Vault(g.vault.clone()), State::Drawer(g.drawer.clone()), State::Position(g.position.clone())]);
    let a = Accepted::new(f.tx.clone(), 200, &f.entries);
    let mut ix = Rebuild::new(t.clone(), cov, Some(states.clone()));
    assert!(matches!(ix.apply(&a), Ok(Some(Event::Genesis(ref ge))) if ge.k == 4));
    assert!(ix.live().values().all(|l| l.state.is_some()));
    let mut bad = states.clone();
    if let State::Drawer(d) = &mut bad[7] {
        d.f_min += 1;
    }
    let mut ix = Rebuild::new(t.clone(), cov, Some(bad));
    let e = ix.apply(&a).unwrap_err();
    assert!(e.contains("does not hash"), "{e}");
    assert!(ix.live().is_empty(), "nothing applied on error");
    // Without states the outputs are live with their roles and unknown states.
    let mut ix = Rebuild::new(t, cov, None);
    ix.apply(&a).unwrap();
    assert_eq!(ix.live().len(), 9);
    assert!(ix.live().values().all(|l| l.state.is_none()));
}

// ---------------------------------------------------------------- IX4

/// IX4: an index missing one buy. At `Close` the rebuilt root of that shard differs from
/// the frozen root (or the spent shard was never seen created), so the round is marked
/// incomplete and `Rebuild::trees` refuses it; the other rounds stay complete and every
/// other transaction still applies.
#[test]
fn ix4_missing_buy_marks_round_incomplete() {
    let (ch, _, gstates) = game();
    // Drop round 1's second buy on shard 0 (log: genesis, set_params, buy, buy, buy, ...).
    let buys: Vec<usize> =
        ch.log.iter().enumerate().filter(|(_, a)| matches!(classify(&a.tx, &ch.t, ch.cov), Some(Event::Buy(ref b)) if b.shard.round == 1)).map(|(i, _)| i).collect();
    for drop in [buys[2], buys[3]] {
        let log: Vec<Accepted> = ch.log.iter().enumerate().filter(|(i, _)| *i != drop).map(|(_, a)| a.clone()).collect();
        let ix = replay(&ch, &log);
        let r1 = ix.round(1).unwrap();
        let why = r1.incomplete.clone().expect("round 1 incomplete");
        println!("IX4 drop log entry {drop}: round 1 incomplete: {why}");
        let e = ix.trees(1).err().expect("round 1's trees refused");
        assert!(e.contains("incomplete"), "{e}");
        for r in [2, 3] {
            assert_eq!(ix.round(r).unwrap().incomplete, None, "round {r}");
            ix.trees(r).unwrap();
        }
        // `ClaimCase::from_store` and `DrawCase::from_store` refuse round 1 (no submission):
        // the claim of the round-1 jackpot ticket from the full log, the draw of round 1
        // from the log just before it (the genesis states given, so only the round's
        // completeness can refuse it).
        let claim = ch.log.iter().find(|a| matches!(classify(&a.tx, &ch.t, ch.cov), Some(Event::Claim(ref c)) if c.ticket.round == 1)).unwrap();
        let ix_before_claim = {
            let pos = log.iter().position(|a| a.tx.id() == claim.tx.id()).unwrap();
            replay_with(&ch, &log[..pos], Some(gstates.clone()))
        };
        let e = ClaimCase::from_store(&ix_before_claim, claim.tx.inputs[0].previous_outpoint, &[funding_of(claim, 2)], &payer_of(claim))
            .err()
            .expect("claim from an incomplete round refused");
        assert!(e.contains("incomplete"), "{e}");
        let draw1 = log.iter().position(|a| matches!(classify(&a.tx, &ch.t, ch.cov), Some(Event::Draw(_)))).unwrap();
        let ix = replay_with(&ch, &log[..draw1], Some(gstates.clone()));
        let a = &log[draw1];
        let e = DrawCase::from_store(&ix, Some(anchor_of(&a.tx)), &[funding_of(a, 4)], test_funder(), &payer_of(a)).err().expect("draw from an incomplete round refused");
        assert!(e.contains("incomplete"), "{e}");
        println!("IX4 from_store refusals: {e}");
    }
}

// ---------------------------------------------------------------- IX5

/// A round-1 snapshot of an index: per round (m, buys, tree roots, close, incomplete), and
/// the live set.
fn snapshot(ix: &Rebuild) -> (Vec<String>, BTreeMap<Op, (Role, Option<State>, u64, u64)>) {
    let rounds = ix
        .rounds()
        .values()
        .map(|r| {
            let roots: Vec<Option<[u8; 32]>> = r.shards.values().map(|s| s.tree.as_ref().map(|t| t.root())).collect();
            format!("{} m {:?} buys {:?} roots {roots:?} close {:?} draw {:?} incomplete {:?}", r.round, r.m, r.buys, r.close, r.draw, r.incomplete)
        })
        .collect();
    let live = ix.live().values().map(|l| (opk(&l.outpoint), (l.role, l.state.clone(), l.value, l.daa))).collect();
    (rounds, live)
}

/// IX5: a reorg. Branch A: buys on shards 0 and 1 in two chain blocks. The blocks are
/// removed (newest first, `revert`), then branch B is added: a different buy on shard 2 and
/// a buy on shard 0, then `close()`. The rebuilt state equals a fresh replay of the final
/// chain only (genesis + branch B). Reverting anything but the last applied transaction is
/// refused.
#[test]
fn ix5_reorg_equals_final_chain_replay() {
    let mut base = Chain::genesis(4);
    let fresh = |ch: &Chain| Rebuild::new(ch.t.clone(), ch.cov, None);
    let mut ix = fresh(&base);
    ix.apply(&base.log[0]).unwrap();
    let mut a = base.clone();
    buy(&mut a, 0, vec![Combo::new([1, 2, 3, 4, 5], 1)], false, 300_000);
    buy(&mut a, 1, vec![Combo::new([2, 3, 4, 5, 6], 2)], true, 300_100);
    for e in &a.log[1..] {
        ix.apply(e).unwrap();
    }
    assert_eq!(ix.round(1).unwrap().buys.len(), 2);
    // A non-last transaction cannot be reverted.
    let e = ix.revert(a.log[1].tx.id()).unwrap_err();
    assert!(e.contains("last applied"), "{e}");
    // Remove both blocks, newest first.
    ix.revert(a.log[2].tx.id()).unwrap();
    ix.revert(a.log[1].tx.id()).unwrap();
    assert_eq!(snapshot(&ix), {
        let mut g = fresh(&base);
        g.apply(&base.log[0]).unwrap();
        snapshot(&g)
    });
    // Branch B on the new chain.
    buy(&mut base, 2, vec![Combo::new([7, 8, 9, 10, 11], 3)], false, 300_050);
    buy(&mut base, 0, vec![Combo::new([1, 2, 3, 4, 5], 1), Combo::new([1, 2, 3, 4, 5], 1)], false, 300_060);
    let (c, ops) = base.close_case(900_000);
    base.close(&c, ops, lottery::close::TEST_DEADLINE as u64 + 10);
    for e in &base.log[1..] {
        ix.apply(e).unwrap();
    }
    let mut want = fresh(&base);
    for e in &base.log {
        want.apply(e).unwrap();
    }
    assert_eq!(snapshot(&ix), snapshot(&want), "IX5: reorged index = replay of the final chain");
    assert_eq!(ix.round(1).unwrap().incomplete, None);
    let roots: Vec<[u8; 32]> = ix.trees(1).unwrap().iter().map(|t| t.root()).collect();
    assert_eq!(roots, ix.round(1).unwrap().close.as_ref().unwrap().frozen_roots);
}

// ---------------------------------------------------------------- IX7

fn ev(tx: &harness::Transaction, cov: Hash) -> Event {
    classify(tx, &Templates::get(b_max()), cov).expect("classified as the instance's")
}

/// IX7: `classify` on every transition shape of M1-M7: genesis, buy (with and without a
/// referral), set_params, close, draw, void, a draw absorbing expired pools (the A4
/// "Absorb"), claim (PAY, FINAL, ACCRUE), both returns, refresh of each object, both
/// requests (partial and whole withdrawal), settle (every mode). Each gives its event with
/// the parsed fields equal to the builder's. A covenant-bound transaction from a foreign
/// (look-alike) template, a tampered genesis, and an input-0 leg are `Unknown`; a
/// transaction of another instance or with no instance input or output is `None`.
#[test]
fn ix7_classify_every_shape() {
    let cov = lottery::buy::COV;
    let t = Templates::get(b_max());
    // Genesis.
    let mut l = Ledger::new();
    let op = l.fund(GENESIS_FUNDING, funding_spk(), 100);
    let e = l.get(&op).unwrap().clone();
    let g = GenesisCase::new(4, vec![(op, e)]).build().finish();
    let gcov = covenant_id(&g);
    assert!(matches!(ev(&g.tx, gcov), Event::Genesis(ref x) if x.k == 4));
    assert_eq!(classify(&g.tx, &t, cov), None, "another instance's genesis");
    let mut tampered = g.tx.clone();
    tampered.outputs[0].value += 1;
    assert!(matches!(ev(&tampered, gcov), Event::Unknown(ref w) if w.contains("covenant ID")), "a tampered genesis is Unknown");
    // Buy, with and without a referral.
    for with_ref in [false, true] {
        let mut c = BuyCase::new(b_max());
        c.with_ref = with_ref;
        let f = c.build().finish();
        match ev(&f.tx, cov) {
            Event::Buy(b) => {
                assert_eq!((b.successor, b.ticket, b.referral, b.with_ref), (c.successor(), c.ticket(), c.referral(), with_ref));
            }
            other => panic!("buy: {other:?}"),
        }
    }
    // set_params.
    let sp = SetParamsCase::new();
    match ev(&sp.build().finish().tx, cov) {
        Event::SetParams(p) => assert_eq!(p.successor, sp.successor()),
        other => panic!("set_params: {other:?}"),
    }
    // close.
    let cc = CloseCase::new(4);
    match ev(&cc.build().finish().tx, cov) {
        Event::Close(c) => {
            assert_eq!(c.m_next, cc.expected().m_next);
            assert_eq!(c.frozen_roots, cc.shards.iter().map(|s| s.state.root).collect::<Vec<_>>());
        }
        other => panic!("close: {other:?}"),
    }
    // draw, void, and a draw absorbing two expired pools.
    let mut absorbing = DrawCase::at_round(4, 2);
    absorbing.absorb(2);
    for (name, c, void, a) in [("draw", DrawCase::new(4), false, 0), ("void", DrawCase::new_void(4), true, 0), ("absorb", absorbing, false, 2)] {
        let f = c.build().finish();
        match ev(&f.tx, cov) {
            Event::Draw(d) => {
                assert_eq!((d.void(), d.absorbed.len()), (void, a), "{name}");
                assert_eq!(d.result, RoundResult::new(&c, &c.expected(), f.tx.id()), "{name}: round result");
            }
            other => panic!("{name}: {other:?}"),
        }
    }
    // Claims.
    for (name, c) in [("PAY", common::m4::pay()), ("FINAL", common::m4::fin()), ("ACCRUE", common::m4::accrue())] {
        match ev(&c.build().finish().tx, cov) {
            Event::Claim(x) => assert_eq!((x.idx, x.ticket, x.pool), (c.idx, c.ticket.clone(), c.pool.clone()), "{name}"),
            other => panic!("claim {name}: {other:?}"),
        }
    }
    // Returns.
    assert!(matches!(ev(&ReturnCase::after().build().finish().tx, cov), Event::ReturnDeposit(ref r) if r.pool.is_none()));
    assert!(matches!(ev(&ReturnCase::with_pool().build().finish().tx, cov), Event::ReturnDeposit(ref r) if r.pool.is_some()));
    // Refreshes.
    for (name, c) in common::m4::refresh_bases().into_iter().chain(refresh5_bases()) {
        match ev(&c.build().finish().tx, cov) {
            Event::Refresh(s) => assert_eq!(s.redeem(&t), c.obj.redeem(&t), "refresh {name}"),
            other => panic!("refresh {name}: {other:?}"),
        }
    }
    // Requests.
    let d = DepositCase::new();
    assert!(matches!(ev(&d.build().finish().tx, cov), Event::RequestDeposit(ref x) if x.owner == d.owner && x.vault == d.vault));
    for w in [WithdrawCase::new(), WithdrawCase::new().whole()] {
        assert!(matches!(ev(&w.build().finish().tx, cov), Event::RequestWithdraw(ref x) if x.w == w.w && x.position == w.position));
    }
    // Settlements (deposit-convert, drained, refund, FINAL, PAY, ACCRUE).
    for (name, s) in settle_bases().into_iter().chain([("deposit fixture", SettleCase::deposit()), ("withdrawal fixture", SettleCase::withdrawal())]) {
        match ev(&s.build().finish().tx, cov) {
            Event::Settle(x) => assert_eq!((x.request, x.record), (s.request.clone(), s.record.clone()), "settle {name}"),
            other => panic!("settle {name}: {other:?}"),
        }
    }
    // A foreign (look-alike) shard template bound to the instance: Unknown.
    for which in [Sil::Shard, Sil::Ticket] {
        let lt = lookalike(which);
        let b = match which {
            Sil::Shard => BuyCase::new(b_max()).build_with(lt),
            _ => common::m4::pay().build_with(lt),
        };
        let f = b.finish();
        assert!(matches!(ev(&f.tx, cov), Event::Unknown(_)), "look-alike {which:?}: not Unknown");
    }
    // A transaction of another instance (outputs bound to another covenant ID): None.
    assert_eq!(classify(&BuyCase::new(b_max()).build().finish().tx, &t, OTHER_COV), None);
    // Outputs bound to the instance and to another covenant ID: Unknown, not skipped.
    let mut mixed = BuyCase::new(b_max()).build().finish().tx;
    mixed.outputs[1].covenant = Some(harness::CovenantBinding::new(0, OTHER_COV));
    assert!(matches!(ev(&mixed, cov), Event::Unknown(ref w) if w.contains("another covenant ID")));
    // A plain transaction (no template input, no bound output): None.
    let mut plain = g.tx.clone();
    for o in &mut plain.outputs {
        o.covenant = None;
    }
    assert_eq!(classify(&plain, &t, gcov), None);
    // A leg at input 0 (a pool spent through ticket_leg first): Unknown.
    let mut c = common::m4::pay().build();
    c.inputs.swap(0, 1);
    let f = c.finish();
    assert!(matches!(ev(&f.tx, cov), Event::Unknown(_)), "a leg at input 0");
}

// ---------------------------------------------------------------- S2b helpers

/// The fee an accepted transaction paid (spent amounts minus output values).
fn fee_of(a: &Accepted) -> u64 {
    a.spent.iter().map(|s| s.amount).sum::<u64>() - a.tx.outputs.iter().map(|o| o.value).sum::<u64>()
}

/// The payer of a builder-made transaction of the tests: the builders' test key and the
/// fee it paid (S2c should-fix 2: `from_store` takes both from the caller).
fn payer_of(a: &Accepted) -> Payer {
    Payer { funding_sk: lottery::buy::FUNDING_SK, fee: fee_of(a) }
}

/// The builders' default draw funder (the test key's Schnorr payout).
fn test_funder() -> Payout {
    Payout::Schnorr(lottery::params::xonly(&lottery::buy::FUNDING_SK))
}

/// Input `i` of an accepted transaction as a funding UTXO of an offline build.
fn funding_of(a: &Accepted, i: usize) -> Funding {
    Funding { outpoint: a.tx.inputs[i].previous_outpoint, value: a.spent[i].amount, daa: a.spent[i].daa }
}

/// The anchor proof a `draw()` transaction carries (its `anchor` argument).
fn anchor_of(tx: &harness::Transaction) -> AnchorProof {
    let d = lottery::index::parse_draw(tx, &Templates::get(b_max())).expect("a draw");
    AnchorProof::from_bytes(&d.anchor).expect("a 272-byte anchor")
}

/// `a` and `b` are the same transaction except the funding inputs' signatures (inputs
/// `from..`): BIP-340 Schnorr signing with auxiliary randomness (secp256k1 0.29
/// `sign_schnorr` draws it from `thread_rng`; rusty-kaspa v2.1.0
/// `consensus/core/src/sign.rs:167-177` calls it), so two signings of one message differ.
/// Every other byte, every covenant input's signature script (arguments and redeem)
/// included, is equal.
fn same_tx(a: &harness::Transaction, b: &harness::Transaction, from: usize, what: &str) {
    let strip = |t: &harness::Transaction| {
        let mut t = t.clone();
        for i in from..t.inputs.len() {
            t.inputs[i].signature_script.clear();
        }
        t.finalize();
        t
    };
    assert_eq!(a.inputs.len(), b.inputs.len(), "{what}: input count");
    assert_eq!(strip(a), strip(b), "{what}: not byte-identical outside the funding signatures");
}

/// A built offline transaction with its outpoints, run by every input.
fn finish_ops(mut b: lottery::buy::Built, ops: Vec<TransactionOutpoint>, what: &str) -> lottery::buy::Final {
    b.outpoints = ops;
    let f = b.finish();
    if let Err((i, e)) = f.run() {
        panic!("{what}: rejected at input {i}: {e:?}");
    }
    f
}

/// The round-1 wallet of the IX1 game built from its transactions as the buyer sees them
/// (the runner is the only buyer): every round-1 buy, the close's frozen roots, the draw's
/// round result, and the claims before log entry `upto`.
fn wallet_r1(ch: &Chain, upto: usize) -> Wallet {
    let t = ch.t.clone();
    let m1 = ch.log.iter().find_map(|a| match classify(&a.tx, &t, ch.cov) {
        Some(Event::Buy(b)) => Some(b.shard.m),
        _ => None,
    });
    let mut w = Wallet::new(ch.cov, t.b_max, 1, m1.unwrap(), 4);
    for a in &ch.log[..upto] {
        match classify(&a.tx, &t, ch.cov) {
            Some(Event::Buy(b)) if b.shard.round == 1 => {
                // Every buy's payout is the runner's in the IX1 game (it is the only buyer).
                w.record_buy(&t, &a.tx, a.daa, &[b.payout]).unwrap();
            }
            Some(Event::Close(c)) if c.controller.round == 1 => w.freeze(&c.frozen_roots).unwrap(),
            Some(Event::Draw(d)) if d.marker.round == 1 => w.set_draw(&t, &d.result, a.daa).unwrap(),
            Some(Event::Claim(c)) if c.ticket.round == 1 => w.apply_claim(&t, &a.tx, a.daa).unwrap(),
            _ => {}
        }
    }
    w
}

// ---------------------------------------------------------------- IX2

/// IX2: from the IX1 game's index alone (replayed up to the transaction, with the genesis
/// states of the launch bundle), `DrawCase::from_store` rebuilds each of the three draws
/// (two with a real anchor proof, the round-3 void absorbing the expired round-1 pools)
/// and `ClaimCase::from_store` each of the three claims; the round-1 claims are also built
/// from the round-1 wallet. Each is accepted by every input in the engine and byte-identical
/// to the builder-made transaction (outside the funding inputs' randomized Schnorr
/// signatures, `same_tx`).
#[test]
fn ix2_from_store_byte_identical() {
    let (ch, draws, gstates) = game();
    let pos = |id| ch.log.iter().position(|a| a.tx.id() == id).unwrap();
    for (r, c, f) in &draws {
        let at = pos(f.tx.id());
        let a = &ch.log[at];
        let ix = replay_with(&ch, &ch.log[..at], Some(gstates.clone()));
        let anchor = (!c.void).then(|| anchor_of(&a.tx));
        let absorb: Vec<TransactionOutpoint> = (0..c.a()).map(|i| a.tx.inputs[4 + i].previous_outpoint).collect();
        let payer = Payer { funding_sk: c.funding_sk, fee: c.fee };
        let (d, ops) = DrawCase::from_store_absorbing(&ix, anchor, &[funding_of(a, 4 + c.a())], &absorb, c.funder, &payer).unwrap_or_else(|e| panic!("round {r}: {e}"));
        assert!(d.funder == c.funder && d.fee == c.fee && d.funding_sk == c.funding_sk, "S2c should-fix 2: the caller's funder and payer");
        let g = finish_ops(d.build_with(ch.t.clone()), ops, &format!("round {r} draw from the index"));
        same_tx(&g.tx, &f.tx, 4 + c.a(), &format!("round {r} draw"));
        println!("IX2 round {r} {} from the index: byte-identical, accepted", if c.void { "void" } else { "draw" });
    }
    let mut claims = 0;
    for (at, a) in ch.log.iter().enumerate() {
        let Some(Event::Claim(cl)) = classify(&a.tx, &ch.t, ch.cov) else { continue };
        let ix = replay_with(&ch, &ch.log[..at], Some(gstates.clone()));
        let ticket = a.tx.inputs[0].previous_outpoint;
        let (c, ops) = ClaimCase::from_store(&ix, ticket, &[funding_of(a, 2)], &payer_of(a)).unwrap_or_else(|e| panic!("claim {at}: {e}"));
        assert_eq!(c.fee, fee_of(a), "S2c should-fix 2: the caller's fee");
        assert_eq!(c.idx, cl.idx, "from_store picks the builder's index");
        assert!(c.roots_match());
        let g = finish_ops(c.build_with(ch.t.clone()), ops, &format!("claim {at} from the index"));
        same_tx(&g.tx, &a.tx, 2, &format!("claim {at} (index)"));
        if cl.ticket.round == 1 {
            let w = wallet_r1(&ch, at);
            let (c, ops) = ClaimCase::from_store(&w, ticket, &[funding_of(a, 2)], &payer_of(a)).unwrap_or_else(|e| panic!("claim {at} (wallet): {e}"));
            let g = finish_ops(c.build_with(ch.t.clone()), ops, &format!("claim {at} from the wallet"));
            same_tx(&g.tx, &a.tx, 2, &format!("claim {at} (wallet)"));
            // The wallet survives its own file format.
            let w2: Wallet = store::read(&store::write(&w, store::WALLET).unwrap(), store::WALLET).unwrap();
            assert_eq!(w2, w);
        }
        claims += 1;
        println!(
            "IX2 claim (round {}, index {}) from the index{}: byte-identical, accepted",
            cl.ticket.round,
            cl.idx,
            if cl.ticket.round == 1 { " and from the wallet" } else { "" }
        );
    }
    assert_eq!(claims, 3);
}

// ---------------------------------------------------------------- IX3

/// An untrusted copy of an index whose round trees have one leaf count changed: shard
/// `shard`'s count of the combination `comb` moved by `delta`.
struct Tampered<'a> {
    ix: &'a Rebuild,
    shard: usize,
    comb: Combo,
    delta: i32,
}

impl ClaimStore for Tampered<'_> {
    fn templates(&self) -> Result<std::sync::Arc<Templates>, String> {
        Ok(self.ix.templates().clone())
    }
    fn cov(&self) -> Hash {
        self.ix.cov()
    }
    fn ticket(&self, op: &TransactionOutpoint) -> Result<(lottery::state::TicketState, u64, u64), String> {
        ClaimStore::ticket(self.ix, op)
    }
    fn pools(&self, round: i64) -> Result<Vec<(TransactionOutpoint, lottery::state::PoolState, u64, u64)>, String> {
        ClaimStore::pools(self.ix, round)
    }
    fn trees(&self, round: i64) -> Result<Vec<lottery::smt::Tree>, String> {
        let mut trees = self.ix.trees(round)?;
        let rank = self.comb.rank();
        let mut counts = trees[self.shard].counts(rank);
        let slot = &mut counts[self.comb.bonus as usize - 1];
        *slot = (*slot as i32 + self.delta) as u16;
        trees[self.shard].set(rank, counts);
        Ok(trees)
    }
}

/// IX3: a lower-tier combination held by two tickets on shards 0 and 1 (h = 2). The claim
/// built from an index copy with shard 1's count of it changed (h - 1 = 1 and h + 1 = 3)
/// builds offline (the store is untrusted; its roots are not the frozen roots,
/// `roots_match` = false) and is refused on chain by the ticket (input 0) at check 7, the K
/// count leaves proven against the pool's frozen roots; the claim from the true index is
/// accepted.
#[test]
fn ix3_tampered_index_claim_refused() {
    let mut ch = chain5();
    let gstates = genesis_states(&ch);
    let m = ch.controller.1.m;
    let seed = [0x31u8; 32];
    let w = win6(&ch, &seed);
    let low = combo_for_tier(&w, TIER_LOWER0, m, 0);
    let base = ch.controller.1.deadline as u64 - 60_000;
    buy(&mut ch, 0, vec![low], false, base);
    buy(&mut ch, 1, vec![low], false, base + 100);
    ch.close_now();
    ch.draw_now(seed);
    let ix = replay_with(&ch, &ch.log, Some(gstates));
    let ticket = ch.tickets[ticket_of(&ch, 1, low.bytes())].0;
    let fop = ch.ledger.fund(10 * KAS, funding_spk(), 2_000_000);
    let funding = [Funding { outpoint: fop, value: 10 * KAS, daa: 2_000_000 }];
    let payer = Payer { funding_sk: lottery::buy::FUNDING_SK, fee: KAS / 10 };
    let (good, ops) = ClaimCase::from_store(&ix, ticket, &funding, &payer).unwrap();
    assert_eq!(good.expected().h, 2);
    assert!(good.roots_match());
    finish_ops(good.build_with(ch.t.clone()), ops, "claim from the true index");
    for delta in [-1, 1] {
        let bad_store = Tampered { ix: &ix, shard: 1, comb: low, delta };
        let (bad, ops) = ClaimCase::from_store(&bad_store, ticket, &funding, &payer).unwrap_or_else(|e| panic!("h {delta:+}: {e}"));
        let x = bad.expected();
        assert_eq!(x.h, 2 + delta as i64, "the tampered copy's h");
        assert!(!bad.roots_match(), "the tampered trees' roots are not the frozen roots");
        let mut b = bad.build_with(ch.t.clone());
        b.outpoints = ops;
        let e = reject_at(&b, 0);
        let check = first_failing_check4(&b, &Case::Claim(bad.clone()), "ticket:7", 0).expect("ticket markers");
        assert_eq!(check, "ticket:7", "h {delta:+}: refused at {check}");
        println!("IX3 h 2 -> {}: prize {} refused on chain at input 0, {check} ({e:?})", x.h, x.prize);
    }
}

// ---------------------------------------------------------------- IX6

/// IX6: `index/1` (log and snapshot), `wallet/1`, `campaign/1` and `bundle/1` round-trip
/// (write then read gives the same value, and the log rebuilds the same index); each file
/// starts with its `format`; an unknown format, a value of 2^53 and an uppercase hex digit
/// are refused on read, and a value of 2^53 on write.
#[test]
fn ix6_store_formats_round_trip() {
    let (ch, _, gstates) = game();
    // index/1: the log (header + one line per accepted transaction, then a reorg's remove
    // and re-add of the last one) and the snapshot.
    let blk = |n: usize| Hash::from_bytes([n as u8; 32]);
    let mut lines: Vec<IndexLogLine> = ch.log.iter().enumerate().map(|(n, a)| IndexLogLine::add(n as u64, blk(n), 1_000 + n as u64, a)).collect();
    let last = ch.log.last().unwrap();
    let n = lines.len() as u64;
    lines.push(IndexLogLine::remove(n, blk(n as usize - 1), 1_000 + n - 1, last));
    lines.push(IndexLogLine::add(n + 1, blk(200), 5_000, last));
    let log = IndexLog { cov: ch.cov, lines, reanchors: vec![] };
    let text = log.to_jsonl().unwrap();
    assert!(text.starts_with("{\"format\":\"index/1\""), "{}", &text[..40]);
    let back = IndexLog::from_jsonl(&text).unwrap();
    assert_eq!(back, log);
    let ix = back.rebuild(ch.t.clone(), ch.cov, Some(gstates.clone())).unwrap();
    let direct = replay_with(&ch, &ch.log, Some(gstates.clone()));
    let snap = IndexSnapshot::of(&ix, Some(blk(200)));
    assert_eq!(snap, IndexSnapshot::of(&direct, Some(blk(200))), "the log rebuilds the same index");
    let js = store::write(&snap, store::INDEX).unwrap();
    assert!(js.starts_with("{\n  \"format\": \"index/1\""), "{}", &js[..40]);
    assert_eq!(store::read::<IndexSnapshot>(&js, store::INDEX).unwrap(), snap);
    for l in &snap.live_utxos {
        assert!(l.decode(&ch.t).unwrap().is_some(), "every live state known and checked");
    }
    for r in snap.rounds.iter().filter_map(|r| r.draw.as_ref()) {
        let rr = r.result.result(&ch.t).unwrap();
        assert_eq!(store::ResultJson::of(&rr, &ch.t), r.result);
    }
    // wallet/1, campaign/1.
    let w = wallet_r1(&ch, ch.log.len());
    assert!(!w.tickets.is_empty() && !w.frozen_roots.is_empty() && w.record.is_some());
    let wj = store::write(&w, store::WALLET).unwrap();
    assert_eq!(store::read::<Wallet>(&wj, store::WALLET).unwrap(), w);
    let mut c = Campaign::new();
    c.cov = Some(ch.cov.as_bytes());
    c.step = "C3".into();
    c.genesis.push(TransactionOutpoint::new(ch.log[0].tx.id(), 0).into());
    c.steps.push(StepJson {
        id: "C2".into(),
        txids: vec![TxRef { txid: ch.log[1].tx.id().as_bytes(), node: "wss://example.invalid/testnet-10".into(), action: "C2/deposit".into() }],
        checks: vec![CheckJson { name: "9.99 KAS deposit refused".into(), ok: true, detail: "rejection reason text".into() }],
        daa: 123_456,
        wall_time: 1_760_000_000,
    });
    let cj = store::write(&c, store::CAMPAIGN).unwrap();
    assert_eq!(store::read::<Campaign>(&cj, store::CAMPAIGN).unwrap(), c);
    // bundle/1.
    let (bundle, _, _) = local_bundle(|_| {});
    let bj = store::write(&bundle, store::BUNDLE).unwrap();
    assert_eq!(store::read::<Bundle>(&bj, store::BUNDLE).unwrap(), bundle);
    // Refusals: an unknown format (another kind or version, none), 2^53 (read and write),
    // uppercase hex.
    for (name, text, fmt) in [("index", &js, store::INDEX), ("wallet", &wj, store::WALLET), ("campaign", &cj, store::CAMPAIGN), ("bundle", &bj, store::BUNDLE)] {
        let other = text.replacen(&format!("\"{fmt}\""), "\"wallet/2\"", 1);
        let e = store::read::<serde_json::Value>(&other, fmt).unwrap_err();
        assert!(e.contains("unknown format"), "{name}: {e}");
        assert!(store::read::<serde_json::Value>(&text.replacen("\"format\"", "\"formal\"", 1), fmt).is_err(), "{name}: no format");
    }
    assert!(store::read::<Wallet>(&wj, store::CAMPAIGN).unwrap_err().contains("unknown format"));
    let big = cj.replace("\"daa\": 123456", "\"daa\": 9007199254740992");
    assert!(store::read::<Campaign>(&big, store::CAMPAIGN).unwrap_err().contains("2^53"));
    assert!(store::read::<Campaign>(&cj.replace("\"daa\": 123456", "\"daa\": 9007199254740991"), store::CAMPAIGN).is_ok(), "2^53 - 1 is accepted");
    let mut c2 = c.clone();
    c2.steps[0].daa = 1 << 53;
    assert!(store::write(&c2, store::CAMPAIGN).unwrap_err().contains("2^53"));
    let mut w2 = w.clone();
    w2.tickets[0].value = 1 << 53;
    assert!(store::write(&w2, store::WALLET).is_err());
    let hex = store::to_hex(&ch.cov.as_bytes());
    let upper = cj.replacen(&hex, &hex.to_uppercase(), 1);
    assert!(store::read::<Campaign>(&upper, store::CAMPAIGN).unwrap_err().contains("lowercase"));
    assert!(IndexLog::from_jsonl(&text.replacen("index/1", "index/2", 1)).unwrap_err().contains("unknown format"));
    println!(
        "IX6: index log {} lines ({} bytes), snapshot {} bytes, wallet {} bytes, campaign {} bytes, bundle {} bytes",
        log.lines.len() + 1,
        text.len(),
        js.len(),
        wj.len(),
        cj.len(),
        bj.len()
    );
}

/// M8 S4 (PENDING A4 amendment): a re-anchor line in `index/1`. It round-trips in the log
/// (in the middle and at the end), changes no index state (`rebuild` gives the index
/// without it), round-trips in the snapshot (and a snapshot without the field still
/// reads), and the log refuses one whose position or chaining is wrong.
#[test]
fn s4_reanchor_line_round_trips_and_changes_no_state() {
    let (ch, _, gstates) = game();
    let blk = |n: usize| Hash::from_bytes([n as u8; 32]);
    let lines: Vec<IndexLogLine> = ch.log.iter().enumerate().map(|(n, a)| IndexLogLine::add(n as u64, blk(n), 1_000 + n as u64, a)).collect();
    let n = lines.len() as u64;
    let ra = |at: u64, old: usize, new: usize| store::ReanchorLine {
        kind: store::REANCHOR.into(),
        at,
        old_block: blk(old).as_bytes(),
        old_daa: 1_000 + old as u64,
        new_block: blk(new).as_bytes(),
        new_daa: 9_000 + new as u64,
        node: "wss://example.invalid/testnet-10".into(),
        live_checked: 42,
        wall_time: 1_792_900_000,
        argument: "KIP-20: every instance output is created by spending an instance UTXO".into(),
    };
    let plain = IndexLog { cov: ch.cov, lines: lines.clone(), reanchors: vec![] };
    let with = IndexLog { cov: ch.cov, lines: lines.clone(), reanchors: vec![ra(2, 1, 250), ra(n, n as usize - 1, 251), ra(n, 251, 252)] };
    let text = with.to_jsonl().unwrap();
    assert_eq!(text.lines().count(), 1 + lines.len() + 3);
    assert!(text.lines().nth(3).unwrap().starts_with("{\"kind\":\"reanchor\",\"at\":2,"), "{}", text.lines().nth(3).unwrap());
    let back = IndexLog::from_jsonl(&text).unwrap();
    assert_eq!(back, with);
    let a = back.rebuild(ch.t.clone(), ch.cov, Some(gstates.clone())).unwrap();
    let b = plain.rebuild(ch.t.clone(), ch.cov, Some(gstates.clone())).unwrap();
    let mut snap = IndexSnapshot::of(&a, Some(blk(252)));
    assert_eq!(snap, IndexSnapshot::of(&b, Some(blk(252))), "a re-anchor line changes no index state");
    snap.reanchors = with.reanchors.clone();
    let js = store::write(&snap, store::INDEX).unwrap();
    assert_eq!(store::read::<IndexSnapshot>(&js, store::INDEX).unwrap(), snap);
    let mut old: serde_json::Value = serde_json::from_str(&js).unwrap();
    old.as_object_mut().unwrap().remove("reanchors");
    assert!(store::read::<IndexSnapshot>(&old.to_string(), store::INDEX).unwrap().reanchors.is_empty(), "an older snapshot reads");
    // Refusals: a wrong position, two lines at one position that do not chain, a bad kind.
    let moved = text.replacen("\"kind\":\"reanchor\",\"at\":2,", "\"kind\":\"reanchor\",\"at\":3,", 1);
    assert!(IndexLog::from_jsonl(&moved).unwrap_err().contains("re-anchor line after 2"));
    let broken = IndexLog { cov: ch.cov, lines: lines.clone(), reanchors: vec![ra(n, 1, 250), ra(n, 7, 251)] }.to_jsonl().unwrap();
    assert!(IndexLog::from_jsonl(&broken).unwrap_err().contains("do not chain"));
    let mut bad = ra(n, 1, 250);
    bad.kind = "add".into();
    assert!(bad.to_line().is_err());
}

// ---------------------------------------------------------------- IX8

/// A local genesis (K = 4, the `GenesisCase::new` fixture changed by `edit`, built without
/// `validate()` so a bad instance can be bundled) and its bundle.
fn local_bundle(edit: impl FnOnce(&mut GenesisCase)) -> (Bundle, GenesisCase, lottery::buy::Final) {
    let mut l = Ledger::new();
    let op = l.fund(GENESIS_FUNDING, funding_spk(), 100);
    let e = l.get(&op).unwrap().clone();
    let mut g = GenesisCase::new(4, vec![(op, e)]);
    edit(&mut g);
    let f = g.build_unchecked_for_tests(g.templates()).finish();
    let live = bundle::genesis_live(&g, &f, 200);
    let b = Bundle::new(&g, &f, live, Some(bundle::Snapshot { daa: 200, chain_block: [7; 32] })).unwrap();
    (b, g, f)
}

type Mutation = Box<dyn Fn(&mut Bundle)>;

/// IX8: `bundle::verify` on a local genesis passes (covenant ID recomputed and equal to the
/// one the transaction bound, every template hash, the states, the values, the genesis
/// validation); one byte changed in a template preimage, an authorized output, the
/// covenant ID, a genesis state or a live UTXO, a changed parameter, and a genesis that
/// the SF1 round-1 whale check refuses (the old default J(1) 1,000,000 KAS) each fail.
#[test]
fn ix8_bundle_verify() {
    let (b, g, f) = local_bundle(|_| {});
    let v = bundle::verify(&b).unwrap();
    assert_eq!(v.cov, covenant_id(&f), "the recomputed covenant ID is the one the genesis bound");
    assert_eq!(v.k, 4);
    // The verified states give a working index of the instance.
    let mut ix = Rebuild::new(v.templates.clone(), v.cov, Some(v.genesis_states.clone()));
    assert!(matches!(ix.apply(&Accepted::new(f.tx.clone(), 200, &f.entries)), Ok(Some(Event::Genesis(_)))));
    assert!(ix.live().values().all(|l| l.state.is_some()));
    fn flip(v: &mut [u8], i: usize) {
        v[i] ^= 1;
    }
    let cases: Vec<(&str, Mutation, &str)> = vec![
        ("template prefix byte", Box::new(|b| flip(&mut b.templates[4].prefix, 0)), "hash"),
        ("template suffix byte", Box::new(|b| flip(&mut b.templates[0].suffix, 0)), "hash"),
        (
            "template rehashed but not the toolchain's",
            Box::new(|b| {
                flip(&mut b.templates[2].prefix, 0);
                b.templates[2].hash = silverscript_abi::template_hash(&b.templates[2].prefix, &b.templates[2].suffix);
            }),
            "pinned toolchain",
        ),
        ("authorized output script byte", Box::new(|b| flip(&mut b.authorized_outputs[1].script, 5)), "authorized output 1"),
        ("authorized output value", Box::new(|b| b.authorized_outputs[0].value += 1), "authorized output 0"),
        ("covenant ID byte", Box::new(|b| b.covenant_id[0] ^= 1), "authorized output 0"),
        (
            "genesis state byte",
            Box::new(|b| {
                let n = b.genesis_states[7].len();
                flip(&mut b.genesis_states[7], n / 2)
            }),
            "genesis state 7",
        ),
        ("parameter", Box::new(|b| *b.params.get_mut("controller.margin").unwrap() += 1), "params"),
        ("live UTXO script", Box::new(|b| flip(&mut b.live_utxos[3].spk, 4)), "live UTXO"),
        (
            "live UTXO state byte",
            Box::new(|b| {
                let s = b.live_utxos[3].state.as_mut().unwrap();
                let n = s.len();
                flip(s, n / 2)
            }),
            "live UTXO",
        ),
        ("live UTXO role", Box::new(|b| b.live_utxos[3].role = Role::Marker), "live UTXO"),
        ("live UTXO round", Box::new(|b| b.live_utxos[3].round = Some(9)), "live UTXO"),
        ("genesis outpoint", Box::new(|b| b.genesis_outpoint.index += 1), "genesis_outpoint"),
        ("network", Box::new(|b| b.network = "mainnet".into()), "mainnet"),
        ("toolchain", Box::new(|b| b.toolchain.silverscript_rev = "0".repeat(40)), "toolchain"),
    ];
    for (name, mutate, want) in cases {
        let mut m = b.clone();
        mutate(&mut m);
        let e = bundle::verify(&m).err().unwrap_or_else(|| panic!("{name}: verify passed"));
        assert!(e.contains(want), "{name}: {e}");
        println!("IX8 {name}: {e}");
    }
    // The covenant ID changed consistently everywhere (the transaction's outputs re-bound,
    // its id updated): the recomputation still refuses it.
    let mut m = b.clone();
    m.covenant_id[0] ^= 1;
    for a in &mut m.authorized_outputs {
        a.covenant_id = m.covenant_id;
    }
    for o in &mut m.genesis_tx.outputs[..9] {
        o.covenant = Some(harness::CovenantBinding::new(0, Hash::from_bytes(m.covenant_id)));
    }
    m.genesis_tx.finalize();
    m.genesis_txid = m.genesis_tx.id().as_bytes();
    let e = bundle::verify(&m).unwrap_err();
    assert!(e.contains("recomputed"), "{e}");
    println!("IX8 consistent covenant ID change: {e}");
    // SF1: the old default J(1) 1,000,000 KAS (L(1) 1,090,000 KAS > W at margin 1.5) is a
    // well-formed bundle that verify refuses at the genesis validation.
    let (bad, _, _) = local_bundle(|g| g.set_jackpot(1_000_000 * KAS as i64));
    let e = bundle::verify(&bad).unwrap_err();
    assert!(e.contains("whale"), "{e}");
    assert!(g.validate().is_ok());
    println!("IX8 SF1: {e}");
    // S2c MF-A: every template hash a genesis state carries is the compiled template's. The
    // controller's route (shard_tpl, marker_tpl, params_tpl) is in state and nothing on
    // chain pins it (controller.sil:9-11): close() would create the marker, which holds the
    // round's money, under whatever marker_tpl says. One bit flipped in each field: a
    // well-formed bundle (the bad genesis is built without validate()) that verify refuses.
    type Edit = fn(&mut GenesisCase);
    let fields: [(&str, Edit); 9] = [
        ("controller.shard_tpl", |g| g.controller.shard_tpl[0] ^= 1),
        ("controller.marker_tpl", |g| g.controller.marker_tpl[0] ^= 1),
        ("controller.params_tpl", |g| g.controller.params_tpl[0] ^= 1),
        ("vault.record_tpl", |g| g.vault.record_tpl[0] ^= 1),
        ("vault.request_tpl", |g| g.vault.request_tpl[0] ^= 1),
        ("vault.position_tpl", |g| g.vault.position_tpl[0] ^= 1),
        ("drawer.pool_tpl", |g| g.drawer.pool_tpl[0] ^= 1),
        ("drawer.vault_tpl", |g| g.drawer.vault_tpl[0] ^= 1),
        ("drawer.ticket_tpl", |g| g.drawer.ticket_tpl[0] ^= 1),
    ];
    let mut passed = vec![];
    for (name, edit) in fields {
        let (bad, bg, _) = local_bundle(edit);
        match bundle::verify(&bad) {
            Ok(_) => passed.push(name),
            Err(e) => {
                assert!(e.contains("tpl"), "{name}: refused for another reason: {e}");
                assert!(bg.validate().is_err(), "{name}: validate() accepts it");
                println!("IX8 MF-A {name}: {e}");
            }
        }
    }
    assert!(passed.is_empty(), "MF-A: verify accepts a wrong template hash in {passed:?}");
}

/// S3b fix (A3 amendment): `bundle/1` stores each UTXO's state bytes only, not its redeem
/// script; the templates carry the prefix and suffix. `verify` rebuilds every redeem as
/// prefix || state || suffix and compares its P2SH with the output script. The old shape (a
/// full redeem script in `genesis_states` or `live_utxos[].state`) is refused.
#[test]
fn s3b_bundle_state_bytes_only() {
    let (b, g, f) = local_bundle(|_| {});
    let t = g.templates();
    let states = bundle::genesis_states(&g);
    assert_eq!(b.genesis_states.len(), states.len());
    for (i, (bytes, s)) in b.genesis_states.iter().zip(&states).enumerate() {
        let tp = lottery::index::template_of(&t, s.role());
        let redeem = s.redeem(&t);
        assert_eq!(redeem, [tp.prefix.as_slice(), bytes, &tp.suffix].concat(), "genesis state {i}: prefix || state || suffix is the redeem");
        assert_eq!(harness::pay_to_script_hash_script(&redeem), f.tx.outputs[i].script_public_key);
    }
    for l in &b.live_utxos {
        let tp = lottery::index::template_of(&t, l.role);
        let bytes = l.state.as_ref().unwrap();
        let redeem = [tp.prefix.as_slice(), bytes, &tp.suffix].concat();
        assert_eq!(harness::pay_to_script_hash_script(&redeem).script(), l.spk.as_slice());
    }
    // The old shape: a full redeem script where the state bytes belong.
    let mut old = b.clone();
    old.genesis_states = states.iter().map(|s| s.redeem(&t)).collect();
    let e = bundle::verify(&old).unwrap_err();
    assert!(e.contains("genesis state 0") && e.contains("redeem"), "{e}");
    println!("S3b old-shape genesis states: {e}");
    let mut old = b.clone();
    for (l, s) in old.live_utxos.iter_mut().zip(&states) {
        l.state = Some(s.redeem(&t));
    }
    let e = bundle::verify(&old).unwrap_err();
    assert!(e.contains("live UTXO") && e.contains("redeem"), "{e}");
    println!("S3b old-shape live UTXOs: {e}");
    // A round trip through the file keeps the state bytes.
    let back: Bundle = store::read(&store::write(&b, store::BUNDLE).unwrap(), store::BUNDLE).unwrap();
    assert_eq!(back, b);
    bundle::verify(&back).unwrap();
}

// ---------------------------------------------------------------- IX9

/// IX9: `fee_for(mass, rate)` raises a rate below 100 sompi per gram to the floor, keeps a
/// higher one, and an overflowing product is an error; at the floor it equals the
/// mempool's relay floor fee of a claim.
#[test]
fn ix9_fee_floor() {
    assert_eq!(fee_for(2_000, 0).unwrap(), 200_000);
    assert_eq!(fee_for(2_000, 99).unwrap(), 200_000);
    assert_eq!(fee_for(2_000, 100).unwrap(), 200_000);
    assert_eq!(fee_for(2_000, 101).unwrap(), 202_000);
    assert_eq!(fee_for(0, 1_000).unwrap(), 0);
    assert_eq!(fee_for(u64::MAX / 100, 0).unwrap(), u64::MAX / 100 * 100);
    assert!(fee_for(u64::MAX / 100 + 1, 0).unwrap_err().contains("overflow"));
    assert!(fee_for(2, u64::MAX).is_err());
    assert_eq!(harness::MIN_FEERATE_SOMPI_PER_GRAM, 100);
    let f = common::m4::pay().build().finish();
    let m = harness::masses(&f.tx, &f.entries);
    let cof = harness::MAINNET_PARAMS.block_mass_limits.cofactors();
    let fee_mass = kaspa_consensus_core::mass::NonContextualMasses::new(m.compute, m.transient).normalized_max(&cof);
    assert_eq!(fee_for(fee_mass, 1).unwrap(), lottery::buy::relay_floor_fee(&m));
}

// ---------------------------------------------------------------- S2c (S2 review fixes)

/// The data pushes of a push-only script, as the engine's push opcodes put them on the
/// stack (rusty-kaspa v2.1.0 `crypto/txscript/src/opcodes/mod.rs:250-345`): an independent
/// copy for the tests.
fn stack_items(s: &[u8]) -> Vec<Vec<u8>> {
    let (mut out, mut i) = (vec![], 0);
    while i < s.len() {
        let op = s[i];
        i += 1;
        let n = match op {
            0x00 => 0,
            0x01..=0x4b => op as usize,
            0x4c => {
                i += 1;
                s[i - 1] as usize
            }
            0x4d => {
                i += 2;
                u16::from_le_bytes([s[i - 2], s[i - 1]]) as usize
            }
            0x4f => {
                out.push(vec![0x81]);
                continue;
            }
            0x51..=0x60 => {
                out.push(vec![op - 0x50]);
                continue;
            }
            _ => panic!("opcode {op:#04x}"),
        };
        out.push(s[i..i + n].to_vec());
        i += n;
    }
    out
}

/// How a test pushes one stack item.
#[derive(Clone, Copy, Debug)]
enum Push {
    /// `ScriptBuilder::add_data` (the canonical form).
    Min,
    /// OP_DATA_n with the bytes as they are (n = 1..75).
    Data,
    /// OP_PUSHDATA1 n bytes.
    Pd1,
}

fn push(out: &mut Vec<u8>, d: &[u8], how: Push) {
    match how {
        Push::Min => out.extend(harness::ScriptBuilder::new().add_data(d).unwrap().drain()),
        Push::Data => {
            assert!((1..=75).contains(&d.len()));
            out.push(d.len() as u8);
            out.extend_from_slice(d);
        }
        Push::Pd1 => {
            out.push(0x4c);
            out.push(d.len() as u8);
            out.extend_from_slice(d);
        }
    }
}

/// Every live UTXO of an index as (role, redeem, value), sorted: the rebuilt state without
/// the outpoints.
fn live_states(ix: &Rebuild) -> Vec<(Role, Vec<u8>, u64)> {
    let mut v: Vec<_> = ix.live().values().map(|l| (l.role, l.state.as_ref().map(|s| s.redeem(ix.templates())).unwrap_or_default(), l.value)).collect();
    v.sort();
    v
}

/// S2c MF-B: serde does not recompute a transaction's id (rusty-kaspa v2.1.0
/// `consensus/core/src/tx/serde_impl.rs:288-291, 401-402`), so the id inside the JSON
/// proves nothing. A bundle whose genesis transaction is edited in the file text (its lock
/// time, which only the txid covers; the stored id and `genesis_txid` kept) must be refused.
#[test]
fn s2c_mfb_bundle_txid_recomputed() {
    let (b, _, _) = local_bundle(|_| {});
    let text = store::write(&b, store::BUNDLE).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let lt = v["genesis_tx"]["lockTime"].as_u64().unwrap();
    v["genesis_tx"]["lockTime"] = serde_json::json!(lt + 1);
    let tampered = serde_json::to_string_pretty(&v).unwrap();
    assert_ne!(tampered, text);
    let back: Bundle = store::read(&tampered, store::BUNDLE).unwrap();
    assert_eq!(back.genesis_tx.id(), b.genesis_tx.id(), "the stored id is read verbatim");
    let r = bundle::verify(&back);
    assert!(r.is_err(), "MF-B: a genesis transaction edited in the file passes verify");
    let e = r.unwrap_err();
    assert!(e.contains("id"), "{e}");
    println!("MF-B bundle: {e}");
}

/// The transaction of an index-log line edited in the file text: its lock time + 1, the
/// stored id and `txid` kept. The `tx` field is the transaction's serde JSON or (S2c MF-C)
/// the hex of its Borsh serialization; neither edit recomputes the id.
fn tamper_line_tx(line: &str) -> String {
    let mut v: serde_json::Value = serde_json::from_str(line).unwrap();
    match &v["tx"] {
        serde_json::Value::Object(_) => {
            let lt = v["tx"]["lockTime"].as_u64().unwrap();
            v["tx"]["lockTime"] = serde_json::json!(lt + 1);
        }
        serde_json::Value::String(h) => {
            let mut tx: harness::Transaction = borsh::from_slice(&store::from_hex(h).unwrap()).unwrap();
            tx.lock_time += 1;
            v["tx"] = serde_json::json!(store::to_hex(&borsh::to_vec(&tx).unwrap()));
        }
        other => panic!("tx field {other:?}"),
    }
    serde_json::to_string(&v).unwrap()
}

/// S2c MF-B: an index-log line whose transaction was edited in the file (the stored id
/// kept) is refused when the log is read back or rebuilt.
#[test]
fn s2c_mfb_log_txid_recomputed() {
    let mut ch = Chain::genesis(4);
    let (c, ops) = ch.buy_case(0);
    ch.buy(0, &c, ops, 300);
    let blk = |n: usize| Hash::from_bytes([n as u8; 32]);
    let lines: Vec<IndexLogLine> = ch.log.iter().enumerate().map(|(n, a)| IndexLogLine::add(n as u64, blk(n), 1_000 + n as u64, a)).collect();
    let log = IndexLog { cov: ch.cov, lines, reanchors: vec![] };
    let text = log.to_jsonl().unwrap();
    // The control: the untouched log rebuilds.
    IndexLog::from_jsonl(&text).unwrap().rebuild(ch.t.clone(), ch.cov, None).unwrap();
    let mut rows: Vec<String> = text.lines().map(String::from).collect();
    rows[2] = tamper_line_tx(&rows[2]); // header, genesis, buy
    let tampered = rows.join("\n");
    let r = IndexLog::from_jsonl(&tampered).and_then(|l| l.rebuild(ch.t.clone(), ch.cov, None).map(|_| ()));
    assert!(r.is_err(), "MF-B: a log line whose transaction was edited (id kept) rebuilds");
    let e = r.unwrap_err();
    assert!(e.contains("id"), "{e}");
    println!("MF-B log: {e}");
}

/// S2c MF-C (1): the engine takes pushes with consensus semantics (any push opcode, numbers
/// not minimally encoded: `crypto/txscript/src/data_stack.rs:163-167`), so a third party can
/// submit a `buy()` whose signature script is not the ABI's canonical encoding. For each
/// re-encoding of the canonical buy's arguments: if every input passes the engine, the
/// index must classify it as the same Buy and rebuild the same state; the result of each
/// variant is printed (accepted or the engine's reason).
#[test]
fn s2c_mfc_noncanonical_buy_pushes() {
    let mut ch = Chain::genesis(4);
    let price = ch.controller.1.price as u64;
    let (mut c, ops) = ch.buy_case_funded(0, 3 * price + 10 * KAS);
    c.bundle = vec![Combo::new([1, 2, 3, 4, 5], 1)];
    c.with_ref = true;
    let canon = ch.buy_built(&c, ops.clone());
    let f0 = canon.finish();
    f0.run().unwrap();
    let sig0 = f0.tx.inputs[0].signature_script.clone();
    let items = stack_items(&sig0);
    assert_eq!(items.len(), 11, "9 arguments, the dispatch tag and the redeem");
    let mut again = vec![];
    for it in &items {
        push(&mut again, it, Push::Min);
    }
    assert_eq!(again, sig0, "the canonical encoding is add_data of each stack item");
    let genesis = ch.log[0].clone();
    let rebuild = |a: &Accepted| {
        let mut ix = Rebuild::new(ch.t.clone(), ch.cov, None);
        ix.apply(&genesis).unwrap();
        let ev = ix.apply(a);
        (ix, ev)
    };
    let (ix0, ev0) = rebuild(&Accepted::new(f0.tx.clone(), 300, &f0.entries));
    let Ok(Some(Event::Buy(b0))) = ev0 else { panic!("canonical buy: {ev0:?}") };
    // (name, argument index, new stack item or None = same item, encoding).
    let variants: Vec<(&str, usize, Option<Vec<u8>>, Push)> = vec![
        ("b = 1 as [0x01, 0x00] (non-minimal number)", 0, Some(vec![0x01, 0x00]), Push::Data),
        ("bundle with OP_PUSHDATA1", 1, None, Push::Pd1),
        ("b = 1 as OP_DATA_1 0x01", 0, None, Push::Data),
        ("leaves with OP_PUSHDATA1", 2, None, Push::Pd1),
        ("with_ref = true as [0x01, 0x00]", 4, Some(vec![0x01, 0x00]), Push::Data),
        // The ABI pushes byte 0 as OP_DATA_1 0x00 (`ScriptBuilder::add_data` keeps it); OP_0
        // puts the empty item on the stack instead.
        ("payout_class 0 as OP_0 (empty item)", 5, Some(vec![]), Push::Min),
        ("with_ref = true as OP_DATA_1 0x01", 4, None, Push::Data),
        // A bool the script reads both as OP_IF truth and as a number (shard.sil 141, 255).
        ("with_ref = 2", 4, Some(vec![0x02]), Push::Data),
    ];
    let mut failures = vec![];
    for (name, arg, item, how) in variants {
        let mut sig = vec![];
        for (n, it) in items.iter().enumerate() {
            if n == arg {
                push(&mut sig, item.as_deref().unwrap_or(it), how);
            } else {
                push(&mut sig, it, Push::Min);
            }
        }
        assert_ne!(sig, sig0, "{name}: not a re-encoding");
        let mut b = canon.clone();
        b.inputs[0].kind = lottery::buy::InKind::Raw(sig);
        let f = b.finish();
        match f.run() {
            Err((i, e)) => {
                // S2d item 4: the index decodes arguments as the engine does, so an argument
                // the engine refuses (SilverScript's entry size checks: byte exactly 1 byte,
                // bool 0 or 1 byte, `compiler/compile/statement.rs:105-150`) is not decoded
                // into a Buy either.
                let (_, ev) = rebuild(&Accepted::new(f.tx.clone(), 300, &f.entries));
                println!("MF-C {name}: the engine rejects it at input {i}: {e:?}; the index gives {}", if ev.is_err() { "Err" } else { "Ok" });
                if !matches!(ev, Err(_)) {
                    failures.push(name);
                }
            }
            Ok(()) => {
                let (ix, ev) = rebuild(&Accepted::new(f.tx.clone(), 300, &f.entries));
                match ev {
                    Ok(Some(Event::Buy(b))) if b == b0 && live_states(&ix) == live_states(&ix0) => {
                        println!("MF-C {name}: accepted by the engine; the index rebuilds the same Buy")
                    }
                    other => {
                        println!("MF-C {name}: accepted by the engine; the index gives {other:?}");
                        failures.push(name);
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "MF-C: re-encodings the index and the engine judge differently: {failures:?}");
    // shard.sil reads with_ref as OP_IF truth (255) and as a number (141: extra = nOut - 2
    // - wr, 0 <= extra <= 1), so with_ref = 2 is accepted above with the canonical four
    // outputs. A larger with_ref, or with_ref = 2 with one more output, passes the `extra`
    // checks with five outputs; the only limit left is the compiler-generated
    // `require(end - start <= max)` of `for(k, 2, nOut, 2)` (shard.sil 146-148; SilverScript
    // `compiler/for.rs:136-153`, marked TODO at 137). Both must be rejected by the engine at
    // the shard (R2); `s2d_silverscript_for_loop_bound_is_enforced` pins the compiler side.
    for wr in [3u8, 2u8] {
        let mut b = canon.clone();
        let last = b.outputs.len() - 1;
        let change = b.outputs[last].clone();
        let half = change.value / 2;
        b.outputs[last].value = change.value - half;
        b.outputs.push(harness::TransactionOutput::new(half, change.script_public_key.clone()));
        assert_eq!(b.outputs.len(), 5);
        let wr_item = [wr];
        let mut sig = vec![];
        for (n, it) in items.iter().enumerate() {
            push(&mut sig, if n == 4 { &wr_item[..] } else { it }, if n == 4 { Push::Data } else { Push::Min });
        }
        b.inputs[0].kind = lottery::buy::InKind::Raw(sig);
        let f = b.finish();
        match f.run() {
            Err((i, e)) => {
                assert_eq!(i, 0, "R2 probe with_ref = {wr}, 5 outputs: rejected at input {i}, expected the shard (input 0): {e:?}");
                println!("R2 probe with_ref = {wr}, 5 outputs: the engine rejects it at input {i}: {e:?}");
            }
            Ok(()) => panic!("R2: buy() with with_ref = {wr} and 5 outputs ({} plain after the referral) ACCEPTED by the engine", f.tx.outputs.len() - 3),
        }
    }
}

/// S2c MF-C (2): a transaction of the instance with a foreign funding input whose
/// `sequence` and spent amount are >= 2^53 (and a change output above 2^53 sompi) is
/// written to the index log, read back and rebuilt. Rebuild never checks a foreign input.
#[test]
fn s2c_mfc_log_line_with_large_foreign_u64() {
    let mut ch = Chain::genesis(4);
    let (c, ops) = ch.buy_case(0);
    ch.buy(0, &c, ops, 300);
    let mut log = ch.log.clone();
    let big = 1u64 << 60;
    let a = &mut log[1];
    a.tx.inputs[1].sequence = big + 5;
    a.spent[1].amount += big;
    let last = a.tx.outputs.len() - 1;
    a.tx.outputs[last].value += big;
    a.tx.finalize();
    let direct = replay(&ch, &log);
    let blk = |n: usize| Hash::from_bytes([n as u8; 32]);
    let lines: Vec<IndexLogLine> = log.iter().enumerate().map(|(n, a)| IndexLogLine::add(n as u64, blk(n), 1_000 + n as u64, a)).collect();
    let il = IndexLog { cov: ch.cov, lines, reanchors: vec![] };
    let text = il.to_jsonl().unwrap_or_else(|e| panic!("MF-C: a log line with a foreign u64 >= 2^53 is unwritable: {e}"));
    let back = IndexLog::from_jsonl(&text).unwrap();
    assert_eq!(back, il);
    let ix = back.rebuild(ch.t.clone(), ch.cov, None).unwrap();
    assert_eq!(live_states(&ix), live_states(&direct));
    assert_eq!(ix.log()[1].tx, log[1].tx);
    assert_eq!(ix.log()[1].spent, log[1].spent);
    println!("MF-C log line with sequence {} and spent {} sompi: written and rebuilt ({} bytes)", big + 5, log[1].spent[1].amount, text.len());
}

/// S2c SF3 (D115): crafted states on node-chosen input are errors, never panics (debug
/// overflow checks on). A `buy()` whose spent shard state carries price = i64::MAX, -1 or
/// i64::MIN + 1 (i64::MIN has no script-number encoding; its redeem replaced, the arguments
/// the canonical ones), and a count string
/// at m = -1.
#[test]
fn s2c_sf3_crafted_states_are_errors() {
    let mut ch = Chain::genesis(4);
    let price = ch.controller.1.price as u64;
    let (mut c, ops) = ch.buy_case_funded(0, 3 * price + 10 * KAS);
    c.bundle = vec![Combo::new([1, 2, 3, 4, 5], 1), Combo::new([1, 2, 3, 4, 6], 1), Combo::new([1, 2, 3, 4, 7], 2)];
    c.with_ref = true;
    let f = ch.buy_built(&c, ops).finish();
    let sig = f.tx.inputs[0].signature_script.clone();
    let redeem = ch.t.shard_redeem(&c.shard);
    let tail = harness::ScriptBuilder::new().add_data(&redeem).unwrap().drain();
    assert!(sig.ends_with(&tail));
    let head = sig[..sig.len() - tail.len()].to_vec();
    for p in [i64::MAX, -1, i64::MIN + 1] {
        let mut s = c.shard.clone();
        s.price = p;
        let mut tx = f.tx.clone();
        tx.inputs[0].signature_script = lottery::buy::push_redeem(head.clone(), &ch.t.shard_redeem(&s));
        tx.finalize();
        let a = Accepted::new(tx.clone(), 300, &f.entries);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut ix = Rebuild::new(ch.t.clone(), ch.cov, None);
            ix.apply(&ch.log[0]).unwrap();
            ix.apply(&a).map(|_| ())
        }));
        match r {
            Ok(Err(e)) => println!("SF3 price {p}: {e}"),
            Ok(Ok(())) => panic!("SF3 price {p}: applied"),
            Err(_) => panic!("SF3 price {p}: the index panicked on a crafted state"),
        }
        let r = std::panic::catch_unwind(|| lottery::index::parse_buy(&tx, &Templates::get(b_max())).map(|_| ()));
        assert!(matches!(r, Ok(Err(_))), "SF3 price {p}: parse_buy panicked or accepted");
    }
    let r = std::panic::catch_unwind(|| lottery::index::leaf_counts(&[0, 0], -1));
    assert!(matches!(r, Ok(Err(_))), "SF3: leaf_counts at m = -1 panicked or accepted");
}

/// S2c SF4: a missed non-shard transition is reported, not accepted silently. Dropping
/// round 1's `set_params` leaves the old params UTXO live while close() spends a params
/// UTXO the index never saw created; dropping round 1's first claim leaves the old ticket
/// and pool live while the second claim spends their successors. Each is a gap: no proof is
/// offered (`trees`), and the gap is listed.
#[test]
fn s2c_sf4_missed_non_shard_transition() {
    let (ch, _, gstates) = game();
    let first = |f: &dyn Fn(&Event) -> bool| ch.log.iter().position(|a| classify(&a.tx, &ch.t, ch.cov).as_ref().is_some_and(|e| f(e))).unwrap();
    let sp = first(&|e| matches!(e, Event::SetParams(_)));
    let cl = first(&|e| matches!(e, Event::Claim(c) if c.ticket.round == 1));
    for (what, drop) in [("set_params", sp), ("round-1 claim", cl)] {
        let log: Vec<Accepted> = ch.log.iter().enumerate().filter(|(i, _)| *i != drop).map(|(_, a)| a.clone()).collect();
        let mut ix = Rebuild::new(ch.t.clone(), ch.cov, Some(gstates.clone()));
        let mut err = None;
        for a in &log {
            if let Err(e) = ix.apply(a) {
                err = Some(e);
                break;
            }
        }
        let reported = err.is_some() || ix.trees(1).is_err();
        assert!(reported, "SF4: the index missed the {what} (log entry {drop}) and reports nothing");
        println!("SF4 {what} dropped: apply error {err:?}; trees(1): {:?}", ix.trees(1).err());
        // After the fix: the whole log still applies, and the gap is listed where it belongs.
        assert!(err.is_none(), "{what}: a gap is reported, not a halt");
        let snap = IndexSnapshot::of(&ix, None);
        if drop == sp {
            assert!(ix.gaps().iter().any(|g| g.contains("Params")), "{:?}", ix.gaps());
            assert_eq!(snap.gaps, ix.gaps());
            for r in [1, 2, 3] {
                assert!(ix.trees(r).err().unwrap_or_default().contains("missed"), "round {r}: no proof from an index with a gap");
            }
        } else {
            assert!(ix.gaps().is_empty(), "{:?}", ix.gaps());
            let r1 = ix.round(1).unwrap();
            assert!(r1.gaps.iter().any(|g| g.contains("Ticket")), "{:?}", r1.gaps);
            assert!(r1.incomplete.as_deref().unwrap_or("").contains("missed"), "{:?}", r1.incomplete);
            assert!(ix.trees(1).err().unwrap_or_default().contains("incomplete"));
            ix.trees(2).unwrap();
        }
    }
}

/// S2c (note): `bundle::verify` bounds the file's B_MAX before compiling templates for it.
#[test]
fn s2c_bundle_b_max_bounded() {
    let (b, _, _) = local_bundle(|_| {});
    for bad in [0u64, lottery::game::TICKET_B_MAX as u64 + 1, u64::MAX] {
        let mut m = b.clone();
        m.toolchain.b_max = bad;
        let r = std::panic::catch_unwind(|| bundle::verify(&m));
        match r {
            Ok(Err(e)) => assert!(e.contains("b_max"), "b_max {bad}: refused for another reason: {e}"),
            Ok(Ok(_)) => panic!("b_max {bad}: verify passed"),
            Err(_) => panic!("b_max {bad}: verify panicked"),
        }
    }
}

/// S2c (note): on public testnet anyone may buy, so the wallet ingests every chain-accepted
/// buy on the instance's shards (its trees must equal the frozen roots) and records only the
/// tickets whose payout is its own.
#[test]
fn s2c_wallet_ingests_third_party_buys() {
    let mut ch = Chain::genesis(4);
    let me = key(1).payout();
    let other = key(2).payout();
    let mut mine = 0;
    for (n, who) in [me, other, me, other].into_iter().enumerate() {
        let (mut c, ops) = ch.buy_case(0);
        c.bundle = vec![Combo::new([1, 2, 3, 4, 5 + n as u8], 1)];
        c.payout = who;
        ch.buy(0, &c, ops, 300 + n as u64);
        mine += (who == me) as usize;
    }
    let t = ch.t.clone();
    let mut w = Wallet::new(ch.cov, t.b_max, 1, ch.controller.1.m, 4);
    let mut recorded = 0;
    for a in &ch.log[1..] {
        recorded += w.record_buy(&t, &a.tx, a.daa, &[me]).unwrap() as usize;
    }
    assert_eq!(recorded, mine);
    // Fed only its own buys, the wallet refuses the next one (it missed a purchase).
    let mut own_only = Wallet::new(ch.cov, t.b_max, 1, ch.controller.1.m, 4);
    own_only.record_buy(&t, &ch.log[1].tx, ch.log[1].daa, &[me]).unwrap();
    assert!(own_only.record_buy(&t, &ch.log[3].tx, ch.log[3].daa, &[me]).unwrap_err().contains("did not see"));
    assert_eq!(w.tickets.len(), mine, "the wallet records only its own tickets");
    assert!(w.tickets.iter().all(|x| Payout::from_fields(x.payout_class, &x.payout_key) == me));
    assert_eq!(w.shard_tree(0).unwrap().root(), ch.shards[0].1.root, "the wallet's tree follows every buy");
}

/// S2d item 1: the only output-count limit of `buy()` with a large `with_ref` is the
/// `require(end - start <= max)` that the pinned SilverScript (v1.0.0) generates for every
/// `for(k, start, end, max)` (`silverscript-lang/src/compiler/for.rs:136-153`; a TODO at 137
/// considers moving it to debug-mode compilation). This test pins that compiler behaviour
/// directly: a probe contract with the shard's loop shape `for(k, 2, n, 2)` is compiled by
/// the same compiler the covenants use and run in the engine. n = 4 (2 iterations) passes,
/// n = 5 (3 > max) must fail. Control: the same loop with max 3 passes n = 5, so the
/// failure comes from the bound, not from the body. A SilverScript change that drops or
/// weakens the bound fails here (and in the R2 probe of `s2c_mfc_noncanonical_buy_pushes`
/// on the real shard script). Behavioural rather than a byte search of the compiled shard:
/// the bound compiles to ordinary opcodes (OP_SUB, a number, OP_LESSTHANOREQUAL, OP_VERIFY)
/// whose position and number encoding a byte pattern would have to guess.
#[test]
fn s2d_silverscript_for_loop_bound_is_enforced() {
    use harness::{ArtifactValue, TransactionOutput, UtxoEntry, compile_source, entry_sigscript, execute, input, p2sh, tx};
    let run = |max: i64, n: i64| {
        let src = format!(
            "pragma silverscript ^0.1.0;\ncontract ForBound(int c) {{\n    int x = c;\n    entry probe(int n) {{\n        for(k, 2, n, {max}) {{\n            require(k >= 2);\n        }}\n    }}\n}}\n"
        );
        let art = compile_source(&src, &[ArtifactValue::Int(0)]);
        let entries = vec![UtxoEntry::new(1_000_000, p2sh(&art), 0, false, None)];
        let sig = entry_sigscript(&art, "probe", &[ArtifactValue::Int(n)]);
        let t = tx(vec![input(0, sig, 0)], vec![TransactionOutput::new(900_000, p2sh(&art))]);
        execute(&t, &entries, 0, false)
    };
    for n in [2, 3, 4] {
        assert!(run(2, n).is_ok(), "for(k, 2, {n}, 2) must pass: {:?}", run(2, n));
    }
    for n in [5, 6, 100] {
        let r = run(2, n);
        assert!(r.is_err(), "SilverScript dropped the for-loop bound: for(k, 2, {n}, 2) passes ({} iterations > max 2)", n - 2);
        println!("for(k, 2, {n}, 2): rejected: {:?}", r.unwrap_err());
    }
    assert!(run(3, 5).is_ok(), "control: for(k, 2, 5, 3) must pass: {:?}", run(3, 5));
    assert!(run(3, 6).is_err(), "control: for(k, 2, 6, 3) must fail");
}

/// A well-formed bundle (every script, the covenant ID, the txid consistent) whose genesis
/// states are the local fixture's after `edit`, the transaction's output values kept. The
/// state values need not be sane: `bundle::verify` must refuse them without panicking.
fn crafted_bundle(edit: impl FnOnce(&mut GenesisCase)) -> Bundle {
    let (_, g0, f0) = local_bundle(|_| {});
    let mut g = g0.clone();
    edit(&mut g);
    let t = g.templates();
    let mut tx = f0.tx.clone();
    let n = g.k() + 5;
    for (i, s) in bundle::genesis_states(&g).iter().enumerate() {
        tx.outputs[i].script_public_key = harness::pay_to_script_hash_script(&s.redeem(&t));
    }
    let cov = bundle::recompute_cov(tx.inputs[0].previous_outpoint, &tx.outputs[..n]);
    for o in &mut tx.outputs[..n] {
        o.covenant = Some(harness::CovenantBinding::new(0, cov));
    }
    tx.finalize();
    let f = lottery::buy::Final { tx, entries: f0.entries.clone(), units: f0.units.clone(), sim: f0.sim.clone() };
    Bundle::new(&g, &f, vec![], None).unwrap()
}

/// S2d item 2: `bundle::verify` computed the genesis output values (`GenesisCase::
/// vault_value`, `shard_value`: unchecked i64 / u64 sums) before `validate_with`, so a
/// crafted bundle panicked (debug) instead of being refused. Each crafted bundle must be an
/// `Err`, never a panic.
#[test]
fn s2d_bundle_verify_crafted_values_are_errors() {
    type Edit = fn(&mut GenesisCase);
    let cases: [(&str, Edit); 3] = [
        ("vault total_assets i64::MAX, pend_dep 1", |g| {
            g.vault.total_assets = i64::MAX;
            g.vault.pend_dep = 1;
        }),
        // i64::MIN has no 8-byte script-number encoding (sign-magnitude), so no redeem
        // script carries it; i64::MIN + 1 - 2 overflows the same subtraction.
        ("shard 0 sales i64::MIN + 1, ref_paid 2", |g| {
            g.shards[0].sales = i64::MIN + 1;
            g.shards[0].ref_paid = 2;
        }),
        ("controller acc i64::MAX", |g| g.controller.acc = i64::MAX),
    ];
    let mut panicked = vec![];
    for (name, edit) in cases {
        let b = crafted_bundle(edit);
        match std::panic::catch_unwind(|| bundle::verify(&b)) {
            Ok(Err(e)) => println!("S2d crafted {name}: refused: {e}"),
            Ok(Ok(_)) => panic!("S2d crafted {name}: verify passed"),
            Err(_) => panicked.push(name),
        }
    }
    assert!(panicked.is_empty(), "bundle::verify panicked on crafted states: {panicked:?}");
}

/// S2d item 3: `IndexLog::rebuild` trusted the covenant ID in the log's header. A log whose
/// header names another instance (here: instance A's lines under instance B's ID) rebuilt
/// "successfully" as B with every line skipped as foreign: an empty index, silently. The
/// caller now passes the covenant ID it verified (the bundle's), and a differing header is
/// refused.
#[test]
fn s2d_index_log_header_cov_checked() {
    let mut a = Chain::genesis(4);
    let (c, ops) = a.buy_case(0);
    a.buy(0, &c, ops, 300);
    let mut other = a.cov.as_bytes();
    other[0] ^= 1;
    let other = Hash::from_bytes(other);
    let blk = |n: usize| Hash::from_bytes([n as u8; 32]);
    let lines: Vec<IndexLogLine> = a.log.iter().enumerate().map(|(n, x)| IndexLogLine::add(n as u64, blk(n), 1_000 + n as u64, x)).collect();
    let good = IndexLog { cov: a.cov, lines: lines.clone(), reanchors: vec![] };
    let ix = good.rebuild(a.t.clone(), a.cov, None).unwrap();
    assert!(!ix.live().is_empty());
    // A's lines under another instance's ID, rebuilt for A (the verified ID) and for B.
    let swapped = IndexLog::from_jsonl(&IndexLog { cov: other, lines, reanchors: vec![] }.to_jsonl().unwrap()).unwrap();
    for (expected, name) in [(a.cov, "header B, expected A"), (other, "header A, expected B")] {
        let log = if expected == a.cov { &swapped } else { &good };
        let r = log.rebuild(a.t.clone(), expected, None);
        match r {
            Err(e) => {
                assert!(e.contains("covenant"), "{name}: refused for another reason: {e}");
                println!("S2d {name}: {e}");
            }
            Ok(ix) => panic!("S2d {name}: a log whose header names another instance rebuilds ({} live UTXOs)", ix.live().len()),
        }
    }
}

/// S2d item 5 (N5): a wallet file is local but still a file. Its `b_max` selects the
/// templates to compile (`Templates::get`) and its `m` sizes the shard trees; out-of-range
/// values must be an `Err` before use (b_max in [1, TICKET_B_MAX], m in [2, M_HARD]), never a
/// panic or an unbounded compile.
#[test]
fn s2d_wallet_b_max_and_m_bounded() {
    let cov = Hash::from_bytes([3; 32]);
    let payer = Payer { funding_sk: lottery::buy::FUNDING_SK, fee: 10_000 };
    let op = TransactionOutpoint::new(Hash::from_bytes([4; 32]).into(), 1);
    // b_max: checked before any template is compiled. Asserted one by one, so that before the
    // fix the run stops before u64::MAX reaches the compiler.
    for bad in [0u64, lottery::game::TICKET_B_MAX as u64 + 1, u64::MAX] {
        let mut w = Wallet::new(cov, 4, 1, 3, 4);
        w.b_max = bad;
        match std::panic::catch_unwind(|| ClaimCase::from_store_index(&w, op, 0, &[], &payer).map(|_| ())) {
            Ok(Err(e)) => {
                assert!(e.contains("b_max"), "b_max {bad}: refused for another reason: {e}");
                println!("S2d wallet b_max {bad}: {e}");
            }
            Ok(Ok(())) => panic!("b_max {bad}: a claim was built"),
            Err(_) => panic!("b_max {bad}: panicked"),
        }
    }
    // m: checked before the tree is sized or a combination checked against it.
    for bad in [i64::MIN, -1, 0, 1, lottery::game::M_HARD + 1, i64::MAX] {
        let mut w = Wallet::new(cov, 4, 1, 3, 4);
        w.m = bad;
        match std::panic::catch_unwind(|| w.shard_tree(0).map(|_| ())) {
            Ok(Err(e)) => {
                assert!(e.contains(" m "), "m {bad}: refused for another reason: {e}");
                println!("S2d wallet m {bad}: {e}");
            }
            Ok(Ok(())) => panic!("m {bad}: shard_tree accepted it"),
            Err(_) => panic!("m {bad}: shard_tree panicked"),
        }
    }
    for good in [2, 3, lottery::game::M_HARD] {
        let mut w = Wallet::new(cov, 4, 1, good, 4);
        w.m = good;
        assert!(w.shard_tree(0).is_ok(), "m {good}");
    }
}

/// S2d item 6 (N4): `DrawCase::from_store_absorbing` refused an incomplete drawn round (its
/// trees) but not an incomplete round of an absorbed pool. With round 1's second buy
/// missing from the index (IX4's gap: round 1 incomplete; rounds 2 and 3 complete), the
/// round-3 void absorbing the expired round-1 pools must be refused like the drawn round's.
#[test]
fn s2d_absorbed_pool_round_must_be_complete() {
    let (ch, draws, gstates) = game();
    let (r, c, f) = draws.iter().find(|(_, c, _)| c.void && c.a() > 0).expect("the round-3 void absorbing round-1 pools");
    let at = ch.log.iter().position(|a| a.tx.id() == f.tx.id()).unwrap();
    let a = &ch.log[at];
    let absorb: Vec<TransactionOutpoint> = (0..c.a()).map(|i| a.tx.inputs[4 + i].previous_outpoint).collect();
    let payer = Payer { funding_sk: c.funding_sk, fee: c.fee };
    let buys: Vec<usize> =
        ch.log.iter().enumerate().filter(|(_, a)| matches!(classify(&a.tx, &ch.t, ch.cov), Some(Event::Buy(ref b)) if b.shard.round == 1)).map(|(i, _)| i).collect();
    let drop = buys[2];
    let log: Vec<Accepted> = ch.log[..at].iter().enumerate().filter(|(i, _)| *i != drop).map(|(_, a)| a.clone()).collect();
    let ix = replay_with(&ch, &log, Some(gstates.clone()));
    // The setting: round 1 incomplete, the drawn round complete, every absorbed pool live,
    // of round 1 and of known state.
    assert!(ix.trees(1).err().unwrap_or_default().contains("incomplete"));
    ix.trees(*r).unwrap_or_else(|e| panic!("round {r}: {e}"));
    for op in &absorb {
        let l = ix.live().get(op).unwrap_or_else(|| panic!("absorbed pool {op} live in the index"));
        assert!(matches!(&l.state, Some(State::Pool(p)) if p.round == 1), "{op}: a round-1 pool");
    }
    let res = DrawCase::from_store_absorbing(&ix, None, &[funding_of(a, 4 + c.a())], &absorb, c.funder, &payer);
    match res {
        Err(e) => {
            assert!(e.contains("round 1") && e.contains("incomplete"), "refused for another reason: {e}");
            println!("S2d round {r} void absorbing pools of an incomplete round 1: {e}");
        }
        Ok(_) => panic!("N4: the round-{r} void absorbing pools of the incomplete round 1 was built"),
    }
    // Control: with the full log the same call builds (IX2 checks it byte for byte).
    let full = replay_with(&ch, &ch.log[..at], Some(gstates.clone()));
    DrawCase::from_store_absorbing(&full, None, &[funding_of(a, 4 + c.a())], &absorb, c.funder, &payer).unwrap();
}
