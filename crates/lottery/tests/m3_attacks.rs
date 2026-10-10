//! M3 attacks on `draw()` and its delegate legs: DA1-DA16 (spec M3 A8), plus the A6 G2
//! guard (user decision 2026-10-10).
//!
//! Each attack starts from a valid draw (`base()` or a stated variant), changes exactly one
//! thing (a case field, one `Lie` field the builder then follows consistently, or one thing
//! in the `Built`), and requires a *script* error from the named input while the valid base
//! passes that same input (`strike`); each test ends by accepting its base (`done`). The
//! comment on each attack names the A4 check it targets. Leg-targeted strikes come first in
//! a test, drawer-targeted (input 0) strikes after.
//!
//! Which A4 check rejects each drawer-targeted strike is printed by the attribution
//! instrument (`M3_ATTRIB=1`, see `common::m2::first_failing_check`).

mod common;

use common::m2::*;
use common::m3 as s4;
use harness::{ArtifactValue, CovenantBinding, TransactionOutput, TxScriptError, UtxoEntry};
use kaspa_consensus_core::tx::GenesisCovenantGroup;
use lottery::buy::{Built, In, InKind, KAS, spk_of};
use lottery::close::FundingIn;
use lottery::combo::RANKS;
use lottery::draw::*;
use lottery::game::{JackpotRule, TOTAL_MAX};
use lottery::state::{ControllerState, DrawerState, Payout, PoolState, VaultState};

const DRW: usize = DrawCase::DRAWER;
const MK: usize = DrawCase::MARKER;
const VLT: usize = DrawCase::VAULT;
const CTL: usize = DrawCase::CONTROLLER;
const POOL0: usize = 4;

/// The attack base: a round 2 draw (K = 4), sales 157 KAS (5 KAS referral), the jackpot held
/// once (shard 1), one second-prize bonus held twice (shard 2, n2 = 1), another rank held in
/// shard 3 (a Z_0 leaf in a non-empty tree), one absorbed round 1 pool, fee paid (A = 2 KAS).
/// np = 5 (jackpot, second, three lower tiers).
fn base() -> DrawCase {
    let mut c = DrawCase::at_round(4, 2);
    c.set_sales(157 * KAS as i64, 5 * KAS as i64);
    c.hold_jackpot(1, 1);
    let b = c.other_bonus(0);
    c.hold(2, b, 2);
    let other = (c.seed().rank + 1) % RANKS;
    c.hold_at(3, other, 1, 1);
    c.absorb(1);
    let x = c.expected();
    assert_eq!((x.h_j, x.n2, x.np(), x.a), (1, 1, 5, 1));
    assert!(x.fee.paid);
    c
}

/// `base()` without the jackpot holding (h_J = 0).
fn base_no_jp() -> DrawCase {
    let mut c = DrawCase::at_round(4, 2);
    c.set_sales(157 * KAS as i64, 5 * KAS as i64);
    let b = c.other_bonus(0);
    c.hold(2, b, 2);
    c.absorb(1);
    assert_eq!(c.expected().h_j, 0);
    c
}

/// Accept the base (the last step of every attack test).
fn done(c: &DrawCase) {
    accept_draw(&c.build());
}

/// Output-side or argument attack: one edit of the `Built`, rejected by `input`.
fn hit(c: &DrawCase, input: usize, mutate: impl FnOnce(&mut Built)) {
    let b = c.build();
    let mut m = b.clone();
    mutate(&mut m);
    strike(&b, &m, input);
}

/// Input-side attack: `bad` differs from `good` in one case field.
fn pair(good: &DrawCase, bad: &DrawCase, input: usize) {
    strike(&good.build(), &bad.build(), input);
}

/// A consistent false claim (`Lie`), rejected by `input`.
fn lie(c: &DrawCase, input: usize, f: impl FnOnce(&mut Lie)) {
    let mut bad = c.clone();
    f(&mut bad.lie);
    pair(c, &bad, input);
}

fn args(c: &DrawCase, f: impl FnOnce(&mut Vec<ArtifactValue>)) {
    hit(c, DRW, |b| f(b.entry_args_mut(DRW)));
}

fn bytes_arg(v: &mut [ArtifactValue], i: usize) -> &mut Vec<u8> {
    match &mut v[i] {
        ArtifactValue::Bytes(b) => b,
        a => panic!("argument {i} is not bytes: {a:?}"),
    }
}

fn pool_out(c: &DrawCase, p: usize, f: impl FnOnce(&mut PoolState)) {
    let x = c.expected();
    let mut s = x.new_pools[p].clone();
    f(&mut s);
    let spk = tpl().pool_spk(&s);
    hit(c, DRW, |b| b.outputs[x.pool_output(p)].script_public_key = spk);
}

fn ctrl_out(c: &DrawCase, f: impl FnOnce(&mut ControllerState)) {
    let mut s = c.expected().controller;
    f(&mut s);
    let spk = tpl().controller_spk(&s);
    hit(c, DRW, |b| b.outputs[2].script_public_key = spk);
}

fn vault_out(c: &DrawCase, f: impl FnOnce(&mut VaultState)) {
    let mut s = c.expected().vault;
    f(&mut s);
    let spk = tpl().vault_spk(&s);
    hit(c, DRW, |b| b.outputs[1].script_public_key = spk);
}

fn drawer_out(c: &DrawCase, f: impl FnOnce(&mut DrawerState)) {
    let mut s = c.expected().drawer;
    f(&mut s);
    let spk = tpl().drawer_spk(&s);
    hit(c, DRW, |b| b.outputs[0].script_public_key = spk);
}

/// Move `delta` sompi between output `idx` and the caller output (last).
fn shift(c: &DrawCase, idx: usize, delta: i64) {
    hit(c, DRW, |b| b.shift_value(idx, delta));
}

// ---------------------------------------------------------------- DA1

/// DA1 (check 9): a wrong leaf among K. Block sizes: 2m = 20 count bytes and 576 sibling
/// bytes per shard.
#[test]
fn da1_wrong_leaf() {
    let c = base();
    let x = c.expected();
    let lb = 2 * c.marker.m as usize;
    let sb = 576;
    let bi = 2 * (x.bonus as usize - 1);
    let b2 = 2 * (c.other_bonus(0) as usize - 1);
    // A count + 1 / - 1 at the bonus (shard 1 holds it once) and elsewhere (shard 2).
    args(&c, |a| bytes_arg(a, ARG_LEAVES)[lb + bi] += 1);
    args(&c, |a| bytes_arg(a, ARG_LEAVES)[lb + bi] -= 1);
    args(&c, |a| bytes_arg(a, ARG_LEAVES)[2 * lb + b2] += 1);
    args(&c, |a| bytes_arg(a, ARG_LEAVES)[2 * lb + b2] -= 1);
    args(&c, |a| bytes_arg(a, ARG_LEAVES)[3 * lb] += 1);
    // Another rank's leaf with its own valid proof (shard 3 holds rank + 1).
    let other = (x.rank + 1) % RANKS;
    let (ol, os) = (c.trees[3].counts_bytes(other), c.trees[3].siblings(other));
    args(&c, |a| {
        bytes_arg(a, ARG_LEAVES)[3 * lb..4 * lb].copy_from_slice(&ol);
        bytes_arg(a, ARG_SIBLINGS)[3 * sb..4 * sb].copy_from_slice(&os);
    });
    // Z_0 (all-zero counts) for shard 1's non-empty leaf.
    args(&c, |a| bytes_arg(a, ARG_LEAVES)[lb..2 * lb].fill(0));
    // The proofs of shards 1 and 2 swapped (leaves and siblings).
    args(&c, |a| {
        let l = bytes_arg(a, ARG_LEAVES);
        let (s1, s2) = (l[lb..2 * lb].to_vec(), l[2 * lb..3 * lb].to_vec());
        l[lb..2 * lb].copy_from_slice(&s2);
        l[2 * lb..3 * lb].copy_from_slice(&s1);
        let s = bytes_arg(a, ARG_SIBLINGS);
        let (p1, p2) = (s[sb..2 * sb].to_vec(), s[2 * sb..3 * sb].to_vec());
        s[sb..2 * sb].copy_from_slice(&p2);
        s[2 * sb..3 * sb].copy_from_slice(&p1);
    });
    // k - 1 proofs.
    args(&c, |a| {
        bytes_arg(a, ARG_LEAVES).truncate(3 * lb);
        bytes_arg(a, ARG_SIBLINGS).truncate(3 * sb);
    });
    // A leaf length 2m + 2 and 2m - 2 (total length off by 2).
    args(&c, |a| bytes_arg(a, ARG_LEAVES).extend([0, 0]));
    args(&c, |a| {
        let l = bytes_arg(a, ARG_LEAVES);
        l.truncate(l.len() - 2);
    });
    // A sibling flipped.
    args(&c, |a| bytes_arg(a, ARG_SIBLINGS)[sb + 5] ^= 1);
    done(&c);
    // (Stage 4 addition, mutant gap) a forged leaf consistent everywhere but the root (check 9 root, D9_root).
    s4::forged_leaf().strike();
}

// ---------------------------------------------------------------- DA2

/// DA2 (checks 12, 13, 20): a wrong h_J or n2, carried consistently into the outputs.
#[test]
fn da2_wrong_counts() {
    let c = base();
    let nj = base_no_jp();
    // A jackpot pool when h_J = 0.
    lie(&nj, DRW, |l| l.counts = Some((1, 1)));
    // No jackpot pool when h_J >= 1.
    lie(&c, DRW, |l| l.counts = Some((0, 1)));
    // A second-prize pool of (n2 + 1) G2 and none ((n2 - 1) = 0).
    lie(&c, DRW, |l| l.counts = Some((1, 2)));
    lie(&c, DRW, |l| l.counts = Some((1, 0)));
    done(&c);
    done(&nj);
    // (Stage 4 addition, mutant gap) a genuine leaf holding a count of 32,768 (check 10 range, D10_range).
    s4::count_sign_bit().strike();
}

// ---------------------------------------------------------------- DA3

/// DA3 (checks 20, 23): the vault paid more than J_house [h_J >= 1] + n2 G2.
#[test]
fn da3_vault_overpaid() {
    let c = base();
    vault_out(&c, |v| v.total_assets -= 1);
    shift(&c, 1, -1);
    shift(&c, 1, 1);
    // The vault pays J_house although the jackpot is not held (consistent claim).
    let nj = base_no_jp();
    let jh = nj.marker.jackpot - nj.marker.j_boost;
    lie(&nj, DRW, |l| l.v_delta = -jh);
    done(&c);
    done(&nj);
    // (Stage 4 addition, mutant gap) the vault keeps J_house or n2 G2 although paid out (check 14 D14_jp, D14_sp); V' = -1 (D14_nonneg).
    s4::vault_keeps_jhouse().strike();
    s4::vault_keeps_second().strike();
    s4::vault_negative().strike();
}

// ---------------------------------------------------------------- DA4

/// Which of (edge, whale, cap) fail at (x, y), exactly (i128).
fn failing(rule: &JackpotRule, x: i64, y: i64) -> [bool; 3] {
    let (x, y) = (x as i128, y as i128);
    let l = (x + y) * rule.g as i128 / 10_000 * (rule.m as i128 - 1);
    [x + l > rule.e_lim as i128, x + y + l > rule.w_lim as i128, x + l > rule.cv as i128]
}

/// DA4 (A5, check 24): J(r+1) witnesses off by one; a J' failing only the edge rule, only
/// the whale check, only the cap; G2' or L' off by one.
#[test]
fn da4_jackpot_fixing() {
    let c = base();
    let x = c.expected();
    let (x1, y, x2) = (x.fix.x1, x.fix.y, x.fix.x2);
    // One fixture per binding constraint: the J' of (x1, y, x2 + 1) fails exactly it.
    // (Fixture facts are asserted before any script runs.)
    let regimes: [(&str, i64, u64, [bool; 3]); 3] = [
        ("edge", 10_000_000, 0, [true, false, false]),
        ("whale", 4_000_000, 600_000, [false, true, false]),
        ("cap", 2_000_000, 0, [false, false, true]),
    ];
    let mut cases = vec![];
    for (name, v, pot, want) in regimes {
        let mut r = base();
        r.vault.total_assets = v * KAS as i64;
        r.pot = pot * KAS;
        let rx = r.expected();
        assert_eq!(failing(&rx.rule, rx.fix.x2 + 1, rx.fix.y), want, "{name} regime: fix {:?} rule {:?}", rx.fix, rx.rule);
        cases.push((r, (rx.fix.x1, rx.fix.y, rx.fix.x2 + 1)));
    }
    for w in [(x1 + 1, y, x2), (x1 - 1, y, x2), (x1, y + 1, x2), (x1, y - 1, x2), (x1, y, x2 + 1), (x1, y, x2 - 1)] {
        lie(&c, DRW, |l| l.witness = Some(w));
    }
    for (r, w) in &cases {
        lie(r, DRW, |l| l.witness = Some(*w));
        done(r);
    }
    ctrl_out(&c, |s| s.g2 += 1);
    ctrl_out(&c, |s| s.g2 -= 1);
    ctrl_out(&c, |s| s.liability += 1);
    ctrl_out(&c, |s| s.liability -= 1);
    ctrl_out(&c, |s| s.jackpot += 1);
    ctrl_out(&c, |s| s.j_boost += 1);
    vault_out(&c, |v| v.liability += 1);
    vault_out(&c, |v| v.liability -= 1);
    done(&c);
    // (Stage 4 addition, mutant gap) each A5 witness branch with the other witnesses refit (check 17 D17_*).
    s4::x1_below_base().strike();
    s4::x1_at_failing_base().strike();
    s4::x1_failing().strike();
    s4::x1_not_largest().strike();
    s4::y_above_pot().strike();
    s4::y_failing().strike();
    s4::y_not_largest().strike();
    s4::x2_below_x1().strike();
    s4::x2_minus_one().strike();
}

// ---------------------------------------------------------------- DA5

/// DA5 (checks 20, 21): the fee redirected, off by one, paid below F_MIN, carried above it.
#[test]
fn da5_fee() {
    let c = base();
    let mut low = base();
    low.controller.acc = 0;
    low.set_sales(10 * KAS as i64, 0);
    assert!(!low.expected().fee.paid);
    let fi = c.expected().fee_output().unwrap();
    hit(&c, DRW, |b| b.outputs[fi].script_public_key = Payout::Schnorr([0x77; 32]).spk());
    shift(&c, fi, 1);
    shift(&c, fi, -1);
    // Paid although T_F < F_MIN.
    lie(&low, DRW, |l| l.paid = Some(true));
    // Carried although T_F >= F_MIN.
    lie(&c, DRW, |l| l.paid = Some(false));
    done(&c);
    done(&low);
}

// ---------------------------------------------------------------- DA6

/// DA6 (check 18, pool L5, check 20): absorption before the deadline; a pool of round >= r;
/// Q not added to O'.
#[test]
fn da6_absorption() {
    // Deadline after seed_ref, so lock_time = deadline - 1 still satisfies check 7.
    let mut good = base();
    good.absorbed[0].state.deadline = good.marker.seed_ref + 1_000;
    good.fix_lock_time();
    assert_eq!(good.lock_time as i64, good.marker.seed_ref + 1_000);
    let mut early = good.clone();
    early.lock_time -= 1;
    pair(&good, &early, POOL0); // pool L5
    // A pool of round r and of round r + 1 (pool L4: drawer round > pool round).
    for d in [0, 1] {
        let mut bad = base();
        bad.absorbed[0].state.round = bad.marker.round + d;
        pair(&base(), &bad, POOL0);
    }
    pair(&good, &early, DRW); // check 18
    for d in [0, 1] {
        let mut bad = base();
        bad.absorbed[0].state.round = bad.marker.round + d;
        pair(&base(), &bad, DRW);
    }
    // Q not added to O' (the controller keeps it out; the caller takes it).
    let c = base();
    let q = c.absorbed[0].remaining as i64;
    lie(&c, DRW, |l| l.o_delta = -q);
    done(&good);
}

// ---------------------------------------------------------------- DA7

/// DA7 (checks 20, 23): unheld J_boost or an unpaid lower-tier budget sent to the vault
/// instead of the boost pot.
#[test]
fn da7_pot_money_to_vault() {
    let nj = base_no_jp();
    let jb = nj.marker.j_boost;
    let mut small = base_no_jp();
    small.set_sales(50_000, 0);
    let x = small.expected();
    assert!(!x.pools.lp[2] && x.amounts.b[2] > 0);
    let b3 = x.amounts.b[2];
    lie(&nj, DRW, |l| {
        l.v_delta = jb;
        l.o_delta = -jb;
    });
    lie(&small, DRW, |l| {
        l.v_delta = b3;
        l.o_delta = -b3;
    });
    done(&nj);
    done(&small);
    // (Stage 4 addition, mutant gap) unheld J_boost or an unpaid B_t to the caller instead of the pot (check 15 D15_jboost, D15_lower).
    s4::jboost_to_caller().strike();
    s4::lower_to_caller().strike();
}

// ---------------------------------------------------------------- DA8

/// DA8 (checks 2, 13, 20, 21): shape attacks.
#[test]
fn da8_shape() {
    let c = base();
    let x = c.expected();
    let cov = c.cov;
    let r = c.reserve();
    // Parallel auth group: a pool output authorized by the marker (marker L3 first, then
    // check 13).
    hit(&c, MK, |b| b.outputs[3].covenant = Some(CovenantBinding::new(1, cov)));
    hit(&c, DRW, |b| b.outputs[3].covenant = Some(CovenantBinding::new(1, cov)));
    // An extra covenant output (a copy of the last, smallest pool) before the plain outputs.
    let first_plain = 3 + x.np();
    hit(&c, DRW, |b| {
        let o = b.outputs[first_plain - 1].clone();
        let last = b.outputs.len() - 1;
        b.outputs[last].value -= o.value;
        b.outputs.insert(first_plain, o);
    });
    // An extra plain output.
    hit(&c, DRW, |b| {
        let last = b.outputs.len() - 1;
        b.outputs[last].value -= KAS;
        b.outputs.push(TransactionOutput::new(KAS, spk_of(&[9u8; 32])));
    });
    // The caller output with a covenant ID (a fresh genesis ID authorized by the funding input).
    let (fi, co) = (c.funding_input(0) as u16, x.caller_output() as u32);
    hit(&c, DRW, |b| b.genesis = vec![GenesisCovenantGroup::new(fi, vec![co])]);
    // A funding input with a covenant ID.
    let fin = c.funding_input(0);
    hit(&c, DRW, |b| {
        let e = &b.inputs[fin].entry;
        b.inputs[fin].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, Some(OTHER_COV));
    });
    // Pools out of tier order.
    hit(&c, DRW, |b| b.outputs.swap(3, 4));
    // The marker refund missing (its R to the caller).
    let mr = x.marker_refund();
    hit(&c, DRW, |b| {
        b.outputs.remove(mr);
        let last = b.outputs.len() - 1;
        b.outputs[last].value += r;
    });
    // The pool refund missing.
    let pr = x.pool_refund(0);
    hit(&c, DRW, |b| {
        b.outputs.remove(pr);
        let last = b.outputs.len() - 1;
        b.outputs[last].value += r;
    });
    // No caller output.
    hit(&c, DRW, |b| {
        b.outputs.pop();
    });
    // a = A_MAX + 1 absorbed pools (A_MAX = 5 is valid).
    let mut five = DrawCase::at_round(4, 2);
    five.absorb(5);
    let mut six = five.clone();
    six.absorb(1);
    pair(&five, &six, DRW);
    // F_MAX + 1 = 9 funding inputs (8 is valid).
    let mut eight = base();
    eight.funding = vec![FundingIn { value: 15 * KAS, daa: eight.funding[0].daa }; 8];
    let mut nine = eight.clone();
    nine.funding.push(nine.funding[0]);
    pair(&eight, &nine, DRW);
    done(&c);
    done(&five);
    done(&eight);
    // (Stage 4 addition, mutant gap) a = -1 (controller without the ID), no funding input, the marker under another ID (check 2).
    s4::a_negative().strike();
    s4::no_funding().strike();
    s4::marker_other_cov().strike();
}

// ---------------------------------------------------------------- DA9

/// DA9 (checks 20, 21): refunds with a wrong script or R +- 1.
#[test]
fn da9_refunds() {
    let c = base();
    let x = c.expected();
    let other = Payout::Schnorr([0x78; 32]).spk();
    let (mr, pr) = (x.marker_refund(), x.pool_refund(0));
    let o1 = other.clone();
    hit(&c, DRW, |b| b.outputs[mr].script_public_key = o1);
    hit(&c, DRW, |b| b.outputs[pr].script_public_key = other);
    // The two refunds swapped (marker funder != pool funder here).
    hit(&c, DRW, |b| {
        let s = b.outputs[mr].script_public_key.clone();
        b.outputs[mr].script_public_key = b.outputs[pr].script_public_key.clone();
        b.outputs[pr].script_public_key = s;
    });
    for i in [mr, pr] {
        shift(&c, i, 1);
        shift(&c, i, -1);
    }
    done(&c);
    // (Stage 4 addition, mutant gap) an absorbed pool's funder class 3 (check 18 D18_class).
    s4::pool_class3().strike();
}

// ---------------------------------------------------------------- DA10

/// DA10 (checks 3-5, 18, 25 and the legs): wrong role, round or template of an input or a
/// new pool.
#[test]
fn da10_roles_rounds_templates() {
    let good = base();
    // Marker of round r - 1: marker L4 (drawer round) and controller L4 reject it.
    let mut bad = good.clone();
    bad.marker.round -= 1;
    pair(&good, &bad, MK);
    pair(&good, &bad, CTL);
    // Controller of round r + 2: controller L4 (marker round = round - 1).
    let mut ctl_round = good.clone();
    ctl_round.controller.round += 1;
    pair(&good, &ctl_round, CTL);
    // Controller already drawn: controller L5.
    let mut drawn = good.clone();
    drawn.controller.drawn = 1;
    pair(&good, &drawn, CTL);
    // Vault of round r: vault L4 (drawer round = vault round + 1).
    let mut vr = good.clone();
    vr.vault.round += 1;
    pair(&good, &vr, VLT);
    // The same input-side cases at the drawer (checks 3-5).
    pair(&good, &bad, DRW);
    pair(&good, &ctl_round, DRW);
    pair(&good, &drawn, DRW);
    pair(&good, &vr, DRW);
    // Look-alike pool template, input side (check 18): the absorbed pool under look-alike
    // code, spent through its permissive entry.
    let fake = lookalike(Sil::Pool);
    let ps = good.absorbed[0].state.clone();
    hit(&good, DRW, |b| {
        let e = b.inputs[POOL0].entry.clone();
        b.inputs[POOL0] = In {
            entry: UtxoEntry::new(e.amount, fake.pool_spk(&ps), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fake.pool.clone(), entry: "backdoor".into(), args: vec![], redeem: fake.pool_redeem(&ps) },
        };
    });
    // Look-alike pool template, output side (check 25), with the real and with the
    // look-alike's prefix, suffix and lengths.
    let np0 = good.expected().new_pools[0].clone();
    let spk = fake.pool_spk(&np0);
    let s1 = spk.clone();
    hit(&good, DRW, |b| b.outputs[3].script_public_key = s1);
    // (Stage 3 addition) The look-alike's prefix and suffix with the real template lengths:
    // pool_pl/pool_sl also locate the absorbed pool's template (check 18), so the variant
    // below that changes them is caught there first; this one reaches check 25.
    let s2 = spk.clone();
    hit(&good, DRW, |b| {
        b.outputs[3].script_public_key = s2;
        let a = b.entry_args_mut(DRW);
        a[ARG_POOL_PREFIX] = ArtifactValue::Bytes(fake.pool.prefix.clone());
        a[ARG_POOL_SUFFIX] = ArtifactValue::Bytes(fake.pool.suffix.clone());
    });
    hit(&good, DRW, |b| {
        b.outputs[3].script_public_key = spk;
        let a = b.entry_args_mut(DRW);
        a[ARG_POOL_PL] = ArtifactValue::Int(fake.pool.prefix.len() as i64);
        a[ARG_POOL_PL + 1] = ArtifactValue::Int(fake.pool.suffix.len() as i64);
        a[ARG_POOL_PREFIX] = ArtifactValue::Bytes(fake.pool.prefix.clone());
        a[ARG_POOL_SUFFIX] = ArtifactValue::Bytes(fake.pool.suffix.clone());
    });
    // Look-alike controller template at input 3 (check 3).
    let fc = lookalike(Sil::Controller);
    let cs = good.controller.clone();
    hit(&good, DRW, |b| {
        let e = b.inputs[CTL].entry.clone();
        b.inputs[CTL] = In {
            entry: UtxoEntry::new(e.amount, fc.controller_spk(&cs), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fc.controller.clone(), entry: "backdoor".into(), args: vec![], redeem: fc.controller_redeem(&cs) },
        };
    });
    done(&good);
    // (Stage 4 addition, mutant gap) role bytes, the marker's round and k fields alone, m above M_DRAW (checks 3-5, 18).
    s4::ctrl_role().strike();
    s4::ctrl_m_above().strike();
    s4::marker_role().strike();
    s4::marker_round_field().strike();
    s4::marker_k().strike();
    s4::marker_m_above().strike();
    s4::vault_role().strike();
    s4::pool_role().strike();
}

// ---------------------------------------------------------------- DA11

/// DA11 (L1-L5): delegates under a look-alike leader; a leg at the wrong index; a leg
/// authorizing an output; a leader of another covenant.
#[test]
fn da11_legs() {
    let c = base();
    let cov = c.cov;
    // A look-alike drawer at input 0 (same state, permissive entry); the legs get its
    // template lengths. Marker, vault and pool L4 reject.
    let fake = lookalike(Sil::Drawer);
    let ds = c.drawer.clone();
    let fleg = DrawCase::drawer_leg_args(&fake);
    let fake_leader = |b: &mut Built| {
        let e = b.inputs[0].entry.clone();
        b.inputs[0] = In {
            entry: UtxoEntry::new(e.amount, fake.drawer_spk(&ds), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fake.drawer.clone(), entry: "backdoor".into(), args: vec![], redeem: fake.drawer_redeem(&ds) },
        };
        for i in [MK, VLT, POOL0] {
            *b.entry_args_mut(i) = fleg.clone();
        }
    };
    for i in [MK, VLT, POOL0] {
        hit(&c, i, fake_leader);
    }
    // Controller L4: a look-alike marker at input 1 (its permissive entry), the real drawer.
    let fm = lookalike(Sil::Marker);
    let ms = c.marker.clone();
    hit(&c, CTL, |b| {
        let e = b.inputs[MK].entry.clone();
        b.inputs[MK] = In {
            entry: UtxoEntry::new(e.amount, fm.marker_spk(&ms), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fm.marker.clone(), entry: "backdoor".into(), args: vec![], redeem: fm.marker_redeem(&ms) },
        };
        *b.entry_args_mut(CTL) = DrawCase::controller_leg_args(&fm);
    });
    // L1: legs at the wrong index (inputs swapped).
    hit(&c, 2, |b| b.inputs.swap(MK, VLT)); // marker at 2
    hit(&c, 1, |b| b.inputs.swap(MK, VLT)); // vault at 1
    hit(&c, 3, |b| b.inputs.swap(CTL, POOL0)); // pool at 3
    hit(&c, 4, |b| b.inputs.swap(CTL, POOL0)); // controller at 4
    // L3: a leg authorizing an output.
    hit(&c, MK, |b| b.outputs[3].covenant = Some(CovenantBinding::new(MK as u16, cov)));
    hit(&c, VLT, |b| b.outputs[1].covenant = Some(CovenantBinding::new(VLT as u16, cov)));
    hit(&c, CTL, |b| b.outputs[2].covenant = Some(CovenantBinding::new(CTL as u16, cov)));
    hit(&c, POOL0, |b| b.outputs[4].covenant = Some(CovenantBinding::new(POOL0 as u16, cov)));
    // L2: the real drawer at input 0 under another covenant ID (its outputs carry that ID).
    let other_leader = |b: &mut Built| {
        let e = b.inputs[0].entry.clone();
        b.inputs[0].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, Some(OTHER_COV));
        for o in b.outputs.iter_mut().filter(|o| o.covenant.is_some()) {
            o.covenant = Some(CovenantBinding::new(0, OTHER_COV));
        }
    };
    for i in [MK, VLT, CTL, POOL0] {
        hit(&c, i, other_leader);
    }
    done(&c);
    // (Stage 4 addition, mutant gap) a real-template drawer (or marker) with a wrong role byte (L4 role).
    s4::drawer_role(MK).strike();
    s4::drawer_role(VLT).strike();
    s4::drawer_role(POOL0).strike();
    s4::marker_role().at(CTL).strike();
}

// ---------------------------------------------------------------- DA12

/// DA12 (checks 7, 8): the winning numbers.
#[test]
fn da12_seed() {
    let c = base();
    let x = c.expected();
    // win5 of another rank, with consistent proofs and pools for that rank.
    lie(&c, DRW, |l| l.rank = Some((x.rank + 1) % RANKS));
    // Non-canonical win5 (reversed order; pools carry it).
    let mut rev = x.win5;
    rev.reverse();
    lie(&c, DRW, |l| l.win5 = Some(rev));
    // Only the win5 argument changed.
    args(&c, |a| bytes_arg(a, ARG_WIN5)[4] ^= 1);
    // A bonus other than the seed's (h_J, n2 and the pools follow it).
    let ob = c.other_bonus(0);
    lie(&c, DRW, |l| l.bonus = Some(ob));
    // Another test seed with the same outputs.
    args(&c, |a| bytes_arg(a, ARG_SEED)[0] ^= 1);
    // lock_time = seed_ref - 1 (the absorbed pool's deadline is earlier).
    let mut early = c.clone();
    early.lock_time = c.marker.seed_ref as u64 - 1;
    assert!(early.absorbed[0].state.deadline as u64 <= early.lock_time);
    pair(&c, &early, DRW);
    done(&c);
    // (Stage 4 addition, mutant gap) a non-canonical win5 with the seed's colex rank; a canonical win5 of another rank with genuine proofs (check 8).
    s4::win5_noncanonical().strike();
    s4::win5_other_rank().strike();
}

// ---------------------------------------------------------------- DA13

/// DA13 (checks 20, 25): each new pool's state and value.
#[test]
fn da13_new_pool_state() {
    let c = base();
    let x = c.expected();
    for p in 0..x.np() {
        pool_out(&c, p, |s| s.slice += 1);
        pool_out(&c, p, |s| s.slice -= 1);
        pool_out(&c, p, |s| s.tier = (s.tier + 1) % 7);
        pool_out(&c, p, |s| s.win[0] ^= 1);
        pool_out(&c, p, |s| s.win[5] ^= 1);
        pool_out(&c, p, |s| s.m += 1);
        pool_out(&c, p, |s| s.k -= 1);
        pool_out(&c, p, |s| s.roots[33] ^= 1);
        pool_out(&c, p, |s| s.round += 1);
        pool_out(&c, p, |s| s.deadline += 1);
        pool_out(&c, p, |s| s.deadline -= 1);
        pool_out(&c, p, |s| s.set_funder(&Payout::Schnorr([0x79; 32])));
        shift(&c, x.pool_output(p), 1);
        shift(&c, x.pool_output(p), -1);
    }
    // The funder argument and the pool field changed together to a class-3 / bad padding.
    let mut f3 = x.new_pools[0].clone();
    f3.funder_class = 3;
    let spk3 = tpl().pool_spk(&f3);
    hit(&c, DRW, |b| {
        b.entry_args_mut(DRW)[ARG_FUNDER_CLASS] = ArtifactValue::Byte(3);
        b.outputs[3].script_public_key = spk3;
    });
    let mut fp = x.new_pools[0].clone();
    fp.funder_key[32] = 1;
    let spkp = tpl().pool_spk(&fp);
    hit(&c, DRW, |b| {
        b.entry_args_mut(DRW)[ARG_FUNDER_KEY] = ArtifactValue::Bytes(fp.funder_key.to_vec());
        b.outputs[3].script_public_key = spkp;
    });
    done(&c);
    // (Stage 4 addition, mutant gap) only old inputs: D_claim must be seed_ref + DRAW_WINDOW + CLAIM_WINDOW (check 19 D19_max).
    s4::deadline_without_draw_window().strike();
}

// ---------------------------------------------------------------- DA14

/// DA14 (checks 22-24): successor states.
#[test]
fn da14_successors() {
    let c = base();
    ctrl_out(&c, |s| s.drawn = 0);
    ctrl_out(&c, |s| s.v_rec += 1);
    ctrl_out(&c, |s| s.acc += 1);
    ctrl_out(&c, |s| s.round += 1);
    ctrl_out(&c, |s| s.m += 1);
    ctrl_out(&c, |s| s.price += 1);
    ctrl_out(&c, |s| s.k -= 1);
    ctrl_out(&c, |s| s.deadline += 1);
    ctrl_out(&c, |s| s.s_f += 1);
    ctrl_out(&c, |s| s.fee_key[0] ^= 1);
    ctrl_out(&c, |s| s.marker_tpl[0] ^= 1);
    ctrl_out(&c, |s| s.shard_tpl[0] ^= 1);
    ctrl_out(&c, |s| s.cap += 1);
    ctrl_out(&c, |s| s.reserve += 1);
    drawer_out(&c, |s| s.round -= 1);
    drawer_out(&c, |s| s.round += 1);
    drawer_out(&c, |s| s.pool_tpl[0] ^= 1);
    drawer_out(&c, |s| s.vault_tpl[0] ^= 1);
    drawer_out(&c, |s| s.f_min += 1);
    drawer_out(&c, |s| s.claim_window += 1);
    drawer_out(&c, |s| s.draw_window += 1);
    vault_out(&c, |v| v.total_shares += 1);
    vault_out(&c, |v| v.pend_dep += 1);
    vault_out(&c, |v| v.pend_wsh += 1);
    vault_out(&c, |v| v.round += 1);
    // Values of the three singletons.
    for i in 0..3 {
        shift(&c, i, 1);
        shift(&c, i, -1);
    }
    done(&c);
}

// ---------------------------------------------------------------- DA15

/// DA15: the leader's committed budget one unit short.
#[test]
fn da15_leader_budget_minus_one() {
    let built = base().build();
    accept_draw(&built);
    let mut b = built.clone();
    b.budget_delta = -1;
    let e = b.finish().run_input(DRW).expect_err("under-budget drawer accepted");
    assert!(matches!(e, TxScriptError::ExceededCommittedScriptUnits { .. }), "expected a budget error, got {e:?}");
}

// ---------------------------------------------------------------- DA16

/// DA16 (checks 4-6): input values off by one.
#[test]
fn da16_input_values() {
    let c = base();
    for i in [MK, VLT] {
        hit(&c, DRW, |b| b.inputs[i].entry.amount += 1);
        hit(&c, DRW, |b| b.inputs[i].entry.amount -= 1);
    }
    hit(&c, DRW, |b| b.inputs[DRW].entry.amount += 1); // check 6: drawer value = R
    done(&c);
    // (Stage 4 addition, mutant gap) the controller worth acc + R - 1, O = -1 (check 6 D6_pot).
    s4::pot_negative().strike();
}

// ---------------------------------------------------------------- DA17 own argument

/// The three false `own` claims of DA17 on `c`: one below the true maximum creation DAA
/// score, one above, and a non-maximum input's (the drawer's) DAA score.
fn own_lies(c: &DrawCase) -> [i64; 3] {
    let own = c.own() as i64;
    assert_ne!(c.drawer_daa as i64, own, "the drawer input is not the newest input");
    [own - 1, own + 1, c.drawer_daa as i64]
}

/// DA17 (check 19, user decision 2026-10-10: `own` is an explicit argument that must equal
/// the maximum OpTxInputDaaScore over all inputs): a consistent false claim (the new pools'
/// D_claim follows it) one too low, one too high, or a non-maximum input's DAA.
#[test]
fn da17_own_argument() {
    let c = base();
    let d0 = c.expected().d_claim;
    for o in own_lies(&c) {
        let mut bad = c.clone();
        bad.lie.own = Some(o);
        assert_ne!(bad.expected().d_claim, d0, "own {o}: the lie must move D_claim");
        pair(&c, &bad, DRW);
    }
    done(&c);
}

/// Point a draw case at templates `t` (a drawer mutant changes the marker, vault and pool
/// hashes): the drawer's pool_tpl/vault_tpl and the controller's marker_tpl.
fn retarget(mut c: DrawCase, t: &lottery::buy::Templates) -> DrawCase {
    c.drawer.pool_tpl = t.pool.hash;
    c.drawer.vault_tpl = t.vault.hash;
    c.controller.marker_tpl = t.marker.hash;
    c
}

/// DA17 negative control: with the check 19 equality removed (mutant `D19_own`), every DA17
/// claim is accepted by every input, with D_claim moved by the claim. Restored = the
/// committed source (the mutant is compiled in memory only).
#[test]
fn da17_negative_control_without_own_check() {
    let t = mutant(Sil::Drawer, &["D19_own"]);
    let c = retarget(base(), &t);
    let d0 = c.expected().d_claim;
    accept_draw(&c.build_with(t.clone()));
    for o in own_lies(&c) {
        let mut bad = c.clone();
        bad.lie.own = Some(o);
        assert_ne!(bad.expected().d_claim, d0);
        accept_draw(&bad.build_with(t.clone()));
    }
}

// ---------------------------------------------------------------- A6 G2 guard

/// A6 G2 guard, user decision (2026-10-10): with a second-prize pool (n2 > 0), G2 >
/// TOTAL_MAX / 254 is rejected; G2 = TOTAL_MAX / 254 is accepted. The vault is large enough
/// that V' >= 0 either way. (n2 = 0 with G2 above the bound is accepted: m3_draw.rs.)
#[test]
fn a6_g2_guard_with_second_pool() {
    let mut good = base();
    good.vault.total_assets = 20_000_000_000_000_000;
    good.marker.g2 = TOTAL_MAX / 254;
    let mut bad = good.clone();
    bad.marker.g2 += 1;
    assert_eq!(bad.expected().n2, 1);
    assert!(bad.expected().pools.sp);
    pair(&good, &bad, DRW);
    done(&good);
}
