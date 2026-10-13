//! M2 measurements (spec M2 A7): CP11 and M2-a .. M2-d. Run with
//! `cargo test -p lottery --release --test m2_measure -- --nocapture --test-threads=1`.
//!
//! Limits per transaction (docs/costs.md line 5, confirmed on testnet-10 in
//! docs/tn10-mass-check.md): compute 500,000, storage 500,000, transient 1,000,000.
//! Masses are computed by the v2.1.0 mass calculator with mainnet parameters
//! (`harness::masses`). Compute mass depends on the committed budgets, which `finish()`
//! sizes from the real scripts' units, so every compute figure needs an accepted close.

mod common;

use common::accept;
use common::m2::*;
use common::measure::{CALLERS, close_case_c, close_storage_sup as storage_sup, r_bound, worst_storage};
use harness::{Masses, masses, priority_fee};
use lottery::buy::{Final, KAS};
use lottery::close::CloseCase;
use lottery::game::muldiv;
use lottery::params::SetParamsCase;
use lottery::state::K_MAX;

const COMPUTE_LIMIT: u64 = 500_000;
const TRANSIENT_LIMIT: u64 = 1_000_000;
const STORAGE_LIMIT: u64 = 500_000;
/// D32: K = 16 must stay within 90 % of every per-transaction mass.
const K_TARGET: usize = 16;
const MARGIN_PCT: u64 = 90;

/// A close at `k` with sales in every shard (1,000 KAS each, 5 % referral), the worst
/// realistic shape for the scripts (every shard carries sales, every root non-trivial).
fn close_k(k: usize) -> CloseCase {
    let mut c = CloseCase::new(k);
    for i in 0..k {
        c.set_sales(i, 1_000 * KAS as i64, 50 * KAS as i64);
        c.shards[i].state.root = [0x20 + i as u8; 32];
    }
    c
}

fn run(c: &CloseCase) -> (Final, Masses) {
    let f = accept(&c.build());
    let m = masses(&f.tx, &f.entries);
    (f, m)
}

fn within(m: &Masses, pct: u64) -> [bool; 3] {
    [m.compute * 100 <= COMPUTE_LIMIT * pct, m.transient * 100 <= TRANSIENT_LIMIT * pct, m.storage * 100 <= STORAGE_LIMIT * pct]
}

// ---------------------------------------------------------------- CP11

#[test]
fn cp11_k16_close_within_limits() {
    let c = close_k(K_TARGET);
    let (f, m) = run(&c);
    println!("\nCP11: K = {K_TARGET} close(), B_MAX = {}: {m:?}, inputs {}", c.b_max, f.tx.inputs.len());
    assert!(within(&m, 100).iter().all(|&ok| ok), "K = {K_TARGET} close() exceeds a per-transaction limit: {m:?}");
}

// ---------------------------------------------------------------- M2-a

#[test]
fn m2a_script_sizes_units_and_masses() {
    let t = tpl();
    let c = close_k(4);
    let x = c.expected();
    println!("\nM2-a: redeem script sizes (B_MAX = {})", t.b_max);
    println!("| object | redeem B |\n|---|---|");
    println!("| controller | {} |", t.controller_redeem(&c.controller).len());
    println!("| params | {} |", t.params_redeem(&c.params).len());
    println!("| shard (buy + close_leg) | {} |", t.shard_redeem(&c.shards[0].state).len());
    println!("| marker (placeholder) | {} |", t.marker.redeem(&x.marker.values()).len());

    println!("\nM2-a: close() per input and masses");
    println!("| K | controller units / sigscript B | params units / sigscript B | shard units max / sigscript B | compute | transient | storage | priority |");
    println!("|---|---|---|---|---|---|---|---|");
    for k in [4, K_TARGET] {
        let c = close_k(k);
        let (f, m) = run(&c);
        let sig = |i: usize| f.tx.inputs[i].signature_script.len();
        let shard_units = (0..k).map(|i| f.units[2 + i]).max().unwrap();
        println!(
            "| {k} | {} / {} | {} / {} | {shard_units} / {} | {} | {} | {} | {} |",
            f.units[0],
            sig(0),
            f.units[1],
            sig(1),
            sig(2),
            m.compute,
            m.transient,
            m.storage,
            m.priority
        );
    }

    println!("\nM2-a: set_params() masses");
    println!("| shape | params units | compute | transient | storage | priority fee (KAS) |\n|---|---|---|---|---|---|");
    for change in [true, false] {
        let mut sp = SetParamsCase::new();
        if !change {
            sp.change = false;
            sp.funding = vec![sp.fee];
        }
        let f = accept(&sp.build());
        let m = masses(&f.tx, &f.entries);
        println!(
            "| change {change} | {} | {} | {} | {} | {:.4} |",
            f.units[0],
            m.compute,
            m.transient,
            m.storage,
            priority_fee(&m) as f64 / KAS as f64
        );
    }
}

// ---------------------------------------------------------------- M2-b

#[test]
fn m2b_largest_k() {
    println!("\nM2-b: close() masses per K (B_MAX = {}, real scripts, sales in every shard)", tpl().b_max);
    println!("| K | compute | transient | storage | <= 90 % (c/t/s) |\n|---|---|---|---|---|");
    let mut k90 = [0usize; 3];
    let mut k100 = [0usize; 3];
    for k in 1..=K_MAX {
        let (_, m) = run(&close_k(k));
        let (w90, w100) = (within(&m, MARGIN_PCT), within(&m, 100));
        for l in 0..3 {
            if w90[l] && k90[l] == k - 1 {
                k90[l] = k;
            }
            if w100[l] && k100[l] == k - 1 {
                k100[l] = k;
            }
        }
        println!("| {k} | {} | {} | {} | {:?} |", m.compute, m.transient, m.storage, w90);
    }
    println!("Largest K (compute, transient, storage) at 90 %: {k90:?}; at 100 %: {k100:?} (K_MAX = {K_MAX} is the layout bound)");
    // D32: K = 16 within 90 % of every limit, else B_MAX drops to 4 and this is re-run.
    assert!(k90.iter().all(|&k| k >= K_TARGET), "D32 triggered: K = {K_TARGET} exceeds {MARGIN_PCT} % of a limit: {k90:?}");
}

// ---------------------------------------------------------------- M2-c

/// M2-c: R re-check with K + 3 fresh reserve outputs. The worst case over sales, boost pot,
/// J_boost, acc and the caller output (>= 1 KAS, builder policy) stays within 90 % of the
/// storage limit at the committed R, for every K <= K_MAX; and the committed R is the
/// closed-form bound rounded up to 0.01 KAS.
#[test]
fn m2c_reserve_recheck() {
    let r0 = lottery::params::TEST_RESERVE as i64;
    let r_need = r_bound(K_MAX);
    println!(
        "
M2-c: R bound 4C(K + 3) / (0.9 L) at K = {K_MAX}: {r_need} sompi; committed R = {r0} sompi ({:.2} KAS)",
        r0 as f64 / KAS as f64
    );
    assert!(r0 as u64 >= r_need, "committed R below the 90 % storage bound");
    assert!(r0 as u64 - r_need < KAS / 100, "committed R is not the bound rounded up to 0.01 KAS");
    assert!(r0 % (KAS as i64 / 100) == 0);
    assert!(lottery::close::MIN_CALLER_OUTPUT >= KAS);

    // The no-script storage figure equals the one finish() commits.
    let c = close_case_c(K_TARGET, r0, 0, 0, 0, 0, KAS).unwrap();
    assert_eq!(c.build().storage_mass(), masses(&c.build().finish().tx, &c.build().finish().entries).storage);

    println!("| K | caller output (KAS) | worst storage at R | % of limit | worst shape (sales/shard, O, J_boost, A) | sup 4C(K+3)/R |");
    println!("|---|---|---|---|---|---|");
    for k in 1..=K_MAX {
        let cls: Vec<u64> = if k == 4 || k == K_TARGET { CALLERS.to_vec() } else { vec![] };
        for &cl in &cls {
            let (st, (s, o, jb, a, _)) = worst_storage(k, r0, &[cl]);
            println!(
                "| {k} | {:.0} | {st} | {:.1} | ({:.0}, {:.0}, {:.0}, {:.0}) | {} |",
                cl as f64 / KAS as f64,
                st as f64 * 100.0 / STORAGE_LIMIT as f64,
                s as f64 / KAS as f64,
                o as f64 / KAS as f64,
                jb as f64 / KAS as f64,
                a as f64 / KAS as f64,
                storage_sup(k, r0)
            );
        }
        let (st, shape) = worst_storage(k, r0, &CALLERS);
        assert!(st <= storage_sup(k, r0), "K = {k}: grid worst {st} above the supremum");
        assert!(st * 100 <= STORAGE_LIMIT * MARGIN_PCT, "K = {k}: storage {st} at {shape:?} exceeds {MARGIN_PCT} %");
    }
    assert!(storage_sup(K_MAX, r0) * 100 <= STORAGE_LIMIT * MARGIN_PCT);
    // The old R = 1.44 KAS fails the same check (the reason R was raised).
    let (old, _) = worst_storage(K_TARGET, 144_000_000, &CALLERS);
    println!("At the M1 R = 1.44 KAS, K = {K_TARGET}: worst {old} ({:.1} %)", old as f64 * 100.0 / STORAGE_LIMIT as f64);
    assert!(old * 100 > STORAGE_LIMIT * MARGIN_PCT);
}

// ---------------------------------------------------------------- M2-d

#[test]
fn m2d_priority_fee_vs_c_close() {
    println!("\nM2-d: close() priority fee (min feerate x priority mass) vs C_close = s_C x S (s_C = 50 bp)");
    println!("| K | priority mass | priority fee (KAS) | S where C_close covers it (KAS) |\n|---|---|---|---|");
    for k in [4, K_TARGET] {
        let c = close_k(k);
        let (_, m) = run(&c);
        let fee = priority_fee(&m) as i64;
        // Smallest S with muldiv(S, s_C) >= fee.
        let s_c = c.controller.s_c;
        let (mut lo, mut hi) = (0i64, 1i64 << 50);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if muldiv(mid, s_c) >= fee { hi = mid } else { lo = mid + 1 }
        }
        println!("| {k} | {} | {:.4} | {:.2} |", m.priority, fee as f64 / KAS as f64, lo as f64 / KAS as f64);
    }
}
