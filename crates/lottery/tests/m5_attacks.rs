//! M5 attacks on the vault entries, the position and record legs, `settle()`, the M5
//! `refresh()`es and the drawer's conversion: VA1-VA15 (spec M5 A10).
//!
//! As M3/M4: full transactions in the v2.1.0 engine with production flags and committed
//! budgets; one change per attack; `strike`: the named input rejects the bad transaction
//! with a script error and passes the good one; the good (base) transaction is then
//! accepted by every input. The attacks are data in `common::m5` (`Atk5`), so Stage 6's
//! m5_mutants.rs re-runs each one under the mutant without its `require`. Range and
//! malformed-state attacks use fabricated UTXOs under the instance covenant ID (D79).
//! Ledger cases (VA4: a second settlement of a spent request) are structural.
//!
//! M5 Stage 2 (tests first): until Stages 3-5 write the scripts, every strike stops at
//! "BASE REJECTED" (the placeholder or the M4 drawer rejects the good transaction).
//! `M5_PREFLIGHT=1` runs every attack against the spec model (`common::m5_spec`) and
//! requires the model to reject it at exactly its named check; `M5_ATTRIB=1` names the
//! script's first failing check once the `// ==== check` markers exist.

mod common;

use common::m2::*;
use common::m5::*;
use harness::TxScriptError;
use lottery::buy::KAS;
use lottery::ledger::LedgerError;

#[test]
fn va1_zero_share_deposit() {
    run5(va1());
}

#[test]
fn va2_cancelling() {
    run5(va2());
}

#[test]
fn va3_wrong_draws_record() {
    run5(va3());
}

#[test]
fn va4_second_settlement() {
    run5(va4());
}

/// VA4 (ledger, structural): after a settlement spends a request, a second settlement of
/// the same outpoint is refused by the UTXO set (input 0 missing). Real chain: genesis,
/// deposit request, buy, close, draw with conversion, settlement.
#[test]
fn va4_second_settlement_ledger() {
    let mut ch = chain5();
    let qi = ch.deposit_now(&lottery::vault::owner().payout(), 10 * KAS as i64, 300_000);
    let (b, ops) = ch.buy_case(0);
    ch.buy(0, &b, ops, 300_500);
    ch.close_now();
    ch.draw_now([1u8; 32]);
    let (c, ops) = ch.settle_case(qi, 2_000_000);
    let spent = ops[0];
    ch.settle(&c, ops, 2_000_000);
    assert!(!ch.ledger.contains(&spent), "the request is spent");
    // The same settlement again from the original request outpoint (fresh funding).
    let mut b2 = c.build_with(ch.t.clone());
    let fop = ch.ledger.fund(10 * KAS, lottery::buy::funding_spk(), 1_999_900);
    let rop = ch.records.first().map(|r| r.0).unwrap_or(spent);
    b2.outpoints = vec![spent, rop, fop];
    let f2 = b2.finish();
    assert_eq!(ch.ledger.entries(&f2.tx), Err(LedgerError::Missing { index: 0, outpoint: spent }));
}

#[test]
fn va5_withdrawal_below_liability() {
    run5(va5());
}

#[test]
fn va6_overflow_boundaries() {
    run5(va6());
}

#[test]
fn va7_stale_or_future_price() {
    run5(va7());
}

#[test]
fn va8_double_conversion() {
    run5(va8());
}

#[test]
fn va9_request_creation() {
    run5(va9());
}

#[test]
fn va10_withdrawal_authorization() {
    run5(va10());
}

#[test]
fn va11_settlement_outputs() {
    run5(va11());
}

#[test]
fn va12_record_leg() {
    run5(va12());
}

#[test]
fn va13_refresh() {
    run5(va13());
}

#[test]
fn va14_j_independence() {
    run5(va14());
}

/// Stage 6: the boundary and shape attacks that isolate one tagged line each for the
/// mutants of m5_mutants.rs (VA6/VA9-VA12 ranges, argument lengths, shapes; engine-only
/// values above MAX_SOMPI as M4 S4-3).
#[test]
fn stage6_boundaries_and_shapes() {
    run5(stage6());
}

/// Stage 7 (review fixes): the vault request age (U-4: D10, W12; age VAULT_REQ_DELAY - 1,
/// no lock time, an unaccepted or out-of-range vault creation) and a drained record that is
/// not empty (F4: S9).
#[test]
fn stage7_review_fixes() {
    run5(stage7());
}

/// VA15: each new entry's leader one budget unit short fails on the budget, after the base
/// is accepted.
#[test]
fn va15_leader_budget_minus_one() {
    for (name, case) in va15_cases() {
        let b = case.build();
        let f = accept5(&format!("{} [VA15 {name}]", case.step()), &case, &b);
        // A leader that fits the free per-input allowance (SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT
        // - 1 = 9,999 script units, rusty-kaspa v2.1.0 consensus/core/src/mass/units.rs:14-24,
        // consensus/core/src/tx.rs:102) commits budget 0, so "budget - 1" does not exist
        // (handoff S5-3). Stated, not skipped silently.
        if harness::budget_for(f.units[0]) == 0 {
            println!("VA15 {name}: committed budget 0 ({} script units <= the free allowance); no budget - 1 exists", f.units[0]);
            continue;
        }
        let mut m = b.clone();
        m.budget_delta = -1;
        let e = match m.finish().run_input(0) {
            Ok(u) => panic!("VA15 {name}: under-budget leader accepted ({u} units)"),
            Err(e) => e,
        };
        if preflight5() && matches!(e, TxScriptError::EarlyReturn | TxScriptError::VerifyError) {
            println!("PREFLIGHT va15 | VA15 {name} | expect budget | bad: {e:?} (script not yet written)");
            continue;
        }
        assert!(matches!(e, TxScriptError::ExceededCommittedScriptUnits { .. }), "VA15 {name}: expected a budget error, got {e:?}");
    }
}

/// Every attack id is unique (m5_mutants.rs looks attacks up by id).
#[test]
fn attack_ids_are_unique() {
    let mut ids: Vec<String> = all_atks5().into_iter().map(|a| a.id).collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate attack ids");
}

/// Every attack changes the transaction and its expected check names a modelled label
/// (a cheap structural check that runs without any script).
#[test]
fn attacks_are_well_formed() {
    let labels = ["vault:D", "vault:W", "position:PO", "position:A7.", "request:S", "request:A7.", "record:RL", "record:A7.", "drawer:"];
    for a in all_atks5() {
        assert!(labels.iter().any(|l| a.check.starts_with(l)), "{}: check {} is not an M5 label", a.id, a.check);
        let (g, b) = a.pair(&tpl());
        let (gf, bf) = (g.finish(), b.finish());
        let sigs = |f: &lottery::buy::Final| f.tx.inputs.iter().map(|i| i.signature_script.clone()).collect::<Vec<_>>();
        assert!(gf.tx.id() != bf.tx.id() || gf.entries != bf.entries || sigs(&gf) != sigs(&bf), "{}: the attack changed nothing", a.id);
    }
}

/// Self-test of the `M5_ATTRIB` instrument (not an attack test): a synthetic position with
/// the committed state layout and a minimal `refresh()` carrying `// ==== check A7.N`
/// markers; the instrument must name A7.2, A7.3 and A7.5 for three VA13 attacks, and the
/// good refresh must pass the synthetic source with its rebuilt output 0.
#[test]
fn attribution_instrument_self_test() {
    let committed = lottery::buy::Sources::committed(lottery::buy::b_max()).position.replace("\r\n", "\n");
    let anchor = "byte[33] owner_key = init_owner_key;";
    let head = &committed[..committed.find(anchor).expect("position state layout") + anchor.len()];
    let src = format!(
        "{head}\n\n    entry refresh() {{\n        require(OpInputCovenantId(0) != vaultTemplateHash);\n        // ==== check A7.1: leader\n        require(this.activeInputIndex == 0);\n        // ==== check A7.2: inputs\n        require(tx.inputs.length == 2);\n        // ==== check A7.3: idle\n        {{\n            int creation = OpTxInputDaaScore(0);\n            require(creation >= 0);\n        }}\n        // ==== check A7.5: outputs\n        require(tx.outputs.length == 2);\n    }}\n}}\n"
    );
    let good: Case5 = rf_position().into();
    let gb = good.build();
    assert!(passes_under_source5(&gb, &good, "position:A7.5", 0, &src), "the good refresh passes the synthetic position");
    for (id, want) in [
        ("VA13 position: a second covenant input", "position:A7.2"),
        ("VA13 position: creation = -1 (unaccepted parent)", "position:A7.3"),
        ("VA13 position: extra plain output", "position:A7.5"),
    ] {
        let (_, bad, bc) = atk5(id).pair_case(&tpl());
        let got = first_failing_check5_in(&bad, &bc, want, 0, Some(&src));
        assert_eq!(got.as_deref(), Some(want), "{id}");
    }
}
