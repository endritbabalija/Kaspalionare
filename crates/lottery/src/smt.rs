//! Off-chain model of one shard's counter tree (spec M1 section 3).
//!
//! Depth-18 sparse Merkle tree keyed by the colex rank of the five main numbers.
//! Leaf  = H(0x00 || rank as 3-byte script number || m counts, 2-byte script numbers)
//! Node  = H(0x01 || left || right)
//! Empty = Z_0 = H(0x02); an all-zero leaf is always Z_0 (canonical empty rule).
//! H = BLAKE2b-256. Bit i of the rank (LSB first) is 1 when the running node is the right child at level i.

use std::collections::HashMap;

pub const DEPTH: usize = 18;
pub const COUNT_MAX: u16 = 32_767;
pub const PATH_BYTES: usize = 32 * DEPTH;

pub type Node = [u8; 32];

pub fn h(data: &[u8]) -> Node {
    let out = blake2b_simd::Params::new().hash_length(32).hash(data);
    out.as_bytes().try_into().unwrap()
}

/// Z_0 .. Z_DEPTH: the hash of an empty subtree at each level.
pub fn empty_hashes() -> [Node; DEPTH + 1] {
    let mut z = [[0u8; 32]; DEPTH + 1];
    z[0] = h(&[0x02]);
    for i in 0..DEPTH {
        z[i + 1] = node(&z[i], &z[i]);
    }
    z
}

pub fn node(l: &Node, r: &Node) -> Node {
    let mut b = Vec::with_capacity(65);
    b.push(0x01);
    b.extend_from_slice(l);
    b.extend_from_slice(r);
    h(&b)
}

/// Little-endian script-number bytes of a non-negative value, zero-padded to `n` bytes
/// (what SilverScript's `x as byte[n]` produces when the value fits with the sign bit clear).
pub fn num_bytes(v: u64, n: usize) -> Vec<u8> {
    assert!(n == 8 || v < 1u64 << (8 * n - 1), "value {v} does not fit {n} script-number bytes");
    v.to_le_bytes()[..n].to_vec()
}

pub fn counts_bytes(counts: &[u16]) -> Vec<u8> {
    counts.iter().flat_map(|&c| num_bytes(c as u64, 2)).collect()
}

pub fn leaf_hash(rank: u32, counts: &[u16]) -> Node {
    if counts.iter().all(|&c| c == 0) {
        return empty_hashes()[0];
    }
    let mut b = vec![0x00];
    b.extend(num_bytes(rank as u64, 3));
    b.extend(counts_bytes(counts));
    h(&b)
}

/// Fold a leaf up a flat sibling string to the root.
pub fn root_from_path(rank: u32, leaf: Node, siblings: &[u8]) -> Node {
    assert_eq!(siblings.len(), PATH_BYTES);
    let mut cur = leaf;
    for i in 0..DEPTH {
        let s: Node = siblings[32 * i..32 * (i + 1)].try_into().unwrap();
        cur = if (rank >> i) & 1 == 0 { node(&cur, &s) } else { node(&s, &cur) };
    }
    cur
}

/// The K count proofs of `draw()` at one rank (spec M3 A4 check 9): `leaves` is the K
/// count strings (2m bytes each, shard order), `siblings` the K sibling strings
/// (PATH_BYTES = 576 bytes each, shard order).
pub fn proofs_at(trees: &[Tree], rank: u32) -> (Vec<u8>, Vec<u8>) {
    let mut leaves = Vec::new();
    let mut siblings = Vec::new();
    for t in trees {
        leaves.extend(t.counts_bytes(rank));
        siblings.extend(t.siblings(rank));
    }
    (leaves, siblings)
}

/// One shard's tree for a round with bonus range `m`.
#[derive(Clone)]
pub struct Tree {
    pub m: usize,
    leaves: HashMap<u32, Vec<u16>>,
    /// Non-empty nodes by (level, index); level 0 are leaves.
    nodes: HashMap<(usize, u32), Node>,
    z: [Node; DEPTH + 1],
}

impl Tree {
    pub fn new(m: usize) -> Self {
        Self { m, leaves: HashMap::new(), nodes: HashMap::new(), z: empty_hashes() }
    }

    pub fn root(&self) -> Node {
        self.get(DEPTH, 0)
    }

    fn get(&self, level: usize, idx: u32) -> Node {
        self.nodes.get(&(level, idx)).copied().unwrap_or(self.z[level])
    }

    /// Current counts of a key (all zero when absent).
    pub fn counts(&self, rank: u32) -> Vec<u16> {
        self.leaves.get(&rank).cloned().unwrap_or_else(|| vec![0; self.m])
    }

    pub fn counts_bytes(&self, rank: u32) -> Vec<u8> {
        counts_bytes(&self.counts(rank))
    }

    /// Flat sibling string for a key, bottom level first.
    pub fn siblings(&self, rank: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(PATH_BYTES);
        let mut idx = rank;
        for level in 0..DEPTH {
            out.extend_from_slice(&self.get(level, idx ^ 1));
            idx >>= 1;
        }
        out
    }

    /// Set a key's counts (used to preload trees for boundary tests).
    pub fn set(&mut self, rank: u32, counts: Vec<u16>) {
        assert_eq!(counts.len(), self.m);
        let leaf = leaf_hash(rank, &counts);
        if counts.iter().all(|&c| c == 0) {
            self.leaves.remove(&rank);
        } else {
            self.leaves.insert(rank, counts);
        }
        let mut idx = rank;
        let mut cur = leaf;
        for level in 0..DEPTH {
            if cur == self.z[level] {
                self.nodes.remove(&(level, idx));
            } else {
                self.nodes.insert((level, idx), cur);
            }
            let sib = self.get(level, idx ^ 1);
            cur = if idx & 1 == 0 { node(&cur, &sib) } else { node(&sib, &cur) };
            idx >>= 1;
        }
        if cur == self.z[DEPTH] {
            self.nodes.remove(&(DEPTH, 0));
        } else {
            self.nodes.insert((DEPTH, 0), cur);
        }
    }

    /// Increment the count of `bonus` (1-based) under `rank`.
    pub fn increment(&mut self, rank: u32, bonus: u8) {
        let mut c = self.counts(rank);
        let slot = &mut c[bonus as usize - 1];
        assert!(*slot < COUNT_MAX, "count overflow");
        *slot += 1;
        self.set(rank, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_chain_and_root() {
        let z = empty_hashes();
        assert_eq!(z[0], h(&[0x02]));
        for i in 0..DEPTH {
            assert_eq!(z[i + 1], node(&z[i], &z[i]));
        }
        assert_eq!(Tree::new(10).root(), z[DEPTH]);
    }

    #[test]
    fn proof_round_trip_and_update() {
        let mut t = Tree::new(10);
        for (rank, bonus) in [(0u32, 1u8), (142_505, 10), (77_777, 3), (77_776, 3), (77_777, 4)] {
            let before = t.root();
            let old = leaf_hash(rank, &t.counts(rank));
            let sib = t.siblings(rank);
            assert_eq!(root_from_path(rank, old, &sib), before);
            t.increment(rank, bonus);
            let new = leaf_hash(rank, &t.counts(rank));
            // The same path gives the new root (siblings are unchanged by this key's update).
            assert_eq!(root_from_path(rank, new, &sib), t.root());
        }
    }

    #[test]
    fn canonical_empty_rule() {
        let z = empty_hashes();
        assert_eq!(leaf_hash(5, &[0; 10]), z[0]);
        assert_ne!(leaf_hash(5, &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0]), z[0]);
        // Setting a key back to all zero restores the empty root.
        let mut t = Tree::new(3);
        t.set(9, vec![0, 2, 0]);
        assert_ne!(t.root(), z[DEPTH]);
        t.set(9, vec![0, 0, 0]);
        assert_eq!(t.root(), z[DEPTH]);
    }

    #[test]
    fn count_encoding() {
        assert_eq!(num_bytes(32_767, 2), vec![0xff, 0x7f]);
        assert_eq!(num_bytes(142_505, 3), vec![0xa9, 0x2c, 0x02]);
    }
}
