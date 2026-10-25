//! The signing split (spec frontend 6.1 "Signing split from building", 6.2 "The post-sign
//! check"; F1 part B). Moved here from `tn10/src/pskt.rs` (F0) so the web crate uses the
//! same check; tn10 keeps its Safe JSON and calls this.
//!
//! 1. `unsigned(&Built)`: `Built::finish_unsigned` (budgets and the storage-mass commit of
//!    the signed transaction, every covenant sigscript final), then the inputs to sign
//!    emptied (the kaspa-wasm convention for unsigned inputs), with the inputs to sign and
//!    the owner each one's script names (`SignInput`), the masses and the fee.
//! 2. The wallet signs the listed inputs (SIG_HASH_ALL).
//! 3. `assemble(&Unsigned, returned)`: `post_sign` (refuses any change the spec lists),
//!    then OUR transaction with only the listed inputs' signature scripts taken from the
//!    wallet's, then the engine on each listed input against OUR entries and committed
//!    budgets (a signature over another entry, mass or output fails here).

use harness::{Masses, Transaction, UtxoEntry, execute};

use super::{Code, Error, Result, WithCode, ensure};
use crate::buy::{Built, OwnerPub, P2PK_SIG_LEN};

/// SIG_HASH_ALL (rusty-kaspa v2.1.0 `consensus/core/src/hashing/sighash_type.rs`).
const SIG_HASH_ALL: u8 = 0x01;

/// An input the wallet must sign, and the owner its UTXO script names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignInput {
    pub index: usize,
    pub owner: OwnerPub,
}

impl SignInput {
    /// The signature script's form, checked by `post_sign`: `OP_DATA_65 <64-byte signature>
    /// SIG_HASH_ALL` (Schnorr, or the compact ECDSA signature for an ECDSA key; rusty-kaspa
    /// v2.1.0 `consensus/core/src/sign.rs:167-177`), and for a P2SH owner that push followed
    /// by `OP_DATA_34 <the 34-byte redeem>` (the Schnorr P2PK script of the key, D67).
    pub fn form_ok(&self, s: &[u8]) -> bool {
        let sig_ok = s.len() >= P2PK_SIG_LEN && s[0] == 65 && s[65] == SIG_HASH_ALL;
        match &self.owner {
            OwnerPub::Schnorr(_) | OwnerPub::Ecdsa(_) => sig_ok && s.len() == P2PK_SIG_LEN,
            OwnerPub::P2sh(_) => {
                let redeem = self.owner.redeem().unwrap_or_default();
                sig_ok && s.len() == P2PK_SIG_LEN + 1 + redeem.len() && s[P2PK_SIG_LEN] as usize == redeem.len() && s[P2PK_SIG_LEN + 1..] == redeem[..]
            }
        }
    }

    fn form_name(&self) -> &'static str {
        match self.owner {
            OwnerPub::Schnorr(_) => "OP_DATA_65 <64-byte Schnorr sig || SIG_HASH_ALL>",
            OwnerPub::Ecdsa(_) => "OP_DATA_65 <64-byte compact ECDSA sig || SIG_HASH_ALL>",
            OwnerPub::P2sh(_) => "OP_DATA_65 <64-byte Schnorr sig || SIG_HASH_ALL> OP_DATA_34 <redeem>",
        }
    }
}

/// The inputs `idx` of a transaction whose entries are `entries`, signed by the P2PK key
/// each entry's script names (tn10's F0 probe lists funding inputs by index only).
pub fn signers_for(entries: &[UtxoEntry], idx: &[usize]) -> Result<Vec<SignInput>> {
    idx.iter()
        .map(|&i| {
            let e = entries.get(i).ok_or_else(|| Error::new(Code::InputRange, format!("input {i} out of range")))?;
            let s = e.script_public_key.script();
            let owner = match (e.script_public_key.version(), s.len(), s.first(), s.last()) {
                (0, 34, Some(0x20), Some(0xac)) => OwnerPub::Schnorr(s[1..33].try_into().map_err(|_| Error::new(Code::WalletScript, "key"))?),
                (0, 35, Some(0x21), Some(0xab)) => OwnerPub::Ecdsa(s[1..34].try_into().map_err(|_| Error::new(Code::WalletScript, "key"))?),
                _ => return Err(Error::new(Code::WalletScript, format!("input {i}: not a P2PK script"))),
            };
            Ok(SignInput { index: i, owner })
        })
        .collect()
}

/// An unsigned transaction ready for the wallet (spec 6.1).
#[derive(Clone, Debug)]
pub struct Unsigned {
    /// The transaction as sent to the wallet: every covenant input's signature script final,
    /// the inputs to sign with an empty signature script; budgets and the storage-mass
    /// commit are those of the signed transaction.
    pub tx: Transaction,
    /// The UTXO entries in input order (the wallet signs over them; the engine checks
    /// against them).
    pub entries: Vec<UtxoEntry>,
    /// The inputs the wallet must sign, in input order.
    pub sign: Vec<SignInput>,
    /// The masses of the signed transaction.
    pub masses: Masses,
    /// The fee (inputs - outputs).
    pub fee: u64,
    /// The script units of each input's unlimited run (the signed ones: what they commit).
    pub units: Vec<u64>,
}

impl Unsigned {
    pub fn sign_indices(&self) -> Vec<usize> {
        self.sign.iter().map(|s| s.index).collect()
    }
}

/// `tx` with the signature scripts of `blank` emptied (the transaction as sent).
pub fn blanked(tx: &Transaction, blank: &[usize]) -> Result<Transaction> {
    let mut t = tx.clone();
    for &i in blank {
        t.inputs.get_mut(i).ok_or_else(|| Error::new(Code::InputRange, format!("blank input {i} out of range")))?.signature_script = vec![];
    }
    t.finalize();
    Ok(t)
}

/// Step 1: the unsigned transaction of a built one. Every input not to be signed (each
/// covenant input) must already pass in the local engine under its committed budget
/// (`Code::Engine` otherwise), as the F0 probe required before anything reached a wallet.
pub fn unsigned(b: &Built) -> Result<Unsigned> {
    // No zero-value UTXO exists (consensus refuses zero-value outputs), and the storage-mass
    // calculator divides by input values on its relaxed path (mod.rs:489-494).
    if let Some(i) = b.inputs.iter().position(|i| i.entry.amount == 0) {
        return Err(Error::new(Code::InputRange, format!("input {i} has value 0")));
    }
    let sign: Vec<SignInput> = b.external_signers().code(Code::WalletScript)?.into_iter().map(|(index, owner)| SignInput { index, owner }).collect();
    let (f, _) = b.finish_unsigned().code(Code::Encode)?;
    for i in (0..f.tx.inputs.len()).filter(|i| !sign.iter().any(|s| s.index == *i)) {
        f.run_input(i).map_err(|e| Error::new(Code::Engine, format!("covenant input {i}: {e}")))?;
    }
    let masses = harness::try_masses(&f.tx, &f.entries).code(Code::Mass)?;
    let fee = f.try_fee().code(Code::FundingShort)?;
    let idx: Vec<usize> = sign.iter().map(|s| s.index).collect();
    Ok(Unsigned { tx: blanked(&f.tx, &idx)?, entries: f.entries, sign, masses, fee, units: f.units })
}

/// The outcome of the post-sign check (spec 6.2).
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Check {
    /// Every difference that refuses the transaction (empty = pass).
    pub refusals: Vec<String>,
    /// The code of each refusal (same order).
    pub codes: Vec<Code>,
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

    /// The first refusal as an error (`None` when the check passes).
    pub fn error(&self) -> Option<Error> {
        Some(Error::new(*self.codes.first()?, self.refusals.join("; ")))
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn short_hash(b: &[u8]) -> String {
    hex(&blake2b_simd::Params::new().hash_length(8).hash(b).as_bytes()[..8])
}

/// Spec 6.2: compare the returned transaction `got` (with its stated entries) against
/// `ours`, the transaction exactly as sent (the inputs to sign empty), with `entries`.
/// Refused if the version, lock time, subnetwork, gas or payload differs; if the output
/// count differs or any output's value, script or covenant binding; if the input count
/// differs or any input's outpoint, sequence or compute commit (v1's form of the sig-op
/// count); if any input not listed changed its signature script (every covenant input is
/// one); if a listed input came back unsigned, or not in its `SignInput::form_ok` form. A
/// different storage-mass commit or UTXO entry is a note: neither is taken from the wallet
/// (`assemble` keeps ours and the engine checks the signatures against our entries).
pub fn post_sign(ours: &Transaction, entries: &[UtxoEntry], sign: &[SignInput], got: &Transaction, got_entries: &[Option<UtxoEntry>]) -> Check {
    let mut c = Check::default();
    let mut notes = vec![];
    let mut refusals: Vec<(Code, String)> = vec![];
    let mut r = |code: Code, s: String| refusals.push((code, s));
    let changed = Code::SignedChanged;
    if got.version != ours.version {
        r(changed, format!("version: built {}, returned {}", ours.version, got.version));
    }
    if got.lock_time != ours.lock_time {
        r(changed, format!("lock time: built {}, returned {}", ours.lock_time, got.lock_time));
    }
    if got.subnetwork_id != ours.subnetwork_id {
        r(changed, format!("subnetwork: built {}, returned {}", ours.subnetwork_id, got.subnetwork_id));
    }
    if got.gas != ours.gas {
        r(changed, format!("gas: built {}, returned {}", ours.gas, got.gas));
    }
    if got.payload != ours.payload {
        r(changed, format!("payload: built {} B `{}`, returned {} B `{}`", ours.payload.len(), hex(&ours.payload), got.payload.len(), hex(&got.payload)));
    }
    if got.storage_mass() != ours.storage_mass() {
        notes.push(format!("storage-mass commit: built {}, returned {} (ours is sent)", ours.storage_mass(), got.storage_mass()));
    }
    if got.outputs.len() != ours.outputs.len() {
        r(changed, format!("output count: built {}, returned {}", ours.outputs.len(), got.outputs.len()));
    }
    for (i, (a, b)) in ours.outputs.iter().zip(&got.outputs).enumerate() {
        if a.value != b.value {
            r(changed, format!("output {i} value: built {}, returned {}", a.value, b.value));
        }
        if a.script_public_key != b.script_public_key {
            r(
                changed,
                format!(
                    "output {i} script: built v{} `{}`, returned v{} `{}`",
                    a.script_public_key.version(),
                    hex(a.script_public_key.script()),
                    b.script_public_key.version(),
                    hex(b.script_public_key.script())
                ),
            );
        }
        if a.covenant != b.covenant {
            r(changed, format!("output {i} covenant binding: built {:?}, returned {:?}", a.covenant, b.covenant));
        }
    }
    for (i, b) in got.outputs.iter().enumerate().skip(ours.outputs.len()) {
        r(changed, format!("output {i} extra: value {}, script `{}`", b.value, hex(b.script_public_key.script())));
    }
    if got.inputs.len() != ours.inputs.len() {
        r(changed, format!("input count: built {}, returned {}", ours.inputs.len(), got.inputs.len()));
    }
    for (i, (a, b)) in ours.inputs.iter().zip(&got.inputs).enumerate() {
        if a.previous_outpoint != b.previous_outpoint {
            r(changed, format!("input {i} outpoint: built {}, returned {}", a.previous_outpoint, b.previous_outpoint));
        }
        if a.sequence != b.sequence {
            r(changed, format!("input {i} sequence: built {}, returned {}", a.sequence, b.sequence));
        }
        if a.compute_commit != b.compute_commit {
            r(changed, format!("input {i} compute commit (sig-op count / budget): built {:?}, returned {:?}", a.compute_commit, b.compute_commit));
        }
        let listed = sign.iter().find(|s| s.index == i);
        let sent_empty = a.signature_script.is_empty();
        let now_empty = b.signature_script.is_empty();
        if sent_empty && !now_empty {
            c.gained.push(i);
        }
        if !sent_empty && now_empty {
            c.lost.push(i);
        }
        match listed {
            Some(s) => {
                if now_empty {
                    r(Code::SignedMissing, format!("input {i}: listed for signing but returned unsigned"));
                } else if !s.form_ok(&b.signature_script) {
                    r(
                        Code::SignatureForm,
                        format!("input {i}: signature script is not {}: {} B `{}`", s.form_name(), b.signature_script.len(), hex(&b.signature_script)),
                    );
                }
            }
            None if a.signature_script != b.signature_script => r(
                changed,
                format!(
                    "input {i} (not listed{}): signature script changed: built {} B (blake2b {}), returned {} B (blake2b {})",
                    if entries.get(i).and_then(|e| e.covenant_id).is_some() { ", covenant" } else { "" },
                    a.signature_script.len(),
                    short_hash(&a.signature_script),
                    b.signature_script.len(),
                    short_hash(&b.signature_script)
                ),
            ),
            None => {}
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
        r(changed, format!("input {i} extra"));
    }
    c.notes = notes;
    (c.codes, c.refusals) = refusals.into_iter().unzip();
    c
}

/// `ours` with the listed inputs' signature scripts taken from `got` (no checks; `assemble`
/// checks first).
pub fn with_signatures(ours: &Transaction, sign: &[usize], got: &Transaction) -> Result<Transaction> {
    let mut tx = ours.clone();
    for &i in sign {
        let s = got.inputs.get(i).ok_or_else(|| Error::new(Code::SignedChanged, format!("input {i} missing in the returned transaction")))?;
        tx.inputs.get_mut(i).ok_or_else(|| Error::new(Code::InputRange, format!("input {i} out of range")))?.signature_script = s.signature_script.clone();
    }
    tx.finalize();
    Ok(tx)
}

/// Step 3: the final transaction from `u` and the wallet's returned transaction `got`
/// (with its stated entries): the post-sign check, then our transaction with only the
/// listed signature scripts from `got`, then the engine on every listed input under its
/// committed budget against our entries. The result is ready to submit.
pub fn assemble(u: &Unsigned, got: &Transaction, got_entries: &[Option<UtxoEntry>]) -> Result<Transaction> {
    let check = post_sign(&u.tx, &u.entries, &u.sign, got, got_entries);
    if let Some(e) = check.error() {
        return Err(e);
    }
    let tx = with_signatures(&u.tx, &u.sign_indices(), got)?;
    // The signature scripts have the placeholder sizes, so the masses are the ones built.
    let m = harness::try_masses(&tx, &u.entries).code(Code::Mass)?;
    ensure((m.compute, m.transient, m.storage) == (u.masses.compute, u.masses.transient, u.masses.storage), Code::SignatureForm, || {
        format!("signed masses {m:?} differ from the built {:?}", u.masses)
    })?;
    for s in &u.sign {
        execute(&tx, &u.entries, s.index, true).map_err(|e| Error::new(Code::SignatureInvalid, format!("input {}: {e}", s.index)))?;
    }
    Ok(tx)
}
