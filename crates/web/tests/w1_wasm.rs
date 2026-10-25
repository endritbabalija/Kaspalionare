//! W1 (spec frontend 8): the wasm build and the native build produce byte-identical
//! transactions and masses, for every transaction shape in the M1–M8 fixtures.
//!
//! Runs in wasm32 under Node (`wasm-bindgen-test-runner`, set as the wasm32 runner in
//! `crates/.cargo/config.toml`). Command (from `crates/`, with the CC/AR variables of
//! CLAUDE.md "wasm build"):
//! `cargo test -j 2 -p web --target wasm32-unknown-unknown --release --test w1_wasm`
//!
//! - `w1_library_digests_equal_native`: `lottery::digest::all_lines()` (every template's ABI
//!   output, and the 21 fixture transactions of M1–M8 with their entries, script units,
//!   masses and storage-mass commits) computed in wasm, against the golden file written
//!   natively before F1 (`lottery/tests/fixtures/f1_digests.txt`, the file `f1_bytes`
//!   compares natively).
//! - `w1_api_through_the_js_boundary`: every web fixture call as a JS object through the
//!   real exports (`buildBuy` … `buildSettle`), the `BuiltTx` read back from JS, digested,
//!   against `tests/fixtures/w1_api.txt`, which `tests/api.rs` compares natively.
//! - `w1_verify_through_the_js_boundary` (F1 review): every decode and verify call of
//!   `common::verify_calls` (decodeState, decodeRedeem, verifyState, checkRoots,
//!   winningNumbers; accepted and refused) against `tests/fixtures/w1_verify.txt`, which
//!   `tests/api.rs` compares natively.
//! - `w1_sign_and_assemble_in_wasm` (F1 review; 14 of the 18 shapes since the re-review):
//!   quickWalletSign and assemble in wasm (Schnorr signing and verification), with a flipped
//!   signature and a changed figure.
//!
//! The four parts run in this order in one test, `w1_wasm_equals_native`: the session is
//! module state (`start`), so the `not_started` check must come before any part starts it,
//! and wasm-bindgen-test does not order separate tests.

#![cfg(target_arch = "wasm32")]

mod common;

use common::{GOLDEN, GOLDEN_VERIFY, Input, Verify, digest_line, input, text_line, verify_calls};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use web::bindings;
use web::convert::hex;
use web::dto::{AssembleInput, BuiltTx, Info, Outcome, QuickSignInput, RootsOut, Signed, WinningOut};
use web::views::StateView;

fn to_js<T: serde::Serialize>(v: &T) -> JsValue {
    v.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).unwrap()
}

fn ok<T: serde::de::DeserializeOwned>(what: &str, v: JsValue) -> T {
    match serde_wasm_bindgen::from_value::<Outcome<T>>(v).unwrap_or_else(|e| panic!("{what}: not an Outcome: {e}")) {
        Outcome::Ok(x) => x,
        Outcome::Error(e) => panic!("{what}: {} ({}): {}", e.code, e.number, e.detail),
    }
}

fn w1_library_digests_equal_native() {
    let want: Vec<&str> = include_str!("../../lottery/tests/fixtures/f1_digests.txt").lines().collect();
    let got = lottery::digest::all_lines();
    assert_eq!(got.len(), want.len(), "line count");
    let mut differ = vec![];
    for (g, w) in got.iter().zip(&want) {
        if g != w {
            differ.push(format!("wasm:   {g}\nnative: {w}"));
        }
    }
    assert!(differ.is_empty(), "{} of {} lines differ:\n{}", differ.len(), want.len(), differ.join("\n"));
    console_log!("W1 library: {} digest lines equal the native golden file", got.len());
}

fn call(i: &Input) -> JsValue {
    let v = match i {
        Input::Buy(x) => to_js(x),
        Input::Claim(x) => to_js(x),
        Input::Return(x) => to_js(x),
        Input::Close(x) => to_js(x),
        Input::Draw(x) => to_js(x),
        Input::Deposit(x) => to_js(x),
        Input::Withdraw(x) => to_js(x),
        Input::Settle(x) => to_js(x),
    };
    match i {
        Input::Buy(_) => bindings::build_buy(v),
        Input::Claim(_) => bindings::build_claim(v),
        Input::Return(_) => bindings::build_return_deposit(v),
        Input::Close(_) => bindings::build_close(v),
        Input::Draw(_) => bindings::build_draw(v),
        Input::Deposit(_) => bindings::build_request_deposit(v),
        Input::Withdraw(_) => bindings::build_request_withdraw(v),
        Input::Settle(_) => bindings::build_settle(v),
    }
}

fn w1_api_through_the_js_boundary() {
    let calls = lottery::web::fixtures::calls().unwrap();
    // Before start: every call is refused with not_started.
    let first = input(&calls[0].1);
    match serde_wasm_bindgen::from_value::<Outcome<BuiltTx>>(call(&first)).unwrap() {
        Outcome::Error(e) => assert_eq!(e.code, "not_started"),
        Outcome::Ok(_) => panic!("built before start"),
    }
    let info: Info = ok("start", bindings::start("testnet-10"));
    assert_eq!(info.network, "testnet-10");
    let want: Vec<&str> = GOLDEN.lines().collect();
    assert_eq!(want.len(), calls.len(), "golden line count");
    for ((name, c, _), w) in calls.iter().zip(&want) {
        let i = input(c);
        let b: BuiltTx = ok(name, call(&i));
        assert_eq!(&digest_line(name, &b), w, "{name} ({}): the wasm BuiltTx differs from the native one", i.export());
    }
    console_log!("W1 API: {} builds through the JS boundary equal the native golden file", calls.len());
}

fn typed<T: serde::Serialize + serde::de::DeserializeOwned>(v: JsValue) -> String {
    let o: Outcome<T> = serde_wasm_bindgen::from_value(v).expect("an Outcome");
    serde_json::to_string(&o).unwrap()
}

/// W1's verify half (F1 review, item 4): every call of `common::verify_calls` (decodeState,
/// decodeRedeem, verifyState on every fixture object, right and wrong; checkRoots and
/// winningNumbers on both paths, accepted and refused) through the real exports, against
/// `tests/fixtures/w1_verify.txt`, which `tests/api.rs` compares natively.
fn w1_verify_through_the_js_boundary() {
    let _ = bindings::start("testnet-10");
    let want: Vec<&str> = GOLDEN_VERIFY.lines().collect();
    let calls = verify_calls();
    assert_eq!(calls.len(), want.len(), "golden line count");
    for ((name, v), w) in calls.iter().zip(&want) {
        let json = match v {
            Verify::Decode(r, h) => typed::<StateView>(bindings::decode_state(to_js(r), h)),
            Verify::Redeem(h) => typed::<StateView>(bindings::decode_redeem(h)),
            Verify::State(c, r, o) => typed::<StateView>(bindings::verify_state(c, to_js(r), to_js(o))),
            Verify::Roots(i) => typed::<RootsOut>(bindings::check_roots(to_js(i))),
            Verify::Winning(i) => typed::<WinningOut>(bindings::winning_numbers(to_js(i))),
        };
        assert_eq!(&text_line(name, &json), w, "{name}: the wasm result differs from the native one: {}", &json[..json.len().min(200)]);
    }
    console_log!("W1 verify: {} decode and verify calls through the JS boundary equal the native golden file", calls.len());
}

/// Signing and the post-sign check in wasm (F1 review, item 4; the reviewer's
/// `review_f1_wasm_assemble.rs` folded in; F1 re-review N4: every shape the quick wallet
/// can sign, 14 of 18), `quickWalletSign` signs (Schnorr signing in wasm) and `assemble` accepts
/// (Schnorr verification in the wasm engine); the assembled transaction is the built one
/// with only the listed signature scripts; a flipped signature byte is `signature_invalid`;
/// a changed figure is `built_mismatch`.
fn w1_sign_and_assemble_in_wasm() {
    let _ = bindings::start("testnet-10");
    let mut n = 0;
    // F1 re-review N4: every shape the quick wallet can sign (14 of the 18 fixture calls);
    // the other 4 are the ECDSA and P2SH owners' withdrawals, which it cannot sign
    // (`signed_missing`, asserted natively in `api.rs`).
    for name in [
        "buy b=1 ref",
        "buy b=4",
        "claim k=4",
        "return after",
        "return with pool",
        "close k=4",
        "close k=16",
        "draw k=4",
        "void k=4",
        "request_deposit",
        "request_withdraw 0 partial",
        "request_withdraw 0 whole",
        "settle deposit",
        "settle withdrawal",
    ] {
        let (_, c, _) = lottery::web::fixtures::calls().unwrap().into_iter().find(|(x, _, _)| x == name).unwrap();
        let i = input(&c);
        let built: BuiltTx = ok(name, call(&i));
        let sk = if name.starts_with("request_withdraw") { hex(&lottery::vault::OWNER_SK) } else { hex(&lottery::web::fixtures::WALLET_SK) };
        let signed: String = ok(&format!("{name}: quickWalletSign"), bindings::quick_wallet_sign(to_js(&QuickSignInput { built: built.clone(), secret_key: sk.clone() })));
        let s: Signed = ok(&format!("{name}: assemble"), bindings::assemble(to_js(&AssembleInput { built: built.clone(), signed: signed.clone() })));
        assert_eq!(s.id, built.id, "{name}: id");
        // The assembled transaction is ours: blank the listed inputs again and compare.
        let (got, _, _) = web::safe_json::from_json(&s.transaction).unwrap();
        let (ours, _, _) = web::safe_json::from_json(&built.transaction).unwrap();
        let idx: Vec<usize> = built.sign.iter().map(|x| x.index as usize).collect();
        assert_eq!(lottery::web::sign::blanked(&got, &idx).unwrap(), ours, "{name}: assembled = built + listed signature scripts");
        assert!(idx.iter().all(|&k| got.inputs[k].signature_script.len() >= 66), "{name}: signed");
        // A flipped signature byte fails verification in wasm.
        let (mut t, e, _) = web::safe_json::from_json(&signed).unwrap();
        t.inputs[idx[0]].signature_script[10] ^= 1;
        let entries: Vec<_> = e.into_iter().map(Option::unwrap).collect();
        let bad = web::safe_json::reserialize(&t, &entries, kaspa_addresses::Prefix::Testnet).unwrap();
        match serde_wasm_bindgen::from_value::<Outcome<Signed>>(bindings::assemble(to_js(&AssembleInput { built: built.clone(), signed: bad }))).unwrap() {
            Outcome::Ok(_) => panic!("{name}: a bad signature assembled in wasm"),
            Outcome::Error(e) => assert_eq!(e.code, "signature_invalid", "{name}: {}", e.detail),
        }
        // A changed figure is refused by both calls in wasm.
        let mut lie = built.clone();
        lie.summary.wallet_back = "1".into();
        for r in [bindings::assemble(to_js(&AssembleInput { built: lie.clone(), signed })), bindings::quick_wallet_sign(to_js(&QuickSignInput { built: lie, secret_key: sk }))] {
            match serde_wasm_bindgen::from_value::<Outcome<serde_json::Value>>(r).unwrap() {
                Outcome::Ok(_) => panic!("{name}: a changed figure was accepted in wasm"),
                Outcome::Error(e) => assert_eq!(e.code, "built_mismatch", "{name}: {}", e.detail),
            }
        }
        n += 1;
    }
    console_log!("W1 sign: {n} transactions signed by the quick wallet and assembled in wasm; a flipped signature and a changed figure refused for each");
}

#[wasm_bindgen_test]
fn w1_wasm_equals_native() {
    w1_api_through_the_js_boundary();
    w1_library_digests_equal_native();
    w1_verify_through_the_js_boundary();
    w1_sign_and_assemble_in_wasm();
}
