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

#![cfg(target_arch = "wasm32")]

mod common;

use common::{GOLDEN, Input, digest_line, input};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use web::bindings;
use web::dto::{BuiltTx, Info, Outcome};

fn to_js<T: serde::Serialize>(v: &T) -> JsValue {
    v.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).unwrap()
}

fn ok<T: serde::de::DeserializeOwned>(what: &str, v: JsValue) -> T {
    match serde_wasm_bindgen::from_value::<Outcome<T>>(v).unwrap_or_else(|e| panic!("{what}: not an Outcome: {e}")) {
        Outcome::Ok(x) => x,
        Outcome::Error(e) => panic!("{what}: {} ({}): {}", e.code, e.number, e.detail),
    }
}

#[wasm_bindgen_test]
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

#[wasm_bindgen_test]
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
