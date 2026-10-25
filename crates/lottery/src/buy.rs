//! Transaction builder for `buy()` (spec M1 section 5), the compiled templates, and the
//! editable transaction (`Built`/`Final`) shared by every builder (`close`, `params`).
//!
//! A `BuyCase` describes a purchase; `BuyCase::build` turns it into an editable `Built`
//! (sigscript arguments, inputs, outputs). Tests mutate a `Built` to make one attack,
//! then `Built::finish` sizes compute budgets, commits the storage mass and signs funding.

use std::sync::{Arc, Mutex, OnceLock};
use std::collections::HashMap;

use harness::{
    ArtifactValue, CovenantBinding, Hash, PopulatedTransaction, ScriptBuilder, ScriptPublicKey, SilAbiArtifact, Transaction,
    ChainSim, TransactionInput, TransactionOutpoint, TransactionOutput, TxScriptError, UtxoEntry, budget_for, execute,
    execute_on_chain, outpoint, pay_to_script_hash_script,
};
#[cfg(feature = "compiler")]
use harness::{compile_source, covenant_path};
use kaspa_consensus_core::{hashing::sighash_type::SIG_HASH_ALL, sign::sign_input, tx::GenesisCovenantGroup};
use secp256k1::{Keypair, SECP256K1};
use silverscript_abi::{encode_contract_entry_sig_script, encode_runtime_state_script};

use crate::combo::{Combo, bundle_bytes};
use crate::smt::{Tree, counts_bytes, num_bytes};
use crate::state::{
    ControllerState, DrawerState, K_MAX, MarkerState, ParamsState, Payout, PoolState, PositionState, RecordState, RequestState, ShardState, TicketState,
    VaultState,
};

pub const KAS: u64 = 100_000_000;
pub const M_REF: u64 = 10_000_000;
/// Ticket deposit per combination used in tests: the genesis floor d = D_FLOOR = 0.13 KAS
/// (M6 D80, D81; was 0.11 KAS), at which the b = 1 buy with a 0.1 KAS referral output is
/// within 90 % of storage mass.
pub const TEST_DEPOSIT: u64 = crate::genesis::D_FLOOR as u64;
/// Test covenant ID of the instance (spec M1 D14).
pub const COV: Hash = Hash::from_bytes([0x4c; 32]);
/// Funding key used by every test buyer.
pub const FUNDING_SK: [u8; 32] = [7u8; 32];
/// Schnorr P2PK sigscript: OP_DATA_65 <sig || sighash type>.
pub const P2PK_SIG_LEN: usize = 66;
/// One signature check: 100,000 script units (consensus/core/src/mass/units.rs).
pub const P2PK_UNITS: u64 = 100_000;
/// Budget basis for an input whose unlimited run fails.
pub const FAILED_RUN_UNITS: u64 = 2_000_000;

/// A template split around its state, plus the hash SilverScript commits to.
#[derive(Clone)]
pub struct Template {
    pub art: SilAbiArtifact,
    pub prefix: Vec<u8>,
    pub suffix: Vec<u8>,
    pub hash: [u8; 32],
}

impl Template {
    /// Public since M7 Stage 1b: SA10 (`tests/m7_attacks.rs`) compiles the pre-M7 marker
    /// (with `seed_ref`) from a test fixture to build a marker output under the old template.
    pub fn from(art: SilAbiArtifact) -> Self {
        Self::try_from_artifact(art).unwrap_or_else(|e| panic!("{e}"))
    }

    /// `from` with an error instead of a panic (front-end F1 part B: the embedded templates
    /// are loaded through this in every build, so a wasm page never aborts on them).
    pub fn try_from_artifact(art: SilAbiArtifact) -> Result<Self, String> {
        if art.contracts.len() != 1 {
            return Err(format!("template artifact with {} contracts, expected 1", art.contracts.len()));
        }
        let c = &art.contracts.values().next().ok_or("template artifact without a contract")?.compiled;
        let (p, _, s) = c.script_parts(&c.bytecode).ok_or("template has no state span")?;
        let (prefix, suffix, hash) = (p.to_vec(), s.to_vec(), c.template_hash);
        if silverscript_abi::template_hash(&prefix, &suffix) != hash {
            return Err("template hash differs from the hash of its prefix and suffix".into());
        }
        Ok(Self { art, prefix, suffix, hash })
    }

    pub fn name(&self) -> &str {
        // `try_from_artifact` checked there is exactly one contract.
        self.art.contracts.keys().next().map(String::as_str).unwrap_or("")
    }

    /// The encoded state alone (the bytes between prefix and suffix; M8 `bundle/1`).
    pub fn state_bytes(&self, values: &std::collections::BTreeMap<String, ArtifactValue>) -> Vec<u8> {
        self.try_state_bytes(values).unwrap_or_else(|e| panic!("{e}"))
    }

    /// `state_bytes`, with the encoder's error instead of a panic (F1 part B).
    pub fn try_state_bytes(&self, values: &std::collections::BTreeMap<String, ArtifactValue>) -> Result<Vec<u8>, String> {
        let c = self.art.contracts.values().next().ok_or("template artifact without a contract")?;
        encode_runtime_state_script(&self.art, &c.runtime_state, values).map_err(|e| format!("encode state: {e}"))
    }

    /// Redeem script for a state: prefix || encoded state || suffix.
    pub fn redeem(&self, values: &std::collections::BTreeMap<String, ArtifactValue>) -> Vec<u8> {
        [self.prefix.as_slice(), &self.state_bytes(values), &self.suffix].concat()
    }

    /// `redeem`, with the encoder's error instead of a panic (F1 part B).
    pub fn try_redeem(&self, values: &std::collections::BTreeMap<String, ArtifactValue>) -> Result<Vec<u8>, String> {
        Ok([self.prefix.as_slice(), &self.try_state_bytes(values)?, &self.suffix].concat())
    }
}

/// The committed B_MAX: the `int constant B_MAX = n;` line of shard.sil, as recorded with the
/// embedded templates (front-end F1; `tests/f1_templates.rs` checks it against shard.sil).
pub fn b_max() -> usize {
    crate::embedded::B_MAX
}

/// The B_MAX of shard.sil on disk (the source of `b_max()`; the generator and the W2 test).
#[cfg(feature = "compiler")]
pub fn source_b_max() -> usize {
    static B: OnceLock<usize> = OnceLock::new();
    *B.get_or_init(|| file_b_max(&read_covenant("shard.sil")))
}

#[cfg(feature = "compiler")]
fn file_b_max(shard_src: &str) -> usize {
    let line = shard_src.lines().find(|l| l.trim_start().starts_with("int constant B_MAX = ")).expect("B_MAX line");
    line.trim().trim_start_matches("int constant B_MAX = ").trim_end_matches(';').parse().expect("B_MAX value")
}

/// Ticket source with `b_max` bundle slots (spec M1 4.2).
#[cfg(feature = "compiler")]
pub fn ticket_source(b_max: usize) -> String {
    let src = std::fs::read_to_string(covenant_path("ticket.sil")).expect("read ticket.sil");
    resize_ticket_arrays(&src, b_max)
}

/// The bundle and claimed arrays are written for the committed B_MAX = n as `byte[6n]` and
/// `byte[ceil(n/8)] ` (with a trailing space) in ticket.sil and in the TicketState structs of
/// shard.sil and pool.sil, and
/// nowhere else.
#[cfg(feature = "compiler")]
fn resize_ticket_arrays(src: &str, to: usize) -> String {
    let from = source_b_max();
    src.replace(&format!("byte[{}]", 6 * from), &format!("byte[{}]", 6 * to))
        .replace(&format!("byte[{}] ", from.div_ceil(8)), &format!("byte[{}] ", to.div_ceil(8)))
}

/// Shard source with `b_max` bundle slots.
#[cfg(feature = "compiler")]
pub fn shard_source(b_max_to: usize) -> String {
    let src = std::fs::read_to_string(covenant_path("shard.sil")).expect("read shard.sil");
    let from = format!("int constant B_MAX = {};", source_b_max());
    resize_ticket_arrays(&src.replace(&from, &format!("int constant B_MAX = {b_max_to};")), b_max_to)
}

/// Covenant sources for one compile (attack and mutant tests swap one of them).
#[cfg(feature = "compiler")]
#[derive(Clone)]
pub struct Sources {
    pub ticket: String,
    pub controller: String,
    pub shard: String,
    pub params: String,
    pub marker: String,
    pub vault: String,
    pub pool: String,
    /// Spec M5 (D74): the settlement record, the share position and the vault request.
    pub record: String,
    pub position: String,
    pub request: String,
    /// The drawer (spec M7 D101: compiled unconditionally; the TEST-ONLY entry is gone).
    pub drawer: String,
}

/// Spec M7 D97 (Stage 5 fix A): the drawer's void floor, `int constant VOID_MIN = 864000;`
/// (2F, `game::VOID_MIN`); check 1 requires draw_window >= VOID_MIN.
pub fn void_min_line() -> String {
    format!("int constant VOID_MIN = {};", crate::game::VOID_MIN)
}

/// TEST-ONLY (spec M7 D103, `probe void-m7`): the line that replaces `void_min_line()` in the
/// short-void drawer variant, so a void can be shown on testnet without waiting 24 h. Only
/// `GenesisCase::templates()` with `allow_short_void_window` (refused off testnet/devnet by
/// `validate`) selects it; never deploy it. Compiled out of a `mainnet` build (SP12).
#[cfg(all(not(feature = "mainnet"), feature = "compiler"))]
pub const SHORT_VOID_LINE: &str = "int constant VOID_MIN = 0; // TEST-ONLY short void window (probe void-m7, D103): never deploy";

/// TEST-ONLY: the drawer source with the void floor removed (`void_min_line()` replaced by
/// `SHORT_VOID_LINE`). Panics unless the line occurs exactly once, as a whole line.
#[cfg(all(not(feature = "mainnet"), feature = "compiler"))]
pub fn short_void_drawer_source(src: &str) -> String {
    let line = void_min_line();
    assert_eq!(src.matches(line.as_str()).count(), 1, "drawer source: `{line}` must occur exactly once");
    assert_eq!(src.lines().filter(|l| l.trim() == line).count(), 1, "drawer source: `{line}` must be a whole line");
    src.replacen(line.as_str(), SHORT_VOID_LINE, 1)
}

#[cfg(feature = "compiler")]
pub(crate) fn read_covenant(name: &str) -> String {
    std::fs::read_to_string(covenant_path(name)).unwrap_or_else(|e| panic!("read {name}: {e}"))
}

#[cfg(feature = "compiler")]
impl Sources {
    /// The committed sources, with the ticket, shard and pool resized to `b_max`.
    pub fn committed(b_max: usize) -> Self {
        Self {
            ticket: ticket_source(b_max),
            controller: read_covenant("controller.sil"),
            shard: shard_source(b_max),
            params: read_covenant("params.sil"),
            marker: read_covenant("marker.sil"),
            vault: read_covenant("vault.sil"),
            // The pool reads the ticket state (ticket_leg), so its TicketState struct follows B_MAX.
            pool: resize_ticket_arrays(&read_covenant("pool.sil"), b_max),
            record: read_covenant("record.sil"),
            position: read_covenant("position.sil"),
            request: read_covenant("request.sil"),
            drawer: read_covenant("drawer.sil"),
        }
    }

    /// TEST-ONLY (spec M7 D103): the committed sources with the short-void drawer variant
    /// (`short_void_drawer_source`). Reached through `Templates::test_only_short_void`.
    #[cfg(all(not(feature = "mainnet"), feature = "compiler"))]
    pub fn test_only_short_void(b_max: usize) -> Self {
        let c = Self::committed(b_max);
        Self { drawer: short_void_drawer_source(&c.drawer), ..c }
    }
}

/// Compiled templates. Compile order (spec M2 D27, M3 D38, M4 D54, M5 D74): controller ->
/// drawer(controller) -> marker, vault, pool(drawer) and record -> position(vault) ->
/// request(record, position) -> ticket(pool) -> shard(ticket, controller) and
/// params(controller). The controller depends on no template: its route table (shard,
/// marker, params hashes) is state. The drawer holds pool_tpl, vault_tpl and ticket_tpl in
/// state (the pool cannot commit to the ticket hash: the ticket commits to POOL_TPL, so
/// draw() writes ticket_tpl into every pool state). The vault holds record_tpl, request_tpl
/// and position_tpl in state (D74: the position commits to VAULT_TPL and the request to
/// RECORD_TPL and POSITION_TPL); the record has no constructor constant (its request_tpl is
/// state, written by draw()).
pub struct Templates {
    pub b_max: usize,
    pub ticket: Arc<Template>,
    pub controller: Arc<Template>,
    pub shard: Arc<Template>,
    pub params: Arc<Template>,
    pub marker: Arc<Template>,
    pub vault: Arc<Template>,
    pub pool: Arc<Template>,
    pub record: Arc<Template>,
    pub position: Arc<Template>,
    pub request: Arc<Template>,
    pub drawer: Arc<Template>,
    /// TEST-ONLY marker (spec M7 D103): the drawer is the short-void variant (no void floor,
    /// `SHORT_VOID_LINE`). `GenesisCase::validate_with` accepts such templates only with
    /// `allow_short_void_window`, and that flag only on testnet or devnet. Absent from a
    /// `mainnet` build (SP12).
    #[cfg(all(not(feature = "mainnet"), feature = "compiler"))]
    pub short_void_window: bool,
}

impl Templates {
    #[cfg(feature = "compiler")]
    pub fn compile(b_max: usize) -> Self {
        Self::compile_sources(b_max, &Sources::committed(b_max))
    }

    /// Compile with a given ticket and shard source, the rest committed (M1 attack tests
    /// use look-alike templates).
    #[cfg(feature = "compiler")]
    pub fn compile_with(b_max: usize, ticket_src: &str, shard_src: &str) -> Self {
        let src = Sources { ticket: ticket_src.to_string(), shard: shard_src.to_string(), ..Sources::committed(b_max) };
        Self::compile_sources(b_max, &src)
    }

    /// Compile from given sources.
    #[cfg(feature = "compiler")]
    pub fn compile_sources(b_max: usize, src: &Sources) -> Self {
        let dummy_shard = ShardState {
            round: 1,
            shard_index: 0,
            m: 10,
            price: KAS as i64,
            d: (KAS / 10) as i64,
            s_r: 1000,
            deadline: 1_000,
            root: [0; 32],
            sales: 0,
            ref_paid: 0,
        };
        let dummy_ticket = TicketState::new(&dummy_shard, &[Combo::new([1, 2, 3, 4, 5], 1)], &Payout::Schnorr([1; 32]), b_max);
        let controller = Template::from(compile_source(&src.controller, &dummy_controller().ctor()));
        let drawer = crate::draw::compile_drawer(&src.drawer, controller.hash);
        let drawer_hash = drawer.hash;
        // M6-b K_MAX variants (measurement only, D91): the marker and pool constructor roots
        // follow the controller source's K_MAX (16 for the committed sources).
        let k_max = source_k_max(&src.controller);
        let marker = Template::from(compile_source(&src.marker, &dummy_marker(k_max).ctor(drawer_hash)));
        let vault = Template::from(compile_source(&src.vault, &dummy_vault().ctor(drawer_hash)));
        let pool = Template::from(compile_source(&src.pool, &dummy_pool(k_max).ctor(drawer_hash)));
        let record = Template::from(compile_source(&src.record, &dummy_record().ctor()));
        let position = Template::from(compile_source(&src.position, &dummy_position().ctor(vault.hash)));
        let request = Template::from(compile_source(&src.request, &dummy_request().ctor(record.hash, position.hash)));
        let ticket = Template::from(compile_source(&src.ticket, &dummy_ticket.ctor(pool.hash)));
        let shard = Template::from(compile_source(&src.shard, &dummy_shard.ctor(ticket.hash, controller.hash)));
        let params = Template::from(compile_source(&src.params, &dummy_params().ctor(controller.hash)));
        Self {
            b_max,
            ticket: Arc::new(ticket),
            controller: Arc::new(controller),
            shard: Arc::new(shard),
            params: Arc::new(params),
            marker: Arc::new(marker),
            vault: Arc::new(vault),
            pool: Arc::new(pool),
            record: Arc::new(record),
            position: Arc::new(position),
            request: Arc::new(request),
            drawer: Arc::new(drawer),
            #[cfg(all(not(feature = "mainnet"), feature = "compiler"))]
            short_void_window: src.drawer.contains(SHORT_VOID_LINE),
        }
    }

    /// The drawer template hash the marker, vault and pool commit to (D38).
    pub fn drawer_hash(&self) -> [u8; 32] {
        self.drawer.hash
    }

    /// Shared templates per B_MAX. The committed B_MAX (`b_max()`) uses the templates embedded
    /// at build time (front-end F1, `crate::embedded`); any other B_MAX is compiled from the
    /// sources, which needs the `compiler` feature. Panics where `try_get` returns an error.
    pub fn get(b_max: usize) -> Arc<Templates> {
        Self::try_get(b_max).unwrap_or_else(|e| panic!("{e}"))
    }

    /// `get`, with an error instead of a panic for a B_MAX that has no templates in this build.
    pub fn try_get(b_max: usize) -> Result<Arc<Templates>, String> {
        static CACHE: OnceLock<Mutex<HashMap<usize, Arc<Templates>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(t) = cache.lock().unwrap().get(&b_max) {
            return Ok(t.clone());
        }
        let t = Arc::new(if b_max == crate::embedded::B_MAX { Templates::embedded()? } else { Self::compile_other(b_max)? });
        Ok(cache.lock().unwrap().entry(b_max).or_insert(t).clone())
    }

    #[cfg(feature = "compiler")]
    fn compile_other(b_max: usize) -> Result<Templates, String> {
        Ok(Templates::compile(b_max))
    }

    #[cfg(not(feature = "compiler"))]
    fn compile_other(b_max: usize) -> Result<Templates, String> {
        Err(format!("b_max {b_max}: only the committed B_MAX = {} is embedded; other values need the `compiler` feature", crate::embedded::B_MAX))
    }

    /// The templates embedded at build time (`crates/lottery/templates/`, committed B_MAX).
    pub fn embedded() -> Result<Templates, String> {
        let e = crate::embedded::artifacts()?;
        let tpl = |name: &str| -> Result<Arc<Template>, String> {
            let art = e.get(name).ok_or_else(|| format!("embedded template `{name}` missing"))?.clone();
            Ok(Arc::new(Template::try_from_artifact(art).map_err(|e| format!("embedded template `{name}`: {e}"))?))
        };
        Ok(Self {
            b_max: crate::embedded::B_MAX,
            ticket: tpl("ticket")?,
            controller: tpl("controller")?,
            shard: tpl("shard")?,
            params: tpl("params")?,
            marker: tpl("marker")?,
            vault: tpl("vault")?,
            pool: tpl("pool")?,
            record: tpl("record")?,
            position: tpl("position")?,
            request: tpl("request")?,
            drawer: tpl("drawer")?,
            #[cfg(all(not(feature = "mainnet"), feature = "compiler"))]
            short_void_window: false,
        })
    }

    /// TEST-ONLY (spec M7 D103, `probe void-m7`): the template set with the short-void drawer
    /// variant (every template that commits to the drawer hash differs from `get`'s). Use it
    /// only through `GenesisCase::templates()` with `allow_short_void_window`. Compiled out
    /// of a `mainnet` build (SP12).
    #[cfg(all(not(feature = "mainnet"), feature = "compiler"))]
    pub fn test_only_short_void(b_max: usize) -> Arc<Templates> {
        static CACHE: OnceLock<Mutex<HashMap<usize, Arc<Templates>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(t) = cache.lock().unwrap().get(&b_max) {
            return t.clone();
        }
        let t = Arc::new(Templates::compile_sources(b_max, &Sources::test_only_short_void(b_max)));
        assert!(t.short_void_window);
        cache.lock().unwrap().entry(b_max).or_insert(t).clone()
    }

    pub fn shard_redeem(&self, s: &ShardState) -> Vec<u8> {
        self.shard.redeem(&s.values())
    }

    pub fn shard_spk(&self, s: &ShardState) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.shard_redeem(s))
    }

    pub fn ticket_spk(&self, t: &TicketState) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.ticket.redeem(&t.values()))
    }

    pub fn controller_redeem(&self, c: &ControllerState) -> Vec<u8> {
        self.controller.redeem(&c.values())
    }

    pub fn controller_spk(&self, c: &ControllerState) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.controller_redeem(c))
    }

    pub fn params_redeem(&self, p: &ParamsState) -> Vec<u8> {
        self.params.redeem(&p.values())
    }

    pub fn params_spk(&self, p: &ParamsState) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.params_redeem(p))
    }

    pub fn marker_redeem(&self, m: &MarkerState) -> Vec<u8> {
        self.marker.redeem(&m.values())
    }

    pub fn marker_spk(&self, m: &MarkerState) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.marker_redeem(m))
    }

    pub fn vault_redeem(&self, v: &VaultState) -> Vec<u8> {
        self.vault.redeem(&v.values())
    }

    pub fn vault_spk(&self, v: &VaultState) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.vault_redeem(v))
    }

    pub fn pool_redeem(&self, p: &PoolState) -> Vec<u8> {
        self.pool.redeem(&p.values())
    }

    pub fn pool_spk(&self, p: &PoolState) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.pool_redeem(p))
    }

    pub fn record_redeem(&self, r: &RecordState) -> Vec<u8> {
        self.record.redeem(&r.values())
    }

    pub fn record_spk(&self, r: &RecordState) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.record_redeem(r))
    }

    pub fn position_redeem(&self, p: &PositionState) -> Vec<u8> {
        self.position.redeem(&p.values())
    }

    pub fn position_spk(&self, p: &PositionState) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.position_redeem(p))
    }

    pub fn request_redeem(&self, r: &RequestState) -> Vec<u8> {
        self.request.redeem(&r.values())
    }

    pub fn request_spk(&self, r: &RequestState) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.request_redeem(r))
    }

    pub fn drawer_redeem(&self, d: &DrawerState) -> Vec<u8> {
        self.drawer.redeem(&d.values())
    }

    pub fn drawer_spk(&self, d: &DrawerState) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.drawer_redeem(d))
    }
}

/// The K_MAX of a controller source: its `int constant K_MAX = n;` line, or the committed
/// K_MAX if there is none (spec M6 A5 M6-b: in-memory K_MAX variants size the marker and
/// pool constructor roots from it; the committed sources give K_MAX = 16).
pub fn source_k_max(controller_src: &str) -> usize {
    controller_src
        .lines()
        .find_map(|l| l.trim().strip_prefix("int constant K_MAX = ")?.strip_suffix(';')?.trim().parse().ok())
        .unwrap_or(K_MAX)
}

/// Minimum relay fee in sompi, as the rusty-kaspa v2.1.0 mempool computes it
/// (`mining/src/mempool/check_transaction_standard.rs:71-76, 92-100`): fee mass =
/// max(compute, ceil(transient x cofactor)), with the transient cofactor L_compute /
/// L_transient = 0.5 of the block mass limits (`consensus/core/src/mass/mod.rs:178-188,
/// 256-259`; limits `consensus/core/src/config/params.rs:626`; the node passes its block
/// mass limits to the mempool, `kaspad/src/daemon.rs:648-652`), times
/// DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE = 100,000 sompi per kg = 100 sompi per gram
/// (`mining/src/mempool/config.rs:20`). Storage mass is not part of it
/// (`check_transaction_standard.rs:67-70`).
pub fn relay_floor_fee(m: &harness::Masses) -> u64 {
    let cof = harness::MAINNET_PARAMS.block_mass_limits.cofactors();
    let fee_mass = kaspa_consensus_core::mass::NonContextualMasses::new(m.compute, m.transient).normalized_max(&cof);
    fee_mass * harness::MIN_FEERATE_SOMPI_PER_GRAM
}

/// Spec M8 D120: the fee of a transaction of fee mass `mass` (the builder's exact mass, as
/// `relay_floor_fee` counts it) at `rate` sompi per gram, the rate raised to the floor of
/// 100 sompi per gram (design section 9: "It takes fees from the node's estimator, at
/// least 100 sompi per gram"; the mempool's DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
/// rusty-kaspa v2.1.0 `mining/src/mempool/config.rs:20`, `harness::MIN_FEERATE_SOMPI_PER_GRAM`).
/// Checked: an overflowing product is an error, never a wrapped fee.
pub fn fee_for(mass: u64, rate: u64) -> Result<u64, String> {
    let r = rate.max(harness::MIN_FEERATE_SOMPI_PER_GRAM);
    mass.checked_mul(r).ok_or_else(|| format!("fee_for: mass {mass} x rate {r} sompi/gram overflows u64"))
}

/// Placeholder states for compiling (the template hash does not depend on state values).
#[cfg(feature = "compiler")]
fn dummy_controller() -> ControllerState {
    ControllerState {
        round: 1,
        k: 1,
        m: 10,
        price: KAS as i64,
        d: (KAS / 10) as i64,
        deadline: 1_000,
        s_f: 500,
        s_r: 1_000,
        fee_class: 0,
        fee_key: [0; 33],
        jackpot: 0,
        j_boost: 0,
        g2: 0,
        liability: 0,
        v_rec: 0,
        acc: 0,
        drawn: 1,
        shard_tpl: [0; 32],
        marker_tpl: [0; 32],
        params_tpl: [0; 32],
        reserve: 1,
        s_c: 50,
        s_d: 50,
        st: [400, 300, 300, 0, 0],
        e: 1_500,
        j: 1_000,
        g: 100,
        margin: 15_000,
        m_min: 2,
        m_max: 50,
        cap: 2_000,
        round_len: 864_000,
        d_min: 72_000,
        seed_offset: 600,
    }
}

#[cfg(feature = "compiler")]
fn dummy_params() -> ParamsState {
    ParamsState { round: 1, admin_hash: [0; 32], nonce: 0, used: 0, next_s_f: 500, next_s_r: 1_000, next_fee_class: 0, next_fee_key: [0; 33] }
}

#[cfg(feature = "compiler")]
fn dummy_marker(k_max: usize) -> MarkerState {
    MarkerState {
        round: 1,
        k: 1,
        roots: vec![0; 32 * k_max],
        sales: 0,
        ref_paid: 0,
        m: 10,
        jackpot: 0,
        j_boost: 0,
        g2: 0,
        s_f: 500,
        s_r: 1_000,
        s_c: 50,
        s_d: 50,
        st: [400, 300, 300, 0, 0],
        funder_class: 0,
        funder_key: [0; 33],
    }
}

#[cfg(feature = "compiler")]
fn dummy_vault() -> VaultState {
    VaultState {
        round: 0,
        total_assets: 0,
        total_shares: 0,
        pend_dep: 0,
        pend_wsh: 0,
        liability: 0,
        record_tpl: [0; 32],
        request_tpl: [0; 32],
        position_tpl: [0; 32],
    }
}

#[cfg(feature = "compiler")]
fn dummy_record() -> RecordState {
    RecordState {
        round: 1,
        request_tpl: [0; 32],
        refund: 0,
        dep_rem: 0,
        mint_rem: 0,
        wsh_rem: 0,
        burn_rem: 0,
        out_rem: 0,
        funder_class: 0,
        funder_key: [0; 33],
    }
}

#[cfg(feature = "compiler")]
fn dummy_position() -> PositionState {
    PositionState { round: 0, shares: 1, owner_class: 0, owner_key: [0; 33] }
}

#[cfg(feature = "compiler")]
fn dummy_request() -> RequestState {
    RequestState { round: 1, kind: 0, amount: 0, owner_class: 0, owner_key: [0; 33] }
}

#[cfg(feature = "compiler")]
fn dummy_pool(k_max: usize) -> PoolState {
    PoolState {
        round: 1,
        tier: 0,
        slice: 0,
        win: [1, 2, 3, 4, 5, 1],
        m: 10,
        k: 1,
        roots: vec![0; 32 * k_max],
        deadline: 0,
        ticket_tpl: [0; 32],
        reserve: 0,
        funder_class: 0,
        funder_key: [0; 33],
    }
}

pub fn funding_keypair() -> Keypair {
    Keypair::from_seckey_slice(SECP256K1, &FUNDING_SK).unwrap()
}

/// Schnorr P2PK script of the test funding key.
pub fn funding_spk() -> ScriptPublicKey {
    spk_of(&FUNDING_SK)
}

/// Schnorr P2PK script of a secret key.
pub fn spk_of(sk: &[u8; 32]) -> ScriptPublicKey {
    let kp = Keypair::from_seckey_slice(SECP256K1, sk).unwrap();
    Payout::Schnorr(kp.x_only_public_key().0.serialize()).spk()
}

/// A purchase, before it is turned into a transaction.
#[derive(Clone)]
pub struct BuyCase {
    pub b_max: usize,
    pub shard: ShardState,
    /// Reserve R carried by the shard on top of S - P_R.
    pub reserve: u64,
    /// Creation DAA score of the spent shard UTXO.
    pub shard_daa: u64,
    pub tree: Tree,
    pub bundle: Vec<Combo>,
    pub with_ref: bool,
    pub referrer: Payout,
    pub payout: Payout,
    pub funding: Vec<u64>,
    pub change: bool,
    pub fee: u64,
    /// Covenant ID of the instance.
    pub cov: Hash,
    /// Key of the funding inputs and the change output.
    pub funding_sk: [u8; 32],
}

impl BuyCase {
    /// A valid one-combination buy on a fresh shard of round 1 (m = 10, price 1 KAS,
    /// d = TEST_DEPOSIT, s_R = 10 %, R = TEST_RESERVE), no referral, with change.
    pub fn new(b_max: usize) -> Self {
        let tree = Tree::new(10);
        Self {
            b_max,
            shard: ShardState {
                round: 1,
                shard_index: 0,
                m: 10,
                price: KAS as i64,
                d: TEST_DEPOSIT as i64,
                s_r: 1_000,
                deadline: 10_000,
                root: tree.root(),
                sales: 0,
                ref_paid: 0,
            },
            reserve: crate::params::TEST_RESERVE,
            shard_daa: 5_000,
            tree,
            bundle: vec![Combo::new([3, 7, 12, 19, 28], 4)],
            with_ref: false,
            referrer: Payout::Schnorr([0x33; 32]),
            payout: Payout::Schnorr([0x22; 32]),
            funding: vec![100 * KAS],
            change: true,
            fee: KAS / 2,
            cov: COV,
            funding_sk: FUNDING_SK,
        }
    }

    /// Replace the tree (and the shard root) with `tree`.
    pub fn with_tree(mut self, tree: Tree) -> Self {
        self.shard.m = tree.m as i64;
        self.shard.root = tree.root();
        self.tree = tree;
        self
    }

    pub fn b(&self) -> u64 {
        self.bundle.len() as u64
    }

    pub fn gross(&self) -> u64 {
        self.b() * self.shard.price as u64
    }

    /// Referral amount r (0 without a referral output).
    pub fn referral(&self) -> u64 {
        if self.with_ref { self.gross() * self.shard.s_r as u64 / 10_000 } else { 0 }
    }

    pub fn shard_value(&self) -> u64 {
        // i128: range tests build shards with out-of-range S and P_R.
        (self.shard.sales as i128 - self.shard.ref_paid as i128 + self.reserve as i128) as u64
    }

    /// Per-slot leaves and siblings, and the tree after all slots.
    pub fn proofs(&self) -> (Vec<u8>, Vec<u8>, Tree) {
        let mut t = self.tree.clone();
        let (mut leaves, mut sibs) = (vec![], vec![]);
        for c in &self.bundle {
            let r = c.rank();
            leaves.extend(t.counts_bytes(r));
            sibs.extend(t.siblings(r));
            t.increment(r, c.bonus);
        }
        (leaves, sibs, t)
    }

    pub fn successor(&self) -> ShardState {
        let (_, _, t) = self.proofs();
        let mut s = self.shard.clone();
        s.root = t.root();
        s.sales += self.gross() as i64;
        s.ref_paid += self.referral() as i64;
        s
    }

    pub fn ticket(&self) -> TicketState {
        TicketState::new(&self.shard, &self.bundle, &self.payout, self.b_max)
    }

    pub fn args(&self, t: &Templates) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        let (leaves, sibs, _) = self.proofs();
        vec![
            Int(self.b() as i64),
            Bytes(bundle_bytes(&self.bundle)),
            Bytes(leaves),
            Bytes(sibs),
            Bool(self.with_ref),
            Byte(self.payout.class()),
            Bytes(self.payout.key33().to_vec()),
            Bytes(t.ticket.prefix.clone()),
            Bytes(t.ticket.suffix.clone()),
        ]
    }

    pub fn build(&self) -> Built {
        self.build_with(Templates::get(self.b_max))
    }

    /// Build against given templates (mutation tests compile a shard with one check removed).
    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        let mut inputs = vec![In {
            entry: UtxoEntry::new(self.shard_value(), t.shard_spk(&self.shard), self.shard_daa, false, Some(self.cov)),
            kind: InKind::Shard(t.shard_redeem(&self.shard)),
        }];
        for &v in &self.funding {
            inputs.push(In { entry: UtxoEntry::new(v, spk_of(&self.funding_sk), 0, false, None), kind: InKind::Funding });
        }
        let succ = self.successor();
        let shard_out = self.shard_value() + self.gross() - self.referral();
        let ticket_value = self.b() * self.shard.d as u64;
        let mut outputs = vec![
            TransactionOutput::with_covenant(shard_out, t.shard_spk(&succ), Some(CovenantBinding::new(0, self.cov))),
            TransactionOutput::with_covenant(ticket_value, t.ticket_spk(&self.ticket()), Some(CovenantBinding::new(0, self.cov))),
        ];
        if self.with_ref {
            outputs.push(TransactionOutput::new(self.referral(), self.referrer.spk()));
        }
        let total_in: u64 = inputs.iter().map(|i| i.entry.amount).sum();
        let total_out: u64 = outputs.iter().map(|o| o.value).sum();
        let change = total_in.checked_sub(total_out + self.fee).expect("funding covers the buy");
        if self.change {
            outputs.push(TransactionOutput::new(change, spk_of(&self.funding_sk)));
        } else {
            assert_eq!(change, 0, "no-change case needs exact funding");
        }
        let outpoints = (0..inputs.len() as u32).map(outpoint).collect();
        Built {
            templates: t.clone(),
            args: self.args(&t),
            inputs,
            outputs,
            genesis: vec![],
            lock_time: 0,
            shard_budget: None,
            budget_delta: 0,
            outpoints,
            funding_sk: self.funding_sk,
            sim: None,
        }
    }
}

#[derive(Clone)]
pub enum InKind {
    /// The shard: sigscript = buy(args) + push(redeem).
    Shard(Vec<u8>),
    /// A Schnorr P2PK input signed with `FUNDING_SK`.
    Funding,
    /// Any other input: a covenant UTXO spent through `buy` with its own redeem script
    /// (parallel-group attacks), with its own arguments.
    Covenant { redeem: Vec<u8>, args: Vec<ArtifactValue> },
    /// A fixed sigscript and units estimate (e.g. a ticket spent through its placeholder entry).
    Raw(Vec<u8>),
    /// A covenant input spent through `entry` of `template` with `args`:
    /// sigscript = entry(args) + push(redeem). Used by `close()` (controller, params and
    /// shard legs) and `set_params()`.
    Entry { template: Arc<Template>, entry: String, args: Vec<ArtifactValue>, redeem: Vec<u8> },
    /// Spec M5 D67: a plain (zero-ID) input spent by a share position's owner, whose
    /// scriptPubKey is the owner's D11 script (`request_withdraw` check 5). It may also fund
    /// the transaction. Signed in `Built::finish` with the owner's key.
    Owner(OwnerKey),
    /// Front-end F1 part B: the same owner input when the owner signs outside this library
    /// (a browser wallet). Only its public script is known; `Built::finish_unsigned` budgets
    /// it as `finish()` budgets the `Owner` input of the same key (`owner_signed_units`) and
    /// lists it for signing. `finish()` cannot sign it and refuses it.
    OwnerPub(OwnerPub),
}

/// The public side of an owner key (front-end F1 part B, spec M5 D67): the owner's D11
/// script without the secret. `P2sh` holds the x-only key of the wrapped Schnorr P2PK redeem
/// script (the only P2SH owner class D67 defines).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerPub {
    Schnorr([u8; 32]),
    Ecdsa([u8; 33]),
    P2sh([u8; 32]),
}

impl OwnerPub {
    /// The P2SH redeem script (Schnorr P2PK of the key); `None` for the P2PK classes.
    pub fn redeem(&self) -> Option<Vec<u8>> {
        match self {
            OwnerPub::P2sh(x) => Some(Payout::Schnorr(*x).script()),
            _ => None,
        }
    }

    /// The owner's D11 payout (a position's owner_class and owner_key).
    pub fn payout(&self) -> Payout {
        match self {
            OwnerPub::Schnorr(x) => Payout::Schnorr(*x),
            OwnerPub::Ecdsa(k) => Payout::Ecdsa(*k),
            OwnerPub::P2sh(x) => {
                let spk = pay_to_script_hash_script(&Payout::Schnorr(*x).script());
                // OP_BLAKE2B OP_DATA_32 <hash> OP_EQUAL: the hash is bytes 2..34 of the 35.
                let mut h = [0u8; 32];
                h.copy_from_slice(&spk.script()[2..34]);
                Payout::P2sh(h)
            }
        }
    }

    pub fn spk(&self) -> ScriptPublicKey {
        self.payout().spk()
    }

    /// A placeholder sigscript of the final size (as `OwnerKey::placeholder`).
    pub fn placeholder(&self) -> Vec<u8> {
        match self {
            OwnerPub::Schnorr(_) | OwnerPub::Ecdsa(_) => vec![0; P2PK_SIG_LEN],
            OwnerPub::P2sh(x) => {
                let mut v = vec![0; P2PK_SIG_LEN];
                // The redeem is 34 bytes: OP_DATA_34 <redeem>, never a failing push.
                v.push(34);
                v.extend_from_slice(&Payout::Schnorr(*x).script());
                v
            }
        }
    }

    /// The class index (0 Schnorr, 1 ECDSA, 2 P2SH), as the D11 payout class.
    pub fn class(&self) -> u8 {
        self.payout().class()
    }
}

impl OwnerKey {
    /// The public side of this key (what a browser wallet's owner input is).
    pub fn public(&self) -> OwnerPub {
        match (self, self.payout()) {
            (OwnerKey::Schnorr(_), Payout::Schnorr(x)) => OwnerPub::Schnorr(x),
            (OwnerKey::Ecdsa(_), Payout::Ecdsa(k)) => OwnerPub::Ecdsa(k),
            _ => OwnerPub::P2sh(self.keypair().x_only_public_key().0.serialize()),
        }
    }
}

/// The script units of a signed owner input of `o`'s class, measured once per class by
/// signing a one-input transaction with a stand-in key of that class and running it
/// (front-end F1 part B). An owner input's script (P2PK Schnorr, P2PK ECDSA, or a P2SH-wrapped
/// Schnorr P2PK) reads no other input or output and its data pushes have fixed sizes (a
/// 65-byte signature push, a 32- or 33-byte key, a 34-byte redeem), so its units depend on
/// the class alone, not on the key, the signature or the transaction around it.
/// `tests/f1_unsigned.rs` checks that `finish()` (which signs with the real key and re-runs)
/// commits the same budget for many keys of every class.
pub fn owner_signed_units(o: &OwnerPub) -> Result<u64, String> {
    static UNITS: OnceLock<[Result<u64, String>; 3]> = OnceLock::new();
    let all = UNITS.get_or_init(|| {
        // Any valid secret key; it signs a throwaway transaction and nothing else.
        const STAND_IN: [u8; 32] = [0x5a; 32];
        [OwnerKey::Schnorr(STAND_IN), OwnerKey::Ecdsa(STAND_IN), OwnerKey::P2sh(STAND_IN)].map(|k| {
            let entry = UtxoEntry::new(KAS, k.spk(), 0, false, None);
            let input = TransactionInput::new_with_compute_budget(outpoint(0), vec![], 0, u16::MAX);
            let mut tx = Transaction::new(1, vec![input], vec![TransactionOutput::new(KAS / 2, k.spk())], 0, Default::default(), 0, vec![]);
            tx.inputs[0].signature_script = k.sign(&PopulatedTransaction::new(&tx, vec![entry.clone()]), 0);
            execute(&tx, &[entry], 0, false).map_err(|e| format!("owner units of class {}: {e}", k.payout().class()))
        })
    });
    all[o.class().min(2) as usize].clone()
}

/// The key behind a D11 owner script (spec M5 D67): P2PK Schnorr, P2PK ECDSA, or P2SH whose
/// redeem script is the Schnorr P2PK script of the key (a P2SH-wrapped P2PK).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerKey {
    Schnorr([u8; 32]),
    Ecdsa([u8; 32]),
    P2sh([u8; 32]),
}

impl OwnerKey {
    fn keypair(&self) -> Keypair {
        let sk = match self {
            OwnerKey::Schnorr(k) | OwnerKey::Ecdsa(k) | OwnerKey::P2sh(k) => k,
        };
        Keypair::from_seckey_slice(SECP256K1, sk).unwrap()
    }

    /// The P2SH redeem script (Schnorr P2PK of the key); `None` for the P2PK classes.
    pub fn redeem(&self) -> Option<Vec<u8>> {
        match self {
            OwnerKey::P2sh(_) => Some(Payout::Schnorr(self.keypair().x_only_public_key().0.serialize()).script()),
            _ => None,
        }
    }

    /// The owner's D11 payout (the position's owner_class and owner_key).
    pub fn payout(&self) -> Payout {
        let kp = self.keypair();
        match self {
            OwnerKey::Schnorr(_) => Payout::Schnorr(kp.x_only_public_key().0.serialize()),
            OwnerKey::Ecdsa(_) => Payout::Ecdsa(kp.public_key().serialize()),
            OwnerKey::P2sh(_) => {
                let spk = pay_to_script_hash_script(&self.redeem().unwrap());
                // OP_BLAKE2B OP_DATA_32 <hash> OP_EQUAL (rusty-kaspa v2.1.0
                // crypto/txscript/src/standard.rs:31-37).
                Payout::P2sh(spk.script()[2..34].try_into().unwrap())
            }
        }
    }

    pub fn spk(&self) -> ScriptPublicKey {
        let spk = self.payout().spk();
        if let OwnerKey::P2sh(_) = self {
            debug_assert_eq!(spk, pay_to_script_hash_script(&self.redeem().unwrap()));
        }
        spk
    }

    /// The signature script of input `i` of `tx` (SIG_HASH_ALL): Schnorr as `sign_input`;
    /// ECDSA: OP_DATA_65 <compact sig || type> over the ECDSA sighash (rusty-kaspa v2.1.0
    /// consensus/core/src/hashing/sighash.rs:282-293, crypto/txscript/src/opcodes/mod.rs:804
    /// OpCheckSigECDSA, crypto/txscript/src/lib.rs:885-892 `Signature::from_compact`); P2SH:
    /// the Schnorr signature push, then the redeem script push. Public since F1 part B: the
    /// tests sign an unsigned withdrawal's owner input with it, as a wallet would.
    pub fn sign(&self, tx: &PopulatedTransaction, i: usize) -> Vec<u8> {
        let sk = self.keypair().secret_bytes();
        match self {
            OwnerKey::Schnorr(_) => sign_input(tx, i, &sk, SIG_HASH_ALL),
            OwnerKey::Ecdsa(_) => {
                let reused = kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync::new();
                let h = kaspa_consensus_core::hashing::sighash::calc_ecdsa_signature_hash(tx, i, SIG_HASH_ALL, &reused);
                let msg = secp256k1::Message::from_digest_slice(h.as_bytes().as_slice()).unwrap();
                let sk = secp256k1::SecretKey::from_slice(&sk).unwrap();
                let sig = SECP256K1.sign_ecdsa(&msg, &sk).serialize_compact();
                std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect()
            }
            OwnerKey::P2sh(_) => push_redeem(sign_input(tx, i, &sk, SIG_HASH_ALL), &self.redeem().unwrap()),
        }
    }

    /// A placeholder sigscript of the final size (for the units probe and the storage mass).
    fn placeholder(&self) -> Vec<u8> {
        match self {
            OwnerKey::Schnorr(_) | OwnerKey::Ecdsa(_) => vec![0; P2PK_SIG_LEN],
            OwnerKey::P2sh(_) => push_redeem(vec![0; P2PK_SIG_LEN], &self.redeem().unwrap()),
        }
    }
}

#[derive(Clone)]
pub struct In {
    pub entry: UtxoEntry,
    pub kind: InKind,
}

/// An editable transaction.
#[derive(Clone)]
pub struct Built {
    pub templates: Arc<Templates>,
    pub args: Vec<ArtifactValue>,
    pub inputs: Vec<In>,
    pub outputs: Vec<TransactionOutput>,
    pub genesis: Vec<GenesisCovenantGroup>,
    /// Transaction lock time (absolute DAA score; `require(tx.daa >= T)` in `close()`).
    pub lock_time: u64,
    /// Override for input 0's committed compute budget (the shard in `buy()`, the leader in
    /// `close()`, the params in `set_params()`).
    pub shard_budget: Option<u16>,
    /// Added to the covering budget of input 0 (attack A11 and CA14 use -1).
    pub budget_delta: i32,
    /// Outpoints of the inputs (dummy ones in local tests).
    pub outpoints: Vec<TransactionOutpoint>,
    pub funding_sk: [u8; 32],
    /// Spec M7 (A7): the fabricated selected chain the scripts see through
    /// `OpChainblockSeqCommit` (`harness::execute_on_chain`). `None` runs without an accessor
    /// (`harness::execute`; the opcode then fails with InvalidOpcode). Set by `DrawCase`.
    pub sim: Option<Arc<ChainSim>>,
}

pub fn push_redeem(sig: Vec<u8>, redeem: &[u8]) -> Vec<u8> {
    try_push_redeem(sig, redeem).unwrap_or_else(|e| panic!("{e}"))
}

/// `push_redeem` with an error instead of a panic (F1 part B).
pub fn try_push_redeem(mut sig: Vec<u8>, redeem: &[u8]) -> Result<Vec<u8>, String> {
    sig.extend_from_slice(&ScriptBuilder::new().add_data(redeem).map_err(|e| format!("push redeem: {e}"))?.drain());
    Ok(sig)
}

fn try_buy_sigscript(t: &Templates, args: &[ArtifactValue], redeem: &[u8]) -> Result<Vec<u8>, String> {
    try_entry_sigscript(&t.shard, "buy", args, redeem)
}

/// Sigscript calling `entry` of `template` with `args`, followed by the redeem script.
///
/// `entry` is either an entry name (encoded through the ABI; the entry must exist) or a
/// full dispatch signature such as `claim(int,byte[],byte[],int,int)` (spec M4 Stage 2,
/// S2-1). With a signature, an entry of that name in the template is encoded through the
/// ABI and its dispatch tag must equal the signature's (so a covenant whose entry
/// signature drifts from the builder's fails loudly); an entry the template does not have
/// yet is encoded by hand (`planned_sig_script`), so the script rejects it at dispatch.
pub fn entry_sigscript(template: &Template, entry: &str, args: &[ArtifactValue], redeem: &[u8]) -> Vec<u8> {
    try_entry_sigscript(template, entry, args, redeem).unwrap_or_else(|e| panic!("{e}"))
}

/// `entry_sigscript` with an error instead of a panic (F1 part B); the same bytes when it
/// succeeds.
pub fn try_entry_sigscript(template: &Template, entry: &str, args: &[ArtifactValue], redeem: &[u8]) -> Result<Vec<u8>, String> {
    let Some(open) = entry.find('(') else {
        let sig = encode_contract_entry_sig_script(&template.art, template.name(), entry, args)
            .map_err(|e| format!("encode {}.{entry} args: {e}", template.name()))?;
        return try_push_redeem(sig, redeem);
    };
    let name = &entry[..open];
    let contract = template.art.contracts.values().next().ok_or("template artifact without a contract")?;
    let sig = match contract.entries.get(name) {
        Some(e) => {
            if e.dispatch_tag.as_bytes() != &dispatch_tag(entry) {
                return Err(format!("{}.{name}: the compiled entry's signature differs from the builder's `{entry}`", template.name()));
            }
            encode_contract_entry_sig_script(&template.art, template.name(), name, args).map_err(|e| format!("encode {}.{name} args: {e}", template.name()))?
        }
        None => try_planned_sig_script(entry, args)?,
    };
    try_push_redeem(sig, redeem)
}

/// SilverScript v1.0.0 dispatch tag of an entry signature `name(type,...)`: the first 4
/// bytes of blake3(signature) (`silverscript-lang/src/compiler/compile/helpers.rs:154-158`;
/// type names as `write_dispatch_type_name`, e.g. `int`, `byte[]`, `byte[33]`, `bool`).
pub fn dispatch_tag(signature: &str) -> [u8; 4] {
    blake3::hash(signature.as_bytes()).as_bytes()[..4].try_into().unwrap()
}

/// The sigscript of an entry by signature, encoded as the ABI does
/// (`silverscript-abi/src/lib.rs` `encode_entry_sig_script`/`push_sig_arg`): each argument
/// pushed in order (int and bool with `add_i64`, byte with a 1-byte push, bytes with
/// `add_data`), then the dispatch tag. Without the redeem push.
pub fn planned_sig_script(signature: &str, args: &[ArtifactValue]) -> Vec<u8> {
    try_planned_sig_script(signature, args).unwrap_or_else(|e| panic!("{e}"))
}

/// `planned_sig_script` with an error instead of a panic (F1 part B).
pub fn try_planned_sig_script(signature: &str, args: &[ArtifactValue]) -> Result<Vec<u8>, String> {
    let mut b = ScriptBuilder::new();
    fn e(what: &'static str) -> impl Fn(kaspa_txscript::script_builder::ScriptBuilderError) -> String {
        move |err| format!("planned_sig_script: push {what}: {err}")
    }
    for a in args {
        match a {
            ArtifactValue::Int(x) => {
                b.add_i64(*x).map_err(e("int"))?;
            }
            ArtifactValue::Bool(x) => {
                b.add_i64(*x as i64).map_err(e("bool"))?;
            }
            ArtifactValue::Byte(x) => {
                b.add_data(&[*x]).map_err(e("byte"))?;
            }
            ArtifactValue::Bytes(x) => {
                b.add_data(x).map_err(e("bytes"))?;
            }
            other => return Err(format!("planned_sig_script: unsupported argument {other:?}")),
        }
    }
    b.add_data(&dispatch_tag(signature)).map_err(e("tag"))?;
    Ok(b.drain())
}

pub struct Final {
    pub tx: Transaction,
    pub entries: Vec<UtxoEntry>,
    /// Script units used by each input (unlimited run).
    pub units: Vec<u64>,
    /// The chain the inputs run against (`Built::sim`).
    pub sim: Option<Arc<ChainSim>>,
}

/// Run input `i` of `tx` through the engine, with the `ChainSim` accessor when `sim` is set
/// (spec M7 A7) and without one otherwise.
pub fn run_script(tx: &Transaction, entries: &[UtxoEntry], i: usize, enforce_budget: bool, sim: Option<&ChainSim>) -> Result<u64, TxScriptError> {
    match sim {
        Some(s) => execute_on_chain(tx, entries, i, enforce_budget, s),
        None => execute(tx, entries, i, enforce_budget),
    }
}

impl Built {
    /// Replace the spent shard's state and value (input 0).
    pub fn set_shard_input(&mut self, s: &ShardState, value: u64) {
        let redeem = self.templates.shard_redeem(s);
        self.inputs[0].entry =
            UtxoEntry::new(value, pay_to_script_hash_script(&redeem), self.inputs[0].entry.block_daa_score, false, self.inputs[0].entry.covenant_id);
        self.inputs[0].kind = InKind::Shard(redeem);
    }

    /// Replace output 0's script with the given successor state.
    pub fn set_successor(&mut self, s: &ShardState) {
        self.outputs[0].script_public_key = self.templates.shard_spk(s);
    }

    /// Arguments of input `i` when it is an `InKind::Entry` (close, close_leg, set_params).
    pub fn entry_args_mut(&mut self, i: usize) -> &mut Vec<ArtifactValue> {
        match &mut self.inputs[i].kind {
            InKind::Entry { args, .. } => args,
            _ => panic!("input {i} is not an entry input"),
        }
    }

    /// Replace input `i`'s covenant state (redeem and P2SH script), keeping its value, DAA
    /// score and covenant ID. Only for `InKind::Entry` inputs.
    pub fn set_entry_redeem(&mut self, i: usize, redeem: Vec<u8>) {
        let e = &self.inputs[i].entry;
        self.inputs[i].entry = UtxoEntry::new(e.amount, pay_to_script_hash_script(&redeem), e.block_daa_score, false, e.covenant_id);
        match &mut self.inputs[i].kind {
            InKind::Entry { redeem: r, .. } => *r = redeem,
            _ => panic!("input {i} is not an entry input"),
        }
    }

    /// Replace output 1's script with the given ticket state.
    pub fn set_ticket(&mut self, t: &TicketState) {
        self.outputs[1].script_public_key = self.templates.ticket_spk(t);
    }

    /// Move `delta` sompi between the last (change) output and output `idx`.
    pub fn shift_value(&mut self, idx: usize, delta: i64) {
        let last = self.outputs.len() - 1;
        self.outputs[idx].value = (self.outputs[idx].value as i64 + delta) as u64;
        self.outputs[last].value = (self.outputs[last].value as i64 - delta) as u64;
    }

    fn op(&self, n: usize) -> TransactionOutpoint {
        self.outpoints.get(n).copied().unwrap_or_else(|| outpoint(n as u32))
    }

    fn try_sigscripts(&self) -> Result<Vec<Vec<u8>>, String> {
        self.inputs
            .iter()
            .map(|i| match &i.kind {
                InKind::Shard(redeem) => try_buy_sigscript(&self.templates, &self.args, redeem),
                InKind::Funding => Ok(vec![0; P2PK_SIG_LEN]),
                InKind::Covenant { redeem, args } => try_buy_sigscript(&self.templates, args, redeem),
                InKind::Raw(sig) => Ok(sig.clone()),
                InKind::Entry { template, entry, args, redeem } => try_entry_sigscript(template, entry, args, redeem),
                InKind::Owner(k) => Ok(k.placeholder()),
                InKind::OwnerPub(o) => Ok(o.placeholder()),
            })
            .collect()
    }

    fn assemble(&self, sigs: &[Vec<u8>], budgets: &[u16]) -> Transaction {
        self.try_assemble(sigs, budgets).unwrap_or_else(|e| panic!("{e}"))
    }

    fn try_assemble(&self, sigs: &[Vec<u8>], budgets: &[u16]) -> Result<Transaction, String> {
        let inputs = sigs
            .iter()
            .zip(budgets)
            .enumerate()
            .map(|(n, (s, &b))| TransactionInput::new_with_compute_budget(self.op(n), s.clone(), 0, b))
            .collect();
        let mut tx = Transaction::new(1, inputs, self.outputs.clone(), self.lock_time, Default::default(), 0, vec![]);
        if !self.genesis.is_empty() {
            tx.populate_genesis_covenants(&self.genesis).map_err(|e| format!("genesis groups: {e}"))?;
        }
        Ok(tx)
    }

    /// The inputs a signer outside this library must sign (front-end F1 part B), with the
    /// owner each one's script names: every funding input (a Schnorr P2PK: its entry's
    /// script) and every owner input. An `Owner` input (secret key held here) counts as its
    /// public side: `finish_unsigned` does not sign it either.
    pub fn external_signers(&self) -> Result<Vec<(usize, OwnerPub)>, String> {
        let mut v = vec![];
        for (n, i) in self.inputs.iter().enumerate() {
            match &i.kind {
                InKind::Funding => {
                    let s = i.entry.script_public_key.script();
                    // OP_DATA_32 <x-only key> OP_CHECKSIG, version 0.
                    if i.entry.script_public_key.version() != 0 || s.len() != 34 || s[0] != 0x20 || s[33] != 0xac {
                        return Err(format!("funding input {n}: its UTXO script is not a Schnorr P2PK script"));
                    }
                    let mut x = [0u8; 32];
                    x.copy_from_slice(&s[1..33]);
                    v.push((n, OwnerPub::Schnorr(x)));
                }
                InKind::Owner(k) => v.push((n, k.public())),
                InKind::OwnerPub(o) => v.push((n, o.clone())),
                _ => {}
            }
        }
        Ok(v)
    }

    /// KIP-9 storage mass of this transaction. It depends only on input and output values
    /// and output scripts, so no script runs (measurement grids use it; `finish()` commits
    /// the same value).
    pub fn storage_mass(&self) -> u64 {
        let entries: Vec<UtxoEntry> = self.inputs.iter().map(|i| i.entry.clone()).collect();
        let n = self.inputs.len();
        harness::masses(&self.assemble(&vec![vec![]; n], &vec![0; n]), &entries).storage
    }

    /// Size budgets from an unlimited run, commit storage mass, sign funding and owner
    /// inputs. An owner input (spec M5) is budgeted at one signature check (P2PK_UNITS) and,
    /// if its signed run needs more (P2SH), re-budgeted from that run and signed again.
    /// Panics on an `OwnerPub` input (no key to sign it) and where `finish_unsigned` errs.
    pub fn finish(&self) -> Final {
        self.try_finish_inner(true).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Spec frontend 6.1 (F0, signing split from building): `finish()` without the
    /// signatures. Budgets, the storage-mass commit and every covenant sigscript are exactly
    /// `finish()`'s; each input a signer outside this library must sign
    /// (`external_signers`: funding inputs and, since F1 part B, owner inputs) keeps the
    /// placeholder sigscript of its final size (66 zero bytes, plus the redeem push for a
    /// P2SH owner), so the masses equal the signed transaction's. An owner input is budgeted
    /// from `owner_signed_units` of its class, which is what `finish()`'s signed re-run
    /// commits (`tests/f1_unsigned.rs`). Returns the transaction and the indices to sign
    /// (SIG_HASH_ALL). Never panics: every failure is an error.
    pub fn finish_unsigned(&self) -> Result<(Final, Vec<usize>), String> {
        let sign = self.external_signers()?.into_iter().map(|(n, _)| n).collect();
        Ok((self.try_finish_inner(false)?, sign))
    }

    fn try_finish_inner(&self, sign: bool) -> Result<Final, String> {
        let entries: Vec<UtxoEntry> = self.inputs.iter().map(|i| i.entry.clone()).collect();
        let sigs = self.try_sigscripts()?;
        let max_budget = vec![u16::MAX; sigs.len()];
        let probe = self.try_assemble(&sigs, &max_budget)?;
        let p2pk_budget = harness::try_budget_for(P2PK_UNITS)?;
        let mut units: Vec<u64> = Vec::with_capacity(sigs.len());
        for (i, inp) in self.inputs.iter().enumerate() {
            units.push(match &inp.kind {
                InKind::Funding => P2PK_UNITS,
                // Signed here: budgeted at P2PK_UNITS, re-budgeted from the signed run below.
                InKind::Owner(_) if sign => P2PK_UNITS,
                // Signed outside: the budget the signed re-run commits for this class.
                InKind::Owner(k) => external_owner_units(&k.public(), p2pk_budget)?,
                InKind::OwnerPub(_) if sign => return Err(format!("input {i} is an owner input signed outside this library; use finish_unsigned")),
                InKind::OwnerPub(o) => external_owner_units(o, p2pk_budget)?,
                // A rejected dry run (attack tests) gets a generous budget, so the run under the
                // committed budget fails on the script's own check, not on the budget.
                _ => run_script(&probe, &entries, i, false, self.sim.as_deref()).unwrap_or(FAILED_RUN_UNITS),
            });
        }
        loop {
            let mut budgets: Vec<u16> = Vec::with_capacity(units.len());
            for (i, &u) in units.iter().enumerate() {
                let b = harness::try_budget_for(u)?;
                budgets.push(if i == 0 { self.shard_budget.unwrap_or((b as i32 + self.budget_delta).max(0) as u16) } else { b });
            }
            let tx = self.try_assemble(&sigs, &budgets)?;
            // Zero-value outputs (range tests with d = 0) are consensus-invalid and the mass
            // calculator divides by the value; the script check is what is under test there.
            if tx.outputs.iter().all(|o| o.value > 0) {
                tx.set_storage_mass(harness::try_masses(&tx, &entries)?.storage);
            }
            let mut tx = tx;
            for (i, inp) in self.inputs.iter().enumerate().filter(|_| sign) {
                match &inp.kind {
                    InKind::Funding => {
                        let sig = sign_input(&PopulatedTransaction::new(&tx, entries.clone()), i, &self.funding_sk, SIG_HASH_ALL);
                        tx.inputs[i].signature_script = sig;
                    }
                    InKind::Owner(k) => {
                        let sig = k.sign(&PopulatedTransaction::new(&tx, entries.clone()), i);
                        tx.inputs[i].signature_script = sig;
                    }
                    _ => {}
                }
            }
            tx.finalize();
            let mut rerun = false;
            for (i, inp) in self.inputs.iter().enumerate().filter(|_| sign) {
                if let InKind::Owner(_) = inp.kind {
                    if let Ok(u) = run_script(&tx, &entries, i, false, self.sim.as_deref()) {
                        if budget_for(u) > budget_for(units[i]) {
                            units[i] = u;
                            rerun = true;
                        }
                    }
                }
            }
            if !rerun {
                return Ok(Final { tx, entries, units, sim: self.sim.clone() });
            }
        }
    }
}

/// The units `finish_unsigned` gives an owner input signed outside this library: the
/// signed run's units if their budget exceeds one signature check's, else P2PK_UNITS
/// (exactly where `finish()`'s re-run loop ends for the same class, see `finish_inner`).
fn external_owner_units(o: &OwnerPub, p2pk_budget: u16) -> Result<u64, String> {
    let u = owner_signed_units(o)?;
    Ok(if harness::try_budget_for(u)? > p2pk_budget { u } else { P2PK_UNITS })
}

impl Final {
    /// Run every input under its committed budget, as consensus does.
    pub fn run(&self) -> Result<(), (usize, TxScriptError)> {
        for i in 0..self.tx.inputs.len() {
            self.run_input(i).map_err(|e| (i, e))?;
        }
        Ok(())
    }

    /// Run one input under its committed budget; returns the script units used.
    pub fn run_input(&self, i: usize) -> Result<u64, TxScriptError> {
        run_script(&self.tx, &self.entries, i, true, self.sim.as_deref())
    }

    /// Every input's result under its committed budget (attack tests check which input
    /// fails, and that it is the named one).
    pub fn run_all(&self) -> Vec<Result<u64, TxScriptError>> {
        (0..self.tx.inputs.len()).map(|i| self.run_input(i)).collect()
    }

    pub fn fee(&self) -> u64 {
        let i: u64 = self.entries.iter().map(|e| e.amount).sum();
        let o: u64 = self.tx.outputs.iter().map(|o| o.value).sum();
        i - o
    }

    /// `fee` in checked arithmetic: an error if the sums overflow or the outputs exceed the
    /// inputs (F1 part B).
    pub fn try_fee(&self) -> Result<u64, String> {
        let i = self.entries.iter().try_fold(0u64, |a, e| a.checked_add(e.amount)).ok_or("input values overflow u64")?;
        let o = self.tx.outputs.iter().try_fold(0u64, |a, x| a.checked_add(x.value)).ok_or("output values overflow u64")?;
        i.checked_sub(o).ok_or_else(|| format!("outputs {o} exceed inputs {i}"))
    }
}

/// Leaf bytes of a combination's counts, for tests that forge leaves.
pub fn leaf_counts(counts: &[u16]) -> Vec<u8> {
    counts_bytes(counts)
}

/// 3-byte script-number rank, for tests.
pub fn rank_bytes(rank: u32) -> Vec<u8> {
    num_bytes(rank as u64, 3)
}
