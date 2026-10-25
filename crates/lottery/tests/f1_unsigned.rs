//! Front-end F1 part B (spec frontend 6.1 "Signing split from building"): every web-facing
//! case can be built unsigned, and the unsigned transaction is the signed one minus the
//! signatures.
//!
//! For every entry point (buy, claim, return_deposit both paths, close, draw and its void,
//! request_deposit, request_withdraw with every owner class, both partial and whole, settle
//! both kinds), `Built::finish_unsigned()` against `Built::finish()` of the same case:
//! - the inputs to sign are the funding inputs and the owner input, nothing else;
//! - every byte except those inputs' signature scripts is equal: version, lock time,
//!   outputs, outpoints, sequences, compute budgets, storage-mass commit, every covenant
//!   signature script; the signature scripts to sign have the signed ones' sizes; the
//!   transaction ids are equal; the masses and units are equal;
//! - signed outside the library (as a wallet signs: SIG_HASH_ALL over the unsigned
//!   transaction), every input passes the engine under its committed budget.
//!
//! The owner-input budget (the design question of part B): `finish()` budgets an owner input
//! at one signature check and re-budgets it from its signed run; `finish_unsigned()` has no
//! key and budgets it from `owner_signed_units` of its class. `owner_budget_depends_on_the_class_only`
//! signs owner inputs of many random keys of each class and checks the signed run's units
//! equal the class constant and the committed budgets agree.

use harness::{PopulatedTransaction, Transaction, UtxoEntry, budget_for, masses};
use kaspa_consensus_core::{hashing::sighash_type::SIG_HASH_ALL, sign::sign_input};
use lottery::buy::{BuyCase, Built, FUNDING_SK, Final, InKind, KAS, OwnerKey, P2PK_UNITS, Templates, b_max, owner_signed_units, run_script};
use lottery::claim::{ClaimCase, ReturnCase};
use lottery::close::{CloseCase, FundingIn};
use lottery::draw::DrawCase;
use lottery::vault::{DepositCase, OWNER_SK, SettleCase, WithdrawCase};

/// Sign the listed inputs of `tx` as a wallet would (SIG_HASH_ALL over the transaction as
/// given; the sighash does not cover signature scripts): funding inputs with FUNDING_SK, the
/// owner input with its key.
fn sign_like_a_wallet(b: &Built, tx: &Transaction, entries: &[UtxoEntry], idx: &[usize]) -> Transaction {
    let mut out = tx.clone();
    for &i in idx {
        let p = PopulatedTransaction::new(tx, entries.to_vec());
        out.inputs[i].signature_script = match &b.inputs[i].kind {
            InKind::Funding => sign_input(&p, i, &FUNDING_SK, SIG_HASH_ALL),
            InKind::Owner(k) => k.sign(&p, i),
            other => panic!("input {i} listed for signing is not a funding or owner input: {}", kind_name(other)),
        };
    }
    out.finalize();
    out
}

fn kind_name(k: &InKind) -> &'static str {
    match k {
        InKind::Shard(_) => "shard",
        InKind::Funding => "funding",
        InKind::Covenant { .. } => "covenant",
        InKind::Raw(_) => "raw",
        InKind::Entry { .. } => "entry",
        InKind::Owner(_) => "owner",
        InKind::OwnerPub(_) => "owner (public)",
    }
}

/// The comparison of part B for one case.
fn check(name: &str, b: &Built) {
    let signed = b.finish();
    signed.run().unwrap_or_else(|(i, e)| panic!("{name}: the signed fixture fails at input {i}: {e:?}"));
    let (u, idx) = b.finish_unsigned().unwrap_or_else(|e| panic!("{name}: finish_unsigned: {e}"));
    let want: Vec<usize> = b.inputs.iter().enumerate().filter(|(_, i)| matches!(i.kind, InKind::Funding | InKind::Owner(_))).map(|(n, _)| n).collect();
    assert_eq!(idx, want, "{name}: the inputs to sign");
    assert!(!idx.is_empty(), "{name}: something to sign");
    let (a, s) = (&u.tx, &signed.tx);
    assert_eq!((a.version, a.lock_time, &a.subnetwork_id, a.gas, &a.payload), (s.version, s.lock_time, &s.subnetwork_id, s.gas, &s.payload), "{name}: header");
    assert_eq!(a.outputs, s.outputs, "{name}: outputs");
    assert_eq!(a.storage_mass(), s.storage_mass(), "{name}: storage-mass commit");
    assert_eq!(a.inputs.len(), s.inputs.len(), "{name}: input count");
    for (i, (x, y)) in a.inputs.iter().zip(&s.inputs).enumerate() {
        assert_eq!((x.previous_outpoint, x.sequence, x.compute_commit), (y.previous_outpoint, y.sequence, y.compute_commit), "{name}: input {i}");
        if idx.contains(&i) {
            assert_eq!(x.signature_script.len(), y.signature_script.len(), "{name}: input {i}: placeholder size");
        } else {
            assert_eq!(x.signature_script, y.signature_script, "{name}: covenant input {i} sigscript");
        }
    }
    assert_eq!(a.id(), s.id(), "{name}: transaction id");
    assert_eq!(u.units, signed.units, "{name}: units");
    assert_eq!(u.entries, signed.entries, "{name}: entries");
    let (ma, ms) = (masses(a, &u.entries), masses(s, &signed.entries));
    assert_eq!((ma.compute, ma.transient, ma.storage), (ms.compute, ms.transient, ms.storage), "{name}: masses");
    // Signed outside the library, every input passes under its committed budget.
    let tx = sign_like_a_wallet(b, a, &u.entries, &idx);
    let f = Final { tx, entries: u.entries.clone(), units: u.units.clone(), sim: u.sim.clone() };
    f.run().unwrap_or_else(|(i, e)| panic!("{name}: externally signed transaction fails at input {i}: {e:?}"));
    println!("{name}: {} inputs, sign {idx:?}, masses {ma:?}: equal to finish() but the signatures", a.inputs.len());
}

#[test]
fn every_web_case_builds_unsigned_and_equals_the_signed_path() {
    let t = Templates::get(b_max());
    // buy: b = 1; b = 4 with a referral and two funding inputs.
    check("buy b=1", &BuyCase::new(b_max()).build());
    let mut c = BuyCase::new(b_max());
    c.bundle = vec![
        lottery::combo::Combo::new([1, 2, 3, 4, 5], 1),
        lottery::combo::Combo::new([2, 4, 6, 8, 10], 2),
        lottery::combo::Combo::new([3, 7, 12, 19, 28], 4),
        lottery::combo::Combo::new([5, 10, 15, 20, 25], 10),
    ];
    c.with_ref = true;
    c.funding = vec![60 * KAS, 50 * KAS];
    check("buy b=4 ref", &c.build());
    // claim: PAY at K = 4; FINAL at K = 1.
    check("claim K=4", &ClaimCase::new(4).build());
    let mut f = ClaimCase::new(1);
    f.make_final();
    check("claim final K=1", &f.build());
    // return_deposit: path b (the ticket alone) and path a (with the expired pool).
    check("return after", &ReturnCase::after().build());
    check("return with pool", &ReturnCase::with_pool().build());
    // close at K = 4 and K = 16.
    check("close K=4", &CloseCase::new(4).build());
    check("close K=16", &CloseCase::new(16).build());
    // draw and void at K = 4.
    check("draw K=4", &DrawCase::new(4).build());
    check("void K=4", &DrawCase::new_void(4).build());
    // vault: request_deposit; request_withdraw for each owner class, partial and whole, and
    // with a second plain input; settle a deposit and a withdrawal.
    check("request_deposit", &DepositCase::new().build());
    for key in [OwnerKey::Schnorr(OWNER_SK), OwnerKey::Ecdsa(OWNER_SK), OwnerKey::P2sh(OWNER_SK)] {
        let w = WithdrawCase::new_with(&t, key.clone());
        check(&format!("request_withdraw partial {key:?}"), &w.build());
        check(&format!("request_withdraw whole {key:?}"), &w.clone().whole().build());
        let mut two = w.clone();
        two.funding = vec![FundingIn { value: 3 * KAS, daa: 1_100_000 }];
        two.owner_at = 1;
        check(&format!("request_withdraw owner second {key:?}"), &two.build());
    }
    check("settle deposit", &SettleCase::deposit().build());
    check("settle withdrawal", &SettleCase::withdrawal().build());
}

/// The design of part B: an owner input's budget without its key. Its script reads no
/// other input or output and its pushes have fixed sizes, so the signed run's units are a
/// constant of the class. For 24 random keys of each class: the signed run's units equal
/// `owner_signed_units` of the class, and `finish()` (signs, re-runs, re-budgets) commits the
/// budget `finish_unsigned()` commits without the key.
#[test]
fn owner_budget_depends_on_the_class_only() {
    let t = Templates::get(b_max());
    let mut seed: u64 = 0xF1B0_0B5E;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let p2pk_budget = budget_for(P2PK_UNITS);
    for class in 0..3 {
        let mut seen = vec![];
        for _ in 0..24 {
            let mut sk = [0u8; 32];
            for c in sk.chunks_mut(8) {
                c.copy_from_slice(&next().to_le_bytes());
            }
            if secp256k1::SecretKey::from_slice(&sk).is_err() {
                continue;
            }
            let key = match class {
                0 => OwnerKey::Schnorr(sk),
                1 => OwnerKey::Ecdsa(sk),
                _ => OwnerKey::P2sh(sk),
            };
            let w = WithdrawCase::new_with(&t, key.clone());
            let b = w.build();
            let oi = w.owner_input();
            let signed = b.finish();
            let (u, _) = b.finish_unsigned().unwrap();
            // The signed run's units, unlimited.
            let units = run_script(&signed.tx, &signed.entries, oi, false, None).unwrap();
            let class_units = owner_signed_units(&key.public()).unwrap();
            assert_eq!(units, class_units, "class {class}: a key's signed units differ from the class constant");
            assert_eq!(u.tx.inputs[oi].compute_commit, signed.tx.inputs[oi].compute_commit, "class {class}: committed budgets differ");
            let want = if budget_for(units) > p2pk_budget { budget_for(units) } else { p2pk_budget };
            let rule = harness::TransactionInput::new_with_compute_budget(signed.tx.inputs[oi].previous_outpoint, vec![], 0, want).compute_commit;
            assert_eq!(signed.tx.inputs[oi].compute_commit, rule, "class {class}: the signed path's budget rule");
            seen.push(units);
        }
        assert!(seen.len() >= 20, "class {class}: enough valid keys");
        println!("owner class {class}: {} keys, signed units {} (budget {}; one signature check {} = budget {p2pk_budget})", seen.len(), seen[0], budget_for(seen[0]).max(p2pk_budget), P2PK_UNITS);
    }
}

/// F0's refusal is lifted: an owner input no longer stops `finish_unsigned`, and an owner
/// input whose key is not held here (`InKind::OwnerPub`) cannot go through `finish()`.
#[test]
fn owner_pub_cannot_be_signed_here() {
    let w = WithdrawCase::new();
    let mut b = w.build();
    let oi = w.owner_input();
    let InKind::Owner(k) = b.inputs[oi].kind.clone() else { panic!("owner input") };
    b.inputs[oi].kind = InKind::OwnerPub(k.public());
    let (u, idx) = b.finish_unsigned().expect("unsigned with an external owner");
    assert!(idx.contains(&oi));
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| b.finish()));
    let msg = caught.err().and_then(|p| p.downcast_ref::<String>().cloned()).unwrap_or_default();
    assert!(msg.contains("signed outside this library"), "finish() must refuse an OwnerPub input: {msg:?}");
    // The same transaction as with the secret key held here.
    let (v, _) = w.build().finish_unsigned().unwrap();
    assert_eq!(u.tx.id(), v.tx.id());
    assert_eq!(u.tx.inputs[oi].compute_commit, v.tx.inputs[oi].compute_commit);
}
