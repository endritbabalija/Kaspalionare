//! M3 measurements (spec M3 A8): DP21 and M3-a .. M3-f. Run with
//! `cargo test -p lottery --release --test m3_measure -- --nocapture --test-threads=1`.
//!
//! Limits per transaction, rusty-kaspa v2.1.0 (refs/rusty-kaspa-v2.1.0):
//! - compute 500,000, storage 500,000, transient 1,000,000:
//!   `consensus/core/src/config/params.rs:626` (MAINNET_PARAMS) and `:683` (TESTNET_PARAMS),
//!   enforced per block in `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:88-95`
//!   (a transaction above a block limit can never be mined);
//! - storage mass (KIP-9): `consensus/core/src/mass/mod.rs:439-512`, plurality `:83-99`,
//!   C = STORAGE_MASS_PARAMETER = 10^12 (`consensus/core/src/constants.rs:26`);
//! - a zero-value output is invalid: `consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs:150-151`.
//!
//! Masses come from the v2.1.0 mass calculator with mainnet parameters (`harness::masses`).
//! Compute mass depends on the committed budgets, which `finish()` sizes from the real
//! scripts' units, so every compute figure is of an accepted draw.

mod common;

use std::sync::Arc;

use common::m2::*;
use harness::{Masses, masses, priority_fee};
use lottery::buy::{Final, KAS, Sources, b_max};
use lottery::close::{CloseCase, FundingIn, MIN_CALLER_OUTPUT};
use lottery::draw::{DrawCase, compile_drawer};
use lottery::game::{A_MAX, F_MIN, F_MIN_FLOOR, M_DRAW, muldiv};
use lottery::smt::Tree;
use lottery::state::K_MAX;

const COMPUTE_LIMIT: u64 = 500_000;
const TRANSIENT_LIMIT: u64 = 1_000_000;
const STORAGE_LIMIT: u64 = 500_000;
const MARGIN_PCT: u64 = 90;
/// KIP-9 storage-mass constant C (rusty-kaspa v2.1.0 `consensus/core/src/constants.rs:26`).
const C: u64 = 1_000_000_000_000;
/// The F_MAX of the drawer (A2).
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

/// Tier shares of round r with all five lower tiers funded (the fixture funds three:
/// st = 400, 300, 300, 0, 0); s_H = 7,000 bp.
const ST_ALL: [i64; 5] = [400, 300, 300, 200, 200];

/// The worst script shape of `draw()` (DP21): round 2, K shards, marker m, every shard's
/// leaf at the winning rank non-empty with every bonus held (h_J = K, n2 = m - 1, so every
/// proof is a full leaf hash plus 18 levels and the n2 loop counts every slot), sales of
/// 1,000,000 KAS (every funded lower tier paid), `a` absorbed pools and `nf` funding inputs
/// of 100 KAS, fee paid. np = 5 with the fixture's three tier shares (the spec's DP21
/// shape), np = 7 (the script maximum: jackpot, second prize, five lower tiers) with
/// `ST_ALL`.
fn worst(k: usize, m: i64, a: usize, nf: usize, np: usize) -> DrawCase {
    let mut c = DrawCase::at_round(k, 2);
    assert!(np == 5 || np == 7);
    if np == 7 {
        c.marker.st = ST_ALL;
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
    assert!(x.fee.paid);
    c
}

fn run(c: &DrawCase) -> (Final, Masses) {
    let f = accept_draw(&c.build());
    let m = masses(&f.tx, &f.entries);
    (f, m)
}

// ---------------------------------------------------------------- DP21

/// DP21: K = 16, m = M_DRAW, A_MAX absorbed pools, 5 new pools (and 7, the script
/// maximum), worst-case proofs, with one and with F_MAX funding inputs: accepted, value
/// conserved, within every limit.
#[test]
fn dp21_k16_mdraw_amax_within_limits() {
    for (np, nf) in [(5, 1), (5, F_MAX), (7, 1), (7, F_MAX)] {
        let c = worst(K_MAX, M_DRAW, A_MAX, nf, np);
        let (f, m) = run(&c);
        let (pc, pt, ps) = pct(&m);
        println!(
            "\nDP21: K = {K_MAX}, m = {M_DRAW}, a = {A_MAX}, np = {np}, funding inputs {nf}: {m:?} ({pc:.1} % / {pt:.1} % / {ps:.1} %), inputs {}, outputs {}, fee {} sompi",
            f.tx.inputs.len(),
            f.tx.outputs.len(),
            f.fee()
        );
        assert_eq!(f.fee(), c.fee, "value conserved (inputs = outputs + fee)");
        // Economic conservation (A4, excluding R), as m3_draw's check_draw.
        let x = c.expected();
        for (i, (o, v)) in f.tx.outputs.iter().zip(&x.values).enumerate() {
            assert_eq!(o.value, *v, "output {i} value");
        }
        let mk = &c.marker;
        let lhs = (mk.sales - mk.ref_paid - x.amounts.c_close + mk.j_boost) as i128
            + c.vault.total_assets as i128
            + (c.pot as i128 + c.controller.acc as i128)
            + x.q.iter().map(|&q| q as i128).sum::<i128>();
        let pools: i128 = x.pools.pools.iter().map(|p| p.value as i128).sum();
        let rhs = pools + x.v_prime as i128 + (x.o_prime + x.fee.a_next) as i128 + if x.fee.paid { x.fee.t_f as i128 } else { 0 } + x.amounts.c_draw as i128;
        assert_eq!(lhs, rhs, "DP21: A4 conservation");
        assert!(within(&m, 100).iter().all(|&ok| ok), "DP21 exceeds a per-transaction limit: {m:?}");
    }
}

// ---------------------------------------------------------------- M3-a

#[test]
fn m3a_redeem_sizes() {
    let t = tpl();
    let c = worst(4, 10, 0, 1, 5);
    let x = c.expected();
    println!("\nM3-a: redeem script sizes (B_MAX = {})", t.b_max);
    println!("| object | prefix B | suffix B | state B | redeem B |\n|---|---|---|---|---|");
    let row = |name: &str, tp: &lottery::buy::Template, redeem: usize| {
        println!("| {name} | {} | {} | {} | {redeem} |", tp.prefix.len(), tp.suffix.len(), redeem - tp.prefix.len() - tp.suffix.len());
    };
    row("drawer (draw_test_seed)", &t.drawer, t.drawer_redeem(&c.drawer).len());
    row("marker (draw_leg)", &t.marker, t.marker_redeem(&c.marker).len());
    row("vault (draw_leg)", &t.vault, t.vault_redeem(&c.vault).len());
    row("pool (absorb_leg + placeholder claim)", &t.pool, t.pool_redeem(&x.new_pools[0]).len());
    row("controller (close + draw_leg)", &t.controller, t.controller_redeem(&c.controller).len());
}

// ---------------------------------------------------------------- M3-b

#[test]
fn m3b_draw_units_and_masses() {
    println!("\nM3-b: draw() per-input script units and masses (worst proofs, fee paid, one funding input)");
    println!("| np | K | m | a | drawer units / sigscript B | marker | vault | controller | pool (max) | compute | transient | storage | priority | % c / t / s |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for np in [5, 7] {
    for k in [4, K_MAX] {
        for m in [10, M_DRAW] {
            for a in [0, A_MAX] {
                let c = worst(k, m, a, 1, np);
                let (f, ms) = run(&c);
                let pool = (0..a).map(|i| f.units[4 + i]).max().map_or("-".to_string(), |u| u.to_string());
                let (pc, pt, ps) = pct(&ms);
                println!(
                    "| {np} | {k} | {m} | {a} | {} / {} | {} | {} | {} | {pool} | {} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} |",
                    f.units[0],
                    f.tx.inputs[0].signature_script.len(),
                    f.units[1],
                    f.units[2],
                    f.units[3],
                    ms.compute,
                    ms.transient,
                    ms.storage,
                    ms.priority
                );
                assert!(within(&ms, 100).iter().all(|&ok| ok), "K = {k}, m = {m}, a = {a}: {ms:?}");
            }
        }
    }
    }
}

// ---------------------------------------------------------------- M3-c

/// drawer.sil with its K_MAX and M_DRAW template constants replaced.
fn drawer_variant(k_max: usize, m_draw: i64) -> lottery::buy::Template {
    let src = Sources::committed(b_max()).drawer;
    let k_line = "    int constant K_MAX = 16;";
    let m_line = "    int constant M_DRAW = 50;";
    assert!(src.contains(k_line) && src.contains(m_line));
    let src = src.replace(k_line, &format!("    int constant K_MAX = {k_max};")).replace(m_line, &format!("    int constant M_DRAW = {m_draw};"));
    compile_drawer(&src, tpl().controller.hash)
}

/// M3-c: drawer redeem size as a function of the template constants K_MAX and M_DRAW, and
/// m_max: the committed drawer (K_MAX = 16, M_DRAW = 50) must draw the DP21 shape at
/// m = M_DRAW within 90 % of every limit; then m_max = M_DRAW = 50 (D49).
#[test]
fn m3c_drawer_size_and_m_max() {
    let state_len = {
        let t = tpl();
        let c = DrawCase::new(4);
        t.drawer_redeem(&c.drawer).len() - t.drawer.prefix.len() - t.drawer.suffix.len()
    };
    println!("\nM3-c: drawer redeem B by K_MAX (rows) and M_DRAW (columns); state {state_len} B");
    println!("| K_MAX | M_DRAW 10 | 25 | 50 | 100 |\n|---|---|---|---|---|");
    let mut sizes = std::collections::BTreeMap::new();
    for k in [4, 8, 16] {
        let row: Vec<usize> = [10, 25, 50, 100]
            .iter()
            .map(|&m| {
                let v = drawer_variant(k, m);
                let n = v.prefix.len() + v.suffix.len() + state_len;
                sizes.insert((k, m), n);
                n
            })
            .collect();
        println!("| {k} | {} | {} | {} | {} |", row[0], row[1], row[2], row[3]);
    }
    let committed = {
        let t = tpl();
        t.drawer.prefix.len() + t.drawer.suffix.len() + state_len
    };
    assert_eq!(sizes[&(16, 50)], committed, "the (16, 50) variant is the committed drawer");

    // m_max: the DP21 shape at m in {10, 25, 50} with the committed drawer.
    println!("\nM3-c: worst shape (K = 16, a = 5, np = 7, F_MAX funding inputs) by m, committed drawer");
    println!("| m | compute | transient | storage | % c / t / s | <= 90 % |\n|---|---|---|---|---|---|");
    let mut last = None;
    for m in [10, 25, M_DRAW] {
        let (_, ms) = run(&worst(K_MAX, m, A_MAX, F_MAX, 7));
        let ok = within(&ms, MARGIN_PCT).iter().all(|&b| b);
        let (pc, pt, ps) = pct(&ms);
        println!("| {m} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {ok} |", ms.compute, ms.transient, ms.storage);
        last = Some((ms, ok));
    }
    let (ms50, ok50) = last.unwrap();
    // Estimate for M_DRAW = 100 (not runnable: the drawer's 128-byte zero buffer covers
    // 2 m <= 128): transient grows by 4 grams per byte (params.rs:21) of extra drawer script
    // and extra leaf bytes 2 (100 - 50) K.
    let extra = (sizes[&(16, 100)] - sizes[&(16, 50)]) as u64 + 2 * 50 * K_MAX as u64;
    println!(
        "Estimate, M_DRAW = 100 at m = 100: transient ~ {} ({:.1} %) from +{extra} B (script units not estimated)",
        ms50.transient + 4 * extra,
        (ms50.transient + 4 * extra) as f64 * 100.0 / TRANSIENT_LIMIT as f64
    );
    // Stop-and-ask trigger (Stage 4): m_max would have to drop below 50.
    assert!(ok50, "M3-c: the DP21 shape at m = M_DRAW = {M_DRAW} exceeds 90 % ({ms50:?}): m_max would have to drop below 50");
    println!("m_max = M_DRAW = {M_DRAW} (the committed genesis bound m_max <= M_DRAW stands)");
}

// ---------------------------------------------------------------- M3-d

/// A draw whose covenant outputs all sit at (or 1 sompi above) R: V' = 0 (vault' R),
/// O' + A' = 0 (controller' R; fee paid at exactly T_F = f_min), J = G2 = 1 sompi, sales
/// just enough for every lower tier (np = 7, `ST_ALL`), `a` absorbed pools with remaining 0 (refunds
/// at R), and a caller output of `caller` sompi. Storage depends only on values and
/// scripts, so no script runs (`Built::storage_mass`).
fn storage_case(a: usize, caller: u64) -> DrawCase {
    storage_case_at(a, caller, F_MIN)
}

/// `storage_case` with the drawer's f_min set to `f_min` (T_F = f_min exactly).
fn storage_case_at(a: usize, caller: u64, f_min: i64) -> DrawCase {
    let mut c = DrawCase::at_round(4, 2);
    c.drawer.f_min = f_min;
    c.marker.st = ST_ALL;
    c.marker.jackpot = 1;
    c.marker.j_boost = 0;
    c.marker.g2 = 1;
    c.hold_jackpot(0, 1);
    let ob = c.other_bonus(0);
    c.hold(1, ob, 1);
    // Smallest sales with every lower tier paid.
    let mut s = 1_000_000i64;
    loop {
        c.set_sales(s, 0);
        if c.expected().pools.lp.iter().all(|&p| p) {
            break;
        }
        s += s / 4;
    }
    if a > 0 {
        c.absorb(a);
        for p in &mut c.absorbed {
            p.remaining = 0;
        }
    }
    c.pot = 0;
    let x = c.expected();
    // T_F = acc + F = f_min exactly.
    c.controller.acc = c.drawer.f_min - x.amounts.f;
    assert!(c.controller.acc >= 0);
    // V' = 0 where reachable (V >= 0).
    c.vault.total_assets = (c.vault.total_assets - c.expected().v_prime).max(0);
    let x = c.expected();
    // caller = funding - np R + C_draw - fee.
    let need = caller as i128 + (x.np() as u64 * c.reserve()) as i128 - x.amounts.c_draw as i128 + c.fee as i128;
    c.funding[0].value = need as u64;
    let x = c.expected();
    assert_eq!(x.values[x.caller_output()], caller);
    c
}

/// Supremum of draw()'s storage mass over every valid state, for `a` absorbed pools, at
/// reserve R, f_min and a caller output >= `caller_min` (KIP-9 harmonic sum, input relief
/// dropped): 3 singleton outputs and up to 7 pools (covenant P2SH, plurality 2: 63 + 35 + 32
/// = 130 B, mass/mod.rs:83-99) each >= R; the fee >= f_min, the marker refund and a pool
/// refunds = R, the caller >= caller_min (standard scripts, plurality 1).
fn storage_sup(a: usize, r: u64, f_min: u64, caller_min: u64) -> u64 {
    10 * (4 * C / r) + (1 + a as u64) * (C / r) + C / f_min + C / caller_min
}

#[test]
fn m3d_reserve_recheck() {
    let r = lottery::params::TEST_RESERVE;
    let f_min = F_MIN as u64;
    println!("\nM3-d: draw() storage worst case at R = {r} sompi, f_min = F_MIN = {f_min} sompi, caller >= MIN_CALLER_OUTPUT = {MIN_CALLER_OUTPUT} sompi");
    println!("| a | caller output (KAS) | storage | % of limit | supremum (relief dropped) |\n|---|---|---|---|---|");
    for a in [0, A_MAX] {
        for caller in [KAS, 10 * KAS, 1_000 * KAS, 1_000_000 * KAS, 100_000_000 * KAS] {
            let c = storage_case(a, caller);
            let st = c.build().storage_mass();
            let sup = storage_sup(a, r, f_min, MIN_CALLER_OUTPUT);
            println!("| {a} | {} | {st} | {:.1} | {sup} |", caller / KAS, st as f64 * 100.0 / STORAGE_LIMIT as f64);
            assert!(st <= sup, "storage {st} above the supremum {sup}");
        }
    }
    let sup = storage_sup(A_MAX, r, f_min, MIN_CALLER_OUTPUT);
    println!("Supremum at a = A_MAX: {sup} ({:.1} %)", sup as f64 * 100.0 / STORAGE_LIMIT as f64);
    // Stop-and-ask trigger (Stage 4): draw storage above 90 %.
    assert!(sup * 100 <= STORAGE_LIMIT * MARGIN_PCT, "M3-d: draw() storage supremum {sup} exceeds 90 %");
    // The smallest R that keeps the supremum within 90 % (f_min = F_MIN, caller >= 1 KAS).
    let budget = STORAGE_LIMIT * MARGIN_PCT / 100 - C / f_min - C / MIN_CALLER_OUTPUT;
    let r_need = (C * (40 + 1 + A_MAX as u64)).div_ceil(budget);
    println!("Smallest R for 90 % (draw alone): {r_need} sompi ({:.3} KAS); committed R = {:.2} KAS (set by close(), M2-c)", r_need as f64 / KAS as f64, r as f64 / KAS as f64);
    assert!(r >= r_need);
    // The fee output's term is C / T_F with T_F >= f_min: the smallest f_min that keeps the
    // supremum within 90 % at the committed R (genesis allows 0 <= f_min <= 10^10, A3.1).
    let rest = 10 * (4 * C / r) + (1 + A_MAX as u64) * (C / r) + C / MIN_CALLER_OUTPUT;
    let f_need = C.div_ceil(STORAGE_LIMIT * MARGIN_PCT / 100 - rest);
    println!("Smallest f_min for 90 % at R = {:.2} KAS: {f_need} sompi ({:.4} KAS); f_min = 0 with T_F = 0 requires a zero-value fee output (invalid)", r as f64 / KAS as f64, f_need as f64 / KAS as f64);
    // Stage 5 (user decision A): the drawer and genesis require f_min >= F_MIN_FLOOR, so the
    // worst state has T_F = F_MIN_FLOOR. Measured on the same worst shapes, and the supremum.
    let floor = F_MIN_FLOOR as u64;
    assert!(floor >= f_need, "F_MIN_FLOOR {floor} below the 90 % floor {f_need}");
    println!("\nM3-d at f_min = F_MIN_FLOOR = {floor} sompi (T_F = F_MIN_FLOOR):");
    println!("| a | caller output (KAS) | storage | % of limit | supremum (relief dropped) |\n|---|---|---|---|---|");
    for a in [0, A_MAX] {
        for caller in [KAS, 10 * KAS, 1_000 * KAS, 1_000_000 * KAS, 100_000_000 * KAS] {
            let c = storage_case_at(a, caller, F_MIN_FLOOR);
            let st = c.build().storage_mass();
            let sup = storage_sup(a, r, floor, MIN_CALLER_OUTPUT);
            println!("| {a} | {} | {st} | {:.1} | {sup} |", caller / KAS, st as f64 * 100.0 / STORAGE_LIMIT as f64);
            assert!(st <= sup, "storage {st} above the supremum {sup}");
            assert!(st * 100 <= STORAGE_LIMIT * MARGIN_PCT, "M3-d: storage {st} at F_MIN_FLOOR exceeds 90 %");
        }
    }
    let sup = storage_sup(A_MAX, r, floor, MIN_CALLER_OUTPUT);
    println!("Supremum at a = A_MAX, f_min = F_MIN_FLOOR: {sup} ({:.1} %)", sup as f64 * 100.0 / STORAGE_LIMIT as f64);
    assert!(sup * 100 <= STORAGE_LIMIT * MARGIN_PCT, "M3-d: draw() storage supremum {sup} at F_MIN_FLOOR exceeds 90 %");
}

// ---------------------------------------------------------------- M3-e

#[test]
fn m3e_priority_fee_vs_c_draw() {
    println!("\nM3-e: draw() priority fee (min feerate x priority mass) vs C_draw = s_D x S");
    println!("| shape | priority mass | priority fee (KAS) | s_D (bp) | S where C_draw covers it (KAS) |\n|---|---|---|---|---|");
    let shapes: Vec<(String, DrawCase)> = vec![
        ("K = 4, m = 10, a = 0, np = 5".into(), worst(4, 10, 0, 1, 5)),
        (format!("K = 16, m = {M_DRAW}, a = {A_MAX}, np = 5"), worst(K_MAX, M_DRAW, A_MAX, 1, 5)),
        (format!("K = 16, m = {M_DRAW}, a = {A_MAX}, np = 7, {F_MAX} funding inputs"), worst(K_MAX, M_DRAW, A_MAX, F_MAX, 7)),
    ];
    for (name, c) in shapes {
        let (_, m) = run(&c);
        let fee = priority_fee(&m) as i64;
        let s_d = c.marker.s_d;
        let (mut lo, mut hi) = (0i64, 1i64 << 50);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if muldiv(mid, s_d) >= fee { hi = mid } else { lo = mid + 1 }
        }
        println!("| {name} | {} | {:.4} | {s_d} | {:.2} |", m.priority, fee as f64 / KAS as f64, lo as f64 / KAS as f64);
    }
    let sup = storage_sup(A_MAX, lottery::params::TEST_RESERVE, F_MIN as u64, MIN_CALLER_OUTPUT);
    println!("Storage supremum (M3-d) {sup}: priority fee at most {:.4} KAS when storage dominates", sup as f64 * 100.0 / KAS as f64);
    // Stage 5: f_min may be as low as F_MIN_FLOOR, which raises the storage supremum.
    let sup = storage_sup(A_MAX, lottery::params::TEST_RESERVE, F_MIN_FLOOR as u64, MIN_CALLER_OUTPUT);
    println!("Storage supremum at f_min = F_MIN_FLOOR (M3-d) {sup}: priority fee at most {:.4} KAS when storage dominates (C_draw covers it from S = {:.2} KAS)", sup as f64 * 100.0 / KAS as f64, sup as f64 * 100.0 / KAS as f64 * 10_000.0 / 50.0);
}

// ---------------------------------------------------------------- M3-f

/// M3-f (D32): K = 16 close() with the M3 controller (draw_leg) and marker (draw_leg) must
/// stay within 90 % of every limit. Same shape as M2-a (1,000 KAS sales per shard, distinct
/// roots).
#[test]
fn m3f_close_k16_recheck() {
    let t: Arc<lottery::buy::Templates> = tpl();
    println!("\nM3-f: close() with the M3 controller ({} B) and marker ({} B redeem)", t.controller_redeem(&CloseCase::new(4).controller).len(), {
        let c = CloseCase::new(4);
        t.marker.redeem(&c.expected().marker.values()).len()
    });
    println!("| K | controller units | compute | transient | storage | % c / t / s |\n|---|---|---|---|---|---|");
    for k in [4, K_MAX] {
        let mut c = CloseCase::new(k);
        for i in 0..k {
            c.set_sales(i, 1_000 * KAS as i64, 50 * KAS as i64);
            c.shards[i].state.root = [0x20 + i as u8; 32];
        }
        let f = common::accept(&c.build());
        let m = masses(&f.tx, &f.entries);
        let (pc, pt, ps) = pct(&m);
        println!("| {k} | {} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} |", f.units[0], m.compute, m.transient, m.storage);
        if k == K_MAX {
            // Stop-and-ask trigger (Stage 4): close at K = 16 above 90 % of a limit.
            assert!(within(&m, MARGIN_PCT).iter().all(|&ok| ok), "M3-f: K = 16 close() exceeds 90 %: {m:?}");
        }
    }
}
