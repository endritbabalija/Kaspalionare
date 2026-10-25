//! The doctored modes cannot be enabled by accident in a production run (F2): the
//! production binary `indexer-api`, built without the `doctored` feature (as it ships),
//! contains no doctor code (a byte scan for the module's marker and every mode name) and
//! refuses `--doctor` as an unknown flag. The positive control: the doctored binary built
//! with the feature contains them.
//!
//! Run: `cargo test -j 2 -p indexer-api --test production -- --nocapture` (it runs two nested
//! `cargo build`s; with `--features doctored` it also checks the doctored binary).

use std::path::PathBuf;
use std::process::Command;

fn cargo_build(bin: &str, features: &[&str]) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let target = root.join("target").join("indexer-api-prod");
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(&root).args(["build", "-j", "2", "-p", "indexer-api", "--bin", bin, "--target-dir"]).arg(&target);
    if !features.is_empty() {
        c.args(["--features", &features.join(",")]);
    }
    let out = c.output().expect("cargo build");
    assert!(out.status.success(), "cargo build {bin}: {}", String::from_utf8_lossy(&out.stderr));
    let exe = target.join("debug").join(format!("{bin}{}", std::env::consts::EXE_SUFFIX));
    assert!(exe.exists(), "{}", exe.display());
    exe
}

fn count(hay: &[u8], needle: &str) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle.as_bytes()).count()
}

const NEEDLES: [&str; 7] = ["KASPALIONARE_DOCTORED_API", "wrong_state", "wrong_tree", "missing_purchase", "extra_purchase", "wrong_winning", "wrong_anchor_proof"];

#[test]
fn production_binary_has_no_doctored_mode() {
    let prod = cargo_build("indexer-api", &[]);
    let bytes = std::fs::read(&prod).unwrap();
    for n in NEEDLES {
        let c = count(&bytes, n);
        println!("{n:28} production {c}");
        assert_eq!(c, 0, "the production binary contains {n}");
    }
    let out = Command::new(&prod).args(["--store", ".", "--cov", &"00".repeat(32), "--doctor", "wrong_state"]).output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(err.contains("unknown flag --doctor"), "{err}");
    println!("production: `--doctor wrong_state` refused: {}", err.lines().next().unwrap_or(""));
    // The positive control: the doctored binary (feature on) has them all.
    let doc = cargo_build("indexer-api-doctored", &["doctored"]);
    let bytes = std::fs::read(&doc).unwrap();
    for n in NEEDLES {
        let c = count(&bytes, n);
        println!("{n:28} doctored   {c}");
        assert!(c > 0, "the doctored binary lacks {n} (the scan would miss it)");
    }
    // And it binds to loopback only.
    let out = Command::new(&doc).args(["--store", ".", "--cov", &"00".repeat(32), "--doctor", "wrong_state", "--bind", "0.0.0.0:8799"]).output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(err.contains("loopback"), "{err}");
}
