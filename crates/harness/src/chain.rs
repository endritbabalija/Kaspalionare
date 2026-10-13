//! `ChainSim`: a fabricated selected chain for the local script engine (M7 spec A7).
//!
//! `OpChainblockSeqCommit` (refs/rusty-kaspa-v2.1.0 `crypto/txscript/src/opcodes/mod.rs:1389-1404`)
//! consults a `kaspa_txscript::SeqCommitAccessor` (`crypto/txscript/src/seq_commit_accessor.rs:3-6`).
//! Consensus answers it from the reachability and header stores of the validating block's
//! selected parent (`consensus/src/model/services/seq_commit_accessor.rs:31-45`); `ChainSim`
//! answers it from a small set of fabricated blocks:
//! - `is_chain_ancestor_from_pov(h)`: `None` if `h` is unknown (the opcode then fails with
//!   `BlockAlreadyPruned`), `Some(false)` if `h` is known but not on the chain (`BlockNotSelected`),
//!   `Some(true)` if it is on the chain;
//! - `seq_commitment_within_depth(h)`: the block's commitment iff `blue_score + f > pov_blue_score`,
//!   the consensus rule `seq_commit_within_threshold` (`seq_commit_accessor.rs:8-10`), else `None`
//!   (`BlockIsTooDeep`).
//!
//! The chain set and the point of view are independent knobs: `set_pov` only moves the depth
//! threshold; `reorg_to` replaces the chain set. To model a point of view whose selected chain
//! ends below the current tip, call `reorg_to` with the earlier block.
//!
//! Commitments follow KIP-21 (`refs/kips/kip-0021.md` §6.5–6.7;
//! `consensus/seq-commit/src/hashing.rs:80-84, 153-173`):
//! `SeqCommit(B) = H_seq(SeqCommit(sp(B)) ‖ H_seq(activity_root ‖ H_seq(H_mc(ts ‖ daa ‖ bs) ‖ mpr)))`
//! with `H_seq` = `kaspa_hashes::SeqCommitMerkleBranch` and `H_mc` = `kaspa_hashes::SeqCommitMergesetContext`
//! (`crypto/hashes/src/hashers.rs:40, 47`). The sim's root blocks have parent `ZERO_HASH`, whose
//! commitment is `ZERO_HASH` (`SeqCommit(genesis)` is `ZERO_HASH`, `kip-0021.md:365`). The activity
//! root and miner payload root are opaque 32-byte values here (deterministic pseudo-random); the
//! script verifies them only as part of the preimage.

use std::collections::{HashMap, HashSet};

use kaspa_consensus_core::config::params::MAINNET_PARAMS;
use kaspa_hashes::{Hash, HasherBase, SeqCommitMergesetContext, SeqCommitMerkleBranch, ZERO_HASH};

/// Bytes of one block's fields in the anchor proof (spec A3).
pub const BLOCK_PROOF_LEN: usize = 152;
/// Bytes of a block's commitment preimage fields (the proof fields without the hash):
/// parent_commit, activity_root, miner_payload_root, sp_timestamp, daa_score, blue_score.
pub const PREIMAGE_LEN: usize = BLOCK_PROOF_LEN - 32;
/// Anchor proof length: X's fields (152) then P's preimage fields without P.hash (120), 272 B
/// (spec A3, D95; M7 Stage 5 fix B: the drawer authenticates P only through
/// `SeqCommit(P) == X.parent_commit`, so P.hash is not part of the proof and no byte of the
/// argument is unconstrained).
pub const ANCHOR_LEN: usize = BLOCK_PROOF_LEN + PREIMAGE_LEN;

/// 32-byte BLAKE3 key of `H_seq`: "SeqCommitmentMerkleBranchHash" zero-padded (spec A2 K_SEQ;
/// `crypto/hashes/src/hashers.rs:40, 120-138`).
pub const K_SEQ: [u8; 32] = pad_key(b"SeqCommitmentMerkleBranchHash");
/// 32-byte BLAKE3 key of `H_mc`: "SeqCommitMergesetContext" zero-padded (spec A2 K_MC; `hashers.rs:47`).
pub const K_MC: [u8; 32] = pad_key(b"SeqCommitMergesetContext");

const fn pad_key(tag: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    let mut i = 0;
    while i < tag.len() {
        key[i] = tag[i];
        i += 1;
    }
    key
}

/// Mainnet finality depth F in blue-score units: `Params::finality_depth()` returns
/// `blockrate.finality_depth` (`consensus/core/src/config/params.rs:424-426`), which is
/// BPS × FINALITY_DURATION = 10 × 43,200 = 432,000 (`bps.rs:92-93`, `constants.rs:70`).
/// Consensus passes it as the accessor threshold (`utxo_validation.rs:373, 445`).
pub fn default_finality_depth() -> u64 {
    MAINNET_PARAMS.finality_depth()
}

/// One fabricated block: the fields of the KIP-21 preimage plus its selected parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimBlock {
    pub hash: Hash,
    /// Selected parent (`ZERO_HASH` for a root of the sim).
    pub parent: Hash,
    pub blue_score: u64,
    pub daa_score: u64,
    /// Timestamp of the selected parent (the mergeset context timestamp, KIP-21 §6.6).
    pub sp_timestamp: u64,
    pub activity_root: Hash,
    pub miner_payload_root: Hash,
}

/// `H_mc(le64(ts) ‖ le64(daa) ‖ le64(bs))` (`hashing.rs:80-84`).
pub fn mergeset_context(sp_timestamp: u64, daa_score: u64, blue_score: u64) -> Hash {
    let mut h = SeqCommitMergesetContext::new();
    h.update(sp_timestamp.to_le_bytes()).update(daa_score.to_le_bytes()).update(blue_score.to_le_bytes());
    h.finalize()
}

fn h_seq(a: &Hash, b: &Hash) -> Hash {
    let mut h = SeqCommitMerkleBranch::new();
    h.update(a).update(b);
    h.finalize()
}

/// The KIP-21 commitment of `b` given its parent's commitment (`hashing.rs:153-173`).
pub fn seq_commit_with_parent(parent_commit: &Hash, b: &SimBlock) -> Hash {
    let ctx = mergeset_context(b.sp_timestamp, b.daa_score, b.blue_score);
    let pcd = h_seq(&ctx, &b.miner_payload_root);
    let state_root = h_seq(&b.activity_root, &pcd);
    h_seq(parent_commit, &state_root)
}

/// The 152-byte proof fields of `b` (spec A3): hash ‖ parent_commit ‖ activity_root ‖
/// miner_payload_root ‖ le64(sp_timestamp) ‖ le64(daa_score) ‖ le64(blue_score).
pub fn block_fields_bytes(b: &SimBlock, parent_commit: &Hash) -> [u8; BLOCK_PROOF_LEN] {
    let mut out = [0u8; BLOCK_PROOF_LEN];
    out[0..32].copy_from_slice(&b.hash.as_bytes());
    out[32..64].copy_from_slice(&parent_commit.as_bytes());
    out[64..96].copy_from_slice(&b.activity_root.as_bytes());
    out[96..128].copy_from_slice(&b.miner_payload_root.as_bytes());
    out[128..136].copy_from_slice(&b.sp_timestamp.to_le_bytes());
    out[136..144].copy_from_slice(&b.daa_score.to_le_bytes());
    out[144..152].copy_from_slice(&b.blue_score.to_le_bytes());
    out
}

/// A fabricated block DAG with a selected chain, a point of view and a finality depth.
/// Immutable once built, `Send + Sync`; share it with `Arc<ChainSim>`.
#[derive(Debug, Clone)]
pub struct ChainSim {
    blocks: HashMap<Hash, SimBlock>,
    /// Commitment of every known block, computed at insertion (parents are inserted first).
    commits: HashMap<Hash, Hash>,
    /// The selected chain of the point of view (ancestor-closed).
    chain: HashSet<Hash>,
    /// Last block of the chain.
    tip: Hash,
    pov_blue_score: u64,
    f: u64,
    /// Counter for deterministic fabricated hashes and roots.
    nonce: u64,
}

const TS_BASE: u64 = 1_700_000_000_000;

impl ChainSim {
    fn empty(f: u64) -> Self {
        Self {
            blocks: HashMap::new(),
            commits: HashMap::new(),
            chain: HashSet::new(),
            tip: ZERO_HASH,
            pov_blue_score: 0,
            f,
            nonce: 0,
        }
    }

    fn fresh(&mut self, tag: &[u8]) -> Hash {
        self.nonce += 1;
        let mut h = blake3::Hasher::new();
        h.update(b"KLOT/ChainSim/").update(tag).update(&self.nonce.to_le_bytes());
        Hash::from_bytes(*h.finalize().as_bytes())
    }

    /// Insert a block whose parent is known (or `ZERO_HASH`); computes its commitment.
    /// Panics on a duplicate hash or an unknown parent.
    pub fn insert(&mut self, b: SimBlock) {
        assert!(!self.blocks.contains_key(&b.hash), "ChainSim: duplicate block hash {}", b.hash);
        let parent_commit = self.parent_commit(&b);
        self.commits.insert(b.hash, seq_commit_with_parent(&parent_commit, &b));
        self.blocks.insert(b.hash, b);
    }

    fn make_block(&mut self, hash: Option<Hash>, parent: Hash, daa_score: u64, blue_score: u64) -> SimBlock {
        let hash = hash.unwrap_or_else(|| self.fresh(b"hash"));
        let sp_timestamp = match self.blocks.get(&parent) {
            Some(p) => TS_BASE + 100 * p.daa_score,
            None => TS_BASE,
        };
        let activity_root = self.fresh(b"activity_root");
        let miner_payload_root = self.fresh(b"miner_payload_root");
        SimBlock { hash, parent, blue_score, daa_score, sp_timestamp, activity_root, miner_payload_root }
    }

    /// A straight chain of `n ≥ 1` blocks: the first has parent `ZERO_HASH`, DAA score
    /// `daa_start` and blue score `blue_start`; each next block adds 1 to both.
    /// The point of view is the tip's blue score; F is the mainnet finality depth.
    pub fn line(daa_start: u64, blue_start: u64, n: usize) -> Self {
        assert!(n >= 1, "ChainSim::line needs at least one block");
        let mut sim = Self::empty(default_finality_depth());
        let root = sim.make_block(None, ZERO_HASH, daa_start, blue_start);
        sim.insert(root);
        sim.chain.insert(root.hash);
        sim.tip = root.hash;
        sim.pov_blue_score = blue_start;
        sim.extend(n - 1);
        sim
    }

    /// The minimal chain for fixtures that need "some valid anchor with this hash" for
    /// reference `d ≥ 1`: P (root, DAA d − 1, blue score d − 1) and X (`x_hash`, DAA d,
    /// blue score d), both on the chain, point of view = X's blue score (in the window).
    pub fn anchored(d: u64, x_hash: Hash) -> Self {
        assert!(d >= 1, "ChainSim::anchored needs d >= 1");
        let mut sim = Self::line(d - 1, d - 1, 1);
        sim.push_chain_with_hash(Some(x_hash), 1, 1);
        sim
    }

    /// A chain of real blocks, oldest first, each the selected parent of the next, for
    /// replaying a real anchor in the local engine (tn10 `draw-m7`, spec M7 D103). The first
    /// block's selected parent is outside the sim: only its commitment `root_parent_commit`
    /// is known (as KIP-21 commits it in the first block), so the accessor reports that parent
    /// as unknown, as for any pruned block. Every block is on the chain; the point of view is
    /// the last block's blue score. Panics if a block's parent is not the previous block.
    pub fn from_real_chain(root_parent_commit: Hash, blocks: &[SimBlock]) -> Self {
        assert!(!blocks.is_empty(), "ChainSim::from_real_chain needs at least one block");
        let mut sim = Self::empty(default_finality_depth());
        assert!(blocks[0].parent != ZERO_HASH, "the first block's parent must be a real (external) hash");
        sim.commits.insert(blocks[0].parent, root_parent_commit);
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 {
                assert_eq!(b.parent, blocks[i - 1].hash, "ChainSim::from_real_chain: block {i}'s parent is not block {}", i - 1);
            }
            sim.insert(*b);
            sim.chain.insert(b.hash);
        }
        let last = blocks[blocks.len() - 1];
        sim.tip = last.hash;
        sim.pov_blue_score = last.blue_score;
        sim
    }

    /// Append one chain block on the tip with DAA score `tip.daa + daa_delta` and blue score
    /// `tip.blue + blue_delta` (`blue_delta ≥ 1`); `hash` = `None` fabricates one. Moves the
    /// point of view to the new tip. Returns the new block's hash.
    pub fn push_chain_with_hash(&mut self, hash: Option<Hash>, daa_delta: u64, blue_delta: u64) -> Hash {
        assert!(blue_delta >= 1, "blue score strictly increases along the chain");
        let tip = self.blocks[&self.tip];
        let b = self.make_block(hash, tip.hash, tip.daa_score + daa_delta, tip.blue_score + blue_delta);
        self.insert(b);
        self.chain.insert(b.hash);
        self.tip = b.hash;
        self.pov_blue_score = b.blue_score;
        b.hash
    }

    /// Append one chain block (`daa_delta` may be 0: equal-DAA chain blocks; consensus bounds
    /// it by the mergeset size, MERGESET_MAX = 248 at 10 BPS).
    pub fn push_chain(&mut self, daa_delta: u64, blue_delta: u64) -> Hash {
        self.push_chain_with_hash(None, daa_delta, blue_delta)
    }

    /// Append `n` chain blocks, each +1 DAA and +1 blue score. Moves the point of view to the tip.
    pub fn extend(&mut self, n: usize) -> &mut Self {
        for _ in 0..n {
            self.push_chain(1, 1);
        }
        self
    }

    /// Fluent: append one chain block whose DAA score jumps by `delta` (blue score +1).
    pub fn with_jump(mut self, delta: u64) -> Self {
        self.push_chain(delta, 1);
        self
    }

    /// Fluent: append `n` ordinary chain blocks.
    pub fn with_extend(mut self, n: usize) -> Self {
        self.extend(n);
        self
    }

    /// Add a known block off the chain with selected parent `parent` and DAA score `daa`
    /// (blue score = parent's + 1). Returns its hash.
    pub fn add_side_block(&mut self, parent: Hash, daa: u64) -> Hash {
        let bs = self.blocks.get(&parent).unwrap_or_else(|| panic!("ChainSim: unknown parent {parent}")).blue_score + 1;
        let b = self.make_block(None, parent, daa, bs);
        self.insert(b);
        b.hash
    }

    /// Fluent form of `add_side_block` (drops the hash; use `add_side_block` when it is needed).
    pub fn with_side_block(mut self, parent: Hash, daa: u64) -> Self {
        self.add_side_block(parent, daa);
        self
    }

    /// Add `n ≥ 1` known blocks off the chain descending from `from` (+1 DAA, +1 blue score
    /// each). Returns the last one's hash (a candidate for `reorg_to`).
    pub fn fork(&mut self, from: Hash, n: usize) -> Hash {
        assert!(n >= 1);
        let mut parent = from;
        for _ in 0..n {
            let p = *self.blocks.get(&parent).unwrap_or_else(|| panic!("ChainSim: unknown parent {parent}"));
            let b = self.make_block(None, parent, p.daa_score + 1, p.blue_score + 1);
            self.insert(b);
            parent = b.hash;
        }
        parent
    }

    /// Make the selected chain the ancestors of `tip` (inclusive); every other known block
    /// stays known but off the chain. Moves the point of view to `tip`'s blue score.
    pub fn reorg_to(&mut self, tip: Hash) -> &mut Self {
        let mut chain = HashSet::new();
        let mut h = tip;
        while h != ZERO_HASH {
            let b = self.blocks.get(&h).unwrap_or_else(|| panic!("ChainSim: unknown block {h}"));
            chain.insert(h);
            h = b.parent;
        }
        self.chain = chain;
        self.tip = tip;
        self.pov_blue_score = self.blocks[&tip].blue_score;
        self
    }

    /// Set the point of view's blue score (the selected parent of the validating block).
    pub fn set_pov(&mut self, blue_score: u64) -> &mut Self {
        self.pov_blue_score = blue_score;
        self
    }

    /// Fluent form of `set_pov`.
    pub fn with_pov(mut self, blue_score: u64) -> Self {
        self.pov_blue_score = blue_score;
        self
    }

    /// Set the finality depth (default: mainnet, 432,000).
    pub fn set_finality_depth(&mut self, f: u64) -> &mut Self {
        self.f = f;
        self
    }

    pub fn finality_depth(&self) -> u64 {
        self.f
    }

    pub fn pov_blue_score(&self) -> u64 {
        self.pov_blue_score
    }

    pub fn tip(&self) -> Hash {
        self.tip
    }

    pub fn block(&self, h: Hash) -> Option<&SimBlock> {
        self.blocks.get(&h)
    }

    pub fn on_chain(&self, h: Hash) -> bool {
        self.chain.contains(&h)
    }

    /// Chain blocks ordered by blue score (root first).
    pub fn chain_blocks(&self) -> Vec<SimBlock> {
        let mut v: Vec<SimBlock> = self.chain.iter().map(|h| self.blocks[h]).collect();
        v.sort_by_key(|b| b.blue_score);
        v
    }

    fn parent_commit(&self, b: &SimBlock) -> Hash {
        if b.parent == ZERO_HASH {
            ZERO_HASH
        } else {
            *self.commits.get(&b.parent).unwrap_or_else(|| panic!("ChainSim: unknown parent {}", b.parent))
        }
    }

    /// The KIP-21 commitment of `b`, its parent's commitment looked up in the sim
    /// (`ZERO_HASH` for a root).
    pub fn seq_commit(&self, b: &SimBlock) -> Hash {
        seq_commit_with_parent(&self.parent_commit(b), b)
    }

    /// The stored commitment of a known block.
    pub fn commit_of(&self, h: Hash) -> Option<Hash> {
        self.commits.get(&h).copied()
    }

    /// The anchor for reference `d`: X = the first chain block with DAA score ≥ d (lowest
    /// blue score among them) and P = its selected parent. `None` if no such X, or X is a root.
    pub fn try_anchor_for(&self, d: u64) -> Option<(SimBlock, SimBlock)> {
        let x = self.chain.iter().map(|h| self.blocks[h]).filter(|b| b.daa_score >= d).min_by_key(|b| b.blue_score)?;
        let p = *self.blocks.get(&x.parent)?;
        Some((x, p))
    }

    /// `try_anchor_for`, panicking if there is no anchor.
    pub fn anchor_for(&self, d: u64) -> (SimBlock, SimBlock) {
        self.try_anchor_for(d).unwrap_or_else(|| panic!("ChainSim: no chain block with daa >= {d} and a known parent"))
    }

    /// The 152-byte proof fields of a known block (spec A3).
    pub fn block_bytes(&self, b: &SimBlock) -> [u8; BLOCK_PROOF_LEN] {
        block_fields_bytes(b, &self.parent_commit(b))
    }

    /// The 272-byte anchor proof `X fields ‖ P fields without P.hash` (spec A3, D95; M7
    /// Stage 5 fix B).
    pub fn proof_bytes(&self, x: &SimBlock, p: &SimBlock) -> Vec<u8> {
        let mut v = Vec::with_capacity(ANCHOR_LEN);
        v.extend_from_slice(&self.block_bytes(x));
        v.extend_from_slice(&self.block_bytes(p)[32..]);
        v
    }

    /// `proof_bytes(anchor_for(d))`.
    pub fn anchor_proof(&self, d: u64) -> Vec<u8> {
        let (x, p) = self.anchor_for(d);
        self.proof_bytes(&x, &p)
    }
}

impl Default for ChainSim {
    /// An empty sim (no blocks; every hash is unknown) with the mainnet finality depth.
    fn default() -> Self {
        Self::empty(default_finality_depth())
    }
}

impl kaspa_txscript::SeqCommitAccessor for ChainSim {
    fn is_chain_ancestor_from_pov(&self, block_hash: Hash) -> Option<bool> {
        self.blocks.contains_key(&block_hash).then(|| self.chain.contains(&block_hash))
    }

    fn seq_commitment_within_depth(&self, block_hash: Hash) -> Option<Hash> {
        let b = self.blocks.get(&block_hash)?;
        // seq_commit_within_threshold(high = pov, low = b, F): low + F > high (seq_commit_accessor.rs:8-10).
        (b.blue_score + self.f > self.pov_blue_score).then(|| self.commits[&block_hash])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_txscript::SeqCommitAccessor;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn chain_sim_is_send_sync() {
        assert_send_sync::<ChainSim>();
        assert_send_sync::<std::sync::Arc<ChainSim>>();
    }

    #[test]
    fn default_f_is_mainnet_finality_depth() {
        assert_eq!(default_finality_depth(), 432_000);
        assert_eq!(ChainSim::line(0, 0, 1).finality_depth(), 432_000);
    }

    #[test]
    fn key_constants_are_the_spec_bytes() {
        let mut seq = [0u8; 32];
        seq[..29].copy_from_slice(b"SeqCommitmentMerkleBranchHash");
        assert_eq!(K_SEQ, seq);
        let mut mc = [0u8; 32];
        mc[..24].copy_from_slice(b"SeqCommitMergesetContext");
        assert_eq!(K_MC, mc);
    }

    #[test]
    fn anchored_has_x_and_p() {
        let x_hash = Hash::from_bytes([9u8; 32]);
        let sim = ChainSim::anchored(1_000_600, x_hash);
        let (x, p) = sim.anchor_for(1_000_600);
        assert_eq!(x.hash, x_hash);
        assert_eq!(x.daa_score, 1_000_600);
        assert_eq!(p.daa_score, 1_000_599);
        assert_eq!(x.parent, p.hash);
        assert_eq!(sim.seq_commitment_within_depth(x_hash), Some(sim.seq_commit(&x)));
        assert_eq!(sim.is_chain_ancestor_from_pov(p.hash), Some(true));
        let proof = sim.proof_bytes(&x, &p);
        assert_eq!(proof.len(), ANCHOR_LEN);
        assert_eq!(&proof[0..32], &x_hash.as_bytes());
        assert_eq!(&proof[32..64], &sim.commit_of(p.hash).unwrap().as_bytes());
        assert_eq!(&proof[152..184], &ZERO_HASH.as_bytes(), "P is a root: its parent commitment is ZERO_HASH");
    }

    #[test]
    fn anchor_for_picks_first_chain_block_at_or_after_d() {
        // daa: 100, 101, 102, then a jump of 248 to 350, then 351.
        let sim = ChainSim::line(100, 50, 3).with_jump(248).with_extend(1);
        let (x, p) = sim.anchor_for(103);
        assert_eq!((x.daa_score, p.daa_score), (350, 102));
        let (x, p) = sim.anchor_for(102);
        assert_eq!((x.daa_score, p.daa_score), (102, 101));
        // Equal-DAA chain blocks: the first one (lowest blue score) is the anchor.
        let sim = ChainSim::line(100, 50, 2).with_jump(0).with_extend(1);
        let (x, p) = sim.anchor_for(101);
        assert_eq!((x.daa_score, x.blue_score, p.daa_score), (101, 51, 100));
        assert!(sim.try_anchor_for(1_000).is_none());
    }

    #[test]
    fn accessor_unknown_and_side_block() {
        let mut sim = ChainSim::line(100, 100, 3);
        let tip = sim.tip();
        let side = sim.add_side_block(sim.block(tip).unwrap().parent, 102);
        assert_eq!(sim.is_chain_ancestor_from_pov(Hash::from_bytes([0xee; 32])), None, "unknown -> None");
        assert_eq!(sim.seq_commitment_within_depth(Hash::from_bytes([0xee; 32])), None);
        assert_eq!(sim.is_chain_ancestor_from_pov(side), Some(false), "side block -> Some(false)");
        assert_eq!(sim.is_chain_ancestor_from_pov(tip), Some(true));
    }

    #[test]
    fn accessor_depth_boundary() {
        let mut sim = ChainSim::line(100, 1_000, 1);
        let h = sim.tip();
        let f = sim.finality_depth();
        sim.set_pov(1_000 + f - 1);
        assert!(sim.seq_commitment_within_depth(h).is_some(), "admitted at pov = bs + f - 1");
        sim.set_pov(1_000 + f);
        assert_eq!(sim.seq_commitment_within_depth(h), None, "too deep at pov = bs + f");
    }

    #[test]
    fn reorg_moves_the_chain() {
        let mut sim = ChainSim::line(100, 100, 3);
        let a_tip = sim.tip();
        let root = sim.chain_blocks()[0].hash;
        let b_tip = sim.fork(root, 4);
        assert_eq!(sim.is_chain_ancestor_from_pov(b_tip), Some(false));
        sim.reorg_to(b_tip);
        assert_eq!(sim.is_chain_ancestor_from_pov(b_tip), Some(true));
        assert_eq!(sim.is_chain_ancestor_from_pov(a_tip), Some(false));
        assert_eq!(sim.is_chain_ancestor_from_pov(root), Some(true));
        assert_eq!(sim.pov_blue_score(), 104);
        let (x, _) = sim.anchor_for(102);
        assert_ne!(x.hash, a_tip);
    }
}
