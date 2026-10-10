//! M1 attack tests A1–A24 (spec M1 8.2).
//!
//! Each test first proves its base transaction is accepted, then changes one thing and
//! requires the shard (input 0) to fail a check (VerifyError). A11 expects a budget error.

mod common;

use common::*;
use harness::{ArtifactValue, CovenantBinding, Hash, ScriptPublicKey, TransactionOutput, TxScriptError, UtxoEntry};
use kaspa_consensus_core::tx::GenesisCovenantGroup;
use lottery::buy::{COV, In, InKind, KAS, Templates, b_max, funding_spk, shard_source, ticket_source};
use lottery::combo::{Combo, bundle_bytes};
use lottery::smt::Tree;
use lottery::state::{Payout, ShardState};

const OTHER: Hash = Hash::from_bytes([0x99; 32]);

fn bytes_arg(v: &ArtifactValue) -> Vec<u8> {
    match v {
        ArtifactValue::Bytes(b) => b.clone(),
        _ => panic!("not bytes"),
    }
}

// ------------------------------------------------------------ shape and binding

#[test]
fn a1_parallel_authorization_group() {
    // A second shard of the same family, authorizing its own successor and ticket.
    let base = case();
    let mut other = case();
    other.shard.shard_index = 1;
    other.bundle = vec![Combo::new([4, 5, 6, 7, 8], 2)];
    let o = other.build();
    attack(&base, |b| {
        let change = b.outputs.pop().unwrap();
        b.inputs.insert(1, In { entry: o.inputs[0].entry.clone(), kind: InKind::Covenant { redeem: match &o.inputs[0].kind {
            InKind::Shard(r) => r.clone(),
            _ => unreachable!(),
        }, args: o.args.clone() } });
        for out in &o.outputs[..2] {
            b.outputs.push(TransactionOutput::with_covenant(out.value, out.script_public_key.clone(), Some(CovenantBinding::new(1, COV))));
        }
        b.outputs.push(change);
        b.inputs[2].entry = UtxoEntry::new(b.inputs[2].entry.amount + o.outputs[0].value, funding_spk(), 0, false, None);
    });
}

#[test]
fn a2_extra_covenant_output() {
    attack(&case(), |b| {
        let extra = TransactionOutput::with_covenant(KAS / 10, b.outputs[1].script_public_key.clone(), Some(CovenantBinding::new(0, COV)));
        let last = b.outputs.len() - 1;
        b.outputs[last].value -= KAS / 10;
        b.outputs.insert(2, extra);
    });
}

#[test]
fn a3_extra_plain_output() {
    attack(&case(), |b| {
        let last = b.outputs.len() - 1;
        b.outputs[last].value -= KAS;
        b.outputs.push(TransactionOutput::new(KAS, funding_spk()));
    });
}

#[test]
fn a4_ticket_not_authorized_by_shard() {
    // Plain output in place of the ticket.
    attack(&case(), |b| b.outputs[1].covenant = None);
    // The ticket is a genesis output of a fresh covenant ID authorized by the funding input.
    attack(&case(), |b| {
        b.outputs[1].covenant = None;
        b.genesis = vec![GenesisCovenantGroup::new(1, vec![1])];
    });
}

#[test]
fn a5_plain_output_with_covenant_id() {
    // Change output becomes a genesis output of a fresh ID.
    attack(&case(), |b| {
        let last = b.outputs.len() as u32 - 1;
        b.genesis = vec![GenesisCovenantGroup::new(1, vec![last])];
    });
    // Referral output becomes a genesis output of a fresh ID.
    let mut c = case();
    c.with_ref = true;
    attack(&c, |b| b.genesis = vec![GenesisCovenantGroup::new(1, vec![2])]);
}

#[test]
fn a6_wrong_role_and_foreign_inputs() {
    // A ticket of the same family as an extra input.
    let base = case();
    let t = Templates::get(b_max());
    let ticket = base.ticket();
    let ticket_redeem = t.ticket.redeem(&ticket.values());
    attack(&base, |b| {
        let sig = lottery::buy::push_redeem(
            // The M1 placeholder `locked()` is gone (M4 Stage 3a): a tag no entry has keeps
            // the ticket input inert, so the attack is judged at the shard as before.
            lottery::buy::planned_sig_script("locked()", &[]),
            &ticket_redeem,
        );
        b.inputs.insert(
            1,
            In { entry: UtxoEntry::new(KAS / 10, harness::pay_to_script_hash_script(&ticket_redeem), 0, false, Some(COV)), kind: InKind::Raw(sig) },
        );
        let last = b.outputs.len() - 1;
        b.outputs[last].value += KAS / 10;
    });
    // A funding input carrying another family's covenant ID.
    attack(&base, |b| {
        b.inputs[1].entry = UtxoEntry::new(b.inputs[1].entry.amount, funding_spk(), 0, false, Some(OTHER));
    });
}

#[test]
fn a7_wrong_round_or_deadline_in_ticket() {
    for (dr, dt) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
        let c = case();
        attack(&c, |b| {
            let mut t = c.ticket();
            t.round += dr;
            t.deadline += dt;
            b.set_ticket(&t);
        });
    }
}

#[test]
fn a8_wrong_ticket_template() {
    // Look-alike: the real ticket plus a permissive entry (M4 Stage 3a: the M1 placeholder
    // `locked()` this test used to invert is gone; same idea, same state layout).
    let src = ticket_source(b_max());
    let end = src.rfind('}').expect("closing brace");
    let look = format!("{}
    entry backdoor() {{
        require(this.activeInputIndex >= 0);
    }}
}}
", &src[..end]);
    let fake = Templates::compile_with(b_max(), &look, &shard_source(b_max()));
    assert_ne!(fake.ticket.hash, Templates::get(b_max()).ticket.hash);
    let c = case();
    // Same state under look-alike code, real prefix/suffix arguments.
    attack(&c, |b| b.outputs[1].script_public_key = fake.ticket_spk(&c.ticket()));
    // Look-alike code and the caller passing the look-alike's prefix and suffix.
    attack(&c, |b| {
        b.outputs[1].script_public_key = fake.ticket_spk(&c.ticket());
        b.args[7] = ArtifactValue::Bytes(fake.ticket.prefix.clone());
        b.args[8] = ArtifactValue::Bytes(fake.ticket.suffix.clone());
    });
}

#[test]
fn a9_malformed_sibling_and_leaf_strings() {
    let c = case();
    for delta in [-1i64, 1] {
        attack(&c, |b| {
            let mut s = bytes_arg(&b.args[3]);
            if delta < 0 { s.pop(); } else { s.push(0); }
            b.args[3] = ArtifactValue::Bytes(s);
        });
    }
    attack(&c, |b| b.args[3] = ArtifactValue::Bytes(vec![]));
    for delta in [-2i64, 2] {
        attack(&c, |b| {
            let mut s = bytes_arg(&b.args[2]);
            if delta < 0 { s.truncate(s.len() - 2); } else { s.extend([0, 0]); }
            b.args[2] = ArtifactValue::Bytes(s);
        });
    }
}

#[test]
fn a10_buy_on_shard_created_at_or_after_deadline() {
    for offset in [0u64, 5] {
        let mut bad = case();
        bad.shard_daa = bad.shard.deadline as u64 + offset;
        attack_case(&case(), &bad);
    }
}

#[test]
fn a11_underfunded_compute_budget() {
    let built = case().build();
    accept(&built);
    let mut b = built.clone();
    b.budget_delta = -1;
    let e = shard_error(&b);
    assert!(matches!(e, TxScriptError::ExceededCommittedScriptUnits { .. }), "expected budget error, got {e:?}");
}

// ------------------------------------------------------------ counter

/// A case whose shard root is `real`'s, but whose proofs and successor come from `forged`.
fn forged(real: &Tree, forged: Tree, bundle: Vec<Combo>) -> (lottery::buy::BuyCase, lottery::buy::BuyCase) {
    let mut good = case().with_tree(real.clone());
    good.bundle = bundle.clone();
    let mut bad = case().with_tree(forged);
    bad.bundle = bundle;
    bad.shard.root = real.root();
    (good, bad)
}

#[test]
fn a12_forged_count() {
    let x = Combo::new([3, 6, 9, 12, 15], 2);
    let mut real = Tree::new(10);
    real.set(x.rank(), vec![0, 4, 0, 0, 0, 0, 0, 0, 0, 0]);
    for c in [3u16, 5] {
        let mut f = Tree::new(10);
        f.set(x.rank(), vec![0, c, 0, 0, 0, 0, 0, 0, 0, 0]);
        let (good, bad) = forged(&real, f, vec![x]);
        attack_case(&good, &bad);
    }
}

#[test]
fn a13_wrong_leaf() {
    let x = Combo::new([3, 6, 9, 12, 15], 2);
    // Empty leaf (Z_0) claimed for a non-empty key.
    let mut real = Tree::new(10);
    real.set(x.rank(), vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    let (good, bad) = forged(&real, Tree::new(10), vec![x]);
    attack_case(&good, &bad);
    // Non-empty leaf claimed for an empty key.
    let mut f = Tree::new(10);
    f.set(x.rank(), vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    let (good, bad) = forged(&Tree::new(10), f, vec![x]);
    attack_case(&good, &bad);
    // A leaf and path of another rank.
    let mut real = Tree::new(10);
    let y = Combo::new([4, 6, 9, 12, 15], 2);
    real.set(y.rank(), vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut c = case().with_tree(real.clone());
    c.bundle = vec![x];
    attack(&c, |b| {
        b.args[2] = ArtifactValue::Bytes(real.counts_bytes(y.rank()));
        b.args[3] = ArtifactValue::Bytes(real.siblings(y.rank()));
    });
}

#[test]
fn a14_wrong_successor_root() {
    let c = case();
    let x = c.bundle[0];
    let mut wrong = vec![];
    // Bonus + 1 incremented instead.
    let mut t = c.tree.clone();
    t.increment(x.rank(), x.bonus + 1);
    wrong.push(t.root());
    // Count + 2.
    let mut t = c.tree.clone();
    t.increment(x.rank(), x.bonus);
    t.increment(x.rank(), x.bonus);
    wrong.push(t.root());
    // Unchanged.
    wrong.push(c.tree.root());
    for root in wrong {
        attack(&c, |b| {
            let mut s = c.successor();
            s.root = root;
            b.set_successor(&s);
        });
    }
}

#[test]
fn a15_non_canonical_combination() {
    let c = case();
    for bad in [[7u8, 3, 12, 19, 28, 4], [3, 3, 12, 19, 28, 4], [0, 7, 12, 19, 28, 4], [3, 7, 12, 19, 31, 4], [3, 7, 12, 19, 28, 0], [3, 7, 12, 19, 28, 11]] {
        attack(&c, |b| b.args[1] = ArtifactValue::Bytes(bad.to_vec()));
    }
}

#[test]
fn a16_bundle_size_out_of_range() {
    // b = 0 with empty bundle, leaves and siblings.
    attack(&case(), |b| {
        b.args[0] = ArtifactValue::Int(0);
        b.args[1] = ArtifactValue::Bytes(vec![]);
        b.args[2] = ArtifactValue::Bytes(vec![]);
        b.args[3] = ArtifactValue::Bytes(vec![]);
    });
    // b = B_MAX + 1, with consistent proofs.
    let mut big = case();
    big.bundle = distinct(b_max() + 1, 10);
    let big_args = big.args(&Templates::get(b_max()));
    attack(&case(), |b| {
        for i in 0..4 {
            b.args[i] = big_args[i].clone();
        }
    });
    // Bundle length != 6b.
    attack(&case(), |b| {
        let mut s = bytes_arg(&b.args[1]);
        s.push(1);
        b.args[1] = ArtifactValue::Bytes(s);
    });
}

#[test]
fn a17_count_overflow() {
    let x = Combo::new([11, 12, 13, 14, 15], 5);
    let mut t = Tree::new(10);
    let mut counts = vec![0u16; 10];
    counts[4] = 32_766;
    t.set(x.rank(), counts.clone());
    let mut c = case().with_tree(t.clone());
    c.bundle = vec![x];
    // Same buy against a shard whose count is already 32,767.
    counts[4] = 32_767;
    t.set(x.rank(), counts);
    attack(&c, |b| {
        let mut s = c.shard.clone();
        s.root = t.root();
        let v = b.inputs[0].entry.amount;
        b.set_shard_input(&s, v);
        b.args[2] = ArtifactValue::Bytes(t.counts_bytes(x.rank()));
        b.args[3] = ArtifactValue::Bytes(t.siblings(x.rank()));
    });
}

// ------------------------------------------------------------ money

#[test]
fn a18_wrong_price_or_sales() {
    let c = case();
    for d in [-1i64, 1] {
        attack(&c, |b| b.shift_value(0, d));
        attack(&c, |b| {
            let mut s = c.successor();
            s.sales += d;
            b.set_successor(&s);
        });
    }
    let mut c = case();
    c.with_ref = true;
    for d in [-1i64, 1] {
        attack(&c, |b| {
            let mut s = c.successor();
            s.ref_paid += d;
            b.set_successor(&s);
        });
    }
}

#[test]
fn a19_wrong_deposit() {
    for d in [-1i64, 1] {
        attack(&case(), |b| b.shift_value(1, d));
    }
}

#[test]
fn a20_wrong_referral() {
    let mut c = case();
    c.with_ref = true;
    for d in [-1i64, 1] {
        attack(&c, |b| b.shift_value(2, d));
    }
    // Below M_REF: s_R = 999 bp gives r = 0.0999 KAS.
    let mut bad = case();
    bad.with_ref = true;
    bad.shard.s_r = 999;
    let mut good = bad.clone();
    good.with_ref = false;
    attack_case(&good, &bad);
}

#[test]
fn a21_non_standard_scripts() {
    let mut c = case();
    c.with_ref = true;
    let mut short_p2sh = vec![0xaa, 0x1f];
    short_p2sh.extend([5u8; 31]);
    short_p2sh.push(0x87);
    let p2pk = Payout::Schnorr([5; 32]).script();
    for spk in [ScriptPublicKey::new(0, short_p2sh.into()), ScriptPublicKey::new(1, p2pk.into()), ScriptPublicKey::new(0, vec![0x51].into())] {
        attack(&c, |b| b.outputs[2].script_public_key = spk);
    }
    // payout_class 3 and a non-zero padding byte, with a ticket state that agrees.
    let c = case();
    attack(&c, |b| {
        let mut t = c.ticket();
        t.payout_class = 3;
        b.args[5] = ArtifactValue::Byte(3);
        b.set_ticket(&t);
    });
    attack(&c, |b| {
        let mut t = c.ticket();
        t.payout_key[32] = 1;
        b.args[6] = ArtifactValue::Bytes(t.payout_key.to_vec());
        b.set_ticket(&t);
    });
}

#[test]
fn a22_wrong_ticket_state() {
    let c = case();
    let mutations: Vec<Box<dyn Fn(&mut lottery::state::TicketState)>> = vec![
        Box::new(|t| t.claimed[0] = 1),
        Box::new(|t| *t.bundle.last_mut().unwrap() = 1),
        Box::new(|t| t.b += 1),
        Box::new(|t| {
            let other = bundle_bytes(&[Combo::new([1, 2, 3, 4, 6], 1)]);
            t.bundle[..6].copy_from_slice(&other);
        }),
    ];
    for m in mutations {
        attack(&c, |b| {
            let mut t = c.ticket();
            m(&mut t);
            b.set_ticket(&t);
        });
    }
}

#[test]
fn a23_successor_changes_other_fields() {
    let c = case();
    let mutations: Vec<fn(&mut ShardState)> = vec![
        |s| s.round += 1,
        |s| s.shard_index += 1,
        |s| s.m += 1,
        |s| s.price += 1,
        |s| s.d += 1,
        |s| s.s_r += 1,
        |s| s.deadline += 1,
    ];
    for m in mutations {
        attack(&c, |b| {
            let mut s = c.successor();
            m(&mut s);
            b.set_successor(&s);
        });
    }
}

#[test]
fn a24_two_plain_outputs_without_referral() {
    attack(&case(), |b| {
        let last = b.outputs.len() - 1;
        b.outputs[last].value -= M_REF_TEST;
        b.outputs.insert(2, TransactionOutput::new(M_REF_TEST, Payout::Schnorr([0x33; 32]).spk()));
    });
}

const M_REF_TEST: u64 = lottery::buy::M_REF;

#[test]
fn b_max_within_overflow_bound() {
    // Check 14 relies on B_MAX <= 10 (spec 5.3).
    assert!(b_max() <= 10);
}
