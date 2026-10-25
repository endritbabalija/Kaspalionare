//! Front-end F0 (spec frontend 6.1, "signing split from building"): `Built::finish_unsigned`
//! is `finish()` without the funding signatures.
//! - Every byte except the funding inputs' signature scripts equals `finish()`'s (inputs,
//!   budgets, covenant sigscripts, outputs, storage-mass commit, lock time); the funding
//!   inputs carry the 66-byte placeholder, so the masses are the signed transaction's.
//! - The returned indices are exactly the funding inputs.
//! - Signing those indices with the funding key (SIG_HASH_ALL) gives a transaction that every
//!   input accepts under its committed budget, as `finish()`'s does.

use kaspa_consensus_core::{hashing::sighash_type::SIG_HASH_ALL, sign::sign_input, tx::PopulatedTransaction};
use lottery::buy::{BuyCase, Built, FUNDING_SK, Final, P2PK_SIG_LEN, b_max};
use lottery::close::CloseCase;

fn strip(f: &Final, idx: &[usize]) -> kaspa_consensus_core::tx::Transaction {
    let mut tx = f.tx.clone();
    for &i in idx {
        tx.inputs[i].signature_script = vec![0; P2PK_SIG_LEN];
    }
    tx
}

fn check(name: &str, b: &Built, want: &[usize]) {
    let signed = b.finish();
    let (unsigned, idx) = b.finish_unsigned().unwrap();
    assert_eq!(idx, want, "{name}: funding indices");
    assert_eq!(strip(&signed, &idx), unsigned.tx, "{name}: unsigned != finish() outside the funding sigscripts");
    assert_eq!(signed.entries, unsigned.entries, "{name}: entries");
    assert_eq!(signed.units, unsigned.units, "{name}: units");
    assert_eq!(format!("{:?}", harness::masses(&signed.tx, &signed.entries)), format!("{:?}", harness::masses(&unsigned.tx, &unsigned.entries)), "{name}: masses");
    for &i in &idx {
        assert_eq!(unsigned.tx.inputs[i].signature_script, vec![0; P2PK_SIG_LEN], "{name}: placeholder at {i}");
    }
    // The unsigned transaction fails only at the funding inputs.
    for (i, r) in unsigned.run_all().iter().enumerate() {
        assert_eq!(r.is_err(), idx.contains(&i), "{name}: unsigned input {i}: {r:?}");
    }
    // Signed externally (as a wallet would) it passes everywhere.
    let mut tx = unsigned.tx.clone();
    for &i in &idx {
        tx.inputs[i].signature_script = sign_input(&PopulatedTransaction::new(&unsigned.tx, unsigned.entries.clone()), i, &FUNDING_SK, SIG_HASH_ALL);
    }
    tx.finalize();
    assert_eq!(tx.id(), signed.tx.id(), "{name}: the id excludes signature scripts");
    let ext = Final { tx, entries: unsigned.entries.clone(), units: unsigned.units.clone(), sim: None };
    ext.run().unwrap_or_else(|(i, e)| panic!("{name}: externally signed input {i}: {e:?}"));
    signed.run().unwrap_or_else(|(i, e)| panic!("{name}: finish() input {i}: {e:?}"));
}

#[test]
fn buy_unsigned_is_finish_without_signatures() {
    let mut c = BuyCase::new(b_max());
    c.funding = vec![60 * lottery::buy::KAS, 50 * lottery::buy::KAS];
    check("buy, two funding inputs", &c.build(), &[1, 2]);
    let mut c = BuyCase::new(b_max());
    c.with_ref = true;
    check("buy with referral", &c.build(), &[1]);
}

#[test]
fn close_unsigned_is_finish_without_signatures() {
    let c = CloseCase::new(16);
    check("close K = 16", &c.build(), &[18]);
}
