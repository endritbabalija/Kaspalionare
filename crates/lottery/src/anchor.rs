//! The anchor proof of `draw()` (spec M7 D95, A3, A9): the KIP-21 preimage fields of the
//! anchor block X (the first selected-chain block with DAA score >= D) and of its selected
//! parent P, 152 bytes each, 304 in all. The drawer recomputes both commitments with
//! `blake3WithKey` and compares them with `OpChainblockSeqCommit` (check 8); this module
//! builds and parses the bytes and computes the same commitment off-chain.
//!
//! Shared by the tests (from `harness::ChainSim`) and `tn10` (from RPC, Stage 4).
//!
//! Layout per block (A3; field order = byte order):
//!   0 hash (32) | 32 parent_commit = SeqCommit(selected parent) (32) | 64 activity_root (32) |
//!   96 miner_payload_root (32) | 128 sp_timestamp (8 LE) | 136 daa_score (8 LE) |
//!   144 blue_score (8 LE)
//!
//! Commitment (KIP-21 §6.5.1, §6.6, §6.7; rusty-kaspa v2.1.0
//! `consensus/seq-commit/src/hashing.rs:80-84, 153-173`):
//!   SeqCommit = H_seq(parent_commit ‖ H_seq(activity_root ‖ H_seq(H_mc(ts ‖ daa ‖ bs) ‖ mpr)))
//! with H_seq = keyed BLAKE3 under K_SEQ and H_mc = keyed BLAKE3 under K_MC (the zero-padded
//! tags, `crypto/hashes/src/hashers.rs:40, 47, 120-138`; the key bytes are pinned against the
//! `kaspa_hashes` hashers in `harness`, SP11). The function here is a separate implementation
//! over raw bytes; a unit test pins it equal to `harness::chain::seq_commit_with_parent`.

use harness::chain::{ANCHOR_LEN, BLOCK_PROOF_LEN, K_MC, K_SEQ};
use harness::{ChainSim, SimBlock};

/// One block's proof fields (spec A3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockFields {
    pub hash: [u8; 32],
    /// SeqCommit of the block's selected parent, as committed in the block.
    pub parent_commit: [u8; 32],
    pub activity_root: [u8; 32],
    pub miner_payload_root: [u8; 32],
    /// Timestamp of the block's selected parent (the mergeset context timestamp, KIP-21 §6.6).
    pub sp_timestamp: u64,
    pub daa_score: u64,
    pub blue_score: u64,
}

impl BlockFields {
    pub const LEN: usize = BLOCK_PROOF_LEN;

    /// The fields of a `ChainSim` block, with its parent's commitment from the sim.
    pub fn from_sim(sim: &ChainSim, b: &SimBlock) -> Self {
        Self::from_bytes(&sim.block_bytes(b)).expect("152 bytes")
    }

    /// The 152 bytes of A3.
    pub fn to_bytes(&self) -> [u8; BLOCK_PROOF_LEN] {
        let mut out = [0u8; BLOCK_PROOF_LEN];
        out[0..32].copy_from_slice(&self.hash);
        out[32..64].copy_from_slice(&self.parent_commit);
        out[64..96].copy_from_slice(&self.activity_root);
        out[96..128].copy_from_slice(&self.miner_payload_root);
        out[128..136].copy_from_slice(&self.sp_timestamp.to_le_bytes());
        out[136..144].copy_from_slice(&self.daa_score.to_le_bytes());
        out[144..152].copy_from_slice(&self.blue_score.to_le_bytes());
        out
    }

    /// Parse exactly 152 bytes.
    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        if b.len() != BLOCK_PROOF_LEN {
            return Err(format!("block fields: {} bytes, expected {BLOCK_PROOF_LEN}", b.len()));
        }
        let h = |o: usize| -> [u8; 32] { b[o..o + 32].try_into().unwrap() };
        let u = |o: usize| -> u64 { u64::from_le_bytes(b[o..o + 8].try_into().unwrap()) };
        Ok(Self {
            hash: h(0),
            parent_commit: h(32),
            activity_root: h(64),
            miner_payload_root: h(96),
            sp_timestamp: u(128),
            daa_score: u(136),
            blue_score: u(144),
        })
    }

    /// The block's KIP-21 commitment recomputed from these fields (what check 8 compares
    /// with `OpChainblockSeqCommit(hash)`).
    pub fn seq_commit(&self) -> [u8; 32] {
        seq_commit(self)
    }
}

/// `H_seq(a ‖ b)`.
fn h_seq(a: &[u8], b: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new_keyed(&K_SEQ);
    h.update(a);
    h.update(b);
    *h.finalize().as_bytes()
}

/// The KIP-21 commitment of a block from its proof fields (spec D95; the drawer's check 8
/// step for step: ctx, pcd, state root, commitment).
pub fn seq_commit(f: &BlockFields) -> [u8; 32] {
    let mut ctx = blake3::Hasher::new_keyed(&K_MC);
    ctx.update(&f.sp_timestamp.to_le_bytes());
    ctx.update(&f.daa_score.to_le_bytes());
    ctx.update(&f.blue_score.to_le_bytes());
    let ctx = *ctx.finalize().as_bytes();
    let pcd = h_seq(&ctx, &f.miner_payload_root);
    let sr = h_seq(&f.activity_root, &pcd);
    h_seq(&f.parent_commit, &sr)
}

/// The anchor proof: X (the first chain block with DAA score >= D) and P (its selected
/// parent), spec D95.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorProof {
    pub x: BlockFields,
    pub p: BlockFields,
}

impl AnchorProof {
    pub const LEN: usize = ANCHOR_LEN;

    /// The proof of the anchor for reference `d` on `sim` (panics if the sim has none).
    pub fn from_sim(sim: &ChainSim, d: u64) -> Self {
        let (x, p) = sim.anchor_for(d);
        Self { x: BlockFields::from_sim(sim, &x), p: BlockFields::from_sim(sim, &p) }
    }

    /// The 304-byte `anchor` argument: X fields ‖ P fields.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(ANCHOR_LEN);
        v.extend_from_slice(&self.x.to_bytes());
        v.extend_from_slice(&self.p.to_bytes());
        v
    }

    /// Parse exactly 304 bytes.
    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        if b.len() != ANCHOR_LEN {
            return Err(format!("anchor: {} bytes, expected {ANCHOR_LEN}", b.len()));
        }
        Ok(Self { x: BlockFields::from_bytes(&b[..BLOCK_PROOF_LEN])?, p: BlockFields::from_bytes(&b[BLOCK_PROOF_LEN..])? })
    }

    /// The seed source X.hash (D94).
    pub fn x_hash(&self) -> [u8; 32] {
        self.x.hash
    }

    /// The off-chain part of check 8 (everything except the two opcode calls): P is X's
    /// selected parent by commitment (pc == x_pc), dX >= D and dP < D, with the daa fields
    /// read as the script reads them (8-byte LE; values >= 2^63 are negative and fail).
    pub fn consistent_with(&self, d: i64) -> bool {
        let (dx, dp) = (self.x.daa_score as i64, self.p.daa_score as i64);
        self.p.seq_commit() == self.x.parent_commit && dx >= d && dp >= 0 && dp < d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness::Hash;
    use harness::chain::{block_fields_bytes, seq_commit_with_parent};

    fn sim_block(n: u8, daa: u64, bs: u64) -> SimBlock {
        SimBlock {
            hash: Hash::from_bytes([n; 32]),
            parent: Hash::from_bytes([n.wrapping_sub(1); 32]),
            blue_score: bs,
            daa_score: daa,
            sp_timestamp: 1_700_000_000_000 + n as u64,
            activity_root: Hash::from_bytes([n ^ 0x5a; 32]),
            miner_payload_root: Hash::from_bytes([n ^ 0xa5; 32]),
        }
    }

    /// `seq_commit` here equals the harness's (which SP11 pins against `kaspa_hashes` and
    /// `kaspa_seq_commit`), on fixed blocks including extreme field values and several parent
    /// commitments, and the byte layout equals `harness::chain::block_fields_bytes`.
    #[test]
    fn seq_commit_pinned_to_harness() {
        let mut blocks = vec![sim_block(1, 0, 0), sim_block(2, 1_000_600, 1_000_600), sim_block(3, 1, 2)];
        let mut max = sim_block(4, u64::MAX, u64::MAX);
        max.sp_timestamp = u64::MAX;
        blocks.push(max);
        for parent in [[0u8; 32], [0xff; 32], [0x13; 32]] {
            let pc = Hash::from_bytes(parent);
            for b in &blocks {
                let bytes = block_fields_bytes(b, &pc);
                let f = BlockFields::from_bytes(&bytes).unwrap();
                assert_eq!(f.to_bytes(), bytes);
                assert_eq!(f.seq_commit(), seq_commit_with_parent(&pc, b).as_bytes());
            }
        }
    }

    /// On a sim: the proof of `anchored(d, x)` parses, carries x as X.hash, round-trips, its
    /// commitments equal the sim's, and it is consistent with d but not with d + 1 or d - 1
    /// (dP < D fails at d - 1 since P has daa d - 1).
    #[test]
    fn anchor_from_sim() {
        let d = 1_000_600u64;
        let x = Hash::from_bytes([0x42; 32]);
        let sim = ChainSim::anchored(d, x);
        let a = AnchorProof::from_sim(&sim, d);
        assert_eq!(a.x_hash(), [0x42; 32]);
        assert_eq!(a.to_bytes(), sim.anchor_proof(d));
        assert_eq!(AnchorProof::from_bytes(&a.to_bytes()).unwrap(), a);
        let (xb, pb) = sim.anchor_for(d);
        assert_eq!(a.x.seq_commit(), sim.commit_of(xb.hash).unwrap().as_bytes());
        assert_eq!(a.p.seq_commit(), sim.commit_of(pb.hash).unwrap().as_bytes());
        assert!(a.consistent_with(d as i64));
        assert!(!a.consistent_with(d as i64 + 1));
        assert!(!a.consistent_with(d as i64 - 1));
        assert!(AnchorProof::from_bytes(&a.to_bytes()[..303]).is_err());
    }
}
