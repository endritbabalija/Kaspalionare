//! The page's local store, format `webstore/1` (spec frontend 6.3; front end F3 part B).
//!
//! The page keeps, in this browser's IndexedDB, what a player needs to claim without the
//! indexer API: for each ticket, its outpoint and full state bytes (R1) with the entry a
//! public node reported for it; for each round the player holds tickets in, the compact
//! purchase list of all its shards (checked against the chain as `checkIndexerRound` checks
//! it). F3 writes both right after a purchase is accepted; F4 completes each round after
//! `close()` (the stage moves to `closed` once the lists rebuild the marker's frozen roots),
//! adds the round result once drawn, and writes and reads the backup file.
//!
//! The records are declared here (tsify), so the page has no hand-written copy of them, and the
//! backup reader F4 adds can refuse a malformed file in Rust (`deny_unknown_fields`). Layout:
//! - database `kaspalionare` (`DATABASE`), version 1 (`DB_VERSION`), object stores:
//!   - `meta`, key `"format"`: one `StoreMeta`;
//!   - `tickets`, key `"<transactionId>:<index>"`: `StoredTicket`;
//!   - `rounds`, key `"<covenantId>:<round>"`: `StoredRound`;
//! - the quick testnet wallet's key lives in a separate database `kaspalionare-wallet`
//!   (`WALLET_DATABASE`, store `keys`, key `"quick"`: `StoredQuickKey`), so a backup of the
//!   store never carries a key (the key has its own export, spec S7). A `mainnet` build has no
//!   key record type (the quick wallet is compiled out, W4).
//!
//! Versioning, as `wallet/1`: a reader refuses any `format` it does not know; a later format
//! is `webstore/2` with its own reader, and a database upgrade (`DB_VERSION` + 1) migrates the
//! records in place. Fields are only ever added with a default, never renamed or retyped,
//! within one format.

use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::dto::{Address, Combination, CovUtxo, Dec, Hex, ReferralFigure};
#[cfg(not(feature = "mainnet"))]
use crate::dto::QuickKey;

/// The format name every record set carries.
pub const FORMAT: &str = "webstore/1";
/// The IndexedDB database of the store.
pub const DATABASE: &str = "kaspalionare";
/// Its schema version (IndexedDB `open(name, version)`).
pub const DB_VERSION: u32 = 1;
/// Its object stores.
pub const STORES: [&str; 3] = ["meta", "tickets", "rounds"];
/// The quick wallet's key database (never part of the store or its backup).
pub const WALLET_DATABASE: &str = "kaspalionare-wallet";

/// What the page needs to open the store (`webstoreInfo`).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebstoreInfo {
    pub format: String,
    pub database: String,
    pub version: u32,
    pub stores: Vec<String>,
    pub wallet_database: String,
}

pub fn info() -> WebstoreInfo {
    WebstoreInfo {
        format: FORMAT.into(),
        database: DATABASE.into(),
        version: DB_VERSION,
        stores: STORES.iter().map(|s| s.to_string()).collect(),
        wallet_database: WALLET_DATABASE.into(),
    }
}

/// The store's `meta` record (key `"format"`): which format its records are in. Every record
/// names its own network and instance (`covenantId`), so one browser can hold the copies of
/// several instances (a testnet-10 beta and a fresh simnet dev run each have their own) and a
/// page reads only its own instance's records.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoreMeta {
    /// `webstore/1`.
    pub format: String,
    /// Unix milliseconds.
    pub created_ms: Dec,
}

/// What the page knows about a purchase from its own build (the figures its cost panel showed,
/// `buyFigures`), kept with the ticket for My tickets (F4).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredBuy {
    pub transaction_id: Hex,
    pub price: Dec,
    pub deposit: Dec,
    pub deposit_back: Dec,
    pub fee: Dec,
    pub referral: ReferralFigure,
}

/// A ticket (the `tickets` store). `ticket.state` is the ticket's full state bytes (R1), the
/// ones `buyFigures` computed and checked against the built output; `ticket.utxo` is the entry
/// a public node reported for the outpoint once the purchase was accepted (checked with
/// `verifyState`). A later claim re-reads the entry from a node by outpoint before use.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredTicket {
    /// `"<transactionId>:<index>"`.
    pub key: String,
    pub covenant_id: Hex,
    pub network: String,
    pub round: Dec,
    pub shard_index: u32,
    pub ticket: CovUtxo,
    pub bundle: Vec<Combination>,
    pub payout: Address,
    pub buy: StoredBuy,
    /// Unix milliseconds.
    pub saved_ms: Dec,
}

/// How far a round's copy has got (spec 6.3: taken while the round is open, completed right
/// after `close()`, checked against the frozen roots once the marker exists).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RoundCopyStage {
    /// Taken while sales are open: every list rebuilt its live shard's root when copied (F3).
    Open,
    /// Completed after `close()`: the lists rebuild the marker's 16 frozen roots (F4).
    Closed,
    /// Closed and drawn: the round result is kept too (F4).
    Drawn,
}

/// A round's copy (the `rounds` store): the compact purchase lists of all k shards, in shard
/// order (six bytes per combination, hex; the form `buildClaim`, `buildDraw` and `checkRoots`
/// take).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredRound {
    /// `"<covenantId>:<round>"`.
    pub key: String,
    pub covenant_id: Hex,
    pub network: String,
    pub round: Dec,
    pub m: u32,
    pub k: u32,
    pub stage: RoundCopyStage,
    pub purchases: Vec<Hex>,
    /// The roots the lists rebuilt (the chain's frozen roots once `closed`), when checked
    /// against a marker or pool.
    #[serde(default)]
    pub roots: Option<Vec<Hex>>,
    /// Unix milliseconds of the last check against the chain.
    pub checked_ms: Dec,
}

/// The quick testnet wallet's key record (database `kaspalionare-wallet`, store `keys`, key
/// `"quick"`; spec 6.2, S7): kept in this browser only. Absent from a `mainnet` build.
#[cfg(not(feature = "mainnet"))]
#[derive(Tsify, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredQuickKey {
    pub network: String,
    pub key: QuickKey,
    /// Unix milliseconds.
    pub created_ms: Dec,
}

#[cfg(not(feature = "mainnet"))]
impl std::fmt::Debug for StoredQuickKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredQuickKey").field("network", &self.network).field("key", &self.key).field("created_ms", &self.created_ms).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_names_the_format() {
        let i = info();
        assert_eq!(i.format, "webstore/1");
        assert_eq!(i.stores, ["meta", "tickets", "rounds"]);
        assert_ne!(i.database, i.wallet_database, "the key never sits in the store's database");
    }
}
