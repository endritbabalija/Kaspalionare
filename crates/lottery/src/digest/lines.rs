//! The golden-digest lines (feature `fixtures`, F1 review): the ABI encodings and the fixture
//! transactions; see the parent module.

use std::collections::BTreeMap;

use harness::{ArtifactValue, SilAbiArtifact};
use silverscript_abi::{TypeArtifact, decode_runtime_state_script, encode_contract_entry_sig_script, encode_runtime_state_script};

use super::template_list;
use crate::buy::{BuyCase, Built, Templates, b_max};

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

/// The review digest of a built transaction (F1 review, `covenant-reviewer`): its own digest
/// function, kept byte for byte so the lines are the ones the reviewer computed on the
/// pre-F1 commit `6633ba3` and on `b3b3520` (equal on both). `finish()` twice, random-nonce
/// signature scripts replaced by their length, then the transaction, entries, masses and
/// storage-mass commit, plus the run under the committed budgets.
pub fn review_digest(b: &Built) -> String {
    let h16 = |b: &[u8]| blake2b_simd::Params::new().hash_length(16).hash(b).to_hex().to_string();
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
    let run = f1.run().map(|_| "ok".to_string()).unwrap_or_else(|(i, e)| format!("fail@{i}:{e:?}"));
    format!(
        "{}in {}out units={:?} storage={} run={} {}",
        tx.inputs.len(),
        tx.outputs.len(),
        f1.units,
        m.storage,
        run,
        h16(format!("{tx:?}
{:?}
{m:?}
{}", f1.entries, f1.tx.storage_mass()).as_bytes())
    )
}

/// (name, digest) lines over the builder shapes the 77 original lines miss (F1 review): the
/// return with a pool, each owner class's withdrawal (partial, whole, owner second among the
/// plain inputs), the final claim, fabricated jackpot, second and lower-tier claims, a draw
/// absorbing three expired pools, a draw with jackpot and second-prize holders, a void at
/// round 3, a buy without change, and both converting draws. Appended to the golden file.
pub fn review_digests() -> Vec<(String, String)> {
    use crate::buy::{KAS, OwnerKey};
    use crate::claim::{ClaimCase, ReturnCase};
    use crate::draw::DrawCase;
    use crate::state::{TIER_JACKPOT, TIER_LOWER0, TIER_SECOND};
    use crate::vault::{OWNER_SK, WithdrawCase};
    let t = Templates::get(b_max());
    let mut out: Vec<(String, String)> = vec![];
    let mut add = |n: &str, b: Built| out.push((format!("review {n}"), review_digest(&b)));
    add("return with_pool", ReturnCase::with_pool().build());
    for key in [OwnerKey::Schnorr(OWNER_SK), OwnerKey::Ecdsa(OWNER_SK), OwnerKey::P2sh(OWNER_SK)] {
        let c = WithdrawCase::new_with(&t, key.clone());
        add(&format!("withdraw {:?} partial", key.payout().class()), c.build());
        add(&format!("withdraw {:?} whole", key.payout().class()), WithdrawCase::new_with(&t, key.clone()).whole().build());
        let mut c2 = WithdrawCase::new_with(&t, key.clone());
        c2.owner_at = 1;
        c2.funding = vec![crate::close::FundingIn { value: 3 * KAS, daa: 1_100_000 }, crate::close::FundingIn { value: 2 * KAS, daa: 1_100_000 }];
        add(&format!("withdraw {:?} owner second", key.payout().class()), c2.build());
    }
    let mut c = ClaimCase::new(4);
    c.make_final();
    add("claim k=4 final", c.build());
    for (tier, holders) in [(TIER_JACKPOT, vec![1u16, 2]), (TIER_SECOND, vec![3u16]), (TIER_LOWER0 + 2, vec![1u16, 1, 5])] {
        add(&format!("claim fabricated tier {tier}"), ClaimCase::fabricated(4, 10, tier, 20_000_000, &holders).build());
    }
    let mut d = DrawCase::at_round(4, 3);
    d.absorb(3);
    add("draw r=3 absorb 3", d.build());
    let mut d = DrawCase::new(16);
    d.hold_jackpot(2, 3);
    let b2 = d.other_bonus(0);
    d.hold(5, b2, 4);
    add("draw K=16 jackpot+second winners", d.build());
    let mut v = DrawCase::new_void(4);
    v.marker.round = 3;
    v.drawer.round = 3;
    v.controller.round = 4;
    v.vault.round = 2;
    add("void K=4", v.build());
    let mut b = BuyCase::new(b_max());
    b.bundle = (0..4u32).map(|i| crate::combo::Combo::new(crate::combo::unrank(99 + i * 7_001), 2 + i as u8)).collect();
    b.change = false;
    b.funding = vec![4 * b.shard.price as u64 + 4 * b.shard.d as u64 + b.fee];
    add("buy b=4 no change", b.build());
    let mut cv = DrawCase::new(4);
    cv.vault.pend_dep = 30 * KAS as i64;
    cv.vault.pend_wsh = cv.vault.total_shares / 100;
    add("draw K=4 converting (deposit + withdrawal pending)", cv.build());
    let mut cd = DrawCase::new(4);
    cd.vault.pend_dep = crate::game::MIN_DEPOSIT;
    add("draw K=4 converting (deposit only)", cd.build());
    out
}

/// Every digest line, `name: digest`: ABI first, then the fixture transactions, then the F1
/// review's shapes (appended; the first 77 lines are the pre-F1 golden lines).
pub fn all_lines() -> Vec<String> {
    abi_digests().into_iter().chain(tx_digests()).chain(review_digests()).map(|(n, d)| format!("{n}: {d}")).collect()
}
