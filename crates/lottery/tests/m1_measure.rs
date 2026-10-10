//! M1 measurements M-a .. M-d (spec M1 8.3). Run with
//! `cargo test -p lottery --test m1_measure -- --nocapture --test-threads=1`.

mod common;

use harness::{
    CovenantBinding, Hash, Masses, ScriptPublicKey, Transaction, TransactionOutput, UtxoEntry, budget_for, input, masses,
    pay_to_script_hash_script, priority_fee, tx,
};
use lottery::buy::{BuyCase, KAS, Templates, b_max};
use lottery::smt::Tree;

const COMPUTE_LIMIT: u64 = 500_000;
const TRANSIENT_LIMIT: u64 = 1_000_000;
const STORAGE_LIMIT: u64 = 500_000;
/// Allowance for the M2 close leg added to the shard's sigscript (spec M-b).
const CLOSE_LEG_ALLOWANCE: usize = 300;
/// Target K and the margin of D7.
const K_TARGET: usize = 16;
const MARGIN_PCT: u64 = 90;
const R_KAS: f64 = lottery::params::TEST_RESERVE as f64 / KAS as f64;

fn sigscript_len(b: &lottery::buy::Built) -> usize {
    b.finish().tx.inputs[0].signature_script.len()
}

/// A buy of `b` distinct combinations at bonus range `m` under templates with `b_max` slots.
fn buy_case(b_max: usize, b: usize, m: usize) -> BuyCase {
    let mut c = BuyCase::new(b_max).with_tree(Tree::new(m));
    c.bundle = common::distinct(b, m.min(255) as u8);
    c
}

/// Bytes the shard's own redeem script takes inside a sigscript (push opcode included).
fn redeem_push_len(t: &Templates) -> usize {
    let redeem = t.shard_redeem(&BuyCase::new(t.b_max).shard);
    harness::ScriptBuilder::canonical_data_size(&redeem) + 0
}

// ---------------------------------------------------------------- close() model (M0 Q5 method)

fn cov_spk() -> ScriptPublicKey {
    pay_to_script_hash_script(&[0x51])
}

fn p2pk() -> ScriptPublicKey {
    lottery::state::Payout::Schnorr([0x11; 32]).spk()
}

const COV: Hash = Hash::from_bytes([5; 32]);

/// close() with K shard inputs whose sigscripts are `shard_sig` bytes, and an M0-sized controller
/// (redeem ~ 190 + 420 K). Reserves R = TEST_RESERVE, sales 10,000 KAS, caller funding 1,000 KAS.
fn close_tx(k: usize, shard_sig: usize) -> (Transaction, Vec<UtxoEntry>) {
    let r = lottery::params::TEST_RESERVE;
    let sales = 10_000 * KAS;
    let ctl_sig = 190 + 420 * k + 4;
    let ctl_budget = budget_for(1_550 + 3_840 * k as u64 + ctl_sig as u64);
    // The close leg pushes the shard's redeem script (charged 1 unit per pushed byte) plus a small leg.
    let shard_budget = budget_for(shard_sig as u64 + 2_000);
    let mut inputs = vec![input(0, vec![0; ctl_sig], ctl_budget)];
    let mut entries = vec![UtxoEntry::new(r, cov_spk(), 0, false, Some(COV))];
    for i in 0..k {
        inputs.push(input(1 + i as u32, vec![0; shard_sig], shard_budget));
        entries.push(UtxoEntry::new(r + sales / k as u64, cov_spk(), 0, false, Some(COV)));
    }
    inputs.push(input(1 + k as u32, vec![0; 66], budget_for(100_000)));
    entries.push(UtxoEntry::new(r + 1_000 * KAS, p2pk(), 0, false, None));
    let cov_out = |v: u64, a: u16| TransactionOutput::with_covenant(v, cov_spk(), Some(CovenantBinding::new(a, COV)));
    let mut outputs = vec![cov_out(r + sales - sales / 200, 0), cov_out(r, 0)];
    for i in 0..k {
        outputs.push(cov_out(r, 1 + i as u16));
    }
    outputs.push(TransactionOutput::new(1_000 * KAS + sales / 200 - KAS, p2pk()));
    (tx(inputs, outputs), entries)
}

fn close_masses(k: usize, shard_sig: usize) -> Masses {
    let (t, e) = close_tx(k, shard_sig);
    masses(&t, &e)
}

fn within(m: &Masses, pct: u64) -> bool {
    m.compute * 100 <= COMPUTE_LIMIT * pct && m.transient * 100 <= TRANSIENT_LIMIT * pct
}

/// Largest K whose close() stays within `pct` % of the compute and transient limits.
fn largest_k(shard_sig: usize, pct: u64) -> usize {
    (1..200).take_while(|&k| within(&close_masses(k, shard_sig), pct)).last().unwrap_or(0)
}

/// Close-leg sigscript of a shard: its redeem push plus the leg allowance.
fn close_sig(t: &Templates) -> usize {
    redeem_push_len(t) + CLOSE_LEG_ALLOWANCE
}

#[test]
fn ma_mb_script_size_and_k_per_b_max() {
    println!("\nM-a / M-b: shard script per B_MAX (close() at K = {K_TARGET}, <= {MARGIN_PCT} % of compute and transient)");
    println!("| B_MAX | redeem B | close sigscript B | buy b=1 m=10: units / sigscript B | buy b=B_MAX m=10: units / sigscript B | buy b=B_MAX m=100: units | largest K (90 %) | largest K (100 %) | K=16 transient | K=16 compute |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    let mut chosen = 0;
    for bm in 1..=8 {
        let t = Templates::get(bm);
        let redeem = t.shard_redeem(&BuyCase::new(bm).shard).len();
        let one = buy_case(bm, 1, 10).build();
        let full = buy_case(bm, bm, 10).build();
        let full100 = buy_case(bm, bm, 100).build();
        let (f1, ff, ff100) = (one.finish(), full.finish(), full100.finish());
        common::accept(&full100);
        let cs = close_sig(&t);
        let k90 = largest_k(cs, MARGIN_PCT);
        let k100 = largest_k(cs, 100);
        let m16 = close_masses(K_TARGET, cs);
        println!(
            "| {bm} | {redeem} | {cs} | {} / {} | {} / {} | {} | {k90} | {k100} | {} | {} |",
            f1.units[0],
            sigscript_len(&one),
            ff.units[0],
            sigscript_len(&full),
            ff100.units[0],
            m16.transient,
            m16.compute
        );
        if k90 >= K_TARGET {
            chosen = bm;
        }
    }
    println!("Largest B_MAX with K = {K_TARGET} at <= {MARGIN_PCT} %: {chosen}; committed B_MAX = {}", b_max());
    // D7 as recorded in DECISIONS: the committed B_MAX must keep K = 16 within 90 % in this model.
    // M1 committed 5 (6 passed by 712 bytes against M0's controller probe). M2 (D32, Stage 4)
    // lowered it to 4: the shard redeem now carries the real close_leg, and the real M2
    // controller put a K = 16 close() above 90 % of transient mass at 5 (m2_measure M2-b).
    assert!(b_max() <= chosen, "committed B_MAX breaks K = {K_TARGET} at {MARGIN_PCT} %");
    assert!(largest_k(close_sig(&Templates::get(b_max())), MARGIN_PCT) >= K_TARGET);
}

// ---------------------------------------------------------------- M-c

fn report(name: &str, c: &BuyCase) {
    let f = common::accept(&c.build());
    let m = masses(&f.tx, &f.entries);
    assert!(m.storage <= STORAGE_LIMIT && m.transient <= TRANSIENT_LIMIT && m.compute <= COMPUTE_LIMIT, "{name}: {m:?}");
    println!(
        "| {name} | {} | {} | {} | {} | {:.4} |",
        m.compute,
        m.transient,
        m.storage,
        f.units[0],
        priority_fee(&m) as f64 / KAS as f64
    );
}

/// Storage mass of a buy with deposit `d` per combination; `small_change` uses a ~1 KAS change.
fn buy_storage(b: usize, with_ref: bool, small_change: bool, d: u64) -> u64 {
    let mut c = buy_case(b_max(), b, 10);
    c.shard.d = d as i64;
    c.with_ref = with_ref;
    if small_change {
        c.funding = vec![c.gross() + c.b() * d + c.fee + KAS];
    }
    let f = c.build().finish();
    masses(&f.tx, &f.entries).storage
}

/// Smallest d (sompi) keeping storage <= 500,000 for every shape with bundle size b.
fn min_deposit(b: usize, with_ref: bool) -> u64 {
    let ok = |d: u64| [false, true].iter().all(|&s| buy_storage(b, with_ref, s, d) <= STORAGE_LIMIT);
    let (mut lo, mut hi) = (1u64, KAS);
    assert!(ok(hi));
    while lo < hi {
        let mid = (lo + hi) / 2;
        if ok(mid) { hi = mid } else { lo = mid + 1 }
    }
    lo
}

#[test]
fn mc_deposit_floor() {
    println!("\nM-c: smallest ticket deposit d per combination, fresh shard at R = {:.2} KAS, worst change", R_KAS);
    println!("| b | no referral (KAS) | with referral 0.1 x b KAS (KAS) |\n|---|---|---|");
    for b in 1..=b_max() {
        let (a, r) = (min_deposit(b, false), min_deposit(b, true));
        println!("| {b} | {:.4} ({a}) | {:.4} ({r}) |", a as f64 / KAS as f64, r as f64 / KAS as f64);
    }
}

#[test]
fn mc_buy_masses_and_referral_minimum() {
    println!("\nM-c: buy() masses, d = 0.11 KAS, price 1 KAS, fresh shard at R = {:.2} KAS (K = 16)", R_KAS);
    println!("| shape | compute | transient | storage | shard units | fee for priority (KAS) |");
    println!("|---|---|---|---|---|---|");
    let bm = b_max();
    for (b, with_ref, small_change) in [(1, false, false), (1, true, false), (1, true, true), (bm, true, false), (bm, true, true)] {
        let mut c = buy_case(bm, b, 10);
        c.with_ref = with_ref;
        if small_change {
            // Worst case for storage: change of about 1 KAS instead of a large one.
            c.funding = vec![c.gross() + c.b() * c.shard.d as u64 + c.fee + KAS];
        }
        let name = format!("b = {b}, referral {}, change {}", if with_ref { "0.1 x b KAS" } else { "none" }, if small_change { "1 KAS" } else { "large" });
        report(&name, &c);
    }
    let mut c = buy_case(bm, bm, 100);
    c.with_ref = true;
    report(&format!("b = {bm}, m = 100, referral, change large"), &c);
}

// ---------------------------------------------------------------- M-d

/// Script units of verifying 16 depth-18 paths (leaf at m = 10) with the given hash.
fn path_units(hash: &str) -> (u64, usize) {
    let src = std::fs::read_to_string(harness::covenant_path("m1/paths_bench.sil")).unwrap().replace("blake2b(", &format!("{hash}("));
    let mut trees = vec![];
    let mut leaves = vec![];
    let mut sibs = vec![];
    let mut roots = vec![];
    let x = lottery::combo::Combo::new([3, 7, 12, 19, 28], 4);
    for k in 0..16u16 {
        let mut t = Tree::new(10);
        t.set(x.rank(), (0..10).map(|i| (i + k) % 3).collect());
        t.set(x.rank() + 1, vec![1; 10]);
        leaves.extend(t.counts_bytes(x.rank()));
        sibs.extend(t.siblings(x.rank()));
        roots.extend(t.root());
        trees.push(t);
    }
    // Off-chain roots are blake2b; for the blake3 variant the bench only measures cost, so it
    // checks nothing against the roots (see paths_bench.sil).
    let art = harness::compile_source(&src, &[]);
    let args = vec![
        harness::ArtifactValue::Int(x.rank() as i64),
        harness::ArtifactValue::Bytes(leaves),
        harness::ArtifactValue::Bytes(sibs),
        harness::ArtifactValue::Bytes(roots),
    ];
    let sig = harness::entry_sigscript(&art, "verify", &args);
    let spk = harness::p2sh(&art);
    let t = tx(vec![input(0, sig.clone(), 0)], vec![TransactionOutput::new(KAS, p2pk())]);
    let e = vec![UtxoEntry::new(2 * KAS, spk, 0, false, None)];
    let units = harness::execute(&t, &e, 0, false).expect("bench runs");
    (units, sig.len())
}

#[test]
fn md_blake3_vs_blake2b() {
    let (b2, len) = path_units("blake2b");
    let (b3, _) = path_units("blake3");
    let saving = 100.0 * (b2 as f64 - b3 as f64) / b2 as f64;
    println!("\nM-d: 16 paths of depth 18, leaf at m = 10, verify only (sigscript {len} B)");
    println!("| hash | script units |\n|---|---|\n| OpBlake2b | {b2} |\n| OpBlake3 | {b3} |");
    println!("Saving with OpBlake3: {saving:.1} % (switch threshold 25 %, spec D12)");
}
