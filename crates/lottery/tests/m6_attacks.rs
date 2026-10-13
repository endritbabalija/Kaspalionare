//! M6 attacks LA1-LA5 and LA7 (spec M6 A8; LA7 from the Stage 5 review RV-1; LA6 is the unit test
//! `genesis::tests::la6_genesis_below_the_floors`).
//!
//! As M5: full transactions in the v2.1.0 engine with production flags and committed
//! budgets; one change per attack; `strike`: the named input rejects the bad transaction
//! with a script error and passes the good one; the good transaction is then accepted by
//! every input. The attacks are data in `common::m6` (`Atk5`), so Stage 2's m6_mutants.rs
//! re-runs LA2 under the vault without the W4b `require`. `M5_ATTRIB=1` names the script's
//! first failing check (`vault:W4b` once the Stage 2 marker exists); `M5_PREFLIGHT=1` checks
//! each attack against the spec model (`common::m5_spec`, which gains W4b in Stage 2).
//!
//! M6 Stage 1 (tests first, scripts untouched): LA1, LA2, LA3 and LA4 stop at "ATTACK
//! ACCEPTED" (the vault has no W4b and MIN_DEPOSIT = 1 KAS in vault.sil and request.sil);
//! LA5 already passes (the old drawer guard ceil(D / 10^8) is stricter than the new one).
//! M6 Stage 2 (scripts with W4b and MIN_DEPOSIT = 10 KAS): all pass; plus the two M5
//! attacks rebased for W4b (`m5_rebased`, Stage 2 S2-1).
//! M6 Stage 5 fix (review RV-1, orchestrator decision O-M6-2): LA7 (a partial withdrawal
//! leaving fewer than wmin shares) and the reviewer's two-step dust split; both stopped at
//! "ATTACK ACCEPTED" by input 0 before the W4b remainder rule, and pass with it.

mod common;

use common::m6::*;
use common::m5::run5;

#[test]
fn la1_deposit_below_min_deposit() {
    run5(la1());
}

#[test]
fn la2_partial_withdrawal_below_wmin() {
    run5(la2());
}

/// LA3: W4b at total_assets = ASSETS_MAX, total_shares = SHARES_MAX (R8): the decisions are
/// `game::w_min`'s and no rejection is an engine arithmetic error.
#[test]
fn la3_w4b_range_edges() {
    la3_no_arith();
    run5(la3());
}

#[test]
fn la4_settle_deposit_below_min_deposit() {
    run5(la4());
}

#[test]
fn la5_draw_conversion_below_the_guard() {
    run5(la5());
}

/// LA7 (Stage 5 RV-1, O-M6-2): a partial withdrawal whose remainder is below wmin is
/// rejected at W4b (remainder 1 from wmin + 1; remainder wmin - 1 with w >= wmin, small and
/// large w).
#[test]
fn la7_partial_withdrawal_remainder_below_wmin() {
    run5(la7());
}

/// The reviewer's `rv_dust_split` (Stage 5 RV-1) as a chain: step 1 (w = wmin from a
/// position of wmin + 1 shares) must be rejected by the vault; the 1-share position' it
/// would have created can then never exist through W4b. Step 2 of the review (the whole
/// withdrawal of a 1-share position) stays accepted: no lock-in, a whole position of any
/// size can leave (O-M6-2).
#[test]
fn la7_dust_split_two_steps() {
    use common::m2::accept_m4;
    use common::m5::wd;
    use lottery::vault::owner;
    use lottery::game::w_min;
    // Step 1: rejected at the vault input.
    let mut c1 = wd(owner());
    let wm = w_min(c1.vault.total_assets, c1.vault.total_shares);
    c1.position.shares = wm + 1;
    c1.w = wm;
    let f1 = c1.build().finish();
    let r1 = f1.run_input(0);
    assert!(r1.is_err(), "ATTACK ACCEPTED (rv_dust_split step 1: w = wmin = {wm} from wmin + 1 shares leaves a 1-share position') by input 0");
    assert!(!common::m2::is_engine_arithmetic(r1.as_ref().unwrap_err()), "step 1 rejected by an arithmetic error: {r1:?}");
    // Step 2 (the 1-share whole withdrawal the split would have enabled) is still a valid
    // whole withdrawal on its own: the whole-position clause has no minimum.
    let mut c2 = c1.clone();
    c2.vault = c1.expected().vault.clone();
    c2.position.shares = 1;
    c2.w = 1;
    c2.at_age(c1.lock_time + 10);
    accept_m4("rv_dust_split step 2: whole 1-share position", &c2.build());
}

/// M6 Stage 2 (S2-1): the two M5 attacks rebased for W4b (`common::m6::m5_rebased`) are
/// struck at their M5 checks (W4, W1).
#[test]
fn m5_attacks_rebased_for_w4b() {
    run5(m5_rebased());
}
