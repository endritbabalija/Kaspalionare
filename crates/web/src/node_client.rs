//! The page's node client (front end F3 part A, feature `node`, wasm32): the rusty-kaspa v2.1.0
//! Borsh wRPC client (`kaspa-wrpc-client`, the same version as every other crate here) behind
//! async wasm-bindgen exports. It runs in the page's Web Worker next to the lottery code.
//!
//! Why this client and not the kaspa-wasm npm package: one rusty-kaspa version in the page
//! (the npm package is a second build of the whole SDK), the types stay generated from Rust
//! (`.d.ts`), and `GetSeqCommitLaneProof` (the anchor proof, spec 6.1) exists only in this
//! client, not in kaspa-wasm's JS binding (F1 part C).
//!
//! Every export is async (wasm-bindgen declares it `Promise<…>` around the stated type) and
//! resolves to `Outcome<T>` (never rejects, never throws): `node_unavailable`
//! when the node cannot be reached or times out (the client's own 60 s request timeout),
//! `node_network` when it reports another network (spec 6.6), `node_refused` when it answers
//! with an error. The entries it returns are the node's (F1 re-review N1: every entry the page
//! passes to the lottery code comes from a node by outpoint, never from the indexer).

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::client::{ConnectOptions, ConnectStrategy};
use kaspa_wrpc_client::prelude::{NetworkId, NetworkType};
use kaspa_wrpc_client::{KaspaRpcClient, Resolver, WrpcEncoding};
use lottery::web::{Code, Error, Result};
use wasm_bindgen::prelude::*;

use crate::bindings::{net, out, parse};
use crate::convert::{self, Net};
use crate::dto::{AnchorBlock, Signed, Utxo};
use crate::indexer::ApiObject;
use crate::node::{self, NodeInfo};

thread_local! {
    static CLIENT: RefCell<Option<Arc<KaspaRpcClient>>> = const { RefCell::new(None) };
}

fn client() -> Result<Arc<KaspaRpcClient>> {
    CLIENT.with(|c| c.borrow().clone()).ok_or_else(|| Error::new(Code::NodeUnavailable, "call nodeConnect first"))
}

fn unavailable(what: &str, e: impl std::fmt::Display) -> Error {
    Error::new(Code::NodeUnavailable, format!("{what}: {e}"))
}

/// An RPC error: a transport failure (`node_unavailable`) or the node's own refusal
/// (`node_refused`).
fn rpc_err(what: &str, e: kaspa_rpc_core::RpcError) -> Error {
    // The client reports transport failures as `General` strings (workflow-rpc's errors), so
    // the class comes from the text; a node's own answer (a rejected transaction, a remote
    // error) is a refusal.
    let s = e.to_string();
    let lower = s.to_ascii_lowercase();
    let transport = ["timeout", "timed out", "not connected", "disconnect", "websocket", "connection", "closed"].iter().any(|k| lower.contains(k));
    let code = if transport && !matches!(e, kaspa_rpc_core::RpcError::RejectedTransaction(..)) { Code::NodeUnavailable } else { Code::NodeRefused };
    Error::new(code, format!("{what}: {s}"))
}

fn network_id(n: Net) -> NetworkId {
    match n {
        Net::Testnet10 => NetworkId::with_suffix(NetworkType::Testnet, 10),
        #[cfg(all(feature = "simnet", not(feature = "mainnet")))]
        Net::Simnet => NetworkId::new(NetworkType::Simnet),
    }
}

async fn info_of(c: &KaspaRpcClient, n: Net) -> Result<NodeInfo> {
    let s = c.get_server_info().await.map_err(|e| rpc_err("getServerInfo", e))?;
    node::check_server(n, &s.network_id.to_string(), s.has_utxo_index)?;
    Ok(NodeInfo {
        url: c.url().unwrap_or_default(),
        network: n.name().into(),
        server_version: s.server_version,
        is_synced: s.is_synced,
        virtual_daa_score: s.virtual_daa_score.to_string(),
    })
}

async fn connect(n: Net, url: Option<String>) -> Result<NodeInfo> {
    // workflow-core (the client's timers) treats a Web Worker as a web page and, on its first timer,
    // replaces `window.setTimeout` with a timer worker: in a worker there is no `window`, and that
    // `window().unwrap()` panics (`workflow-core-0.18.0/src/wasm/overrides.rs:251`). The worker's own
    // timers are not throttled the way a background tab's are, so the overrides are not needed here.
    workflow_core::task::disable_persistent_timer_overrides();
    if let Some(u) = &url {
        node::check_url(n, u)?;
    }
    #[cfg(all(feature = "simnet", not(feature = "mainnet")))]
    if n == Net::Simnet && url.is_none() {
        return Err(Error::new(Code::InputFormat, "the simnet has no resolver: give the node URL"));
    }
    disconnect().await;
    let resolver = if url.is_none() { Some(Resolver::default()) } else { None };
    let c = KaspaRpcClient::new(WrpcEncoding::Borsh, url.as_deref(), resolver, Some(network_id(n)), None).map_err(|e| unavailable("client", e))?;
    let opts = ConnectOptions { block_async_connect: true, connect_timeout: Some(Duration::from_secs(20)), strategy: ConnectStrategy::Fallback, ..Default::default() };
    c.connect(Some(opts)).await.map_err(|e| unavailable("connect", e))?;
    match info_of(&c, n).await {
        Ok(i) => {
            CLIENT.with(|x| *x.borrow_mut() = Some(Arc::new(c)));
            Ok(i)
        }
        Err(e) => {
            let _ = c.disconnect().await;
            Err(e)
        }
    }
}

async fn disconnect() {
    let old = CLIENT.with(|x| x.borrow_mut().take());
    if let Some(c) = old {
        let _ = c.disconnect().await;
    }
}

fn dto_of(e: kaspa_rpc_core::RpcUtxosByAddressesEntry) -> Utxo {
    let op: TransactionOutpoint = e.outpoint.into();
    let entry: UtxoEntry = e.utxo_entry.into();
    convert::utxo_dto(&op, &entry)
}

/// The node's UTXOs at one address, as the RPC returns them. rusty-kaspa v2.1.0 has no query
/// by outpoint (`rpc/core/src/api/rpc.rs`: `get_utxos_by_addresses`, line 363, is the only UTXO
/// read), so the answer is everything at the address; its size is bounded only by the wRPC
/// message limit. Callers filter it as it is consumed (F2/F3 review S5), one address at a time,
/// so dust at one address never meets another address's answer.
async fn utxos_at(c: &KaspaRpcClient, n: Net, address: &str) -> Result<Vec<kaspa_rpc_core::RpcUtxosByAddressesEntry>> {
    n.script_of("address", address)?;
    let a = kaspa_addresses::Address::try_from(address).map_err(|e| Error::new(Code::InputFormat, format!("address: {e}")))?;
    c.get_utxos_by_addresses(vec![a]).await.map_err(|e| rpc_err("getUtxosByAddresses", e))
}

async fn entries(n: Net, objects: Vec<ApiObject>) -> Result<Vec<Utxo>> {
    let c = client()?;
    let addrs = node::object_addresses(n, &objects)?;
    let want = node::wanted(&objects);
    let mut out = vec![];
    for a in &addrs {
        let v = utxos_at(&c, n, a).await?;
        let kept = node::keep_outpoints(&want, v, |e| (convert::hex(&e.outpoint.transaction_id.as_bytes()), e.outpoint.index));
        out.extend(kept.into_iter().map(dto_of));
    }
    // One entry per outpoint, also when two objects share an address.
    Ok(node::entries_for(&objects, out))
}

/// The instance's covenant objects at each script (F4: a pool by its script; dust dropped as
/// the answer streams past, at most `SCRIPT_ENTRIES_MAX` per script).
async fn script_entries(n: Net, i: node::ScriptEntriesInput) -> Result<Vec<Utxo>> {
    let c = client()?;
    let mut out = vec![];
    for (script, a) in node::script_addresses(n, &i)? {
        let v = utxos_at(&c, n, &a).await?;
        out.extend(node::script_entries(&i.covenant_id, &script, v.into_iter().map(dto_of)));
    }
    Ok(out)
}

/// The address's plain coins: the `limit` largest, the largest first (None: all, for the balance).
async fn coins(n: Net, address: String, limit: Option<usize>) -> Result<Vec<Utxo>> {
    let c = client()?;
    let a = node::coin_address(n, &address)?;
    let v = utxos_at(&c, n, &a).await?;
    let plain = |e: &kaspa_rpc_core::RpcUtxosByAddressesEntry| e.utxo_entry.covenant_id.is_none();
    let kept = match limit {
        Some(k) => node::largest_coins(v, k, plain, |e| e.utxo_entry.amount),
        None => v.into_iter().filter(plain).collect(),
    };
    Ok(kept.into_iter().map(dto_of).collect())
}

async fn fee_rate() -> Result<u32> {
    let c = client()?;
    let e = c.get_fee_estimate().await.map_err(|e| rpc_err("getFeeEstimate", e))?;
    let b = e.normal_buckets.first().ok_or_else(|| Error::new(Code::NodeRefused, "getFeeEstimate: no normal bucket"))?;
    node::round_rate(b.feerate)
}

async fn anchor_block(hash: String) -> Result<AnchorBlock> {
    let c = client()?;
    let h = Hash::from_bytes(convert::hash32("hash", &hash)?);
    let blk = c.get_block(h, false).await.map_err(|e| rpc_err("getBlock", e))?;
    let header = kaspa_consensus_core::header::Header::try_from(&blk.header).map_err(|e| Error::new(Code::NodeRefused, format!("getBlock: header: {e}")))?;
    let is_chain_block = blk.verbose_data.as_ref().is_some_and(|v| v.is_chain_block);
    Ok(AnchorBlock { header: convert::header_dto(&header), is_chain_block })
}

async fn submit(n: Net, s: Signed) -> Result<String> {
    let c = client()?;
    let (tx, _, _) = crate::safe_json::from_json(&s.transaction).map_err(|e| Error::new(Code::InputFormat, format!("signed transaction: {e}")))?;
    if convert::hex(&tx.id().as_bytes()) != s.id.to_ascii_lowercase() {
        return Err(Error::new(Code::InputFormat, "the signed transaction's id is not its stated id"));
    }
    let _ = n;
    // An input already spent (shard contention, spec S1) is `node_conflict`: the page rebuilds.
    let id = c.submit_transaction((&tx).into(), false).await.map_err(|e| node::submit_refusal(rpc_err("submitTransaction", e)))?;
    Ok(convert::hex(&id.as_bytes()))
}

/// Connect to a node on the started network: `url` (`ws://` or `wss://`), or a public node
/// from the resolver when undefined (testnet-10 only). Refuses a node on another network or
/// without a UTXO index (`node_network`). Replaces any earlier connection.
#[wasm_bindgen(js_name = nodeConnect, unchecked_return_type = "Outcome<NodeInfo>")]
pub async fn node_connect(url: Option<String>) -> JsValue {
    out(match net() {
        Ok(n) => connect(n, url).await,
        Err(e) => Err(e),
    })
}

/// Close the node connection, if any.
#[wasm_bindgen(js_name = nodeDisconnect, unchecked_return_type = "Outcome<null>")]
pub async fn node_disconnect() -> JsValue {
    disconnect().await;
    out(Ok(()))
}

/// The connected node's network, version, sync state and virtual DAA score.
#[wasm_bindgen(js_name = nodeInfo, unchecked_return_type = "Outcome<NodeInfo>")]
pub async fn node_info() -> JsValue {
    let r = async {
        let n = net()?;
        let c = client()?;
        info_of(&c, n).await
    };
    out(r.await)
}

/// The node's live entries for the indexer's objects (by their scripts' addresses, kept for
/// exactly the objects' outpoints): the `utxos` for `checkIndexerObjects`/`checkIndexerRound`.
#[wasm_bindgen(js_name = nodeEntries, unchecked_return_type = "Outcome<Utxo[]>")]
pub async fn node_entries(#[wasm_bindgen(unchecked_param_type = "ApiObject[]")] objects: JsValue) -> JsValue {
    let r = async {
        let n = net()?;
        let objects: Vec<ApiObject> = parse("objects", objects)?;
        entries(n, objects).await
    };
    out(r.await)
}

/// The instance's covenant objects at these scripts, as a public node holds them (F4: a prize
/// pool found by its script, which its claims never change; at most 8 per script).
#[wasm_bindgen(js_name = nodeScriptEntries, unchecked_return_type = "Outcome<Utxo[]>")]
pub async fn node_script_entries(#[wasm_bindgen(unchecked_param_type = "ScriptEntriesInput")] input: JsValue) -> JsValue {
    let r = async {
        let n = net()?;
        script_entries(n, parse("input", input)?).await
    };
    out(r.await)
}

/// A wallet address's plain (non-covenant) coins as the node reports them: the
/// `COINS_LISTED_MAX` largest, the largest first (dust beyond them is never listed).
#[wasm_bindgen(js_name = nodeCoins, unchecked_return_type = "Outcome<Utxo[]>")]
pub async fn node_coins(address: String) -> JsValue {
    let r = async { coins(net()?, address, Some(node::COINS_LISTED_MAX)).await };
    out(r.await)
}

/// A wallet address's balance (sompi): the total of its plain coins as the node reports them
/// (spec S7; never the wallet's own word).
#[wasm_bindgen(js_name = nodeBalance, unchecked_return_type = "Outcome<Dec>")]
pub async fn node_balance(address: String) -> JsValue {
    let r = async { node::coins_total(&coins(net()?, address, None).await?) };
    out(r.await)
}

/// The node's normal fee estimate as the builders' rate (sompi/gram; 100..=1000).
#[wasm_bindgen(js_name = nodeFeeRate, unchecked_return_type = "Outcome<number>")]
pub async fn node_fee_rate() -> JsValue {
    let r = async {
        net()?;
        fee_rate().await
    };
    out(r.await)
}

/// A block's header and whether the node has it on its selected chain: the `anchorBlock` for
/// `winningNumbers` and `checkIndexerRound` (F1 re-review S1: never from the indexer).
#[wasm_bindgen(js_name = nodeAnchorBlock, unchecked_return_type = "Outcome<AnchorBlock>")]
pub async fn node_anchor_block(hash: String) -> JsValue {
    let r = async {
        net()?;
        anchor_block(hash).await
    };
    out(r.await)
}

/// Submit an assembled transaction (`assemble`'s `Signed`); resolves to its id.
#[wasm_bindgen(js_name = nodeSubmit, unchecked_return_type = "Outcome<string>")]
pub async fn node_submit(#[wasm_bindgen(unchecked_param_type = "Signed")] signed: JsValue) -> JsValue {
    let r = async {
        let n = net()?;
        let s: Signed = parse("signed", signed)?;
        submit(n, s).await
    };
    out(r.await)
}
