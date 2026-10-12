//! M5 Rust units (spec M5 A10): VP12, Rust half (`mulfrac` against u128 at the boundaries
//! and on 100,000 random triples, the script's 63-step algorithm in checked i64, `convert`
//! and `settle` against brute-force rational references, the A6.2 drain invariants over
//! random request sets and orders), VP1 (genesis: S0, the deployer position, the vault's
//! template fields, the builder's refusals), and the M5 Stage 1 scaffolding (the D74 compile
//! order and the always-rejecting placeholder entries of request, record and position).
//!
//! The script half of VP12 (the script accepts the reference outputs over a grid) comes with
//! the drawer conversion and `settle()` (M5 Stages 4 and 5).

use std::collections::HashSet;

use harness::{ArtifactValue, compile_source, covenant_path};
use lottery::buy::{FUNDING_SK, KAS, Templates, b_max, funding_spk};
use lottery::game::{
    ASSETS_MAX, Conversion, MIN_DEPOSIT, P_MIN, SHARE_UNIT, SHARES_MAX, SettleMode, TOTAL_MAX, VA, VS, convert, mulfrac, mulfrac_steps, settle,
    wc_witness_ok, withdraw_out,
};
use lottery::genesis::{GENESIS_FUNDING, GenesisCase, covenant_id};
use lottery::ledger::Ledger;
use lottery::params::xonly;
use lottery::state::{KIND_DEPOSIT, KIND_WITHDRAW, Payout, PositionState, RecordState, RequestState, VaultState};

/// splitmix64 (as the other game tests).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        assert!(lo <= hi);
        lo + (self.next() % ((hi - lo) as u64 + 1)) as i64
    }
    /// A value in [lo, hi] of random magnitude: uniform in [lo, min(hi, lo + 2^k)] for a
    /// random bit length k, so small, medium and boundary values all occur.
    fn mag(&mut self, lo: i64, hi: i64) -> i64 {
        let k = self.next() % 64;
        let span = if k >= 63 { hi - lo } else { (hi - lo).min(1i64 << k) };
        self.range(lo, lo + span)
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

fn floor_u128(a: i64, b: i64, c: i64) -> i64 {
    ((a as u128 * b as u128) / c as u128) as i64
}

// ------------------------------------------------------------------------- mulfrac

/// VP12: mulfrac and the script's 63-step algorithm against u128 at the boundaries
/// a in {0, 1, 2^62}, r in {0, c - 1}, c in {1, 2^62} (and a few more c), and the domain:
/// r >= c, c < 1, c > 2^62, a < 0 and a > 2^62 are outside it (the script's preconditions).
#[test]
fn vp12_mulfrac_boundaries() {
    let cs = [1, 2, 3, 1_000, (1 << 31) + 7, TOTAL_MAX - 1, TOTAL_MAX];
    let mut n = 0;
    for &a in &[0, 1, 2, TOTAL_MAX - 1, TOTAL_MAX] {
        for &c in &cs {
            for r in [0, c / 2, c - 1] {
                let want = floor_u128(a, r, c);
                assert_eq!(mulfrac(a, r, c), Some(want), "mulfrac({a}, {r}, {c})");
                assert_eq!(mulfrac_steps(a, r, c), Some(want), "mulfrac_steps({a}, {r}, {c})");
                assert!(want <= a);
                n += 1;
            }
        }
    }
    assert_eq!(n, 5 * 7 * 3);
    // The extreme product: (2^62)(2^62 - 1) / 2^62 = 2^62 - 1.
    assert_eq!(mulfrac(TOTAL_MAX, TOTAL_MAX - 1, TOTAL_MAX), Some(TOTAL_MAX - 1));
    assert_eq!(mulfrac_steps(TOTAL_MAX, TOTAL_MAX - 1, TOTAL_MAX), Some(TOTAL_MAX - 1));
    for (a, r, c) in [(1, 1, 1), (1, 5, 5), (1, 6, 5), (1, 0, 0), (1, 0, -1), (1, 0, TOTAL_MAX + 1), (-1, 0, 1), (TOTAL_MAX + 1, 0, 1), (1, -1, 5)] {
        assert_eq!(mulfrac(a, r, c), None, "({a}, {r}, {c}) is outside the domain");
        assert_eq!(mulfrac_steps(a, r, c), None, "({a}, {r}, {c}) is outside the domain");
    }
}

/// VP12: 100,000 random triples of random magnitudes over the whole domain.
#[test]
fn vp12_mulfrac_random() {
    let mut g = Rng(0x5eed_0005);
    for i in 0..100_000 {
        let c = g.mag(1, TOTAL_MAX);
        let r = g.mag(0, c - 1);
        let a = g.mag(0, TOTAL_MAX);
        let want = floor_u128(a, r, c);
        assert_eq!(mulfrac(a, r, c), Some(want), "#{i}: mulfrac({a}, {r}, {c})");
        assert_eq!(mulfrac_steps(a, r, c), Some(want), "#{i}: mulfrac_steps({a}, {r}, {c})");
    }
}

// ------------------------------------------------------------------------- convert

/// Brute-force rational reference of C1-C3 (independent of `convert`): mint = floor(D N / M)
/// in u128; the deposits convert iff D > 0, mint <= SHARES_MAX - S and mint x MIN_DEPOSIT >= D
/// (mint >= ceil(D / MIN_DEPOSIT)); wc = the largest w <= W with floor(M w / N) <= slack,
/// by binary search on the monotone payout.
fn convert_ref(v: i64, s: i64, d: i64, w: i64, l: i64) -> (bool, i64, i64, i64) {
    let (n, m) = ((s + VS) as u128, (v + VA) as u128);
    let mint = d as u128 * n / m;
    let ok = d > 0 && mint <= (SHARES_MAX - s) as u128 && mint * MIN_DEPOSIT as u128 >= d as u128;
    let d_c = if ok { d } else { 0 };
    let slack = (v + d_c - l) as u128;
    let out = |x: u128| m * x / n;
    let (mut lo, mut hi) = (0u128, w as u128);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if out(mid) <= slack { lo = mid } else { hi = mid - 1 }
    }
    (ok, if ok { mint as i64 } else { 0 }, lo as i64, out(lo) as i64)
}

/// Checks every C3 relation and the derived invariants of one conversion.
fn check_conversion(x: &Conversion) {
    let (ok, mint_c, wc, out_c) = convert_ref(x.v_prime, x.s, x.d, x.w, x.l_next);
    let tag = format!("{x:?}");
    assert_eq!((x.ok, x.mint_c, x.wc, x.out_c), (ok, mint_c, wc, out_c), "{tag}");
    assert_eq!(x.d_c, if ok { x.d } else { 0 }, "{tag}");
    assert_eq!(x.refund, x.d > 0 && !ok, "{tag}");
    assert_eq!(x.slack, x.v_prime + x.d_c - x.l_next, "{tag}");
    // A zero-share mint is impossible (D70): a conversion mints at least ceil(D / MIN_DEPOSIT).
    if x.ok {
        assert!(x.mint_c >= 1 && x.mint_c >= (x.d + MIN_DEPOSIT - 1) / MIN_DEPOSIT, "{tag}");
    }
    // The C2 witness is unique: wc passes, wc - 1 and wc + 1 fail.
    assert!(wc_witness_ok(x.v_prime, x.s, x.w, x.slack, x.wc), "{tag}");
    if x.wc > 0 {
        assert!(!wc_witness_ok(x.v_prime, x.s, x.w, x.slack, x.wc - 1), "{tag}: wc - 1 also passes");
    }
    assert!(!wc_witness_ok(x.v_prime, x.s, x.w, x.slack, x.wc + 1), "{tag}: wc + 1 passes");
    assert_eq!(withdraw_out(x.v_prime, x.s, x.wc), Some(x.out_c));
    // D69: vault' total_assets >= L'; shares and pending in range.
    assert!(x.total_assets() >= x.l_next, "{tag}: below L'");
    assert!((0..=SHARES_MAX).contains(&x.total_shares()), "{tag}");
    assert!(x.total_assets() <= TOTAL_MAX, "{tag}");
    assert!((0..=x.total_shares()).contains(&(x.w - x.wc)), "{tag}: pend_wsh' > total_shares'");
    // Derived: rounding never dilutes the holders that stay, (A' + VA)/(S' + VS) >= (V' + VA)/(S + VS).
    let lhs = (x.total_assets() + VA) as u128 * (x.s + VS) as u128;
    let rhs = (x.v_prime + VA) as u128 * (x.total_shares() + VS) as u128;
    assert!(lhs >= rhs, "{tag}: the price per share fell");
    // Record (C3): exactly when something was pending; value out_c + (D - D_c) + R.
    let funder = Payout::Schnorr([9; 32]);
    let rec = x.record(7, [3; 32], &funder);
    assert_eq!(rec.is_some(), x.d > 0 || x.w > 0, "{tag}");
    if let Some(r) = rec {
        assert_eq!((r.refund, r.dep_rem, r.mint_rem, r.wsh_rem, r.burn_rem, r.out_rem), (x.refund as u8, x.d, x.mint_c, x.w, x.wc, x.out_c));
        assert!(lottery::game::record_in_range(&r), "{tag}: record out of the A3.3 ranges");
        assert_eq!(x.record_value(5), x.out_c + x.d - x.d_c + 5);
    }
    // Conservation (A5, vault side, R excluded): V' + D = vault' total_assets + record assets.
    assert_eq!(x.v_prime + x.d, x.total_assets() + x.out_c + (x.d - x.d_c), "{tag}");
}

/// VP12: `convert` against the brute-force reference over random states (every magnitude,
/// L' up to V'), and over an exhaustive small grid where wc is also found by a linear scan.
#[test]
fn vp12_convert_matches_reference() {
    let mut g = Rng(0x00c0_17e7);
    let mut seen = HashSet::new();
    for _ in 0..20_000 {
        let v = g.mag(0, ASSETS_MAX);
        let s = g.mag(0, SHARES_MAX);
        let d = if g.next() % 4 == 0 { 0 } else { g.mag(0, TOTAL_MAX - v) };
        let w = if g.next() % 4 == 0 { 0 } else { g.mag(0, s) };
        let l = g.mag(0, v);
        let x = convert(v, s, d, w, l).expect("in range");
        check_conversion(&x);
        seen.insert((x.ok, x.refund, x.wc == x.w, x.rec()));
    }
    // Exhaustive small grid; wc by linear scan.
    for v in [0, 1, 2, 5, 999, 1_000, 1_001] {
        for s in [0, 1, 3, 10, 1_000] {
            for d in [0, 1, 2, 7, MIN_DEPOSIT - 1, MIN_DEPOSIT, MIN_DEPOSIT + 1] {
                for w in 0..=s.min(12) {
                    for l in 0..=v.min(6) {
                        let x = convert(v, s, d, w, l).unwrap();
                        check_conversion(&x);
                        let lin = (0..=w).rev().find(|&c| floor_u128(v + VA, c, s + VS) <= x.slack).unwrap();
                        assert_eq!(x.wc, lin, "{x:?}");
                        seen.insert((x.ok, x.refund, x.wc == x.w, x.rec()));
                    }
                }
            }
        }
    }
    // Every branch combination was reached: converting and refunding deposits, nothing
    // pending, capped and uncapped withdrawals.
    for want in [(true, false, true, true), (false, true, true, true), (false, false, true, false), (true, false, false, true), (false, true, false, true), (false, false, false, true)] {
        assert!(seen.contains(&want), "branch {want:?} not reached");
    }
}

/// VP9 (Rust level), VP6 and VA6 boundaries: refund when mint < ceil(D / MIN_DEPOSIT) and when
/// mint > SHARES_MAX - S (both at the pre-guard and after it); the cap binds and same-draw
/// deposits raise wc (D69); out-of-range states are rejected.
#[test]
fn vp12_convert_boundaries() {
    // Price 10^9 sompi per share (S = 0, V' + 1 = 10^12): 1 KAS mints 0 < 1 share: refund.
    let x = convert(1_000_000_000_000 - 1, 0, MIN_DEPOSIT, 0, 0).unwrap();
    assert!(x.refund && !x.ok && x.mint == 0 && x.d_c == 0 && x.mint_c == 0);
    check_conversion(&x);
    // At price exactly MIN_DEPOSIT per share (V' + 1 = 10^8 (S + VS)), D mints D / 10^8 shares
    // = ceil(D / 10^8) for D a multiple of 10^8: converts. One sompi more costly: refund.
    let s = 4_000;
    let v = MIN_DEPOSIT * (s + VS) - VA;
    let x = convert(v, s, 3 * MIN_DEPOSIT, 0, 0).unwrap();
    assert!(x.ok && x.mint_c == 3, "{x:?}");
    let x = convert(v + 1, s, 3 * MIN_DEPOSIT, 0, 0).unwrap();
    assert!(x.refund && x.mint == 2, "{x:?}");
    // mint > SHARES_MAX - S: the pre-guard D <= (SHARES_MAX - S) / q at and one above.
    let (s, v) = (SHARES_MAX - 11_000, 0); // N = SHARES_MAX - 10,000, M = 1: q = N
    let q = s + VS;
    let room = SHARES_MAX - s;
    assert_eq!(room / q, 0);
    let x = convert(v, s, 1, 0, 0).unwrap();
    assert!(x.refund && x.mint == 0, "pre-guard fails at D = 1: {x:?}");
    let (s, v) = (0, 0); // N = 1,000, M = 1: q = 1,000
    let dmax = (SHARES_MAX - s) / (s + VS);
    let x = convert(v, s, dmax, 0, 0).unwrap();
    assert!(x.ok && x.mint_c == dmax * 1_000 && x.mint_c <= SHARES_MAX, "{x:?}");
    let x = convert(v, s, dmax + 1, 0, 0).unwrap();
    assert!(x.refund && x.mint == 0, "{x:?}");
    // The cap binds: wc < W with vault' >= L' and out(wc + 1) > slack; deposits raise wc.
    let (v, s, w) = (1_000_000 * KAS as i64, 1_000_000_000_000, 500_000_000_000);
    let l = v - 100 * KAS as i64;
    let x = convert(v, s, 0, w, l).unwrap();
    assert!(x.wc < w && x.total_assets() >= l, "{x:?}");
    assert!(withdraw_out(v, s, x.wc + 1).unwrap() > x.slack);
    let y = convert(v, s, 50 * KAS as i64, w, l).unwrap();
    assert!(y.ok && y.wc > x.wc, "D69: same-draw deposits raise wc: {x:?} {y:?}");
    check_conversion(&x);
    check_conversion(&y);
    // Out-of-range states are rejected (no lineage produces them, A8).
    for (v, s, d, w, l) in [
        (ASSETS_MAX + 1, 0, 0, 0, 0),
        (0, SHARES_MAX + 1, 0, 0, 0),
        (-1, 0, 0, 0, 0),
        (0, 5, 0, 6, 0),
        (0, 5, 0, -1, 0),
        (0, 0, -1, 0, 0),
        (10, 0, TOTAL_MAX - 9, 0, 0),
        (10, 0, 0, 0, 11),
        (10, 0, 0, 0, -1),
    ] {
        assert_eq!(convert(v, s, d, w, l), None, "({v}, {s}, {d}, {w}, {l})");
    }
    // Largest values: V' = ASSETS_MAX, S = SHARES_MAX, everything withdrawn, nothing locked.
    let x = convert(ASSETS_MAX, SHARES_MAX, 0, SHARES_MAX, 0).unwrap();
    assert_eq!(x.wc, SHARES_MAX);
    check_conversion(&x);
    // Nothing pending: no record, vault' = V', S unchanged.
    let x = convert(5 * KAS as i64, 7, 0, 0, KAS as i64).unwrap();
    assert!(!x.rec() && x.total_assets() == 5 * KAS as i64 && x.total_shares() == 7);
    let vt = VaultState {
        round: 3,
        total_assets: 1,
        total_shares: 7,
        pend_dep: 0,
        pend_wsh: 0,
        liability: 0,
        record_tpl: [1; 32],
        request_tpl: [2; 32],
        position_tpl: [3; 32],
    };
    let after = x.vault_after(&vt, 4);
    assert_eq!(after, VaultState { round: 4, total_assets: 5 * KAS as i64, liability: KAS as i64, ..vt });
}

// -------------------------------------------------------------------------- settle

fn owner() -> Payout {
    Payout::Schnorr([0x0b; 32])
}

fn record(round: i64, refund: u8, rem: [i64; 5]) -> RecordState {
    RecordState {
        round,
        request_tpl: [4; 32],
        refund,
        dep_rem: rem[0],
        mint_rem: rem[1],
        wsh_rem: rem[2],
        burn_rem: rem[3],
        out_rem: rem[4],
        funder_class: 0,
        funder_key: [0; 33],
    }
}

/// VP12: `settle` against u128 floors over random fabricated records and requests, and its
/// rejections (wrong draw's record, s above the remaining side, sh = 0, ranges).
#[test]
fn vp12_settle_matches_reference() {
    let mut g = Rng(0x5e771e);
    let mut modes = HashSet::new();
    for _ in 0..20_000 {
        let refund = (g.next() % 5 == 0) as u8;
        let dep = g.mag(0, TOTAL_MAX);
        let mint = if refund == 1 { 0 } else { g.mag(0, SHARES_MAX) };
        let wsh = g.mag(0, TOTAL_MAX);
        let burn = if g.next() % 8 == 0 { wsh } else { g.mag(0, wsh) };
        let out = g.mag(0, TOTAL_MAX);
        let rc = record(9, refund, [dep, mint, wsh, burn, out]);
        let kind = (g.next() % 2) as u8;
        let s = if kind == KIND_DEPOSIT {
            if dep < MIN_DEPOSIT {
                continue;
            }
            if g.next() % 8 == 0 { dep } else { g.mag(MIN_DEPOSIT, dep) }
        } else {
            if wsh < 1 {
                continue;
            }
            if g.next() % 8 == 0 { wsh.min(SHARES_MAX) } else { g.mag(1, wsh.min(SHARES_MAX)) }
        };
        let rq = RequestState::new(9, kind, s, &owner());
        let got = settle(&rc, &rq);
        if kind == KIND_DEPOSIT {
            let sh = if refund == 1 { 0 } else if s == dep { mint } else { floor_u128(mint, s, dep) };
            if refund == 0 && sh < 1 {
                assert_eq!(got, None, "sh = 0 must be rejected");
                continue;
            }
            let x = got.expect("deposit settles");
            assert_eq!(x.mode, if refund == 1 { SettleMode::DepositRefund } else { SettleMode::DepositConvert });
            assert_eq!((x.sh, x.record.dep_rem, x.record.mint_rem), (sh, dep - s, mint - sh));
            assert_eq!((x.record.wsh_rem, x.record.burn_rem, x.record.out_rem), (wsh, burn, out));
            modes.insert(x.mode);
        } else {
            let x = got.expect("withdrawal settles");
            let conv = if s == wsh { burn } else { floor_u128(burn, s, wsh) };
            let pay = if burn == 0 { 0 } else { floor_u128(out, conv, burn) };
            assert_eq!((x.conv, x.pay, x.left), (conv, pay, s - conv));
            let mode = if s == conv { SettleMode::Final } else if pay >= P_MIN { SettleMode::Pay } else { SettleMode::Accrue };
            assert_eq!(x.mode, mode);
            assert_eq!((x.record.wsh_rem, x.record.burn_rem, x.record.out_rem), (wsh - s, burn - conv, out - pay));
            assert_eq!((x.record.dep_rem, x.record.mint_rem), (dep, mint));
            assert!(x.record.burn_rem <= x.record.wsh_rem);
            modes.insert(x.mode);
        }
    }
    assert_eq!(modes.len(), 5, "every settlement mode reached: {modes:?}");
    // Rejections.
    let rc = record(9, 0, [3 * MIN_DEPOSIT, 3, 10, 4, 1_000]);
    let dep = |s| RequestState::new(9, KIND_DEPOSIT, s, &owner());
    assert!(settle(&rc, &dep(MIN_DEPOSIT)).is_some());
    for (name, rc2, rq) in [
        ("record t - 1", RecordState { round: 8, ..rc.clone() }, dep(MIN_DEPOSIT)),
        ("record t + 1", RecordState { round: 10, ..rc.clone() }, dep(MIN_DEPOSIT)),
        ("s > dep_rem", rc.clone(), dep(3 * MIN_DEPOSIT + 1)),
        ("s < MIN_DEPOSIT", rc.clone(), dep(MIN_DEPOSIT - 1)),
        ("sh = 0", RecordState { mint_rem: 2, ..rc.clone() }, dep(MIN_DEPOSIT)),
        ("w > wsh_rem", rc.clone(), RequestState::new(9, KIND_WITHDRAW, 11, &owner())),
        ("w = 0", rc.clone(), RequestState::new(9, KIND_WITHDRAW, 0, &owner())),
        ("kind 2", rc.clone(), RequestState::new(9, 2, MIN_DEPOSIT, &owner())),
        ("owner class 3", rc.clone(), RequestState { owner_class: 3, ..dep(MIN_DEPOSIT) }),
        ("owner padding", rc.clone(), RequestState { owner_key: [1; 33], ..dep(MIN_DEPOSIT) }),
        ("burn > wsh", RecordState { burn_rem: 11, ..rc.clone() }, dep(MIN_DEPOSIT)),
        ("refund with mint", RecordState { refund: 1, ..rc.clone() }, dep(MIN_DEPOSIT)),
        ("refund 2", RecordState { refund: 2, mint_rem: 0, ..rc.clone() }, dep(MIN_DEPOSIT)),
        ("mint > SHARES_MAX", RecordState { mint_rem: SHARES_MAX + 1, ..rc.clone() }, dep(MIN_DEPOSIT)),
        ("out > TOTAL_MAX", RecordState { out_rem: TOTAL_MAX + 1, ..rc.clone() }, dep(MIN_DEPOSIT)),
        ("record round 0", RecordState { round: 0, ..rc.clone() }, RequestState { round: 0, ..dep(MIN_DEPOSIT) }),
    ] {
        assert_eq!(settle(&rc2, &rq), None, "{name}");
    }
    // PAY and ACCRUE at the P_MIN boundary (VP7): pay = P_MIN pays, P_MIN - 1 accrues.
    for (out, mode) in [(2 * P_MIN, SettleMode::Pay), (2 * P_MIN - 2, SettleMode::Accrue)] {
        let rc = record(9, 0, [0, 0, 20, 10, out]);
        let x = settle(&rc, &RequestState::new(9, KIND_WITHDRAW, 10, &owner())).unwrap();
        assert_eq!((x.conv, x.left, x.mode), (5, 5, mode), "out {out}");
        assert_eq!(x.pay, out / 2);
        assert_eq!(x.payout(10, 3), if mode == SettleMode::Pay { Some(out / 2) } else { None });
    }
    // Payouts (A6.1 check 9): refund s + value, FINAL pay + value.
    let rc = record(9, 1, [MIN_DEPOSIT, 0, 0, 0, 0]);
    let x = settle(&rc, &dep(MIN_DEPOSIT)).unwrap();
    assert_eq!((x.mode, x.payout(MIN_DEPOSIT, 7), x.drained), (SettleMode::DepositRefund, Some(MIN_DEPOSIT + 7), true));
    let rc = record(9, 0, [0, 0, 4, 4, 1_000]);
    let x = settle(&rc, &RequestState::new(9, KIND_WITHDRAW, 4, &owner())).unwrap();
    assert_eq!((x.mode, x.pay, x.payout(4, 7), x.drained), (SettleMode::Final, 1_000, Some(1_007), true));
}

// ------------------------------------------------------------ A6.2 drain invariants

/// One random draw with deposits and withdrawals: the requests, the conversion, and a
/// random settlement order.
struct Scenario {
    deps: Vec<i64>,
    wds: Vec<i64>,
    x: Conversion,
}

fn scenario(g: &mut Rng) -> Scenario {
    let nd = g.range(0, 6) as usize;
    let nw = g.range(0, 6) as usize;
    let unit = [1i64, 1_000, KAS as i64, 1_000_000 * KAS as i64][g.range(0, 3) as usize];
    let deps: Vec<i64> = (0..nd).map(|_| MIN_DEPOSIT + g.mag(0, 1_000 * unit)).collect();
    let wds: Vec<i64> = (0..nw).map(|_| 1 + g.mag(0, 1_000 * unit)).collect();
    let (d, w): (i64, i64) = (deps.iter().sum(), wds.iter().sum());
    let s = w + g.mag(0, 10_000 * unit);
    // The price: random, sometimes so high that deposits refund.
    let v = match g.range(0, 3) {
        0 => g.mag(0, 1_000_000 * KAS as i64),
        1 => (s + VS).saturating_mul(g.range(1, 2_000)).min(ASSETS_MAX - d),
        2 => (s + VS).saturating_mul(MIN_DEPOSIT + g.range(0, 10)).min(ASSETS_MAX - d),
        _ => g.mag(0, ASSETS_MAX - d),
    };
    // L' from 0 to V' (the cap binds sometimes).
    let l = match g.range(0, 2) {
        0 => 0,
        1 => v - g.mag(0, v),
        _ => g.mag(0, v),
    };
    let x = convert(v, s, d, w, l).expect("in range");
    Scenario { deps, wds, x }
}

/// A6.2 over random request sets and settlement orders (VP12): the remaining ratios never
/// decrease; every converting deposit gets sh >= 1; burn_rem <= wsh_rem after every step;
/// the last settler of a side gets its exact remainder; sum sh = mint_c, sum conv = wc,
/// sum pay = out_c, sum left = W - wc; the record is drained by the last settlement. Also
/// (derived from the ratio invariant): every settler gets at least the floor of its
/// pro-rata share of the converted totals, and a refund-mode record repays exactly D.
#[test]
fn a62_drain_invariants_random() {
    let mut g = Rng(0xa62);
    let mut modes = HashSet::new();
    let mut scenarios = 0;
    while scenarios < 3_000 {
        let sc = scenario(&mut g);
        let x = &sc.x;
        let Some(rc0) = x.record(5, [8; 32], &owner()) else { continue };
        scenarios += 1;
        // Two independent random orders of the same requests.
        for _ in 0..2 {
            let mut order: Vec<(u8, i64, usize)> =
                sc.deps.iter().enumerate().map(|(i, &s)| (KIND_DEPOSIT, s, i)).chain(sc.wds.iter().enumerate().map(|(i, &s)| (KIND_WITHDRAW, s, i))).collect();
            g.shuffle(&mut order);
            let mut rc = rc0.clone();
            let (mut sum_sh, mut sum_conv, mut sum_pay, mut sum_left, mut refunded) = (0i64, 0i64, 0i64, 0i64, 0i64);
            for (step, &(kind, s, _)) in order.iter().enumerate() {
                assert!(!(rc.dep_rem == 0 && rc.wsh_rem == 0), "drained before the last settlement");
                let rq = RequestState::new(5, kind, s, &owner());
                let st = settle(&rc, &rq).unwrap_or_else(|| panic!("settlement {step} rejected: {rc:?} {rq:?} {x:?}"));
                let nx = &st.record;
                modes.insert(st.mode);
                if kind == KIND_DEPOSIT {
                    if rc.refund == 1 {
                        assert_eq!(st.sh, 0);
                        refunded += s;
                    } else {
                        assert!(st.sh >= 1, "A6.2: a converting deposit gets sh >= 1");
                        assert!(st.sh >= floor_u128(x.mint_c, s, x.d), "below the pro-rata floor");
                    }
                    if nx.dep_rem == 0 {
                        assert_eq!(st.sh, rc.mint_rem, "the last depositor takes the exact remainder");
                        assert_eq!(nx.mint_rem, 0);
                    } else {
                        // mint'/dep' >= mint/dep.
                        assert!(nx.mint_rem as u128 * rc.dep_rem as u128 >= rc.mint_rem as u128 * nx.dep_rem as u128, "mint ratio fell");
                    }
                    sum_sh += st.sh;
                } else {
                    assert!(st.conv >= floor_u128(x.wc, s, x.w), "conv below the pro-rata floor");
                    if nx.wsh_rem == 0 {
                        assert_eq!((st.conv, st.pay), (rc.burn_rem, rc.out_rem), "the last withdrawer takes the exact remainder");
                        assert_eq!((nx.burn_rem, nx.out_rem), (0, 0));
                    } else {
                        assert!(nx.burn_rem as u128 * rc.wsh_rem as u128 >= rc.burn_rem as u128 * nx.wsh_rem as u128, "burn ratio fell");
                        if nx.burn_rem > 0 {
                            assert!(nx.out_rem as u128 * rc.burn_rem as u128 >= rc.out_rem as u128 * nx.burn_rem as u128, "out ratio fell");
                        }
                    }
                    assert_eq!(st.left, s - st.conv);
                    if st.left > 0 {
                        // request' {round + 1, left} is a valid withdrawal request.
                        assert!(lottery::game::request_in_range(&RequestState::new(6, KIND_WITHDRAW, st.left, &owner())));
                    }
                    sum_conv += st.conv;
                    sum_pay += st.pay;
                    sum_left += st.left;
                }
                assert!(nx.burn_rem <= nx.wsh_rem, "A6.2: burn_rem <= wsh_rem");
                assert!(lottery::game::record_in_range(nx));
                assert_eq!(st.drained, step + 1 == order.len(), "drained exactly at the last settlement");
                rc = nx.clone();
            }
            assert_eq!((sum_sh, sum_conv, sum_pay), (x.mint_c, x.wc, x.out_c), "A6.2 sums");
            assert_eq!(sum_left, x.w - x.wc, "A6.2: sum left = W - wc = vault' pend_wsh");
            assert_eq!(refunded, x.d - x.d_c, "refund mode repays exactly D");
            assert_eq!((rc.dep_rem, rc.mint_rem, rc.wsh_rem, rc.burn_rem, rc.out_rem), (0, 0, 0, 0, 0));
        }
    }
    for m in [SettleMode::DepositConvert, SettleMode::DepositRefund, SettleMode::Final, SettleMode::Pay, SettleMode::Accrue] {
        assert!(modes.contains(&m), "mode {m:?} never reached");
    }
}

/// A6.2, sh >= 1 at its boundary: mint = ceil(D / MIN_DEPOSIT) exactly and every s = MIN_DEPOSIT
/// or just above, in every order of up to 6 requests.
#[test]
fn a62_min_mint_every_share_positive() {
    let mut g = Rng(0x0001);
    for n in 1..=6usize {
        for _ in 0..500 {
            let deps: Vec<i64> = (0..n).map(|_| MIN_DEPOSIT + g.range(0, MIN_DEPOSIT / 2)).collect();
            let d: i64 = deps.iter().sum();
            let mint = (d + MIN_DEPOSIT - 1) / MIN_DEPOSIT;
            let mut rc = record(2, 0, [d, mint, 0, 0, 0]);
            let mut order = deps.clone();
            g.shuffle(&mut order);
            let mut sum = 0;
            for s in order {
                let st = settle(&rc, &RequestState::new(2, KIND_DEPOSIT, s, &owner())).expect("sh >= 1 at the minimum mint");
                assert!(st.sh >= 1);
                sum += st.sh;
                rc = st.record;
            }
            assert_eq!(sum, mint);
        }
    }
}

// --------------------------------------------------------------------------- VP1

fn genesis_case() -> (Ledger, GenesisCase) {
    let mut l = Ledger::new();
    let op = l.fund(GENESIS_FUNDING, funding_spk(), 100);
    let e = l.get(&op).unwrap().clone();
    let g = GenesisCase::new(4, vec![(op, e)]);
    (l, g)
}

/// VP1: the built genesis has total_shares = S0 = floor(V0 / SHARE_UNIT), the deployer
/// position {0, S0, owner} at output k + 4 with value R inside the covenant group, the
/// vault's template fields = the compiled hashes, and every protocol output bound to the
/// new covenant ID.
#[test]
fn vp1_genesis_shares_position_templates() {
    let t = Templates::get(b_max());
    for owner in [Payout::Schnorr(xonly(&FUNDING_SK)), Payout::Ecdsa([2; 33]), Payout::P2sh([5; 32])] {
        let (mut l, mut g) = genesis_case();
        g.set_owner(&owner);
        assert_eq!(g.validate(), Ok(()));
        let v0 = g.vault.total_assets;
        let s0 = v0 / SHARE_UNIT;
        assert!(s0 >= 1);
        assert_eq!(g.vault.total_shares, s0);
        assert_eq!(g.position, PositionState::new(0, s0, &owner));
        assert_eq!((g.vault.record_tpl, g.vault.request_tpl, g.vault.position_tpl), (t.record.hash, t.request.hash, t.position.hash));
        let f = g.build().finish();
        f.run().unwrap_or_else(|(i, e)| panic!("genesis input {i} rejected: {e:?}"));
        let cov = covenant_id(&f);
        let k = g.k();
        assert_eq!(g.position_output(), k + 4);
        assert_eq!(f.tx.outputs.len(), k + 6, "k + 5 protocol outputs and change");
        for (i, o) in f.tx.outputs.iter().enumerate() {
            assert_eq!(o.covenant.map(|b| b.covenant_id), (i <= k + 4).then_some(cov), "output {i} binding");
        }
        let pos = &f.tx.outputs[k + 4];
        assert_eq!(pos.value, g.reserve());
        assert_eq!(pos.script_public_key, t.position_spk(&PositionState::new(0, s0, &owner)));
        assert_eq!(f.tx.outputs[g.vault_output()].script_public_key, t.vault_spk(&g.vault));
        assert_eq!(f.tx.outputs[g.vault_output()].value, v0 as u64 + g.reserve());
        // Conservation: inputs = outputs + fee.
        assert_eq!(f.fee(), g.fee);
        let outs = l.apply(&f.tx, &f.entries, 200).expect("genesis applies");
        assert_eq!(l.get(&outs[k + 4]).unwrap().covenant_id, Some(cov));
    }
    // S0 rounds down: V0 = 10^6 + 999 sompi gives S0 = 1,000.
    let (_, mut g) = genesis_case();
    g.set_jackpot(0);
    g.set_vault(1_000_000 + 999);
    assert_eq!((g.vault.total_shares, g.position.shares), (1_000, 1_000));
    assert_eq!(g.validate(), Ok(()));
}

/// VP1: `validate` refuses V0 < SHARE_UNIT (S0 = 0) and V0 > ASSETS_MAX, a wrong template
/// field, a position with the wrong share count, round or owner, and the builder panics
/// on each; the boundaries V0 = SHARE_UNIT and V0 = ASSETS_MAX are accepted.
#[test]
fn vp1_genesis_refusals() {
    let (_, good) = genesis_case();
    let t = Templates::get(b_max());
    let mut zero = good.clone();
    zero.set_jackpot(0);
    assert_eq!(zero.validate(), Ok(()));
    let mut cases: Vec<(&str, GenesisCase, &str)> = vec![];
    let mut add = |name: &'static str, base: &GenesisCase, f: &dyn Fn(&mut GenesisCase), needle: &'static str| {
        let mut g = base.clone();
        f(&mut g);
        cases.push((name, g, needle));
    };
    add("V0 = SHARE_UNIT - 1", &zero, &|g| g.set_vault(SHARE_UNIT - 1), "total_assets");
    add("V0 = 0", &zero, &|g| g.set_vault(0), "total_assets");
    add("V0 = ASSETS_MAX + 1", &zero, &|g| g.set_vault(ASSETS_MAX + 1), "total_assets");
    add("total_shares = S0 - 1", &good, &|g| g.vault.total_shares -= 1, "S0");
    add("total_shares = 0", &good, &|g| g.vault.total_shares = 0, "S0");
    add("record_tpl", &good, &|g| g.vault.record_tpl[0] ^= 1, "D74");
    add("request_tpl", &good, &|g| g.vault.request_tpl = [0; 32], "D74");
    add("position_tpl", &good, &|g| g.vault.position_tpl[31] ^= 1, "D74");
    add("record/request swapped", &good, &|g| std::mem::swap(&mut g.vault.record_tpl, &mut g.vault.request_tpl), "D74");
    add("position_tpl = vault hash", &good, &|g| g.vault.position_tpl = Templates::get(b_max()).vault.hash, "D74");
    add("position shares S0 + 1", &good, &|g| g.position.shares += 1, "deployer position");
    add("position shares S0 - 1", &good, &|g| g.position.shares -= 1, "deployer position");
    add("position round 1", &good, &|g| g.position.round = 1, "deployer position");
    add("position owner class 3", &good, &|g| g.position.owner_class = 3, "D11");
    add("position owner padding", &good, &|g| g.position.owner_key[32] = 1, "D11");
    for (name, g, needle) in cases {
        match g.validate() {
            Err(e) => assert!(e.contains(needle), "{name}: refused for another reason: {e}"),
            Ok(()) => panic!("{name}: bad instance accepted"),
        }
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.build_with(t.clone())));
        assert!(r.is_err(), "{name}: builder accepted the instance");
    }
    // Accepted boundaries.
    for v0 in [SHARE_UNIT, ASSETS_MAX] {
        let mut g = zero.clone();
        g.set_vault(v0);
        assert_eq!(g.validate(), Ok(()), "V0 = {v0}");
        assert_eq!(g.vault.total_shares, v0 / SHARE_UNIT);
    }
    let mut g = good.clone();
    g.set_owner(&Payout::Ecdsa([0xff; 33]));
    assert_eq!(g.validate(), Ok(()), "an ECDSA owner uses all 33 key bytes");
}

// ---------------------------------------------------- Stage 1 scaffolding (D74, D13)

fn read_covenant(name: &str) -> String {
    std::fs::read_to_string(covenant_path(name)).unwrap_or_else(|e| panic!("read {name}: {e}"))
}

fn template_hash(src: &str, ctor: &[ArtifactValue]) -> [u8; 32] {
    compile_source(src, ctor).contracts.values().next().unwrap().compiled.template_hash
}

/// D74: the position commits to VAULT_TPL and the request to RECORD_TPL and POSITION_TPL
/// (their template hashes change with those constants); the record has no constant; the
/// committed templates are compiled in that order, and the vault holds the three hashes as
/// state (its template does not depend on them).
#[test]
fn d74_template_route() {
    let t = Templates::get(b_max());
    let pos = PositionState::new(0, 1, &owner());
    let p = |h: [u8; 32]| template_hash(&read_covenant("position.sil"), &pos.ctor(h));
    assert_eq!(p(t.vault.hash), t.position.hash, "position compiled with VAULT_TPL = the vault hash");
    assert_ne!(p([0; 32]), t.position.hash, "VAULT_TPL is part of the position template");
    let rq = RequestState::new(1, KIND_DEPOSIT, MIN_DEPOSIT, &owner());
    let r = |a: [u8; 32], b: [u8; 32]| template_hash(&read_covenant("request.sil"), &rq.ctor(a, b));
    assert_eq!(r(t.record.hash, t.position.hash), t.request.hash, "request compiled with RECORD_TPL, POSITION_TPL");
    assert_ne!(r([0; 32], t.position.hash), t.request.hash, "RECORD_TPL is part of the request template");
    assert_ne!(r(t.record.hash, [0; 32]), t.request.hash, "POSITION_TPL is part of the request template");
    let rec = record(1, 0, [0; 5]);
    assert_eq!(template_hash(&read_covenant("record.sil"), &rec.ctor()), t.record.hash);
    // The vault's template does not depend on its state (record_tpl etc. are state).
    let vt = VaultState { round: 0, total_assets: 0, total_shares: 0, pend_dep: 0, pend_wsh: 0, liability: 0, record_tpl: [1; 32], request_tpl: [2; 32], position_tpl: [3; 32] };
    assert_eq!(template_hash(&read_covenant("vault.sil"), &vt.ctor(t.drawer_hash())), t.vault.hash);
    let all = [t.controller.hash, t.drawer_hash(), t.marker.hash, t.vault.hash, t.pool.hash, t.record.hash, t.position.hash, t.request.hash, t.ticket.hash, t.shard.hash, t.params.hash];
    assert_eq!(all.iter().collect::<HashSet<_>>().len(), all.len(), "all templates differ");
}
