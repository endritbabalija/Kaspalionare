//! M5 measurements M5-a .. M5-e (spec M5 A10 "Measurements"; M5-f is the testnet-10 probe
//! `vault-m5`). Run with
//! `cargo test -p lottery --release --test m5_measure -- --nocapture --test-threads=1`.
//!
//! Limits per transaction, rusty-kaspa v2.1.0 (as m3/m4_measure.rs, checked against
//! `refs/rusty-kaspa-v2.1.0`, commit 01b532e8): compute 500,000, storage 500,000, transient
//! 1,000,000 (`consensus/core/src/config/params.rs:626` mainnet, `:683` testnet;
//! enforced per block and so per transaction at
//! `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:88-96`); storage
//! mass KIP-9 (`consensus/core/src/mass/mod.rs`), C = 10^12
//! (`consensus/core/src/constants.rs:26`, STORAGE_MASS_PARAMETER = SOMPI_PER_KASPA x 10,000).
//! Masses come from the v2.1.0 mass calculator with mainnet parameters (`harness::masses`);
//! compute follows the committed budgets, which `finish()` sizes from the real scripts'
//! units, so every compute figure is of an accepted transaction.
//!
//! Stop rules (D78): (a) the worst `draw()` with conversion and the record (DP21 shape, both
//! sides pending, the refund branch, the cap binding) and the `draw()` storage supremum with
//! the record's R output must be <= 90 % of every mass; (b) `close()` K = 16 with the M5
//! marker <= 90 %. The tests assert them: a failure here is the stop-and-ask signal, not
//! something to optimise around.

mod common;

use common::m2::*;
use common::m5::*;
use harness::{Masses, masses};
use lottery::buy::{Built, Final, KAS, Sources, Template, Templates, b_max};
use lottery::claim::RefreshCase;
use lottery::close::{CloseCase, FundingIn, MIN_CALLER_OUTPUT};
use lottery::draw::{DrawCase, compile_drawer};
use lottery::game::{A_MAX, F_MIN, F_MIN_FLOOR, M_DRAW, P_MIN, SettleMode};
use lottery::smt::Tree;
use lottery::state::{K_MAX, KIND_DEPOSIT, KIND_WITHDRAW, RequestState};
use lottery::vault::{SettleCase, owner};

const COMPUTE_LIMIT: u64 = 500_000;
const TRANSIENT_LIMIT: u64 = 1_000_000;
const STORAGE_LIMIT: u64 = 500_000;
const MARGIN_PCT: u64 = 90;
/// KIP-9 storage-mass constant C (rusty-kaspa v2.1.0 `consensus/core/src/constants.rs:26`).
const C: u64 = 1_000_000_000_000;
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

fn run(step: &str, case: &Case5, b: &Built) -> (Final, Masses) {
    let f = accept5(step, case, b);
    let m = masses(&f.tx, &f.entries);
    (f, m)
}

// ---------------------------------------------------------------- M5-a

/// Redeem length of `src` with `from` replaced by `to` minus the committed one (`which`
/// = "drawer" or "request"), to size one inlined mulfrac call.
fn size_without(which: &str, from: &str, to: &str) -> (usize, usize) {
    let t = tpl();
    let mut src = Sources::committed(b_max());
    let code = |tp: &Template| tp.prefix.len() + tp.suffix.len();
    match which {
        "drawer" => {
            assert_eq!(src.drawer.matches(from).count(), 1, "drawer: `{from}` must occur once");
            let v = compile_drawer(&src.drawer.replace(from, to), t.controller.hash);
            (code(&t.drawer), code(&v))
        }
        _ => {
            assert_eq!(src.request.matches(from).count(), 1, "request: `{from}` must occur once");
            src.request = src.request.replace(from, to);
            let v = Templates::compile_sources(b_max(), &src);
            (code(&t.request), code(&v.request))
        }
    }
}

#[test]
fn m5a_redeem_sizes() {
    let t = tpl();
    let d = DrawCase::new(4);
    let st = st_dep();
    let p = rf_position();
    println!("\nM5-a: redeem script sizes (B_MAX = {})", t.b_max);
    println!("| object | prefix B | suffix B | state B | redeem B |\n|---|---|---|---|---|");
    let row = |name: &str, tp: &Template, redeem: usize| {
        println!("| {name} | {} | {} | {} | {redeem} |", tp.prefix.len(), tp.suffix.len(), redeem - tp.prefix.len() - tp.suffix.len());
    };
    row("vault (draw_leg, request_deposit, request_withdraw)", &t.vault, t.vault_redeem(&d.vault).len());
    row("request (settle, refresh; 2 mulfrac copies)", &t.request, t.request_redeem(&st.request).len());
    row("record (request_leg, refresh)", &t.record, t.record_redeem(&st.record).len());
    let lottery::claim::RefreshObj::Position(ps) = &p.obj else { unreachable!() };
    row("position (vault_leg, refresh)", &t.position, t.position_redeem(ps).len());
    row("drawer (draw_test_seed; 3 mulfrac copies)", &t.drawer, t.drawer_redeem(&d.drawer).len());
    row("marker (draw_leg, refresh)", &t.marker, t.marker_redeem(&CloseCase::new(4).expected().marker).len());

    // Bytes per inlined mulfrac call: the same source with one call replaced by a plain
    // expression of the same arguments (the rest of the script unchanged).
    let (dc, dv) = size_without("drawer", "require(mulfrac(mA, wc + 1, nS) > slack);", "require(mA + wc + 1 + nS > slack);");
    let (rc, rv) = size_without("request", "pay = mulfrac(rc.out_rem, conv, rc.burn_rem);", "pay = rc.out_rem + conv + rc.burn_rem;");
    println!("\nM5-a: bytes per mulfrac call (code bytes of the committed script minus a variant with one call replaced)");
    println!("| script | committed code B | one call removed B | per call B |\n|---|---|---|---|");
    println!("| drawer (third call, out(wc + 1)) | {dc} | {dv} | {} |", dc - dv);
    println!("| request (pay call) | {rc} | {rv} | {} |", rc - rv);
    assert!(dc > dv && rc > rv);
}

// ---------------------------------------------------------------- M5-b

/// ST_ALL with every lower tier funded (np = 7), as m3/m4_measure.rs.
const ST_ALL_DRAW: [i64; 5] = [400, 300, 300, 200, 200];

/// The DP21 worst shape (m4_measure.rs `dp21`): round 2, K shards, marker m, every leaf at
/// the winning rank non-empty with every bonus held, sales 1,000,000 KAS, `a` absorbed
/// pools, `nf` funding inputs.
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

#[derive(Clone, Copy, Debug)]
enum Pending {
    Nothing,
    /// 30 KAS deposited (converted) and 3,000,000 shares withdrawn within the slack (wc = W:
    /// two mulfrac calls).
    Both,
    /// 30 KAS deposited (converted) and 95 % of the shares withdrawn: the cap binds (wc < W:
    /// all three mulfrac calls).
    Cap,
    /// The refund branch: total_shares = 1,000,000 (price about 10^9 sompi per share), 30
    /// KAS deposited (mint about 3 < ceil(D / MIN_DEPOSIT) = 30: C1 computes the mint and
    /// refunds) and 95 % of the shares withdrawn (cap binding: three mulfrac calls).
    RefundCap,
}

fn with_pending(mut c: DrawCase, p: Pending) -> DrawCase {
    match p {
        Pending::Nothing => {}
        Pending::Both => {
            c.vault.pend_dep = 30 * KAS as i64;
            c.vault.pend_wsh = 3_000_000;
        }
        Pending::Cap => {
            c.vault.pend_dep = 30 * KAS as i64;
            c.vault.pend_wsh = c.vault.total_shares / 100 * 95;
        }
        Pending::RefundCap => {
            c.vault.total_shares = 1_000_000;
            c.vault.pend_dep = 30 * KAS as i64;
            c.vault.pend_wsh = 950_000;
        }
    }
    let x = c.expected();
    let cv = &x.conv;
    match p {
        Pending::Nothing => assert!(!x.rec),
        Pending::Both => assert!(x.rec && cv.ok && cv.wc == cv.w, "{cv:?}"),
        Pending::Cap => assert!(x.rec && cv.ok && cv.wc < cv.w, "{cv:?}"),
        Pending::RefundCap => assert!(x.rec && cv.refund && cv.mint > 0 && cv.wc < cv.w, "{cv:?}"),
    }
    c
}

#[test]
fn m5b_dp21_with_conversion_and_record() {
    let t = tpl();
    println!(
        "\nM5-b: DP21 (K = {K_MAX}, m = {M_DRAW}, a = A_MAX = {A_MAX}) with the conversion and the record; drawer redeem {} B, vault redeem {} B",
        t.drawer_redeem(&DrawCase::new(4).drawer).len(),
        t.vault_redeem(&DrawCase::new(4).vault).len()
    );
    println!("| pending | np | funding inputs | drawer units / sigscript B | vault leg units | outputs | compute | transient | storage | % c / t / s | <= 90 % |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    let mut worst = [0f64; 3];
    let mut fails = vec![];
    for p in [Pending::Nothing, Pending::Both, Pending::Cap, Pending::RefundCap] {
        for (np, nf) in [(5, 1), (5, F_MAX), (7, 1), (7, F_MAX)] {
            let c = with_pending(dp21(K_MAX, M_DRAW, A_MAX, nf, np), p);
            let case: Case5 = c.clone().into();
            let (f, m) = run(&format!("M5-b {p:?} np {np} nf {nf}"), &case, &c.build());
            if !matches!(p, Pending::Nothing) {
                check_draw5(&c, &f);
            }
            let ok = within(&m, MARGIN_PCT).iter().all(|&b| b);
            let (pc, pt, ps) = pct(&m);
            worst = [worst[0].max(pc), worst[1].max(pt), worst[2].max(ps)];
            println!(
                "| {p:?} | {np} | {nf} | {} / {} | {} | {} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {ok} |",
                f.units[0],
                f.tx.inputs[0].signature_script.len(),
                f.units[2],
                f.tx.outputs.len(),
                m.compute,
                m.transient,
                m.storage
            );
            if !ok {
                fails.push(format!("{p:?} np {np} nf {nf}: {m:?}"));
            }
        }
    }
    println!("M5-b worst: compute {:.1} %, transient {:.1} %, storage {:.1} %", worst[0], worst[1], worst[2]);
    assert!(fails.is_empty(), "M5-b (D78a STOP): draw() above 90 % of a mass limit: {fails:?}");
}

// ---------------------------------------------------------------- M5-c

/// m3_measure.rs `storage_case_at` (every covenant output at R or 1 sompi above, T_F =
/// f_min, caller output `caller`) with W = 1 share pending at the smallest V': wc = 1, out_c =
/// floor((V' + 1) x 1 / (S + VS)) = 0, so the record is written at exactly R (vault' at R + V'): the draw with one more R output. Storage depends only on values and scripts.
fn storage_case_rec(a: usize, caller: u64, f_min: i64) -> DrawCase {
    let mut c = DrawCase::at_round(4, 2);
    c.drawer.f_min = f_min;
    c.marker.st = ST_ALL_DRAW;
    c.marker.jackpot = 1;
    c.marker.j_boost = 0;
    c.marker.g2 = 1;
    c.hold_jackpot(0, 1);
    let ob = c.other_bonus(0);
    c.hold(1, ob, 1);
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
    c.controller.acc = c.drawer.f_min - x.amounts.f;
    assert!(c.controller.acc >= 0);
    c.vault.total_assets = (c.vault.total_assets - c.expected().v_prime).max(0);
    c.vault.liability = 0;
    c.vault.pend_wsh = 1;
    let x = c.expected();
    let need = caller as i128 + (x.np() as u64 * c.reserve()) as i128 + c.reserve() as i128 - x.amounts.c_draw as i128 + c.fee as i128;
    c.funding[0].value = need as u64;
    let x = c.expected();
    assert!(x.rec && x.conv.out_c == 0 && x.conv.wc == 1, "{:?}", x.conv);
    assert_eq!(x.values[x.record_output().unwrap()], c.reserve(), "record at R");
    // V' is as small as the state allows (m3_measure.rs: V = max(V - V', 0)), so vault' is
    // R + V' with V' a few hundredths of a KAS at most.
    assert!(x.values[1] >= c.reserve() && x.v_prime < KAS as i64, "vault' near R: V' = {}", x.v_prime);
    assert_eq!(x.values[x.caller_output()], caller);
    c
}

/// m3_measure.rs `storage_sup` with one more covenant output at >= R (the record: P2SH with
/// a covenant binding, plurality 2, so 4 C / R): 3 singleton outputs, up to 7 pools and the
/// record, each >= R; the fee >= f_min, the marker refund and a pool refunds = R, the
/// caller >= caller_min. Input relief dropped.
fn storage_sup_rec(a: usize, r: u64, f_min: u64, caller_min: u64) -> u64 {
    11 * (4 * C / r) + (1 + a as u64) * (C / r) + C / f_min + C / caller_min
}

#[test]
fn m5c_draw_storage_with_the_record() {
    let r = lottery::params::TEST_RESERVE;
    let mut fails = vec![];
    for f_min in [F_MIN, F_MIN_FLOOR] {
        println!("\nM5-c: draw() storage, every covenant output at R incl. the record (R = {r} sompi), T_F = f_min = {f_min} sompi, caller >= MIN_CALLER_OUTPUT = {MIN_CALLER_OUTPUT} sompi");
        println!("| a | caller output (KAS) | storage | % of limit | supremum (relief dropped) |\n|---|---|---|---|---|");
        for a in [0, A_MAX] {
            for caller in [KAS, 10 * KAS, 1_000 * KAS, 1_000_000 * KAS, 100_000_000 * KAS] {
                let c = storage_case_rec(a, caller, f_min);
                let st = c.build().storage_mass();
                let sup = storage_sup_rec(a, r, f_min as u64, MIN_CALLER_OUTPUT);
                println!("| {a} | {} | {st} | {:.1} | {sup} |", caller / KAS, st as f64 * 100.0 / STORAGE_LIMIT as f64);
                assert!(st <= sup, "storage {st} above the supremum {sup}");
            }
        }
        let sup = storage_sup_rec(A_MAX, r, f_min as u64, MIN_CALLER_OUTPUT);
        println!("Supremum at a = A_MAX, f_min = {f_min}: {sup} ({:.1} %)", sup as f64 * 100.0 / STORAGE_LIMIT as f64);
        if sup * 100 > STORAGE_LIMIT * MARGIN_PCT {
            fails.push((f_min, sup));
        }
    }
    // One measured case with the scripts run (the record at R is a valid draw).
    let c = storage_case_rec(0, KAS, F_MIN_FLOOR);
    let f = accept5("M5-c run", &c.clone().into(), &c.build());
    check_draw5(&c, &f);
    assert!(fails.is_empty(), "M5-c (D78a STOP): draw() storage supremum with the record above 90 %: {fails:?}");
}

// ---------------------------------------------------------------- M5-d

#[test]
fn m5d_close_k16_with_the_m5_marker() {
    let t = tpl();
    println!("\nM5-d: close() with the M5 marker ({} B redeem; its DRAWER_TPL is the M5 drawer's hash)", t.marker_redeem(&CloseCase::new(4).expected().marker).len());
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
            assert!(ok, "M5-d (D78b STOP): K = 16 close() above 90 %: {m:?}");
        }
    }
}

// ---------------------------------------------------------------- M5-e

fn st(rc: lottery::state::RecordState, kind: u8, amount: i64, mode: SettleMode, drained: bool) -> SettleCase {
    let c = SettleCase::new(rc, RequestState::new(2, kind, amount, &owner().payout()));
    let x = c.expected();
    assert!(x.reference.is_some(), "settle base rejected by game::settle");
    assert_eq!((x.mode, x.drained), (mode, drained));
    c
}

/// Every settle mode, drained and not.
fn settle_shapes() -> Vec<(&'static str, SettleCase)> {
    let k = KAS as i64;
    vec![
        ("deposit-convert", st_dep()),
        ("deposit-convert, drained", st_dep_last()),
        ("deposit-refund", st_refund()),
        ("deposit-refund, drained", st(record(1, 10 * k, 0, 0, 0, 0), KIND_DEPOSIT, 10 * k, SettleMode::DepositRefund, true)),
        ("withdrawal FINAL", st_final()),
        ("withdrawal FINAL, drained", st(record(0, 0, 0, 1_000_000, 1_000_000, 15 * k), KIND_WITHDRAW, 1_000_000, SettleMode::Final, true)),
        ("withdrawal PAY", st_pay()),
        ("withdrawal PAY, drained", st(record(0, 0, 0, 1_000_000, 500_000, 15 * k), KIND_WITHDRAW, 1_000_000, SettleMode::Pay, true)),
        ("withdrawal ACCRUE", st_accrue()),
        ("withdrawal ACCRUE, drained", st(record(0, 0, 0, 1_000_000, 500_000, P_MIN - 1), KIND_WITHDRAW, 1_000_000, SettleMode::Accrue, true)),
    ]
}

#[test]
fn m5e_request_settle_refresh_masses() {
    println!("\nM5-e: request entries, settle() and refresh() (one funding input, change; covenant outputs at R)");
    println!("| transaction | leader units / sigscript B | leg units | outputs | compute | transient | storage | % c / t / s | <= 90 % |");
    println!("|---|---|---|---|---|---|---|---|---|");
    let mut shapes: Vec<(String, Case5)> = vec![
        ("request_deposit".into(), dep().into()),
        ("request_withdraw whole (Schnorr owner)".into(), wd_whole(owner()).into()),
        ("request_withdraw partial (Schnorr owner)".into(), wd(owner()).into()),
    ];
    for (name, key) in keys() {
        if name != "Schnorr" {
            shapes.push((format!("request_withdraw partial ({name} owner)"), wd(key).into()));
        }
    }
    for (name, c) in settle_shapes() {
        shapes.push((format!("settle {name}"), c.into()));
    }
    for (name, c) in refresh5_bases() {
        let c: RefreshCase = c;
        shapes.push((format!("refresh {name}"), c.into()));
    }
    let mut fails = vec![];
    for (name, case) in shapes {
        let b = case.build();
        let (f, m) = run(&name, &case, &b);
        let leg = if f.tx.inputs.len() > 1 && matches!(case, Case5::Withdraw(_) | Case5::Settle(_)) { f.units[1].to_string() } else { "-".into() };
        let ok = within(&m, MARGIN_PCT).iter().all(|&x| x);
        let (pc, pt, ps) = pct(&m);
        println!(
            "| {name} | {} / {} | {leg} | {} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {ok} |",
            f.units[0],
            f.tx.inputs[0].signature_script.len(),
            f.tx.outputs.len(),
            m.compute,
            m.transient,
            m.storage
        );
        if !ok {
            fails.push(format!("{name}: {m:?}"));
        }
    }
    assert!(fails.is_empty(), "M5-e: above 90 %: {fails:?}");
}
