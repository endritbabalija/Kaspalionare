//! M0 smoke test: a trivial SilverScript covenant runs in the local
//! rusty-kaspa v2.1.0 script engine. Source: covenants/m0/smoke.sil.

use harness::*;

const COV: Hash = Hash::from_bytes([7u8; 32]);

fn counter(v: i64) -> SilAbiArtifact {
    compile_file("m0/smoke.sil", &[ArtifactValue::Int(v)])
}

fn spend(active: &SilAbiArtifact, delta: i64, out: TransactionOutput) -> (Transaction, Vec<UtxoEntry>) {
    let sig = covenant_call_sigscript(active, "bump", &[ArtifactValue::Int(delta)], false);
    let t = tx(vec![input(0, sig, 0)], vec![out]);
    let entries = vec![UtxoEntry::new(10_000, p2sh(active), 0, false, Some(COV))];
    (t, entries)
}

fn successor(art: &SilAbiArtifact) -> TransactionOutput {
    TransactionOutput::with_covenant(10_000, p2sh(art), Some(CovenantBinding::new(0, COV)))
}

#[test]
fn smoke_accepts_correct_successor_state() {
    let (t, e) = spend(&counter(10), 3, successor(&counter(13)));
    let units = execute(&t, &e, 0, true).expect("valid transition must pass");
    println!("smoke: redeem script {} bytes, {} script units", bytecode(&counter(10)).len(), units);
}

#[test]
fn smoke_rejects_wrong_successor_state() {
    let (t, e) = spend(&counter(10), 3, successor(&counter(12)));
    let err = execute(&t, &e, 0, true).expect_err("wrong successor state must fail");
    println!("rejected as expected: {err:?}");
}

#[test]
fn smoke_rejects_successor_without_covenant_binding() {
    let out = TransactionOutput::new(10_000, p2sh(&counter(13)));
    let (t, e) = spend(&counter(10), 3, out);
    let err = execute(&t, &e, 0, true).expect_err("unbound successor must fail");
    println!("rejected as expected: {err:?}");
}
