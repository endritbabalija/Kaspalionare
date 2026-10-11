//! M3 positive tests for `draw()` (spec M3 A8: DP1-DP16, DP19, DP20, DP22, and the script
//! halves of DP17 and DP18; DP17/DP18 Rust halves are in m3_game.rs, DP21 in m3_measure.rs).
//!
//! Every transaction runs every input in the v2.1.0 engine under committed budgets
//! (`accept_draw`: the legs and funding inputs first, then the drawer at input 0). Every
//! positive test also checks every output against `DrawExpected` and value conservation
//! (A11 item 4) through `check_draw`.
//!
//! Each test first asserts, on the Rust side, that its fixture really exercises the case it
//! names (e.g. y < O' for DP9), so a fixture that drifts fails before any script runs.

mod common;

use common::m2::*;
use harness::CovenantBinding;
use lottery::buy::{Final, KAS};
use lottery::close::FundingIn;
use lottery::combo::{Combo, unrank};
use lottery::draw::{DrawCase, DrawExpected, RoundResult};
use lottery::game::{A_MAX, CLAIM_WINDOW, DRAW_WINDOW, F_MIN, F_MIN_FLOOR, TOTAL_MAX, muldiv, witness_ok};
use lottery::smt::{Tree, empty_hashes};
use lottery::state::{Payout, TIER_JACKPOT, TIER_LOWER0, TIER_SECOND};

/// Every output (count, value, script, covenant binding) as `DrawExpected` says, the caller
/// output by the A4 formula, inputs - outputs = fee, and the economic conservation of A4.
fn check_draw(c: &DrawCase, f: &Final) -> DrawExpected {
    let x = c.expected();
    let t = tpl();
    let n = x.n_outputs();
    assert_eq!(f.tx.outputs.len(), n, "A4: 3 + np + paid + 1 + a + 1 outputs");
    for (i, (o, v)) in f.tx.outputs.iter().zip(&x.values).enumerate() {
        assert_eq!(o.value, *v, "output {i} value");
    }
    assert_eq!(f.tx.outputs[0].script_public_key, t.drawer_spk(&x.drawer));
    assert_eq!(f.tx.outputs[1].script_public_key, t.vault_spk(&x.vault));
    assert_eq!(f.tx.outputs[2].script_public_key, t.controller_spk(&x.controller));
    for (p, s) in x.new_pools.iter().enumerate() {
        assert_eq!(f.tx.outputs[x.pool_output(p)].script_public_key, t.pool_spk(s), "pool {p}");
    }
    let cov_outs = 3 + x.np();
    for (i, o) in f.tx.outputs.iter().enumerate() {
        if i < cov_outs {
            assert_eq!(o.covenant, Some(CovenantBinding::new(0, c.cov)), "output {i} authorized by input 0");
        } else {
            assert!(o.covenant.is_none(), "output {i} has the zero ID");
        }
    }
    if let Some(i) = x.fee_output() {
        let fee = Payout::from_fields(c.controller.fee_class, &c.controller.fee_key);
        assert_eq!(f.tx.outputs[i].script_public_key, fee.spk(), "D41 fee address of the round r+1 controller");
    }
    let mk = Payout::from_fields(c.marker.funder_class, &c.marker.funder_key);
    assert_eq!(f.tx.outputs[x.marker_refund()].script_public_key, mk.spk(), "marker refund");
    for (i, p) in c.absorbed.iter().enumerate() {
        let fu = Payout::from_fields(p.state.funder_class, &p.state.funder_key);
        assert_eq!(f.tx.outputs[x.pool_refund(i)].script_public_key, fu.spk(), "pool refund {i}");
    }
    // Caller output = funding - np R + C_draw - fee (A4).
    assert_eq!(f.tx.outputs[x.caller_output()].value as i128, c.caller_formula(&x), "caller output");
    assert_eq!(f.fee(), c.fee, "inputs - outputs = fee");
    // Economic conservation (A4, excluding R).
    let mk = &c.marker;
    let lhs = (mk.sales - mk.ref_paid - x.amounts.c_close + mk.j_boost) as i128
        + c.vault.total_assets as i128
        + (c.pot as i128 + c.controller.acc as i128)
        + x.q.iter().map(|&q| q as i128).sum::<i128>();
    let pools: i128 = x.pools.pools.iter().map(|p| p.value as i128).sum();
    let rhs = pools + x.v_prime as i128 + (x.o_prime + x.fee.a_next) as i128 + if x.fee.paid { x.fee.t_f as i128 } else { 0 } + x.amounts.c_draw as i128;
    assert_eq!(lhs, rhs, "A4 conservation");
    x
}

/// Run a standalone draw case on the committed templates.
fn run(c: &DrawCase) -> (DrawExpected, Final) {
    let f = accept_draw(&c.build());
    let x = check_draw(c, &f);
    (x, f)
}

/// A standalone round 1 draw with sales of 100 KAS (5 KAS referral paid).
fn sold() -> DrawCase {
    let mut c = DrawCase::new(4);
    c.set_sales(100 * KAS as i64, 5 * KAS as i64);
    c
}

fn tiers(x: &DrawExpected) -> Vec<u8> {
    x.pools.pools.iter().map(|p| p.tier).collect()
}

// ---------------------------------------------------------------- DP1

/// DP1: genesis (K = 4) -> buys in shards 0 and 2 (a jackpot ticket, a second-prize ticket
/// and other combinations) -> close -> draw. Every output equals `DrawExpected`
/// (`check_draw`); the key fields are also written out here; the round result rebuilds
/// every pool (R4).
#[test]
fn dp1_genesis_buys_close_draw() {
    let mut ch = Chain::genesis(4);
    let seed = [1u8; 32];
    let w = ch.winning(&seed);
    let second = if w.bonus == 1 { 2 } else { 1 };
    let (mut b0, ops0) = ch.buy_case(0);
    b0.with_ref = true;
    b0.bundle = vec![Combo::new(w.main, w.bonus)];
    ch.buy(0, &b0, ops0, 300_000);
    let (mut b2, ops2) = ch.buy_case(2);
    let mut bundle = common::distinct(3, 10);
    bundle.retain(|c| c.main != w.main);
    bundle.push(Combo::new(w.main, second));
    b2.bundle = bundle;
    ch.buy(2, &b2, ops2, 400_000);
    let (cc, ops) = ch.close_case(900_000);
    ch.close(&cc, ops, lottery::close::TEST_DEADLINE as u64 + 10);
    let mk = ch.marker.clone().unwrap().1;
    let fund_daa = (mk.seed_ref + DRAW_WINDOW + 1_000) as u64;
    let (c, ops) = ch.draw_case(seed, &[], fund_daa);
    let x = c.expected();
    let vault0 = c.vault.clone();
    let ctrl0 = c.controller.clone();

    // Rust-side facts of the fixture.
    assert_eq!((x.rank, x.bonus), (w.rank, w.bonus));
    assert_eq!((x.h_j, x.n2), (1, 1));
    assert_eq!(tiers(&x), vec![TIER_JACKPOT, TIER_SECOND, TIER_LOWER0, TIER_LOWER0 + 1, TIER_LOWER0 + 2]);
    assert!(x.fee.paid);

    let f = ch.draw(&c, ops, c.lock_time + 10);
    check_draw(&c, &f);
    let t = tpl();
    let r = ctrl0.reserve as u64;
    // Vault': V' = V + H - J_house - n2 G2 (5.4), round r, liability L'.
    let j_house = mk.jackpot - mk.j_boost;
    let v_prime = vault0.total_assets + x.amounts.h - j_house - mk.g2;
    assert_eq!(x.vault.total_assets, v_prime);
    assert_eq!(x.vault.round, 1);
    assert_eq!(f.tx.outputs[1].value, v_prime as u64 + r);
    // Controller': drawn, v_rec = V', J' = x2 + y, L' = x2 + G2' (m' - 1).
    assert_eq!(x.controller.drawn, 1);
    assert_eq!(x.controller.v_rec, v_prime);
    assert_eq!(x.controller.jackpot, x.fix.x2 + x.fix.y);
    assert_eq!(x.controller.g2, muldiv(x.controller.jackpot, ctrl0.g));
    assert_eq!(x.controller.liability, x.fix.x2 + x.controller.g2 * (ctrl0.m - 1));
    assert_eq!(x.vault.liability, x.controller.liability);
    assert!(witness_ok(&x.rule, x.v_prime, x.o_prime, ctrl0.j, x.fix.x1, x.fix.y, x.fix.x2));
    // Drawer' round 2.
    assert_eq!(x.drawer.round, 2);
    // Pools: J, G2 x 1, B_1..3 (+ R each); win = main || bonus; deadline = own + CLAIM_WINDOW.
    assert_eq!(f.tx.outputs[3].value, mk.jackpot as u64 + r);
    assert_eq!(f.tx.outputs[4].value, mk.g2 as u64 + r);
    for p in &x.new_pools {
        assert_eq!(p.win, [w.main[0], w.main[1], w.main[2], w.main[3], w.main[4], w.bonus]);
        assert_eq!(p.deadline, fund_daa as i64 + CLAIM_WINDOW);
        assert_eq!(p.roots, mk.roots);
        // Spec M4 A7 check 25 (D54, D55).
        assert_eq!(p.ticket_tpl, t.ticket.hash);
        assert_eq!(p.reserve, r as i64);
    }
    // R4: the round result alone rebuilds every pool.
    let rr = RoundResult::new(&c, &x, f.tx.id());
    // Spec M4 A8: ticket_tpl from the drawer input, R from output 0.
    assert_eq!(rr.ticket_tpl, t.ticket.hash);
    assert_eq!(rr.reserve, r);
    // A9: the record derived from the draw() transaction bytes alone equals the builder's.
    assert_eq!(RoundResult::from_tx(&f.tx, &t), Ok(rr.clone()));
    for p in 0..x.np() {
        assert_eq!(t.pool_spk(&rr.pool_state(p)), f.tx.outputs[3 + p].script_public_key);
        assert_eq!(rr.pools[p].value, f.tx.outputs[3 + p].value);
    }
    assert_eq!(ch.pools.len(), 5);
    assert!(ch.marker.is_none());
}

// ---------------------------------------------------------------- DP2-DP4

/// DP2: jackpot held once: a jackpot pool of J; the vault pays J_house.
#[test]
fn dp2_jackpot_held_once() {
    let mut c = sold();
    c.hold_jackpot(1, 1);
    let x = c.expected();
    assert_eq!((x.h_j, x.n2), (1, 0));
    assert!(x.pools.jp && !x.pools.sp);
    assert_eq!(x.new_pools[0].tier, TIER_JACKPOT);
    assert_eq!(x.new_pools[0].slice, c.marker.jackpot);
    assert_eq!(x.v_prime, c.vault.total_assets + x.amounts.h - x.j_house);
    run(&c);
}

/// DP3: jackpot held 3 times in 2 shards: h_J = 3 summed over K; still one pool of J.
#[test]
fn dp3_jackpot_held_three_times_in_two_shards() {
    let mut c = sold();
    c.hold_jackpot(0, 2);
    c.hold_jackpot(3, 1);
    let x = c.expected();
    assert_eq!(x.h_j, 3);
    assert_eq!(x.new_pools.iter().filter(|p| p.tier == TIER_JACKPOT).count(), 1);
    assert_eq!(x.values[3], c.marker.jackpot as u64 + c.reserve());
    run(&c);
}

/// DP4: jackpot not held: no jackpot pool; J_house stays in the vault; J_boost goes to O'.
#[test]
fn dp4_jackpot_not_held() {
    let c = sold();
    let x = c.expected();
    assert_eq!(x.h_j, 0);
    assert!(!x.pools.jp);
    assert!(x.new_pools.iter().all(|p| p.tier != TIER_JACKPOT));
    assert_eq!(x.v_prime, c.vault.total_assets + x.amounts.h);
    let lower_unpaid: i64 = (0..5).filter(|&t| !x.pools.lp[t]).map(|t| x.amounts.b[t]).sum();
    assert_eq!(x.o_prime, c.pot as i64 + x.amounts.b_r + c.marker.j_boost + lower_unpaid);
    run(&c);
}

// ---------------------------------------------------------------- DP5-DP7

/// DP5: n2 = 0 (G2 > 0, no second-prize holding; another rank is held): no second pool.
#[test]
fn dp5_n2_zero() {
    let mut c = sold();
    let other = (c.seed().rank + 1) % lottery::combo::RANKS;
    c.hold_at(2, other, 3, 1);
    let x = c.expected();
    assert_eq!(x.n2, 0);
    assert!(!x.pools.sp && c.marker.g2 > 0);
    run(&c);
}

/// DP6: n2 = 1 (one other bonus held 3 times in one shard): pool of 1 x G2.
#[test]
fn dp6_n2_one() {
    let mut c = sold();
    let b = c.other_bonus(0);
    c.hold(2, b, 3);
    let x = c.expected();
    assert_eq!(x.n2, 1);
    let p = x.new_pools.iter().position(|p| p.tier == TIER_SECOND).unwrap();
    assert_eq!(x.values[3 + p], c.marker.g2 as u64 + c.reserve());
    assert_eq!(x.v_prime, c.vault.total_assets + x.amounts.h - c.marker.g2);
    run(&c);
}

/// DP7: n2 = m - 1: every second-prize combination held (spread over the shards).
#[test]
fn dp7_n2_m_minus_one() {
    let mut c = sold();
    for n in 0..(c.marker.m - 1) as u8 {
        let b = c.other_bonus(n);
        c.hold(n as usize % 4, b, 1);
    }
    let x = c.expected();
    assert_eq!(x.n2, c.marker.m - 1);
    let p = x.new_pools.iter().position(|p| p.tier == TIER_SECOND).unwrap();
    assert_eq!(x.values[3 + p], (9 * c.marker.g2) as u64 + c.reserve());
    run(&c);
}

// ---------------------------------------------------------------- DP8-DP11

/// DP8: boost pot below the safe level: y = O'. (With a large vault x1 sits on the edge
/// rule, which y also loads through G2 = muldiv(x + y, g), so y would be ~0; a 1,000,000 KAS
/// vault leaves room.)
#[test]
fn dp8_boost_below_safe_level() {
    let mut c = sold();
    c.vault.total_assets = 1_000_000 * KAS as i64;
    c.pot = 1_000 * KAS;
    let x = c.expected();
    assert_eq!(x.fix.y, x.o_prime);
    assert!(x.rule.pass(x.fix.x1, x.fix.y));
    run(&c);
}

/// DP9: boost pot above the safe level: y < O' (hold-back).
#[test]
fn dp9_boost_above_safe_level() {
    let mut c = sold();
    c.pot = 500_000 * KAS;
    let x = c.expected();
    assert!(x.fix.y < x.o_prime, "y = {} O' = {}", x.fix.y, x.o_prime);
    assert!(!x.rule.pass(x.fix.x1, x.fix.y + 1));
    run(&c);
}

/// DP10: the base x_b = muldiv(V', j) fails (here the edge rule): x1 < x_b.
#[test]
fn dp10_base_fails() {
    let c = sold();
    let x = c.expected();
    assert!(!x.rule.pass(x.fix.x_b, 0));
    assert!(x.fix.x1 < x.fix.x_b);
    run(&c);
}

/// DP11: top-up: the base passes with room left, x2 > x1.
#[test]
fn dp11_top_up() {
    let mut c = sold();
    c.vault.total_assets = 1_000_000 * KAS as i64;
    let x = c.expected();
    assert!(x.rule.pass(x.fix.x_b, 0));
    assert_eq!(x.fix.x1, x.fix.x_b);
    assert!(x.fix.x2 > x.fix.x1, "x1 = {} x2 = {}", x.fix.x1, x.fix.x2);
    run(&c);
}

// ---------------------------------------------------------------- DP12, DP13

/// DP12: fee below F_MIN: carried, A' = A + F, no fee output.
#[test]
fn dp12_fee_carried() {
    let mut c = DrawCase::new(4);
    c.set_sales(10 * KAS as i64, 0);
    c.controller.acc = 0;
    let x = c.expected();
    assert!(x.fee.t_f < F_MIN && !x.fee.paid);
    assert_eq!(x.controller.acc, x.amounts.f);
    assert!(x.fee_output().is_none());
    run(&c);
}

/// DP13: fee exactly at F_MIN and above it: one output of A + F, A' = 0.
#[test]
fn dp13_fee_paid_at_and_above_f_min() {
    let mut c = DrawCase::new(4);
    c.set_sales(10 * KAS as i64, 0);
    let f = c.expected().amounts.f;
    c.controller.acc = F_MIN - f;
    let x = c.expected();
    assert_eq!(x.fee.t_f, F_MIN);
    assert!(x.fee.paid && x.controller.acc == 0);
    assert_eq!(x.values[x.fee_output().unwrap()], F_MIN as u64);
    run(&c);
    let mut c = sold();
    c.controller.acc = 7 * KAS as i64;
    let x = c.expected();
    assert!(x.fee.t_f > F_MIN && x.fee.paid);
    run(&c);
}

// ---------------------------------------------------------------- DP14, DP16 (chains)

/// Round 1 of a chain that produces all five pools (jackpot, second prize, three lower
/// tiers): genesis -> buys -> close -> draw. Returns the chain after the round 2 close.
fn two_round_chain(seed1: [u8; 32]) -> Chain {
    two_round_chain_with(seed1, None, 5)
}

/// `two_round_chain` with the controller's lower-tier shares `st` (fixture's when `None`);
/// round 1's draw creates `np` pools.
fn two_round_chain_with(seed1: [u8; 32], st: Option<[i64; 5]>, np: usize) -> Chain {
    let mut ch = match st {
        None => Chain::genesis(4),
        Some(st) => Chain::genesis_with(4, |g| g.controller.st = st, true),
    };
    let w = ch.winning(&seed1);
    let second = if w.bonus == 1 { 2 } else { 1 };
    let (mut b, ops) = ch.buy_case(0);
    b.bundle = vec![Combo::new(w.main, w.bonus), Combo::new(w.main, second)];
    ch.buy(0, &b, ops, 300_000);
    let (b, ops) = ch.buy_case(1);
    ch.buy(1, &b, ops, 310_000);
    let (cc, ops) = ch.close_case(900_000);
    ch.close(&cc, ops, lottery::close::TEST_DEADLINE as u64 + 10);
    let seed_ref = ch.marker.as_ref().unwrap().1.seed_ref;
    let (c, ops) = ch.draw_case(seed1, &[], (seed_ref + DRAW_WINDOW + 1_000) as u64);
    assert_eq!(c.expected().np(), np, "round 1 creates {np} pools");
    ch.draw(&c, ops, c.lock_time + 10);
    // Round 2: one buy, close at T2.
    let t2 = ch.controller.1.deadline as u64;
    let (b, ops) = ch.buy_case(3);
    ch.buy(3, &b, ops, t2 - 1_000);
    let (mut c2, ops) = ch.close_case(t2 - 500);
    c2.lock_time = t2;
    ch.close(&c2, ops, t2 + 10);
    ch
}

/// DP14: round 2's draw absorbs one round 1 pool, and (separately) all five of them, at
/// lock_time = their deadline: Q goes to O'; each pool's funder gets R. With every lower
/// tier funded (`ST_ALL`, seven round 1 pools), it absorbs A_MAX = 7 of them (M4 review U5).
#[test]
fn dp14_round_2_absorbs_round_1_pools() {
    assert_eq!(A_MAX, 7);
    let five = two_round_chain([1u8; 32]);
    let seven = two_round_chain_with([1u8; 32], Some(ST_ALL), 7);
    for (ch, absorb) in [(&five, vec![2]), (&five, vec![0, 1, 2, 3, 4]), (&seven, (0..A_MAX).collect())] {
        let n0 = ch.pools.len();
        let deadline = ch.pools[0].1.deadline as u64;
        assert!(ch.pools.iter().all(|p| p.1.deadline as u64 == deadline));
        let mut ch = ch.clone();
        let (c, ops) = ch.draw_case([2u8; 32], &absorb, deadline);
        assert_eq!(c.lock_time, deadline, "lock_time = the pools' deadline");
        let x = c.expected();
        assert_eq!(x.a, absorb.len());
        let q: i64 = absorb.iter().map(|&i| ch.pools[i].2 as i64).sum();
        assert_eq!(x.q.iter().sum::<i64>(), q);
        let unpaid: i64 = (0..5).filter(|&t| !x.pools.lp[t]).map(|t| x.amounts.b[t]).sum();
        let jb = if x.pools.jp { 0 } else { c.marker.j_boost };
        assert_eq!(x.o_prime, c.pot as i64 + x.amounts.b_r + jb + unpaid + q, "Q goes to O'");
        let f = ch.draw(&c, ops, deadline + 10);
        let x = check_draw(&c, &f);
        for i in 0..absorb.len() {
            assert_eq!(f.tx.outputs[x.pool_refund(i)].value, c.reserve());
        }
        assert_eq!(ch.pools.len(), n0 - absorb.len() + x.np());
    }
    assert_eq!(seven.pools.len(), A_MAX);
}

/// Every lower tier funded (shares 400, 300, 300, 200, 200): a draw creates seven pools.
const ST_ALL: [i64; 5] = [400, 300, 300, 200, 200];

/// DP16: two full rounds: round 2's close uses J(2), m and the flags from draw 1, and round
/// 2 is drawn.
#[test]
fn dp16_two_full_rounds() {
    let ch0 = two_round_chain([1u8; 32]);
    let mut ch = ch0.clone();
    let mk2 = ch.marker.clone().unwrap().1;
    // The round 2 marker froze the values draw 1 fixed in the round 2 controller.
    let drawn2 = {
        // Rebuild draw 1's controller' from the chain: the vault's liability is L(2).
        let v = &ch.vault.1;
        assert_eq!(v.round, 1);
        v.liability
    };
    assert_eq!(mk2.round, 2);
    assert_eq!(mk2.jackpot - mk2.j_boost + mk2.g2 * (mk2.m - 1), drawn2, "marker 2 carries J(2), J_boost(2), G2(2) with L(2) = the vault's");
    assert_eq!(ch.controller.1.round, 3);
    let seed_ref = mk2.seed_ref;
    let (c, ops) = ch.draw_case([3u8; 32], &[], (seed_ref + DRAW_WINDOW + 1_000) as u64);
    let f = ch.draw(&c, ops, c.lock_time + 10);
    check_draw(&c, &f);
    assert_eq!(ch.drawer.1.round, 3);
    assert_eq!(ch.vault.1.round, 2);
    assert_eq!(ch.controller.1.drawn, 1);
}

// ---------------------------------------------------------------- DP15

/// DP15: a zero-sales draw (no pools at all, B = 0), and a draw whose tier 3+b budget is
/// positive but below W (slice 0): no pool for it, its B goes to the boost pot.
#[test]
fn dp15_zero_sales_and_slice_zero() {
    let c = DrawCase::new(4);
    let x = c.expected();
    assert_eq!(x.np(), 0);
    assert_eq!(x.amounts.b, [0; 5]);
    run(&c);
    let mut c = DrawCase::new(4);
    c.set_sales(50_000, 0);
    let x = c.expected();
    assert_eq!(x.pools.slices, [16, 1, 0, 0, 0]);
    assert!(x.amounts.b[2] > 0 && !x.pools.lp[2]);
    assert_eq!(tiers(&x), vec![TIER_LOWER0, TIER_LOWER0 + 1]);
    let unpaid = x.amounts.b[2] + x.amounts.b[3] + x.amounts.b[4];
    assert_eq!(x.o_prime, c.pot as i64 + x.amounts.b_r + c.marker.j_boost + unpaid);
    run(&c);
}

// ---------------------------------------------------------------- DP17, DP18 (script halves)

/// DP17 (script half): the drawer accepts the reference witnesses over a grid of V', O',
/// price and m'.
#[test]
fn dp17_script_accepts_reference_witnesses() {
    for v in [1_000_000i64, 4_000_000, 10_000_000] {
        for pot in [0u64, 50_000, 600_000] {
            for price in [KAS as i64, 10 * KAS as i64] {
                for m in [5i64, 30] {
                    let mut c = sold();
                    c.vault.total_assets = v * KAS as i64;
                    c.pot = pot * KAS;
                    c.controller.price = price;
                    c.controller.m = m;
                    let x = c.expected();
                    assert!(witness_ok(&x.rule, x.v_prime, x.o_prime, c.controller.j, x.fix.x1, x.fix.y, x.fix.x2));
                    run(&c);
                }
            }
        }
    }
}

/// DP18 (script half): Rust = script for the seeds giving rank 0, rank 142,505, bonus 1 and
/// bonus m (round 1, m = 10, cov = COV; seeds from m3_game `dp18_seed_search`).
#[test]
fn dp18_script_seed_mapping() {
    for (i, want) in [(70_922u64, "rank 0"), (36_808, "rank max"), (15, "bonus 1"), (2, "bonus m")] {
        let mut c = sold();
        c.test_only_seed = [0; 32];
        c.test_only_seed[..8].copy_from_slice(&i.to_le_bytes());
        let s = c.seed();
        match want {
            "rank 0" => assert_eq!(s.rank, 0),
            "rank max" => assert_eq!(s.rank, lottery::combo::RANKS - 1),
            "bonus 1" => assert_eq!(s.bonus, 1),
            _ => assert_eq!(s.bonus as i64, c.marker.m),
        }
        c.hold_jackpot(0, 1);
        let x = c.expected();
        assert_eq!(x.new_pools[0].win, [s.main[0], s.main[1], s.main[2], s.main[3], s.main[4], s.bonus]);
        assert_eq!(s.main, unrank(s.rank));
        run(&c);
    }
}

// ---------------------------------------------------------------- DP19

/// DP19: claim deadline (D39): a fresh funding input gives own + CLAIM_WINDOW; only old
/// inputs give seed_ref + DRAW_WINDOW + CLAIM_WINDOW.
#[test]
fn dp19_claim_deadline() {
    let mut c = sold();
    c.hold_jackpot(0, 1);
    let own = c.own() as i64;
    assert!(own > c.marker.seed_ref + DRAW_WINDOW);
    let x = c.expected();
    assert_eq!(x.d_claim, own + CLAIM_WINDOW);
    assert!(x.new_pools.iter().all(|p| p.deadline == own + CLAIM_WINDOW));
    run(&c);
    let mut c = c.clone();
    c.funding = vec![FundingIn { value: 100 * KAS, daa: 900_000 }];
    assert!((c.own() as i64) < c.marker.seed_ref + DRAW_WINDOW);
    let x = c.expected();
    assert_eq!(x.d_claim, c.marker.seed_ref + DRAW_WINDOW + CLAIM_WINDOW);
    run(&c);
}

// ---------------------------------------------------------------- DP20

/// DP20: empty leaves (Z_0) for shards with no holding at the winning rank: shard 0 holds
/// another rank (non-empty tree), shard 1 is empty, shard 2 holds the jackpot.
#[test]
fn dp20_empty_leaves() {
    let mut c = sold();
    let other = (c.seed().rank + 7) % lottery::combo::RANKS;
    c.hold_at(0, other, 2, 1);
    c.hold_jackpot(2, 1);
    let x = c.expected();
    let z = empty_hashes();
    assert!(x.counts[0].iter().all(|&n| n == 0) && c.trees[0].root() != z[lottery::smt::DEPTH]);
    assert!(x.counts[1].iter().all(|&n| n == 0) && c.trees[1].root() == Tree::new(10).root());
    assert_eq!(x.h_j, 1);
    run(&c);
}

// ---------------------------------------------------------------- A6 G2 guard (user decision)

/// A6 G2 guard, user decision (2026-10-10): "G2 <= TOTAL_MAX / 254" is checked only when a
/// second-prize pool is created (n2 > 0). A draw with G2 = TOTAL_MAX / 254 + 1 and n2 = 0 is
/// accepted. (The n2 > 0 rejection is in m3_attacks.rs, `a6_g2_guard_with_second_pool`.)
#[test]
fn a6_g2_guard_skipped_without_second_pool() {
    let mut c = sold();
    c.marker.g2 = TOTAL_MAX / 254 + 1;
    c.hold_jackpot(0, 1);
    let x = c.expected();
    assert_eq!(x.n2, 0);
    assert!(!x.pools.sp);
    run(&c);
}

// ---------------------------------------------------------------- Stage 5 (review)

/// DP23 (Stage 5, user decision A): a zero-sales round at f_min = F_MIN_FLOOR (0.1 KAS)
/// with acc = 0 (T_F = 0) is drawn end to end (genesis -> close -> draw, then the round 2
/// close): the fee is carried (no fee output) and every output has a positive value
/// (a zero-value output is consensus-invalid, rusty-kaspa v2.1.0
/// `tx_validation_in_isolation.rs:150-151`, which the script engine does not check).
/// Rust side: with f_min = 0 (refused by genesis and by the drawer since Stage 5) the same
/// round would need a fee output of 0 sompi.
#[test]
fn dp23_zero_sales_round_at_f_min_floor() {
    let edit = |f_min: i64| {
        move |g: &mut lottery::genesis::GenesisCase| {
            g.drawer.f_min = f_min;
            g.controller.acc = 0;
        }
    };
    // f_min = 0 (unchecked genesis): the draw would pay a 0-sompi fee output.
    let mut ch0 = Chain::genesis_with(4, edit(0), false);
    let (c, ops) = ch0.close_case(900_000);
    ch0.close(&c, ops, lottery::close::TEST_DEADLINE as u64 + 10);
    let seed_ref = ch0.marker.as_ref().unwrap().1.seed_ref;
    let (d0, _) = ch0.draw_case([4u8; 32], &[], (seed_ref + DRAW_WINDOW + 1_000) as u64);
    let x0 = d0.expected();
    assert_eq!(x0.fee.t_f, 0);
    assert_eq!(x0.fee_output().map(|i| x0.values[i]), Some(0), "f_min = 0: zero-value fee output");
    // f_min = F_MIN_FLOOR (checked genesis).
    let mut ch = Chain::genesis_with(4, edit(F_MIN_FLOOR), true);
    let (c, ops) = ch.close_case(900_000);
    ch.close(&c, ops, lottery::close::TEST_DEADLINE as u64 + 10);
    let mk = ch.marker.as_ref().unwrap().1.clone();
    assert_eq!((mk.sales, ch.controller.1.acc), (0, 0), "zero sales, A = 0");
    let seed_ref = mk.seed_ref;
    let (d, ops) = ch.draw_case([4u8; 32], &[], (seed_ref + DRAW_WINDOW + 1_000) as u64);
    let x = d.expected();
    assert_eq!(x.fee.t_f, 0);
    assert!(!x.fee.paid && x.fee_output().is_none(), "T_F = 0 < f_min: carried");
    let f = ch.draw(&d, ops, d.lock_time + 10);
    check_draw(&d, &f);
    assert!(f.tx.outputs.iter().all(|o| o.value > 0), "every output value > 0");
    assert_eq!(ch.drawer.1.f_min, F_MIN_FLOOR, "drawer' keeps f_min");
    // The next round is not blocked: the round 2 close.
    let t2 = ch.controller.1.deadline as u64;
    let (mut c2, ops) = ch.close_case(t2 - 500);
    c2.lock_time = t2;
    ch.close(&c2, ops, t2 + 10);
    assert_eq!(ch.marker.as_ref().unwrap().1.round, 2);
}

/// DP24 (Stage 5, review item 5, D45 zero-prize rules): J = 0 with the jackpot held
/// (h_J >= 1) creates no jackpot pool; G2 = 0 with a second-prize bonus held (n2 >= 1)
/// creates no second-prize pool. Both draws are accepted (the m3_mutants weakened
/// `bool jp = hJ >= 1;` / `bool sp = n2 >= 1;` reject them).
#[test]
fn dp24_zero_prizes_held() {
    let c = common::m3::zero_jackpot_held();
    let (x, _) = run(&c);
    assert!(!tiers(&x).contains(&TIER_JACKPOT));
    let c = common::m3::zero_g2_held();
    let (x, _) = run(&c);
    assert!(!tiers(&x).contains(&TIER_SECOND));
}

// ---------------------------------------------------------------- DP22

/// DP22 (D43): `cargo check -p lottery --no-default-features --features mainnet,test-seed`
/// fails with the gate's compile_error, and `--no-default-features --features mainnet`
/// builds (without a drawer). Shells out to the same cargo with a separate target directory
/// (`crates/target/dp22`) so it does not wait on this test run's build lock; the first run
/// checks the dependencies once (a few minutes), later runs are fast.
#[test]
fn dp22_seed_gate() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml");
    let target = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/dp22");
    let check = |features: &str| {
        std::process::Command::new(env!("CARGO"))
            .args(["check", "-p", "lottery", "--no-default-features", "--features", features, "--manifest-path", manifest, "--target-dir", target])
            .output()
            .expect("run cargo")
    };
    let gated = check("mainnet,test-seed");
    let err = String::from_utf8_lossy(&gated.stderr);
    assert!(!gated.status.success(), "mainnet + test-seed must not build");
    assert!(err.contains("TEST-ONLY seed cannot be built for mainnet"), "gate message missing:\n{err}");
    let main = check("mainnet");
    assert!(main.status.success(), "mainnet without test-seed must build:\n{}", String::from_utf8_lossy(&main.stderr));
}
