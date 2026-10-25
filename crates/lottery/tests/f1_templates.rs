//! Front-end F1, W2 (spec frontend 8): the templates embedded at build time
//! (`crates/lottery/templates/`, `lottery::embedded`) equal a fresh compile of `covenants/`.
//! A `.sil` change without regenerating fails here; regenerate from `crates/` with
//! `cargo run -p lottery --bin regen-templates`.

use lottery::buy::{Templates, b_max, source_b_max};
use lottery::digest::template_list;
use lottery::embedded::{REGENERATE, embedded_file, generate};

#[test]
fn w2_embedded_files_equal_a_fresh_compile() {
    let mut stale = vec![];
    for (file, fresh) in generate() {
        match embedded_file(&file) {
            Some(e) if e == fresh => {}
            Some(e) => stale.push(format!("{file}: embedded {} B, fresh compile {} B", e.len(), fresh.len())),
            None => stale.push(format!("{file}: not embedded")),
        }
    }
    assert!(
        stale.is_empty(),
        "the embedded templates (crates/lottery/templates/) differ from a fresh compile of covenants/:\n  {}\nregenerate them from crates/ with: {REGENERATE}",
        stale.join("\n  ")
    );
}

#[test]
fn w2_b_max_is_shard_sil() {
    assert_eq!(b_max(), source_b_max(), "embedded B_MAX differs from shard.sil; regenerate from crates/ with: {REGENERATE}");
}

/// The runtime path (`Templates::get`, embedded) and a fresh compile give the same templates:
/// artifacts, prefixes, suffixes and template hashes.
#[test]
fn w2_runtime_templates_equal_a_fresh_compile() {
    let got = Templates::get(b_max());
    let fresh = Templates::compile(b_max());
    for ((name, g), (_, f)) in template_list(&got).into_iter().zip(template_list(&fresh)) {
        assert!(g.art == f.art, "{name}: artifact differs; regenerate from crates/ with: {REGENERATE}");
        assert_eq!(g.prefix, f.prefix, "{name}: prefix");
        assert_eq!(g.suffix, f.suffix, "{name}: suffix");
        assert_eq!(g.hash, f.hash, "{name}: template hash");
    }
    let e = Templates::embedded().expect("embedded templates parse and pass check_consistency");
    assert_eq!(e.b_max, b_max());
}

/// A B_MAX other than the committed one is compiled (only with the `compiler` feature).
#[test]
fn w2_other_b_max_compiles() {
    let other = if b_max() == 1 { 2 } else { 1 };
    let t = Templates::try_get(other).expect("compiler builds compile other B_MAX values");
    assert_eq!(t.b_max, other);
    assert_ne!(t.ticket.hash, Templates::get(b_max()).ticket.hash);
}
