//! Front-end F1 (spec frontend 6.1, 12): "no encoding change" across the F1 refactor.
//!
//! `lottery::digest::all_lines()` digests every ABI encoding of the committed templates and
//! the transactions of the builders' fixture cases (M1 to M8 shapes). The golden file
//! `tests/fixtures/f1_digests.txt` was written before F1 changed anything that reaches
//! encoding (embedded templates, `silverscript-abi` built against rusty-kaspa v2.1.0). The
//! test fails on any line that differs.
//!
//! Rewrite the golden file only for a deliberate change, from `crates/`:
//! `cargo test -p lottery --release --test f1_bytes -- --ignored write_golden`

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/f1_digests.txt");

#[test]
fn f1_encoding_and_transactions_unchanged() {
    let want = std::fs::read_to_string(GOLDEN).expect("golden file tests/fixtures/f1_digests.txt");
    let want: Vec<&str> = want.lines().filter(|l| !l.is_empty()).collect();
    let got = lottery::digest::all_lines();
    let mut diffs = vec![];
    for i in 0..want.len().max(got.len()) {
        let (w, g) = (want.get(i).copied().unwrap_or("<missing>"), got.get(i).map(String::as_str).unwrap_or("<missing>"));
        if w != g {
            diffs.push(format!("line {}:\n  golden {w}\n  now    {g}", i + 1));
        }
    }
    assert!(diffs.is_empty(), "{} of {} digest lines differ from the golden file:\n{}", diffs.len(), want.len(), diffs.join("\n"));
    println!("f1 digests: {} lines equal the golden file", got.len());
}

#[test]
#[ignore = "writes the golden file; run only for a deliberate encoding change"]
fn write_golden() {
    let lines = lottery::digest::all_lines();
    std::fs::write(GOLDEN, lines.join("\n") + "\n").unwrap();
    println!("wrote {} lines to {GOLDEN}", lines.len());
}
