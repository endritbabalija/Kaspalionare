//! The in-process simnet node (spec A5b): the pinned rusty-kaspa v2.1.0 daemon library,
//! started in this process in the pattern of
//! `refs/rusty-kaspa-v2.1.0/testing/integration/src/daemon_integration_tests.rs:98-134` and
//! `testing/integration/src/common/daemon.rs` (create_core_with_runtime, Core::start,
//! shutdown, join), with:
//! - simnet, `--utxoindex`, `--enable-unsynced-mining` (submit_block otherwise answers
//!   IsInIBD, `rpc/service/src/service.rs:306-311`);
//! - no peers: outbound target 0, inbound limit 0, no DNS seeding, no UPnP, no connect or
//!   add peers (`kaspad/src/daemon.rs:564-570`), the P2P listener on 127.0.0.1;
//! - a Borsh wRPC server on 127.0.0.1 only; gRPC and JSON wRPC off;
//! - a fresh data directory `crates/target/simnet-data/<unix-ms>` per run;
//! - block template cache lifetime 0, so each template follows the block just submitted.
//!
//! `net::check_args` refuses any other configuration before the node starts.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use kaspa_core::{core::Core, signals::Shutdown};
use kaspa_rpc_core::{GetServerInfoResponse, api::rpc::RpcApi};
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use kaspad_lib::{
    args::Args,
    daemon::{Runtime, create_core_with_runtime},
};

use crate::net;

pub const PINNED_VERSION: &str = "2.1.0";

/// The node arguments: simnet only, no peers, loopback listeners (see the module doc).
pub fn node_args(data_dir: &Path, p2p_port: u16, borsh_port: u16) -> Args {
    Args {
        appdir: Some(data_dir.to_string_lossy().into_owned()),
        simnet: true,
        utxoindex: true,
        enable_unsynced_mining: true,
        listen: Some(format!("127.0.0.1:{p2p_port}").as_str().try_into().expect("p2p address")),
        rpclisten_borsh: Some(format!("127.0.0.1:{borsh_port}").parse().expect("borsh address")),
        rpclisten_json: None,
        disable_grpc: true,
        outbound_target: 0,
        inbound_limit: 0,
        connect_peers: vec![],
        add_peers: vec![],
        disable_dns_seeding: true,
        disable_upnp: true,
        block_template_cache_lifetime: Some(0),
        log_level: std::env::var("SIMNET_LOG").unwrap_or_else(|_| "warn".into()),
        async_threads: 2,
        rpc_max_clients: 8,
        yes: true,
        ..Default::default()
    }
}

fn free_port() -> Result<u16, String> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    Ok(l.local_addr().map_err(|e| e.to_string())?.port())
}

/// `crates/target/simnet-data/<unix-ms>`, created fresh.
fn fresh_data_dir() -> Result<PathBuf, String> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("target").join("simnet-data");
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis();
    let dir = base.join(ms.to_string());
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

pub struct Node {
    core: Arc<Core>,
    workers: Option<Vec<std::thread::JoinHandle<()>>>,
    pub data_dir: PathBuf,
    pub url: String,
}

impl Node {
    /// Start a fresh private simnet node in this process.
    pub fn start() -> Result<Node, String> {
        Self::start_on(None)
    }

    /// The same, with the Borsh wRPC listener on 127.0.0.1:`borsh` when given (the dev run's
    /// fixed port, F3), else on a free port.
    pub fn start_on(borsh: Option<u16>) -> Result<Node, String> {
        let data_dir = fresh_data_dir()?;
        let p2p = free_port()?;
        let borsh = match borsh {
            Some(p) => {
                std::net::TcpListener::bind(("127.0.0.1", p)).map_err(|e| format!("127.0.0.1:{p} is not free (another dev run?): {e}"))?;
                p
            }
            None => free_port()?,
        };
        let args = node_args(&data_dir, p2p, borsh);
        net::check_args(&args)?;
        let url = format!("ws://127.0.0.1:{borsh}");
        net::check_url(&url)?;
        let fd_total_budget = (kaspa_utils::fd_budget::limit() - args.rpc_max_clients as i32).max(64);
        let runtime = Runtime::from_args(&args);
        let (core, rpc_core_service) = create_core_with_runtime(&runtime, &args, fd_total_budget);
        // The RpcCoreService must be released before the core shuts down (daemon.rs doc).
        drop(rpc_core_service);
        let workers = core.start();
        println!("simnet node: data {}, P2P 127.0.0.1:{p2p} (no peers, inbound 0), Borsh wRPC {url}", data_dir.display());
        Ok(Node { core, workers: Some(workers), data_dir, url })
    }

    /// Connect a Borsh wRPC client and check the network (simnet), the version, the UTXO index.
    pub async fn client(&self) -> Result<KaspaRpcClient, String> {
        net::check_url(&self.url)?;
        let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&self.url), None, Some(net::network()), None).map_err(|e| e.to_string())?;
        let options = ConnectOptions {
            block_async_connect: true,
            connect_timeout: Some(Duration::from_secs(5)),
            strategy: ConnectStrategy::Fallback,
            ..Default::default()
        };
        let mut last = String::new();
        for _ in 0..30 {
            match client.connect(Some(options.clone())).await {
                Ok(_) => {
                    last.clear();
                    break;
                }
                Err(e) => last = e.to_string(),
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if !last.is_empty() {
            return Err(format!("wRPC connect {}: {last}", self.url));
        }
        let GetServerInfoResponse { server_version, network_id, has_utxo_index, virtual_daa_score, .. } =
            client.get_server_info().await.map_err(|e| e.to_string())?;
        let net_s = network_id.to_string();
        println!("node: version {server_version}, network {net_s}, utxoindex {has_utxo_index}, DAA {virtual_daa_score}");
        if let Err(e) = net::check_network(&net_s) {
            let _ = client.disconnect().await;
            return Err(e);
        }
        if !server_version.starts_with(PINNED_VERSION) {
            return Err(format!("node version {server_version} is not the pinned {PINNED_VERSION}"));
        }
        if !has_utxo_index {
            return Err("node has no UTXO index".into());
        }
        let peers = client.get_connected_peer_info().await.map_err(|e| e.to_string())?.peer_info.len();
        if peers != 0 {
            return Err(format!("node reports {peers} connected peers; the private simnet must have none"));
        }
        Ok(client)
    }

    pub fn shutdown(mut self) {
        self.core.shutdown();
        if let Some(w) = self.workers.take() {
            self.core.join(w);
        }
        println!("simnet node stopped (data left in {})", self.data_dir.display());
    }
}
