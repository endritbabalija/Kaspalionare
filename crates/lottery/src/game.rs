//! Rust reference for the arithmetic of `close()` (spec M2 A4 checks 11, 17-20, and A6).
//!
//! Every function mirrors the script step for step in signed 64-bit integers. Overflow
//! panics here (checked ops), so a test that drives a quantity past its A6 bound fails
//! loudly instead of wrapping; in SilverScript overflow is undefined behaviour (R8), so
//! the script range-checks inputs to stay inside the same bounds.

use crate::state::{ControllerState, ParamsState, TierShares};

/// Basis points per unit.
pub const BP: i64 = 10_000;
/// Number of main-number combinations, C = N x m.
pub const N: i64 = 142_506;
/// Saturation of the jackpot estimate (D26).
pub const SAT: i64 = 100_000_000_000_000_000;
pub const TOTAL_MAX: i64 = 1 << 62;
pub const DAA_MAX: i64 = 500_000_000_000;
pub const M_HARD: i64 = 255;
pub const S_F_CAP: i64 = 1_000;
pub const S_R_CAP: i64 = 1_500;

/// floor(x x s / 10^4) without a 128-bit intermediate: exact for 0 <= x <= 2^62 and
/// 0 <= s <= 10^4 (spec A6). Maximum 2^62 + 10^4.
pub fn muldiv(x: i64, s: i64) -> i64 {
    assert!((0..=TOTAL_MAX).contains(&x), "muldiv x out of range: {x}");
    assert!((0..=BP).contains(&s), "muldiv s out of range: {s}");
    (x / BP) * s + ((x % BP) * s) / BP
}

/// The per-round shares in basis points (design 5.1, spec A3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shares {
    pub s_f: i64,
    pub s_r: i64,
    pub s_c: i64,
    pub s_d: i64,
    pub st: TierShares,
}

impl Shares {
    /// The spec's test values: s_F 5 %, s_R 10 %, s_C = s_D = 50 bp, tiers (400, 300, 300, 0, 0)
    /// (D18, D19).
    pub const TEST: Shares = Shares { s_f: 500, s_r: 1_000, s_c: 50, s_d: 50, st: [400, 300, 300, 0, 0] };

    /// Active shares of a controller.
    pub fn of(c: &ControllerState) -> Self {
        Self { s_f: c.s_f, s_r: c.s_r, s_c: c.s_c, s_d: c.s_d, st: c.st }
    }

    /// Next-round shares (A4 check 17): s_F and s_R from the params, the rest unchanged.
    pub fn next(c: &ControllerState, p: &ParamsState) -> Self {
        Self { s_f: p.next_s_f, s_r: p.next_s_r, ..Self::of(c) }
    }

    pub fn st_sum(&self) -> i64 {
        self.st.iter().sum()
    }

    /// s_H = 10^4 - s_F - s_R - s_C - s_D - sum s_t (A4 checks 11 and 17 require >= 0).
    pub fn s_h(&self) -> i64 {
        BP - self.s_f - self.s_r - self.s_c - self.s_d - self.st_sum()
    }

    /// w = 10^4 - sum s_t - s_R - s_C - s_D (whale check, A4 check 18) = s_H + s_F.
    pub fn w(&self) -> i64 {
        BP - self.st_sum() - self.s_r - self.s_c - self.s_d
    }
}

/// Round amounts from S and P_R (design 5.1, A4 check 11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundAmounts {
    pub f: i64,
    pub c_close: i64,
    pub c_draw: i64,
    /// B_1..B_5 in the tier slot order.
    pub b: [i64; 5],
    pub h: i64,
    /// Boost contribution b_r (residual); the script requires >= 0.
    pub b_r: i64,
}

impl RoundAmounts {
    pub fn b_sum(&self) -> i64 {
        self.b.iter().sum()
    }
}

/// A4 check 11: every share except the referral is muldiv(S, share); b_r is the residual.
/// Requires 0 <= S <= TOTAL_MAX and shares in [0, 10^4] (the script's range checks).
pub fn round_amounts(sales: i64, ref_paid: i64, sh: &Shares) -> RoundAmounts {
    let s_h = sh.s_h();
    assert!(s_h >= 0, "s_H < 0");
    let f = muldiv(sales, sh.s_f);
    let c_close = muldiv(sales, sh.s_c);
    let c_draw = muldiv(sales, sh.s_d);
    let b = sh.st.map(|s| muldiv(sales, s));
    let h = muldiv(sales, s_h);
    let b_r = sales - ref_paid - f - c_close - c_draw - b.iter().sum::<i64>() - h;
    RoundAmounts { f, c_close, c_draw, b, h, b_r }
}

/// The jackpot estimate at `close()` (A4 check 18, D26).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Estimate {
    pub j_house: i64,
    pub j_boost: i64,
    /// Ĵ = Ĵ_house + Ĵ_boost.
    pub j: i64,
    /// Ĝ2 = muldiv(Ĵ, g).
    pub g2: i64,
}

/// min(a + b, SAT) for a, b >= 0, evaluated as SAT when a >= SAT, so the sum never
/// overflows (A6). The script must use the same order.
pub fn sat_add(a: i64, b: i64) -> i64 {
    assert!(a >= 0 && b >= 0 && b <= TOTAL_MAX);
    if a >= SAT { SAT } else { (a + b).min(SAT) }
}

/// Ĵ_house = muldiv(min(V_rec + H, SAT), j); Ĵ_boost = min((O - J_boost) + b_r, SAT);
/// Ĝ2 = muldiv(Ĵ, g). `pot_left` is O - J_boost (the boost pot left in the controller
/// after `close()`), `b_r` the round's boost contribution.
pub fn estimate(v_rec: i64, h: i64, pot_left: i64, b_r: i64, j: i64, g: i64) -> Estimate {
    let j_house = muldiv(sat_add(v_rec, h), j);
    let j_boost = sat_add(pot_left, b_r);
    let jj = j_house + j_boost;
    Estimate { j_house, j_boost, j: jj, g2: muldiv(jj, g) }
}

/// Allowance per combination a = muldiv(price x s_H', 10^4 - e) / 10^4
/// = floor(p (10^4 - e) s_H' / 10^8) (nested floors, A6).
pub fn allowance(price: i64, s_h_next: i64, e: i64) -> i64 {
    muldiv(price.checked_mul(s_h_next).expect("price x s_H"), BP - e) / BP
}

/// Whale allowance per combination q = price x w' / margin (rounds down: favours the check).
pub fn whale_allowance(price: i64, w_next: i64, margin: i64) -> i64 {
    price.checked_mul(w_next).expect("price x w") / margin
}

/// The two checks of the game-size rule at Ĵ (design 4.6, A4 check 18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeRule {
    pub est: Estimate,
    pub a: i64,
    pub q: i64,
    pub m_min: i64,
    pub m_max: i64,
}

impl SizeRule {
    /// Edge rule: Ĵ_house + Ĝ2 (x - 1) <= a N x.
    pub fn edge(&self, x: i64) -> bool {
        let lhs = self.est.j_house + self.est.g2.checked_mul(x - 1).expect("G2 (x-1)");
        lhs <= self.a.checked_mul(N).and_then(|v| v.checked_mul(x)).expect("a N x")
    }

    /// Whale check: Ĵ + Ĝ2 (x - 1) <= q N x.
    pub fn whale(&self, x: i64) -> bool {
        let lhs = self.est.j + self.est.g2.checked_mul(x - 1).expect("G2 (x-1)");
        lhs <= self.q.checked_mul(N).and_then(|v| v.checked_mul(x)).expect("q N x")
    }

    pub fn pass(&self, x: i64) -> bool {
        self.edge(x) && self.whale(x)
    }

    /// The two checks in the form c <= s x (D25, revised 2026-10-10): the edge rule has
    /// c = Ĵ_house - Ĝ2 and s = a N - Ĝ2, the whale check c = Ĵ - Ĝ2 and s = q N - Ĝ2.
    /// (Ĵ_house + Ĝ2 (x - 1) <= a N x  <=>  Ĵ_house - Ĝ2 <= x (a N - Ĝ2).)
    pub fn checks(&self) -> [(i64, i64); 2] {
        let g2 = self.est.g2;
        let a_n = self.a.checked_mul(N).expect("a N");
        let q_n = self.q.checked_mul(N).expect("q N");
        [(self.est.j_house - g2, a_n - g2), (self.est.j - g2, q_n - g2)]
    }

    /// Reference by brute force: the smallest passing m in [m_min, m_max], else m_max.
    pub fn choose_brute(&self) -> i64 {
        (self.m_min..=self.m_max).find(|&x| self.pass(x)).unwrap_or(self.m_max)
    }

    /// D25 closed form, step for step as the script computes it (A4 check 19). Start from
    /// [lo, hi] = [m_min, m_max]; for each check c <= s x (x >= 1):
    ///   s > 0: if c > 0, lo = max(lo, ceil(c / s)), with ceil(c / s) = (c - 1) / s + 1;
    ///   s = 0: if c > 0, no x passes;
    ///   s < 0: if c > 0, no x passes; else hi = min(hi, (-c) / (-s)).
    /// Every division has non-negative operands. m' = lo if some x passes and lo <= hi,
    /// else m_max. The script requires the argument m_next = m'.
    pub fn choose(&self) -> i64 {
        let (mut lo, mut hi, mut any) = (self.m_min, self.m_max, true);
        for (c, s) in self.checks() {
            if s > 0 {
                if c > 0 {
                    lo = lo.max((c - 1) / s + 1);
                }
            } else if s == 0 {
                if c > 0 {
                    any = false;
                }
            } else if c > 0 {
                any = false;
            } else {
                hi = hi.min((-c) / (-s));
            }
        }
        if any && lo <= hi { lo } else { self.m_max }
    }
}

/// Everything `close()` derives from its inputs (A4 checks 11-20), for the builder and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseMath {
    pub sales: i64,
    pub ref_paid: i64,
    pub amounts: RoundAmounts,
    /// O = V_c - A - R.
    pub pot: i64,
    /// Output 0 value: V_c - J_boost.
    pub controller_out: i64,
    /// Output k + 2 value: S - P_R - C_close + J_boost + R.
    pub marker_value: i64,
    pub next: Shares,
    pub rule: SizeRule,
    /// Reference m(r+1).
    pub m_next: i64,
    /// Max creation DAA score over all inputs.
    pub own: i64,
    pub deadline: i64,
    pub seed_ref: i64,
}

/// T' = max(T + round_len, own + d_min) (A4 check 20); the script requires T' < DAA_MAX.
pub fn next_deadline(t: i64, round_len: i64, own: i64, d_min: i64) -> i64 {
    (t + round_len).max(own + d_min)
}

/// seed_ref = own + seed_offset (A4 check 20; randomness parked, the offset is a placeholder).
pub fn seed_ref(own: i64, seed_offset: i64) -> i64 {
    own + seed_offset
}

/// Reference computation of a `close()`: `shards` are (S_i, P_R,i), `controller_value` is
/// V_c, `own` the max input creation DAA score.
pub fn close_math(c: &ControllerState, p: &ParamsState, shards: &[(i64, i64)], controller_value: i64, own: i64) -> CloseMath {
    let sales: i64 = shards.iter().map(|s| s.0).sum();
    let ref_paid: i64 = shards.iter().map(|s| s.1).sum();
    let amounts = round_amounts(sales, ref_paid, &Shares::of(c));
    let pot = controller_value - c.acc - c.reserve;
    let next = Shares::next(c, p);
    assert!(next.s_h() >= 0, "s_H' < 0");
    let est = estimate(c.v_rec, amounts.h, pot - c.j_boost, amounts.b_r, c.j, c.g);
    let rule = SizeRule {
        est,
        a: allowance(c.price, next.s_h(), c.e),
        q: whale_allowance(c.price, next.w(), c.margin),
        m_min: c.m_min,
        m_max: c.m_max,
    };
    CloseMath {
        sales,
        ref_paid,
        amounts,
        pot,
        controller_out: controller_value - c.j_boost,
        marker_value: sales - ref_paid - amounts.c_close + c.j_boost + c.reserve,
        next,
        rule,
        m_next: rule.choose(),
        own,
        deadline: next_deadline(c.deadline, c.round_len, own, c.d_min),
        seed_ref: seed_ref(own, c.seed_offset),
    }
}

#[cfg(test)]
mod tests {
    //! CP9: closed form (D25 revised) vs brute force, muldiv exactness, shares, b_r >= 0.
    use super::*;

    /// splitmix64: deterministic test randomness without a dependency.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
        /// Uniform in [lo, hi].
        fn range(&mut self, lo: i64, hi: i64) -> i64 {
            lo + (self.next() % ((hi - lo) as u64 + 1)) as i64
        }
        /// Log-uniform-ish in [0, hi]: picks a magnitude first, so small and huge values both occur.
        fn mag(&mut self, hi: i64) -> i64 {
            let bits = self.range(0, 63 - hi.leading_zeros() as i64);
            self.range(0, ((1i128 << bits) - 1).min(hi as i128) as i64)
        }
    }

    const KAS: i64 = 100_000_000;

    #[test]
    fn muldiv_exact() {
        let mut r = Rng(1);
        let edges = [0, 1, BP - 1, BP, BP + 1, TOTAL_MAX - 1, TOTAL_MAX, SAT, i64::MAX / BP];
        for &x in edges.iter().filter(|&&x| x <= TOTAL_MAX) {
            for s in [0, 1, 50, 9_999, BP] {
                assert_eq!(muldiv(x, s) as i128, x as i128 * s as i128 / BP as i128, "x={x} s={s}");
            }
        }
        for _ in 0..200_000 {
            let x = r.mag(TOTAL_MAX);
            let s = r.range(0, BP);
            assert_eq!(muldiv(x, s) as i128, x as i128 * s as i128 / BP as i128, "x={x} s={s}");
        }
        // Maximum (A6): 2^62 x 10^4 / 10^4.
        assert_eq!(muldiv(TOTAL_MAX, BP), TOTAL_MAX);
    }

    #[test]
    fn shares_sum_to_one() {
        let s = Shares::TEST;
        assert_eq!(s.s_h(), 7_400);
        assert_eq!(s.w(), s.s_h() + s.s_f);
        assert_eq!(s.s_h() + s.s_f + s.s_r + s.s_c + s.s_d + s.st_sum(), BP);
        // Caps leave s_H >= 0 at the test tiers and fees.
        let capped = Shares { s_f: S_F_CAP, s_r: S_R_CAP, ..s };
        assert_eq!(capped.s_h(), 6_400);
    }

    #[test]
    fn round_amounts_conserve_and_b_r_nonnegative() {
        let mut r = Rng(2);
        for _ in 0..100_000 {
            let s_r = r.range(0, S_R_CAP);
            let sh = Shares { s_f: r.range(0, S_F_CAP), s_r, s_c: r.range(0, 200), s_d: r.range(0, 200), st: [400, 300, 300, 0, 0] };
            // P_R is a sum of per-purchase floors of P x s_R / 10^4, each at most the floor of
            // the round total, so P_R <= floor(S s_R / 10^4).
            let mut sales = 0i64;
            let mut paid = 0i64;
            for _ in 0..r.range(0, 20) {
                let p = r.range(1, 1_000) * r.range(1, 10 * KAS);
                sales += p;
                if r.next() % 2 == 0 {
                    paid += p * s_r / BP;
                }
            }
            let a = round_amounts(sales, paid, &sh);
            assert!(a.b_r >= 0, "b_r < 0: S={sales} P_R={paid} {sh:?}");
            assert_eq!(a.b_r + paid + a.f + a.c_close + a.c_draw + a.b_sum() + a.h, sales);
        }
        // Worst case for the residual: P_R = floor(S s_R / 10^4) exactly.
        let sh = Shares::TEST;
        for sales in [0, 1, 9_999, 10_000, 123_456_789, TOTAL_MAX] {
            let a = round_amounts(sales, muldiv(sales, sh.s_r), &sh);
            assert!(a.b_r >= 0);
        }
    }

    fn random_rule(r: &mut Rng) -> SizeRule {
        let price = if r.next() % 4 == 0 { r.range(1, 100_000_000_000) } else { [KAS / 10, KAS, 10 * KAS][r.range(0, 2) as usize] };
        let sh = Shares { s_f: r.range(0, S_F_CAP), s_r: r.range(0, S_R_CAP), s_c: 50, s_d: 50, st: [400, 300, 300, 0, 0] };
        let e = r.range(0, 5_000);
        let j = r.range(0, 5_000);
        let g = r.range(0, 1_000);
        let margin = r.range(BP, 2 * BP);
        let m_min = r.range(2, 60);
        let m_max = r.range(m_min, M_HARD);
        let est = estimate(r.mag(TOTAL_MAX), r.mag(TOTAL_MAX), r.mag(TOTAL_MAX), r.mag(TOTAL_MAX), j, g);
        SizeRule { est, a: allowance(price, sh.s_h(), e), q: whale_allowance(price, sh.w(), margin), m_min, m_max }
    }

    /// A rule whose house estimate is small against a large boost and a high g, so the
    /// edge slope a N - Ĝ2 is often negative (the passing set is bounded above).
    fn boost_heavy_rule(r: &mut Rng) -> SizeRule {
        let price = [KAS / 10, KAS, 10 * KAS][r.range(0, 2) as usize];
        let sh = Shares { s_f: r.range(0, S_F_CAP), s_r: r.range(0, S_R_CAP), s_c: 50, s_d: 50, st: [400, 300, 300, 0, 0] };
        let e = r.range(2_500, 5_000);
        let g = r.range(500, 1_000);
        let m_min = r.range(2, 20);
        let m_max = r.range(m_min, M_HARD);
        let boost = r.range(1_000, 50_000_000) * KAS;
        let house = if r.next() % 2 == 0 { 0 } else { r.mag(10_000_000 * KAS) };
        let est = estimate(house, 0, boost, r.mag(1_000 * KAS), r.range(0, 5_000), g);
        SizeRule { est, a: allowance(price, sh.s_h(), e), q: whale_allowance(price, sh.w(), r.range(BP, 2 * BP)), m_min, m_max }
    }

    /// A rule built directly near the edge rule's upper bound: Ĝ2 slightly above a N, so
    /// the passing set ends at about x = Ĝ2 / (Ĝ2 - a N) inside [m_min, m_max].
    fn bounded_rule(r: &mut Rng) -> SizeRule {
        let sh = Shares::TEST;
        let price = [KAS / 10, KAS, 10 * KAS][r.range(0, 2) as usize];
        let (a, q) = (allowance(price, sh.s_h(), r.range(0, 5_000)), whale_allowance(price, sh.w(), r.range(BP, 2 * BP)));
        let t = r.range(2, 300);
        let g2 = a * N + a * N / t + r.range(-1_000, 1_000);
        let j_house = r.range(0, g2);
        let j = j_house + r.mag(g2);
        let m_min = r.range(2, 20);
        SizeRule { est: Estimate { j_house, j_boost: j - j_house, j, g2 }, a, q, m_min, m_max: r.range(m_min, M_HARD) }
    }

    /// CP9 / D25: the closed form equals "smallest m in [m_min, m_max] passing both checks,
    /// else m_max" over many random inputs, and the generators reach every branch.
    #[test]
    fn closed_form_matches_brute_force() {
        let mut r = Rng(3);
        // Branch coverage: [edge s<0, edge s=0, edge s>0 with c>0, none pass, bounded above
        // and some pass, m_min passes].
        let mut cov = [0u32; 6];
        for n in 0..60_000 {
            let rule = match n % 3 {
                0 => random_rule(&mut r),
                1 => boost_heavy_rule(&mut r),
                _ => bounded_rule(&mut r),
            };
            let m = rule.choose();
            assert_eq!(m, rule.choose_brute(), "{rule:?}");
            let [(c1, s1), _] = rule.checks();
            if s1 < 0 { cov[0] += 1 }
            if s1 == 0 { cov[1] += 1 }
            if s1 > 0 && c1 > 0 { cov[2] += 1 }
            if !rule.pass(m) { cov[3] += 1 }
            if rule.pass(m) && !rule.pass(rule.m_max) { cov[4] += 1 }
            if m == rule.m_min && rule.pass(m) { cov[5] += 1 }
        }
        println!("closed-form branch coverage [s1<0, s1=0, s1>0&c1>0, none, bounded, m_min]: {cov:?}");
        assert!(cov[0] > 1_000 && cov[2] > 1_000 && cov[3] > 1_000 && cov[4] > 100 && cov[5] > 1_000, "coverage {cov:?}");
        // s1 = 0 exactly is rare at random; check that branch directly.
        let mut rule = random_rule(&mut r);
        rule.est = Estimate { j_house: 5, j_boost: 0, j: 5, g2: rule.a * N };
        assert_eq!(rule.checks()[0].1, 0);
        assert_eq!(rule.choose(), rule.choose_brute());
        rule.est = Estimate { j_house: 0, j_boost: 0, j: 0, g2: rule.a * N };
        assert_eq!(rule.choose(), rule.choose_brute());
    }

    /// The whale check's constant term Ĵ - Ĝ2 is never negative (g <= 10^4), so its passing
    /// set is always upward closed; only the edge rule can be bounded above.
    #[test]
    fn whale_check_upward_closed() {
        let mut r = Rng(5);
        for n in 0..5_000 {
            let rule = if n % 2 == 0 { random_rule(&mut r) } else { boost_heavy_rule(&mut r) };
            assert!(rule.checks()[1].0 >= 0);
            for x in rule.m_min..rule.m_max {
                assert!(!rule.whale(x) || rule.whale(x + 1), "x={x} {rule:?}");
            }
        }
    }

    /// The boost-dominated case of the revised D25 (CA4): e = 5,000, g = 1,000, boost
    /// 539,000 KAS, no house estimate, price 1 KAS, m in [2, 50]. The passing set is an
    /// interval bounded above that does not reach m_max; the unique reference is m = 12.
    #[test]
    fn boost_dominated_reference_is_unique() {
        let sh = Shares::TEST;
        let rule = SizeRule {
            est: estimate(0, 0, 539_000 * KAS, 0, 1_000, 1_000),
            a: allowance(KAS, sh.s_h(), 5_000),
            q: whale_allowance(KAS, sh.w(), 11_700),
            m_min: 2,
            m_max: 50,
        };
        assert!(rule.checks()[0].1 < 0, "edge slope negative");
        assert_eq!(rule.choose(), 12);
        assert_eq!(rule.choose_brute(), 12);
        assert!(rule.pass(12) && !rule.pass(11) && !rule.pass(50));
        let passing: Vec<i64> = (2..=50).filter(|&x| rule.pass(x)).collect();
        assert!(passing.windows(2).all(|w| w[1] == w[0] + 1), "an interval: {passing:?}");
        assert!(*passing.last().unwrap() < 50);
    }

    #[test]
    fn a6_bounds_hold_at_extremes() {
        // Largest operands allowed by the A3 ranges.
        let est = estimate(TOTAL_MAX, TOTAL_MAX, TOTAL_MAX, TOTAL_MAX, 5_000, 1_000);
        assert_eq!(est.j_house, muldiv(SAT, 5_000));
        assert_eq!(est.j_boost, SAT);
        assert!(est.j <= 5 * 10i64.pow(16) + SAT);
        assert!(est.g2 <= 15 * 10i64.pow(15));
        let rule = SizeRule {
            est,
            a: allowance(100_000_000_000, BP, 0),
            q: whale_allowance(100_000_000_000, BP, BP),
            m_min: 2,
            m_max: M_HARD,
        };
        // No overflow anywhere in the scan (checked ops panic otherwise).
        let _ = rule.choose_brute();
        assert!(rule.a.checked_mul(N).unwrap().checked_mul(M_HARD).is_some());
    }

    #[test]
    fn worked_example_design_4_6() {
        // Design 4.6: p = 1 KAS, Ĵ_house = 1,000,000 KAS, G2 = 10,000 KAS, no boost -> m = 13
        // (the design's s_H = 74 %, e = 15 %).
        let sh = Shares::TEST;
        let est = estimate(10_000_000 * KAS, 0, 0, 0, 1_000, 100);
        assert_eq!((est.j_house, est.g2), (1_000_000 * KAS, 10_000 * KAS));
        let rule = SizeRule {
            est,
            a: allowance(KAS, sh.s_h(), 1_500),
            q: whale_allowance(KAS, sh.w(), 11_700),
            m_min: 2,
            m_max: 50,
        };
        assert_eq!(rule.a, 62_900_000);
        assert_eq!(rule.choose(), 13);
        assert_eq!(rule.choose_brute(), 13);
        assert!(rule.pass(13) && !rule.pass(12));
    }

    #[test]
    fn deadline_and_seed() {
        assert_eq!(next_deadline(1_000_000, 864_000, 1_500_000, 72_000), 1_864_000);
        assert_eq!(next_deadline(1_000_000, 864_000, 1_900_000, 72_000), 1_972_000);
        assert_eq!(seed_ref(1_900_000, 600), 1_900_600);
    }
}
