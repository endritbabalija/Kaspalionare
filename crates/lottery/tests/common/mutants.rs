//! The M5 mutant mechanism, shared by `m5_mutants.rs` and `m6_mutants.rs` (moved from
//! `m5_mutants.rs` in M6 Stage 2, unchanged in behaviour): a `// mut:<ID>` tag line is
//! removed in memory (the sources on disk are untouched), the templates are recompiled, and
//! an `Atk5` is run against them (`killed_atk`: the named input accepts the attack;
//! `redundant_atk`: it still rejects it by a `require`).

use std::sync::Arc;

use harness::TxScriptError;
use lottery::buy::{Sources, Templates, b_max};

use super::m2::{Sil, accept_m4, is_engine_arithmetic, tpl};
use super::m5::{Atk5, atk5};

/// "file:TAG" or "file:TAG#n".
pub fn parse(t: &str) -> (Sil, &str, Option<usize>) {
    let (file, rest) = t.split_once(':').expect("file:TAG");
    let sil = match file {
        "vault" => Sil::Vault,
        "drawer" => Sil::Drawer,
        "request" => Sil::Request,
        "record" => Sil::Record,
        "position" => Sil::Position,
        f => panic!("no M5 file {f}"),
    };
    match rest.split_once('#') {
        Some((tag, n)) => (sil, tag, Some(n.parse().expect("line index"))),
        None => (sil, rest, None),
    }
}

/// The tags of a line (a header comment's `// mut:M5<ID>` is not a tag).
pub fn line_tags(line: &str) -> Vec<&str> {
    match line.find("// mut:") {
        Some(p) if line[p + 7..].starts_with("M5<") || line[p + 7..].starts_with('<') => vec![],
        Some(p) => line[p + 7..].split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).collect(),
        None => vec![],
    }
}

/// `src` without the `nth` line carrying `tag` (every such line if `None`).
pub fn strip(src: &str, tag: &str, nth: Option<usize>) -> String {
    let mut k = 0;
    let mut removed = 0;
    let out: Vec<&str> = src
        .lines()
        .map(|l| {
            if line_tags(l).contains(&tag) {
                let this = k;
                k += 1;
                if nth.is_none_or(|n| n == this) {
                    removed += 1;
                    return "";
                }
            }
            l
        })
        .collect();
    assert!(removed > 0 && nth.is_none_or(|_| removed == 1), "tag {tag} line {nth:?} not found");
    out.join("\n")
}

pub fn field(s: &mut Sources, which: Sil) -> &mut String {
    match which {
        Sil::Vault => &mut s.vault,
        Sil::Drawer => &mut s.drawer,
        Sil::Request => &mut s.request,
        Sil::Record => &mut s.record,
        Sil::Position => &mut s.position,
        _ => unreachable!(),
    }
}

pub fn hash(t: &Templates, which: Sil) -> [u8; 32] {
    match which {
        Sil::Vault => t.vault.hash,
        Sil::Drawer => t.drawer.hash,
        Sil::Request => t.request.hash,
        Sil::Record => t.record.hash,
        Sil::Position => t.position.hash,
        _ => unreachable!(),
    }
}

/// Templates compiled with the given tag lines removed (one or more files). Lines of the
/// same tag are given as distinct tags only (stripping shifts the line index).
pub fn mutant5(tags: &[&str]) -> Arc<Templates> {
    let mut src = Sources::committed(b_max());
    let mut files = vec![];
    for t in tags {
        let (sil, tag, n) = parse(t);
        let s = field(&mut src, sil);
        *s = strip(s, tag, n);
        files.push(sil);
    }
    let t = Templates::compile_sources(b_max(), &src);
    for sil in files {
        assert_ne!(hash(&t, sil), hash(&tpl(), sil), "mutant {tags:?} compiled to the committed template");
    }
    Arc::new(t)
}

/// The attack `id` of `common::m5::atk5` (M5), killed (`killed_atk`).
pub fn killed(tags: &[&str], id: &str) {
    killed_atk(tags, &atk5(id));
}

/// The attack is rejected with the committed templates (strike), the base is accepted
/// under the mutant, and the mutant's named input accepts the attack.
pub fn killed_atk(tags: &[&str], a: &Atk5) {
    let id = a.id.as_str();
    a.strike();
    let t = mutant5(tags);
    let (g, b) = a.pair(&t);
    accept_m4(&format!("base under mutant {tags:?}"), &g);
    if let Err(e) = b.finish().run_input(a.input) {
        panic!("mutant {tags:?}: input {} still rejects `{id}`: {e:?}", a.input);
    }
}

/// The attack `id` of `common::m5::atk5` (M5), redundant (`redundant_atk`).
pub fn redundant(tags: &[&str], id: &str) {
    redundant_atk(tags, &atk5(id));
}

/// Redundant line: the mutant still rejects the attack at the named input (not on the
/// budget, and not only by an engine arithmetic error such as division by zero or a 64-bit
/// overflow, which is no guard: Stage 7 S7-1), and still accepts the base.
pub fn redundant_atk(tags: &[&str], a: &Atk5) {
    let id = a.id.as_str();
    a.strike();
    let t = mutant5(tags);
    let (g, b) = a.pair(&t);
    accept_m4(&format!("base under mutant {tags:?}"), &g);
    match b.finish().run_input(a.input) {
        Ok(_) => panic!("mutant {tags:?} accepts `{id}` at input {}: the line is not redundant", a.input),
        Err(e) => {
            assert!(!matches!(e, TxScriptError::ExceededCommittedScriptUnits { .. }), "budget error: {e:?}");
            assert!(!is_engine_arithmetic(&e), "mutant {tags:?} `{id}`: input {} rejects only by an engine arithmetic error, not a require: {e:?}", a.input);
            println!("redundant {tags:?} `{id}`: still rejected at input {} by {e:?}", a.input);
        }
    }
}
