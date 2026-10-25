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
//! - `quick_wallet_signs_what_it_owns`, `views_decode_every_role`, `verify_roots_and_numbers`.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{GOLDEN, Input, RATE, addr, digest_line, equivalent, input, with_fee};
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
        let got = api::unsigned_of(&b).unwrap_or_else(|e| panic!("{name}: unsigned_of: {e}"));
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

#[test]
#[ignore]
fn write_golden() {
    let lines = digest_lines();
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/w1_api.txt"), lines.join("\n") + "\n").unwrap();
    println!("wrote {} lines", lines.len());
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

#[test]
fn verify_roots_and_numbers() {
    let (_, call, _) = all_inputs().into_iter().find(|(n, _, _)| n == "draw k=4").unwrap();
    let Call::Draw { cov, marker, purchases, proof, controller, .. } = call else { unreachable!() };
    let ri = RootsInput { m: 0, purchases: purchases.iter().map(|p| hex(p)).collect(), against: web::convert::obj_dto(&marker), covenant_id: hex(&cov.as_bytes()) };
    let m = lottery::web::cases::marker(&marker, &cov).unwrap();
    let ok = api::check_roots(&RootsInput { m: m.m as u32, ..ri.clone() }).unwrap();
    assert_eq!(ok.roots.len(), 4);
    let mut bad = ri.clone();
    bad.m = m.m as u32;
    bad.purchases[1].push_str("010203040501");
    assert_eq!(api::check_roots(&bad).unwrap_err().code, Code::TreeMismatch);
    // The winning numbers from the proof equal the library's.
    let c = lottery::web::cases::controller(&controller, &cov).unwrap();
    let d = lottery::game::anchor_daa(marker.entry.block_daa_score as i64, c.seed_offset);
    let p = proof.unwrap();
    let w = api::winning_numbers(&WinningInput { covenant_id: hex(&cov.as_bytes()), round: m.round.to_string(), d: d.to_string(), m: m.m as u32, anchor_proof: hex(&p) }).unwrap();
    let want = lottery::web::verify::winning_numbers_from_anchor(&cov.as_bytes(), m.round, &p, d, m.m).unwrap();
    assert_eq!([w.winning.main.to_vec(), vec![w.winning.bonus]].concat(), want.to_vec());
    // A proof for another D is refused.
    let e = api::winning_numbers(&WinningInput { covenant_id: hex(&cov.as_bytes()), round: m.round.to_string(), d: (d + 1_000_000).to_string(), m: m.m as u32, anchor_proof: hex(&p) }).unwrap_err();
    assert_eq!(e.code, Code::AnchorInvalid);
}
