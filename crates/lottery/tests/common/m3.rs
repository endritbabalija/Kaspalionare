//! M3 Stage 4: `draw()` attacks expressed as data (`Atk`), shared by `m3_attacks.rs` (run
//! against the committed templates) and `m3_mutants.rs` (re-run under the mutant that lacks
//! the `require` an attack targets).
//!
//! An `Atk` is a valid draw (`good`), one case edit and one transaction edit. Under any
//! templates `t` the good case is first pointed at `t` (`retarget`), so the same attack runs
//! against a mutant whose template hashes differ. The edits never change the case's
//! templates.

use std::collections::BTreeMap;
use std::sync::Arc;

use harness::{ArtifactValue, CovenantBinding, ScriptPublicKey, TransactionOutput, UtxoEntry, pay_to_script_hash_script};
use kaspa_consensus_core::tx::GenesisCovenantGroup;
use lottery::buy::{Built, KAS, Templates, spk_of};
use lottery::close::FundingIn;
use lottery::combo::{RANKS, unrank};
use lottery::draw::*;
use lottery::game::{DRAW_WINDOW, JackpotRule, M_DRAW, TOTAL_MAX, witness_ok};
use lottery::smt::Tree;
use lottery::state::{Payout, PoolState};

use super::m2::*;

pub const DRW: usize = DrawCase::DRAWER;
pub const MK: usize = DrawCase::MARKER;
pub const VLT: usize = DrawCase::VAULT;
pub const CTL: usize = DrawCase::CONTROLLER;
pub const POOL0: usize = 4;

pub type CaseEdit = Box<dyn Fn(&mut DrawCase)>;
pub type BuiltEdit = Box<dyn Fn(&mut Built, &DrawCase)>;

/// One attack: `good` with `case` applied, built, then `built` applied, must be rejected by
/// input `input` while `good` passes it.
pub struct Atk {
    pub input: usize,
    pub good: DrawCase,
    pub case: CaseEdit,
    pub built: BuiltEdit,
}

impl Atk {
    pub fn case(input: usize, good: DrawCase, f: impl Fn(&mut DrawCase) + 'static) -> Self {
        Self { input, good, case: Box::new(f), built: Box::new(|_, _| {}) }
    }

    pub fn built(input: usize, good: DrawCase, f: impl Fn(&mut Built, &DrawCase) + 'static) -> Self {
        Self { input, good, case: Box::new(|_| {}), built: Box::new(f) }
    }

    pub fn both(input: usize, good: DrawCase, c: impl Fn(&mut DrawCase) + 'static, b: impl Fn(&mut Built, &DrawCase) + 'static) -> Self {
        Self { input, good, case: Box::new(c), built: Box::new(b) }
    }

    /// The same attack aimed at another input.
    pub fn at(mut self, input: usize) -> Self {
        self.input = input;
        self
    }

    /// (good, bad) transactions under templates `t`.
    pub fn pair(&self, t: &Arc<Templates>) -> (Built, Built) {
        let good = retarget(self.good.clone(), t);
        let mut bad = good.clone();
        (self.case)(&mut bad);
        let mut bb = bad.build_with(t.clone());
        (self.built)(&mut bb, &bad);
        (good.build_with(t.clone()), bb)
    }

    /// Committed templates: the bad draw is rejected by `input` with a script error, the good
    /// draw passes `input` and is accepted by every input.
    pub fn strike(&self) {
        let (g, b) = self.pair(&tpl());
        strike(&g, &b, self.input);
        accept_draw(&g);
    }
}

/// Point a draw case at templates `t`: a mutant or look-alike changes the drawer, marker,
/// vault and pool hashes (and a controller mutant every hash but the ticket's).
pub fn retarget(mut c: DrawCase, t: &Templates) -> DrawCase {
    c.drawer.pool_tpl = t.pool.hash;
    c.drawer.vault_tpl = t.vault.hash;
    c.controller.marker_tpl = t.marker.hash;
    c.controller.shard_tpl = t.shard.hash;
    c.controller.params_tpl = t.params.hash;
    c
}

// ---------------------------------------------------------------- bases (as m3_attacks.rs)

/// The m3_attacks base: round 2, K = 4, sales 157 KAS, the jackpot held once, n2 = 1, a Z_0
/// leaf in a non-empty tree, one absorbed pool, fee paid; np = 5.
pub fn base() -> DrawCase {
    let mut c = DrawCase::at_round(4, 2);
    c.set_sales(157 * KAS as i64, 5 * KAS as i64);
    c.hold_jackpot(1, 1);
    let b = c.other_bonus(0);
    c.hold(2, b, 2);
    let other = (c.seed().rank + 1) % RANKS;
    c.hold_at(3, other, 1, 1);
    c.absorb(1);
    c
}

/// `base()` without the jackpot holding (h_J = 0).
pub fn base_no_jp() -> DrawCase {
    let mut c = DrawCase::at_round(4, 2);
    c.set_sales(157 * KAS as i64, 5 * KAS as i64);
    let b = c.other_bonus(0);
    c.hold(2, b, 2);
    c.absorb(1);
    c
}

/// `base()` without the absorbed pool (a = 0).
pub fn base_no_absorb() -> DrawCase {
    let mut c = base();
    c.absorbed.clear();
    c.fix_lock_time();
    c
}

/// `base()` with one funding input of `kas` KAS (attacks that keep money in the vault or
/// the pot need it to stay balanced).
pub fn base_funded(kas: u64) -> DrawCase {
    let mut c = base();
    c.funding[0].value = kas * KAS;
    c
}

// ---------------------------------------------------------------- state edits

fn with_role(mut v: BTreeMap<String, ArtifactValue>, role: u8) -> BTreeMap<String, ArtifactValue> {
    v.insert("role".into(), ArtifactValue::Byte(role));
    v
}

/// A role byte that no M3 object uses.
pub const BAD_ROLE: u8 = 0x07;

/// The value-preserving redeem of input `i` replaced by `redeem`.
fn set_redeem(b: &mut Built, i: usize, redeem: Vec<u8>) {
    b.set_entry_redeem(i, redeem);
}

// ---------------------------------------------------------------- check 2 (DA8)

/// a = -1 (D2_a): the controller input without the covenant ID and no absorbed pool, so
/// OpCovInputCount = 3. The caller output is dropped so the output count matches
/// 3 + np + paid + 1 + a + 1 (its value becomes fee).
pub fn a_negative() -> Atk {
    Atk::built(DRW, base_no_absorb(), |b, _| {
        let e = b.inputs[CTL].entry.clone();
        b.inputs[CTL].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, None);
        b.outputs.pop();
    })
}

/// a = A_MAX + 1 = 6 absorbed pools (D2_a upper half; DA8).
pub fn a_above_max() -> Atk {
    let mut five = DrawCase::at_round(4, 2);
    five.absorb(5);
    Atk::case(DRW, five, |c| c.absorb(1))
}

/// No funding input (D2_fund, n_fund = 0). Sales of 100,000 KAS make C_draw cover the five
/// pool reserves, so the draw balances without funding.
pub fn no_funding() -> Atk {
    let mut good = base();
    good.set_sales(100_000 * KAS as i64, 5 * KAS as i64);
    Atk::case(DRW, good, |c| c.funding.clear())
}

/// F_MAX + 1 = 9 funding inputs (D2_fund upper half; DA8).
pub fn nine_funding() -> Atk {
    let mut eight = base();
    eight.funding = vec![FundingIn { value: 15 * KAS, daa: eight.funding[0].daa }; 8];
    Atk::case(DRW, eight, |c| c.funding.push(c.funding[0]))
}

/// The marker input under another covenant ID (D2_cov).
pub fn marker_other_cov() -> Atk {
    Atk::built(DRW, base(), |b, _| {
        let e = b.inputs[MK].entry.clone();
        b.inputs[MK].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, Some(OTHER_COV));
    })
}

/// A funding input with another covenant ID (D2_zero; DA8).
pub fn funding_other_cov() -> Atk {
    Atk::built(DRW, base(), |b, c| {
        let fin = c.funding_input(0);
        let e = b.inputs[fin].entry.clone();
        b.inputs[fin].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, Some(OTHER_COV));
    })
}

// ---------------------------------------------------------------- checks 3-6 (DA10, DA16)

/// The controller input (and its successor, which copies the role) with role 0x07 (D3_role).
pub fn ctrl_role() -> Atk {
    Atk::built(DRW, base(), |b, c| {
        let t = b.templates.clone();
        set_redeem(b, CTL, t.controller.redeem(&with_role(c.controller.values(), BAD_ROLE)));
        b.outputs[2].script_public_key = pay_to_script_hash_script(&t.controller.redeem(&with_role(c.expected().controller.values(), BAD_ROLE)));
    })
}

/// Controller of round r + 2 (D3_round, CL4_round's sibling; DA10).
pub fn ctrl_round() -> Atk {
    Atk::case(DRW, base(), |c| c.controller.round += 1)
}

/// Controller already drawn (D3_drawn, CL5; DA10).
pub fn ctrl_drawn() -> Atk {
    Atk::case(DRW, base(), |c| c.controller.drawn = 1)
}

/// Controller (round r + 1) with m' = M_DRAW + 1: round r + 1 could never be drawn
/// (D3_mdraw).
pub fn ctrl_m_above() -> Atk {
    Atk::case(DRW, base(), |c| c.controller.m = M_DRAW + 1)
}

/// The marker input with role 0x07 (D4_role; CL4_role at input 3).
pub fn marker_role() -> Atk {
    Atk::built(DRW, base(), |b, c| {
        let t = b.templates.clone();
        set_redeem(b, MK, t.marker.redeem(&with_role(c.marker.values(), BAD_ROLE)));
    })
}

/// Only the marker input's round field set to r - 1 (seed, pools and outputs unchanged;
/// D4_round).
pub fn marker_round_field() -> Atk {
    Atk::built(DRW, base(), |b, c| {
        let mut m = c.marker.clone();
        m.round -= 1;
        let r = b.templates.marker_redeem(&m);
        set_redeem(b, MK, r);
    })
}

/// Marker of round r - 1, case level (the seed follows; DA10; ML4_round, CL4_round).
pub fn marker_round() -> Atk {
    Atk::case(DRW, base(), |c| c.marker.round -= 1)
}

/// The marker input with k = ctrl.k - 1 (D4_k).
pub fn marker_k() -> Atk {
    Atk::built(DRW, base(), |b, c| {
        let mut m = c.marker.clone();
        m.k -= 1;
        let r = b.templates.marker_redeem(&m);
        set_redeem(b, MK, r);
    })
}

/// A marker with m = M_DRAW + 1 and consistent trees (D4_mdraw); the good draw has m = M_DRAW.
pub fn marker_m_above() -> Atk {
    let set_m = |c: &mut DrawCase, m: i64| {
        c.marker.m = m;
        c.trees = (0..c.k()).map(|_| Tree::new(m as usize)).collect();
        c.sync_roots();
    };
    let mut good = DrawCase::at_round(4, 2);
    set_m(&mut good, M_DRAW);
    Atk::case(DRW, good, move |c| set_m(c, M_DRAW + 1))
}

/// The vault input (and vault', which copies the role) with role 0x07 (D5_role).
pub fn vault_role() -> Atk {
    Atk::built(DRW, base(), |b, c| {
        let t = b.templates.clone();
        set_redeem(b, VLT, t.vault.redeem(&with_role(c.vault.values(), BAD_ROLE)));
        b.outputs[1].script_public_key = pay_to_script_hash_script(&t.vault.redeem(&with_role(c.expected().vault.values(), BAD_ROLE)));
    })
}

/// Vault of round r (D5_round, VL4_round; DA10).
pub fn vault_round() -> Atk {
    Atk::case(DRW, base(), |c| c.vault.round += 1)
}

/// Input `i`'s value + `d` (D4_value, D5_value, D6_drawer; DA16).
pub fn input_value(i: usize, d: i64) -> Atk {
    Atk::built(DRW, base(), move |b, _| b.inputs[i].entry.amount = (b.inputs[i].entry.amount as i64 + d) as u64)
}

/// The controller input worth acc + R - 1 (O = -1), with O' and the witnesses following the
/// script's O' (D6_pot). The good draw has O = 0.
pub fn pot_negative() -> Atk {
    let mut good = base();
    good.pot = 0;
    Atk::both(DRW, good, |c| c.lie.o_delta = -1, |b, _| b.inputs[CTL].entry.amount -= 1)
}

// ---------------------------------------------------------------- checks 7-10 (DA1, DA2, DA12)

/// lock_time = seed_ref - 1 (D7; DA12).
pub fn early_lock() -> Atk {
    Atk::case(DRW, base(), |c| c.lock_time = c.marker.seed_ref as u64 - 1)
}

/// A non-canonical win5 with the seed's colex rank (D8_canon): (n1 + C(n2 - 1, 2), 1, n3,
/// n4, n5) has the same rank under the script's formula. The pools carry it. A round 1 draw
/// whose seed gives a rank where that first byte fits.
pub fn win5_noncanonical() -> Atk {
    for i in 0u32..10_000 {
        let mut c = DrawCase::new(4);
        c.test_only_seed[..4].copy_from_slice(&i.to_le_bytes());
        let w = unrank(c.seed().rank);
        let y2 = w[1] as u32 - 1;
        let n1 = w[0] as u32 + y2 * (y2.saturating_sub(1)) / 2;
        if y2 >= 2 && n1 <= 255 {
            let bad = [n1 as u8, 1, w[2], w[3], w[4]];
            assert_eq!(colex(&bad), c.seed().rank as i64);
            return Atk::case(DRW, c, move |c| c.lie.win5 = Some(bad));
        }
    }
    panic!("no seed with a same-rank non-canonical win5");
}

/// The script's colex formula of A4 check 8 (no canonicity assumed).
pub fn colex(w: &[u8; 5]) -> i64 {
    let n: Vec<i64> = w.iter().map(|&b| b as i64).collect();
    let (y2, y3, y4, y5) = (n[1] - 1, n[2] - 1, n[3] - 1, n[4] - 1);
    (n[0] - 1) + y2 * (y2 - 1) / 2 + y3 * (y3 - 1) * (y3 - 2) / 6 + y4 * (y4 - 1) * (y4 - 2) * (y4 - 3) / 24 + y5 * (y5 - 1) * (y5 - 2) * (y5 - 3) * (y5 - 4) / 120
}

/// A canonical win5 of another rank with the seed rank's proofs (D8_rank): the pools record
/// numbers the seed did not draw.
pub fn win5_other_rank() -> Atk {
    Atk::case(DRW, base(), |c| {
        let r = (c.seed().rank + 1) % RANKS;
        c.lie.win5 = Some(unrank(r));
    })
}

/// The leaves argument 2 bytes longer (D9_len; DA1).
pub fn leaves_longer() -> Atk {
    Atk::built(DRW, base(), |b, _| match &mut b.entry_args_mut(DRW)[ARG_LEAVES] {
        ArtifactValue::Bytes(l) => l.extend([0, 0]),
        a => panic!("leaves: {a:?}"),
    })
}

/// A forged leaf, consistent everywhere else (D9_root): shard 0's leaf claims one more
/// second-prize bonus (n2 = 2), the marker roots stay genuine; pools and vault follow n2 = 2.
pub fn forged_leaf() -> Atk {
    Atk::case(DRW, base(), |c| {
        let roots = c.marker.roots.clone();
        let b = c.other_bonus(1);
        c.hold(0, b, 1);
        c.marker.roots = roots;
    })
}

/// A leaf with a count of 32,768 (bytes 0x00 0x80, sign bit set) at a second-prize bonus,
/// proven against its root (D10_range). The Rust tree cannot hold that count (buy() never
/// writes it), so the case holds a count of 1 there (the same n2 and pools as a nonzero
/// count) and the leaf bytes and shard 0's marker root are forged to the 0x8000 leaf.
pub fn count_sign_bit() -> Atk {
    use lottery::smt::{h, num_bytes, root_from_path};
    fn slot(c: &DrawCase) -> usize {
        2 * (c.other_bonus(1) as usize - 1)
    }
    Atk::both(
        DRW,
        base(),
        |c| {
            let rank = c.seed().rank;
            let b = c.other_bonus(1);
            c.hold(0, b, 1);
            let mut counts = c.trees[0].counts_bytes(rank);
            counts[slot(c)..slot(c) + 2].copy_from_slice(&[0x00, 0x80]);
            let leaf = h(&[&[0x00][..], &num_bytes(rank as u64, 3), &counts].concat());
            let root = root_from_path(rank, leaf, &c.trees[0].siblings(rank));
            c.marker.roots[..32].copy_from_slice(&root);
        },
        |b, c| match &mut b.entry_args_mut(DRW)[ARG_LEAVES] {
            ArtifactValue::Bytes(l) => l[slot(c)..slot(c) + 2].copy_from_slice(&[0x00, 0x80]),
            a => panic!("leaves: {a:?}"),
        },
    )
}

/// No second-prize pool although n2 = 1 (D10_n2; DA2).
pub fn n2_zero_claim() -> Atk {
    Atk::case(DRW, base(), |c| c.lie.counts = Some((1, 0)))
}

// ---------------------------------------------------------------- checks 13-16 (DA2-DA8)

/// A pool output authorized by the marker (D13, ML3; DA8).
pub fn parallel_auth() -> Atk {
    Atk::built(DRW, base(), |b, c| b.outputs[3].covenant = Some(CovenantBinding::new(MK as u16, c.cov)))
}

/// The vault keeps J_house although the jackpot is held (D14_jp); the caller funds it.
pub fn vault_keeps_jhouse() -> Atk {
    Atk::case(DRW, base_funded(2_000_000), |c| c.lie.v_delta = c.marker.jackpot - c.marker.j_boost)
}

/// G2 = TOTAL_MAX / 254 + 1 with n2 = 1 (D14_g2guard; A6 guard test).
pub fn g2_guard() -> Atk {
    let mut good = base();
    good.vault.total_assets = 20_000_000_000_000_000;
    good.marker.g2 = TOTAL_MAX / 254;
    Atk::case(DRW, good, |c| c.marker.g2 += 1)
}

/// The vault keeps n2 G2 although a second-prize pool is paid (D14_sp).
pub fn vault_keeps_second() -> Atk {
    Atk::case(DRW, base_funded(100_000), |c| c.lie.v_delta = c.marker.g2 * c.expected().n2)
}

/// V' = -1 (D14_nonneg): the good draw settles the vault to exactly 0; the attack spends a
/// vault 1 sompi poorer and records total_assets' = -1 (vault' value R - 1, V_rec' = -1).
/// The witnesses are the same at V' = 0 and -1 (Cv = x_b = 0).
pub fn vault_negative() -> Atk {
    let mut good = base();
    let x = good.expected();
    good.vault.total_assets -= x.v_prime;
    assert_eq!(good.expected().v_prime, 0);
    Atk::built(DRW, good, |b, c| {
        let t = b.templates.clone();
        let mut v = c.vault.clone();
        v.total_assets -= 1;
        set_redeem(b, VLT, t.vault_redeem(&v));
        b.inputs[VLT].entry.amount -= 1;
        let x = c.expected();
        let mut vp = x.vault.clone();
        vp.total_assets = -1;
        b.outputs[1].script_public_key = t.vault_spk(&vp);
        b.shift_value(1, -1);
        let mut cp = x.controller.clone();
        cp.v_rec = -1;
        b.outputs[2].script_public_key = t.controller_spk(&cp);
    })
}

/// Unheld J_boost to the caller instead of the pot (D15_jboost).
pub fn jboost_to_caller() -> Atk {
    Atk::case(DRW, base_no_jp(), |c| c.lie.o_delta = -c.marker.j_boost)
}

/// A draw with B_3 > 0 below one slice (lp_3 = 0).
pub fn small_sales() -> DrawCase {
    let mut c = base_no_jp();
    c.set_sales(50_000, 0);
    let x = c.expected();
    assert!(!x.pools.lp[2] && x.amounts.b[2] > 0);
    c
}

/// An unpaid lower-tier budget to the caller instead of the pot (D15_lower).
pub fn lower_to_caller() -> Atk {
    Atk::case(DRW, small_sales(), |c| c.lie.o_delta = -c.expected().amounts.b[2])
}

/// Q not added to O' (D15_q; DA6).
pub fn q_to_caller() -> Atk {
    Atk::case(DRW, base(), |c| c.lie.o_delta = -(c.absorbed[0].remaining as i64))
}

/// A fee base where T_F < F_MIN.
pub fn fee_low() -> DrawCase {
    let mut low = base();
    low.controller.acc = 0;
    low.set_sales(10 * KAS as i64, 0);
    assert!(!low.expected().fee.paid);
    low
}

/// Fee paid although T_F < F_MIN (D16; DA5).
pub fn fee_paid_below() -> Atk {
    Atk::case(DRW, fee_low(), |c| c.lie.paid = Some(true))
}

/// Fee carried although T_F >= F_MIN (D16; DA5).
pub fn fee_carried_above() -> Atk {
    Atk::case(DRW, base(), |c| c.lie.paid = Some(false))
}

// ---------------------------------------------------------------- check 17 (A5, DA4)

fn largest(mut lo: i64, mut hi: i64, pred: impl Fn(i64) -> bool) -> i64 {
    assert!(pred(lo));
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if pred(mid) { lo = mid } else { hi = mid - 1 }
    }
    lo
}

/// The largest x2 >= x1 passing with y.
pub fn x2_for(rule: &JackpotRule, x1: i64, y: i64) -> i64 {
    largest(x1, rule.e_lim.min(rule.cv).max(x1), |x| rule.pass(x, y))
}

/// The largest y in [0, hi] passing with x1.
pub fn y_for(rule: &JackpotRule, x1: i64, hi: i64) -> i64 {
    largest(0, hi, |y| rule.pass(x1, y))
}

/// The first `base()` variant (vault V, boost pot O, in KAS) whose expected draw satisfies
/// `pred` (the A5 branch an attack needs).
pub fn regime(name: &str, pred: impl Fn(&DrawExpected) -> bool) -> DrawCase {
    for v in [10_000_000i64, 5_000_000, 4_000_000, 3_000_000, 2_000_000, 1_500_000, 1_200_000, 1_000_000] {
        for o in [0u64, 1, 100, 10_000, 100_000, 600_000, 10_000_000] {
            let mut c = base();
            c.vault.total_assets = v * KAS as i64;
            c.pot = o * KAS;
            if pred(&c.expected()) {
                return c;
            }
        }
    }
    panic!("no A5 regime for {name}");
}

/// A consistent false witness triple (the outputs follow it); asserts that the A5 checks
/// reject it.
pub fn witness(good: DrawCase, w: (i64, i64, i64)) -> Atk {
    let x = good.expected();
    let (x1, y, x2) = w;
    assert!(x1 >= 0 && y >= 0 && x2 >= 0);
    assert!(!witness_ok(&x.rule, x.v_prime, x.o_prime, good.controller.j, x1, y, x2), "witness {w:?} is valid");
    Atk::case(DRW, good, move |c| c.lie.witness = Some(w))
}

fn xb_passes(x: &DrawExpected) -> bool {
    x.rule.pass(x.fix.x_b, 0)
}

/// x_b passes but x1 = x_b - 1 (y and x2 refit to it) (D17_x1_base).
pub fn x1_below_base() -> Atk {
    let c = regime("x_b passes", |x| xb_passes(x) && x.fix.x_b >= 1);
    let x = c.expected();
    let x1 = x.fix.x_b - 1;
    let y = y_for(&x.rule, x1, x.o_prime);
    witness(c, (x1, y, x2_for(&x.rule, x1, y)))
}

/// x_b fails and x1 = x_b (D17_x1_below).
pub fn x1_at_failing_base() -> Atk {
    let c = regime("x_b fails", |x| !xb_passes(x));
    let x = c.expected();
    witness(c, (x.fix.x_b, x.fix.y, x.fix.x2.max(x.fix.x_b)))
}

/// x_b fails and x1 = the largest passing value + 1 (D17_x1_pass).
pub fn x1_failing() -> Atk {
    let c = regime("x_b fails", |x| !xb_passes(x));
    let x = c.expected();
    witness(c, (x.fix.x1 + 1, x.fix.y, x.fix.x2.max(x.fix.x1 + 1)))
}

/// x_b fails and x1 = the largest passing value - 1 (y and x2 refit) (D17_x1_max).
pub fn x1_not_largest() -> Atk {
    let c = regime("x_b fails, x1 >= 1", |x| !xb_passes(x) && x.fix.x1 >= 1);
    let x = c.expected();
    let x1 = x.fix.x1 - 1;
    let y = y_for(&x.rule, x1, x.o_prime);
    witness(c, (x1, y, x2_for(&x.rule, x1, y)))
}

/// y = the largest passing boost although it exceeds O' (D17_y_pot): J_boost' unbacked by
/// the pot.
pub fn y_above_pot() -> Atk {
    let c = regime("pass(x1, O' + 1)", |x| x.rule.pass(x.fix.x1, x.o_prime + 1));
    let x = c.expected();
    let y = y_for(&x.rule, x.fix.x1, x.rule.w_lim.min(TOTAL_MAX));
    assert!(y > x.o_prime);
    witness(c, (x.fix.x1, y, x2_for(&x.rule, x.fix.x1, y)))
}

/// y = O' although it fails (capped regime) (D17_y_pass).
pub fn y_failing() -> Atk {
    let c = regime("y capped", |x| x.fix.y < x.o_prime);
    let x = c.expected();
    witness(c, (x.fix.x1, x.o_prime, x.fix.x2))
}

/// y = the largest passing boost - 1 in the capped regime (x2 refit) (D17_y_max).
pub fn y_not_largest() -> Atk {
    let c = regime("y capped, y >= 1", |x| x.fix.y < x.o_prime && x.fix.y >= 1);
    let x = c.expected();
    let y = x.fix.y - 1;
    witness(c, (x.fix.x1, y, x2_for(&x.rule, x.fix.x1, y)))
}

/// x2 = x1 - 1 (D17_x2_ge).
pub fn x2_below_x1() -> Atk {
    let c = regime("x1 >= 1", |x| x.fix.x1 >= 1);
    let x = c.expected();
    witness(c, (x.fix.x1, x.fix.y, x.fix.x1 - 1))
}

/// x2 + 1 (D17_x2_pass; DA4).
pub fn x2_plus_one() -> Atk {
    let c = base();
    let f = c.expected().fix;
    witness(c, (f.x1, f.y, f.x2 + 1))
}

/// x2 - 1 with x2 > x1 (D17_x2_max; DA4).
pub fn x2_minus_one() -> Atk {
    let c = regime("x2 > x1", |x| x.fix.x2 > x.fix.x1);
    let f = c.expected().fix;
    witness(c, (f.x1, f.y, f.x2 - 1))
}

// ---------------------------------------------------------------- checks 18-21 (DA6, DA9, DA13)

/// The absorbed pool input with role 0x07 (D18_role).
pub fn pool_role() -> Atk {
    Atk::built(DRW, base(), |b, c| {
        let t = b.templates.clone();
        set_redeem(b, POOL0, t.pool.redeem(&with_role(c.absorbed[0].state.values(), BAD_ROLE)));
    })
}

/// An absorbed pool of round r (D18_round, PL4_round; DA6).
pub fn pool_round_r() -> Atk {
    Atk::case(DRW, base(), |c| c.absorbed[0].state.round = c.marker.round)
}

/// A base whose absorbed pool's deadline is after seed_ref (so lock_time = deadline - 1
/// passes check 7).
pub fn late_deadline() -> DrawCase {
    let mut good = base();
    good.absorbed[0].state.deadline = good.marker.seed_ref + 1_000;
    good.fix_lock_time();
    good
}

/// lock_time = deadline - 1 (D18_deadline, PL5; DA6).
pub fn before_deadline() -> Atk {
    Atk::case(DRW, late_deadline(), |c| c.lock_time -= 1)
}

/// The absorbed pool's funder class 3, with its refund paid to the script the class-3 key
/// would map to (the P2SH form, payoutSpk's default) (D18_class).
pub fn pool_class3() -> Atk {
    Atk::built(DRW, base(), |b, c| {
        let t = b.templates.clone();
        let mut s: PoolState = c.absorbed[0].state.clone();
        s.funder_class = 3;
        set_redeem(b, POOL0, t.pool_redeem(&s));
        let script = [&[0xaa, 0x20][..], &s.funder_key[..32], &[0x87]].concat();
        let i = c.expected().pool_refund(0);
        b.outputs[i].script_public_key = ScriptPublicKey::new(0, script.into());
    })
}

/// own_daa one below the true maximum (D19_own; DA17).
pub fn own_lie() -> Atk {
    Atk::case(DRW, base(), |c| c.lie.own = Some(c.own() as i64 - 1))
}

/// Only inputs created before seed_ref + DRAW_WINDOW, and the new pools' deadline
/// own + CLAIM_WINDOW instead of seed_ref + DRAW_WINDOW + CLAIM_WINDOW (D19_max).
pub fn deadline_without_draw_window() -> Atk {
    let mut good = base();
    good.funding[0].daa = (good.marker.seed_ref + DRAW_WINDOW - 10) as u64;
    assert!((good.own() as i64) < good.marker.seed_ref + DRAW_WINDOW);
    Atk::built(DRW, good, |b, c| {
        let x = c.expected();
        let d = x.own + c.drawer.claim_window;
        assert!(d < x.d_claim);
        for p in 0..x.np() {
            let mut s = x.new_pools[p].clone();
            s.deadline = d;
            b.outputs[x.pool_output(p)].script_public_key = b.templates.pool_spk(&s);
        }
    })
}

/// An extra plain output after the caller output (D20_count; DA8).
pub fn extra_plain() -> Atk {
    Atk::built(DRW, base(), |b, _| {
        let last = b.outputs.len() - 1;
        b.outputs[last].value -= KAS;
        b.outputs.push(TransactionOutput::new(KAS, spk_of(&[9u8; 32])));
    })
}

/// Output `idx` value + `d` against the caller output.
pub fn output_value(good: DrawCase, idx: impl Fn(&DrawExpected) -> usize + 'static, d: i64) -> Atk {
    Atk::built(DRW, good, move |b, c| b.shift_value(idx(&c.expected()), d))
}

/// The caller output under a fresh covenant ID (D21_zero; DA8).
pub fn caller_with_cov() -> Atk {
    Atk::built(DRW, base(), |b, c| {
        let x = c.expected();
        b.genesis = vec![GenesisCovenantGroup::new(c.funding_input(0) as u16, vec![x.caller_output() as u32])];
    })
}

/// Output `idx` paid to another key (D21_fee, D21_mrefund, D21_prefund; DA5, DA9).
pub fn output_script(idx: impl Fn(&DrawExpected) -> usize + 'static) -> Atk {
    Atk::built(DRW, base(), move |b, c| b.outputs[idx(&c.expected())].script_public_key = Payout::Schnorr([0x77; 32]).spk())
}

// ---------------------------------------------------------------- check 25 (DA13)

/// funder_class = 3 in the argument and the new pools (D25_class; DA13).
pub fn new_pool_class3() -> Atk {
    Atk::built(DRW, base(), |b, c| {
        let x = c.expected();
        b.entry_args_mut(DRW)[ARG_FUNDER_CLASS] = ArtifactValue::Byte(3);
        for p in 0..x.np() {
            let mut s = x.new_pools[p].clone();
            s.funder_class = 3;
            b.outputs[x.pool_output(p)].script_public_key = b.templates.pool_spk(&s);
        }
    })
}

/// A class-0 funder key with a nonzero padding byte, in the argument and the new pools
/// (D25_pad; DA13).
pub fn new_pool_padding() -> Atk {
    Atk::built(DRW, base(), |b, c| {
        let x = c.expected();
        let mut key = x.new_pools[0].funder_key;
        key[32] = 1;
        b.entry_args_mut(DRW)[ARG_FUNDER_KEY] = ArtifactValue::Bytes(key.to_vec());
        for p in 0..x.np() {
            let mut s = x.new_pools[p].clone();
            s.funder_key = key;
            b.outputs[x.pool_output(p)].script_public_key = b.templates.pool_spk(&s);
        }
    })
}

// ---------------------------------------------------------------- legs (DA11)

/// The real drawer template at input 0 with role 0x07 (ML4_role, VL4_role, PL4_role).
pub fn drawer_role(input: usize) -> Atk {
    Atk::built(input, base(), |b, c| {
        let t = b.templates.clone();
        set_redeem(b, DRW, t.drawer.redeem(&with_role(c.drawer.values(), BAD_ROLE)));
    })
}

/// Inputs `i` and `j` swapped; aimed at `input` (L1).
pub fn swapped(i: usize, j: usize, input: usize) -> Atk {
    Atk::built(input, base(), move |b, _| b.inputs.swap(i, j))
}

/// Output `o` authorized by input `i` (L3).
pub fn leg_authorizes(i: usize, o: usize) -> Atk {
    Atk::built(i, base(), move |b, c| b.outputs[o].covenant = Some(CovenantBinding::new(i as u16, c.cov)))
}

/// The real drawer at input 0 under another covenant ID (its outputs carry it) (L2).
pub fn other_leader(input: usize) -> Atk {
    Atk::built(input, base(), |b, _| {
        let e = b.inputs[0].entry.clone();
        b.inputs[0].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, Some(OTHER_COV));
        for o in b.outputs.iter_mut().filter(|o| o.covenant.is_some()) {
            o.covenant = Some(CovenantBinding::new(0, OTHER_COV));
        }
    })
}
