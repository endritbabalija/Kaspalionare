//! Combinations of the fixed 5/30 game plus a bonus (spec M1 3.1).

/// Number of 5-number sets from 1..30.
pub const RANKS: u32 = 142_506;
pub const MAIN_MAX: u8 = 30;

/// A combination: five main numbers and a bonus, one byte each (6 bytes on chain).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Combo {
    pub main: [u8; 5],
    pub bonus: u8,
}

impl Combo {
    pub fn new(main: [u8; 5], bonus: u8) -> Self {
        Self { main, bonus }
    }

    pub fn bytes(&self) -> [u8; 6] {
        let m = self.main;
        [m[0], m[1], m[2], m[3], m[4], self.bonus]
    }

    /// Canonical: 1 <= n1 < ... < n5 <= 30 and 1 <= bonus <= m.
    pub fn is_canonical(&self, m: u8) -> bool {
        let n = self.main;
        n[0] >= 1 && n.windows(2).all(|w| w[0] < w[1]) && n[4] <= MAIN_MAX && self.bonus >= 1 && self.bonus <= m
    }

    /// Colex rank of the main numbers: sum of C(n_i - 1, i), i = 1..5.
    pub fn rank(&self) -> u32 {
        self.main.iter().enumerate().map(|(i, &n)| binom(n as u32 - 1, i as u32 + 1)).sum()
    }
}

/// C(n, k) for the small values used here.
pub fn binom(n: u32, k: u32) -> u32 {
    if k > n {
        return 0;
    }
    let mut r: u64 = 1;
    for i in 0..k as u64 {
        r = r * (n as u64 - i) / (i + 1);
    }
    r as u32
}

/// Inverse of `rank` for the main numbers.
pub fn unrank(mut rank: u32) -> [u8; 5] {
    let mut out = [0u8; 5];
    for k in (1..=5u32).rev() {
        let mut n = k - 1;
        while binom(n + 1, k) <= rank {
            n += 1;
        }
        rank -= binom(n, k);
        out[k as usize - 1] = (n + 1) as u8;
    }
    out
}

/// Concatenated bundle bytes, 6 per combination.
pub fn bundle_bytes(bundle: &[Combo]) -> Vec<u8> {
    bundle.iter().flat_map(|c| c.bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P14: rank is a bijection from canonical main-number sets onto 0..142,505.
    #[test]
    fn rank_bijection() {
        let mut seen = vec![false; RANKS as usize];
        let mut count = 0;
        for a in 1..=26u8 {
            for b in a + 1..=27 {
                for c in b + 1..=28 {
                    for d in c + 1..=29 {
                        for e in d + 1..=30 {
                            let combo = Combo::new([a, b, c, d, e], 1);
                            let r = combo.rank();
                            assert!(r < RANKS);
                            assert!(!seen[r as usize]);
                            seen[r as usize] = true;
                            assert_eq!(unrank(r), combo.main);
                            count += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(count, RANKS);
        assert_eq!(Combo::new([1, 2, 3, 4, 5], 1).rank(), 0);
        assert_eq!(Combo::new([26, 27, 28, 29, 30], 1).rank(), RANKS - 1);
    }

    #[test]
    fn canonical_rules() {
        assert!(Combo::new([1, 2, 3, 4, 5], 1).is_canonical(2));
        assert!(Combo::new([26, 27, 28, 29, 30], 255).is_canonical(255));
        assert!(!Combo::new([2, 1, 3, 4, 5], 1).is_canonical(10));
        assert!(!Combo::new([1, 1, 3, 4, 5], 1).is_canonical(10));
        assert!(!Combo::new([0, 1, 3, 4, 5], 1).is_canonical(10));
        assert!(!Combo::new([1, 2, 3, 4, 31], 1).is_canonical(10));
        assert!(!Combo::new([1, 2, 3, 4, 5], 0).is_canonical(10));
        assert!(!Combo::new([1, 2, 3, 4, 5], 11).is_canonical(10));
    }
}
