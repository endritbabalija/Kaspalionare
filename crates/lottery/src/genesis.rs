//! Off-chain genesis builder (spec M2 D31, M3 D50, M5 D66): one transaction creates the
//! round 1 controller, the params, K shards, the vault (round 0), the drawer (round 1) and
//! the deployer's share position under a fresh KIP-20 covenant ID.
//!
//! Shape: inputs = deployer funding (P2PK, signed with `funding_sk`); outputs 0 controller,
//! 1 params, 2 + i shard i, k + 2 vault, k + 3 drawer, k + 4 deployer position (spec M5
//! A9), then optional change. The vault, drawer and position are appended after the shards
//! so the M2 and M3 indices are kept. One `GenesisCovenantGroup` authorized by funding
//! input 0 covers outputs 0..=k+4, so the covenant ID is
//! H(outpoint of input 0, index/value/script of each of those outputs)
//! (rusty-kaspa v2.1.0 `consensus/core/src/hashing/covenant_id.rs:16-31`,
//! `consensus/core/src/tx.rs:335-385`). No covenant script runs at genesis: the deployer
//! commits to every initial state (D31, D50).
//!
//! Spec M7 (D100, D101): the drawer's seed is the anchor block's hash; its miner-withholding
//! bias is OPEN on the release gate, so the builder still refuses any network other than
//! testnet or devnet ("the drawer is deployed on testnet or devnet only until the release
//! gate"). draw_window must be at least VOID_MIN = 2F (D97; the drawer's check 1 enforces it
//! too since M7 Stage 5 fix A), except with the TEST-ONLY `allow_short_void_window` flag,
//! itself refused unless the network is testnet or devnet, which also selects the TEST-ONLY
//! short-void drawer variant (`Templates::test_only_short_void`; `probe void-m7`, D103).
//!
//! Because nothing on chain validates genesis, `build*` refuses an instance that breaks an
//! A3 range or the share-room invariant (`GenesisCase::validate`). `close()` check 2 rejects
//! such a controller anyway (M2 review finding R2), so a bad genesis could never close.
//! It also refuses a genesis whose exact masses exceed 100 % of the mainnet limits
//! (`genesis_mass_check`, user decision U-M6-1: genesis is exempt from the 90 % margin).

use std::sync::Arc;

use harness::{Hash, TransactionOutpoint, TransactionOutput, UtxoEntry};
pub use kaspa_consensus_core::network::NetworkType;
use kaspa_consensus_core::tx::GenesisCovenantGroup;

use crate::buy::{Built, FUNDING_SK, Final, In, InKind, KAS, Templates, b_max, spk_of};
use crate::close::{CloseCase, z18};
use crate::game::{
    ASSETS_MAX, BP, CLAIM_WINDOW, DAA_MAX, DRAW_WINDOW, D_RET, F_MIN, F_MIN_FLOOR, F_MIN_MAX, G2_MAX, JackpotRule, MARGIN_MIN, M_DRAW, M_HARD, SHARE_UNIT, SHARES_MAX, S_F_CAP,
    S_R_CAP, TOTAL_MAX, VOID_MIN, muldiv,
};
use crate::params::xonly;
use crate::state::{ControllerState, DrawerState, K_MAX, ParamsState, Payout, PositionState, ShardState, VaultState};

/// Spec M2 A3 range bounds that have no named constant elsewhere (as in `controller.sil`).
const PRICE_MAX: i64 = 100_000_000_000;
const DEPOSIT_MAX: i64 = 10_000_000_000;
const RESERVE_MAX: i64 = 10_000_000_000;
/// M3 re-review S2: the smallest reserve R whose storage-mass bounds were measured (M2-c
/// for close() at K = 16; M3-d for draw() at f_min = F_MIN_FLOOR needs about 1.353 KAS).
/// R is fixed at genesis.
pub const R_MIN: u64 = crate::params::TEST_RESERVE;
/// M3 re-review S1: draw_window + claim_window cap, so D_claim (check 19) stays below
/// DAA_MAX while own < DAA_MAX - WINDOWS_MAX. Both windows are fixed at genesis.
pub const WINDOWS_MAX: i64 = DAA_MAX / 4;
/// Spec M6 A2, D81 (replaces D57's D_CLAIM_FLOOR = 0.11 KAS): the smallest deposit d the
/// builder accepts, 0.13 KAS. It covers two bounds that both assume d >= the floor: the
/// b = 1 referral `buy()` within 90 % of storage mass (D80) and the P_MIN bound (worst
/// PAY-mode claim at b = 2, D82). Fixed at genesis per round's controller d.
pub const D_FLOOR: i64 = 13_000_000;
/// Spec M6 A2, D84: the smallest genesis vault V0 (1 KAS). With SHARE_UNIT = 1 genesis mints
/// S0 = V0 shares, so the VS virtual shares own < 0.001 % of the vault.
pub const V0_MIN: i64 = 100_000_000;

#[derive(Clone)]
pub struct GenesisCase {
    pub b_max: usize,
    pub controller: ControllerState,
    /// Initial boost pot O (normally 0); controller value = O + acc + reserve.
    pub pot: u64,
    pub params: ParamsState,
    pub shards: Vec<ShardState>,
    /// The house vault (D50, D66): round 0, total_assets = V0 = controller v_rec,
    /// total_shares = S0 = floor(V0 / SHARE_UNIT), pending fields 0, liability = L(1), and
    /// the record, request and position template hashes (D74). Value total_assets +
    /// pend_dep + R.
    pub vault: VaultState,
    /// The drawer (D37, D49): round 1, value R.
    pub drawer: DrawerState,
    /// The deployer's share position (D66, spec M5 A9): round 0, S0 shares, owned by a D11
    /// payout script (the funding key's Schnorr P2PK by default). Value R.
    pub position: PositionState,
    /// Target network; only testnet and devnet are accepted until the release gate (spec M7
    /// D100: the seed's miner-withholding bias is OPEN).
    pub network: NetworkType,
    /// TEST-ONLY (spec M7 D103, `probe void-m7`): accept draw_window < VOID_MIN, so a void
    /// can be shown on testnet without waiting 24 h. Refused unless the network is testnet
    /// or devnet; never set it for a deployment. It selects the TEST-ONLY short-void drawer
    /// variant (`templates()`): the production drawer rejects draw_window < VOID_MIN in
    /// check 1, so such an instance could neither draw nor void under it. Absent from a
    /// `mainnet` build, with the variant itself (SP12).
    #[cfg(not(feature = "mainnet"))]
    pub allow_short_void_window: bool,
    /// Funding inputs: outpoint (e.g. from a `Ledger`) and entry.
    pub funding: Vec<(TransactionOutpoint, UtxoEntry)>,
    pub change: bool,
    pub fee: u64,
    pub funding_sk: [u8; 32],
}

impl GenesisCase {
    /// Genesis of the `CloseCase::new(k)` fixture's round 1 (zero sales, drawn = 1 with
    /// J(1), J_boost(1), G2(1), L(1) and V_rec from deployment inputs), boost pot 0 unless
    /// set, funded by `funding`. J(1) = `GENESIS_JACKPOT` (500,000 KAS, J_boost 0, G2 5,000
    /// KAS, L(1) 545,000 KAS): M8 S2 review SF1, the round-1 whale check refuses the
    /// `CloseCase` value 1,000,000 KAS (1,090,000 KAS > q N m = 750,531.59 KAS at 1.5).
    pub fn new(k: usize, funding: Vec<(TransactionOutpoint, UtxoEntry)>) -> Self {
        Self::new_with(k, funding, &Templates::get(b_max()))
    }

    pub fn new_with(k: usize, funding: Vec<(TransactionOutpoint, UtxoEntry)>, t: &Templates) -> Self {
        let c = CloseCase::new_with(k, t);
        let controller = ControllerState { j_boost: 0, ..c.controller };
        let s0 = controller.v_rec / SHARE_UNIT;
        let vault = VaultState {
            round: controller.round - 1,
            total_assets: controller.v_rec,
            total_shares: s0,
            pend_dep: 0,
            pend_wsh: 0,
            liability: controller.liability,
            record_tpl: t.record.hash,
            request_tpl: t.request.hash,
            position_tpl: t.position.hash,
        };
        let position = PositionState::new(0, s0, &Payout::Schnorr(xonly(&FUNDING_SK)));
        let drawer = DrawerState {
            round: controller.round,
            pool_tpl: t.pool.hash,
            vault_tpl: t.vault.hash,
            ticket_tpl: t.ticket.hash,
            f_min: F_MIN,
            claim_window: CLAIM_WINDOW,
            draw_window: DRAW_WINDOW,
        };
        let mut g = Self {
            b_max: t.b_max,
            controller,
            pot: 0,
            params: c.params,
            shards: c.shards.into_iter().map(|s| s.state).collect(),
            vault,
            drawer,
            position,
            network: NetworkType::Testnet,
            #[cfg(not(feature = "mainnet"))]
            allow_short_void_window: false,
            funding,
            change: true,
            fee: KAS / 2,
            funding_sk: FUNDING_SK,
        };
        g.set_jackpot(GENESIS_JACKPOT);
        g
    }

    pub fn k(&self) -> usize {
        self.shards.len()
    }

    // Output indices (D50: vault and drawer after the shards, M2 indices kept).
    pub const CONTROLLER: usize = 0;
    pub const PARAMS: usize = 1;
    pub fn shard_output(i: usize) -> usize {
        2 + i
    }
    pub fn vault_output(&self) -> usize {
        self.k() + 2
    }
    pub fn drawer_output(&self) -> usize {
        self.k() + 3
    }
    /// The deployer position (spec M5 A9: after the drawer).
    pub fn position_output(&self) -> usize {
        self.k() + 4
    }

    /// Set the initial vault V0 (total_assets and the controller's v_rec, D50), and the
    /// genesis share count S0 = floor(V0 / SHARE_UNIT) in the vault and the deployer
    /// position (D66).
    pub fn set_vault(&mut self, v0: i64) {
        self.vault.total_assets = v0;
        self.controller.v_rec = v0;
        self.vault.total_shares = v0 / SHARE_UNIT;
        self.position.shares = v0 / SHARE_UNIT;
    }

    /// Set the owner of the deployer position (D66, D67).
    pub fn set_owner(&mut self, owner: &Payout) {
        self.position.owner_class = owner.class();
        self.position.owner_key = owner.key33();
    }

    /// Set J(1) with J_boost(1) = 0, G2(1) = muldiv(J, g) and L(1) = J + G2 (m - 1), in the
    /// controller and the vault's liability.
    pub fn set_jackpot(&mut self, jackpot: i64) {
        let c = &mut self.controller;
        c.jackpot = jackpot;
        c.j_boost = 0;
        c.g2 = muldiv(jackpot, c.g);
        c.liability = c.g2.checked_mul(c.m - 1).and_then(|x| x.checked_add(jackpot)).expect("set_jackpot: L(1) overflows i64");
        self.vault.liability = c.liability;
    }

    /// Vault value total_assets + pend_dep + R. Panics on a state outside its ranges; use
    /// `checked_vault_value` on untrusted states (S2d: `bundle::verify`).
    pub fn vault_value(&self) -> u64 {
        self.checked_vault_value().expect("vault value: state outside its ranges")
    }

    pub fn reserve(&self) -> u64 {
        self.controller.reserve as u64
    }

    pub fn controller_value(&self) -> u64 {
        self.checked_controller_value().expect("controller value: state outside its ranges")
    }

    /// Shard i's value S - P_R + R (S = P_R = 0 at genesis).
    pub fn shard_value(&self, i: usize) -> u64 {
        self.checked_shard_value(i).expect("shard value: state outside its ranges")
    }

    /// The reserve R as u64, `None` if negative.
    fn checked_reserve(&self) -> Option<u64> {
        u64::try_from(self.controller.reserve).ok()
    }

    /// `vault_value` with checked arithmetic: `None` on a negative term or an overflow.
    pub fn checked_vault_value(&self) -> Option<u64> {
        let v = self.vault.total_assets.checked_add(self.vault.pend_dep)?;
        u64::try_from(v).ok()?.checked_add(self.checked_reserve()?)
    }

    /// `controller_value` with checked arithmetic.
    pub fn checked_controller_value(&self) -> Option<u64> {
        self.pot.checked_add(u64::try_from(self.controller.acc).ok()?)?.checked_add(self.checked_reserve()?)
    }

    /// `shard_value` with checked arithmetic (`None` also for a missing shard).
    pub fn checked_shard_value(&self, i: usize) -> Option<u64> {
        let s = self.shards.get(i)?;
        let v = s.sales.checked_sub(s.ref_paid)?;
        u64::try_from(v).ok()?.checked_add(self.checked_reserve()?)
    }

    /// Every A3 range of the initial states (the bounds `close()` check 2 applies, spec M2
    /// A3), the share-room invariant sum(st) + s_C + s_D + S_F_CAP + S_R_CAP <= 10^4 (review
    /// finding R2: without it an admin change within the caps can make s_H' < 0 and block
    /// every later `close()`), and the consistency of the params and shards with the
    /// controller. The drawer's route (pool_tpl, vault_tpl and ticket_tpl, D38, D54) is
    /// checked against the committed templates; use `validate_with` for other templates.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_with(&self.templates())
    }

    /// TEST-ONLY (spec M7 D103, `probe void-m7`): set `allow_short_void_window` and the draw
    /// window, and point every template hash the genesis states carry (the controller's
    /// route shard_tpl, marker_tpl and params_tpl, the drawer's pool_tpl, vault_tpl and
    /// ticket_tpl, the vault's record_tpl, request_tpl and position_tpl) at the short-void
    /// template set `templates()` now returns. The variant changes the drawer hash, so the
    /// marker, vault and pool (drawer) and from them position (vault), request (position),
    /// ticket (pool) and shard (ticket) differ from the production set; controller, params
    /// and record do not. `validate` checks all nine against that set (S2c MF-A) and still
    /// refuses the flag off testnet/devnet. Not in a `mainnet` build (SP12).
    #[cfg(not(feature = "mainnet"))]
    pub fn set_test_only_short_void_window(&mut self, draw_window: i64) {
        self.allow_short_void_window = true;
        self.drawer.draw_window = draw_window;
        let t = self.templates();
        self.controller.shard_tpl = t.shard.hash;
        self.controller.marker_tpl = t.marker.hash;
        self.controller.params_tpl = t.params.hash;
        self.drawer.pool_tpl = t.pool.hash;
        self.drawer.vault_tpl = t.vault.hash;
        self.drawer.ticket_tpl = t.ticket.hash;
        self.vault.record_tpl = t.record.hash;
        self.vault.request_tpl = t.request.hash;
        self.vault.position_tpl = t.position.hash;
    }

    /// The templates this instance deploys under: the committed ones, or with
    /// `allow_short_void_window` the TEST-ONLY short-void drawer variant (spec M7 D103; the
    /// only route to it, and `validate` refuses the flag off testnet/devnet).
    pub fn templates(&self) -> Arc<Templates> {
        #[cfg(not(feature = "mainnet"))]
        if self.allow_short_void_window {
            return Templates::test_only_short_void(self.b_max);
        }
        Templates::get(self.b_max)
    }

    /// `validate()` with the drawer's route checked against the templates `t`.
    pub fn validate_with(&self, t: &Templates) -> Result<(), String> {
        let c = &self.controller;
        let k = self.k();
        let range = |name: &str, v: i64, lo: i64, hi: i64| -> Result<(), String> {
            if lo <= v && v <= hi { Ok(()) } else { Err(format!("{name} = {v} outside [{lo}, {hi}]")) }
        };
        range("round", c.round, 1, TOTAL_MAX - 1)?;
        range("k", c.k, 1, K_MAX as i64)?;
        if c.k != k as i64 {
            return Err(format!("controller k = {} but {k} shards", c.k));
        }
        range("m_min", c.m_min, 2, M_HARD)?;
        range("m_max", c.m_max, c.m_min, M_HARD)?;
        range("m", c.m, c.m_min, c.m_max)?;
        range("price", c.price, 1, PRICE_MAX)?;
        // D81 (spec M6 A2): d >= D_FLOOR, so the buy() and P_MIN storage-mass bounds hold.
        if c.d < D_FLOOR {
            return Err(format!("d = {} < D_FLOOR = {D_FLOOR} (D81)", c.d));
        }
        range("d", c.d, 1, DEPOSIT_MAX)?;
        range("s_f", c.s_f, 0, S_F_CAP)?;
        range("s_r", c.s_r, 0, S_R_CAP)?;
        range("s_c", c.s_c, 0, BP)?;
        range("s_d", c.s_d, 0, BP)?;
        for (i, &st) in c.st.iter().enumerate() {
            range(&format!("st{}", i + 1), st, 0, BP)?;
        }
        // Arithmetic in validate() is i128 (M3 review 3): each term is range-checked above,
        // but the sums and products below must not depend on that order to be exact.
        let room: i128 = c.st.iter().map(|&s| s as i128).sum::<i128>() + c.s_c as i128 + c.s_d as i128 + (S_F_CAP + S_R_CAP) as i128;
        if room > BP as i128 {
            return Err(format!("share room: sum(st) + s_C + s_D + S_F_CAP + S_R_CAP = {room} > {BP}"));
        }
        range("e", c.e, 0, 5_000)?;
        range("j", c.j, 0, 5_000)?;
        range("g", c.g, 0, 1_000)?;
        range("margin", c.margin, MARGIN_MIN, 2 * BP)?;
        range("cap", c.cap, 0, BP)?;
        for (name, v) in [("jackpot", c.jackpot), ("j_boost", c.j_boost), ("g2", c.g2), ("liability", c.liability), ("v_rec", c.v_rec), ("acc", c.acc)] {
            range(name, v, 0, TOTAL_MAX)?;
        }
        range("reserve", c.reserve, R_MIN as i64, RESERVE_MAX)?;
        for (name, v) in [("deadline", c.deadline), ("round_len", c.round_len), ("d_min", c.d_min)] {
            range(name, v, 0, DAA_MAX - 1)?;
        }
        // M7 S2-5: the anchor must lie strictly after the chain block that accepted close()
        // (spec M7 D93, D97); controller close() requires the same.
        range("seed_offset", c.seed_offset, 1, DAA_MAX - 1)?;
        if c.fee_class > 2 || (c.fee_class != 1 && c.fee_key[32] != 0) {
            return Err("fee address is not a D11 standard script".into());
        }
        // M3 review: round 1 is born with its jackpot fixed (D31); close() requires drawn = 1.
        if c.drawn != 1 {
            return Err(format!("controller drawn = {}, must be 1 at genesis", c.drawn));
        }
        // M3 review: the drawer's check 4 requires J_boost <= J of the frozen marker, so a
        // round 1 with J_boost > J could never be drawn.
        if c.j_boost > c.jackpot {
            return Err(format!("j_boost {} exceeds the jackpot {}", c.j_boost, c.jackpot));
        }
        if c.j_boost as u64 > self.pot {
            return Err(format!("j_boost {} exceeds the boost pot {}", c.j_boost, self.pot));
        }
        let p = &self.params;
        if p.round != c.round || p.used != 0 || p.nonce < 0 {
            return Err("params: round must equal the controller's, used = 0, nonce >= 0".into());
        }
        range("next_s_f", p.next_s_f, 0, S_F_CAP)?;
        range("next_s_r", p.next_s_r, 0, S_R_CAP)?;
        if p.next_fee_class > 2 || (p.next_fee_class != 1 && p.next_fee_key[32] != 0) {
            return Err("params: next fee address is not a D11 standard script".into());
        }
        // M3 (D49, D50): the drawer can draw every m the controller may choose; the vault
        // backs L(1) under the cap; the drawer is the round 1 drawer, in range.
        if c.m_max > M_DRAW {
            return Err(format!("m_max = {} > M_DRAW = {M_DRAW}: such a round could never be drawn", c.m_max));
        }
        // Decision B (M3 Stage 5): the drawer's A6 guard rejects a second-prize pool with
        // G2 > TOTAL_MAX / 254, so a round 1 with such a G2 and n2 >= 1 could never be drawn.
        if c.g2 > G2_MAX {
            return Err(format!("g2 = {} > G2_MAX = TOTAL_MAX / 254 = {G2_MAX}", c.g2));
        }
        // Exact (i128): G2 (m - 1) can exceed 2^63 for G2 <= TOTAL_MAX (M3 review 3).
        if c.liability as i128 != c.jackpot as i128 - c.j_boost as i128 + c.g2 as i128 * (c.m as i128 - 1) {
            return Err("controller liability is not J_house + G2 (m - 1)".into());
        }
        // M8 S2c (review should-fix 1): G2(1) is tied to g as draw() ties G2' to g for every
        // later round (G2' = muldiv(J', g), `game::JackpotFix::from_witness`), so round 1's
        // second prize follows the same rule. jackpot is in [0, TOTAL_MAX] and g in [0, 1000]
        // (ranges above), so `muldiv` cannot panic.
        if c.g2 != muldiv(c.jackpot, c.g) {
            return Err(format!("g2 = {} is not muldiv(J, g) = {} (G2 is tied to g, review S2c)", c.g2, muldiv(c.jackpot, c.g)));
        }
        // M8 S2, review SF1 (D113): round 1 is born with its jackpot fixed, and neither
        // close() nor draw() checks it (draw() fixes round r + 1), so genesis applies the
        // game-size rule of design 4.6 to it with round 1's own shares, e and margin, in the
        // limits `draw()` uses (`JackpotRule`: E = a N m, W = q N m, a = `allowance`,
        // q = `whale_allowance`): the edge rule J_house + G2 (m - 1) <= a N m and the whale
        // check J + G2 (m - 1) <= q N m (checked first: above margin 1.256 it is the one that
        // binds, D113). Exact in i128. The ranges above (m <= M_DRAW, price <= PRICE_MAX, the
        // share room so s_H >= 0, 0 <= v_rec) keep `JackpotRule::new` in i64; only its e_lim
        // and w_lim are used (the cap check is below).
        let rule = JackpotRule::new(c, c.v_rec);
        let g2m = c.g2 as i128 * (c.m as i128 - 1);
        let j_house = c.jackpot as i128 - c.j_boost as i128;
        if c.jackpot as i128 + g2m > rule.w_lim as i128 {
            return Err(format!("round 1 whale check: J + G2 (m - 1) = {} > q N m = {} at margin {} (design 4.6, review SF1)", c.jackpot as i128 + g2m, rule.w_lim, c.margin));
        }
        if j_house + g2m > rule.e_lim as i128 {
            return Err(format!("round 1 edge rule: J_house + G2 (m - 1) = {} > a N m = {} (design 4.6, review SF1)", j_house + g2m, rule.e_lim));
        }
        let v = &self.vault;
        if v.round != c.round - 1 {
            return Err(format!("vault round {} != controller round - 1", v.round));
        }
        // D84 (spec M6 A2): V0 >= V0_MIN = 1 KAS, so the VS virtual shares own < 0.001 % of
        // the vault at SHARE_UNIT = 1.
        if v.total_assets < V0_MIN {
            return Err(format!("vault total_assets V0 = {} < V0_MIN = {V0_MIN} (D84)", v.total_assets));
        }
        // D66 (spec M5 A9): V0 <= ASSETS_MAX and S0 = floor(V0 / SHARE_UNIT) >= 1.
        range("vault total_assets", v.total_assets, SHARE_UNIT, ASSETS_MAX)?;
        let s0 = v.total_assets / SHARE_UNIT;
        // D84: S0 <= SHARES_MAX (the vault's W1 range, and room for the S + VS <= 2^62 bound).
        if s0 > SHARES_MAX {
            return Err(format!("vault S0 = floor(V0 / SHARE_UNIT) = {s0} > SHARES_MAX = {SHARES_MAX} (D84)"));
        }
        if v.total_shares != s0 {
            return Err(format!("vault total_shares {} != S0 = floor(V0 / SHARE_UNIT) = {s0} (D66)", v.total_shares));
        }
        if v.pend_dep != 0 || v.pend_wsh != 0 {
            return Err("vault: pending fields must be 0 at genesis".into());
        }
        // D74: the vault's instance constants are the compiled templates (draw() reads
        // record_tpl and request_tpl; the request entries bind to request_tpl and
        // position_tpl), so a wrong value would make every request or record unusable.
        if v.record_tpl != t.record.hash || v.request_tpl != t.request.hash || v.position_tpl != t.position.hash {
            return Err("vault record_tpl / request_tpl / position_tpl do not match the templates (D74)".into());
        }
        // D66: the deployer position holds every genesis share.
        let ps = &self.position;
        if ps.round != 0 || ps.shares != v.total_shares {
            return Err(format!("deployer position {{round {}, shares {}}} != {{0, S0 = {}}} (D66)", ps.round, ps.shares, v.total_shares));
        }
        if ps.owner_class > 2 || (ps.owner_class != 1 && ps.owner_key[32] != 0) {
            return Err("deployer position owner is not a D11 standard script".into());
        }
        if v.liability != c.liability {
            return Err("vault liability != controller liability L(1)".into());
        }
        if c.v_rec != v.total_assets {
            return Err(format!("controller v_rec {} != vault V0 {}", c.v_rec, v.total_assets));
        }
        if c.liability > muldiv(v.total_assets, c.cap) {
            return Err(format!("L(1) = {} > floor(V0 x cap / 10^4) = {}", c.liability, muldiv(v.total_assets, c.cap)));
        }
        let d = &self.drawer;
        if d.round != c.round {
            return Err(format!("drawer round {} != controller round", d.round));
        }
        range("f_min", d.f_min, F_MIN_FLOOR, F_MIN_MAX)?;
        range("claim_window", d.claim_window, 0, DAA_MAX - 1)?;
        range("draw_window", d.draw_window, 0, DAA_MAX - 1)?;
        // Spec M7 D97: at tx.daa >= D + draw_window the anchor must be outside the opcode
        // window from every point of view (A5), so a void and a draw are never both valid.
        // The drawer's check 1 enforces the same floor (Stage 5 fix A); the TEST-ONLY
        // short-void variant drops it and goes only with the TEST-ONLY flag (D103).
        // A `mainnet` build has neither the flag nor the variant (SP12): the floor always applies.
        #[cfg(feature = "mainnet")]
        let short = false;
        #[cfg(not(feature = "mainnet"))]
        let short = {
            if t.short_void_window && !self.allow_short_void_window {
                return Err("the TEST-ONLY short-void drawer variant requires allow_short_void_window (D103)".into());
            }
            if self.allow_short_void_window {
                if !matches!(self.network, NetworkType::Testnet | NetworkType::Devnet) {
                    return Err(format!("network {:?}: allow_short_void_window is TEST-ONLY, allowed only on testnet or devnet (D103)", self.network));
                }
                if !t.short_void_window {
                    return Err("allow_short_void_window needs the TEST-ONLY short-void drawer variant (GenesisCase::templates()): the production drawer requires draw_window >= VOID_MIN (check 1)".into());
                }
            }
            self.allow_short_void_window
        };
        if !short && d.draw_window < VOID_MIN {
            return Err(format!("draw_window = {} < VOID_MIN = {VOID_MIN} (2F, D97): a void could overlap a valid draw", d.draw_window));
        }
        if d.draw_window + d.claim_window > WINDOWS_MAX {
            return Err(format!("draw_window + claim_window > WINDOWS_MAX = {WINDOWS_MAX}: D_claim could reach DAA_MAX"));
        }
        // M4 review S1, user decision U3: every pool of an on-time draw keeps at least 7
        // days to claim (design 4.5), and the claim deadline of a round closed at T and
        // drawn, D + draw_window + claim_window (spec M7 D98, D = the marker's creation DAA
        // score + seed_offset; D102 bounds the case of a close accepted late), is at most
        // T + D_RET, the earliest return_deposit_after (ticket.sil A5b.3), so path b
        // cannot forfeit a winning ticket while its pool is claimable. Exact in i128.
        if d.claim_window < CLAIM_WINDOW {
            return Err(format!("claim_window = {} < CLAIM_WINDOW = {CLAIM_WINDOW} (7 days, design 4.5)", d.claim_window));
        }
        let span = c.seed_offset as i128 + d.draw_window as i128 + d.claim_window as i128;
        if span > D_RET as i128 {
            return Err(format!("seed_offset + draw_window + claim_window = {span} > D_RET = {D_RET}: return_deposit_after could forfeit a winning ticket before its claim deadline"));
        }
        // Route of trust (D38, D54): the drawer's instance constants are the compiled
        // templates. draw() copies ticket_tpl into every pool, and the pool's ticket leg
        // (M4) trusts it, so a wrong ticket_tpl would make every prize unclaimable.
        if d.pool_tpl != t.pool.hash || d.vault_tpl != t.vault.hash {
            return Err("drawer pool_tpl / vault_tpl do not match the templates".into());
        }
        if d.ticket_tpl != t.ticket.hash {
            return Err("drawer ticket_tpl does not match the compiled ticket template (D54)".into());
        }
        // M8 S2c (review MF-A): the controller's route table is state (controller.sil:9-11)
        // and nothing on chain pins it: close() reads the params and shards under params_tpl
        // and shard_tpl and creates the marker, which holds the round's money (marker.sil:10),
        // under marker_tpl (controller.sil:~593). So each must be the compiled template's
        // hash. With the vault's three (D74) and the drawer's three (D38, D54) above, every
        // template hash a genesis state carries is checked (params, shard and position
        // states carry none; pool, ticket, request, record and marker states are not genesis
        // outputs, and draw() / buy() / request entries copy theirs from the checked ones).
        if c.shard_tpl != t.shard.hash || c.marker_tpl != t.marker.hash || c.params_tpl != t.params.hash {
            return Err("controller shard_tpl / marker_tpl / params_tpl do not match the compiled templates (route table, review MF-A)".into());
        }
        // Spec M7 D100: the seed's miner-withholding bias is OPEN on the release gate.
        if !matches!(self.network, NetworkType::Testnet | NetworkType::Devnet) {
            return Err(format!(
                "network {:?}: the drawer is deployed on testnet or devnet only until the release gate (D100: the seed's miner-withholding bias is OPEN)",
                self.network
            ));
        }
        for (i, s) in self.shards.iter().enumerate() {
            let same = s.round == c.round
                && s.shard_index == i as i64
                && s.m == c.m
                && s.price == c.price
                && s.d == c.d
                && s.s_r == c.s_r
                && s.deadline == c.deadline
                && s.root == z18()
                && s.sales == 0
                && s.ref_paid == 0;
            if !same {
                return Err(format!("shard {i} does not match the controller or is not empty"));
            }
        }
        Ok(())
    }

    pub fn build(&self) -> Built {
        self.build_with(self.templates())
    }

    /// Panics if the instance fails `validate_with(t)` (which includes the drawer's route) or
    /// the U-M6-1 mass check (`try_build_with`).
    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        self.try_build_with(t).unwrap_or_else(|e| panic!("genesis refused: {e}"))
    }

    /// `validate_with(t)`, then the transaction, then the U-M6-1 mass check: the finished
    /// (signed, storage-mass committed) transaction's exact compute, transient and storage
    /// masses must each be within 100 % of the mainnet limits (`genesis_mass_check`).
    pub fn try_build_with(&self, t: Arc<Templates>) -> Result<Built, String> {
        self.validate_with(&t)?;
        let built = self.build_unchecked_for_tests(t);
        let f = built.finish();
        genesis_mass_check(&harness::masses(&f.tx, &f.entries))?;
        Ok(built)
    }

    /// TEST-ONLY: builds a genesis without `validate()`, so attack tests can deploy an
    /// instance the builder refuses and show that the covenants reject it on their own.
    /// Never use it to deploy.
    #[doc(hidden)]
    pub fn build_unchecked_for_tests(&self, t: Arc<Templates>) -> Built {
        let inputs: Vec<In> = self.funding.iter().map(|(_, e)| In { entry: e.clone(), kind: InKind::Funding }).collect();
        let mut outputs = vec![
            TransactionOutput::new(self.controller_value(), t.controller_spk(&self.controller)),
            TransactionOutput::new(self.reserve(), t.params_spk(&self.params)),
        ];
        for (i, s) in self.shards.iter().enumerate() {
            outputs.push(TransactionOutput::new(self.shard_value(i), t.shard_spk(s)));
        }
        outputs.push(TransactionOutput::new(self.vault_value(), t.vault_spk(&self.vault)));
        outputs.push(TransactionOutput::new(self.reserve(), t.drawer_spk(&self.drawer)));
        outputs.push(TransactionOutput::new(self.reserve(), t.position_spk(&self.position)));
        let total_in: u64 = inputs.iter().map(|i| i.entry.amount).sum();
        let total_out: u64 = outputs.iter().map(|o| o.value).sum();
        let change = total_in.checked_sub(total_out + self.fee).expect("funding covers genesis");
        if self.change {
            outputs.push(TransactionOutput::new(change, spk_of(&self.funding_sk)));
        } else {
            assert_eq!(change, 0, "no-change genesis needs exact funding");
        }
        let group = GenesisCovenantGroup::new(0, (0..(self.k() + 5) as u32).collect());
        Built {
            templates: t,
            args: vec![],
            inputs,
            outputs,
            genesis: vec![group],
            lock_time: 0,
            shard_budget: None,
            budget_delta: 0,
            outpoints: self.funding.iter().map(|(o, _)| *o).collect(),
            funding_sk: self.funding_sk,
            sim: None,
        }
    }
}


/// User decision U-M6-1 (spec M6, the Stage 3a D90 stop): genesis is exempt from the 90 %
/// margin of D90, because its masses depend only on deployer-chosen output values (K + 4
/// covenant outputs at about R, the vault at V0 + R, the change) and a genesis over a limit
/// only means "cannot deploy" (no funds locked, no round frozen). The builder still refuses a
/// genesis that could never be mined: each mass must be within 100 % of the mainnet block
/// mass limits, compute 500,000, storage 500,000, transient 1,000,000
/// (rusty-kaspa v2.1.0 `consensus/core/src/config/params.rs:626`). The mempool rejects a
/// transaction with compute or transient above the limit
/// (`mining/src/mempool/check_transaction_limits.rs:19-26`) or storage above it (`:37-41`),
/// and a block whose summed masses exceed them is invalid
/// (`consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:88-96`); `>` in
/// both, so exactly 100 % is allowed. `m` comes from the same v2.1.0 mass calculator with
/// mainnet parameters as every measurement (`harness::masses`).
pub fn genesis_mass_check(m: &harness::Masses) -> Result<(), String> {
    let l = &harness::MAINNET_PARAMS.block_mass_limits;
    for (name, v, lim) in [("compute", m.compute, l.compute), ("transient", m.transient, l.transient), ("storage", m.storage, l.storage)] {
        if v > lim {
            return Err(format!("genesis {name} mass {v} > the mainnet limit {lim} (U-M6-1: genesis is exempt from the 90 % margin, not from the limit)"));
        }
    }
    Ok(())
}

/// The fixture J(1) of `GenesisCase::new` (M8 S2, review SF1): 500,000 KAS with J_boost 0,
/// so J + G2 (m - 1) = 545,000 KAS at m 10, within the whale limit q N m at every margin
/// in [MARGIN_MIN, 2 BP] (750,531.59 KAS at 1.5, 562,898.7 KAS at 2.0) and the edge limit
/// a N m = 896,362.74 KAS. Was 1,000,000 KAS (the `CloseCase` value) before SF1.
pub const GENESIS_JACKPOT: i64 = 500_000 * KAS as i64;

/// Funding the test fixtures use for a genesis: covers the fixture vault V0 = 10,000,000 KAS
/// plus the reserves and the fee.
pub const GENESIS_FUNDING: u64 = 10_100_000 * KAS;

/// The covenant ID a finished genesis transaction assigned to its protocol outputs.
pub fn covenant_id(f: &Final) -> Hash {
    f.tx.outputs[0].covenant.expect("genesis output bound").covenant_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buy::funding_spk;
    use crate::game::SHARES_MAX;
    use crate::ledger::Ledger;

    fn case() -> GenesisCase {
        let mut l = Ledger::new();
        let op = l.fund(GENESIS_FUNDING, funding_spk(), 100);
        let e = l.get(&op).unwrap().clone();
        GenesisCase::new(4, vec![(op, e)])
    }

    #[test]
    fn genesis_refuses_bad_instances() {
        let good = case();
        assert_eq!(good.validate(), Ok(()));
        good.build();
        // Review finding R2: st = (4000, 3000, 600, 0, 0) leaves s_H = 800 >= 0 at the
        // initial s_F/s_R, but sum(st) + s_C + s_D + S_F_CAP + S_R_CAP = 10,200 > 10^4.
        let mut bad = good.clone();
        bad.controller.st = [4_000, 3_000, 600, 0, 0];
        let err = bad.validate().unwrap_err();
        assert!(err.contains("share room") && err.contains("10200"), "{err}");
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| bad.build()));
        assert!(r.is_err(), "builder accepted a genesis that breaks the share-room invariant");
        // The largest room that passes: sum(st) = 10^4 - 100 - 2,500 = 7,400.
        // M8 S2 (review SF1): at s_H = 1,000 bp the round-1 edge limit a N m is 121,130.1
        // KAS, below the fixture L(1) of 545,000 KAS, so this share-room boundary runs with
        // J(1) = 0 (it tests the share room, not the jackpot).
        let mut edge = good.clone();
        edge.controller.st = [4_000, 3_000, 400, 0, 0];
        edge.set_jackpot(0);
        assert_eq!(edge.validate(), Ok(()));
        // A3 ranges, one at a time.
        let cases: Vec<(&str, fn(&mut GenesisCase))> = vec![
            ("round", |g| g.controller.round = 0),
            ("k", |g| g.controller.k = 3),
            ("m_min", |g| g.controller.m_min = 1),
            ("m", |g| g.controller.m = 51),
            ("m_max", |g| g.controller.m_max = 256),
            ("price", |g| g.controller.price = 0),
            ("d", |g| g.controller.d = DEPOSIT_MAX + 1),
            ("s_f", |g| g.controller.s_f = S_F_CAP + 1),
            ("s_r", |g| g.controller.s_r = -1),
            ("s_c", |g| g.controller.s_c = -1),
            ("st1", |g| g.controller.st[0] = -1),
            ("e", |g| g.controller.e = 5_001),
            ("g", |g| g.controller.g = 1_001),
            ("margin", |g| g.controller.margin = MARGIN_MIN - 1),
            ("cap", |g| g.controller.cap = BP + 1),
            ("v_rec", |g| g.controller.v_rec = -1),
            ("reserve", |g| g.controller.reserve = 0),
            ("deadline", |g| g.controller.deadline = DAA_MAX),
            ("fee", |g| g.controller.fee_class = 3),
            ("j_boost", |g| g.controller.j_boost = 1),
            ("params", |g| g.params.round = 2),
            ("shard", |g| g.shards[1].m = 11),
            // M3 (D43, D49, D50).
            ("m_max > M_DRAW", |g| g.controller.m_max = M_DRAW + 1),
            ("v_rec != V0", |g| g.controller.v_rec -= 1),
            ("vault round", |g| g.vault.round = 1),
            // M5 (D66): total_shares must be S0 = floor(V0 / SHARE_UNIT), no longer 0.
            ("vault shares 0", |g| g.vault.total_shares = 0),
            ("vault shares S0 + 1", |g| g.vault.total_shares += 1),
            ("vault pending", |g| g.vault.pend_dep = 1),
            ("vault liability", |g| g.vault.liability += 1),
            ("L(1) formula", |g| g.controller.liability += 1),
            ("cap", |g| {
                let v0 = g.controller.liability * BP / g.controller.cap - BP;
                g.set_vault(v0)
            }),
            ("drawer round", |g| g.drawer.round = 2),
            ("f_min", |g| g.drawer.f_min = F_MIN_MAX + 1),
            ("claim_window", |g| g.drawer.claim_window = DAA_MAX),
            ("draw_window", |g| g.drawer.draw_window = -1),
            // Spec M7 D97: draw_window >= VOID_MIN = 2F.
            ("draw_window = VOID_MIN - 1", |g| g.drawer.draw_window = VOID_MIN - 1),
            ("mainnet", |g| g.network = NetworkType::Mainnet),
            ("simnet", |g| g.network = NetworkType::Simnet),
        ];
        for (name, f) in cases {
            let mut g = good.clone();
            f(&mut g);
            assert!(g.validate().is_err(), "{name}: bad instance accepted");
        }
        // The cap boundary: L(1) = floor(V0 x cap / 10^4) exactly is accepted; devnet too.
        let mut edge = good.clone();
        edge.set_vault(good.controller.liability * BP / good.controller.cap);
        assert_eq!(muldiv(edge.vault.total_assets, edge.controller.cap), edge.controller.liability);
        assert_eq!(edge.validate(), Ok(()));
        let mut dev = good.clone();
        dev.network = NetworkType::Devnet;
        assert_eq!(dev.validate(), Ok(()));
        // A mainnet genesis is refused by the builder (D100).
        let mut main = good.clone();
        main.network = NetworkType::Mainnet;
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| main.build()));
        assert!(r.is_err(), "builder accepted a mainnet genesis");
    }

    /// M3 Stage 5 (review): the genesis checks added after the covenant review. Each bad
    /// instance must be refused by `validate()` with an `Err` (never a panic or a silent
    /// overflow), and each boundary instance accepted.
    #[test]
    fn genesis_refuses_stage5_review_instances() {
        use crate::game::{F_MIN_FLOOR, G2_MAX};
        let good = case();
        assert_eq!(good.validate(), Ok(()));
        let refuse = |name: &str, g: &GenesisCase, needle: &str| {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.validate()));
            match r {
                Ok(Err(e)) => assert!(e.contains(needle), "{name}: refused for another reason: {e}"),
                Ok(Ok(())) => panic!("{name}: bad instance accepted"),
                Err(_) => panic!("{name}: validate() panicked instead of returning Err"),
            }
        };
        // Decision A: f_min >= F_MIN_FLOOR (0.1 KAS); F_MIN_FLOOR itself is accepted.
        let mut g = good.clone();
        g.drawer.f_min = F_MIN_FLOOR - 1;
        refuse("f_min = F_MIN_FLOOR - 1", &g, "f_min");
        let mut g = good.clone();
        g.drawer.f_min = 0;
        refuse("f_min = 0", &g, "f_min");
        let mut g = good.clone();
        g.drawer.f_min = F_MIN_FLOOR;
        assert_eq!(g.validate(), Ok(()), "f_min = F_MIN_FLOOR");
        // Review 2: J_boost <= J (the drawer's check 4 rejects J_boost > J, so round 1 could
        // never be drawn). J = 1 KAS, J_boost = 2 KAS with a boost pot that covers J_boost
        // and L(1) consistent: only the new check can refuse it.
        let mut g = good.clone();
        g.pot = 2 * KAS;
        g.controller.jackpot = KAS as i64;
        g.controller.j_boost = 2 * KAS as i64;
        g.controller.liability = g.controller.jackpot - g.controller.j_boost + g.controller.g2 * (g.controller.m - 1);
        g.vault.liability = g.controller.liability;
        refuse("j_boost > jackpot", &g, "j_boost");
        // Review 4: the round 1 controller must be drawn (close() requires drawn = 1).
        let mut g = good.clone();
        g.controller.drawn = 0;
        refuse("drawn = 0", &g, "drawn");
        // Decision B: G2(1) <= G2_MAX = TOTAL_MAX / 254 (the drawer's A6 guard rejects any
        // second-prize pool above it). L(1) consistent and backed by the vault, so only the
        // G2 check can refuse G2_MAX + 1 (G2_MAX itself was accepted before M8 S2c, below).
        // M8 S2 (review SF1): the round-1 whale check needs J + G2 (m - 1) <= q N m, which
        // the fixture (m 10, price 1 KAS: q N m = 750,531.59 KAS) cannot meet at G2 = G2_MAX
        // (about 1.8 x 10^16 sompi). This boundary therefore runs at m = 2, price = PRICE_MAX
        // (10^11 sompi), s_R = 0 and no tier shares (w = 9,900, q = 6.6 x 10^10, q N 2 =
        // 1.881 x 10^16 >= G2_MAX + J), so only the G2 check can refuse G2_MAX + 1.
        let mut wide = good.clone();
        wide.controller.m = 2;
        wide.controller.price = PRICE_MAX;
        wide.controller.s_r = 0;
        wide.controller.st = [0; 5];
        wide.params.next_s_r = 0;
        for s in &mut wide.shards {
            s.m = 2;
            s.price = PRICE_MAX;
            s.s_r = 0;
        }
        // M8 S2c (review should-fix 1): G2(1) = muldiv(J(1), g) is now required too, and
        // with g <= 1,000 bp that means J(1) >= 10 G2(1): at G2 = G2_MAX, J + G2 (m - 1) >=
        // G2_MAX (m + 9), above q N m (9.4 x 10^15 m at price PRICE_MAX) for every m, so no
        // genesis reaches G2 = G2_MAX; the G2_MAX guard stays and refuses G2_MAX + 1 first.
        // Re-derived (old -> new): G2 = G2_MAX at the fixture's J(1) "accepted" -> refused
        // by the muldiv rule; G2_MAX + 1 refused by the G2_MAX check as before.
        for (g2, needle) in [(G2_MAX, "muldiv"), (G2_MAX + 1, "G2_MAX")] {
            let mut g = wide.clone();
            g.controller.g2 = g2;
            g.controller.liability = g.controller.jackpot - g.controller.j_boost + g2 * (g.controller.m - 1);
            g.vault.liability = g.controller.liability;
            // M5 (D66): V0 <= ASSETS_MAX = TOTAL_MAX - 1 (was TOTAL_MAX). M6 (D84): S0 = V0 <=
            // SHARES_MAX, so the largest V0 is SHARES_MAX (was ASSETS_MAX).
            g.set_vault(SHARES_MAX);
            refuse(&format!("G2 = {g2}"), &g, needle);
        }
        // Review 3: G2 (m - 1) must not overflow. G2 = TOTAL_MAX = 2^62 and m = 5 give
        // 2^64, which wraps to 0 in a release build, so J = J_boost = L = 0 would pass the
        // L(1) formula; in a debug build the multiplication panics.
        let mut g = good.clone();
        g.controller.m = 5;
        for s in &mut g.shards {
            s.m = 5;
        }
        g.controller.jackpot = 0;
        g.controller.j_boost = 0;
        g.controller.g2 = TOTAL_MAX;
        g.controller.liability = 0;
        g.vault.liability = 0;
        refuse("G2 (m - 1) overflow", &g, "");
    }

    /// M3 re-review S1, S2: genesis windows and reserve that would make every draw()
    /// impossible (D_claim >= DAA_MAX at check 19, or a draw/close storage mass over the
    /// limit). Neither value can change after genesis.
    #[test]
    fn genesis_refuses_rereview_instances() {
        let good = case();
        assert_eq!(good.validate(), Ok(()));
        for (name, f, needle) in [
            ("claim_window = DAA_MAX - 1", (|g: &mut GenesisCase| g.drawer.claim_window = DAA_MAX - 1) as fn(&mut GenesisCase), "window"),
            ("draw_window = DAA_MAX - 1", |g| g.drawer.draw_window = DAA_MAX - 1, "window"),
            ("windows sum = WINDOWS_MAX + 1", |g| {
                g.drawer.draw_window = WINDOWS_MAX / 2;
                g.drawer.claim_window = WINDOWS_MAX - WINDOWS_MAX / 2 + 1;
            }, "window"),
            ("reserve = R_MIN - 1", |g| g.controller.reserve = R_MIN as i64 - 1, "reserve"),
            ("reserve = 1 KAS", |g| g.controller.reserve = KAS as i64, "reserve"),
        ] {
            let mut g = good.clone();
            f(&mut g);
            match g.validate() {
                Err(e) => assert!(e.contains(needle), "{name}: refused for another reason: {e}"),
                Ok(()) => panic!("{name}: bad instance accepted"),
            }
        }
        // M4 review U3: windows summing to WINDOWS_MAX were accepted here before U3; the
        // tighter rule seed_offset + draw_window + claim_window <= D_RET now refuses them
        // (D_RET < WINDOWS_MAX, so the WINDOWS_MAX check is implied while seed_offset >= 0;
        // it stays as a guard should D_RET change). The accepted boundary is in
        // `u3_genesis_refuses_windows_beyond_d_ret`.
        assert!(D_RET <= WINDOWS_MAX);
        let mut g = good.clone();
        g.drawer.draw_window = WINDOWS_MAX / 2;
        g.drawer.claim_window = WINDOWS_MAX - WINDOWS_MAX / 2;
        let e = g.validate().unwrap_err();
        assert!(e.contains("D_RET"), "windows sum = WINDOWS_MAX: refused for another reason: {e}");
        assert_eq!(good.controller.reserve, R_MIN as i64, "fixtures use R = R_MIN");
    }

    /// M4 review S1, user decision U3: the windows must fit inside the ticket's
    /// return_deposit_after delay, so a winning ticket of a round closed at T and drawn
    /// within draw_window cannot be forfeited by return_deposit_after (T + D_RET) before
    /// its pool's claim deadline (D + draw_window + claim_window, spec M7 D98), and every pool
    /// keeps at least 7 days to claim (design 4.5). Rules: claim_window >= CLAIM_WINDOW
    /// (6,048,000) and seed_offset + draw_window + claim_window <= D_RET.
    #[test]
    fn u3_genesis_refuses_windows_beyond_d_ret() {
        let good = case();
        assert_eq!(good.validate(), Ok(()));
        let so = good.controller.seed_offset;
        assert_eq!((good.drawer.claim_window, good.drawer.draw_window), (CLAIM_WINDOW, DRAW_WINDOW), "fixture defaults");
        for (name, f, needle) in [
            // Reviewer repros (S1).
            ("claim_window = 14 days", (|g: &mut GenesisCase| g.drawer.claim_window = 2 * CLAIM_WINDOW) as fn(&mut GenesisCase), "D_RET"),
            ("claim_window = 0", |g| g.drawer.claim_window = 0, "claim_window"),
            ("draw_window = 7 days", |g| g.drawer.draw_window = CLAIM_WINDOW, "D_RET"),
            // Boundaries.
            ("claim_window = CLAIM_WINDOW - 1", |g| g.drawer.claim_window = CLAIM_WINDOW - 1, "claim_window"),
            ("sum = D_RET + 1 (draw_window)", |g| g.drawer.draw_window = D_RET - g.controller.seed_offset - g.drawer.claim_window + 1, "D_RET"),
            ("sum = D_RET + 1 (claim_window)", |g| g.drawer.claim_window = D_RET - g.controller.seed_offset - g.drawer.draw_window + 1, "D_RET"),
            ("sum = D_RET + 1 (seed_offset)", |g| g.controller.seed_offset = D_RET - g.drawer.draw_window - g.drawer.claim_window + 1, "D_RET"),
            ("seed_offset = D_RET", |g| g.controller.seed_offset = D_RET, "D_RET"),
            // M7 S2-5: the anchor must lie strictly after the block that accepted close().
            ("seed_offset = 0", |g| g.controller.seed_offset = 0, "seed_offset"),
        ] {
            let mut g = good.clone();
            f(&mut g);
            match g.validate() {
                Err(e) => assert!(e.contains(needle), "{name}: refused for another reason: {e}"),
                Ok(()) => panic!("{name}: bad instance accepted"),
            }
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.build()));
            assert!(r.is_err(), "{name}: builder accepted the instance");
        }
        // Accepted boundaries: claim_window = CLAIM_WINDOW exactly (the default), and the
        // sum = D_RET exactly through each of the three terms.
        for (name, f) in [
            ("sum = D_RET (draw_window)", (|g: &mut GenesisCase| g.drawer.draw_window = D_RET - g.controller.seed_offset - g.drawer.claim_window) as fn(&mut GenesisCase)),
            ("sum = D_RET (claim_window)", |g| g.drawer.claim_window = D_RET - g.controller.seed_offset - g.drawer.draw_window),
            ("sum = D_RET (seed_offset)", |g| g.controller.seed_offset = D_RET - g.drawer.draw_window - g.drawer.claim_window),
            // Spec M7 D97: the draw_window floor is VOID_MIN (was 0 before M7).
            ("draw_window = VOID_MIN", |g| g.drawer.draw_window = VOID_MIN),
        ] {
            let mut g = good.clone();
            f(&mut g);
            assert_eq!(g.validate(), Ok(()), "{name}");
        }
        assert!(so + DRAW_WINDOW + CLAIM_WINDOW <= D_RET, "the defaults satisfy the rule");
    }

    /// Spec M7 D97, D103: draw_window < VOID_MIN is refused unless the TEST-ONLY
    /// `allow_short_void_window` flag is set, and the flag is refused off testnet/devnet.
    #[test]
    #[cfg(not(feature = "mainnet"))]
    fn m7_void_window_floor_and_test_only_flag() {
        let good = case();
        assert_eq!(good.drawer.draw_window, VOID_MIN);
        let mut short = good.clone();
        short.drawer.draw_window = 1_200;
        let e = short.validate().unwrap_err();
        assert!(e.contains("VOID_MIN"), "{e}");
        // The flag alone, with the committed templates' constants: refused (the production
        // drawer's check 1 would reject every draw and void of such an instance).
        let mut flag_only = short.clone();
        flag_only.allow_short_void_window = true;
        let e = flag_only.validate_with(&Templates::get(flag_only.b_max)).unwrap_err();
        assert!(e.contains("needs the TEST-ONLY short-void drawer variant"), "{e}");
        short.set_test_only_short_void_window(1_200);
        assert_eq!(short.validate(), Ok(()));
        short.network = NetworkType::Devnet;
        assert_eq!(short.validate(), Ok(()));
        for net in [NetworkType::Mainnet, NetworkType::Simnet] {
            let mut g = short.clone();
            g.network = net;
            let e = g.validate().unwrap_err();
            assert!(e.contains("allow_short_void_window"), "{net:?}: {e}");
        }
        let mut main = good.clone();
        main.network = NetworkType::Mainnet;
        let e = main.validate().unwrap_err();
        assert!(e.contains("testnet or devnet only until the release gate"), "{e}");
    }

    /// M7 Stage 5 fix A: the production drawer source carries the void floor VOID_MIN =
    /// 864,000 (= game::VOID_MIN = 2F) exactly once and requires it in check 1; the committed
    /// templates are not the short-void variant; the variant (the same source with only that
    /// line replaced) is reached only through `GenesisCase::templates()` with the TEST-ONLY
    /// flag, and `validate_with` refuses it without the flag.
    #[test]
    #[cfg(not(feature = "mainnet"))]
    fn m7_void_floor_in_production_drawer_and_variant_only_via_flag() {
        use crate::buy::{SHORT_VOID_LINE, Sources, short_void_drawer_source, void_min_line};
        assert_eq!(VOID_MIN, 864_000);
        assert_eq!(void_min_line(), "int constant VOID_MIN = 864000;");
        let src = crate::buy::read_covenant("drawer.sil");
        assert_eq!(src.lines().filter(|l| l.trim() == "int constant VOID_MIN = 864000;").count(), 1, "drawer.sil: the VOID_MIN constant line");
        assert_eq!(src.matches("int constant VOID_MIN").count(), 1);
        assert_eq!(src.lines().filter(|l| l.trim().starts_with("require(draw_window >= VOID_MIN);")).count(), 1, "drawer.sil: check 1 requires the floor");
        assert!(!src.contains(SHORT_VOID_LINE), "the committed drawer is not the TEST-ONLY variant");
        assert_eq!(Sources::committed(crate::buy::b_max()).drawer, src);
        // The variant differs from the source in exactly that line.
        let v = short_void_drawer_source(&src);
        let (a, b): (Vec<&str>, Vec<&str>) = (src.lines().collect(), v.lines().collect());
        assert_eq!(a.len(), b.len());
        let diff: Vec<(&str, &str)> = a.iter().zip(&b).filter(|(x, y)| x != y).map(|(x, y)| (x.trim(), y.trim())).collect();
        assert_eq!(diff, vec![("int constant VOID_MIN = 864000;", SHORT_VOID_LINE)]);
        // Template selection: only the flag reaches the variant.
        let good = case();
        let prod = Templates::get(good.b_max);
        assert!(!prod.short_void_window);
        assert!(Arc::ptr_eq(&good.templates(), &prod), "without the flag: the committed templates");
        let mut short = good.clone();
        short.set_test_only_short_void_window(1_200);
        let tv = short.templates();
        assert!(tv.short_void_window && Arc::ptr_eq(&tv, &Templates::test_only_short_void(good.b_max)));
        assert_ne!(tv.drawer.hash, prod.drawer.hash);
        // Without the flag the variant is refused, even at a draw_window >= VOID_MIN.
        let mut no_flag = short.clone();
        no_flag.allow_short_void_window = false;
        no_flag.drawer.draw_window = VOID_MIN;
        let e = no_flag.validate_with(&tv).unwrap_err();
        assert!(e.contains("requires allow_short_void_window"), "{e}");
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| no_flag.build_with(tv.clone())));
        assert!(r.is_err(), "the builder deploys the variant without the flag");
        // With the flag: testnet and devnet only.
        assert_eq!(short.validate(), Ok(()));
        let b = short.build();
        assert_eq!(b.outputs[short.drawer_output()].script_public_key, tv.drawer_spk(&short.drawer), "the variant drawer is deployed");
        for net in [NetworkType::Mainnet, NetworkType::Simnet] {
            let mut g = short.clone();
            g.network = net;
            assert!(g.validate().is_err(), "{net:?}");
        }
    }

    /// M8 S2c (review MF-A fix-up): the TEST-ONLY variant genesis carries the variant set's
    /// hashes in all nine template fields (the setter repoints the controller's route too:
    /// shard and marker commit, through ticket/pool and directly, to the drawer hash; params
    /// does not), and validate() checks each against that set: one bit flipped in any field,
    /// or the production hash put back, is refused, by validate() and by the builder.
    #[test]
    #[cfg(not(feature = "mainnet"))]
    fn s2c_mfa_variant_genesis_route_checked_against_variant_templates() {
        let good = case();
        let prod = Templates::get(good.b_max);
        let mut short = good.clone();
        short.set_test_only_short_void_window(1_200);
        let tv = short.templates();
        assert!(tv.short_void_window);
        assert_eq!(short.validate(), Ok(()));
        // Which hashes the variant changes (drawer -> marker, vault, pool -> position,
        // ticket -> request, shard) and which it keeps (controller, params, record).
        assert_ne!(tv.shard.hash, prod.shard.hash);
        assert_ne!(tv.marker.hash, prod.marker.hash);
        assert_eq!(tv.params.hash, prod.params.hash);
        assert_eq!(tv.controller.hash, prod.controller.hash);
        assert_eq!(tv.record.hash, prod.record.hash);
        let c = &short.controller;
        assert_eq!((c.shard_tpl, c.marker_tpl, c.params_tpl), (tv.shard.hash, tv.marker.hash, tv.params.hash));
        // The variant genesis against the production set is refused (route mismatch).
        assert!(short.validate_with(&prod).is_err());
        type Edit = fn(&mut GenesisCase, &Templates);
        let fields: [(&str, Edit); 9] = [
            ("controller.shard_tpl", |g, p| g.controller.shard_tpl = p.shard.hash),
            ("controller.marker_tpl", |g, p| g.controller.marker_tpl = p.marker.hash),
            ("controller.params_tpl", |g, p| g.controller.params_tpl = p.params.hash),
            ("vault.record_tpl", |g, p| g.vault.record_tpl = p.record.hash),
            ("vault.request_tpl", |g, p| g.vault.request_tpl = p.request.hash),
            ("vault.position_tpl", |g, p| g.vault.position_tpl = p.position.hash),
            ("drawer.pool_tpl", |g, p| g.drawer.pool_tpl = p.pool.hash),
            ("drawer.vault_tpl", |g, p| g.drawer.vault_tpl = p.vault.hash),
            ("drawer.ticket_tpl", |g, p| g.drawer.ticket_tpl = p.ticket.hash),
        ];
        let flip = |g: &mut GenesisCase, name: &str| {
            let f = match name {
                "controller.shard_tpl" => &mut g.controller.shard_tpl,
                "controller.marker_tpl" => &mut g.controller.marker_tpl,
                "controller.params_tpl" => &mut g.controller.params_tpl,
                "vault.record_tpl" => &mut g.vault.record_tpl,
                "vault.request_tpl" => &mut g.vault.request_tpl,
                "vault.position_tpl" => &mut g.vault.position_tpl,
                "drawer.pool_tpl" => &mut g.drawer.pool_tpl,
                "drawer.vault_tpl" => &mut g.drawer.vault_tpl,
                "drawer.ticket_tpl" => &mut g.drawer.ticket_tpl,
                _ => unreachable!(),
            };
            f[0] ^= 1;
        };
        for (name, to_prod) in fields {
            let mut g = short.clone();
            flip(&mut g, name);
            let e = g.validate().expect_err(name);
            assert!(e.contains("tpl"), "{name} flipped: refused for another reason: {e}");
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.build()));
            assert!(r.is_err(), "{name} flipped: the builder deploys it");
            // The production hash in a variant genesis (mixed sets): refused where it differs.
            let mut g = short.clone();
            to_prod(&mut g, &prod);
            let same = matches!(name, "controller.params_tpl" | "vault.record_tpl");
            if same {
                assert_eq!(g.validate(), Ok(()), "{name}: the production hash equals the variant's");
            } else {
                let e = g.validate().expect_err(name);
                assert!(e.contains("tpl"), "{name} = production hash: refused for another reason: {e}");
            }
        }
    }

    /// The Rust D_RET mirrors `int constant D_RET` of ticket.sil (the genesis window rule
    /// above is only sound if they agree).
    #[test]
    fn d_ret_mirrors_ticket_sil() {
        let src = crate::buy::read_covenant("ticket.sil");
        let line = src.lines().find(|l| l.trim_start().starts_with("int constant D_RET = ")).expect("D_RET line in ticket.sil");
        let v: i64 = line.trim().trim_start_matches("int constant D_RET = ").trim_end_matches(';').parse().expect("D_RET value");
        assert_eq!(v, D_RET, "ticket.sil D_RET != game::D_RET");
        assert_eq!(src.matches("int constant D_RET").count(), 1);
    }

    /// M4 review U5: the Rust A_MAX mirrors `int constant A_MAX` of drawer.sil.
    #[test]
    fn a_max_mirrors_drawer_sil() {
        let src = crate::buy::read_covenant("drawer.sil");
        let line = src.lines().find(|l| l.trim_start().starts_with("int constant A_MAX = ")).expect("A_MAX line in drawer.sil");
        let v: usize = line.trim().trim_start_matches("int constant A_MAX = ").trim_end_matches(';').parse().expect("A_MAX value");
        assert_eq!(v, crate::game::A_MAX, "drawer.sil A_MAX != game::A_MAX");
        assert_eq!(v, 7, "U5: one draw() absorbs as many pools as it creates (7)");
    }

    /// KP19 (spec M4 A9): the drawer's ticket_tpl is the compiled ticket hash (D54) and is
    /// verified; d < D_FLOOR is refused (D81, was D57's D_CLAIM_FLOOR); D_FLOOR itself is
    /// accepted.
    #[test]
    fn kp19_genesis_ticket_tpl_and_claim_floor() {
        let good = case();
        assert_eq!(good.validate(), Ok(()));
        let t = Templates::get(good.b_max);
        assert_eq!(good.drawer.ticket_tpl, t.ticket.hash, "genesis drawer ticket_tpl = compiled ticket hash");
        assert_ne!(t.ticket.hash, [0u8; 32]);
        // The built genesis carries the drawer with that ticket_tpl (output spk).
        let built = good.build();
        let spk = t.drawer_spk(&good.drawer);
        assert_eq!(built.outputs[good.drawer_output()].script_public_key, spk);
        // A bad ticket_tpl is refused by validate() and by the builder.
        for bad_tpl in [[0u8; 32], t.pool.hash, {
            let mut h = t.ticket.hash;
            h[31] ^= 1;
            h
        }] {
            let mut g = good.clone();
            g.drawer.ticket_tpl = bad_tpl;
            let e = g.validate().unwrap_err();
            assert!(e.contains("ticket_tpl"), "refused for another reason: {e}");
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.build()));
            assert!(r.is_err(), "builder accepted a bad ticket_tpl");
        }
        // d below the floor (D81); every shard carries the controller's d.
        let with_d = |d: i64| {
            let mut g = good.clone();
            g.controller.d = d;
            for s in &mut g.shards {
                s.d = d;
            }
            g
        };
        for d in [D_FLOOR - 1, 11_000_000, KAS as i64 / 10, 1] {
            let g = with_d(d);
            let e = g.validate().unwrap_err();
            assert!(e.contains("D_FLOOR") && e.contains("D81"), "d = {d}: refused for another reason: {e}");
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.build()));
            assert!(r.is_err(), "builder accepted d = {d}");
        }
        assert_eq!(good.controller.d, D_FLOOR, "fixtures use d = D_FLOOR");
        assert_eq!(with_d(D_FLOOR).validate(), Ok(()));
    }

    /// LA6 (spec M6 A8): genesis below the floors is refused by `validate` and by the
    /// builder: d = D_FLOOR - 1 (names D81), V0 = V0_MIN - 1 (names D84) and a V0 whose
    /// S0 = V0 exceeds SHARES_MAX (names D84); the floors themselves are accepted. The
    /// jackpot is 0 (L(1) = 0), so the cap check cannot refuse a small V0.
    #[test]
    fn la6_genesis_below_the_floors() {
        let mut zero = case();
        zero.set_jackpot(0);
        assert_eq!(zero.validate(), Ok(()));
        let with = |f: &dyn Fn(&mut GenesisCase)| {
            let mut g = zero.clone();
            f(&mut g);
            g
        };
        let set_d = |g: &mut GenesisCase, d: i64| {
            g.controller.d = d;
            for s in &mut g.shards {
                s.d = d;
            }
        };
        let bad: Vec<(&str, GenesisCase, &str)> = vec![
            ("d = D_FLOOR - 1", with(&|g| set_d(g, D_FLOOR - 1)), "D81"),
            ("V0 = V0_MIN - 1", with(&|g| g.set_vault(V0_MIN - 1)), "D84"),
            ("V0 = 1", with(&|g| g.set_vault(1)), "D84"),
            ("S0 = V0 = SHARES_MAX + 1", with(&|g| g.set_vault(SHARES_MAX + 1)), "D84"),
            ("S0 = V0 = ASSETS_MAX", with(&|g| g.set_vault(ASSETS_MAX)), "D84"),
        ];
        let mut accepted = vec![];
        for (name, g, needle) in &bad {
            assert_eq!(g.vault.total_shares, g.vault.total_assets, "{name}: S0 = V0 (SHARE_UNIT = 1)");
            match g.validate() {
                Err(e) => assert!(e.contains(needle), "{name}: refused for another reason: {e}"),
                Ok(()) => {
                    accepted.push(*name);
                    continue;
                }
            }
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.build()));
            assert!(r.is_err(), "{name}: builder accepted the instance");
        }
        assert!(accepted.is_empty(), "LA6: genesis accepts {accepted:?}");
        // The floors themselves are accepted.
        for (name, g) in [("d = D_FLOOR", with(&|g| set_d(g, D_FLOOR))), ("V0 = V0_MIN", with(&|g| g.set_vault(V0_MIN))), ("V0 = SHARES_MAX", with(&|g| g.set_vault(SHARES_MAX)))] {
            assert_eq!(g.validate(), Ok(()), "{name}");
        }
    }

    /// User decision U-M6-1 (spec M6, answer to the Stage 3a D90 stop): genesis is exempt
    /// from the 90 % margin, but the builder refuses a genesis whose exact compute, transient
    /// or storage mass exceeds 100 % of the mainnet limits (such a transaction could never be
    /// mined). Shape: the fixture genesis at K = K_MAX (22 outputs) with its change output
    /// shrunk by raising the fee: 0.2 KAS of change adds C / 0.2 KAS = 50,000 grams of storage
    /// to the measured 460,532 (Stage 3a), so it is over 500,000; 1,000 sompi of change is far
    /// over. Large change and no change (exact funding) are accepted.
    #[test]
    fn u_m6_1_genesis_refuses_mass_over_the_limit() {
        let mut l = Ledger::new();
        let op = l.fund(GENESIS_FUNDING, funding_spk(), 100);
        let e = l.get(&op).unwrap().clone();
        let good = GenesisCase::new(K_MAX, vec![(op, e)]);
        assert_eq!(good.validate(), Ok(()));
        good.build();
        let outs: u64 = good.controller_value()
            + good.reserve()
            + (0..good.k()).map(|i| good.shard_value(i)).sum::<u64>()
            + good.vault_value()
            + 2 * good.reserve();
        let with_change = |change: u64| {
            let mut g = good.clone();
            g.fee = GENESIS_FUNDING - outs - change;
            g
        };
        for change in [KAS / 5, 1_000] {
            let g = with_change(change);
            assert_eq!(g.validate(), Ok(()), "change {change}: validate() alone accepts it");
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.build()));
            let msg = match r {
                Ok(_) => panic!("U-M6-1: builder accepted a genesis with {change} sompi of change (storage mass over 100 % of the limit)"),
                Err(p) => p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default(),
            };
            assert!(msg.contains("U-M6-1") && msg.contains("storage"), "change {change}: refused for another reason: {msg}");
        }
        // 2 KAS of change (C / 2 KAS = 5,000 grams) is accepted.
        with_change(2 * KAS).build();
        // No change: funding exactly covers the outputs and the fee.
        let mut l = Ledger::new();
        let op = l.fund(outs + good.fee, funding_spk(), 100);
        let e = l.get(&op).unwrap().clone();
        let mut g = good.clone();
        g.funding = vec![(op, e)];
        g.change = false;
        g.build();
    }
}
