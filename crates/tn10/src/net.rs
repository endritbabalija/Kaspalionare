//! Connection to a testnet-10 node, with the network guard.

use std::time::Duration;

use kaspa_addresses::{Address, Prefix};
use kaspa_rpc_core::{GetServerInfoResponse, RpcUtxosByAddressesEntry, api::rpc::RpcApi};
use kaspa_wrpc_client::{
    KaspaRpcClient, Resolver, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
    prelude::{NetworkId, NetworkType},
};

pub const NETWORK: &str = "testnet-10";
pub const PINNED_VERSION: &str = "2.1.0";

/// Connect to `url`, or to a public testnet-10 node via the resolver, and
/// refuse anything that is not testnet-10.
pub async fn connect(url: Option<String>) -> Result<KaspaRpcClient, String> {
    if let Some(u) = &url {
        // Same ports the project hook blocks: rusty-kaspa mainnet defaults.
        if [":16110", ":17110", ":18110"].iter().any(|p| u.contains(p)) {
            return Err(format!("{u} uses a mainnet default port; refusing"));
        }
    }
    let resolver = if url.is_none() { Some(Resolver::default()) } else { None };
    let network = NetworkId::with_suffix(NetworkType::Testnet, 10);
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, url.as_deref(), resolver, Some(network), None)
        .map_err(|e| e.to_string())?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_secs(15)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    client.connect(Some(options)).await.map_err(|e| e.to_string())?;
    let GetServerInfoResponse { server_version, network_id, is_synced, has_utxo_index, virtual_daa_score, .. } =
        client.get_server_info().await.map_err(|e| e.to_string())?;
    let net = network_id.to_string();
    println!(
        "Node: {} version {server_version}, network {net}, synced {is_synced}, utxoindex {has_utxo_index}, DAA {virtual_daa_score}",
        client.url().unwrap_or_default()
    );
    if net != NETWORK {
        let _ = client.disconnect().await;
        return Err(format!("node reports network {net}, expected {NETWORK}; refusing"));
    }
    if !server_version.starts_with(PINNED_VERSION) {
        println!("WARNING: node version {server_version} differs from pinned {PINNED_VERSION}; results apply to that version");
    }
    if !is_synced || !has_utxo_index {
        return Err("node is not synced or has no UTXO index".into());
    }
    Ok(client)
}

pub fn check_testnet(addr: &Address) -> Result<(), String> {
    if addr.prefix != Prefix::Testnet {
        return Err(format!("{addr} is not a testnet address; refusing"));
    }
    Ok(())
}

pub async fn utxos(client: &KaspaRpcClient, addr: &Address) -> Result<Vec<RpcUtxosByAddressesEntry>, String> {
    check_testnet(addr)?;
    client.get_utxos_by_addresses(vec![addr.clone()]).await.map_err(|e| e.to_string())
}
