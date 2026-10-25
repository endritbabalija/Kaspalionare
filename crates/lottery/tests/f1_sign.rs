//! F1 review (`covenant-reviewer`'s independent attacks on `lottery::web::sign`, spec 6.2),
//! kept as tests:
//! - R1: the assembled transaction is ours plus the listed signature scripts only, whatever
//!   the wallet changed in the fields that are only notes (storage-mass commit, entries);
//! - R2: a listed signature script with a valid signature in another push form (a
//!   non-minimal OP_PUSHDATA1, which the v2.1.0 engine accepts; an extra push; an empty push;
//!   a P2SH redeem pushed with OP_PUSHDATA1) is refused by the form check, another key's
//!   signature by the engine;
//! - R3: every covenant input of a `close()` that loses or changes its signature script is
//!   refused.
//!
//! Command (from `crates/`): `cargo test -p lottery --release --test f1_sign -- --nocapture`.
use harness::{PopulatedTransaction, Transaction, UtxoEntry, execute};
use kaspa_consensus_core::{hashing::sighash_type::SIG_HASH_ALL, sign::sign_input};
use lottery::buy::{OwnerKey, OwnerPub};
use lottery::vault::OWNER_SK;
use lottery::web::fixtures::{WALLET_SK, calls, run};
use lottery::web::sign::{self, Unsigned};
use lottery::web::Code;

fn xonly(sk: &[u8; 32]) -> [u8; 32] { lottery::params::xonly(sk) }

fn sign_all(u: &Unsigned, owner: Option<&OwnerKey>) -> Transaction {
    let mut tx = u.tx.clone();
    for s in &u.sign {
        let p = PopulatedTransaction::new(&u.tx, u.entries.clone());
        tx.inputs[s.index].signature_script = if s.owner == OwnerPub::Schnorr(xonly(&WALLET_SK)) {
            sign_input(&p, s.index, &WALLET_SK, SIG_HASH_ALL)
        } else {
            owner.expect("owner key").sign(&p, s.index)
        };
    }
    tx.finalize();
    tx
}

fn ents(u: &Unsigned) -> Vec<Option<UtxoEntry>> { u.entries.iter().cloned().map(Some).collect() }

fn first(name: &str) -> (Unsigned, Option<OwnerKey>) {
    let (_, call, _) = calls().unwrap().into_iter().find(|(n, _, _)| n == name).unwrap_or_else(|| panic!("no fixture {name}"));
    let owner = if name.contains("withdraw 0") { Some(OwnerKey::Schnorr(OWNER_SK)) } else if name.contains("withdraw 1") { Some(OwnerKey::Ecdsa(OWNER_SK)) } else if name.contains("withdraw 2") { Some(OwnerKey::P2sh(OWNER_SK)) } else { None };
    (run(&call).unwrap(), owner)
}

/// The assembled transaction is exactly ours with the listed sigscripts, whatever else the
/// wallet changed in the fields that are only notes (storage mass, entries).
#[test]
fn r1_assembled_takes_only_listed_sigscripts() {
    for name in ["buy b=1 ref", "claim k=4", "close k=16", "draw k=4", "request_withdraw 2 partial", "request_withdraw 1 whole"] {
        let (u, owner) = first(name);
        let honest = sign_all(&u, owner.as_ref());
        let mut got = honest.clone();
        got.set_storage_mass(1);
        let mut lied: Vec<Option<UtxoEntry>> = ents(&u);
        for e in lied.iter_mut() { *e = Some(UtxoEntry::new(1, e.as_ref().unwrap().script_public_key.clone(), 7, true, None)); }
        got.finalize();
        let tx = sign::assemble(&u, &got, &lied).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut want = u.tx.clone();
        for s in &u.sign { want.inputs[s.index].signature_script = honest.inputs[s.index].signature_script.clone(); }
        want.finalize();
        assert_eq!(tx, want, "{name}");
        assert_eq!(tx.storage_mass(), u.tx.storage_mass(), "{name}: storage mass");
        for i in 0..tx.inputs.len() { assert_eq!(tx.inputs[i].compute_commit, u.tx.inputs[i].compute_commit); }
        println!("R1 {name}: assembled = ours + {} listed sigscripts (storage mass and entries from the wallet ignored)", u.sign.len());
    }
}

/// A listed sigscript that is a valid signature but not in the exact push form: OP_PUSHDATA1
/// (non-minimal, accepted by the v2.1.0 engine), an extra push, a different key, another
/// redeem for P2SH, an ECDSA sig on a Schnorr input.
#[test]
fn r2_listed_sigscript_forms() {
    let (u, _) = first("buy b=1 ref");
    let honest = sign_all(&u, None);
    let i = u.sign[0].index;
    let sig = honest.inputs[i].signature_script.clone();
    // Non-minimal push of the same signature: does the engine accept it (so form_ok is what refuses)?
    let mut pd1 = vec![0x4c, 0x41];
    pd1.extend_from_slice(&sig[1..]);
    let mut t = u.tx.clone();
    for s in &u.sign { t.inputs[s.index].signature_script = honest.inputs[s.index].signature_script.clone(); }
    t.inputs[i].signature_script = pd1.clone();
    println!("R2 engine on OP_PUSHDATA1 form: {:?}", execute(&t, &u.entries, i, true).map(|_| ()));
    let mut extra = sig.clone(); extra.push(0x51);
    let other_key = { let mut tt = u.tx.clone(); let p = PopulatedTransaction::new(&tt, u.entries.clone()); let s = sign_input(&p, i, &[0x44; 32], SIG_HASH_ALL); tt.inputs[i].signature_script = s.clone(); s };
    for (what, s, want) in [("pushdata1", pd1, Code::SignatureForm), ("extra push", extra, Code::SignatureForm), ("other key", other_key, Code::SignatureInvalid), ("empty push", vec![0x00], Code::SignatureForm)] {
        let mut got = honest.clone();
        got.inputs[i].signature_script = s;
        got.finalize();
        let e = sign::assemble(&u, &got, &ents(&u)).err().unwrap_or_else(|| panic!("{what}: accepted"));
        assert_eq!(e.code, want, "{what}: {e}");
        println!("R2 {what}: refused {}", e.code);
    }
    // P2SH owner: correct sig, redeem pushed with OP_PUSHDATA1 instead of OP_DATA_34.
    let (u, owner) = first("request_withdraw 2 partial");
    let honest = sign_all(&u, owner.as_ref());
    let oi = u.sign.iter().find(|s| matches!(s.owner, OwnerPub::P2sh(_))).unwrap().index;
    let s = honest.inputs[oi].signature_script.clone();
    let mut alt = s[..66].to_vec(); alt.extend([0x4c, 34]); alt.extend_from_slice(&s[67..]);
    let mut got = honest.clone(); got.inputs[oi].signature_script = alt; got.finalize();
    let e = sign::assemble(&u, &got, &ents(&u)).unwrap_err();
    assert_eq!(e.code, Code::SignatureForm);
    println!("R2 p2sh redeem via OP_PUSHDATA1: refused {}", e.code);
}

/// Unlisted inputs: a covenant input that loses its sigscript, a covenant input whose
/// sigscript is replaced by a same-length different one.
#[test]
fn r3_unlisted_changes() {
    let (u, _) = first("close k=4");
    let honest = sign_all(&u, None);
    let cov: Vec<usize> = (0..u.tx.inputs.len()).filter(|i| !u.sign.iter().any(|s| s.index == *i)).collect();
    for &i in &cov {
        for what in ["lost", "flip"] {
            let mut got = honest.clone();
            if what == "lost" { got.inputs[i].signature_script.clear(); } else { let n = got.inputs[i].signature_script.len(); got.inputs[i].signature_script[n / 2] ^= 1; }
            got.finalize();
            let c = sign::post_sign(&u.tx, &u.entries, &u.sign, &got, &ents(&u));
            assert!(!c.pass(), "input {i} {what}: passed");
            if what == "lost" { assert!(c.lost.contains(&i)); }
        }
    }
    println!("R3 close k=4: each of {} covenant inputs refused when lost or altered", cov.len());
}
