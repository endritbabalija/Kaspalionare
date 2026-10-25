//! Byte digests of everything the builders produce, for byte-identity checks (front-end F1).
//!
//! - `abi_digests`: for every embedded template, the ABI's output on deterministic
//!   arguments: each entry's signature script (several argument sets per entry, including
//!   push-size and integer-encoding edge cases and error cases), each runtime state's encoding,
//!   its decode, and decodes of altered state bytes. These are the only bytes
//!   `silverscript-abi` produces, so they pin its encoding.
//! - `tx_digests`: the transactions of the builders' fixture cases (`BuyCase::new` and the
//!   other `*Case` defaults from M1 to M8), with their UTXO entries, script units and
//!   masses. Signatures over random nonces differ per run; an input whose signature script
//!   differs between two `finish()` calls is replaced by its length.
//!
//! The golden file `tests/fixtures/f1_digests.txt` was written before the F1 ABI change
//! (`silverscript-abi` built against rusty-kaspa v2.1.0); `tests/f1_bytes.rs` compares
//! against it. Part B's W1 runs the same functions in wasm and compares with the same file.

use std::collections::BTreeMap;

use harness::{ArtifactValue, SilAbiArtifact};
use silverscript_abi::{TypeArtifact, decode_runtime_state_script, encode_contract_entry_sig_script, encode_runtime_state_script};

use crate::buy::{BuyCase, Built, Template, Templates, b_max};

fn seed(name: &str) -> u64 {
    let d = blake2b_simd::Params::new().hash_length(32).hash(name.as_bytes());
    u64::from_le_bytes(d.as_bytes()[..8].try_into().unwrap())
}

fn h(bytes: &[u8]) -> String {
    let d = blake2b_simd::Params::new().hash_length(32).hash(bytes);
    d.to_hex().to_string()
}

/// Deterministic xorshift64* generator (no dependency, same in wasm and native).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

/// Integers at the script-number encoding boundaries (OP_0, OP_1..OP_16, OP_1NEGATE, the
/// sign bit at every byte width, the 8-byte limit, and i64::MIN, which the ABI refuses).
const INTS: [i64; 30] = [
    0,
    1,
    -1,
    2,
    16,
    17,
    -2,
    -16,
    -17,
    75,
    76,
    127,
    128,
    -127,
    -128,
    -129,
    255,
    256,
    32_767,
    32_768,
    -32_768,
    65_535,
    8_388_607,
    8_388_608,
    2_147_483_647,
    2_147_483_648,
    1 << 40,
    i64::MAX,
    -i64::MAX,
    i64::MIN,
];

/// Byte-string lengths around the push-opcode boundaries (OP_DATA_75, OP_PUSHDATA1/2).
const LENS: [usize; 12] = [0, 1, 2, 20, 32, 75, 76, 255, 256, 520, 521, 1_000];

fn struct_fields<'a>(art: &'a SilAbiArtifact, name: &str, state: &'a [(String, TypeArtifact)]) -> Vec<(String, TypeArtifact)> {
    if name == "State" {
        return state.to_vec();
    }
    art.structs.get(name).map(|s| s.fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect()).unwrap_or_default()
}

/// A value of type `ty`. Variant 0 takes the first edge case of each kind, later variants
/// rotate through the edge cases and random values.
fn value(art: &SilAbiArtifact, state: &[(String, TypeArtifact)], ty: &TypeArtifact, rng: &mut Rng, variant: usize, depth: usize) -> ArtifactValue {
    use ArtifactValue::*;
    let pick_int = |rng: &mut Rng| if variant % 3 == 2 { rng.next() as i64 >> rng.below(63) } else { INTS[(variant * 7 + rng.below(INTS.len() as u64) as usize) % INTS.len()] };
    let small_bytes = |rng: &mut Rng, len: usize| -> Vec<u8> {
        // Single bytes 0x00..0x11 and 0x81 hit the small-integer push forms.
        if len == 1 && variant % 2 == 1 { vec![[0x00, 0x01, 0x05, 0x10, 0x11, 0x81, 0x80][rng.below(7) as usize]] } else { rng.bytes(len) }
    };
    match ty {
        TypeArtifact::Int | TypeArtifact::Temporal => Int(pick_int(rng)),
        TypeArtifact::Bool => Bool(rng.below(2) == 1),
        TypeArtifact::Byte => Byte(rng.next() as u8),
        TypeArtifact::Bytes => {
            let len = if variant == 0 { 0 } else { LENS[(variant + rng.below(LENS.len() as u64) as usize) % LENS.len()] };
            Bytes(small_bytes(rng, len))
        }
        TypeArtifact::Text => Text((0..rng.below(40)).map(|_| (b'a' + rng.below(26) as u8) as char).collect()),
        TypeArtifact::Pubkey => Bytes(rng.bytes(32)),
        TypeArtifact::Sig => Bytes(rng.bytes(65)),
        TypeArtifact::Datasig => Bytes(rng.bytes(64)),
        TypeArtifact::FixedBytes { len } => Bytes(small_bytes(rng, *len)),
        TypeArtifact::FixedArray { item, len } => Array((0..*len).map(|_| value(art, state, item, rng, variant, depth + 1)).collect()),
        TypeArtifact::DynamicArray { item } => {
            let n = if depth > 2 { 0 } else { rng.below(4) as usize };
            Array((0..n).map(|_| value(art, state, item, rng, variant, depth + 1)).collect())
        }
        TypeArtifact::Struct { name } => Object(
            struct_fields(art, name, state).iter().map(|(n, t)| (n.clone(), value(art, state, t, rng, variant, depth + 1))).collect(),
        ),
    }
}

fn outcome(r: Result<Vec<u8>, impl std::fmt::Display>) -> String {
    match r {
        Ok(b) => format!("ok {} {}", b.len(), h(&b)),
        Err(e) => format!("err {e}"),
    }
}

/// The templates `abi_digests` covers, by name (`Templates` field order).
pub fn template_list(t: &Templates) -> Vec<(&'static str, &Template)> {
    vec![
        ("ticket", &*t.ticket),
        ("controller", &*t.controller),
        ("shard", &*t.shard),
        ("params", &*t.params),
        ("marker", &*t.marker),
        ("vault", &*t.vault),
        ("pool", &*t.pool),
        ("record", &*t.record),
        ("position", &*t.position),
        ("request", &*t.request),
        ("drawer", &*t.drawer),
    ]
}

/// With `F1_DIGEST_VERBOSE` set, print the lines behind a digest (to find a differing variant).
fn verbose(name: &str, lines: &[String]) {
    if std::env::var_os("F1_DIGEST_VERBOSE").is_some() {
        for l in lines {
            println!("{name} | {l}");
        }
    }
}

/// Variants per entry and per state.
const VARIANTS: usize = 12;

/// (name, digest) lines over the ABI's encodings of the committed templates.
pub fn abi_digests() -> Vec<(String, String)> {
    let t = Templates::get(b_max());
    let mut out = vec![];
    for (tname, tpl) in template_list(&t) {
        let art = &tpl.art;
        let (cname, c) = art.contracts.first_key_value().unwrap();
        out.push((format!("{tname} template_hash"), h(&silverscript_abi::template_hash(&tpl.prefix, &tpl.suffix))));
        out.push((format!("{tname} artifact json"), h(serde_json::to_string(art).unwrap().as_bytes())));
        let state: Vec<(String, TypeArtifact)> = c.runtime_state.fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect();
        for (ename, entry) in &c.entries {
            let mut lines = vec![];
            for v in 0..VARIANTS {
                let mut rng = Rng::new(seed(&format!("{tname}/{ename}/{v}")));
                let args: Vec<ArtifactValue> = entry.params.iter().map(|p| value(art, &state, &p.ty, &mut rng, v, 0)).collect();
                lines.push(outcome(encode_contract_entry_sig_script(art, cname, ename, &args)));
            }
            // Error cases: one argument short, and a type mismatch in the first argument.
            let mut rng = Rng::new(99);
            let mut args: Vec<ArtifactValue> = entry.params.iter().map(|p| value(art, &state, &p.ty, &mut rng, 0, 0)).collect();
            if !args.is_empty() {
                args.pop();
                lines.push(outcome(encode_contract_entry_sig_script(art, cname, ename, &args)));
                let mut rng = Rng::new(98);
                let mut args: Vec<ArtifactValue> = entry.params.iter().map(|p| value(art, &state, &p.ty, &mut rng, 0, 0)).collect();
                args[0] = ArtifactValue::Text("x".into());
                lines.push(outcome(encode_contract_entry_sig_script(art, cname, ename, &args)));
            }
            verbose(&format!("{tname} entry {ename}"), &lines);
            out.push((format!("{tname} entry {ename}"), h(lines.join("\n").as_bytes())));
        }
        // Runtime state: encode, decode back, and decode altered bytes.
        let mut lines = vec![];
        for v in 0..VARIANTS {
            let mut rng = Rng::new(seed(&format!("{tname}/state/{v}")));
            let values: BTreeMap<String, ArtifactValue> =
                state.iter().map(|(n, ty)| (n.clone(), value(art, &state, ty, &mut rng, v, 0))).collect();
            let enc = encode_runtime_state_script(art, &c.runtime_state, &values);
            lines.push(outcome(enc.clone().map_err(|e| e.to_string())));
            if let Ok(bytes) = enc {
                match decode_runtime_state_script(art, &c.runtime_state, &bytes) {
                    Ok(back) => lines.push(format!("decode {}", if back == values { "round trip".to_string() } else { h(format!("{back:?}").as_bytes()) })),
                    Err(e) => lines.push(format!("decode err {e}")),
                }
                // Altered bytes: one flipped byte, a truncation and a trailing push.
                let mut flipped = bytes.clone();
                if !flipped.is_empty() {
                    let i = rng.below(flipped.len() as u64) as usize;
                    flipped[i] ^= 1 << rng.below(8);
                }
                let truncated = bytes[..bytes.len() / 2].to_vec();
                let mut trailing = bytes.clone();
                trailing.extend([0x01, 0x07]);
                for b in [flipped, truncated, trailing] {
                    lines.push(match decode_runtime_state_script(art, &c.runtime_state, &b) {
                        Ok(vals) => format!("altered ok {}", h(format!("{vals:?}").as_bytes())),
                        Err(e) => format!("altered err {e}"),
                    });
                }
            }
        }
        verbose(&format!("{tname} state"), &lines);
        out.push((format!("{tname} state"), h(lines.join("\n").as_bytes())));
    }
    out
}

/// The digest of a built transaction: `finish()` twice, signature scripts that differ between
/// the two runs (random-nonce signatures) replaced by their length; then the transaction, the
/// entries, the script units and the masses.
pub fn built_digest(b: &Built) -> String {
    let f1 = b.finish();
    let f2 = b.finish();
    let mut tx = f1.tx.clone();
    for (i, inp) in tx.inputs.iter_mut().enumerate() {
        if inp.signature_script != f2.tx.inputs[i].signature_script {
            let n = inp.signature_script.len();
            inp.signature_script = format!("signed:{n}").into_bytes();
        }
    }
    let m = harness::masses(&f1.tx, &f1.entries);
    let text = format!("{tx:?}\n{:?}\n{:?}\n{m:?}\nmass {}", f1.entries, f1.units, f1.tx.storage_mass());
    format!("{} in {} out units {} storage {} {}", tx.inputs.len(), tx.outputs.len(), f1.units.iter().sum::<u64>(), m.storage, h(text.as_bytes()))
}

/// (name, digest) lines over the transactions of the builders' fixture cases.
pub fn tx_digests() -> Vec<(String, String)> {
    use crate::claim::{ClaimCase, RefreshCase, RefreshObj, ReturnCase};
    use crate::close::CloseCase;
    use crate::draw::DrawCase;
    use crate::genesis::{GENESIS_FUNDING, GenesisCase};
    use crate::params::SetParamsCase;
    use crate::vault::{DepositCase, SettleCase, WithdrawCase};
    let mut out: Vec<(String, String)> = vec![];
    let mut add = |name: &str, b: Built| out.push((name.to_string(), built_digest(&b)));

    let buy = BuyCase::new(b_max());
    add("buy b=1", buy.build());
    let mut c = buy.clone();
    c.with_ref = true;
    add("buy b=1 referral", c.build());
    let mut c = buy.clone();
    c.bundle = (0..4u32).map(|i| crate::combo::Combo::new(crate::combo::unrank(7 + i * 13_001), 1 + i as u8)).collect();
    c.with_ref = true;
    c.funding = vec![60 * crate::buy::KAS, 50 * crate::buy::KAS];
    add("buy b=4 referral two funding", c.build());

    add("close K=16", CloseCase::new(16).build());
    add("close K=4", CloseCase::new(4).build());
    add("draw K=16", DrawCase::new(16).build());
    add("draw K=4", DrawCase::new(4).build());
    add("void K=16", DrawCase::new_void(16).build());

    let claim = ClaimCase::new(1);
    add("claim k=1", claim.build());
    add("claim k=16", ClaimCase::new(16).build());
    add("return_deposit after", ReturnCase::after().build());
    add("refresh ticket", RefreshCase::new(RefreshObj::Ticket(claim.ticket.clone()), claim.ticket_value, 1_500_000).build());
    add("refresh pool", RefreshCase::new(RefreshObj::Pool(claim.pool.clone()), claim.pool_value, 1_500_000).build());

    add("set_params", SetParamsCase::new().build());
    add("request_deposit", DepositCase::new().build());
    add("request_withdraw", WithdrawCase::new().build());
    add("settle deposit", SettleCase::deposit().build());
    add("settle withdrawal", SettleCase::withdrawal().build());

    let mut l = crate::ledger::Ledger::new();
    let op = l.fund(GENESIS_FUNDING, crate::buy::funding_spk(), 100);
    let e = l.get(&op).unwrap().clone();
    add("genesis K=4", GenesisCase::new(4, vec![(op, e.clone())]).build());
    add("genesis K=16", GenesisCase::new(16, vec![(op, e)]).build());
    out
}

/// Every digest line, `name: digest`, ABI first.
pub fn all_lines() -> Vec<String> {
    abi_digests().into_iter().chain(tx_digests()).map(|(n, d)| format!("{n}: {d}")).collect()
}
