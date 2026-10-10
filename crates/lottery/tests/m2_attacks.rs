//! M2 attacks on `close()` and its delegate legs: CA1-CA15 (spec M2 A7).
//!
//! Each test starts from a valid close (`common::m2::close_base`, or a stated variant),
//! changes exactly one thing, and requires a *script* error from the named input
//! (`attack_at` / `attack_pair`: not a budget error). The comment on each attack names the
//! A4 check it targets. Input 0 is the controller (leader) unless stated otherwise.

mod common;

use common::accept;
use common::m2::*;
use harness::{ArtifactValue, CovenantBinding, TransactionOutput, TxScriptError, UtxoEntry};
use kaspa_consensus_core::tx::GenesisCovenantGroup;
use lottery::buy::{Built, In, InKind, KAS, push_redeem, spk_of};
use lottery::close::{CloseCase, FundingIn, TEST_DEADLINE, TEST_ROUND_LEN, z18};
use lottery::combo::Combo;
use lottery::ledger::LedgerError;
use lottery::params::SetParamsCase;
use lottery::state::{ControllerState, MarkerState, ParamsState, Payout, ShardState, TicketState};

const CTRL: usize = CloseCase::CONTROLLER;

fn base() -> CloseCase {
    close_base(&tpl())
}

/// Output-side attack on input 0.
fn out_attack(c: &CloseCase, mutate: impl FnOnce(&mut Built)) {
    attack_at(&c.build(), CTRL, mutate);
}

/// Input-side attack: `bad` differs from `good` in one case field.
fn case_attack(good: &CloseCase, bad: &CloseCase) {
    attack_pair(&good.build(), &bad.build(), CTRL);
}

fn marker_attack(c: &CloseCase, f: impl FnOnce(&mut MarkerState)) {
    let mut m = c.expected().marker;
    f(&mut m);
    let spk = tpl().marker_spk(&m);
    let i = c.marker_output();
    out_attack(c, |b| b.outputs[i].script_public_key = spk);
}

fn controller_attack(c: &CloseCase, f: impl FnOnce(&mut ControllerState)) {
    let mut s = c.expected().controller;
    f(&mut s);
    let spk = tpl().controller_spk(&s);
    out_attack(c, |b| b.outputs[0].script_public_key = spk);
}

fn params_attack(c: &CloseCase, f: impl FnOnce(&mut ParamsState)) {
    let mut s = c.expected().params;
    f(&mut s);
    let spk = tpl().params_spk(&s);
    out_attack(c, |b| b.outputs[1].script_public_key = spk);
}

fn shard_attack(c: &CloseCase, i: usize, f: impl FnOnce(&mut ShardState)) {
    let mut s = c.expected().shards[i].clone();
    f(&mut s);
    let spk = tpl().shard_spk(&s);
    out_attack(c, |b| b.outputs[2 + i].script_public_key = spk);
}

/// Move `delta` sompi between output `idx` and the caller output (last).
fn shift(c: &CloseCase, idx: usize, delta: i64) {
    out_attack(c, |b| b.shift_value(idx, delta));
}

// ---------------------------------------------------------------- CA1, CA2

#[test]
fn ca1_lock_time_before_deadline() {
    // Check 7: tx.daa >= T (CLTV against the transaction lock time).
    let good = base();
    let mut bad = good.clone();
    bad.lock_time = TEST_DEADLINE as u64 - 1;
    case_attack(&good, &bad);
}

#[test]
fn ca2_round_not_drawn() {
    // Check 8: drawn = 1.
    let good = base();
    let mut bad = good.clone();
    bad.controller.drawn = 0;
    case_attack(&good, &bad);
}

// ---------------------------------------------------------------- CA3

#[test]
fn ca3_late_buy_on_old_shard_ledger() {
    // Ledger part (D33): after close() spends shard 1, a buy that spends the old shard
    // outpoint is rejected by the UTXO set. The buy's own scripts would accept it (its
    // shard DAA is before the deadline): the cut-off is structural, not a script check.
    let mut ch = Chain::genesis(4);
    let (late, late_ops) = ch.buy_case(1);
    let old = late_ops[0];
    let (c, ops) = ch.close_case(900_000);
    ch.close(&c, ops, TEST_DEADLINE as u64 + 10);
    let f = accept(&ch.buy_built(&late, late_ops));
    assert_eq!(ch.ledger.apply(&f.tx, &f.entries, TEST_DEADLINE as u64 + 20), Err(LedgerError::Missing { index: 0, outpoint: old }));
}

#[test]
fn ca3_round_r_shard_in_round_r_plus_1_close() {
    // Check 10 (round): a round-1 shard in round 2's close.
    let mut good = base();
    good.controller.round = 2;
    good.params.round = 2;
    for s in &mut good.shards {
        s.state.round = 2;
    }
    let mut bad = good.clone();
    bad.shards[1].state.round = 1;
    case_attack(&good, &bad);
}

// ---------------------------------------------------------------- CA4

fn with_m(c: &CloseCase, m: i64) -> CloseCase {
    let mut b = c.clone();
    b.m_next = Some(m);
    b
}

#[test]
fn ca4_wrong_m_next() {
    // Check 19 (D25 closed form; the bad m is carried consistently into controller' and
    // the new shards, so only check 19, or the range part of check 2/19, can reject).
    let good = base();
    let x = good.expected();
    let (m, lo, hi) = (x.m_next, good.controller.m_min, good.controller.m_max);
    assert!(lo < m && m < hi, "base reference m is interior: {m}");
    assert!(x.math.rule.pass(m));
    for bad in [m - 1, m + 1, lo - 1, hi + 1, hi] {
        case_attack(&good, &with_m(&good, bad));
    }
}

#[test]
fn ca4_boost_dominated_m_max_rejected() {
    // Check 19: e = 5,000, g = 1,000, boost 539,000 KAS, no house estimate: reference m = 12;
    // m_max = 50 (which the old witness form also accepted) is rejected.
    let mut good = CloseCase::new(4);
    good.controller.e = 5_000;
    good.controller.g = 1_000;
    good.controller.v_rec = 0;
    good.controller.j_boost = 0;
    good.pot = 539_000 * KAS;
    let x = good.expected();
    assert_eq!(x.m_next, 12);
    assert!(!x.math.rule.pass(50));
    for bad in [11, 13, 50] {
        case_attack(&good, &with_m(&good, bad));
    }
}

// ---------------------------------------------------------------- CA5: marker

#[test]
fn ca5_marker_fields() {
    // Check 24: every marker field is validated.
    let c = base();
    marker_attack(&c, |m| m.sales += 1);
    marker_attack(&c, |m| m.sales -= 1);
    marker_attack(&c, |m| m.ref_paid += 1);
    marker_attack(&c, |m| m.ref_paid -= 1);
    marker_attack(&c, |m| {
        let (a, b) = (m.roots[..32].to_vec(), m.roots[64..96].to_vec());
        m.roots[..32].copy_from_slice(&b);
        m.roots[64..96].copy_from_slice(&a);
    });
    marker_attack(&c, |m| m.roots[32 * 4] = 1); // a non-zero root slot i >= k
    marker_attack(&c, |m| m.k += 1);
    marker_attack(&c, |m| m.round += 1);
    marker_attack(&c, |m| m.m += 1);
    marker_attack(&c, |m| m.jackpot += 1);
    marker_attack(&c, |m| m.j_boost += 1);
    marker_attack(&c, |m| m.g2 += 1);
    marker_attack(&c, |m| m.s_f += 1);
    marker_attack(&c, |m| m.s_r += 1);
    marker_attack(&c, |m| m.s_c += 1);
    marker_attack(&c, |m| m.s_d += 1);
    marker_attack(&c, |m| m.st[0] += 1);
    marker_attack(&c, |m| m.st[4] += 1);
    marker_attack(&c, |m| m.seed_ref += 1);
    marker_attack(&c, |m| m.seed_ref -= 1);
    marker_attack(&c, |m| m.set_funder(&Payout::Schnorr([0x66; 32])));
    marker_attack(&c, |m| m.funder_class = 2);
}

#[test]
fn ca5_marker_value() {
    // Check 16: marker value = S - P_R - C_close + J_boost + R.
    let c = base();
    shift(&c, c.marker_output(), 1);
    shift(&c, c.marker_output(), -1);
}

// ---------------------------------------------------------------- CA6: controller'

#[test]
fn ca6_controller_successor_value() {
    // Check 13: out0 = V_c - J_boost.
    let c = base();
    shift(&c, 0, 1);
    shift(&c, 0, -1);
}

#[test]
fn ca6_controller_successor_fields() {
    // Check 21.
    let c = base();
    let x = c.expected();
    controller_attack(&c, |s| s.acc += 1);
    controller_attack(&c, |s| s.v_rec += 1);
    controller_attack(&c, |s| s.round += 1);
    controller_attack(&c, |s| s.m += 1);
    controller_attack(&c, |s| s.deadline += 1);
    controller_attack(&c, |s| s.deadline -= 1);
    controller_attack(&c, |s| s.drawn = 1);
    controller_attack(&c, |s| s.jackpot = c.controller.jackpot);
    controller_attack(&c, |s| s.j_boost = c.controller.j_boost);
    controller_attack(&c, |s| s.g2 = c.controller.g2);
    controller_attack(&c, |s| s.liability = c.controller.liability);
    // Route table.
    controller_attack(&c, |s| s.shard_tpl[0] ^= 1);
    controller_attack(&c, |s| s.marker_tpl[0] ^= 1);
    controller_attack(&c, |s| s.params_tpl[0] ^= 1);
    // Immutables (D17).
    controller_attack(&c, |s| s.k += 1);
    controller_attack(&c, |s| s.price += 1);
    controller_attack(&c, |s| s.d += 1);
    controller_attack(&c, |s| s.reserve += 1);
    controller_attack(&c, |s| s.s_c += 1);
    controller_attack(&c, |s| s.s_d += 1);
    controller_attack(&c, |s| s.st[2] += 1);
    controller_attack(&c, |s| s.e += 1);
    controller_attack(&c, |s| s.j += 1);
    controller_attack(&c, |s| s.g += 1);
    controller_attack(&c, |s| s.margin += 1);
    controller_attack(&c, |s| s.m_min += 1);
    controller_attack(&c, |s| s.m_max += 1);
    controller_attack(&c, |s| s.cap += 1);
    controller_attack(&c, |s| s.round_len += 1);
    controller_attack(&c, |s| s.d_min += 1);
    controller_attack(&c, |s| s.seed_offset += 1);
    assert_eq!(x.controller.drawn, 0);
}

#[test]
fn ca6_params_not_applied() {
    // Check 21: s_f', s_r' and the fee address come from the params.
    let mut c = base();
    c.params.next_s_f = 700;
    c.params.next_s_r = 1_200;
    c.params.set_next_fee(&Payout::Schnorr([0x55; 32]));
    let old = c.controller.clone();
    controller_attack(&c, |s| s.s_f = old.s_f);
    controller_attack(&c, |s| s.s_r = old.s_r);
    controller_attack(&c, |s| s.set_fee(&lottery::close::TEST_FEE_ADDR));
}

// ---------------------------------------------------------------- CA7: params'

#[test]
fn ca7_params_successor() {
    // Check 22 (state) and 14 (values).
    let c = base();
    params_attack(&c, |p| p.used = 1);
    params_attack(&c, |p| p.round = c.params.round);
    params_attack(&c, |p| p.next_s_f += 1);
    params_attack(&c, |p| p.next_s_r += 1);
    params_attack(&c, |p| p.next_fee_key[0] ^= 1);
    params_attack(&c, |p| p.nonce += 1);
    params_attack(&c, |p| p.admin_hash[0] ^= 1);
    shift(&c, 1, 1);
    shift(&c, 1, -1);
    // Check 14: the params input value must be R.
    out_attack(&c, |b| b.inputs[1].entry.amount += 1);
}

// ---------------------------------------------------------------- CA8: new shards

#[test]
fn ca8_new_shard() {
    // Check 23 (state) and 15 (value R).
    let c = base();
    shard_attack(&c, 1, |s| s.root = [0x10; 32]);
    shard_attack(&c, 1, |s| s.sales = 1);
    shard_attack(&c, 1, |s| s.ref_paid = 1);
    shard_attack(&c, 1, |s| s.m += 1);
    shard_attack(&c, 1, |s| s.s_r += 1);
    shard_attack(&c, 1, |s| s.deadline += 1);
    shard_attack(&c, 1, |s| s.shard_index = 2);
    shard_attack(&c, 1, |s| s.round += 1);
    shard_attack(&c, 1, |s| s.price += 1);
    shard_attack(&c, 1, |s| s.d += 1);
    shift(&c, 3, 1);
    shift(&c, 3, -1);
    assert_eq!(c.expected().shards[1].root, z18());
}

// ---------------------------------------------------------------- CA9: shape

#[test]
fn ca9_k_minus_1_shards() {
    // Check 3: nIn = k + 2 + n.
    let good = base();
    let mut bad = good.clone();
    bad.shards.pop();
    case_attack(&good, &bad);
}

#[test]
fn ca9_shards_out_of_order() {
    // Check 10 (index): shard i must sit at input 2 + i.
    let good = base();
    let mut bad = good.clone();
    bad.shards.swap(0, 1);
    case_attack(&good, &bad);
}

#[test]
fn ca9_extra_covenant_output() {
    // Checks 5-6: an extra output authorized by input 0 before the caller output.
    let c = base();
    let cov = c.cov;
    out_attack(&c, |b| {
        let spk = b.outputs[2].script_public_key.clone();
        let last = b.outputs.len() - 1;
        b.outputs[last].value -= c.reserve();
        b.outputs.insert(last, TransactionOutput::with_covenant(c.reserve(), spk, Some(CovenantBinding::new(0, cov))));
    });
}

#[test]
fn ca9_extra_plain_output() {
    // Check 6: exactly k + 4 outputs.
    out_attack(&base(), |b| {
        let last = b.outputs.len() - 1;
        b.outputs[last].value -= KAS;
        b.outputs.push(TransactionOutput::new(KAS, spk_of(&[9u8; 32])));
    });
}

#[test]
fn ca9_caller_output_with_covenant_id() {
    // Check 6: output k + 3 has the zero ID (here a genesis output of a fresh ID authorized
    // by the funding input).
    let c = base();
    let (fi, co) = (c.funding_input(0) as u16, c.caller_output() as u32);
    out_attack(&c, |b| b.genesis = vec![GenesisCovenantGroup::new(fi, vec![co])]);
}

#[test]
fn ca9_no_caller_output() {
    // Check 6.
    out_attack(&base(), |b| {
        b.outputs.pop();
    });
}

#[test]
fn ca9_parallel_auth_group() {
    // Check 5: the marker output authorized by the params input (a second auth group).
    let c = base();
    let (i, cov) = (c.marker_output(), c.cov);
    out_attack(&c, |b| b.outputs[i].covenant = Some(CovenantBinding::new(1, cov)));
}

#[test]
fn ca9_funding_input_with_covenant_id() {
    // Check 3: inputs >= k + 2 have the zero ID.
    let c = base();
    let fi = c.funding_input(0);
    out_attack(&c, |b| {
        let e = &b.inputs[fi].entry;
        b.inputs[fi].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, Some(OTHER_COV));
    });
}

#[test]
fn ca9_too_many_funding_inputs() {
    // Check 3: 1 <= n <= F_MAX (F_MAX = 8 passes in CP3).
    let good = base();
    let mut bad = good.clone();
    bad.funding = vec![FundingIn { value: 15 * KAS, daa: 900_000 }; 9];
    case_attack(&good, &bad);
}

// ---------------------------------------------------------------- CA10: wrong objects

#[test]
fn ca10_ticket_in_shard_slot() {
    // Check 10 (template): a ticket of the same family at input 3.
    let c = base();
    let t = tpl();
    let ticket = TicketState::new(&c.shards[1].state, &[Combo::new([1, 2, 3, 4, 5], 1)], &Payout::Schnorr([1; 32]), t.b_max);
    let redeem = t.ticket.redeem(&ticket.values());
    let sig = push_redeem(silverscript_abi::encode_contract_entry_sig_script(&t.ticket.art, t.ticket.name(), "locked", &[]).unwrap(), &redeem);
    let cov = c.cov;
    out_attack(&c, |b| {
        let amount = b.inputs[3].entry.amount;
        b.inputs[3] = In { entry: UtxoEntry::new(amount, harness::pay_to_script_hash_script(&redeem), 5_000, false, Some(cov)), kind: InKind::Raw(sig) };
    });
}

#[test]
fn ca10_shard_in_params_slot() {
    // Check 9 (template): a shard at input 1.
    let c = base();
    let t = tpl();
    let s = c.shards[1].state.clone();
    let cov = c.cov;
    out_attack(&c, |b| {
        let amount = b.inputs[1].entry.amount;
        b.inputs[1] = In {
            entry: UtxoEntry::new(amount, t.shard_spk(&s), 5_000, false, Some(cov)),
            kind: InKind::Entry { template: t.shard.clone(), entry: "close_leg".into(), args: CloseCase::leg_args(&t), redeem: t.shard_redeem(&s) },
        };
    });
}

#[test]
fn ca10_lookalike_shard_input() {
    // Check 10 (template): same state under look-alike shard code.
    let c = base();
    let fake = lookalike(Sil::Shard);
    let s = c.shards[1].state.clone();
    out_attack(&c, |b| {
        let e = b.inputs[3].entry.clone();
        b.inputs[3] = In {
            entry: UtxoEntry::new(e.amount, fake.shard_spk(&s), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fake.shard.clone(), entry: "backdoor".into(), args: vec![], redeem: fake.shard_redeem(&s) },
        };
    });
}

#[test]
fn ca10_lookalike_marker() {
    // Check 24 (template): the marker under look-alike code, with the real and with the
    // look-alike's prefix and suffix arguments.
    let c = base();
    let fake = lookalike(Sil::Marker);
    let m = c.expected().marker;
    let i = c.marker_output();
    out_attack(&c, |b| b.outputs[i].script_public_key = fake.marker_spk(&m));
    out_attack(&c, |b| {
        b.outputs[i].script_public_key = fake.marker_spk(&m);
        b.entry_args_mut(0)[7] = ArtifactValue::Bytes(fake.marker.prefix.clone());
        b.entry_args_mut(0)[8] = ArtifactValue::Bytes(fake.marker.suffix.clone());
    });
}

// ---------------------------------------------------------------- CA11

#[test]
fn ca11_shard_input_value() {
    // Check 10: input value = S_i - P_R,i + R.
    let c = base();
    out_attack(&c, |b| b.inputs[2].entry.amount += 1);
    out_attack(&c, |b| b.inputs[2].entry.amount -= 1);
    out_attack(&c, |b| b.inputs[3].entry.amount += 1);
}

// ---------------------------------------------------------------- CA12: T' and seed

#[test]
fn ca12_deadline_rule() {
    // Check 20/21: T' = max(T + round_len, own + d_min).
    // Schedule case (own + d_min < T + round_len): T' = own + d_min is rejected.
    let c = base();
    let x = c.expected();
    assert_eq!(x.math.deadline, TEST_DEADLINE + TEST_ROUND_LEN);
    controller_attack(&c, |s| s.deadline = x.math.own + c.controller.d_min);
    // Late case: T' = T + round_len is rejected (controller' and, separately, a new shard).
    let mut late = base();
    late.funding[0].daa = TEST_DEADLINE as u64 + 2_000_000;
    let lx = late.expected();
    assert_eq!(lx.math.deadline, lx.math.own + late.controller.d_min);
    controller_attack(&late, |s| s.deadline = TEST_DEADLINE + TEST_ROUND_LEN);
    shard_attack(&late, 0, |s| s.deadline = TEST_DEADLINE + TEST_ROUND_LEN);
}

#[test]
fn ca12_seed_ref() {
    // Check 20/24: seed_ref = max input DAA + SEED_OFFSET; using a non-maximal input's DAA
    // (here the newest covenant input, ignoring the funding input) is rejected.
    let c = base();
    let not_max = c.shards.iter().map(|s| s.daa).max().unwrap() as i64;
    assert!(not_max < c.own() as i64);
    marker_attack(&c, |m| m.seed_ref = not_max + c.controller.seed_offset);
}

// ---------------------------------------------------------------- CA13: legs dragged

/// Leg arguments for another controller template.
fn legs_for(b: &mut Built, from: usize, to: usize, args: &[ArtifactValue]) {
    for i in from..to {
        *b.entry_args_mut(i) = args.to_vec();
    }
}

#[test]
fn ca13_lookalike_controller_leads() {
    // L4 (shard input 2; params input 1 likewise): a look-alike controller (same state, a
    // permissive entry) at input 0 leading the real shards and params.
    let c = base();
    let fake = lookalike(Sil::Controller);
    let leg = CloseCase::leg_args(&fake);
    let mk = |b: &mut Built| {
        let e = b.inputs[0].entry.clone();
        b.inputs[0] = In {
            entry: UtxoEntry::new(e.amount, fake.controller_spk(&c.controller), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fake.controller.clone(), entry: "backdoor".into(), args: vec![], redeem: fake.controller_redeem(&c.controller) },
        };
        legs_for(b, 1, 2 + c.k(), &leg);
    };
    attack_at(&c.build(), CloseCase::shard_input(0), mk);
    attack_at(&c.build(), CloseCase::PARAMS, mk);
}

#[test]
fn ca13_controller_of_another_covenant() {
    // L2: the real controller template at input 0, but under another covenant ID.
    let c = base();
    let mk = |b: &mut Built| {
        let e = b.inputs[0].entry.clone();
        b.inputs[0].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, Some(OTHER_COV));
        // Its outputs carry its own ID (consensus requires the authorizing input's ID).
        for o in b.outputs.iter_mut().filter(|o| o.covenant.is_some()) {
            o.covenant = Some(CovenantBinding::new(0, OTHER_COV));
        }
    };
    attack_at(&c.build(), CloseCase::shard_input(0), mk);
    attack_at(&c.build(), CloseCase::PARAMS, mk);
}

#[test]
fn ca13_shard_cospent_with_set_params() {
    // L1/L4 (shard input 1) and set_params check 1 (params input 0): params running
    // set_params with a shard co-spent.
    let sp = SetParamsCase::new();
    let t = tpl();
    let s = CloseCase::new(4).shards[0].state.clone();
    let r = sp.value;
    let mk = |b: &mut Built| {
        b.inputs.insert(
            1,
            In {
                entry: UtxoEntry::new(r, t.shard_spk(&s), 5_000, false, Some(sp.cov)),
                kind: InKind::Entry { template: t.shard.clone(), entry: "close_leg".into(), args: CloseCase::leg_args(&t), redeem: t.shard_redeem(&s) },
            },
        );
        let last = b.outputs.len() - 1;
        b.outputs[last].value += r;
    };
    attack_at(&sp.build(), 1, mk);
    attack_at(&sp.build(), 0, mk);
}

#[test]
fn ca13_leg_at_wrong_index() {
    // L1: a shard leg at input 1 or 0 (inputs swapped), and the params leg at input 2.
    let c = base();
    attack_at(&c.build(), 1, |b| b.inputs.swap(1, 2)); // shard 0 at index 1
    attack_at(&c.build(), 0, |b| b.inputs.swap(0, 2)); // shard 0 at index 0
    attack_at(&c.build(), 2, |b| b.inputs.swap(1, 2)); // params at index 2
}

#[test]
fn ca13_leg_authorizes_an_output() {
    // L3: shard 1 (input 3) authorizes its new shard output; the params (input 1) authorizes
    // params'.
    let c = base();
    let cov = c.cov;
    attack_at(&c.build(), 3, |b| b.outputs[3].covenant = Some(CovenantBinding::new(3, cov)));
    attack_at(&c.build(), 1, |b| b.outputs[1].covenant = Some(CovenantBinding::new(1, cov)));
}

#[test]
fn ca13_extra_shard_beyond_k() {
    // L5 (shard input 6) and checks 3/4 (input 0): a fifth shard of the family in a close
    // whose controller says k = 4.
    let good = base();
    let mut bad = good.clone();
    let mut extra = bad.shards[3].clone();
    extra.state.shard_index = 4;
    bad.shards.push(extra);
    attack_pair(&good.build(), &bad.build(), CloseCase::shard_input(4));
    attack_pair(&good.build(), &bad.build(), CTRL);
}

// ---------------------------------------------------------------- R2: share room

/// Tier shares that leave s_H = 800 >= 0 at the initial s_F = 500, s_R = 1,000, but whose
/// room sum(st) + s_C + s_D + S_F_CAP + S_R_CAP = 7,600 + 100 + 2,500 = 10,200 > 10^4
/// (M2 review finding R2).
const BAD_ST: [i64; 5] = [4_000, 3_000, 600, 0, 0];

#[test]
fn r2_share_room_rejected_at_first_close() {
    // Check 2 (share room): a controller with BAD_ST is rejected at input 0 on its first
    // close(), although s_H >= 0 and s_H' >= 0 hold for it (checks 11 and 17 pass).
    let good = base();
    let mut bad = good.clone();
    bad.controller.st = BAD_ST;
    let x = bad.expected();
    assert_eq!(lottery::game::Shares::of(&bad.controller).s_h(), 800);
    assert!(x.math.amounts.b_r >= 0);
    case_attack(&good, &bad);
}

#[test]
fn r2_reviewer_reproduction_bad_genesis_never_closes() {
    // The reviewer's reproduction: an instance deployed with BAD_ST (through the TEST-ONLY
    // unchecked genesis; the builder refuses it) followed by set_params(1000, 1500), which
    // the params script accepts (both within the caps), and then close(). Before the fix the
    // first close was accepted and every close after the admin change was rejected (s_H' =
    // -200, check 17). With the fix the instance is rejected at its first close, before any
    // admin action, and stays rejected after it.
    let mut ch = Chain::genesis_with(4, |g| g.controller.st = BAD_ST, false);
    let (c, ops) = ch.close_case(900_000);
    let first = ch.close_built(&c, ops);
    reject_at(&first, CTRL);
    let mut sp = SetParamsCase::new();
    sp.s_f = 1_000;
    sp.s_r = 1_500;
    ch.set_params(&mut sp, 950_000);
    assert_eq!((ch.params.1.next_s_f, ch.params.1.next_s_r), (1_000, 1_500));
    // s_H' = 10^4 - 1,000 - 1,500 - 100 - 7,600 = -200: the Rust reference cannot build
    // this close's outputs, so the same transaction is reused with the new params input.
    let mut after = first.clone();
    after.set_entry_redeem(CloseCase::PARAMS, ch.t.params_redeem(&ch.params.1));
    after.outpoints[CloseCase::PARAMS] = ch.params.0;
    reject_at(&after, CTRL);
    // The same instance with the largest valid room (sum(st) = 7,400) closes after the
    // same admin change (s_H' = 0).
    let mut ok = Chain::genesis_with(4, |g| g.controller.st = [4_000, 3_000, 400, 0, 0], true);
    let mut sp = SetParamsCase::new();
    sp.s_f = 1_000;
    sp.s_r = 1_500;
    ok.set_params(&mut sp, 950_000);
    let (c, ops) = ok.close_case(900_000);
    ok.close(&c, ops, TEST_DEADLINE as u64 + 10);
}

// ---------------------------------------------------------------- Note A

#[test]
fn note_a_shard_leg_authorizes_fresh_genesis_output() {
    // OpAuthOutputCount ignores genesis outputs (rusty-kaspa v2.1.0
    // crypto/txscript/src/covenants.rs:137-143: a genesis-authorized output is not added to
    // the script-engine contexts), so a shard leg
    // that authorizes an extra output under a fresh genesis ID still passes L3. The leader
    // rejects it at check 6 (exactly k + 4 outputs).
    let c = base();
    let si = CloseCase::shard_input(1);
    let mutate = |b: &mut Built| {
        let last = b.outputs.len() - 1;
        b.outputs[last].value -= KAS;
        b.outputs.push(TransactionOutput::new(KAS, spk_of(&[9u8; 32])));
        let extra = (b.outputs.len() - 1) as u32;
        b.genesis = vec![GenesisCovenantGroup::new(si as u16, vec![extra])];
    };
    let mut m = c.build();
    mutate(&mut m);
    let f = m.finish();
    assert!(f.tx.outputs.last().unwrap().covenant.is_some(), "extra output carries a fresh genesis ID");
    assert!(f.run_input(si).is_ok(), "the shard leg's L3 does not see a genesis output (Note A)");
    attack_at(&c.build(), CTRL, mutate);
}

// ---------------------------------------------------------------- CA14

#[test]
fn ca14_leader_budget_minus_one() {
    let built = base().build();
    accept(&built);
    let mut b = built.clone();
    b.budget_delta = -1;
    let e = b.finish().run_input(CTRL).expect_err("under-budget leader accepted");
    assert!(matches!(e, TxScriptError::ExceededCommittedScriptUnits { .. }), "expected budget error, got {e:?}");
}

// ---------------------------------------------------------------- CA15

#[test]
fn ca15_non_standard_funder() {
    // Check 24 (D11 rule): funder class 3; class 0 with a non-zero padding byte. The
    // argument and the marker field change together.
    let c = base();
    let mut m3 = c.expected().marker;
    m3.funder_class = 3;
    let spk3 = tpl().marker_spk(&m3);
    let i = c.marker_output();
    out_attack(&c, |b| {
        b.entry_args_mut(0)[1] = ArtifactValue::Byte(3);
        b.outputs[i].script_public_key = spk3;
    });
    let mut mp = c.expected().marker;
    mp.funder_key[32] = 1;
    let spkp = tpl().marker_spk(&mp);
    out_attack(&c, |b| {
        b.entry_args_mut(0)[2] = ArtifactValue::Bytes(mp.funder_key.to_vec());
        b.outputs[i].script_public_key = spkp;
    });
}
