//! M7 positive tests SP1-SP13 (spec M7 A8 "Positive"): the real seed (the anchor block's
//! hash, D93-D95), the void (D96), the claim deadline (D98), the marker without `seed_ref`
//! and `refresh()` (D99), the deleted TEST-ONLY path (D101) and the A5 drift inequality.
//!
//! Every transaction runs every input in the v2.1.0 engine under committed budgets against a
//! `harness::ChainSim` (`execute_on_chain`), legs first, then the drawer; every draw and void
//! is also checked output by output against `DrawExpected` with the conservation of M3 A4 /
//! M7 D105 (`check_draw`).
//!
//! M7 Stage 1b (tests first, scripts untouched): every test that draws, voids or closes stops
//! at the committed scripts (the drawer has only `draw_test_seed`: EarlyReturn at input 0;
//! the controller's close check 24 still writes `seed_ref`: VerifyError at input 0). SP9,
//! SP11, the feature half of SP12 and the `lottery/src` half of SP12's grep pass already.

mod common;

use std::sync::Arc;

use common::m2::*;
use common::m5::{accept_draw5, check_draw5};
use common::m7::*;
use common::measure::{F_MAX, MARGIN_PCT, Pending, dp21, pct, with_pending, within};
use harness::{ChainSim, CovenantBinding, K_MC, K_SEQ, TxScriptError, masses};
use lottery::anchor::{AnchorProof, BlockFields};
use lottery::buy::{Built, Final, InKind, KAS};
use lottery::claim::{combo_for_tier, combo_matching};
use lottery::close::TEST_DEADLINE;
use lottery::combo::{Combo, RANKS, unrank};
use lottery::draw::{DrawCase, DrawExpected, RoundResult};
use lottery::game::{A_MAX, CLAIM_WINDOW, DRAW_WINDOW, M_DRAW, SEED_OFFSET, VOID_MIN, seed_numbers, witness_ok};
use lottery::state::{MarkerState, Payout, TIER_JACKPOT, TIER_LOWER0};

// ---------------------------------------------------------------- checks

/// Every output (count, value, script, covenant binding) as `DrawExpected` says, the caller
/// output by the A4 formula, inputs - outputs = fee, and the economic conservation of M3 A4
/// (spec M7 D105: in a void H moves from V' to O', no pools).
fn check_draw(c: &DrawCase, f: &Final) -> DrawExpected {
    let x = c.expected();
    let t = tpl();
    assert_eq!(f.tx.outputs.len(), x.n_outputs(), "A4: 3 + np + rec + paid + 1 + a + 1 outputs");
    for (i, (o, v)) in f.tx.outputs.iter().zip(&x.values).enumerate() {
        assert_eq!(o.value, *v, "output {i} value");
    }
    assert_eq!(f.tx.outputs[0].script_public_key, t.drawer_spk(&x.drawer));
    assert_eq!(f.tx.outputs[1].script_public_key, t.vault_spk(&x.vault));
    assert_eq!(f.tx.outputs[2].script_public_key, t.controller_spk(&x.controller));
    for (p, s) in x.new_pools.iter().enumerate() {
        assert_eq!(f.tx.outputs[x.pool_output(p)].script_public_key, t.pool_spk(s), "pool {p}");
    }
    for (i, o) in f.tx.outputs.iter().enumerate() {
        if i < x.n_cov() {
            assert_eq!(o.covenant, Some(CovenantBinding::new(0, c.cov)), "output {i} authorized by input 0");
        } else {
            assert!(o.covenant.is_none(), "output {i} has the zero ID");
        }
    }
    if let Some(i) = x.fee_output() {
        assert_eq!(f.tx.outputs[i].script_public_key, Payout::from_fields(c.controller.fee_class, &c.controller.fee_key).spk());
    }
    assert_eq!(f.tx.outputs[x.marker_refund()].script_public_key, Payout::from_fields(c.marker.funder_class, &c.marker.funder_key).spk());
    assert_eq!(f.tx.outputs[x.caller_output()].value as i128, c.caller_formula(&x) - if x.rec { c.reserve() as i128 } else { 0 }, "caller output");
    assert_eq!(f.fee(), c.fee, "inputs - outputs = fee");
    let mk = &c.marker;
    let lhs = (mk.sales - mk.ref_paid - x.amounts.c_close + mk.j_boost) as i128
        + c.vault.total_assets as i128
        + (c.pot as i128 + c.controller.acc as i128)
        + x.q.iter().map(|&q| q as i128).sum::<i128>();
    let pools: i128 = x.pools.pools.iter().map(|p| p.value as i128).sum();
    let rhs = pools + x.v_prime as i128 + (x.o_prime + x.fee.a_next) as i128 + if x.fee.paid { x.fee.t_f as i128 } else { 0 } + x.amounts.c_draw as i128;
    assert_eq!(lhs, rhs, "A4 / D105 conservation");
    if x.rec {
        check_draw5(c, f);
    }
    x
}

/// Accept a standalone draw or void (every input, legs first) and check its outputs.
fn run(c: &DrawCase) -> (DrawExpected, Final) {
    let f = accept_draw5("M7 draw", c, &c.build());
    let x = check_draw(c, &f);
    (x, f)
}

/// The fixture draw on `sim`: round 2, K = 4, sales 157 KAS, the jackpot held (shard 1), a
/// second-prize bonus held (shard 2), one absorbed pool; the holdings follow the sim's X.hash.
fn held_on(sim: ChainSim) -> DrawCase {
    let mut c = with_sim(DrawCase::at_round(4, 2), sim);
    c.set_sales(157 * KAS as i64, 5 * KAS as i64);
    c.hold_jackpot(1, 1);
    let b = c.other_bonus(0);
    c.hold(2, b, 2);
    c.absorb(1);
    let x = c.expected();
    assert_eq!((x.h_j, x.n2), (1, 1));
    c
}

/// Draw the chain's closed round on `sim` (X.hash = the sim's anchor for D), one funding
/// input created at `fund_daa`, applied at lock_time + 10.
fn draw_on(ch: &mut Chain, sim: ChainSim, fund_daa: u64) -> (DrawCase, Final) {
    let d = ch.anchor_d() as u64;
    let xh = sim.anchor_for(d).0.hash.as_bytes();
    let (mut c, ops) = ch.draw_case(xh, &[], fund_daa);
    c.sim = Some(Arc::new(sim));
    let f = ch.draw(&c, ops, c.lock_time + 10);
    check_draw(&c, &f);
    (c, f)
}

/// Close the chain's round at T + 10 (funding created at T - 500, lock time T).
fn close_at_t(ch: &mut Chain) -> u64 {
    let t = ch.controller.1.deadline as u64;
    let (mut cc, ops) = ch.close_case(t - 500);
    cc.lock_time = t;
    ch.close(&cc, ops, t + 10);
    t + 10
}

// ---------------------------------------------------------------- SP1

/// SP1: genesis -> buys in shards 0, 1 and 2 (the jackpot, a second prize, others) -> close
/// (marker created at DAA c) -> a chain with irregular DAA steps past D = c + 600 -> draw()
/// with the real anchor (X = the first chain block with DAA >= D): every output equals
/// `DrawExpected`, conservation holds, the winning numbers are seed_numbers(cov, 1, X.hash,
/// m), and the round result record rebuilds X.hash, mk_daa and D_claim.
#[test]
fn sp1_full_round_on_a_chain_sim() {
    let mut ch = Chain::genesis(4);
    let close_daa = TEST_DEADLINE as u64 + 10;
    let d = close_daa + ch.controller.1.seed_offset as u64;
    assert_eq!(ch.controller.1.seed_offset, SEED_OFFSET);
    let sim = irregular_sim(d);
    let (xb, pb) = sim.anchor_for(d);
    assert!(xb.daa_score >= d && pb.daa_score < d && xb.parent == pb.hash);
    let xh = xb.hash.as_bytes();
    let w = ch.winning(&xh);
    let second = if w.bonus == 1 { 2 } else { 1 };
    let (mut b0, ops) = ch.buy_case(0);
    b0.with_ref = true;
    b0.bundle = vec![Combo::new(w.main, w.bonus)];
    ch.buy(0, &b0, ops, 300_000);
    let (mut b1, ops) = ch.buy_case(1);
    b1.bundle = vec![Combo::new(w.main, second)];
    ch.buy(1, &b1, ops, 310_000);
    let (mut b2, ops) = ch.buy_case(2);
    let mut others = common::distinct(3, 10);
    others.retain(|c| c.main != w.main);
    b2.bundle = others;
    ch.buy(2, &b2, ops, 320_000);
    let (cc, ops) = ch.close_case(900_000);
    ch.close(&cc, ops, close_daa);
    assert_eq!(ch.anchor_d() as u64, d, "D = the marker's creation DAA + seed_offset");
    let (c, f) = draw_on(&mut ch, sim, d + DRAW_WINDOW as u64 + 1_000);
    let x = c.expected();
    let cov = ch.cov.as_bytes();
    assert_eq!(x.seed, seed_numbers(&cov, 1, &xh, c.marker.m), "the seed is X.hash's");
    assert_eq!((x.rank, x.bonus, x.win5), (w.rank, w.bonus, w.main));
    assert_eq!((x.h_j, x.n2), (1, 1));
    assert_eq!(x.anchor, AnchorProof::from_sim(&c.chain(), d).to_bytes(), "the 304-byte proof of (X, P)");
    for p in &x.new_pools {
        assert_eq!(p.win, [w.main[0], w.main[1], w.main[2], w.main[3], w.main[4], w.bonus]);
        assert_eq!(p.deadline, d as i64 + DRAW_WINDOW + CLAIM_WINDOW);
    }
    let rr = RoundResult::from_tx(&f.tx, &tpl()).expect("round result");
    assert_eq!((rr.x_hash, rr.mk_daa, rr.void, rr.d_claim), (xh, close_daa as i64, false, d as i64 + DRAW_WINDOW + CLAIM_WINDOW));
    assert_eq!(rr.win, [w.main[0], w.main[1], w.main[2], w.main[3], w.main[4], w.bonus]);
}

// ---------------------------------------------------------------- SP2

/// SP2: anchor boundaries. X at DAA D with P at D - 1; X at D + 247 with P at D - 1 (the
/// largest DAA jump, MERGESET_MAX = 248); P whose DAA equals its own selected parent's
/// (equal-DAA chain blocks); the point of view at X.bs + F - 1 (the last admissible one), with
/// lock_time = D and with lock_time = D + draw_window (a 304-byte anchor is a draw whatever the
/// lock; the mode follows the anchor, spec M7 SA7 note).
#[test]
fn sp2_anchor_boundaries() {
    let d = DrawCase::at_round(4, 2).d() as u64;
    // X at D, P at D - 1.
    let (s, _) = line_sim(d);
    let c = held_on(s);
    let (x, p) = c.chain().anchor_for(d);
    assert_eq!((x.daa_score, p.daa_score), (d, d - 1));
    run(&c);
    // X at D + 247, P at D - 1.
    let s = ChainSim::line(d - 10, B0, 10).with_jump(248).with_extend(5);
    let c = held_on(s);
    let (x, p) = c.chain().anchor_for(d);
    assert_eq!((x.daa_score, p.daa_score), (d + 247, d - 1));
    run(&c);
    // P (D - 1) with an equal-DAA selected parent (D - 1).
    let s = ChainSim::line(d - 10, B0, 9).with_jump(1).with_jump(0).with_jump(1).with_extend(5);
    let c = held_on(s);
    let (x, p) = c.chain().anchor_for(d);
    let pp = c.chain().block(p.parent).copied().unwrap();
    assert_eq!((x.daa_score, p.daa_score, pp.daa_score), (d, d - 1, d - 1));
    run(&c);
    // The last admissible point of view: X.bs + F - 1 (P is one blue score older, so the
    // opcode on P admits it only if P.bs + F > pov: with blue +1 per block P is at X.bs - 1
    // and pov = X.bs + F - 1 = P.bs + F: P is refused. The last point of view at which both
    // are admitted is P.bs + F - 1 = X.bs + F - 2.) Both are measured: X.bs + F - 2 accepted;
    // X.bs + F - 1 is the last for X alone (rejected by the opcode on P).
    let (s, _) = line_sim(d);
    let (x, p) = s.anchor_for(d);
    let f = s.finality_depth();
    assert_eq!(p.blue_score + 1, x.blue_score);
    let c = held_on(s.clone().with_pov(p.blue_score + f - 1));
    run(&c);
    let mut late = c.clone();
    late.lock_time = d + DRAW_WINDOW as u64;
    run(&late);
    // X.bs + F - 1: X admitted, P too deep (BlockIsTooDeep on P).
    let c = held_on(s.with_pov(x.blue_score + f - 1));
    let b = c.build();
    let e = b.finish().run_input(0).expect_err("pov = X.bs + F - 1 = P.bs + F: the opcode refuses P");
    assert!(matches!(e, TxScriptError::BlockIsTooDeep(_)), "{e:?}");
}

// ---------------------------------------------------------------- SP3

/// The first X.hash (from a counter in bytes 0..8) whose seed for (cov, round 1, m = 10)
/// satisfies `pred`.
fn search_x_hash(pred: impl Fn(&lottery::game::SeedNumbers) -> bool) -> [u8; 32] {
    let c = DrawCase::new(4);
    let cov = c.cov.as_bytes();
    for i in 0u64..20_000_000 {
        let mut h = [0x5au8; 32];
        h[..8].copy_from_slice(&i.to_le_bytes());
        if pred(&seed_numbers(&cov, 1, &h, c.marker.m)) {
            return h;
        }
    }
    panic!("no X.hash found");
}

/// SP3: the seed mapping, Rust = script, at the edges: X.hash values giving rank 0, rank
/// 142,505 (= N - 1), bonus 1 and bonus m. Each draw holds the jackpot (so h_J = 1 needs the
/// script's rank and bonus) and creates lower-tier pools carrying win5 || bonus.
#[test]
fn sp3_seed_mapping_edges() {
    let m = DrawCase::new(4).marker.m as u8;
    let targets: [(&str, Box<dyn Fn(&lottery::game::SeedNumbers) -> bool>); 4] = [
        ("rank 0", Box::new(|s| s.rank == 0)),
        ("rank N - 1", Box::new(|s| s.rank == RANKS - 1)),
        ("bonus 1", Box::new(|s| s.bonus == 1)),
        ("bonus m", Box::new(move |s| s.bonus == m)),
    ];
    assert_eq!(RANKS, 142_506);
    for (name, pred) in targets {
        let h = search_x_hash(&*pred);
        let mut c = DrawCase::new(4);
        c.x_hash = h;
        c.set_sales(100 * KAS as i64, 5 * KAS as i64);
        c.hold_jackpot(0, 1);
        let x = c.expected();
        assert!(pred(&x.seed), "{name}");
        assert_eq!(x.win5, unrank(x.seed.rank));
        assert_eq!((x.h_j, x.pools.jp), (1, true), "{name}");
        println!("SP3 {name}: X.hash {} -> rank {}, bonus {}, win5 {:?}", hex(&h), x.seed.rank, x.seed.bonus, x.win5);
        run(&c);
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ---------------------------------------------------------------- SP4

/// A void of round 2 (spec M7 D96): fee paid or not, a pending deposit and withdrawal or not,
/// `a` absorbed pools.
fn void_case(a: usize, paid: bool, pending: bool) -> DrawCase {
    let mut c = DrawCase::at_round(4, 2);
    if paid {
        c.set_sales(157 * KAS as i64, 5 * KAS as i64);
    } else {
        c.controller.acc = 0;
        c.set_sales(10 * KAS as i64, 0);
    }
    if a > 0 {
        c.absorb(a);
    }
    c.set_void(true);
    if pending {
        c = with_pending(c, Pending::Both);
    }
    let x = c.expected();
    assert_eq!((x.fee.paid, x.rec, x.a), (paid, pending, a));
    c
}

/// SP4: void after D + 2F: no pools, O' = O + b_r + J_boost + sum B_t + sum Q + H, V' = V;
/// fee paid and unpaid; with and without pending vault requests (the record); with 0 and
/// A_MAX absorbed pools; conservation; J(r+1) fixed and the controller drawn as in a draw.
#[test]
fn sp4_void() {
    for a in [0, A_MAX] {
        for paid in [true, false] {
            for pending in [false, true] {
                let c = void_case(a, paid, pending);
                let x = c.expected();
                let name = format!("void a = {a}, fee paid {paid}, pending {pending}");
                assert_eq!(c.lock_time as i64, c.d() + c.drawer.draw_window, "{name}: lock = D + draw_window");
                assert_eq!(c.drawer.draw_window, VOID_MIN);
                assert!(x.void && x.anchor.is_empty() && x.win5 == [0; 5] && x.bonus == 0, "{name}");
                assert!(x.leaves.is_empty() && x.siblings.is_empty() && (x.h_j, x.n2) == (0, 0), "{name}");
                assert_eq!(x.np(), 0, "{name}: no pools");
                assert_eq!(x.v_prime, c.vault.total_assets, "{name}: V' = V");
                let q: i64 = x.q.iter().sum();
                let b: i64 = x.amounts.b.iter().sum();
                assert_eq!(x.o_prime, c.pot as i64 + x.amounts.b_r + c.marker.j_boost + b + q + x.amounts.h, "{name}: O'");
                assert_eq!(x.controller.drawn, 1);
                assert_eq!(x.controller.jackpot, x.fix.x2 + x.fix.y);
                assert!(witness_ok(&x.rule, x.v_prime, x.o_prime, c.controller.j, x.fix.x1, x.fix.y, x.fix.x2), "{name}: J(r+1) witnesses");
                assert_eq!(x.d_claim, c.d() + DRAW_WINDOW + CLAIM_WINDOW);
                let (_, f) = run(&c);
                let rr = RoundResult::from_tx(&f.tx, &tpl()).expect("round result");
                assert!(rr.void && rr.pools.is_empty() && rr.x_hash == [0; 32], "{name}: the round result is a void");
            }
        }
    }
}

// ---------------------------------------------------------------- SP5

/// SP5 (R1 closed, spec M7 D93): three closes of the same round whose funding inputs were
/// created at DAA 5,000, 400,000 and 900,000 (T = 1,000,000), accepted at the same chain block,
/// give markers with identical state and the same D; the same draw (anchor proof, outputs) is
/// valid after each.
#[test]
fn sp5_reference_independent_of_funding_age() {
    let mut base = Chain::genesis(4);
    assert_eq!(base.controller.1.deadline, 1_000_000);
    let (b, ops) = base.buy_case(0);
    base.buy(0, &b, ops, 300_000);
    let at = TEST_DEADLINE as u64 + 10;
    let mut chains = vec![];
    for fd in [5_000u64, 400_000, 900_000] {
        let mut ch = base.clone();
        let (cc, ops) = ch.close_case(fd);
        ch.close(&cc, ops, at);
        chains.push(ch);
    }
    let mk: Vec<MarkerState> = chains.iter().map(|c| c.marker.clone().unwrap().1).collect();
    assert!(mk.iter().all(|m| *m == mk[0]), "identical marker states");
    let ds: Vec<i64> = chains.iter().map(|c| c.anchor_d()).collect();
    assert!(ds.iter().all(|&d| d == ds[0]), "the same D: {ds:?}");
    let d = ds[0] as u64;
    let (sim, _) = line_sim(d);
    let xh = sim.anchor_for(d).0.hash.as_bytes();
    let mut outs = vec![];
    for ch in &mut chains {
        let fund = d + DRAW_WINDOW as u64 + 1_000;
        let (mut c, ops) = ch.draw_case(xh, &[], fund);
        c.sim = Some(Arc::new(sim.clone()));
        let x = c.expected();
        outs.push((x.anchor.clone(), x.values.clone(), c.build().outputs.iter().map(|o| o.script_public_key.clone()).collect::<Vec<_>>()));
        let f = ch.draw(&c, ops, c.lock_time + 10);
        check_draw(&c, &f);
    }
    assert!(outs.iter().all(|o| *o == outs[0]), "the same anchor proof and outputs after each close");
}

// ---------------------------------------------------------------- SP6

/// SP6 (U4 closed, spec M7 D98): the pools carry D + 2F + CLAIM_WINDOW whether the draw is
/// funded by a fresh input or an old one; M4's claim() (a PAY claim of the jackpot) and
/// return_deposit_with_pool() (at the pool's deadline) run unchanged against them.
#[test]
fn sp6_claim_deadline() {
    const ST: [i64; 5] = [400, 300, 300, 0, 0];
    let price = 100 * KAS;
    let mut base = Chain::genesis_with(
        4,
        |g| {
            g.controller.price = price as i64;
            g.controller.st = ST;
            for s in &mut g.shards {
                s.price = price as i64;
            }
        },
        true,
    );
    let close_daa = TEST_DEADLINE as u64 + 10;
    let d = close_daa + SEED_OFFSET as u64;
    let (sim, _) = line_sim(d);
    let xh = sim.anchor_for(d).0.hash.as_bytes();
    let w = base.winning(&xh);
    let win = [w.main[0], w.main[1], w.main[2], w.main[3], w.main[4], w.bonus];
    let (mut b, ops) = base.buy_case_funded(0, 2 * price + 10 * KAS);
    b.bundle = vec![combo_for_tier(&win, TIER_JACKPOT, 10, 0), combo_matching(&win, 0, false, 10, 1)];
    base.buy(0, &b, ops, 300_000);
    let (mut b, ops) = base.buy_case_funded(1, price + 10 * KAS);
    b.bundle = vec![combo_matching(&win, 0, false, 10, 2)];
    base.buy(1, &b, ops, 310_000);
    let (cc, ops) = base.close_case(900_000);
    base.close(&cc, ops, close_daa);
    assert_eq!(base.anchor_d() as u64, d);
    let deadline = d as i64 + DRAW_WINDOW + CLAIM_WINDOW;
    for (name, fund) in [("fresh", d + DRAW_WINDOW as u64 + 1_000), ("old", 300)] {
        let mut ch = base.clone();
        draw_on(&mut ch, sim.clone(), fund);
        assert!(!ch.pools.is_empty() && ch.pools.iter().all(|p| p.1.deadline == deadline), "{name} funding: every pool's deadline is D + 2F + CLAIM_WINDOW");
        // claim(): the jackpot, PAY (b = 2), well before the deadline.
        let now = d + 1_000_000;
        let pj = ch.pool_of(1, TIER_JACKPOT);
        let (c, ops) = ch.claim_case(0, pj, 0, now - 1_000);
        assert_eq!(c.expected().prize, c.pool.slice);
        ch.claim(&c, ops, now);
        // return_deposit_with_pool(): ticket 1 (no prize) with a lower-tier pool at its deadline.
        let pl = ch.pool_of(1, TIER_LOWER0);
        let (r, ops) = ch.return_with_pool_case(1, pl, now);
        assert_eq!(r.lock_time as i64, deadline);
        ch.return_deposit(&r, ops, r.lock_time);
    }
}

// ---------------------------------------------------------------- SP7

/// SP7: reorg. A draw valid on chain A fails after the chain reorganizes to B, where A's X is
/// no longer a chain block (BlockNotSelected, `seqcommit-accessor.md` "Reorg Behavior"); B's
/// anchor is a different block with a different outcome; the draw rebuilt on B is valid.
#[test]
fn sp7_reorg() {
    let d = DrawCase::at_round(4, 2).d() as u64;
    let mut s = ChainSim::line(d - 10, B0, 7);
    let fork = s.tip();
    assert_eq!(s.block(fork).unwrap().daa_score, d - 4);
    s.extend(20);
    let b_tip = s.fork(fork, 25);
    let a = s.clone();
    let mut b = s;
    b.reorg_to(b_tip);
    let (xa, xb) = (a.anchor_for(d).0, b.anchor_for(d).0);
    assert_ne!(xa.hash, xb.hash);
    assert_eq!((xa.daa_score, xb.daa_score), (d, d));
    let ca = held_on(a);
    let good = ca.build();
    run(&ca);
    // The same transaction after the reorg.
    let gf = good.finish();
    let after = Final { tx: gf.tx.clone(), entries: gf.entries.clone(), units: gf.units.clone(), sim: Some(Arc::new(b.clone())) };
    let e = after.run_input(0).expect_err("A's anchor is off the chain after the reorg");
    assert!(matches!(e, TxScriptError::BlockNotSelected(_)), "{e:?}");
    // B's anchor: another block, another seed.
    let cb = held_on(b);
    assert_ne!(cb.seed().h, ca.seed().h, "a different anchor gives a different seed");
    assert_ne!(cb.anchor_bytes(), ca.anchor_bytes());
    run(&cb);
}

// ---------------------------------------------------------------- SP8

/// SP8: two full rounds, a round followed by a void, then the next round's close and draw
/// (the M3 DP16 chain fixture on `ChainSim`s): every draw and the void accepted, the rounds
/// strictly ordered (the close after the void waits for it: drawn = 1).
#[test]
fn sp8_rounds_and_a_void() {
    let mut ch = Chain::genesis(4);
    let buy = |ch: &mut Chain, shard: usize, daa: u64| {
        let (b, ops) = ch.buy_case(shard);
        ch.buy(shard, &b, ops, daa);
    };
    // Round 1: buy, close, draw.
    buy(&mut ch, 0, 300_000);
    close_at_t(&mut ch);
    let d1 = ch.anchor_d() as u64;
    draw_on(&mut ch, line_sim(d1).0, d1 + DRAW_WINDOW as u64 + 1_000);
    assert_eq!(ch.controller.1.drawn, 1);
    // Round 2: buy, close, draw.
    let t2 = ch.controller.1.deadline as u64;
    buy(&mut ch, 1, t2 - 1_000);
    close_at_t(&mut ch);
    let d2 = ch.anchor_d() as u64;
    draw_on(&mut ch, line_sim(d2).0, d2 + DRAW_WINDOW as u64 + 1_000);
    // Round 3: buy, close, void.
    let t3 = ch.controller.1.deadline as u64;
    buy(&mut ch, 2, t3 - 1_000);
    close_at_t(&mut ch);
    let pot_before = ch.pot;
    let d3 = ch.anchor_d();
    let (vc, ops) = ch.void_case(&[], (d3 + DRAW_WINDOW + 1_000) as u64);
    let vx = vc.expected();
    let fv = ch.draw(&vc, ops, vc.lock_time + 10);
    check_draw(&vc, &fv);
    assert!(vx.void && vx.np() == 0);
    assert_eq!(ch.pot as i64, vx.o_prime);
    assert!(ch.pot > pot_before, "the void's money went to the boost pot");
    assert_eq!(ch.controller.1.drawn, 1);
    // Round 4: buy, close (after the void), draw.
    let t4 = ch.controller.1.deadline as u64;
    buy(&mut ch, 3, t4 - 1_000);
    let (mut cc, ops) = ch.close_case(t4 - 500);
    cc.lock_time = t4;
    let at = t4.max(vc.lock_time + 10) + 10;
    ch.close(&cc, ops, at);
    let d4 = ch.anchor_d() as u64;
    assert_eq!(d4, at + SEED_OFFSET as u64);
    draw_on(&mut ch, line_sim(d4).0, d4 + DRAW_WINDOW as u64 + 1_000);
    assert_eq!((ch.drawer.1.round, ch.vault.1.round, ch.controller.1.round), (5, 4, 5));
}

// ---------------------------------------------------------------- SP9

/// The A5 bound on bs(P_pov) - bs(X) at the void lock, in the worst case the spec's own
/// facts allow: lock_time = D + draw_window and consensus accepts a DAA lock only if
/// lock_time < daa(C) (`tx_validation_in_header_context.rs:61-70`), so daa(C) >= D + 2F + 1;
/// daa(X) <= daa(P) + M <= D - 1 + M; bs(C) - bs(X) >= daa(C) - daa(X) - reds;
/// bs(P_pov) >= bs(C) - M.
fn worst_gap(f: i64, m: i64, reds: i64) -> i64 {
    let d = 1_000_000i64;
    let daa_c = d + 2 * f + 1;
    let daa_x = d - 1 + m;
    let bs_x = 0i64;
    let bs_c = bs_x + (daa_c - daa_x) - reds;
    let bs_pov = bs_c - m;
    bs_pov - bs_x
}

/// SP9 (spec M7 A5): for F = 432,000, M = 248 and every reds in [0, F - 2M], the void lock
/// implies X.blue_score + F <= P_pov.blue_score (the accessor refuses X: `seq_commit_accessor.rs:
/// 8-10, 36-44`, admitted iff bs + F > pov). The exact threshold with the spec's bounds is
/// reds = F - 2M + 2 (worst gap F); the spec's "at reds = F - 2M + 1 it does not" is off by
/// two (it rounded `daa(C) > D + 2F` and `daa(X) <= D - 1 + M` loosely; PENDING S1b-3): the
/// implication still holds at F - 2M + 1 and F - 2M + 2 and first fails at F - 2M + 3.
#[test]
fn sp9_drift_inequality() {
    let (f, m) = (harness::chain::default_finality_depth() as i64, 248i64);
    assert_eq!((f, DRAW_WINDOW, VOID_MIN), (432_000, 2 * 432_000, 2 * 432_000));
    let refused = |reds: i64| worst_gap(f, m, reds) >= f;
    for reds in 0..=f - 2 * m {
        assert!(refused(reds), "reds = {reds}: gap {} < F", worst_gap(f, m, reds));
    }
    assert_eq!(f - 2 * m, 431_504, "spec D97: F - 2 MERGESET_MAX");
    assert!(refused(f - 2 * m + 1) && refused(f - 2 * m + 2), "the spec's bound is conservative by two");
    assert!(!refused(f - 2 * m + 3), "the implication fails from F - 2M + 3");
    assert_eq!(worst_gap(f, m, 0), 2 * f + 2 - 2 * m);
}

// ---------------------------------------------------------------- SP10

/// SP10 (spec M7 D99): the compiled marker has exactly one entry, `draw_leg`; a `refresh()`
/// call fails at dispatch; the marker state has no `seed_ref` (Rust and compiled layout).
#[test]
fn sp10_marker_draw_leg_only() {
    assert!(!DrawCase::new(4).marker.values().contains_key("seed_ref"), "Rust MarkerState has no seed_ref");
    let t = tpl();
    let c = t.marker.art.contracts.values().next().unwrap();
    let entries: Vec<&String> = c.entries.keys().collect();
    assert_eq!(entries, vec!["draw_leg"], "marker entries");
    let fields: Vec<&str> = c.runtime_state.fields.iter().map(|f| f.name.as_str()).collect();
    assert!(!fields.contains(&"seed_ref"), "compiled marker state: {fields:?}");
    // refresh() in a valid draw (input 1) fails at dispatch; draw_leg passes.
    let dc = sa_base();
    let good = dc.build();
    let mut bad: Built = good.clone();
    match &mut bad.inputs[1].kind {
        InKind::Entry { entry, args, .. } => {
            // The full signature: refresh() is no longer in the ABI, so it is encoded by hand
            // (`planned_sig_script`) as the pre-M7 dispatch would have read it.
            *entry = "refresh()".into();
            args.clear();
        }
        _ => unreachable!(),
    }
    let e = bad.finish().run_input(1).expect_err("marker refresh() accepted");
    assert!(matches!(e, TxScriptError::EarlyReturn), "{e:?}");
    good.finish().run_input(1).expect("draw_leg passes");
}

// ---------------------------------------------------------------- SP11

/// SP11 (lottery side): the 32-byte keys are the spec's strings zero-padded (spec M7 A2), and
/// `lottery::anchor::seq_commit` (the builder's and tn10's) equals `ChainSim`'s commitment for
/// every block of a chain with a jump, an equal-DAA block and a side block. The pins against
/// `kaspa_hashes` and `kaspa_seq_commit` themselves are `harness/tests/m7_chain.rs`
/// (`sp11_k_seq_pins_seq_commit_merkle_branch`, `sp11_k_mc_pins_seq_commit_mergeset_context`,
/// `sp11_seq_commit_equals_kaspa_seq_commit`).
#[test]
fn sp11_seq_commit_pins() {
    let mut k_seq = [0u8; 32];
    k_seq[..29].copy_from_slice(b"SeqCommitmentMerkleBranchHash");
    let mut k_mc = [0u8; 32];
    k_mc[..24].copy_from_slice(b"SeqCommitMergesetContext");
    assert_eq!((K_SEQ, K_MC), (k_seq, k_mc));
    let mut s = ChainSim::line(1_000, B0, 5).with_jump(248).with_jump(0).with_extend(3);
    let tip = s.tip();
    let side = s.add_side_block(tip, 1_300);
    let mut blocks = s.chain_blocks();
    blocks.push(*s.block(side).unwrap());
    for b in blocks {
        let f = BlockFields::from_sim(&s, &b);
        assert_eq!(f.seq_commit(), s.commit_of(b.hash).unwrap().as_bytes(), "block at DAA {}", b.daa_score);
        assert_eq!(BlockFields::from_bytes(&f.to_bytes()).unwrap().seq_commit(), f.seq_commit());
    }
    let d = 1_006;
    let a = AnchorProof::from_sim(&s, d);
    assert!(a.consistent_with(d as i64));
    assert_eq!(a.to_bytes(), s.anchor_proof(d));
}

// ---------------------------------------------------------------- SP12

/// SP12 (spec M7 D101, DP22 re-target): `--no-default-features --features mainnet,test-seed`
/// fails because `test-seed` is not a feature of the crate, `--features mainnet` builds.
/// Shells out to the same cargo with a separate target directory (`crates/target/sp12`) so it
/// does not wait on this test run's build lock; the first run checks the dependencies once
/// (minutes), later runs are fast.
#[test]
fn sp12_test_seed_feature_is_gone() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml");
    let target = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/sp12");
    let check = |features: &str| {
        std::process::Command::new(env!("CARGO"))
            .args(["check", "-p", "lottery", "--no-default-features", "--features", features, "--manifest-path", manifest, "--target-dir", target])
            .output()
            .expect("run cargo")
    };
    let gated = check("mainnet,test-seed");
    let err = String::from_utf8_lossy(&gated.stderr);
    assert!(!gated.status.success(), "mainnet + test-seed must not build");
    assert!(err.contains("does not contain this feature: test-seed"), "cargo 1.94.0 wording missing:\n{err}");
    let main = check("mainnet");
    assert!(main.status.success(), "mainnet must build:\n{}", String::from_utf8_lossy(&main.stderr));
}

/// Every file under `dir` (recursive) containing one of `needles`: (path, line number, line).
fn grep(dir: &std::path::Path, needles: &[&str]) -> Vec<String> {
    let mut hits = vec![];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            stack.extend(std::fs::read_dir(&p).unwrap().filter_map(|e| e.ok()).map(|e| e.path()));
        } else if let Ok(s) = std::fs::read_to_string(&p) {
            for (i, l) in s.lines().enumerate() {
                if needles.iter().any(|n| l.contains(n)) {
                    hits.push(format!("{}:{}: {}", p.display(), i + 1, l.trim()));
                }
            }
        }
    }
    hits
}

/// SP12 (spec M7 D101): no `draw_test_seed` or `test_only_seed` in `crates/lottery/src` or
/// `covenants/` (code and comments).
#[test]
fn sp12_no_test_only_symbols() {
    let needles = ["draw_test_seed", "test_only_seed"];
    let src = grep(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")), &needles);
    assert!(src.is_empty(), "crates/lottery/src:\n{}", src.join("\n"));
    let cov = grep(harness::covenant_path("drawer.sil").parent().unwrap(), &needles);
    assert!(cov.is_empty(), "covenants/:\n{}", cov.join("\n"));
}

// ---------------------------------------------------------------- SP13

/// SP13 (M7-a worst case): K = 16, m = M_DRAW, A_MAX absorbed pools, every lower tier funded
/// (np = 7, the M3 S4-1 worst; and np = 5 as the spec states it), F_MAX funding inputs,
/// worst-case proofs (every leaf at the winning rank non-empty), the record (cap binding), with
/// the 304-byte anchor: within 90 % of compute, transient and storage.
#[test]
fn sp13_worst_draw_within_90() {
    for np in [7, 5] {
        let c = with_pending(dp21(16, M_DRAW, A_MAX, F_MAX, np), Pending::Cap);
        assert_eq!(c.anchor_bytes().len(), harness::ANCHOR_LEN);
        let f = accept_draw5("SP13 worst draw", &c, &c.build());
        let m = masses(&f.tx, &f.entries);
        let (pc, pt, ps) = pct(&m);
        println!("SP13 worst draw np = {np}: compute {} ({pc:.1} %), transient {} ({pt:.1} %), storage {} ({ps:.1} %)", m.compute, m.transient, m.storage);
        assert!(within(&m, MARGIN_PCT).iter().all(|&b| b), "SP13 (D104 STOP): worst draw np = {np} above 90 %: {m:?}");
    }
}
