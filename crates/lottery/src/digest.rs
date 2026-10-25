//! Byte digests of everything the builders produce, for byte-identity checks (front-end F1).
//!
//! - `abi_digests`: for every embedded template, the ABI's output on deterministic
//!   arguments: each entry's signature script (several argument sets per entry, including
//!   push-size and integer-encoding edge cases and error cases), each runtime state's encoding,
//!   its decode, and decodes of altered state bytes. These are the only bytes
//!   `silverscript-abi` produces, so they pin its encoding.
//! - `review_digests` (F1 review): 20 more builder shapes, appended to the golden file.
//! - `tx_digests`: the transactions of the builders' fixture cases (`BuyCase::new` and the
//!   other `*Case` defaults from M1 to M8), with their UTXO entries, script units and
//!   masses. Signatures over random nonces differ per run; an input whose signature script
//!   differs between two `finish()` calls is replaced by its length.
//!
//! The golden file `tests/fixtures/f1_digests.txt` was written before the F1 ABI change
//! (`silverscript-abi` built against rusty-kaspa v2.1.0); `tests/f1_bytes.rs` compares
//! against it. Part B's W1 runs the same functions in wasm and compares with the same file.

//! `template_list` is in every build (the web API's `start` reports the template hashes from
//! it). The digests themselves, with the fixture cases and their keys, are in `lines`, behind
//! the `fixtures` feature (F1 review: the deployed web build has none of them).

use crate::buy::{Template, Templates};

#[cfg(feature = "fixtures")]
mod lines;
#[cfg(feature = "fixtures")]
pub use lines::{abi_digests, all_lines, built_digest, review_digest, review_digests, tx_digests};

/// The templates `abi_digests` covers, by name (`Templates` field order).
pub fn template_list(t: &Templates) -> Vec<(&'static str, &Template)> {
    vec![
        ("ticket", &*t.ticket),
        ("controller", &*t.controller),
        ("shard", &*t.shard),
        ("params", &*t.params),
        ("marker", &*t.marker),
        ("vault", &*t.vault),
        ("pool", &*t.pool),
        ("record", &*t.record),
        ("position", &*t.position),
        ("request", &*t.request),
        ("drawer", &*t.drawer),
    ]
}
