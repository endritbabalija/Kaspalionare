//! The page's local store, format `webstore/2` (spec frontend 6.3; front end F3 part B, F4).
//!
//! The page keeps, in this browser's IndexedDB, what a player needs to claim without the
//! indexer API: for each ticket, its outpoint and full state bytes (R1) with the entry a
//! public node reported for it; for each round the player holds tickets in, the compact
//! purchase list of all its shards (checked against the chain as `checkIndexerRound` checks
//! it). F3 writes both right after a purchase is accepted; F4 completes each round after
//! `close()` (the stage moves to `closed` once the lists rebuild the marker's frozen roots),
//! adds the round result once drawn, and writes and reads the backup file (`backup.rs`).
//!
//! The records are declared here (tsify), so the page has no hand-written copy of them, and the
//! backup reader refuses a malformed file in Rust (`deny_unknown_fields`). Layout:
//! - database `kaspalionare` (`DATABASE`), version 2 (`DB_VERSION`), object stores:
//!   - `meta`, key `"format"`: one `StoreMeta`;
//!   - `tickets`, key `"<transactionId>:<index>"` of the purchase: `StoredTicket`;
//!   - `rounds`, key `"<covenantId>:<round>"`: `StoredRound`;
//!   - `pending`, key the purchase's transaction ID: `StoredPending`;
//! - the quick testnet wallet's key lives in a separate database `kaspalionare-wallet`
//!   (`WALLET_DATABASE`, store `keys`, key `"quick"`: `StoredQuickKey`), so a backup of the
//!   store never carries a key (the key has its own export, spec S7). A `mainnet` build has no
//!   key record type (the quick wallet is compiled out, W4).
//!
//! Versioning, as `wallet/1`: a reader refuses any `format` it does not know; a later format
//! has its own name and a database upgrade (`DB_VERSION` + 1). Fields are only ever added with
//! a default, never renamed or retyped, within one format.
//!
//! F4: `webstore/2` (database version 2). What changed from F3's `webstore/1`:
//! - a fourth object store `pending` (`StoredPending`): a signed purchase whose outcome the page
//!   did not learn before it closed (F2/F3 review S3); the next visit settles it from the chain;
//! - `StoredTicket`: `shardIndex` and `buy` are optional (a ticket found by payout through the
//!   indexer API, J3, has no purchase record of this browser), `ended` (the claim or return that
//!   consumed it) added;
//! - `StoredRound`: `against` (the chain object whose frozen roots the lists rebuilt: the marker
//!   once closed, a pool once drawn) and `result` (the round result once drawn) added.
//!
//! Every `webstore/1` record reads as a `webstore/2` record unchanged (the new fields are
//! optional; `tests::webstore_1_records_read_as_webstore_2`), so the migration is: create the
//! `pending` store and rewrite the `meta` record's format (`READS` lists the formats a page
//! migrates from). The backup file carries `webstore/2` records.

use serde::{Deserialize, Serialize};
use tsify::Tsify;

#[cfg(not(feature = "mainnet"))]
use crate::dto::QuickKey;
use crate::dto::{Address, Combination, CovUtxo, Dec, Hex, ReferralFigure, Signed, TicketOut};
use crate::views::RoleName;

/// The format name every record set carries.
pub const FORMAT: &str = "webstore/2";
/// The earlier formats a page migrates from (their records read as this format's).
pub const READS: [&str; 1] = ["webstore/1"];
/// The IndexedDB database of the store.
pub const DATABASE: &str = "kaspalionare";
/// Its schema version (IndexedDB `open(name, version)`).
pub const DB_VERSION: u32 = 2;
/// Its object stores.
pub const STORES: [&str; 4] = ["meta", "tickets", "rounds", "pending"];
/// The quick wallet's key database (never part of the store or its backup).
pub const WALLET_DATABASE: &str = "kaspalionare-wallet";

/// What the page needs to open the store (`webstoreInfo`).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebstoreInfo {
    pub format: String,
    /// Earlier formats whose records read as this format's (the page migrates them in place).
    pub reads: Vec<String>,
    pub database: String,
    pub version: u32,
    pub stores: Vec<String>,
    pub wallet_database: String,
}

pub fn info() -> WebstoreInfo {
    WebstoreInfo {
        format: FORMAT.into(),
        reads: READS.iter().map(|s| s.to_string()).collect(),
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
    /// `webstore/2`.
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
/// ones `buyFigures` (or a claim's `claimFigures`) computed and checked against the built
/// output; `ticket.utxo` is the entry a public node reported for the outpoint (checked with
/// `verifyState`). A later claim re-reads the entry from a node by outpoint before use.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredTicket {
    /// `"<transactionId>:<index>"` of the purchase (the ticket's identity; it stays the same
    /// when a claim moves the ticket to its successor).
    pub key: String,
    pub covenant_id: Hex,
    pub network: String,
    pub round: Dec,
    /// The shard it was bought on (absent for a ticket found by payout, J3).
    #[serde(default)]
    pub shard_index: Option<u32>,
    /// The live ticket: after a PAY or ACCRUE claim, its successor (the claim's output 1).
    pub ticket: CovUtxo,
    pub bundle: Vec<Combination>,
    pub payout: Address,
    /// This browser's purchase record (absent for a ticket found by payout, J3).
    #[serde(default)]
    pub buy: Option<StoredBuy>,
    /// The claim or return that consumed the ticket, once one did.
    #[serde(default)]
    pub ended: Option<TicketEnd>,
    /// Unix milliseconds.
    pub saved_ms: Dec,
}

/// How a ticket ended.
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TicketEndKind {
    /// Its last claim (FINAL) paid the prize and its value.
    Claimed,
    /// `return_deposit()` paid its value.
    Returned,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TicketEnd {
    pub kind: TicketEndKind,
    pub transaction_id: Hex,
    /// What it paid to the payout (sompi).
    pub amount: Dec,
    /// Unix milliseconds.
    pub ended_ms: Dec,
}

/// How far a round's copy has got (spec 6.3: taken while the round is open, completed right
/// after `close()`, checked against the frozen roots once the marker exists).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RoundCopyStage {
    /// Taken while sales are open: every list rebuilt its live shard's root when copied (F3).
    Open,
    /// Completed after `close()`: the lists rebuild the marker's frozen roots (F4).
    Closed,
    /// Closed and drawn: the round result is kept too (F4).
    Drawn,
}

/// A chain object kept with a round's copy: its role, full state bytes (R1) and script, so the
/// page can find it on a node by its script (a pool keeps its state through every claim) and
/// check the lists against it again without the indexer API.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredObject {
    pub role: RoleName,
    pub state: Hex,
    pub script_public_key: Hex,
}

/// The round result (`draw()`'s pools), kept once the round is drawn (spec 6.3): the winning
/// numbers, the claim deadline and every live pool's state, checked against the pools' UTXOs
/// when it was taken.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredResult {
    pub winning: Combination,
    /// D_claim (DAA score).
    pub claim_deadline: Dec,
    pub pools: Vec<StoredObject>,
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
    /// The chain object the lists were last checked against (`closed`: the marker; `drawn`: a
    /// pool), F4.
    #[serde(default)]
    pub against: Option<StoredObject>,
    /// The round result (`drawn`), F4.
    #[serde(default)]
    pub result: Option<StoredResult>,
    /// Unix milliseconds of the last check against the chain.
    pub checked_ms: Dec,
}

/// A signed purchase whose outcome the page has not learnt (the `pending` store, key the
/// transaction ID): the figures its cost panel showed and the ticket it creates, so a later
/// visit settles it from the chain (the ticket output live: it becomes a `StoredTicket`; the
/// same signed transaction is sent again otherwise; an input spent without the ticket: dropped).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredPending {
    /// The transaction ID.
    pub key: Hex,
    pub covenant_id: Hex,
    pub network: String,
    pub round: Dec,
    pub shard_index: u32,
    pub ticket: TicketOut,
    pub bundle: Vec<Combination>,
    pub payout: Address,
    pub buy: StoredBuy,
    pub signed: Signed,
    /// Unix milliseconds.
    pub saved_ms: Dec,
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
        assert_eq!(i.format, "webstore/2");
        assert_eq!(i.reads, ["webstore/1"]);
        assert_eq!(i.version, 2);
        assert_eq!(i.stores, ["meta", "tickets", "rounds", "pending"]);
        assert_ne!(i.database, i.wallet_database, "the key never sits in the store's database");
    }

    /// The migration (F4): records F3 wrote as `webstore/1` read as `webstore/2` records, with
    /// the new fields absent; an unknown field is still refused.
    #[test]
    fn webstore_1_records_read_as_webstore_2() {
        let t = concat!(
            r#"{"key":"11:1","covenantId":"aa","network":"simnet","round":"3","shardIndex":4,"#,
            r#""ticket":{"utxo":{"transactionId":"11","index":1,"amount":"13000000","scriptPublicKey":"0000aa","blockDaaScore":"9","isCoinbase":false,"covenantId":"aa"},"state":"00"},"#,
            r#""bundle":[{"main":[1,2,3,4,5],"bonus":1}],"payout":"kaspasim:q","#,
            r#""buy":{"transactionId":"11","price":"100000000","deposit":"13000000","depositBack":"1","fee":"1","referral":{"amount":"5000000","paid":false,"address":null}},"#,
            r#""savedMs":"1"}"#
        );
        let t: StoredTicket = serde_json::from_str(t).unwrap();
        assert_eq!(t.shard_index, Some(4));
        assert_eq!(t.buy.as_ref().map(|b| b.price.as_str()), Some("100000000"));
        assert!(t.ended.is_none());
        let r = r#"{"key":"aa:3","covenantId":"aa","network":"simnet","round":"3","m":2,"k":16,"stage":"open","purchases":[],"roots":["00"],"checkedMs":"1"}"#;
        let r: StoredRound = serde_json::from_str(r).unwrap();
        assert_eq!(r.stage, RoundCopyStage::Open);
        assert!(r.against.is_none() && r.result.is_none());
        let x = r#"{"key":"aa:3","covenantId":"aa","network":"simnet","round":"3","m":2,"k":16,"stage":"open","purchases":[],"checkedMs":"1","x":1}"#;
        assert!(serde_json::from_str::<StoredRound>(x).is_err());
    }
}
