//! Off-chain genesis builder (spec M2 D31, M3 D50): one transaction creates the round 1
//! controller, the params, K shards, the vault (round 0) and the drawer (round 1) under a
//! fresh KIP-20 covenant ID.
//!
//! Shape: inputs = deployer funding (P2PK, signed with `funding_sk`); outputs 0 controller,
//! 1 params, 2 + i shard i, k + 2 vault, k + 3 drawer, then optional change. The vault and
//! drawer are appended after the shards so the M2 indices are kept. One
//! `GenesisCovenantGroup` authorized by funding input 0 covers outputs 0..=k+3, so the
//! covenant ID is
//! H(outpoint of input 0, index/value/script of each of those outputs)
//! (rusty-kaspa v2.1.0 `consensus/core/src/hashing/covenant_id.rs:16-31`,
//! `consensus/core/src/tx.rs:335-385`). No covenant script runs at genesis: the deployer
//! commits to every initial state (D31, D50).
//!
//! The drawer takes a TEST-ONLY seed (D42), so the builder refuses any network other than
//! testnet or devnet, and a build without the `test-seed` feature has no drawer and builds no
//! genesis (D43).
//!
//! Because nothing on chain validates genesis, `build*` refuses an instance that breaks an
//! A3 range or the share-room invariant (`GenesisCase::validate`). `close()` check 2 rejects
//! such a controller anyway (M2 review finding R2), so a bad genesis could never close.

use std::sync::Arc;

use harness::{Hash, TransactionOutpoint, TransactionOutput, UtxoEntry};
pub use kaspa_consensus_core::network::NetworkType;
use kaspa_consensus_core::tx::GenesisCovenantGroup;

use crate::buy::{Built, FUNDING_SK, Final, In, InKind, KAS, Templates, b_max, spk_of};
use crate::close::{CloseCase, z18};
use crate::game::{BP, CLAIM_WINDOW, DAA_MAX, DRAW_WINDOW, F_MIN, F_MIN_MAX, M_DRAW, M_HARD, S_F_CAP, S_R_CAP, TOTAL_MAX, muldiv};
use crate::state::{ControllerState, DrawerState, K_MAX, ParamsState, ShardState, VaultState};

/// Spec M2 A3 range bounds that have no named constant elsewhere (as in `controller.sil`).
const PRICE_MAX: i64 = 100_000_000_000;
const DEPOSIT_MAX: i64 = 10_000_000_000;
const RESERVE_MAX: i64 = 10_000_000_000;

#[derive(Clone)]
pub struct GenesisCase {
    pub b_max: usize,
    pub controller: ControllerState,
    /// Initial boost pot O (normally 0); controller value = O + acc + reserve.
    pub pot: u64,
    pub params: ParamsState,
    pub shards: Vec<ShardState>,
    /// The house vault (D50): round 0, total_assets = V0 = controller v_rec, total_shares = 0,
    /// pending fields 0, liability = L(1). Value total_assets + pend_dep + R.
    pub vault: VaultState,
    /// The drawer (D37, D49): round 1, value R.
    pub drawer: DrawerState,
    /// Target network; only testnet and devnet are accepted (D43).
    pub network: NetworkType,
    /// Funding inputs: outpoint (e.g. from a `Ledger`) and entry.
    pub funding: Vec<(TransactionOutpoint, UtxoEntry)>,
    pub change: bool,
    pub fee: u64,
    pub funding_sk: [u8; 32],
}

impl GenesisCase {
    /// Genesis of the `CloseCase::new(k)` fixture's round 1 (zero sales, drawn = 1 with
    /// J(1), J_boost(1), G2(1), L(1) and V_rec from deployment inputs), boost pot 0 unless
    /// set, funded by `funding`.
    pub fn new(k: usize, funding: Vec<(TransactionOutpoint, UtxoEntry)>) -> Self {
        Self::new_with(k, funding, &Templates::get(b_max()))
    }

    pub fn new_with(k: usize, funding: Vec<(TransactionOutpoint, UtxoEntry)>, t: &Templates) -> Self {
        let c = CloseCase::new_with(k, t);
        let controller = ControllerState { j_boost: 0, ..c.controller };
        let vault = VaultState {
            round: controller.round - 1,
            total_assets: controller.v_rec,
            total_shares: 0,
            pend_dep: 0,
            pend_wsh: 0,
            liability: controller.liability,
        };
        let drawer = DrawerState {
            round: controller.round,
            pool_tpl: t.pool.hash,
            vault_tpl: t.vault.hash,
            f_min: F_MIN,
            claim_window: CLAIM_WINDOW,
            draw_window: DRAW_WINDOW,
        };
        Self {
            b_max: t.b_max,
            controller,
            pot: 0,
            params: c.params,
            shards: c.shards.into_iter().map(|s| s.state).collect(),
            vault,
            drawer,
            network: NetworkType::Testnet,
            funding,
            change: true,
            fee: KAS / 2,
            funding_sk: FUNDING_SK,
        }
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

    /// Set the initial vault V0 (total_assets and the controller's v_rec, D50).
    pub fn set_vault(&mut self, v0: i64) {
        self.vault.total_assets = v0;
        self.controller.v_rec = v0;
    }

    /// Set J(1) with J_boost(1) = 0, G2(1) = muldiv(J, g) and L(1) = J + G2 (m - 1), in the
    /// controller and the vault's liability.
    pub fn set_jackpot(&mut self, jackpot: i64) {
        let c = &mut self.controller;
        c.jackpot = jackpot;
        c.j_boost = 0;
        c.g2 = muldiv(jackpot, c.g);
        c.liability = jackpot + c.g2 * (c.m - 1);
        self.vault.liability = c.liability;
    }

    /// Vault value total_assets + pend_dep + R.
    pub fn vault_value(&self) -> u64 {
        (self.vault.total_assets + self.vault.pend_dep) as u64 + self.reserve()
    }

    pub fn reserve(&self) -> u64 {
        self.controller.reserve as u64
    }

    pub fn controller_value(&self) -> u64 {
        self.pot + self.controller.acc as u64 + self.reserve()
    }

    /// Shard i's value S - P_R + R (S = P_R = 0 at genesis).
    pub fn shard_value(&self, i: usize) -> u64 {
        (self.shards[i].sales - self.shards[i].ref_paid) as u64 + self.reserve()
    }

    /// Every A3 range of the initial states (the bounds `close()` check 2 applies, spec M2
    /// A3), the share-room invariant sum(st) + s_C + s_D + S_F_CAP + S_R_CAP <= 10^4 (review
    /// finding R2: without it an admin change within the caps can make s_H' < 0 and block
    /// every later `close()`), and the consistency of the params and shards with the
    /// controller.
    pub fn validate(&self) -> Result<(), String> {
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
        range("d", c.d, 1, DEPOSIT_MAX)?;
        range("s_f", c.s_f, 0, S_F_CAP)?;
        range("s_r", c.s_r, 0, S_R_CAP)?;
        range("s_c", c.s_c, 0, BP)?;
        range("s_d", c.s_d, 0, BP)?;
        for (i, &st) in c.st.iter().enumerate() {
            range(&format!("st{}", i + 1), st, 0, BP)?;
        }
        let room = c.st.iter().sum::<i64>() + c.s_c + c.s_d + S_F_CAP + S_R_CAP;
        if room > BP {
            return Err(format!("share room: sum(st) + s_C + s_D + S_F_CAP + S_R_CAP = {room} > {BP}"));
        }
        range("e", c.e, 0, 5_000)?;
        range("j", c.j, 0, 5_000)?;
        range("g", c.g, 0, 1_000)?;
        range("margin", c.margin, BP, 2 * BP)?;
        range("cap", c.cap, 0, BP)?;
        for (name, v) in [("jackpot", c.jackpot), ("j_boost", c.j_boost), ("g2", c.g2), ("liability", c.liability), ("v_rec", c.v_rec), ("acc", c.acc)] {
            range(name, v, 0, TOTAL_MAX)?;
        }
        range("reserve", c.reserve, 1, RESERVE_MAX)?;
        for (name, v) in [("deadline", c.deadline), ("round_len", c.round_len), ("d_min", c.d_min), ("seed_offset", c.seed_offset)] {
            range(name, v, 0, DAA_MAX - 1)?;
        }
        if c.fee_class > 2 || (c.fee_class != 1 && c.fee_key[32] != 0) {
            return Err("fee address is not a D11 standard script".into());
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
        if c.liability != c.jackpot - c.j_boost + c.g2 * (c.m - 1) {
            return Err("controller liability is not J_house + G2 (m - 1)".into());
        }
        let v = &self.vault;
        if v.round != c.round - 1 {
            return Err(format!("vault round {} != controller round - 1", v.round));
        }
        range("vault total_assets", v.total_assets, 0, TOTAL_MAX)?;
        if v.total_shares != 0 || v.pend_dep != 0 || v.pend_wsh != 0 {
            return Err("vault: total_shares and pending fields must be 0 at genesis".into());
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
        range("f_min", d.f_min, 0, F_MIN_MAX)?;
        range("claim_window", d.claim_window, 0, DAA_MAX - 1)?;
        range("draw_window", d.draw_window, 0, DAA_MAX - 1)?;
        // D43: the drawer takes a TEST-ONLY seed.
        if !matches!(self.network, NetworkType::Testnet | NetworkType::Devnet) {
            return Err(format!("network {:?}: the TEST-ONLY seed drawer is allowed only on testnet or devnet (D43)", self.network));
        }
        if cfg!(not(feature = "test-seed")) {
            return Err("no drawer in a build without the test-seed feature (D43)".into());
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
        self.build_with(Templates::get(self.b_max))
    }

    /// Panics if the instance fails `validate()` or the drawer's route does not match `t`.
    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        if let Err(e) = self.validate() {
            panic!("genesis refused: {e}");
        }
        if self.drawer.pool_tpl != t.pool.hash || self.drawer.vault_tpl != t.vault.hash {
            panic!("genesis refused: drawer pool_tpl / vault_tpl do not match the templates");
        }
        self.build_unchecked_for_tests(t)
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
        outputs.push(TransactionOutput::new(self.reserve(), drawer_spk(&t, &self.drawer)));
        let total_in: u64 = inputs.iter().map(|i| i.entry.amount).sum();
        let total_out: u64 = outputs.iter().map(|o| o.value).sum();
        let change = total_in.checked_sub(total_out + self.fee).expect("funding covers genesis");
        if self.change {
            outputs.push(TransactionOutput::new(change, spk_of(&self.funding_sk)));
        } else {
            assert_eq!(change, 0, "no-change genesis needs exact funding");
        }
        let group = GenesisCovenantGroup::new(0, (0..(self.k() + 4) as u32).collect());
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
        }
    }
}

#[cfg(feature = "test-seed")]
fn drawer_spk(t: &Templates, d: &DrawerState) -> harness::ScriptPublicKey {
    t.drawer_spk(d)
}

/// D43: without the `test-seed` feature there is no drawer, so there is no genesis.
#[cfg(not(feature = "test-seed"))]
fn drawer_spk(_: &Templates, _: &DrawerState) -> harness::ScriptPublicKey {
    panic!("no drawer in a build without the test-seed feature (D43)")
}

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
        let mut edge = good.clone();
        edge.controller.st = [4_000, 3_000, 400, 0, 0];
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
            ("margin", |g| g.controller.margin = BP - 1),
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
            ("vault shares", |g| g.vault.total_shares = 1),
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
        // A mainnet genesis is refused by the builder (D43).
        let mut main = good.clone();
        main.network = NetworkType::Mainnet;
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| main.build()));
        assert!(r.is_err(), "builder accepted a mainnet genesis");
    }
}
