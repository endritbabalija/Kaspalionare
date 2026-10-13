//! Measurement helpers shared by the `m*_measure` test files (spec M6 A5: reuse, not
//! duplicate). Integration test files cannot import each other, so the builders used by
//! M6 were moved here unchanged from `m1_measure.rs` (M-c), `m2_measure.rs` (M2-c grid),
//! `m4_measure.rs` (M4-b, M4-c) and `m5_measure.rs` (DP21, pending, M5-c, M5-e); those
//! files now import them. The K_MAX and drawer variants of M6 (D91: measurement only,
//! compiled in memory, never committed) are here too.
//!
//! Limits per transaction, rusty-kaspa v2.1.0: compute 500,000, storage 500,000, transient
//! 1,000,000 (`consensus/core/src/config/params.rs:626`); KIP-9 C = 10^12
//! (`consensus/core/src/constants.rs:26`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use harness::Masses;
use lottery::buy::{BuyCase, KAS, Sources, Templates, b_max};
use lottery::claim::ClaimCase;
use lottery::close::{CloseCase, FundingIn};
use lottery::draw::DrawCase;
use lottery::game::{ClaimMode, P_MIN, SettleMode};
use lottery::params::TEST_RESERVE;
use lottery::smt::Tree;
use lottery::state::{KIND_DEPOSIT, KIND_WITHDRAW, RequestState, TIER_LOWER0};
use lottery::vault::{SettleCase, owner};

use super::m5::{record, st_accrue, st_dep, st_dep_last, st_final, st_pay, st_refund};

pub const COMPUTE_LIMIT: u64 = 500_000;
pub const TRANSIENT_LIMIT: u64 = 1_000_000;
pub const STORAGE_LIMIT: u64 = 500_000;
pub const MARGIN_PCT: u64 = 90;
/// KIP-9 storage-mass constant C (rusty-kaspa v2.1.0 `consensus/core/src/constants.rs:26`,
/// STORAGE_MASS_PARAMETER = SOMPI_PER_KASPA x 10,000).
pub const C: u64 = 1_000_000_000_000;
/// The F_MAX of the drawer and controller (A2).
pub const F_MAX: usize = 8;

pub fn within(m: &Masses, pct: u64) -> [bool; 3] {
    [m.compute * 100 <= COMPUTE_LIMIT * pct, m.transient * 100 <= TRANSIENT_LIMIT * pct, m.storage * 100 <= STORAGE_LIMIT * pct]
}

pub fn pct(m: &Masses) -> (f64, f64, f64) {
    (
        m.compute as f64 * 100.0 / COMPUTE_LIMIT as f64,
        m.transient as f64 * 100.0 / TRANSIENT_LIMIT as f64,
        m.storage as f64 * 100.0 / STORAGE_LIMIT as f64,
    )
}

// ------------------------------------------------------------------ M1 (m1_measure.rs M-c)

/// A buy of `b` distinct combinations at bonus range `m` under templates with `b_max` slots.
pub fn buy_case(b_max: usize, b: usize, m: usize) -> BuyCase {
    let mut c = BuyCase::new(b_max).with_tree(Tree::new(m));
    c.bundle = super::distinct(b, m.min(255) as u8);
    c
}

/// Storage mass of a buy with deposit `d` per combination; `small_change` uses a ~1 KAS change.
pub fn buy_storage(b: usize, with_ref: bool, small_change: bool, d: u64) -> u64 {
    let mut c = buy_case(b_max(), b, 10);
    c.shard.d = d as i64;
    c.with_ref = with_ref;
    if small_change {
        c.funding = vec![c.gross() + c.b() * d + c.fee + KAS];
    }
    let f = c.build().finish();
    harness::masses(&f.tx, &f.entries).storage
}

// ------------------------------------------------------------------ M2-c (m2_measure.rs)

/// A close at `k` with reserve `r`, sales `sales` per shard (no referral), boost pot `pot`,
/// J_boost `jb` (0 <= jb <= pot, A4 check 12), acc `acc` and a caller output of `caller`
/// sompi. `None` when that caller output is unreachable (C_close alone exceeds it).
pub fn close_case_c(k: usize, r: i64, sales: i64, pot: u64, jb: u64, acc: u64, caller: u64) -> Option<CloseCase> {
    let mut c = CloseCase::new(k);
    c.controller.reserve = r;
    c.controller.j_boost = jb as i64;
    c.controller.acc = acc as i64;
    c.pot = pot;
    for i in 0..k {
        c.set_sales(i, sales, 0);
    }
    let c_close = c.expected().math.amounts.c_close as u64;
    // caller = funding - R - fee + C_close  =>  funding = caller + R + fee - C_close.
    let need = caller + r as u64 + c.fee;
    if c_close >= need {
        return None;
    }
    c.funding = vec![FundingIn { value: need - c_close, daa: 900_000 }];
    Some(c)
}

/// Storage mass of a close; it depends only on values and output scripts (KIP-9), so no
/// script runs.
pub fn close_storage(k: usize, r: i64, sales: i64, pot: u64, jb: u64, acc: u64, caller: u64) -> Option<u64> {
    let c = close_case_c(k, r, sales, pot, jb, acc, caller)?;
    let b = c.build();
    assert_eq!(b.outputs[c.caller_output()].value, caller);
    Some(b.storage_mass())
}

/// The value grid: sales per shard, boost pot O, J_boost as a fraction of O (0, 1/2, 1),
/// acc A and the caller output (>= MIN_CALLER_OUTPUT, the builder policy).
pub const SALES: [i64; 3] = [0, 10 * KAS as i64, 10_000 * KAS as i64];
pub const POTS: [u64; 3] = [0, 100_000 * KAS, 10_000_000 * KAS];
pub const ACCS: [u64; 2] = [0, 2 * KAS];
pub const CALLERS: [u64; 5] = [KAS, 10 * KAS, 1_000 * KAS, 1_000_000 * KAS, 100_000_000 * KAS];

/// Worst storage over the grid at (k, r), with its shape.
pub fn worst_storage(k: usize, r: i64, callers: &[u64]) -> (u64, (i64, u64, u64, u64, u64)) {
    let mut worst = (0, (0, 0, 0, 0, 0));
    for &s in &SALES {
        for &o in &POTS {
            for jb in [0, o / 2, o] {
                for &a in &ACCS {
                    for &cl in callers {
                        let Some(st) = close_storage(k, r, s, o, jb, a, cl) else { continue };
                        if st > worst.0 {
                            worst = (st, (s, o, jb, a, cl));
                        }
                    }
                }
            }
        }
    }
    worst
}

/// Supremum of a close()'s storage mass at (k, r) over every A3 state and every caller
/// output >= MIN_CALLER_OUTPUT. Outputs: controller' O - J_boost + A + R, params R, k shards
/// R, marker S - P_R - C_close + J_boost + R (>= R), caller >= 1 KAS (plurality 1). At most
/// k + 3 covenant outputs (plurality 2, 4C/R each) can sit at R at once (O = J_boost = A = 0
/// and zero sales), and the input relief |I|^2 C / sum(inputs) is positive but tends to 0 as
/// the caller's funding grows. A large O takes one output off R (controller' or marker), and
/// that 4C/R always exceeds the caller term C/caller <= C/1 KAS; with O = 0 and a small
/// caller output the relief exceeds the caller term.
/// KIP-9 as implemented in rusty-kaspa v2.1.0 `consensus/core/src/mass/mod.rs:439-512`.
pub fn close_storage_sup(k: usize, r: i64) -> u64 {
    (k as u64 + 3) * (4 * C / r as u64)
}

/// Smallest R, in sompi, with close_storage_sup <= 90 % of the limit.
pub fn r_bound(k: usize) -> u64 {
    (4 * C * (k as u64 + 3)).div_ceil(STORAGE_LIMIT * MARGIN_PCT / 100)
}

// ------------------------------------------------------------------ M4 (m4_measure.rs)

/// The worst claim shape: K shards, bonus range m, every shard's leaf at the rank non-empty
/// (each holds the combination once, h = K), in `mode`, with `nf` funding inputs.
pub fn worst_claim(k: usize, m: i64, mode: ClaimMode, nf: usize) -> ClaimCase {
    let holders = vec![1u16; k];
    // PAY: prize = slice / K >= P_MIN; ACCRUE: prize = 1 sompi; FINAL: as PAY, index 1 claimed.
    let slice = match mode {
        ClaimMode::Accrue => k as i64,
        _ => 2 * P_MIN * k as i64,
    };
    let mut c = ClaimCase::fabricated(k, m, TIER_LOWER0, slice, &holders);
    if mode == ClaimMode::Final {
        c.make_final();
    }
    if nf > 1 {
        let daa = c.funding[0].daa;
        c.funding = (0..nf).map(|_| FundingIn { value: KAS, daa }).collect();
    }
    let x = c.expected();
    assert!(x.rejects.is_empty(), "{:?}", x.rejects);
    assert_eq!((x.mode, x.h), (mode, k as i64));
    assert_eq!(x.leaves.len() + x.siblings.len(), k * (576 + 2 * m as usize), "proof bytes k (576 + 2m)");
    c
}

/// The worst PAY-mode storage case (D57, D82): b = 2 ticket at d = D_FLOOR (ticket' = 0.26
/// KAS; 0.22 KAS at M4's d = 0.11 KAS), pool' at R (the prize empties the pool's remaining), one funding input, change of
/// `change` sompi, payout `p` sompi (h = 1, slice = p). Storage depends only on values and
/// scripts (no script run).
pub fn pay_storage(p: u64, change: u64) -> u64 {
    pay_case(p, change).build().storage_mass()
}

/// The `pay_storage` case itself (M6-h runs it at P = P_MIN, where the PAY shape is a valid
/// claim, to get every mass).
pub fn pay_case(p: u64, change: u64) -> ClaimCase {
    let mut c = ClaimCase::fabricated(4, 10, TIER_LOWER0, p as i64, &[1]);
    assert_eq!(c.ticket_value, 2 * lottery::genesis::D_FLOOR as u64);
    assert_eq!(c.ticket.b, 2);
    c.pool_value = c.pool.reserve as u64 + p;
    c.funding[0].value = c.fee + change;
    let x = c.expected();
    assert!(x.rejects.is_empty(), "{:?}", x.rejects);
    assert_eq!(x.values[0], TEST_RESERVE, "pool' at R");
    c.lie.mode = Some(ClaimMode::Pay); // the PAY shape at any p (the script requires p >= P_MIN)
    c
}

// ------------------------------------------------------------------ M5 (m5_measure.rs)

/// ST_ALL with every lower tier funded (np = 7), as m3/m4_measure.rs.
pub const ST_ALL_DRAW: [i64; 5] = [400, 300, 300, 200, 200];

/// The DP21 worst shape (m4_measure.rs `dp21`): round 2, K shards, marker m, every leaf at
/// the winning rank non-empty with every bonus held, sales 1,000,000 KAS, `a` absorbed
/// pools, `nf` funding inputs.
pub fn dp21(k: usize, m: i64, a: usize, nf: usize, np: usize) -> DrawCase {
    dp21_with(&Templates::get(b_max()), k, m, a, nf, np)
}

/// `dp21` against given templates (M6: drawer variants, compiled in memory, D91).
pub fn dp21_with(t: &Templates, k: usize, m: i64, a: usize, nf: usize, np: usize) -> DrawCase {
    let mut c = DrawCase::at_round_with(k, 2, t);
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
pub enum Pending {
    Nothing,
    /// 30 KAS deposited (converted) and 3,000,000 shares withdrawn within the slack (wc = W:
    /// two mulfrac calls).
    Both,
    /// 30 KAS deposited (converted) and 95 % of the shares withdrawn: the cap binds (wc < W:
    /// all three mulfrac calls).
    Cap,
    /// The refund branch: total_shares = 500,000 (price about 2 x 10^9 sompi per share), 30
    /// KAS deposited (mint floor(1.5) = 1 < ceil(D / MIN_DEPOSIT) = 3: C1 computes the mint
    /// and refunds) and 95 % of the shares withdrawn (cap binding: three mulfrac calls). M6
    /// (D85): total_shares was 1,000,000 (mint 3 < 30 at MIN_DEPOSIT = 1 KAS; at 10 KAS that
    /// mint 3 = ceil(D / MIN_DEPOSIT) would convert).
    RefundCap,
}

pub fn with_pending(mut c: DrawCase, p: Pending) -> DrawCase {
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
            c.vault.total_shares = 500_000;
            c.vault.pend_dep = 30 * KAS as i64;
            c.vault.pend_wsh = 475_000;
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

/// m3_measure.rs `storage_case_at` (every covenant output at R or 1 sompi above, T_F =
/// f_min, caller output `caller`) with W = 1 share pending at the smallest V': wc = 1, out_c =
/// floor((V' + 1) x 1 / (S + VS)) = 0, so the record is written at exactly R (vault' at R + V'): the draw with one more R output. Storage depends only on values and scripts.
pub fn storage_case_rec(a: usize, caller: u64, f_min: i64) -> DrawCase {
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
pub fn storage_sup_rec(a: usize, r: u64, f_min: u64, caller_min: u64) -> u64 {
    11 * (4 * C / r) + (1 + a as u64) * (C / r) + C / f_min + C / caller_min
}

pub fn st(rc: lottery::state::RecordState, kind: u8, amount: i64, mode: SettleMode, drained: bool) -> SettleCase {
    let c = SettleCase::new(rc, RequestState::new(2, kind, amount, &owner().payout()));
    let x = c.expected();
    assert!(x.reference.is_some(), "settle base rejected by game::settle");
    assert_eq!((x.mode, x.drained), (mode, drained));
    c
}

/// Every settle mode, drained and not.
pub fn settle_shapes() -> Vec<(&'static str, SettleCase)> {
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

// ------------------------------------------------------------------ M6 variants (D91)

/// The exact template-constant lines the variants substitute.
pub const K_LINE: &str = "    int constant K_MAX = 16;";
pub const M_DRAW_LINE: &str = "    int constant M_DRAW = 50;";

/// The committed sources at K_MAX = `k` (spec M6 A5 M6-b): the `int constant K_MAX = 16;`
/// line of controller, drawer and ticket, and every `byte[512]` roots array (32 x 16) of
/// controller, marker, drawer, pool and ticket, become K_MAX = k and `byte[32 k]`. Nothing
/// else in those sources depends on K_MAX (the controller's 512-byte zero buffer pads
/// 32 (K_MAX - k) bytes, which is 0 at k = K_MAX, the only k measured).
pub fn k_sources(k: usize) -> Sources {
    let mut s = Sources::committed(b_max());
    for (name, src) in [("controller", &mut s.controller), ("drawer", &mut s.drawer), ("ticket", &mut s.ticket)] {
        assert_eq!(src.matches(K_LINE).count(), 1, "{name}: exactly one K_MAX line");
        *src = src.replace(K_LINE, &format!("    int constant K_MAX = {k};"));
    }
    for (name, src) in [("controller", &mut s.controller), ("marker", &mut s.marker), ("drawer", &mut s.drawer), ("pool", &mut s.pool), ("ticket", &mut s.ticket)] {
        assert!(src.contains("byte[512]"), "{name}: no byte[512] roots array");
        *src = src.replace("byte[512]", &format!("byte[{}]", 32 * k));
    }
    s
}

/// The committed sources with the drawer's K_MAX and M_DRAW constants replaced (only the
/// drawer changes; the m3_measure.rs M3-c variant, here compiled with the whole chain so a
/// draw can run against it).
pub fn drawer_sources(k_max: usize, m_draw: i64) -> Sources {
    let mut s = Sources::committed(b_max());
    assert!(s.drawer.contains(K_LINE) && s.drawer.contains(M_DRAW_LINE));
    s.drawer = s.drawer.replace(K_LINE, &format!("    int constant K_MAX = {k_max};")).replace(M_DRAW_LINE, &format!("    int constant M_DRAW = {m_draw};"));
    s
}

/// Compile `src` once per `key` (compiling the chain takes seconds). `Err` carries the
/// compiler's message (for example SilverScript's 244-item stack limit) instead of
/// panicking, so a frontier can record where compilation stops.
pub fn variant(key: &str, src: impl FnOnce() -> Sources) -> Result<Arc<Templates>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Result<Arc<Templates>, String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(t) = cache.lock().unwrap().get(key) {
        return t.clone();
    }
    let src = src();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Arc::new(Templates::compile_sources(b_max(), &src)))).map_err(|e| {
        e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "compile panicked".into())
    });
    cache.lock().unwrap().entry(key.to_string()).or_insert(r).clone()
}
