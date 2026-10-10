//! M3 mutation tests (spec M3 A8 "Mutants", negative controls as M2). For every
//! `// mut:<ID>` tag in drawer.sil and in the M3 legs (marker, vault, pool, controller
//! `draw_leg`): with the committed sources the named input rejects the attack and accepts
//! the valid draw; with only the lines tagged `<ID>` removed from one covenant (compiled in
//! memory, the sources on disk are untouched), the named input ACCEPTS the attack. The
//! attack is the strike of the m3_attacks.rs test named in each comment, so that test fails
//! under the mutant (the mutant is killed). Other inputs may still reject (layered checks);
//! the claim is about the named input only.
//!
//! Mutants of an assignment line (D10_n2, D14_jp, D14_sp, D15_jboost, D15_lower, D15_q,
//! D19_max, and the weakened D16) change what the script computes: under them the valid
//! draw is rejected (every positive test fails) and the attack is accepted
//! (`killed_assign`).
//!
//! Two kinds of exception, each a test of its own:
//! - `weakened_*`: D16 tags a declaration (`bool paid = ...`), which cannot be removed; the
//!   mutant replaces its right-hand side by a constant instead.
//! - `weakened_d12_*` (Stage 5): D12_jp / D12_sp tag the pool-existence declarations; the
//!   mutant drops the `J > 0` / `G2 > 0` half, and the valid DP24 draw (a zero prize held)
//!   is rejected by the drawer (`killed_positive`).
//! - `redundant_*`: tags whose check is implied by other checks of the same script (argument
//!   in docs/specs/3-handoff.md, Stage 4). The test shows the mutant still rejects the
//!   attack at the named input, so a later change that removes the implying check is noticed.
//!
//! Attacks are built in `common::m3` (`Atk`), shared with m3_attacks.rs.

mod common;

use std::sync::Arc;

use common::m2::*;
use common::m3::*;
use lottery::buy::{Built, Sources, Templates, b_max};

/// The attack is rejected with the committed templates, and accepted by its input under
/// the mutant without `ids` in `which`.
fn killed(which: Sil, ids: &[&str], atk: Atk) {
    killed_under(mutant(which, ids), &format!("{which:?} {ids:?}"), atk);
}

fn killed_under(t: Arc<Templates>, label: &str, atk: Atk) {
    atk.strike();
    let (g, b) = atk.pair(&t);
    accept_draw(&g);
    survives(&b, atk.input, label);
}

fn survives(b: &Built, input: usize, label: &str) {
    if let Err(e) = b.finish().run_input(input) {
        panic!("mutant {label}: input {input} still rejects the attack: {e:?}");
    }
}

/// A mutant of an assignment (not a `require`): it changes what the script computes, so
/// the valid draw is rejected by the named input (the positive tests fail), and the attack,
/// which matches the mutant's computation, is accepted by it.
fn killed_assign(which: Sil, ids: &[&str], atk: Atk) {
    killed_assign_under(mutant(which, ids), &format!("{which:?} {ids:?}"), atk);
}

fn killed_assign_under(t: Arc<Templates>, label: &str, atk: Atk) {
    atk.strike();
    let (g, b) = atk.pair(&t);
    reject_at(&g, atk.input);
    survives(&b, atk.input, label);
}

/// Redundant check: the mutant still rejects the attack at its input (another check of the
/// same script implies the removed one).
fn redundant(which: Sil, ids: &[&str], atk: Atk) {
    atk.strike();
    let t = mutant(which, ids);
    let (g, b) = atk.pair(&t);
    accept_draw(&g);
    reject_at(&b, atk.input);
}

/// The drawer with the line tagged `id` replaced by `line` (a weakened check).
fn drawer_replaced(id: &str, line: &str) -> Arc<Templates> {
    let mut src = Sources::committed(b_max());
    let tag = format!("// mut:{id}");
    let n = src.drawer.lines().filter(|l| l.contains(&tag)).count();
    assert_eq!(n, 1, "tag {tag} must mark exactly one line");
    src.drawer = src.drawer.lines().map(|l| if l.contains(&tag) { line } else { l }).collect::<Vec<_>>().join("\n");
    let t = Templates::compile_sources(b_max(), &src);
    assert_ne!(t.drawer.hash, tpl().drawer.hash);
    Arc::new(t)
}

const D: Sil = Sil::Drawer;

/// A mutant that only changes what counts as a valid draw (Stage 5, D12_jp / D12_sp): the
/// committed templates accept `good` (a positive test), the mutant rejects it at the
/// drawer, so that positive test fails under the mutant.
fn killed_positive(t: Arc<Templates>, label: &str, good: lottery::draw::DrawCase) {
    accept_draw(&good.build());
    let g = retarget(good, &t).build_with(t);
    let e = reject_at(&g, DRW);
    println!("mutant {label}: valid draw rejected at input {DRW}: {e:?}");
}

// ================================================================ drawer: check 1 (Stage 5)

#[test]
fn d1_fmin() {
    // DA18 (Stage 5, decision A): drawer state f_min = F_MIN_FLOOR - 1.
    killed(D, &["D1_fmin"], f_min_below_floor());
}

// ================================================================ drawer: check 2

#[test]
fn d2_a() {
    // DA8 (Stage 4): a = -1. The upper half (a <= A_MAX) is backed by the loop guards, see
    // redundant_d2_a_upper.
    killed(D, &["D2_a"], a_negative());
}

#[test]
fn redundant_d2_a_upper() {
    // a = 6: `for(ci, 1, 4 + a, 3 + A_MAX)` and the other A_MAX-bounded loops reject
    // end - start > max_iterations (silverscript-lang compiler/for.rs:136-150).
    redundant(D, &["D2_a"], a_above_max());
}

#[test]
fn d2_fund() {
    // DA8 (Stage 4): no funding input.
    killed(D, &["D2_fund"], no_funding());
}

#[test]
fn redundant_d2_fund_upper() {
    // 9 funding inputs: the funding loop is bounded by F_MAX.
    redundant(D, &["D2_fund"], nine_funding());
}

#[test]
fn redundant_d2_cov() {
    // Implied by a := OpCovInputCount(cov) - 4 and D2_zero (pigeonhole: 4 + a inputs carry
    // cov, none at index >= 4 + a, so all of 0..3 + a do).
    redundant(D, &["D2_cov"], marker_other_cov());
}

#[test]
fn d2_zero() {
    // DA8: a funding input with another covenant ID.
    killed(D, &["D2_zero"], funding_other_cov());
}

// ================================================================ drawer: checks 3-6

#[test]
fn d3_role() {
    killed(D, &["D3_role"], ctrl_role()); // DA10 (Stage 4)
}

#[test]
fn d3_round() {
    killed(D, &["D3_round"], ctrl_round()); // DA10
}

#[test]
fn d3_drawn() {
    killed(D, &["D3_drawn"], ctrl_drawn()); // DA10
}

#[test]
fn d3_mdraw() {
    killed(D, &["D3_mdraw"], ctrl_m_above()); // DA10 (Stage 4)
}

#[test]
fn d4_role() {
    killed(D, &["D4_role"], marker_role()); // DA10 (Stage 4)
}

#[test]
fn d4_round() {
    // DA10 (Stage 4): the marker's round field alone (the case-level DA10 strike also moves
    // the seed, which check 8 catches without D4_round).
    killed(D, &["D4_round"], marker_round_field());
}

#[test]
fn d4_k() {
    killed(D, &["D4_k"], marker_k()); // DA10 (Stage 4)
}

#[test]
fn redundant_d4_mdraw() {
    // mk.m = M_DRAW + 1: the check 10 loop `for(b, 0, mk.m, M_DRAW)` rejects it.
    redundant(D, &["D4_mdraw"], marker_m_above());
}

#[test]
fn d4_value() {
    killed(D, &["D4_value"], input_value(MK, 1)); // DA16
}

#[test]
fn d5_role() {
    killed(D, &["D5_role"], vault_role()); // DA10 (Stage 4)
}

#[test]
fn d5_round() {
    killed(D, &["D5_round"], vault_round()); // DA10
}

#[test]
fn d5_value() {
    killed(D, &["D5_value"], input_value(VLT, -1)); // DA16
}

#[test]
fn d6_drawer() {
    killed(D, &["D6_drawer"], input_value(DRW, 1)); // DA16
}

#[test]
fn d6_pot() {
    killed(D, &["D6_pot"], pot_negative()); // DA16 (Stage 4)
}

// ================================================================ drawer: checks 7-10

#[test]
fn d7() {
    killed(D, &["D7"], early_lock()); // DA12
}

#[test]
fn d8_canon() {
    killed(D, &["D8_canon"], win5_noncanonical()); // DA12 (Stage 4)
}

#[test]
fn d8_rank() {
    killed(D, &["D8_rank"], win5_other_rank()); // DA12 (Stage 4)
}

#[test]
fn d9_len() {
    killed(D, &["D9_len"], leaves_longer()); // DA1
}

#[test]
fn d9_root() {
    killed(D, &["D9_root"], forged_leaf()); // DA1 (Stage 4)
}

#[test]
fn d10_range() {
    killed(D, &["D10_range"], count_sign_bit()); // DA2 (Stage 4)
}

#[test]
fn d10_n2() {
    killed_assign(D, &["D10_n2"], n2_zero_claim()); // DA2
}

// ================================================================ drawer: checks 13-16

#[test]
fn d13() {
    killed(D, &["D13"], parallel_auth()); // DA8
}

#[test]
fn d14_jp() {
    killed_assign(D, &["D14_jp"], vault_keeps_jhouse()); // DA3 (Stage 4)
}

#[test]
fn d14_g2guard() {
    killed(D, &["D14_g2guard"], g2_guard()); // a6_g2_guard_with_second_pool
}

#[test]
fn d14_sp() {
    killed_assign(D, &["D14_sp"], vault_keeps_second()); // DA3 (Stage 4)
}

#[test]
fn d14_nonneg() {
    killed(D, &["D14_nonneg"], vault_negative()); // DA3 (Stage 4)
}

#[test]
fn d15_jboost() {
    killed_assign(D, &["D15_jboost"], jboost_to_caller()); // DA7 (Stage 4)
}

#[test]
fn d15_lower() {
    killed_assign(D, &["D15_lower"], lower_to_caller()); // DA7 (Stage 4)
}

#[test]
fn d15_q() {
    killed_assign(D, &["D15_q"], q_to_caller()); // DA6
}

#[test]
fn weakened_d16_always_paid() {
    // DA5: paid although T_F < F_MIN.
    killed_assign_under(drawer_replaced("D16", "        bool paid = true;"), "D16 paid = true", fee_paid_below());
}

#[test]
fn weakened_d16_never_paid() {
    // DA5: carried although T_F >= F_MIN.
    killed_assign_under(drawer_replaced("D16", "        bool paid = false;"), "D16 paid = false", fee_carried_above());
}

// ================================================================ drawer: check 12 (Stage 5)

#[test]
fn weakened_d12_jp() {
    // DP24: J = 0 with h_J >= 1 creates no jackpot pool; `jp = hJ >= 1` demands one.
    killed_positive(drawer_replaced("D12_jp", "        bool jp = hJ >= 1;"), "D12_jp jp = hJ >= 1", zero_jackpot_held());
}

#[test]
fn weakened_d12_sp() {
    // DP24: G2 = 0 with n2 >= 1 creates no second-prize pool; `sp = n2 >= 1` demands one.
    killed_positive(drawer_replaced("D12_sp", "        bool sp = n2 >= 1;"), "D12_sp sp = n2 >= 1", zero_g2_held());
}

// ================================================================ drawer: check 17 (A5)

#[test]
fn d17_y_pass_and_x2_ge_together() {
    // Review item 7: each of D17_y_pass and D17_x2_ge is redundant only because the other
    // is present. With BOTH removed the out-of-order witness (y = O' above the largest
    // passing y, x2 < x1) is accepted: da4_out_of_order_witness is killed. With either
    // one removed the drawer still rejects it.
    redundant(D, &["D17_y_pass"], y_pot_x2_below_x1());
    redundant(D, &["D17_x2_ge"], y_pot_x2_below_x1());
    killed(D, &["D17_y_pass", "D17_x2_ge"], y_pot_x2_below_x1());
}

#[test]
fn d17_x1_base() {
    killed(D, &["D17_x1_base"], x1_below_base()); // DA4 (Stage 4)
}

#[test]
fn redundant_d17_x1_below() {
    // x1 >= x_b in the failing branch: pass is non-increasing in x, so pass(x1, 0) fails
    // (D17_x1_pass).
    redundant(D, &["D17_x1_below"], x1_at_failing_base());
}

#[test]
fn redundant_d17_x1_pass() {
    // pass(x1, y) (D17_y_pass) implies pass(x1, 0): pass is non-increasing in y.
    redundant(D, &["D17_x1_pass"], x1_failing());
}

#[test]
fn d17_x1_max() {
    killed(D, &["D17_x1_max"], x1_not_largest()); // DA4 (Stage 4)
}

#[test]
fn d17_y_pot() {
    killed(D, &["D17_y_pot"], y_above_pot()); // DA4 (Stage 4)
}

#[test]
fn redundant_d17_y_pass() {
    // x2 >= x1 and pass(x2, y) (D17_x2_ge, D17_x2_pass) imply pass(x1, y).
    redundant(D, &["D17_y_pass"], y_failing());
}

#[test]
fn d17_y_max() {
    killed(D, &["D17_y_max"], y_not_largest()); // DA4 (Stage 4)
}

#[test]
fn redundant_d17_x2_ge() {
    // x2 < x1 with pass(x1, y) (D17_y_pass) means pass(x2 + 1, y), so D17_x2_max rejects.
    redundant(D, &["D17_x2_ge"], x2_below_x1());
}

#[test]
fn d17_x2_pass() {
    killed(D, &["D17_x2_pass"], x2_plus_one()); // DA4
}

#[test]
fn d17_x2_max() {
    killed(D, &["D17_x2_max"], x2_minus_one()); // DA4 (Stage 4 regime with x2 > x1)
}

// ================================================================ drawer: checks 18-21

#[test]
fn d18_role() {
    killed(D, &["D18_role"], pool_role()); // DA10 (Stage 4)
}

#[test]
fn d18_round() {
    killed(D, &["D18_round"], pool_round_r()); // DA6
}

#[test]
fn d18_deadline() {
    killed(D, &["D18_deadline"], before_deadline()); // DA6
}

#[test]
fn d18_class() {
    killed(D, &["D18_class"], pool_class3()); // DA9 (Stage 4)
}

#[test]
fn d19_own() {
    killed(D, &["D19_own"], own_lie()); // DA17 (and its negative control)
}

#[test]
fn d19_max() {
    killed_assign(D, &["D19_max"], deadline_without_draw_window()); // DA13 (Stage 4)
}

#[test]
fn d20_count() {
    killed(D, &["D20_count"], extra_plain()); // DA8
}

#[test]
fn d20_drawer() {
    killed(D, &["D20_drawer"], output_value(base(), |_| 0, 1)); // DA14
}

#[test]
fn d20_vault() {
    killed(D, &["D20_vault"], output_value(base(), |_| 1, -1)); // DA3
}

#[test]
fn d20_ctrl() {
    killed(D, &["D20_ctrl"], output_value(base(), |_| 2, 1)); // DA14
}

#[test]
fn d20_pool() {
    // DA13: each pool kind's value line (jackpot, second prize, a lower tier).
    for p in [0, 1, 2] {
        killed(D, &["D20_pool"], output_value(base(), move |x| x.pool_output(p), 1));
    }
}

#[test]
fn d20_fee() {
    killed(D, &["D20_fee"], output_value(base(), |x| x.fee_output().unwrap(), -1)); // DA5
}

#[test]
fn d20_refund() {
    // DA9: the marker refund and the pool refund lines.
    killed(D, &["D20_refund"], output_value(base(), |x| x.marker_refund(), 1));
    killed(D, &["D20_refund"], output_value(base(), |x| x.pool_refund(0), -1));
}

#[test]
fn d21_zero() {
    killed(D, &["D21_zero"], caller_with_cov()); // DA8
}

#[test]
fn d21_fee() {
    killed(D, &["D21_fee"], output_script(|x| x.fee_output().unwrap())); // DA5
}

#[test]
fn d21_mrefund() {
    killed(D, &["D21_mrefund"], output_script(|x| x.marker_refund())); // DA9
}

#[test]
fn d21_prefund() {
    killed(D, &["D21_prefund"], output_script(|x| x.pool_refund(0))); // DA9
}

// ================================================================ drawer: check 25

#[test]
fn d25_class() {
    killed(D, &["D25_class"], new_pool_class3()); // DA13
}

#[test]
fn d25_pad() {
    killed(D, &["D25_pad"], new_pool_padding()); // DA13
}

// ================================================================ marker draw_leg

#[test]
fn ml1() {
    killed(Sil::Marker, &["ML1"], swapped(MK, VLT, VLT)); // DA11: the marker at input 2
}

#[test]
fn ml2() {
    killed(Sil::Marker, &["ML2"], other_leader(MK)); // DA11
}

#[test]
fn ml3() {
    killed(Sil::Marker, &["ML3"], leg_authorizes(MK, 3)); // DA11
}

#[test]
fn ml4_role() {
    killed(Sil::Marker, &["ML4_role"], drawer_role(MK)); // DA11 (Stage 4)
}

#[test]
fn ml4_round() {
    killed(Sil::Marker, &["ML4_round"], marker_round().at(MK)); // DA10
}

// ================================================================ vault draw_leg

#[test]
fn vl1() {
    killed(Sil::Vault, &["VL1"], swapped(MK, VLT, MK)); // DA11: the vault at input 1
}

#[test]
fn vl2() {
    killed(Sil::Vault, &["VL2"], other_leader(VLT)); // DA11
}

#[test]
fn vl3() {
    killed(Sil::Vault, &["VL3"], leg_authorizes(VLT, 1)); // DA11
}

#[test]
fn vl4_role() {
    killed(Sil::Vault, &["VL4_role"], drawer_role(VLT)); // DA11 (Stage 4)
}

#[test]
fn vl4_round() {
    killed(Sil::Vault, &["VL4_round"], vault_round().at(VLT)); // DA10
}

// ================================================================ controller draw_leg

#[test]
fn cl1() {
    killed(Sil::Controller, &["CL1"], swapped(CTL, POOL0, POOL0)); // DA11: the controller at input 4
}

#[test]
fn cl2() {
    killed(Sil::Controller, &["CL2"], other_leader(CTL)); // DA11
}

#[test]
fn cl3() {
    killed(Sil::Controller, &["CL3"], leg_authorizes(CTL, 2)); // DA11
}

#[test]
fn cl4_role() {
    killed(Sil::Controller, &["CL4_role"], marker_role().at(CTL)); // DA11 (Stage 4)
}

#[test]
fn cl4_round() {
    killed(Sil::Controller, &["CL4_round"], marker_round().at(CTL)); // DA10
}

#[test]
fn cl5() {
    killed(Sil::Controller, &["CL5"], ctrl_drawn().at(CTL)); // DA10
}

// ================================================================ pool absorb_leg

#[test]
fn pl1() {
    killed(Sil::Pool, &["PL1"], swapped(CTL, POOL0, CTL)); // DA11: the pool at input 3
}

#[test]
fn pl2() {
    killed(Sil::Pool, &["PL2"], other_leader(POOL0)); // DA11
}

#[test]
fn pl3() {
    killed(Sil::Pool, &["PL3"], leg_authorizes(POOL0, 4)); // DA11
}

#[test]
fn pl4_role() {
    killed(Sil::Pool, &["PL4_role"], drawer_role(POOL0)); // DA11 (Stage 4)
}

#[test]
fn pl4_round() {
    killed(Sil::Pool, &["PL4_round"], pool_round_r().at(POOL0)); // DA6
}

#[test]
fn pl5() {
    killed(Sil::Pool, &["PL5"], before_deadline().at(POOL0)); // DA6
}

// ================================================================ coverage

/// Every `// mut:` tag of the M3 sources has a test above (a new tag must get one).
#[test]
fn every_m3_tag_is_tested() {
    let me = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/m3_mutants.rs")).expect("own source");
    let mut n = 0;
    for f in ["drawer.sil", "marker.sil", "vault.sil", "pool.sil", "controller.sil"] {
        let src = std::fs::read_to_string(harness::covenant_path(f)).expect("source");
        let mut tags: Vec<&str> = src.lines().filter_map(|l| l.split("// mut:").nth(1)).flat_map(|t| t.split(',')).map(str::trim).collect();
        // controller.sil: only the M3 draw_leg tags (CL*); its close() tags are M2's.
        if f == "controller.sil" {
            tags.retain(|t| t.starts_with("CL"));
        }
        tags.sort();
        tags.dedup();
        for t in tags {
            if t.starts_with('<') {
                continue; // the header comment's `// mut:<ID>`
            }
            assert!(me.contains(&format!("\"{t}\"")), "{f}: tag {t} has no mutant test");
            n += 1;
        }
    }
    // 66 drawer tags and 22 leg tags (Stage 4: 63 + 22; Stage 5 added D1_fmin, D12_jp,
    // D12_sp).
    assert_eq!(n, 88, "M3 tag count changed: update this test and the handoff table");
}
