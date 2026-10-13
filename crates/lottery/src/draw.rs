//! `draw()` (spec M3 A4, M5 A5, M7 A4): the seed is the anchor block's hash X.hash, proven
//! by the 304-byte anchor argument (spec M7 D93-D95, `crate::anchor`); an empty anchor is a
//! void (D96: no winners, everything but the fee and C_draw to the boost pot). This is the
//! only place `covenants/drawer.sil` is compiled.
//!
//! Shape (A4, extended by spec M5 A5; np = 0 in a void):
//!   inputs : 0 drawer r (leader, `draw`); 1 marker r (`draw_leg`); 2 vault r - 1
//!            (`draw_leg`); 3 controller r + 1, drawn = 0 (`draw_leg`); 4 + i absorbed pool i
//!            (`absorb_leg`), i < a <= A_MAX; 4 + a .. caller funding (zero ID, 1..F_MAX)
//!   outputs: 0 drawer'; 1 vault'; 2 controller'; 3 .. 2 + np the new pools in tier order;
//!            3 + np the settlement record iff anything was pending (spec M5 A5 C3; all
//!            authorized by input 0 under the instance covenant ID); then plain: the fee
//!            output (only if A + F >= f_min), the marker refund, one refund per absorbed pool
//!            in input order, the caller output (last).
//!
//! Spec M5 (D65): the vault's pending totals are converted at the post-settlement price
//! (`game::convert`, C1-C3) and the record is written; the caller posts the record's R.
//! The entry is called by its M7 signature (`SIG_DRAW`: the anchor first, mk_daa last; wc,
//! record_prefix and record_suffix before mk_daa as in M5). The M4 signature was removed in
//! M6 (D89); the M5 TEST-ONLY entry in M7 (D101).
//!
//! Spec M7 (A7): the case runs against a `harness::ChainSim` (`DrawCase::sim`, default
//! `ChainSim::anchored(D, x_hash)`), so `OpChainblockSeqCommit` sees the anchor.
//!
//! `DrawCase` describes a draw with public fields; `expected()` derives every output from
//! the Rust reference in `game` (never from the script); `build()` returns an editable
//! `Built`. Tests change one case field (input-side attacks), one `Lie` field (a consistent
//! false claim: the builder computes every output from it, so only the check that catches
//! the claim can reject), or one thing in the `Built` (output-side attacks).

use std::sync::Arc;

use harness::{ArtifactValue, ChainSim, CovenantBinding, Hash, Transaction, TransactionId, TransactionOutpoint, TransactionOutput, UtxoEntry, compile_source, outpoint};

use crate::anchor::AnchorProof;
use crate::buy::{Built, COV, FUNDING_SK, In, InKind, KAS, Template, Templates, b_max, spk_of};
use crate::close::{CloseCase, FundingIn, TEST_DEADLINE, TEST_ROUND_LEN};
use crate::combo::unrank;
use crate::game::{
    CLAIM_WINDOW, Conversion, DRAW_WINDOW, F_MIN, FeeSplit, JackpotFix, JackpotRule, PoolSet, RoundAmounts, SeedNumbers, VA, VS,
    anchor_daa, boost_after, claim_deadline, convert, draw_amounts, fee_split, fix_jackpot, guarded_add, muldiv, pool_set, seed_numbers, vault_settle,
    void_lock, void_pool_set, win_counts, withdraw_out,
};
use crate::params::xonly;
use crate::smt::{Tree, proofs_at};
use crate::state::{ControllerState, DrawerState, K_MAX, MarkerState, Payout, PoolState, RecordState, TIER_LOWER0, VaultState};

/// Compile the drawer source with the controller template hash as its constructor constant
/// (D38). The template hash does not depend on the state values. Public for the tests'
/// check-attribution instrument (`tests/common/m2.rs`, `M3_ATTRIB`), which compiles truncated
/// drawer sources.
pub fn compile_drawer(src: &str, controller_hash: [u8; 32]) -> Template {
    Template::from(compile_source(src, &dummy_drawer().ctor(controller_hash)))
}

/// Placeholder drawer state for compiling.
fn dummy_drawer() -> DrawerState {
    DrawerState { round: 1, pool_tpl: [0; 32], vault_tpl: [0; 32], ticket_tpl: [0; 32], f_min: F_MIN, claim_window: CLAIM_WINDOW, draw_window: DRAW_WINDOW }
}

/// Creation DAA score of the round 1 marker in the standalone fixture (round r:
/// + (r - 1) x 10^6), so D = TEST_MARKER_DAA + SEED_OFFSET = 1,000,600 as the M3-M6 seed_ref.
pub const TEST_MARKER_DAA: i64 = 1_000_000;
/// The fixture's anchor block hash X.hash (the seed source, D94). Any non-zero value works;
/// zero is refused because `ChainSim` marks its roots with a zero parent hash.
pub const TEST_X_HASH: [u8; 32] = [0x58; 32];
/// Fixture vault V (10,000,000 KAS, as the genesis fixture).
pub const TEST_VAULT: i64 = 10_000_000 * KAS as i64;

// Argument positions of `draw` (spec M7 A4 "Arguments", drawer.sil order).
/// `byte[] anchor`: 304 bytes (X fields, P fields; spec M7 A3) or empty (void).
pub const ARG_ANCHOR: usize = 0;
pub const ARG_WIN5: usize = 1;
pub const ARG_LEAVES: usize = 2;
pub const ARG_SIBLINGS: usize = 3;
pub const ARG_X1: usize = 4;
pub const ARG_Y: usize = 5;
pub const ARG_X2: usize = 6;
pub const ARG_CTRL_PL: usize = 7;
pub const ARG_MARKER_PL: usize = 9;
pub const ARG_VAULT_PL: usize = 11;
pub const ARG_POOL_PL: usize = 13;
pub const ARG_POOL_PREFIX: usize = 15;
pub const ARG_POOL_SUFFIX: usize = 16;
pub const ARG_FUNDER_CLASS: usize = 17;
pub const ARG_FUNDER_KEY: usize = 18;
/// Spec M5 A5: the withdrawal witness wc and the record template's prefix and suffix,
/// before mk_daa.
pub const ARG_WC: usize = 19;
pub const ARG_RECORD_PREFIX: usize = 20;
pub const ARG_RECORD_SUFFIX: usize = 21;
/// `int mk_daa`: the marker's creation DAA score, required equal to `OpTxInputDaaScore(1)`
/// by check 7 (spec M7 D93, D98; replaces own_daa at the same position), so the round result
/// record stays derivable from the transaction bytes (M3 A9).
pub const ARG_MK_DAA: usize = 22;
/// Number of `draw` arguments.
pub const DRAW_ARGS: usize = 23;

/// The M7 signature (spec M7 A4 "Arguments", 23 arguments): `byte[] anchor` replaces the
/// TEST-ONLY 32-byte seed, `int mk_daa` replaces own_daa, the rest as M5.
pub const SIG_DRAW: &str =
    "draw(byte[],byte[5],byte[],byte[],int,int,int,int,int,int,int,int,int,int,int,byte[],byte[],byte,byte[33],int,byte[],byte[],int)";

/// An expired pool of an earlier round absorbed by this draw (input 4 + i). Value
/// remaining + R.
#[derive(Clone, Debug)]
pub struct PoolIn {
    pub state: PoolState,
    pub remaining: u64,
    pub daa: u64,
}

/// TEST-ONLY consistent false claims (attack tests). Each set field replaces one value the
/// reference would derive; the builder then computes every output from it, so the
/// transaction is self-consistent except for that claim.
#[derive(Clone, Debug, Default)]
pub struct Lie {
    /// (h_J, n2) instead of the counts the proofs show (DA2).
    pub counts: Option<(i64, i64)>,
    /// A rank other than the seed's; leaves, siblings and win5 follow it (DA12).
    pub rank: Option<u32>,
    /// win5 bytes instead of unrank(rank) (DA12 non-canonical).
    pub win5: Option<[u8; 5]>,
    /// A bonus other than the seed's; h_J, n2 and the pools' win follow it (DA12).
    pub bonus: Option<u8>,
    /// (x1, y, x2) instead of the reference witnesses (DA4).
    pub witness: Option<(i64, i64, i64)>,
    /// Fee paid or carried regardless of T_F (DA5).
    pub paid: Option<bool>,
    /// Added to V' (before J fixing) (DA3, DA7).
    pub v_delta: i64,
    /// Added to O' (before J fixing) (DA6, DA7).
    pub o_delta: i64,
    /// `mk_daa` argument instead of the marker's true creation DAA score; D and D_claim
    /// follow it (spec M7 SA5; replaces the M3 `own` lie of DA17).
    pub mk_daa: Option<i64>,
    /// The `anchor` argument bytes instead of the chain's proof (spec M7 SA1-SA4, SA7). In
    /// draw mode the seed follows bytes 0..32 (the claimed X.hash) when there are at least
    /// 32; the mode itself follows `DrawCase::void`, not this length.
    pub anchor: Option<Vec<u8>>,
    // Spec M5 (stage 2 attacks). Each changes the conversion the builder writes; vault',
    // the record and controller' v_rec follow it.
    /// The withdrawal witness wc instead of the reference's; out_c = out(wc) follows (VA5).
    pub wc: Option<i64>,
    /// Convert at this V instead of V' (VA7 stale or future price). vault' total_assets is
    /// still V' + D_c - out_c.
    pub conv_v: Option<i64>,
    /// Fix J(r+1) from this V instead of V' (VA14); L' follows.
    pub j_v: Option<i64>,
    /// The refund decision instead of C1's (VA1, VA6): `Some(false)` converts D at
    /// mint = floor(D (S + VS) / (V' + VA)) even when C1 refunds; `Some(true)` refunds.
    pub refund: Option<bool>,
    /// Write a record (`Some(true)`, all zero when nothing is pending) or omit it
    /// (`Some(false)`) regardless of what is pending (VA8).
    pub record: Option<bool>,
    /// Added to out_c after wc (VA5 out_c + 1).
    pub out_delta: i64,
}

#[derive(Clone)]
pub struct DrawCase {
    pub b_max: usize,
    /// Drawer of round r (input 0); value R.
    pub drawer: DrawerState,
    pub drawer_daa: u64,
    /// Marker of round r (input 1); value S - P_R - C_close + J_boost + R. Its roots must be
    /// the trees' roots for a valid case (`sync_roots`).
    pub marker: MarkerState,
    pub marker_daa: u64,
    /// Vault of round r - 1 (input 2); value total_assets + pend_dep + R.
    pub vault: VaultState,
    pub vault_daa: u64,
    /// Controller of round r + 1, drawn = 0 (input 3); value O + acc + R.
    pub controller: ControllerState,
    /// Boost pot O.
    pub pot: u64,
    pub controller_daa: u64,
    /// The K shard trees of round r (m = marker.m); the count proofs come from them.
    pub trees: Vec<Tree>,
    pub absorbed: Vec<PoolIn>,
    pub funding: Vec<FundingIn>,
    /// Spec M7 D94: the anchor block's hash, the seed source. With `sim = None` the default
    /// chain is `ChainSim::anchored(D, x_hash)`, so this is X.hash; with a given `sim` the
    /// anchor (and X.hash) is the sim's first chain block with DAA >= D and this field is
    /// not used (`x_hash()`).
    pub x_hash: [u8; 32],
    /// Spec M7 A7: the selected chain the scripts see. `None` = `ChainSim::anchored(D,
    /// x_hash)`, rebuilt from the current fields at every `build`, so a test that changes
    /// marker_daa, the controller's seed_offset or x_hash keeps a valid anchor.
    pub sim: Option<Arc<ChainSim>>,
    /// Spec M7 D96: a void (empty anchor, lock >= D + draw_window, no winners).
    pub void: bool,
    /// Reserve funder of the new pools (argument and pool field).
    pub funder: Payout,
    /// Transaction fee.
    pub fee: u64,
    /// Lock time: max(D, absorbed deadlines) for a valid draw, max(D + draw_window, absorbed
    /// deadlines) for a valid void (D51, spec M7 check 7).
    pub lock_time: u64,
    pub lie: Lie,
    pub cov: Hash,
    pub funding_sk: [u8; 32],
}

/// Expected outputs of a draw (A4 checks 8-25), from the `game` reference.
#[derive(Clone, Debug)]
pub struct DrawExpected {
    pub seed: SeedNumbers,
    /// Effective rank, bonus and main numbers (the seed's unless a `Lie` replaces them).
    pub rank: u32,
    pub bonus: u8,
    pub win5: [u8; 5],
    pub leaves: Vec<u8>,
    pub siblings: Vec<u8>,
    /// Each shard's m counts at the rank.
    pub counts: Vec<Vec<u16>>,
    pub h_j: i64,
    pub n2: i64,
    pub amounts: RoundAmounts,
    pub j_house: i64,
    pub pools: PoolSet,
    /// V' and O' (including any `Lie` deltas).
    pub v_prime: i64,
    pub q: Vec<i64>,
    pub o_prime: i64,
    pub fee: FeeSplit,
    pub rule: JackpotRule,
    pub fix: JackpotFix,
    /// Spec M7: the mk_daa argument (the `Lie` applied), D = mk_daa + seed_offset and
    /// D_claim = D + draw_window + claim_window (check 19).
    pub mk_daa: i64,
    pub d: i64,
    pub d_claim: i64,
    /// Spec M7 D96: a void (np = 0, win5 = 0, bonus = 0, no leaves).
    pub void: bool,
    /// The anchor argument (empty in a void).
    pub anchor: Vec<u8>,
    pub drawer: DrawerState,
    pub vault: VaultState,
    pub controller: ControllerState,
    /// The new pools, output order 3..2+np.
    pub new_pools: Vec<PoolState>,
    /// Spec M5 A5: the conversion (C1-C2, `Lie` applied), whether the record is written
    /// (C3), and the record (output 3 + np).
    pub conv: Conversion,
    pub rec: bool,
    pub record: Option<RecordState>,
    /// Every output value in order (the caller output last, as the remainder).
    pub values: Vec<u64>,
    /// Number of absorbed pools a.
    pub a: usize,
}

impl DrawExpected {
    pub fn np(&self) -> usize {
        self.pools.np()
    }
    pub fn pool_output(&self, p: usize) -> usize {
        3 + p
    }
    /// The record's index 3 + np (spec M5 A5 check 13), if written.
    pub fn record_output(&self) -> Option<usize> {
        self.rec.then_some(3 + self.np())
    }
    /// nCov = 3 + np + rec (spec M5 A5 check 13).
    pub fn n_cov(&self) -> usize {
        3 + self.np() + self.rec as usize
    }
    /// The fee output's index, if the fee is paid.
    pub fn fee_output(&self) -> Option<usize> {
        self.fee.paid.then_some(self.n_cov())
    }
    pub fn marker_refund(&self) -> usize {
        self.n_cov() + self.fee.paid as usize
    }
    pub fn pool_refund(&self, i: usize) -> usize {
        self.marker_refund() + 1 + i
    }
    pub fn caller_output(&self) -> usize {
        self.marker_refund() + 1 + self.a
    }
    /// 3 + np + rec + paid + 1 + a + 1 (A4 check 20, spec M5 A5).
    pub fn n_outputs(&self) -> usize {
        self.caller_output() + 1
    }
}

impl DrawCase {
    /// A valid draw of round 1 with `k` empty shard trees (m = 10), zero sales, the fixture
    /// marker (J = 1,000,000 KAS, J_boost = 50,000 KAS, G2 = 10,000 KAS), the fixture vault
    /// (10,000,000 KAS), boost pot 100,000 KAS, fee accumulator 2 KAS and one fresh funding
    /// input of 100 KAS.
    pub fn new(k: usize) -> Self {
        Self::new_with(k, &Templates::get(b_max()))
    }

    pub fn new_with(k: usize, t: &Templates) -> Self {
        Self::at_round_with(k, 1, t)
    }

    /// As `new` for round `r` (round r + 1 controller, round r - 1 vault).
    pub fn at_round(k: usize, r: i64) -> Self {
        Self::at_round_with(k, r, &Templates::get(b_max()))
    }

    pub fn at_round_with(k: usize, r: i64, t: &Templates) -> Self {
        assert!(r >= 1 && (1..=K_MAX).contains(&k));
        let base = CloseCase::new_with(k, t).controller;
        let controller = ControllerState {
            round: r + 1,
            deadline: TEST_DEADLINE + r * TEST_ROUND_LEN,
            jackpot: 0,
            j_boost: 0,
            g2: 0,
            liability: 0,
            v_rec: TEST_VAULT,
            acc: 2 * KAS as i64,
            drawn: 0,
            ..base
        };
        let m = 10;
        let trees: Vec<Tree> = (0..k).map(|_| Tree::new(m as usize)).collect();
        let marker_daa = TEST_MARKER_DAA + (r - 1) * 1_000_000;
        let d = anchor_daa(marker_daa, controller.seed_offset);
        let mut marker = MarkerState {
            round: r,
            k: k as i64,
            roots: vec![],
            sales: 0,
            ref_paid: 0,
            m,
            jackpot: 1_000_000 * KAS as i64,
            j_boost: 50_000 * KAS as i64,
            g2: 10_000 * KAS as i64,
            s_f: controller.s_f,
            s_r: controller.s_r,
            s_c: controller.s_c,
            s_d: controller.s_d,
            st: controller.st,
            funder_class: 0,
            funder_key: [0; 33],
        };
        marker.set_funder(&Payout::Schnorr(xonly(&FUNDING_SK)));
        let vault = VaultState {
            round: r - 1,
            total_assets: TEST_VAULT,
            // D66: S0 = floor(V0 / SHARE_UNIT), as genesis (S1-10, S2-3).
            total_shares: TEST_VAULT / crate::game::SHARE_UNIT,
            pend_dep: 0,
            pend_wsh: 0,
            liability: 1_040_000 * KAS as i64,
            record_tpl: t.record.hash,
            request_tpl: t.request.hash,
            position_tpl: t.position.hash,
        };
        let drawer = DrawerState {
            round: r,
            pool_tpl: t.pool.hash,
            vault_tpl: t.vault.hash,
            ticket_tpl: t.ticket.hash,
            f_min: F_MIN,
            claim_window: CLAIM_WINDOW,
            draw_window: DRAW_WINDOW,
        };
        let close_daa = marker_daa as u64;
        let mut c = Self {
            b_max: t.b_max,
            drawer,
            drawer_daa: close_daa,
            marker,
            marker_daa: close_daa,
            vault,
            vault_daa: 200,
            controller,
            pot: 100_000 * KAS,
            controller_daa: close_daa,
            trees,
            absorbed: vec![],
            // A fresh funding input (its creation DAA score no longer matters: D_claim is a
            // constant of the round, spec M7 D98).
            funding: vec![FundingIn { value: 100 * KAS, daa: (d + DRAW_WINDOW + 10_000) as u64 }],
            x_hash: TEST_X_HASH,
            sim: None,
            void: false,
            funder: Payout::Schnorr(xonly(&FUNDING_SK)),
            fee: KAS / 2,
            lock_time: d as u64,
            lie: Lie::default(),
            cov: COV,
            funding_sk: FUNDING_SK,
        };
        c.sync_roots();
        c
    }

    /// As `new(k)`, a void (spec M7 D96): empty anchor, lock_time = D + draw_window.
    pub fn new_void(k: usize) -> Self {
        let mut c = Self::new(k);
        c.set_void(true);
        c
    }

    /// Switch between draw and void mode and re-fix the lock time.
    pub fn set_void(&mut self, void: bool) {
        self.void = void;
        self.fix_lock_time();
    }

    /// D = marker creation DAA score + seed_offset (spec M7 D93, check 7), from the true
    /// marker_daa and the round r + 1 controller's seed_offset.
    pub fn d(&self) -> i64 {
        anchor_daa(self.marker_daa as i64, self.controller.seed_offset)
    }

    /// The chain the case runs against: `sim`, or `ChainSim::anchored(D, x_hash)`.
    pub fn chain(&self) -> Arc<ChainSim> {
        match &self.sim {
            Some(s) => s.clone(),
            None => {
                assert!(self.x_hash != [0; 32], "DrawCase: x_hash must be non-zero (ChainSim marks roots with a zero parent hash)");
                Arc::new(ChainSim::anchored(self.d() as u64, Hash::from_bytes(self.x_hash)))
            }
        }
    }

    /// The anchor argument: `lie.anchor`, else empty in a void, else the chain's proof for D
    /// (spec M7 A3).
    pub fn anchor_bytes(&self) -> Vec<u8> {
        if let Some(a) = &self.lie.anchor {
            return a.clone();
        }
        if self.void {
            return vec![];
        }
        AnchorProof::from_sim(&self.chain(), self.d() as u64).to_bytes()
    }

    /// The seed source X.hash of this case (D94): the claimed X.hash of `lie.anchor` (its
    /// first 32 bytes), else the given sim's anchor for D, else `x_hash`.
    pub fn x_hash(&self) -> [u8; 32] {
        if let Some(a) = self.lie.anchor.as_ref().filter(|a| a.len() >= 32) {
            return a[..32].try_into().unwrap();
        }
        match &self.sim {
            Some(s) => s.anchor_for(self.d() as u64).0.hash.as_bytes(),
            None => self.x_hash,
        }
    }

    pub fn k(&self) -> usize {
        self.trees.len()
    }

    pub fn a(&self) -> usize {
        self.absorbed.len()
    }

    // Input indices (A4 shape).
    pub const DRAWER: usize = 0;
    pub const MARKER: usize = 1;
    pub const VAULT: usize = 2;
    pub const CONTROLLER: usize = 3;
    pub fn pool_input(i: usize) -> usize {
        4 + i
    }
    pub fn funding_input(&self, n: usize) -> usize {
        4 + self.a() + n
    }

    /// marker.roots = the trees' roots, zero-padded to K_MAX.
    pub fn sync_roots(&mut self) {
        let roots: Vec<[u8; 32]> = self.trees.iter().map(|t| t.root()).collect();
        self.marker.roots = MarkerState::roots_bytes(&roots);
    }

    /// The seed mapping of this draw (D42, spec M7 D94): the instance ID, the marker's round,
    /// X.hash and m. Not used by a void.
    pub fn seed(&self) -> SeedNumbers {
        seed_numbers(&self.cov.as_bytes(), self.marker.round, &self.x_hash(), self.marker.m)
    }

    /// Add `n` holdings of (rank, bonus) to shard `i`'s tree and resync the marker roots.
    pub fn hold_at(&mut self, i: usize, rank: u32, bonus: u8, n: u16) {
        for _ in 0..n {
            self.trees[i].increment(rank, bonus);
        }
        self.sync_roots();
    }

    /// Add `n` holdings of the winning main numbers with bonus `bonus` to shard `i`.
    pub fn hold(&mut self, i: usize, bonus: u8, n: u16) {
        let rank = self.seed().rank;
        self.hold_at(i, rank, bonus, n);
    }

    /// Add `n` jackpot holdings (winning main numbers and bonus) to shard `i`.
    pub fn hold_jackpot(&mut self, i: usize, n: u16) {
        let b = self.seed().bonus;
        self.hold(i, b, n);
    }

    /// A bonus value other than the winning one (second prize).
    pub fn other_bonus(&self, nth: u8) -> u8 {
        let b = self.seed().bonus;
        let others: Vec<u8> = (1..=self.marker.m as u8).filter(|&x| x != b).collect();
        others[nth as usize]
    }

    /// Set round r's sales and referral total (the marker's S and P_R).
    pub fn set_sales(&mut self, sales: i64, ref_paid: i64) {
        self.marker.sales = sales;
        self.marker.ref_paid = ref_paid;
    }

    /// Absorb `n` more expired pools of round r - 1 (lower tiers, remaining 3 + i KAS,
    /// deadline 100,000 DAA before D, distinct funders). Requires r >= 2.
    pub fn absorb(&mut self, n: usize) {
        assert!(self.marker.round >= 2, "absorbed pools need an earlier round");
        for _ in 0..n {
            let i = self.absorbed.len();
            let mut state = PoolState {
                round: self.marker.round - 1,
                tier: TIER_LOWER0 + (i % 5) as u8,
                slice: 1_000 + i as i64,
                win: [1, 2, 3, 4, 5, 1],
                m: 10,
                k: self.k() as i64,
                roots: vec![0x21; 32 * K_MAX],
                deadline: self.d() - 100_000,
                ticket_tpl: self.drawer.ticket_tpl,
                reserve: self.controller.reserve,
                funder_class: 0,
                funder_key: [0; 33],
            };
            state.set_funder(&Payout::Schnorr([0x60 + i as u8; 32]));
            self.absorbed.push(PoolIn { state, remaining: (3 + i as u64) * KAS, daa: 500_000 });
        }
        self.fix_lock_time();
    }

    /// lock_time = max(D, absorbed deadlines) for a draw, max(D + draw_window, absorbed
    /// deadlines) for a void (D51, spec M7 check 7).
    pub fn fix_lock_time(&mut self) {
        let d = self.d();
        let mut l = if self.void { void_lock(d, self.drawer.draw_window).expect("D + draw_window < DAA_MAX") } else { d };
        for p in &self.absorbed {
            l = l.max(p.state.deadline);
        }
        self.lock_time = l as u64;
    }

    pub fn reserve(&self) -> u64 {
        self.controller.reserve as u64
    }

    /// S - P_R - C_close + J_boost + R (i128: attack cases use out-of-range values).
    pub fn marker_value(&self) -> u64 {
        let mk = &self.marker;
        let c_close = if (0..=crate::game::TOTAL_MAX).contains(&mk.sales) && (0..=10_000).contains(&mk.s_c) { muldiv(mk.sales, mk.s_c) } else { 0 };
        (mk.sales as i128 - mk.ref_paid as i128 - c_close as i128 + mk.j_boost as i128 + self.reserve() as i128) as u64
    }

    pub fn vault_value(&self) -> u64 {
        (self.vault.total_assets as i128 + self.vault.pend_dep as i128 + self.reserve() as i128) as u64
    }

    /// O + A + R.
    pub fn controller_value(&self) -> u64 {
        self.pot + self.controller.acc as u64 + self.reserve()
    }

    pub fn pool_value(&self, i: usize) -> u64 {
        self.absorbed[i].remaining + self.reserve()
    }

    pub fn funding_total(&self) -> u64 {
        self.funding.iter().map(|f| f.value).sum()
    }

    /// Every successor state and output value of the draw (A4 checks 8-25), from `game`.
    /// Panics where the reference rejects (V' < 0, a guard of A6, D_claim >= DAA_MAX).
    ///
    /// Spec M7 void (D96, D105): win5 = 0 and bonus = 0 (unless a `Lie` replaces them), no
    /// leaves or siblings, h_J = n2 = 0, every lp_t = 0 (np = 0), V' = total_assets (H not
    /// paid to the vault), O' = O + b_r + J_boost + sum B_t + sum Q + H; fee, C_draw, the
    /// conversion and J(r+1) as in a draw. The caller output is the remainder, so the M3 A4
    /// identity holds with H moved from V' to O'.
    pub fn expected(&self) -> DrawExpected {
        let mk = &self.marker;
        let ctrl = &self.controller;
        let void = self.void;
        // A void has no seed (D96); its `seed` field is the mapping of the x_hash field only.
        let seed = if void { seed_numbers(&self.cov.as_bytes(), mk.round, &self.x_hash, mk.m) } else { self.seed() };
        let amounts = draw_amounts(mk);
        let j_house = mk.jackpot - mk.j_boost;
        let q: Vec<i64> = self.absorbed.iter().map(|p| p.remaining as i64).collect();
        let (rank, bonus, win5, leaves, siblings, counts, h_j, n2, pools, v_prime, o_prime);
        if void {
            rank = self.lie.rank.unwrap_or(0);
            bonus = self.lie.bonus.unwrap_or(0);
            win5 = self.lie.win5.unwrap_or([0; 5]);
            (leaves, siblings) = (vec![], vec![]);
            counts = vec![];
            (h_j, n2) = self.lie.counts.unwrap_or((0, 0));
            pools = void_pool_set(&amounts.b, mk.m);
            v_prime = vault_settle(self.vault.total_assets, 0, j_house, false, mk.g2, 0, false).expect("V' >= 0 (check 14)") + self.lie.v_delta;
            o_prime = boost_after(self.pot as i64, amounts.b_r, mk.j_boost, false, &amounts.b, &pools.lp, &q)
                .and_then(|o| guarded_add(o, amounts.h))
                .expect("O' guards (check 15)")
                + self.lie.o_delta;
        } else {
            rank = self.lie.rank.unwrap_or(seed.rank);
            bonus = self.lie.bonus.unwrap_or(seed.bonus);
            win5 = self.lie.win5.unwrap_or(unrank(rank));
            (leaves, siblings) = proofs_at(&self.trees, rank);
            counts = self.trees.iter().map(|t| t.counts(rank)).collect::<Vec<Vec<u16>>>();
            (h_j, n2) = self.lie.counts.unwrap_or_else(|| win_counts(&counts, bonus, mk.m));
            pools = pool_set(h_j, n2, mk.jackpot, mk.g2, &amounts.b, mk.m);
            v_prime = vault_settle(self.vault.total_assets, amounts.h, j_house, pools.jp, mk.g2, n2, pools.sp).expect("V' >= 0 (check 14)")
                + self.lie.v_delta;
            o_prime = boost_after(self.pot as i64, amounts.b_r, mk.j_boost, pools.jp, &amounts.b, &pools.lp, &q).expect("O' guards (check 15)")
                + self.lie.o_delta;
        }
        let mut fee = fee_split(ctrl.acc, amounts.f, self.drawer.f_min).expect("T_F guard (check 16)");
        if let Some(p) = self.lie.paid {
            fee.paid = p;
            fee.a_next = if p { 0 } else { fee.t_f };
        }
        // M5 VA14: J may be fixed (falsely) from another V.
        let j_v = self.lie.j_v.unwrap_or(v_prime);
        let rule = JackpotRule::new(ctrl, j_v);
        let fix = match self.lie.witness {
            None => fix_jackpot(&rule, j_v, o_prime, ctrl.j),
            Some((x1, y, x2)) => {
                let jackpot = x2 + y;
                let g2 = muldiv(jackpot, rule.g);
                JackpotFix { x_b: muldiv(j_v, ctrl.j), x1, y, x2, jackpot, j_boost: y, g2, liability: x2 + g2 * (rule.m - 1) }
            }
        };
        let mk_daa = self.lie.mk_daa.unwrap_or(self.marker_daa as i64);
        let d = anchor_daa(mk_daa, ctrl.seed_offset);
        let d_claim = claim_deadline(d, self.drawer.draw_window, self.drawer.claim_window).expect("D_claim < DAA_MAX (check 19)");

        let drawer = DrawerState { round: self.drawer.round + 1, ..self.drawer.clone() };
        // Spec M5 A5 C1-C3 (D65, D69, D71): convert the pending totals at V'.
        let conv = self.conversion(v_prime, fix.liability);
        let rec = self.lie.record.unwrap_or(conv.rec());
        let total_assets = v_prime + conv.d_c - conv.out_c;
        let vault = VaultState {
            round: mk.round,
            total_assets,
            total_shares: self.vault.total_shares + conv.mint_c - conv.wc,
            pend_dep: 0,
            pend_wsh: self.vault.pend_wsh - conv.wc,
            liability: fix.liability,
            ..self.vault.clone()
        };
        let record = rec.then(|| RecordState {
            round: mk.round,
            request_tpl: self.vault.request_tpl,
            refund: conv.refund as u8,
            dep_rem: conv.d,
            mint_rem: conv.mint_c,
            wsh_rem: conv.w,
            burn_rem: conv.wc,
            out_rem: conv.out_c,
            funder_class: self.funder.class(),
            funder_key: self.funder.key33(),
        });
        let controller = ControllerState {
            jackpot: fix.jackpot,
            j_boost: fix.j_boost,
            g2: fix.g2,
            liability: fix.liability,
            // D71: V_rec = total_assets after the conversions.
            v_rec: total_assets,
            acc: fee.a_next,
            drawn: 1,
            ..ctrl.clone()
        };
        let win = [win5[0], win5[1], win5[2], win5[3], win5[4], bonus];
        let new_pools: Vec<PoolState> = pools
            .pools
            .iter()
            .map(|p| {
                let mut s = PoolState {
                    round: mk.round,
                    tier: p.tier,
                    slice: p.slice,
                    win,
                    m: mk.m,
                    k: mk.k,
                    roots: mk.roots.clone(),
                    deadline: d_claim,
                    // Spec M4 A7 check 25 (D54, D55).
                    ticket_tpl: self.drawer.ticket_tpl,
                    reserve: ctrl.reserve,
                    funder_class: 0,
                    funder_key: [0; 33],
                };
                s.set_funder(&self.funder);
                s
            })
            .collect();

        let r = self.reserve();
        // vault' = total_assets' + R (pend_dep' = 0, spec M5 A5 check 20).
        let mut values = vec![r, (total_assets as i128 + r as i128) as u64, (o_prime + fee.a_next) as u64 + r];
        values.extend(pools.pools.iter().map(|p| p.value as u64 + r));
        if rec {
            // The record: out_c + (D - D_c) + R (C3).
            values.push((conv.out_c as i128 + (conv.d - conv.d_c) as i128 + r as i128) as u64);
        }
        if fee.paid {
            values.push(fee.t_f as u64);
        }
        values.push(r);
        values.extend(std::iter::repeat_n(r, self.a()));
        let total_in: i128 = r as i128
            + self.marker_value() as i128
            + self.vault_value() as i128
            + self.controller_value() as i128
            + (0..self.a()).map(|i| self.pool_value(i) as i128).sum::<i128>()
            + self.funding_total() as i128;
        let caller = total_in - values.iter().map(|&v| v as i128).sum::<i128>() - self.fee as i128;
        assert!(caller > 0, "funding does not cover the draw: caller output {caller}");
        values.push(caller as u64);

        DrawExpected {
            seed,
            rank,
            bonus,
            win5,
            leaves,
            siblings,
            counts,
            h_j,
            n2,
            amounts,
            j_house,
            pools,
            v_prime,
            q,
            o_prime,
            fee,
            rule,
            fix,
            mk_daa,
            d,
            d_claim,
            void,
            anchor: self.anchor_bytes(),
            drawer,
            vault,
            controller,
            new_pools,
            conv,
            rec,
            record,
            values,
            a: self.a(),
        }
    }

    /// The conversion of this draw's vault at V' with L' (spec M5 A5 C1-C2, `game::convert`),
    /// with the `Lie` applied. Outside the reference's domain (fabricated range attacks, D79)
    /// nothing converts (D_c = mint_c = wc = out_c = 0, refund = D > 0), so the builder still
    /// produces a transaction for the script to reject.
    pub fn conversion(&self, v_prime: i64, l_next: i64) -> Conversion {
        let vt = &self.vault;
        let v = self.lie.conv_v.unwrap_or(v_prime);
        let (s, d, w) = (vt.total_shares, vt.pend_dep, vt.pend_wsh);
        let mut c = convert(v, s, d, w, l_next).unwrap_or(Conversion {
            v_prime: v,
            s,
            d,
            w,
            l_next,
            mint: 0,
            ok: false,
            d_c: 0,
            mint_c: 0,
            refund: d > 0,
            slack: 0,
            wc: 0,
            out_c: 0,
        });
        let raw_out = |wc: i64| -> i64 {
            withdraw_out(v, s, wc).unwrap_or_else(|| ((v as i128 + VA as i128) * wc as i128 / (s as i128 + VS as i128).max(1)) as i64)
        };
        if let Some(wc) = self.lie.wc {
            c.wc = wc;
            c.out_c = raw_out(wc);
        }
        match self.lie.refund {
            Some(false) if d > 0 => {
                let mint = ((d as i128) * (s as i128 + VS as i128) / (v as i128 + VA as i128).max(1)).min(i64::MAX as i128) as i64;
                c.mint = mint;
                c.ok = true;
                c.d_c = d;
                c.mint_c = mint;
                c.refund = false;
            }
            Some(true) => {
                c.ok = false;
                c.d_c = 0;
                c.mint_c = 0;
                c.refund = d > 0;
            }
            _ => {}
        }
        c.out_c += self.lie.out_delta;
        c
    }

    /// Whether anything is pending in the input vault (spec M5 C3), before any `Lie`.
    pub fn pending(&self) -> bool {
        self.vault.pend_dep > 0 || self.vault.pend_wsh > 0
    }

    /// The caller output by the A4 formula: funding - np R + C_draw - fee.
    pub fn caller_formula(&self, x: &DrawExpected) -> i128 {
        self.funding_total() as i128 - (x.np() as u64 * self.reserve()) as i128 + x.amounts.c_draw as i128 - self.fee as i128
    }

    /// draw(anchor, win5, leaves, siblings, x1, y, x2, ctrl_pl, ctrl_sl, marker_pl, marker_sl,
    /// vault_pl, vault_sl, pool_pl, pool_sl, pool_prefix, pool_suffix, funder_class,
    /// funder_key, wc, record_prefix, record_suffix, mk_daa): the M7 signature (`SIG_DRAW`,
    /// 23 arguments; spec M7 A4 "Arguments").
    pub fn args(&self, t: &Templates, x: &DrawExpected) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        vec![
            Bytes(x.anchor.clone()),
            Bytes(x.win5.to_vec()),
            Bytes(x.leaves.clone()),
            Bytes(x.siblings.clone()),
            Int(x.fix.x1),
            Int(x.fix.y),
            Int(x.fix.x2),
            Int(t.controller.prefix.len() as i64),
            Int(t.controller.suffix.len() as i64),
            Int(t.marker.prefix.len() as i64),
            Int(t.marker.suffix.len() as i64),
            Int(t.vault.prefix.len() as i64),
            Int(t.vault.suffix.len() as i64),
            Int(t.pool.prefix.len() as i64),
            Int(t.pool.suffix.len() as i64),
            Bytes(t.pool.prefix.clone()),
            Bytes(t.pool.suffix.clone()),
            Byte(self.funder.class()),
            Bytes(self.funder.key33().to_vec()),
            Int(x.conv.wc),
            Bytes(t.record.prefix.clone()),
            Bytes(t.record.suffix.clone()),
            Int(x.mk_daa),
        ]
    }

    /// draw_leg(drawer_pl, drawer_sl) of the marker and the vault, absorb_leg of a pool.
    pub fn drawer_leg_args(t: &Templates) -> Vec<ArtifactValue> {
        vec![ArtifactValue::Int(t.drawer.prefix.len() as i64), ArtifactValue::Int(t.drawer.suffix.len() as i64)]
    }

    /// draw_leg(marker_pl, marker_sl) of the controller.
    pub fn controller_leg_args(t: &Templates) -> Vec<ArtifactValue> {
        vec![ArtifactValue::Int(t.marker.prefix.len() as i64), ArtifactValue::Int(t.marker.suffix.len() as i64)]
    }

    /// The leg arguments of input `i` (1 marker, 2 vault, 3 controller, >= 4 pool).
    pub fn leg_args(t: &Templates, i: usize) -> Vec<ArtifactValue> {
        if i == Self::CONTROLLER { Self::controller_leg_args(t) } else { Self::drawer_leg_args(t) }
    }

    pub fn build(&self) -> Built {
        self.build_with(Templates::get(self.b_max))
    }

    /// Build against given templates (mutants and look-alikes).
    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        let x = self.expected();
        let cov = Some(self.cov);
        let dleg = Self::drawer_leg_args(&t);
        let mut inputs = vec![
            In {
                entry: UtxoEntry::new(self.reserve(), t.drawer_spk(&self.drawer), self.drawer_daa, false, cov),
                kind: InKind::Entry {
                    template: t.drawer.clone(),
                    entry: SIG_DRAW.into(),
                    args: self.args(&t, &x),
                    redeem: t.drawer_redeem(&self.drawer),
                },
            },
            In {
                entry: UtxoEntry::new(self.marker_value(), t.marker_spk(&self.marker), self.marker_daa, false, cov),
                kind: InKind::Entry { template: t.marker.clone(), entry: "draw_leg".into(), args: dleg.clone(), redeem: t.marker_redeem(&self.marker) },
            },
            In {
                entry: UtxoEntry::new(self.vault_value(), t.vault_spk(&self.vault), self.vault_daa, false, cov),
                kind: InKind::Entry { template: t.vault.clone(), entry: "draw_leg".into(), args: dleg.clone(), redeem: t.vault_redeem(&self.vault) },
            },
            In {
                entry: UtxoEntry::new(self.controller_value(), t.controller_spk(&self.controller), self.controller_daa, false, cov),
                kind: InKind::Entry {
                    template: t.controller.clone(),
                    entry: "draw_leg".into(),
                    args: Self::controller_leg_args(&t),
                    redeem: t.controller_redeem(&self.controller),
                },
            },
        ];
        for (i, p) in self.absorbed.iter().enumerate() {
            inputs.push(In {
                entry: UtxoEntry::new(self.pool_value(i), t.pool_spk(&p.state), p.daa, false, cov),
                kind: InKind::Entry { template: t.pool.clone(), entry: "absorb_leg".into(), args: dleg.clone(), redeem: t.pool_redeem(&p.state) },
            });
        }
        for f in &self.funding {
            inputs.push(In { entry: UtxoEntry::new(f.value, spk_of(&self.funding_sk), f.daa, false, None), kind: InKind::Funding });
        }

        let bind = Some(CovenantBinding::new(0, self.cov));
        let mut outputs = vec![
            TransactionOutput::with_covenant(x.values[0], t.drawer_spk(&x.drawer), bind),
            TransactionOutput::with_covenant(x.values[1], t.vault_spk(&x.vault), bind),
            TransactionOutput::with_covenant(x.values[2], t.controller_spk(&x.controller), bind),
        ];
        for (p, s) in x.new_pools.iter().enumerate() {
            outputs.push(TransactionOutput::with_covenant(x.values[x.pool_output(p)], t.pool_spk(s), bind));
        }
        if let (Some(i), Some(rc)) = (x.record_output(), &x.record) {
            outputs.push(TransactionOutput::with_covenant(x.values[i], t.record_spk(rc), bind));
        }
        if let Some(i) = x.fee_output() {
            let fee_addr = Payout::from_fields(self.controller.fee_class, &self.controller.fee_key);
            outputs.push(TransactionOutput::new(x.values[i], fee_addr.spk()));
        }
        let mk_funder = Payout::from_fields(self.marker.funder_class, &self.marker.funder_key);
        outputs.push(TransactionOutput::new(x.values[x.marker_refund()], mk_funder.spk()));
        for (i, p) in self.absorbed.iter().enumerate() {
            let f = Payout::from_fields(p.state.funder_class, &p.state.funder_key);
            outputs.push(TransactionOutput::new(x.values[x.pool_refund(i)], f.spk()));
        }
        outputs.push(TransactionOutput::new(x.values[x.caller_output()], spk_of(&self.funding_sk)));

        let outpoints = (0..inputs.len() as u32).map(outpoint).collect();
        Built {
            templates: t,
            args: vec![],
            inputs,
            outputs,
            genesis: vec![],
            lock_time: self.lock_time,
            shard_budget: None,
            budget_delta: 0,
            outpoints,
            funding_sk: self.funding_sk,
            sim: Some(self.chain()),
        }
    }
}

/// One pool of a round result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultPool {
    pub tier: u8,
    pub slice: i64,
    /// Output value (remaining + R).
    pub value: u64,
    pub outpoint: TransactionOutpoint,
}

/// The published record of a draw (spec M3 A9, design 6, 9): enough to rebuild every new
/// pool's state (R4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundResult {
    pub round: i64,
    pub win: [u8; 6],
    pub m: i64,
    pub k: i64,
    pub roots: Vec<[u8; 32]>,
    pub pools: Vec<ResultPool>,
    pub d_claim: i64,
    pub funder: Payout,
    /// The ticket template hash from the drawer input state (spec M4 A8, D54).
    pub ticket_tpl: [u8; 32],
    /// R: the drawer' value, output 0 (spec M4 A8, D55).
    pub reserve: u64,
    /// Spec M5 A5 (R4 for the record): the settlement record written by this draw, if
    /// anything was pending.
    pub record: Option<ResultRecord>,
    /// Spec M7 A3 (round result record): the anchor block's hash (zero in a void), the
    /// marker's creation DAA score (the mk_daa argument; D = mk_daa + seed_offset), and
    /// whether the round was voided (D96).
    pub x_hash: [u8; 32],
    pub mk_daa: i64,
    pub void: bool,
}

/// The settlement record of a round result (spec M5 A3.3, A5 C3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultRecord {
    pub state: RecordState,
    /// out_c + (D - D_c) + R.
    pub value: u64,
    pub outpoint: TransactionOutpoint,
}

impl RoundResult {
    /// The record of a draw built from `case`, whose transaction has id `txid`.
    pub fn new(case: &DrawCase, x: &DrawExpected, txid: TransactionId) -> Self {
        let mk = &case.marker;
        let roots = (0..mk.k as usize).map(|i| mk.roots[32 * i..32 * (i + 1)].try_into().unwrap()).collect();
        let pools = x
            .pools
            .pools
            .iter()
            .enumerate()
            .map(|(p, np)| ResultPool {
                tier: np.tier,
                slice: np.slice,
                value: x.values[x.pool_output(p)],
                outpoint: TransactionOutpoint::new(txid, x.pool_output(p) as u32),
            })
            .collect();
        Self {
            round: mk.round,
            win: [x.win5[0], x.win5[1], x.win5[2], x.win5[3], x.win5[4], x.bonus],
            m: mk.m,
            k: mk.k,
            roots,
            pools,
            d_claim: x.d_claim,
            funder: case.funder,
            ticket_tpl: case.drawer.ticket_tpl,
            reserve: x.values[0],
            record: match (x.record_output(), &x.record) {
                (Some(i), Some(s)) => Some(ResultRecord { state: s.clone(), value: x.values[i], outpoint: TransactionOutpoint::new(txid, i as u32) }),
                _ => None,
            },
            x_hash: if x.void { [0; 32] } else { case.x_hash() },
            mk_daa: x.mk_daa,
            void: x.void,
        }
    }

    /// The record rebuilt from the `draw()` transaction bytes alone (spec A9), without the
    /// builder's data or the spent UTXO entries:
    ///   - cov from output 0's covenant binding;
    ///   - round, m, k, roots and the shares from the marker's redeem (input 1);
    ///   - draw_window, claim_window and ticket_tpl from the drawer's redeem (input 0);
    ///   - R (the pools' reserve) from output 0's value (drawer' = R, check 20);
    ///   - the anchor (empty = void, spec M7 D96), win5, the count leaves and the funder from
    ///     input 0's arguments; X.hash = the anchor's bytes 0..32 (D94);
    ///   - bonus from the seed mapping (D42); h_J and n2 from the leaves; the pool set (D45)
    ///     and slices from the marker; values and outpoints from the outputs; in a void no
    ///     pools (win5 = 0, no leaves);
    ///   - D_claim = D + draw_window + claim_window (spec M7 D98) with D = mk_daa +
    ///     seed_offset: mk_daa from its argument, which check 7 requires to equal the
    ///     marker's creation DAA score, seed_offset from the controller's redeem (input 3).
    ///   - spec M5: the record (if anything was pending) from the vault's redeem (input 2:
    ///     S, D, W, request_tpl), V' (as check 14), L' (from x2, y and the controller's g
    ///     and m, input 3's redeem), the wc argument and the funder; checked against the
    ///     record output's script and value at 3 + np.
    /// `t` supplies the committed marker, drawer, vault, controller and record layouts.
    /// Returns an error on any inconsistency (wrong shape, pool count or value). Only the M7
    /// entry (23 arguments) is read.
    pub fn from_tx(tx: &Transaction, t: &Templates) -> Result<Self, String> {
        let cov = tx.outputs.first().and_then(|o| o.covenant).ok_or("output 0 has no covenant binding")?.covenant_id;
        let drawer_pushes = pushes(&tx.inputs.first().ok_or("no input 0")?.signature_script)?;
        let marker_pushes = pushes(&tx.inputs.get(1).ok_or("no input 1")?.signature_script)?;
        let vault_pushes = pushes(&tx.inputs.get(2).ok_or("no input 2")?.signature_script)?;
        let ctrl_pushes = pushes(&tx.inputs.get(3).ok_or("no input 3")?.signature_script)?;
        // draw has 23 arguments (M7), then the dispatch tag and the redeem script.
        if drawer_pushes.len() != DRAW_ARGS + 2 {
            return Err(format!("input 0: {} pushes, expected {} (M7)", drawer_pushes.len(), DRAW_ARGS + 2));
        }
        let drawer = state_values(&t.drawer, drawer_pushes.last().unwrap())?;
        let mk = state_values(&t.marker, marker_pushes.last().ok_or("input 1: empty sigscript")?)?;
        let int = |v: &std::collections::BTreeMap<String, ArtifactValue>, k: &str| -> Result<i64, String> {
            match v.get(k) {
                Some(ArtifactValue::Int(x)) => Ok(*x),
                other => Err(format!("state field {k}: {other:?}")),
            }
        };
        let round = int(&mk, "round")?;
        let m = int(&mk, "m")?;
        let k = int(&mk, "k")?;
        if !(2..=crate::game::M_DRAW).contains(&m) || !(1..=K_MAX as i64).contains(&k) {
            return Err(format!("marker m {m} or k {k} out of range"));
        }
        let roots_bytes = match mk.get("roots") {
            Some(ArtifactValue::Bytes(b)) if b.len() == 32 * K_MAX => b.clone(),
            other => return Err(format!("marker roots: {other:?}")),
        };
        let marker = MarkerState {
            round,
            k,
            roots: roots_bytes.clone(),
            sales: int(&mk, "sales")?,
            ref_paid: int(&mk, "ref_paid")?,
            m,
            jackpot: int(&mk, "jackpot")?,
            j_boost: int(&mk, "j_boost")?,
            g2: int(&mk, "g2")?,
            s_f: int(&mk, "s_f")?,
            s_r: int(&mk, "s_r")?,
            s_c: int(&mk, "s_c")?,
            s_d: int(&mk, "s_d")?,
            st: [int(&mk, "st1")?, int(&mk, "st2")?, int(&mk, "st3")?, int(&mk, "st4")?, int(&mk, "st5")?],
            funder_class: 0,
            funder_key: [0; 33],
        };
        let arg = |i: usize, len: usize| -> Result<&Vec<u8>, String> {
            let a = &drawer_pushes[i];
            if a.len() == len { Ok(a) } else { Err(format!("argument {i}: {} bytes, expected {len}", a.len())) }
        };
        let anchor = &drawer_pushes[ARG_ANCHOR];
        let void = anchor.is_empty();
        let x_hash = if void { [0; 32] } else { AnchorProof::from_bytes(anchor)?.x_hash() };
        let win5: [u8; 5] = arg(ARG_WIN5, 5)?.as_slice().try_into().unwrap();
        let leaves = arg(ARG_LEAVES, if void { 0 } else { 2 * m as usize * k as usize })?;
        if void && !drawer_pushes[ARG_SIBLINGS].is_empty() {
            return Err("void with siblings".into());
        }
        let class = arg(ARG_FUNDER_CLASS, 1)?[0];
        let key: [u8; 33] = arg(ARG_FUNDER_KEY, 33)?.as_slice().try_into().unwrap();
        if class > 2 {
            return Err(format!("funder class {class}"));
        }
        let funder = Payout::from_fields(class, &key);

        let amounts = draw_amounts(&marker);
        let (bonus, n2, set) = if void {
            if win5 != [0; 5] {
                return Err("void with non-zero win5".into());
            }
            (0, 0, void_pool_set(&amounts.b, m))
        } else {
            let sn = seed_numbers(&cov.as_bytes(), round, &x_hash, m);
            if win5 != sn.main {
                return Err("win5 is not the seed's main numbers".into());
            }
            let counts: Vec<Vec<u16>> =
                leaves.chunks(2 * m as usize).map(|c| c.chunks(2).map(|x| u16::from_le_bytes([x[0], x[1]])).collect()).collect();
            let (h_j, n2) = win_counts(&counts, sn.bonus, m);
            (sn.bonus, n2, pool_set(h_j, n2, marker.jackpot, marker.g2, &amounts.b, m))
        };
        let vt = state_values(&t.vault, vault_pushes.last().ok_or("input 2: empty sigscript")?)?;
        let ctrl = state_values(&t.controller, ctrl_pushes.last().ok_or("input 3: empty sigscript")?)?;
        let (s_tot, d_pend, w_pend) = (int(&vt, "total_shares")?, int(&vt, "pend_dep")?, int(&vt, "pend_wsh")?);
        let rec = d_pend > 0 || w_pend > 0;
        let n_cov = tx.outputs.iter().filter(|o| o.covenant.is_some()).count();
        if n_cov != 3 + set.np() + rec as usize {
            return Err(format!("{n_cov} covenant outputs, expected 3 + {} + {}", set.np(), rec as usize));
        }
        let txid = tx.id();
        // R = drawer' value (check 20).
        let r = tx.outputs[0].value;
        let mut pools = vec![];
        for (p, np) in set.pools.iter().enumerate() {
            let value = tx.outputs[3 + p].value;
            if value != np.value as u64 + r {
                return Err(format!("pool {p} value {value}"));
            }
            pools.push(ResultPool { tier: np.tier, slice: np.slice, value, outpoint: TransactionOutpoint::new(txid, 3 + p as u32) });
        }
        let mk_daa = script_num(&drawer_pushes[ARG_MK_DAA])?;
        let d = anchor_daa(mk_daa, int(&ctrl, "seed_offset")?);
        let d_claim = claim_deadline(d, int(&drawer, "draw_window")?, int(&drawer, "claim_window")?).ok_or("D_claim out of range")?;
        let roots = (0..k as usize).map(|i| roots_bytes[32 * i..32 * (i + 1)].try_into().unwrap()).collect();
        let bytes32 = |v: &std::collections::BTreeMap<String, ArtifactValue>, f: &str| -> Result<[u8; 32], String> {
            match v.get(f) {
                Some(ArtifactValue::Bytes(b)) if b.len() == 32 => Ok(b.as_slice().try_into().unwrap()),
                other => Err(format!("state field {f}: {other:?}")),
            }
        };
        let ticket_tpl = bytes32(&drawer, "ticket_tpl")?;
        // Spec M5 A5 C1-C3: the record, rebuilt from the transaction bytes.
        let record = if rec {
            let wc = script_num(&drawer_pushes[ARG_WC])?;
            let (x2, y) = (script_num(&drawer_pushes[ARG_X2])?, script_num(&drawer_pushes[ARG_Y])?);
            let (g, cm) = (int(&ctrl, "g")?, int(&ctrl, "m")?);
            let l_next = x2 + muldiv(x2 + y, g) * (cm - 1);
            // Spec M7 D96: a void pays H to the boost pot, not the vault.
            let h = if void { 0 } else { amounts.h };
            let v_prime = vault_settle(int(&vt, "total_assets")?, h, marker.jackpot - marker.j_boost, set.jp, marker.g2, n2, set.sp).ok_or("V' < 0")?;
            let conv = convert(v_prime, s_tot, d_pend, w_pend, l_next).ok_or("the conversion is outside its domain")?;
            if conv.wc != wc {
                return Err(format!("wc argument {wc} is not the largest fitting share count {}", conv.wc));
            }
            let state = conv.record(round, bytes32(&vt, "request_tpl")?, &funder).expect("rec");
            let i = 3 + set.np();
            let value = conv.record_value(r as i64) as u64;
            if tx.outputs[i].value != value || tx.outputs[i].script_public_key != t.record_spk(&state) {
                return Err(format!("record output {i} does not match the conversion"));
            }
            Some(ResultRecord { state, value, outpoint: TransactionOutpoint::new(txid, i as u32) })
        } else {
            None
        };
        Ok(Self {
            round,
            win: [win5[0], win5[1], win5[2], win5[3], win5[4], bonus],
            m,
            k,
            roots,
            pools,
            d_claim,
            funder,
            ticket_tpl,
            reserve: r,
            record,
            x_hash,
            mk_daa,
            void,
        })
    }

    /// Pool p's state rebuilt from the record alone (R4).
    pub fn pool_state(&self, p: usize) -> PoolState {
        let mut s = PoolState {
            round: self.round,
            tier: self.pools[p].tier,
            slice: self.pools[p].slice,
            win: self.win,
            m: self.m,
            k: self.k,
            roots: MarkerState::roots_bytes(&self.roots),
            deadline: self.d_claim,
            ticket_tpl: self.ticket_tpl,
            reserve: self.reserve as i64,
            funder_class: 0,
            funder_key: [0; 33],
        };
        s.set_funder(&self.funder);
        s
    }
}

/// The data pushes of a push-only script (a signature script), in order: OP_0 -> empty,
/// OP_DATA_1..75 / OP_PUSHDATA1/2/4 -> the data, OP_1NEGATE -> [0x81], OP_1..OP_16 -> [n]
/// (the canonical forms `ScriptBuilder::add_data` emits, rusty-kaspa v2.1.0
/// `crypto/txscript/src/script_builder.rs:126-212`).
fn pushes(script: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut out = vec![];
    let mut i = 0;
    let take = |i: &mut usize, n: usize| -> Result<Vec<u8>, String> {
        let end = i.checked_add(n).filter(|&e| e <= script.len()).ok_or("push past the end of the script")?;
        let v = script[*i..end].to_vec();
        *i = end;
        Ok(v)
    };
    while i < script.len() {
        let op = script[i];
        i += 1;
        let data = match op {
            0x00 => vec![],
            0x01..=0x4b => take(&mut i, op as usize)?,
            0x4c => {
                let n = take(&mut i, 1)?[0] as usize;
                take(&mut i, n)?
            }
            0x4d => {
                let b = take(&mut i, 2)?;
                take(&mut i, u16::from_le_bytes([b[0], b[1]]) as usize)?
            }
            0x4e => {
                let b = take(&mut i, 4)?;
                take(&mut i, u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)?
            }
            0x4f => vec![0x81],
            0x51..=0x60 => vec![op - 0x50],
            _ => return Err(format!("opcode {op:#04x} in a push-only script")),
        };
        out.push(data);
    }
    Ok(out)
}

/// The state field values of `redeem` under template `t` (prefix and suffix must match).
fn state_values(t: &Template, redeem: &[u8]) -> Result<std::collections::BTreeMap<String, ArtifactValue>, String> {
    if redeem.len() < t.prefix.len() + t.suffix.len() || !redeem.starts_with(&t.prefix) || !redeem.ends_with(&t.suffix) {
        return Err(format!("redeem script is not a {} instance", t.name()));
    }
    let state = &redeem[t.prefix.len()..redeem.len() - t.suffix.len()];
    let c = t.art.contracts.values().next().unwrap();
    silverscript_abi::decode_runtime_state_script(&t.art, &c.runtime_state, state).map_err(|e| format!("{} state: {e}", t.name()))
}

/// A script number push (little-endian sign-magnitude, at most 8 bytes; empty = 0), as
/// `ScriptBuilder::add_i64` encodes an int argument.
fn script_num(b: &[u8]) -> Result<i64, String> {
    if b.len() > 8 {
        return Err(format!("script number of {} bytes", b.len()));
    }
    let Some((&last, _)) = b.split_last() else { return Ok(0) };
    let mut mag: u64 = 0;
    for (i, &x) in b.iter().enumerate() {
        let x = if i == b.len() - 1 { x & 0x7f } else { x };
        mag |= (x as u64) << (8 * i);
    }
    let v = i64::try_from(mag).map_err(|_| "script number out of range".to_string())?;
    Ok(if last & 0x80 != 0 { -v } else { v })
}


#[cfg(test)]
mod tests {
    use super::*;

    /// The M3 A4 identity (excluding R) with spec M7 D105: marker (S - P_R - C_close +
    /// J_boost) + V + (O + A) + sum Q = pools + V' + (O' + A') + fee + C_draw.
    fn conserves(c: &DrawCase, x: &DrawExpected) {
        let mk = &c.marker;
        let lhs = (mk.sales - mk.ref_paid - x.amounts.c_close + mk.j_boost) as i128
            + c.vault.total_assets as i128
            + (c.pot as i128 + c.controller.acc as i128)
            + x.q.iter().map(|&q| q as i128).sum::<i128>();
        let pools: i128 = x.pools.pools.iter().map(|p| p.value as i128).sum();
        let fee = if x.fee.paid { x.fee.t_f as i128 } else { 0 };
        let rhs = pools + x.v_prime as i128 + (x.o_prime + x.fee.a_next) as i128 + fee + x.amounts.c_draw as i128;
        assert_eq!(lhs, rhs, "D105 conservation (void = {})", c.void);
    }

    /// Spec M7 (Rust side, no script): draw and void cases of the m3 attack base shape, with
    /// and without a pending deposit (record), conserve value (D105), carry the M7 fields
    /// (anchor, mk_daa, D, D_claim = D + draw_window + claim_window, the lock), and the round
    /// result rebuilt from the transaction bytes alone equals the builder's (M3 A9, M7 A3).
    #[test]
    fn m7_draw_and_void_expected_and_round_result() {
        let t = Templates::get(b_max());
        for void in [false, true] {
            for pending in [0, 100 * KAS as i64] {
                let mut c = DrawCase::at_round(4, 2);
                c.set_sales(157 * KAS as i64, 5 * KAS as i64);
                c.hold_jackpot(1, 1);
                let b = c.other_bonus(0);
                c.hold(2, b, 2);
                c.absorb(1);
                c.vault.pend_dep = pending;
                c.set_void(void);
                let d = c.d();
                assert_eq!(d, TEST_MARKER_DAA + 1_000_000 + SEED_OFFSET_FIXTURE);
                let x = c.expected();
                conserves(&c, &x);
                assert_eq!((x.void, x.mk_daa, x.d), (void, c.marker_daa as i64, d));
                assert_eq!(x.d_claim, d + c.drawer.draw_window + c.drawer.claim_window);
                assert_eq!(x.rec, pending > 0);
                if void {
                    assert_eq!(c.lock_time as i64, d + c.drawer.draw_window);
                    assert!(x.anchor.is_empty() && x.leaves.is_empty() && x.siblings.is_empty());
                    assert_eq!((x.win5, x.bonus, x.h_j, x.n2, x.np()), ([0; 5], 0, 0, 0, 0));
                    assert_eq!(x.v_prime, c.vault.total_assets, "void: H is not paid to the vault");
                    let lower: i64 = x.amounts.b.iter().sum();
                    let q: i64 = x.q.iter().sum();
                    assert_eq!(x.o_prime, c.pot as i64 + x.amounts.b_r + c.marker.j_boost + lower + q + x.amounts.h);
                } else {
                    assert_eq!(c.lock_time as i64, d);
                    assert_eq!(x.anchor.len(), harness::ANCHOR_LEN);
                    let a = AnchorProof::from_bytes(&x.anchor).unwrap();
                    assert_eq!(a.x_hash(), TEST_X_HASH);
                    assert!(a.consistent_with(d));
                    assert_eq!((x.h_j, x.n2), (1, 1));
                    assert!(x.np() > 0);
                }
                let built = c.build();
                assert!(built.sim.is_some());
                let f = built.finish();
                assert!(f.sim.is_some());
                let rr = RoundResult::from_tx(&f.tx, &t).unwrap_or_else(|e| panic!("void = {void}, pending = {pending}: {e}"));
                assert_eq!(rr, RoundResult::new(&c, &x, f.tx.id()));
                assert_eq!((rr.void, rr.mk_daa, rr.x_hash == TEST_X_HASH), (void, c.marker_daa as i64, !void));
                for p in 0..x.np() {
                    assert_eq!(t.pool_spk(&rr.pool_state(p)), f.tx.outputs[3 + p].script_public_key);
                }
            }
        }
    }

    const SEED_OFFSET_FIXTURE: i64 = crate::game::SEED_OFFSET;

    /// A false mk_daa claim moves D and D_claim with it (SA5's consistent lie); a given sim
    /// supplies X.hash; an explicit anchor lie supplies the claimed X.hash.
    #[test]
    fn m7_lies_and_sim() {
        let c = DrawCase::new(4);
        let d0 = c.expected().d_claim;
        let mut l = c.clone();
        l.lie.mk_daa = Some(c.marker_daa as i64 + 1);
        assert_eq!(l.expected().d_claim, d0 + 1);
        assert_eq!(l.expected().d, c.d() + 1);
        let mut s = c.clone();
        let sim = ChainSim::anchored(c.d() as u64, Hash::from_bytes([0x33; 32]));
        s.sim = Some(Arc::new(sim));
        assert_eq!(s.x_hash(), [0x33; 32]);
        assert_ne!(s.seed(), c.seed());
        let mut a = c.clone();
        let mut bytes = c.anchor_bytes();
        bytes[0] ^= 1;
        a.lie.anchor = Some(bytes.clone());
        assert_eq!(a.x_hash()[..], bytes[..32]);
        assert_eq!(a.expected().anchor, bytes);
    }
}
