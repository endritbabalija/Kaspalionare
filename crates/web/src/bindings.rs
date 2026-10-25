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

fn net() -> Result<Net> {
    match NET.load(Ordering::SeqCst) {
        1 => Ok(Net::Testnet10),
        #[cfg(all(feature = "simnet", not(feature = "mainnet")))]
        2 => Ok(Net::Simnet),
        _ => Err(Error::new(Code::NotStarted, "call start(network) first")),
    }
}

fn out<T: Serialize>(r: Result<T>) -> JsValue {
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

fn parse<T: DeserializeOwned>(what: &str, v: JsValue) -> Result<T> {
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

/// Spec 6.3: the indexer's state bytes against the live UTXO (its script and the instance).
#[wasm_bindgen(js_name = verifyState, unchecked_return_type = "Outcome<StateView>")]
#[allow(non_snake_case)]
pub fn verify_state(covenantId: &str, #[wasm_bindgen(unchecked_param_type = "RoleName")] role: JsValue, #[wasm_bindgen(unchecked_param_type = "CovUtxo")] object: JsValue) -> JsValue {
    out(net().and_then(|n| api::verify_state(n, covenantId, parse("role", role)?, &parse("object", object)?)))
}

/// Spec 6.3, J6: the shard trees rebuilt from their purchases against a live marker's or
/// pool's frozen roots.
#[wasm_bindgen(js_name = checkRoots, unchecked_return_type = "Outcome<RootsOut>")]
pub fn check_roots(#[wasm_bindgen(unchecked_param_type = "RootsInput")] input: JsValue) -> JsValue {
    out(net().and_then(|_| api::check_roots(&parse("input", input)?)))
}

/// Spec J6: the winning numbers from the round's anchor proof.
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

/// The withdrawal minimum (shares) of the live vault (spec S5, D86).
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
