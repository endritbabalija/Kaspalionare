//! The compiled templates, embedded at build time (front-end F1, spec frontend 6.1).
//!
//! `crates/lottery/templates/` holds the SilverScript ABI artifact (`SilAbiArtifact`, the
//! `silverc` JSON format, `silverscript_abi::to_pretty_json`) of each of the 11 templates the
//! builders use, compiled from `covenants/` at the committed B_MAX, and `b_max.in` (that
//! B_MAX). `Templates::get(b_max())` reads these, so the library needs neither the `.sil`
//! files nor the compiler at run time (the web build has neither).
//!
//! `tests/f1_templates.rs` (W2) compiles `covenants/` afresh and fails if any file differs.
//! After a `.sil` change, regenerate from `crates/`: `cargo run -p lottery --bin regen-templates`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use harness::SilAbiArtifact;

/// The command that rewrites `crates/lottery/templates/` (run from `crates/`).
pub const REGENERATE: &str = "cargo run -p lottery --bin regen-templates";

/// The committed B_MAX the embedded templates were compiled at.
pub const B_MAX: usize = include!("../templates/b_max.in");

/// (template name, file name, embedded JSON), in `Templates` field order.
pub const FILES: [(&str, &str, &str); 11] = [
    ("ticket", "ticket.json", include_str!("../templates/ticket.json")),
    ("controller", "controller.json", include_str!("../templates/controller.json")),
    ("shard", "shard.json", include_str!("../templates/shard.json")),
    ("params", "params.json", include_str!("../templates/params.json")),
    ("marker", "marker.json", include_str!("../templates/marker.json")),
    ("vault", "vault.json", include_str!("../templates/vault.json")),
    ("pool", "pool.json", include_str!("../templates/pool.json")),
    ("record", "record.json", include_str!("../templates/record.json")),
    ("position", "position.json", include_str!("../templates/position.json")),
    ("request", "request.json", include_str!("../templates/request.json")),
    ("drawer", "drawer.json", include_str!("../templates/drawer.json")),
];

/// The embedded artifacts by template name, parsed once.
pub fn artifacts() -> Result<&'static BTreeMap<String, SilAbiArtifact>, String> {
    static PARSED: OnceLock<Result<BTreeMap<String, SilAbiArtifact>, String>> = OnceLock::new();
    PARSED
        .get_or_init(|| {
            FILES
                .iter()
                .map(|(name, file, json)| {
                    let art: SilAbiArtifact = serde_json::from_str(json).map_err(|e| format!("embedded template {file}: {e}"))?;
                    art.check_consistency().map_err(|e| format!("embedded template {file}: {e}"))?;
                    Ok((name.to_string(), art))
                })
                .collect()
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// The files `regen-templates` writes, compiled afresh from `covenants/` at the B_MAX of
/// shard.sil: (file name, contents). The W2 test compares them with the embedded files.
#[cfg(feature = "compiler")]
pub fn generate() -> Vec<(String, String)> {
    let b = crate::buy::source_b_max();
    let t = crate::buy::Templates::compile(b);
    let mut out = vec![("b_max.in".to_string(), format!("{b}\n"))];
    for (name, tpl) in crate::digest::template_list(&t) {
        let file = FILES.iter().find(|f| f.0 == name).expect("every template has a file").1;
        let json = silverscript_abi::to_pretty_json(&tpl.art).expect("artifact serializes");
        out.push((file.to_string(), json + "\n"));
    }
    out
}

/// The embedded contents of a generated file (line endings normalized to LF).
#[cfg(feature = "compiler")]
pub fn embedded_file(file: &str) -> Option<String> {
    let s = if file == "b_max.in" { format!("{B_MAX}\n") } else { FILES.iter().find(|f| f.1 == file)?.2.to_string() };
    Some(s.replace("\r\n", "\n"))
}
