//! M6 mutation tests (spec M6 A8 "Mutants"), with the M5 mechanism (`common::mutants`, moved
//! from m5_mutants.rs): the one M6 tag `M6W4b_min` (vault.sil `request_withdraw`, check W4b,
//! D86). With that line removed (compiled in memory; the sources on disk are untouched), the
//! vault accepts LA2's first two attacks (`common::m6::la2`), and the valid bases are still
//! accepted by every input. The constant changes (MIN_DEPOSIT, D85) carry no tag.
//!
//! `every_m6_tag_is_tested` pins the M6 tag set: exactly one tag (`M6...`) on one line,
//! across every covenant source, and each is named here.

mod common;

use common::m6::atk6;
use common::mutants::{killed_atk, line_tags};

macro_rules! killed6 {
    ($name:ident, [$($tag:expr),+], $id:expr) => {
        #[test]
        fn $name() {
            killed_atk(&[$($tag),+], &atk6($id));
        }
    };
}

// ================================================================ vault request_withdraw W4b

killed6!(m6w4b_min, ["vault:M6W4b_min"], "LA2 request_withdraw w = wmin - 1 (q >= 1)");
killed6!(m6w4b_min_whole_small, ["vault:M6W4b_min"], "LA2 request_withdraw w = shares - 1 < wmin (whole position below wmin)");

// ================================================================ coverage

/// Every `// mut:M6...` tag line of every covenant source is named above as "file:TAG", and
/// there is exactly one such tag on one line (spec M6 A8).
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
    assert_eq!((tags.len(), lines), (1, 1), "M6 tag count changed: update this test and the handoff");
}
