//! M6 measurements M6-a .. M6-j (spec M6 A5). Run with
//! `cargo test -p lottery --release --test m6_measure -- --nocapture --test-threads=1`.
//!
//! Limits per transaction, rusty-kaspa v2.1.0 (`refs/rusty-kaspa-v2.1.0`, commit 01b532e8):
//! compute 500,000, storage 500,000, transient 1,000,000
//! (`consensus/core/src/config/params.rs:626`, enforced per block and so per transaction at
//! `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:88-96`); storage
//! mass KIP-9 (`consensus/core/src/mass/mod.rs:439-512`, plurality `:83-99`), C = 10^12
//! (`consensus/core/src/constants.rs:26`); transient 4 grams per byte
//! (`consensus/core/src/constants.rs:31`, TRANSIENT_BYTE_TO_MASS_FACTOR). Masses come from
//! the v2.1.0 mass calculator with mainnet parameters (`harness::masses`); compute follows
//! the committed budgets, which `finish()` sizes from the real scripts' units, so every
//! compute figure is of an accepted transaction (estimates are labelled "estimate").
//!
//! Fees: the relay floor is `lottery::buy::relay_floor_fee` (100 sompi per gram of
//! max(compute, ceil(transient / 2)), rusty-kaspa v2.1.0
//! `mining/src/mempool/check_transaction_standard.rs:71-76, 92-100`, `config.rs:20`); the
//! priority fee is `harness::priority_fee` (100 sompi per gram of the normalized max over
//! compute, transient and storage, `mining/src/mempool/model/frontier/feerate_key.rs:86-98`).
//!
//! Stop rule D90: every M6-a shape <= 90 % of every mass, and the committed (K, B_MAX,
//! m_max) = (16, 4, 50) within 90 % for every transition. The tests assert them; a failure
//! is the stop-and-ask signal, not something to tune around. User decision U-M6-1: genesis
//! is exempt from the 90 % margin (deployer-chosen output values; over the limit means only
//! "cannot deploy"); its rows are asserted <= 100 % and the builder refuses a genesis over
//! 100 % (`lottery::genesis::genesis_mass_check`). Frontier variants (K_MAX != 16,
//! B_MAX != 4, M_DRAW != 50) are measurement only, compiled in memory (D91).

mod common;

use std::sync::Arc;

use common::m2::{accept_draw, accept_m4, tpl};
use common::m4::{refresh_pool, refresh_ticket};
use common::m5::{Case5, accept5, dep, keys, refresh5_bases, wd, wd_whole};
use common::m6::wd_at_wmin;
use common::measure::*;
use harness::{Masses, masses, priority_fee};
use lottery::buy::{BuyCase, Built, Final, KAS, M_REF, TEST_DEPOSIT, Template, Templates, b_max, funding_spk, relay_floor_fee};
use lottery::claim::{ClaimCase, RefreshObj, ReturnCase};
use lottery::close::{CloseCase, MIN_CALLER_OUTPUT};
use lottery::draw::DrawCase;
use lottery::game::{A_MAX, ClaimMode, F_MIN, F_MIN_FLOOR, M_CLAIM, M_DRAW, P_MIN, muldiv, w_min};
use lottery::genesis::{D_FLOOR, GENESIS_FUNDING, GenesisCase, R_MIN, V0_MIN};
use lottery::params::{SetParamsCase, TEST_RESERVE};
use lottery::smt::Tree;
use lottery::state::K_MAX;
use lottery::vault::owner;

/// The committed (K, B_MAX, m_max) of spec M6 A2 (K and m_max provisional, D83).
const K_FINAL: usize = 16;
const B_MAX_FINAL: usize = 4;
const M_FINAL: i64 = 50;

fn kas(s: u64) -> f64 {
    s as f64 / KAS as f64
}

/// One measured shape: input 0's units and sigscript bytes, output count and masses.
struct Row {
    name: String,
    units0: u64,
    sig0: usize,
    leg: Option<u64>,
    outputs: usize,
    m: Masses,
    /// U-M6-1: genesis is exempt from the 90 % margin (asserted <= 100 % instead).
    exempt: bool,
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
            exempt: false,
        }
    }

    /// A genesis row (U-M6-1): asserted <= 100 %, not <= 90 %.
    fn genesis(name: impl Into<String>, f: &Final) -> Self {
        Row { exempt: true, ..Row::new(name, f, None) }
    }

    fn ok(&self) -> bool {
        within(&self.m, if self.exempt { 100 } else { MARGIN_PCT }).iter().all(|&b| b)
    }

    fn within90(&self) -> bool {
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
        if r.exempt { format!("{}; exempt (U-M6-1), <= 100 %: {}", r.within90(), r.ok()) } else { r.ok().to_string() }
    );
}

// ---------------------------------------------------------------- shape builders

/// Genesis at K = 16 with the fixture instance (GenesisCase::new_with, V0 = 10,000,000 KAS,
/// one funding input of GENESIS_FUNDING with change).
fn genesis(k: usize) -> Final {
    let t = tpl();
    let mut ledger = lottery::ledger::Ledger::new();
    let op = ledger.fund(GENESIS_FUNDING, funding_spk(), 100);
    let entry = ledger.get(&op).unwrap().clone();
    let g = GenesisCase::new_with(k, vec![(op, entry)], &t);
    common::accept(&g.build_with(t))
}

/// Sum of the genesis protocol outputs (controller, params, K shards, vault, drawer, position).
fn genesis_outs(g: &GenesisCase) -> u64 {
    g.controller_value() + g.reserve() + (0..g.k()).map(|i| g.shard_value(i)).sum::<u64>() + g.vault_value() + 2 * g.reserve()
}

/// The worst genesis `validate()` allows for storage (U-M6-1): K = 16, every covenant output
/// at its smallest value: acc = pot = 0 (controller at R), J = 0 (so L(1) = 0 and V0 = V0_MIN
/// passes the cap check), V0 = V0_MIN (vault at V0_MIN + R), R = R_MIN; one funding input
/// covering the outputs, `fee` and `change` (None: no change output, exact funding).
fn worst_genesis_case(t: &Arc<Templates>, fee: u64, change: Option<u64>) -> GenesisCase {
    let mut g = GenesisCase::new_with(K_FINAL, vec![], t);
    g.controller.acc = 0;
    g.pot = 0;
    g.set_jackpot(0);
    g.set_vault(V0_MIN);
    assert_eq!(g.reserve(), R_MIN, "fixture R = R_MIN");
    g.fee = fee;
    g.change = change.is_some();
    let mut ledger = lottery::ledger::Ledger::new();
    let op = ledger.fund(genesis_outs(&g) + fee + change.unwrap_or(0), funding_spk(), 100);
    let entry = ledger.get(&op).unwrap().clone();
    g.funding = vec![(op, entry)];
    g
}

/// U-M6-1 genesis rows: the worst genesis with no change, with large change (1,000,000 KAS),
/// and with the smallest change the builder's mass check accepts (binary search; one sompi
/// less is refused). The fee is the relay floor of the change shape. Returns the rows and the
/// smallest accepted change.
fn worst_genesis_rows(t: &Arc<Templates>) -> (Vec<Row>, u64) {
    // The fee: the relay floor of the change shape (the larger of the two shapes' floors; the
    // fee does not change compute or transient, and changes storage by a few grams at most).
    let probe = common::accept(&worst_genesis_case(t, KAS / 2, Some(1_000_000 * KAS)).build_with(t.clone()));
    let fee = relay_floor_fee(&masses(&probe.tx, &probe.entries));
    let ok = |c: u64| worst_genesis_case(t, fee, Some(c)).try_build_with(t.clone()).is_ok();
    let c_min = search(1, 1_000 * KAS, ok);
    assert!(!ok(c_min - 1));
    let err = worst_genesis_case(t, fee, Some(c_min - 1)).try_build_with(t.clone()).err().unwrap();
    assert!(err.contains("U-M6-1") && err.contains("storage"), "{err}");
    let lead = format!("genesis worst under validate() + U-M6-1 (K = {K_FINAL}, acc = pot = 0, J = 0, V0 = V0_MIN, R = R_MIN, fee = relay floor {fee})");
    let mut rows = vec![];
    for (name, change) in [("no change", None), ("change 1,000,000 KAS", Some(1_000_000 * KAS)), ("smallest accepted change", Some(c_min))] {
        let f = common::accept(&worst_genesis_case(t, fee, change).build_with(t.clone()));
        let label = match change {
            Some(c) if c == c_min => format!("{lead}, {name} {c} sompi ({:.4} KAS; {} refused)", kas(c), c - 1),
            _ => format!("{lead}, {name}"),
        };
        rows.push(Row::genesis(label, &f));
    }
    (rows, c_min)
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

/// M1/M4 buy shapes: b combinations, referral 0.1 x b KAS at price 1 KAS (b = 1: M_REF), large
/// change or about 1 KAS of change.
fn buy_shape(bm: usize, b: usize, m: usize, with_ref: bool, small_change: bool) -> BuyCase {
    let mut c = BuyCase::new(bm).with_tree(Tree::new(m));
    c.bundle = common::distinct(b, m.min(255) as u8);
    c.with_ref = with_ref;
    if small_change {
        c.funding = vec![c.gross() + c.b() * c.shard.d as u64 + c.fee + KAS];
    }
    c
}

/// The worst draw of M5-b (DP21 at K, m, a = A_MAX, np = 7, F_MAX funding inputs, the cap
/// binding with the record), against templates `t`.
fn worst_draw_with(t: &Templates, k: usize, m: i64) -> DrawCase {
    with_pending(dp21_with(t, k, m, A_MAX, F_MAX, 7), Pending::Cap)
}

/// A typical draw: K = 16, round 2, nobody holds the jackpot or a second prize, a = 0, three
/// lower-tier pools (np = 3, the fixture's tier shares 400 / 300 / 300), one funding input.
fn typical_draw() -> DrawCase {
    let mut c = DrawCase::at_round(K_FINAL, 2);
    let mut seen = vec![];
    for s in [10u64, 100, 1_000, 10_000, 100_000] {
        c.set_sales((s * KAS) as i64, (s * KAS / 20) as i64);
        let x = c.expected();
        seen.push((s, x.np()));
        if x.np() == 3 && x.h_j == 0 && x.n2 == 0 {
            return c;
        }
    }
    panic!("no typical draw with np = 3: (sales KAS, np) {seen:?}");
}

fn run_draw(c: &DrawCase, t: Arc<Templates>) -> (Final, Masses) {
    let f = accept_draw(&c.build_with(t));
    let m = masses(&f.tx, &f.entries);
    (f, m)
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

/// Smallest sales S with muldiv(S, s_bp) >= fee.
fn sales_covering(fee: u64, s_bp: i64) -> u64 {
    search(0, 1 << 50, |s| muldiv(s as i64, s_bp) >= fee as i64)
}

// ---------------------------------------------------------------- M6-a, M6-i: redeem sizes

#[test]
fn m6a_redeem_sizes() {
    let t = tpl();
    let draw = DrawCase::new(4);
    let close = CloseCase::new(4);
    let claim = ClaimCase::new(4);
    let st = common::m5::st_dep();
    let pos = common::m5::rf_position();
    let RefreshObj::Position(ps) = &pos.obj else { unreachable!() };
    println!("\nM6-a / M6-i: redeem script sizes of every template at the final constants (B_MAX = {}, K_MAX = {K_MAX}, M_DRAW = M_CLAIM = {M_DRAW})", t.b_max);
    println!("| object | prefix B | suffix B | state B | redeem B |\n|---|---|---|---|---|");
    let row = |name: &str, tp: &Template, redeem: usize| {
        println!("| {name} | {} | {} | {} | {redeem} |", tp.prefix.len(), tp.suffix.len(), redeem - code(tp));
    };
    row("shard (buy, close_leg)", &t.shard, t.shard_redeem(&BuyCase::new(t.b_max).shard).len());
    row("ticket (claim, return x2, refresh)", &t.ticket, t.ticket.redeem(&claim.ticket.values()).len());
    row("controller (close, draw_leg)", &t.controller, t.controller_redeem(&close.controller).len());
    row("params (set_params, close_leg)", &t.params, t.params_redeem(&close.params).len());
    row("marker (draw_leg)", &t.marker, t.marker_redeem(&close.expected().marker).len());
    row("drawer (draw, anchor proof)", &t.drawer, t.drawer_redeem(&draw.drawer).len());
    row("vault (draw_leg, request_deposit, request_withdraw with W4b)", &t.vault, t.vault_redeem(&draw.vault).len());
    row("pool (absorb_leg, ticket_leg, refresh)", &t.pool, t.pool_redeem(&claim.pool).len());
    row("record (request_leg, refresh)", &t.record, t.record_redeem(&st.record).len());
    row("position (vault_leg, refresh)", &t.position, t.position_redeem(ps).len());
    row("request (settle, refresh)", &t.request, t.request_redeem(&st.request).len());
}

// ---------------------------------------------------------------- M6-a, M6-i: every transition

/// Every M6-a shape, accepted by the real scripts (one funding input unless stated).
fn m6a_rows() -> Vec<Row> {
    let mut rows = vec![];
    let t = tpl();
    let bm = b_max();
    assert_eq!(bm, B_MAX_FINAL);

    rows.push(Row::genesis(format!("genesis K = {K_FINAL} (fixture: V0 = 10,000,000 KAS, acc 2 KAS, large change)"), &genesis(K_FINAL)));
    rows.extend(worst_genesis_rows(&t).0);

    for b in [1, bm] {
        for with_ref in [false, true] {
            for small in [false, true] {
                let c = buy_shape(bm, b, 10, with_ref, small);
                let f = common::accept(&c.build());
                let name = format!(
                    "buy b = {b}, referral {}, change {}",
                    if with_ref { format!("{:.1} KAS", kas(c.referral())) } else { "none".into() },
                    if small { "1 KAS" } else { "large" }
                );
                rows.push(Row::new(name, &f, None));
            }
        }
    }

    for change in [true, false] {
        let mut sp = SetParamsCase::new();
        if !change {
            sp.change = false;
            sp.funding = vec![sp.fee];
        }
        rows.push(Row::new(format!("set_params, change {change}"), &common::accept(&sp.build()), None));
    }

    let c = close_k_with(K_FINAL, &t);
    rows.push(Row::new(format!("close K = {K_FINAL}, sales in every shard (M2-a shape)"), &common::accept(&c.build()), None));
    let (st, (s, o, jb, a, cl)) = worst_storage(K_FINAL, TEST_RESERVE as i64, &CALLERS);
    let c = close_case_c(K_FINAL, TEST_RESERVE as i64, s, o, jb, a, cl).unwrap();
    let f = common::accept(&c.build());
    assert_eq!(masses(&f.tx, &f.entries).storage, st);
    rows.push(Row::new(format!("close K = {K_FINAL}, storage worst case (M2-c grid: sales {:.0}, O {:.0}, J_boost {:.0}, A {:.0}, caller {:.0} KAS)", kas(s as u64), kas(o), kas(jb), kas(a), kas(cl)), &f, None));

    let c = worst_draw_with(&t, K_FINAL, M_DRAW);
    let (f, _) = run_draw(&c, t.clone());
    rows.push(Row::new(format!("draw worst (DP21 K = {K_FINAL}, m = {M_DRAW}, a = {A_MAX}, np = 7, {F_MAX} funding inputs, cap binding, record)"), &f, Some(2)));
    let c = typical_draw();
    let (f, _) = run_draw(&c, t.clone());
    rows.push(Row::new(format!("draw typical (K = {K_FINAL}, m = 10, a = 0, np = 3, no record)"), &f, Some(2)));

    for mode in [ClaimMode::Pay, ClaimMode::Accrue, ClaimMode::Final] {
        let c = worst_claim(K_FINAL, M_CLAIM, mode, 1);
        let f = accept_m4("M6-a claim", &c.build());
        rows.push(Row::new(format!("claim {mode:?} K = {K_FINAL}, m = {M_CLAIM} (h = K)"), &f, Some(1)));
    }
    let c = worst_claim(K_FINAL, M_CLAIM, ClaimMode::Pay, F_MAX);
    rows.push(Row::new(format!("claim Pay K = {K_FINAL}, m = {M_CLAIM}, {F_MAX} funding inputs"), &accept_m4("M6-a claim F_MAX", &c.build()), Some(1)));
    let f = accept_m4("M6-a claim P_MIN", &pay_case(P_MIN as u64, 1_000_000 * KAS).build());
    rows.push(Row::new("claim Pay at P = P_MIN, storage worst case (M4-c: b = 2, pool' at R, change 1,000,000 KAS)", &f, Some(1)));

    rows.push(Row::new("return_deposit_after (b = 2 ticket)", &accept_m4("M6-a return", &ReturnCase::after().build()), None));
    rows.push(Row::new("return_deposit_with_pool (b = 2 ticket, K = 1 pool)", &accept_m4("M6-a return pool", &ReturnCase::with_pool().build()), Some(1)));

    // Spec M7 D99: the marker's refresh rows are removed (the marker has no `refresh()`).
    for (name, r) in [("ticket", refresh_ticket()), ("pool (K = 4)", refresh_pool())] {
        rows.push(Row::new(format!("refresh {name}"), &accept_m4("M6-a refresh", &r.build()), None));
    }
    for (name, r) in refresh5_bases() {
        let case: Case5 = r.into();
        rows.push(Row::new(format!("refresh {name}"), &accept5("M6-a refresh", &case, &case.build()), None));
    }

    let mut vault_cases: Vec<(String, Case5, Option<usize>)> = vec![("request_deposit (D = 10 KAS = MIN_DEPOSIT)".into(), dep().into(), None)];
    vault_cases.push(("request_withdraw whole (Schnorr owner)".into(), wd_whole(owner()).into(), Some(1)));
    for (kname, key) in keys() {
        vault_cases.push((format!("request_withdraw partial ({kname} owner)"), wd(key).into(), Some(1)));
    }
    let wb = wd_at_wmin(owner());
    assert_eq!(wb.w, w_min(wb.vault.total_assets, wb.vault.total_shares));
    vault_cases.push((format!("request_withdraw partial at the W4b boundary w = wmin = {} (M6-i, Schnorr owner)", wb.w), wb.into(), Some(1)));
    for (name, s) in settle_shapes() {
        vault_cases.push((format!("settle {name}"), s.into(), Some(1)));
    }
    for (name, case, leg) in vault_cases {
        let f = accept5(&name, &case, &case.build());
        rows.push(Row::new(name, &f, leg));
    }
    rows
}

#[test]
fn m6a_every_transition_within_90() {
    let rows = m6a_rows();
    println!("\nM6-a / M6-h / M6-i: every transition at the final constants (K = {K_FINAL}, B_MAX = {B_MAX_FINAL}, m_max = {M_FINAL}, d = {TEST_DEPOSIT}, R = {TEST_RESERVE}, MIN_DEPOSIT = W_MIN_VALUE = 10 KAS); fees at 100 sompi per gram");
    println!("{TABLE_HEAD}");
    let mut worst: [(f64, String); 3] = Default::default();
    for r in &rows {
        print_row(r);
        if r.exempt {
            continue;
        }
        let (pc, pt, ps) = pct(&r.m);
        for (i, p) in [pc, pt, ps].into_iter().enumerate() {
            if p > worst[i].0 {
                worst[i] = (p, r.name.clone());
            }
        }
    }
    println!("Tightest (genesis excluded, U-M6-1): compute {:.1} % ({}); transient {:.1} % ({}); storage {:.1} % ({})", worst[0].0, worst[0].1, worst[1].0, worst[1].1, worst[2].0, worst[2].1);
    for r in rows.iter().filter(|r| r.exempt) {
        let (pc, pt, ps) = pct(&r.m);
        println!("Genesis, exempt from the 90 % margin (U-M6-1), asserted <= 100 %: {pc:.1} / {pt:.1} / {ps:.1} % ({})", r.name);
    }
    let fails: Vec<String> = rows.iter().filter(|r| !r.ok()).map(|r| format!("{}: {:?}", r.name, r.m)).collect();
    assert!(fails.is_empty(), "M6-a (D90 STOP): non-genesis shapes above 90 % or genesis above 100 % (U-M6-1): {fails:#?}");
}

// ---------------------------------------------------------------- M6-b: K frontier

#[test]
fn m6b_k_frontier() {
    let committed = tpl();
    // Baselines for the draw() and claim() estimates (committed K_MAX = 16).
    let (_, draw16) = run_draw(&worst_draw_with(&committed, K_FINAL, M_DRAW), committed.clone());
    let claim16 = {
        let f = accept_m4("M6-b claim", &worst_claim(K_FINAL, M_CLAIM, ClaimMode::Pay, F_MAX).build());
        masses(&f.tx, &f.entries)
    };
    println!("\nM6-b: close() at K = K_MAX on in-memory K_MAX variants (controller, marker, drawer, pool, ticket: K_MAX line and byte[32 K_MAX] roots; D91), M2-a shape (1,000 KAS sales and 50 KAS referral per shard, distinct roots), R = {TEST_RESERVE}");
    println!("| K_MAX = K | controller redeem B | controller units / sigscript B | compute | transient | storage at R = 1.69 | % c / t / s | R(K) = 4C(K+3)/(0.9 L) (KAS, 0.01 up) | storage at R(K) |");
    println!("|---|---|---|---|---|---|---|---|---|");
    let mut k90 = [0usize; 3];
    let mut k100 = [0usize; 3];
    let mut est = vec![];
    let mut sizes = vec![];
    for k in K_FINAL..=20 {
        let t = match variant(&format!("K_MAX {k}"), || k_sources(k)) {
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
        println!(
            "| {k} | {} | {} / {} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {:.2} | {st_rk} ({:.1} %) |",
            t.controller_redeem(&c.controller).len(),
            f.units[0],
            f.tx.inputs[0].signature_script.len(),
            m.compute,
            m.transient,
            m.storage,
            kas(rk),
            st_rk as f64 * 100.0 / STORAGE_LIMIT as f64
        );
        let (w90, w100) = (within(&m, MARGIN_PCT), within(&m, 100));
        for l in 0..3 {
            if w90[l] && k90[l] == k - 1 || (k == K_FINAL && w90[l]) {
                k90[l] = k;
            }
            if w100[l] && k100[l] == k - 1 || (k == K_FINAL && w100[l]) {
                k100[l] = k;
            }
        }
        if k == K_FINAL {
            assert!(w90.iter().all(|&b| b), "M6-b (D90 STOP): K = 16 close() above 90 %: {m:?}");
        }
        // draw() and claim() at K_MAX = k: redeem-size estimates (transient only, 4 grams per
        // byte). Components: the drawer and controller code (pushed in draw), the marker and
        // every absorbed pool (code + 32 B per extra root), the pool code again as the drawer's
        // pool prefix/suffix arguments, and one more worst proof (576 + 2m B) per extra shard;
        // claim: the ticket code, the pool (code + 32 B per extra root) and the proofs.
        let dk = k - K_FINAL;
        let d_drawer = code(&t.drawer) as i64 - code(&committed.drawer) as i64;
        let d_ctrl = code(&t.controller) as i64 - code(&committed.controller) as i64;
        let d_marker = code(&t.marker) as i64 - code(&committed.marker) as i64 + 32 * dk as i64;
        let d_pool_code = code(&t.pool) as i64 - code(&committed.pool) as i64;
        let d_pool = d_pool_code + 32 * dk as i64;
        let d_ticket = code(&t.ticket) as i64 - code(&committed.ticket) as i64;
        let proofs = (dk * (576 + 2 * M_DRAW as usize)) as i64;
        let draw_b = d_drawer + d_ctrl + d_marker + A_MAX as i64 * d_pool + d_pool_code + proofs;
        let claim_b = d_ticket + d_pool + proofs;
        sizes.push((k, code(&t.drawer), code(&t.ticket), d_drawer, d_ticket));
        est.push((k, draw_b, draw16.transient as i64 + 4 * draw_b, claim_b, claim16.transient as i64 + 4 * claim_b));
        if m.transient > TRANSIENT_LIMIT {
            println!("(transient above 100 % at K = {k}: stop)");
            break;
        }
    }
    println!(
        "Largest K (compute, transient, storage at R = 1.69 KAS) within 90 %: {k90:?}; within 100 %: {k100:?} (measured K = {K_FINAL}..; K <= 16 holds from M2-b)"
    );
    println!("\nM6-b: drawer and ticket code bytes (prefix + suffix) by K_MAX, and draw() / claim() transient ESTIMATES (4 grams per byte, constants.rs:31; compute not estimated)");
    println!("| K_MAX | drawer code B (delta) | ticket code B (delta) | draw: extra B | draw worst transient (estimate) | % | claim PAY F_MAX: extra B | claim transient (estimate) | % |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for ((k, dc, tc, dd, dt), (_, db, de, cb, ce)) in sizes.iter().zip(&est) {
        println!(
            "| {k} | {dc} ({dd:+}) | {tc} ({dt:+}) | {db} | {de} (estimate) | {:.1} | {cb} | {ce} (estimate) | {:.1} |",
            *de as f64 * 100.0 / TRANSIENT_LIMIT as f64,
            *ce as f64 * 100.0 / TRANSIENT_LIMIT as f64
        );
    }
    println!("Baselines (measured, K = 16): draw worst transient {}, claim PAY F_MAX transient {}", draw16.transient, claim16.transient);
}

// ---------------------------------------------------------------- M6-c: B_MAX frontier

#[test]
fn m6c_b_max_frontier() {
    println!("\nM6-c: B_MAX frontier at K = {K_FINAL} (shard, ticket and pool resized, Templates::get(b))");
    println!("| B_MAX | shard redeem B | ticket redeem B | close K = 16: compute | transient | storage | % c / t / s | buy b = B_MAX (referral, large change): compute | transient | storage | % c / t / s |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    let mut fails = vec![];
    for bm in [3, B_MAX_FINAL, 5] {
        let t = Templates::get(bm);
        let c = close_k_with(K_FINAL, &t);
        let f = common::accept(&c.build_with(t.clone()));
        let mc = masses(&f.tx, &f.entries);
        let b = buy_shape(bm, bm, 10, true, false);
        let fb = common::accept(&b.build_with(t.clone()));
        let mb = masses(&fb.tx, &fb.entries);
        let ((c1, c2, c3), (b1, b2, b3)) = (pct(&mc), pct(&mb));
        println!(
            "| {bm} | {} | {} | {} | {} | {} | {c1:.1} / {c2:.1} / {c3:.1} | {} | {} | {} | {b1:.1} / {b2:.1} / {b3:.1} |",
            t.shard_redeem(&BuyCase::new(bm).shard).len(),
            t.ticket.redeem(&b.ticket().values()).len(),
            mc.compute,
            mc.transient,
            mc.storage,
            mb.compute,
            mb.transient,
            mb.storage
        );
        if bm == B_MAX_FINAL {
            for (name, m) in [("close", mc), ("buy", mb)] {
                if !within(&m, MARGIN_PCT).iter().all(|&x| x) {
                    fails.push(format!("{name} at B_MAX = {bm}: {m:?}"));
                }
            }
        }
    }
    assert!(fails.is_empty(), "M6-c (D90 STOP): B_MAX = 4 not within 90 %: {fails:?}");
}

// ---------------------------------------------------------------- M6-d: m frontier

#[test]
fn m6d_m_frontier() {
    let committed = tpl();
    let t25 = variant("drawer K_MAX 16 M_DRAW 25", || drawer_sources(16, 25)).expect("M_DRAW = 25 drawer compiles");
    println!("\nM6-d: m frontier (K = {K_FINAL}); buy b = {B_MAX_FINAL} with referral; draw worst (a = {A_MAX}, np = 7, {F_MAX} funding inputs, cap binding, record); claim worst (h = K)");
    println!("| transition | m | compute | transient | storage | % c / t / s | <= 90 % |\n|---|---|---|---|---|---|---|");
    let mut fails = vec![];
    let mut line = |name: &str, m: i64, ms: &Masses, asserted: bool| {
        let ok = within(ms, MARGIN_PCT).iter().all(|&b| b);
        let (pc, pt, ps) = pct(ms);
        println!("| {name} | {m} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {ok} |", ms.compute, ms.transient, ms.storage);
        if asserted && !ok {
            fails.push(format!("{name} m = {m}: {ms:?}"));
        }
    };
    for m in [10, 25, 50] {
        let c = buy_shape(B_MAX_FINAL, B_MAX_FINAL, m as usize, true, false);
        let f = common::accept(&c.build());
        line("buy b = 4", m, &masses(&f.tx, &f.entries), m == M_FINAL);
    }
    for m in [10, 25, 50] {
        let (_, ms) = run_draw(&worst_draw_with(&committed, K_FINAL, m), committed.clone());
        line("draw, drawer M_DRAW = 50 (committed)", m, &ms, m == M_FINAL);
    }
    for m in [10, 25] {
        let (_, ms) = run_draw(&worst_draw_with(&t25, K_FINAL, m), t25.clone());
        line("draw, drawer M_DRAW = 25 (variant)", m, &ms, false);
    }
    for mode in [ClaimMode::Pay, ClaimMode::Accrue, ClaimMode::Final] {
        for m in [10, 25, 50] {
            let nf = if mode == ClaimMode::Pay { F_MAX } else { 1 };
            let f = accept_m4("M6-d claim", &worst_claim(K_FINAL, m, mode, nf).build());
            line(&format!("claim {mode:?}, {nf} funding inputs"), m, &masses(&f.tx, &f.entries), m == M_FINAL);
        }
    }
    println!(
        "drawer code B: M_DRAW = 25 {} / M_DRAW = 50 {}",
        code(&t25.drawer),
        code(&committed.drawer)
    );
    assert!(fails.is_empty(), "M6-d (D90 STOP): m = 50 not within 90 %: {fails:?}");
}

// ---------------------------------------------------------------- M6-e: d frontier

/// A buy of b combinations at deposit d on a fresh shard at R; with a referral, the price is
/// set so the referral output is M_REF (s_R = 10 %: price = ceil(1 KAS / b)), the smallest
/// referral output and so the largest storage term.
fn buy_at(b: usize, with_ref: bool, small_change: bool, d: u64) -> BuyCase {
    let mut c = buy_case(b_max(), b, 10);
    c.shard.d = d as i64;
    c.with_ref = with_ref;
    if with_ref {
        c.shard.price = KAS.div_ceil(b as u64) as i64;
        assert_eq!(c.referral(), M_REF, "referral at M_REF");
    }
    if small_change {
        c.funding = vec![c.gross() + c.b() * d + c.fee + KAS];
    }
    c
}

/// Worst storage of large and 1 KAS change (no script runs; equals the committed storage).
fn buy_storage_at(b: usize, with_ref: bool, d: u64) -> u64 {
    [false, true].iter().map(|&s| buy_at(b, with_ref, s, d).build().storage_mass()).max().unwrap()
}

#[test]
fn m6e_d_frontier() {
    println!("\nM6-e: smallest d per combination (sompi) with buy() storage <= 90 % / <= 100 %, fresh shard at R = {TEST_RESERVE}, worst of large and 1 KAS change; referral output at M_REF = {M_REF}");
    println!("| b | no referral: 90 % | 100 % | referral at M_REF: 90 % | 100 % |\n|---|---|---|---|---|");
    for b in 1..=b_max() {
        let f = |r: bool, pct: u64| search(1, 10 * KAS, |d| buy_storage_at(b, r, d) * 100 <= STORAGE_LIMIT * pct);
        let (a90, a100, r90, r100) = (f(false, MARGIN_PCT), f(false, 100), f(true, MARGIN_PCT), f(true, 100));
        println!("| {b} | {a90} ({:.4} KAS) | {a100} ({:.4}) | {r90} ({:.4}) | {r100} ({:.4}) |", kas(a90), kas(a100), kas(r90), kas(r100));
        if b == 1 {
            assert!(r90 <= D_FLOOR as u64, "M6-e: the b = 1 referral floor {r90} is above D_FLOOR");
        }
    }
    println!("\nM6-e: b = 1 with a referral at M_REF by d (the D80 shape; both changes, scripts run)");
    println!("| d (KAS) | change | compute | transient | storage | % of limit |\n|---|---|---|---|---|---|");
    let mut at_floor = vec![];
    for d in [11_000_000u64, 12_000_000, D_FLOOR as u64] {
        for small in [false, true] {
            let f = common::accept(&buy_at(1, true, small, d).build());
            let m = masses(&f.tx, &f.entries);
            println!("| {:.2} | {} | {} | {} | {} | {:.1} |", kas(d), if small { "1 KAS" } else { "large" }, m.compute, m.transient, m.storage, m.storage as f64 * 100.0 / STORAGE_LIMIT as f64);
            if d == D_FLOOR as u64 {
                at_floor.push(m);
            }
        }
    }
    for m in at_floor {
        assert!(within(&m, MARGIN_PCT).iter().all(|&x| x), "M6-e (D90 STOP): b = 1 referral buy at d = D_FLOOR above 90 %: {m:?}");
    }
}

// ---------------------------------------------------------------- M6-f: P_MIN

#[test]
fn m6f_p_min() {
    let budget = STORAGE_LIMIT * MARGIN_PCT / 100;
    let ps = [3_000_000u64, 3_670_000, 4_000_000, 5_000_000, 10_000_000];
    println!("\nM6-f: worst PAY claim storage (b = 2, d = D_FLOOR = {D_FLOOR}, ticket' {} sompi, pool' at R, one funding input) by payout P and change", 2 * D_FLOOR);
    println!("| change | P = 0.03 KAS | 0.0367 | 0.04 | 0.05 (P_MIN) | 0.1 | smallest P <= 90 % (sompi) | rounded up to 0.01 KAS |");
    println!("|---|---|---|---|---|---|---|---|");
    let mut worst = 0u64;
    for (name, change) in [("1 KAS", KAS), ("1,000 KAS", 1_000 * KAS), ("1,000,000 KAS", 1_000_000 * KAS)] {
        let cols: Vec<String> = ps.iter().map(|&p| pay_storage(p, change).to_string()).collect();
        let lo = search(1, 100 * KAS, |p| pay_storage(p, change) <= budget);
        let rounded = lo.div_ceil(KAS / 100) * (KAS / 100);
        worst = worst.max(rounded);
        println!("| {name} | {} | {} | {} | {} | {} | {lo} ({:.6} KAS) | {:.2} KAS |", cols[0], cols[1], cols[2], cols[3], cols[4], kas(lo), kas(rounded));
    }
    let at = [KAS, 1_000 * KAS, 1_000_000 * KAS].iter().map(|&ch| pay_storage(P_MIN as u64, ch)).max().unwrap();
    println!(
        "P_MIN = {P_MIN} sompi: worst PAY storage {at} ({:.1} %); smallest P within 90 % rounded up to 0.01 KAS: {:.2} KAS",
        at as f64 * 100.0 / STORAGE_LIMIT as f64,
        kas(worst)
    );
    assert!(at <= budget, "M6-f (D90 STOP): the PAY claim at P_MIN exceeds 90 % of storage: {at}");
}

// ---------------------------------------------------------------- M6-g: R re-check

#[test]
fn m6g_r_recheck() {
    let r = TEST_RESERVE;
    let (cst, (s, o, jb, a, cl)) = worst_storage(K_FINAL, r as i64, &CALLERS);
    let csup = close_storage_sup(K_FINAL, r as i64);
    println!("\nM6-g: R re-check at R = {r} sompi, K = {K_FINAL}, d = {TEST_DEPOSIT}");
    println!("| transition | measured worst | % | supremum | % |\n|---|---|---|---|---|");
    println!(
        "| close (M2-c grid; worst at sales {:.0}, O {:.0}, J_boost {:.0}, A {:.0}, caller {:.0} KAS) | {cst} | {:.1} | {csup} = (K + 3) 4C/R | {:.1} |",
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
                let st = storage_case_rec(a, caller, f_min).build().storage_mass();
                let sup = storage_sup_rec(a, r, f_min as u64, MIN_CALLER_OUTPUT);
                assert!(st <= sup);
                worst = worst.max(st);
            }
        }
        let sup = storage_sup_rec(A_MAX, r, f_min as u64, MIN_CALLER_OUTPUT);
        println!(
            "| draw with the record (M5-c method), f_min = {f_min} | {worst} | {:.1} | {sup} = 11 (4C/R) + (1 + a) C/R + C/f_min + C/caller | {:.1} |",
            worst as f64 * 100.0 / STORAGE_LIMIT as f64,
            sup as f64 * 100.0 / STORAGE_LIMIT as f64
        );
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
    assert!(cst * 100 <= STORAGE_LIMIT * MARGIN_PCT && csup * 100 <= STORAGE_LIMIT * MARGIN_PCT, "M6-g (D90 STOP): close storage above 90 %");
    for (name, sup) in &sups {
        assert!(sup * 100 <= STORAGE_LIMIT * MARGIN_PCT, "M6-g (D90 STOP): {name} storage supremum {sup} above 90 %");
    }
    assert!(r >= r_close && r >= r_draw);
}

// ---------------------------------------------------------------- M6-h: fees

#[test]
fn m6h_fees() {
    let t = tpl();
    println!("\nM6-h: claim fees against prizes (relay floor = 100 sompi per gram of max(compute, transient / 2); priority = 100 sompi per gram of the priority mass)");
    println!("| claim shape | priority mass | relay floor (KAS) | priority fee (KAS) | priority fee % of P_MIN | % of 0.1 KAS | % of 1 KAS | relay floor % of P_MIN |");
    println!("|---|---|---|---|---|---|---|---|");
    let mut shapes: Vec<(String, Built)> = vec![];
    for mode in [ClaimMode::Pay, ClaimMode::Accrue, ClaimMode::Final] {
        shapes.push((format!("{mode:?}, K = 16, m = 50, h = K"), worst_claim(K_FINAL, M_CLAIM, mode, 1).build()));
    }
    shapes.push(("Pay at P = P_MIN, storage worst case (b = 2, pool' at R, change 1,000,000 KAS)".into(), pay_case(P_MIN as u64, 1_000_000 * KAS).build()));
    shapes.push(("Pay at P = P_MIN, change 1 KAS".into(), pay_case(P_MIN as u64, KAS).build()));
    for (name, b) in shapes {
        let f = accept_m4(&name, &b);
        let m = masses(&f.tx, &f.entries);
        let (rf, pf) = (relay_floor_fee(&m), priority_fee(&m));
        println!(
            "| {name} | {} | {:.5} | {:.5} | {:.1} | {:.1} | {:.2} | {:.1} |",
            m.priority,
            kas(rf),
            kas(pf),
            pf as f64 * 100.0 / P_MIN as f64,
            pf as f64 * 100.0 / (KAS / 10) as f64,
            pf as f64 * 100.0 / KAS as f64,
            rf as f64 * 100.0 / P_MIN as f64
        );
    }
    println!("\nM6-h: round sales S at which C_close = s_C x S and C_draw = s_D x S cover the transaction's fee");
    println!("| transaction | s (bp) | priority mass | priority fee (KAS) | S covering it (KAS) | relay floor (KAS) | S covering the relay floor (KAS) |\n|---|---|---|---|---|---|---|");
    let close = close_k_with(K_FINAL, &t);
    let s_c = close.controller.s_c;
    let (st, (s, o, jb, a, cl)) = worst_storage(K_FINAL, TEST_RESERVE as i64, &CALLERS);
    let worst_close = close_case_c(K_FINAL, TEST_RESERVE as i64, s, o, jb, a, cl).unwrap();
    let mut rows: Vec<(String, i64, Masses)> = vec![];
    let f = common::accept(&close.build());
    rows.push((format!("close K = {K_FINAL} (M2-a shape)"), s_c, masses(&f.tx, &f.entries)));
    let f = common::accept(&worst_close.build());
    let m = masses(&f.tx, &f.entries);
    assert_eq!(m.storage, st);
    rows.push((format!("close K = {K_FINAL}, storage worst case"), s_c, m));
    let wd = worst_draw_with(&t, K_FINAL, M_DRAW);
    let s_d = wd.marker.s_d;
    rows.push(("draw worst (M6-a)".into(), s_d, run_draw(&wd, t.clone()).1));
    rows.push(("draw typical (M6-a)".into(), s_d, run_draw(&typical_draw(), t.clone()).1));
    for (name, s_bp, m) in rows {
        let (pf, rf) = (priority_fee(&m), relay_floor_fee(&m));
        println!(
            "| {name} | {s_bp} | {} | {:.5} | {:.2} | {:.5} | {:.2} |",
            m.priority,
            kas(pf),
            kas(sales_covering(pf, s_bp)),
            kas(rf),
            kas(sales_covering(rf, s_bp))
        );
    }
    let sup = storage_sup_rec(A_MAX, TEST_RESERVE, F_MIN_FLOOR as u64, MIN_CALLER_OUTPUT);
    println!(
        "draw storage supremum with the record at F_MIN_FLOOR (M6-g) {sup}: priority fee at most {:.5} KAS when storage dominates; C_draw covers it from S = {:.2} KAS",
        kas(sup * 100),
        kas(sales_covering(sup * 100, s_d))
    );
}

// ---------------------------------------------------------------- M6-j: drawer sensitivity

#[test]
fn m6j_drawer_sensitivity() {
    let committed = tpl();
    let t25 = variant("drawer K_MAX 16 M_DRAW 25", || drawer_sources(16, 25)).expect("M_DRAW = 25 drawer compiles");
    let t17 = variant("drawer K_MAX 17 M_DRAW 50", || drawer_sources(17, 50)).expect("drawer K_MAX = 17 compiles");
    let redeem = |t: &Templates| t.drawer_redeem(&DrawCase::new(4).drawer).len() as i64;
    println!("\nM6-j: drawer sensitivity of the worst draw() (K = {K_FINAL}, a = {A_MAX}, np = 7, {F_MAX} funding inputs, cap binding, record); drawer-only variants (marker, vault, pool commit to the variant's hash)");
    println!("| pair | m | drawer redeem B (a / b) | delta B | compute (a / b) | transient (a / b) | delta compute per B | delta transient per B |\n|---|---|---|---|---|---|---|---|");
    let mut slopes = vec![];
    for (name, ta, tb, m) in [("M_DRAW 25 vs 50", &t25, &committed, 25i64), ("K_MAX 16 vs 17", &committed, &t17, M_DRAW)] {
        let (_, ma) = run_draw(&worst_draw_with(ta, K_FINAL, m), ta.clone());
        let (_, mb) = run_draw(&worst_draw_with(tb, K_FINAL, m), tb.clone());
        let db = redeem(tb) - redeem(ta);
        let sc = (mb.compute as f64 - ma.compute as f64) / db as f64;
        let stt = (mb.transient as f64 - ma.transient as f64) / db as f64;
        println!(
            "| {name} | {m} | {} / {} | {db} | {} / {} | {} / {} | {sc:.3} | {stt:.3} |",
            redeem(ta),
            redeem(tb),
            ma.compute,
            mb.compute,
            ma.transient,
            mb.transient
        );
        slopes.push((sc, stt));
    }
    let (_, base) = run_draw(&worst_draw_with(&committed, K_FINAL, M_DRAW), committed.clone());
    let sc = slopes.iter().map(|s| s.0).fold(0.0, f64::max);
    let stt = slopes.iter().map(|s| s.1).fold(0.0, f64::max);
    let room_c = (COMPUTE_LIMIT * MARGIN_PCT / 100) as f64 - base.compute as f64;
    let room_t = (TRANSIENT_LIMIT * MARGIN_PCT / 100) as f64 - base.transient as f64;
    let (bc, bt) = (room_c / sc, room_t / stt);
    println!(
        "Worst draw at the committed drawer ({} B): compute {} ({:.1} %), transient {} ({:.1} %). Using the larger slope of each: {sc:.3} compute and {stt:.3} transient grams per drawer byte. Drawer byte budget to 90 %: compute {bc:.0} B, transient {bt:.0} B; binding: {} ({:.0} B)",
        redeem(&committed),
        base.compute,
        base.compute as f64 * 100.0 / COMPUTE_LIMIT as f64,
        base.transient,
        base.transient as f64 * 100.0 / TRANSIENT_LIMIT as f64,
        if bc <= bt { "compute" } else { "transient" },
        bc.min(bt)
    );
    println!("Stack-slot budget (not a mass): the M5 drawer has 4 free slots of SilverScript's 244-item stack limit at check 24 (DECISIONS 2026-10-12 Fact S4-1; M6 changed only the MIN_DEPOSIT constant, so the drawer compiles unchanged)");
}
