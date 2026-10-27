//! The wasm-bindgen exports: one thin wrapper per `api` function. Each takes plain JS values
//! (typed in the generated `.d.ts` by `unchecked_param_type`/`unchecked_return_type` with the
//! tsify-declared types), returns `{ ok: T }` or `{ error: ApiError }` (`Outcome<T>`), and
//! never throws: a malformed input object is `input_format`. Every call but `start` needs a
//! started session (`not_started` otherwise); a `mainnet` build never starts (W4) and also
//! throws at instantiation (`refuse_mainnet`).

use std::sync::atomic::{AtomicU8, Ordering};

use lottery::web::{Code, Error, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

use crate::api;
use crate::convert::Net;
use crate::dto::{ApiError, Outcome};
use crate::views::RoleName;

/// The every-code union, generated from `lottery::web::Code` (one table, no mirror).
#[wasm_bindgen(typescript_custom_section)]
const ERROR_NAMES: &'static str = lottery::web::Code::TS_DECL;

/// 0 = not started, 1 = testnet-10, 2 = simnet.
static NET: AtomicU8 = AtomicU8::new(0);

pub(crate) fn net() -> Result<Net> {
    match NET.load(Ordering::SeqCst) {
        1 => Ok(Net::Testnet10),
        #[cfg(all(feature = "simnet", not(feature = "mainnet")))]
        2 => Ok(Net::Simnet),
        _ => Err(Error::new(Code::NotStarted, "call start(network) first")),
    }
}

pub(crate) fn out<T: Serialize>(r: Result<T>) -> JsValue {
    let o = match r {
        Ok(v) => Outcome::Ok(v),
        Err(e) => Outcome::Error(ApiError::from(e)),
    };
    // Plain objects (not ES Maps), and None as `undefined`, as the generated `.d.ts` declares
    // optional fields (`T | undefined`).
    let ser = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    match o.serialize(&ser) {
        Ok(v) => v,
        Err(e) => {
            let fallback: Outcome<()> = Outcome::Error(ApiError::from(Error::new(Code::Encode, format!("result not serializable: {e}"))));
            fallback.serialize(&ser).unwrap_or_else(|_| JsValue::from_str("encode"))
        }
    }
}

pub(crate) fn parse<T: DeserializeOwned>(what: &str, v: JsValue) -> Result<T> {
    serde_wasm_bindgen::from_value(v).map_err(|e| Error::new(Code::InputFormat, format!("{what}: {e}")))
}

/// Spec 6.6, W4: a `mainnet` build throws when the module is instantiated.
#[cfg(feature = "mainnet")]
#[wasm_bindgen(start)]
pub fn refuse_mainnet() -> std::result::Result<(), JsValue> {
    Err(out::<()>(api::start("mainnet").map(|_| ())))
}

/// Start the lottery code on `network` ("testnet-10"; "simnet" in a `simnet` build). Every
/// other call needs it. A `mainnet` build refuses (`mainnet_refused`).
#[wasm_bindgen(js_name = start, unchecked_return_type = "Outcome<Info>")]
pub fn start(network: &str) -> JsValue {
    out(api::start(network).map(|(n, info)| {
        let code = match n {
            Net::Testnet10 => 1,
            #[cfg(all(feature = "simnet", not(feature = "mainnet")))]
            Net::Simnet => 2,
        };
        NET.store(code, Ordering::SeqCst);
        info
    }))
}

// ------------------------------------------------------------------------------ decode, verify

/// The state of `role` from its encoded state bytes (hex).
#[wasm_bindgen(js_name = decodeState, unchecked_return_type = "Outcome<StateView>")]
pub fn decode_state(#[wasm_bindgen(unchecked_param_type = "RoleName")] role: JsValue, state: &str) -> JsValue {
    out(net().and_then(|n| api::decode_state(n, parse::<RoleName>("role", role)?, state)))
}

/// The state of a full redeem script (hex), by the embedded template it carries.
#[wasm_bindgen(js_name = decodeRedeem, unchecked_return_type = "Outcome<StateView>")]
pub fn decode_redeem(redeem: &str) -> JsValue {
    out(net().and_then(|n| api::decode_redeem(n, redeem)))
}

/// Spec 6.3: the indexer's state bytes against a UTXO entry from a public node (its script
/// and the instance; the entry's liveness is the page's: read it by outpoint).
#[wasm_bindgen(js_name = verifyState, unchecked_return_type = "Outcome<StateView>")]
#[allow(non_snake_case)]
pub fn verify_state(covenantId: &str, #[wasm_bindgen(unchecked_param_type = "RoleName")] role: JsValue, #[wasm_bindgen(unchecked_param_type = "CovUtxo")] object: JsValue) -> JsValue {
    out(net().and_then(|n| api::verify_state(n, covenantId, parse("role", role)?, &parse("object", object)?)))
}

/// Spec 6.3, J6: the shard trees rebuilt from their purchases against a marker's or pool's
/// frozen roots (its entry from a public node by outpoint).
#[wasm_bindgen(js_name = checkRoots, unchecked_return_type = "Outcome<RootsOut>")]
pub fn check_roots(#[wasm_bindgen(unchecked_param_type = "RootsInput")] input: JsValue) -> JsValue {
    out(net().and_then(|_| api::check_roots(&parse("input", input)?)))
}

/// Spec J6: the winning numbers from the round's anchor proof: `final` after the draw (the
/// pool's), `provisional` before it (X bound to the anchor block a public node reports).
#[wasm_bindgen(js_name = winningNumbers, unchecked_return_type = "Outcome<WinningOut>")]
pub fn winning_numbers(#[wasm_bindgen(unchecked_param_type = "WinningInput")] input: JsValue) -> JsValue {
    out(net().and_then(|_| api::winning_numbers(&parse("input", input)?)))
}

// ------------------------------------------------------------------------------ estimate

/// The fee (sompi) of a priority mass at a rate (sompi/gram).
#[wasm_bindgen(js_name = feeFor, unchecked_return_type = "Outcome<string>")]
pub fn fee_for(#[wasm_bindgen(unchecked_param_type = "FeeInput")] input: JsValue) -> JsValue {
    out(net().and_then(|_| api::fee_for(&parse("input", input)?)))
}

/// The withdrawal minimum (shares) of the vault (spec S5, D86).
#[wasm_bindgen(js_name = wMin, unchecked_return_type = "Outcome<string>")]
#[allow(non_snake_case)]
pub fn w_min(covenantId: &str, #[wasm_bindgen(unchecked_param_type = "CovUtxo")] vault: JsValue) -> JsValue {
    out(net().and_then(|_| api::w_min(covenantId, &parse("vault", vault)?)))
}

// ------------------------------------------------------------------------------ build

macro_rules! builder {
    ($js:literal, $f:ident, $api:ident, $ty:literal, $doc:tt) => {
        #[doc = $doc]
        #[wasm_bindgen(js_name = $js, unchecked_return_type = "Outcome<BuiltTx>")]
        pub fn $f(#[wasm_bindgen(unchecked_param_type = $ty)] input: JsValue) -> JsValue {
            out(net().and_then(|n| api::$api(n, &parse("input", input)?)))
        }
    };
}

builder!("buildBuy", build_buy, build_buy, "BuyInput", "`buy()`: unsigned, with the inputs to sign, the masses, the fee and the figures.");
builder!("buildClaim", build_claim, build_claim, "ClaimInput", "`claim()` of one winning combination.");
builder!("buildReturnDeposit", build_return_deposit, build_return_deposit, "ReturnInput", "`return_deposit()`.");
builder!("buildClose", build_close, build_close, "CloseInput", "`close()`.");
builder!("buildDraw", build_draw, build_draw, "DrawInput", "`draw()`, or the void when `anchorProof` is null.");
builder!("buildRequestDeposit", build_request_deposit, build_request_deposit, "DepositInput", "`request_deposit()`.");
builder!("buildRequestWithdraw", build_request_withdraw, build_request_withdraw, "WithdrawInput", "`request_withdraw()`.");
builder!("buildSettle", build_settle, build_settle, "SettleInput", "`settle()`.");

// ------------------------------------------------------------------------------ assemble

/// Spec 6.2: the post-sign check on the wallet's returned Safe JSON, then the final
/// transaction to submit.
#[wasm_bindgen(js_name = assemble, unchecked_return_type = "Outcome<Signed>")]
pub fn assemble(#[wasm_bindgen(unchecked_param_type = "AssembleInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| api::assemble(n, &parse("input", input)?)))
}

/// The address of a script public key (Safe JSON form) on the started network.
#[wasm_bindgen(js_name = addressOf, unchecked_return_type = "Outcome<string>")]
#[allow(non_snake_case)]
pub fn address_of(scriptPublicKey: &str) -> JsValue {
    out(net().and_then(|n| api::address_of(n, scriptPublicKey)))
}

/// The script public key (Safe JSON form) of an address of the started network.
#[wasm_bindgen(js_name = scriptOf, unchecked_return_type = "Outcome<string>")]
pub fn script_of(address: &str) -> JsValue {
    out(net().and_then(|n| api::script_of(n, address)))
}

/// Spec S1, 7: the cost panel's rows of a built `buy()`, each checked against its transaction.
#[wasm_bindgen(js_name = buyFigures, unchecked_return_type = "Outcome<BuyFigures>")]
pub fn buy_figures(#[wasm_bindgen(unchecked_param_type = "BuyFiguresInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| api::buy_figures(n, &parse("input", input)?)))
}

// ------------------------------------------------------------------------------ my tickets (F4)

/// Spec S2: a ticket card's figures (tiers, matches, each claim's prize, h and mode, when the
/// deposit comes back, whether `return_deposit()` is valid now).
#[wasm_bindgen(js_name = ticketCard, unchecked_return_type = "Outcome<TicketCard>")]
pub fn ticket_card(#[wasm_bindgen(unchecked_param_type = "TicketCardInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::tickets::ticket_card(n, &parse("input", input)?)))
}

/// Spec S2, 7: the claim row's figures of a built `claim()`, each checked against its transaction.
#[wasm_bindgen(js_name = claimFigures, unchecked_return_type = "Outcome<ClaimFigures>")]
pub fn claim_figures(#[wasm_bindgen(unchecked_param_type = "ClaimFiguresInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::tickets::claim_figures(n, &parse("input", input)?)))
}

/// Spec S2, 7: a built `return_deposit()`'s figures, each checked against its transaction.
#[wasm_bindgen(js_name = returnFigures, unchecked_return_type = "Outcome<ReturnFigures>")]
pub fn return_figures(#[wasm_bindgen(unchecked_param_type = "ReturnFiguresInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::tickets::return_figures(n, &parse("input", input)?)))
}

/// Spec S8: the backup file of the store's records (its text, and the records left out).
#[wasm_bindgen(js_name = backupWrite, unchecked_return_type = "Outcome<BackupWritten>")]
pub fn backup_write(#[wasm_bindgen(unchecked_param_type = "BackupWriteInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::backup::backup_write(n, &parse("input", input)?)))
}

/// Spec S8: a backup file read back, every record checked (`backup_invalid` otherwise).
#[wasm_bindgen(js_name = backupRead, unchecked_return_type = "Outcome<BackupContents>")]
pub fn backup_read(#[wasm_bindgen(unchecked_param_type = "BackupReadInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::backup::backup_read(n, &parse("input", input)?)))
}

/// Spec 6.3, S2: a kept round copy against the node's live entries at its objects' scripts.
#[wasm_bindgen(js_name = checkCopy, unchecked_return_type = "Outcome<CopyCheck>")]
pub fn check_copy(#[wasm_bindgen(unchecked_param_type = "CopyCheckInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::backup::check_copy(n, &parse("input", input)?)))
}

/// Spec 6.3 (F4 review M2): the round copy to keep from a newly checked one and the stored one
/// (never a stage back, never fewer pools; with the marker kept, every pool of the draw).
#[wasm_bindgen(js_name = mergeCopy, unchecked_return_type = "Outcome<MergedCopy>")]
pub fn merge_copy(#[wasm_bindgen(unchecked_param_type = "MergeCopyInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::backup::merge_copy(n, &parse("input", input)?)))
}

/// The local store's format (spec 6.3, `webstore/2`): its name, version and object stores.
#[wasm_bindgen(js_name = webstoreInfo, unchecked_return_type = "WebstoreInfo")]
pub fn webstore_info() -> JsValue {
    let ser = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    crate::webstore::info().serialize(&ser).unwrap_or(JsValue::NULL)
}

// ------------------------------------------------------------------------------ indexer API (F2)

/// The size caps of the indexer API's responses (bytes; the server enforces the same).
#[wasm_bindgen(js_name = indexerLimits, unchecked_return_type = "Limits")]
pub fn indexer_limits() -> JsValue {
    let ser = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    crate::indexer::LIMITS.serialize(&ser).unwrap_or(JsValue::NULL)
}

macro_rules! parse_indexer {
    ($js:literal, $f:ident, $parse:ident, $ty:literal, $doc:tt) => {
        #[doc = $doc]
        #[wasm_bindgen(js_name = $js, unchecked_return_type = $ty)]
        pub fn $f(body: &str) -> JsValue {
            out(net().and_then(|_| crate::indexer::$parse(body)))
        }
    };
}

parse_indexer!("parseIndexerStatus", parse_indexer_status, parse_status, "Outcome<ApiStatus>", "`GET /v1/status`, from its body text (size-capped first).");
parse_indexer!("parseIndexerRounds", parse_indexer_rounds, parse_rounds, "Outcome<ApiRounds>", "`GET /v1/rounds`.");
parse_indexer!("parseIndexerRound", parse_indexer_round, parse_round, "Outcome<ApiRound>", "`GET /v1/rounds/{r}`.");
parse_indexer!("parseIndexerShard", parse_indexer_shard, parse_shard, "Outcome<ApiShard>", "`GET /v1/rounds/{r}/shards/{i}`.");
parse_indexer!("parseIndexerPurchases", parse_indexer_purchases, parse_purchases, "Outcome<ApiPurchases>", "`GET /v1/rounds/{r}/purchases`.");
parse_indexer!("parseIndexerAnchorProof", parse_indexer_anchor_proof, parse_anchor_proof, "Outcome<ApiAnchorProof>", "`GET /v1/rounds/{r}/anchor-proof`.");
parse_indexer!("parseIndexerTickets", parse_indexer_tickets, parse_tickets, "Outcome<ApiTickets>", "`GET /v1/tickets?payout=`.");
parse_indexer!("parseIndexerObjects", parse_indexer_objects, parse_objects, "Outcome<ApiObjects>", "`GET /v1/vault`, `/v1/positions?owner=`, `/v1/requests?owner=`.");
parse_indexer!("parseIndexerError", parse_indexer_error, parse_error, "Outcome<ApiErrorBody>", "An error response body.");

/// Spec 6.3: the launch bundle the API serves, verified locally for this page's instance.
#[wasm_bindgen(js_name = checkIndexerInstance, unchecked_return_type = "Outcome<CheckedInstance>")]
pub fn check_indexer_instance(#[wasm_bindgen(unchecked_param_type = "InstanceCheckInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::indexer::check_instance(n, &parse("input", input)?)))
}

/// Spec 6.3: API objects against the node's entries for their outpoints.
#[wasm_bindgen(js_name = checkIndexerObjects, unchecked_return_type = "Outcome<CheckedObject[]>")]
pub fn check_indexer_objects(#[wasm_bindgen(unchecked_param_type = "ObjectsCheckInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::indexer::check_objects(n, &parse("input", input)?)))
}

/// Spec 6.3: a round (objects, status, roots, purchases, winning numbers, anchor proof)
/// against the chain.
#[wasm_bindgen(js_name = checkIndexerRound, unchecked_return_type = "Outcome<CheckedRound>")]
pub fn check_indexer_round(#[wasm_bindgen(unchecked_param_type = "RoundCheckInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::indexer::check_round(n, &parse("input", input)?)))
}

// ------------------------------------------------------------------------------ rounds (F5)

/// Spec J6: every check of a round on its own (each object's state, the status, the frozen roots,
/// each shard's tree, the winning numbers, the anchor block), each match, mismatch or not checkable.
#[wasm_bindgen(js_name = verifyRound, unchecked_return_type = "Outcome<RoundVerify>")]
pub fn verify_round(#[wasm_bindgen(unchecked_param_type = "RoundCheckInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::rounds::verify_round(n, &parse("input", input)?)))
}

/// Spec S4, 7: the caller panel's rows of a built `close()`, `draw()` or void, each checked against
/// its transaction.
#[wasm_bindgen(js_name = callerFigures, unchecked_return_type = "Outcome<CallerFigures>")]
pub fn caller_figures(#[wasm_bindgen(unchecked_param_type = "CallerFiguresInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::rounds::caller_figures(n, &parse("input", input)?)))
}

/// Spec S4, design 7.3 (F5 review S1): the expired pools a `draw()` or void absorbs, from the
/// indexer's candidates checked against the node's entries for them, oldest first, at most A_MAX.
#[wasm_bindgen(js_name = absorbablePools, unchecked_return_type = "Outcome<Absorbable>")]
pub fn absorbable_pools(#[wasm_bindgen(unchecked_param_type = "AbsorbInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::rounds::absorbable_pools(n, &parse("input", input)?)))
}

/// X of an anchor proof (its hash and DAA score), to fetch the anchor block from a public node.
#[wasm_bindgen(js_name = anchorProofX, unchecked_return_type = "Outcome<AnchorX>")]
pub fn anchor_proof_x(proof: &str) -> JsValue {
    out(net().and_then(|_| crate::rounds::anchor_x(proof)))
}

// ------------------------------------------------------------------------------ vault (F6)

/// Spec S5: the vault's figures and the connected wallet's positions and requests (with what each
/// settlement does), from node-checked chain objects.
#[wasm_bindgen(js_name = vaultView, unchecked_return_type = "Outcome<VaultPage>")]
pub fn vault_view(#[wasm_bindgen(unchecked_param_type = "VaultViewInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::vault::vault_view(n, &parse("input", input)?)))
}

/// Spec S5 (D86, W4b): the shares a withdrawal of an amount (or of the whole position) takes, or
/// `vault_amount` when the vault does not take it.
#[wasm_bindgen(js_name = withdrawChoice, unchecked_return_type = "Outcome<WithdrawChoice>")]
pub fn withdraw_choice(#[wasm_bindgen(unchecked_param_type = "WithdrawChoiceInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::vault::withdraw_choice(n, &parse("input", input)?)))
}

/// Spec S5, 7: a built `request_deposit()`'s rows, each checked against its transaction.
#[wasm_bindgen(js_name = depositFigures, unchecked_return_type = "Outcome<DepositFigures>")]
pub fn deposit_figures(#[wasm_bindgen(unchecked_param_type = "DepositFiguresInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::vault::deposit_figures(n, &parse("input", input)?)))
}

/// Spec S5, 7: a built `request_withdraw()`'s rows, each checked against its transaction.
#[wasm_bindgen(js_name = withdrawFigures, unchecked_return_type = "Outcome<WithdrawFigures>")]
pub fn withdraw_figures(#[wasm_bindgen(unchecked_param_type = "WithdrawFiguresInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::vault::withdraw_figures(n, &parse("input", input)?)))
}

/// Spec S5, 7: a built `settle()`'s rows, each checked against its transaction.
#[wasm_bindgen(js_name = settleFigures, unchecked_return_type = "Outcome<SettleFigures>")]
pub fn settle_figures(#[wasm_bindgen(unchecked_param_type = "SettleFiguresInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::vault::settle_figures(n, &parse("input", input)?)))
}

// ------------------------------------------------------------------------------ page tables (F3)

/// Every error code with its one message (the one table, `lottery::web::Code`). Needs no
/// started session: the page loads it first, so the codes it raises itself (a stopped worker,
/// an unreachable indexer) read the same way as the wasm's.
#[wasm_bindgen(js_name = errorTable, unchecked_return_type = "ErrorEntry[]")]
pub fn error_table() -> JsValue {
    let ser = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    crate::node::error_table().serialize(&ser).unwrap_or(JsValue::NULL)
}

/// The game's fixed shape (5 of 1..30, B_MAX, F_MAX) for the picker and the coin choice.
#[wasm_bindgen(js_name = gameShape, unchecked_return_type = "GameShape")]
pub fn game_shape() -> JsValue {
    let ser = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    crate::node::game_shape().serialize(&ser).unwrap_or(JsValue::NULL)
}

/// The jackpot odds at bonus range m: "1 in" this many (C = N x m, design 4.1).
#[wasm_bindgen(js_name = jackpotOdds, unchecked_return_type = "Outcome<Dec>")]
pub fn jackpot_odds(m: u32) -> JsValue {
    out(net().and_then(|_| crate::node::jackpot_odds(m)))
}

// ------------------------------------------------------------------------------ dev only

/// The names of the library's fixture calls (feature `fixtures`: dev builds only, never the
/// release artifact). W4's positive control: it links the fixtures, so the symbol scan that
/// finds none in the release artifact is shown to find them when they are there.
#[cfg(feature = "fixtures")]
#[wasm_bindgen(js_name = devFixtureNames, unchecked_return_type = "Outcome<string[]>")]
pub fn dev_fixture_names() -> JsValue {
    out(net().and_then(|_| Ok(lottery::web::fixtures::calls()?.into_iter().map(|(n, _, _)| n).collect::<Vec<_>>())))
}

// ------------------------------------------------------------------------------ quick wallet

/// A new quick-testnet-wallet key (absent from a `mainnet` build).
#[cfg(not(feature = "mainnet"))]
#[wasm_bindgen(js_name = quickWalletNew, unchecked_return_type = "Outcome<QuickKey>")]
pub fn quick_wallet_new() -> JsValue {
    out(net().and_then(crate::quick_wallet::quick_wallet_new))
}

/// The quick wallet's key from its exported secret key.
#[cfg(not(feature = "mainnet"))]
#[wasm_bindgen(js_name = quickWalletImport, unchecked_return_type = "Outcome<QuickKey>")]
#[allow(non_snake_case)]
pub fn quick_wallet_import(secretKey: &str) -> JsValue {
    out(net().and_then(|n| crate::quick_wallet::quick_wallet_import(n, secretKey)))
}

/// Sign the listed inputs this key owns; returns the signed Safe JSON for `assemble`.
#[cfg(not(feature = "mainnet"))]
#[wasm_bindgen(js_name = quickWalletSign, unchecked_return_type = "Outcome<string>")]
pub fn quick_wallet_sign(#[wasm_bindgen(unchecked_param_type = "QuickSignInput")] input: JsValue) -> JsValue {
    out(net().and_then(|n| crate::quick_wallet::quick_wallet_sign(n, &parse("input", input)?)))
}
