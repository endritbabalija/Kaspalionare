//! Spec M8 A3/A4 (D116, D121): the launch bundle `bundle/1` and its local verification.
//!
//! The bundle lets anyone check an instance without trusting the project: the genesis
//! transaction, its authorized outputs and their states, the covenant ID, the template
//! preimages, every genesis parameter and the live UTXOs at publication. `verify` checks it
//! locally (no node): the template preimages hash to their hashes and equal the pinned
//! toolchain's compilation, the genesis transaction and its authorized outputs agree, the
//! covenant ID is recomputed with the consensus function (rusty-kaspa v2.1.0
//! `consensus/core/src/hashing/covenant_id.rs:16-31`), every genesis state hashes to its
//! output script, the output values follow from the states, the parameters equal the
//! states', the full genesis `validate()` passes (including the M8 SF1 round-1 game-size
//! rule), and every live UTXO's state hashes to its script. Comparing the live UTXOs with a
//! node is `tn10 bundle-verify`'s job (S3).

use std::sync::Arc;

use harness::{ArtifactValue, Hash, Transaction, TransactionOutpoint, TransactionOutput, pay_to_script_hash_script};
use crate::genesis::NetworkType;
use serde::{Deserialize, Serialize};

use crate::buy::{Final, Templates};
use crate::genesis::GenesisCase;
use crate::index::{ROLES, Role, State, template_of};
use crate::store::{BUNDLE, LiveJson, OutpointJson, Params, hex, hex_vec};

/// The pinned rusty-kaspa release v2.1.0 (CLAUDE.md, `crates/Cargo.toml`).
pub const RUSTY_KASPA_REV: &str = "01b532e8b553523216471682649693af92f0fd16";
/// The pinned SilverScript v1.0.0 (`crates/Cargo.toml`).
pub const SILVERSCRIPT_REV: &str = "3ed973335b59269293564805cc2c58a14595ec03";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toolchain {
    pub rusty_kaspa_rev: String,
    pub silverscript_rev: String,
    pub b_max: u64,
}

/// A committed template: its role, the compiler output around the state (prefix, suffix)
/// and the template hash the covenants commit to (`silverscript_abi::template_hash`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateJson {
    pub role: Role,
    #[serde(with = "hex")]
    pub prefix: Vec<u8>,
    #[serde(with = "hex")]
    pub suffix: Vec<u8>,
    #[serde(with = "hex")]
    pub hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizedOutput {
    pub index: u32,
    pub value: u64,
    pub spk_version: u16,
    #[serde(with = "hex")]
    pub script: Vec<u8>,
    pub authorizing_input: u16,
    #[serde(with = "hex")]
    pub covenant_id: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub daa: u64,
    #[serde(with = "hex")]
    pub chain_block: [u8; 32],
}

/// `docs/launch/<name>.json` (A3 `bundle/1`). M8 S2 additions to A3: `genesis_tx` (so the
/// txid, the genesis outpoint and the authorized outputs are checked against the
/// transaction itself) and `genesis_states` (the authorized outputs' states as redeem
/// scripts: outputs are P2SH, so the states are in no transaction).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundle {
    pub format: String,
    pub network: String,
    pub toolchain: Toolchain,
    #[serde(with = "hex")]
    pub genesis_txid: [u8; 32],
    pub genesis_outpoint: OutpointJson,
    pub genesis_tx: Transaction,
    pub authorized_outputs: Vec<AuthorizedOutput>,
    #[serde(with = "hex_vec")]
    pub genesis_states: Vec<Vec<u8>>,
    #[serde(with = "hex")]
    pub covenant_id: [u8; 32],
    pub templates: Vec<TemplateJson>,
    pub params: Params,
    pub snapshot: Option<Snapshot>,
    pub live_utxos: Vec<LiveJson>,
}

/// The genesis states in output order (controller, params, k shards, vault, drawer,
/// position; D50).
pub fn genesis_states(g: &GenesisCase) -> Vec<State> {
    let mut v = vec![State::Controller(g.controller.clone()), State::Params(g.params.clone())];
    v.extend(g.shards.iter().map(|s| State::Shard(s.clone())));
    v.extend([State::Vault(g.vault.clone()), State::Drawer(g.drawer.clone()), State::Position(g.position.clone())]);
    v
}

/// Every integer genesis value (A2): the integer fields of the controller, params, drawer,
/// vault and position states and of shard 0, by `<object>.<field>`, plus the boost pot.
pub fn genesis_params(g: &GenesisCase) -> Params {
    let mut p = Params::new();
    let mut add = |obj: &str, s: &State| {
        for (k, v) in s.values() {
            if let ArtifactValue::Int(x) = v {
                p.insert(format!("{obj}.{k}"), x);
            }
        }
    };
    add("controller", &State::Controller(g.controller.clone()));
    add("params", &State::Params(g.params.clone()));
    if let Some(s) = g.shards.first() {
        add("shard", &State::Shard(s.clone()));
    }
    add("vault", &State::Vault(g.vault.clone()));
    add("drawer", &State::Drawer(g.drawer.clone()));
    add("position", &State::Position(g.position.clone()));
    p.insert("pot".into(), g.pot as i64);
    p.insert("k".into(), g.k() as i64);
    p
}

fn network_name(n: NetworkType) -> &'static str {
    match n {
        NetworkType::Mainnet => "mainnet",
        NetworkType::Testnet => "testnet-10",
        NetworkType::Devnet => "devnet",
        NetworkType::Simnet => "simnet",
    }
}

fn network_of(s: &str) -> Result<NetworkType, String> {
    match s {
        "testnet-10" => Ok(NetworkType::Testnet),
        "devnet" => Ok(NetworkType::Devnet),
        "simnet" => Ok(NetworkType::Simnet),
        other => Err(format!("network {other:?} is not testnet-10, devnet or simnet (genesis refuses mainnet, D100)")),
    }
}

impl Bundle {
    /// The bundle of a finished genesis `f` of `g` (its states and templates), with the live
    /// UTXOs `live` at the snapshot. It records; it does not validate (`verify` does).
    pub fn new(g: &GenesisCase, f: &Final, live: Vec<LiveJson>, snapshot: Option<Snapshot>) -> Result<Self, String> {
        let t = g.templates();
        let n = g.k() + 5;
        let tx = f.tx.clone();
        let op = tx.inputs.first().ok_or("genesis: no input")?.previous_outpoint;
        let authorized_outputs = tx.outputs[..n]
            .iter()
            .enumerate()
            .map(|(i, o)| {
                let b = o.covenant.ok_or_else(|| format!("genesis output {i} is not bound"))?;
                Ok(AuthorizedOutput {
                    index: i as u32,
                    value: o.value,
                    spk_version: o.script_public_key.version(),
                    script: o.script_public_key.script().to_vec(),
                    authorizing_input: b.authorizing_input,
                    covenant_id: b.covenant_id.as_bytes(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            format: BUNDLE.into(),
            network: network_name(g.network).into(),
            toolchain: Toolchain { rusty_kaspa_rev: RUSTY_KASPA_REV.into(), silverscript_rev: SILVERSCRIPT_REV.into(), b_max: t.b_max as u64 },
            genesis_txid: tx.id().as_bytes(),
            genesis_outpoint: op.into(),
            authorized_outputs,
            genesis_states: genesis_states(g).iter().map(|s| s.redeem(&t)).collect(),
            covenant_id: recompute_cov(op, &tx.outputs[..n]).as_bytes(),
            templates: ROLES
                .iter()
                .map(|&role| {
                    let tp = template_of(&t, role);
                    TemplateJson { role, prefix: tp.prefix.clone(), suffix: tp.suffix.clone(), hash: tp.hash }
                })
                .collect(),
            params: genesis_params(g),
            snapshot,
            live_utxos: live,
            genesis_tx: tx,
        })
    }
}

/// KIP-20 covenant ID of the genesis outpoint and its authorized outputs 0..n
/// (`kaspa_consensus_core::hashing::covenant_id::covenant_id`, as `index::classify`).
pub fn recompute_cov(op: TransactionOutpoint, outs: &[TransactionOutput]) -> Hash {
    kaspa_consensus_core::hashing::covenant_id::covenant_id(op, outs.iter().enumerate().map(|(i, o)| (i as u32, o)))
}

/// What a verified bundle describes.
#[derive(Clone)]
pub struct Verified {
    pub cov: Hash,
    pub k: usize,
    pub templates: Arc<Templates>,
    /// The genesis states in output order (for `Rebuild::new`).
    pub genesis_states: Vec<State>,
    pub genesis: GenesisCase,
}

impl std::fmt::Debug for Verified {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verified").field("cov", &self.cov).field("k", &self.k).finish_non_exhaustive()
    }
}

/// Verify a bundle locally (no RPC); the first failure is the error.
pub fn verify(b: &Bundle) -> Result<Verified, String> {
    if b.format != BUNDLE {
        return Err(format!("unknown format {:?}", b.format));
    }
    let network = network_of(&b.network)?;
    if b.toolchain.rusty_kaspa_rev != RUSTY_KASPA_REV || b.toolchain.silverscript_rev != SILVERSCRIPT_REV {
        return Err(format!("toolchain {} / {} is not the pinned {RUSTY_KASPA_REV} / {SILVERSCRIPT_REV}", b.toolchain.rusty_kaspa_rev, b.toolchain.silverscript_rev));
    }
    // Templates: each role once, its preimage hashes to its hash, and equals the pinned
    // toolchain's compilation (so the local templates decode the states below).
    // S2c (review note): the file's B_MAX is bounded before any template is compiled for
    // it: 1 ..= TICKET_B_MAX (the claim reference's and the ticket script's bound,
    // `game::claim_mode`); anything else would not compile or would compile without end.
    let b_max = crate::store::checked_b_max(b.toolchain.b_max).map_err(|e| format!("toolchain {e}"))?;
    let t = Templates::get(b_max);
    if b.templates.len() != ROLES.len() {
        return Err(format!("{} templates, expected {}", b.templates.len(), ROLES.len()));
    }
    for (tj, role) in b.templates.iter().zip(ROLES) {
        if tj.role != role {
            return Err(format!("template {:?} where {role:?} is expected", tj.role));
        }
        if silverscript_abi::template_hash(&tj.prefix, &tj.suffix) != tj.hash {
            return Err(format!("template {role:?}: the preimage does not hash to the template hash"));
        }
        let local = template_of(&t, role);
        if local.prefix != tj.prefix || local.suffix != tj.suffix || local.hash != tj.hash {
            return Err(format!("template {role:?}: differs from the pinned toolchain's compilation"));
        }
    }
    // The genesis transaction, its outpoint and its authorized outputs.
    let tx = &b.genesis_tx;
    // S2c MF-B: the id recomputed (serde reads it verbatim), then compared.
    let id = crate::store::checked_id(tx).map_err(|e| format!("genesis_tx: {e}"))?;
    if id.as_bytes() != b.genesis_txid {
        return Err("genesis_txid is not the genesis transaction's id".into());
    }
    let op = tx.inputs.first().ok_or("genesis: no input")?.previous_outpoint;
    if OutpointJson::from(op) != b.genesis_outpoint {
        return Err("genesis_outpoint is not the genesis transaction's input 0".into());
    }
    let n = b.authorized_outputs.len();
    if n < 6 || n > tx.outputs.len() {
        return Err(format!("{n} authorized outputs"));
    }
    let k = n - 5;
    let cov = Hash::from_bytes(b.covenant_id);
    for (i, a) in b.authorized_outputs.iter().enumerate() {
        let o = &tx.outputs[i];
        let same = a.index == i as u32
            && a.value == o.value
            && a.spk_version == o.script_public_key.version()
            && a.script == o.script_public_key.script()
            && o.covenant.is_some_and(|c| c.authorizing_input == a.authorizing_input && c.covenant_id.as_bytes() == a.covenant_id);
        if !same {
            return Err(format!("authorized output {i} differs from the genesis transaction's output {i}"));
        }
        if a.authorizing_input != 0 || a.covenant_id != b.covenant_id {
            return Err(format!("authorized output {i}: not bound to the covenant ID by input 0"));
        }
    }
    if tx.outputs[n..].iter().any(|o| o.covenant.is_some()) {
        return Err("a covenant-bound output after the authorized outputs".into());
    }
    let id = recompute_cov(op, &tx.outputs[..n]);
    if id != cov {
        return Err(format!("covenant_id {cov} is not the recomputed {id}"));
    }
    // The genesis states: each decodes under the templates, has its role (D50 order) and
    // hashes to its authorized output's script.
    if b.genesis_states.len() != n {
        return Err(format!("{} genesis states for {n} authorized outputs", b.genesis_states.len()));
    }
    let mut roles = vec![Role::Controller, Role::Params];
    roles.extend(std::iter::repeat_n(Role::Shard, k));
    roles.extend([Role::Vault, Role::Drawer, Role::Position]);
    let mut states = vec![];
    for (i, (r, role)) in b.genesis_states.iter().zip(&roles).enumerate() {
        let s = State::decode(&t, r).map_err(|e| format!("genesis state {i}: {e}"))?;
        if s.role() != *role {
            return Err(format!("genesis state {i} is a {:?}, expected {role:?}", s.role()));
        }
        if pay_to_script_hash_script(r) != tx.outputs[i].script_public_key {
            return Err(format!("genesis state {i} does not hash to its output script"));
        }
        states.push(s);
    }
    // The instance the states describe, its output values and its validation (SF1 too).
    let mut g = GenesisCase::new_with(k, vec![], &t);
    g.network = network;
    for s in &states {
        match s {
            State::Controller(x) => g.controller = x.clone(),
            State::Params(x) => g.params = x.clone(),
            State::Vault(x) => g.vault = x.clone(),
            State::Drawer(x) => g.drawer = x.clone(),
            State::Position(x) => g.position = x.clone(),
            _ => {}
        }
    }
    g.shards = states.iter().filter_map(|s| if let State::Shard(x) = s { Some(x.clone()) } else { None }).collect();
    let base = (g.controller.acc as u64).checked_add(g.controller.reserve as u64).ok_or("acc + R overflows")?;
    g.pot = tx.outputs[0].value.checked_sub(base).ok_or("controller value below acc + R")?;
    // S2d (review): the states are untrusted, so they are validated (every range, SF1, the
    // routes) before any value is derived from them, and the values use checked arithmetic.
    g.validate_with(&t).map_err(|e| format!("genesis validate: {e}"))?;
    let overflow = |what: &str| format!("genesis {what} value overflows or is negative");
    let r = g.reserve();
    let mut want = vec![g.checked_controller_value().ok_or_else(|| overflow("controller"))?, r];
    for i in 0..k {
        want.push(g.checked_shard_value(i).ok_or_else(|| overflow("shard"))?);
    }
    want.extend([g.checked_vault_value().ok_or_else(|| overflow("vault"))?, r, r]);
    for (i, w) in want.iter().enumerate() {
        if tx.outputs[i].value != *w {
            return Err(format!("genesis output {i}: value {}, the states give {w}", tx.outputs[i].value));
        }
    }
    if b.params != genesis_params(&g) {
        return Err("params differ from the genesis states' values".into());
    }
    // Live UTXOs at publication: bound to the instance, each state hashing to its script.
    for l in &b.live_utxos {
        if l.covenant_id != b.covenant_id {
            return Err(format!("live UTXO {}:{}: another covenant ID", crate::store::to_hex(&l.outpoint.txid), l.outpoint.index));
        }
        l.decode(&t)?;
    }
    Ok(Verified { cov, k, templates: t, genesis_states: states, genesis: g })
}

/// The live UTXOs of a genesis (its authorized outputs, every state known), for a bundle
/// published right after genesis.
pub fn genesis_live(g: &GenesisCase, f: &Final, daa: u64) -> Vec<LiveJson> {
    let t = g.templates();
    let txid = f.tx.id();
    genesis_states(g)
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let o = &f.tx.outputs[i];
            LiveJson {
                outpoint: TransactionOutpoint::new(txid, i as u32).into(),
                role: s.role(),
                round: Some(s.round()),
                value: o.value,
                daa,
                covenant_id: o.covenant.map(|c| c.covenant_id.as_bytes()).unwrap_or([0; 32]),
                spk_version: o.script_public_key.version(),
                spk: o.script_public_key.script().to_vec(),
                state: Some(s.redeem(&t)),
            }
        })
        .collect()
}
