//! M4 Rust units (spec M4 A9 KP18, Rust half): `tier_of`, `claim_amount` and `claim_mode`
//! against independent brute-force references, and the conservation of `ClaimExpected` over
//! random fabricated claims. The script half of KP18 (the script accepts the reference
//! outputs over a grid) is `kp18_script_accepts_reference_grid` in m4_claim.rs.

use std::collections::HashSet;

use lottery::claim::{ClaimCase, combo_matching, tier_shape};
use lottery::combo::{Combo, RANKS, unrank};
use lottery::game::{ClaimMode, P_MIN, TICKET_B_MAX, TOTAL_MAX, claim_amount, claim_mode, tier_of};

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
        lo + (self.next() % ((hi - lo) as u64 + 1)) as i64
    }
}

/// Brute-force tier: set intersection of the main numbers and the design's prize table
/// (design 4.1: 5+b, 5, 4+b, 4, 3+b, 3, 2+b; anything else wins nothing).
fn tier_ref(comb: &[u8; 6], win: &[u8; 6]) -> Option<u8> {
    let a: HashSet<u8> = comb[..5].iter().copied().collect();
    let b: HashSet<u8> = win[..5].iter().copied().collect();
    let matched = a.intersection(&b).count();
    let bm = comb[5] == win[5];
    let table = [(5, true), (5, false), (4, true), (4, false), (3, true), (3, false), (2, true)];
    table.iter().position(|&s| s == (matched, bm)).map(|p| p as u8)
}

fn random_combo(r: &mut Rng, m: i64) -> Combo {
    Combo::new(unrank(r.range(0, RANKS as i64 - 1) as u32), r.range(1, m) as u8)
}

/// KP18: every (matched, bm) pair (12 pairs, several combinations each, several m), then
/// random canonical combinations against random winning numbers.
#[test]
fn kp18_tier_of_matches_brute_force() {
    let mut r = Rng(41);
    let mut seen = HashSet::new();
    for m in [2i64, 3, 10, 50] {
        for _ in 0..50 {
            let w = random_combo(&mut r, m);
            let win = w.bytes();
            for matched in 0..=5usize {
                for bm in [true, false] {
                    for v in 0..5 {
                        let c = combo_matching(&win, matched, bm, m, v);
                        assert!(c.is_canonical(m as u8));
                        let got = tier_of(&c.bytes(), &win);
                        assert_eq!(got, tier_ref(&c.bytes(), &win), "m {m} matched {matched} bm {bm} {c:?} vs {win:?}");
                        seen.insert((matched, bm, got));
                    }
                }
            }
        }
    }
    // All 12 pairs, of which exactly 7 win (tier 0..6) and 5 do not.
    assert_eq!(seen.len(), 12);
    assert_eq!(seen.iter().filter(|s| s.2.is_some()).count(), 7);
    for tier in 0..=6u8 {
        let (matched, bm) = tier_shape(tier);
        assert!(seen.contains(&(matched, bm, Some(tier))));
    }
    let mut counts = [0u32; 8];
    for _ in 0..300_000 {
        let m = r.range(2, 50);
        let (c, w) = (random_combo(&mut r, m), random_combo(&mut r, m));
        let got = tier_of(&c.bytes(), &w.bytes());
        assert_eq!(got, tier_ref(&c.bytes(), &w.bytes()), "{c:?} vs {w:?}");
        counts[got.map_or(7, |t| t as usize)] += 1;
    }
    println!("random tier counts [0..6, none]: {counts:?}");
    assert!(counts[3..].iter().all(|&n| n > 0), "random sampling reaches tiers 3..6 and none: {counts:?}");
}

/// KP18: claim_amount = floor(slice / h) in exact arithmetic, None for h < 1.
#[test]
fn kp18_claim_amount_matches_brute_force() {
    let mut r = Rng(42);
    for (slice, h) in [(0, 1), (1, 2), (P_MIN, 1), (P_MIN - 1, 1), (TOTAL_MAX, 1), (TOTAL_MAX, 3), (7, 7), (6, 7)] {
        assert_eq!(claim_amount(slice, h), Some((slice as i128 / h as i128) as i64));
    }
    assert_eq!(claim_amount(100, 0), None);
    assert_eq!(claim_amount(100, -1), None);
    for _ in 0..200_000 {
        let slice = r.range(0, TOTAL_MAX);
        let h = r.range(1, 16 * 32_767);
        let p = claim_amount(slice, h).unwrap();
        assert_eq!(p as i128, slice as i128 / h as i128);
        // Holders of one combination receive at most the slice in total (solvency, 4.4).
        assert!(p as i128 * h as i128 <= slice as i128);
    }
}

/// Reference mode with the bit-OR form of D56: final = (claimed | 2^idx) = 2^b - 1.
fn mode_ref(claimed: u8, b: i64, idx: i64, prize: i64) -> Option<ClaimMode> {
    if !(1..=TICKET_B_MAX).contains(&b) || idx < 0 || idx >= b {
        return None;
    }
    let bit = 1u32 << idx;
    if claimed as u32 & bit != 0 {
        return None;
    }
    if (claimed as u32 | bit) == (1u32 << b) - 1 {
        Some(ClaimMode::Final)
    } else if prize >= P_MIN {
        Some(ClaimMode::Pay)
    } else {
        Some(ClaimMode::Accrue)
    }
}

/// KP18: claim_mode over every claimed byte, b in -1..=5, idx in -2..=5 and prizes around
/// P_MIN.
#[test]
fn kp18_claim_mode_matches_brute_force() {
    let mut modes = HashSet::new();
    for claimed in 0..=255u8 {
        for b in -1..=5i64 {
            for idx in -2..=5i64 {
                for prize in [0, 1, P_MIN - 1, P_MIN, P_MIN + 1, TOTAL_MAX] {
                    let got = claim_mode(claimed, b, idx, prize);
                    assert_eq!(got, mode_ref(claimed, b, idx, prize), "claimed {claimed:#010b} b {b} idx {idx} prize {prize}");
                    modes.insert(got);
                }
            }
        }
    }
    assert_eq!(modes.len(), 4, "None, FINAL, PAY and ACCRUE all occur");
    // D56 examples.
    assert_eq!(claim_mode(0, 1, 0, 0), Some(ClaimMode::Final));
    assert_eq!(claim_mode(0b10, 2, 0, 1), Some(ClaimMode::Final));
    assert_eq!(claim_mode(0, 2, 0, P_MIN), Some(ClaimMode::Pay));
    assert_eq!(claim_mode(0, 2, 0, P_MIN - 1), Some(ClaimMode::Accrue));
    assert_eq!(claim_mode(0b01, 2, 0, P_MIN), None);
    assert_eq!(ClaimMode::Final.n_cov(), 1);
    assert_eq!((ClaimMode::Pay.n_cov(), ClaimMode::Pay.payout_output()), (2, Some(2)));
    assert_eq!((ClaimMode::Accrue.n_cov(), ClaimMode::Accrue.payout_output()), (2, None));
}

/// KP18 / A4 conservation on the Rust side: random fabricated claims (tier, slice, holders,
/// claimed bits, change) give reference outputs with pool + ticket + funding = outputs +
/// fee, pool' = pool - prize, prize = floor(slice / h), and the mode of `claim_mode`.
#[test]
fn kp18_random_claims_conserve_value() {
    let mut r = Rng(43);
    let mut modes = HashSet::new();
    for n in 0..300 {
        let k = r.range(1, 4) as usize;
        let m = [4, 10, 50][n % 3];
        let tier = r.range(0, 6) as u8;
        let holders: Vec<u16> = (0..k).map(|i| if i == 0 { r.range(1, 3) as u16 } else { r.range(0, 2) as u16 }).collect();
        let slice = [0, 1, P_MIN - 1, P_MIN, 3 * P_MIN, r.range(0, 1_000 * P_MIN)][r.range(0, 5) as usize];
        let mut c = ClaimCase::fabricated(k, m, tier, slice, &holders);
        if r.next() % 3 == 0 {
            c.make_final();
        }
        c.change = r.next() % 4 != 0;
        if !c.change {
            c.funding[0].value = c.fee;
        }
        let x = c.expected();
        assert!(x.rejects.is_empty(), "{:?}", x.rejects);
        let h: i64 = holders.iter().map(|&n| n as i64).sum();
        assert_eq!(x.h, h, "k {k} m {m} tier {tier} holders {holders:?} comb {:?} counts {:?}", x.comb, x.counts);
        assert_eq!(x.prize, slice / h);
        assert_eq!(Some(x.mode), claim_mode(c.ticket.claimed[0], c.ticket.b, c.idx, x.prize));
        assert_eq!(x.values[0], c.pool_value - x.prize as u64);
        let ins = c.pool_value as u128 + c.ticket_value as u128 + c.funding_total() as u128;
        let outs: u128 = x.values.iter().map(|&v| v as u128).sum();
        assert_eq!(ins, outs + c.fee as u128, "conservation");
        assert_eq!(x.values.len(), x.mode.n_fixed_outputs() + c.change as usize);
        modes.insert(x.mode);
    }
    assert_eq!(modes.len(), 3, "all three modes occur");
}
