//! The anchor proof of `draw()` (spec M7 D95, A3, A9): the KIP-21 preimage fields of the
//! anchor block X (the first selected-chain block with DAA score >= D), with X.hash, and the
//! preimage fields of its selected parent P, without P.hash: 152 + 120 = 272 bytes. The
//! drawer recomputes both commitments with `blake3WithKey`, compares X's with
//! `OpChainblockSeqCommit(X.hash)` and P's with X's committed parent commitment (check 8);
//! this module builds and parses the bytes and computes the same commitments off-chain.
//!
//! M7 Stage 5 fix B (was 304 bytes, with P.hash and a second opcode call on it): `pc == x_pc`
//! alone pins P's whole preimage (X's commitment is opcode-authenticated and commits to
//! SeqCommit(P), KIP-21 §6.7), and P.hash was used for nothing else. Dropping it leaves no
//! unconstrained byte in the argument: an unchecked byte would let any relayer change the
//! txid without changing the outcome.
//!
//! Shared by the tests (from `harness::ChainSim`) and `tn10` (from RPC).
//!
//! Layout (A3; field order = byte order):
//!   X:   0 hash (32) | 32 parent_commit = SeqCommit(P) (32) | 64 activity_root (32) |
//!        96 miner_payload_root (32) | 128 sp_timestamp (8 LE) | 136 daa_score (8 LE) |
//!        144 blue_score (8 LE)
//!   P: 152 parent_commit (32) | 184 activity_root (32) | 216 miner_payload_root (32) |
//!      248 sp_timestamp (8 LE) | 256 daa_score (8 LE) | 264 blue_score (8 LE)   (272 total)
//!
//! Commitment (KIP-21 §6.5.1, §6.6, §6.7; rusty-kaspa v2.1.0
//! `consensus/seq-commit/src/hashing.rs:80-84, 153-173`):
//!   SeqCommit = H_seq(parent_commit ‖ H_seq(activity_root ‖ H_seq(H_mc(ts ‖ daa ‖ bs) ‖ mpr)))
//! with H_seq = keyed BLAKE3 under K_SEQ and H_mc = keyed BLAKE3 under K_MC (the zero-padded
//! tags, `crypto/hashes/src/hashers.rs:40, 47, 120-138`; the key bytes are pinned against the
//! `kaspa_hashes` hashers in `harness`, SP11). The function here is a separate implementation
//! over raw bytes; a unit test pins it equal to `harness::chain::seq_commit_with_parent`.

use harness::chain::{ANCHOR_LEN, BLOCK_PROOF_LEN, K_MC, K_SEQ, PREIMAGE_LEN};
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

    /// F1 re-review S1: `h` is the header of this block, i.e. the header binds the proof's
    /// hash to its commitment fields the way `draw()`'s `OpChainblockSeqCommit(hash)` does on
    /// chain (the opcode returns the stored header's `accepted_id_merkle_root`, rusty-kaspa
    /// v2.1.0 `consensus/src/model/services/seq_commit_accessor.rs:36-43`):
    /// - the header hashes to `hash` (`consensus/core/src/hashing/header.rs:6-35`: version,
    ///   parents, hash_merkle_root, accepted_id_merkle_root, utxo_commitment, timestamp, bits,
    ///   nonce, daa_score, blue_score, blue_work, pruning_point), and its stated hash is that one;
    /// - its `accepted_id_merkle_root` is the KIP-21 commitment of these fields (the field
    ///   carries SeqCommit: `consensus/src/pipeline/virtual_processor/utxo_validation.rs:202-207`);
    /// - its DAA score and blue score are these fields' (the commitment's mergeset context
    ///   takes `header.daa_score` and the block's blue score, `utxo_validation.rs:614-618`).
    ///
    /// What this cannot show: that the header is a real block on the selected chain. A
    /// header with another nonce hashes to another value, so anyone can make a header for a
    /// chosen hash's fields; only a node that holds the block (and has validated its
    /// commitment, which consensus does for chain blocks only, `processor.rs:424-456`) can
    /// vouch for it.
    pub fn check_header(&self, h: &kaspa_consensus_core::header::Header) -> Result<(), String> {
        let hash = kaspa_consensus_core::hashing::header::hash(h);
        if hash.as_bytes() != self.hash {
            return Err(format!("the header hashes to {hash}, not to the proof's X.hash {}", harness::Hash::from_bytes(self.hash)));
        }
        if h.hash != hash {
            return Err(format!("the header states hash {} but hashes to {hash}", h.hash));
        }
        if h.accepted_id_merkle_root.as_bytes() != self.seq_commit() {
            return Err(format!("the header's commitment {} is not the KIP-21 commitment of the proof's X fields", h.accepted_id_merkle_root));
        }
        if (h.daa_score, h.blue_score) != (self.daa_score, self.blue_score) {
            return Err(format!("the header's DAA and blue scores ({}, {}) are not the proof's ({}, {})", h.daa_score, h.blue_score, self.daa_score, self.blue_score));
        }
        Ok(())
    }

    /// The commitment preimage fields (everything but the hash): P's part of the proof.
    pub fn preimage(&self) -> PreimageFields {
        PreimageFields {
            parent_commit: self.parent_commit,
            activity_root: self.activity_root,
            miner_payload_root: self.miner_payload_root,
            sp_timestamp: self.sp_timestamp,
            daa_score: self.daa_score,
            blue_score: self.blue_score,
        }
    }
}

/// A block's commitment preimage fields without its hash (P's part of the anchor proof, M7
/// Stage 5 fix B): 120 bytes, the A3 block layout from offset 32 on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreimageFields {
    pub parent_commit: [u8; 32],
    pub activity_root: [u8; 32],
    pub miner_payload_root: [u8; 32],
    pub sp_timestamp: u64,
    pub daa_score: u64,
    pub blue_score: u64,
}

impl PreimageFields {
    pub const LEN: usize = PREIMAGE_LEN;

    /// The 120 bytes: parent_commit, activity_root, miner_payload_root, sp_timestamp,
    /// daa_score, blue_score.
    pub fn to_bytes(&self) -> [u8; PREIMAGE_LEN] {
        let mut out = [0u8; PREIMAGE_LEN];
        out.copy_from_slice(&self.with_hash([0; 32]).to_bytes()[32..]);
        out
    }

    /// Parse exactly 120 bytes.
    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        if b.len() != PREIMAGE_LEN {
            return Err(format!("preimage fields: {} bytes, expected {PREIMAGE_LEN}", b.len()));
        }
        Ok(BlockFields::from_bytes(&[&[0u8; 32][..], b].concat())?.preimage())
    }

    /// The block fields with `hash`.
    pub fn with_hash(&self, hash: [u8; 32]) -> BlockFields {
        BlockFields {
            hash,
            parent_commit: self.parent_commit,
            activity_root: self.activity_root,
            miner_payload_root: self.miner_payload_root,
            sp_timestamp: self.sp_timestamp,
            daa_score: self.daa_score,
            blue_score: self.blue_score,
        }
    }

    /// The KIP-21 commitment recomputed from these fields (check 8 compares P's with
    /// X.parent_commit; the hash is not part of the preimage).
    pub fn seq_commit(&self) -> [u8; 32] {
        seq_commit(&self.with_hash([0; 32]))
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

/// The anchor proof: X (the first chain block with DAA score >= D) and the preimage fields of
/// P (its selected parent, without P.hash), spec D95 and M7 Stage 5 fix B.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorProof {
    pub x: BlockFields,
    pub p: PreimageFields,
}

impl AnchorProof {
    pub const LEN: usize = ANCHOR_LEN;

    /// The proof of the anchor for reference `d` on `sim` (panics if the sim has none).
    pub fn from_sim(sim: &ChainSim, d: u64) -> Self {
        let (x, p) = sim.anchor_for(d);
        Self { x: BlockFields::from_sim(sim, &x), p: BlockFields::from_sim(sim, &p).preimage() }
    }

    /// The 272-byte `anchor` argument: X fields ‖ P's preimage fields.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(ANCHOR_LEN);
        v.extend_from_slice(&self.x.to_bytes());
        v.extend_from_slice(&self.p.to_bytes());
        v
    }

    /// Parse exactly 272 bytes.
    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        if b.len() != ANCHOR_LEN {
            return Err(format!("anchor: {} bytes, expected {ANCHOR_LEN}", b.len()));
        }
        Ok(Self { x: BlockFields::from_bytes(&b[..BLOCK_PROOF_LEN])?, p: PreimageFields::from_bytes(&b[BLOCK_PROOF_LEN..])? })
    }

    /// The seed source X.hash (D94).
    pub fn x_hash(&self) -> [u8; 32] {
        self.x.hash
    }

    /// The off-chain part of check 8 (everything except the opcode call on X): P is X's
    /// selected parent by commitment (pc == x_pc), dX >= D and dP < D, with the daa fields
    /// read as the script reads them (8-byte LE; values >= 2^63 are negative and fail).
    pub fn consistent_with(&self, d: i64) -> bool {
        let (dx, dp) = (self.x.daa_score as i64, self.p.daa_score as i64);
        self.p.seq_commit() == self.x.parent_commit && dx >= d && dp >= 0 && dp < d
    }
}

/// Fixtures (F1 re-review S1): a header that commits to `x`'s fields (its
/// `accepted_id_merkle_root` is `x`'s commitment; DAA and blue score are `x`'s), with
/// `nonce` and fixed other fields. Its hash is not `x.hash`: a caller sets `x.hash =
/// header.hash` (the commitment does not depend on the hash) to get a proof the header binds.
#[cfg(any(test, feature = "fixtures"))]
pub fn header_committing_to(x: &BlockFields, nonce: u64) -> kaspa_consensus_core::header::Header {
    use harness::Hash;
    let parents: kaspa_consensus_core::header::CompressedParents =
        vec![vec![Hash::from_bytes([0x51; 32]), Hash::from_bytes([0x52; 32])], vec![Hash::from_bytes([0x53; 32])]].try_into().expect("two levels");
    kaspa_consensus_core::header::Header::new_finalized(
        1,
        parents,
        Hash::from_bytes([0x61; 32]),
        Hash::from_bytes(x.seq_commit()),
        Hash::from_bytes([0x62; 32]),
        x.sp_timestamp + 100,
        0x1e7f_ffff,
        nonce,
        x.daa_score,
        kaspa_consensus_core::BlueWorkType::from_u64(0x1234_5678_9abc),
        x.blue_score,
        Hash::from_bytes([0x63; 32]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness::Hash;
    use harness::chain::{block_fields_bytes, seq_commit_with_parent};

    /// F1 re-review S1: a header binds a proof's X.hash to its fields: the header that hashes
    /// to X.hash and commits to X's fields passes; another nonce (another hash), another
    /// commitment, DAA or blue score, or a wrong stated hash fails.
    #[test]
    fn header_binds_x_hash_to_its_fields() {
        let sim = ChainSim::anchored(1_000_600, Hash::from_bytes([0x42; 32]));
        let mut a = AnchorProof::from_sim(&sim, 1_000_600);
        let h = header_committing_to(&a.x, 7);
        assert!(a.x.check_header(&h).unwrap_err().contains("hashes to"), "the sim's X.hash is not this header's");
        a.x.hash = h.hash.as_bytes();
        a.x.check_header(&h).unwrap();
        assert!(a.consistent_with(1_000_600), "the commitment does not depend on the hash");
        let other = header_committing_to(&a.x, 8);
        assert!(a.x.check_header(&other).unwrap_err().contains("hashes to"));
        let mut stated = h.clone();
        stated.hash = Hash::from_bytes([1; 32]);
        assert!(a.x.check_header(&stated).unwrap_err().contains("states hash"));
        // A changed commitment, DAA or blue score: re-finalized so the hash matches, and the
        // proof's X.hash moved to it, so only the named field is wrong.
        for (what, f) in [
            ("commitment", (|h: &mut kaspa_consensus_core::header::Header| h.accepted_id_merkle_root = Hash::from_bytes([9; 32])) as fn(&mut _)),
            ("DAA", |h| h.daa_score += 1),
            ("blue", |h| h.blue_score += 1),
        ] {
            let mut m = h.clone();
            f(&mut m);
            m.finalize();
            let mut b = a;
            b.x.hash = m.hash.as_bytes();
            let e = b.x.check_header(&m).unwrap_err();
            assert!(e.contains(if what == "commitment" { "commitment" } else { "scores" }), "{what}: {e}");
        }
    }

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
                // P's part: the same commitment without the hash, the same bytes from 32 on.
                let pf = f.preimage();
                assert_eq!(pf.to_bytes()[..], bytes[32..]);
                assert_eq!(PreimageFields::from_bytes(&pf.to_bytes()).unwrap(), pf);
                assert_eq!(pf.seq_commit(), f.seq_commit());
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
        assert_eq!(a.to_bytes().len(), 272);
        assert_eq!(a.p, BlockFields::from_sim(&sim, &pb).preimage());
        assert_eq!(a.to_bytes()[152..], sim.block_bytes(&pb)[32..], "P without its hash");
        assert!(AnchorProof::from_bytes(&a.to_bytes()[..271]).is_err());
        let mut long = a.to_bytes();
        long.push(0);
        assert!(AnchorProof::from_bytes(&long).is_err());
    }
}
