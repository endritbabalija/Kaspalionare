//! M7 measurements M7-a .. M7-f (spec M7 A8 "Measurements"; the A10 re-confirmation of K = 16,
//! R = 1.69 KAS and m_max = 50, D83, with the D104 stop rules). Run with
//! `cargo test -p lottery --release --test m7_measure -- --nocapture --test-threads=1`.
//!
//! Limits, mass rules and fee formulas as `m6_measure.rs` (rusty-kaspa v2.1.0: compute 500,000,
//! storage 500,000, transient 1,000,000 per transaction, `consensus/core/src/config/params.rs:
//! 626`; KIP-9 storage mass, `consensus/core/src/mass/mod.rs:439-512`; relay floor
//! `lottery::buy::relay_floor_fee`, priority fee `harness::priority_fee`). Masses come from
//! the v2.1.0 mass calculator on transactions accepted by the real scripts (compute follows
//! the committed budgets `finish()` sizes from the real runs); estimates are labelled.
//!
//! Stop rules (D104): any final shape above 90 % of compute, transient or storage (genesis
//! exempt, U-M6-1); the drawer's stack headroom at check 24 below 4 slots or a compiler stack
//! overflow; `close()` at K = 16 above 90 %. The tests assert them; a failure is the
//! stop-and-ask signal. Frontier variants (K_MAX != 16, M_DRAW != 50, B_MAX != 4) are
//! measurement only, compiled in memory (D91).
//!
//! Coverage (milestone M7 "K, R and m_max are re-confirmed", spec M6 A5a rows marked
//! "re-run"): M7-a = the M6-a `draw()` rows and the dependent M6-h draw fee rows; M7-b = the
//! void rows (new); M7-c = the M6-a `close()` rows (M2-a shape and storage worst case), their
//! M6-h fee rows, M6-b (K frontier) and M6-c (B_MAX frontier); M7-d = the M6-b `draw()`
//! estimates at the K_MAX variants and the M6-d `draw()` rows with the M_DRAW variants; M7-e
//! = M6-g (close storage worst case, draw storage supremum); M7-f = M6-j (drawer sensitivity,
//! byte budget) and the measured stack headroom at check 24.

mod common;

use std::sync::Arc;

use common::m2::{accept_draw, accept_m4, tpl};
use common::measure::*;
use harness::{Masses, masses, priority_fee};
use lottery::buy::{BuyCase, Final, KAS, Template, Templates, relay_floor_fee};
use lottery::close::{CloseCase, MIN_CALLER_OUTPUT};
use lottery::draw::{DrawCase, compile_drawer};
use lottery::game::{A_MAX, ClaimMode, F_MIN, F_MIN_FLOOR, M_CLAIM, M_DRAW, muldiv};
use lottery::params::TEST_RESERVE;
use lottery::smt::Tree;
use lottery::state::K_MAX;

const K_FINAL: usize = 16;
const B_MAX_FINAL: usize = 4;
const M_FINAL: i64 = 50;
/// D104: free stack slots required at the drawer's check 24.
const STACK_MIN_FREE: usize = 4;

fn kas(s: u64) -> f64 {
    s as f64 / KAS as f64
}

fn code(tp: &Template) -> usize {
    tp.prefix.len() + tp.suffix.len()
}

/// Smallest x in [lo, hi] with `ok(x)`, `ok` monotone (false then true).
fn search(mut lo: u64, mut hi: u64, ok: impl Fn(u64) -> bool) -> u64 {
    assert!(ok(hi), "search: upper end {hi} not ok");
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if ok(mid) { hi = mid } else { lo = mid + 1 }
    }
    lo
}

fn sales_covering(fee: u64, s_bp: i64) -> u64 {
    search(0, 1 << 50, |s| muldiv(s as i64, s_bp) >= fee as i64)
}

/// One measured shape (as m6_measure.rs `Row`; `leg` = the units of the named delegate input).
struct Row {
    name: String,
    units0: u64,
    sig0: usize,
    leg: Option<u64>,
    outputs: usize,
    m: Masses,
}

impl Row {
    fn new(name: impl Into<String>, f: &Final, leg: Option<usize>) -> Self {
        Row {
            name: name.into(),
            units0: f.units[0],
            sig0: f.tx.inputs[0].signature_script.len(),
            leg: leg.map(|i| f.units[i]),
            outputs: f.tx.outputs.len(),
            m: masses(&f.tx, &f.entries),
        }
    }
    fn ok(&self) -> bool {
        within(&self.m, MARGIN_PCT).iter().all(|&b| b)
    }
}

const TABLE_HEAD: &str = "| transition | input 0 units / sigscript B | leg units | outputs | compute | transient | storage | priority | % c / t / s | relay floor fee (KAS) | priority fee (KAS) | <= 90 % |\n|---|---|---|---|---|---|---|---|---|---|---|---|";

fn print_row(r: &Row) {
    let (pc, pt, ps) = pct(&r.m);
    println!(
        "| {} | {} / {} | {} | {} | {} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {:.5} | {:.5} | {} |",
        r.name,
        r.units0,
        r.sig0,
        r.leg.map_or("-".into(), |u| u.to_string()),
        r.outputs,
        r.m.compute,
        r.m.transient,
        r.m.storage,
        r.m.priority,
        kas(relay_floor_fee(&r.m)),
        kas(priority_fee(&r.m)),
        r.ok()
    );
}

/// The M2-a close shape: sales 1,000 KAS and 50 KAS referral paid per shard, distinct roots.
fn close_k_with(k: usize, t: &Templates) -> CloseCase {
    let mut c = CloseCase::new_with(k, t);
    for i in 0..k {
        c.set_sales(i, 1_000 * KAS as i64, 50 * KAS as i64);
        c.shards[i].state.root = [0x20 + i as u8; 32];
    }
    c
}

/// The worst draw (DP21 at K, m, a = A_MAX, np = 7, F_MAX funding inputs, cap binding with
/// the record) against templates `t`, with the 304-byte anchor (spec M7 A3).
fn worst_draw_with(t: &Templates, k: usize, m: i64) -> DrawCase {
    let c = with_pending(dp21_with(t, k, m, A_MAX, F_MAX, 7), Pending::Cap);
    assert_eq!(c.anchor_bytes().len(), harness::ANCHOR_LEN);
    c
}

/// A typical draw: K = 16, round 2, nobody holds the jackpot or a second prize, a = 0, three
/// lower-tier pools, one funding input.
fn typical_draw() -> DrawCase {
    let mut c = DrawCase::at_round(K_FINAL, 2);
    for s in [10u64, 100, 1_000, 10_000, 100_000] {
        c.set_sales((s * KAS) as i64, (s * KAS / 20) as i64);
        let x = c.expected();
        if x.np() == 3 && x.h_j == 0 && x.n2 == 0 {
            return c;
        }
    }
    panic!("no typical draw with np = 3");
}

/// The worst void (spec M7 D96; M7-b): K = 16, a = A_MAX absorbed pools, F_MAX funding inputs,
/// the record with the cap binding, fee paid. A void has no leaves, siblings or pools.
fn worst_void() -> DrawCase {
    let mut c = dp21(K_FINAL, M_DRAW, A_MAX, F_MAX, 7);
    c.set_void(true);
    let c = with_pending(c, Pending::Cap);
    let x = c.expected();
    assert!(x.void && x.np() == 0 && x.rec && x.fee.paid && x.a == A_MAX);
    c
}

/// A typical void: K = 16, round 2, a = 0, one funding input, nothing pending.
fn typical_void() -> DrawCase {
    let mut c = typical_draw();
    c.set_void(true);
    c
}

fn run_draw(c: &DrawCase, t: Arc<Templates>) -> (Final, Masses) {
    let f = accept_draw(&c.build_with(t));
    let m = masses(&f.tx, &f.entries);
    (f, m)
}

// ---------------------------------------------------------------- M7-a

/// M7-a: redeem sizes (drawer, marker, controller; the marker against the pre-M7 marker
/// fixture), the worst and typical `draw()` masses with the anchor, and the M6-h fee rows
/// of the draw.
#[test]
fn m7a_sizes_draw_masses_and_fees() {
    let t = tpl();
    let draw = DrawCase::new(4);
    let close = CloseCase::new(4);
    println!("\nM7-a: redeem script sizes (B_MAX = {}, K_MAX = {K_MAX}, M_DRAW = {M_DRAW})", t.b_max);
    println!("| object | prefix B | suffix B | state B | redeem B |\n|---|---|---|---|---|");
    let row = |name: &str, tp: &Template, redeem: usize| println!("| {name} | {} | {} | {} | {redeem} |", tp.prefix.len(), tp.suffix.len(), redeem - code(tp));
    let mk = close.expected().marker;
    row("drawer (draw: anchor proof and void)", &t.drawer, t.drawer_redeem(&draw.drawer).len());
    row("marker (draw_leg only; no seed_ref)", &t.marker, t.marker_redeem(&mk).len());
    row("controller (close, draw_leg)", &t.controller, t.controller_redeem(&close.controller).len());
    // The pre-M7 marker (fixture, with seed_ref and refresh()) for the delta (spec D99: the
    // state shrinks by the 9-byte seed_ref push).
    let old_src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/marker_pre_m7.sil")).expect("fixture");
    let mut ctor = mk.ctor(t.drawer.hash);
    let at = ctor.len() - 2;
    ctor.insert(at, harness::ArtifactValue::Int(0));
    let old = Template::from(harness::compile_source(&old_src, &ctor));
    let mut v = mk.values();
    v.insert("seed_ref".into(), harness::ArtifactValue::Int(1_000_600));
    let (new_len, old_len) = (t.marker_redeem(&mk).len(), old.redeem(&v).len());
    row("marker before M7 (fixture: draw_leg, refresh, seed_ref)", &old, old_len);
    println!("marker redeem: {old_len} -> {new_len} B ({:+} B; state {:+} B, code {:+} B)", new_len as i64 - old_len as i64, (new_len - code(&t.marker)) as i64 - (old_len - code(&old)) as i64, code(&t.marker) as i64 - code(&old) as i64);

    let mut rows = vec![];
    let (f, _) = run_draw(&worst_draw_with(&t, K_FINAL, M_DRAW), t.clone());
    rows.push(Row::new(format!("draw worst (DP21 K = {K_FINAL}, m = {M_DRAW}, a = {A_MAX}, np = 7, {F_MAX} funding inputs, cap binding, record, anchor 304 B)"), &f, Some(2)));
    let (f, _) = run_draw(&typical_draw(), t.clone());
    rows.push(Row::new(format!("draw typical (K = {K_FINAL}, m = 10, a = 0, np = 3, no record)"), &f, Some(2)));
    println!("\nM7-a: draw() with the anchor proof (fees at 100 sompi per gram)\n{TABLE_HEAD}");
    for r in &rows {
        print_row(r);
    }
    println!("\nM7-a (M6-h rows): round sales S at which C_draw = s_D x S covers the draw's fee");
    println!("| transaction | s_D (bp) | priority mass | priority fee (KAS) | S covering it (KAS) | relay floor (KAS) | S covering the relay floor (KAS) |\n|---|---|---|---|---|---|---|");
    let s_d = typical_draw().marker.s_d;
    for r in &rows {
        let (pf, rf) = (priority_fee(&r.m), relay_floor_fee(&r.m));
        println!("| {} | {s_d} | {} | {:.5} | {:.2} | {:.5} | {:.2} |", r.name, r.m.priority, kas(pf), kas(sales_covering(pf, s_d)), kas(rf), kas(sales_covering(rf, s_d)));
    }
    let sup = storage_sup_rec(A_MAX, TEST_RESERVE, F_MIN_FLOOR as u64, MIN_CALLER_OUTPUT);
    println!(
        "draw storage supremum with the record at F_MIN_FLOOR (M7-e) {sup}: priority fee at most {:.5} KAS when storage dominates; C_draw covers it from S = {:.2} KAS",
        kas(sup * 100),
        kas(sales_covering(sup * 100, s_d))
    );
    let fails: Vec<String> = rows.iter().filter(|r| !r.ok()).map(|r| format!("{}: {:?}", r.name, r.m)).collect();
    assert!(fails.is_empty(), "M7-a (D104 STOP): draw above 90 %: {fails:#?}");
}

// ---------------------------------------------------------------- M7-b

/// M7-b: void masses (worst: A_MAX absorbed pools, the record, fee paid, F_MAX funding
/// inputs; typical: nothing absorbed or pending).
#[test]
fn m7b_void_masses() {
    let t = tpl();
    let mut rows = vec![];
    let (f, _) = run_draw(&worst_void(), t.clone());
    rows.push(Row::new(format!("void worst (K = {K_FINAL}, a = {A_MAX}, {F_MAX} funding inputs, cap binding, record, fee paid)"), &f, Some(2)));
    let (f, _) = run_draw(&typical_void(), t.clone());
    rows.push(Row::new(format!("void typical (K = {K_FINAL}, a = 0, nothing pending)"), &f, Some(2)));
    println!("\nM7-b: void (empty anchor, lock >= D + draw_window)\n{TABLE_HEAD}");
    for r in &rows {
        print_row(r);
    }
    let s_d = typical_draw().marker.s_d;
    println!("| transaction | s_D (bp) | priority mass | priority fee (KAS) | S covering it (KAS) | relay floor (KAS) | S covering the relay floor (KAS) |\n|---|---|---|---|---|---|---|");
    for r in &rows {
        let (pf, rf) = (priority_fee(&r.m), relay_floor_fee(&r.m));
        println!("| {} | {s_d} | {} | {:.5} | {:.2} | {:.5} | {:.2} |", r.name, r.m.priority, kas(pf), kas(sales_covering(pf, s_d)), kas(rf), kas(sales_covering(rf, s_d)));
    }
    let fails: Vec<String> = rows.iter().filter(|r| !r.ok()).map(|r| format!("{}: {:?}", r.name, r.m)).collect();
    assert!(fails.is_empty(), "M7-b (D104 STOP): void above 90 %: {fails:#?}");
}

// ---------------------------------------------------------------- M7-c

/// M7-c: `close()` at K = 16 with the M7 controller and marker (the marker lost seed_ref):
/// the M6-a close rows (M2-a shape and the M2-c storage worst case) within 90 % (D104) and
/// their M6-h fee rows; the M6-b K frontier (K_MAX variants, K = 16.. until transient passes
/// 100 %, with R(K) and the storage at R(K)) and the M6-c B_MAX frontier (3, 4, 5) re-run.
#[test]
fn m7c_close_k16_and_frontiers() {
    let committed = tpl();
    let close = close_k_with(K_FINAL, &committed);
    let f = common::accept(&close.build());
    let r16 = Row::new(format!("close K = {K_FINAL}, sales in every shard (M2-a shape)"), &f, None);
    let (st, (s, o, jb, a, cl)) = worst_storage(K_FINAL, TEST_RESERVE as i64, &CALLERS);
    let wc = close_case_c(K_FINAL, TEST_RESERVE as i64, s, o, jb, a, cl).unwrap();
    let f = common::accept(&wc.build());
    let rw = Row::new(
        format!("close K = {K_FINAL}, storage worst case (M2-c grid: sales {:.0}, O {:.0}, J_boost {:.0}, A {:.0}, caller {:.0} KAS)", kas(s as u64), kas(o), kas(jb), kas(a), kas(cl)),
        &f,
        None,
    );
    assert_eq!(rw.m.storage, st, "the accepted worst close has the grid's storage");
    println!("\nM7-c (M6-a): close() with the M7 controller and marker\n{TABLE_HEAD}");
    print_row(&r16);
    print_row(&rw);
    let s_c = close.controller.s_c;
    println!("\nM7-c (M6-h rows): round sales S at which C_close = s_C x S covers the close's fee");
    println!("| transaction | s_C (bp) | priority mass | priority fee (KAS) | S covering it (KAS) | relay floor (KAS) | S covering the relay floor (KAS) |\n|---|---|---|---|---|---|---|");
    for r in [&r16, &rw] {
        let (pf, rf) = (priority_fee(&r.m), relay_floor_fee(&r.m));
        println!("| {} | {s_c} | {} | {:.5} | {:.2} | {:.5} | {:.2} |", r.name, r.m.priority, kas(pf), kas(sales_covering(pf, s_c)), kas(rf), kas(sales_covering(rf, s_c)));
    }

    println!("\nM7-c (M6-b): close() at K = K_MAX on in-memory K_MAX variants (controller, marker, drawer, pool, ticket; D91), M2-a shape, R = {TEST_RESERVE}");
    println!("| K_MAX = K | controller redeem B | controller units / sigscript B | compute | transient | storage at R = 1.69 | % c / t / s | R(K) = 4C(K+3)/(0.9 L) (KAS, 0.01 up) | storage at R(K) | <= 90 % |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    let mut k90 = [0usize; 3];
    let mut k100 = [0usize; 3];
    for k in K_FINAL..=20 {
        let t = match variant(&format!("M7 K_MAX {k}"), || k_sources(k)) {
            Ok(t) => t,
            Err(e) => {
                println!("| {k} | compile failed: {} |", e.lines().next().unwrap_or(""));
                break;
            }
        };
        if k == K_FINAL {
            for (a, b) in [(&t.controller, &committed.controller), (&t.marker, &committed.marker), (&t.drawer, &committed.drawer), (&t.pool, &committed.pool), (&t.ticket, &committed.ticket)] {
                assert_eq!(a.hash, b.hash, "the K_MAX = 16 variant is the committed template");
            }
        }
        let mut c = close_k_with(k, &t);
        c.k_max = k;
        let f = common::accept(&c.build_with(t.clone()));
        let m = masses(&f.tx, &f.entries);
        let rk = r_bound(k).div_ceil(KAS / 100) * (KAS / 100);
        let mut c2 = c.clone();
        c2.controller.reserve = rk as i64;
        let st_rk = c2.build_with(t.clone()).storage_mass();
        let (pc, pt, ps) = pct(&m);
        let (w90, w100) = (within(&m, MARGIN_PCT), within(&m, 100));
        println!(
            "| {k} | {} | {} / {} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {:.2} | {st_rk} ({:.1} %) | {} |",
            t.controller_redeem(&c.controller).len(),
            f.units[0],
            f.tx.inputs[0].signature_script.len(),
            m.compute,
            m.transient,
            m.storage,
            kas(rk),
            st_rk as f64 * 100.0 / STORAGE_LIMIT as f64,
            w90.iter().all(|&b| b)
        );
        for l in 0..3 {
            if w90[l] && (k == K_FINAL || k90[l] == k - 1) {
                k90[l] = k;
            }
            if w100[l] && (k == K_FINAL || k100[l] == k - 1) {
                k100[l] = k;
            }
        }
        if m.transient > TRANSIENT_LIMIT {
            println!("(transient above 100 % at K = {k}: stop)");
            break;
        }
    }
    println!("Largest K (compute, transient, storage at R = 1.69 KAS) within 90 %: {k90:?}; within 100 %: {k100:?} (measured from K = {K_FINAL})");
    println!("\nM7-c (M6-c): B_MAX frontier at K = {K_FINAL}");
    println!("| B_MAX | close: compute | transient | storage | % c / t / s | buy b = B_MAX (referral): compute | transient | storage | % c / t / s |\n|---|---|---|---|---|---|---|---|---|");
    let mut fails = vec![];
    for bm in [3, B_MAX_FINAL, 5] {
        let t = Templates::get(bm);
        let f = common::accept(&close_k_with(K_FINAL, &t).build_with(t.clone()));
        let mc = masses(&f.tx, &f.entries);
        let mut b = BuyCase::new(bm).with_tree(Tree::new(10));
        b.bundle = common::distinct(bm, 10);
        b.with_ref = true;
        let fb = common::accept(&b.build_with(t.clone()));
        let mb = masses(&fb.tx, &fb.entries);
        let ((c1, c2, c3), (b1, b2, b3)) = (pct(&mc), pct(&mb));
        println!("| {bm} | {} | {} | {} | {c1:.1} / {c2:.1} / {c3:.1} | {} | {} | {} | {b1:.1} / {b2:.1} / {b3:.1} |", mc.compute, mc.transient, mc.storage, mb.compute, mb.transient, mb.storage);
        if bm == B_MAX_FINAL {
            for (name, m) in [("close", mc), ("buy", mb)] {
                if !within(&m, MARGIN_PCT).iter().all(|&x| x) {
                    fails.push(format!("{name} at B_MAX = {bm}: {m:?}"));
                }
            }
        }
    }
    assert!(r16.ok(), "M7-c (D104 STOP): close() K = 16 above 90 %: {:?}", r16.m);
    assert!(rw.ok(), "M7-c (D104 STOP): close() K = 16 storage worst case above 90 %: {:?}", rw.m);
    assert_eq!(k90[1], K_FINAL, "M7-c: K = 16 is the largest K within 90 % of transient");
    assert!(fails.is_empty(), "M7-c (D104 STOP): B_MAX = 4 not within 90 %: {fails:?}");
}

// ---------------------------------------------------------------- M7-d

/// M7-d: `draw()` with the anchor at the K_MAX variants (drawer-only K_MAX 16 and 17, M6-b)
/// and the M_DRAW variants (25 and 50; m = 10, 25, 50, M6-d): sets K and m_max.
#[test]
fn m7d_draw_frontiers() {
    let committed = tpl();
    let t25 = variant("M7 drawer K_MAX 16 M_DRAW 25", || drawer_sources(16, 25)).expect("M_DRAW = 25 drawer compiles");
    let t17 = variant("M7 drawer K_MAX 17 M_DRAW 50", || drawer_sources(17, 50)).expect("drawer K_MAX = 17 compiles");
    println!("\nM7-d: worst draw() with the anchor (a = {A_MAX}, np = 7, {F_MAX} funding inputs, cap binding, record) at drawer variants");
    println!("| drawer | K | m | compute | transient | storage | % c / t / s | <= 90 % |\n|---|---|---|---|---|---|---|---|");
    let mut fails = vec![];
    let mut line = |name: &str, k: usize, m: i64, ms: &Masses, asserted: bool| {
        let ok = within(ms, MARGIN_PCT).iter().all(|&b| b);
        let (pc, pt, ps) = pct(ms);
        println!("| {name} | {k} | {m} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {ok} |", ms.compute, ms.transient, ms.storage);
        if asserted && !ok {
            fails.push(format!("{name} K = {k} m = {m}: {ms:?}"));
        }
    };
    for m in [10, 25, 50] {
        let (_, ms) = run_draw(&worst_draw_with(&committed, K_FINAL, m), committed.clone());
        line("committed (K_MAX 16, M_DRAW 50)", K_FINAL, m, &ms, m == M_FINAL);
    }
    for m in [10, 25] {
        let (_, ms) = run_draw(&worst_draw_with(&t25, K_FINAL, m), t25.clone());
        line("M_DRAW 25 (variant)", K_FINAL, m, &ms, false);
    }
    let (_, ms) = run_draw(&worst_draw_with(&t17, K_FINAL, M_FINAL), t17.clone());
    line("K_MAX 17 (variant, K = 16 drawn)", K_FINAL, M_FINAL, &ms, false);
    println!("drawer code B: committed {} / M_DRAW 25 {} / K_MAX 17 {}", code(&committed.drawer), code(&t25.drawer), code(&t17.drawer));

    // M6-b estimates: draw() and claim() at K_MAX = k (every template at K_MAX = k), transient
    // only, 4 grams per byte (`consensus/core/src/constants.rs:31`), the M6-b formula: the
    // drawer and controller code, the marker and every absorbed pool (code + 32 B per extra
    // root), the pool code again as the drawer's pool prefix/suffix arguments, and one more
    // worst proof (576 + 2m B) per extra shard; claim: ticket code, pool and the proofs.
    let (_, draw16) = run_draw(&worst_draw_with(&committed, K_FINAL, M_DRAW), committed.clone());
    let claim16 = {
        let f = accept_m4("M7-d claim", &worst_claim(K_FINAL, M_CLAIM, ClaimMode::Pay, F_MAX).build());
        masses(&f.tx, &f.entries)
    };
    println!("\nM7-d (M6-b): drawer and ticket code bytes by K_MAX, and draw() / claim() transient ESTIMATES (4 grams per byte; compute not estimated); baselines measured at K = 16 with the M7 drawer");
    println!("| K_MAX | drawer code B (delta) | ticket code B (delta) | draw: extra B | draw worst transient (estimate) | % | claim PAY F_MAX: extra B | claim transient (estimate) | % |\n|---|---|---|---|---|---|---|---|---|");
    for k in K_FINAL..=19 {
        let t = match variant(&format!("M7 K_MAX {k}"), || k_sources(k)) {
            Ok(t) => t,
            Err(e) => {
                println!("| {k} | compile failed: {} |", e.lines().next().unwrap_or(""));
                break;
            }
        };
        let dk = (k - K_FINAL) as i64;
        let d_drawer = code(&t.drawer) as i64 - code(&committed.drawer) as i64;
        let d_ctrl = code(&t.controller) as i64 - code(&committed.controller) as i64;
        let d_marker = code(&t.marker) as i64 - code(&committed.marker) as i64 + 32 * dk;
        let d_pool_code = code(&t.pool) as i64 - code(&committed.pool) as i64;
        let d_pool = d_pool_code + 32 * dk;
        let d_ticket = code(&t.ticket) as i64 - code(&committed.ticket) as i64;
        let proofs = dk * (576 + 2 * M_DRAW);
        let draw_b = d_drawer + d_ctrl + d_marker + A_MAX as i64 * d_pool + d_pool_code + proofs;
        let claim_b = d_ticket + d_pool + proofs;
        let (de, ce) = (draw16.transient as i64 + 4 * draw_b, claim16.transient as i64 + 4 * claim_b);
        println!(
            "| {k} | {} ({d_drawer:+}) | {} ({d_ticket:+}) | {draw_b} | {de} ({}) | {:.1} | {claim_b} | {ce} ({}) | {:.1} |",
            code(&t.drawer),
            code(&t.ticket),
            if k == K_FINAL { "measured" } else { "estimate" },
            de as f64 * 100.0 / TRANSIENT_LIMIT as f64,
            if k == K_FINAL { "measured" } else { "estimate" },
            ce as f64 * 100.0 / TRANSIENT_LIMIT as f64
        );
    }
    println!("Baselines (measured, K = 16): draw worst transient {}, claim PAY F_MAX transient {}", draw16.transient, claim16.transient);
    assert!(fails.is_empty(), "M7-d (D104 STOP): K = 16, m = 50 not within 90 %: {fails:?}");
}

// ---------------------------------------------------------------- M7-e

/// M7-e: R re-check at R = 1.69 KAS: the `close()` storage worst case (same outputs; the marker
/// lost 9 bytes, storage depends on values and output count) and the `draw()` storage supremum
/// with the record (same outputs: the anchor adds no output; a void has fewer outputs).
#[test]
fn m7e_r_recheck() {
    let r = TEST_RESERVE;
    let (cst, (s, o, jb, a, cl)) = worst_storage(K_FINAL, r as i64, &CALLERS);
    let csup = close_storage_sup(K_FINAL, r as i64);
    println!("\nM7-e: R re-check at R = {r} sompi, K = {K_FINAL}");
    println!("| transition | measured worst | % | supremum | % |\n|---|---|---|---|---|");
    println!(
        "| close (M2-c grid; worst at sales {:.0}, O {:.0}, J_boost {:.0}, A {:.0}, caller {:.0} KAS) | {cst} | {:.1} | {csup} | {:.1} |",
        kas(s as u64),
        kas(o),
        kas(jb),
        kas(a),
        kas(cl),
        cst as f64 * 100.0 / STORAGE_LIMIT as f64,
        csup as f64 * 100.0 / STORAGE_LIMIT as f64
    );
    let mut sups = vec![("close", csup)];
    for f_min in [F_MIN, F_MIN_FLOOR] {
        let mut worst = 0;
        for a in [0, A_MAX] {
            for caller in CALLERS {
                let c = storage_case_rec(a, caller, f_min);
                assert_eq!(c.anchor_bytes().len(), harness::ANCHOR_LEN, "a draw with the anchor");
                let st = c.build().storage_mass();
                let mut v = c.clone();
                v.set_void(true);
                let sv = v.build().storage_mass();
                worst = worst.max(st).max(sv);
            }
        }
        let sup = storage_sup_rec(A_MAX, r, f_min as u64, MIN_CALLER_OUTPUT);
        println!("| draw / void with the record, f_min = {f_min} | {worst} | {:.1} | {sup} | {:.1} |", worst as f64 * 100.0 / STORAGE_LIMIT as f64, sup as f64 * 100.0 / STORAGE_LIMIT as f64);
        assert!(worst <= sup);
        if f_min == F_MIN_FLOOR {
            sups.push(("draw (f_min = F_MIN_FLOOR)", sup));
        }
    }
    let r_close = r_bound(K_FINAL);
    let budget = STORAGE_LIMIT * MARGIN_PCT / 100 - C / F_MIN_FLOOR as u64 - C / MIN_CALLER_OUTPUT;
    let r_draw = (C * (44 + 1 + A_MAX as u64)).div_ceil(budget);
    let binding = sups.iter().max_by_key(|x| x.1).unwrap();
    println!(
        "R = 4C(K + 3) / (0.9 L) at K = {K_FINAL}: {r_close} sompi -> {:.2} KAS (close); draw alone needs R >= {r_draw} sompi ({:.3} KAS) at f_min = F_MIN_FLOOR. Binding transition: {} (supremum {} = {:.1} %)",
        kas(r_close.div_ceil(KAS / 100) * (KAS / 100)),
        kas(r_draw),
        binding.0,
        binding.1,
        binding.1 as f64 * 100.0 / STORAGE_LIMIT as f64
    );
    assert!(cst * 100 <= STORAGE_LIMIT * MARGIN_PCT, "M7-e (D104 STOP): close storage above 90 %");
    for (name, sup) in &sups {
        assert!(sup * 100 <= STORAGE_LIMIT * MARGIN_PCT, "M7-e (D104 STOP): {name} storage supremum {sup} above 90 %");
    }
    assert!(csup * 100 <= STORAGE_LIMIT * MARGIN_PCT, "M7-e (D104 STOP): close storage supremum above 90 %");
    assert!(r >= r_close && r >= r_draw);
}

// ---------------------------------------------------------------- M7-f

/// drawer.sil with `n` extra `int` locals declared at the top of the `draw` entry (before
/// `// ==== check 1:`) and used after check 24 (before `// ==== check 25:`), so all `n` are
/// live across check 24 (the M5 S4-1 method). The values are not constants (`tx.inputs.length
/// + i`), so the compiler keeps a binding for each.
fn drawer_with_live_ints(src: &str, n: usize) -> String {
    let c1 = src.find("        // ==== check 1:").expect("check 1 marker");
    let c25 = src.find("        // ==== check 25:").expect("check 25 marker");
    assert!(c1 < c25);
    let decl: String = (0..n).map(|i| format!("        int xtraLive{i} = tx.inputs.length + {i};\n")).collect();
    let used = if n == 0 { String::new() } else { format!("        require({} >= 0);\n", (0..n).map(|i| format!("xtraLive{i}")).collect::<Vec<_>>().join(" + ")) };
    format!("{}{decl}{}{used}{}", &src[..c1], &src[c1..c25], &src[c25..])
}

/// Compile a drawer source; `Err` carries the compiler's panic message (the panic hook is not
/// replaced: it is process-wide and would hide other tests' panics; the compiler's message
/// is printed once per failing compile).
fn try_drawer(src: &str, ctrl: [u8; 32]) -> Result<Template, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compile_drawer(src, ctrl)))
        .map_err(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "compile panicked".into()))
}

/// The largest n with which the drawer still compiles (n extra live ints across check 24) and
/// the compiler's message at n + 1.
fn stack_headroom() -> (usize, String) {
    let src = std::fs::read_to_string(harness::covenant_path("drawer.sil")).expect("drawer.sil").replace("\r\n", "\n");
    let ctrl = tpl().controller.hash;
    let committed = try_drawer(&src, ctrl).expect("the committed drawer compiles");
    let base = try_drawer(&drawer_with_live_ints(&src, 0), ctrl).expect("n = 0 compiles");
    assert_eq!(code(&base), code(&committed), "n = 0 is the committed drawer");
    for n in 1..=64 {
        if let Err(e) = try_drawer(&drawer_with_live_ints(&src, n), ctrl) {
            return (n - 1, e);
        }
    }
    panic!("64 extra live ints compile: the instrument does not reach the stack limit");
}

/// M7-f: the drawer's stack headroom at check 24 (>= 4 free slots, D104; replaces m6_measure's
/// fixed sentence), and the drawer sensitivity and byte budget (M6-j): the slope of the worst
/// draw's compute and transient per drawer byte (M_DRAW 25 vs 50, K_MAX 16 vs 17) and the
/// bytes the drawer can still grow before the worst draw reaches 90 %.
#[test]
fn m7f_stack_headroom() {
    let (free, msg) = stack_headroom();
    println!("\nM7-f: drawer stack headroom: {free} extra live int locals across check 24 compile; {} fail: {}", free + 1, msg.lines().next().unwrap_or(""));
    assert!(msg.contains("Stack") || msg.contains("stack"), "the first failure is not the stack limit: {msg}");
    assert!(free >= STACK_MIN_FREE, "M7-f (D104 STOP): stack headroom {free} < {STACK_MIN_FREE} free slots at check 24");
}

#[test]
fn m7f_drawer_sensitivity() {
    let committed = tpl();
    let t25 = variant("M7 drawer K_MAX 16 M_DRAW 25", || drawer_sources(16, 25)).expect("M_DRAW = 25 drawer compiles");
    let t17 = variant("M7 drawer K_MAX 17 M_DRAW 50", || drawer_sources(17, 50)).expect("drawer K_MAX = 17 compiles");
    let redeem = |t: &Templates| t.drawer_redeem(&DrawCase::new(4).drawer).len() as i64;
    println!("\nM7-f (M6-j): drawer sensitivity of the worst draw() with the anchor");
    println!("| pair | m | drawer redeem B (a / b) | delta B | compute (a / b) | transient (a / b) | delta compute per B | delta transient per B |\n|---|---|---|---|---|---|---|---|");
    let mut slopes = vec![];
    for (name, ta, tb, m) in [("M_DRAW 25 vs 50", &t25, &committed, 25i64), ("K_MAX 16 vs 17", &committed, &t17, M_DRAW)] {
        let (_, ma) = run_draw(&worst_draw_with(ta, K_FINAL, m), ta.clone());
        let (_, mb) = run_draw(&worst_draw_with(tb, K_FINAL, m), tb.clone());
        let db = redeem(tb) - redeem(ta);
        let sc = (mb.compute as f64 - ma.compute as f64) / db as f64;
        let stt = (mb.transient as f64 - ma.transient as f64) / db as f64;
        println!("| {name} | {m} | {} / {} | {db} | {} / {} | {} / {} | {sc:.3} | {stt:.3} |", redeem(ta), redeem(tb), ma.compute, mb.compute, ma.transient, mb.transient);
        slopes.push((sc, stt));
    }
    let (_, base) = run_draw(&worst_draw_with(&committed, K_FINAL, M_DRAW), committed.clone());
    let sc = slopes.iter().map(|s| s.0).fold(0.0, f64::max);
    let stt = slopes.iter().map(|s| s.1).fold(0.0, f64::max);
    let room_c = (COMPUTE_LIMIT * MARGIN_PCT / 100) as f64 - base.compute as f64;
    let room_t = (TRANSIENT_LIMIT * MARGIN_PCT / 100) as f64 - base.transient as f64;
    let (bc, bt) = (room_c / sc, room_t / stt);
    println!(
        "Worst draw at the committed drawer ({} B): compute {} ({:.1} %), transient {} ({:.1} %). Drawer byte budget to 90 %: compute {bc:.0} B, transient {bt:.0} B; binding: {}",
        redeem(&committed),
        base.compute,
        base.compute as f64 * 100.0 / COMPUTE_LIMIT as f64,
        base.transient,
        base.transient as f64 * 100.0 / TRANSIENT_LIMIT as f64,
        if bc <= bt { "compute" } else { "transient" }
    );
}
