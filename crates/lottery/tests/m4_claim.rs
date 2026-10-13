//! M4 positive tests (spec M4 A9: KP1-KP17, the script half of KP18, KP19; the Rust half of
//! KP18 is in m4_game.rs).
//!
//! As M3 A8: full transactions in the v2.1.0 engine with production flags and committed
//! budgets (`accept`: every input under its committed budget, input 0 first). Every claim
//! is checked output by output against `ClaimExpected` and for conservation (A11 item 6)
//! through `check_claim`; returns and refreshes through `check_return` / `check_refresh`.
//! Main positives run real genesis -> buys -> `close()` -> `draw()` -> claims (D63); range
//! and boundary cases use fabricated tickets and pools under the instance covenant ID.
//!
//! Each test first asserts, on the Rust side, that its fixture exercises the case it names
//! (mode, tier, h, ...), so a fixture that drifts fails before any script runs.
//!
//! Stage 2a (tests first): until stage 3 writes the ticket's entries, every claim, return
//! and ticket refresh is rejected by the ticket placeholder at input 0 (dispatch finds no
//! entry with the `SIG_*` tag and ends in OP_RETURN); pool and marker refreshes are
//! rejected by the pool / marker at input 0 the same way.

mod common;

use common::m2::*;
use harness::CovenantBinding;
use lottery::buy::{Final, KAS, TEST_DEPOSIT, spk_of};
use lottery::claim::*;
use lottery::close::{FundingIn, TEST_DEADLINE};
use lottery::combo::Combo;
use lottery::game::{ClaimMode, D_IDLE, D_RET, M_CLAIM, P_MIN};
use lottery::genesis::{D_FLOOR, GenesisCase};
use lottery::smt::{PATH_BYTES, empty_hashes, leaf_hash};
use lottery::state::{K_MAX, Payout, TIER_JACKPOT, TIER_LOWER0, TIER_SECOND};

// ---------------------------------------------------------------- checks

/// Every output (count, value, script, covenant binding) as `ClaimExpected` says,
/// inputs - outputs = fee, and the A4 conservation pool + ticket = pool' + (payout | ticket'
/// [+ payout]).
fn check_claim(c: &ClaimCase, f: &Final) -> ClaimExpected {
    let x = c.expected();
    assert!(x.rejects.is_empty(), "{:?}", x.rejects);
    let t = tpl();
    assert_eq!(f.tx.outputs.len(), x.n_outputs(), "A4 output count");
    for (i, (o, v)) in f.tx.outputs.iter().zip(&x.values).enumerate() {
        assert_eq!(o.value, *v, "output {i} value");
    }
    // pool' is byte-identical to the pool (check 14).
    assert_eq!(f.tx.outputs[0].script_public_key, t.pool_spk(&c.pool), "pool' = pool");
    assert_eq!(f.tx.outputs[0].value, c.pool_value - x.prize as u64, "pool drops by exactly the prize");
    for (i, o) in f.tx.outputs.iter().enumerate() {
        if i < x.n_cov() {
            assert_eq!(o.covenant, Some(CovenantBinding::new(0, c.cov)), "output {i} authorized by input 0");
        } else {
            assert!(o.covenant.is_none(), "output {i} has the zero ID");
        }
    }
    let payout = Payout::from_fields(c.ticket.payout_class, &c.ticket.payout_key).spk();
    match x.mode {
        ClaimMode::Final => {
            assert_eq!(f.tx.outputs[1].script_public_key, payout);
            assert_eq!(f.tx.outputs[1].value, c.ticket_value + x.prize as u64, "FINAL: prize + ticket value");
        }
        ClaimMode::Pay => {
            let tn = x.ticket.as_ref().unwrap();
            assert_eq!(f.tx.outputs[1].script_public_key, t.ticket_spk(tn));
            assert_eq!(f.tx.outputs[1].value, c.ticket_value, "PAY: ticket' keeps its value");
            assert_eq!(f.tx.outputs[2].script_public_key, payout);
            assert_eq!(f.tx.outputs[2].value, x.prize as u64);
        }
        ClaimMode::Accrue => {
            let tn = x.ticket.as_ref().unwrap();
            assert_eq!(f.tx.outputs[1].script_public_key, t.ticket_spk(tn));
            assert_eq!(f.tx.outputs[1].value, c.ticket_value + x.prize as u64, "ACCRUE: ticket' + prize");
        }
    }
    if let Some(tn) = &x.ticket {
        assert_eq!(tn.claimed[0], c.ticket.claimed[0] | x.bit, "ticket' marks the index");
        assert_eq!((tn.round, tn.deadline, tn.b, &tn.bundle, tn.payout_class, tn.payout_key), (c.ticket.round, c.ticket.deadline, c.ticket.b, &c.ticket.bundle, c.ticket.payout_class, c.ticket.payout_key));
    }
    if let Some(i) = x.change_output {
        assert_eq!(i, f.tx.outputs.len() - 1, "change last");
        assert_eq!(f.tx.outputs[i].script_public_key, spk_of(&c.funding_sk));
    }
    assert_eq!(f.fee(), c.fee, "inputs - outputs = fee");
    let covenant_out: u64 = x.values[..x.mode.n_fixed_outputs()].iter().sum();
    assert_eq!(c.pool_value + c.ticket_value, covenant_out, "A4 conservation (reserves do not move)");
    x
}

/// The return's outputs: (path a) pool' byte-identical with the same value; the payout of
/// the whole ticket value to the ticket's payout script; change last; fee.
fn check_return(c: &ReturnCase, f: &Final) {
    let x = c.expected();
    let t = tpl();
    assert_eq!(f.tx.outputs.len(), x.values.len());
    for (i, (o, v)) in f.tx.outputs.iter().zip(&x.values).enumerate() {
        assert_eq!(o.value, *v, "output {i} value");
    }
    if let ReturnPath::WithPool { pool, pool_value, .. } = &c.path {
        assert_eq!(f.tx.outputs[0].script_public_key, t.pool_spk(pool), "pool' byte-identical");
        assert_eq!(f.tx.outputs[0].value, *pool_value, "pool' same value");
        assert_eq!(f.tx.outputs[0].covenant, Some(CovenantBinding::new(0, c.cov)));
    }
    let p = &f.tx.outputs[x.payout_output];
    assert_eq!(p.script_public_key, Payout::from_fields(c.ticket.payout_class, &c.ticket.payout_key).spk());
    assert_eq!(p.value, c.ticket_value, "the whole ticket value");
    assert!(p.covenant.is_none());
    assert_eq!(f.fee(), c.fee);
}

/// The refresh's output 0 is the input's script and value under the same covenant ID.
fn check_refresh(c: &RefreshCase, f: &Final) {
    let o = &f.tx.outputs[0];
    assert_eq!(o.script_public_key, f.entries[0].script_public_key, "byte-identical successor");
    assert_eq!(o.value, c.value, "same value");
    assert_eq!(o.covenant, Some(CovenantBinding::new(0, c.cov)));
    assert_eq!(f.tx.outputs.len(), 1 + c.change as usize);
    assert_eq!(f.fee(), c.fee);
}

/// Run a standalone claim on the committed templates.
fn run(c: &ClaimCase) -> (ClaimExpected, Final) {
    let f = accept_m4("claim (standalone)", &c.build());
    let x = check_claim(c, &f);
    (x, f)
}

// ---------------------------------------------------------------- chain fixtures

/// Claims, returns and refreshes of round 1 are funded at this DAA and applied after it.
const NOW: u64 = 2_000_000;

/// Genesis (K = 4) with `price` sompi per combination and lower-tier shares `st`.
fn genesis_priced(price: u64, st: [i64; 5]) -> Chain {
    Chain::genesis_with(
        4,
        |g| {
            g.controller.price = price as i64;
            g.controller.st = st;
            for s in &mut g.shards {
                s.price = price as i64;
            }
        },
        true,
    )
}

/// The winning numbers (n1..n5, bonus) the TEST-ONLY seed gives the chain's current round.
fn win6(ch: &Chain, seed: &[u8; 32]) -> [u8; 6] {
    let w = ch.winning(seed);
    [w.main[0], w.main[1], w.main[2], w.main[3], w.main[4], w.bonus]
}

/// Buy `bundles` (shard, combinations) in order before T, close the round and draw it.
fn sell_close_draw(ch: &mut Chain, seed: [u8; 32], bundles: Vec<(usize, Vec<Combo>)>) {
    let price = ch.controller.1.price as u64;
    for (n, (i, b)) in bundles.into_iter().enumerate() {
        let (mut c, ops) = ch.buy_case_funded(i, b.len() as u64 * price + 10 * KAS);
        c.bundle = b;
        ch.buy(i, &c, ops, 300_000 + 10_000 * n as u64);
    }
    let (cc, ops) = ch.close_case(900_000);
    ch.close(&cc, ops, TEST_DEADLINE as u64 + 10);
    ch.draw_round(seed);
}

/// Round 1 sold and drawn: genesis at `price` / `st`, the bundles built from the winning
/// numbers (`m` = 10), then close and draw with seed [1; 32].
fn round1(price: u64, st: [i64; 5], bundles: impl FnOnce(&[u8; 6]) -> Vec<(usize, Vec<Combo>)>) -> Chain {
    let mut ch = genesis_priced(price, st);
    let seed = [1u8; 32];
    let win = win6(&ch, &seed);
    let b = bundles(&win);
    sell_close_draw(&mut ch, seed, b);
    ch
}

const ST: [i64; 5] = [400, 300, 300, 0, 0];
const P100: u64 = 100 * KAS;

/// The tier combination (variant 0) and a non-winning one (`n`-th) at m = 10.
fn tc(win: &[u8; 6], tier: u8) -> Combo {
    combo_for_tier(win, tier, 10, 0)
}
fn nowin(win: &[u8; 6], n: usize) -> Combo {
    combo_matching(win, 0, false, 10, n)
}

/// Claim index `idx` of ticket `ti` against round 1's pool of `tier` at NOW.
fn claim_case(ch: &mut Chain, ti: usize, tier: u8, idx: i64) -> (ClaimCase, Vec<harness::TransactionOutpoint>) {
    let pi = ch.pool_of(1, tier);
    ch.claim_case(ti, pi, idx, NOW - 1_000)
}

/// Run and check a chain claim.
fn chain_claim(ch: &mut Chain, c: &ClaimCase, ops: Vec<harness::TransactionOutpoint>) -> ClaimExpected {
    let f = ch.claim(c, ops, NOW);
    check_claim(c, &f)
}

// ---------------------------------------------------------------- KP1

/// KP1: genesis (K = 4) -> buys in three shards -> close -> draw -> a PAY claim on a lower
/// tier (4 + bonus, h = 1). Every output equals `ClaimExpected`; conservation holds.
#[test]
fn kp1_pay_claim_lower_tier() {
    let mut ch = round1(P100, ST, |w| vec![(0, vec![tc(w, 2), nowin(w, 1)]), (1, vec![tc(w, 3)]), (2, vec![nowin(w, 2)])]);
    let (c, ops) = claim_case(&mut ch, 0, TIER_LOWER0, 0);
    let x = c.expected();
    assert!(x.rejects.is_empty(), "{:?}", x.rejects);
    assert_eq!((x.tier, x.h, x.mode), (Some(TIER_LOWER0), 1, ClaimMode::Pay));
    assert_eq!(x.prize, c.pool.slice);
    assert!(x.prize >= P_MIN);
    chain_claim(&mut ch, &c, ops);
}

// ---------------------------------------------------------------- KP2

/// KP2: one combination held 3 times (twice in one bundle in shard 0, once in shard 1):
/// h = 3 summed over K; three claims each pay floor(slice / 3); the pool drops by exactly
/// 3 x prize.
#[test]
fn kp2_duplicates_split_exactly() {
    let mut ch = round1(P100, ST, |w| vec![(0, vec![tc(w, 2), tc(w, 2)]), (1, vec![tc(w, 2)])]);
    let pi = ch.pool_of(1, TIER_LOWER0);
    let (pool0, rem0) = (ch.pools[pi].1.clone(), ch.pools[pi].2);
    let prize = pool0.slice / 3;
    // Ticket 1 (b = 1) first: FINAL, removed; then ticket 0's indices 0 and 1.
    for (ti, idx) in [(1, 0), (0, 0), (0, 1)] {
        let (c, ops) = claim_case(&mut ch, ti, TIER_LOWER0, idx);
        let x = c.expected();
        assert_eq!((x.h, x.prize), (3, prize), "h = 3 over K; prize = floor(slice / 3)");
        chain_claim(&mut ch, &c, ops);
    }
    let pi = ch.pool_of(1, TIER_LOWER0);
    assert_eq!(ch.pools[pi].2, rem0 - 3 * prize as u64, "pool drops by exactly 3 x prize");
}

// ---------------------------------------------------------------- KP3, KP4, KP5

/// KP3: jackpot claims (tier 0), floor(J / h_J), with h_J = 1 (a b = 2 ticket: PAY) and
/// h_J = 2 (two b = 1 tickets in two shards: FINAL each).
#[test]
fn kp3_jackpot_claims() {
    let mut ch = round1(KAS, ST, |w| vec![(0, vec![tc(w, TIER_JACKPOT), nowin(w, 1)])]);
    let (c, ops) = claim_case(&mut ch, 0, TIER_JACKPOT, 0);
    let x = c.expected();
    assert_eq!((x.h, x.prize, x.mode), (1, c.pool.slice, ClaimMode::Pay));
    chain_claim(&mut ch, &c, ops);

    let mut ch = round1(KAS, ST, |w| vec![(0, vec![tc(w, TIER_JACKPOT)]), (1, vec![tc(w, TIER_JACKPOT)])]);
    for _ in 0..2 {
        let (c, ops) = claim_case(&mut ch, 0, TIER_JACKPOT, 0);
        let x = c.expected();
        assert_eq!((x.h, x.prize, x.mode), (2, c.pool.slice / 2, ClaimMode::Final));
        chain_claim(&mut ch, &c, ops);
    }
}

/// KP4: second-prize claims (tier 1), floor(G2 / h) with h = 2 at one other bonus (n2 = 1:
/// the pool holds G2).
#[test]
fn kp4_second_prize_claims() {
    let mut ch = round1(KAS, ST, |w| vec![(0, vec![tc(w, TIER_SECOND)]), (2, vec![tc(w, TIER_SECOND)])]);
    let pi = ch.pool_of(1, TIER_SECOND);
    assert_eq!(ch.pools[pi].2 as i64, ch.pools[pi].1.slice, "n2 = 1: the pool holds G2");
    for _ in 0..2 {
        let (c, ops) = claim_case(&mut ch, 0, TIER_SECOND, 0);
        let x = c.expected();
        assert_eq!((x.h, x.prize), (2, c.pool.slice / 2));
        chain_claim(&mut ch, &c, ops);
    }
}

/// KP5: FINAL on a b = 1 ticket: payout = prize + d x b; no ticket output.
#[test]
fn kp5_final_single_combination() {
    let mut ch = round1(P100, ST, |w| vec![(0, vec![tc(w, 2)])]);
    let (c, ops) = claim_case(&mut ch, 0, TIER_LOWER0, 0);
    let x = c.expected();
    assert_eq!((x.mode, c.ticket.b, c.ticket_value), (ClaimMode::Final, 1, TEST_DEPOSIT));
    assert!(x.ticket.is_none());
    assert_eq!(x.values[1], x.prize as u64 + TEST_DEPOSIT);
    chain_claim(&mut ch, &c, ops);
    assert!(ch.tickets.is_empty(), "the ticket is consumed");
}

// ---------------------------------------------------------------- KP6, KP7

/// KP6: FINAL on a b = 2 ticket whose other index was claimed earlier by ACCRUE: payout =
/// prize + d b + the accrued amount.
#[test]
fn kp6_final_after_accrue() {
    let mut ch = round1(P100, ST, |w| vec![(0, vec![tc(w, 3), tc(w, 2)])]);
    let (c, ops) = claim_case(&mut ch, 0, TIER_LOWER0 + 1, 0);
    let x1 = c.expected();
    assert_eq!(x1.mode, ClaimMode::Accrue);
    assert!(x1.prize > 0 && x1.prize < P_MIN);
    chain_claim(&mut ch, &c, ops);
    let (c, ops) = claim_case(&mut ch, 0, TIER_LOWER0, 1);
    let x2 = c.expected();
    assert_eq!(x2.mode, ClaimMode::Final);
    assert_eq!(x2.values[1], x2.prize as u64 + 2 * TEST_DEPOSIT + x1.prize as u64);
    chain_claim(&mut ch, &c, ops);
}

/// KP7: ACCRUE (prize < P_MIN): ticket' value = value + prize; a later
/// `return_deposit_after` pays the whole value.
#[test]
fn kp7_accrue_then_return() {
    let mut ch = round1(P100, ST, |w| vec![(0, vec![tc(w, 3), nowin(w, 1)])]);
    let (c, ops) = claim_case(&mut ch, 0, TIER_LOWER0 + 1, 0);
    let x = c.expected();
    assert_eq!(x.mode, ClaimMode::Accrue);
    assert_eq!(x.values[1], 2 * TEST_DEPOSIT + x.prize as u64);
    chain_claim(&mut ch, &c, ops);
    let (r, ops) = ch.return_after_case(0, NOW);
    assert_eq!(r.ticket_value, 2 * TEST_DEPOSIT + x.prize as u64);
    let f = ch.return_deposit(&r, ops, r.lock_time + 10);
    check_return(&r, &f);
}

// ---------------------------------------------------------------- KP8, KP9 (fabricated, D63)

/// KP8: prize = P_MIN is PAY; prize = P_MIN - 1 is ACCRUE.
#[test]
fn kp8_p_min_boundary() {
    for (slice, mode) in [(P_MIN, ClaimMode::Pay), (P_MIN - 1, ClaimMode::Accrue)] {
        let c = ClaimCase::fabricated(4, 10, TIER_LOWER0, slice, &[1]);
        let x = c.expected();
        assert_eq!((x.prize, x.mode), (slice, mode));
        run(&c);
    }
}

/// KP9: zero prize (slice < h): ACCRUE of 0; the pool value is unchanged; the bit is set.
#[test]
fn kp9_zero_prize() {
    let c = ClaimCase::fabricated(4, 10, TIER_LOWER0, 1, &[1, 1]);
    let x = c.expected();
    assert_eq!((x.h, x.prize, x.mode), (2, 0, ClaimMode::Accrue));
    assert_eq!(x.values[0], c.pool_value);
    assert_eq!(x.values[1], c.ticket_value);
    assert_eq!(x.ticket.as_ref().unwrap().claimed[0], 1);
    run(&c);
}

// ---------------------------------------------------------------- KP10, KP11

/// KP10: one claim per tier 0..6 (all five lower tiers funded, `ST_ALL`), each on a clone
/// of the drawn chain.
#[test]
fn kp10_one_claim_per_tier() {
    let ch = round1(P100, ST_ALL, |w| vec![(0, (0..4).map(|t| tc(w, t)).collect()), (1, (4..7).map(|t| tc(w, t)).collect())]);
    let tiers: Vec<u8> = ch.pools.iter().map(|p| p.1.tier).collect();
    assert_eq!(tiers, (0..=6).collect::<Vec<u8>>(), "all seven pools exist");
    for tier in 0..=6u8 {
        let mut ch = ch.clone();
        let (ti, idx) = if tier < 4 { (0, tier as i64) } else { (1, tier as i64 - 4) };
        let (c, ops) = claim_case(&mut ch, ti, tier, idx);
        let x = c.expected();
        assert!(x.rejects.is_empty(), "tier {tier}: {:?}", x.rejects);
        assert_eq!((x.tier, x.h), (Some(tier), 1));
        chain_claim(&mut ch, &c, ops);
    }
}

/// KP11: empty leaves (Z_0) in shards without the combination, every one of the K shards
/// non-empty.
#[test]
fn kp11_empty_leaves() {
    let mut ch = round1(KAS, ST, |w| vec![(0, vec![tc(w, 2)]), (1, vec![nowin(w, 1)]), (2, vec![nowin(w, 2)]), (3, vec![nowin(w, 3)])]);
    let (c, ops) = claim_case(&mut ch, 0, TIER_LOWER0, 0);
    let x = c.expected();
    let z0 = empty_hashes()[0];
    for (i, counts) in x.counts.iter().enumerate() {
        assert_ne!(c.trees[i].root(), lottery::close::z18(), "shard {i} non-empty");
        if i > 0 {
            assert!(counts.iter().all(|&n| n == 0), "shard {i}: no holding at the rank");
            assert_eq!(leaf_hash(x.rank, counts), z0, "shard {i}: Z_0 leaf");
        }
    }
    assert_eq!(x.h, 1);
    chain_claim(&mut ch, &c, ops);
}

// ---------------------------------------------------------------- KP12

/// KP12: a pool drained by all its holders (two b = 1 tickets, h = 2), then round 2's
/// `draw()` absorbs it at its deadline: the remainder goes to O', R to the funder.
#[test]
fn kp12_drained_pool_absorbed() {
    let mut ch = round1(P100, ST, |w| vec![(0, vec![tc(w, 2)]), (1, vec![tc(w, 2)])]);
    let rem_before = ch.pools[ch.pool_of(1, TIER_LOWER0)].2;
    for _ in 0..2 {
        let (c, ops) = claim_case(&mut ch, 0, TIER_LOWER0, 0);
        assert_eq!(c.expected().h, 2);
        chain_claim(&mut ch, &c, ops);
    }
    let pi = ch.pool_of(1, TIER_LOWER0);
    let (pool, rem) = (ch.pools[pi].1.clone(), ch.pools[pi].2);
    assert_eq!(rem, rem_before - 2 * (pool.slice / 2) as u64, "drained by both holders");
    // Round 2: one buy, close at T2, draw absorbing the drained pool at its deadline.
    let t2 = ch.controller.1.deadline as u64;
    let (mut b, ops) = ch.buy_case_funded(3, P100 + 10 * KAS);
    // Stage 3a fixture fix: the default bundle has bonus 4, but round 2's m comes from the
    // game-size rule and is 2 here; bonus 1 is valid at every m >= 2.
    assert!(ch.shards[3].1.m >= 2);
    b.bundle = vec![Combo::new([3, 7, 12, 19, 28], 1)];
    ch.buy(3, &b, ops, t2 - 1_000);
    let (mut c2, ops) = ch.close_case(t2 - 500);
    c2.lock_time = t2;
    ch.close(&c2, ops, t2 + 10);
    let deadline = pool.deadline as u64;
    let (d, ops) = ch.draw_case([2u8; 32], &[pi], deadline);
    let x = d.expected();
    assert_eq!(x.q, vec![rem as i64], "Q = the drained pool's remainder");
    let unpaid: i64 = (0..5).filter(|&t| !x.pools.lp[t]).map(|t| x.amounts.b[t]).sum();
    let jb = if x.pools.jp { 0 } else { d.marker.j_boost };
    assert_eq!(x.o_prime, d.pot as i64 + x.amounts.b_r + jb + unpaid + rem as i64, "the remainder goes to O'");
    let f = ch.draw(&d, ops, deadline + 10);
    assert_eq!(f.tx.outputs[x.pool_refund(0)].value, d.reserve(), "R to the funder");
    assert!(ch.pools.iter().all(|p| p.1.round != 1 || p.1.tier != TIER_LOWER0), "the drained pool is gone");
}

// ---------------------------------------------------------------- KP13, KP14

/// KP13: `return_deposit_after` at lock_time = T + D_RET exactly.
#[test]
fn kp13_return_after_at_t_plus_d_ret() {
    let mut ch = round1(KAS, ST, |w| vec![(0, vec![nowin(w, 1), nowin(w, 2)])]);
    let (r, ops) = ch.return_after_case(0, NOW);
    assert_eq!(r.lock_time as i64, r.ticket.deadline + D_RET);
    assert_eq!(r.ticket_value, 2 * TEST_DEPOSIT);
    let f = ch.return_deposit(&r, ops, r.lock_time);
    check_return(&r, &f);
    assert!(ch.tickets.is_empty());
}

/// KP14: `return_deposit_with_pool` at lock_time = pool.deadline: pool' byte-identical with
/// an equal value.
#[test]
fn kp14_return_with_pool_at_deadline() {
    let mut ch = round1(KAS, ST, |w| vec![(0, vec![nowin(w, 1)])]);
    let pi = ch.pool_of(1, TIER_LOWER0);
    let (r, ops) = ch.return_with_pool_case(0, pi, NOW);
    assert_eq!(r.lock_time as i64, ch.pools[pi].1.deadline);
    let f = ch.return_deposit(&r, ops, r.lock_time);
    check_return(&r, &f);
}

// ---------------------------------------------------------------- KP15

/// KP15: `refresh()` of a marker, a ticket and two pools at creation + D_IDLE (byte-identical
/// successors, same values); afterwards a `draw()` on the refreshed marker, a claim on the
/// refreshed ticket and pool, and round 2's absorption of the other refreshed pool are all
/// accepted.
#[test]
fn kp15_refresh_then_use() {
    let mut ch = genesis_priced(KAS, ST);
    let seed = [1u8; 32];
    let win = win6(&ch, &seed);
    let (mut b, ops) = ch.buy_case(0);
    b.bundle = vec![tc(&win, 2), nowin(&win, 1)];
    ch.buy(0, &b, ops, 300_000);
    let (cc, ops) = ch.close_case(900_000);
    ch.close(&cc, ops, TEST_DEADLINE as u64 + 10);
    // Marker refresh, then the draw spends the refreshed marker.
    let (r, ops) = ch.refresh_marker_case(ch.marker_daa + 1);
    assert_eq!(r.lock_time, r.daa + D_IDLE as u64);
    let f = ch.refresh(&r, ops, r.lock_time + 10);
    check_refresh(&r, &f);
    ch.draw_round(seed);
    // Ticket and pool refreshes, then a claim on both.
    let pa = ch.pool_of(1, TIER_LOWER0);
    let (r, ops) = ch.refresh_pool_case(pa, NOW);
    let f = ch.refresh(&r, ops, r.lock_time + 10);
    check_refresh(&r, &f);
    let (r, ops) = ch.refresh_ticket_case(0, NOW);
    let f = ch.refresh(&r, ops, r.lock_time + 10);
    check_refresh(&r, &f);
    let pb = ch.pool_of(1, TIER_LOWER0 + 1);
    let (r, ops) = ch.refresh_pool_case(pb, NOW);
    let f = ch.refresh(&r, ops, r.lock_time + 10);
    check_refresh(&r, &f);
    let later = r.lock_time + 1_000;
    let (c, ops) = ch.claim_case(0, ch.pool_of(1, TIER_LOWER0), 0, later);
    let f = ch.claim(&c, ops, later + 10);
    check_claim(&c, &f);
    // Round 2 absorbs the other refreshed pool.
    let t2 = ch.controller.1.deadline as u64;
    let (b, ops) = ch.buy_case(3);
    ch.buy(3, &b, ops, t2 - 1_000);
    let (mut c2, ops) = ch.close_case(t2 - 500);
    c2.lock_time = t2;
    ch.close(&c2, ops, t2 + 10);
    let pb = ch.pool_of(1, TIER_LOWER0 + 1);
    let deadline = ch.pools[pb].1.deadline as u64;
    let (d, ops) = ch.draw_case([2u8; 32], &[pb], deadline);
    ch.draw(&d, ops, deadline + 10);
}

/// KP15 (standalone half): `refresh()` of `obj` fabricated under the instance ID (D63) at
/// creation + D_IDLE exactly, with and without change.
fn refresh_standalone(name: &str, obj: RefreshObj, value: u64) {
    for change in [true, false] {
        let mut r = RefreshCase::new(obj.clone(), value, 1_500_000);
        assert_eq!(r.lock_time, 1_500_000 + D_IDLE as u64);
        if !change {
            r.change = false;
            r.funding[0].value = r.fee;
        }
        let f = accept_m4(name, &r.build());
        check_refresh(&r, &f);
    }
}

#[test]
fn kp15_refresh_ticket_standalone() {
    let c = ClaimCase::new(4);
    refresh_standalone("refresh (standalone ticket)", RefreshObj::Ticket(c.ticket), c.ticket_value);
}

#[test]
fn kp15_refresh_pool_standalone() {
    let c = ClaimCase::new(4);
    refresh_standalone("refresh (standalone pool)", RefreshObj::Pool(c.pool), c.pool_value);
}

#[test]
fn kp15_refresh_marker_standalone() {
    let marker = lottery::close::CloseCase::new(4).expected().marker;
    refresh_standalone("refresh (standalone marker)", RefreshObj::Marker(marker), 50 * KAS);
}

// ---------------------------------------------------------------- KP16, KP17

/// KP16: with and without the change output; F_MAX funding inputs.
#[test]
fn kp16_change_and_f_max_funding() {
    let mut c = ClaimCase::new(4);
    c.change = false;
    c.funding = vec![FundingIn { value: c.fee, daa: 2_100_000 }];
    let x = c.expected();
    assert_eq!(x.values.len(), 3, "PAY without change: pool', ticket', payout");
    run(&c);
    let mut c = ClaimCase::new(4);
    c.funding = vec![FundingIn { value: KAS, daa: 2_100_000 }; 8];
    let (_, f) = run(&c);
    assert_eq!(f.tx.inputs.len(), 2 + 8);
    // Returns and refreshes without change too.
    let mut r = ReturnCase::after();
    r.change = false;
    r.funding = vec![FundingIn { value: r.fee, daa: 2_100_000 }];
    let f = accept_m4("return_deposit (standalone)", &r.build());
    check_return(&r, &f);
}

/// KP17: K = 16, m = M_CLAIM, worst proofs (every one of the 16 leaves non-empty, h = 16),
/// each mode.
#[test]
fn kp17_k16_m50_each_mode() {
    let holders = [1u16; K_MAX];
    for mode in [ClaimMode::Pay, ClaimMode::Accrue, ClaimMode::Final] {
        let slice = if mode == ClaimMode::Accrue { 16 * (P_MIN - 1) } else { 16 * 2 * P_MIN };
        let mut c = ClaimCase::fabricated(K_MAX, M_CLAIM, TIER_LOWER0, slice, &holders);
        if mode == ClaimMode::Final {
            c.make_final();
        }
        let x = c.expected();
        assert_eq!((x.h, x.mode), (16, mode));
        assert_eq!(x.leaves.len(), K_MAX * 2 * M_CLAIM as usize);
        assert_eq!(x.siblings.len(), K_MAX * PATH_BYTES);
        assert!(x.counts.iter().all(|cs| cs.iter().any(|&n| n > 0)), "every leaf non-empty");
        run(&c);
    }
}

// ---------------------------------------------------------------- KP18 (script half)

/// KP18: the script accepts the reference outputs over a grid: every tier 0..6 x every
/// mode (standalone, K = 4, m = 10, h = 1).
#[test]
fn kp18_script_accepts_reference_grid() {
    for tier in 0..=6u8 {
        for mode in [ClaimMode::Final, ClaimMode::Pay, ClaimMode::Accrue] {
            let slice = if mode == ClaimMode::Accrue { P_MIN / 2 } else { 2 * P_MIN };
            let mut c = ClaimCase::fabricated(4, 10, tier, slice, &[1]);
            if mode == ClaimMode::Final {
                c.make_final();
            }
            let x = c.expected();
            assert_eq!((x.tier, x.mode), (Some(tier), mode));
            run(&c);
        }
    }
}

// ---------------------------------------------------------------- KP19

/// KP19 (integration variant; the unit test is `genesis::tests::kp19_genesis_ticket_tpl_and_claim_floor`):
/// the deployed drawer carries ticket_tpl = the compiled ticket hash, and the first draw
/// writes it into every pool; d < D_FLOOR (M6 D81, was D_CLAIM_FLOOR) and a wrong
/// ticket_tpl are refused.
#[test]
fn kp19_genesis_ticket_tpl() {
    let t = tpl();
    let ch = round1(KAS, ST, |w| vec![(0, vec![nowin(w, 1)])]);
    assert_eq!(ch.drawer.1.ticket_tpl, t.ticket.hash);
    assert!(!ch.pools.is_empty());
    for p in &ch.pools {
        assert_eq!(p.1.ticket_tpl, t.ticket.hash, "draw() writes ticket_tpl into every pool");
        assert_eq!(p.1.reserve, ch.controller.1.reserve, "and reserve = R");
    }
    let mut g = GenesisCase::new(4, vec![]);
    g.controller.d = D_FLOOR - 1;
    for s in &mut g.shards {
        s.d = D_FLOOR - 1;
    }
    assert!(g.validate().unwrap_err().contains("D_FLOOR"));
    let mut g = GenesisCase::new(4, vec![]);
    g.drawer.ticket_tpl = t.pool.hash;
    assert!(g.validate().unwrap_err().contains("ticket_tpl"));
}

// ---------------------------------------------------------------- M4 review U3 (S1)

/// M4 review S1, user decision U3: the reviewer's r1 instances are refused at genesis.
/// `r1_return_after_before_claim_deadline` (claim_window = 14 days) let a third party forfeit
/// a winning jackpot ticket with `return_deposit_after` (T + D_RET) before its pool's claim
/// deadline, with an on-time close and draw. claim_window = 0 and draw_window = 7 days are
/// the reviewer's other repros. Each is refused by `validate()` and by the checked builder.
#[test]
fn u3_r1_windows_refused_at_genesis() {
    use lottery::game::{CLAIM_WINDOW, DRAW_WINDOW};
    let edits: [(&str, fn(&mut GenesisCase), &str); 3] = [
        ("claim_window = 14 days", |g| g.drawer.claim_window = 2 * CLAIM_WINDOW, "D_RET"),
        ("claim_window = 0", |g| g.drawer.claim_window = 0, "claim_window"),
        ("draw_window = 7 days", |g| g.drawer.draw_window = CLAIM_WINDOW, "D_RET"),
    ];
    let mut accepted = vec![];
    for (name, edit, needle) in edits {
        let mut g = GenesisCase::new(4, vec![]);
        edit(&mut g);
        match g.validate() {
            Ok(()) => accepted.push(name),
            Err(e) => assert!(e.contains(needle), "{name}: refused for another reason: {e}"),
        }
        // The chain fixture builds through the checked builder (`build_with`).
        let r = std::panic::catch_unwind(|| Chain::genesis_with(4, edit, true));
        if r.is_ok() && !accepted.contains(&name) {
            accepted.push(name);
        }
    }
    assert!(accepted.is_empty(), "genesis accepted: {accepted:?}");
    let _ = DRAW_WINDOW;
}

/// U3 positive: on the default genesis (seed_offset + draw_window + claim_window <= D_RET),
/// the r1 sequence (on-time close at T + 10, on-time draw) gives the jackpot pool a claim
/// deadline strictly before the earliest `return_deposit_after` lock_time T + D_RET, so path
/// b cannot forfeit the winning ticket while its pool is claimable.
#[test]
fn u3_default_windows_claim_deadline_before_return_after() {
    let mut ch = Chain::genesis(4);
    let seed = [1u8; 32];
    let win = win6(&ch, &seed);
    let (mut b, ops) = ch.buy_case(0);
    b.bundle = vec![tc(&win, TIER_JACKPOT), nowin(&win, 1)];
    ch.buy(0, &b, ops, 300_000);
    let (cc, ops) = ch.close_case(900_000);
    ch.close(&cc, ops, TEST_DEADLINE as u64 + 10);
    let seed_ref = ch.marker.as_ref().unwrap().1.seed_ref;
    let (c, ops) = ch.draw_case(seed, &[], (seed_ref + 10) as u64);
    let lt = c.lock_time;
    ch.draw(&c, ops, lt + 10);
    let pi = ch.pool_of(1, TIER_JACKPOT);
    let deadline = ch.pools[pi].1.deadline;
    let (r, _) = ch.return_after_case(0, 2_000_000);
    assert_eq!(r.lock_time as i64, TEST_DEADLINE + D_RET);
    assert!(deadline < r.lock_time as i64, "pool deadline {deadline} >= return_after lock_time {}", r.lock_time);
    assert!(deadline - (lt as i64) >= lottery::game::CLAIM_WINDOW, "at least 7 days to claim after an on-time draw");
}

/// PARKED (M4 review S2, user decision U4; the missed-draw item, release gate): this test
/// DOCUMENTS current behaviour, it does not endorse it. A late `draw()` whose caller funds
/// it with old inputs (drawer check 19: D_claim = max(own, seed_ref + draw_window) +
/// claim_window, own = the latest input creation score) creates pools whose claim deadline
/// has already passed, so a third party can forfeit a winning ticket through
/// `return_deposit_with_pool` one DAA after the draw. Design 4.5's "at least 7 days" holds
/// only for a draw within draw_window. When the missed-draw item is resolved, this test
/// must fail and be replaced by the attack test for the fix.
#[test]
fn parked_u4_late_draw_creates_already_expired_pools() {
    let mut ch = Chain::genesis(4);
    let seed = [1u8; 32];
    let win = win6(&ch, &seed);
    let (mut b, ops) = ch.buy_case(0);
    b.bundle = vec![tc(&win, TIER_JACKPOT), nowin(&win, 1)];
    ch.buy(0, &b, ops, 300_000);
    let (cc, ops) = ch.close_case(900_000);
    ch.close(&cc, ops, TEST_DEADLINE as u64 + 10);
    let (mut c, ops) = ch.draw_case(seed, &[], 1_000);
    let late = 8_000_000u64;
    c.lock_time = late;
    ch.draw(&c, ops, late);
    let pi = ch.pool_of(1, TIER_JACKPOT);
    let deadline = ch.pools[pi].1.deadline;
    assert!(deadline <= late as i64, "pool deadline {deadline} is not past at the late draw {late}");
    let (r, ops) = ch.return_with_pool_case(0, pi, late);
    ch.return_deposit(&r, ops, late + 1);
}
