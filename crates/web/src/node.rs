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
/// The most wallet coins `nodeCoins` lists (the largest first). A build takes at most
/// `lottery::web::build::F_MAX` of them; anything past this many is dust for the page.
pub const COINS_LISTED_MAX: usize = 256;
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

/// The game's fixed shape as the page's picker and coin choice use it (spec 1: the page has no
/// TypeScript copy of game-size arithmetic; F2/F3 review note): 5 of 1..30, at most B_MAX
/// combinations a ticket, at most F_MAX wallet coins a build.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameShape {
    /// The highest main number (`lottery::combo::MAIN_MAX`).
    pub main_max: u32,
    /// Main numbers in a combination (`Combo::main`).
    pub main_picks: u32,
    /// Combinations on one ticket (`lottery::game::TICKET_B_MAX`; an instance's B_MAX may be
    /// lower: `checkIndexerInstance`).
    pub b_max: u32,
    /// Wallet coins one build takes (`lottery::web::build::F_MAX`).
    pub coins_max: u32,
}

pub fn game_shape() -> GameShape {
    GameShape {
        main_max: u32::from(lottery::combo::MAIN_MAX),
        main_picks: lottery::combo::Combo::new([0; 5], 0).main.len() as u32,
        b_max: lottery::game::TICKET_B_MAX as u32,
        coins_max: lottery::web::build::F_MAX as u32,
    }
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

/// The outpoints (lower-case txid, index) the page asks the node about for these objects.
pub fn wanted(objects: &[ApiObject]) -> BTreeSet<(String, u32)> {
    objects.iter().map(|o| (o.transaction_id.to_ascii_lowercase(), o.index)).collect()
}

/// The node's entries kept for exactly the `want`ed outpoints, the rest dropped as they stream
/// past (F2/F3 review S5: anyone can send dust to a covenant address, so the node's answer for
/// an address has no size the page can rely on; nothing is counted before this filter, and
/// what is kept is at most one entry per wanted outpoint). An object with no entry is missing
/// from the result: `checkIndexerObjects`/`checkIndexerRound` then refuse it as not live
/// (`chain_mismatch`).
pub fn keep_outpoints<T>(want: &BTreeSet<(String, u32)>, node: impl IntoIterator<Item = T>, outpoint: impl Fn(&T) -> (String, u32)) -> Vec<T> {
    let mut seen = BTreeSet::new();
    node.into_iter()
        .filter(|u| {
            let k = outpoint(u);
            want.contains(&k) && seen.insert(k)
        })
        .collect()
}

/// `keep_outpoints` over the page's `Utxo` form (native tests; the client filters the RPC
/// entries before converting them).
pub fn entries_for(objects: &[ApiObject], node: impl IntoIterator<Item = Utxo>) -> Vec<Utxo> {
    keep_outpoints(&wanted(objects), node, |u| (u.transaction_id.to_ascii_lowercase(), u.index))
}

/// The `n` largest of a wallet's plain coins (covenant outputs dropped), the largest first, kept
/// in a bounded heap as the node's answer streams past (F2/F3 review S5: dust sent to the
/// wallet's address cannot crowd out its real coins or stop the page). Ties keep the earlier.
pub fn largest_coins<T>(node: impl IntoIterator<Item = T>, n: usize, plain: impl Fn(&T) -> bool, amount: impl Fn(&T) -> u64) -> Vec<T> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    // A min-heap on (amount, later first): its top is the coin to drop next.
    let mut heap: BinaryHeap<Reverse<(u64, Reverse<usize>, usize)>> = BinaryHeap::new();
    let mut kept: Vec<Option<T>> = vec![];
    for (seq, c) in node.into_iter().filter(|c| plain(c)).enumerate() {
        let a = amount(&c);
        if heap.len() < n {
            heap.push(Reverse((a, Reverse(seq), kept.len())));
            kept.push(Some(c));
        } else if let Some(Reverse((min, _, slot))) = heap.peek().copied() {
            if a > min {
                heap.pop();
                kept[slot] = Some(c);
                heap.push(Reverse((a, Reverse(seq), slot)));
            }
        }
    }
    let mut order: Vec<(u64, usize, usize)> = heap.into_iter().map(|Reverse((a, Reverse(seq), slot))| (a, seq, slot)).collect();
    order.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)));
    order.into_iter().filter_map(|(_, _, slot)| kept[slot].take()).collect()
}

/// The address a wallet coin query is for, checked for the started network (spec 6.6).
pub fn coin_address(net: Net, address: &str) -> Result<Address> {
    net.script_of("address", address)?;
    Ok(address.to_string())
}

/// The node's refusal texts that mean an input is already spent (rusty-kaspa v2.1.0
/// `mining/errors/src/mempool.rs`: `RejectDoubleSpendInMempool` "output {0} already spent by
/// transaction {1} in the mempool", line 33; `RejectDisallowedOrphan` "transaction {0} is an
/// orphan where orphan is disallowed", line 77, which the page meets because it submits with
/// `allow_orphan = false` and an input no longer in the UTXO set makes the transaction an
/// orphan; `RejectMissingOutpoint` "at least one outpoint of the transaction is lacking a
/// matching UTXO entry", line 18). Front end F3 part B: shard contention (spec S1).
pub const CONFLICT_TEXTS: [&str; 3] = ["already spent by transaction", "is an orphan where orphan is disallowed", "is lacking a matching utxo entry"];

/// A submit refusal as the page reads it: `node_conflict` when the node says an input is
/// already spent (another buyer took the shard first, or the wallet's coin was spent: the page
/// rebuilds on fresh entries), else the error as it was.
pub fn submit_refusal(e: Error) -> Error {
    if e.code != Code::NodeRefused {
        return e;
    }
    let lower = e.detail.to_ascii_lowercase();
    if CONFLICT_TEXTS.iter().any(|t| lower.contains(t)) { Error::new(Code::NodeConflict, e.detail) } else { e }
}

/// The total of a wallet's coins (sompi), for the wallet sheet's balance (spec S7).
pub fn coins_total(coins: &[Utxo]) -> Result<Dec> {
    let mut t = 0u64;
    for c in coins {
        let v = convert::dec("amount", &c.amount)?;
        t = t.checked_add(v).ok_or_else(|| Error::new(Code::Overflow, "the coins' total"))?;
    }
    Ok(t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_conflicts_are_classified() {
        let refused = |t: &str| Error::new(Code::NodeRefused, format!("submitTransaction: Rejected transaction {}: {t}", "ab".repeat(32)));
        for t in [
            "output 1f2e…:0 already spent by transaction 3c4d… in the mempool",
            "transaction 3c4d… is an orphan where orphan is disallowed",
            "at least one outpoint of the transaction is lacking a matching UTXO entry",
        ] {
            assert_eq!(submit_refusal(refused(t)).code, Code::NodeConflict, "{t}");
        }
        for t in ["transaction has 10 fees which is under the required amount of 20 for compute mass 30", "transaction 3c4d… is invalid"] {
            assert_eq!(submit_refusal(refused(t)).code, Code::NodeRefused, "{t}");
        }
        // A transport failure stays what it is, whatever its text.
        assert_eq!(submit_refusal(Error::new(Code::NodeUnavailable, "already spent by transaction")).code, Code::NodeUnavailable);
    }

    fn utxo(txid: u8, index: u32, amount: u64, cov: bool) -> Utxo {
        Utxo {
            transaction_id: format!("{txid:02x}").repeat(32),
            index,
            amount: amount.to_string(),
            script_public_key: String::new(),
            block_daa_score: "1".into(),
            is_coinbase: false,
            covenant_id: cov.then(|| "cc".repeat(32)),
        }
    }

    /// F2/F3 review S5: more than 8,000 dust entries at an object's address (anyone can send
    /// them) neither stop the page nor hide the object's own entry: the node's answer is
    /// filtered by outpoint before anything is counted.
    #[test]
    fn dust_at_an_address_does_not_stop_the_page() {
        let o = |txid: u8, index: u32| ApiObject {
            role: crate::views::RoleName::Shard,
            transaction_id: format!("{txid:02X}").repeat(32),
            index,
            amount: "1".into(),
            block_daa_score: "1".into(),
            script_public_key: String::new(),
            state: String::new(),
        };
        let objects = [o(0xab, 0), o(0xcd, 3)];
        let mut node: Vec<Utxo> = (0..9_000u32).map(|i| utxo(0x11, i, 1, false)).collect();
        node.insert(4_321, utxo(0xab, 0, 5, true));
        node.push(utxo(0xcd, 3, 7, true));
        node.push(utxo(0xcd, 3, 7, true)); // the same outpoint twice: kept once
        node.push(utxo(0xcd, 4, 7, true)); // another output of that transaction
        let kept = entries_for(&objects, node);
        assert_eq!(kept.len(), 2, "exactly the objects' outpoints");
        assert_eq!((kept[0].index, kept[0].amount.as_str()), (0, "5"));
        assert_eq!((kept[1].index, kept[1].amount.as_str()), (3, "7"));
    }

    /// F2/F3 review S5 for a wallet address: 9,000 dust coins and a covenant output, and the
    /// wallet's real coins are still the ones listed (the largest first, at most n).
    #[test]
    fn dust_coins_do_not_crowd_out_the_wallet_coins() {
        let mut node: Vec<Utxo> = (0..9_000u32).map(|i| utxo(0x22, i, 1 + u64::from(i % 3), false)).collect();
        node.insert(17, utxo(0x33, 0, 50_000_000_000, false));
        node.insert(8_000, utxo(0x33, 1, 7_000_000_000, false));
        node.push(utxo(0x33, 2, 900_000_000_000, true)); // a covenant output is not a coin
        node.push(utxo(0x33, 3, 7_000_000_000, false)); // a tie: the earlier first
        let amount = |u: &Utxo| u.amount.parse::<u64>().unwrap();
        let plain = |u: &Utxo| u.covenant_id.is_none();
        let top = largest_coins(node.clone(), 4, plain, amount);
        let got: Vec<(u32, u64)> = top.iter().map(|u| (u.index, amount(u))).collect();
        assert_eq!(got[..3], [(0, 50_000_000_000), (1, 7_000_000_000), (3, 7_000_000_000)]);
        assert_eq!(got[3].1, 3, "then the largest dust");
        assert_eq!(largest_coins(node.clone(), COINS_LISTED_MAX, plain, amount).len(), COINS_LISTED_MAX);
        assert!(largest_coins(node, 0, plain, amount).is_empty());
        assert!(largest_coins(Vec::<Utxo>::new(), 4, plain, amount).is_empty());
    }

    #[test]
    fn coin_totals() {
        let u = |a: &str| Utxo { transaction_id: "00".repeat(32), index: 0, amount: a.into(), script_public_key: String::new(), block_daa_score: "1".into(), is_coinbase: false, covenant_id: None };
        assert_eq!(coins_total(&[]).unwrap(), "0");
        assert_eq!(coins_total(&[u("10000000000"), u("5")]).unwrap(), "10000000005");
        assert_eq!(coins_total(&[u(&u64::MAX.to_string()), u("1")]).unwrap_err().code, Code::Overflow);
        assert_eq!(coins_total(&[u("x")]).unwrap_err().code, Code::InputFormat);
    }

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
    fn game_shape_is_the_library_s() {
        assert_eq!(game_shape(), GameShape { main_max: 30, main_picks: 5, b_max: 4, coins_max: 8 });
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
