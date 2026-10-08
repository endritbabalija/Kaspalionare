//! Shared M0 probe builders (controller_probe.sil + shard_probe.sil).
#![allow(dead_code)]

use harness::*;

pub fn shard(count: i64) -> SilAbiArtifact {
    compile_file("m0/shard_probe.sil", &[ArtifactValue::Int(count)])
}

/// (prefix len, suffix len, template hash) of the shard template.
pub fn shard_template() -> (i64, i64, Vec<u8>) {
    let a = shard(0);
    let c = &a.contracts.values().next().unwrap().compiled;
    let (start, len) = (c.state_span.offset, c.state_span.len);
    (start as i64, (c.bytecode.len() - start - len) as i64, c.template_hash.to_vec())
}

/// Controller probe with its unrolled shard loop bound set to `max_k`.
pub fn controller(round: i64, max_k: usize) -> SilAbiArtifact {
    let (p, s, h) = shard_template();
    let src = std::fs::read_to_string(covenant_path("m0/controller_probe.sil")).unwrap();
    let src = src.replace("int constant MAX_K = 8;", &format!("int constant MAX_K = {max_k};"));
    compile_source(&src, &[ArtifactValue::Int(round), ArtifactValue::Int(p), ArtifactValue::Int(s), ArtifactValue::Bytes(h)])
}

pub fn close_sigscript(ctl: &SilAbiArtifact, k: usize) -> Vec<u8> {
    entry_sigscript(ctl, "close", &[ArtifactValue::Int(k as i64)])
}

pub fn shard_sigscript(sh: &SilAbiArtifact) -> Vec<u8> {
    entry_sigscript(sh, "closeLeg", &[])
}

/// OpTrue stand-in for a caller's or player's standard script (spk length 1).
pub fn plain_spk() -> ScriptPublicKey {
    ScriptPublicKey::new(0, vec![0x51].into())
}

/// A standard 34-byte pay-to-pubkey script (OpData32 <32 bytes> OpCheckSig), for mass sizing only.
pub fn p2pk_spk() -> ScriptPublicKey {
    let mut s = vec![0x20];
    s.extend_from_slice(&[0x11; 32]);
    s.push(0xac);
    ScriptPublicKey::new(0, s.into())
}

pub const PROBE_COV: Hash = Hash::from_bytes([5u8; 32]);
pub const KAS: u64 = 100_000_000;
/// Value of each covenant object in the probe (large enough for storage mass, see m0_mass).
pub const OBJ: u64 = 10 * KAS;
/// Caller funding; the caller output returns it minus the fee.
pub const FUNDING: u64 = 10 * KAS;
pub const PROBE_FEE: u64 = KAS / 2;

/// Variants of the close()-shaped probe transaction; `ok(k)` is the valid one.
#[derive(Clone)]
pub struct Shape {
    pub k: usize,
    pub max_k: usize,
    pub shard_cov: Hash,
    pub next_shard_count: i64,
    pub extra_output: bool,
    pub reward_bound: bool,
}

impl Shape {
    pub fn ok(k: usize) -> Self {
        Shape { k, max_k: k.max(8), shard_cov: PROBE_COV, next_shard_count: 0, extra_output: false, reward_bound: false }
    }
}

/// inputs: controller, K shards, caller funding; outputs: controller', K shards', caller output.
/// Values conserve: covenant objects pass OBJ through; the caller output is FUNDING - fee.
pub fn build_close(s: &Shape, budgets: &[u16]) -> (Transaction, Vec<UtxoEntry>) {
    let b = |i: usize| budgets.get(i).copied().unwrap_or(0);
    let ctl = controller(7, s.max_k);
    let mut inputs = vec![input(0, close_sigscript(&ctl, s.k), b(0))];
    let mut entries = vec![UtxoEntry::new(OBJ, p2sh(&ctl), 0, false, Some(PROBE_COV))];
    let mut outputs =
        vec![TransactionOutput::with_covenant(OBJ, p2sh(&controller(8, s.max_k)), Some(CovenantBinding::new(0, PROBE_COV)))];
    for i in 0..s.k {
        let old = shard(10 + i as i64);
        inputs.push(input(1 + i as u32, shard_sigscript(&old), b(1 + i)));
        entries.push(UtxoEntry::new(OBJ, p2sh(&old), 0, false, Some(s.shard_cov)));
        outputs.push(TransactionOutput::with_covenant(
            OBJ,
            p2sh(&shard(s.next_shard_count)),
            Some(CovenantBinding::new(1 + i as u16, s.shard_cov)),
        ));
    }
    // Caller funding: OpTrue script, empty sigscript (stands in for the caller's signed input).
    inputs.push(input(1 + s.k as u32, vec![], b(1 + s.k)));
    entries.push(UtxoEntry::new(FUNDING, plain_spk(), 0, false, None));
    let extra = if s.extra_output { KAS / 10 } else { 0 };
    let binding = s.reward_bound.then(|| CovenantBinding::new(0, PROBE_COV));
    outputs.push(TransactionOutput::with_covenant(FUNDING - PROBE_FEE - extra, plain_spk(), binding));
    if s.extra_output {
        outputs.push(TransactionOutput::new(extra, plain_spk()));
    }
    (tx(inputs, outputs), entries)
}

/// Commit budgets measured on the valid shape `ok(k)`, then run every input of `s`
/// under its committed budget, as consensus does. Returns (tx, entries, units per input) or (input, error).
pub fn run_close(s: &Shape) -> Result<(Transaction, Vec<UtxoEntry>, Vec<u64>), (usize, TxScriptError)> {
    let valid = Shape { max_k: s.max_k, ..Shape::ok(s.k) };
    let (t0, e0) = build_close(&valid, &[]);
    let budgets: Vec<u16> = (0..t0.inputs.len())
        .map(|i| budget_for(execute(&t0, &e0, i, false).unwrap_or_else(|e| panic!("valid shape input {i}: {e:?}"))))
        .collect();
    let (t, e) = build_close(s, &budgets);
    let units = (0..t.inputs.len()).map(|i| execute(&t, &e, i, true).map_err(|err| (i, err))).collect::<Result<Vec<_>, _>>()?;
    Ok((t, e, units))
}

/// Minimum relay fee in sompi: 100 sompi per gram of max(compute, ceil(transient / 2))
/// (rusty-kaspa-v2.1.0 mining/src/mempool/check_transaction_standard.rs:71-75, config.rs:20).
pub fn min_relay_fee(m: &Masses) -> u64 {
    m.compute.max(m.transient.div_ceil(2)) * 100
}

/// Asserts block/tx mass limits (params.rs:626) and the fee floor for a transaction paying `fee`.
pub fn assert_valid_masses(m: &Masses, fee: u64) {
    assert!(m.compute <= 500_000 && m.storage <= 500_000 && m.transient <= 1_000_000, "masses over limits: {m:?}");
    assert!(fee >= min_relay_fee(m), "fee {fee} below minimum {}", min_relay_fee(m));
}
