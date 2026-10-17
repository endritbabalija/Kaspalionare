//! On-demand miner (spec A5b "Mining"): `get_block_template` to a simnet address, then
//! `submit_block` of that template unchanged (timestamp and nonce from the template; simnet
//! skips the proof-of-work check, `consensus/core/src/config/params.rs:737`,
//! `consensus/src/pipeline/header_processor/pre_ghostdag_validation.rs:102-105`). One block
//! at a time, each on the previous one, so the chain is linear. Lottery transactions reach
//! blocks only through `submit_transaction` and the node's own templates.

use std::time::{Duration, Instant};

use kaspa_addresses::Address;
use kaspa_consensus_core::header::Header;
use kaspa_hashes::Hash;
use kaspa_rpc_core::{RpcTransactionId, SubmitBlockReport, api::rpc::RpcApi};

use crate::net;

/// One mined block: its hash, DAA score, timestamp and the number of non-coinbase transactions.
#[derive(Clone, Copy, Debug)]
pub struct Mined {
    pub hash: Hash,
    pub daa: u64,
    pub timestamp: u64,
    pub txs: usize,
}

/// Mine one block paying `pay`, on top of the current sink, and wait until it is the sink.
pub async fn mine_one<C: RpcApi + ?Sized>(c: &C, pay: &Address) -> Result<Mined, String> {
    net::check_simnet(pay)?;
    let sink = c.get_block_dag_info().await.map_err(|e| e.to_string())?.sink;
    let template = c.get_block_template(pay.clone(), vec![]).await.map_err(|e| format!("get_block_template: {e}"))?;
    let header: Header = (&template.block.header).try_into().map_err(|e| format!("template header: {e:?}"))?;
    // Linear chain: the template's selected parent must be the current sink.
    if !header.direct_parents().contains(&sink) || header.direct_parents().len() != 1 {
        return Err(format!("template parents {:?} are not exactly the sink {sink}", header.direct_parents()));
    }
    let mined = Mined { hash: header.hash, daa: header.daa_score, timestamp: header.timestamp, txs: template.block.transactions.len().saturating_sub(1) };
    let r = c.submit_block(template.block, false).await.map_err(|e| format!("submit_block: {e}"))?;
    if r.report != SubmitBlockReport::Success {
        return Err(format!("submit_block {}: {:?}", mined.hash, r.report));
    }
    let t0 = Instant::now();
    loop {
        let s = c.get_block_dag_info().await.map_err(|e| e.to_string())?.sink;
        if s == mined.hash {
            return Ok(mined);
        }
        if t0.elapsed() > Duration::from_secs(10) {
            return Err(format!("block {} is not the sink after 10 s (sink {s})", mined.hash));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Mine `n` blocks; returns the last.
pub async fn mine<C: RpcApi + ?Sized>(c: &C, pay: &Address, n: u64) -> Result<Option<Mined>, String> {
    let mut last = None;
    for _ in 0..n {
        last = Some(mine_one(c, pay).await?);
    }
    Ok(last)
}

/// Mine until the virtual DAA score is greater than `target`.
pub async fn mine_past_daa<C: RpcApi + ?Sized>(c: &C, pay: &Address, target: u64) -> Result<u64, String> {
    loop {
        let v = c.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
        if v > target {
            return Ok(v);
        }
        mine_one(c, pay).await?;
    }
}

/// The chain block that accepted `txid`, and its DAA score: mine blocks (at most `max`)
/// until a selected-chain block after `start` lists the transaction as accepted
/// (`get_virtual_chain_from_block` with accepted ids; the virtual's own acceptance is not a
/// chain block). Outputs of the transaction then carry that block's DAA score in the UTXO set
/// (`pov_daa_score = header.daa_score`,
/// `consensus/src/pipeline/virtual_processor/processor.rs:446-455`).
pub async fn mine_until_accepted<C: RpcApi + ?Sized>(c: &C, pay: &Address, start: Hash, txid: RpcTransactionId, max: u32) -> Result<(Hash, u64), String> {
    for _ in 0..max {
        mine_one(c, pay).await?;
        let vc = c.get_virtual_chain_from_block(start, true, None).await.map_err(|e| e.to_string())?;
        if let Some(a) = vc.accepted_transaction_ids.iter().find(|a| a.accepted_transaction_ids.contains(&txid)) {
            let daa = c.get_block(a.accepting_block_hash, false).await.map_err(|e| e.to_string())?.header.daa_score;
            return Ok((a.accepting_block_hash, daa));
        }
    }
    Err(format!("{txid} not accepted by a chain block within {max} mined blocks"))
}
