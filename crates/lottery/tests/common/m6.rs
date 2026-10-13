//! M6 Stage 1: the M6 attacks LA1-LA5 (spec M6 A8; LA7 added by the Stage 5 fix RV-1) as
//! data (`Atk5`, the M5 method), and the fabricated bases they and `m6_limits.rs` share, so
//! `m6_mutants.rs` can re-run LA2 and LA7 under the mutants without a W4b `require`.
//!
//! Check labels as M5 (`<file>:<label>`, `common::m5` module doc); the new vault check is
//! `vault:W4b` (spec M6 A4, D86). A strike: the bad transaction is rejected by `input` with
//! a script error, the good one passes `input` and is then accepted by every input.
//!
//! Values above MAX_SOMPI (LA3: total_assets = ASSETS_MAX) run in the engine only, as M5
//! S4-3 (VA6): the engine does not bound UTXO values, consensus does.

use harness::TxScriptError;
use lottery::buy::{KAS, OwnerKey};
use lottery::draw::DrawCase;
use lottery::game::{ASSETS_MAX, MIN_DEPOSIT, SHARES_MAX, VA, VS, W_MIN_VALUE, w_min};
use lottery::vault::*;

use super::m2::{is_engine_arithmetic, tpl};
use super::m5::*;

// ---------------------------------------------------------------- bases

/// A deposit request of exactly D = MIN_DEPOSIT (10 KAS, D85) into the fixture vault.
pub fn dep_min() -> DepositCase {
    let mut c = dep();
    c.d = MIN_DEPOSIT;
    c
}

/// The fixture vault's wmin (V = S = 10^15: q = 10^6, wmin = 10^9).
pub fn wmin_of(c: &WithdrawCase) -> i64 {
    w_min(c.vault.total_assets, c.vault.total_shares)
}

/// A partial withdrawal of exactly w = wmin from the fixture position (5 x 10^10 shares).
pub fn wd_at_wmin(key: OwnerKey) -> WithdrawCase {
    let mut c = wd(key);
    c.w = wmin_of(&c);
    assert!(c.w >= 2 && c.w < c.position.shares, "wd_at_wmin: a partial withdrawal at wmin = {}", c.w);
    c
}

/// A whole position smaller than wmin: shares = wmin - 1, w = shares (accepted only by the
/// whole-position clause of W4b).
pub fn wd_whole_small(key: OwnerKey) -> WithdrawCase {
    let mut c = wd(key);
    let wm = wmin_of(&c);
    c.position.shares = wm - 1;
    c.w = c.position.shares;
    c
}

/// A fabricated vault at q = 1 exactly: total_assets + VA = W_MIN_VALUE, so wmin = S + VS
/// (above total_shares: no partial withdrawal fits W4). total_shares = the position's
/// 5 x 10^10 shares, nothing pending, liability 0. The base is the whole position.
pub fn wd_q1_whole(key: OwnerKey) -> WithdrawCase {
    let mut c = wd(key);
    c.vault.total_assets = W_MIN_VALUE - VA;
    c.vault.total_shares = c.position.shares;
    c.vault.liability = 0;
    assert_eq!((c.vault.total_assets + VA) / W_MIN_VALUE, 1, "q = 1");
    assert_eq!(wmin_of(&c), c.vault.total_shares + VS);
    c.w = c.position.shares;
    c
}

/// A fabricated vault at q = 0: total_assets + VA = W_MIN_VALUE - 1 (wmin = 1); the
/// position and total_shares as `wd_q1_whole`; a partial withdrawal of w = 1.
pub fn wd_q0_one(key: OwnerKey) -> WithdrawCase {
    let mut c = wd_q1_whole(key);
    c.vault.total_assets = W_MIN_VALUE - VA - 1;
    assert_eq!(wmin_of(&c), 1, "q = 0: wmin = 1");
    c.w = 1;
    c
}

/// LA3 base (R8 range edge, engine only): total_assets = ASSETS_MAX and total_shares =
/// SHARES_MAX; a partial withdrawal at w = wmin (q = floor(2^62 / 10^9), wmin about 10^9).
pub fn wd_extreme_at_wmin(key: OwnerKey) -> WithdrawCase {
    let mut c = wd(key);
    c.vault.total_assets = ASSETS_MAX;
    c.vault.total_shares = SHARES_MAX;
    c.w = wmin_of(&c);
    assert!(c.w >= 2 && c.w < c.position.shares, "extreme base: wmin = {}", c.w);
    c
}

/// LA5 base: refund mode at the C1 guard edge of the new constant. A fabricated vault with
/// S = 199 (N = 1,199) and V' + VA = 10^12, D = MIN_DEPOSIT + 1: mint = floor(D N / M) = 1 <
/// ceil(D / MIN_DEPOSIT) = 2, so the deposits refund (at MIN_DEPOSIT = 1 KAS the bound
/// was 11, also a refund).
pub fn d_guard_edge() -> DrawCase {
    let c = d_at_price(1_000_000_000_000, 199, MIN_DEPOSIT + 1);
    let cv = c.expected().conv;
    assert!(cv.refund && cv.mint == 1, "guard edge: {cv:?}");
    c
}

/// A fabricated draw (round 1, no sales held, liability 0) whose vault has V' + VA = `m`,
/// total_shares `s` and `d` pending deposits (as `d_refund_price`). V' = V + H here, so V
/// is set to m - VA - (V' - V).
pub fn d_at_price(m: i64, s: i64, d: i64) -> DrawCase {
    let mut c = DrawCase::new(4);
    c.vault.liability = 0;
    c.vault.total_shares = s;
    c.vault.pend_dep = d;
    c.vault.total_assets = m - VA;
    c.controller.v_rec = c.vault.total_assets;
    let off = c.expected().v_prime - c.vault.total_assets;
    c.vault.total_assets -= off;
    c.controller.v_rec = c.vault.total_assets;
    assert_eq!(c.expected().v_prime + VA, m, "d_at_price: V' + VA");
    c
}

/// The settlement bases of LA4: a converting and a refunding deposit settlement (the M5
/// fixture records, 10 KAS of 30 KAS).
pub fn la4_bases() -> Vec<(&'static str, SettleCase)> {
    vec![("deposit-convert", st_dep()), ("deposit-refund", st_refund())]
}

// ---------------------------------------------------------------- LA1-LA5

/// LA1: a deposit below the new minimum (D85), at the D = MIN_DEPOSIT base.
pub fn la1() -> Vec<Atk5> {
    vec![
        Atk5::new("LA1 request_deposit D = MIN_DEPOSIT - 1", VLT0, "vault:D5", dep_min()).dep(|c| c.d = MIN_DEPOSIT - 1),
        Atk5::new("LA1 request_deposit D = 1 KAS (the M5 minimum)", VLT0, "vault:D5", dep_min()).dep(|c| c.d = KAS as i64),
    ]
}

/// LA2: partial withdrawals below wmin (W4b, D86).
pub fn la2() -> Vec<Atk5> {
    vec![
        // q >= 1 (the fixture vault, wmin = 10^9).
        Atk5::new("LA2 request_withdraw w = wmin - 1 (q >= 1)", VLT0, "vault:W4b", wd_at_wmin(owner())).wd(|c| c.w -= 1),
        // shares - 1 < wmin: the whole position (shares = wmin - 1) is the base; one share
        // less is a partial withdrawal below wmin.
        Atk5::new("LA2 request_withdraw w = shares - 1 < wmin (whole position below wmin)", VLT0, "vault:W4b", wd_whole_small(owner())).wd(|c| c.w = c.position.shares - 1),
        // q = 1: wmin = S + VS > total_shares, so no partial withdrawal passes. The spec's
        // literal w = S + VS - 1 exceeds the position (shares = S) and is W4's (below); the
        // largest partial W4 accepts, w = shares - 1, is W4b's (Stage 1 decision S1-3).
        Atk5::new("LA2 request_withdraw at q = 1: w = shares - 1 = S - 1 < S + VS", VLT0, "vault:W4b", wd_q1_whole(owner())).wd(|c| c.w = c.position.shares - 1),
        Atk5::new("LA2 request_withdraw at q = 1: w = S + VS - 1 > shares (W4 first)", VLT0, "vault:W4", wd_q1_whole(owner())).wd(|c| c.w = c.vault.total_shares + VS - 1),
    ]
}

/// LA3: the W4b arithmetic at the R8 range edges (fabricated vault total_assets =
/// ASSETS_MAX, total_shares = SHARES_MAX, engine only): w = wmin - 1 is rejected by W4b,
/// w = wmin (the base) accepted; neither by an engine arithmetic error (`la3_no_arith`).
pub fn la3() -> Vec<Atk5> {
    vec![Atk5::new("LA3 request_withdraw w = wmin - 1 at total_assets = ASSETS_MAX, total_shares = SHARES_MAX", VLT0, "vault:W4b", wd_extreme_at_wmin(owner())).wd(|c| c.w -= 1)]
}

/// LA3 extra: run input 0 of every LA3 transaction (good and bad) and require that no
/// rejection is an engine arithmetic error (overflow, division by zero), and that the
/// Rust mirror decides the base and the attack as A4 (w = wmin accepted, wmin - 1 not).
pub fn la3_no_arith() {
    for a in la3() {
        let (g, b, bc) = a.pair_case(&tpl());
        let Case5::Withdraw(bw) = &bc else { unreachable!() };
        let wm = w_min(bw.vault.total_assets, bw.vault.total_shares);
        assert!(bw.w < wm && bw.w != bw.position.shares, "{}: the Rust mirror rejects the attack", a.id);
        assert!(bw.w + 1 >= wm, "{}: the attack sits at wmin - 1", a.id);
        for (name, built) in [("good", &g), ("bad", &b)] {
            let f = built.finish();
            if let Err(e) = f.run_input(VLT0) {
                assert!(!is_engine_arithmetic(&e), "{} ({name}): engine arithmetic error {e:?}", a.id);
            }
        }
    }
}

/// LA4: settling a fabricated deposit request below the new minimum (request S1 range).
pub fn la4() -> Vec<Atk5> {
    let mut v = vec![];
    for (name, base) in la4_bases() {
        v.push(Atk5::new(format!("LA4 settle ({name}): deposit request amount = MIN_DEPOSIT - 1 (fabricated)"), REQ, "request:S1", base.clone()).st(|c| c.request.amount = MIN_DEPOSIT - 1));
        v.push(Atk5::new(format!("LA4 settle ({name}): deposit request amount = 1 KAS, the M5 minimum (fabricated)"), REQ, "request:S1", base).st(|c| c.request.amount = KAS as i64));
    }
    v
}

/// LA5: a draw claiming conversion (refund = 0) when mint < ceil(D / MIN_DEPOSIT) at the new
/// constant (C1 has no `require`; the first failing check is 20, M5 S2-7).
pub fn la5() -> Vec<Atk5> {
    vec![
        Atk5::new("LA5 draw claiming conversion when mint = 1 < ceil(D / MIN_DEPOSIT) = 2", DRW5, "drawer:20", d_guard_edge()).dr(|c| c.lie.refund = Some(false)),
        Atk5::new("LA5 draw claiming conversion at the price refund base (mint 1 < 3)", DRW5, "drawer:20", d_refund_price()).dr(|c| c.lie.refund = Some(false)),
    ]
}

/// A partial withdrawal from a fabricated vault where wmin = 0 (Stage 1 S1-1: q > S + VS):
/// the fixture's total_assets 10^15 (q = 10^6) with total_shares 500,000, a position of
/// 100,000 shares, w = 10,000. Only W4's `w >= 1` keeps w = 0 out here.
pub fn wd_wmin0(key: OwnerKey) -> WithdrawCase {
    let mut c = wd(key);
    c.vault.total_shares = 500_000;
    c.position.shares = 100_000;
    c.w = 10_000;
    assert_eq!(wmin_of(&c), 0, "wd_wmin0: wmin = 0");
    c
}

/// M6 Stage 2 (S2-1): two M5 attacks rebased so that the M5 line they kill is still the only
/// guard once W4b exists. At the fixture base W4b also rejects them (w = 0 < wmin = 10^9;
/// S = SHARES_MAX + 1 inflates wmin above w = 10^10), so the M5 mutants `M5W4_min` and
/// `M5W1_shares#1` were no longer killed there. Here W4b passes: wmin = 0, and the whole
/// position. Used by m5_mutants.rs (killed) and m6_attacks.rs (strike).
pub fn m5_rebased() -> Vec<Atk5> {
    vec![
        Atk5::new("VA6 request_withdraw w = 0 at wmin = 0 (M6 rebase)", VLT0, "vault:W4", wd_wmin0(owner())).wd(|c| c.w = 0),
        Atk5::new("VA6 request_withdraw whole position: total_shares = SHARES_MAX + 1 (fabricated vault, M6 rebase)", VLT0, "vault:W1", wd_whole(owner())).wd(|c| c.vault.total_shares = SHARES_MAX + 1),
    ]
}

// ---------------------------------------------------------------- LA7 (RV-1, O-M6-2)

/// LA7a base: a whole position of wmin + 1 shares at the fixture vault (wmin = 10^9),
/// withdrawn whole (accepted by W4b's whole-position clause).
pub fn wd_whole_wmin_plus_1(key: OwnerKey) -> WithdrawCase {
    let mut c = wd(key);
    c.position.shares = wmin_of(&c) + 1;
    c.w = c.position.shares;
    c
}

/// LA7b base / LP3 positive: a partial w = wmin from a position of 2 x wmin shares, so the
/// remainder is exactly wmin (the smallest remainder W4b accepts).
pub fn wd_rest_at_wmin(key: OwnerKey) -> WithdrawCase {
    let mut c = wd(key);
    let wm = wmin_of(&c);
    c.position.shares = 2 * wm;
    c.w = wm;
    c
}

/// LA7c base / LP3 positive: the fixture position (5 x 10^10 shares) with the largest
/// partial withdrawal W4b accepts, w = shares - wmin (remainder exactly wmin).
pub fn wd_rest_at_wmin_large(key: OwnerKey) -> WithdrawCase {
    let mut c = wd(key);
    c.w = c.position.shares - wmin_of(&c);
    assert!(c.w > wmin_of(&c), "large partial: w = {} > wmin", c.w);
    c
}

/// LA7 (Stage 5 RV-1, O-M6-2): a partial withdrawal whose remainder (position' shares) is
/// below wmin. Without the remainder rule, a prepared position of wmin + 1 shares splits
/// into w = wmin and a 1-share position', whose whole withdrawal later takes a request slot
/// for 1 share plus R (the U-6 griefing cost D87 / O-M6-1 assume away).
pub fn la7() -> Vec<Atk5> {
    vec![
        Atk5::new("LA7a request_withdraw w = wmin from a position of wmin + 1 (remainder 1 < wmin)", VLT0, "vault:W4b", wd_whole_wmin_plus_1(owner())).wd(|c| c.w = wmin_of(c)),
        Atk5::new("LA7b request_withdraw w = wmin + 1 from 2 x wmin (remainder wmin - 1, w >= wmin)", VLT0, "vault:W4b", wd_rest_at_wmin(owner())).wd(|c| c.w += 1),
        Atk5::new("LA7c request_withdraw w = shares - wmin + 1 (remainder wmin - 1, large w)", VLT0, "vault:W4b", wd_rest_at_wmin_large(owner())).wd(|c| c.w += 1),
    ]
}

pub fn all_atks6() -> Vec<Atk5> {
    [la1(), la2(), la3(), la4(), la5(), la7(), m5_rebased()].into_iter().flatten().collect()
}

pub fn atk6(id: &str) -> Atk5 {
    let mut found: Vec<Atk5> = all_atks6().into_iter().filter(|a| a.id == id).collect();
    assert_eq!(found.len(), 1, "atk6({id}): {} matches", found.len());
    found.pop().unwrap()
}

/// An engine verdict with arithmetic errors told apart (LP4, LA3).
pub fn verdict(r: &Result<u64, TxScriptError>) -> &'static str {
    match r {
        Ok(_) => "accepted",
        Err(e) if is_engine_arithmetic(e) => "ARITHMETIC",
        Err(_) => "rejected",
    }
}
