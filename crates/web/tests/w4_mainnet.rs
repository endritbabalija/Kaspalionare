//! W4 (spec frontend 6.6, 8): a `mainnet` build of `crates/web` has no quick-wallet symbol
//! and refuses to start. The same pattern as SP12 (`lottery/tests/m7_seed.rs`): real builds,
//! a positive control that proves the scan sees the symbols, and the absence check.
//!
//! Command (from `crates/`, about 3-5 minutes; needs the CC/AR variables of CLAUDE.md "wasm
//! build" or LLVM at `C:\Program Files\LLVM\bin`, and `wasm-bindgen` 0.2.100 and `node` on
//! PATH): `cargo test -j 2 -p web --release --test w4_mainnet -- --nocapture`
//!
//! What it does, in its own target directory (`target/w4`):
//! 1. builds the wasm32 release cdylib twice: the beta build (no feature) and `--features
//!    mainnet`;
//! 2. scans each `.wasm` file's bytes (exports, the `name` section, data, custom sections)
//!    for the quick wallet's names: the exports `quickWalletNew`/`quickWalletImport`/
//!    `quickWalletSign`, the Rust path `quick_wallet` (in every mangled symbol of the module),
//!    and the marker `KASPALIONARE_QUICK_WALLET`. The beta build must contain every one (the
//!    scan works), the mainnet build none;
//! 3. runs each through `wasm-bindgen --target nodejs` and Node: the beta module loads and
//!    starts on testnet-10 (and refuses "mainnet" as a network); loading the mainnet module
//!    throws `mainnet_refused` (the `#[wasm_bindgen(start)]` refusal), before any call.

use std::path::{Path, PathBuf};
use std::process::Command;

const NAMES: [&str; 5] = ["quickWalletNew", "quickWalletImport", "quickWalletSign", "quick_wallet", "KASPALIONARE_QUICK_WALLET"];

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn tool_ok(name: &str, arg: &str) -> bool {
    Command::new(name).arg(arg).output().is_ok_and(|o| o.status.success())
}

/// Build the wasm32 release cdylib with `features` into `target/w4`; its path.
fn build(features: &[&str]) -> PathBuf {
    let dir = crates_dir();
    let target = dir.join("target").join("w4");
    let mut args = vec!["build", "-j", "2", "-p", "web", "--target", "wasm32-unknown-unknown", "--release", "--target-dir"];
    let t = target.to_string_lossy().to_string();
    args.push(&t);
    let f = features.join(",");
    if !features.is_empty() {
        args.extend(["--features", &f]);
    }
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(&args).current_dir(&dir);
    for (var, exe) in [("CC_wasm32_unknown_unknown", "clang.exe"), ("AR_wasm32_unknown_unknown", "llvm-ar.exe")] {
        if std::env::var_os(var).is_none() {
            let p = Path::new(r"C:\Program Files\LLVM\bin").join(exe);
            assert!(p.exists(), "set {var} (CLAUDE.md, wasm build): no {}", p.display());
            cmd.env(var, p);
        }
    }
    let o = cmd.output().expect("run cargo");
    assert!(o.status.success(), "cargo {args:?} failed:\n{}", String::from_utf8_lossy(&o.stderr));
    let wasm = target.join("wasm32-unknown-unknown").join("release").join("web.wasm");
    // Keep a copy per variant (the second build overwrites web.wasm).
    let copy = target.join(format!("web-{}.wasm", if features.is_empty() { "beta" } else { "mainnet" }));
    std::fs::copy(&wasm, &copy).unwrap();
    copy
}

fn count(hay: &[u8], needle: &str) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle.as_bytes()).count()
}

/// wasm-bindgen --target nodejs, then `node -e script`; (status ok, stdout + stderr).
fn node(wasm: &Path, script: &str) -> (bool, String) {
    let out = wasm.with_extension("pkg");
    let _ = std::fs::remove_dir_all(&out);
    let o = Command::new("wasm-bindgen").args(["--target", "nodejs", "--out-dir"]).arg(&out).arg(wasm).output().expect("run wasm-bindgen");
    assert!(o.status.success(), "wasm-bindgen failed: {}", String::from_utf8_lossy(&o.stderr));
    let js = out.join(wasm.file_stem().unwrap()).with_extension("js");
    let script = script.replace("$JS", &js.to_string_lossy().replace('\\', "/"));
    let o = Command::new("node").args(["-e", &script]).output().expect("run node");
    (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}

#[test]
fn w4_mainnet_build_has_no_quick_wallet_and_refuses_to_start() {
    assert!(tool_ok("wasm-bindgen", "--version"), "wasm-bindgen 0.2.100 must be on PATH: cargo install wasm-bindgen-cli --version 0.2.100 --locked");
    assert!(tool_ok("node", "--version"), "node must be on PATH");
    let beta = build(&[]);
    let main = build(&["mainnet"]);
    let (b, m) = (std::fs::read(&beta).unwrap(), std::fs::read(&main).unwrap());
    println!("beta build {} B, mainnet build {} B", b.len(), m.len());
    for n in NAMES {
        let (cb, cm) = (count(&b, n), count(&m, n));
        println!("{n:28} beta {cb:4}  mainnet {cm:4}");
        assert!(cb > 0, "positive control: the beta build lacks `{n}`, so the scan proves nothing");
        assert_eq!(cm, 0, "the mainnet build contains `{n}`");
    }
    // The beta build starts on testnet-10 and refuses mainnet as a network.
    let (ok, log) = node(
        &beta,
        r#"const m = require("$JS");
           const a = m.start("testnet-10"); if (!a.ok || a.ok.network !== "testnet-10") throw new Error("start: " + JSON.stringify(a));
           const b = m.start("mainnet"); if (!b.error || b.error.code !== "address_network") throw new Error("mainnet network: " + JSON.stringify(b));
           if (typeof m.quickWalletNew !== "function") throw new Error("no quick wallet in the beta build");
           console.log("beta: started on", a.ok.network, "; start(mainnet) refused:", b.error.code);"#,
    );
    println!("{}", log.trim());
    assert!(ok, "the beta module did not start: {log}");
    // The mainnet build throws at load, with mainnet_refused.
    let (ok, log) = node(
        &main,
        r#"try { require("$JS"); console.log("LOADED"); process.exit(3); }
           catch (e) { const s = JSON.stringify(e); if (!s.includes("mainnet_refused")) { console.log("other error", s, String(e)); process.exit(4); }
                       console.log("mainnet: load refused:", e.error.code, "-", e.error.message); }"#,
    );
    println!("{}", log.trim());
    assert!(ok && log.contains("mainnet: load refused: mainnet_refused"), "the mainnet module did not refuse to start: {log}");
}
