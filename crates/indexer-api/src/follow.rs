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

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use kaspa_hashes::Hash;
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
use crate::node_indexer::{self, Indexer};
use crate::server::{AnchorProvider, FollowHealth};

/// The follower's options (`--node`, `--start`, `--node-poll-ms`).
#[derive(Clone, Debug)]
pub struct FollowOpts {
    /// The node's wRPC (Borsh) URL; `None`: a public node from the rusty-kaspa resolver.
    pub node: Option<String>,
    /// The chain block to start a new store from (before genesis).
    pub start: Option<Hash>,
    /// Time between polls once caught up.
    pub every: Duration,
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
    let mut x = open_indexer(&cfg, &fo, &first).await?;
    let shared: Shared = Arc::new(RwLock::new(Arc::new(first)));
    let starts: Starts = Default::default();
    note_closes(&x, &starts);
    let health = Arc::new(FollowHealth::new(cfg.stale));
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
        let polled = tokio::select! {
            _ = &mut until => break Ok(()),
            r = node_indexer::poll(&*c, &mut x) => r,
        };
        let pause = match polled {
            Ok(p) => {
                fails = 0;
                health.ok(x.last_daa);
                note_closes(&x, &starts);
                if p.kept > 0 || p.reverted > 0 {
                    println!("follow: {} chain blocks, {} kept, {} reverted; last DAA {}; gaps {:?}", p.blocks, p.kept, p.reverted, x.last_daa, x.ix.gaps());
                }
                fo.every
            }
            Err(e) => {
                fails += 1;
                eprintln!("follow: poll failed ({fails} in a row): {e}");
                health.err(e.clone(), !node_indexer::transient(&e));
                if fails >= 2 && node_indexer::transient(&e) {
                    match connect(fo.node.as_deref(), net).await {
                        Ok(n) => {
                            let old = current(&shared);
                            if let Ok(mut g) = shared.write() {
                                *g = Arc::new(n);
                            }
                            let _ = old.disconnect().await;
                            println!("follow: reconnected");
                        }
                        Err(ce) => eprintln!("follow: reconnect failed: {ce}"),
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
    println!("follow: stopped at chain block {} (DAA {}), {} log lines persisted", x.last, x.last_daa, x.lines().len());
    result
}
