//! M0 question 1: stateful covenants with several inputs and outputs, two
//! templates in one KIP-20 covenant family, and an exact transaction shape.
//! Probe only. Sources: covenants/m0/controller_probe.sil, shard_probe.sil, smoke.sil.
//!
//! Run: cargo test --test m0_shapes -- --nocapture

use harness::*;

mod common;
use common::*;

const COV: Hash = PROBE_COV;
const OTHER_COV: Hash = Hash::from_bytes([6u8; 32]);
const V: u64 = 100_000_000;

#[test]
fn close_shape_across_two_templates_passes() {
    for k in [1usize, 3, 8] {
        let (t, e, units) = run_close(&Shape::ok(k)).unwrap_or_else(|e| panic!("k={k}: valid shape rejected: {e:?}"));
        let m = masses(&t, &e);
        assert_valid_masses(&m, PROBE_FEE);
        println!(
            "k={k}: controller redeem {} B, shard redeem {} B, units per input {:?}, budgets {:?}, masses {:?}, min fee {} sompi",
            bytecode(&controller(7, 8)).len(),
            bytecode(&shard(0)).len(),
            units,
            t.inputs.iter().map(|i| i.compute_commit.compute_budget().unwrap()).collect::<Vec<_>>(),
            m,
            min_relay_fee(&m)
        );
    }
}

/// Each attack must be refused by the controller (input 0) with a failed require.
fn assert_controller_rejects(s: Shape, what: &str) {
    let (i, err) = run_close(&s).err().unwrap_or_else(|| panic!("{what} must be rejected"));
    assert_eq!(i, 0, "{what}: expected the controller (input 0) to reject, got input {i}: {err:?}");
    assert!(matches!(err, TxScriptError::VerifyError), "{what}: wrong failure {err:?}");
    println!("{what}: rejected at input {i}: {err:?}");
}

#[test]
fn close_shape_rejects_extra_output() {
    assert_controller_rejects(Shape { extra_output: true, ..Shape::ok(3) }, "extra output");
}

#[test]
fn close_shape_rejects_wrong_shard_successor_state() {
    assert_controller_rejects(Shape { next_shard_count: 1, ..Shape::ok(3) }, "wrong shard successor state");
}

#[test]
fn close_shape_rejects_shard_from_other_family() {
    assert_controller_rejects(Shape { shard_cov: OTHER_COV, ..Shape::ok(3) }, "shard from another family");
}

#[test]
fn close_shape_rejects_reward_bound_into_family() {
    assert_controller_rejects(Shape { reward_bound: true, ..Shape::ok(3) }, "reward bound into the family");
}

/// Finding for R2: the `#[covenant.singleton]` sugar checks only the outputs its
/// input authorizes, so an extra plain output passes. Exact shapes must be coded by hand.
#[test]
fn singleton_sugar_does_not_reject_extra_plain_output() {
    let active = compile_file("m0/smoke.sil", &[ArtifactValue::Int(10)]);
    let next = compile_file("m0/smoke.sil", &[ArtifactValue::Int(13)]);
    let sig = covenant_call_sigscript(&active, "bump", &[ArtifactValue::Int(3)], false);
    let outputs = vec![
        TransactionOutput::with_covenant(V, p2sh(&next), Some(CovenantBinding::new(0, COV))),
        TransactionOutput::new(V, ScriptPublicKey::new(0, vec![0x51].into())),
    ];
    let t = tx(vec![input(0, sig, 0)], outputs);
    let e = vec![UtxoEntry::new(2 * V, p2sh(&active), 0, false, Some(COV))];
    execute(&t, &e, 0, true).expect("singleton sugar accepts an extra plain output (R2 must be hand-coded)");
}
