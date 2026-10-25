//! W4 (spec frontend 6.6, 8): a `mainnet` build of `crates/web` has no quick-wallet symbol
//! and refuses to start; and (F1 review, item 6 and the fixtures note) the release artifact,
//! the one the page deploys, refuses the private simnet and carries no fixture code. The same
//! pattern as SP12 (`lottery/tests/m7_seed.rs`): real builds, a positive control that proves
//! each scan sees what it looks for, and the absence checks.
//!
//! Command (from `crates/`, about 5-8 minutes; needs the CC/AR variables of CLAUDE.md "wasm
//! build" or LLVM at `C:\Program Files\LLVM\bin`, and `wasm-bindgen` 0.2.100 and `node` on
//! PATH): `cargo test -j 2 -p web --release --test w4_mainnet -- --nocapture`
//!
//! What it builds (wasm32, release cdylib):
//! - **release**: the documented wasm build command exactly (CLAUDE.md "wasm build":
//!   `cargo build -j 2 -p web --target wasm32-unknown-unknown --release --features node`,
//!   default target directory; F3 added the page's node client, `node`), so the file checked
//!   is the release artifact itself, `target/wasm32-unknown-unknown/release/web.wasm` (copied
//!   to `target/w4/web-release.wasm`), the one `web/scripts/wasm.mjs release` packages;
//! - **dev**: `--features simnet,fixtures,node` (the local simnet dev run), in `target/w4`:
//!   the positive control of the simnet and fixture checks;
//! - **mainnet**: `--features mainnet,node`, in `target/w4`.
//!
//! What it checks:
//! 1. byte scans of each `.wasm` (exports, the `name` section, data, custom sections):
//!    - the quick wallet's names (`quickWalletNew`, `quickWalletImport`, `quickWalletSign`,
//!      the Rust path `quick_wallet`, the marker `KASPALIONARE_QUICK_WALLET`): in the release
//!      build (positive control), none in the mainnet build;
//!    - the fixture code's names (the module paths `lottery::web::fixtures` and
//!      `lottery::digest::lines`, mangled or not, and the export `devFixtureNames`): in the
//!      dev build (positive control, for the fixtures path and the export), none in the
//!      release or mainnet build; and the bytes of the fixtures' wallet key (32 x 0x21)
//!      likewise;
//! 2. each build run through `wasm-bindgen --target nodejs` and Node:
//!    - release: starts on testnet-10; `start("simnet")` and `start("mainnet")` are
//!      `address_network`; once started, a `kaspasim:` address in `scriptOf` and as a
//!      builder's change address is `address_network`; the quick wallet is there and
//!      `devFixtureNames` is not;
//!    - dev: starts on the simnet with `kaspasim` addresses, and `devFixtureNames` lists the
//!      fixture calls;
//!    - mainnet: loading it throws `mainnet_refused` (the `#[wasm_bindgen(start)]` refusal),
//!      before any call.

use std::path::{Path, PathBuf};
use std::process::Command;

const QUICK: [&str; 5] = ["quickWalletNew", "quickWalletImport", "quickWalletSign", "quick_wallet", "KASPALIONARE_QUICK_WALLET"];
/// The fixture code: (what, the names it shows up under). The dev build links the first two
/// groups (its `devFixtureNames` export calls `lottery::web::fixtures`); nothing links
/// `lottery::digest::lines` outside the tests, so its absence is checked with the same scan.
const FIXTURES: [(&str, &[&str], bool); 3] = [
    ("lottery::web::fixtures", &["web8fixtures", "web::fixtures"], true),
    ("devFixtureNames", &["devFixtureNames"], true),
    ("lottery::digest::lines", &["digest5lines", "digest::lines"], false),
];

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn tool_ok(name: &str, arg: &str) -> bool {
    Command::new(name).arg(arg).output().is_ok_and(|o| o.status.success())
}

/// `cargo build -j 2 -p web --target wasm32-unknown-unknown --release` with `features`, in
/// `target_dir` (None: the default, as the documented command); the `.wasm`, copied to
/// `target/w4/web-{tag}.wasm`.
fn build(tag: &str, features: &[&str], target_dir: Option<&str>) -> PathBuf {
    let dir = crates_dir();
    let mut args: Vec<String> = ["build", "-j", "2", "-p", "web", "--target", "wasm32-unknown-unknown", "--release"].map(String::from).to_vec();
    if let Some(t) = target_dir {
        args.extend(["--target-dir".into(), dir.join("target").join(t).to_string_lossy().to_string()]);
    }
    if !features.is_empty() {
        args.extend(["--features".into(), features.join(",")]);
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
    let out = dir.join("target").join(target_dir.unwrap_or("")).join("wasm32-unknown-unknown").join("release").join("web.wasm");
    let w4 = dir.join("target").join("w4");
    std::fs::create_dir_all(&w4).unwrap();
    // A copy per variant (the builds in target/w4 overwrite each other's web.wasm).
    let copy = w4.join(format!("web-{tag}.wasm"));
    std::fs::copy(&out, &copy).unwrap();
    println!("{tag}: cargo {} -> {} ({} B)", args.join(" "), out.display(), std::fs::metadata(&copy).unwrap().len());
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
    // F3: every page build has the node client (`node`); the release build is the page's
    // (`web/scripts/wasm.mjs release` runs exactly this command).
    let release = build("release", &["node"], None);
    let dev = build("dev", &["simnet", "fixtures", "node"], Some("w4"));
    let main = build("mainnet", &["mainnet", "node"], Some("w4"));
    let (r, d, m) = (std::fs::read(&release).unwrap(), std::fs::read(&dev).unwrap(), std::fs::read(&main).unwrap());
    // 1. The scans.
    for n in QUICK {
        let (cr, cm) = (count(&r, n), count(&m, n));
        println!("{n:28} release {cr:4}  mainnet {cm:4}");
        assert!(cr > 0, "positive control: the release build lacks `{n}`, so the scan proves nothing");
        assert_eq!(cm, 0, "the mainnet build contains `{n}`");
    }
    for (what, needles, linked_in_dev) in FIXTURES {
        let sum = |b: &[u8]| needles.iter().map(|n| count(b, n)).sum::<usize>();
        let (cr, cd, cm) = (sum(&r), sum(&d), sum(&m));
        println!("{what:28} release {cr:4}  dev {cd:4}  mainnet {cm:4}");
        if linked_in_dev {
            assert!(cd > 0, "positive control: the dev build lacks `{what}`, so the scan proves nothing");
        }
        assert_eq!(cr, 0, "the release artifact contains `{what}`");
        assert_eq!(cm, 0, "the mainnet build contains `{what}`");
    }
    // The web fixtures' wallet key (`lottery::web::fixtures::WALLET_SK`, 32 bytes of 0x21)
    // as raw bytes: in the dev build's data, not in the release artifact or the mainnet build.
    let key = [0x21u8; 32];
    let raw = |b: &[u8]| b.windows(32).filter(|w| *w == key).count();
    let (kr, kd, km) = (raw(&r), raw(&d), raw(&m));
    println!("{:28} release {kr:4}  dev {kd:4}  mainnet {km:4}", "fixtures WALLET_SK bytes");
    assert!(kd > 0, "positive control: the dev build lacks the fixture key's bytes");
    assert_eq!((kr, km), (0, 0), "the fixture key's bytes are in the release or mainnet build");
    // 2. The Node runs.
    let simnet_addr = kaspa_addresses::Address::new(kaspa_addresses::Prefix::Simnet, kaspa_addresses::Version::PubKey, &[0x42; 32]).to_string();
    let testnet_addr = kaspa_addresses::Address::new(kaspa_addresses::Prefix::Testnet, kaspa_addresses::Version::PubKey, &[0x42; 32]).to_string();
    let (ok, log) = node(
        &release,
        &r#"const m = require("$JS");
           const want = (what, r, code) => { if (!r.error || r.error.code !== code) throw new Error(what + ": " + JSON.stringify(r)); };
           const s = m.start("simnet"); want("start(simnet) before testnet", s, "address_network");
           const a = m.start("testnet-10"); if (!a.ok || a.ok.network !== "testnet-10") throw new Error("start: " + JSON.stringify(a));
           want("start(simnet)", m.start("simnet"), "address_network");
           want("start(mainnet)", m.start("mainnet"), "address_network");
           want("scriptOf(kaspasim:)", m.scriptOf("$SIM"), "address_network");
           if (!m.scriptOf("$TEST").ok) throw new Error("scriptOf(kaspatest:) refused");
           const b = m.buildRequestDeposit({covenantId: "00".repeat(32), vault: {utxo: {transactionId: "00".repeat(32), index: 0, amount: "1", scriptPublicKey: "0000", blockDaaScore: "1", isCoinbase: false}, state: ""}, amount: "1", owner: "$TEST", funds: {coins: [], change: "$SIM", feeRate: 100}});
           want("a kaspasim: change address", b, "address_network");
           if (typeof m.quickWalletNew !== "function") throw new Error("no quick wallet in the release build");
           if (typeof m.devFixtureNames !== "undefined") throw new Error("devFixtureNames in the release build");
           console.log("release: started on", a.ok.network, "; start(simnet), start(mainnet), a kaspasim: address and change refused:", s.error.code);"#
            .replace("$SIM", &simnet_addr)
            .replace("$TEST", &testnet_addr),
    );
    println!("{}", log.trim());
    assert!(ok, "the release artifact did not pass: {log}");
    let (ok, log) = node(
        &dev,
        &r#"const m = require("$JS");
           const a = m.start("simnet"); if (!a.ok || a.ok.addressPrefix !== "kaspasim") throw new Error("dev start(simnet): " + JSON.stringify(a));
           if (!m.scriptOf("$SIM").ok) throw new Error("dev: kaspasim: refused on the simnet");
           const f = m.devFixtureNames(); if (!f.ok || !f.ok.includes("buy b=1 ref")) throw new Error("devFixtureNames: " + JSON.stringify(f));
           console.log("dev: started on", a.ok.network, "with", a.ok.addressPrefix, "addresses;", f.ok.length, "fixture calls");"#
            .replace("$SIM", &simnet_addr),
    );
    println!("{}", log.trim());
    assert!(ok, "the dev build (positive control) did not start on the simnet: {log}");
    let (ok, log) = node(
        &main,
        r#"try { require("$JS"); console.log("LOADED"); process.exit(3); }
           catch (e) { const s = JSON.stringify(e); if (!s.includes("mainnet_refused")) { console.log("other error", s, String(e)); process.exit(4); }
                       console.log("mainnet: load refused:", e.error.code, "-", e.error.message); }"#,
    );
    println!("{}", log.trim());
    assert!(ok && log.contains("mainnet: load refused: mainnet_refused"), "the mainnet module did not refuse to start: {log}");
}
