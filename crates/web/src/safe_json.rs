//! The wallets' transaction format, Safe JSON (front-end F0, moved here from `tn10/src/pskt.rs`
//! in F1 part C so the page and the tn10 probe share one serialiser).
//!
//! KasWare's `signPskt({txJsonString, options})` takes and returns a transaction "in "Safe JSON"
//! format that can be deserialized by kaspa-wasm's `Transaction.deserializeFromSafeJSON()`"
//! (docs.kasware.xyz, wallet/dev-base/kaspa-transaction, read 2026-10-25; Kaspire's `signPskt`
//! takes the same `txJsonString`). That function is `string::SerializableTransaction::
//! deserialize_from_json` in rusty-kaspa v2.1.0 `consensus/client/src/transaction.rs:544-547`,
//! and `serializeToSafeJSON` is `serialize_to_json` (`:526-529`), schema in
//! `consensus/client/src/serializable/string.rs`. This module uses that very type from the
//! pinned `kaspa-consensus-client`, so the JSON is the v2.1.0 Safe JSON byte for byte:
//! - u64 fields as decimal strings; `signatureScript` and `payload` as hex; `scriptPublicKey`
//!   as hex of the 2-byte version then the script;
//! - v1 inputs carry `computeBudget`, outputs `covenant: {authorizingInput, covenantId}`
//!   (KIP-20), entries `covenantId`; the mass is written as `storageMass`;
//! - each input's `utxo.address` is filled from its script on the given network (P2PK or
//!   P2SH), as kaspa-wasm's own generator does from `getUtxosByAddresses` entries.
//! Inputs the wallet must sign go out with an empty `signatureScript` (the kaspa-wasm
//! convention for unsigned inputs); every covenant input goes out fully formed.

use kaspa_addresses::Prefix;
use kaspa_consensus_client::{numeric, string::SerializableTransaction as Safe};
use kaspa_consensus_core::tx::{SignableTransaction, Transaction, UtxoEntry};
use kaspa_txscript::extract_script_pub_key_address;

type R<T> = Result<T, String>;

/// The longest JSON accepted (a K = 16 close is about 441 KB; 8 MB is far above any shape).
pub const JSON_MAX: usize = 8 << 20;

/// The Safe JSON of `tx` with `entries`, the signature scripts of `blank` emptied, the
/// `utxo.address` fields on the network of `prefix`.
pub fn to_safe_json(tx: &Transaction, entries: &[UtxoEntry], blank: &[usize], prefix: Prefix) -> R<String> {
    let t = lottery::web::sign::blanked(tx, blank).map_err(|e| e.to_string())?;
    let mut s = Safe::from_signable_transaction(&SignableTransaction::with_entries(t, entries.to_vec())).map_err(|e| e.to_string())?;
    for inp in &mut s.inputs {
        inp.utxo.address = extract_script_pub_key_address(&inp.utxo.script_public_key, prefix).ok();
    }
    s.serialize_to_json().map_err(|e| e.to_string())
}

/// Parse a wallet's returned transaction: Safe JSON (strings), else the numeric JSON form.
/// Returns the transaction (finalized: its id computed), its entries as stated, and which
/// form parsed.
pub fn from_json(json: &str) -> R<(Transaction, Vec<Option<UtxoEntry>>, &'static str)> {
    if json.len() > JSON_MAX {
        return Err(format!("{} bytes of JSON (at most {JSON_MAX})", json.len()));
    }
    let (st, form) = match Safe::deserialize_from_json(json) {
        Ok(s) => (SignableTransaction::try_from(s).map_err(|e| format!("Safe JSON: {e}"))?, "safe"),
        Err(e1) => {
            let n = numeric::SerializableTransaction::deserialize_from_json(json).map_err(|e2| format!("not Safe JSON ({e1}) nor numeric JSON ({e2})"))?;
            (SignableTransaction::try_from(n).map_err(|e| format!("numeric JSON: {e}"))?, "numeric")
        }
    };
    let mut tx = st.tx;
    tx.finalize();
    Ok((tx, st.entries, form))
}

/// Re-serialize a parsed transaction as Safe JSON (a signer's output).
pub fn reserialize(tx: &Transaction, entries: &[UtxoEntry], prefix: Prefix) -> R<String> {
    to_safe_json(tx, entries, &[], prefix)
}
