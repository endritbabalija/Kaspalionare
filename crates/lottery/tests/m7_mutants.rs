//! M7 mutation tests (spec M7 A8 "Mutants", negative controls as M2/M5), with the M5
//! mechanism (`common::mutants`): a `// mut:<TAG>` line of drawer.sil is removed in memory
//! (the sources on disk are untouched), the templates are recompiled, and the SA attack named
//! below (data in `common::m7`) is run against them.
//!
//! - `killed7!`: the line is a `require`; the mutant still accepts the valid base at every
//!   input and its drawer (input 0) ACCEPTS the attack (`common::mutants::killed_atk`).
//! - `killed7_assign!`: the line is an assignment (M7V_lp, M7V_H, M7C_claim change what the
//!   drawer computes); under the mutant the valid base is REJECTED by the drawer and the
//!   attack, which matches the mutant's computation, is ACCEPTED (as M3 `killed_assign`).
//!
//! Every attack is first struck with the committed templates (`Atk5::strike`), so a mutant
//! test also fails while the committed scripts reject the base (M7 Stage 1b: the drawer has
//! no `draw` entry and no M7 tags yet; Stage 2 places the tags, see the line table in
//! `docs/specs/7-handoff.md` Stage 1b).
//!
//! `every_m7_tag_is_tested` pins the M7 tag set: 14 tags on 16 lines of drawer.sil (M7V_leaves
//! and M7V_H tag two lines each), every one named here as "drawer:TAG". The M3 tags D7, D19_own
//! and D19_max are retired (m3_mutants.rs pin 85).

mod common;

use common::m5::Atk5;
use common::m7::atk7;
use common::mutants::{killed_atk, line_tags, mutant5};

macro_rules! killed7 {
    ($name:ident, [$($tag:expr),+], $id:expr) => {
        #[test]
        fn $name() {
            killed_atk(&[$($tag),+], &atk7($id));
        }
    };
}

macro_rules! killed7_assign {
    ($name:ident, [$($tag:expr),+], $id:expr) => {
        #[test]
        fn $name() {
            killed_assign_atk(&[$($tag),+], &atk7($id));
        }
    };
}

/// An assignment mutant: the attack is struck with the committed templates; under the mutant
/// the drawer rejects the valid base (the line changes what it computes) and accepts the
/// attack.
fn killed_assign_atk(tags: &[&str], a: &Atk5) {
    a.strike();
    let t = mutant5(tags);
    let (g, b) = a.pair(&t);
    if g.finish().run_input(a.input).is_ok() {
        panic!("assignment mutant {tags:?}: input {} still accepts the valid base of `{}` (the line changes nothing it checks)", a.input, a.id);
    }
    if let Err(e) = b.finish().run_input(a.input) {
        panic!("mutant {tags:?}: input {} still rejects `{}`: {e:?}", a.input, a.id);
    }
}

// ================================================================ check 1

killed7!(m7a_len, ["drawer:M7A_len"], "SA4 anchor length 305 (a zero byte appended) (M7A_len)");

// ================================================================ check 7

// Spec M7 A4: D is formed from mkDaa = OpTxInputDaaScore(1); with the equality removed the
// mk_daa argument reaches nothing on chain, so a lying argument with every output from the
// true D is accepted (the round result record, M3 A9, would be wrong).
killed7!(m7d_mk, ["drawer:M7D_mk"], "SA5 mk_daa argument + 1, every output from the true D (M7D_mk)");
killed7!(m7d_lock, ["drawer:M7D_lock"], "SA5 lock_time = D - 1 (M7D_lock)");
killed7!(m7v_lock, ["drawer:M7V_lock"], "SA7 void at lock_time = D + draw_window - 1 (M7V_lock)");

// ================================================================ check 8

killed7!(m7a_xc, ["drawer:M7A_xc"], "SA2 an unknown X.hash with X's genuine preimage: BlockAlreadyPruned; the seed follows the claimed hash (M7A_xc)");
killed7!(m7a_xc_side_block, ["drawer:M7A_xc"], "SA2 a known side block N at DAA D whose selected parent is P: BlockNotSelected (M7A_xc)");
killed7!(m7a_pc, ["drawer:M7A_pc"], "SA2 an unknown P.hash with P's genuine preimage: BlockAlreadyPruned (M7A_pc)");
killed7!(m7a_pceq, ["drawer:M7A_pceq"], "SA2 X+ with P as its claimed parent (P is on the chain, not X+'s selected parent): pc != x_pc (M7A_pceq)");
killed7!(m7a_dx, ["drawer:M7A_dX"], "SA1 P as the anchor with its parent: dX = D - 1 < D (M7A_dX)");
killed7!(m7a_dp, ["drawer:M7A_dP"], "SA1 X+ (X's chain child) with X as P: dP = daa(X) = D (M7A_dP)");
killed7!(m7v_win5, ["drawer:M7V_win5"], "SA7 void with win5 = the X.hash seed's main numbers (M7V_win5)");
killed7!(m7v_leaves, ["drawer:M7V_leaves"], "SA7 void with non-empty leaves (the seed rank's count leaves) (M7V_leaves)");
killed7!(m7v_leaves_siblings, ["drawer:M7V_leaves"], "SA7 void with non-empty siblings (the seed rank's paths) (M7V_leaves)");

// ================================================================ checks 12, 14/15, 19 (assignments)

killed7_assign!(m7v_lp, ["drawer:M7V_lp"], "SA7 void creating the lower-tier pools a draw would (their budgets out of O') (M7V_lp)");
killed7_assign!(m7v_h, ["drawer:M7V_H"], "SA7 void paying H to the vault (V' = V + H, O' without H) (M7V_H)");
killed7_assign!(m7c_claim, ["drawer:M7C_claim"], "SA9 DA13 every new pool's deadline D + claim_window, without the draw window (M7C_claim)");

// ================================================================ coverage

/// Every `// mut:M7...` tag line of every covenant source is named above as "file:TAG"; there
/// are exactly 14 M7 tags on 16 lines, all in drawer.sil (spec M7 A8).
#[test]
fn every_m7_tag_is_tested() {
    let me = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/m7_mutants.rs")).expect("own source");
    let dir = harness::covenant_path("drawer.sil").parent().expect("covenants dir").to_path_buf();
    let mut files: Vec<_> = std::fs::read_dir(&dir).expect("covenants dir").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "sil")).collect();
    files.sort();
    assert!(files.len() >= 11, "covenant sources: {files:?}");
    let (mut tags, mut lines) = (std::collections::BTreeSet::new(), 0);
    for p in files {
        let file = p.file_stem().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(&p).expect("source");
        for l in src.lines() {
            for t in line_tags(l) {
                if t.starts_with("M7") {
                    assert_eq!(file, "drawer", "M7 tag {t} outside drawer.sil");
                    assert!(me.contains(&format!("\"{file}:{t}\"")), "{file}.sil: tag {t} has no mutant test");
                    tags.insert(format!("{file}:{t}"));
                    lines += 1;
                }
            }
        }
    }
    println!("M7 tags: {tags:?}, tagged lines: {lines}");
    let spec = [
        "M7A_len", "M7D_mk", "M7D_lock", "M7V_lock", "M7A_xc", "M7A_pc", "M7A_pceq", "M7A_dX", "M7A_dP", "M7V_win5", "M7V_leaves", "M7V_lp", "M7V_H", "M7C_claim",
    ];
    for t in spec {
        assert!(tags.contains(&format!("drawer:{t}")), "spec M7 A8 tag {t} is not in drawer.sil");
    }
    assert_eq!((tags.len(), lines), (14, 16), "M7 tag count changed: update this test and the handoff");
}
