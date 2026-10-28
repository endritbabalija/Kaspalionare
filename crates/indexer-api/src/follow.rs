//! The one-process mode (F6 part C; spec frontend 6.3, "What F6 needs"): the M8 indexer
//! follows a node and writes the store, and the API serves it, with a node-backed anchor
//! proof for the closing round. `indexer-api --store <dir> --cov <hex> --node <url|resolver>`.
//!
//! - **The indexer** is `tn10 indexer`'s code (`tn10/src/indexer.rs`, compiled in here as
//!   `crate::node_indexer`): `get_virtual_chain_from_block_v2` with Full verbosity from the
//!   store's last chain block, every batch ingested and persisted (log first, then
//!   `index.json` by rename) before the next RPC call. So a stop at any point (a signal, a
//!   crash, a reboot) loses nothing: the next start resumes from the log (graceful restart
//!   from the store). A new store needs `--start <chain block before genesis>`.
//! - **The API** follows the same log as in the store-only mode (`store::Follower`), so what
//!   it serves is exactly what the standalone binary would serve from that store.
//! - **The anchor proof** before the draw is built from the node (`tn10/src/anchor.rs`,
//!   `crate::node_anchor::rpc_anchor`, as `simnet dev` does), behind `CachedAnchor` (one build
//!   at a time, kept 60 s, a failure 5 s). The page checks it against the node's block and D
//!   (spec 6.3: nothing from the API is trusted).
//! - **Node failures** never stop the API: the follower retries (5 s doubling to 120 s, a new
//!   connection after two failures in a row) and `/health` says 503 with the error until a
//!   poll succeeds again. A resume block the node no longer has (pruned: the server was off
//!   longer than the node keeps blocks) is reported the same way; it needs
//!   `tn10 campaign reanchor` on the store (docs/launch/indexer-server.md).
//! - **Network guard:** the node must report the bundle's network (testnet-10, or the
//!   private simnet in tests), be synced and have a UTXO index; a URL on a mainnet default
//!   port is refused before connecting.
//! - **The followed node is pinned per run** (F6 security review S4): with `resolver`, the node
//!   the resolver hands out first is kept for every reconnect of this run (a restart may pick
//!   another), so one run never mixes answers of several nodes.
//! - **A witness node** (S4, `--witness <url|resolver|off>`): before a batch is persisted, a
//!   second node must report the same: the same chain blocks removed, and for each added chain
//!   block the same hash and the same accepted transactions (ID and signature scripts; the
//!   witness is asked from the same block, at Low verbosity). Only the prefix both report is
//!   persisted (`agree`); the rest waits for the next poll. A chain block both name with
//!   different transactions is a lie of one of them: nothing is persisted and `/health` is 503
//!   at once until they agree again. A primary whose blocks the witness never confirms makes no
//!   progress, and `/health` turns 503 after the staleness limit. So one bad node can stop the
//!   store from advancing, but cannot write into it. The witness must be another node than the
//!   followed one (with `resolver`, a different node is asked for); a testnet-10 instance needs
//!   `--witness` set (`off` accepts the single node).

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use kaspa_consensus_core::tx::TransactionId;
use kaspa_hashes::Hash;
use kaspa_rpc_core::RpcDataVerbosityLevel;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, Resolver, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
    prelude::{NetworkId, NetworkType},
};
use web::convert::Net;

use crate::config::Config;
use crate::limits::CachedAnchor;
use crate::node_anchor;
use crate::node_indexer::{self, ChainBatch, Indexer};
use crate::server::{AnchorProvider, FollowHealth};

/// The follower's options (`--node`, `--witness`, `--start`, `--node-poll-ms`).
#[derive(Clone, Debug)]
pub struct FollowOpts {
    /// The node's wRPC (Borsh) URL; `None`: a public node from the rusty-kaspa resolver.
    pub node: Option<String>,
    /// The chain block to start a new store from (before genesis).
    pub start: Option<Hash>,
    /// Time between polls once caught up.
    pub every: Duration,
    /// The second node that confirms every batch before it is persisted.
    pub witness: Witness,
}

/// `--witness`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Witness {
    /// Not given: refused for a testnet-10 instance (the operator chooses).
    Unset,
    /// One node only (the operator accepts it).
    Off,
    /// A public node from the resolver, another than the followed one.
    Resolver,
    Url(String),
}

/// What a node reports for one call from a chain block: the chain blocks removed (sorted), and
/// per added chain block its hash and its accepted transactions (ID and signature scripts),
/// sorted by ID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChainView {
    pub removed: Vec<Hash>,
    pub added: Vec<(Hash, Vec<(TransactionId, Vec<Vec<u8>>)>)>,
}

/// The view of the followed node's batch (Full verbosity, already converted by `batch_of`).
pub fn view_of_batch(b: &ChainBatch) -> ChainView {
    let mut removed = b.removed.clone();
    removed.sort();
    let added = b
        .added
        .iter()
        .map(|k| {
            let mut txs: Vec<_> = k.txs.iter().map(|t| (t.tx.id(), t.tx.inputs.iter().map(|i| i.signature_script.clone()).collect())).collect();
            txs.sort();
            (k.hash, txs)
        })
        .collect();
    ChainView { removed, added }
}

/// The view of a witness's answer (Low verbosity: header hashes, transaction IDs and signature
/// scripts). A missing field is an error (the witness then confirms nothing).
pub fn view_of_response(r: &kaspa_rpc_core::GetVirtualChainFromBlockV2Response) -> Result<ChainView, String> {
    let hashes = r.added_chain_block_hashes.as_ref();
    let acc = r.chain_block_accepted_transactions.as_ref();
    if hashes.len() != acc.len() {
        return Err(format!("witness: {} added chain blocks but {} acceptance entries", hashes.len(), acc.len()));
    }
    let mut added = Vec::with_capacity(acc.len());
    for (h, a) in hashes.iter().zip(acc) {
        if a.chain_block_header.hash != Some(*h) {
            return Err(format!("witness: the acceptance entry of chain block {h} names another header"));
        }
        let mut txs = Vec::with_capacity(a.accepted_transactions.len());
        for t in &a.accepted_transactions {
            let id = t.verbose_data.as_ref().and_then(|v| v.transaction_id).ok_or_else(|| format!("witness: chain block {h}: a transaction without its ID"))?;
            let sigs = t.inputs.iter().map(|i| i.signature_script.clone().ok_or_else(|| format!("witness: chain block {h}: transaction {id}: an input without its signature script"))).collect::<Result<Vec<_>, _>>()?;
            txs.push((id, sigs));
        }
        txs.sort();
        added.push((*h, txs));
    }
    let mut removed = r.removed_chain_block_hashes.as_ref().clone();
    removed.sort();
    Ok(ChainView { removed, added })
}

/// How much of the followed node's batch the witness confirms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Agreement {
    /// The first `n` added chain blocks (and the removals, when n > 0 or there is nothing to add)
    /// are reported the same by both; the rest waits.
    Take(usize),
    /// The two nodes report one chain block with different transactions: one of them lies.
    Diverged(String),
}

/// `primary` against `witness`, both from the same start block (see the module doc).
pub fn agree(primary: &ChainView, witness: &ChainView) -> Agreement {
    if primary.removed != witness.removed {
        // A reorg one node has seen and the other not (yet): nothing is taken, not even the removals.
        return Agreement::Take(0);
    }
    let mut n = 0;
    for ((ph, ptx), (wh, wtx)) in primary.added.iter().zip(&witness.added) {
        if ph != wh {
            break;
        }
        if ptx != wtx {
            return Agreement::Diverged(format!("chain block {ph}: the followed node and the witness report different accepted transactions ({} and {})", ptx.len(), wtx.len()));
        }
        n += 1;
    }
    Agreement::Take(n)
}

/// What one witnessed poll did.
#[derive(Clone, Debug, Default)]
pub struct Witnessed {
    pub blocks: usize,
    pub kept: usize,
    pub reverted: usize,
    /// Chain blocks the followed node reported and the witness did not confirm (yet).
    pub unconfirmed: usize,
}

/// One step: `batch` from the followed node, confirmed by `witness` (`None`: no witness), ingested
/// and persisted as far as confirmed. Returns (what was done, whether the followed node had more).
/// A divergence is an error and nothing of the batch is ingested.
pub fn apply_witnessed(x: &mut Indexer, mut batch: ChainBatch, witness: Option<&ChainView>) -> Result<(Witnessed, bool), String> {
    let offered = batch.added.len();
    let mut out = Witnessed::default();
    if let Some(w) = witness {
        match agree(&view_of_batch(&batch), w) {
            Agreement::Diverged(why) => return Err(format!("{DIVERGED}: {why}; nothing persisted")),
            Agreement::Take(n) => {
                if n == 0 && (offered > 0 || batch.removed != w.removed) {
                    out.unconfirmed = offered.max(1);
                    return Ok((out, false));
                }
                batch.added.truncate(n);
                out.unconfirmed = offered - n;
            }
        }
    }
    let g = x.ingest(batch)?;
    x.persist()?;
    out.blocks = g.blocks;
    out.kept = g.kept;
    out.reverted = g.reverted;
    let more = offered > 0 && out.unconfirmed == 0;
    Ok((out, more))
}

/// The prefix of a divergence error (`/health` 503 at once).
pub const DIVERGED: &str = "the followed node and the witness disagree";

/// Poll `c` until caught up, each batch confirmed by `w` before it is persisted.
pub async fn poll_witnessed<C: RpcApi + ?Sized, W: RpcApi + ?Sized>(c: &C, w: Option<&W>, x: &mut Indexer) -> Result<Witnessed, String> {
    let mut total = Witnessed::default();
    loop {
        let from = x.last;
        let r = c.get_virtual_chain_from_block_v2(from, Some(RpcDataVerbosityLevel::Full), None).await.map_err(|e| {
            let e = e.to_string();
            if node_indexer::transient(&e) {
                format!("get_virtual_chain_from_block_v2 from {from} (DAA {}): {e} (node or connection failure; the position is saved)", x.last_daa)
            } else {
                format!("get_virtual_chain_from_block_v2 from {from} (DAA {}): {e}; the resume block is unknown to this node or below its retention root: stopping, nothing skipped", x.last_daa)
            }
        })?;
        let b = node_indexer::batch_of(r)?;
        let wv = match w {
            Some(w) => {
                let r = w.get_virtual_chain_from_block_v2(from, Some(RpcDataVerbosityLevel::Low), None).await.map_err(|e| format!("witness: get_virtual_chain_from_block_v2 from {from}: {e}"))?;
                Some(view_of_response(&r)?)
            }
            None => None,
        };
        let (done, more) = apply_witnessed(x, b, wv.as_ref())?;
        total.blocks += done.blocks;
        total.kept += done.kept;
        total.reverted += done.reverted;
        total.unconfirmed = done.unconfirmed;
        if !more {
            return Ok(total);
        }
    }
}

type Shared = Arc<RwLock<Arc<KaspaRpcClient>>>;
type Starts = Arc<std::sync::Mutex<std::collections::BTreeMap<u64, Hash>>>;

fn current(c: &Shared) -> Arc<KaspaRpcClient> {
    match c.read() {
        Ok(g) => g.clone(),
        Err(p) => p.into_inner().clone(),
    }
}

/// Refuse a URL on a rusty-kaspa mainnet default port (16110 gRPC, 17110 wRPC Borsh,
/// 18110 wRPC JSON) before any connection, as `tn10/src/net.rs` does.
pub fn check_url(url: &str) -> Result<(), String> {
    if [":16110", ":17110", ":18110"].iter().any(|p| url.contains(p)) || url.to_ascii_lowercase().contains("mainnet") {
        return Err(format!("{url} looks like a mainnet node; refusing"));
    }
    Ok(())
}

fn network_id(net: Net) -> NetworkId {
    match net {
        Net::Testnet10 => NetworkId::with_suffix(NetworkType::Testnet, 10),
        #[allow(unreachable_patterns)]
        _ => NetworkId::new(NetworkType::Simnet),
    }
}

/// The host of a wRPC URL (for the witness's independence check).
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    rest.split(['/', '?']).next().unwrap_or("").to_ascii_lowercase()
}

/// Connect the witness (`--witness`): another node than `primary` (by host; on the private
/// simnet, with its one node, the same node is accepted for the follow check).
pub async fn connect_witness(w: &Witness, primary: &str, net: Net) -> Result<Option<KaspaRpcClient>, String> {
    let other = |u: &str| net != Net::Testnet10 || host_of(u) != host_of(primary);
    match w {
        Witness::Off => Ok(None),
        Witness::Unset if net == Net::Testnet10 => Err("a testnet-10 instance needs --witness <wss-url|resolver> (a second node that confirms every batch), or --witness off to follow one node unchecked".into()),
        Witness::Unset => Ok(None),
        Witness::Url(u) => {
            if !other(u) {
                return Err(format!("--witness {u}: the same node as the followed one ({primary})"));
            }
            Ok(Some(connect(Some(u), net).await?))
        }
        Witness::Resolver => {
            for _ in 0..8 {
                let c = connect(None, net).await?;
                let u = c.url().unwrap_or_default();
                if other(&u) {
                    return Ok(Some(c));
                }
                let _ = c.disconnect().await;
            }
            Err(format!("--witness resolver: the resolver handed out only the followed node ({primary}) in 8 tries; give a witness URL"))
        }
    }
}

/// Connect to `url` (or a public node from the resolver) and refuse a node of another
/// network than `net`, one not synced, or one without a UTXO index.
pub async fn connect(url: Option<&str>, net: Net) -> Result<KaspaRpcClient, String> {
    if let Some(u) = url {
        check_url(u)?;
    }
    let resolver = url.is_none().then(Resolver::default);
    let c = KaspaRpcClient::new(WrpcEncoding::Borsh, url, resolver, Some(network_id(net)), None).map_err(|e| e.to_string())?;
    let o = ConnectOptions { block_async_connect: true, connect_timeout: Some(Duration::from_secs(20)), strategy: ConnectStrategy::Fallback, ..Default::default() };
    c.connect(Some(o)).await.map_err(|e| format!("connect: {e}"))?;
    let info = c.get_server_info().await.map_err(|e| e.to_string())?;
    let got = info.network_id.to_string();
    if got != net.name() {
        let _ = c.disconnect().await;
        return Err(format!("the node reports network {got}, the instance is on {}; refusing", net.name()));
    }
    if !info.is_synced || !info.has_utxo_index {
        let _ = c.disconnect().await;
        return Err(format!("the node is not synced ({}) or has no UTXO index ({})", info.is_synced, info.has_utxo_index));
    }
    println!("follow: node {} version {}, network {got}, virtual DAA {}", c.url().unwrap_or_default(), info.server_version, info.virtual_daa_score);
    Ok(c)
}

/// The closing round's anchor proof, built from the node.
struct NodeAnchor {
    c: Shared,
    starts: Starts,
}

impl AnchorProvider for NodeAnchor {
    fn proof(&self, round: i64, d: u64, marker_daa: u64) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + '_>> {
        Box::pin(async move {
            let start = self.starts.lock().map_err(|e| e.to_string())?.get(&marker_daa).copied().ok_or(format!("round {round}: its close's chain block is not indexed yet"))?;
            let c = current(&self.c);
            let virt = c.get_block_dag_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
            if virt < d {
                return Err(format!("round {round}: the anchor reference D = {d} is not reached yet (virtual DAA {virt})"));
            }
            let to = node_anchor::Timeouts { anchor: Duration::from_secs(20), blue_score: Duration::from_secs(5), poll: Duration::from_millis(250) };
            let a = node_anchor::rpc_anchor(&*c, start, d, &to).await?;
            Ok(a.proof.to_bytes().to_vec())
        })
    }
}

/// Record the accepting chain block of every closed round's `close()` (the anchor search
/// starts there).
fn note_closes(x: &Indexer, starts: &Starts) {
    let Ok(mut m) = starts.lock() else { return };
    for r in x.ix.rounds().values() {
        if let Some(cl) = &r.close {
            if let Some(h) = x.chain_block_of(&cl.txid) {
                m.insert(cl.marker_daa, h);
            }
        }
    }
}

/// Wait for Ctrl-C or (Unix) SIGTERM, the signal systemd and Docker send on stop.
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut t) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = t.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// The wait before retry `n` (1-based): 5 s doubling, at most 120 s.
pub fn backoff(n: u32) -> Duration {
    Duration::from_secs((5u64 << n.saturating_sub(1).min(5)).min(120))
}

/// Open the indexer on the store (resume from its log, or start a new one at `--start`),
/// catch nothing up yet, and persist it so the API's follower finds a log.
async fn open_indexer(cfg: &Config, fo: &FollowOpts, c: &KaspaRpcClient) -> Result<Indexer, String> {
    let (_, _, t, genesis) = crate::store::read_bundle(&cfg.bundle, cfg.cov)?;
    let log = cfg.store.join(node_indexer::LOG_FILE);
    let (start, sdaa) = if log.exists() {
        (Hash::from_bytes([0; 32]), 0)
    } else {
        let h = fo.start.ok_or_else(|| format!("{}: no index log; a new store needs --start <chain block before genesis>", log.display()))?;
        let d = c.get_block(h, false).await.map_err(|e| format!("--start {h}: {e}"))?.header.daa_score;
        (h, d)
    };
    std::fs::create_dir_all(&cfg.store).map_err(|e| format!("{}: {e}", cfg.store.display()))?;
    let mut x = Indexer::open(Some(&cfg.store), t, cfg.cov, Some(genesis), start, sdaa)?;
    x.persist()?;
    Ok(x)
}

/// Run the one-process mode until a stop signal (or `until` resolves, for tests): follow the
/// node into the store and serve it. Returns after the current batch is persisted.
pub async fn run_following(cfg: Config, fo: FollowOpts, until: impl Future<Output = ()> + Send) -> Result<(), String> {
    let (_, net, _, _) = crate::store::read_bundle(&cfg.bundle, cfg.cov)?;
    let first = connect(fo.node.as_deref(), net).await?;
    // S4: the node this run follows, pinned for every reconnect (a resolver pick is kept).
    let pinned = first.url().or(fo.node.clone()).ok_or("the followed node has no URL")?;
    let witness = connect_witness(&fo.witness, &pinned, net).await?;
    let witness_url = witness.as_ref().and_then(|w| w.url());
    match &witness_url {
        Some(w) => println!("follow: pinned to {pinned}; witness {w} confirms every batch before it is persisted"),
        None => println!("follow: pinned to {pinned}; NO witness: batches are persisted as this one node reports them"),
    }
    let witness: Option<Shared> = witness.map(|w| Arc::new(RwLock::new(Arc::new(w))));
    let mut x = open_indexer(&cfg, &fo, &first).await?;
    let shared: Shared = Arc::new(RwLock::new(Arc::new(first)));
    let starts: Starts = Default::default();
    note_closes(&x, &starts);
    let health = Arc::new(FollowHealth::new(cfg.stale));
    health.set_nodes(pinned.clone(), witness_url.clone());
    let mut acfg = cfg.clone();
    let wait = cfg.request_timeout.saturating_sub(Duration::from_secs(1)).max(Duration::from_millis(500));
    let cache = CachedAnchor::new(Arc::new(NodeAnchor { c: shared.clone(), starts: starts.clone() }), Duration::from_secs(60), Duration::from_secs(5), wait);
    acfg.anchor = Some(Arc::new(cache));
    let mut st = crate::server::open(&acfg)?;
    st.follow = Some(health.clone());
    let router = crate::server::router(st.clone(), &acfg.allow_origins)?;
    let listener = tokio::net::TcpListener::bind(acfg.bind).await.map_err(|e| format!("bind {}: {e}", acfg.bind))?;
    let s = st.snapshot();
    println!(
        "indexer-api: http://{}/v1/ for {} instance {} following {} ({} log lines, {} rounds, {} live objects); resume from {} (DAA {})",
        listener.local_addr().map_err(|e| e.to_string())?,
        s.net.name(),
        s.cov,
        fo.node.as_deref().unwrap_or("a public node (resolver)"),
        s.log_lines,
        s.rounds.len(),
        s.live.len(),
        x.last,
        x.last_daa
    );
    let server = tokio::spawn(async move { crate::server::serve(listener, router, &acfg).await });
    let mut until = Box::pin(until);
    let mut fails = 0u32;
    let result = loop {
        if server.is_finished() {
            break Err("the HTTP server stopped".to_string());
        }
        let c = current(&shared);
        let w = witness.as_ref().map(current);
        let polled = tokio::select! {
            _ = &mut until => break Ok(()),
            r = poll_witnessed(&*c, w.as_deref(), &mut x) => r,
        };
        let pause = match polled {
            Ok(p) => {
                fails = 0;
                note_closes(&x, &starts);
                if p.kept > 0 || p.reverted > 0 {
                    println!("follow: {} chain blocks, {} kept, {} reverted; last DAA {}; gaps {:?}", p.blocks, p.kept, p.reverted, x.last_daa, x.ix.gaps());
                }
                if p.blocks == 0 && p.unconfirmed > 0 {
                    // No progress: the witness confirms none of what the followed node reports (yet).
                    // Not a good poll: /health turns 503 once this lasts past the staleness limit.
                    let e = format!("the witness has not confirmed the followed node's next {} chain blocks (from DAA {})", p.unconfirmed, x.last_daa);
                    eprintln!("follow: {e}");
                    health.err(e, false);
                } else {
                    health.ok(x.last_daa);
                }
                fo.every
            }
            Err(e) => {
                fails += 1;
                eprintln!("follow: poll failed ({fails} in a row): {e}");
                let diverged = e.starts_with(DIVERGED);
                health.err(e.clone(), diverged || !node_indexer::transient(&e));
                if fails >= 2 && node_indexer::transient(&e) {
                    // Reconnect the pinned node that failed (never a new resolver pick).
                    let (which, url) = match (&witness, &witness_url) {
                        (Some(ws), Some(wu)) if e.starts_with("witness") => (ws.clone(), wu.clone()),
                        _ => (shared.clone(), pinned.clone()),
                    };
                    match connect(Some(&url), net).await {
                        Ok(n) => {
                            let old = current(&which);
                            if let Ok(mut g) = which.write() {
                                *g = Arc::new(n);
                            }
                            let _ = old.disconnect().await;
                            println!("follow: reconnected to {url}");
                        }
                        Err(ce) => eprintln!("follow: reconnect to {url} failed: {ce}"),
                    }
                }
                backoff(fails)
            }
        };
        tokio::select! {
            _ = &mut until => break Ok(()),
            _ = tokio::time::sleep(pause) => {}
        }
    };
    server.abort();
    let _ = current(&shared).disconnect().await;
    if let Some(w) = &witness {
        let _ = current(w).disconnect().await;
    }
    println!("follow: stopped at chain block {} (DAA {}), {} log lines persisted", x.last, x.last_daa, x.lines().len());
    result
}
