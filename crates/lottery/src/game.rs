//! Rust reference for the arithmetic of `close()` (spec M2 A4 checks 11, 17-20, and A6) and
//! of `draw()` (spec M3 A4 checks 8-20, A5, A6).
//!
//! Every function mirrors the script step for step in signed 64-bit integers. Overflow
//! panics here (checked ops), so a test that drives a quantity past its A6 bound fails
//! loudly instead of wrapping; in SilverScript overflow is undefined behaviour (R8), so
//! the script range-checks inputs to stay inside the same bounds.

use crate::state::{ControllerState, MarkerState, ParamsState, TIER_JACKPOT, TIER_LOWER0, TIER_SECOND, TierShares};

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

// =============================================================================== M3 draw()
//
// Rust reference for the arithmetic of `draw()` (spec M3 A4 checks 8-20, A5, A6). Each
// function mirrors the script step for step. Functions return `None` where the script
// rejects (a guard of A6 or a `>= 0` requirement fails).

/// Largest m the drawer can draw (D49): its count loop is unrolled to M_DRAW.
pub const M_DRAW: i64 = 50;
/// Most expired pools one `draw()` absorbs (D44; 7 since M4 review U5, as many as one
/// `draw()` creates). Mirrors drawer.sil `A_MAX`.
pub const A_MAX: usize = 7;
/// Minimum fee payout, 1 KAS (D41).
pub const F_MIN: i64 = 100_000_000;
/// Upper bound of the drawer's f_min (spec M3 A3 ranges).
pub const F_MIN_MAX: i64 = 10_000_000_000;
/// Lower bound of the drawer's f_min, 0.1 KAS (USER-APPROVED Stage 5 decision A, M3 review;
/// the user called it "F_MIN"; `F_MIN` above stays the default f_min of 1 KAS, D41).
/// - f_min = 0 makes a round with T_F = 0 need a zero-value fee output, which is invalid
///   (rusty-kaspa v2.1.0 `consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs:150-151`),
///   so that round and every later one could never be drawn.
/// - A tiny f_min makes the fee output's storage term C / T_F unbounded (costs.md M3-d).
/// Enforced by the drawer (A3.1 ranges, check 1) and by genesis `validate()`.
pub const F_MIN_FLOOR: i64 = 10_000_000;
/// Largest G2 the drawer accepts when it creates a second-prize pool (A6, n2 x G2 with
/// n2 <= 254): TOTAL_MAX / 254. Genesis `validate()` requires G2(1) <= G2_MAX.
pub const G2_MAX: i64 = TOTAL_MAX / 254;
/// Claim window, 7 days at 10 BPS (D39).
pub const CLAIM_WINDOW: i64 = 6_048_000;
/// Seed-validity window, 12 h at 10 BPS (D39, design 11).
pub const DRAW_WINDOW: i64 = 432_000;
/// Domain tag of the TEST-ONLY seed hash (D42), 14 bytes.
pub const SEED_TAG: &[u8; 14] = b"KLOT/test-seed";

impl Shares {
    /// The round-r shares frozen in a marker (spec M3 check 11).
    pub fn of_marker(mk: &MarkerState) -> Self {
        Self { s_f: mk.s_f, s_r: mk.s_r, s_c: mk.s_c, s_d: mk.s_d, st: mk.st }
    }
}

/// Round amounts of round r from its marker (check 11): F, C_close, C_draw, B_1..5, H, b_r.
pub fn draw_amounts(mk: &MarkerState) -> RoundAmounts {
    round_amounts(mk.sales, mk.ref_paid, &Shares::of_marker(mk))
}

/// Winning combinations per lower slot (4+b, 4, 3+b, 3, 2+b) at bonus range m (D46, design
/// 4.1): 125, 125 (m - 1), 3,000, 3,000 (m - 1), 23,000. At most 3,000 x 254 < 10^6 (A6).
pub fn tier_w(m: i64) -> [i64; 5] {
    assert!((2..=M_HARD).contains(&m), "m out of range: {m}");
    [125, 125 * (m - 1), 3_000, 3_000 * (m - 1), 23_000]
}

/// h_J and n2 from the K count strings at the winning rank (check 10): h_J = sum over the
/// shards of count[bonus]; n2 = #{b in 1..m, b != bonus : sum_i count_i[b] >= 1}. `counts[i]`
/// is shard i's m counts (bonus values 1..m).
pub fn win_counts(counts: &[Vec<u16>], bonus: u8, m: i64) -> (i64, i64) {
    let total = |b: usize| counts.iter().map(|c| c[b - 1] as i64).sum::<i64>();
    let h_j = total(bonus as usize);
    let n2 = (1..=m as usize).filter(|&b| b != bonus as usize && total(b) >= 1).count() as i64;
    (h_j, n2)
}

/// One new prize pool of `draw()` (A3.3): tier code, slice and economic value (without R).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewPool {
    pub tier: u8,
    pub slice: i64,
    pub value: i64,
}

/// The pools `draw()` creates (D45, check 12) and their indicators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolSet {
    /// jp = [h_J >= 1 and J > 0].
    pub jp: bool,
    /// sp = [n2 >= 1 and G2 > 0].
    pub sp: bool,
    /// lp_t = [slice_t >= 1].
    pub lp: [bool; 5],
    /// slice_t = floor(B_t / W_t).
    pub slices: [i64; 5],
    /// The pools that exist, in tier order 0..6 (output order 3..2+np).
    pub pools: Vec<NewPool>,
}

impl PoolSet {
    pub fn np(&self) -> usize {
        self.pools.len()
    }
}

/// D45 pool creation. `jackpot` = J, `g2` = G2 (marker), `b` the five lower budgets, `m` the
/// marker's m. The second-prize pool holds n2 x G2 (n2 <= 254; overflow panics where the
/// script's checked multiplication rejects).
pub fn pool_set(h_j: i64, n2: i64, jackpot: i64, g2: i64, b: &[i64; 5], m: i64) -> PoolSet {
    let jp = h_j >= 1 && jackpot > 0;
    let sp = n2 >= 1 && g2 > 0;
    let w = tier_w(m);
    let slices: [i64; 5] = std::array::from_fn(|t| b[t] / w[t]);
    let lp = slices.map(|s| s >= 1);
    let mut pools = vec![];
    if jp {
        pools.push(NewPool { tier: TIER_JACKPOT, slice: jackpot, value: jackpot });
    }
    if sp {
        pools.push(NewPool { tier: TIER_SECOND, slice: g2, value: n2.checked_mul(g2).expect("n2 x G2") });
    }
    for t in 0..5 {
        if lp[t] {
            pools.push(NewPool { tier: TIER_LOWER0 + t as u8, slice: slices[t], value: b[t] });
        }
    }
    PoolSet { jp, sp, lp, slices, pools }
}

/// a + b for money amounts with the A6 guard a <= TOTAL_MAX - b (`None` = script rejects).
pub fn guarded_add(a: i64, b: i64) -> Option<i64> {
    (a >= 0 && (0..=TOTAL_MAX).contains(&b) && a <= TOTAL_MAX - b).then(|| a + b)
}

/// Vault settlement (design 5.4, check 14): V' = total_assets + H - J_house [jp] - G2 n2 [sp],
/// required >= 0.
pub fn vault_settle(total_assets: i64, h: i64, j_house: i64, jp: bool, g2: i64, n2: i64, sp: bool) -> Option<i64> {
    let mut v = guarded_add(total_assets, h)?;
    if jp {
        v -= j_house;
    }
    if sp {
        v -= n2.checked_mul(g2)?;
    }
    (v >= 0).then_some(v)
}

/// Boost pot after `draw()` (design 5.5, check 15):
/// O' = O + b_r + J_boost [not jp] + sum B_t [not lp_t] + sum Q, every addition guarded.
pub fn boost_after(o: i64, b_r: i64, j_boost: i64, jp: bool, b: &[i64; 5], lp: &[bool; 5], q: &[i64]) -> Option<i64> {
    let mut v = guarded_add(o, b_r)?;
    if !jp {
        v = guarded_add(v, j_boost)?;
    }
    for t in 0..5 {
        if !lp[t] {
            v = guarded_add(v, b[t])?;
        }
    }
    for &x in q {
        v = guarded_add(v, x)?;
    }
    Some(v)
}

/// Fee outcome (D41, check 16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeSplit {
    /// T_F = A + F.
    pub t_f: i64,
    /// A fee output of T_F exists.
    pub paid: bool,
    /// A' in the controller.
    pub a_next: i64,
}

pub fn fee_split(acc: i64, f: i64, f_min: i64) -> Option<FeeSplit> {
    let t_f = guarded_add(acc, f)?;
    let paid = t_f >= f_min;
    Some(FeeSplit { t_f, paid, a_next: if paid { 0 } else { t_f } })
}

/// The three bounds J(r+1) must respect (spec M3 A5, D47), with m' >= 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JackpotRule {
    /// E = a N m' (edge rule).
    pub e_lim: i64,
    /// W = q N m' (whale check).
    pub w_lim: i64,
    /// Cv = muldiv(V', cap).
    pub cv: i64,
    pub m: i64,
    pub g: i64,
}

impl JackpotRule {
    /// From the round r+1 controller (its m, price, active shares and the immutable e, g,
    /// margin, cap) and the settled vault V'.
    pub fn new(ctrl: &ControllerState, v_prime: i64) -> Self {
        let sh = Shares::of(ctrl);
        assert!(sh.s_h() >= 0, "s_H' < 0");
        let a = allowance(ctrl.price, sh.s_h(), ctrl.e);
        let q = whale_allowance(ctrl.price, sh.w(), ctrl.margin);
        let nm = N.checked_mul(ctrl.m).expect("N m'");
        Self {
            e_lim: a.checked_mul(nm).expect("a N m'"),
            w_lim: q.checked_mul(nm).expect("q N m'"),
            cv: muldiv(v_prime, ctrl.cap),
            m: ctrl.m,
            g: ctrl.g,
        }
    }

    /// pass(x, y) of A5, in the script's order (no overflow: x + y is formed only after
    /// y <= W - x, and W <= 3.6 x 10^18). Non-increasing in x and in y; pass(0, 0) holds.
    pub fn pass(&self, x: i64, y: i64) -> bool {
        assert!(x >= 0 && y >= 0 && self.m >= 2);
        if x > self.e_lim || x > self.cv || y > self.w_lim - x {
            return false;
        }
        let gg = muldiv(x + y, self.g);
        let d = self.m - 1;
        gg <= (self.e_lim - x) / d && gg <= (self.w_lim - x - y) / d && gg <= (self.cv - x) / d
    }

    /// The three inequalities in exact 128-bit arithmetic (test cross-check of `pass`):
    /// edge x + G (m'-1) <= E, whale x + y + G (m'-1) <= W, cap x + G (m'-1) <= Cv.
    pub fn pass_exact(&self, x: i64, y: i64) -> bool {
        let (x, y) = (x as i128, y as i128);
        let gg = (x + y) * self.g as i128 / BP as i128;
        let l = gg * (self.m as i128 - 1);
        x + l <= self.e_lim as i128 && x + y + l <= self.w_lim as i128 && x + l <= self.cv as i128
    }
}

/// J(r+1) fixed by `draw()` (A5 and check 17).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JackpotFix {
    /// Base x_b = muldiv(V', j).
    pub x_b: i64,
    pub x1: i64,
    pub y: i64,
    pub x2: i64,
    /// J' = x2 + y.
    pub jackpot: i64,
    /// J_boost' = y.
    pub j_boost: i64,
    /// G2' = muldiv(J', g).
    pub g2: i64,
    /// L' = x2 + G2' (m' - 1).
    pub liability: i64,
}

impl JackpotFix {
    fn from_witness(rule: &JackpotRule, x_b: i64, x1: i64, y: i64, x2: i64) -> Self {
        let jackpot = x2 + y;
        let g2 = muldiv(jackpot, rule.g);
        Self { x_b, x1, y, x2, jackpot, j_boost: y, g2, liability: x2 + g2 * (rule.m - 1) }
    }
}

/// Largest v in [lo, hi] with pred(v), given pred(lo) and pred non-increasing.
fn largest(mut lo: i64, mut hi: i64, pred: impl Fn(i64) -> bool) -> i64 {
    debug_assert!(pred(lo));
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if pred(mid) { lo = mid } else { hi = mid - 1 }
    }
    lo
}

/// D47/D48 reference by bisection: base -> lower if it fails -> boost -> top-up.
pub fn fix_jackpot(rule: &JackpotRule, v_prime: i64, o_prime: i64, j: i64) -> JackpotFix {
    assert!((0..=TOTAL_MAX).contains(&v_prime) && (0..=TOTAL_MAX).contains(&o_prime));
    let x_b = muldiv(v_prime, j);
    let x1 = if rule.pass(x_b, 0) { x_b } else { largest(0, x_b - 1, |x| rule.pass(x, 0)) };
    let y = largest(0, o_prime, |y| rule.pass(x1, y));
    let top = rule.e_lim.min(rule.cv).max(x1);
    let x2 = largest(x1, top, |x| rule.pass(x, y));
    JackpotFix::from_witness(rule, x_b, x1, y, x2)
}

/// Brute-force reference for small instances (DP17): linear scans that do not assume
/// monotonicity ("largest value" = the maximum over the whole range).
pub fn fix_jackpot_brute(rule: &JackpotRule, v_prime: i64, o_prime: i64, j: i64) -> JackpotFix {
    let x_b = muldiv(v_prime, j);
    let x1 = if rule.pass(x_b, 0) { x_b } else { (0..x_b).rev().find(|&x| rule.pass(x, 0)).expect("pass(0, 0)") };
    let y = (0..=o_prime).filter(|&y| rule.pass(x1, y)).max().expect("pass(x1, 0)");
    let top = rule.e_lim.min(rule.cv);
    let x2 = (x1..=top.max(x1)).filter(|&x| rule.pass(x, y)).max().expect("pass(x1, y)");
    JackpotFix::from_witness(rule, x_b, x1, y, x2)
}

/// The A5 witness checks as the script applies them to the caller's x1, y, x2.
pub fn witness_ok(rule: &JackpotRule, v_prime: i64, o_prime: i64, j: i64, x1: i64, y: i64, x2: i64) -> bool {
    let x_b = muldiv(v_prime, j);
    let c1 = if rule.pass(x_b, 0) { x1 == x_b } else { 0 <= x1 && x1 < x_b && rule.pass(x1, 0) && !rule.pass(x1 + 1, 0) };
    c1 && 0 <= y
        && y <= o_prime
        && rule.pass(x1, y)
        && (y == o_prime || !rule.pass(x1, y + 1))
        && x2 >= x1
        && rule.pass(x2, y)
        && !rule.pass(x2 + 1, y)
}

/// The TEST-ONLY seed mapping (D42).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedNumbers {
    pub h: [u8; 32],
    pub rank: u32,
    pub bonus: u8,
    /// The canonical main numbers of colex rank `rank`.
    pub main: [u8; 5],
}

/// bin2num(bytes || 0x00) for 7 bytes: the non-negative 56-bit little-endian value
/// (OpBin2Num decodes a little-endian sign-magnitude number of up to 8 bytes, non-minimal
/// encodings allowed: rusty-kaspa v2.1.0 `crypto/txscript/src/opcodes/mod.rs:1310-1316`,
/// `crypto/txscript/src/data_stack.rs:164-178`).
pub fn u56_le(b: &[u8]) -> i64 {
    assert_eq!(b.len(), 7);
    b.iter().rev().fold(0i64, |acc, &x| (acc << 8) | x as i64)
}

/// h = blake2b("KLOT/test-seed" || cov || round (8 B LE) || test_only_seed); rank =
/// u56(h[0..7]) mod N; bonus = u56(h[7..14]) mod m + 1 (D42).
pub fn seed_numbers(cov: &[u8; 32], round: i64, test_only_seed: &[u8; 32], m: i64) -> SeedNumbers {
    assert!(m >= 2);
    let mut msg = Vec::with_capacity(86);
    msg.extend_from_slice(SEED_TAG);
    msg.extend_from_slice(cov);
    msg.extend_from_slice(&round.to_le_bytes());
    msg.extend_from_slice(test_only_seed);
    let h = crate::params::blake2b(&msg);
    let rank = (u56_le(&h[0..7]) % N) as u32;
    let bonus = (u56_le(&h[7..14]) % m + 1) as u8;
    SeedNumbers { h, rank, bonus, main: crate::combo::unrank(rank) }
}

/// D_claim = max(own, seed_ref + draw_window) + claim_window (D39, check 19); `None` where
/// the script rejects (own or the sums reach DAA_MAX).
pub fn claim_deadline(own: i64, seed_ref: i64, draw_window: i64, claim_window: i64) -> Option<i64> {
    if !(0..DAA_MAX).contains(&own) {
        return None;
    }
    let s = seed_ref.checked_add(draw_window)?;
    if s >= DAA_MAX {
        return None;
    }
    let d = own.max(s) + claim_window;
    (d < DAA_MAX).then_some(d)
}

// ------------------------------------------------------------------------ M4: claim()

/// Smallest non-final payout (spec M4 D57, ticket template constant; confirm by M4-c).
pub const P_MIN: i64 = 5_000_000;
/// `return_deposit_after` delay after T, 14 days at 10 BPS (D58).
pub const D_RET: i64 = 12_096_000;
/// `refresh()` idle time since the object's creation, 24 h at 10 BPS (D59).
pub const D_IDLE: i64 = 864_000;
/// Largest pool m the ticket can claim (D62, = M_DRAW).
pub const M_CLAIM: i64 = 50;
/// Largest bundle the ticket accepts (spec M4 A2, = the shard's B_MAX).
pub const TICKET_B_MAX: i64 = 4;
/// Largest per-shard count of one leaf entry (spec M4 A2, M1 COUNT_MAX).
pub const CLAIM_COUNT_MAX: i64 = 32_767;
/// Largest reserve (spec M4 A2, the controller's RESERVE_MAX).
pub const CLAIM_RESERVE_MAX: i64 = 10_000_000_000;

/// The three claim modes (D56).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClaimMode {
    /// Every other index already claimed: the ticket is consumed, payout = prize + value.
    Final,
    /// Not final, prize >= P_MIN: payout = prize, ticket' keeps its value.
    Pay,
    /// Not final, prize < P_MIN (including 0): no payout, ticket' value = value + prize.
    Accrue,
}

impl ClaimMode {
    /// Covenant outputs authorized by the ticket (A4 check 9): pool' (and ticket').
    pub fn n_cov(self) -> usize {
        if self == ClaimMode::Final { 1 } else { 2 }
    }
    /// Index of the payout output (A4 shape), if the mode has one.
    pub fn payout_output(self) -> Option<usize> {
        match self {
            ClaimMode::Final => Some(1),
            ClaimMode::Pay => Some(2),
            ClaimMode::Accrue => None,
        }
    }
    /// Outputs before the optional change output, per the A4 shape table: FINAL 2 (pool',
    /// payout), PAY 3 (pool', ticket', payout), ACCRUE 2 (pool', ticket'), i.e.
    /// nCov + [FINAL or PAY]. NOTE (S2-2, open question to the user): A4 check 11 writes
    /// nOut = nCov + [pay] + ch, which gives 1 + ch for FINAL and contradicts the table; the
    /// builder follows the table.
    pub fn n_fixed_outputs(self) -> usize {
        self.n_cov() + (self != ClaimMode::Accrue) as usize
    }
}

/// A4 check 6: the tier of combination `comb` (n1..n5, bonus) against `win` (w1..w5, wb).
/// matched = sum over i, j of [n_i = w_j]; bm = [bonus = wb]; tier 0 (5, bm), 1 (5, not
/// bm), 2 (4, bm), 3 (4, not bm), 4 (3, bm), 5 (3, not bm), 6 (2, bm); `None` for any
/// other case (the script rejects). The caller checks canonical form first (check 5).
pub fn tier_of(comb: &[u8; 6], win: &[u8; 6]) -> Option<u8> {
    let mut matched = 0;
    for i in 0..5 {
        for j in 0..5 {
            if comb[i] == win[j] {
                matched += 1;
            }
        }
    }
    let bm = comb[5] == win[5];
    match (matched, bm) {
        (5, true) => Some(0),
        (5, false) => Some(1),
        (4, true) => Some(2),
        (4, false) => Some(3),
        (3, true) => Some(4),
        (3, false) => Some(5),
        (2, true) => Some(6),
        _ => None,
    }
}

/// A4 check 8: prize = floor(slice / h), with h >= 1 (`None` = the script rejects). The
/// check prize <= remaining is the caller's (it needs the pool value).
pub fn claim_amount(slice: i64, h: i64) -> Option<i64> {
    if h < 1 {
        return None;
    }
    Some(slice / h)
}

/// A4 checks 1 (index range), 4 and 9: the mode of claiming index `idx` of a b-combination
/// ticket whose claimed byte is `claimed`, at `prize`. bit = 2^idx (the table 1, 2, 4, 8);
/// the index must be unclaimed ((c / bit) % 2 = 0); final = (c + bit = 2^b - 1), the
/// script's form of (c | bit) = 2^b - 1 once the bit is known to be clear; otherwise PAY
/// if prize >= P_MIN, else ACCRUE (D56). `None` where the script rejects (idx outside
/// [0, b), b outside [1, B_MAX], or the bit already set).
pub fn claim_mode(claimed: u8, b: i64, idx: i64, prize: i64) -> Option<ClaimMode> {
    if !(1..=TICKET_B_MAX).contains(&b) || !(0..b).contains(&idx) {
        return None;
    }
    const BIT: [i64; 4] = [1, 2, 4, 8];
    let bit = BIT[idx as usize];
    let c = claimed as i64;
    if (c / bit) % 2 != 0 {
        return None;
    }
    let full = BIT[b as usize - 1] * 2 - 1;
    if c + bit == full {
        Some(ClaimMode::Final)
    } else if prize >= P_MIN {
        Some(ClaimMode::Pay)
    } else {
        Some(ClaimMode::Accrue)
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
