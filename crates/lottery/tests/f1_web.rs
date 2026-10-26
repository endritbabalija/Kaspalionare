//! Front-end F1 part B: the web-facing layer (`lottery::web`) end to end, natively.
//!
//! - `web_builders_equal_native_and_assemble`: for every entry point, the web builder
//!   (wallet coins, change script, placeholder key replaced) gives the native builder's
//!   transaction byte for byte when the native case is funded by the same key; a stand-in
//!   wallet signs the listed inputs; `sign::assemble` passes it and every input passes the
//!   engine.
//! - `post_sign_refuses_every_wallet_alteration`: spec 8 "Post-sign check": a mock wallet
//!   that alters each listed item is refused, each with its code.
//! - `guards_refuse_with_their_codes`: the builders' input checks, one refusal per code.
//! - `untrusted_api_mismatches_are_refused`: spec 8 "Untrusted API" at the library level (a
//!   wrong state, a wrong tree, a missing or extra purchase, a wrong winning number, a wrong
//!   anchor proof).
//! - `error_codes_are_stable`: the code list, numbers and names are pinned.

use harness::{PopulatedTransaction, ScriptPublicKey, Transaction, TransactionOutpoint, UtxoEntry, outpoint};
use kaspa_consensus_core::{hashing::sighash_type::SIG_HASH_ALL, sign::sign_input};
use lottery::anchor::AnchorProof;
use lottery::buy::{BuyCase, Built, Final, KAS, OwnerKey, OwnerPub, Templates, b_max, spk_of};
use lottery::claim::{ClaimCase, ReturnCase};
use lottery::close::{CloseCase, FundingIn};
use lottery::draw::DrawCase;
use lottery::index::{Role, State};
use lottery::vault::{DepositCase, OWNER_SK, SettleCase, WithdrawCase};
use lottery::web::build::{self, Coin, Funds, OwnerCoin};
use lottery::web::sign::{self, Unsigned};
use lottery::web::{Code, verify};

const WALLET_SK: [u8; 32] = [0x21; 32];

fn wallet_spk() -> ScriptPublicKey {
    spk_of(&WALLET_SK)
}

/// The wallet's coins for these (value, DAA) pairs, outpoints numbered from `first`.
fn coins(v: &[(u64, u64)], first: u32) -> Vec<Coin> {
    v.iter().enumerate().map(|(n, &(value, daa))| Coin { outpoint: outpoint(first + n as u32), entry: UtxoEntry::new(value, wallet_spk(), daa, false, None) }).collect()
}

fn funds(v: &[(u64, u64)], first: u32, fee: u64) -> Funds {
    Funds { coins: coins(v, first), change: wallet_spk(), fee }
}

fn ops(n: u32) -> Vec<TransactionOutpoint> {
    (0..n).map(outpoint).collect()
}

/// A stand-in wallet: signs each listed input with the key of its script (the wallet key,
/// or the owner key for an owner input).
fn wallet_sign(u: &Unsigned, owner: Option<&OwnerKey>) -> Transaction {
    let mut tx = u.tx.clone();
    for s in &u.sign {
        let p = PopulatedTransaction::new(&u.tx, u.entries.clone());
        tx.inputs[s.index].signature_script = match owner {
            _ if s.owner == OwnerPub::Schnorr(xonly(&WALLET_SK)) => sign_input(&p, s.index, &WALLET_SK, SIG_HASH_ALL),
            Some(k) if k.public() == s.owner => k.sign(&p, s.index),
            _ => panic!("no key for input {}", s.index),
        };
    }
    tx.finalize();
    tx
}

fn xonly(sk: &[u8; 32]) -> [u8; 32] {
    lottery::params::xonly(sk)
}

fn entries_of(u: &Unsigned) -> Vec<Option<UtxoEntry>> {
    u.entries.iter().cloned().map(Some).collect()
}

/// The web result against the native one (`native` funded by the wallet key), then signed
/// by the stand-in wallet and assembled; every input passes.
fn equal_and_assemble(name: &str, u: &Unsigned, native: &Built, owner: Option<&OwnerKey>) {
    let (n, idx) = native.finish_unsigned().unwrap_or_else(|e| panic!("{name}: native: {e}"));
    assert_eq!(u.sign_indices(), idx, "{name}: the inputs to sign");
    let n_sent = sign::blanked(&n.tx, &idx).unwrap();
    assert_eq!(u.tx, n_sent, "{name}: the web transaction differs from the native builder's");
    assert_eq!(u.tx.storage_mass(), n_sent.storage_mass(), "{name}: storage-mass commit");
    assert_eq!(u.entries, n.entries, "{name}: entries");
    assert_eq!(u.units, n.units, "{name}: units");
    let got = wallet_sign(u, owner);
    let tx = sign::assemble(u, &got, &entries_of(u)).unwrap_or_else(|e| panic!("{name}: assemble: {e}"));
    let f = Final { tx, entries: u.entries.clone(), units: u.units.clone(), sim: native.sim.clone() };
    f.run().unwrap_or_else(|(i, e)| panic!("{name}: assembled transaction fails at input {i}: {e:?}"));
    println!("{name}: web = native, {} inputs, sign {idx:?}, fee {}, masses {:?}", u.tx.inputs.len(), u.fee, u.masses);
}

fn proofed(mut c: DrawCase) -> DrawCase {
    let p = AnchorProof::from_sim(&c.chain(), c.d() as u64);
    c.x_hash = p.x_hash();
    c.proof = Some(p);
    c
}

#[test]
fn web_builders_equal_native_and_assemble() {
    let t = Templates::get(b_max());
    // buy (funding entries carry DAA 0 in the native builder).
    let mut c = BuyCase::new(b_max());
    c.with_ref = true;
    c.funding_sk = WALLET_SK;
    let f = funds(&[(100 * KAS, 0)], 1, c.fee);
    let u = build::buy(&c, outpoint(0), &f).unwrap();
    equal_and_assemble("buy", &u, &c.build(), None);
    // claim
    let mut c = ClaimCase::new(4);
    c.funding_sk = WALLET_SK;
    let fd: Vec<(u64, u64)> = c.funding.iter().map(|x| (x.value, x.daa)).collect();
    let u = build::claim(&c, outpoint(0), outpoint(1), &funds(&fd, 2, c.fee)).unwrap();
    equal_and_assemble("claim", &u, &c.build(), None);
    // return_deposit, both paths
    for (name, mut c, n_cov) in [("return after", ReturnCase::after(), 1u32), ("return with pool", ReturnCase::with_pool(), 2)] {
        c.funding_sk = WALLET_SK;
        let fd: Vec<(u64, u64)> = c.funding.iter().map(|x| (x.value, x.daa)).collect();
        let pool = (n_cov == 2).then(|| outpoint(1));
        let u = build::return_deposit(&c, outpoint(0), pool, &funds(&fd, n_cov, c.fee)).unwrap();
        equal_and_assemble(name, &u, &c.build(), None);
    }
    // close K = 4
    let mut c = CloseCase::new(4);
    c.funding_sk = WALLET_SK;
    let fd: Vec<(u64, u64)> = c.funding.iter().map(|x| (x.value, x.daa)).collect();
    let u = build::close(&c, &ops(6), &funds(&fd, 6, c.fee), 1_000_100).unwrap();
    equal_and_assemble("close", &u, &c.build(), None);
    // draw K = 4 with its anchor proof, and the void
    for (name, c) in [("draw", proofed(DrawCase::new(4))), ("void", DrawCase::new_void(4))] {
        let mut c = c;
        c.funding_sk = WALLET_SK;
        let fd: Vec<(u64, u64)> = c.funding.iter().map(|x| (x.value, x.daa)).collect();
        let u = build::draw(&c, &ops(4), &funds(&fd, 4, c.fee)).unwrap();
        equal_and_assemble(name, &u, &c.build(), None);
    }
    // request_deposit
    let mut c = DepositCase::new();
    c.funding_sk = WALLET_SK;
    let fd: Vec<(u64, u64)> = c.funding.iter().map(|x| (x.value, x.daa)).collect();
    let u = build::request_deposit(&c, outpoint(0), &funds(&fd, 1, c.fee)).unwrap();
    equal_and_assemble("request_deposit", &u, &c.build(), None);
    // request_withdraw, every owner class, partial and whole, with and without a funding coin
    for key in [OwnerKey::Schnorr(OWNER_SK), OwnerKey::Ecdsa(OWNER_SK), OwnerKey::P2sh(OWNER_SK)] {
        for whole in [false, true] {
            for extra in [false, true] {
                let mut c = WithdrawCase::new_with(&t, key.clone());
                if whole {
                    c = c.whole();
                }
                c.funding_sk = WALLET_SK;
                if extra {
                    c.funding = vec![FundingIn { value: 3 * KAS, daa: 1_100_000 }];
                }
                let oc = OwnerCoin { coin: Coin { outpoint: outpoint(2), entry: UtxoEntry::new(c.owner_value, key.spk(), c.owner_daa, false, None) }, owner: key.public() };
                let fd: Vec<(u64, u64)> = c.funding.iter().map(|x| (x.value, x.daa)).collect();
                let u = build::request_withdraw(&c, outpoint(0), outpoint(1), &oc, &funds(&fd, 3, c.fee)).unwrap();
                equal_and_assemble(&format!("request_withdraw {:?} whole={whole} extra={extra}", key.public().class()), &u, &c.build(), Some(&key));
            }
        }
    }
    // settle, both kinds
    for (name, mut c) in [("settle deposit", SettleCase::deposit()), ("settle withdrawal", SettleCase::withdrawal())] {
        c.funding_sk = WALLET_SK;
        let fd: Vec<(u64, u64)> = c.funding.iter().map(|x| (x.value, x.daa)).collect();
        let u = build::settle(&c, outpoint(0), outpoint(1), &funds(&fd, 2, c.fee)).unwrap();
        equal_and_assemble(name, &u, &c.build(), None);
    }
}

/// The fee loop: the fee settles at or above what the transaction's own mass needs.
#[test]
fn with_fee_covers_the_mass() {
    let c = BuyCase::new(b_max());
    let u = build::with_fee(100, |fee| build::buy(&c, outpoint(0), &funds(&[(100 * KAS, 0)], 1, fee))).unwrap();
    assert!(u.fee >= build::fee_of(&u.masses, 100).unwrap());
    assert_eq!(build::fee_of(&u.masses, 1_001).unwrap_err().code, Code::FeeRate);
}

/// F1 review notes (build.rs:207, 587-598 and 442). The money behaviour is unchanged (the
/// same coins are refused or accepted as before); only the codes are made accurate:
/// - coins that cover a buy without change, but whose change would be too small to pay for
///   its own output (its storage mass), are `change_dust`, not `funding_short`: the fee loop
///   never drops change (that would pay the leftover as fee), so it refuses, and now says why;
/// - coins that do not cover the buy even without change stay `funding_short`;
/// - a draw whose coins do not cover the new reserves and the fee is `funding_short`, not
///   `refused`.
#[test]
fn small_change_and_short_draws_have_their_codes() {
    let c = BuyCase::new(b_max());
    let rate = 100;
    let cost = c.bundle.len() as u64 * (c.shard.price + c.shard.d) as u64;
    // The fee the buy needs without a change output: coins of exactly cost + fee leave none.
    let exact = |fee: u64| build::buy(&c, outpoint(0), &funds(&[(cost + fee, 0)], 1, fee)).unwrap();
    let mut need0 = 0;
    for _ in 0..4 {
        need0 = build::fee_of(&exact(need0.max(1)).masses, rate).unwrap();
    }
    let u0 = exact(need0);
    assert_eq!(u0.tx.outputs.len(), 2, "the exact-coin buy has no change output");
    assert!(u0.fee >= build::fee_of(&u0.masses, rate).unwrap());
    let run = |coin: u64| build::with_fee(rate, |fee| build::buy(&c, outpoint(0), &funds(&[(coin, 0)], 1, fee)));
    let mut seen = vec![];
    for extra in [0u64, 1, 1_000, 100_000, 1_000_000, 10_000_000] {
        let r = run(cost + need0 + extra);
        let what = match &r {
            Ok(u) => format!("ok, fee {}, {} outputs", u.fee, u.tx.outputs.len()),
            Err(e) => format!("{e}"),
        };
        println!("coin = cost + need0 ({need0}) + {extra}: {what}");
        // Owner decision (DECISIONS 2026-10-26 (2)): a leftover of exactly zero builds without a
        // change output and with no extra fee; any other leftover below dust stays `change_dust`.
        match r {
            Ok(u) if extra == 0 => {
                assert_eq!(u.tx.outputs.len(), 2, "the exact-coin buy has no change output");
                assert_eq!(u.fee, build::fee_of(&u.masses, rate).unwrap(), "the exact-coin buy pays no extra fee");
                assert_eq!(u.fee, need0, "the exact-coin buy pays the no-change fee");
            }
            Ok(u) => assert_eq!(u.tx.outputs.len(), 3, "extra {extra}: accepted only with its change output"),
            Err(e) => {
                assert_ne!(extra, 0, "a leftover of exactly zero builds without change: {e}");
                assert_eq!(e.code, Code::ChangeDust, "extra {extra}: {e}")
            }
        }
        seen.push(extra);
    }
    // Below the no-change fee: short, whatever the change.
    assert_code("coins below cost + fee", run(cost + need0 - 1), Code::FundingShort);
    assert_code("coins below cost", run(cost - 1), Code::FundingShort);
    // A large change pays for itself.
    assert!(run(cost + need0 + 100_000_000).is_ok());
    // A draw funded below its new reserves and fee.
    let d = proofed(DrawCase::new(4));
    assert_code("a draw funded short", build::draw(&d, &ops(4), &funds(&[(1_000, 0)], 4, d.fee)), Code::FundingShort);
}

fn assert_code(name: &str, r: lottery::web::Result<Unsigned>, code: Code) {
    match r {
        Ok(_) => panic!("{name}: accepted, expected {code}"),
        Err(e) => assert_eq!(e.code, code, "{name}: {e}"),
    }
}

/// Spec 8 "Post-sign check": a mock wallet that alters each of these is refused.
#[test]
fn post_sign_refuses_every_wallet_alteration() {
    let mut c = BuyCase::new(b_max());
    c.with_ref = true;
    let u = build::buy(&c, outpoint(0), &funds(&[(60 * KAS, 0), (50 * KAS, 0)], 1, c.fee)).unwrap();
    let honest = wallet_sign(&u, None);
    let e = entries_of(&u);
    sign::assemble(&u, &honest, &e).expect("the honest wallet passes");
    let other = spk_of(&[9u8; 32]);
    let last = honest.outputs.len() - 1;
    type Edit = Box<dyn Fn(&mut Transaction)>;
    let cases: Vec<(&str, Edit, Code)> = vec![
        ("a covenant signature script", Box::new(|t| t.inputs[0].signature_script = vec![0]), Code::SignedChanged),
        ("an output value", Box::new(|t| t.outputs[1].value -= 1), Code::SignedChanged),
        ("an output script", Box::new(|t| t.outputs[2].script_public_key = spk_of(&[9u8; 32])), Code::SignedChanged),
        ("an extra output", Box::new(|t| t.outputs.push(t.outputs[2].clone())), Code::SignedChanged),
        ("the change address", Box::new(move |t| t.outputs[last].script_public_key = other.clone()), Code::SignedChanged),
        (
            "an unlisted input's signature",
            Box::new(|t| {
                let s = t.inputs[1].signature_script.clone();
                t.inputs[0].signature_script = s
            }),
            Code::SignedChanged,
        ),
        ("the lock time", Box::new(|t| t.lock_time = 5), Code::SignedChanged),
        ("the payload", Box::new(|t| t.payload = vec![0xab]), Code::SignedChanged),
        ("a covenant binding", Box::new(|t| t.outputs[0].covenant = None), Code::SignedChanged),
        ("a sequence", Box::new(|t| t.inputs[1].sequence = 7), Code::SignedChanged),
        ("a compute budget", Box::new(|t| t.inputs[0] = harness::TransactionInput::new_with_compute_budget(t.inputs[0].previous_outpoint, t.inputs[0].signature_script.clone(), 0, 1)), Code::SignedChanged),
        ("an outpoint", Box::new(|t| t.inputs[2].previous_outpoint = outpoint(99)), Code::SignedChanged),
        ("an extra input", Box::new(|t| t.inputs.push(t.inputs[2].clone())), Code::SignedChanged),
        ("a listed input left unsigned", Box::new(|t| t.inputs[2].signature_script = vec![]), Code::SignedMissing),
        ("a signature with another sighash type", Box::new(|t| t.inputs[1].signature_script[65] = 0x81), Code::SignatureForm),
        ("a signature of the wrong size", Box::new(|t| t.inputs[1].signature_script.push(0)), Code::SignatureForm),
        ("a signature that does not verify", Box::new(|t| t.inputs[1].signature_script[10] ^= 1), Code::SignatureInvalid),
    ];
    for (name, edit, code) in cases {
        let mut got = honest.clone();
        edit(&mut got);
        got.finalize();
        let err = sign::assemble(&u, &got, &e).err().unwrap_or_else(|| panic!("{name}: accepted"));
        assert_eq!(err.code, code, "{name}: {err}");
    }
    // A signature made over another UTXO amount (a wallet that trusts a lying entry) fails in
    // the engine against our entries; the returned entry itself is only a note.
    let mut lied = u.entries.clone();
    lied[1] = UtxoEntry::new(5, lied[1].script_public_key.clone(), 0, false, None);
    let mut got = honest.clone();
    got.inputs[1].signature_script = sign_input(&PopulatedTransaction::new(&u.tx, lied.clone()), 1, &WALLET_SK, SIG_HASH_ALL);
    got.finalize();
    let check = sign::post_sign(&u.tx, &u.entries, &u.sign, &got, &lied.iter().cloned().map(Some).collect::<Vec<_>>());
    assert!(check.pass() && check.notes.iter().any(|n| n.starts_with("input 1 UTXO entry")), "{check:?}");
    assert_eq!(sign::assemble(&u, &got, &e).unwrap_err().code, Code::SignatureInvalid);
    // A P2SH owner's redeem must be the position's (a wallet that pushes another redeem).
    let t = Templates::get(b_max());
    let key = OwnerKey::P2sh(OWNER_SK);
    let w = WithdrawCase::new_with(&t, key.clone());
    let oc = OwnerCoin { coin: Coin { outpoint: outpoint(2), entry: UtxoEntry::new(w.owner_value, key.spk(), w.owner_daa, false, None) }, owner: key.public() };
    let u = build::request_withdraw(&w, outpoint(0), outpoint(1), &oc, &funds(&[], 3, w.fee)).unwrap();
    let mut got = wallet_sign(&u, Some(&key));
    let n = got.inputs[2].signature_script.len();
    got.inputs[2].signature_script[n - 2] ^= 1;
    got.finalize();
    assert_eq!(sign::assemble(&u, &got, &entries_of(&u)).unwrap_err().code, Code::SignatureForm);
}

#[test]
fn guards_refuse_with_their_codes() {
    let t = Templates::get(b_max());
    let b = BuyCase::new(b_max());
    let ok = || funds(&[(100 * KAS, 0)], 1, b.fee);
    build::buy(&b, outpoint(0), &ok()).expect("the base buy");
    assert_code("funds short", build::buy(&b, outpoint(0), &funds(&[(KAS, 0)], 1, b.fee)), Code::FundingShort);
    let mut f = ok();
    f.coins[0].entry = UtxoEntry::new(100 * KAS, wallet_spk(), u64::MAX, false, None);
    assert_code("unconfirmed coin", build::buy(&b, outpoint(0), &f), Code::CoinUnconfirmed);
    let mut f = ok();
    f.coins[0].entry = UtxoEntry::new(100 * KAS, wallet_spk(), 0, false, Some(lottery::buy::COV));
    assert_code("coin with a covenant ID", build::buy(&b, outpoint(0), &f), Code::WalletScript);
    let mut f = ok();
    f.change = ScriptPublicKey::new(0, vec![0x51].into());
    assert_code("non-standard change", build::buy(&b, outpoint(0), &f), Code::WalletScript);
    let mut f = ok();
    f.fee = build::MAX_FEE_SOMPI + 1;
    assert_code("fee above the cap", build::buy(&b, outpoint(0), &f), Code::FeeRate);
    let mut x = b.clone();
    x.tree.increment(lottery::combo::Combo::new([1, 2, 3, 4, 5], 1).rank(), 1);
    assert_code("a tree that is not the shard's", build::buy(&x, outpoint(0), &ok()), Code::TreeMismatch);
    let mut x = b.clone();
    x.bundle = vec![lottery::combo::Combo::new([5, 4, 3, 2, 1], 1)];
    assert_code("a non-canonical combination", build::buy(&x, outpoint(0), &ok()), Code::PurchaseInvalid);
    let mut x = b.clone();
    x.shard.price = 0;
    assert_code("a shard state out of range", build::buy(&x, outpoint(0), &ok()), Code::StateRange);
    let mut x = b.clone();
    x.shard.s_r = 1;
    x.with_ref = true;
    assert_code("a referral below M_REF", build::buy(&x, outpoint(0), &ok()), Code::Refused);
    // claim: an index already claimed, trees that do not give the pool's roots, a lie.
    let c = ClaimCase::new(4);
    let cf = || funds(&[(10 * KAS, 2_100_000)], 2, c.fee);
    let mut x = c.clone();
    x.ticket.claimed[0] = 1;
    assert_code("a claimed index", build::claim(&x, outpoint(0), outpoint(1), &cf()), Code::Refused);
    let mut x = c.clone();
    x.trees[1].increment(7, 1);
    assert_code("a wrong tree", build::claim(&x, outpoint(0), outpoint(1), &cf()), Code::TreeMismatch);
    let mut x = c.clone();
    x.lie.h = Some(1);
    assert_code("a TEST-ONLY lie", build::claim(&x, outpoint(0), outpoint(1), &cf()), Code::InputRange);
    // close: stale inputs (D24), a previous round not drawn.
    let c = CloseCase::new(4);
    let ff = || funds(&[(100 * KAS, 900_000)], 6, c.fee);
    assert_code("stale close", build::close(&c, &ops(6), &ff(), 3_000_000), Code::Policy);
    let mut x = c.clone();
    x.controller.drawn = 0;
    assert_code("close before the draw", build::close(&x, &ops(6), &ff(), 1_000_100), Code::Refused);
    assert_code("close with too few outpoints", build::close(&c, &ops(5), &ff(), 1_000_100), Code::InputRange);
    // draw: no anchor proof; a proof that does not anchor D.
    let d = DrawCase::new(4);
    let df = || funds(&[(100 * KAS, 0)], 4, d.fee);
    assert_code("a draw without its proof", build::draw(&d, &ops(4), &df()), Code::InputRange);
    let mut x = proofed(d.clone());
    x.marker_daa += 10;
    assert_code("a proof of another D", build::draw(&x, &ops(4), &df()), Code::AnchorInvalid);
    // vault: a deposit below MIN_DEPOSIT, a withdrawal below wmin, another owner.
    let mut x = DepositCase::new();
    x.d = lottery::game::MIN_DEPOSIT - 1;
    assert_code("a deposit below MIN_DEPOSIT", build::request_deposit(&x, outpoint(0), &funds(&[(20 * KAS, 1_100_000)], 1, x.fee)), Code::Refused);
    let w = WithdrawCase::new();
    let key = OwnerKey::Schnorr(OWNER_SK);
    let oc = OwnerCoin { coin: Coin { outpoint: outpoint(2), entry: UtxoEntry::new(w.owner_value, key.spk(), w.owner_daa, false, None) }, owner: key.public() };
    let mut x = w.clone();
    x.w = x.w_min() - 1;
    assert_code("below wmin", build::request_withdraw(&x, outpoint(0), outpoint(1), &oc, &funds(&[], 3, w.fee)), Code::Policy);
    let stranger = OwnerCoin { owner: OwnerKey::Schnorr([0x41; 32]).public(), ..oc.clone() };
    assert_code("another owner", build::request_withdraw(&w, outpoint(0), outpoint(1), &stranger, &funds(&[], 3, w.fee)), Code::WalletScript);
    // settle: a request against the wrong round's record.
    let mut x = SettleCase::deposit();
    x.request.round += 1;
    assert_code("a settlement the reference refuses", build::settle(&x, outpoint(0), outpoint(1), &funds(&[(10 * KAS, 1_300_000)], 2, x.fee)), Code::Refused);
    let _ = t;
}

/// Spec 8 "Untrusted API" at the library level: each makes the page refuse to build.
#[test]
fn untrusted_api_mismatches_are_refused() {
    let t = Templates::get(b_max());
    // Every fixture state round-trips through its state bytes, and is checked against its
    // UTXO script.
    let c = CloseCase::new(4);
    let d = DrawCase::new(4);
    let states = vec![
        State::Controller(c.controller.clone()),
        State::Params(c.params.clone()),
        State::Shard(c.shards[0].state.clone()),
        State::Marker(d.marker.clone()),
        State::Drawer(d.drawer.clone()),
        State::Vault(d.vault.clone()),
        State::Pool(ClaimCase::new(4).pool),
        State::Ticket(ClaimCase::new(4).ticket),
        State::Position(WithdrawCase::new().position),
        State::Request(SettleCase::deposit().request),
        State::Record(SettleCase::deposit().record),
    ];
    for s in &states {
        let bytes = verify::state_bytes(s).unwrap();
        let spk = verify::state_spk(s).unwrap();
        assert_eq!(&verify::decode_state(s.role(), &bytes).unwrap(), s);
        assert_eq!(&verify::verify_state(s.role(), &bytes, &spk).unwrap(), s);
        assert_eq!(&verify::decode_redeem(&s.redeem(&t)).unwrap(), s);
        // A wrong state: another state's script.
        let other = verify::state_spk(&states[(states.iter().position(|x| x == s).unwrap() + 1) % states.len()]).unwrap();
        assert_eq!(verify::verify_state(s.role(), &bytes, &other).unwrap_err().code, Code::ChainMismatch);
    }
    // A doctored state (one field changed) against the live UTXO's script.
    let mut sh = c.shards[0].state.clone();
    let live = verify::state_spk(&State::Shard(sh.clone())).unwrap();
    sh.sales += 1;
    let bytes = verify::state_bytes(&State::Shard(sh)).unwrap();
    assert_eq!(verify::verify_state(Role::Shard, &bytes, &live).unwrap_err().code, Code::ChainMismatch);
    // Trees from purchases against the frozen roots: exact, a missing purchase, an extra one.
    let buys: Vec<[u8; 6]> = vec![[1, 2, 3, 4, 5, 1], [3, 7, 12, 19, 28, 4], [1, 2, 3, 4, 5, 1]];
    let tree = verify::rebuild_tree(10, &buys).unwrap();
    let mut roots = vec![0u8; 32 * lottery::state::K_MAX];
    roots[..32].copy_from_slice(&tree.root());
    verify::check_roots(&[tree], &roots, 1).unwrap();
    let missing = verify::rebuild_tree(10, &buys[..2]).unwrap();
    assert_eq!(verify::check_roots(&[missing], &roots, 1).unwrap_err().code, Code::TreeMismatch);
    let mut more = buys.clone();
    more.push([2, 4, 6, 8, 10, 3]);
    let extra = verify::rebuild_tree(10, &more).unwrap();
    assert_eq!(verify::check_roots(&[extra], &roots, 1).unwrap_err().code, Code::TreeMismatch);
    assert_eq!(verify::rebuild_tree(10, &[[1, 2, 3, 4, 5, 11]]).err().map(|e| e.code), Some(Code::PurchaseInvalid));
    // The winning numbers from the anchor proof: the drawer's; a wrong anchor is refused.
    let dc = proofed(DrawCase::new(4));
    let p = dc.proof.unwrap().to_bytes();
    let seed = dc.seed();
    let win = verify::winning_numbers_from_anchor(&dc.cov.as_bytes(), dc.marker.round, &p, dc.d(), dc.marker.m).unwrap();
    assert_eq!(win, [seed.main[0], seed.main[1], seed.main[2], seed.main[3], seed.main[4], seed.bonus]);
    let mut bad = p.clone();
    bad[200] ^= 1;
    assert_eq!(verify::winning_numbers_from_anchor(&dc.cov.as_bytes(), dc.marker.round, &bad, dc.d(), dc.marker.m).unwrap_err().code, Code::AnchorInvalid);
    assert_eq!(verify::winning_numbers_from_anchor(&dc.cov.as_bytes(), dc.marker.round, &p[..271], dc.d(), dc.marker.m).unwrap_err().code, Code::BytesLength);
    // A wrong winning number claimed by an API differs from the recomputed one.
    let other = verify::winning_numbers(&dc.cov.as_bytes(), dc.marker.round, &[0x77; 32], dc.marker.m).unwrap();
    assert_ne!([other.main[0], other.main[1], other.main[2], other.main[3], other.main[4], other.bonus], win);
}

#[test]
fn error_codes_are_stable() {
    let table: Vec<String> = Code::ALL.iter().map(|c| format!("{} {}", c.number(), c.as_str())).collect();
    assert_eq!(
        table.join(","),
        "1 bytes_length,2 not_template,3 not_canonical,4 state_range,5 chain_mismatch,6 tree_mismatch,7 purchase_invalid,8 store_format,\
         9 store_incomplete,10 anchor_invalid,11 input_range,12 funding_short,13 wallet_script,14 coin_unconfirmed,15 overflow,16 refused,\
         17 policy,18 encode,19 mass,20 engine,21 templates,22 fee_rate,23 signed_changed,24 signed_missing,25 signature_form,26 signature_invalid,\
         27 address_network,28 input_format,29 not_started,30 mainnet_refused,31 built_mismatch,32 owner_type,33 change_dust,34 mixed_signers,\
         35 node_unavailable,36 node_network,37 node_refused,38 indexer_unavailable,39 worker_stopped,40 node_conflict,\
         41 wallet_missing,42 wallet_rejected,43 wallet_network,44 storage_unavailable,45 sales_closed,46 purchase_changed,47 submit_unknown,48 wallet_timeout,49 wallet_changed,\
         50 backup_invalid,51 backup_mismatch,52 copy_mismatch,53 fee_above_prize,54 fee_above_reward"
    );
    let msgs: std::collections::HashSet<&str> = Code::ALL.iter().map(|c| c.message()).collect();
    assert_eq!(msgs.len(), Code::ALL.len(), "one distinct message per code");
    // F1 part C: numbers are 1.. in table order, names round-trip, and the TypeScript union
    // (emitted into crates/web's .d.ts) lists every name.
    for (i, c) in Code::ALL.iter().enumerate() {
        assert_eq!(c.number() as usize, i + 1, "{c}: number");
        assert_eq!(Code::from_name(c.as_str()), Some(*c), "{c}: from_name");
        assert!(Code::TS_DECL.contains(&format!("\"{}\" | ", c.as_str())), "{c}: TS_DECL");
    }
    assert!(Code::TS_DECL.contains("export type ErrorName = \"bytes_length\" | ") && Code::TS_DECL.ends_with(" | never;\n"));
}
