//! M4 measurements M4-a .. M4-h (spec M4 A9; M4-i is the testnet-10 probe `claim-m4`).
//! Run with `cargo test -p lottery --release --test m4_measure -- --nocapture --test-threads=1`.
//!
//! Limits per transaction, rusty-kaspa v2.1.0 (as m3_measure.rs): compute 500,000, storage
//! 500,000, transient 1,000,000 (`consensus/core/src/config/params.rs:626`, `:683`;
//! `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:88-95`); storage
//! mass KIP-9 (`consensus/core/src/mass/mod.rs:439-512`), C = 10^12
//! (`consensus/core/src/constants.rs:26`). Masses come from the v2.1.0 mass calculator with
//! mainnet parameters (`harness::masses`); compute follows the committed budgets, which
//! `finish()` sizes from the real scripts' units, so every compute figure is of an accepted
//! transaction.
//!
//! Stop rules (D64, user decision U2): `close()` K = 16, `draw()` DP21 and `buy()` compute
//! and transient must each be <= 90 % of every limit (the tests assert it). `buy()` storage
//! was 95.7 % at b = 1, d = 0.11 KAS, 0.1 KAS referral (D15, U2); M6 (D80, D81) raises d to
//! D_FLOOR = 0.13 KAS, and every `buy()` shape, the b = 1 referral ones included, is now
//! asserted <= 90 % of storage. P_MIN (M4-c) is reported; the constant is not changed here
//! (a user decision if it differs).

mod common;

use std::sync::Arc;

use common::m2::*;
use common::m4::{refresh_marker, refresh_pool, refresh_ticket};
use common::measure::{pay_storage, worst_claim};
use harness::{Masses, masses, priority_fee};
use lottery::buy::{BuyCase, Built, Final, KAS, Sources, TEST_DEPOSIT, Templates, b_max};
use lottery::claim::*;
use lottery::close::{CloseCase, FundingIn};
use lottery::draw::DrawCase;
use lottery::game::{A_MAX, ClaimMode, M_CLAIM, M_DRAW, P_MIN};
use lottery::params::TEST_RESERVE;
use lottery::smt::Tree;
use lottery::state::K_MAX;

const COMPUTE_LIMIT: u64 = 500_000;
const TRANSIENT_LIMIT: u64 = 1_000_000;
const STORAGE_LIMIT: u64 = 500_000;
const MARGIN_PCT: u64 = 90;
const F_MAX: usize = 8;

fn within(m: &Masses, pct: u64) -> [bool; 3] {
    [m.compute * 100 <= COMPUTE_LIMIT * pct, m.transient * 100 <= TRANSIENT_LIMIT * pct, m.storage * 100 <= STORAGE_LIMIT * pct]
}

fn pct(m: &Masses) -> (f64, f64, f64) {
    (
        m.compute as f64 * 100.0 / COMPUTE_LIMIT as f64,
        m.transient as f64 * 100.0 / TRANSIENT_LIMIT as f64,
        m.storage as f64 * 100.0 / STORAGE_LIMIT as f64,
    )
}

fn run(step: &str, b: &Built) -> (Final, Masses) {
    let f = accept_m4(step, b);
    let m = masses(&f.tx, &f.entries);
    (f, m)
}

fn kas(s: u64) -> f64 {
    s as f64 / KAS as f64
}

// ---------------------------------------------------------------- M4-a

/// The committed ticket source with K_MAX and M_CLAIM replaced; returns the redeem length
/// of the standalone ticket state under it.
fn ticket_variant(k_max: usize, m_claim: i64) -> usize {
    let mut src = Sources::committed(b_max());
    let k_line = "    int constant K_MAX = 16;";
    let m_line = "    int constant M_CLAIM = 50;";
    assert!(src.ticket.contains(k_line) && src.ticket.contains(m_line));
    src.ticket = src.ticket.replace(k_line, &format!("    int constant K_MAX = {k_max};")).replace(m_line, &format!("    int constant M_CLAIM = {m_claim};"));
    let t = Templates::compile_sources(b_max(), &src);
    t.ticket.redeem(&ClaimCase::new(1).ticket.values()).len()
}

#[test]
fn m4a_redeem_sizes() {
    let t = tpl();
    let c = ClaimCase::new(4);
    let close = CloseCase::new(4);
    let draw = DrawCase::new(4);
    println!("\nM4-a: redeem script sizes (B_MAX = {})", t.b_max);
    println!("| object | prefix B | suffix B | state B | redeem B |\n|---|---|---|---|---|");
    let row = |name: &str, tp: &lottery::buy::Template, redeem: usize| {
        println!("| {name} | {} | {} | {} | {redeem} |", tp.prefix.len(), tp.suffix.len(), redeem - tp.prefix.len() - tp.suffix.len());
    };
    row("ticket (claim, return x2, refresh)", &t.ticket, t.ticket.redeem(&c.ticket.values()).len());
    row("pool (absorb_leg, ticket_leg, refresh)", &t.pool, t.pool_redeem(&c.pool).len());
    row("marker (draw_leg, refresh)", &t.marker, t.marker_redeem(&close.expected().marker).len());
    row("drawer (draw_test_seed)", &t.drawer, t.drawer_redeem(&draw.drawer).len());
    row("vault (draw_leg)", &t.vault, t.vault_redeem(&draw.vault).len());
    row("controller (close, draw_leg)", &t.controller, t.controller_redeem(&close.controller).len());
    row("shard (buy, close_leg)", &t.shard, t.shard_redeem(&BuyCase::new(t.b_max).shard).len());

    println!("\nM4-a: ticket redeem B by K_MAX (rows) and M_CLAIM (columns)");
    println!("| K_MAX | M_CLAIM 10 | 25 | 50 |\n|---|---|---|---|");
    let mut committed = 0;
    for k in [4, 8, 16] {
        let row: Vec<usize> = [10, 25, 50].iter().map(|&m| ticket_variant(k, m)).collect();
        if k == 16 {
            committed = row[2];
        }
        println!("| {k} | {} | {} | {} |", row[0], row[1], row[2]);
    }
    assert_eq!(committed, t.ticket.redeem(&c.ticket.values()).len(), "the (16, 50) variant is the committed ticket");
}

// ---------------------------------------------------------------- M4-b

#[test]
fn m4b_claim_units_and_masses() {
    println!("\nM4-b: claim() per-input script units and masses (worst proofs: h = K, every leaf non-empty; one funding input)");
    println!("| mode | K | m | proof B k(576+2m) | ticket units / sigscript B | pool leg units | compute | transient | storage | priority | % c / t / s |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    for mode in [ClaimMode::Pay, ClaimMode::Accrue, ClaimMode::Final] {
        for k in [4, K_MAX] {
            for m in [10, M_CLAIM] {
                let c = worst_claim(k, m, mode, 1);
                let (f, ms) = run("claim (M4-b)", &c.build());
                let (pc, pt, ps) = pct(&ms);
                println!(
                    "| {mode:?} | {k} | {m} | {} | {} / {} | {} | {} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} |",
                    k * (576 + 2 * m as usize),
                    f.units[0],
                    f.tx.inputs[0].signature_script.len(),
                    f.units[1],
                    ms.compute,
                    ms.transient,
                    ms.storage,
                    ms.priority
                );
                assert!(within(&ms, 100).iter().all(|&ok| ok), "{mode:?} K = {k}, m = {m}: {ms:?}");
            }
        }
    }
    println!("\nM4-b: K = {K_MAX}, m = {M_CLAIM}, PAY with F_MAX = {F_MAX} funding inputs");
    let c = worst_claim(K_MAX, M_CLAIM, ClaimMode::Pay, F_MAX);
    let (f, ms) = run("claim (M4-b, F_MAX)", &c.build());
    let (pc, pt, ps) = pct(&ms);
    println!("units ticket {} / pool {}; {ms:?} ({pc:.1} % / {pt:.1} % / {ps:.1} %)", f.units[0], f.units[1]);
    assert!(within(&ms, MARGIN_PCT).iter().all(|&ok| ok), "claim K = 16, m = 50 above 90 %: {ms:?}");
}

// ---------------------------------------------------------------- M4-c

#[test]
fn m4c_p_min() {
    let budget = STORAGE_LIMIT * MARGIN_PCT / 100;
    println!("\nM4-c: worst PAY claim storage (b = 2, d = {} sompi, ticket' {} sompi, pool' at R = {} sompi, one funding input) by payout P", TEST_DEPOSIT, 2 * TEST_DEPOSIT, TEST_RESERVE);
    println!("| change | P = 0.03 KAS | 0.04 | 0.0426 | 0.05 (P_MIN) | 0.1 | smallest P <= 90 % (sompi) | rounded up to 0.01 KAS |");
    println!("|---|---|---|---|---|---|---|---|");
    let mut worst_needed = 0u64;
    // A larger change lowers the KIP-9 input relief (|I|^2 C / sum I) more than it lowers its
    // own C / change term, so the supremum is approached as the change grows (1,000,000 KAS
    // row; payout and change scripts are standard, plurality 1, mass/mod.rs:83-99).
    for (name, change) in [("1 KAS", KAS), ("1,000 KAS", 1_000 * KAS), ("1,000,000 KAS", 1_000_000 * KAS)] {
        let cols: Vec<String> = [3_000_000u64, 4_000_000, 4_260_000, P_MIN as u64, 10_000_000].iter().map(|&p| pay_storage(p, change).to_string()).collect();
        // Smallest P with storage <= 90 % (storage falls as P grows).
        let (mut lo, mut hi) = (1u64, 100 * KAS);
        assert!(pay_storage(hi, change) <= budget);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if pay_storage(mid, change) <= budget { hi = mid } else { lo = mid + 1 }
        }
        let rounded = lo.div_ceil(KAS / 100) * (KAS / 100);
        worst_needed = worst_needed.max(rounded);
        println!("| {name} | {} | {} | {} | {} | {} | {lo} ({:.6} KAS) | {:.2} KAS |", cols[0], cols[1], cols[2], cols[3], cols[4], kas(lo), kas(rounded));
    }
    let at = [KAS, 1_000 * KAS, 1_000_000 * KAS].iter().map(|&ch| pay_storage(P_MIN as u64, ch)).max().unwrap();
    println!(
        "P_MIN (committed) = {P_MIN} sompi: worst PAY storage {at} ({:.1} %). Measured P_MIN (smallest P within 90 %, rounded up to 0.01 KAS) = {worst_needed} sompi ({:.2} KAS): {}",
        at as f64 * 100.0 / STORAGE_LIMIT as f64,
        kas(worst_needed),
        if worst_needed == P_MIN as u64 { "D57 confirmed" } else { "DIFFERS from the committed P_MIN (user decision; constant unchanged)" }
    );
    assert!(at <= budget, "the committed P_MIN's worst PAY claim exceeds 90 % of storage: {at}");
}

// ---------------------------------------------------------------- M4-d

fn buy_report(name: &str, c: &BuyCase, storage_capped: bool) {
    let f = common::accept(&c.build());
    let m = masses(&f.tx, &f.entries);
    let (pc, pt, ps) = pct(&m);
    println!("| {name} | {} | {} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} |", f.units[0], f.tx.inputs[0].signature_script.len(), m.compute, m.transient, m.storage);
    // D64 for buy(): the masses M4 changes (compute, transient) <= 90 %; storage <= 90 %
    // (M6 D80: every shape at d = D_FLOOR; `storage_capped = false` keeps only the limit).
    let [c_ok, t_ok, s_ok] = within(&m, MARGIN_PCT);
    assert!(c_ok && t_ok, "M4-d (D64 stop): {name}: buy() compute or transient above 90 %: {m:?}");
    if storage_capped {
        assert!(s_ok, "M4-d: {name}: storage above 90 %: {m:?}");
    } else {
        assert!(within(&m, 100)[2], "M4-d: {name}: storage above the limit: {m:?}");
    }
}

#[test]
fn m4d_buy_with_the_m4_ticket() {
    let t = tpl();
    println!(
        "\nM4-d: buy() with the M4 ticket template (ticket redeem {} B; d = {} sompi, price 1 KAS, m = 10)",
        t.ticket.redeem(&ClaimCase::new(1).ticket.values()).len(),
        TEST_DEPOSIT
    );
    println!("| shape | shard units | shard sigscript B | compute | transient | storage | % c / t / s |\n|---|---|---|---|---|---|---|");
    let bm = b_max();
    for (b, with_ref, small_change) in [(1, false, false), (1, true, false), (1, true, true), (bm, true, false), (bm, true, true)] {
        let mut c = BuyCase::new(bm).with_tree(Tree::new(10));
        c.bundle = common::distinct(b, 10);
        c.with_ref = with_ref;
        if small_change {
            c.funding = vec![c.gross() + c.b() * c.shard.d as u64 + c.fee + KAS];
        }
        let name = format!("b = {b}, referral {}, change {}", if with_ref { "0.1 x b KAS" } else { "none" }, if small_change { "1 KAS" } else { "large" });
        // U2 closed by M6 (D80): at d = 0.13 KAS the b = 1 referral shapes are <= 90 % too
        // (was asserted <= 100 % only at d = 0.11 KAS).
        buy_report(&name, &c, true);
    }
}

// ---------------------------------------------------------------- M4-e

/// ST_ALL with every lower tier funded (np = 7).
const ST_ALL_DRAW: [i64; 5] = [400, 300, 300, 200, 200];

/// The DP21 worst shape (m3_measure.rs `worst`): round 2, K shards, marker m, every leaf at
/// the winning rank non-empty with every bonus held, sales 1,000,000 KAS, `a` absorbed
/// pools (with the M4 pool code: absorb_leg, ticket_leg, refresh), `nf` funding inputs.
fn dp21(k: usize, m: i64, a: usize, nf: usize, np: usize) -> DrawCase {
    let mut c = DrawCase::at_round(k, 2);
    if np == 7 {
        c.marker.st = ST_ALL_DRAW;
    }
    c.marker.m = m;
    let rank = c.seed().rank;
    c.trees = (0..k)
        .map(|_| {
            let mut t = Tree::new(m as usize);
            t.set(rank, vec![1; m as usize]);
            t
        })
        .collect();
    c.sync_roots();
    c.set_sales(1_000_000 * KAS as i64, 50_000 * KAS as i64);
    if a > 0 {
        c.absorb(a);
    }
    let daa = c.funding[0].daa;
    c.funding = vec![FundingIn { value: 100 * KAS, daa }; nf];
    let x = c.expected();
    assert_eq!((x.h_j, x.n2, x.np(), x.a), (k as i64, m - 1, np, a));
    c
}

#[test]
fn m4e_dp21_a_max_with_the_m4_pool() {
    let t = tpl();
    println!("\nM4-e: DP21 with the M4 pool ({} B redeem) and marker; K = {K_MAX}, m = {M_DRAW}, a = A_MAX = {A_MAX}", t.pool_redeem(&ClaimCase::new(1).pool).len());
    println!("| np | funding inputs | drawer units | pool leg units (max) | compute | transient | storage | % c / t / s | <= 90 % |\n|---|---|---|---|---|---|---|---|---|");
    for (np, nf) in [(5, 1), (5, F_MAX), (7, 1), (7, F_MAX)] {
        let c = dp21(K_MAX, M_DRAW, A_MAX, nf, np);
        let f = accept_draw(&c.build());
        let m = masses(&f.tx, &f.entries);
        let pool = (0..A_MAX).map(|i| f.units[4 + i]).max().unwrap();
        let ok = within(&m, MARGIN_PCT).iter().all(|&b| b);
        let (pc, pt, ps) = pct(&m);
        println!("| {np} | {nf} | {} | {pool} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {ok} |", f.units[0], m.compute, m.transient, m.storage);
        assert!(ok, "M4-e (D64 stop): DP21 np = {np}, {nf} funding inputs above 90 %: {m:?}");
    }
    println!("A_MAX = {A_MAX} stands (D44).");
}

// ---------------------------------------------------------------- M4-f

#[test]
fn m4f_close_k16_with_the_m4_marker() {
    let t: Arc<Templates> = tpl();
    println!("\nM4-f: close() with the M4 marker ({} B redeem)", t.marker.redeem(&CloseCase::new(4).expected().marker.values()).len());
    println!("| K | controller units | compute | transient | storage | % c / t / s | <= 90 % |\n|---|---|---|---|---|---|---|");
    for k in [4, K_MAX] {
        let mut c = CloseCase::new(k);
        for i in 0..k {
            c.set_sales(i, 1_000 * KAS as i64, 50 * KAS as i64);
            c.shards[i].state.root = [0x20 + i as u8; 32];
        }
        let f = common::accept(&c.build());
        let m = masses(&f.tx, &f.entries);
        let ok = within(&m, MARGIN_PCT).iter().all(|&b| b);
        let (pc, pt, ps) = pct(&m);
        println!("| {k} | {} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {ok} |", f.units[0], m.compute, m.transient, m.storage);
        if k == K_MAX {
            println!("transient headroom below 90 %: {} grams", TRANSIENT_LIMIT * MARGIN_PCT / 100 - m.transient.min(TRANSIENT_LIMIT * MARGIN_PCT / 100));
            assert!(ok, "M4-f (D64 stop): K = 16 close() above 90 %: {m:?}");
        }
    }
}

// ---------------------------------------------------------------- M4-g

#[test]
fn m4g_return_and_refresh() {
    println!("\nM4-g: return_deposit() and refresh() (one funding input, change)");
    println!("| transaction | leader units / sigscript B | pool leg units | compute | transient | storage | % c / t / s |\n|---|---|---|---|---|---|---|");
    let shapes: Vec<(&str, Built)> = vec![
        ("return_deposit_after (b = 2 ticket)", ReturnCase::after().build()),
        ("return_deposit_with_pool (b = 2 ticket, K = 1 pool)", ReturnCase::with_pool().build()),
        ("refresh ticket", refresh_ticket().build()),
        ("refresh pool (K = 4)", refresh_pool().build()),
        ("refresh marker (K = 4)", refresh_marker().build()),
    ];
    for (name, b) in shapes {
        let (f, m) = run(name, &b);
        let (pc, pt, ps) = pct(&m);
        let leg = if name.contains("with_pool") { f.units[1].to_string() } else { "-".into() };
        println!("| {name} | {} / {} | {leg} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} |", f.units[0], f.tx.inputs[0].signature_script.len(), m.compute, m.transient, m.storage);
        assert!(within(&m, MARGIN_PCT).iter().all(|&ok| ok), "{name}: {m:?}");
    }
    // A K = 16 marker refresh (the largest marker state) for scale.
    let mut c = CloseCase::new(K_MAX);
    for i in 0..K_MAX {
        c.shards[i].state.root = [0x20 + i as u8; 32];
    }
    let r = RefreshCase::new(RefreshObj::Marker(c.expected().marker), 50 * KAS, 1_500_000);
    let (f, m) = run("refresh marker K = 16", &r.build());
    let (pc, pt, ps) = pct(&m);
    println!("| refresh marker (K = 16) | {} / {} | - | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} |", f.units[0], f.tx.inputs[0].signature_script.len(), m.compute, m.transient, m.storage);
}

// ---------------------------------------------------------------- M4-h

#[test]
fn m4h_priority_fee_vs_prizes() {
    println!("\nM4-h: claim() priority fee (min feerate x priority mass) against P_MIN and typical lower-tier prizes (informational)");
    println!("| shape | priority mass | priority fee (KAS) | % of P_MIN | % of 0.1 KAS | % of 1 KAS |\n|---|---|---|---|---|---|");
    let shapes: Vec<(String, ClaimCase)> = vec![
        ("PAY, K = 4, m = 10".into(), worst_claim(4, 10, ClaimMode::Pay, 1)),
        (format!("PAY, K = 16, m = {M_CLAIM}"), worst_claim(K_MAX, M_CLAIM, ClaimMode::Pay, 1)),
        (format!("ACCRUE, K = 16, m = {M_CLAIM}"), worst_claim(K_MAX, M_CLAIM, ClaimMode::Accrue, 1)),
        (format!("FINAL, K = 16, m = {M_CLAIM}"), worst_claim(K_MAX, M_CLAIM, ClaimMode::Final, 1)),
    ];
    for (name, c) in shapes {
        let (_, m) = run(&name, &c.build());
        let fee = priority_fee(&m);
        println!(
            "| {name} | {} | {:.5} | {:.1} | {:.1} | {:.2} |",
            m.priority,
            kas(fee),
            fee as f64 * 100.0 / P_MIN as f64,
            fee as f64 * 100.0 / (KAS / 10) as f64,
            fee as f64 * 100.0 / KAS as f64
        );
    }
    // The worst PAY storage at P = P_MIN dominates the priority mass of a small claim.
    let st = pay_storage(P_MIN as u64, 1_000_000 * KAS);
    // Priority fee = priority mass x MIN_FEERATE_SOMPI_PER_GRAM (harness::priority_fee).
    let fee = st * harness::MIN_FEERATE_SOMPI_PER_GRAM;
    println!("Worst PAY claim at P = P_MIN: storage {st}; if storage is its priority mass the priority fee is {:.5} KAS ({:.1} % of P_MIN)", kas(fee), fee as f64 * 100.0 / P_MIN as f64);
}
