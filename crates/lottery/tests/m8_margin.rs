//! M8 Part 1 (spec M8 A0, D109): the whale-margin floor MARGIN_MIN = 15,000 bp (1.5).
//!
//! - MF1 `close()`: a controller with margin 14,999 is refused by the controller (input 0);
//!   15,000 is accepted. Which `require` refuses it is shown by MF4 (the M2 controller has no
//!   truncation-attribution instrument): without the line tagged `C2_margin_min` the same
//!   14,999 close is accepted by input 0.
//! - MF2 `draw()`: ctrl.margin 14,999 is refused by the drawer (input 0); 15,000 is accepted.
//!   The A4 check is named by the M3 truncation instrument (`first_failing_check`: check 3,
//!   the controller ranges), and the exact line by MF4 (`D_margin_min`; also killed in
//!   `m3_mutants.rs` `d_margin_min`, which keeps the M3 tag pin).
//! - MF3 genesis `validate()`: 14,999 / 15,000 / 20,000 / 20,001 → refused / accepted /
//!   accepted / refused.
//! - MF4 negative controls (the M2 suite has no tag pin, so `C2_margin_min` is covered here).
//! - MF5 `fix_jackpot` reaches EV = q where the whale check binds, at margins 1.17 and 1.5.

mod common;

use common::accept;
use common::m2::*;
use common::m3::{Atk, ctrl_margin_below_floor};
use lottery::buy::{Built, KAS};
use lottery::close::CloseCase;
use lottery::game::{BP, JackpotRule, MARGIN_MIN, N, Shares, TOTAL_MAX, fix_jackpot, fix_jackpot_brute, whale_allowance, witness_ok};
use lottery::genesis::{GENESIS_FUNDING, GenesisCase};
use lottery::ledger::Ledger;
use lottery::state::ControllerState;

#[test]
fn mf0_constant() {
    assert_eq!(MARGIN_MIN, 15_000);
    assert_eq!(BP, 10_000);
}

// ---------------------------------------------------------------- MF1 close()

fn close_at(margin: i64) -> CloseCase {
    let mut c = close_base(&tpl());
    c.controller.margin = margin;
    c
}

#[test]
fn mf1_close_margin_floor() {
    let good = close_at(15_000);
    let bad = close_at(14_999);
    // The 14,999 close is otherwise consistent (the builder recomputes q and m' from it).
    attack_pair(&good.build(), &bad.build(), 0);
    // The upper bound 2 x BP is unchanged.
    accept(&close_at(20_000).build());
    attack_pair(&good.build(), &close_at(20_001).build(), 0);
}

// ---------------------------------------------------------------- MF2 draw()

#[test]
fn mf2_draw_margin_floor() {
    let atk = ctrl_margin_below_floor();
    assert_eq!(atk.good.controller.margin, 15_000);
    // Refused by the drawer (input 0) at 14,999; the 15,000 base is accepted by every input.
    atk.strike();
    // Attribution (M3 truncation instrument): the first A4 check that rejects it is check 3
    // (the controller's M2 A3 ranges, where the D_margin_min line sits).
    let (_, bad) = atk.pair(&tpl());
    let label = first_failing_check(&bad);
    println!("MF2 attribution: 14,999 refused first at drawer check {label}");
    assert_eq!(label, "3");
}

// ---------------------------------------------------------------- MF3 genesis validate()

fn genesis_at(margin: i64) -> GenesisCase {
    let mut l = Ledger::new();
    let op = l.fund(GENESIS_FUNDING, lottery::buy::funding_spk(), 100);
    let e = l.get(&op).unwrap().clone();
    let mut g = GenesisCase::new(4, vec![(op, e)]);
    g.controller.margin = margin;
    g
}

#[test]
fn mf3_genesis_margin_range() {
    for (margin, ok) in [(14_999, false), (15_000, true), (20_000, true), (20_001, false)] {
        let r = genesis_at(margin).validate();
        println!("MF3 margin {margin}: {r:?}");
        match (ok, r) {
            (true, Ok(())) => {}
            (false, Err(e)) => assert!(e.contains("margin"), "margin {margin}: refused for another reason: {e}"),
            (want, got) => panic!("margin {margin}: expected ok = {want}, got {got:?}"),
        }
    }
}

// ---------------------------------------------------------------- MF4 negative controls

fn survives(b: &Built, input: usize, label: &str) {
    if let Err(e) = b.finish().run_input(input) {
        panic!("mutant {label}: input {input} still rejects the 14,999 strike: {e:?}");
    }
}

/// `close()` without the `C2_margin_min` line accepts the 14,999 close at input 0 (and the
/// 15,000 base); with the committed controller it is refused (MF1).
#[test]
fn mf4_close_without_floor_accepts_14999() {
    attack_pair(&close_at(15_000).build(), &close_at(14_999).build(), 0);
    let mt = mutant(Sil::Controller, &["C2_margin_min"]);
    let at = |margin: i64| {
        let mut c = close_base(&mt);
        c.controller.margin = margin;
        c.build_with(mt.clone())
    };
    accept(&at(15_000));
    survives(&at(14_999), 0, "Controller [C2_margin_min]");
}

/// `draw()` without the `D_margin_min` line accepts the 14,999 draw at input 0 (and the
/// 15,000 base); with the committed drawer it is refused (MF2).
#[test]
fn mf4_draw_without_floor_accepts_14999() {
    let atk: Atk = ctrl_margin_below_floor();
    atk.strike();
    let mt = mutant(Sil::Drawer, &["D_margin_min"]);
    let (g, b) = atk.pair(&mt);
    accept_draw(&g);
    survives(&b, atk.input, "Drawer [D_margin_min]");
}

// ---------------------------------------------------------------- MF5 fix_jackpot reaches EV = q

/// The round r + 1 controller draw() fixes J for: the fixture shares (s_F 5 %, s_R 10 %,
/// s_C = s_D = 0.5 %, s_t 10 %), e 15 %, j 10 %, g 1 %, cap 20 % (`CloseCase` fixture), with
/// the given price, m' and margin.
fn rule_ctrl(price: i64, m: i64, margin: i64) -> ControllerState {
    let mut c = CloseCase::new(4).controller;
    c.price = price;
    c.m = m;
    c.margin = margin;
    c
}

/// Which of (edge, whale, cap) fail at (x, y), exactly (i128), as m3_attacks.rs `failing`.
fn failing(rule: &JackpotRule, x: i64, y: i64) -> [bool; 3] {
    let (x, y) = (x as i128, y as i128);
    let l = (x + y) * rule.g as i128 / BP as i128 * (rule.m as i128 - 1);
    [x + l > rule.e_lim as i128, x + y + l > rule.w_lim as i128, x + l > rule.cv as i128]
}

/// MF5 at real magnitudes (price 1 KAS). Grid (deterministic, exhaustive over the listed
/// values, no sampling): m' in {2, 10, 50}; V' and O' each in {0} ∪ {10^k KAS : k = 0..10}
/// (10^10 KAS < TOTAL_MAX); margins 1.17 and 1.5. This is the D100-amended setting (fixture
/// shares, p = 1 KAS, a vault and a boost pot over many magnitudes; its worked point V' =
/// 1,000,000 KAS, m' = 10, a large pot is on the grid).
///
/// At every point where neither the vault edge nor the cap binds (one more sompi of J_house
/// fails the whale check alone): (x2, y) passes, one more sompi of x or of y fails `pass`,
/// the A5 witness checks accept (x1, y, x2) and reject x2 + 1, `pass` agrees with the exact
/// i128 inequalities there, and no larger x2 or y passes within 2,000 sompi (a local brute
/// scan: `fix_jackpot_brute` itself scans linearly and cannot run at 10^14-sompi magnitudes;
/// it is run exactly on the scaled copy below). The jackpot money J' + G2' (m' - 1) is then
/// within m' sompi below W = q N m', i.e. per ticket within 1/N sompi below q, where
/// q = floor(p w / margin).
#[test]
fn mf5_fix_jackpot_reaches_q_real_magnitudes() {
    let mags: Vec<i64> = std::iter::once(0).chain((0..=10).map(|k| 10i64.pow(k) * KAS as i64)).collect();
    assert!(*mags.last().unwrap() <= TOTAL_MAX);
    for margin in [11_700, MARGIN_MIN] {
        let mut whale_points = 0;
        let mut other_points = 0;
        let mut worst_gap = 0i64; // max W - money, sompi
        let mut pattern = std::collections::BTreeMap::new();
        for m in [2, 10, 50] {
            let c = rule_ctrl(KAS as i64, m, margin);
            let w = Shares::of(&c).w();
            let q = whale_allowance(c.price, w, margin);
            // q is p w / margin rounded down: less than one sompi below the exact value.
            assert!(q * margin <= c.price * w && c.price * w < (q + 1) * margin);
            for &v in &mags {
                let rule = JackpotRule::new(&c, v);
                assert_eq!(rule.w_lim, q * N * m);
                for &o in &mags {
                    let f = fix_jackpot(&rule, v, o, c.j);
                    assert!(witness_ok(&rule, v, o, c.j, f.x1, f.y, f.x2));
                    assert!(rule.pass(f.x2, f.y) && rule.pass_exact(f.x2, f.y));
                    let fl = failing(&rule, f.x2 + 1, f.y);
                    *pattern.entry((fl, f.y == o, f.x1 < f.x_b)).or_insert(0) += 1;
                    if fl != [false, true, false] {
                        other_points += 1;
                        continue;
                    }
                    whale_points += 1;
                    // The largest the whale check allows: one more sompi of x or y fails.
                    assert!(!rule.pass(f.x2 + 1, f.y) && !rule.pass_exact(f.x2 + 1, f.y), "x2 + 1 passes: {f:?} {rule:?}");
                    assert!(!rule.pass(f.x2, f.y + 1) && !rule.pass_exact(f.x2, f.y + 1), "y + 1 passes: {f:?} {rule:?}");
                    assert!(!witness_ok(&rule, v, o, c.j, f.x1, f.y, f.x2 + 1));
                    for d in 1..2_000 {
                        assert!(!rule.pass(f.x2 + d, f.y) && !rule.pass(f.x2, f.y + d), "local scan d = {d}: {f:?}");
                    }
                    // EV = q: the money J' + G2' (m' - 1) is in (W - m', W].
                    let money = f.jackpot + f.g2 * (m - 1);
                    assert_eq!(money, f.liability + f.j_boost);
                    let gap = rule.w_lim - money;
                    assert!((0..m).contains(&gap), "margin {margin} m {m} V' {v} O' {o}: W - money = {gap} not in [0, m'): {f:?}");
                    worst_gap = worst_gap.max(gap);
                }
            }
            if m == 10 && margin == MARGIN_MIN {
                // The D100-amended worked point: V' = 1,000,000 KAS, m' = 10, a large pot.
                let v = 1_000_000 * KAS as i64;
                let o = 10_000_000_000 * KAS as i64;
                let rule = JackpotRule::new(&c, v);
                let f = fix_jackpot(&rule, v, o, c.j);
                assert_eq!(failing(&rule, f.x2 + 1, f.y), [false, true, false]);
                let money = f.jackpot + f.g2 * (m - 1);
                println!("MF5 worked point margin 1.5: V' 1,000,000 KAS, m' 10: J_house {} J_boost {} G2 {} money {} W {} (q = {q} sompi = {:.4} p)", f.x2, f.y, f.g2, money, rule.w_lim, q as f64 / c.price as f64);
            }
        }
        println!("MF5 margin {:.2}: {whale_points} whale-binding points (EV = q, worst W - money = {worst_gap} sompi), {other_points} points where the edge rule, the cap or the base binds", margin as f64 / BP as f64);
        println!("MF5 margin {margin} patterns ([edge, whale, cap] at x2 + 1, y = O', base lowered): {pattern:?}");
        // Non-vacuity (counts measured 2026-10-13: 16 at 1.17, 154 at 1.5 of 432 points).
        assert!(whale_points >= if margin == MARGIN_MIN { 100 } else { 10 }, "margin {margin}: the grid must reach the whale regime ({whale_points} points)");
        // At 1.5 (fixture shares, e 15 %) q = 0.5267 p < a = 0.629 p, so the edge rule binds
        // nowhere on the grid: the whale check binds wherever the cap does not (D108).
        if margin == MARGIN_MIN {
            assert!(pattern.keys().all(|(fl, _, _)| !fl[0]), "edge binds at 1.5: {pattern:?}");
        }
    }
}

/// MF5, scaled copy for the exact brute force: the same controller with price 3 sompi (so
/// the limits are about N m' sompi and `fix_jackpot_brute`'s linear scans finish in a debug
/// build); m' in {2, 10}; V' in {0, 10^4, 10^6, 10^7} sompi (j 10 %, cap 20 %); O' in
/// {0, 10^4, 3 x 10^5, 2 x 10^6} sompi; margins 1.17 and 1.5. Exhaustive over these values.
/// `fix_jackpot` (bisection) equals `fix_jackpot_brute` everywhere; where neither the edge
/// rule nor the cap binds, one more sompi of x or y fails and J' + G2' (m' - 1) is in
/// (W - m', W].
#[test]
fn mf5_fix_jackpot_matches_brute_scaled() {
    for margin in [11_700, MARGIN_MIN] {
        let mut whale_points = 0;
        for m in [2, 10] {
            let c = rule_ctrl(3, m, margin);
            for v in [0, 10_000, 1_000_000, 10_000_000] {
                let rule = JackpotRule::new(&c, v);
                for o in [0, 10_000, 300_000, 2_000_000] {
                    let f = fix_jackpot(&rule, v, o, c.j);
                    assert_eq!(f, fix_jackpot_brute(&rule, v, o, c.j), "margin {margin} m {m} V' {v} O' {o} {rule:?}");
                    if failing(&rule, f.x2 + 1, f.y) == [false, true, false] {
                        whale_points += 1;
                        assert!(!rule.pass(f.x2 + 1, f.y) && !rule.pass(f.x2, f.y + 1));
                        let gap = rule.w_lim - (f.jackpot + f.g2 * (m - 1));
                        assert!((0..m).contains(&gap), "gap {gap}");
                    }
                }
            }
        }
        println!("MF5 scaled (price 3 sompi) margin {:.2}: bisection = brute on 32 points, {whale_points} whale-binding", margin as f64 / BP as f64);
        assert!(whale_points >= 2, "margin {margin}: whale regime reached {whale_points} times");
    }
}

/// The per-ticket jackpot money at the whale bound, as a share of the price (D108: about
/// 52.7 % at 1.5 against 67.5 % at 1.17 for w = 79 %).
#[test]
fn mf5_q_share_of_price() {
    let w = Shares::TEST.w();
    assert_eq!(w, 7_900);
    for (margin, want) in [(11_700, 0.6752), (MARGIN_MIN, 0.5267)] {
        let q = whale_allowance(KAS as i64, w, margin) as f64 / KAS as f64;
        println!("MF5 q / p at margin {:.2}: {q:.4}", margin as f64 / BP as f64);
        assert!((q - want).abs() < 5e-5);
    }
}
