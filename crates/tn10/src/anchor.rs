//! M7 Stage 4 (spec D103, A9): the anchor proof of a real testnet-10 block, rebuilt from RPC.
//! M8 S3: every function takes any `RpcApi` (no probe context, no network assumption), so
//! the S4 simnet runner can reuse it; the waits are `Timeouts`.
//!
//! `draw()` takes X (the first selected-chain block with DAA score >= D) as 152 bytes (spec
//! A3: hash, parent_commit, activity_root, miner_payload_root, sp_timestamp, daa_score,
//! blue_score) and its selected parent P as the same fields without the hash (120 bytes; M7
//! Stage 5 fix B), 272 in all. The drawer recomputes
//! `SeqCommit = H_seq(parent_commit ‖ H_seq(activity_root ‖ H_seq(H_mc(ts ‖ daa ‖ bs) ‖ mpr)))`
//! for both, compares X's with `OpChainblockSeqCommit(X.hash)` and P's with X's
//! parent_commit. This module reads every field from the node (P's hash too, for the report
//! and the local replay) and checks the recomputation against the header before anything is
//! built.
//!
//! Sources (rusty-kaspa v2.1.0, `refs/rusty-kaspa-v2.1.0`):
//! - `get_block(hash, include_transactions)` (`rpc/core/src/api/rpc.rs:230`): the header
//!   (`accepted_id_merkle_root` = SeqCommit(B), timestamp, daa_score, blue_score, blue_work;
//!   `rpc/core/src/model/header.rs:35-50`) and the verbose data (`selected_parent_hash`,
//!   `merge_set_blues_hashes`, `merge_set_reds_hashes`, `is_chain_block`;
//!   `rpc/core/src/model/block.rs:67-77`). Transaction 0 of a block is its coinbase.
//! - `get_seq_commit_lane_proof(hash, lane_key)` (`rpc/core/src/api/rpc.rs:239`, response
//!   `rpc/core/src/model/message.rs:3783-3789`; consensus `consensus/src/consensus/mod.rs:1504-1590`):
//!   `smt_proof` (`kaspa_smt::proof::OwnedSmtProof` wire format), `lane` (tip, blue score),
//!   `payload_and_ctx_digest`, `parent_seq_commit` (= the selected parent's
//!   `accepted_id_merkle_root`) and `inactivity_shortcut`. With the coinbase lane key
//!   (`kaspa_seq_commit::hashing::COINBASE_LANE_KEY`, `consensus/seq-commit/src/hashing.rs:41`),
//!   `lanes_root = proof.compute_root::<SeqCommitActiveNode>(key, smt_leaf_hash(tip, bs))`
//!   (the consensus debug check at `consensus/mod.rs:1563-1580`) and
//!   `activity_root = activity_root_hash(inactivity_shortcut, lanes_root)` (`hashing.rs:93-98`).
//! - The context hash uses the selected parent's timestamp and the block's own DAA and blue
//!   score (`consensus/src/pipeline/virtual_processor/utxo_validation.rs:614-618`).
//! - The miner payload root is the Merkle root (H_seq) of one leaf per mergeset block,
//!   `miner_payload_leaf(block hash, blue_work big-endian, coinbase payload)`
//!   (`hashing.rs:118-136`), in the order of `ctx.mergeset_acceptance_data`
//!   (`utxo_validation.rs:465-490`), which is the selected parent first, then the rest of
//!   the mergeset (blues and reds together) ascending by (blue_work, hash)
//!   (`utxo_validation.rs:116-125`; `consensus/src/model/stores/ghostdag.rs:116-180`;
//!   `consensus/src/processes/ghostdag/ordering.rs:38-42`). The order is checked at run time
//!   against `payload_and_ctx_digest`; if it does not match, the alternatives below are
//!   tried and the matching one is reported.

use std::time::{Duration, Instant};

use harness::SimBlock;
use kaspa_hashes::{Hash, SeqCommitActiveNode};
use kaspa_rpc_core::{RpcBlock, api::rpc::RpcApi};
use kaspa_seq_commit::hashing::{
    COINBASE_LANE_KEY, activity_root_hash, mergeset_context_hash, miner_payload_leaf, miner_payload_root, payload_and_context_digest, seq_commit,
    seq_state_root, smt_leaf_hash,
};
use kaspa_seq_commit::types::{MergesetContext, MinerPayloadLeafInput, SeqCommitInput, SeqState, SmtLeafInput};
use kaspa_smt::proof::OwnedSmtProof;
use lottery::anchor::{AnchorProof, BlockFields};


/// The mergeset orders tried for the miner payload root, the consensus order first.
pub const ORDERS: [&str; 4] = [
    "consensus: selected parent, then blues and reds ascending by (blue_work, hash)",
    "rpc: merge_set_blues_hashes as returned, then merge_set_reds_hashes as returned",
    "selected parent, then blues and reds descending by (blue_work, hash)",
    "selected parent, then blues ascending, then reds ascending",
];

/// One block's proof fields, rebuilt and self-checked, plus what the report needs.
#[derive(Clone, Debug)]
pub struct RpcBlockProof {
    pub fields: BlockFields,
    /// The same fields as a `ChainSim` block (for the local engine replay).
    pub sim: SimBlock,
    pub selected_parent: Hash,
    /// Index into `ORDERS` of the mergeset order that matched `payload_and_ctx_digest`.
    pub order: usize,
    pub mergeset_blues: usize,
    pub mergeset_reds: usize,
    pub lane_active: bool,
    pub header_commit: Hash,
}

/// How long the anchor helpers wait (M8 S3: configurable; the defaults are the M7 values).
#[derive(Clone, Copy, Debug)]
pub struct Timeouts {
    /// `find_anchor`: until a chain block with DAA >= D exists.
    pub anchor: Duration,
    /// `wait_blue_score`: until the sink's blue score reaches the target.
    pub blue_score: Duration,
    /// Poll interval of both.
    pub poll: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self { anchor: Duration::from_secs(600), blue_score: Duration::from_secs(300), poll: Duration::from_secs(2) }
    }
}

async fn block<C: RpcApi + ?Sized>(c: &C, h: Hash, txs: bool) -> Result<RpcBlock, String> {
    c.get_block(h, txs).await.map_err(|e| format!("get_block({h}): {e}"))
}

/// Rebuild the A3 proof fields of chain block `hash` from RPC and check them:
/// (1) `H_seq(ctx ‖ mpr) == payload_and_ctx_digest` for the mergeset order used;
/// (2) `parent_seq_commit` == the selected parent's `accepted_id_merkle_root`;
/// (3) the full KIP-21 recomputation (kaspa_seq_commit) == the block's `accepted_id_merkle_root`;
/// (4) `lottery::anchor::seq_commit` (the script's formula over the A3 bytes) == the same;
/// (5) the header hashes to the block's hash and binds the fields (`BlockFields::check_header`,
/// what the page's `winningNumbers` checks before the draw).
/// Any failure is an error; nothing is built from an unchecked proof (spec D103).
pub async fn anchor_from_rpc<C: RpcApi + ?Sized>(c: &C, hash: Hash) -> Result<RpcBlockProof, String> {
    let b = block(c, hash, false).await?;
    let v = b.verbose_data.as_ref().ok_or(format!("get_block({hash}): no verbose data"))?;
    if !v.is_chain_block {
        return Err(format!("block {hash} is not a chain block"));
    }
    let hdr = &b.header;
    let sp = v.selected_parent_hash;
    // Post-Toccata, the first direct parent of a chain block is its selected parent
    // (`consensus/core/src/header.rs:220-227`); the consensus lane proof uses that one.
    if hdr.direct_parents().first() != Some(&sp) {
        return Err(format!("block {hash}: verbose selected parent {sp} != first direct parent {:?}", hdr.direct_parents().first()));
    }
    let sp_hdr = block(c, sp, false).await?.header;

    // Mergeset blocks: hash, blue work, coinbase payload.
    let mut ms: Vec<(Hash, Vec<u8>, Vec<u8>)> = vec![]; // (hash, blue_work BE, payload)
    let blues = v.merge_set_blues_hashes.clone();
    let reds = v.merge_set_reds_hashes.clone();
    if !blues.contains(&sp) {
        return Err(format!("block {hash}: selected parent {sp} not among the mergeset blues"));
    }
    for h in blues.iter().chain(reds.iter()) {
        let mb = block(c, *h, true).await?;
        let cb = mb.transactions.first().ok_or(format!("mergeset block {h}: no transactions"))?;
        ms.push((*h, mb.header.blue_work.to_be_bytes().to_vec(), cb.payload.clone()));
    }
    let leaf = |h: &Hash| -> Hash {
        let (bh, bw, pl) = ms.iter().find(|m| &m.0 == h).unwrap();
        miner_payload_leaf(MinerPayloadLeafInput { block_hash: bh, blue_work_be_bytes: bw, payload: pl })
    };
    let key = |h: &Hash| -> (Vec<u8>, Hash) {
        let (_, bw, _) = ms.iter().find(|m| &m.0 == h).unwrap();
        // Big-endian bytes of equal width compare as the numbers do.
        (bw.clone(), *h)
    };
    let mut rest: Vec<Hash> = blues.iter().chain(reds.iter()).copied().filter(|h| *h != sp).collect();
    rest.sort_by_key(key);
    let consensus: Vec<Hash> = std::iter::once(sp).chain(rest.iter().copied()).collect();
    let rpc_order: Vec<Hash> = blues.iter().chain(reds.iter()).copied().collect();
    let descending: Vec<Hash> = std::iter::once(sp).chain(rest.iter().rev().copied()).collect();
    let mut bl: Vec<Hash> = blues.iter().copied().filter(|h| *h != sp).collect();
    bl.sort_by_key(key);
    let mut rd = reds.clone();
    rd.sort_by_key(key);
    let split: Vec<Hash> = std::iter::once(sp).chain(bl).chain(rd).collect();
    let orders = [consensus, rpc_order, descending, split];

    let lane = c.get_seq_commit_lane_proof(hash, COINBASE_LANE_KEY).await.map_err(|e| format!("get_seq_commit_lane_proof({hash}): {e}"))?;
    let context = mergeset_context_hash(&MergesetContext { timestamp: sp_hdr.timestamp, daa_score: hdr.daa_score, blue_score: hdr.blue_score });
    let mut found = None;
    for (i, o) in orders.iter().enumerate() {
        let mpr = miner_payload_root(o.iter().map(&leaf).collect::<Vec<_>>().into_iter());
        if payload_and_context_digest(&context, &mpr) == lane.payload_and_ctx_digest {
            found = Some((i, mpr));
            break;
        }
    }
    let (order, mpr) = found.ok_or(format!(
        "block {hash}: no mergeset order ({} blues, {} reds) gives payload_and_ctx_digest {} (context from sp timestamp {}, daa {}, bs {})",
        blues.len(),
        reds.len(),
        lane.payload_and_ctx_digest,
        sp_hdr.timestamp,
        hdr.daa_score,
        hdr.blue_score
    ))?;

    if lane.parent_seq_commit != sp_hdr.accepted_id_merkle_root {
        return Err(format!("block {hash}: parent_seq_commit {} != selected parent's accepted_id_merkle_root {}", lane.parent_seq_commit, sp_hdr.accepted_id_merkle_root));
    }
    let proof = OwnedSmtProof::from_bytes(&lane.smt_proof).map_err(|e| format!("block {hash}: smt proof: {e:?}"))?;
    let leaf_hash = lane.lane.as_ref().map(|l| smt_leaf_hash(&SmtLeafInput { lane_tip: &l.tip, blue_score: l.blue_score }));
    let lanes_root = proof.compute_root::<SeqCommitActiveNode>(&COINBASE_LANE_KEY, leaf_hash).map_err(|e| format!("block {hash}: lanes root: {e:?}"))?;
    let activity_root = activity_root_hash(&lane.inactivity_shortcut, &lanes_root);
    let state_root = seq_state_root(&SeqState { activity_root: &activity_root, payload_and_ctx_digest: &lane.payload_and_ctx_digest });
    let full = seq_commit(&SeqCommitInput { parent_seq_commit: &lane.parent_seq_commit, state_root: &state_root });
    if full != hdr.accepted_id_merkle_root {
        return Err(format!("block {hash}: KIP-21 recomputation {full} != header accepted_id_merkle_root {}", hdr.accepted_id_merkle_root));
    }
    let fields = BlockFields {
        hash: hash.as_bytes(),
        parent_commit: lane.parent_seq_commit.as_bytes(),
        activity_root: activity_root.as_bytes(),
        miner_payload_root: mpr.as_bytes(),
        sp_timestamp: sp_hdr.timestamp,
        daa_score: hdr.daa_score,
        blue_score: hdr.blue_score,
    };
    if fields.seq_commit() != hdr.accepted_id_merkle_root.as_bytes() {
        return Err(format!("block {hash}: the script's formula over the A3 bytes differs from the header commitment"));
    }
    // (5) F1 re-review S1: the header binds the fields as the page's `winningNumbers` checks
    // them (`BlockFields::check_header`: the header hashes to the block's hash, commits to
    // the fields, and has their DAA and blue score), on the node's real header.
    let consensus_hdr = kaspa_consensus_core::header::Header::try_from(hdr).map_err(|e| format!("block {hash}: header: {e}"))?;
    fields.check_header(&consensus_hdr).map_err(|e| format!("block {hash}: {e}"))?;
    let sim = SimBlock {
        hash,
        parent: sp,
        blue_score: hdr.blue_score,
        daa_score: hdr.daa_score,
        sp_timestamp: sp_hdr.timestamp,
        activity_root,
        miner_payload_root: mpr,
    };
    Ok(RpcBlockProof {
        fields,
        sim,
        selected_parent: sp,
        order,
        mergeset_blues: blues.len(),
        mergeset_reds: reds.len(),
        lane_active: lane.lane.is_some(),
        header_commit: hdr.accepted_id_merkle_root,
    })
}

/// The first selected-chain block X with DAA score >= `d` and its selected parent P, found
/// by walking the virtual chain from `start` (a chain block with DAA < `d`, e.g. the sink
/// taken before `close()` was submitted). Waits (up to `to.anchor`) until such a block exists.
/// DAA is non-decreasing along the chain (`consensus/src/processes/difficulty.rs:27-30`), so
/// a binary search over the added chain blocks finds X.
pub async fn find_anchor<C: RpcApi + ?Sized>(c: &C, start: Hash, d: u64, to: &Timeouts) -> Result<(Hash, Hash), String> {
    let t0 = Instant::now();
    loop {
        let vc = c.get_virtual_chain_from_block(start, false, None).await.map_err(|e| format!("get_virtual_chain_from_block: {e}"))?;
        let added = vc.added_chain_block_hashes;
        if !vc.removed_chain_block_hashes.is_empty() {
            println!("find_anchor: start {start} is off the chain now ({} removed chain blocks); walking from the fork point", vc.removed_chain_block_hashes.len());
        }
        let daa = |h: Hash| async move { block(c, h, false).await.map(|b| b.header.daa_score) };
        if let Some(&last) = added.last() {
            if daa(last).await? >= d {
                // Binary search: first index with daa >= d.
                let (mut lo, mut hi) = (0usize, added.len() - 1);
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    if daa(added[mid]).await? >= d { hi = mid } else { lo = mid + 1 }
                }
                let x = added[lo];
                let xb = block(c, x, false).await?;
                let p = xb.verbose_data.as_ref().ok_or("no verbose data")?.selected_parent_hash;
                let pd = daa(p).await?;
                if lo > 0 && added[lo - 1] != p {
                    return Err(format!("find_anchor: chain block before X {x} is {} but X's selected parent is {p}", added[lo - 1]));
                }
                if xb.header.daa_score < d || pd >= d {
                    return Err(format!("find_anchor: X {x} daa {} / P {p} daa {pd} do not bracket D = {d}", xb.header.daa_score));
                }
                println!("anchor for D = {d}: X {x} (DAA {}, blue score {}), P {p} (DAA {pd}); {} chain blocks after start", xb.header.daa_score, xb.header.blue_score, lo + 1);
                return Ok((x, p));
            }
        }
        if t0.elapsed() > to.anchor {
            return Err(format!("find_anchor: no chain block with DAA >= {d} within {} s", to.anchor.as_secs()));
        }
        tokio::time::sleep(to.poll).await;
    }
}

/// Wait until the sink's blue score is at least `bs` (confirmations for X before building).
pub async fn wait_blue_score<C: RpcApi + ?Sized>(c: &C, bs: u64, to: &Timeouts) -> Result<(), String> {
    let t0 = Instant::now();
    loop {
        let sink = c.get_block_dag_info().await.map_err(|e| e.to_string())?.sink;
        let sbs = block(c, sink, false).await?.header.blue_score;
        if sbs >= bs {
            return Ok(());
        }
        if t0.elapsed() > to.blue_score {
            return Err(format!("sink blue score {sbs} < {bs} after {} s", to.blue_score.as_secs()));
        }
        tokio::time::sleep(to.poll).await;
    }
}

/// X and P rebuilt from RPC, checked against each other and against D: P's recomputed
/// commitment is the parent commitment X carries (pc == x_pc), dX >= D, 0 <= dP < D.
pub struct RpcAnchor {
    pub d: u64,
    pub x: RpcBlockProof,
    pub p: RpcBlockProof,
    pub proof: AnchorProof,
}

pub async fn rpc_anchor<C: RpcApi + ?Sized>(c: &C, start: Hash, d: u64, to: &Timeouts) -> Result<RpcAnchor, String> {
    let (xh, ph) = find_anchor(c, start, d, to).await?;
    let x = anchor_from_rpc(c, xh).await?;
    let p = anchor_from_rpc(c, ph).await?;
    if x.selected_parent != ph {
        return Err(format!("X {xh}: selected parent {} != P {ph}", x.selected_parent));
    }
    let proof = AnchorProof { x: x.fields, p: p.fields.preimage() };
    if !proof.consistent_with(d as i64) {
        return Err(format!("anchor X {xh} / P {ph} is not consistent with D = {d} (pc == x_pc, dX >= D, 0 <= dP < D)"));
    }
    for (n, b) in [("X", &x), ("P", &p)] {
        println!(
            "{n} {}: DAA {}, blue score {}, mergeset {} blues + {} reds, coinbase lane active {}, mergeset order matched: [{}] {}; commitment {} = header",
            b.sim.hash, b.sim.daa_score, b.sim.blue_score, b.mergeset_blues, b.mergeset_reds, b.lane_active, b.order, ORDERS[b.order], b.header_commit
        );
    }
    Ok(RpcAnchor { d, x, p, proof })
}
