//! The page's network layer, plain-Rust half (front end F3 part A; spec 6.1, 6.3, 6.6).
//!
//! The page talks to a public node over wRPC with the rusty-kaspa v2.1.0 client compiled into
//! this wasm (`node_client`, feature `node`): one rusty-kaspa version, and `GetSeqCommitLaneProof`
//! is reachable (it is not in kaspa-wasm's JS binding, F1 part C). This module holds what that
//! client needs that is not networking, so native tests reach it:
//! - which URLs the page may connect to (`check_url`), and that the node is on the started
//!   network with a UTXO index (`check_server`, spec 6.6);
//! - the node's fee estimate as the whole sompi-per-gram rate the builders take
//!   (`round_rate`: rounded up, the relay floor, refused above the cap, as tn10's D120/D115);
//! - the addresses to ask the node for, given the indexer's objects, and the node's entries
//!   kept for exactly those outpoints (`object_addresses`, `entries_for`): the F1 re-review
//!   rule that every UTXO entry passed to the wasm comes from a node by outpoint, never from
//!   the indexer;
//! - the error table (`error_table`) and the jackpot odds (`jackpot_odds`), so the page has
//!   no copy of either.

use std::collections::BTreeSet;

use lottery::web::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::convert::{self, Net};
use crate::dto::{Address, Dec, Utxo};
use crate::indexer::ApiObject;

/// The most objects one `nodeEntries` call takes (the indexer's own list cap).
pub const OBJECTS_MAX: usize = crate::indexer::OBJECTS_MAX as usize;
/// The most UTXOs a node answer may carry into the page (more is refused, not truncated).
pub const UTXOS_MAX: usize = 4 * OBJECTS_MAX;
/// The longest node URL accepted.
pub const URL_MAX: usize = 512;

/// What the page shows about its node, after `nodeConnect` (spec 3: node status).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo {
    /// The node's URL (the one the resolver picked when none was given).
    pub url: String,
    /// "testnet-10" (or "simnet" in a `simnet` build started on the simnet).
    pub network: String,
    pub server_version: String,
    pub is_synced: bool,
    /// The node's virtual DAA score (the page's clock: 10 per second).
    pub virtual_daa_score: Dec,
}

/// One row of the error table: the stable name, number and its one message.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEntry {
    #[tsify(type = "ErrorName")]
    pub code: String,
    pub number: u16,
    pub message: String,
}

/// Every error code with its message (`lottery::web::Code`, the one table): the page keeps it
/// so that codes it raises itself (`indexer_unavailable`, `worker_stopped`) read the same way.
pub fn error_table() -> Vec<ErrorEntry> {
    Code::ALL.iter().map(|c| ErrorEntry { code: c.as_str().into(), number: c.number(), message: c.message().into() }).collect()
}

/// The jackpot odds at bonus range m: 1 in C = N x m (design 4.1; `lottery::game::N`).
pub fn jackpot_odds(m: u32) -> Result<Dec> {
    if m == 0 || i64::from(m) > lottery::game::M_HARD {
        return Err(Error::new(Code::InputRange, format!("m = {m} (1..={})", lottery::game::M_HARD)));
    }
    Ok((lottery::game::N * i64::from(m)).to_string())
}

/// A node URL the page may connect to on `net`: `ws://` or `wss://`, no spaces or controls,
/// at most `URL_MAX` bytes. On the simnet, loopback only (D125: the private node).
pub fn check_url(net: Net, url: &str) -> Result<()> {
    let bad = |d: &str| Error::new(Code::InputFormat, format!("node URL: {d}"));
    if url.len() > URL_MAX {
        return Err(bad("too long"));
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad("contains spaces or control characters"));
    }
    let rest = url.strip_prefix("wss://").or_else(|| url.strip_prefix("ws://")).ok_or_else(|| bad("not a ws:// or wss:// URL"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_only = if host.starts_with('[') { host.split(']').next().map(|h| format!("{h}]")).unwrap_or_default() } else { host.rsplit_once(':').map(|(h, _)| h.to_string()).unwrap_or_else(|| host.to_string()) };
    if host_only.is_empty() {
        return Err(bad("no host"));
    }
    #[cfg(all(feature = "simnet", not(feature = "mainnet")))]
    if net == Net::Simnet && !matches!(host_only.as_str(), "127.0.0.1" | "localhost" | "[::1]") {
        return Err(Error::new(Code::NodeNetwork, "the simnet node must be on this computer (loopback)"));
    }
    let _ = net;
    Ok(())
}

/// The node's report against the started network (spec 6.6): the same network, with a UTXO
/// index (the page reads every entry by address). Anything else is `node_network`.
pub fn check_server(net: Net, network_id: &str, has_utxo_index: bool) -> Result<()> {
    if network_id != net.name() {
        return Err(Error::new(Code::NodeNetwork, format!("the node reports `{}`, the page runs on {}", network_id.chars().take(32).collect::<String>(), net.name())));
    }
    if !has_utxo_index {
        return Err(Error::new(Code::NodeNetwork, "the node has no UTXO index"));
    }
    Ok(())
}

/// The node's normal-bucket fee estimate (sompi/gram, a float) as the builders' whole rate:
/// rounded up, raised to the relay floor (100), and refused above `MAX_RATE_SOMPI_PER_GRAM`
/// (1,000) or when not a finite non-negative number (`fee_rate`; never clamped: a wrong
/// estimate stops the page instead of overpaying, as tn10 `fees::round_rate`, D115).
pub fn round_rate(raw: f64) -> Result<u32> {
    if !raw.is_finite() || raw < 0.0 {
        return Err(Error::new(Code::FeeRate, format!("fee estimate {raw} is not a finite non-negative rate")));
    }
    let up = raw.ceil();
    let max = lottery::web::build::MAX_RATE_SOMPI_PER_GRAM;
    if up > max as f64 {
        return Err(Error::new(Code::FeeRate, format!("fee estimate {raw} sompi/gram is above {max}")));
    }
    Ok((up as u64).max(harness::MIN_FEERATE_SOMPI_PER_GRAM) as u32)
}

/// The distinct addresses (on `net`) of the objects' scripts, in first-seen order: what the
/// page asks the node for (`getUtxosByAddresses`) to read the objects' entries by outpoint.
pub fn object_addresses(net: Net, objects: &[ApiObject]) -> Result<Vec<Address>> {
    if objects.len() > OBJECTS_MAX {
        return Err(Error::new(Code::InputRange, format!("{} objects (at most {OBJECTS_MAX})", objects.len())));
    }
    let mut out: Vec<Address> = vec![];
    for o in objects {
        let spk = convert::spk("object.scriptPublicKey", &o.script_public_key)?;
        let a = net.address(&spk).ok_or_else(|| Error::new(Code::WalletScript, format!("{:?} at {}:{}: not a standard script", o.role, o.transaction_id, o.index)))?;
        if !out.contains(&a) {
            out.push(a);
        }
    }
    Ok(out)
}

/// The node's entries for exactly the objects' outpoints (others dropped). An object with no
/// entry is missing from the result: `checkIndexerObjects`/`checkIndexerRound` then refuse it
/// as not live (`chain_mismatch`).
pub fn entries_for(objects: &[ApiObject], node: Vec<Utxo>) -> Result<Vec<Utxo>> {
    if node.len() > UTXOS_MAX {
        return Err(Error::new(Code::InputRange, format!("the node returned {} UTXOs (at most {UTXOS_MAX})", node.len())));
    }
    let want: BTreeSet<(String, u32)> = objects.iter().map(|o| (o.transaction_id.to_ascii_lowercase(), o.index)).collect();
    Ok(node.into_iter().filter(|u| want.contains(&(u.transaction_id.to_ascii_lowercase(), u.index))).collect())
}

/// The address a wallet coin query is for, checked for the started network (spec 6.6).
pub fn coin_address(net: Net, address: &str) -> Result<Address> {
    net.script_of("address", address)?;
    Ok(address.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_round_up_with_floor_and_cap() {
        assert_eq!(round_rate(0.0).unwrap(), 100);
        assert_eq!(round_rate(1.0).unwrap(), 100);
        assert_eq!(round_rate(100.2).unwrap(), 101);
        assert_eq!(round_rate(1000.0).unwrap(), 1000);
        for bad in [1000.01, f64::NAN, f64::INFINITY, -1.0] {
            assert_eq!(round_rate(bad).unwrap_err().code, Code::FeeRate, "{bad}");
        }
    }

    #[test]
    fn urls() {
        let n = Net::Testnet10;
        assert!(check_url(n, "wss://node.example/kaspa/testnet-10/wrpc/borsh").is_ok());
        assert!(check_url(n, "ws://127.0.0.1:18610").is_ok());
        for bad in ["http://x", "ws://", "wss:// x", "ftp://a", ""] {
            assert_eq!(check_url(n, bad).unwrap_err().code, Code::InputFormat, "{bad}");
        }
        assert!(check_url(n, &format!("wss://{}", "a".repeat(600))).is_err());
    }

    #[cfg(all(feature = "simnet", not(feature = "mainnet")))]
    #[test]
    fn simnet_node_is_loopback_only() {
        for ok in ["ws://127.0.0.1:18610", "ws://localhost:18610/", "ws://[::1]:18610"] {
            assert!(check_url(Net::Simnet, ok).is_ok(), "{ok}");
        }
        for bad in ["ws://10.0.0.2:18610", "wss://node.example/simnet", "ws://127.0.0.1.example:1"] {
            assert_eq!(check_url(Net::Simnet, bad).unwrap_err().code, Code::NodeNetwork, "{bad}");
        }
        assert!(check_server(Net::Simnet, "simnet", true).is_ok());
        assert_eq!(check_server(Net::Simnet, "testnet-10", true).unwrap_err().code, Code::NodeNetwork);
    }

    #[test]
    fn server_network() {
        assert!(check_server(Net::Testnet10, "testnet-10", true).is_ok());
        for (id, idx) in [("mainnet", true), ("testnet-11", true), ("simnet", true), ("testnet-10", false)] {
            assert_eq!(check_server(Net::Testnet10, id, idx).unwrap_err().code, Code::NodeNetwork, "{id} {idx}");
        }
    }

    #[test]
    fn odds_and_table() {
        assert_eq!(jackpot_odds(10).unwrap(), "1425060");
        assert_eq!(jackpot_odds(2).unwrap(), "285012");
        assert_eq!(jackpot_odds(0).unwrap_err().code, Code::InputRange);
        assert_eq!(jackpot_odds(256).unwrap_err().code, Code::InputRange);
        let t = error_table();
        assert_eq!(t.len(), Code::ALL.len());
        assert!(t.iter().enumerate().all(|(i, e)| e.number as usize == i + 1));
    }
}
