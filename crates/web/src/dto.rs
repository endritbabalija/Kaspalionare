//! The types that cross the wasm boundary (F1 part C). Each derives `Tsify`, so the `.d.ts`
//! wasm-bindgen emits declares it: the page's only type source (spec 1.1, no hand-written
//! mirror). Conventions, the same for every type:
//! - every 64-bit integer (sompi amounts, DAA scores, masses, shares, i64 state fields) is a
//!   decimal string, so values above 2^53 survive JSON, IndexedDB and `postMessage`;
//! - bytes are lowercase hex; a script public key is the Safe JSON form (2-byte big-endian
//!   version, then the script);
//! - an address is a `kaspatest:` address (testnet-10; `kaspasim:` only in a `simnet` build
//!   started on the simnet); any other prefix is refused (`address_network`);
//! - field names are camelCase; an unknown input field is refused (`input_format`), so a
//!   misspelt option never goes unnoticed.

use serde::{Deserialize, Serialize};
use tsify::Tsify;

// ------------------------------------------------------------------------------ named strings

/// A 64-bit integer as a decimal string (sompi amounts, DAA scores, masses, shares, the i64
/// fields of states).
#[tsify::declare]
pub type Dec = String;

/// Bytes as lowercase hex (a script public key is the Safe JSON form: 2-byte big-endian
/// version, then the script).
#[tsify::declare]
pub type Hex = String;

/// An address of the started network (`kaspatest:` on testnet-10).
#[tsify::declare]
pub type Address = String;

/// The five lower-tier shares of a controller or marker (D18 slot order), as decimals.
#[tsify::declare]
pub type Shares = Vec<Dec>;

// ------------------------------------------------------------------------------ outcome

/// An error crossing the boundary (spec 1.1 AX): the stable code, its number and its one
/// message (what the page shows), and the detail (for logs and developers, never page copy).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    #[tsify(type = "ErrorName")]
    pub code: String,
    pub number: u16,
    pub message: String,
    pub detail: String,
}

impl From<lottery::web::Error> for ApiError {
    fn from(e: lottery::web::Error) -> Self {
        Self { code: e.code.as_str().into(), number: e.code.number(), message: e.code.message().into(), detail: e.detail }
    }
}

/// Every function returns `{ ok: T }` or `{ error: ApiError }`; none throws, none panics.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Outcome<T> {
    Ok(T),
    Error(ApiError),
}

// ------------------------------------------------------------------------------ start

/// What `start` reports.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// "testnet-10" (or "simnet" in a `simnet` build).
    pub network: String,
    /// The address prefix every address must carry ("kaspatest").
    pub address_prefix: String,
    /// This crate's version.
    pub version: String,
    /// The committed B_MAX (combinations per ticket) and K_MAX (shards per round).
    pub b_max: u32,
    pub k_max: u32,
    /// The embedded templates' hashes (hex), by role name.
    pub templates: Vec<TemplateHash>,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TemplateHash {
    pub name: String,
    pub hash: Hex,
}

// ------------------------------------------------------------------------------ chain objects

/// A UTXO as a public node returns it (outpoint and entry, flattened; the Safe JSON field
/// names). Extra fields (such as `address`) are ignored.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Utxo {
    /// 32 bytes, hex.
    pub transaction_id: Hex,
    pub index: u32,
    /// Sompi.
    pub amount: Dec,
    /// Safe JSON form.
    pub script_public_key: Hex,
    pub block_daa_score: Dec,
    pub is_coinbase: bool,
    /// 32 bytes, hex; null for a plain UTXO.
    #[serde(default)]
    pub covenant_id: Option<Hex>,
}

/// A covenant UTXO and the state bytes given for it (by the indexer API or the local copy;
/// checked against the UTXO's script and the instance's covenant ID before use, spec 6.3).
/// No call can check that the UTXO is live (unspent): the page reads `utxo` from a public
/// node by outpoint, never from the indexer (F1 re-review N1). A builder given a spent one
/// builds a transaction the node rejects, at no cost.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CovUtxo {
    pub utxo: Utxo,
    /// The encoded state bytes, hex.
    pub state: Hex,
}

/// One combination: five main numbers (1..=30, ascending, distinct) and a bonus (1..=m).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Combination {
    pub main: [u8; 5],
    pub bonus: u8,
}

/// What pays for a transaction: the wallet's confirmed coins (in input order), its change
/// address (also the caller's reward address in `close()` and `draw()`, and the reserve
/// funder there), and the fee rate in sompi per gram (the node's estimate; 100..=1000).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Funds {
    pub coins: Vec<Utxo>,
    pub change: Address,
    pub fee_rate: u32,
}

// ------------------------------------------------------------------------------ build inputs

/// `buy()` (spec S1): the live shard and its purchases so far, the bundle, the payout, and the
/// referrer (paid only if the referral reaches M_REF; below it the output is omitted).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuyInput {
    pub covenant_id: Hex,
    pub shard: CovUtxo,
    /// The shard's purchases, six bytes each in purchase order, hex.
    pub purchases: Hex,
    pub bundle: Vec<Combination>,
    pub payout: Address,
    #[serde(default)]
    pub referrer: Option<Address>,
    pub funds: Funds,
}

/// `claim()` (spec S2): the live ticket, the live pools of its round, the round's purchases
/// (one hex list per shard, shard order), and the index (null: the first unclaimed index
/// that wins a tier with a live pool).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaimInput {
    pub covenant_id: Hex,
    pub ticket: CovUtxo,
    pub pools: Vec<CovUtxo>,
    pub purchases: Vec<Hex>,
    #[serde(default)]
    pub index: Option<u32>,
    pub funds: Funds,
}

/// `return_deposit()`: the live ticket alone (after T + D_RET), or with an expired pool of
/// its round.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReturnInput {
    pub covenant_id: Hex,
    pub ticket: CovUtxo,
    #[serde(default)]
    pub pool: Option<CovUtxo>,
    pub funds: Funds,
}

/// `close()` (spec S4): the live controller, params and the round's k shards in shard
/// order; `virtualDaaScore` is the node's (the stale-input guard).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseInput {
    pub covenant_id: Hex,
    pub controller: CovUtxo,
    pub params: CovUtxo,
    pub shards: Vec<CovUtxo>,
    pub virtual_daa_score: Dec,
    pub funds: Funds,
}

/// `draw()` (spec S4): the live drawer, marker, vault and next controller, expired pools to
/// absorb, the round's purchases (one hex list per shard), and the 272-byte anchor proof
/// (null: the void, after D + draw_window).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrawInput {
    pub covenant_id: Hex,
    pub drawer: CovUtxo,
    pub marker: CovUtxo,
    pub vault: CovUtxo,
    pub controller: CovUtxo,
    #[serde(default)]
    pub absorbed: Vec<CovUtxo>,
    pub purchases: Vec<Hex>,
    #[serde(default)]
    pub anchor_proof: Option<Hex>,
    pub funds: Funds,
}

/// `request_deposit()` (spec S5): `amount` sompi (at least MIN_DEPOSIT) for `owner`.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DepositInput {
    pub covenant_id: Hex,
    pub vault: CovUtxo,
    pub amount: Dec,
    pub owner: Address,
    pub funds: Funds,
}

/// The kind of key behind an address (D67).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum KeyKind {
    Schnorr,
    Ecdsa,
    P2sh,
}

/// The owner input of a withdrawal: a confirmed coin at the position owner's address and the
/// owner's public key (x-only for Schnorr and P2SH, 33 bytes for ECDSA).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerInput {
    pub coin: Utxo,
    pub kind: KeyKind,
    pub public_key: Hex,
}

/// `request_withdraw()` (spec S5): `shares` of the live position (the whole position, or
/// w >= wmin with a remainder >= wmin). `funds.coins` may be empty: the owner coin pays.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WithdrawInput {
    pub covenant_id: Hex,
    pub vault: CovUtxo,
    pub position: CovUtxo,
    pub shares: Dec,
    pub owner: OwnerInput,
    pub funds: Funds,
}

/// `settle()` (spec S5): the live request against the live record of its round.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettleInput {
    pub covenant_id: Hex,
    pub request: CovUtxo,
    pub record: CovUtxo,
    pub funds: Funds,
}

// ------------------------------------------------------------------------------ built

/// An input the wallet must sign: KasWare's and Kaspire's `signInputs` item (`index`,
/// `sighashType` = 1, SIG_HASH_ALL) plus who signs it.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignItem {
    pub index: u32,
    pub sighash_type: u8,
    pub kind: KeyKind,
    pub public_key: Hex,
    /// The address of the input's UTXO.
    pub address: Option<Address>,
}

/// The masses of the signed transaction (KIP-9/KIP-13; priority = what the fee pays for).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MassesOut {
    pub compute: Dec,
    pub transient: Dec,
    pub storage: Dec,
    pub priority: Dec,
}

/// One output of a built transaction.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputItem {
    pub index: u32,
    pub value: Dec,
    /// The output's address (P2SH for covenant outputs).
    pub address: Option<Address>,
    /// A covenant output (bound to an input by KIP-20).
    pub covenant: bool,
    /// Pays back to the wallet: the script of an input the wallet signs (its own address;
    /// read from the transaction alone, F1 review).
    pub to_wallet: bool,
}

/// The figures of a built transaction, read from the transaction itself (spec 7: amounts
/// come from the built transaction, not from separate arithmetic). `assemble` (and any
/// signing call of this API) recomputes every figure of a `BuiltTx` from its `transaction`
/// and refuses any difference (`built_mismatch`, F1 review).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Summary {
    /// The wallet's coins spent: the inputs it signs (funding and owner inputs).
    pub wallet_in: Dec,
    /// What comes back to the wallet in this transaction: the outputs with `toWallet`.
    pub wallet_back: Dec,
    pub outputs: Vec<OutputItem>,
}

/// An unsigned transaction ready for the wallet (spec 6.1): the Safe JSON to pass as
/// `txJsonString` (every covenant input final, the inputs to sign with an empty signature
/// script, budgets and the storage-mass commit of the signed transaction), the inputs to
/// sign, the masses, the fee and the figures. Pass it back unchanged to `assemble`.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuiltTx {
    pub transaction: String,
    /// The transaction ID (v1: it does not cover signature scripts, so it is final now).
    pub id: Hex,
    pub sign: Vec<SignItem>,
    pub masses: MassesOut,
    pub fee: Dec,
    /// The script units of each input's run (what the compute budgets commit).
    pub units: Vec<Dec>,
    pub summary: Summary,
}

/// `assemble`: the built transaction and the wallet's returned Safe JSON.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssembleInput {
    pub built: BuiltTx,
    pub signed: String,
}

/// The final transaction: our transaction with only the listed inputs' signature scripts
/// taken from the wallet's (spec 6.2), checked in the local engine; ready to submit.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Signed {
    pub transaction: String,
    pub id: Hex,
}

// ------------------------------------------------------------------------------ verify

/// Spec 6.3: rebuild each shard's tree from its purchases and compare with frozen roots.
/// The round, m, k and the roots all come from the chain object `against` (F1 review: the
/// page passes no derived game value).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootsInput {
    pub covenant_id: Hex,
    /// The frozen state to compare with: a marker or a pool of the round (checked against
    /// its UTXO entry; the page reads the entry from a public node by outpoint).
    pub against: CovUtxo,
    /// One hex purchase list per shard, shard order.
    pub purchases: Vec<Hex>,
}

/// The rebuilt roots (they matched), with the round and m read from the chain object.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RootsOut {
    pub round: Dec,
    pub m: u32,
    pub roots: Vec<Hex>,
}

/// A block header as a public node returns it (`getBlock`; the fields of rusty-kaspa
/// v2.1.0 `RpcHeader`, `rpc/core/src/model/header.rs:35-51`, every 64-bit integer as a
/// decimal string). `blueWork` is big-endian hex of at most 24 bytes (leading zeros
/// optional). F1 re-review S1.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockHeader {
    pub hash: Hex,
    pub version: u16,
    pub parents_by_level: Vec<Vec<Hex>>,
    pub hash_merkle_root: Hex,
    pub accepted_id_merkle_root: Hex,
    pub utxo_commitment: Hex,
    /// Milliseconds.
    pub timestamp: Dec,
    pub bits: u32,
    pub nonce: Dec,
    pub daa_score: Dec,
    pub blue_work: Hex,
    pub blue_score: Dec,
    pub pruning_point: Hex,
}

/// The anchor block X as a public node reports it, fetched by the anchor proof's X.hash
/// (`getBlock(hash)`): its header and the verbose data's `isChainBlock`. Never from the
/// indexer: the indexer could make a header for any hash it likes (another nonce gives
/// another hash); only a node that holds the block vouches for it. F1 re-review S1.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnchorBlock {
    pub header: BlockHeader,
    pub is_chain_block: bool,
}

/// The chain objects that fix a round's winning numbers (F1 review: the round, m and the
/// reference D are derived from them in Rust, never passed by the page). Each is checked
/// against its UTXO entry (the state rebuilds the entry's script, under the instance's
/// covenant ID); that the entry is live (unspent) is the page's to ensure: it reads every
/// entry from a public node by outpoint, never from the indexer (F1 re-review N1).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum RoundObjs {
    /// Closed, not drawn yet: the marker of round r, the controller of round r + 1, and
    /// the anchor block from a public node. D = the marker's creation DAA score + the
    /// controller's seed_offset (what `draw()` uses). The numbers are `provisional`: X is
    /// bound to the node's header and chain (F1 re-review S1), but until `draw()` the
    /// node's chain can still change.
    Closed {
        marker: CovUtxo,
        controller: CovUtxo,
        #[serde(rename = "anchorBlock")]
        anchor_block: AnchorBlock,
    },
    /// Drawn: a pool of round r (its numbers, round and m) and the instance's drawer. D =
    /// the pool's claim deadline - draw_window - claim_window (drawer check 19); the
    /// numbers from the proof must be the pool's, which `draw()` wrote after checking the
    /// anchor on chain: `final`.
    Drawn { pool: CovUtxo, drawer: CovUtxo },
}

/// Spec J6: the winning numbers from the round's anchor proof.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WinningInput {
    pub covenant_id: Hex,
    pub from: RoundObjs,
    /// 272 bytes, hex.
    pub anchor_proof: Hex,
}

/// Whether winning numbers are the round's for good (F1 re-review S1).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NumbersStatus {
    /// Drawn: equal to the pool's `win`, which `draw()` wrote on chain. The page may show
    /// them as verified.
    Final,
    /// Closed, before `draw()`: from the anchor block as a public node reports it now. The
    /// page shows them as provisional, never as verified.
    Provisional,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WinningOut {
    pub winning: Combination,
    /// `final` (drawn path) or `provisional` (closed path).
    pub status: NumbersStatus,
    /// The round, its m and its reference D, as derived from the chain objects.
    pub round: Dec,
    pub m: u32,
    pub d: Dec,
    /// The anchor block X (the seed source) and its DAA score, from the proof. On the
    /// closed path X.hash is bound to the node's header; on the drawn path the numbers are
    /// bound (to the pool's) but X.hash is the proof's only.
    pub anchor_hash: Hex,
    pub anchor_daa_score: Dec,
}

/// A fee for a priority mass at a rate (sompi/gram).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FeeInput {
    pub priority_mass: Dec,
    pub fee_rate: u32,
}

// ------------------------------------------------------------------------------ quick wallet

/// The quick testnet wallet's key (spec 6.2, S7): kept in this browser only. Absent from a
/// `mainnet` build.
#[cfg(not(feature = "mainnet"))]
#[derive(Tsify, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuickKey {
    /// 32 bytes, hex. Never logged (`Debug` prints it redacted).
    pub secret_key: Hex,
    pub public_key: Hex,
    pub address: Address,
}

/// `quickWalletSign`: sign the listed inputs of a built transaction that this key owns.
#[cfg(not(feature = "mainnet"))]
#[derive(Tsify, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuickSignInput {
    pub built: BuiltTx,
    /// Never logged (`Debug` prints it redacted).
    pub secret_key: Hex,
}

/// What `Debug` prints for a secret key (F1 review: a `{:?}` of a key type in a log or a
/// panic message must not leak it).
#[cfg(not(feature = "mainnet"))]
pub const REDACTED: &str = "<redacted>";

#[cfg(not(feature = "mainnet"))]
impl std::fmt::Debug for QuickKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuickKey").field("secret_key", &REDACTED).field("public_key", &self.public_key).field("address", &self.address).finish()
    }
}

#[cfg(not(feature = "mainnet"))]
impl std::fmt::Debug for QuickSignInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuickSignInput").field("built", &self.built).field("secret_key", &REDACTED).finish()
    }
}
