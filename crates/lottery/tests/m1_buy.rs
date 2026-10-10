//! M1 positive tests P1–P13 (spec M1 8.1). P14 is in the crate's unit tests.
//!
//! Every transaction runs through the rusty-kaspa v2.1.0 engine with production flags,
//! each input under the compute budget the builder commits.

mod common;

use common::*;
use lottery::buy::{BuyCase, Final, KAS, M_REF, Templates, b_max, funding_spk};
use lottery::combo::Combo;
use lottery::smt::Tree;
use lottery::state::Payout;

/// Value conservation (design 5.5, spec 5.4) over economic value:
/// funding = (P - r) + r + d*b + change + fee, with the shard growing by exactly P - r.
fn assert_conserved(c: &BuyCase, f: &Final) {
    let p = c.gross();
    let r = c.referral();
    let funding: u64 = f.entries[1..].iter().map(|e| e.amount).sum();
    let shard_delta = f.tx.outputs[0].value - f.entries[0].amount;
    assert_eq!(shard_delta, p - r, "shard grows by P - r");
    assert_eq!(f.tx.outputs[1].value, c.b() * c.shard.d as u64, "ticket holds d x b");
    let ref_out = if c.with_ref { f.tx.outputs[2].value } else { 0 };
    assert_eq!(ref_out, r);
    let change = if c.change { f.tx.outputs.last().unwrap().value } else { 0 };
    assert_eq!(funding, shard_delta + ref_out + f.tx.outputs[1].value + change + f.fee());
}

/// The on-chain successor must match an independently updated tree.
fn assert_counts(c: &BuyCase) {
    let mut t = c.tree.clone();
    for combo in &c.bundle {
        t.increment(combo.rank(), combo.bonus);
    }
    assert_eq!(c.successor().root, t.root());
}

fn run(c: &BuyCase) -> Final {
    let f = accept(&c.build());
    assert_conserved(c, &f);
    assert_counts(c);
    f
}

#[test]
fn p1_single_combination_fresh_shard() {
    let c = case();
    let f = run(&c);
    let t = Templates::get(b_max());
    let succ = c.successor();
    assert_eq!(succ.sales, KAS as i64);
    assert_eq!(succ.ref_paid, 0);
    assert_eq!(f.tx.outputs[0].script_public_key, t.shard_spk(&succ));
    let ticket = c.ticket();
    assert_eq!(ticket.deadline, c.shard.deadline, "ticket records T");
    assert_eq!(ticket.round, c.shard.round);
    assert!(ticket.claimed.iter().all(|&x| x == 0));
    assert_eq!(f.tx.outputs[1].script_public_key, t.ticket_spk(&ticket));
    assert_eq!(f.tx.outputs.len(), 3);
}

#[test]
fn p2_full_bundle_with_referral_and_change() {
    let mut c = case();
    c.bundle = distinct(b_max(), 10);
    c.with_ref = true;
    let f = run(&c);
    assert_eq!(f.tx.outputs.len(), 4);
    assert_eq!(f.tx.outputs[2].value, b_max() as u64 * KAS / 10);
    assert_eq!(c.successor().ref_paid, (b_max() as u64 * KAS / 10) as i64);
}

#[test]
fn p3_repeated_combination() {
    let mut c = case();
    let x = Combo::new([2, 4, 6, 8, 10], 7);
    c.bundle = vec![x; b_max()];
    run(&c);
    let (_, _, t) = c.proofs();
    assert_eq!(t.counts(x.rank())[6], b_max() as u16);
}

#[test]
fn p4_shared_main_numbers() {
    let mut c = case();
    let main = [5, 6, 7, 8, 9];
    c.bundle = vec![Combo::new(main, 1), Combo::new(main, 2), Combo::new(main, 1)];
    c.bundle.truncate(b_max());
    run(&c);
    let (_, _, t) = c.proofs();
    let counts = t.counts(c.bundle[0].rank());
    assert_eq!(counts[0], c.bundle.iter().filter(|x| x.bonus == 1).count() as u16);
}

#[test]
fn p5_sequential_buys_on_one_shard() {
    let mut c = case();
    let first = Combo::new([1, 9, 17, 23, 30], 3);
    let bundles = [vec![first], vec![Combo::new([2, 3, 4, 5, 6], 10), first], vec![first]];
    for bundle in bundles {
        c.bundle = bundle;
        let f = run(&c);
        let (_, _, t) = c.proofs();
        let succ = c.successor();
        assert_eq!(succ.root, t.root(), "off-chain tree root equals on-chain root");
        // The next buy spends the successor.
        c.reserve = f.tx.outputs[0].value - (succ.sales - succ.ref_paid) as u64;
        c.shard = succ;
        c.tree = t;
    }
    assert_eq!(c.tree.counts(first.rank())[2], 3);
}

#[test]
fn p6_no_change_output() {
    let mut c = case();
    c.change = false;
    c.funding = vec![c.gross() + c.b() * c.shard.d as u64 + c.fee];
    let f = run(&c);
    assert_eq!(f.tx.outputs.len(), 2);
}

#[test]
fn p7_max_funding_inputs() {
    let mut c = case();
    c.funding = vec![2 * KAS; 8];
    run(&c);
}

#[test]
fn p8_boundaries() {
    for m in [2u8, 255] {
        let mut c = case().with_tree(Tree::new(m as usize));
        let mut bundle = vec![Combo::new([1, 2, 3, 4, 5], 1), Combo::new([26, 27, 28, 29, 30], m)];
        bundle.truncate(b_max());
        c.bundle = bundle;
        run(&c);
        // The other order and the other extreme bonus.
        if b_max() >= 2 {
            c.bundle = vec![Combo::new([26, 27, 28, 29, 30], 1), Combo::new([1, 2, 3, 4, 5], m)];
            run(&c);
        }
    }
}

#[test]
fn p9_referral_threshold_and_omission() {
    // r = floor(1 KAS x 1,000 / 10^4) = 0.1 KAS = M_REF exactly.
    let mut c = case();
    c.with_ref = true;
    assert_eq!(c.referral(), M_REF);
    run(&c);
    // Referral omitted while r >= M_REF: the share stays in the shard.
    let c = case();
    let f = run(&c);
    assert_eq!(f.tx.outputs[0].value - f.entries[0].amount, c.gross());
    assert_eq!(c.successor().ref_paid, 0);
}

#[test]
fn p10_any_shard() {
    for idx in [0, 3] {
        let mut c = case();
        c.shard.shard_index = idx;
        run(&c);
    }
}

#[test]
fn p11_count_reaches_max() {
    let x = Combo::new([11, 12, 13, 14, 15], 5);
    let mut t = Tree::new(10);
    let mut counts = vec![0u16; 10];
    counts[4] = 32_766;
    t.set(x.rank(), counts);
    let mut c = case().with_tree(t);
    c.bundle = vec![x];
    run(&c);
    assert_eq!(c.proofs().2.counts(x.rank())[4], 32_767);
}

#[test]
fn p12_one_late_buy() {
    // Shard created at T - 1; no lock time involved, so the buy is valid after T.
    let mut c = case();
    c.shard_daa = c.shard.deadline as u64 - 1;
    run(&c);
}

#[test]
fn p13_round_amounts_from_sales() {
    let mut c = case();
    let mut sales = 0u64;
    let mut paid = 0u64;
    for (i, with_ref) in [true, false, true, false, false].into_iter().enumerate() {
        c.with_ref = with_ref;
        c.bundle = distinct(1 + i % b_max(), 10);
        let f = run(&c);
        let (_, _, t) = c.proofs();
        let succ = c.successor();
        sales += c.gross();
        paid += c.referral();
        assert_eq!((succ.sales as u64, succ.ref_paid as u64), (sales, paid));
        c.reserve = f.tx.outputs[0].value - (succ.sales - succ.ref_paid) as u64;
        c.shard = succ;
        c.tree = t;
    }
    let (f, cc, cd, bt, h, br) = round_amounts(sales, paid, c.shard.s_r as u64);
    assert!(br >= 0, "boost contribution b_r >= 0");
    assert_eq!(br as u64 + paid + f + cc + cd + bt + h, sales);
    // The shard holds exactly S - P_R + R.
    assert_eq!(c.shard_value(), sales - paid + c.reserve);
}

#[test]
fn payout_classes_accepted() {
    for p in [Payout::Schnorr([9; 32]), Payout::Ecdsa([2; 33]), Payout::P2sh([8; 32])] {
        let mut c = case();
        c.payout = p;
        c.with_ref = true;
        c.referrer = p;
        run(&c);
    }
    let _ = funding_spk();
}
