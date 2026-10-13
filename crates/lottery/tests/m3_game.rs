//! M3 reference arithmetic (spec M3 A5, A6, D39, D41, D42, D45): DP17 (J fixing against
//! brute force, monotone pass), DP18 (seed mapping, Rust side) and unit tests of the
//! `game` draw functions.
//!
//! The script halves of DP17 ("the script accepts the reference witnesses over a grid") and
//! DP18 ("Rust = script") need the real drawer (Stage 3); they go in m3_draw.rs. The seeds
//! printed by `dp18_seed_search` (rank 0, rank 142,505, bonus 1, bonus m) are the fixtures.

use lottery::combo::{Combo, RANKS, unrank};
use lottery::game::*;
use lottery::state::{ControllerState, MarkerState, TIER_JACKPOT, TIER_LOWER0, TIER_SECOND};

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
    /// Log-uniform-ish in [0, hi].
    fn mag(&mut self, hi: i64) -> i64 {
        let bits = self.range(0, 63 - hi.leading_zeros() as i64);
        self.range(0, ((1i128 << bits) - 1).min(hi as i128) as i64)
    }
}

const KAS: i64 = 100_000_000;

/// A round r+1 controller with the given J-fixing inputs; other fields are the fixture's.
fn ctrl(price: i64, m: i64, s_f: i64, s_r: i64, e: i64, g: i64, margin: i64, cap: i64) -> ControllerState {
    ControllerState {
        round: 2,
        k: 4,
        m,
        price,
        d: 13_000_000, // M6 (D80): d = D_FLOOR, was 11,000,000 (cosmetic)
        deadline: 0,
        s_f,
        s_r,
        fee_class: 0,
        fee_key: [0; 33],
        jackpot: 0,
        j_boost: 0,
        g2: 0,
        liability: 0,
        v_rec: 0,
        acc: 0,
        drawn: 0,
        shard_tpl: [0; 32],
        marker_tpl: [0; 32],
        params_tpl: [0; 32],
        reserve: 1,
        s_c: 50,
        s_d: 50,
        st: [400, 300, 300, 0, 0],
        e,
        j: 1_000,
        g,
        margin,
        m_min: 2,
        m_max: M_DRAW,
        cap,
        round_len: 0,
        d_min: 0,
        seed_offset: 0,
    }
}

fn random_ctrl(r: &mut Rng, price: i64) -> ControllerState {
    ctrl(price, r.range(2, M_DRAW), r.range(0, S_F_CAP), r.range(0, S_R_CAP), r.range(0, 5_000), r.range(0, 1_000), r.range(BP, 2 * BP), r.range(0, BP))
}

/// Small instance built through the controller (realistic E and W; the cap and the edge
/// rule usually bind).
fn small_via_ctrl(r: &mut Rng) -> (JackpotRule, i64, i64, i64) {
    let price = r.range(1, 400);
    let c = random_ctrl(r, price);
    let v = r.range(0, 40_000);
    (JackpotRule::new(&c, v), v, r.range(0, 40_000), r.range(0, 5_000))
}

/// Small instance with the three limits drawn directly, so the whale check binds too.
fn small_direct(r: &mut Rng) -> (JackpotRule, i64, i64, i64) {
    let v = r.range(0, 40_000);
    let rule = JackpotRule { e_lim: r.range(0, 60_000), w_lim: r.range(0, 60_000), cv: r.range(0, v), m: r.range(2, M_DRAW), g: r.range(0, 1_000) };
    (rule, v, r.range(0, 40_000), r.range(0, 5_000))
}

/// DP17: bisection = brute force on many small random instances; the reference satisfies
/// the A5 witness checks and every off-by-one witness fails them; all branches are reached.
#[test]
fn dp17_fix_jackpot_matches_brute_force() {
    let mut r = Rng(17);
    // [base passes, base fails (DP10), y = O' (DP8), y < O' (DP9), x2 > x1 (DP11),
    //  edge binds, whale binds, cap binds]
    let mut cov = [0u32; 8];
    for n in 0..1_200 {
        let (rule, v, o, j) = if n % 2 == 0 { small_via_ctrl(&mut r) } else { small_direct(&mut r) };
        let f = fix_jackpot(&rule, v, o, j);
        assert_eq!(f, fix_jackpot_brute(&rule, v, o, j), "{rule:?} V'={v} O'={o} j={j}");
        assert!(witness_ok(&rule, v, o, j, f.x1, f.y, f.x2), "{rule:?}");
        for (dx1, dy, dx2) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
            let (a, b, c) = (f.x1 + dx1, f.y + dy, f.x2 + dx2);
            if a >= 0 && b >= 0 && c >= 0 {
                assert!(!witness_ok(&rule, v, o, j, a, b, c), "witness ({a}, {b}, {c}) accepted; ref {f:?} {rule:?}");
            }
        }
        // The fixed values satisfy the exact inequalities; L' as check 17.
        assert!(rule.pass_exact(f.x2, f.y));
        assert_eq!(f.liability, f.x2 + f.g2 * (rule.m - 1));
        assert!(f.liability <= rule.cv, "cap on the total liability (D47)");
        if f.x1 == f.x_b { cov[0] += 1 } else { cov[1] += 1 }
        if f.y == o { cov[2] += 1 } else { cov[3] += 1 }
        if f.x2 > f.x1 { cov[4] += 1 }
        let (x, y) = (f.x2 as i128 + 1, f.y as i128);
        let l = (x + y) * rule.g as i128 / BP as i128 * (rule.m as i128 - 1);
        if x + l > rule.e_lim as i128 { cov[5] += 1 }
        if x + y + l > rule.w_lim as i128 { cov[6] += 1 }
        if x + l > rule.cv as i128 { cov[7] += 1 }
    }
    println!("DP17 coverage [base ok, base fails, y=O', y<O', top-up, edge, whale, cap]: {cov:?}");
    assert!(cov.iter().all(|&c| c >= 20), "coverage {cov:?}");
}

/// DP17 at real magnitudes (up to TOTAL_MAX): the bisection result passes the witness
/// checks, pass agrees with the exact 128-bit inequalities, and no overflow occurs.
#[test]
fn dp17_fix_jackpot_large_values() {
    let mut r = Rng(1717);
    for _ in 0..3_000 {
        let price = if r.next() % 3 == 0 { r.range(1, 100_000_000_000) } else { [KAS / 10, KAS, 10 * KAS][r.range(0, 2) as usize] };
        let c = random_ctrl(&mut r, price);
        let v = r.mag(TOTAL_MAX);
        let o = r.mag(TOTAL_MAX);
        let rule = JackpotRule::new(&c, v);
        let f = fix_jackpot(&rule, v, o, c.j);
        assert!(witness_ok(&rule, v, o, c.j, f.x1, f.y, f.x2), "{rule:?} V'={v} O'={o}");
        assert!(!witness_ok(&rule, v, o, c.j, f.x1, f.y, f.x2 + 1));
        assert!(f.x2 == 0 || !witness_ok(&rule, v, o, c.j, f.x1, f.y, f.x2 - 1));
        assert!(rule.pass_exact(f.x2, f.y) && f.liability <= rule.cv);
        for _ in 0..20 {
            let x = r.mag(rule.e_lim.max(1));
            let y = r.mag(rule.w_lim.max(1));
            assert_eq!(rule.pass(x, y), rule.pass_exact(x, y), "x={x} y={y} {rule:?}");
        }
    }
}

/// pass is non-increasing in x and in y, and pass(0, 0) holds (A5).
#[test]
fn dp17_pass_monotone() {
    let mut r = Rng(99);
    for n in 0..4_000 {
        let rule = if n % 2 == 0 { small_via_ctrl(&mut r).0 } else { small_direct(&mut r).0 };
        assert!(rule.pass(0, 0));
        for _ in 0..30 {
            let (x, y) = (r.range(0, 70_000), r.range(0, 70_000));
            if rule.pass(x, y) {
                assert!(x == 0 || rule.pass(x - 1, y), "x not monotone at {x},{y} {rule:?}");
                assert!(y == 0 || rule.pass(x, y - 1), "y not monotone at {x},{y} {rule:?}");
            } else {
                assert!(!rule.pass(x + 1, y) && !rule.pass(x, y + 1));
            }
        }
    }
}

/// Design 4.6 worked example: V' = 10,000,000 KAS, no boost, m' = 13, cap 20 %: the base
/// J_house = 1,000,000 KAS passes and the top-up raises it to where the edge rule binds.
#[test]
fn dp17_worked_example_design_4_6() {
    let c = ctrl(KAS, 13, 500, 1_000, 1_500, 100, 11_700, 2_000);
    let v = 10_000_000 * KAS;
    let rule = JackpotRule::new(&c, v);
    let f = fix_jackpot(&rule, v, 0, 1_000);
    assert_eq!((f.x_b, f.x1, f.y), (1_000_000 * KAS, 1_000_000 * KAS, 0));
    assert!(f.x2 > f.x1);
    // Design: about 1,040,421 KAS with the edge exactly 15 %.
    assert_eq!(f.x2 / KAS, 1_040_421);
    assert!(f.x2 + muldiv(f.x2, 100) * 12 <= rule.e_lim);
    assert!(!rule.pass(f.x2 + 1, 0));
    assert_eq!(f, fix_jackpot_brute_bounded(&rule, v, 0, 1_000));
}

/// Brute force is too slow at KAS magnitudes; check the top-up by a local scan instead.
fn fix_jackpot_brute_bounded(rule: &JackpotRule, v: i64, o: i64, j: i64) -> JackpotFix {
    let f = fix_jackpot(rule, v, o, j);
    for d in 1..2_000 {
        assert!(!rule.pass(f.x2 + d, f.y));
    }
    f
}

/// DP18 (Rust side): the mapping is the D42 hash, rank < N, bonus in [1, m], the main
/// numbers are canonical with the given colex rank.
#[test]
fn dp18_seed_mapping() {
    let cov = [0x4c; 32];
    let mut r = Rng(18);
    for _ in 0..2_000 {
        let mut seed = [0u8; 32];
        seed.iter_mut().for_each(|b| *b = r.next() as u8);
        let round = r.range(1, 1 << 40);
        let m = r.range(2, M_DRAW);
        let s = seed_numbers(&cov, round, &seed, m);
        // Independent recomputation of h.
        let mut msg = b"KLOT/test-seed".to_vec();
        assert_eq!(msg.len(), 14);
        msg.extend_from_slice(&cov);
        msg.extend_from_slice(&round.to_le_bytes());
        msg.extend_from_slice(&seed);
        assert_eq!(msg.len(), 86);
        let h = blake2b_simd::Params::new().hash_length(32).hash(&msg);
        assert_eq!(&s.h[..], h.as_bytes());
        assert!(s.rank < RANKS && (1..=m as u8).contains(&s.bonus));
        let combo = Combo::new(s.main, s.bonus);
        assert!(combo.is_canonical(m as u8));
        assert_eq!(combo.rank(), s.rank);
        assert_eq!(s.rank as i64, script_num(&[&s.h[0..7], &[0]].concat()) % N);
        assert_eq!(s.bonus as i64, script_num(&[&s.h[7..14], &[0]].concat()) % m + 1);
    }
}

/// OpBin2Num's decoding (rusty-kaspa v2.1.0 `crypto/txscript/src/data_stack.rs:164-178`):
/// little-endian sign-magnitude, sign in the top bit of the last byte, at most 8 bytes.
fn script_num(v: &[u8]) -> i64 {
    assert!(v.len() <= 8);
    if v.is_empty() {
        return 0;
    }
    let msb = v[v.len() - 1];
    let sign = 1 - 2 * ((msb >> 7) as i64);
    v[..v.len() - 1].iter().rev().fold((msb & 0x7f) as i64, |a, &b| (a << 8) + b as i64) * sign
}

#[test]
fn dp18_u56_matches_script_number() {
    assert_eq!(u56_le(&[0xff; 7]), (1 << 56) - 1);
    assert_eq!(u56_le(&[1, 0, 0, 0, 0, 0, 0]), 1);
    assert_eq!(u56_le(&[0, 0, 0, 0, 0, 0, 0x80]), 1 << 55);
    let mut r = Rng(56);
    for _ in 0..10_000 {
        let b: Vec<u8> = (0..7).map(|_| r.next() as u8).collect();
        assert_eq!(u56_le(&b), script_num(&[&b[..], &[0]].concat()));
    }
}

/// DP18 fixtures: test seeds for round 1, m = 10, cov = COV giving rank 0, rank 142,505,
/// bonus 1 and bonus m. Printed for the script test (Stage 3).
#[test]
fn dp18_seed_search() {
    let cov = [0x4c; 32];
    let m = 10;
    let (mut r0, mut rmax, mut b1, mut bm) = (None, None, None, None);
    for i in 0u64..3_000_000 {
        let mut seed = [0u8; 32];
        seed[..8].copy_from_slice(&i.to_le_bytes());
        let s = seed_numbers(&cov, 1, &seed, m);
        if s.rank == 0 && r0.is_none() {
            r0 = Some(i);
        }
        if s.rank == RANKS - 1 && rmax.is_none() {
            rmax = Some(i);
        }
        if s.bonus == 1 && b1.is_none() {
            b1 = Some(i);
        }
        if s.bonus == m as u8 && bm.is_none() {
            bm = Some(i);
        }
        if r0.is_some() && rmax.is_some() && b1.is_some() && bm.is_some() {
            break;
        }
    }
    println!("DP18 seeds (seed[0..8] = i LE, rest 0; cov 0x4c.., round 1, m 10): rank0 {r0:?} rankmax {rmax:?} bonus1 {b1:?} bonusm {bm:?}");
    assert!(r0.is_some() && rmax.is_some() && b1.is_some() && bm.is_some());
    assert_eq!(unrank(0), [1, 2, 3, 4, 5]);
    assert_eq!(unrank(RANKS - 1), [26, 27, 28, 29, 30]);
}

fn marker(sales: i64, ref_paid: i64, m: i64, jackpot: i64, j_boost: i64, g2: i64) -> MarkerState {
    MarkerState {
        round: 1,
        k: 4,
        roots: vec![0; 512],
        sales,
        ref_paid,
        m,
        jackpot,
        j_boost,
        g2,
        s_f: 500,
        s_r: 1_000,
        s_c: 50,
        s_d: 50,
        st: [400, 300, 300, 0, 0],
        seed_ref: 0,
        funder_class: 0,
        funder_key: [0; 33],
    }
}

#[test]
fn tier_w_and_pool_set_d45() {
    assert_eq!(tier_w(10), [125, 1_125, 3_000, 27_000, 23_000]);
    assert_eq!(tier_w(M_DRAW), [125, 125 * 49, 3_000, 3_000 * 49, 23_000]);
    assert!(tier_w(M_HARD)[3] < 1_000_000);
    let b = [400 * KAS, 300 * KAS, 300 * KAS, 0, 0];
    // Every pool: jackpot, second (n2 = 3), lower slots 0..2; slots 3, 4 have B = 0.
    let p = pool_set(2, 3, 1_000 * KAS, 10 * KAS, &b, 10);
    assert_eq!(p.np(), 5);
    assert_eq!(p.pools[0], NewPool { tier: TIER_JACKPOT, slice: 1_000 * KAS, value: 1_000 * KAS });
    assert_eq!(p.pools[1], NewPool { tier: TIER_SECOND, slice: 10 * KAS, value: 30 * KAS });
    assert_eq!(p.pools[2], NewPool { tier: TIER_LOWER0, slice: 400 * KAS / 125, value: 400 * KAS });
    assert_eq!(p.pools[3].tier, TIER_LOWER0 + 1);
    assert_eq!(p.pools[4], NewPool { tier: TIER_LOWER0 + 2, slice: 300 * KAS / 3_000, value: 300 * KAS });
    assert_eq!(p.lp, [true, true, true, false, false]);
    // Jackpot unheld, J = 0, n2 = 0, G2 = 0: no such pools.
    assert!(!pool_set(0, 3, 1_000 * KAS, 10 * KAS, &b, 10).jp);
    assert!(!pool_set(1, 3, 0, 10 * KAS, &b, 10).jp);
    assert!(!pool_set(1, 0, 1_000 * KAS, 10 * KAS, &b, 10).sp);
    assert!(!pool_set(1, 3, 1_000 * KAS, 0, &b, 10).sp);
    // Slice 0 (DP15): B_t < W_t makes no pool; B_t = W_t makes a pool of slice 1.
    let p = pool_set(0, 0, 0, 0, &[124, 1_125, 2_999, 27_000, 0], 10);
    assert_eq!(p.lp, [false, true, false, true, false]);
    assert_eq!(p.slices, [0, 1, 0, 1, 0]);
    assert_eq!(p.np(), 2);
}

/// Conservation (spec M3 A4, economic value without R): marker + vault + controller + sum Q
/// = pools + vault' + controller' + fee + C_draw, over random rounds.
#[test]
fn draw_conservation() {
    let mut r = Rng(5);
    let mut seen = [0u32; 4];
    for _ in 0..50_000 {
        let m = r.range(2, M_DRAW);
        let sales = r.mag(1 << 50);
        let ref_paid = r.range(0, muldiv(sales, 1_000));
        let jackpot = r.mag(1 << 50);
        let j_boost = r.range(0, jackpot);
        let g2 = muldiv(jackpot, r.range(0, 1_000));
        let mk = marker(sales, ref_paid, m, jackpot, j_boost, g2);
        let a = draw_amounts(&mk);
        assert!(a.b_r >= 0);
        let h_j = if r.next() % 2 == 0 { 0 } else { r.range(1, 5) };
        let n2 = r.range(0, m - 1);
        let p = pool_set(h_j, n2, jackpot, g2, &a.b, m);
        let vault = r.mag(1 << 55);
        let Some(v2) = vault_settle(vault, a.h, jackpot - j_boost, p.jp, g2, n2, p.sp) else { continue };
        let o = r.mag(1 << 50);
        let acc = r.range(0, 2 * KAS);
        let q: Vec<i64> = (0..r.range(0, A_MAX as i64)).map(|_| r.mag(1 << 45)).collect();
        let o2 = boost_after(o, a.b_r, j_boost, p.jp, &a.b, &p.lp, &q).unwrap();
        let fee = fee_split(acc, a.f, F_MIN).unwrap();
        let ins = (sales - ref_paid - a.c_close + j_boost) as i128 + vault as i128 + (o + acc) as i128 + q.iter().map(|&x| x as i128).sum::<i128>();
        let pools: i128 = p.pools.iter().map(|x| x.value as i128).sum();
        let outs = pools + v2 as i128 + (o2 + fee.a_next) as i128 + if fee.paid { fee.t_f as i128 } else { 0 } + a.c_draw as i128;
        assert_eq!(ins, outs);
        seen[p.jp as usize] += 1;
        seen[2 + fee.paid as usize] += 1;
    }
    assert!(seen.iter().all(|&s| s > 1_000), "{seen:?}");
}

#[test]
fn vault_and_boost_examples() {
    // DP2: jackpot held: the vault pays J_house, J_boost does not return to the pot.
    assert_eq!(vault_settle(1_000, 100, 300, true, 10, 2, true), Some(780));
    // DP4: unheld: J_house stays; J_boost goes to O'.
    assert_eq!(vault_settle(1_000, 100, 300, false, 10, 0, false), Some(1_100));
    assert_eq!(boost_after(50, 5, 7, false, &[1, 2, 3, 4, 5], &[true, false, true, false, true], &[11, 13]), Some(50 + 5 + 7 + 2 + 4 + 24));
    assert_eq!(boost_after(50, 5, 7, true, &[1, 2, 3, 4, 5], &[true; 5], &[]), Some(55));
    // Overdrawn vault and guard failures reject.
    assert_eq!(vault_settle(10, 0, 11, true, 0, 0, false), None);
    assert_eq!(vault_settle(TOTAL_MAX, 1, 0, false, 0, 0, false), None);
    assert_eq!(boost_after(TOTAL_MAX, 1, 0, true, &[0; 5], &[true; 5], &[]), None);
}

#[test]
fn fee_split_d41() {
    // DP12: below F_MIN the fee is carried.
    assert_eq!(fee_split(10, F_MIN - 11, F_MIN), Some(FeeSplit { t_f: F_MIN - 1, paid: false, a_next: F_MIN - 1 }));
    // DP13: at and above F_MIN it is paid and A' = 0.
    assert_eq!(fee_split(10, F_MIN - 10, F_MIN), Some(FeeSplit { t_f: F_MIN, paid: true, a_next: 0 }));
    assert_eq!(fee_split(KAS, 5 * KAS, F_MIN), Some(FeeSplit { t_f: 6 * KAS, paid: true, a_next: 0 }));
    assert_eq!(fee_split(TOTAL_MAX, 1, F_MIN), None);
}

#[test]
fn claim_deadline_d39() {
    // DP19: a fresh funding input: own + CLAIM_WINDOW.
    let seed_ref = 1_000_600;
    let own = seed_ref + DRAW_WINDOW + 5_000;
    assert_eq!(claim_deadline(own, seed_ref, DRAW_WINDOW, CLAIM_WINDOW), Some(own + CLAIM_WINDOW));
    // Only old inputs: seed_ref + DRAW_WINDOW + CLAIM_WINDOW.
    assert_eq!(claim_deadline(seed_ref - 10, seed_ref, DRAW_WINDOW, CLAIM_WINDOW), Some(seed_ref + DRAW_WINDOW + CLAIM_WINDOW));
    // A6 bounds.
    assert_eq!(claim_deadline(DAA_MAX, 0, 0, 0), None);
    assert_eq!(claim_deadline(0, DAA_MAX - DRAW_WINDOW, DRAW_WINDOW, 0), None);
    assert_eq!(claim_deadline(DAA_MAX - 1, 0, 0, 1), None);
    assert_eq!(claim_deadline(DAA_MAX - 2, 0, 0, 1), Some(DAA_MAX - 1));
    // Constants (D39): 7 days and 12 h at 10 BPS.
    assert_eq!(CLAIM_WINDOW, 7 * 24 * 3600 * 10);
    assert_eq!(DRAW_WINDOW, 12 * 3600 * 10);
}

#[test]
fn win_counts_sums_over_shards() {
    // m = 4, bonus 2: shard counts per bonus value 1..4.
    let counts = vec![vec![0, 1, 0, 0], vec![0, 2, 0, 3], vec![0, 0, 0, 0]];
    assert_eq!(win_counts(&counts, 2, 4), (3, 1));
    assert_eq!(win_counts(&counts, 4, 4), (3, 1));
    assert_eq!(win_counts(&counts, 1, 4), (0, 2));
    assert_eq!(win_counts(&[vec![0; 4]], 1, 4), (0, 0));
}
