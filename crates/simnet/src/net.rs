//! Network guard of the private simnet runner (spec A5b, D125), the counterpart of
//! `tn10/src/net.rs`: everything this crate starts, connects to, mines to or pays must be
//! simnet. It accepts only the simnet network, `kaspasim:` addresses, and a node URL on the
//! loopback interface, and it refuses any node configuration with peers, DNS seeding, UPnP
//! or a non-loopback listener.

use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_utils::networking::ContextualNetAddress;
use kaspa_wrpc_server::address::WrpcNetAddress;
use kaspad_lib::args::Args;

pub const NETWORK: &str = "simnet";

/// The only network this crate runs on.
pub fn network() -> NetworkId {
    NetworkId::new(NetworkType::Simnet)
}

/// A node's reported network id must be simnet.
pub fn check_network(reported: &str) -> Result<(), String> {
    if reported != NETWORK {
        return Err(format!("node reports network {reported}, expected {NETWORK}; refusing"));
    }
    Ok(())
}

/// Every address this crate mines to or pays must be a `kaspasim:` address.
pub fn check_simnet(addr: &Address) -> Result<(), String> {
    if addr.prefix != Prefix::Simnet {
        return Err(format!("{addr} is not a simnet (kaspasim:) address; refusing"));
    }
    Ok(())
}

/// Parse an address string and require the simnet prefix.
#[allow(dead_code)] // used by the part-2 runner and the unit test
pub fn parse_simnet(s: &str) -> Result<Address, String> {
    let a = Address::try_from(s).map_err(|e| format!("{s}: {e}"))?;
    check_simnet(&a)?;
    Ok(a)
}

/// The wRPC URL must be `ws://127.0.0.1:<port>` (the in-process node), never a remote node.
pub fn check_url(url: &str) -> Result<(), String> {
    let rest = url.strip_prefix("ws://127.0.0.1:").ok_or_else(|| format!("{url}: only ws://127.0.0.1:<port> is allowed; refusing"))?;
    let port: u16 = rest.parse().map_err(|_| format!("{url}: bad port; refusing"))?;
    // Same ports the project hook blocks: rusty-kaspa mainnet defaults.
    if [16110, 17110, 18110].contains(&port) {
        return Err(format!("{url} uses a mainnet default port; refusing"));
    }
    Ok(())
}

fn loopback(a: &ContextualNetAddress) -> bool {
    a.normalize(0).ip.is_loopback()
}

/// The node configuration must be simnet only, with no peers (no connect/add peers,
/// outbound target 0, inbound limit 0, no DNS seeding, no UPnP) and every listener on the
/// loopback interface (`kaspad/src/daemon.rs:564-572`).
pub fn check_args(a: &Args) -> Result<(), String> {
    if !a.simnet || a.testnet || a.devnet {
        return Err("node args: only --simnet is allowed (testnet/devnet/mainnet refused)".into());
    }
    if a.network() != network() {
        return Err(format!("node args select network {}, expected {NETWORK}", a.network()));
    }
    if !a.connect_peers.is_empty() || !a.add_peers.is_empty() {
        return Err("node args: peers are not allowed on the private simnet".into());
    }
    if a.outbound_target != 0 || a.inbound_limit != 0 {
        return Err(format!("node args: outbound_target {} and inbound_limit {} must be 0", a.outbound_target, a.inbound_limit));
    }
    if !a.disable_dns_seeding || !a.disable_upnp {
        return Err("node args: DNS seeding and UPnP must be disabled".into());
    }
    match &a.listen {
        Some(l) if loopback(l) => {}
        _ => return Err("node args: the P2P listener must be on 127.0.0.1".into()),
    }
    match &a.rpclisten_borsh {
        Some(WrpcNetAddress::Custom(c)) if loopback(c) => {}
        _ => return Err("node args: the Borsh wRPC listener must be 127.0.0.1:<port>".into()),
    }
    if a.rpclisten_json.is_some() {
        return Err("node args: no JSON wRPC listener".into());
    }
    if !a.disable_grpc {
        return Err("node args: gRPC must be disabled (Borsh wRPC on 127.0.0.1 only)".into());
    }
    if !a.utxoindex || !a.enable_unsynced_mining {
        return Err("node args: --utxoindex and --enable-unsynced-mining are required".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_addresses::Version;

    #[test]
    fn addresses_only_simnet() {
        let key = [3u8; 32];
        let sim = Address::new(Prefix::Simnet, Version::PubKey, &key);
        assert!(sim.to_string().starts_with("kaspasim:"));
        assert_eq!(check_simnet(&sim), Ok(()));
        assert_eq!(parse_simnet(&sim.to_string()).map(|a| a == sim), Ok(true));
        for p in [Prefix::Mainnet, Prefix::Testnet, Prefix::Devnet] {
            let a = Address::new(p, Version::PubKey, &key);
            assert!(check_simnet(&a).is_err(), "{a} accepted");
            assert!(parse_simnet(&a.to_string()).is_err(), "{a} parsed");
        }
    }

    #[test]
    fn network_and_url() {
        assert_eq!(check_network("simnet"), Ok(()));
        for n in ["mainnet", "testnet-10", "testnet-11", "devnet", ""] {
            assert!(check_network(n).is_err(), "{n}");
        }
        assert_eq!(check_url("ws://127.0.0.1:41234"), Ok(()));
        for u in ["ws://127.0.0.1:17110", "ws://127.0.0.1:16110", "ws://10.0.0.2:17510", "wss://node.example:443", "ws://localhost:17510", "ws://127.0.0.1:x"] {
            assert!(check_url(u).is_err(), "{u}");
        }
    }

    #[test]
    fn node_args_guard() {
        let good = crate::node::node_args(std::path::Path::new("x"), 40001, 40002);
        assert_eq!(check_args(&good), Ok(()));
        let cases: Vec<(&str, fn(&mut Args))> = vec![
            ("mainnet", |a| a.simnet = false),
            ("testnet", |a| {
                a.simnet = false;
                a.testnet = true
            }),
            ("devnet", |a| {
                a.simnet = false;
                a.devnet = true
            }),
            ("connect peer", |a| a.connect_peers = vec!["127.0.0.1:16111".try_into().unwrap()]),
            ("add peer", |a| a.add_peers = vec!["10.1.1.1:16111".try_into().unwrap()]),
            ("outbound", |a| a.outbound_target = 8),
            ("inbound", |a| a.inbound_limit = 1),
            ("dns", |a| a.disable_dns_seeding = false),
            ("upnp", |a| a.disable_upnp = false),
            ("p2p public", |a| a.listen = Some("0.0.0.0:40001".try_into().unwrap())),
            ("p2p unset", |a| a.listen = None),
            ("borsh public", |a| a.rpclisten_borsh = Some("0.0.0.0:40002".parse().unwrap())),
            ("borsh default", |a| a.rpclisten_borsh = Some(WrpcNetAddress::Default)),
            ("json", |a| a.rpclisten_json = Some("127.0.0.1:40003".parse().unwrap())),
            ("grpc", |a| a.disable_grpc = false),
            ("no utxoindex", |a| a.utxoindex = false),
            ("no unsynced mining", |a| a.enable_unsynced_mining = false),
        ];
        for (name, f) in cases {
            let mut a = good.clone();
            f(&mut a);
            assert!(check_args(&a).is_err(), "{name}: accepted");
        }
    }
}
