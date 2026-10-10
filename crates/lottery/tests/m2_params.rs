//! M2 `set_params()` tests (spec M2 A5, A7): SP1 and SA1-SA12.
//!
//! Base: `SetParamsCase::new()` (a valid change in round 1: s_F 7 %, s_R 12 %, a new fee
//! address, nonce = stored + 1, with change). Each attack changes one thing and requires a
//! script error from the params input (input 0) unless stated otherwise.

mod common;

use common::accept;
use common::m2::*;
use harness::{TransactionOutput, UtxoEntry};
use lottery::buy::{Built, In, InKind, KAS, spk_of};
use lottery::close::CloseCase;
use lottery::params::{ADMIN_SK, SetParamsCase};
use lottery::state::{ParamsState, Payout};

const P: usize = 0;
/// Another key (test-only).
const OTHER_SK: [u8; 32] = [0x0b; 32];

fn base() -> SetParamsCase {
    SetParamsCase::new()
}

fn case_attack(good: &SetParamsCase, bad: &SetParamsCase) {
    attack_pair(&good.build(), &bad.build(), P);
}

fn out_attack(c: &SetParamsCase, mutate: impl FnOnce(&mut Built)) {
    attack_at(&c.build(), P, mutate);
}

fn successor_attack(c: &SetParamsCase, f: impl FnOnce(&mut ParamsState)) {
    let mut s = c.successor();
    f(&mut s);
    let spk = tpl().params_spk(&s);
    out_attack(c, |b| b.outputs[0].script_public_key = spk);
}

// ---------------------------------------------------------------- SP1

#[test]
fn sp1_valid_changes() {
    // With change (the base).
    let f = accept(&base().build());
    assert_eq!(f.tx.outputs.len(), 2);
    // Without change: funding = fee exactly.
    let mut c = base();
    c.change = false;
    c.funding = vec![c.fee];
    let f = accept(&c.build());
    assert_eq!(f.tx.outputs.len(), 1);
    // The caps themselves: s_F = 1,000, s_R = 1,500.
    let mut c = base();
    c.s_f = 1_000;
    c.s_r = 1_500;
    accept(&c.build());
    // Nonce = stored + 1 and stored + 100 (gaps allowed, D34).
    for step in [1, 100] {
        let mut c = base();
        c.params.nonce = 41;
        c.nonce = 41 + step;
        accept(&c.build());
    }
    // ECDSA and P2SH fee addresses (D11 classes 1 and 2).
    let mut c = base();
    c.set_fee(&Payout::Ecdsa([0x02; 33]));
    accept(&c.build());
    let mut c = base();
    c.set_fee(&Payout::P2sh([0x31; 32]));
    accept(&c.build());
}

// ---------------------------------------------------------------- SA1-SA3

#[test]
fn sa1_s_f_above_cap() {
    // Check 3: s_F <= 1,000.
    let good = base();
    let mut bad = good.clone();
    bad.s_f = 1_001;
    case_attack(&good, &bad);
}

#[test]
fn sa2_s_r_above_cap() {
    // Check 3: s_R <= 1,500.
    let good = base();
    let mut bad = good.clone();
    bad.s_r = 1_501;
    case_attack(&good, &bad);
}

#[test]
fn sa2b_negative_shares() {
    // Check 3: 0 <= s_F and 0 <= s_R.
    let good = base();
    let mut bad = good.clone();
    bad.s_f = -1;
    case_attack(&good, &bad);
    let mut bad = good.clone();
    bad.s_r = -1;
    case_attack(&good, &bad);
}

#[test]
fn sa3_second_change_in_round() {
    // Check 2: used = 0.
    let good = base();
    let mut bad = good.clone();
    bad.params.used = 1;
    case_attack(&good, &bad);
}

// ---------------------------------------------------------------- SA4-SA9: authentication

#[test]
fn sa4_wrong_key() {
    // Check 6: another key signs (the passed pubkey is the admin's).
    let good = base();
    let mut bad = good.clone();
    bad.signer_sk = OTHER_SK;
    case_attack(&good, &bad);
    // Check 5: a correct signature by another key, with that key passed: it does not hash
    // to admin_hash.
    let mut bad = good.clone();
    bad.signer_sk = OTHER_SK;
    bad.admin_sk = OTHER_SK;
    case_attack(&good, &bad);
    // Check 5: the admin's signature with another pubkey passed.
    let mut bad = good.clone();
    bad.admin_sk = OTHER_SK;
    bad.signer_sk = ADMIN_SK;
    case_attack(&good, &bad);
}

#[test]
fn sa5_replay_same_nonce() {
    // Check 4: stored nonce < nonce.
    let good = base();
    let mut bad = good.clone();
    bad.params.nonce = 7;
    bad.nonce = 7;
    case_attack(&good, &bad);
}

#[test]
fn sa6_stale_nonce() {
    // Check 4.
    let good = base();
    let mut bad = good.clone();
    bad.params.nonce = 7;
    bad.nonce = 6;
    case_attack(&good, &bad);
    // Check 4 upper bound: nonce < 2^62.
    let mut bad = good.clone();
    bad.nonce = 1 << 62;
    case_attack(&good, &bad);
}

#[test]
fn sa7_signature_for_other_round() {
    // Check 6: target round = round + 1; a signature for round r or r + 2.
    let good = base();
    for target in [good.params.round, good.params.round + 2] {
        let mut bad = good.clone();
        bad.msg = Some(lottery::params::ParamsMsg { target_round: target, ..good.honest_msg() });
        case_attack(&good, &bad);
    }
}

#[test]
fn sa8_signature_for_other_covenant() {
    // Check 6: the signed covenant ID.
    let good = base();
    let mut bad = good.clone();
    bad.msg = Some(lottery::params::ParamsMsg { cov: OTHER_COV, ..good.honest_msg() });
    case_attack(&good, &bad);
}

#[test]
fn sa9_signed_values_differ_from_arguments() {
    // Check 6: the signature covers s_F (and every other argument).
    let good = base();
    let h = good.honest_msg();
    for m in [
        lottery::params::ParamsMsg { s_f: h.s_f + 1, ..h },
        lottery::params::ParamsMsg { s_r: h.s_r + 1, ..h },
        lottery::params::ParamsMsg { fee_class: 2, ..h },
        lottery::params::ParamsMsg { nonce: h.nonce + 1, ..h },
    ] {
        let mut bad = good.clone();
        bad.msg = Some(m);
        case_attack(&good, &bad);
    }
    let mut fk = h.fee_key;
    fk[0] ^= 1;
    let mut bad = good.clone();
    bad.msg = Some(lottery::params::ParamsMsg { fee_key: fk, ..h });
    case_attack(&good, &bad);
}

// ---------------------------------------------------------------- SA10, SA11: successor

#[test]
fn sa10_successor_fields() {
    // Check 8: everything but nonce, used and next_* is unchanged; used = 1; next_* = args.
    let c = base();
    successor_attack(&c, |s| s.admin_hash[0] ^= 1);
    successor_attack(&c, |s| s.round += 1);
    successor_attack(&c, |s| s.nonce += 1);
    successor_attack(&c, |s| s.used = 0);
    successor_attack(&c, |s| s.next_s_f += 1);
    successor_attack(&c, |s| s.next_s_r -= 1);
    successor_attack(&c, |s| s.next_fee_class = 2);
    successor_attack(&c, |s| s.next_fee_key[5] ^= 1);
}

#[test]
fn sa11_value() {
    // Check 7: out0 value = in0 value.
    let c = base();
    out_attack(&c, |b| b.shift_value(0, 1));
    out_attack(&c, |b| b.shift_value(0, -1));
}

// ---------------------------------------------------------------- SA12: shape and fee script

#[test]
fn sa12_controller_cospent() {
    // Check 1: OpCovInputCount = 1 (a controller of the family co-spent at input 1).
    let c = base();
    let t = tpl();
    let cc = CloseCase::new(4);
    let ctrl = cc.controller.clone();
    let value = cc.controller_value();
    let args = cc.args(&t, cc.expected().m_next);
    let cov = c.cov;
    out_attack(&c, |b| {
        b.inputs.insert(
            1,
            In {
                entry: UtxoEntry::new(value, t.controller_spk(&ctrl), 4_000, false, Some(cov)),
                kind: InKind::Entry { template: t.controller.clone(), entry: "close".into(), args, redeem: t.controller_redeem(&ctrl) },
            },
        );
        let last = b.outputs.len() - 1;
        b.outputs[last].value += value;
    });
}

#[test]
fn sa12_extra_output() {
    // Check 1: outputs are the successor and at most one change output.
    let c = base();
    out_attack(&c, |b| {
        let last = b.outputs.len() - 1;
        b.outputs[last].value -= KAS;
        b.outputs.push(TransactionOutput::new(KAS, spk_of(&[9u8; 32])));
    });
    // An extra covenant output authorized by the params.
    let succ = tpl().params_spk(&c.successor());
    let cov = c.cov;
    out_attack(&c, |b| {
        let last = b.outputs.len() - 1;
        b.outputs[last].value -= KAS;
        b.outputs.insert(1, TransactionOutput::with_covenant(KAS, succ, Some(harness::CovenantBinding::new(0, cov))));
    });
}

#[test]
fn sa12_non_standard_fee_script() {
    // Check 3 (D11): fee class 3; class 0 with a non-zero padding byte. Signed honestly.
    let good = base();
    let mut bad = good.clone();
    bad.fee_class = 3;
    case_attack(&good, &bad);
    let mut bad = good.clone();
    bad.fee_key[32] = 1;
    case_attack(&good, &bad);
}

#[test]
fn sa12_change_with_covenant_id() {
    // Check 1: the change output has the zero ID (a genesis output of a fresh ID).
    let c = base();
    out_attack(&c, |b| b.genesis = vec![kaspa_consensus_core::tx::GenesisCovenantGroup::new(1, vec![1])]);
}
