//! `crates/web`, F1 part A: the link probe of the browser build (spec frontend 6.1).
//!
//! `f1_link_probe` builds every fixture transaction shape of `lottery::digest` (buy, close,
//! draw and void, claim, return, refresh, set_params, the vault requests and settlements,
//! genesis), runs each input in the script engine, and digests the ABI encodings of every
//! template. That keeps the builders, the engine, the mass calculator and the ABI codec in
//! the wasm binary, so linking it proves one rusty-kaspa version (no duplicate symbols) and
//! its size is the size of the builders without the compiler. Part B replaces this with the
//! wasm-bindgen API.

/// The number of digest lines (`lottery::digest::all_lines`).
#[unsafe(no_mangle)]
pub extern "C" fn f1_link_probe() -> u32 {
    lottery::digest::all_lines().len() as u32
}
