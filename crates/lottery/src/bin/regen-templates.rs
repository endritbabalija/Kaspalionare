//! Rewrites `crates/lottery/templates/` (the embedded templates, front-end F1) from a fresh
//! compile of `covenants/`. Run from `crates/`: `cargo run -p lottery --bin regen-templates`.
//! `tests/f1_templates.rs` (W2) fails until this is run after a `.sil` change.

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    std::fs::create_dir_all(&dir).expect("create templates/");
    let files = lottery::embedded::generate();
    let mut changed = 0;
    for (name, contents) in &files {
        let path = dir.join(name);
        let old = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
        if old != *contents {
            std::fs::write(&path, contents).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
            changed += 1;
            println!("wrote {} ({} B)", path.display(), contents.len());
        }
    }
    println!("{} files, {changed} changed, in {}", files.len(), dir.display());
}
