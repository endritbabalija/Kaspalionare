//! Front-end F1 part C: the builders from chain objects (`lottery::web::cases`).
//!
//! - `chain_object_calls_equal_the_fixture_cases`: every fixture call (`web::fixtures`: the
//!   covenant UTXOs, their state bytes, the purchases and the wallet's coins) builds exactly
//!   `web::build`'s transaction for the fixture case: so the fields `cases` derives from the
//!   chain (reserves, the pot, lock times, trees, the referral decision) are the fixture's.
//! - `untrusted_objects_are_refused`: spec 6.3 / 8 "Untrusted API" on the chain-object path:
//!   a state that is not the UTXO's, a UTXO of another instance, an unconfirmed or coinbase
//!   covenant UTXO, a state of another role, a missing or an extra purchase, a UTXO value the
//!   state cannot account for, and a wrong anchor proof, each refused with its code.
//! - `referral_below_m_ref_is_omitted`: a referrer is named but r < M_REF: no referral
//!   output, the transaction is the one without a referrer (spec 7).

use harness::{Hash, UtxoEntry};
use lottery::web::cases::{self, BuyChoice, Obj};
use lottery::web::fixtures::{Call, calls, run};
use lottery::web::{Code, sign::Unsigned};

fn same(name: &str, got: &Unsigned, want: &Unsigned) {
    assert_eq!(got.tx, want.tx, "{name}: transaction");
    assert_eq!(got.entries, want.entries, "{name}: entries");
    assert_eq!(got.sign, want.sign, "{name}: inputs to sign");
    assert_eq!((got.masses.compute, got.masses.transient, got.masses.storage, got.masses.priority), (want.masses.compute, want.masses.transient, want.masses.storage, want.masses.priority), "{name}: masses");
    assert_eq!(got.fee, want.fee, "{name}: fee");
    assert_eq!(got.units, want.units, "{name}: units");
}

#[test]
fn chain_object_calls_equal_the_fixture_cases() {
    let all = calls().expect("fixture calls");
    assert!(all.len() >= 18, "{} calls", all.len());
    for (name, call, want) in &all {
        let got = run(call).unwrap_or_else(|e| panic!("{name}: {e}"));
        same(name, &got, want);
        println!("{name}: chain objects = fixture case, {} inputs, sign {:?}, fee {}", got.tx.inputs.len(), got.sign_indices(), got.fee);
    }
}

fn code_of(name: &str, call: &Call) -> Code {
    match run(call) {
        Ok(_) => panic!("{name}: accepted"),
        Err(e) => {
            println!("{name}: refused, {e}");
            e.code
        }
    }
}

fn find(name: &str) -> Call {
    calls().unwrap().into_iter().find(|(n, _, _)| n == name).map(|(_, c, _)| c).unwrap_or_else(|| panic!("no call {name}"))
}

fn with_entry(o: &Obj, f: impl FnOnce(&mut UtxoEntry)) -> Obj {
    let mut o = o.clone();
    f(&mut o.entry);
    o
}

#[test]
fn untrusted_objects_are_refused() {
    let Call::Buy { cov, shard, purchases, choice, funds } = find("buy b=1 ref") else { unreachable!() };
    let buy = |shard: &Obj, purchases: &[u8]| Call::Buy { cov, shard: shard.clone(), purchases: purchases.to_vec(), choice: choice.clone(), funds: funds.clone() };
    // A wrong state: one flipped byte of the root.
    let mut bad = shard.clone();
    let n = bad.state.len();
    bad.state[n - 10] ^= 1;
    assert!(matches!(code_of("wrong state", &buy(&bad, &purchases)), Code::ChainMismatch | Code::NotTemplate | Code::NotCanonical));
    // A UTXO of another instance.
    let other = with_entry(&shard, |e| e.covenant_id = Some(Hash::from_bytes([7; 32])));
    assert_eq!(code_of("other instance", &buy(&other, &purchases)), Code::ChainMismatch);
    // No covenant ID at all.
    let none = with_entry(&shard, |e| e.covenant_id = None);
    assert_eq!(code_of("no covenant ID", &buy(&none, &purchases)), Code::ChainMismatch);
    // Unconfirmed and coinbase.
    let unconf = with_entry(&shard, |e| e.block_daa_score = u64::MAX);
    assert_eq!(code_of("unconfirmed", &buy(&unconf, &purchases)), Code::CoinUnconfirmed);
    let cb = with_entry(&shard, |e| e.is_coinbase = true);
    assert_eq!(code_of("coinbase", &buy(&cb, &purchases)), Code::ChainMismatch);
    // A value the state cannot account for (S - P_R + R with R <= 0).
    let low = with_entry(&shard, |e| e.amount = 0);
    assert_eq!(code_of("shard value below S - P_R", &buy(&low, &purchases)), Code::ChainMismatch);
    // The value comes from the node, not the indexer: a shard UTXO one sompi richer is a
    // different, valid UTXO (its reserve is one more; shard.sil derives R from its value).
    // The build spends exactly that entry and the shard's successor carries the extra sompi.
    let off = with_entry(&shard, |e| e.amount += 1);
    let a = run(&buy(&shard, &purchases)).unwrap();
    let b = run(&buy(&off, &purchases)).unwrap();
    assert_eq!(b.entries[0], off.entry, "the given UTXO is the one spent");
    assert_eq!(b.tx.outputs[0].value, a.tx.outputs[0].value + 1, "the successor keeps the extra sompi");
    // An extra purchase (the indexer adds one): the tree no longer gives the root.
    let mut extra = purchases.clone();
    extra.extend_from_slice(&[1, 2, 3, 4, 5, 1]);
    assert_eq!(code_of("extra purchase", &buy(&shard, &extra)), Code::TreeMismatch);

    // A state of another role at a UTXO (a ticket's bytes as a shard).
    let Call::Claim { cov: ccov, ticket, pools, purchases: cp, idx, funds: cf } = find("claim k=4") else { unreachable!() };
    let as_shard = Obj { state: ticket.state.clone(), ..shard.clone() };
    let c = code_of("ticket bytes as a shard", &buy(&as_shard, &purchases));
    assert!(matches!(c, Code::NotTemplate | Code::ChainMismatch | Code::NotCanonical), "{c}");
    // Claim: a missing purchase in one shard, then an extra one.
    let claim = |purchases: Vec<Vec<u8>>, pools: Vec<Obj>| Call::Claim { cov: ccov, ticket: ticket.clone(), pools, purchases, idx, funds: cf.clone() };
    let mut missing = cp.clone();
    let s = missing.iter().position(|p| !p.is_empty()).expect("a shard with purchases");
    let len = missing[s].len();
    missing[s].truncate(len - 6);
    assert_eq!(code_of("claim, a missing purchase", &claim(missing, pools.clone())), Code::TreeMismatch);
    let mut more = cp.clone();
    more[0].extend_from_slice(&[1, 2, 3, 4, 5, 1]);
    assert_eq!(code_of("claim, an extra purchase", &claim(more, pools.clone())), Code::TreeMismatch);
    // Claim: no live pool given.
    assert_eq!(code_of("claim, no pool", &claim(cp.clone(), vec![])), Code::StoreIncomplete);
    // Claim: the pool's state replaced by the ticket's (role and script mismatch).
    let wrong_pool = Obj { state: ticket.state.clone(), ..pools[0].clone() };
    let c = code_of("claim, a ticket as the pool", &claim(cp.clone(), vec![wrong_pool]));
    assert!(matches!(c, Code::NotTemplate | Code::ChainMismatch | Code::NotCanonical), "{c}");

    // Draw: a wrong anchor proof (one byte of X.hash flipped: the parent commitment no longer
    // matches) is refused off-chain; a truncated one is refused as malformed.
    let Call::Draw { cov: dcov, drawer, marker, vault, controller, absorbed, purchases: dp, proof, funder, funds: df } = find("draw k=4") else { unreachable!() };
    let draw = |proof: Option<Vec<u8>>, purchases: Vec<Vec<u8>>| Call::Draw {
        cov: dcov,
        drawer: drawer.clone(),
        marker: marker.clone(),
        vault: vault.clone(),
        controller: controller.clone(),
        absorbed: absorbed.clone(),
        purchases,
        proof,
        funder,
        funds: df.clone(),
    };
    let mut p = proof.clone().unwrap();
    let last = p.len() - 1;
    p[last] ^= 1;
    assert_eq!(code_of("draw, a doctored anchor proof (P's fields)", &draw(Some(p), dp.clone())), Code::AnchorInvalid);
    // X's own hash is not checkable off-chain (only OpChainblockSeqCommit on the node knows
    // the chain): a proof with another X.hash builds here, and the node refuses it in draw()
    // check 8 (spec 8 "a wrong anchor proof (refused by the node)"; M7 attack tests).
    let mut x = proof.clone().unwrap();
    x[0] ^= 1;
    assert!(run(&draw(Some(x), dp.clone())).is_ok(), "X.hash is the node's check");
    let short = proof.clone().unwrap()[..100].to_vec();
    assert_eq!(code_of("draw, a truncated proof", &draw(Some(short), dp.clone())), Code::AnchorInvalid);
    // Draw: the purchases of one shard missing (k lists expected).
    assert_eq!(code_of("draw, a shard list missing", &draw(proof.clone(), dp[1..].to_vec())), Code::StoreIncomplete);
    // Draw: an extra purchase changes a root.
    let mut more = dp.clone();
    more[0].extend_from_slice(&[1, 2, 3, 4, 5, 1]);
    assert_eq!(code_of("draw, an extra purchase", &draw(proof.clone(), more)), Code::TreeMismatch);

    // Vault: a value below total_assets + pend_dep (no reserve left).
    let Call::Deposit { cov: vcov, vault: v, d, owner, funds: vf } = find("request_deposit") else { unreachable!() };
    let low = with_entry(&v, |e| e.amount = 1);
    assert_eq!(code_of("vault value below its accounted part", &Call::Deposit { cov: vcov, vault: low, d, owner, funds: vf.clone() }), Code::ChainMismatch);
    // Settle: a record value the state cannot account for.
    let Call::Settle { cov: scov, request, record, funds: sf } = find("settle deposit") else { unreachable!() };
    let low = with_entry(&record, |e| e.amount = 0);
    assert_eq!(code_of("record value below its accounted part", &Call::Settle { cov: scov, request, record: low, funds: sf }), Code::ChainMismatch);
}

#[test]
fn referral_below_m_ref_is_omitted() {
    let Call::Buy { cov, shard, purchases, choice, funds } = find("buy b=1 ref") else { unreachable!() };
    let s = cases::shard(&shard, &cov).unwrap();
    // The fixture shard pays r = 0.1 KAS = M_REF for b = 1: paid.
    assert_eq!(cases::referral(&s, 1).unwrap(), (lottery::buy::M_REF, true));
    let with = run(&Call::Buy { cov, shard: shard.clone(), purchases: purchases.clone(), choice: choice.clone(), funds: funds.clone() }).unwrap();
    let without = run(&Call::Buy { cov, shard: shard.clone(), purchases: purchases.clone(), choice: BuyChoice { referrer: None, ..choice.clone() }, funds: funds.clone() }).unwrap();
    assert_eq!(with.tx.outputs.len(), without.tx.outputs.len() + 1, "the referral output");
    // A shard with s_R lowered so that r < M_REF: the referrer is named, the output omitted.
    let mut low = s.clone();
    low.s_r = 999;
    assert_eq!(cases::referral(&low, 1).unwrap().1, false);
}
