//! Front-end F0 (spec frontend 6.2, 9): the wallet's transaction format and the post-sign
//! check.
//!
//! Format. KasWare's `signPskt({txJsonString, options})` takes and returns a transaction "in
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
use kaspa_consensus_client::{numeric, string::SerializableTransaction as Safe};
use kaspa_consensus_core::tx::{SignableTransaction, Transaction, UtxoEntry};
use kaspa_txscript::extract_script_pub_key_address;

type R<T> = Result<T, String>;

/// `tx` with the signature scripts of `blank` emptied: the transaction as sent to the wallet.
pub fn blanked(tx: &Transaction, blank: &[usize]) -> R<Transaction> {
    let mut t = tx.clone();
    for &i in blank {
        t.inputs.get_mut(i).ok_or(format!("blank input {i} out of range"))?.signature_script = vec![];
    }
    t.finalize();
    Ok(t)
}

/// The Safe JSON of `tx` with `entries`, the signature scripts of `blank` emptied.
pub fn to_safe_json(tx: &Transaction, entries: &[UtxoEntry], blank: &[usize]) -> R<String> {
    let t = blanked(tx, blank)?;
    let mut s = Safe::from_signable_transaction(&SignableTransaction::with_entries(t, entries.to_vec())).map_err(|e| e.to_string())?;
    for inp in &mut s.inputs {
        inp.utxo.address = extract_script_pub_key_address(&inp.utxo.script_public_key, Prefix::Testnet).ok();
    }
    s.serialize_to_json().map_err(|e| e.to_string())
}

/// Parse a wallet's returned transaction: Safe JSON (strings), else the numeric JSON form.
/// Returns the transaction, its entries as stated, and which form parsed.
pub fn from_json(json: &str) -> R<(Transaction, Vec<Option<UtxoEntry>>, &'static str)> {
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

/// Re-serialize a parsed transaction as Safe JSON (the stand-in signer's output).
pub fn reserialize(tx: &Transaction, entries: &[UtxoEntry]) -> R<String> {
    to_safe_json(tx, entries, &[])
}

/// The outcome of the post-sign check.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Check {
    /// Every difference that refuses the transaction (empty = pass).
    pub refusals: Vec<String>,
    /// Inputs whose signature script was empty when sent and is not now.
    pub gained: Vec<usize>,
    /// Inputs whose signature script was non-empty when sent and is empty now.
    pub lost: Vec<usize>,
    /// Differences in fields never taken from the wallet (storage-mass commit, UTXO entries).
    pub notes: Vec<String>,
}

impl Check {
    pub fn pass(&self) -> bool {
        self.refusals.is_empty()
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Compare the returned transaction `got` (with its stated entries) against `ours`, the
/// transaction exactly as sent to the wallet (`blanked`: covenant sigscripts final, the
/// inputs to sign empty), with `entries`; `sign` lists the inputs the wallet was asked to
/// sign. Every input not listed must come back with the signature script it was sent with.
pub fn post_sign(ours: &Transaction, entries: &[UtxoEntry], sign: &[usize], got: &Transaction, got_entries: &[Option<UtxoEntry>]) -> Check {
    let mut c = Check::default();
    let mut notes = vec![];
    let mut r = |s: String| c.refusals.push(s);
    if got.version != ours.version {
        r(format!("version: built {}, returned {}", ours.version, got.version));
    }
    if got.lock_time != ours.lock_time {
        r(format!("lock time: built {}, returned {}", ours.lock_time, got.lock_time));
    }
    if got.subnetwork_id != ours.subnetwork_id {
        r(format!("subnetwork: built {}, returned {}", ours.subnetwork_id, got.subnetwork_id));
    }
    if got.gas != ours.gas {
        r(format!("gas: built {}, returned {}", ours.gas, got.gas));
    }
    if got.payload != ours.payload {
        r(format!("payload: built {} B `{}`, returned {} B `{}`", ours.payload.len(), hex(&ours.payload), got.payload.len(), hex(&got.payload)));
    }
    if got.storage_mass() != ours.storage_mass() {
        notes.push(format!("storage-mass commit: built {}, returned {} (ours is sent)", ours.storage_mass(), got.storage_mass()));
    }
    if got.outputs.len() != ours.outputs.len() {
        r(format!("output count: built {}, returned {}", ours.outputs.len(), got.outputs.len()));
    }
    for (i, (a, b)) in ours.outputs.iter().zip(&got.outputs).enumerate() {
        if a.value != b.value {
            r(format!("output {i} value: built {}, returned {}", a.value, b.value));
        }
        if a.script_public_key != b.script_public_key {
            r(format!(
                "output {i} script: built v{} `{}`, returned v{} `{}`",
                a.script_public_key.version(),
                hex(a.script_public_key.script()),
                b.script_public_key.version(),
                hex(b.script_public_key.script())
            ));
        }
        if a.covenant != b.covenant {
            r(format!("output {i} covenant binding: built {:?}, returned {:?}", a.covenant, b.covenant));
        }
    }
    for (i, b) in got.outputs.iter().enumerate().skip(ours.outputs.len()) {
        r(format!("output {i} extra: value {}, script `{}`", b.value, hex(b.script_public_key.script())));
    }
    if got.inputs.len() != ours.inputs.len() {
        r(format!("input count: built {}, returned {}", ours.inputs.len(), got.inputs.len()));
    }
    for (i, (a, b)) in ours.inputs.iter().zip(&got.inputs).enumerate() {
        if a.previous_outpoint != b.previous_outpoint {
            r(format!("input {i} outpoint: built {}, returned {}", a.previous_outpoint, b.previous_outpoint));
        }
        if a.sequence != b.sequence {
            r(format!("input {i} sequence: built {}, returned {}", a.sequence, b.sequence));
        }
        if a.compute_commit != b.compute_commit {
            r(format!("input {i} compute commit (sig-op count / budget): built {:?}, returned {:?}", a.compute_commit, b.compute_commit));
        }
        let listed = sign.contains(&i);
        let sent_empty = a.signature_script.is_empty();
        let now_empty = b.signature_script.is_empty();
        if sent_empty && !now_empty {
            c.gained.push(i);
        }
        if !sent_empty && now_empty {
            c.lost.push(i);
        }
        if listed {
            if now_empty {
                r(format!("input {i}: listed for signing but returned unsigned"));
            } else if b.signature_script.len() != 66 || b.signature_script[0] != 65 || b.signature_script[65] != 1 {
                r(format!("input {i}: signature script is not OP_DATA_65 <64-byte Schnorr sig || SIG_HASH_ALL>: {} B `{}`", b.signature_script.len(), hex(&b.signature_script)));
            }
        } else if a.signature_script != b.signature_script {
            r(format!(
                "input {i} (not listed{}): signature script changed: built {} B (blake2b {}), returned {} B (blake2b {})",
                if entries.get(i).and_then(|e| e.covenant_id).is_some() { ", covenant" } else { "" },
                a.signature_script.len(),
                short_hash(&a.signature_script),
                b.signature_script.len(),
                short_hash(&b.signature_script)
            ));
        }
        match got_entries.get(i) {
            Some(Some(e)) => {
                if Some(e) != entries.get(i) {
                    notes.push(format!("input {i} UTXO entry: built {:?}, returned {e:?} (ours is used)", entries.get(i)));
                }
            }
            _ => notes.push(format!("input {i}: UTXO entry missing in the returned transaction (ours is used)")),
        }
    }
    for (i, _) in got.inputs.iter().enumerate().skip(ours.inputs.len()) {
        r(format!("input {i} extra"));
    }
    c.notes = notes;
    c
}

fn short_hash(b: &[u8]) -> String {
    hex(&blake2b_simd::Params::new().hash_length(8).hash(b).as_bytes()[..8])
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
