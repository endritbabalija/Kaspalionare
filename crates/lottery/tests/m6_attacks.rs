//! M6 attacks LA1-LA5 (spec M6 A8; LA6 is the unit test
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

/// M6 Stage 2 (S2-1): the two M5 attacks rebased for W4b (`common::m6::m5_rebased`) are
/// struck at their M5 checks (W4, W1).
#[test]
fn m5_attacks_rebased_for_w4b() {
    run5(m5_rebased());
}
