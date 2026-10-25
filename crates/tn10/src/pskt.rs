//! Front-end F0 (spec frontend 6.2, 9): the wallet's transaction format and the post-sign
//! check.
//!
//! Format (F1 part C: the serialiser itself is `web::safe_json`, shared with the page). KasWare's `signPskt({txJsonString, options})` takes and returns a transaction "in
//! "Safe JSON" format that can be deserialized by kaspa-wasm's
//! `Transaction.deserializeFromSafeJSON()`" (docs.kasware.xyz, wallet/dev-base/
//! kaspa-transaction, read 2026-10-25; Kaspire's `signPskt` takes the same `txJsonString`,
//! kaspire.kaslab.space/developers/extension). That function is
//! `string::SerializableTransaction::deserialize_from_json` in rusty-kaspa v2.1.0
//! `consensus/client/src/transaction.rs:544-547`, and `serializeToSafeJSON` is
//! `string::SerializableTransaction::serialize_to_json` (`:526-529`), schema in
//! `consensus/client/src/serializable/string.rs`. We use that very type from the pinned
//! `kaspa-consensus-client` (already in Cargo.lock through the wRPC client), so the JSON is
//! the v2.1.0 Safe JSON byte for byte, with no hand-written mirror:
//! - u64 fields (`sequence`, `amount`, `blockDaaScore`, `value`, `lockTime`, `gas`,
//!   `storageMass`) as decimal strings; `signatureScript` and `payload` as hex;
//!   `scriptPublicKey` as hex of the 2-byte version then the script;
//! - v1 inputs carry `computeBudget` (and `sigOpCount` 0, `serializable/string.rs:135-150`),
//!   outputs carry `covenant: {authorizingInput, covenantId}` (KIP-20), entries `covenantId`;
//! - each input's `utxo.address` is filled from its script (P2PK or P2SH, testnet prefix),
//!   as kaspa-wasm's own generator does from `getUtxosByAddresses` entries.
//! Inputs the wallet must sign go out with an empty `signatureScript` (the kaspa-wasm
//! convention for unsigned inputs); every covenant input goes out fully formed.
//!
//! Post-sign check (spec 6.2, an attack-test target): the returned transaction is compared
//! with the one built. Any difference in the outputs, version, lock time, subnetwork, gas or
//! payload; in any input's outpoint, sequence or compute commit (v1's form of the sig-op
//! count); in the signature script of any input not listed for signing; or a listed input
//! left unsigned or not in the Schnorr SIG_HASH_ALL form, refuses the transaction. A
//! different storage-mass commit or UTXO entry is a note, not a refusal: neither is taken
//! from the wallet (`assemble` keeps ours, and the engine checks the wallet's signatures
//! against our entries), and an older kaspa-wasm may drop or rename them (the KasWare doc's
//! sample has `mass`, v2.1.0 writes `storageMass` and reads both).

use kaspa_addresses::Prefix;
use kaspa_consensus_core::tx::{Transaction, UtxoEntry};

type R<T> = Result<T, String>;

/// `tx` with the signature scripts of `blank` emptied: the transaction as sent to the wallet.
/// (F1 part B: moved to `lottery::web::sign::blanked`.)
pub fn blanked(tx: &Transaction, blank: &[usize]) -> R<Transaction> {
    lottery::web::sign::blanked(tx, blank).map_err(|e| e.to_string())
}

/// The Safe JSON of `tx` with `entries`, the signature scripts of `blank` emptied (F1 part C:
/// the one serialiser is `web::safe_json`, which the page uses too).
pub fn to_safe_json(tx: &Transaction, entries: &[UtxoEntry], blank: &[usize]) -> R<String> {
    web::safe_json::to_safe_json(tx, entries, blank, Prefix::Testnet)
}

/// Parse a wallet's returned transaction: Safe JSON (strings), else the numeric JSON form.
/// Returns the transaction, its entries as stated, and which form parsed.
pub fn from_json(json: &str) -> R<(Transaction, Vec<Option<UtxoEntry>>, &'static str)> {
    web::safe_json::from_json(json)
}

/// Re-serialize a parsed transaction as Safe JSON (the stand-in signer's output).
pub fn reserialize(tx: &Transaction, entries: &[UtxoEntry]) -> R<String> {
    web::safe_json::reserialize(tx, entries, Prefix::Testnet)
}

/// The outcome of the post-sign check (F1 part B: moved to `lottery::web::sign`, so the
/// web crate runs the same check; each refusal now also has a typed code).
pub use lottery::web::sign::Check;

/// Compare the returned transaction `got` (with its stated entries) against `ours`, the
/// transaction exactly as sent to the wallet (`blanked`: covenant sigscripts final, the
/// inputs to sign empty), with `entries`; `sign` lists the inputs the wallet was asked to
/// sign, each a P2PK input of the key its entry's script names. The check itself is
/// `lottery::web::sign::post_sign` (spec 6.2).
pub fn post_sign(ours: &Transaction, entries: &[UtxoEntry], sign: &[usize], got: &Transaction, got_entries: &[Option<UtxoEntry>]) -> Check {
    match lottery::web::sign::signers_for(entries, sign) {
        Ok(s) => lottery::web::sign::post_sign(ours, entries, &s, got, got_entries),
        Err(e) => Check { refusals: vec![e.to_string()], codes: vec![e.code], ..Default::default() },
    }
}

/// The transaction to send after a passing check: `ours` (as sent), with the listed inputs'
/// signature scripts taken from the wallet's.
pub fn assemble(ours: &Transaction, sign: &[usize], got: &Transaction) -> Transaction {
    let mut tx = ours.clone();
    for &i in sign {
        tx.inputs[i].signature_script = got.inputs[i].signature_script.clone();
    }
    tx.finalize();
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{hashing::sighash_type::SIG_HASH_ALL, sign::sign_input, tx::PopulatedTransaction};
    use lottery::buy::{BuyCase, FUNDING_SK, b_max};

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// A stand-in wallet: parse the JSON as kaspa-wasm would, sign `idx` (or, with `None`,
    /// every input whose script is our P2PK) with `sk`, return Safe JSON.
    fn wallet(json: &str, idx: Option<&[usize]>, sk: &[u8; 32]) -> String {
        let (mut tx, entries, _) = from_json(json).unwrap();
        let entries: Vec<UtxoEntry> = entries.into_iter().map(Option::unwrap).collect();
        let me = lottery::buy::spk_of(sk);
        let list: Vec<usize> = match idx {
            Some(l) => l.to_vec(),
            None => (0..tx.inputs.len()).filter(|&i| entries[i].script_public_key == me).collect(),
        };
        let unsigned = tx.clone();
        for i in list {
            tx.inputs[i].signature_script = sign_input(&PopulatedTransaction::new(&unsigned, entries.clone()), i, sk, SIG_HASH_ALL);
        }
        reserialize(&tx, &entries).unwrap()
    }

    fn built(funding: Vec<u64>) -> (lottery::buy::Final, Vec<usize>) {
        let mut c = BuyCase::new(b_max());
        c.with_ref = true;
        c.funding = funding;
        c.build().finish_unsigned().unwrap()
    }

    /// The check against the transaction as sent (every funding input blank), asking `ask`.
    fn check_json(f: &lottery::buy::Final, ask: &[usize], json: &str) -> Check {
        let (got, ge, _) = from_json(json).unwrap();
        let blank: Vec<usize> = (0..f.tx.inputs.len()).filter(|&i| f.entries[i].covenant_id.is_none()).collect();
        post_sign(&blanked(&f.tx, &blank).unwrap(), &f.entries, ask, &got, &ge)
    }

    #[test]
    fn safe_json_round_trip_keeps_every_field() {
        let (f, idx) = built(vec![100 * lottery::buy::KAS]);
        let json = to_safe_json(&f.tx, &f.entries, &idx).unwrap();
        for k in ["\"computeBudget\"", "\"covenant\":{\"authorizingInput\":0,\"covenantId\"", "\"storageMass\"", "\"covenantId\"", "\"address\":\"kaspatest:"] {
            assert!(json.contains(k), "missing {k}: {json}");
        }
        let (got, ge, form) = from_json(&json).unwrap();
        assert_eq!(form, "safe");
        let c = post_sign(&blanked(&f.tx, &idx).unwrap(), &f.entries, &idx, &got, &ge);
        assert_eq!(c.refusals, vec!["input 1: listed for signing but returned unsigned".to_string()]);
        assert_eq!(got.id(), f.tx.id(), "the JSON keeps the id-relevant fields");
        assert_eq!(got.inputs[0].signature_script, f.tx.inputs[0].signature_script, "covenant sigscript kept");
    }

    #[test]
    fn honest_wallet_passes_and_engine_accepts() {
        let (f, idx) = built(vec![100 * lottery::buy::KAS]);
        let json = to_safe_json(&f.tx, &f.entries, &idx).unwrap();
        for list in [Some(&idx[..]), None] {
            let signed = wallet(&json, list, &FUNDING_SK);
            let c = check_json(&f, &idx, &signed);
            assert!(c.pass(), "{list:?}: {:?}", c.refusals);
            assert_eq!(c.gained, idx);
            let (got, _, _) = from_json(&signed).unwrap();
            let tx = assemble(&blanked(&f.tx, &idx).unwrap(), &idx, &got);
            let fin = lottery::buy::Final { tx, entries: f.entries.clone(), units: f.units.clone(), sim: None };
            fin.run().unwrap();
        }
    }

    /// Spec 8 "Post-sign check": a mock wallet that alters each listed item is refused.
    #[test]
    fn mock_wallet_alterations_are_refused() {
        let (f, idx) = built(vec![60 * lottery::buy::KAS, 50 * lottery::buy::KAS]);
        // Both funding inputs go out blank; ask for input 1 only (the change is the same key).
        let ask = vec![idx[0]];
        let json = to_safe_json(&f.tx, &f.entries, &idx).unwrap();
        let honest = wallet(&json, Some(&ask), &FUNDING_SK);
        assert!(check_json(&f, &ask, &honest).pass());
        let edit = |g: &dyn Fn(&mut serde_json::Value)| {
            let mut v: serde_json::Value = serde_json::from_str(&honest).unwrap();
            g(&mut v);
            v.to_string()
        };
        let other = lottery::buy::spk_of(&[9u8; 32]);
        let other_hex = format!("0000{}", hex(other.script()));
        let cases: Vec<(&str, String, &str)> = vec![
            ("covenant signature script", edit(&|v| v["inputs"][0]["signatureScript"] = "00".into()), "input 0 (not listed, covenant): signature script changed"),
            ("output value", edit(&|v| v["outputs"][1]["value"] = "1".into()), "output 1 value"),
            ("output script", edit(&|v| v["outputs"][2]["scriptPublicKey"] = other_hex.clone().into()), "output 2 script"),
            (
                "extra output",
                edit(&|v| {
                    let o = v["outputs"][3].clone();
                    v["outputs"].as_array_mut().unwrap().push(o)
                }),
                "output count",
            ),
            ("change address", edit(&|v| v["outputs"][3]["scriptPublicKey"] = other_hex.clone().into()), "output 3 script"),
            ("unlisted input's signature", wallet(&json, None, &FUNDING_SK), "input 2 (not listed): signature script changed"),
            ("lock time", edit(&|v| v["lockTime"] = "5".into()), "lock time"),
            ("payload", edit(&|v| v["payload"] = "ab".into()), "payload"),
            ("covenant binding", edit(&|v| v["outputs"][0]["covenant"] = serde_json::Value::Null), "output 0 covenant binding"),
            ("sequence", edit(&|v| v["inputs"][1]["sequence"] = "7".into()), "input 1 sequence"),
            ("compute budget", edit(&|v| v["inputs"][0]["computeBudget"] = 1.into()), "input 0 compute commit"),
        ];
        for (name, j, want) in cases {
            let c = check_json(&f, &ask, &j);
            assert!(!c.pass(), "{name}: accepted");
            assert!(c.refusals.iter().any(|r| r.starts_with(want)), "{name}: refusals {:?} lack `{want}`", c.refusals);
        }
        // Fields never taken from the wallet are notes: the storage mass, an entry.
        let c = check_json(&f, &ask, &edit(&|v| v["storageMass"] = "1".into()));
        assert!(c.pass() && c.notes.iter().any(|n| n.starts_with("storage-mass commit")), "{c:?}");
        let c = check_json(&f, &ask, &edit(&|v| v["inputs"][1]["utxo"]["amount"] = "5".into()));
        assert!(c.pass() && c.notes.iter().any(|n| n.starts_with("input 1 UTXO entry")), "{c:?}");
        // ... and the engine still refuses a signature made over the wallet's own entry.
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v["inputs"][1]["utxo"]["amount"] = "5".into();
        let lied = wallet(&v.to_string(), Some(&ask), &FUNDING_SK);
        let (got, _, _) = from_json(&lied).unwrap();
        let blank: Vec<usize> = idx.clone();
        let tx = assemble(&blanked(&f.tx, &blank).unwrap(), &ask, &got);
        let fin = lottery::buy::Final { tx, entries: f.entries.clone(), units: f.units.clone(), sim: None };
        assert!(fin.run_input(ask[0]).is_err(), "a signature over another amount verifies");
    }
}
