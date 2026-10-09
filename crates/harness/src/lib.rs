//! Local test harness: compiles SilverScript covenants and runs them in the
//! rusty-kaspa v2.1.0 script engine, and measures transaction mass.
//!
//! Only bytecode crosses from the SilverScript compiler (which links its own
//! rusty-kaspa revision) into the pinned engine.

use std::path::PathBuf;

pub use kaspa_consensus_core::{
    Hash,
    config::params::MAINNET_PARAMS,
    hashing::sighash::SigHashReusedValuesUnsync,
    mass::{ComputeBudget, Gram, Mass, MassCalculator, ScriptUnits},
    subnets::SubnetworkId,
    tx::{
        CovenantBinding, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionInput,
        TransactionOutpoint, TransactionOutput, UtxoEntry,
    },
};
pub use kaspa_txscript::{
    EngineCtx, EngineFlags, TxScriptEngine, caches::Cache, covenants::CovenantsContext, pay_to_script_hash_script,
    script_builder::ScriptBuilder,
};
pub use kaspa_txscript_errors::TxScriptError;
pub use silverscript_abi::{ArtifactValue, SilAbiArtifact};
use silverscript_lang::compiler::compile_to_sil_abi_artifact;

/// Path of a covenant source under the repo's `covenants/` directory.
pub fn covenant_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../covenants").join(rel)
}

/// Compile a covenant source file (relative to `covenants/`) with constructor args.
pub fn compile_file(rel: &str, ctor: &[ArtifactValue]) -> SilAbiArtifact {
    let path = covenant_path(rel);
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    compile_source(&src, ctor)
}

pub fn compile_source(src: &str, ctor: &[ArtifactValue]) -> SilAbiArtifact {
    compile_to_sil_abi_artifact(src, ctor).unwrap_or_else(|e| panic!("compile failed: {e}"))
}

fn single(art: &SilAbiArtifact) -> &String {
    assert_eq!(art.contracts.len(), 1, "expected exactly one contract");
    art.contracts.first_key_value().unwrap().0
}

/// Redeem script bytes of a single-contract artifact.
pub fn bytecode(art: &SilAbiArtifact) -> Vec<u8> {
    art.contracts[single(art)].compiled.bytecode.clone()
}

pub fn p2sh(art: &SilAbiArtifact) -> ScriptPublicKey {
    pay_to_script_hash_script(&bytecode(art))
}

fn push_redeem(mut sig: Vec<u8>, redeem: &[u8]) -> Vec<u8> {
    sig.extend_from_slice(&ScriptBuilder::new().add_data(redeem).expect("push redeem script").drain());
    sig
}

/// Signature script calling a covenant-declaration entry (e.g. `#[covenant.singleton]`).
pub fn covenant_call_sigscript(art: &SilAbiArtifact, func: &str, args: &[ArtifactValue], is_leader: bool) -> Vec<u8> {
    let sig = silverscript_abi::encode_contract_covenant_decl_sig_script(art, single(art), func, is_leader, args)
        .unwrap_or_else(|e| panic!("encode covenant call: {e}"));
    push_redeem(sig, &bytecode(art))
}

/// Signature script calling a plain public entry point.
pub fn entry_sigscript(art: &SilAbiArtifact, entry: &str, args: &[ArtifactValue]) -> Vec<u8> {
    let sig = silverscript_abi::encode_contract_entry_sig_script(art, single(art), entry, args)
        .unwrap_or_else(|e| panic!("encode entry call: {e}"));
    push_redeem(sig, &bytecode(art))
}

/// Deterministic dummy outpoint.
pub fn outpoint(n: u32) -> TransactionOutpoint {
    let mut id = [0u8; 32];
    id[..4].copy_from_slice(&(n + 1).to_le_bytes());
    TransactionOutpoint::new(TransactionId::from_bytes(id), n)
}

/// A version-1 (post-Toccata) input with an explicit compute budget.
pub fn input(n: u32, sig: Vec<u8>, compute_budget: u16) -> TransactionInput {
    TransactionInput::new_with_compute_budget(outpoint(n), sig, 0, compute_budget)
}

/// A version-1 transaction.
pub fn tx(inputs: Vec<TransactionInput>, outputs: Vec<TransactionOutput>) -> Transaction {
    Transaction::new(1, inputs, outputs, 0, SubnetworkId::default(), 0, vec![])
}

/// Engine flags as consensus builds them (sigop price from mainnet params).
pub fn engine_flags() -> EngineFlags {
    EngineFlags { sigop_script_units: Gram(MAINNET_PARAMS.mass_per_sig_op).into() }
}

/// Run input `idx` through the engine and return the script units used.
/// `enforce_budget = true` limits execution to the input's committed compute
/// budget plus the free allowance, as consensus does; `false` is unlimited (for measuring).
pub fn execute(tx: &Transaction, entries: &[UtxoEntry], idx: usize, enforce_budget: bool) -> Result<u64, TxScriptError> {
    let populated = PopulatedTransaction::new(tx, entries.to_vec());
    let cov = CovenantsContext::from_tx(&populated).map_err(TxScriptError::from)?;
    let cache = Cache::new(10_000);
    let reused = SigHashReusedValuesUnsync::new();
    let ctx = EngineCtx::new(&cache).with_reused(&reused).with_covenants_ctx(&cov);
    let input = &tx.inputs[idx];
    let limit = if enforce_budget { input.compute_commit.allowed_script_units() } else { ScriptUnits(u64::MAX) };
    let mut vm = TxScriptEngine::from_transaction_input_with_script_units_limit(
        &populated,
        input,
        idx,
        &entries[idx],
        ctx,
        engine_flags(),
        limit,
    );
    vm.execute()?;
    Ok(vm.used_script_units().0)
}

/// Smallest compute budget covering `script_units` beyond the free per-input allowance.
pub fn budget_for(script_units: u64) -> u16 {
    ComputeBudget::checked_covering_script_units(ScriptUnits(script_units)).expect("budget fits u16").value()
}

#[derive(Debug, Clone, Copy)]
pub struct Masses {
    pub compute: u64,
    pub transient: u64,
    pub storage: u64,
    /// Normalized max over compute, transient and storage: the mass the mempool divides the
    /// fee by when it ranks transactions (mining/src/mempool/model/frontier/feerate_key.rs:86-98).
    pub priority: u64,
}

/// Masses of `tx` under mainnet consensus parameters.
pub fn masses(tx: &Transaction, entries: &[UtxoEntry]) -> Masses {
    let mc = MassCalculator::new_with_consensus_params(&MAINNET_PARAMS);
    let nc = mc.calc_non_contextual_masses(tx);
    let populated = PopulatedTransaction::new(tx, entries.to_vec());
    let st = mc.calc_contextual_masses(&populated).expect("storage mass computable");
    let priority = Mass::new(nc, st).normalized_max(&MAINNET_PARAMS.block_mass_limits.cofactors());
    Masses { compute: nc.compute_mass, transient: nc.transient_mass, storage: st.storage_mass, priority }
}

/// Write the storage-mass commitment consensus checks (tx_validation_in_utxo_context.rs:126-133).
pub fn commit_storage_mass(tx: &Transaction, entries: &[UtxoEntry]) {
    tx.set_storage_mass(masses(tx, entries).storage);
}

/// Minimum feerate 100 sompi per gram (mining/src/mempool/config.rs:20, :138-141).
pub const MIN_FEERATE_SOMPI_PER_GRAM: u64 = 100;

/// Fee for the minimum feerate over the priority mass: what a transaction needs to rank at
/// the floor when blocks are full. The relay floor itself ignores storage mass.
pub fn priority_fee(m: &Masses) -> u64 {
    m.priority * MIN_FEERATE_SOMPI_PER_GRAM
}
