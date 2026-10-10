//! Mutation tests for M1 (review follow-up). Each test proves that a named `require` in
//! shard.sil is the check that stops an attack: the real shard rejects the attack, and a shard
//! compiled with only that line removed accepts it. Where several checks back each other up,
//! the test says so and checks the backstop instead.
//!
//! Also the boundary tests the review found missing: extra >= 0, F_MAX + 1 funding inputs,
//! every state range check and the sales cap.

mod common;

use std::sync::Arc;

use common::*;
use harness::{CovenantBinding, Hash, TransactionOutput, UtxoEntry};
use kaspa_consensus_core::tx::GenesisCovenantGroup;
use lottery::buy::{BuyCase, Built, COV, In, InKind, KAS, Templates, b_max, funding_spk, shard_source, ticket_source};
use lottery::combo::Combo;
use lottery::smt::Tree;
use lottery::state::Payout;

/// Shard templates with exactly one occurrence of `needle` removed.
fn mutant(needle: &str) -> Arc<Templates> {
    let src = shard_source(b_max());
    assert_eq!(src.matches(needle).count(), 1, "needle must be unique: {needle}");
    Arc::new(Templates::compile_with(b_max(), &ticket_source(b_max()), &src.replace(needle, "")))
}

/// `mutate` on `base` is rejected by the real shard and accepted without `needle`.
fn killed_by(base: &BuyCase, needle: &str, mutate: impl Fn(&mut Built)) {
    attack(base, &mutate);
    let mut b = base.build_with(mutant(needle));
    accept(&b);
    mutate(&mut b);
    accept(&b);
}

/// `bad` is rejected by the real shard and accepted without `needle`; `good` is accepted.
fn case_killed_by(good: &BuyCase, bad: &BuyCase, needle: &str) {
    attack_case(good, bad);
    accept(&bad.build_with(mutant(needle)));
}

fn with_combo(main: [u8; 5]) -> BuyCase {
    let mut c = case();
    c.bundle = vec![Combo::new(main, 4)];
    c
}

// ------------------------------------------------------------ canonical combination (check 9)

#[test]
fn canonical_order_checks_each_kill_an_attack() {
    // Each non-canonical main set has the rank of a canonical one, so without its check the buy
    // would count it under another combination's key.
    let cases: [(&str, [u8; 5]); 8] = [
        ("require(n1 < n2);", [2, 1, 3, 4, 5]),
        ("require(n2 < n3);", [1, 3, 2, 4, 5]),
        ("require(n2 < n3);", [1, 2, 2, 4, 5]),
        ("require(n3 < n4);", [1, 2, 4, 3, 5]),
        ("require(n3 < n4);", [1, 2, 3, 3, 5]),
        ("require(n4 < n5);", [1, 2, 3, 5, 4]),
        ("require(n4 < n5);", [1, 2, 3, 4, 4]),
        ("require(n5 <= 30);", [1, 2, 3, 4, 31]),
    ];
    for (needle, main) in cases {
        case_killed_by(&case(), &with_combo(main), needle);
    }
    // The reviewer's scenario: [1,2,3,5,4] has the rank of (1,2,3,4,6).
    assert_eq!(Combo::new([1, 2, 3, 5, 4], 1).rank(), Combo::new([1, 2, 3, 4, 6], 1).rank());
}

// ------------------------------------------------------------ counter (checks 10-13)

#[test]
fn root_check_kills_forged_count() {
    // A12: the successor is built from the forged count, so only the old-root check rejects.
    let x = Combo::new([3, 6, 9, 12, 15], 2);
    let mut real = Tree::new(10);
    real.set(x.rank(), vec![0, 4, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut forged = Tree::new(10);
    forged.set(x.rank(), vec![0, 3, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut good = case().with_tree(real.clone());
    good.bundle = vec![x];
    let mut bad = case().with_tree(forged);
    bad.bundle = vec![x];
    bad.shard.root = real.root();
    case_killed_by(&good, &bad, "require(oldNode == cur);");
}

// ------------------------------------------------------------ shape (checks 2-5)

#[test]
fn zero_id_input_check_kills_foreign_funding() {
    killed_by(&case(), "require(OpInputCovenantId(k) == zeroId);", |b| {
        b.inputs[1].entry = UtxoEntry::new(b.inputs[1].entry.amount, funding_spk(), 0, false, Some(Hash::from_bytes([0x99; 32])));
    });
}

#[test]
fn zero_id_output_check_kills_genesis_change() {
    // A5: the change output is a genesis output of a fresh ID.
    killed_by(&case(), "require(OpOutputCovenantId(k) == zeroId);", |b| {
        let last = b.outputs.len() as u32 - 1;
        b.genesis = vec![GenesisCovenantGroup::new(1, vec![last])];
    });
}

#[test]
fn same_family_input_is_stopped_by_check_2_with_count_backstop() {
    // A1 / A6: a second input of the family fails check 2 (zero-ID funding). Check 3
    // (OpCovInputCount = 1) cannot fire first, and it still rejects with check 2 removed.
    let base = case();
    let mut other = case();
    other.shard.shard_index = 1;
    let o = other.build();
    let add_shard = |b: &mut Built| {
        let change = b.outputs.pop().unwrap();
        let redeem = match &o.inputs[0].kind {
            InKind::Shard(r) => r.clone(),
            _ => unreachable!(),
        };
        b.inputs.insert(1, In { entry: o.inputs[0].entry.clone(), kind: InKind::Covenant { redeem, args: o.args.clone() } });
        for out in &o.outputs[..2] {
            b.outputs.push(TransactionOutput::with_covenant(out.value, out.script_public_key.clone(), Some(CovenantBinding::new(1, COV))));
        }
        b.outputs.push(change);
        b.inputs[2].entry = UtxoEntry::new(b.inputs[2].entry.amount + o.outputs[0].value, funding_spk(), 0, false, None);
    };
    attack(&base, add_shard);
    let mut m = base.build_with(mutant("require(OpInputCovenantId(k) == zeroId);"));
    accept(&m);
    add_shard(&mut m);
    assert_check_fails(&m);
}

#[test]
fn extra_lower_bound() {
    // with_ref = true but no referral output (and no change): extra = -1. This guard is also
    // what keeps tx.outputs[2] from being read when it does not exist.
    let mut c = case();
    c.with_ref = true;
    c.change = false;
    c.funding = vec![c.gross() + c.b() * c.shard.d as u64 + c.fee];
    attack(&c, |b| {
        let r = b.outputs.remove(2);
        // Keep the fee the same: nothing else changes.
        let _ = r;
    });
}

#[test]
fn too_many_funding_inputs() {
    let mut good = case();
    good.funding = vec![2 * KAS; 8];
    let mut bad = case();
    bad.funding = vec![2 * KAS; 9];
    attack_case(&good, &bad);
}

// ------------------------------------------------------------ time (check 6)

#[test]
fn deadline_check_kills_late_shard() {
    let mut bad = case();
    bad.shard_daa = bad.shard.deadline as u64;
    case_killed_by(&case(), &bad, "require(OpTxInputDaaScore(0) < deadline);");
}

// ------------------------------------------------------------ ranges (checks 7-8, 14)

#[test]
fn state_range_checks() {
    let good = case();
    let big = |mut c: BuyCase| {
        c.funding = vec![1_000_000 * KAS];
        c
    };
    let mut bads: Vec<BuyCase> = vec![];
    let mut push = |f: &dyn Fn(&mut BuyCase)| {
        let mut c = big(case());
        f(&mut c);
        bads.push(c);
    };
    push(&|c| c.shard.round = 0);
    push(&|c| c.shard.shard_index = -1);
    push(&|c| {
        *c = big(case().with_tree(Tree::new(1)));
        c.bundle = vec![Combo::new([3, 7, 12, 19, 28], 1)];
    });
    push(&|c| *c = big(case().with_tree(Tree::new(256))));
    push(&|c| c.shard.price = 0);
    push(&|c| c.shard.price = 100_000_000_001);
    push(&|c| c.shard.d = 0);
    push(&|c| c.shard.d = 10_000_000_001);
    push(&|c| c.shard.s_r = -1);
    push(&|c| c.shard.s_r = 1_501);
    push(&|c| {
        c.shard.deadline = 500_000_000_000;
        c.shard_daa = 5_000;
    });
    push(&|c| {
        c.shard.sales = -1;
        c.shard.ref_paid = -1;
    });
    push(&|c| {
        c.shard.sales = 5;
        c.shard.ref_paid = 6;
    });
    // Sales cap: S + P would pass SALES_MAX = 2^62.
    push(&|c| {
        c.shard.sales = (1i64 << 62) - KAS as i64 + 1;
        c.shard.ref_paid = c.shard.sales;
    });
    for bad in &bads {
        attack_case(&good, bad);
    }
    // The upper bounds themselves are accepted.
    let mut edge = big(case().with_tree(Tree::new(255)));
    edge.shard.s_r = 1_500;
    edge.shard.price = 100_000_000_000;
    edge.shard.d = 10_000_000_000;
    edge.shard.sales = (1i64 << 62) - 100_000_000_000;
    edge.shard.ref_paid = edge.shard.sales;
    accept(&edge.build());
}

// ------------------------------------------------------------ money and scripts (checks 14-20)

#[test]
fn money_checks_each_kill_an_attack() {
    let mut r = case();
    r.with_ref = true;
    killed_by(&r, "require(tx.outputs[2].value == r);", |b| b.shift_value(2, 1));
    killed_by(&case(), "require(tx.outputs[1].value == d * b);", |b| b.shift_value(1, 1));
    killed_by(&case(), "require(tx.outputs[0].value == tx.inputs[0].value + p - r);", |b| b.shift_value(0, -1));
    // Below M_REF: s_R = 999 bp gives r = 0.0999 KAS.
    let mut bad = case();
    bad.with_ref = true;
    bad.shard.s_r = 999;
    let mut good = bad.clone();
    good.with_ref = false;
    case_killed_by(&good, &bad, "require(r >= M_REF);");
}

#[test]
fn script_class_checks_each_kill_an_attack() {
    let mut r = case();
    r.with_ref = true;
    killed_by(&r, "require(std);", |b| b.outputs[2].script_public_key = harness::ScriptPublicKey::new(0, vec![0x51].into()));
    let c = case();
    let class3 = |b: &mut Built| {
        let mut t = c.ticket();
        t.payout_class = 3;
        b.args[5] = harness::ArtifactValue::Byte(3);
        b.set_ticket(&t);
    };
    killed_by(&c, "require(pc <= 2);", class3);
    let pad = |b: &mut Built| {
        let mut t = c.ticket();
        t.payout_key[32] = 1;
        b.args[6] = harness::ArtifactValue::Bytes(t.payout_key.to_vec());
        b.set_ticket(&t);
    };
    killed_by(&c, "require(payout_key.slice(32, 33) == byte[](0x00));", pad);
    let _ = Payout::Schnorr([0; 32]);
}
