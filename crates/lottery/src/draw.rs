//! `draw()` with a TEST-ONLY seed (spec M3 A4, D42, D43). This module exists only with the
//! `test-seed` feature, and it is the only place `covenants/drawer.sil` is compiled (D43).
//!
//! Shape (A4):
//!   inputs : 0 drawer r (leader, `draw_test_seed`); 1 marker r (`draw_leg`); 2 vault r - 1
//!            (`draw_leg`); 3 controller r + 1, drawn = 0 (`draw_leg`); 4 + i absorbed pool i
//!            (`absorb_leg`), i < a <= A_MAX; 4 + a .. caller funding (zero ID, 1..F_MAX)
//!   outputs: 0 drawer'; 1 vault'; 2 controller'; 3 .. 2 + np the new pools in tier order
//!            (all authorized by input 0 under the instance covenant ID); then plain: the fee
//!            output (only if A + F >= f_min), the marker refund, one refund per absorbed pool
//!            in input order, the caller output (last).
//!
//! `DrawCase` describes a draw with public fields; `expected()` derives every output from
//! the Rust reference in `game` (never from the script); `build()` returns an editable
//! `Built`. Tests change one case field (input-side attacks), one `Lie` field (a consistent
//! false claim: the builder computes every output from it, so only the check that catches
//! the claim can reject), or one thing in the `Built` (output-side attacks).

use std::sync::Arc;

use harness::{ArtifactValue, CovenantBinding, Hash, Transaction, TransactionId, TransactionOutpoint, TransactionOutput, UtxoEntry, compile_source, outpoint};

use crate::buy::{Built, COV, FUNDING_SK, In, InKind, KAS, Template, Templates, b_max, spk_of};
use crate::close::{CloseCase, FundingIn, TEST_DEADLINE, TEST_ROUND_LEN};
use crate::combo::unrank;
use crate::game::{
    CLAIM_WINDOW, DRAW_WINDOW, F_MIN, FeeSplit, JackpotFix, JackpotRule, PoolSet, RoundAmounts, SeedNumbers, boost_after, claim_deadline,
    draw_amounts, fee_split, fix_jackpot, muldiv, pool_set, seed_numbers, vault_settle, win_counts,
};
use crate::params::xonly;
use crate::smt::{Tree, proofs_at};
use crate::state::{ControllerState, DrawerState, K_MAX, MarkerState, Payout, PoolState, TIER_LOWER0, VaultState};

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

/// seed_ref of the round 1 marker in the standalone fixture (round r: + (r - 1) x 10^6).
pub const TEST_SEED_REF: i64 = 1_000_600;
/// Fixture vault V (10,000,000 KAS, as the genesis fixture).
pub const TEST_VAULT: i64 = 10_000_000 * KAS as i64;

// Argument positions of `draw_test_seed` (A4 "Arguments", drawer.sil order).
pub const ARG_SEED: usize = 0;
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
/// `own_daa`: the maximum creation DAA score of the inputs, verified by check 19 (user
/// decision 2026-10-10, spec correction of A4 check 19 and A9).
pub const ARG_OWN: usize = 19;

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
    /// `own_daa` argument instead of the true maximum input DAA score; D_claim follows it
    /// (DA17).
    pub own: Option<i64>,
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
    pub test_only_seed: [u8; 32],
    /// Reserve funder of the new pools (argument and pool field).
    pub funder: Payout,
    /// Transaction fee.
    pub fee: u64,
    /// Lock time: max(seed_ref, absorbed deadlines) for a valid case (D51).
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
    pub own: i64,
    pub d_claim: i64,
    pub drawer: DrawerState,
    pub vault: VaultState,
    pub controller: ControllerState,
    /// The new pools, output order 3..2+np.
    pub new_pools: Vec<PoolState>,
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
    /// The fee output's index, if the fee is paid.
    pub fn fee_output(&self) -> Option<usize> {
        self.fee.paid.then_some(3 + self.np())
    }
    pub fn marker_refund(&self) -> usize {
        3 + self.np() + self.fee.paid as usize
    }
    pub fn pool_refund(&self, i: usize) -> usize {
        self.marker_refund() + 1 + i
    }
    pub fn caller_output(&self) -> usize {
        self.marker_refund() + 1 + self.a
    }
    /// 3 + np + paid + 1 + a + 1 (A4 check 20).
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
        let seed_ref = TEST_SEED_REF + (r - 1) * 1_000_000;
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
            seed_ref,
            funder_class: 0,
            funder_key: [0; 33],
        };
        marker.set_funder(&Payout::Schnorr(xonly(&FUNDING_SK)));
        let vault = VaultState {
            round: r - 1,
            total_assets: TEST_VAULT,
            total_shares: 0,
            pend_dep: 0,
            pend_wsh: 0,
            liability: 1_040_000 * KAS as i64,
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
        let close_daa = (seed_ref - 600) as u64;
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
            // Fresh: created after seed_ref + DRAW_WINDOW, so D_claim = own + CLAIM_WINDOW.
            funding: vec![FundingIn { value: 100 * KAS, daa: (seed_ref + DRAW_WINDOW + 10_000) as u64 }],
            test_only_seed: [0; 32],
            funder: Payout::Schnorr(xonly(&FUNDING_SK)),
            fee: KAS / 2,
            lock_time: seed_ref as u64,
            lie: Lie::default(),
            cov: COV,
            funding_sk: FUNDING_SK,
        };
        c.sync_roots();
        c
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

    /// The seed mapping of this draw (D42): the instance ID, the marker's round and m.
    pub fn seed(&self) -> SeedNumbers {
        seed_numbers(&self.cov.as_bytes(), self.marker.round, &self.test_only_seed, self.marker.m)
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
    /// deadline 100,000 DAA before seed_ref, distinct funders). Requires r >= 2.
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
                deadline: self.marker.seed_ref - 100_000,
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

    /// lock_time = max(seed_ref, absorbed deadlines) (D51).
    pub fn fix_lock_time(&mut self) {
        let mut l = self.marker.seed_ref;
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

    /// Max creation DAA score over all inputs (check 19).
    pub fn own(&self) -> u64 {
        let mut own = self.drawer_daa.max(self.marker_daa).max(self.vault_daa).max(self.controller_daa);
        for p in &self.absorbed {
            own = own.max(p.daa);
        }
        for f in &self.funding {
            own = own.max(f.daa);
        }
        own
    }

    /// Every successor state and output value of the draw (A4 checks 8-25), from `game`.
    /// Panics where the reference rejects (V' < 0, a guard of A6, D_claim >= DAA_MAX).
    pub fn expected(&self) -> DrawExpected {
        let mk = &self.marker;
        let ctrl = &self.controller;
        let seed = self.seed();
        let rank = self.lie.rank.unwrap_or(seed.rank);
        let bonus = self.lie.bonus.unwrap_or(seed.bonus);
        let win5 = self.lie.win5.unwrap_or(unrank(rank));
        let (leaves, siblings) = proofs_at(&self.trees, rank);
        let counts: Vec<Vec<u16>> = self.trees.iter().map(|t| t.counts(rank)).collect();
        let (h_j, n2) = self.lie.counts.unwrap_or_else(|| win_counts(&counts, bonus, mk.m));
        let amounts = draw_amounts(mk);
        let j_house = mk.jackpot - mk.j_boost;
        let pools = pool_set(h_j, n2, mk.jackpot, mk.g2, &amounts.b, mk.m);
        let v_prime = vault_settle(self.vault.total_assets, amounts.h, j_house, pools.jp, mk.g2, n2, pools.sp).expect("V' >= 0 (check 14)")
            + self.lie.v_delta;
        let q: Vec<i64> = self.absorbed.iter().map(|p| p.remaining as i64).collect();
        let o_prime = boost_after(self.pot as i64, amounts.b_r, mk.j_boost, pools.jp, &amounts.b, &pools.lp, &q).expect("O' guards (check 15)")
            + self.lie.o_delta;
        let mut fee = fee_split(ctrl.acc, amounts.f, self.drawer.f_min).expect("T_F guard (check 16)");
        if let Some(p) = self.lie.paid {
            fee.paid = p;
            fee.a_next = if p { 0 } else { fee.t_f };
        }
        let rule = JackpotRule::new(ctrl, v_prime);
        let fix = match self.lie.witness {
            None => fix_jackpot(&rule, v_prime, o_prime, ctrl.j),
            Some((x1, y, x2)) => {
                let jackpot = x2 + y;
                let g2 = muldiv(jackpot, rule.g);
                JackpotFix { x_b: muldiv(v_prime, ctrl.j), x1, y, x2, jackpot, j_boost: y, g2, liability: x2 + g2 * (rule.m - 1) }
            }
        };
        let own = self.lie.own.unwrap_or(self.own() as i64);
        let d_claim = claim_deadline(own, mk.seed_ref, self.drawer.draw_window, self.drawer.claim_window).expect("D_claim < DAA_MAX (check 19)");

        let drawer = DrawerState { round: self.drawer.round + 1, ..self.drawer.clone() };
        let vault = VaultState { round: mk.round, total_assets: v_prime, liability: fix.liability, ..self.vault.clone() };
        let controller = ControllerState {
            jackpot: fix.jackpot,
            j_boost: fix.j_boost,
            g2: fix.g2,
            liability: fix.liability,
            v_rec: v_prime,
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
        let mut values = vec![r, (v_prime + self.vault.pend_dep) as u64 + r, (o_prime + fee.a_next) as u64 + r];
        values.extend(pools.pools.iter().map(|p| p.value as u64 + r));
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
            own,
            d_claim,
            drawer,
            vault,
            controller,
            new_pools,
            values,
            a: self.a(),
        }
    }

    /// The caller output by the A4 formula: funding - np R + C_draw - fee.
    pub fn caller_formula(&self, x: &DrawExpected) -> i128 {
        self.funding_total() as i128 - (x.np() as u64 * self.reserve()) as i128 + x.amounts.c_draw as i128 - self.fee as i128
    }

    /// draw_test_seed(test_only_seed, win5, leaves, siblings, x1, y, x2, ctrl_pl, ctrl_sl,
    /// marker_pl, marker_sl, vault_pl, vault_sl, pool_pl, pool_sl, pool_prefix, pool_suffix,
    /// funder_class, funder_key, own_daa).
    pub fn args(&self, t: &Templates, x: &DrawExpected) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        vec![
            Bytes(self.test_only_seed.to_vec()),
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
            Int(x.own),
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
                    entry: "draw_test_seed".into(),
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
        }
    }

    /// The record rebuilt from the `draw()` transaction bytes alone (spec A9), without the
    /// builder's data or the spent UTXO entries:
    ///   - cov from output 0's covenant binding;
    ///   - round, m, k, roots, the shares and seed_ref from the marker's redeem (input 1);
    ///   - draw_window, claim_window and ticket_tpl from the drawer's redeem (input 0);
    ///   - R (the pools' reserve) from output 0's value (drawer' = R, check 20);
    ///   - test_only_seed, win5, the count leaves and the funder from input 0's arguments;
    ///   - bonus from the seed mapping (D42); h_J and n2 from the leaves; the pool set (D45)
    ///     and slices from the marker; values and outpoints from the outputs;
    ///   - D_claim from the `own_daa` argument, which check 19 requires to equal the maximum
    ///     creation DAA score of the inputs (user decision 2026-10-10), seed_ref and the
    ///     drawer's windows (D39).
    /// `t` supplies the committed marker and drawer layouts. Returns an error on any
    /// inconsistency (wrong shape, pool count or value).
    pub fn from_tx(tx: &Transaction, t: &Templates) -> Result<Self, String> {
        let cov = tx.outputs.first().and_then(|o| o.covenant).ok_or("output 0 has no covenant binding")?.covenant_id;
        let drawer_pushes = pushes(&tx.inputs.first().ok_or("no input 0")?.signature_script)?;
        let marker_pushes = pushes(&tx.inputs.get(1).ok_or("no input 1")?.signature_script)?;
        // draw_test_seed has 20 arguments, then the dispatch tag and the redeem script.
        if drawer_pushes.len() != 22 {
            return Err(format!("input 0: {} pushes, expected 22", drawer_pushes.len()));
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
            seed_ref: int(&mk, "seed_ref")?,
            funder_class: 0,
            funder_key: [0; 33],
        };
        let arg = |i: usize, len: usize| -> Result<&Vec<u8>, String> {
            let a = &drawer_pushes[i];
            if a.len() == len { Ok(a) } else { Err(format!("argument {i}: {} bytes, expected {len}", a.len())) }
        };
        let seed: [u8; 32] = arg(ARG_SEED, 32)?.as_slice().try_into().unwrap();
        let win5: [u8; 5] = arg(ARG_WIN5, 5)?.as_slice().try_into().unwrap();
        let leaves = arg(ARG_LEAVES, 2 * m as usize * k as usize)?;
        let class = arg(ARG_FUNDER_CLASS, 1)?[0];
        let key: [u8; 33] = arg(ARG_FUNDER_KEY, 33)?.as_slice().try_into().unwrap();
        if class > 2 {
            return Err(format!("funder class {class}"));
        }
        let funder = Payout::from_fields(class, &key);

        let sn = seed_numbers(&cov.as_bytes(), round, &seed, m);
        if win5 != sn.main {
            return Err("win5 is not the seed's main numbers".into());
        }
        let counts: Vec<Vec<u16>> =
            leaves.chunks(2 * m as usize).map(|c| c.chunks(2).map(|x| u16::from_le_bytes([x[0], x[1]])).collect()).collect();
        let (h_j, n2) = win_counts(&counts, sn.bonus, m);
        let amounts = draw_amounts(&marker);
        let set = pool_set(h_j, n2, marker.jackpot, marker.g2, &amounts.b, m);
        let n_cov = tx.outputs.iter().filter(|o| o.covenant.is_some()).count();
        if n_cov != 3 + set.np() {
            return Err(format!("{n_cov} covenant outputs, expected 3 + {}", set.np()));
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
        let own = script_num(&drawer_pushes[ARG_OWN])?;
        let d_claim = claim_deadline(own, marker.seed_ref, int(&drawer, "draw_window")?, int(&drawer, "claim_window")?).ok_or("D_claim out of range")?;
        let roots = (0..k as usize).map(|i| roots_bytes[32 * i..32 * (i + 1)].try_into().unwrap()).collect();
        let ticket_tpl: [u8; 32] = match drawer.get("ticket_tpl") {
            Some(ArtifactValue::Bytes(b)) if b.len() == 32 => b.as_slice().try_into().unwrap(),
            other => return Err(format!("drawer ticket_tpl: {other:?}")),
        };
        Ok(Self { round, win: [win5[0], win5[1], win5[2], win5[3], win5[4], sn.bonus], m, k, roots, pools, d_claim, funder, ticket_tpl, reserve: r })
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

