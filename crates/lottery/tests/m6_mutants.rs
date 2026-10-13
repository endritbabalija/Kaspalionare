//! M6 mutation tests (spec M6 A8 "Mutants"), with the M5 mechanism (`common::mutants`, moved
//! from m5_mutants.rs): the two M6 tags of vault.sil `request_withdraw`, check W4b (D86):
//! `M6W4b_min` (a partial needs w >= wmin) and `M6W4b_rest` (a partial leaves >= wmin
//! shares; Stage 5 review RV-1, orchestrator decision O-M6-2). Each line is removed in
//! memory (the sources on disk are untouched); the vault without it accepts the attacks
//! named below (`common::m6::la2`, `la7`), and the valid bases are still accepted by every
//! input. The constant changes (MIN_DEPOSIT, D85) carry no tag.
//!
//! RV-1 split the M6 Stage 2 one-line `require(w >= wmin || w == ps.shares)` into the two
//! requires inside `if (w != ps.shares)`. LA2's "w = shares - 1 < wmin (whole position below
//! wmin)" leaves a 1-share remainder, so each line alone rejects it: it is now killed only
//! by removing both lines, and is redundant for each line alone (both asserted below).
//!
//! `every_m6_tag_is_tested` pins the M6 tag set: exactly two tags (`M6...`) on two lines,
//! across every covenant source, and each is named here.

mod common;

use common::m6::atk6;
use common::mutants::{killed_atk, line_tags, redundant_atk};

macro_rules! killed6 {
    ($name:ident, [$($tag:expr),+], $id:expr) => {
        #[test]
        fn $name() {
            killed_atk(&[$($tag),+], &atk6($id));
        }
    };
}

// ================================================================ vault request_withdraw W4b

macro_rules! redundant6 {
    ($name:ident, [$($tag:expr),+], $id:expr) => {
        #[test]
        fn $name() {
            redundant_atk(&[$($tag),+], &atk6($id));
        }
    };
}

killed6!(m6w4b_min, ["vault:M6W4b_min"], "LA2 request_withdraw w = wmin - 1 (q >= 1)");
// RV-1: both lines reject a 1-share remainder below wmin, so this LA2 case needs both gone.
killed6!(m6w4b_min_whole_small, ["vault:M6W4b_min", "vault:M6W4b_rest"], "LA2 request_withdraw w = shares - 1 < wmin (whole position below wmin)");
redundant6!(m6w4b_min_whole_small_min_only, ["vault:M6W4b_min"], "LA2 request_withdraw w = shares - 1 < wmin (whole position below wmin)");
redundant6!(m6w4b_min_whole_small_rest_only, ["vault:M6W4b_rest"], "LA2 request_withdraw w = shares - 1 < wmin (whole position below wmin)");

// The remainder rule (Stage 5 RV-1, O-M6-2): LA7's three attacks pass `w >= wmin`.
killed6!(m6w4b_rest_dust_split, ["vault:M6W4b_rest"], "LA7a request_withdraw w = wmin from a position of wmin + 1 (remainder 1 < wmin)");
killed6!(m6w4b_rest_small_w, ["vault:M6W4b_rest"], "LA7b request_withdraw w = wmin + 1 from 2 x wmin (remainder wmin - 1, w >= wmin)");
killed6!(m6w4b_rest_large_w, ["vault:M6W4b_rest"], "LA7c request_withdraw w = shares - wmin + 1 (remainder wmin - 1, large w)");

// M5's W4 `w <= ps.shares` is implied by the remainder rule (m5_mutants.rs
// w4_shares_redundant, decision S5-1); with both lines removed the vault accepts M5's
// "w = shares + 1" again, so the pair still guards it.
#[test]
fn m5w4_shares_with_m6w4b_rest() {
    killed_atk(&["vault:M5W4_shares", "vault:M6W4b_rest"], &common::m5::atk5("VA6 request_withdraw w = shares + 1"));
}

// ================================================================ coverage

/// Every `// mut:M6...` tag line of every covenant source is named above as "file:TAG", and
/// there are exactly two such tags on two lines (spec M6 A8; `M6W4b_rest` added by the
/// Stage 5 RV-1 fix).
#[test]
fn every_m6_tag_is_tested() {
    let me = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/m6_mutants.rs")).expect("own source");
    let dir = harness::covenant_path("vault.sil").parent().expect("covenants dir").to_path_buf();
    let mut files: Vec<_> = std::fs::read_dir(&dir).expect("covenants dir").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "sil")).collect();
    files.sort();
    assert!(files.len() >= 11, "covenant sources: {files:?}");
    let (mut tags, mut lines) = (std::collections::BTreeSet::new(), 0);
    for p in files {
        let file = p.file_stem().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(&p).expect("source");
        for l in src.lines() {
            for t in line_tags(l) {
                if t.starts_with("M6") {
                    assert!(me.contains(&format!("\"{file}:{t}\"")), "{file}.sil: tag {t} has no mutant test");
                    tags.insert(format!("{file}:{t}"));
                    lines += 1;
                }
            }
        }
    }
    println!("M6 tags: {tags:?}, tagged lines: {lines}");
    assert_eq!((tags.len(), lines), (2, 2), "M6 tag count changed: update this test and the handoff");
}
