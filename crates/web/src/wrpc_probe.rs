//! F1 part C, spec 6.1 "Anchor proof": can the page's worker call `GetSeqCommitLaneProof`
//! itself? That RPC is in the Rust wRPC client only (rusty-kaspa v2.1.0
//! `rpc/wrpc/client/src/client.rs:667`), not in the JS binding. This probe (feature
//! `wrpc-probe`, never in a shipped build) compiles that client into this wasm and calls it:
//! it connects to a public testnet-10 node through the resolver, reads the sink (a chain
//! block), asks for its coinbase-lane proof, and recomputes the block's KIP-21 commitment from
//! the proof the way `tn10/src/anchor.rs` does (lanes root from the SMT proof, activity root,
//! state root, commitment) against the header's `accepted_id_merkle_root`. The miner payload
//! root (the mergeset's coinbase payloads, needed for the 272-byte `draw()` argument) is not
//! rebuilt here: that is the anchor builder's job (F2/F5).

use std::time::Duration;

use kaspa_hashes::SeqCommitActiveNode;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_seq_commit::hashing::{COINBASE_LANE_KEY, activity_root_hash, seq_commit, seq_state_root, smt_leaf_hash};
use kaspa_seq_commit::types::{SeqCommitInput, SeqState, SmtLeafInput};
use kaspa_smt::proof::OwnedSmtProof;
use kaspa_wrpc_client::client::{ConnectOptions, ConnectStrategy};
use kaspa_wrpc_client::prelude::{NetworkId, NetworkType};
use kaspa_wrpc_client::{KaspaRpcClient, Resolver, WrpcEncoding};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeOut {
    node: String,
    network: String,
    block: String,
    daa_score: String,
    smt_proof_bytes: usize,
    has_lane: bool,
    commitment_matches: bool,
}

async fn run(url: Option<String>) -> Result<ProbeOut, String> {
    let network = NetworkId::with_suffix(NetworkType::Testnet, 10);
    let resolver = if url.is_none() { Some(Resolver::default()) } else { None };
    let c = KaspaRpcClient::new(WrpcEncoding::Borsh, url.as_deref(), resolver, Some(network), None).map_err(|e| e.to_string())?;
    let opts = ConnectOptions { block_async_connect: true, connect_timeout: Some(Duration::from_secs(20)), strategy: ConnectStrategy::Fallback, ..Default::default() };
    c.connect(Some(opts)).await.map_err(|e| format!("connect: {e}"))?;
    let info = c.get_block_dag_info().await.map_err(|e| format!("get_block_dag_info: {e}"))?;
    let net = info.network.to_string();
    if net != "testnet-10" {
        let _ = c.disconnect().await;
        return Err(format!("node reports {net}; refusing"));
    }
    let hash = info.sink;
    let block = c.get_block(hash, false).await.map_err(|e| format!("get_block: {e}"))?;
    let lane = c.get_seq_commit_lane_proof(hash, COINBASE_LANE_KEY).await.map_err(|e| format!("get_seq_commit_lane_proof: {e}"))?;
    let proof = OwnedSmtProof::from_bytes(&lane.smt_proof).map_err(|e| format!("smt proof: {e:?}"))?;
    let leaf = lane.lane.as_ref().map(|l| smt_leaf_hash(&SmtLeafInput { lane_tip: &l.tip, blue_score: l.blue_score }));
    let lanes_root = proof.compute_root::<SeqCommitActiveNode>(&COINBASE_LANE_KEY, leaf).map_err(|e| format!("lanes root: {e:?}"))?;
    let activity_root = activity_root_hash(&lane.inactivity_shortcut, &lanes_root);
    let state_root = seq_state_root(&SeqState { activity_root: &activity_root, payload_and_ctx_digest: &lane.payload_and_ctx_digest });
    let full = seq_commit(&SeqCommitInput { parent_seq_commit: &lane.parent_seq_commit, state_root: &state_root });
    let out = ProbeOut {
        node: c.url().unwrap_or_default(),
        network: net,
        block: hash.to_string(),
        daa_score: block.header.daa_score.to_string(),
        smt_proof_bytes: lane.smt_proof.len(),
        has_lane: lane.lane.is_some(),
        commitment_matches: full == block.header.accepted_id_merkle_root,
    };
    let _ = c.disconnect().await;
    Ok(out)
}

/// Connect (the resolver when `url` is undefined), fetch the sink's coinbase-lane proof and
/// check it against the header. Resolves to `{ ok }` or `{ error }` (a string).
#[wasm_bindgen(js_name = wrpcProbe)]
pub async fn wrpc_probe(url: Option<String>) -> JsValue {
    let r = run(url).await;
    let v = match r {
        Ok(o) => serde_json::json!({ "ok": o }),
        Err(e) => serde_json::json!({ "error": e }),
    };
    v.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).unwrap_or(JsValue::NULL)
}
