//! Front-end F1 part C: the `crates/web` API natively (the wasm run is `tests/w1_wasm.rs`).
//!
//! - `api_builds_equal_the_library`: every fixture call, as the page's JSON input, builds
//!   exactly `lottery::web`'s transaction (the same case through `cases`, at the same fee
//!   rate), and the boundary form `BuiltTx` reads back to the same `Unsigned`.
//! - `api_digests_match_the_golden_file`: the `BuiltTx` JSON of every fixture call against
//!   `tests/fixtures/w1_api.txt` (W1's API half: the wasm test compares with the same file).
//! - `assemble_takes_a_wallets_signatures_only`: a stand-in wallet signs the Safe JSON;
//!   `assemble` passes it; spec 8's mock-wallet alterations, made on the JSON, are refused.
//! - `other_network_addresses_are_refused`: spec 6.6 at every entry that takes an address.
//! - `untrusted_inputs_never_panic`: W3 at the boundary (mutated JSON inputs, mutated Safe
//!   JSON, random hex), each call an error or a build, never a panic.
//! - `quick_wallet_signs_what_it_owns`, `views_decode_every_role`.
//!
//! F1 review (the reviewer's probes P1-P6 of `review_f1_probes.rs`, folded in as attack
//! tests; each failed, or printed the faulty behaviour, before its fix):
//! - `roots_and_numbers_come_from_chain_objects` (item 1, P4): `checkRoots` and
//!   `winningNumbers` take only chain objects; round, m and D are derived in Rust, and the
//!   numbers of a drawn round must be its pool's.
//! - `assemble_binds_every_figure` (item 2, P6): a `BuiltTx` with any figure altered (fee,
//!   ID, masses, units, sign list, summary) is refused by `assemble` and `quickWalletSign`.
//! - `deposit_owners_are_withdrawable_from_the_page` (item 3, P1, P2): `request_deposit`
//!   takes Schnorr P2PK owners only; such an owner's deposit, settlement and withdrawal go
//!   through, signed by the quick wallet.
//! - `small_change_is_change_dust` (note, P5) and `quick_keys_are_redacted_in_debug` (note).

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{GOLDEN, GOLDEN_VERIFY, Input, RATE, addr, digest_line, drawn, equivalent, input, output_of, created, verify_calls, verify_native, with_fee};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::sign::sign_input;
use kaspa_consensus_core::tx::PopulatedTransaction;
use lottery::web::Code;
use lottery::web::fixtures::{Call, WALLET_SK, calls, run};
use web::api;
use web::convert::{Net, hex};
use web::dto::*;
use web::safe_json;

fn all_inputs() -> Vec<(String, Call, Input)> {
    calls().expect("fixture calls").into_iter().map(|(n, c, _)| (n, c.clone(), input(&c))).collect()
}

#[test]
fn api_builds_equal_the_library() {
    for (name, call, inp) in all_inputs() {
        let b = inp.run().unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = lottery::web::build::with_fee(RATE as u64, |fee| run(&with_fee(&equivalent(&call), fee))).unwrap_or_else(|e| panic!("{name}: library: {e}"));
        let got = api::unsigned_of(Net::Testnet10, &b).unwrap_or_else(|e| panic!("{name}: unsigned_of: {e}"));
        assert_eq!(got.tx, want.tx, "{name}: transaction");
        assert_eq!(got.tx.id(), want.tx.id(), "{name}: id");
        assert_eq!(b.id, want.tx.id().to_string(), "{name}: id field");
        assert_eq!(got.entries, want.entries, "{name}: entries");
        assert_eq!(got.sign, want.sign, "{name}: sign");
        assert_eq!(got.fee, want.fee, "{name}: fee");
        assert_eq!(got.units, want.units, "{name}: units");
        assert_eq!((got.masses.compute, got.masses.storage, got.masses.priority), (want.masses.compute, want.masses.storage, want.masses.priority), "{name}: masses");
        // The figures are the transaction's own.
        let outs: u64 = b.summary.outputs.iter().map(|o| o.value.parse::<u64>().unwrap()).sum();
        assert_eq!(outs, want.tx.outputs.iter().map(|o| o.value).sum::<u64>(), "{name}: outputs");
        assert!(b.sign.iter().all(|s| s.sighash_type == 1 && s.address.as_deref().is_some_and(|a| a.starts_with("kaspatest:"))), "{name}: sign items");
        println!("{name}: api = library, fee {} sompi, {} B of Safe JSON, sign {:?}", b.fee, b.transaction.len(), b.sign.iter().map(|s| s.index).collect::<Vec<_>>());
    }
}

fn digest_lines() -> Vec<String> {
    all_inputs().iter().map(|(name, _, inp)| digest_line(name, &inp.run().unwrap_or_else(|e| panic!("{name}: {e}")))).collect()
}

#[test]
fn api_digests_match_the_golden_file() {
    let got = digest_lines();
    let want: Vec<&str> = GOLDEN.lines().collect();
    assert!(!want.is_empty(), "tests/fixtures/w1_api.txt is empty: write it with `cargo test -p web --release --test api -- --ignored write_golden`");
    assert_eq!(got.len(), want.len(), "line count");
    for (g, w) in got.iter().zip(&want) {
        assert_eq!(g, w);
    }
}

fn verify_lines() -> Vec<String> {
    verify_calls().iter().map(|(name, v)| common::text_line(name, &verify_native(v))).collect()
}

/// W1's verify half (F1 review, item 4): every decode and verify call of `verify_calls`
/// natively, against `tests/fixtures/w1_verify.txt`, which the wasm run compares too.
#[test]
fn verify_digests_match_the_golden_file() {
    let got = verify_lines();
    let want: Vec<&str> = GOLDEN_VERIFY.lines().collect();
    assert_eq!(got.len(), want.len(), "line count (rewrite with -- --ignored write_golden)");
    for (g, w) in got.iter().zip(&want) {
        assert_eq!(g, w);
    }
    // The outcomes themselves: each refused variant is an error, every other call ok.
    for (name, v) in verify_calls() {
        let j = verify_native(&v);
        let refused = [" as ", "another", "extra purchase", "a drawer as"].iter().any(|k| name.contains(k));
        assert_eq!(j.starts_with("{\"error\""), refused, "{name}: {}", &j[..j.len().min(160)]);
    }
    println!("{} verify lines equal the golden file", got.len());
}

#[test]
#[ignore]
fn write_golden() {
    let lines = digest_lines();
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/w1_api.txt"), lines.join("\n") + "\n").unwrap();
    let v = verify_lines();
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/w1_verify.txt"), v.join("\n") + "\n").unwrap();
    println!("wrote {} + {} lines", lines.len(), v.len());
}

/// A stand-in wallet: parse the Safe JSON as kaspa-wasm does, sign the listed inputs with
/// the key each needs (the wallet key; the fixture owner key for an owner input), and
/// re-serialize.
fn wallet(b: &BuiltTx) -> String {
    let (mut tx, entries, _) = safe_json::from_json(&b.transaction).unwrap();
    let entries: Vec<_> = entries.into_iter().map(Option::unwrap).collect();
    let unsigned = tx.clone();
    for s in &b.sign {
        let i = s.index as usize;
        let p = PopulatedTransaction::new(&unsigned, entries.clone());
        let wallet_x = hex(&lottery::params::xonly(&WALLET_SK));
        tx.inputs[i].signature_script = if s.kind == KeyKind::Schnorr && s.public_key == wallet_x {
            sign_input(&p, i, &WALLET_SK, SIG_HASH_ALL)
        } else {
            let k = [lottery::buy::OwnerKey::Schnorr(lottery::vault::OWNER_SK), lottery::buy::OwnerKey::Ecdsa(lottery::vault::OWNER_SK), lottery::buy::OwnerKey::P2sh(lottery::vault::OWNER_SK)]
                .into_iter()
                .find(|k| hex(&match k.public() {
                    lottery::buy::OwnerPub::Schnorr(x) | lottery::buy::OwnerPub::P2sh(x) => x.to_vec(),
                    lottery::buy::OwnerPub::Ecdsa(x) => x.to_vec(),
                }) == s.public_key && k.spk() == entries[i].script_public_key)
                .expect("an owner key");
            k.sign(&p, i)
        };
    }
    tx.finalize();
    safe_json::reserialize(&tx, &entries, Prefix::Testnet).unwrap()
}

#[test]
fn assemble_takes_a_wallets_signatures_only() {
    for (name, _, inp) in all_inputs() {
        let b = inp.run().unwrap();
        let signed = wallet(&b);
        let s = api::assemble(Net::Testnet10, &AssembleInput { built: b.clone(), signed }).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(s.id, b.id, "{name}: the final id is the built one (v1 ids exclude signature scripts)");
    }
    // Spec 8's mock-wallet list, on the returned JSON of a buy with a referral.
    let (_, _, inp) = all_inputs().into_iter().find(|(n, _, _)| n == "buy b=1 ref").unwrap();
    let b = inp.run().unwrap();
    let honest: serde_json::Value = serde_json::from_str(&wallet(&b)).unwrap();
    let other_spk = web::convert::spk_hex(&lottery::buy::spk_of(&[9u8; 32]));
    type Edit = Box<dyn Fn(&mut serde_json::Value)>;
    let edits: Vec<(&str, Edit, Code)> = vec![
        ("a covenant signature script", Box::new(|v| v["inputs"][0]["signatureScript"] = "00".into()), Code::SignedChanged),
        ("an output value", Box::new(|v| v["outputs"][1]["value"] = "1".into()), Code::SignedChanged),
        ("an output script", Box::new(move |v| v["outputs"][2]["scriptPublicKey"] = other_spk.clone().into()), Code::SignedChanged),
        ("an extra output", Box::new(|v| {
            let o = v["outputs"][2].clone();
            v["outputs"].as_array_mut().unwrap().push(o)
        }), Code::SignedChanged),
        ("the change address", Box::new(|v| {
            let n = v["outputs"].as_array().unwrap().len();
            v["outputs"][n - 1]["scriptPublicKey"] = web::convert::spk_hex(&lottery::buy::spk_of(&[8u8; 32])).into()
        }), Code::SignedChanged),
        ("the lock time", Box::new(|v| v["lockTime"] = "1".into()), Code::SignedChanged),
        ("the payload", Box::new(|v| v["payload"] = "00".into()), Code::SignedChanged),
        ("an unsigned input", Box::new(|v| v["inputs"][1]["signatureScript"] = "".into()), Code::SignedMissing),
    ];
    for (what, edit, code) in edits {
        let mut v = honest.clone();
        edit(&mut v);
        match api::assemble(Net::Testnet10, &AssembleInput { built: b.clone(), signed: v.to_string() }) {
            Ok(_) => panic!("{what}: accepted"),
            Err(e) => {
                assert_eq!(e.code, code, "{what}: {e}");
                println!("{what}: refused, {}", e.code);
            }
        }
    }
    // Our own built object altered (a page bug or a tampered store): the self-consistency
    // check refuses before any comparison.
    let mut lie = b.clone();
    lie.sign.clear();
    assert_eq!(api::assemble(Net::Testnet10, &AssembleInput { built: lie, signed: honest.to_string() }).unwrap_err().code, Code::InputFormat);
}

fn other(prefix: Prefix) -> String {
    Address::new(prefix, Version::PubKey, &[0x42; 32]).to_string()
}

#[test]
fn other_network_addresses_are_refused() {
    let wrong = [other(Prefix::Mainnet), other(Prefix::Devnet), other(Prefix::Simnet), "kaspatest:notanaddress".into(), "".into()];
    let inputs = all_inputs();
    let buy = inputs.iter().find(|(n, _, _)| n == "buy b=1 ref").unwrap().2.clone();
    let dep = inputs.iter().find(|(n, _, _)| n == "request_deposit").unwrap().2.clone();
    for w in &wrong {
        let mut cases: Vec<(&str, Input)> = vec![];
        if let Input::Buy(b) = &buy {
            cases.push(("buy payout", Input::Buy(BuyInput { payout: w.clone(), ..b.clone() })));
            cases.push(("buy referrer", Input::Buy(BuyInput { referrer: Some(w.clone()), ..b.clone() })));
            cases.push(("buy change", Input::Buy(BuyInput { funds: Funds { change: w.clone(), ..b.funds.clone() }, ..b.clone() })));
        }
        if let Input::Deposit(d) = &dep {
            cases.push(("deposit owner", Input::Deposit(DepositInput { owner: w.clone(), ..d.clone() })));
        }
        // Every builder's change (and caller) address.
        for (n, _, inp) in &inputs {
            let mut v = inp.json();
            v["funds"]["change"] = w.clone().into();
            let i = inp.with_json(v).unwrap();
            let r = i.run();
            let code = r.as_ref().map(|_| ()).unwrap_err().code;
            assert!(matches!(code, Code::AddressNetwork | Code::InputFormat), "{n} change `{w}`: {code}");
        }
        for (what, i) in cases {
            let e = i.run().map(|_| ()).unwrap_err();
            assert!(matches!(e.code, Code::AddressNetwork | Code::InputFormat), "{what} `{w}`: {e}");
            if w.contains(':') && !w.starts_with("kaspatest:") {
                assert_eq!(e.code, Code::AddressNetwork, "{what} `{w}`");
            }
        }
        let e = api::script_of(Net::Testnet10, w).unwrap_err();
        assert!(matches!(e.code, Code::AddressNetwork | Code::InputFormat), "scriptOf `{w}`: {e}");
    }
    // The testnet address itself is accepted, and printed back.
    let me = addr(&lottery::state::Payout::Schnorr([0x42; 32]));
    assert_eq!(api::address_of(Net::Testnet10, &api::script_of(Net::Testnet10, &me).unwrap()).unwrap(), me);
    // Networks: only testnet-10 starts (simnet only in a `simnet` build; mainnet never).
    assert!(api::start("testnet-10").is_ok());
    for n in ["mainnet", "testnet-11", "devnet", "kaspa", ""] {
        assert_eq!(api::start(n).unwrap_err().code, Code::AddressNetwork, "{n}");
    }
    #[cfg(not(feature = "simnet"))]
    assert_eq!(api::start("simnet").unwrap_err().code, Code::AddressNetwork);
    // A `simnet` build started on the simnet takes `kaspasim:` addresses and refuses the
    // testnet's (and mainnet's) in turn.
    #[cfg(feature = "simnet")]
    {
        let (n, info) = api::start("simnet").unwrap();
        assert_eq!(info.address_prefix, "kaspasim");
        assert!(api::script_of(n, &other(Prefix::Simnet)).is_ok());
        for p in [Prefix::Testnet, Prefix::Mainnet] {
            assert_eq!(api::script_of(n, &other(p)).unwrap_err().code, Code::AddressNetwork);
        }
    }
}

/// Deterministic xorshift.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Every leaf path of a JSON value.
fn paths(v: &serde_json::Value, at: Vec<String>, out: &mut Vec<Vec<String>>) {
    match v {
        serde_json::Value::Object(m) => m.iter().for_each(|(k, x)| paths(x, [at.clone(), vec![k.clone()]].concat(), out)),
        serde_json::Value::Array(a) => a.iter().enumerate().for_each(|(i, x)| paths(x, [at.clone(), vec![i.to_string()]].concat(), out)),
        _ => {}
    }
    out.push(at);
}

fn pointer(p: &[String]) -> String {
    p.iter().map(|k| format!("/{k}")).collect()
}

fn mutate(rng: &mut Rng, s: &str) -> serde_json::Value {
    match rng.below(14) {
        0 => serde_json::Value::Null,
        1 => true.into(),
        2 => (-1).into(),
        3 => "".into(),
        4 => "-1".into(),
        5 => "18446744073709551616".into(),
        6 => "9223372036854775808".into(),
        7 => "0".into(),
        8 => "zz".into(),
        9 => serde_json::json!([]),
        10 => serde_json::json!({}),
        11 => s.chars().rev().collect::<String>().into(),
        12 => format!("{s}00").into(),
        _ => s.get(..s.len() / 2).unwrap_or("").into(),
    }
}

#[test]
fn untrusted_inputs_never_panic() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let iters: usize = std::env::var("F1_API_ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
    let (mut calls_n, mut built) = (0, 0);
    for (name, _, inp) in all_inputs() {
        let base = inp.json();
        let mut ps = vec![];
        paths(&base, vec![], &mut ps);
        for _ in 0..iters {
            let mut v = base.clone();
            for _ in 0..1 + rng.below(3) {
                let p = pointer(&ps[rng.below(ps.len())]);
                let Some(x) = v.pointer_mut(&p) else { continue };
                let cur = x.to_string();
                *x = mutate(&mut rng, cur.trim_matches('"'));
            }
            let Ok(i) = inp.with_json(v.clone()) else { continue };
            calls_n += 1;
            match catch_unwind(AssertUnwindSafe(|| i.run())) {
                Err(_) => panic!("{name}: panicked on {v}"),
                Ok(Ok(_)) => built += 1,
                Ok(Err(_)) => {}
            }
        }
        // The returned JSON mutated as text, into assemble.
        let b = inp.run().unwrap();
        let signed = wallet(&b);
        for _ in 0..iters / 4 {
            let mut s = signed.clone().into_bytes();
            let k = rng.below(s.len());
            match rng.below(3) {
                0 => s.truncate(k),
                1 => s[k] = b"0123456789abcdef\"{}[],:"[rng.below(23)],
                _ => s.insert(k, b'1'),
            }
            let s = String::from_utf8_lossy(&s).to_string();
            if catch_unwind(AssertUnwindSafe(|| api::assemble(Net::Testnet10, &AssembleInput { built: b.clone(), signed: s.clone() }))).is_err() {
                panic!("{name}: assemble panicked");
            }
            if catch_unwind(AssertUnwindSafe(|| safe_json::from_json(&s))).is_err() {
                panic!("{name}: from_json panicked");
            }
        }
    }
    // Random hex and roles into the decoders.
    for n in 0..300 {
        let len = rng.below(600);
        let h: String = (0..len).map(|_| hex(&[rng.next() as u8])).collect();
        let role: web::views::RoleName = serde_json::from_value(serde_json::json!(["controller", "params", "shard", "marker", "drawer", "vault", "pool", "ticket", "position", "request", "record"][n % 11])).unwrap();
        assert!(catch_unwind(|| api::decode_state(Net::Testnet10, role, &h)).is_ok());
        assert!(catch_unwind(|| api::decode_redeem(Net::Testnet10, &h)).is_ok());
        assert!(catch_unwind(|| api::address_of(Net::Testnet10, &h)).is_ok());
    }
    println!("{calls_n} mutated calls parsed, {built} built, none panicked");
}

#[cfg(not(feature = "mainnet"))]
#[test]
fn quick_wallet_signs_what_it_owns() {
    use web::quick_wallet::{quick_wallet_import, quick_wallet_new, quick_wallet_sign};
    let k = quick_wallet_new(Net::Testnet10).unwrap();
    assert!(k.address.starts_with("kaspatest:q"));
    assert_eq!(quick_wallet_import(Net::Testnet10, &k.secret_key).unwrap(), k);
    assert_ne!(quick_wallet_new(Net::Testnet10).unwrap().secret_key, k.secret_key, "two keys differ");
    // A buy paid from the quick key's own coin.
    let (_, _, inp) = all_inputs().into_iter().find(|(n, _, _)| n == "buy b=1 ref").unwrap();
    let Input::Buy(mut b) = inp else { unreachable!() };
    let spk = api::script_of(Net::Testnet10, &k.address).unwrap();
    for c in &mut b.funds.coins {
        c.script_public_key = spk.clone();
    }
    b.funds.change = k.address.clone();
    let built = api::build_buy(Net::Testnet10, &b).unwrap();
    let signed = quick_wallet_sign(Net::Testnet10, &QuickSignInput { built: built.clone(), secret_key: k.secret_key.clone() }).unwrap();
    let s = api::assemble(Net::Testnet10, &AssembleInput { built: built.clone(), signed }).unwrap();
    assert_eq!(s.id, built.id);
    // Another key owns nothing here.
    let other = quick_wallet_new(Net::Testnet10).unwrap();
    assert_eq!(quick_wallet_sign(Net::Testnet10, &QuickSignInput { built, secret_key: other.secret_key }).unwrap_err().code, Code::SignedMissing);
}

#[test]
fn views_decode_every_role() {
    let mut seen = std::collections::BTreeSet::new();
    for (name, call, _) in all_inputs() {
        let objs: Vec<lottery::web::cases::Obj> = match &call {
            Call::Buy { shard, .. } => vec![shard.clone()],
            Call::Claim { ticket, pools, .. } => [vec![ticket.clone()], pools.clone()].concat(),
            Call::Return { ticket, pool, .. } => [vec![ticket.clone()], pool.iter().cloned().collect()].concat(),
            Call::Close { controller, params, shards, .. } => [vec![controller.clone(), params.clone()], shards.clone()].concat(),
            Call::Draw { drawer, marker, vault, controller, .. } => vec![drawer.clone(), marker.clone(), vault.clone(), controller.clone()],
            Call::Deposit { vault, .. } => vec![vault.clone()],
            Call::Withdraw { vault, position, .. } => vec![vault.clone(), position.clone()],
            Call::Settle { request, record, .. } => vec![request.clone(), record.clone()],
        };
        for o in objs {
            let v = web::convert::obj_dto(&o);
            // Each role's view through verifyState (the UTXO check) and through its JSON.
            let mut ok = false;
            for r in ["controller", "params", "shard", "marker", "drawer", "vault", "pool", "ticket", "position", "request", "record"] {
                let role: web::views::RoleName = serde_json::from_value(serde_json::json!(r)).unwrap();
                let cov = hex(&o.entry.covenant_id.unwrap().as_bytes());
                if let Ok(view) = api::verify_state(Net::Testnet10, &cov, role, &v) {
                    let j = serde_json::to_value(&view).unwrap();
                    assert_eq!(j["role"], r, "{name}");
                    assert_eq!(api::decode_state(Net::Testnet10, role, &v.state).unwrap(), view);
                    for (k, x) in j.as_object().unwrap() {
                        if k.ends_with("Address") {
                            assert!(x.as_str().is_some_and(|a| a.starts_with("kaspatest:")), "{name} {r}.{k} = {x}");
                        }
                    }
                    if r == "ticket" {
                        assert!(!j["combinations"].as_array().unwrap().is_empty());
                    }
                    seen.insert(r);
                    ok = true;
                }
            }
            assert!(ok, "{name}: no role decodes its object");
        }
    }
    assert_eq!(seen.len(), 11, "every role decoded: {seen:?}");
}

fn find(name: &str) -> (Call, Input) {
    let (_, c, i) = all_inputs().into_iter().find(|(n, _, _)| n == name).unwrap_or_else(|| panic!("no fixture {name}"));
    (c, i)
}

/// Item 1 (reviewer P4): `checkRoots` and `winningNumbers` derive the round, m and D from
/// chain objects; the page passes none of them. Before the fix both took round, m and D from
/// the page: m = 2 and m = 30 against a marker of m = 10 passed `checkRoots`, and a wrong m
/// or round gave other numbers as Ok.
#[test]
fn roots_and_numbers_come_from_chain_objects() {
    let x = drawn();
    let cov = hex(&x.cov.as_bytes());
    let lists: Vec<String> = x.purchases.iter().map(|p| hex(p)).collect();
    let mk = lottery::web::cases::marker(&web::convert::obj("m", &x.marker).unwrap(), &x.cov).unwrap();
    // checkRoots: against the marker and against a pool of the drawn round.
    for (what, against) in [("marker", x.marker.clone()), ("pool", x.pools[0].0.clone())] {
        let r = api::check_roots(&RootsInput { covenant_id: cov.clone(), against: against.clone(), purchases: lists.clone() }).unwrap_or_else(|e| panic!("{what}: {e}"));
        assert_eq!((r.round.as_str(), r.m as i64, r.roots.len()), (mk.round.to_string().as_str(), mk.m, 4), "{what}");
        let mut extra = lists.clone();
        extra[1].push_str("010203040501");
        let e = api::check_roots(&RootsInput { covenant_id: cov.clone(), against: against.clone(), purchases: extra }).unwrap_err();
        assert_eq!(e.code, Code::TreeMismatch, "{what}: an extra purchase");
        let e = api::check_roots(&RootsInput { covenant_id: cov.clone(), against, purchases: lists[..3].to_vec() }).unwrap_err();
        assert_eq!(e.code, Code::TreeMismatch, "{what}: a missing shard list");
    }
    // A state that is neither (the controller) is refused.
    assert!(api::check_roots(&RootsInput { covenant_id: cov.clone(), against: x.controller.clone(), purchases: lists.clone() }).is_err());
    // winningNumbers before the draw: marker + next controller; D derived as draw() does.
    let ctrl = lottery::web::cases::controller(&web::convert::obj("c", &x.controller).unwrap(), &x.cov).unwrap();
    let marker_daa: i64 = x.marker.utxo.block_daa_score.parse().unwrap();
    let d = lottery::game::anchor_daa(marker_daa, ctrl.seed_offset);
    let want = lottery::web::verify::winning_numbers_from_anchor(&x.cov.as_bytes(), mk.round, &x.proof, d, mk.m).unwrap();
    let win = |from: RoundObjs, proof: &[u8]| api::winning_numbers(&WinningInput { covenant_id: cov.clone(), from, anchor_proof: hex(proof) });
    let closed = RoundObjs::Closed { marker: x.marker.clone(), controller: x.controller.clone() };
    let w = win(closed.clone(), &x.proof).unwrap();
    assert_eq!([w.winning.main.to_vec(), vec![w.winning.bonus]].concat(), want.to_vec());
    assert_eq!((w.round.clone(), w.m, w.d.clone()), (mk.round.to_string(), mk.m as u32, d.to_string()));
    println!("closed: round {} m {} D {} -> {:?} + {}", w.round, w.m, w.d, w.winning.main, w.winning.bonus);
    // The marker's own creation DAA fixes D: the same proof for a marker created later is refused.
    let mut later = x.marker.clone();
    later.utxo.block_daa_score = (marker_daa + 10).to_string();
    assert_eq!(win(RoundObjs::Closed { marker: later, controller: x.controller.clone() }, &x.proof).unwrap_err().code, Code::AnchorInvalid);
    // The controller must be the marker's next round's: the round r drawer (or any other
    // role) in its place is refused, and so is a controller of another round.
    assert!(win(RoundObjs::Closed { marker: x.marker.clone(), controller: x.drawer_before.clone() }, &x.proof).is_err());
    let (Call::Close { controller: c_close, .. }, _) = find("close k=4") else { unreachable!() };
    let e = win(RoundObjs::Closed { marker: x.marker.clone(), controller: web::convert::obj_dto(&c_close) }, &x.proof).unwrap_err();
    assert_eq!(e.code, Code::ChainMismatch, "a controller of another round: {e}");
    // winningNumbers after the draw: a pool of the round + the live drawer; D from the
    // pool's claim deadline; the numbers must be the pool's.
    for (pool, pool_win) in &x.pools {
        let w = win(RoundObjs::Drawn { pool: pool.clone(), drawer: x.drawer.clone() }, &x.proof).unwrap();
        assert_eq!([w.winning.main.to_vec(), vec![w.winning.bonus]].concat(), pool_win.to_vec());
        assert_eq!((w.round.clone(), w.m, w.d.clone()), (mk.round.to_string(), mk.m as u32, d.to_string()), "D from the pool equals D from the marker");
    }
    println!("drawn: {} pools give the same numbers and D", x.pools.len());
    // An indexer's proof with another X.hash (another block, the same D window) gives other
    // numbers: refused against the pool's (before the fix there was no pool to compare).
    let mut other = lottery::anchor::AnchorProof::from_bytes(&x.proof).unwrap();
    other.x.hash[0] ^= 1;
    let other = other.to_bytes().to_vec();
    assert!(win(closed.clone(), &other).is_ok(), "off-chain, another X.hash still anchors D (only the node knows the chain)");
    let e = win(RoundObjs::Drawn { pool: x.pools[0].0.clone(), drawer: x.drawer.clone() }, &other).unwrap_err();
    assert_eq!(e.code, Code::ChainMismatch, "{e}");
    // The drawer that has not drawn the pool's round (the round r drawer) is refused.
    let e = win(RoundObjs::Drawn { pool: x.pools[0].0.clone(), drawer: x.drawer_before.clone() }, &x.proof).unwrap_err();
    assert_eq!(e.code, Code::ChainMismatch, "{e}");
    // The page's JSON has no field for a derived value.
    let j = serde_json::json!({"covenantId": cov, "from": {"closed": {"marker": x.marker, "controller": x.controller}}, "anchorProof": hex(&x.proof), "m": 10});
    assert!(serde_json::from_value::<WinningInput>(j).is_err(), "an extra `m` field is refused");
    let j = serde_json::json!({"covenantId": cov, "against": x.marker, "purchases": lists, "m": 10});
    assert!(serde_json::from_value::<RootsInput>(j).is_err(), "an extra `m` field is refused");
}

/// Item 2 (reviewer P6): `assemble` (and `quickWalletSign`) recompute every figure of a
/// `BuiltTx` from its transaction and refuse any difference. Before the fix a `BuiltTx` with
/// its fee, ID, wallet-back and an output value altered assembled.
#[test]
fn assemble_binds_every_figure() {
    type Tamper = (&'static str, Box<dyn Fn(&mut BuiltTx)>);
    let bump = |s: &mut String| *s = (s.parse::<u64>().unwrap() + 1).to_string();
    let other_addr = addr(&lottery::state::Payout::Schnorr([0x42; 32]));
    let mut n = 0;
    for name in ["buy b=1 ref", "claim k=4", "close k=4", "draw k=4", "void k=4", "request_withdraw 2 partial", "request_withdraw 0 whole", "settle withdrawal"] {
        let (_, inp) = find(name);
        let b = inp.run().unwrap();
        let signed = wallet(&b);
        api::assemble(Net::Testnet10, &AssembleInput { built: b.clone(), signed: signed.clone() }).unwrap_or_else(|e| panic!("{name}: honest: {e}"));
        let last = b.summary.outputs.len() - 1;
        // The first covenant input whose run needs no chain (a draw's input 0, the drawer,
        // calls OpChainblockSeqCommit: see the drawer case below).
        let ci = if name.starts_with("draw") { 1 } else { 0 };
        let oa = other_addr.clone();
        let tampers: Vec<Tamper> = vec![
            ("fee", Box::new(move |t| bump(&mut t.fee))),
            ("id", Box::new(|t| t.id = "00".repeat(32))),
            ("masses.compute", Box::new(move |t| bump(&mut t.masses.compute))),
            ("masses.transient", Box::new(move |t| bump(&mut t.masses.transient))),
            ("masses.storage", Box::new(move |t| bump(&mut t.masses.storage))),
            ("masses.priority", Box::new(move |t| bump(&mut t.masses.priority))),
            ("units of a covenant input", Box::new(move |t| bump(&mut t.units[ci]))),
            ("units of the last input (a signed one)", Box::new(move |t| {
                let k = t.units.len() - 1;
                bump(&mut t.units[k])
            })),
            ("summary.walletIn", Box::new(move |t| bump(&mut t.summary.wallet_in))),
            ("summary.walletBack", Box::new(move |t| bump(&mut t.summary.wallet_back))),
            ("an output value", Box::new(move |t| bump(&mut t.summary.outputs[0].value))),
            ("an output address", Box::new(move |t| t.summary.outputs[last].address = Some(oa.clone()))),
            ("an output's covenant flag", Box::new(move |t| t.summary.outputs[last].covenant ^= true)),
            ("an output's toWallet flag", Box::new(move |t| t.summary.outputs[0].to_wallet ^= true)),
            ("an output index", Box::new(move |t| t.summary.outputs[0].index = 7)),
            ("an extra summary output", Box::new(|t| {
                let o = t.summary.outputs[0].clone();
                t.summary.outputs.push(o)
            })),
            ("a summary output removed", Box::new(|t| {
                t.summary.outputs.pop();
            })),
            ("a sign item's address", Box::new(|t| t.sign[0].address = None)),
            ("the transaction reformatted", Box::new(|t| t.transaction = serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(&t.transaction).unwrap()).unwrap())),
        ];
        for (what, f) in &tampers {
            let mut t = b.clone();
            f(&mut t);
            assert_ne!(t, b, "{name}: {what}: no change");
            let e = api::assemble(Net::Testnet10, &AssembleInput { built: t.clone(), signed: signed.clone() }).err().unwrap_or_else(|| panic!("{name}: {what}: assembled"));
            assert_eq!(e.code, Code::BuiltMismatch, "{name}: {what}: {e}");
            #[cfg(not(feature = "mainnet"))]
            {
                let sk = hex(&WALLET_SK);
                let e = web::quick_wallet::quick_wallet_sign(Net::Testnet10, &QuickSignInput { built: t, secret_key: sk }).unwrap_err();
                assert_eq!(e.code, Code::BuiltMismatch, "{name}: {what}: quickWalletSign: {e}");
            }
            n += 1;
        }
    }
    // The known limit: the drawer's input runs OpChainblockSeqCommit, which needs the node's
    // chain, so its units are bound by its committed budget only (budget steps of 10,000
    // units). A change that keeps the budget passes; one that changes it is refused.
    let (_, inp) = find("draw k=4");
    let b = inp.run().unwrap();
    let signed = wallet(&b);
    let at = |d: u64| {
        let mut t = b.clone();
        t.units[0] = (t.units[0].parse::<u64>().unwrap() + d).to_string();
        api::assemble(Net::Testnet10, &AssembleInput { built: t, signed: signed.clone() })
    };
    let units0: u64 = b.units[0].parse().unwrap();
    if units0 % 10_000 != 9_999 {
        assert!(at(1).is_ok(), "the drawer's units within one budget step are not recomputed");
    }
    assert_eq!(at(10_000).unwrap_err().code, Code::BuiltMismatch, "the drawer's units are bound by the budget");
    println!("{n} tampered BuiltTx refused (built_mismatch) by assemble and quickWalletSign");
}

/// Item 3 (reviewer P1, P2): the page creates vault positions only for owners it can
/// withdraw for. Before the fix `request_deposit` built for any `kaspatest:` owner,
/// including a P2SH nobody's page wallet can sign (P2), and the quick wallet could not sign
/// the ECDSA or P2SH owner inputs that such positions need (P1, still true, so they must
/// not be created).
#[cfg(not(feature = "mainnet"))]
#[test]
fn deposit_owners_are_withdrawable_from_the_page() {
    use lottery::index::State;
    use lottery::state::{KIND_DEPOSIT, Payout, RequestState};
    use web::quick_wallet::{quick_wallet_new, quick_wallet_sign};
    let n = Net::Testnet10;
    let k = quick_wallet_new(n).unwrap();
    let x: [u8; 32] = web::convert::hash32("k", &k.public_key).unwrap();
    let sk = secp256k1::SecretKey::from_slice(&web::convert::hash32("sk", &k.secret_key).unwrap()).unwrap();
    let ecdsa: [u8; 33] = secp256k1::PublicKey::from_secret_key(secp256k1::SECP256K1, &sk).serialize();
    let multisig = [vec![0x52u8, 0x20], vec![1; 32], vec![0x20], vec![2; 32], vec![0x52, 0xae]].concat();
    let multisig_addr = n.address(&kaspa_txscript::pay_to_script_hash_script(&multisig)).unwrap();
    let (_, dep) = find("request_deposit");
    let Input::Deposit(d) = dep else { unreachable!() };
    for (what, owner) in [
        ("an ECDSA P2PK owner (the same key)", addr(&Payout::Ecdsa(ecdsa))),
        ("a P2SH-wrapped Schnorr owner (the same key)", addr(&lottery::buy::OwnerPub::P2sh(x).payout())),
        ("a P2SH 2-of-2 owner (reviewer P2)", multisig_addr),
    ] {
        let e = api::build_request_deposit(n, &DepositInput { owner, ..d.clone() }).unwrap_err();
        assert_eq!(e.code, Code::OwnerType, "{what}: {e}");
    }
    // The accepted type end to end: a deposit for the quick wallet's own address ...
    let built = api::build_request_deposit(n, &DepositInput { owner: k.address.clone(), ..d.clone() }).unwrap();
    let u = api::unsigned_of(n, &built).unwrap();
    let cov = kaspa_hashes::Hash::from_bytes(web::convert::hash32("cov", &d.covenant_id).unwrap());
    let vault = lottery::web::cases::vault(&web::convert::obj("vault", &d.vault).unwrap(), &cov).unwrap();
    let req = RequestState::new(vault.round + 1, KIND_DEPOSIT, d.amount.parse().unwrap(), &Payout::Schnorr(x));
    output_of(&u.tx, &State::Request(req.clone()));
    // ... settles into a position of that owner (the settle covenant runs in the engine) ...
    let mut sc = lottery::vault::SettleCase::deposit();
    sc.request.owner_class = req.owner_class;
    sc.request.owner_key = req.owner_key;
    let pos = sc.expected().position.expect("a deposit settlement mints a position");
    assert_eq!(Payout::from_fields(pos.owner_class, &pos.owner_key), Payout::Schnorr(x));
    let wspk = lottery::buy::spk_of(&WALLET_SK);
    let coin = lottery::web::build::Coin { outpoint: harness::outpoint(2), entry: kaspa_consensus_core::tx::UtxoEntry::new(10 * lottery::buy::KAS, wspk.clone(), 1_300_000, false, None) };
    let su = lottery::web::build::settle(&sc, harness::outpoint(0), harness::outpoint(1), &lottery::web::build::Funds { coins: vec![coin], change: wspk, fee: sc.fee }).unwrap();
    let (_, wd) = find("request_withdraw 0 whole");
    let Input::Withdraw(w) = wd else { unreachable!() };
    let wcov = kaspa_hashes::Hash::from_bytes(web::convert::hash32("cov", &w.covenant_id).unwrap());
    let position = created(&su.tx, output_of(&su.tx, &State::Position(pos.clone())), &wcov, 1_400_000, &State::Position(pos.clone()));
    // ... and that position is withdrawn whole from the page: the owner coin at the quick
    // wallet's address, signed by the quick wallet, assembled (every input in the engine).
    let mut owner_coin = w.owner.coin.clone();
    owner_coin.script_public_key = api::script_of(n, &k.address).unwrap();
    let wi = WithdrawInput {
        position,
        shares: pos.shares.to_string(),
        owner: OwnerInput { coin: owner_coin, kind: KeyKind::Schnorr, public_key: k.public_key.clone() },
        funds: Funds { change: k.address.clone(), ..w.funds.clone() },
        ..w.clone()
    };
    let wb = api::build_request_withdraw(n, &wi).unwrap();
    assert!(wb.sign.iter().all(|s| s.kind == KeyKind::Schnorr && s.public_key == k.public_key));
    let signed = quick_wallet_sign(n, &QuickSignInput { built: wb.clone(), secret_key: k.secret_key.clone() }).unwrap();
    let s = api::assemble(n, &AssembleInput { built: wb.clone(), signed }).unwrap();
    assert_eq!(s.id, wb.id);
    assert!(wb.summary.outputs.iter().any(|o| o.to_wallet), "the change comes back to the owner's wallet");
    println!("deposit -> settle -> withdraw for {}…: assembled {}", &k.address[..20], &s.id[..16]);
    // Owner types the page cannot create stay unsignable by the quick wallet (positions from
    // other tools; D67 allows them at the covenant level): ECDSA and P2SH owner inputs.
    for name in ["request_withdraw 1 partial", "request_withdraw 2 partial"] {
        let (_, i) = find(name);
        let Input::Withdraw(w) = i else { unreachable!() };
        let b = api::build_request_withdraw(n, &w).unwrap();
        let e = quick_wallet_sign(n, &QuickSignInput { built: b, secret_key: hex(&lottery::vault::OWNER_SK) }).unwrap_err();
        assert_eq!(e.code, Code::SignedMissing, "{name}");
    }
}

/// Note (reviewer P5, build.rs:207, 587-598): coins that cover a buy without change, but
/// leave a change too small to pay for its own output, are refused as `change_dust`, not
/// `funding_short`. The money behaviour is unchanged (still refused; dropping the change
/// would pay it as fee, an owner decision).
#[test]
fn small_change_is_change_dust() {
    let (_, inp) = find("buy b=1 ref");
    let Input::Buy(b) = inp else { unreachable!() };
    let ok = api::build_buy(Net::Testnet10, &b).unwrap();
    let (fee, back, coin): (u64, u64, u64) = (ok.fee.parse().unwrap(), ok.summary.wallet_back.parse().unwrap(), b.funds.coins[0].amount.parse().unwrap());
    let cost = coin - back - fee;
    for extra in [0u64, 1, 1_000, 100_000, 10_000_000, 50_000_000, 200_000_000] {
        let mut x = b.clone();
        x.funds.coins[0].amount = (cost + fee + extra).to_string();
        let r = api::build_buy(Net::Testnet10, &x);
        println!("coin = cost + fee ({fee}) + {extra}: {:?}", r.as_ref().map(|b| (&b.fee, &b.summary.wallet_back)).map_err(|e| (e.code, &e.detail)));
        match r {
            Ok(_) => assert!(extra >= 50_000_000, "{extra}: built"),
            Err(e) => assert_eq!(e.code, Code::ChangeDust, "{extra}: {e}"),
        }
    }
    let mut x = b.clone();
    x.funds.coins[0].amount = (cost - 1).to_string();
    assert_eq!(api::build_buy(Net::Testnet10, &x).unwrap_err().code, Code::FundingShort);
}

/// Note (dto.rs:412): `Debug` of the quick wallet's key types never prints the secret.
#[cfg(not(feature = "mainnet"))]
#[test]
fn quick_keys_are_redacted_in_debug() {
    let k = web::quick_wallet::quick_wallet_new(Net::Testnet10).unwrap();
    let (_, inp) = find("buy b=1 ref");
    let b = inp.run().unwrap();
    for s in [format!("{k:?}"), format!("{:?}", QuickSignInput { built: b, secret_key: k.secret_key.clone() })] {
        assert!(!s.contains(&k.secret_key), "the secret key is printed: {}", &s[..s.len().min(120)]);
        assert!(s.contains(web::dto::REDACTED));
    }
    assert!(format!("{k:?}").contains(&k.public_key), "the public parts still print");
}
