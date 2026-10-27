//! The indexer API's response types and the page's checks of them (spec frontend 6.3; F2).
//!
//! One definition, two users: `crates/indexer-api` serializes exactly these types, and the
//! page parses them here (the `.d.ts` declares them through tsify, so nothing in `web/`
//! mirrors them by hand, spec 1.1). Conventions are `dto`'s: 64-bit integers as decimal
//! strings, bytes as lowercase hex, camelCase, unknown fields refused.
//!
//! **Nothing the API serves is trusted** (spec 6.3, D132). The page:
//! 1. reads a response as text and parses it here (`parse_*`): the size cap of its kind
//!    (`LIMITS`, which the server also enforces) is checked before any JSON is read, so a
//!    huge or deep response never reaches the worker's memory as an object (F1 part C open
//!    question 4);
//! 2. fetches from a public node the UTXO entry of every object the response names, by
//!    outpoint (never from the API: the entry's liveness is the page's, F1 re-review N1),
//!    and the anchor block's header by hash (F1 re-review S1);
//! 3. calls `check_objects` or `check_round`, which rebuild each object's script from the
//!    embedded template and its state bytes and compare it with the node's entry, rebuild
//!    the shard trees from the purchases and compare them with the chain's roots, and
//!    compare the winning numbers and the anchor proof with the pools `draw()` wrote. They
//!    reuse the F1 checks (`cases::live`, `api::check_roots`, `api::winning_numbers`);
//!    nothing here re-implements a rule.
//!
//! Every disagreement between the API's copy and the chain is `chain_mismatch` ("The
//! indexer's copy does not match the chain."), whatever check found it (the detail names
//! the underlying code, e.g. `tree_mismatch: ...`). A lying or dead API can only stop the
//! page from building; the chain rejects anything wrong (spec 6.3).

use lottery::state::K_MAX;
use lottery::web::cases;
use lottery::web::{Code, Error, Result, verify};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::api;
use crate::convert::{self, Net, hex, unhex};
use crate::dto::{Address, AnchorBlock, Combination, CovUtxo, Dec, Hex, RootsInput, RootsOut, RoundObjs, Utxo, WinningInput, WinningOut};
use crate::views::{RoleName, StateView};

/// The API's version prefix: every endpoint is under `/v1/`.
pub const API_VERSION: &str = "v1";

// ------------------------------------------------------------------------------ limits

/// The largest response body of each kind, in bytes. The server refuses to send more
/// (`too_large`), and the page refuses to parse more (`parse_*`, `bytes_length`).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    /// `GET /v1/instance` (the launch bundle; testnet-10's is 347 KB).
    pub instance: u32,
    /// `GET /v1/status`, an error body, `GET /v1/rounds/{r}/anchor-proof`.
    pub small: u32,
    /// `GET /v1/rounds` and `GET /v1/rounds/{r}`.
    pub round: u32,
    /// `GET /v1/rounds/{r}/purchases` and `GET /v1/rounds/{r}/shards/{i}` (12 hex digits per
    /// combination: about 700,000 combinations).
    pub purchases: u32,
    /// `GET /v1/tickets`, `/v1/vault`, `/v1/positions`, `/v1/requests`.
    pub objects: u32,
    /// The most objects one `nodeEntries` call reads (and one list response returns): the page
    /// asks in batches of at most this many (F4 review S7).
    pub node_objects: u32,
    /// The largest backup file (`backup::BACKUP_MAX`).
    pub backup: u32,
    /// The page's cap on one worker call's arguments as JSON (`backup::WORKER_ARGS_MAX`; equal to
    /// `ARGS_MAX_BYTES` in `web/src/worker/client.ts`, pinned by a Vitest).
    pub worker_args: u32,
}

pub const LIMITS: Limits = Limits {
    instance: 4 << 20,
    small: 64 << 10,
    round: 1 << 20,
    purchases: 8 << 20,
    objects: 4 << 20,
    node_objects: OBJECTS_MAX,
    backup: crate::backup::BACKUP_MAX as u32,
    worker_args: crate::backup::WORKER_ARGS_MAX as u32,
};

/// The most rounds one `GET /v1/rounds` returns (newest first; `?before=r` pages back).
pub const ROUNDS_PAGE: u32 = 200;

/// The most objects one list response returns (tickets, positions, requests).
pub const OBJECTS_MAX: u32 = 2_000;

// ------------------------------------------------------------------------------ responses

/// A covenant UTXO as the index holds it: its role, outpoint, entry and state bytes. The
/// page uses the outpoint to read the entry from a public node, and the state bytes only
/// after `check_objects`/`check_round` (never `amount` or `blockDaaScore` from here).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiObject {
    pub role: RoleName,
    /// 32 bytes, hex.
    pub transaction_id: Hex,
    pub index: u32,
    /// Sompi, as the index holds it.
    pub amount: Dec,
    pub block_daa_score: Dec,
    /// Safe JSON form (2-byte big-endian version, then the script).
    pub script_public_key: Hex,
    /// The encoded state bytes (`bundle/1`'s form), hex.
    pub state: Hex,
}

/// A round's status: `open` (sales), `closing` (closed, waiting for `draw()`), `drawn`, or
/// `void` (no draw in the window, D96).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RoundStatus {
    Open,
    Closing,
    Drawn,
    Void,
}

/// `GET /v1/status`: what the server serves and how far its index has followed the chain.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiStatus {
    /// "v1".
    pub api: String,
    /// "testnet-10" or "simnet".
    pub network: String,
    pub covenant_id: Hex,
    /// Add and remove lines applied from the index log.
    pub log_lines: Dec,
    /// The chain block the indexer last reached and its DAA score (from its snapshot; a
    /// lower bound of its progress), if known.
    pub last_chain_block: Option<Hex>,
    pub last_chain_daa_score: Option<Dec>,
    /// The newest round the index knows.
    pub current_round: Option<Dec>,
    /// Missed transitions (the index refuses to serve trees while any exist).
    pub gaps: Vec<String>,
    /// The index stopped applying the log at this error (it serves its last good state).
    pub error: Option<String>,
    /// Unix milliseconds of the last reload.
    pub updated: Dec,
}

/// One row of `GET /v1/rounds` (the index's figures; the page checks a round through
/// `GET /v1/rounds/{r}` before showing it as verified).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiRoundRow {
    pub round: Dec,
    pub status: RoundStatus,
    pub m: Option<u32>,
    pub k: u32,
    /// Number of `buy()` transactions and of combinations bought.
    pub buys: u32,
    pub combinations: u32,
    /// Sompi: the shards' sales (open) or the marker's (closed).
    pub sales: Option<Dec>,
    /// Sompi: the controller's jackpot (open) or the marker's (closed).
    pub jackpot: Option<Dec>,
    pub incomplete: bool,
}

/// `GET /v1/rounds`: newest first, at most `ROUNDS_PAGE` rows (`?before=r` for older ones).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiRounds {
    pub covenant_id: Hex,
    pub rounds: Vec<ApiRoundRow>,
    /// The `before` value of the next page, if there are older rounds.
    pub next: Option<Dec>,
}

/// A round's `close()`, as the index saw it.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiClose {
    pub transaction_id: Hex,
    /// The marker's creation DAA score (D = it + seed_offset).
    pub marker_daa_score: Dec,
    /// The k frozen roots, shard order.
    pub frozen_roots: Vec<Hex>,
    /// m of the next round.
    pub m_next: u32,
}

/// A prize pool as `draw()` created it (the live pool is in `objects`, if still live).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiResultPool {
    pub tier: u8,
    pub slice: Dec,
    pub value: Dec,
    pub transaction_id: Hex,
    pub index: u32,
}

/// A round's `draw()` (or void), as the index saw it.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiDraw {
    pub transaction_id: Hex,
    pub void: bool,
    /// The winning numbers (absent in a void).
    pub winning: Option<Combination>,
    /// The anchor block X (zero in a void).
    pub anchor_hash: Hex,
    pub claim_deadline: Dec,
    pub pools: Vec<ApiResultPool>,
}

/// `GET /v1/rounds/{r}`: the round as the index holds it, with the live objects a page needs
/// for it: open: the controller, the params and the round's shards; closing: the marker,
/// the next controller, the drawer and the vault (`draw()`'s inputs); drawn or void: the
/// round's live pools and record, and the drawer.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiRound {
    pub round: Dec,
    pub status: RoundStatus,
    pub m: Option<u32>,
    pub k: u32,
    /// T, the sales deadline (DAA score), once known.
    pub deadline: Option<Dec>,
    /// D, the anchor reference (DAA score), once closed.
    pub anchor_daa_score: Option<Dec>,
    pub close: Option<ApiClose>,
    pub draw: Option<ApiDraw>,
    pub buys: u32,
    pub combinations: u32,
    pub objects: Vec<ApiObject>,
    /// Why the index cannot rebuild this round's trees, if it cannot.
    pub incomplete: Option<String>,
    pub gaps: Vec<String>,
}

/// `GET /v1/rounds/{r}/shards/{i}`: one shard's purchases, six bytes per combination (five
/// main numbers and the bonus), in the shard's purchase order: enough to rebuild its tree.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiShard {
    pub round: Dec,
    pub shard_index: u32,
    pub m: u32,
    pub count: u32,
    pub purchases: Hex,
}

/// `GET /v1/rounds/{r}/purchases`: every shard's purchases (as `ApiShard`, shard order), the
/// form `buildClaim`, `buildDraw` and `checkRoots` take.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiPurchases {
    pub round: Dec,
    pub m: u32,
    pub k: u32,
    pub shards: Vec<Hex>,
}

/// Where an anchor proof comes from: the round's `draw()` transaction, or built from a node
/// by the server (before the draw; only a server that follows a node can).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProofSource {
    Draw,
    Node,
}

/// `GET /v1/rounds/{r}/anchor-proof` (spec 6.1 fallback): the 272-byte proof, unverified
/// (`check_round` binds it; `draw()` checks it on chain).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiAnchorProof {
    pub round: Dec,
    pub anchor_proof: Hex,
    pub source: ProofSource,
}

/// A purchase as the index saw it (for "My tickets" history; the live ticket is in
/// `tickets`).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiBuy {
    pub round: Dec,
    pub shard_index: u32,
    pub transaction_id: Hex,
    /// The ticket output it created (output 1).
    pub ticket_index: u32,
    pub combinations: Vec<Combination>,
    /// Sompi paid to the referrer (0 without a referral output).
    pub referral: Dec,
}

/// `GET /v1/tickets?payout=`: the live tickets paying that address, and its purchases.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiTickets {
    pub payout: Address,
    pub tickets: Vec<ApiObject>,
    pub bought: Vec<ApiBuy>,
}

/// `GET /v1/vault` (the vault, the controller, the drawer and the live records),
/// `GET /v1/positions?owner=` (the owner's positions), `GET /v1/requests?owner=` (the
/// owner's requests and the live records they settle against).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiObjects {
    pub owner: Option<Address>,
    pub objects: Vec<ApiObject>,
}

/// The API's error codes (one message each).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorCode {
    NotFound,
    MethodNotAllowed,
    BadRequest,
    AddressNetwork,
    RoundUnknown,
    ShardUnknown,
    AnchorUnavailable,
    TooLarge,
    Busy,
    Timeout,
    Unavailable,
}

impl ApiErrorCode {
    pub fn message(self) -> &'static str {
        match self {
            ApiErrorCode::NotFound => "No such endpoint.",
            ApiErrorCode::MethodNotAllowed => "The indexer API is read-only: GET only.",
            ApiErrorCode::BadRequest => "The request is malformed.",
            ApiErrorCode::AddressNetwork => "The address is not on this indexer's network.",
            ApiErrorCode::RoundUnknown => "The index has no such round.",
            ApiErrorCode::ShardUnknown => "The round has no such shard.",
            ApiErrorCode::AnchorUnavailable => "No anchor proof is available for this round.",
            ApiErrorCode::TooLarge => "The response would be too large.",
            ApiErrorCode::Busy => "The indexer API is busy; retry.",
            ApiErrorCode::Timeout => "The request took too long.",
            ApiErrorCode::Unavailable => "The index is not available.",
        }
    }

    /// The HTTP status of the code.
    pub fn status(self) -> u16 {
        match self {
            ApiErrorCode::NotFound | ApiErrorCode::RoundUnknown | ApiErrorCode::ShardUnknown | ApiErrorCode::AnchorUnavailable => 404,
            ApiErrorCode::MethodNotAllowed => 405,
            ApiErrorCode::BadRequest | ApiErrorCode::AddressNetwork => 400,
            ApiErrorCode::TooLarge => 500,
            ApiErrorCode::Busy | ApiErrorCode::Unavailable => 503,
            ApiErrorCode::Timeout => 408,
        }
    }
}

#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiErrorDetail {
    pub code: ApiErrorCode,
    pub message: String,
    pub detail: String,
}

/// Every error response body: `{ "error": { code, message, detail } }`.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiErrorBody {
    pub error: ApiErrorDetail,
}

impl ApiErrorBody {
    pub fn new(code: ApiErrorCode, detail: impl Into<String>) -> Self {
        Self { error: ApiErrorDetail { code, message: code.message().into(), detail: detail.into() } }
    }
}

// ------------------------------------------------------------------------------ parse

fn parse_capped<T: serde::de::DeserializeOwned>(what: &str, body: &str, cap: u32) -> Result<T> {
    if body.len() > cap as usize {
        return Err(Error::new(Code::BytesLength, format!("{what}: {} bytes, the cap is {cap}", body.len())));
    }
    serde_json::from_str(body).map_err(|e| Error::new(Code::InputFormat, format!("{what}: {e}")))
}

pub fn parse_status(body: &str) -> Result<ApiStatus> {
    parse_capped("status", body, LIMITS.small)
}
pub fn parse_rounds(body: &str) -> Result<ApiRounds> {
    parse_capped("rounds", body, LIMITS.round)
}
pub fn parse_round(body: &str) -> Result<ApiRound> {
    let r: ApiRound = parse_capped("round", body, LIMITS.round)?;
    // F5 review: every transaction ID and hash is 32 bytes of hex (a page shows and links them).
    let mut ids: Vec<(&str, &str)> = r.objects.iter().map(|o| ("objects.transactionId", o.transaction_id.as_str())).collect();
    if let Some(c) = &r.close {
        ids.push(("close.transactionId", &c.transaction_id));
        ids.extend(c.frozen_roots.iter().map(|h| ("close.frozenRoots", h.as_str())));
    }
    if let Some(d) = &r.draw {
        ids.push(("draw.transactionId", &d.transaction_id));
        ids.push(("draw.anchorHash", &d.anchor_hash));
        ids.extend(d.pools.iter().map(|p| ("draw.pools.transactionId", p.transaction_id.as_str())));
    }
    hex32s(&ids)?;
    Ok(r)
}

/// Each `(what, value)` is 64 hex digits.
fn hex32s(ids: &[(&str, &str)]) -> Result<()> {
    for (what, v) in ids {
        if v.len() != 64 || !v.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::new(Code::InputFormat, format!("{what}: {v:?} is not 32 bytes of hex")));
        }
    }
    Ok(())
}
pub fn parse_shard(body: &str) -> Result<ApiShard> {
    parse_capped("shard", body, LIMITS.purchases)
}
pub fn parse_purchases(body: &str) -> Result<ApiPurchases> {
    parse_capped("purchases", body, LIMITS.purchases)
}
pub fn parse_anchor_proof(body: &str) -> Result<ApiAnchorProof> {
    parse_capped("anchor proof", body, LIMITS.small)
}
pub fn parse_tickets(body: &str) -> Result<ApiTickets> {
    let t: ApiTickets = parse_capped("tickets", body, LIMITS.objects)?;
    let mut ids: Vec<(&str, &str)> = t.tickets.iter().map(|o| ("tickets.transactionId", o.transaction_id.as_str())).collect();
    ids.extend(t.bought.iter().map(|b| ("bought.transactionId", b.transaction_id.as_str())));
    hex32s(&ids)?;
    Ok(t)
}
pub fn parse_objects(body: &str) -> Result<ApiObjects> {
    let o: ApiObjects = parse_capped("objects", body, LIMITS.objects)?;
    hex32s(&o.objects.iter().map(|o| ("objects.transactionId", o.transaction_id.as_str())).collect::<Vec<_>>())?;
    Ok(o)
}
pub fn parse_error(body: &str) -> Result<ApiErrorBody> {
    parse_capped("error", body, LIMITS.small)
}

// ------------------------------------------------------------------------------ checks

/// Any failure of a check of the API's copy against the chain: `chain_mismatch`, with the
/// underlying code in the detail (spec 6.3: one message for every such refusal).
fn mismatch(e: Error) -> Error {
    if e.code == Code::ChainMismatch {
        return e;
    }
    Error::new(Code::ChainMismatch, format!("{}: {}", e.code.as_str(), e.detail))
}

fn chain_mismatch(detail: impl Into<String>) -> Error {
    Error::new(Code::ChainMismatch, detail.into())
}

/// `checkIndexerObjects`: API objects against the node's entries for their outpoints.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectsCheckInput {
    pub covenant_id: Hex,
    pub objects: Vec<ApiObject>,
    /// The entries a public node returned for the objects' outpoints (any order; extra ones
    /// ignored). An object with no entry here is not live on the node: `chain_mismatch`.
    pub utxos: Vec<Utxo>,
}

/// An object that passed: its role, the covenant UTXO ready for a builder (the node's entry
/// with the API's state bytes) and its decoded state.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckedObject {
    pub role: RoleName,
    pub object: CovUtxo,
    pub view: StateView,
}

fn find_utxo<'a>(o: &ApiObject, utxos: &'a [Utxo]) -> Result<&'a Utxo> {
    let id = o.transaction_id.to_ascii_lowercase();
    utxos
        .iter()
        .find(|u| u.transaction_id.eq_ignore_ascii_case(&id) && u.index == o.index)
        .ok_or_else(|| chain_mismatch(format!("{:?} at {}:{}: the node has no live UTXO at this outpoint", o.role, o.transaction_id, o.index)))
}

/// One API object against the node's entry: the same outpoint, the same entry as the API
/// claims (amount, creation DAA, script), and its state bytes rebuilding that script under
/// the instance's covenant ID (`cases::live`).
pub(crate) fn check_object(net: Net, cov: &kaspa_hashes::Hash, o: &ApiObject, utxos: &[Utxo]) -> Result<(CheckedObject, lottery::index::State)> {
    let u = find_utxo(o, utxos)?;
    let same_entry = u.amount == o.amount && u.block_daa_score == o.block_daa_score && u.script_public_key.eq_ignore_ascii_case(&o.script_public_key);
    if !same_entry {
        return Err(chain_mismatch(format!("{:?} at {}:{}: the node's entry differs from the indexer's", o.role, o.transaction_id, o.index)));
    }
    let object = CovUtxo { utxo: u.clone(), state: o.state.clone() };
    let s = cases::live(o.role.role(), &convert::obj("state", &object).map_err(mismatch)?, cov).map_err(mismatch)?;
    Ok((CheckedObject { role: o.role, object, view: StateView::of(&s, net) }, s))
}

fn cov_of(s: &str) -> Result<kaspa_hashes::Hash> {
    Ok(kaspa_hashes::Hash::from_bytes(convert::hash32("covenantId", s)?))
}

/// Spec 6.3: every API object checked against the node's entry for its outpoint.
pub fn check_objects(net: Net, i: &ObjectsCheckInput) -> Result<Vec<CheckedObject>> {
    let cov = cov_of(&i.covenant_id)?;
    if i.objects.len() > OBJECTS_MAX as usize || i.utxos.len() > 2 * OBJECTS_MAX as usize {
        return Err(Error::new(Code::InputRange, format!("{} objects, {} UTXOs", i.objects.len(), i.utxos.len())));
    }
    i.objects.iter().map(|o| check_object(net, &cov, o, &i.utxos).map(|x| x.0)).collect()
}

/// `checkIndexerRound`: a round response (and optionally its purchases and anchor proof)
/// against the chain.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoundCheckInput {
    pub covenant_id: Hex,
    pub round: ApiRound,
    /// The node's entries for `round.objects`' outpoints.
    pub utxos: Vec<Utxo>,
    /// `GET /v1/rounds/{r}/purchases`, to check against the chain's roots.
    #[serde(default)]
    pub purchases: Option<ApiPurchases>,
    /// `GET /v1/rounds/{r}/anchor-proof`, to check against the pools (drawn) or the node's
    /// anchor block (closing).
    #[serde(default)]
    pub anchor_proof: Option<ApiAnchorProof>,
    /// For a closing round's proof: the anchor block as a public node reports it
    /// (`getBlock(X.hash)`), never from the API.
    #[serde(default)]
    pub anchor_block: Option<AnchorBlock>,
}

/// A round that passed. Only the fields checked against the chain are here.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckedRound {
    pub round: Dec,
    /// The status the chain objects show (`open`: live shards or round r's controller;
    /// `closing`: the marker; `drawn`: a pool or the record), or the API's when no live object
    /// of the round shows one (`void`, or a drawn round whose pools are gone); `statusChecked`
    /// says which. A page shows only a checked status (F2/F3 review S4).
    pub status: RoundStatus,
    pub status_checked: bool,
    pub m: Option<u32>,
    /// F5 review S3: `m` is fixed by a live marker, pool, open controller or shard of the round
    /// (else it is the indexer's, unverified).
    pub m_checked: bool,
    pub k: u32,
    pub objects: Vec<CheckedObject>,
    /// The roots rebuilt from the purchases (they matched the chain's), position i for shard i,
    /// when purchases were given and the chain fixes every one of the k roots: the marker's or a
    /// pool's frozen roots once closed; while open, round r's controller and a live shard for
    /// every index 0..k-1 (else the check fails: F2/F3 review M1).
    pub roots: Option<RootsOut>,
    /// The purchases, all k lists checked against `roots` (as given), ready for
    /// `buildClaim`/`buildDraw`. Present exactly when `roots` is.
    pub purchases: Option<Vec<Hex>>,
    /// The winning numbers from the anchor proof (`final` against the pools, `provisional`
    /// against the node's anchor block), when a proof was given.
    pub winning: Option<WinningOut>,
    /// The indexer says void and the chain supports it as far as the chain can (F4 review S1):
    /// the live drawer served is past round r (the round was drawn or voided), and no live
    /// object of round r that a void cannot leave was served (its marker, a pool, its controller
    /// or a shard). The chain cannot show a void positively (a void leaves nothing of the round;
    /// a draw's pools are found only by their scripts, which need the winning numbers), so the
    /// page also refuses "void" when its own copy shows the marker live or holds the result.
    pub void_supported: bool,
    /// F5: `winning`'s X.hash is bound to a public node's header and selected chain: always on the
    /// closed path (it needs `anchorBlock`); on the drawn path only when `anchorBlock` was given
    /// (the pool's numbers alone do not fix X.hash: a 32-byte value giving the same numbers is
    /// easy to find). A page shows X.hash as checked only when this is true.
    pub anchor_bound: bool,
    /// F5 (S3, S4): what the node-checked objects fix of the round (T, S, J, D, the claim
    /// deadline, the void lock, the frozen roots, the caller reward and R), each with the
    /// objects it is read from; a figure no live object fixes is absent.
    pub facts: crate::rounds::RoundFacts,
}

fn dec_of(what: &str, s: &str) -> Result<i64> {
    convert::dec_i64(what, s)
}

/// Spec 6.3 for a round: the objects against the node's entries (state, script, covenant
/// ID, and the round each belongs to); the status, m and k the API states against those
/// objects; the purchases against the chain's roots (the marker's or a pool's frozen roots
/// once closed, each live shard's root while open); the winning numbers against every live
/// pool's `win`; and the anchor proof against the pools (`final`) or, before the draw,
/// against the anchor block a public node reports (`provisional`). Any disagreement is
/// `chain_mismatch`.
/// A round's API objects, each checked against the node's entry (`check_object`) and required to
/// belong to round `r`, sorted by role (shared by `check_round` and `rounds::verify_round`).
pub(crate) struct Classified {
    pub objects: Vec<CheckedObject>,
    pub shards: Vec<(CovUtxo, lottery::state::ShardState)>,
    pub marker: Option<(CovUtxo, lottery::state::MarkerState)>,
    pub pools: Vec<(CovUtxo, lottery::state::PoolState)>,
    pub controller: Option<(CovUtxo, lottery::state::ControllerState)>,
    pub drawer: Option<(CovUtxo, lottery::state::DrawerState)>,
    pub record: Option<(CovUtxo, lottery::state::RecordState)>,
}

pub(crate) fn classify(net: Net, cov: &kaspa_hashes::Hash, r: i64, objects: &[ApiObject], utxos: &[Utxo]) -> Result<Classified> {
    use lottery::index::State;
    let mut c = Classified { objects: vec![], shards: vec![], marker: None, pools: vec![], controller: None, drawer: None, record: None };
    for o in objects {
        let (co, s) = check_object(net, cov, o, utxos)?;
        let belongs = match &s {
            State::Shard(x) => x.round == r,
            State::Marker(x) => x.round == r,
            State::Pool(x) => x.round == r,
            State::Record(x) => x.round == r,
            State::Controller(x) => x.round == r || x.round == r + 1,
            State::Drawer(x) => x.round >= r,
            State::Params(_) | State::Vault(_) => true,
            State::Ticket(_) | State::Position(_) | State::Request(_) => false,
        };
        if !belongs {
            return Err(chain_mismatch(format!("{:?} of round {} served for round {r}", s.role(), s.round())));
        }
        match &s {
            State::Shard(x) => c.shards.push((co.object.clone(), x.clone())),
            State::Marker(x) => c.marker = Some((co.object.clone(), x.clone())),
            State::Pool(x) => c.pools.push((co.object.clone(), x.clone())),
            State::Controller(x) => c.controller = Some((co.object.clone(), x.clone())),
            State::Drawer(x) => c.drawer = Some((co.object.clone(), x.clone())),
            State::Record(x) => c.record = Some((co.object.clone(), x.clone())),
            _ => {}
        }
        c.objects.push(co);
    }
    Ok(c)
}

pub fn check_round(net: Net, i: &RoundCheckInput) -> Result<CheckedRound> {
    let cov = cov_of(&i.covenant_id)?;
    let a = &i.round;
    let r = dec_of("round.round", &a.round)?;
    if a.objects.len() > 64 || i.utxos.len() > 256 {
        return Err(Error::new(Code::InputRange, format!("{} objects, {} UTXOs", a.objects.len(), i.utxos.len())));
    }
    let cl = classify(net, &cov, r, &a.objects, &i.utxos)?;
    let facts = crate::rounds::facts(r, &cl)?;
    let Classified { objects, shards, marker, pools, controller, drawer, record } = cl;
    // The status the chain shows. A live controller of round r means round r is not closed yet
    // (`close()` spends it and creates round r + 1's): open, whether or not its shards were served
    // (F2/F3 review S4: an open round served with its controller and without its shards must not
    // pass as drawn).
    let open_controller = controller.as_ref().is_some_and(|(_, c)| c.round == r);
    let shown = if marker.is_some() {
        Some(RoundStatus::Closing)
    } else if !pools.is_empty() {
        Some(RoundStatus::Drawn)
    } else if !shards.is_empty() || open_controller {
        Some(RoundStatus::Open)
    } else {
        None
    };
    if open_controller && (marker.is_some() || !pools.is_empty() || record.is_some()) {
        return Err(chain_mismatch(format!("round {r}: its controller is live, so it is not closed, yet a closed round's object was served")));
    }
    // F5 review S3: a settlement record of round r is written by its `draw()` or by its void (a
    // void with pending vault requests also leaves one), so a record alone shows the round past
    // its draw window's transition without telling a draw from a void: no checked status, but
    // anything other than drawn or void contradicts it.
    if shown.is_none() && record.is_some() && !matches!(a.status, RoundStatus::Drawn | RoundStatus::Void) {
        return Err(chain_mismatch(format!("the indexer says round {r} is {:?}; its settlement record says it was drawn or voided", a.status)));
    }
    if let Some(s) = shown {
        if s != a.status {
            return Err(chain_mismatch(format!("the indexer says round {r} is {:?}; the chain's objects say {s:?}", a.status)));
        }
    }
    if a.status == RoundStatus::Void && (!pools.is_empty() || a.draw.as_ref().is_some_and(|d| !d.void || d.winning.is_some())) {
        return Err(chain_mismatch(format!("round {r}: a void with pools or winning numbers")));
    }
    if marker.is_some() && controller.as_ref().is_some_and(|(_, c)| c.round != r + 1) {
        return Err(chain_mismatch(format!("round {r} is closing but the controller served is not round {}'s", r + 1)));
    }
    // m and k against the chain objects.
    let chain_mk: Vec<(i64, i64)> = marker
        .iter()
        .map(|(_, x)| (x.m, x.k))
        .chain(pools.iter().map(|(_, p)| (p.m, p.k)))
        .chain(controller.iter().filter(|(_, c)| c.round == r).map(|(_, c)| (c.m, c.k)))
        .collect();
    for (m, k) in &chain_mk {
        if a.m.map(i64::from) != Some(*m) || i64::from(a.k) != *k {
            return Err(chain_mismatch(format!("round {r}: the indexer says m = {:?}, k = {}; the chain says m = {m}, k = {k}", a.m, a.k)));
        }
    }
    if shards.iter().any(|(_, s)| a.m.map(i64::from) != Some(s.m) || s.shard_index >= i64::from(a.k)) {
        return Err(chain_mismatch(format!("round {r}: a shard's m or index differs from the indexer's m and k")));
    }
    // F5 review S3: m is the chain's only when a live marker, pool, open controller or shard of the
    // round fixed it (compared above); otherwise it is the indexer's.
    let m_checked = a.m.is_some() && (!chain_mk.is_empty() || !shards.is_empty());
    // F5 review S4: the close's and the draw's transaction IDs against the live objects only they
    // create (`facts.close_tx`, `facts.draw_tx`).
    if let (Some(c), Some(t)) = (&a.close, &facts.close_tx) {
        if !c.transaction_id.eq_ignore_ascii_case(&t.transaction_id) {
            return Err(chain_mismatch(format!("round {r}: the indexer's close() is {}, the chain's {}", c.transaction_id, t.transaction_id)));
        }
    }
    if let (Some(d), Some(t)) = (&a.draw, &facts.draw_tx) {
        if !d.transaction_id.eq_ignore_ascii_case(&t.transaction_id) {
            return Err(chain_mismatch(format!("round {r}: the indexer's draw() is {}, the chain's {}", d.transaction_id, t.transaction_id)));
        }
    }
    // The frozen roots the API states against the chain's (marker or pool).
    if let Some(close) = &a.close {
        let chain_roots = marker.as_ref().map(|(_, x)| x.roots.clone()).or_else(|| pools.first().map(|(_, p)| p.roots.clone()));
        if let Some(roots) = chain_roots {
            let mut want = vec![0u8; 32 * K_MAX];
            if close.frozen_roots.len() > K_MAX {
                return Err(chain_mismatch("more frozen roots than shards"));
            }
            for (n, h) in close.frozen_roots.iter().enumerate() {
                want[32 * n..32 * (n + 1)].copy_from_slice(&convert::hash32("close.frozenRoots", h).map_err(mismatch)?);
            }
            if want != roots {
                return Err(chain_mismatch(format!("round {r}: the indexer's frozen roots differ from the chain's")));
            }
        }
    }
    // The purchases against the chain's roots.
    let (mut roots_out, mut checked_purchases) = (None, None);
    if let Some(p) = &i.purchases {
        if dec_of("purchases.round", &p.round)? != r || Some(p.m) != a.m || p.k != a.k || p.shards.len() != a.k as usize {
            return Err(chain_mismatch(format!("round {r}: the purchases' round, m or k differ from the round's")));
        }
        let against = marker.as_ref().map(|x| x.0.clone()).or_else(|| pools.first().map(|x| x.0.clone()));
        if let Some(against) = against {
            let out = api::check_roots(&RootsInput { covenant_id: i.covenant_id.clone(), against, purchases: p.shards.clone() }).map_err(mismatch)?;
            roots_out = Some(out);
        } else if shown == Some(RoundStatus::Open) {
            // Open (F2/F3 review M1): every list is checked, or none is returned. That needs round r's
            // controller (it fixes m and k on the chain, checked above) and, for every index 0..k-1,
            // exactly one live shard of the round whose root the list rebuilds. The roots come out
            // keyed by shard index (position i is shard i's).
            if !open_controller {
                return Err(chain_mismatch(format!("round {r}: its controller was not served, so k and m are not the chain's")));
            }
            if a.k as usize > K_MAX {
                return Err(chain_mismatch(format!("round {r}: k = {} is above {K_MAX}", a.k)));
            }
            let mut by_index: Vec<Option<&lottery::state::ShardState>> = vec![None; a.k as usize];
            for (_, s) in &shards {
                let slot = by_index.get_mut(s.shard_index as usize).ok_or_else(|| chain_mismatch(format!("round {r}: shard index {} is not below k", s.shard_index)))?;
                if slot.replace(s).is_some() {
                    return Err(chain_mismatch(format!("round {r}: shard {} served twice", s.shard_index)));
                }
            }
            let mut roots = vec![];
            for (n, (slot, list)) in by_index.iter().zip(&p.shards).enumerate() {
                let s = slot.ok_or_else(|| chain_mismatch(format!("round {r}: shard {n} was not served, so its purchases are not checked")))?;
                let tree = verify::rebuild_tree_bytes(s.m, &unhex("purchases.shards", list).map_err(mismatch)?).map_err(mismatch)?;
                if tree.root() != s.root {
                    return Err(chain_mismatch(format!("tree_mismatch: shard {n}'s purchases do not rebuild its live root")));
                }
                roots.push(hex(&s.root));
            }
            roots_out = Some(RootsOut { round: r.to_string(), m: a.m.unwrap_or(0), roots });
        }
        if roots_out.is_some() {
            checked_purchases = Some(p.shards.clone());
        }
    }
    // The winning numbers against the pools `draw()` wrote.
    if let Some(d) = &a.draw {
        let w = d.winning.map(|c| [c.main[0], c.main[1], c.main[2], c.main[3], c.main[4], c.bonus]);
        if let Some((_, p)) = pools.iter().find(|(_, p)| Some(p.win) != w) {
            return Err(chain_mismatch(format!("round {r}: the indexer's winning numbers are not pool tier {}'s", p.tier)));
        }
    }
    // The anchor proof.
    let (mut winning, mut anchor_bound) = (None, false);
    if let Some(ap) = &i.anchor_proof {
        if dec_of("anchorProof.round", &ap.round)? != r {
            return Err(chain_mismatch("the anchor proof is for another round"));
        }
        let from = if let (Some(pool), Some(dr)) = (pools.first(), drawer.as_ref()) {
            Some(RoundObjs::Drawn { pool: pool.0.clone(), drawer: dr.0.clone(), anchor_block: i.anchor_block.clone() })
        } else if let (Some(mk), Some(ct), Some(block)) = (marker.as_ref(), controller.as_ref(), i.anchor_block.as_ref()) {
            Some(RoundObjs::Closed { marker: mk.0.clone(), controller: ct.0.clone(), anchor_block: block.clone() })
        } else {
            None
        };
        if let Some(from) = from {
            let (w, bound) = api::winning_numbers_bound(&WinningInput { covenant_id: i.covenant_id.clone(), from, anchor_proof: ap.anchor_proof.clone() }).map_err(mismatch)?;
            if let Some(d) = &a.draw {
                if !d.anchor_hash.eq_ignore_ascii_case(&w.anchor_hash) || d.winning != Some(w.winning) {
                    return Err(chain_mismatch(format!("round {r}: the indexer's anchor hash or numbers are not the proof's")));
                }
            }
            if a.anchor_daa_score.as_deref() != Some(w.d.as_str()) {
                return Err(chain_mismatch(format!("round {r}: the indexer's D is not the chain's {}", w.d)));
            }
            winning = Some(w);
            anchor_bound = bound;
        }
    }
    let void_supported = a.status == RoundStatus::Void && shown.is_none() && drawer.as_ref().is_some_and(|(_, d)| d.round > r);
    Ok(CheckedRound {
        void_supported,
        round: r.to_string(),
        status: shown.unwrap_or(a.status),
        status_checked: shown.is_some(),
        m: a.m,
        m_checked,
        k: a.k,
        objects,
        roots: roots_out,
        purchases: checked_purchases,
        winning,
        anchor_bound,
        facts,
    })
}

/// `checkIndexerInstance`: the launch bundle the API serves (`GET /v1/instance`, as text).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstanceCheckInput {
    /// The response body, unparsed (its hash is over these bytes).
    pub bundle: String,
    /// The instance the page is built for (build-time configuration, spec 6.5).
    pub covenant_id: Hex,
    /// The launch bundle's hash, if the page is built with one (blake2b-256 of the file
    /// bytes, hex).
    #[serde(default)]
    pub bundle_hash: Option<Hex>,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckedInstance {
    pub covenant_id: Hex,
    pub network: String,
    pub k: u32,
    pub b_max: u32,
    pub genesis_transaction_id: Hex,
    /// blake2b-256 of the bundle bytes, hex.
    pub bundle_hash: Hex,
    /// Every integer genesis value (`bundle/1` `params`).
    pub params: Vec<Param>,
}

/// A named integer genesis value (`<object>.<field>`).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Param {
    pub name: String,
    pub value: Dec,
}

/// blake2b-256 (unkeyed) of a launch bundle's bytes: the hash a page is built with.
pub fn bundle_hash(bytes: &[u8]) -> [u8; 32] {
    let h = blake2b_simd::Params::new().hash_length(32).hash(bytes);
    h.as_bytes().try_into().expect("32-byte hash")
}

/// The launch bundle verified locally (`bundle::verify`: toolchain, templates equal to the
/// embedded ones, genesis transaction, covenant ID, states and values), for the instance and
/// network the page runs on.
pub fn check_instance(net: Net, i: &InstanceCheckInput) -> Result<CheckedInstance> {
    if i.bundle.len() > LIMITS.instance as usize {
        return Err(Error::new(Code::BytesLength, format!("bundle of {} bytes", i.bundle.len())));
    }
    let want = convert::hash32("covenantId", &i.covenant_id)?;
    let h = bundle_hash(i.bundle.as_bytes());
    if let Some(bh) = &i.bundle_hash {
        if convert::hash32("bundleHash", bh)? != h {
            return Err(chain_mismatch("the bundle's hash is not the one this page is built for"));
        }
    }
    let b: lottery::bundle::Bundle = lottery::store::read(&i.bundle, lottery::store::BUNDLE).map_err(|e| Error::new(Code::StoreFormat, e))?;
    if b.network != net.name() {
        return Err(Error::new(Code::AddressNetwork, format!("the bundle is for {}, the page runs on {}", b.network, net.name())));
    }
    let v = lottery::bundle::verify(&b).map_err(chain_mismatch)?;
    if v.cov.as_bytes() != want {
        return Err(chain_mismatch(format!("the bundle is instance {}, not this page's", v.cov)));
    }
    Ok(CheckedInstance {
        covenant_id: hex(&want),
        network: b.network.clone(),
        k: v.k as u32,
        b_max: v.templates.b_max as u32,
        genesis_transaction_id: hex(&b.genesis_txid),
        bundle_hash: hex(&h),
        params: b.params.iter().map(|(k, v)| Param { name: k.clone(), value: v.to_string() }).collect(),
    })
}

/// The API object of a live index UTXO (the server's side of the shape).
pub fn api_object(role: lottery::index::Role, outpoint: &kaspa_consensus_core::tx::TransactionOutpoint, value: u64, daa: u64, spk: &kaspa_consensus_core::tx::ScriptPublicKey, state: &[u8]) -> ApiObject {
    ApiObject {
        role: RoleName::of(role),
        transaction_id: hex(&outpoint.transaction_id.as_bytes()),
        index: outpoint.index,
        amount: value.to_string(),
        block_daa_score: daa.to_string(),
        script_public_key: convert::spk_hex(spk),
        state: hex(state),
    }
}
