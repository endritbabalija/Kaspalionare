//! M0 question 2: verify a sparse-Merkle path and compute the updated root from
//! the same path, at depth 16 and 23, with OpBlake2b and OpBlake3.
//! Measurement only. Source: covenants/m0/merkle_counter.sil.
//!
//! Run: cargo test --test m0_merkle -- --nocapture

use harness::*;

const COV: Hash = Hash::from_bytes([9u8; 32]);

#[derive(Clone, Copy, Debug)]
enum H {
    Blake2b,
    Blake3,
}

fn hash(h: H, data: &[u8]) -> [u8; 32] {
    match h {
        H::Blake2b => blake2b_simd::Params::new().hash_length(32).hash(data).as_bytes().try_into().unwrap(),
        H::Blake3 => *blake3::hash(data).as_bytes(),
    }
}

/// Script-number encoding of a non-negative value as fixed `n` bytes (OpNum2Bin).
fn num(v: i64, n: usize) -> Vec<u8> {
    assert!(v >= 0 && (v as u64) < (1u64 << (8 * n - 1)));
    v.to_le_bytes()[..n].to_vec()
}

fn leaf(h: H, index: i64, count: i64) -> [u8; 32] {
    hash(h, &[vec![0u8], num(index, 4), num(count, 8)].concat())
}

fn root(h: H, index: i64, count: i64, siblings: &[[u8; 32]]) -> [u8; 32] {
    let mut node = leaf(h, index, count);
    for (i, s) in siblings.iter().enumerate() {
        node = if (index >> i) & 1 == 0 {
            hash(h, &[&[1u8][..], &node, s].concat())
        } else {
            hash(h, &[&[1u8][..], s, &node].concat())
        };
    }
    node
}

fn source(depth: usize, h: H) -> String {
    let src = std::fs::read_to_string(covenant_path("m0/merkle_counter.sil")).unwrap();
    let src = src.replace("int constant DEPTH = 23;", &format!("int constant DEPTH = {depth};"));
    match h {
        H::Blake2b => src,
        H::Blake3 => src.replace("blake2b(", "blake3("),
    }
}

struct Run {
    redeem: usize,
    sigscript: usize,
    units: u64,
    budget: u16,
    m: Masses,
    result: Result<u64, TxScriptError>,
}

fn run(depth: usize, h: H, claimed_count: i64) -> Run {
    let index: i64 = (1 << depth) - 3; // mixed left/right path
    let count = 4;
    let siblings: Vec<[u8; 32]> = (0..depth).map(|i| hash(h, &[0xEEu8, i as u8])).collect();
    let old_root = root(h, index, count, &siblings);
    let new_root = root(h, index, count + 1, &siblings);

    let src = source(depth, h);
    let active = compile_source(&src, &[ArtifactValue::Bytes(old_root.to_vec())]);
    let next = compile_source(&src, &[ArtifactValue::Bytes(new_root.to_vec())]);
    let args = [
        ArtifactValue::Int(index),
        ArtifactValue::Int(claimed_count),
        ArtifactValue::Array(siblings.iter().map(|s| ArtifactValue::Bytes(s.to_vec())).collect()),
    ];
    let sig = covenant_call_sigscript(&active, "increment", &args, false);
    let entries = vec![UtxoEntry::new(1_000_000_000, p2sh(&active), 0, false, Some(COV))];
    let out = TransactionOutput::with_covenant(1_000_000_000, p2sh(&next), Some(CovenantBinding::new(0, COV)));

    // Budget comes from the genuine call (claimed count == true count), measured unlimited.
    // The call under test then runs under that committed budget, as consensus does, so a
    // forged count fails on the root check, not on running out of budget.
    let genuine_args = [args[0].clone(), ArtifactValue::Int(count), args[2].clone()];
    let genuine_sig = covenant_call_sigscript(&active, "increment", &genuine_args, false);
    let probe = tx(vec![input(0, genuine_sig, 0)], vec![out.clone()]);
    let units = execute(&probe, &entries, 0, false).expect("genuine update runs");
    let budget = budget_for(units);
    let t = tx(vec![input(0, sig.clone(), budget)], vec![out]);
    let result = execute(&t, &entries, 0, true);
    Run { redeem: bytecode(&active).len(), sigscript: sig.len(), units, budget, m: masses(&t, &entries), result }
}

#[test]
fn merkle_path_verify_and_update_fits_limits() {
    println!("| depth | hash | redeem B | sigscript B | script units | budget | compute mass | transient mass | storage mass |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for h in [H::Blake2b, H::Blake3] {
        for depth in [16usize, 23] {
            let r = run(depth, h, 4);
            r.result.as_ref().unwrap_or_else(|e| panic!("depth {depth} {h:?}: valid update rejected: {e:?}"));
            println!(
                "| {depth} | {h:?} | {} | {} | {} | {} | {} | {} | {} |",
                r.redeem, r.sigscript, r.units, r.budget, r.m.compute, r.m.transient, r.m.storage
            );
            // Fits one transaction under the block limits (refs/rusty-kaspa-v2.1.0/consensus/core/src/config/params.rs:626).
            assert!(r.m.compute <= 500_000 && r.m.transient <= 1_000_000 && r.m.storage <= 500_000);
        }
    }
}

#[test]
fn merkle_rejects_forged_count() {
    for h in [H::Blake2b, H::Blake3] {
        for forged in [3i64, 5, 0] {
            let r = run(23, h, forged);
            let err = r.result.expect_err("forged count must be rejected");
            // Root mismatch fails a require (VerifyError), not the compute budget.
            assert!(matches!(err, TxScriptError::VerifyError), "{h:?} count {forged}: wrong failure {err:?}");
        }
    }
}
