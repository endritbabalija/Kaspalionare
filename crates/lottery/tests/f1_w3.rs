//! Front-end F1 part B, spec frontend 8 W3: every decoder and builder of the web-facing
//! layer (`lottery::web`), fed truncated, oversized or random bytes and out-of-range
//! values, returns an error, never a panic.
//!
//! A wasm page runs with `panic = "abort"`: any panic kills the worker. Natively a panic
//! unwinds, so each call here runs under `catch_unwind`, and an unwind (a panic that would
//! have aborted the page) fails the test with the input that caused it. Run in debug (the
//! default `cargo test`), the arithmetic overflow checks are on too, so an overflow that
//! would wrap silently in a release wasm also fails here.
//!
//! Deterministic: a fixed-seed xorshift generator; `F1_W3_ITERS` scales the iteration
//! counts (default 1). Coverage (each with the counts of Ok and Err outcomes printed):
//! - decoders: `decode_state` for all 11 roles, `decode_redeem`, `anchor_proof`,
//!   `check_anchor`, `winning_numbers(_from_anchor)`, `rebuild_tree(_bytes)`,
//!   `check_roots`, `payout_of_spk`, `read_wallet`, `read_index_log`,
//!   `read_index_snapshot`, `read_bundle`;
//! - builders: `buy`, `claim`, `claim_from_store` (from a fuzzed `wallet/1`),
//!   `return_deposit` (both paths), `close`, `draw` (draw and void), `request_deposit`,
//!   `request_withdraw`, `settle`, `with_fee`, `w_min`;
//! - signing: `sign::unsigned` (inside every builder), `sign::post_sign` and
//!   `sign::assemble` on fuzzed returned transactions.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use harness::{ArtifactValue, Hash, ScriptPublicKey, Transaction, TransactionOutpoint, UtxoEntry, outpoint};
use lottery::anchor::AnchorProof;
use lottery::buy::{BuyCase, KAS, OwnerKey, OwnerPub, Templates, b_max, spk_of};
use lottery::claim::{ClaimCase, ReturnCase, ReturnPath};
use lottery::close::{CloseCase, FundingIn};
use lottery::combo::Combo;
use lottery::draw::{DrawCase, PoolIn, ResultPool, RoundResult};
use lottery::game::{DAA_MAX, TOTAL_MAX};
use lottery::index::{ROLES, Role, State};
use lottery::smt::Tree;
use lottery::state::Payout;
use lottery::store::{LeafUpdate, OutpointJson, ResultJson, Wallet, WalletPool, WalletTicket};
use lottery::vault::{DepositCase, OWNER_SK, SettleCase, WithdrawCase};
use lottery::web::build::{self, Coin, Funds, OwnerCoin, payout_of_spk};
use lottery::web::{sign, verify};

// ------------------------------------------------------------------------------ harness

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
    fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
    fn rbytes(&mut self, max: u64) -> Vec<u8> {
        let n = self.below(max) as usize;
        self.bytes(n)
    }
    fn arr<const N: usize>(&mut self) -> [u8; N] {
        let mut a = [0u8; N];
        for x in a.iter_mut() {
            *x = self.next() as u8;
        }
        a
    }
    fn i64(&mut self) -> i64 {
        const E: [i64; 18] =
            [0, 1, -1, 2, 15, 16, 255, 256, i64::MIN, i64::MAX, TOTAL_MAX, TOTAL_MAX + 1, TOTAL_MAX - 1, DAA_MAX, DAA_MAX - 1, 1_000_000_000, 10_000, 5_000];
        match self.below(4) {
            0 => self.next() as i64,
            1 => self.below(1_000) as i64 - 10,
            _ => E[self.below(E.len() as u64) as usize],
        }
    }
    fn u64(&mut self) -> u64 {
        const E: [u64; 9] = [0, 1, u64::MAX, u64::MAX / 2, TOTAL_MAX as u64, TOTAL_MAX as u64 + 1, 100_000_000, 1_000_000_000_000, DAA_MAX as u64];
        match self.below(3) {
            0 => self.next(),
            1 => self.below(10_000_000_000),
            _ => E[self.below(E.len() as u64) as usize],
        }
    }
    fn u8(&mut self) -> u8 {
        const E: [u8; 7] = [0, 1, 2, 3, 6, 7, 255];
        if self.coin() { self.next() as u8 } else { E[self.below(7) as usize] }
    }
    /// Bytes derived from `b`: truncated, extended, a bit flipped, a byte replaced, a run
    /// inserted or removed, or fully random.
    fn mutate_bytes(&mut self, b: &[u8]) -> Vec<u8> {
        let mut v = b.to_vec();
        match self.below(7) {
            0 => v.truncate(self.below(b.len() as u64 + 1) as usize),
            1 => {
                let n = self.below(64) as usize + 1;
                v.extend(self.bytes(n))
            }
            2 if !v.is_empty() => {
                let i = self.below(v.len() as u64) as usize;
                v[i] ^= 1 << self.below(8)
            }
            3 if !v.is_empty() => {
                let i = self.below(v.len() as u64) as usize;
                v[i] = self.next() as u8
            }
            4 => {
                let i = self.below(v.len() as u64 + 1) as usize;
                let n = self.below(9) as usize + 1;
                let ins = self.bytes(n);
                v.splice(i..i, ins);
            }
            5 if !v.is_empty() => {
                let i = self.below(v.len() as u64) as usize;
                let n = (self.below(9) as usize + 1).min(v.len() - i);
                v.drain(i..i + n);
            }
            _ => {
                let n = self.below(2 * b.len() as u64 + 8) as usize;
                v = self.bytes(n)
            }
        }
        v
    }
}

#[derive(Default)]
struct Tally {
    ok: usize,
    err: usize,
}

impl Tally {
    fn add<T, E>(&mut self, r: &Result<T, E>) {
        if r.is_ok() { self.ok += 1 } else { self.err += 1 }
    }
}

thread_local! {
    /// Set while `no_panic` runs a target: the hook stays silent for that panic only.
    static IN_TARGET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Run `f`; an unwind (a panic) fails the test, naming the target and the input.
fn no_panic<T>(target: &str, input: impl FnOnce() -> String, f: impl FnOnce() -> T) -> T {
    IN_TARGET.with(|g| g.set(true));
    let r = catch_unwind(AssertUnwindSafe(f));
    IN_TARGET.with(|g| g.set(false));
    match r {
        Ok(v) => v,
        Err(p) => {
            let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            panic!("W3: {target} panicked ({msg}) on input: {}", input())
        }
    }
}

fn iters(n: usize) -> usize {
    n * std::env::var("F1_W3_ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(1)
}

/// The panics `no_panic` catches are reported once, by `no_panic`; every other panic (a
/// bug in this test) prints as usual.
fn quiet() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !IN_TARGET.with(|g| g.get()) {
                default(info)
            }
        }));
    });
}

fn report(name: &str, t: &Tally) {
    println!("W3 {name}: {} calls, {} ok, {} err, no panic", t.ok + t.err, t.ok, t.err);
}

// ------------------------------------------------------------------------------ states

/// One field of `s` set to a random value of its type (or of another type, or removed).
fn mutate_state(r: &mut Rng, s: &State) -> Option<State> {
    let mut v: BTreeMap<String, ArtifactValue> = s.values();
    let keys: Vec<String> = v.keys().cloned().collect();
    let k = &keys[r.below(keys.len() as u64) as usize];
    let cur = v.get(k).cloned();
    let new = match (cur.as_ref(), r.below(10)) {
        (_, 0) => {
            v.remove(k);
            return State::from_values(s.role(), &v).ok();
        }
        (_, 1) => ArtifactValue::Bool(r.coin()),
        (Some(ArtifactValue::Int(_)), _) => ArtifactValue::Int(r.i64()),
        (Some(ArtifactValue::Byte(_)), _) => ArtifactValue::Byte(r.u8()),
        (Some(ArtifactValue::Bytes(b)), _) => {
            let b = b.clone();
            ArtifactValue::Bytes(if r.coin() { r.bytes(b.len()) } else { r.mutate_bytes(&b) })
        }
        (_, _) => ArtifactValue::Int(r.i64()),
    };
    v.insert(k.clone(), new);
    State::from_values(s.role(), &v).ok()
}

macro_rules! mutate_as {
    ($r:expr, $s:expr, $v:ident) => {
        match mutate_state($r, &State::$v($s.clone())) {
            Some(State::$v(x)) => $s = x,
            _ => {}
        }
    };
}

fn fixture_states() -> Vec<State> {
    let c = CloseCase::new(4);
    let d = DrawCase::new(4);
    let k = ClaimCase::new(4);
    vec![
        State::Controller(c.controller.clone()),
        State::Params(c.params.clone()),
        State::Shard(c.shards[0].state.clone()),
        State::Marker(d.marker.clone()),
        State::Drawer(d.drawer.clone()),
        State::Vault(d.vault.clone()),
        State::Pool(k.pool.clone()),
        State::Ticket(k.ticket.clone()),
        State::Position(WithdrawCase::new().position),
        State::Request(SettleCase::deposit().request),
        State::Record(SettleCase::deposit().record),
    ]
}

// ------------------------------------------------------------------------------ funds

const WALLET_SK: [u8; 32] = [0x21; 32];

fn wallet_spk() -> ScriptPublicKey {
    spk_of(&WALLET_SK)
}

fn coins_of(v: &[(u64, u64)], first: u32) -> Vec<Coin> {
    v.iter().enumerate().map(|(n, &(value, daa))| Coin { outpoint: outpoint(first + n as u32), entry: UtxoEntry::new(value, wallet_spk(), daa, false, None) }).collect()
}

fn random_spk(r: &mut Rng) -> ScriptPublicKey {
    match r.below(5) {
        0 => Payout::Schnorr(r.arr()).spk(),
        1 => Payout::Ecdsa(r.arr()).spk(),
        2 => Payout::P2sh(r.arr()).spk(),
        3 => ScriptPublicKey::new(r.below(3) as u16, r.rbytes(40).into()),
        _ => wallet_spk(),
    }
}

/// `f` with one of its parts changed (coin count, value, DAA, script, covenant ID, coinbase
/// flag, change script, fee), or unchanged.
fn mutate_funds(r: &mut Rng, f: &Funds) -> Funds {
    let mut f = f.clone();
    let n = f.coins.len();
    match r.below(10) {
        0 => f.coins.truncate(r.below(n as u64 + 1) as usize),
        1 => {
            for _ in 0..r.below(10) {
                let i = f.coins.len() as u32 + 30;
                f.coins.push(Coin { outpoint: outpoint(i), entry: UtxoEntry::new(r.u64(), wallet_spk(), r.below(1_000_000), false, None) });
            }
        }
        2 if n > 0 => {
            let i = r.below(n as u64) as usize;
            let e = &f.coins[i].entry;
            f.coins[i].entry = UtxoEntry::new(r.u64(), e.script_public_key.clone(), e.block_daa_score, e.is_coinbase, e.covenant_id);
        }
        3 if n > 0 => {
            let i = r.below(n as u64) as usize;
            let e = &f.coins[i].entry;
            f.coins[i].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), r.u64(), e.is_coinbase, e.covenant_id);
        }
        4 if n > 0 => {
            let i = r.below(n as u64) as usize;
            let e = &f.coins[i].entry;
            f.coins[i].entry = UtxoEntry::new(e.amount, random_spk(r), e.block_daa_score, r.coin(), if r.coin() { Some(Hash::from_bytes(r.arr())) } else { None });
        }
        5 => f.change = random_spk(r),
        6 => f.fee = r.u64(),
        _ => {}
    }
    f
}

fn random_combo(r: &mut Rng, m: i64) -> Combo {
    if r.below(3) == 0 {
        Combo::new(r.arr(), r.u8())
    } else {
        let mut n: Vec<u8> = (1..=30).collect();
        for i in (1..n.len()).rev() {
            n.swap(i, r.below(i as u64 + 1) as usize);
        }
        let mut main: [u8; 5] = [n[0], n[1], n[2], n[3], n[4]];
        main.sort_unstable();
        Combo::new(main, 1 + r.below(m.clamp(1, 255) as u64) as u8)
    }
}

fn random_tree(r: &mut Rng, m: i64) -> Tree {
    // Capped: a tree's m sizes its count vectors (the builders refuse an m out of range
    // before they touch a tree; this keeps the test's own trees allocatable).
    let m = if r.below(4) == 0 { r.below(300) as i64 } else { m.clamp(0, 300) };
    let mut t = Tree::new(m as usize);
    for _ in 0..r.below(4) {
        let c = random_combo(r, m);
        if c.is_canonical(m.clamp(0, 255) as u8) {
            let _ = t.try_increment(c.rank(), c.bonus);
        }
    }
    t
}

fn random_payout(r: &mut Rng) -> Payout {
    match r.below(3) {
        0 => Payout::Schnorr(r.arr()),
        1 => Payout::Ecdsa(r.arr()),
        _ => Payout::P2sh(r.arr()),
    }
}

fn random_ops(r: &mut Rng, n: u32) -> Vec<TransactionOutpoint> {
    let n = match r.below(8) {
        0 => n.saturating_sub(1),
        1 => n + 1,
        _ => n,
    };
    (0..n).map(outpoint).collect()
}

// ------------------------------------------------------------------------------ decoders

#[test]
fn w3_decoders_never_panic() {
    quiet();
    let mut r = Rng(0x5EED_0F30_0001_u64);
    let states = fixture_states();
    let t = Templates::get(b_max());
    let mut tally = Tally::default();
    for s in &states {
        let bytes = verify::state_bytes(s).unwrap();
        let redeem = s.redeem(&t);
        for _ in 0..iters(300) {
            let role = if r.below(4) == 0 { ROLES[r.below(11) as usize] } else { s.role() };
            let b = r.mutate_bytes(&bytes);
            let res = no_panic("decode_state", || format!("{role:?} {}", lottery::store::to_hex(&b)), || verify::decode_state(role, &b));
            tally.add(&res);
            let rd = r.mutate_bytes(&redeem);
            let res = no_panic("decode_redeem", || lottery::store::to_hex(&rd), || verify::decode_redeem(&rd));
            tally.add(&res);
            let spk = random_spk(&mut r);
            let res = no_panic("verify_state", || format!("{role:?} {}", lottery::store::to_hex(&b)), || verify::verify_state(role, &b, &spk));
            tally.add(&res);
        }
    }
    for _ in 0..iters(300) {
        let n = r.below(3000) as usize;
        let b = r.bytes(n);
        let role = ROLES[r.below(11) as usize];
        tally.add(&no_panic("decode_state random", || format!("{role:?} {n} B"), || verify::decode_state(role, &b)));
        tally.add(&no_panic("decode_redeem random", || format!("{n} B"), || verify::decode_redeem(&b)));
    }
    report("state decoders (11 roles, mutated and random bytes)", &tally);

    // The anchor proof and the winning numbers.
    let mut tally = Tally::default();
    let d = DrawCase::new(4);
    let proof = AnchorProof::from_sim(&d.chain(), d.d() as u64).to_bytes();
    let cov = d.cov.as_bytes();
    for _ in 0..iters(1_000) {
        let b = r.mutate_bytes(&proof);
        let (round, dd, m) = (r.i64(), if r.coin() { d.d() } else { r.i64() }, if r.coin() { d.marker.m } else { r.i64() });
        let p = no_panic("anchor_proof", || lottery::store::to_hex(&b), || verify::anchor_proof(&b));
        tally.add(&p);
        if let Ok(p) = p {
            tally.add(&no_panic("check_anchor", || format!("d {dd}"), || verify::check_anchor(&p, dd)));
            tally.add(&no_panic("sim_from_proof", || lottery::store::to_hex(&b), || build::sim_from_proof(&p).map(|_| ())));
        }
        tally.add(&no_panic("winning_numbers_from_anchor", || format!("round {round} d {dd} m {m}"), || verify::winning_numbers_from_anchor(&cov, round, &b, dd, m)));
        let x: [u8; 32] = r.arr();
        tally.add(&no_panic("winning_numbers", || format!("round {round} m {m}"), || verify::winning_numbers(&cov, round, &x, m).map(|_| ())));
    }
    report("anchor proof and winning numbers", &tally);

    // Trees from compact purchases, and roots.
    let mut tally = Tally::default();
    for _ in 0..iters(400) {
        let m = if r.coin() { 10 } else { r.i64() };
        let n = r.below(40) as usize;
        let purchases: Vec<[u8; 6]> = (0..n).map(|_| random_combo(&mut r, m.clamp(2, 50)).bytes()).collect();
        let tr = no_panic("rebuild_tree", || format!("m {m} {purchases:?}"), || verify::rebuild_tree(m, &purchases));
        tally.add(&tr);
        let raw = r.rbytes(100);
        tally.add(&no_panic("rebuild_tree_bytes", || format!("m {m} {}", lottery::store::to_hex(&raw)), || verify::rebuild_tree_bytes(m, &raw).map(|_| ())));
        if let Ok(tr) = tr {
            let roots = r.mutate_bytes(&[tr.root().to_vec(), vec![0; 32 * 15]].concat());
            let k = if r.coin() { 1 } else { r.i64() };
            tally.add(&no_panic("check_roots", || format!("k {k}"), || verify::check_roots(&[tr], &roots, k)));
        }
        let spk = random_spk(&mut r);
        let _ = no_panic("payout_of_spk", || format!("{spk:?}"), || payout_of_spk(&spk));
    }
    // A count at COUNT_MAX: 32,768 purchases of one combination overflow.
    let one = vec![[1u8, 2, 3, 4, 5, 1]; 32_768];
    let res = no_panic("rebuild_tree count overflow", || "32768 x the same".into(), || verify::rebuild_tree(10, &one));
    assert_eq!(res.err().map(|e| e.code), Some(lottery::web::Code::PurchaseInvalid));
    report("trees and roots", &tally);
}

// ------------------------------------------------------------------------------ JSON

/// A random change to a JSON document: one node replaced (an edge number, a string of hex
/// or text, null, a bool, an empty array or object), an array element duplicated or
/// removed, an object key removed; or the text truncated or a character changed.
fn mutate_json(r: &mut Rng, text: &str) -> String {
    if r.below(5) == 0 {
        let mut b = text.as_bytes().to_vec();
        let b2 = r.mutate_bytes(&b);
        b = b2;
        return String::from_utf8_lossy(&b).into_owned();
    }
    let mut v: serde_json::Value = serde_json::from_str(text).unwrap();
    fn paths(v: &serde_json::Value, p: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
        out.push(p.clone());
        match v {
            serde_json::Value::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    p.push(i.to_string());
                    paths(x, p, out);
                    p.pop();
                }
            }
            serde_json::Value::Object(o) => {
                for (k, x) in o {
                    p.push(k.clone());
                    paths(x, p, out);
                    p.pop();
                }
            }
            _ => {}
        }
    }
    let mut all = vec![];
    paths(&v, &mut vec![], &mut all);
    let path = &all[r.below(all.len() as u64) as usize];
    let ptr: String = path.iter().map(|k| format!("/{}", k.replace('~', "~0").replace('/', "~1"))).collect();
    let node = v.pointer_mut(&ptr).unwrap();
    let cur = node.clone();
    let nums: [serde_json::Value; 8] = [0.into(), 1.into(), (-1).into(), ((1u64 << 53) - 1).into(), (1u64 << 53).into(), u64::MAX.into(), i64::MIN.into(), 255.into()];
    *node = match r.below(9) {
        0 => nums[r.below(8) as usize].clone(),
        1 => serde_json::Value::String(lottery::store::to_hex(&r.rbytes(70))),
        2 => serde_json::Value::String("zz".into()),
        3 => serde_json::Value::Null,
        4 => serde_json::Value::Bool(r.coin()),
        5 => serde_json::Value::Array(vec![]),
        6 => match cur.clone() {
            serde_json::Value::Array(mut a) if !a.is_empty() => {
                let x = a[r.below(a.len() as u64) as usize].clone();
                for _ in 0..r.below(20) + 1 {
                    a.push(x.clone());
                }
                serde_json::Value::Array(a)
            }
            other => other,
        },
        7 => match cur.clone() {
            serde_json::Value::Array(mut a) if !a.is_empty() => {
                a.remove(r.below(a.len() as u64) as usize);
                serde_json::Value::Array(a)
            }
            serde_json::Value::Object(mut o) if !o.is_empty() => {
                let k = o.keys().nth(r.below(o.len() as u64) as usize).unwrap().clone();
                o.remove(&k);
                serde_json::Value::Object(o)
            }
            other => other,
        },
        _ => match &cur {
            serde_json::Value::Number(n) => (n.as_i64().unwrap_or(0).wrapping_add(r.i64() % 3)).into(),
            serde_json::Value::String(s) => serde_json::Value::String(lottery::store::to_hex(&r.mutate_bytes(&lottery::store::from_hex(s).unwrap_or_default()))),
            other => other.clone(),
        },
    };
    v.to_string()
}

/// A `wallet/1` that can claim index 0 of `ClaimCase::new(4)`'s ticket: its four shards'
/// purchases replayed as leaf updates, the frozen roots, the ticket, and a round result
/// whose pool is the fixture pool.
fn fixture_wallet() -> (Wallet, TransactionOutpoint, ClaimCase) {
    let t = Templates::get(b_max());
    let c = ClaimCase::new(4);
    let p = &c.pool;
    let mut w = Wallet::new(c.cov, t.b_max, c.ticket.round, p.m, c.trees.len());
    // The fixture trees are built from the fabricated holdings; replay them as purchases.
    let win = [p.win[0], p.win[1], p.win[2], p.win[3], p.win[4], p.win[5]];
    let comb = lottery::claim::combo_for_tier(&win, p.tier, p.m, 0);
    let filler = lottery::claim::combo_matching(&win, 0, false, p.m, 1);
    for (i, tr) in c.trees.iter().enumerate() {
        let mut replay = Tree::new(p.m as usize);
        let held = if i == 0 { vec![comb, filler] } else { vec![lottery::claim::combo_matching(&win, 1, false, p.m, 2 + i)] };
        for cb in held {
            replay.increment(cb.rank(), cb.bonus);
            w.shards[i].push(LeafUpdate { combo: cb.bytes(), count_after: replay.counts(cb.rank())[cb.bonus as usize - 1], root_after: replay.root() });
        }
        assert_eq!(replay.root(), tr.root(), "fixture replay of shard {i}");
    }
    w.frozen_roots = c.trees.iter().map(|tr| tr.root()).collect();
    let top = outpoint(0);
    w.tickets.push(WalletTicket {
        outpoint: OutpointJson::from(top),
        deadline: c.ticket.deadline,
        b: c.ticket.b,
        bundle: c.ticket.bundle.clone(),
        claimed: c.ticket.claimed.clone(),
        payout_class: c.ticket.payout_class,
        payout_key: c.ticket.payout_key,
        value: c.ticket_value,
        daa: c.ticket_daa,
    });
    let pop = outpoint(1);
    let rr = RoundResult {
        round: p.round,
        win: p.win,
        m: p.m,
        k: p.k,
        roots: c.trees.iter().map(|tr| tr.root()).collect(),
        pools: vec![ResultPool { tier: p.tier, slice: p.slice, value: c.pool_value, outpoint: pop }],
        d_claim: p.deadline,
        funder: Payout::from_fields(p.funder_class, &p.funder_key),
        ticket_tpl: p.ticket_tpl,
        reserve: p.reserve as u64,
        record: None,
        x_hash: [0x33; 32],
        mk_daa: 1,
        void: false,
    };
    assert_eq!(rr.pool_state(0), *p, "the fixture pool from the round result (R4)");
    w.record = Some(ResultJson::of(&rr, &t));
    w.pools = vec![WalletPool { tier: p.tier, outpoint: OutpointJson::from(pop), value: c.pool_value, daa: c.pool_daa }];
    (w, top, c)
}

#[test]
fn w3_store_readers_and_store_builders_never_panic() {
    quiet();
    let mut r = Rng(0x5EED_0F30_0002_u64);
    let t = Templates::get(b_max());
    // wallet/1, and a claim built from it (`claim_from_store`).
    let (w, top, c) = fixture_wallet();
    let json = lottery::store::write(&w, lottery::store::WALLET).unwrap();
    let funds = Funds { coins: coins_of(&[(10 * KAS, 2_100_000)], 2), change: wallet_spk(), fee: c.fee };
    let base = verify::read_wallet(&json).unwrap();
    build::claim_from_store(&base, top, &funds).expect("the fixture wallet claims");
    let mut tally = Tally::default();
    let mut built = 0;
    for _ in 0..iters(400) {
        let j = mutate_json(&mut r, &json);
        let res = no_panic("read_wallet", || j.clone(), || verify::read_wallet(&j));
        tally.add(&res);
        if let Ok(w) = res {
            let f = mutate_funds(&mut r, &funds);
            let res = no_panic("claim_from_store", || j.clone(), || build::claim_from_store(&w, top, &f));
            built += res.is_ok() as usize;
            tally.add(&res);
        }
    }
    report(&format!("read_wallet + claim_from_store ({built} claims built)"), &tally);

    // index/1: the log and the snapshot.
    let mut tally = Tally::default();
    let f = BuyCase::new(b_max()).build().finish();
    let a = lottery::index::Accepted::new(f.tx.clone(), 500, &f.entries);
    let log = lottery::store::IndexLog { cov: lottery::buy::COV, lines: vec![lottery::store::IndexLogLine::add(0, Hash::from_bytes([3; 32]), 500, &a)], reanchors: vec![] };
    let jsonl = log.to_jsonl().unwrap();
    verify::read_index_log(&jsonl).expect("the fixture log reads");
    for _ in 0..iters(300) {
        let lines: Vec<&str> = jsonl.lines().collect();
        let i = r.below(lines.len() as u64) as usize;
        let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        out[i] = mutate_json(&mut r, lines[i]);
        let j = out.join("\n");
        tally.add(&no_panic("read_index_log", || j.clone(), || verify::read_index_log(&j)));
    }
    let mut snap = lottery::store::IndexSnapshot::of(&lottery::index::Rebuild::new(t.clone(), lottery::buy::COV, None), None);
    for (n, s) in fixture_states().iter().enumerate() {
        let spk = s.spk(&t);
        snap.live_utxos.push(lottery::store::LiveJson {
            outpoint: OutpointJson::from(outpoint(n as u32)),
            role: s.role(),
            round: Some(s.round()),
            value: KAS,
            daa: 100,
            covenant_id: lottery::buy::COV.as_bytes(),
            spk_version: spk.version(),
            spk: spk.script().to_vec(),
            state: Some(s.redeem(&t)),
        });
    }
    let sj = lottery::store::write(&snap, lottery::store::INDEX).unwrap();
    verify::read_index_snapshot(&sj).expect("the fixture snapshot reads");
    for _ in 0..iters(200) {
        let j = mutate_json(&mut r, &sj);
        tally.add(&no_panic("read_index_snapshot", || j.clone(), || verify::read_index_snapshot(&j)));
    }
    report("index/1 log and snapshot", &tally);

    // bundle/1 (the indexer API's launch bundle), verified locally.
    let mut tally = Tally::default();
    let mut l = lottery::ledger::Ledger::new();
    let op = l.fund(lottery::genesis::GENESIS_FUNDING, lottery::buy::funding_spk(), 100);
    let e = l.get(&op).unwrap().clone();
    let g = lottery::genesis::GenesisCase::new(4, vec![(op, e)]);
    let gf = g.build().finish();
    let live = lottery::bundle::genesis_live(&g, &gf, 200);
    let b = lottery::bundle::Bundle::new(&g, &gf, live, None).unwrap();
    let bj = lottery::store::write(&b, lottery::store::BUNDLE).unwrap();
    verify::read_bundle(&bj).expect("the fixture bundle verifies");
    for _ in 0..iters(150) {
        let j = mutate_json(&mut r, &bj);
        tally.add(&no_panic("read_bundle", || j.clone(), || verify::read_bundle(&j).map(|_| ())));
    }
    report("bundle/1", &tally);
}

// ------------------------------------------------------------------------------ builders

#[test]
fn w3_builders_never_panic() {
    quiet();
    let mut r = Rng(0x5EED_0F30_0003_u64);
    let t = Templates::get(b_max());

    // buy
    let base = BuyCase::new(b_max());
    let bf = Funds { coins: coins_of(&[(100 * KAS, 0)], 1), change: wallet_spk(), fee: base.fee };
    build::buy(&base, outpoint(0), &bf).expect("base buy");
    let mut tally = Tally::default();
    for _ in 0..iters(300) {
        let mut c = base.clone();
        for _ in 0..1 + r.below(3) {
            match r.below(9) {
                0 => mutate_as!(&mut r, c.shard, Shard),
                1 => c.reserve = r.u64(),
                2 => c.shard_daa = r.u64(),
                3 => c.tree = random_tree(&mut r, c.shard.m),
                4 => c.bundle = (0..r.below(7)).map(|_| random_combo(&mut r, c.shard.m)).collect(),
                5 => c.with_ref = r.coin(),
                6 => c.referrer = random_payout(&mut r),
                7 => c.b_max = r.below(8) as usize,
                _ => c.payout = random_payout(&mut r),
            }
        }
        let f = mutate_funds(&mut r, &bf);
        let res = no_panic("buy", || format!("shard {:?} bundle {:?} funds {f:?}", c.shard, c.bundle), || build::buy(&c, outpoint(0), &f));
        tally.add(&res);
    }
    report("buy", &tally);

    // claim
    let base = ClaimCase::new(4);
    let cf = Funds { coins: coins_of(&[(10 * KAS, 2_100_000)], 2), change: wallet_spk(), fee: base.fee };
    build::claim(&base, outpoint(0), outpoint(1), &cf).expect("base claim");
    let mut tally = Tally::default();
    for _ in 0..iters(250) {
        let mut c = base.clone();
        for _ in 0..1 + r.below(3) {
            match r.below(8) {
                0 => mutate_as!(&mut r, c.ticket, Ticket),
                1 => mutate_as!(&mut r, c.pool, Pool),
                2 => c.ticket_value = r.u64(),
                3 => c.pool_value = r.u64(),
                4 => c.idx = r.i64(),
                5 => {
                    let n = r.below(18) as usize;
                    c.trees = (0..n).map(|_| random_tree(&mut r, c.pool.m)).collect()
                }
                6 => {
                    c.ticket_daa = r.u64();
                    c.pool_daa = r.u64()
                }
                _ => c.lock_time = r.u64(),
            }
        }
        let f = mutate_funds(&mut r, &cf);
        let res = no_panic("claim", || format!("ticket {:?} pool {:?} idx {}", c.ticket, c.pool, c.idx), || build::claim(&c, outpoint(0), outpoint(1), &f));
        tally.add(&res);
    }
    report("claim", &tally);

    // return_deposit, both paths
    let mut tally = Tally::default();
    for base in [ReturnCase::after(), ReturnCase::with_pool()] {
        let n_cov = if matches!(base.path, ReturnPath::After) { 1 } else { 2 };
        let rf = Funds { coins: coins_of(&[(10 * KAS, 2_100_000)], n_cov), change: wallet_spk(), fee: base.fee };
        let pool_op = (n_cov == 2).then(|| outpoint(1));
        build::return_deposit(&base, outpoint(0), pool_op, &rf).expect("base return");
        for _ in 0..iters(150) {
            let mut c = base.clone();
            match r.below(5) {
                0 => mutate_as!(&mut r, c.ticket, Ticket),
                1 => c.ticket_value = r.u64(),
                2 => {
                    if let ReturnPath::WithPool { pool, pool_value, pool_daa } = &mut c.path {
                        mutate_as!(&mut r, *pool, Pool);
                        if r.coin() {
                            *pool_value = r.u64();
                            *pool_daa = r.u64();
                        }
                    }
                }
                3 => c.path = ReturnPath::After,
                _ => c.lock_time = r.u64(),
            }
            let po = if r.below(6) == 0 { None } else { pool_op };
            let f = mutate_funds(&mut r, &rf);
            tally.add(&no_panic("return_deposit", || format!("ticket {:?}", c.ticket), || build::return_deposit(&c, outpoint(0), po, &f)));
        }
    }
    report("return_deposit", &tally);

    // close (K = 4)
    let base = CloseCase::new(4);
    let clf = Funds { coins: coins_of(&[(100 * KAS, 900_000)], 6), change: wallet_spk(), fee: base.fee };
    build::close(&base, &(0..6).map(outpoint).collect::<Vec<_>>(), &clf, 1_000_100).expect("base close");
    let mut tally = Tally::default();
    for _ in 0..iters(150) {
        let mut c = base.clone();
        for _ in 0..1 + r.below(3) {
            match r.below(8) {
                0 | 1 => mutate_as!(&mut r, c.controller, Controller),
                2 => mutate_as!(&mut r, c.params, Params),
                3 if !c.shards.is_empty() => {
                    let i = r.below(c.shards.len() as u64) as usize;
                    mutate_as!(&mut r, c.shards[i].state, Shard);
                }
                4 => c.pot = r.u64(),
                5 => {
                    c.controller_daa = r.u64();
                    c.params_daa = r.u64()
                }
                6 => {
                    let n = r.below(18) as usize;
                    let s = base.shards[0].clone();
                    c.shards = vec![s; n];
                }
                _ => c.funder = random_payout(&mut r),
            }
        }
        let n = c.shards.len() as u32 + 2;
        let o = random_ops(&mut r, n);
        let f = mutate_funds(&mut r, &clf);
        let now = if r.coin() { 1_000_100 } else { r.u64() };
        tally.add(&no_panic("close", || format!("controller {:?}", c.controller), || build::close(&c, &o, &f, now)));
    }
    report("close", &tally);

    // draw (K = 4) and its void
    let mut tally = Tally::default();
    let proofed = |mut c: DrawCase| {
        let p = AnchorProof::from_sim(&c.chain(), c.d() as u64);
        c.x_hash = p.x_hash();
        c.proof = Some(p);
        c
    };
    for base in [proofed(DrawCase::new(4)), DrawCase::new_void(4)] {
        let df = Funds { coins: coins_of(&[(100 * KAS, 0)], 4), change: wallet_spk(), fee: base.fee };
        build::draw(&base, &(0..4).map(outpoint).collect::<Vec<_>>(), &df).expect("base draw");
        for _ in 0..iters(120) {
            let mut c = base.clone();
            for _ in 0..1 + r.below(3) {
                match r.below(10) {
                    0 => mutate_as!(&mut r, c.drawer, Drawer),
                    1 | 2 => mutate_as!(&mut r, c.marker, Marker),
                    3 => mutate_as!(&mut r, c.vault, Vault),
                    4 => mutate_as!(&mut r, c.controller, Controller),
                    5 => c.pot = r.u64(),
                    6 => {
                        c.marker_daa = r.u64();
                        c.drawer_daa = r.u64()
                    }
                    7 => {
                        let n = r.below(18) as usize;
                        c.trees = (0..n).map(|_| random_tree(&mut r, c.marker.m)).collect()
                    }
                    8 => {
                        if let Some(p) = &c.proof {
                            let b = r.mutate_bytes(&p.to_bytes());
                            c.proof = AnchorProof::from_bytes(&b).ok();
                        }
                        c.void = r.coin();
                    }
                    _ => {
                        let mut pool = ClaimCase::new(4).pool;
                        mutate_as!(&mut r, pool, Pool);
                        c.absorbed.push(PoolIn { state: pool, remaining: r.u64(), daa: r.u64() });
                    }
                }
            }
            let n = 4 + c.absorbed.len() as u32;
            let o = random_ops(&mut r, n);
            let f = mutate_funds(&mut r, &df);
            tally.add(&no_panic("draw", || format!("marker {:?} vault {:?} void {}", c.marker, c.vault, c.void), || build::draw(&c, &o, &f)));
        }
    }
    report("draw and void", &tally);

    // request_deposit
    let base = DepositCase::new();
    let vf = Funds { coins: coins_of(&[(20 * KAS, 1_100_000)], 1), change: wallet_spk(), fee: base.fee };
    build::request_deposit(&base, outpoint(0), &vf).expect("base deposit");
    let mut tally = Tally::default();
    for _ in 0..iters(200) {
        let mut c = base.clone();
        match r.below(5) {
            0 | 1 => mutate_as!(&mut r, c.vault, Vault),
            2 => c.reserve = r.u64(),
            3 => c.d = r.i64(),
            _ => c.owner = random_payout(&mut r),
        }
        let f = mutate_funds(&mut r, &vf);
        tally.add(&no_panic("request_deposit", || format!("vault {:?} d {}", c.vault, c.d), || build::request_deposit(&c, outpoint(0), &f)));
        tally.add(&no_panic("w_min", || format!("vault {:?}", c.vault), || build::w_min(&c.vault)));
    }
    report("request_deposit and w_min", &tally);

    // request_withdraw (every owner class)
    let mut tally = Tally::default();
    for key in [OwnerKey::Schnorr(OWNER_SK), OwnerKey::Ecdsa(OWNER_SK), OwnerKey::P2sh(OWNER_SK)] {
        let base = WithdrawCase::new_with(&t, key.clone());
        let oc = OwnerCoin { coin: Coin { outpoint: outpoint(2), entry: UtxoEntry::new(base.owner_value, key.spk(), base.owner_daa, false, None) }, owner: key.public() };
        let wf = Funds { coins: vec![], change: wallet_spk(), fee: base.fee };
        build::request_withdraw(&base, outpoint(0), outpoint(1), &oc, &wf).expect("base withdraw");
        for _ in 0..iters(100) {
            let mut c = base.clone();
            let mut o = oc.clone();
            match r.below(7) {
                0 => mutate_as!(&mut r, c.vault, Vault),
                1 => mutate_as!(&mut r, c.position, Position),
                2 => c.w = r.i64(),
                3 => c.reserve = r.u64(),
                4 => o.owner = [OwnerPub::Schnorr(r.arr()), OwnerPub::Ecdsa(r.arr()), OwnerPub::P2sh(r.arr())][r.below(3) as usize].clone(),
                5 => o.coin.entry = UtxoEntry::new(r.u64(), random_spk(&mut r), r.u64(), r.coin(), None),
                _ => c.funding = vec![FundingIn { value: r.u64(), daa: r.u64() }],
            }
            let mut f = mutate_funds(&mut r, &wf);
            if r.coin() {
                f.coins.clear();
            }
            tally.add(&no_panic("request_withdraw", || format!("position {:?} w {}", c.position, c.w), || build::request_withdraw(&c, outpoint(0), outpoint(1), &o, &f)));
        }
    }
    report("request_withdraw", &tally);

    // settle, both kinds
    let mut tally = Tally::default();
    for base in [SettleCase::deposit(), SettleCase::withdrawal()] {
        let sf = Funds { coins: coins_of(&[(10 * KAS, 1_300_000)], 2), change: wallet_spk(), fee: base.fee };
        build::settle(&base, outpoint(0), outpoint(1), &sf).expect("base settle");
        for _ in 0..iters(120) {
            let mut c = base.clone();
            match r.below(5) {
                0 | 1 => mutate_as!(&mut r, c.request, Request),
                2 => mutate_as!(&mut r, c.record, Record),
                3 => c.request_value = r.u64(),
                _ => c.record_reserve = r.u64(),
            }
            let f = mutate_funds(&mut r, &sf);
            tally.add(&no_panic("settle", || format!("request {:?} record {:?}", c.request, c.record), || build::settle(&c, outpoint(0), outpoint(1), &f)));
        }
    }
    report("settle", &tally);

    // The fee loop at random rates.
    let mut tally = Tally::default();
    let base = BuyCase::new(b_max());
    for _ in 0..iters(30) {
        let rate = r.u64() % 2_000;
        tally.add(&no_panic("with_fee", || format!("rate {rate}"), || build::with_fee(rate, |fee| build::buy(&base, outpoint(0), &Funds { coins: coins_of(&[(100 * KAS, 0)], 1), change: wallet_spk(), fee }))));
    }
    report("with_fee", &tally);
}

// ------------------------------------------------------------------------------ signing

#[test]
fn w3_post_sign_never_panics() {
    quiet();
    let mut r = Rng(0x5EED_0F30_0004_u64);
    let mut c = BuyCase::new(b_max());
    c.with_ref = true;
    let u = build::buy(&c, outpoint(0), &Funds { coins: coins_of(&[(60 * KAS, 0), (50 * KAS, 0)], 1), change: wallet_spk(), fee: c.fee }).unwrap();
    let mut tally = Tally::default();
    for _ in 0..iters(600) {
        let mut got: Transaction = u.tx.clone();
        for _ in 0..1 + r.below(3) {
            match r.below(8) {
                0 => got.inputs.truncate(r.below(4) as usize),
                1 => got.outputs.truncate(r.below(5) as usize),
                2 if !got.inputs.is_empty() => {
                    let i = r.below(got.inputs.len() as u64) as usize;
                    got.inputs[i].signature_script = r.mutate_bytes(&got.inputs[i].signature_script);
                }
                3 if !got.inputs.is_empty() => got.inputs.push(got.inputs[0].clone()),
                4 if !got.outputs.is_empty() => {
                    let i = r.below(got.outputs.len() as u64) as usize;
                    got.outputs[i].value = r.u64();
                }
                5 => got.payload = r.rbytes(10),
                6 => got.lock_time = r.u64(),
                _ => {
                    for s in &u.sign {
                        if let Some(inp) = got.inputs.get_mut(s.index) {
                            inp.signature_script = std::iter::once(65u8).chain(r.bytes(64)).chain([1]).collect();
                        }
                    }
                }
            }
        }
        got.finalize();
        let ge: Vec<Option<UtxoEntry>> = (0..r.below(6)).map(|i| u.entries.get(i as usize).cloned()).collect();
        let _ = no_panic("post_sign", || format!("{got:?}"), || sign::post_sign(&u.tx, &u.entries, &u.sign, &got, &ge));
        tally.add(&no_panic("assemble", || format!("{got:?}"), || sign::assemble(&u, &got, &ge)));
        let idx: Vec<usize> = (0..r.below(5)).map(|_| r.below(6) as usize).collect();
        tally.add(&no_panic("with_signatures", || format!("{idx:?}"), || sign::with_signatures(&u.tx, &idx, &got)));
        tally.add(&no_panic("blanked", || format!("{idx:?}"), || sign::blanked(&u.tx, &idx)));
        tally.add(&no_panic("signers_for", || format!("{idx:?}"), || sign::signers_for(&u.entries, &idx)));
    }
    report("post_sign, assemble, with_signatures, blanked, signers_for", &tally);
    let _ = Role::Shard;
}
