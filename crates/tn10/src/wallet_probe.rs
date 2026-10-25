//! Front-end F0 (spec frontend 9, 6.2): the wallet experiment.
//!
//! `tn10 wallet-probe serve [--port 8710]` serves one page on `http://127.0.0.1:<port>`.
//! The page connects KasWare (`window.kasware`) or Kaspire (`window.kaspire`), refuses any
//! network other than testnet-10 and any address that is not `kaspatest:`, and posts the
//! address here. This server builds each transaction natively with the `lottery` builders
//! (the covenant inputs, plus the wallet's own P2PK UTXOs as funding), with the funding
//! inputs unsigned (`Built::finish_unsigned`), and returns it as Safe JSON (`pskt`) with the
//! indices to sign. The page asks the wallet to sign; the server parses what comes back, runs
//! the post-sign check (spec 6.2), runs every input in the local v2.1.0 engine, and only then
//! submits to testnet-10 (or, for the push variants, the wallet submits and the server checks
//! what reached the node).
//!
//! Transactions:
//! - `buy()`: one combination on a fresh shard of a probe instance (shard covenant input 0,
//!   the wallet's UTXOs as funding 1..n, change to the wallet; the ticket pays the wallet).
//! - `close()` (the claim-type transaction, F0 handoff): K = 16, 18 covenant inputs
//!   (controller, params, 16 shards) and the wallet's fee input(s) after them; the caller
//!   output goes to the wallet. It is the reachable covenant spend with a user-signed P2PK
//!   input: a probe instance's T is 600 DAA (about a minute) after its genesis.
//!
//! Instances: the server makes a fresh K = 16 genesis (TN10_PRIVATE_KEY pays, about 94 KAS
//! locked on testnet per instance) when it starts, and again whenever the current one is
//! closed or its 16 shards are used. A successful close consumes the instance.
//!
//! Variants (one button each): `indices` = signPskt listing only the P2PK funding indices;
//! `nolist` = signPskt with no signInputs list; `push` = KasWare pushPskt (P2PK indices
//! listed), or for Kaspire, signPskt (P2PK indices), the server's check, then
//! `kaspire.pushTx`.
//!
//! Log: every step is one JSON line in `tn10-data/wallet-probe/<unix seconds>.jsonl`
//! (git-ignored): the unsigned JSON, the signed JSON as returned, the check, the engine
//! result, the submission result or txid, and any wallet error text.
//!
//! `tn10 wallet-probe selftest` runs the same handlers with a stand-in signer (the TN10 key
//! as "the wallet", signing only the listed indices through the same JSON round trip),
//! feeds deliberately altered signed JSON to show the check refuses it, and submits a buy
//! (indices), a buy (no list) and a close to testnet-10.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::{
    Hash,
    hashing::sighash_type::SIG_HASH_ALL,
    sign::sign_input,
    tx::{PopulatedTransaction, ScriptPublicKey, Transaction, TransactionOutpoint as Op, UtxoEntry},
};
use kaspa_rpc_core::{RpcDataVerbosityLevel, RpcHash, RpcTransaction, api::rpc::RpcApi};
use lottery::buy::{BuyCase, Final, InKind, Templates, b_max};
use lottery::close::{CloseCase, FundingIn, ShardIn};
use lottery::combo::{Combo, RANKS, unrank};
use lottery::genesis::{GenesisCase, covenant_id};
use lottery::smt::Tree;
use lottery::state::{Payout, ShardState};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::probe::{Ctx, KAS, lottery_fee, sent_of};
use crate::{cases, fees, pskt};

type R<T> = Result<T, String>;

const K: usize = 16;
/// The page (no external requests; extension providers are injected by the browser).
const PAGE: &str = include_str!("wallet_probe.html");
/// Funding inputs per transaction (shard.sil and close.sil allow F_MAX = 8).
const MAX_FUNDING: usize = 4;

/// Unix milliseconds.
fn now_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The append-only JSONL log.
pub struct Log {
    pub path: PathBuf,
}

impl Log {
    pub fn new(tag: &str) -> R<Self> {
        let dir = crate::indexer::data_root().join("wallet-probe");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(format!("{}-{tag}.jsonl", now_ms() / 1000));
        Ok(Self { path })
    }

    /// Append one event: `{t_ms, step, ...v}`; print `summary` (or the step) to stdout.
    pub fn ev(&self, step: &str, v: Value) {
        let mut o = match v {
            Value::Object(m) => m,
            other => {
                let mut m = serde_json::Map::new();
                m.insert("value".into(), other);
                m
            }
        };
        o.insert("t_ms".into(), json!(now_ms() as u64));
        o.insert("step".into(), json!(step));
        let line = Value::Object(o.clone()).to_string();
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = writeln!(f, "{line}");
        }
        let s = o.get("summary").and_then(Value::as_str).map(str::to_string).unwrap_or_default();
        println!("[{step}] {s}");
    }
}

/// A connected wallet: its kind (`kasware`, `kaspire`, `standin`), address and payout.
#[derive(Clone)]
pub struct Wallet {
    pub kind: String,
    pub addr: Address,
    pub payout: Payout,
    pub spk: ScriptPublicKey,
}

/// The network string each provider reports for testnet-10 (KasWare `getNetwork`:
/// docs.kasware.xyz kaspa-connection; Kaspire `getNetwork`: kaspire.kaslab.space/developers/extension).
fn testnet_name(kind: &str) -> R<&'static str> {
    match kind {
        "kasware" | "standin" => Ok("kaspa_testnet_10"),
        "kaspire" => Ok("testnet-10"),
        o => Err(format!("unknown wallet `{o}`")),
    }
}

/// A `kaspatest:` P2PK address (Schnorr or ECDSA) as a payout; anything else is refused.
pub fn parse_addr(s: &str) -> R<(Address, Payout)> {
    if !s.starts_with("kaspatest:") {
        return Err(format!("`{s}` is not a kaspatest: address; refusing"));
    }
    let a = Address::try_from(s).map_err(|e| format!("`{s}`: {e}"))?;
    if a.prefix != Prefix::Testnet {
        return Err(format!("`{s}`: not a testnet address; refusing"));
    }
    let p = match a.version {
        Version::PubKey => Payout::Schnorr(a.payload.as_slice().try_into().map_err(|_| "bad Schnorr payload".to_string())?),
        Version::PubKeyECDSA => Payout::Ecdsa(a.payload.as_slice().try_into().map_err(|_| "bad ECDSA payload".to_string())?),
        v => return Err(format!("`{s}`: address version {v:?} is not a P2PK wallet address")),
    };
    Ok((a, p))
}

#[derive(Clone)]
struct Slot {
    state: ShardState,
    op: Op,
    value: u64,
    /// Creation DAA score (genesis shards); bought shards are read from the node at close.
    daa: u64,
    /// Sink before the buy that spent this slot was submitted (for `cases::merged`).
    start: Option<RpcHash>,
    tree: Tree,
    used: bool,
}

struct Inst {
    n: usize,
    t: Arc<Templates>,
    g: GenesisCase,
    cov: Hash,
    gid: Hash,
    gdaa: u64,
    deadline: u64,
    slots: Vec<Slot>,
    closed: bool,
}

enum What {
    Buy { slot: usize, bc: BuyCase },
    Close,
}

struct Pending {
    what: What,
    variant: String,
    inst: usize,
    /// The transaction exactly as sent to the wallet (inputs to sign empty).
    sent: Transaction,
    fin: Final,
    sign: Vec<usize>,
    start: RpcHash,
    funding: Vec<Op>,
    created: std::time::Instant,
}

pub struct Probe {
    pub ctx: Ctx,
    pub log: Log,
    inst: Option<Inst>,
    n_inst: usize,
    wallet: Option<Wallet>,
    pending: HashMap<u64, Pending>,
    next_id: u64,
    combo: u32,
    /// Wallet outpoints in a pending build or already spent by a submitted transaction.
    locked: HashSet<Op>,
}

impl Probe {
    pub fn new(ctx: Ctx, log: Log) -> Self {
        Self { ctx, log, inst: None, n_inst: 0, wallet: None, pending: HashMap::new(), next_id: 1, combo: 0, locked: HashSet::new() }
    }

    async fn daa(&self) -> R<u64> {
        Ok(self.ctx.client.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score)
    }

    /// A fresh K = 16 instance (the round1_drawn genesis: J(1) = 10 KAS, V0 = 60 KAS), T 600
    /// DAA ahead, paid by TN10_PRIVATE_KEY.
    async fn genesis(&mut self) -> R<()> {
        use crate::probe::Utxo;
        let t = Templates::get(b_max());
        let daa = self.daa().await?;
        let deadline = daa + 600;
        // Skip outpoints a submitted transaction already spent (the self-test's stand-in
        // wallet is this same key, and the UTXO index lists an input until acceptance).
        let f: Utxo = self.ctx.funds().await?.into_iter().find(|u| !self.locked.contains(&u.op)).ok_or("TN10 wallet has no confirmed funds")?;
        self.locked.insert(f.op);
        let me = Payout::Schnorr(secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &self.ctx.sk).map_err(|e| e.to_string())?.x_only_public_key().0.serialize());
        let mut g = GenesisCase::new_with(K, vec![(f.op, f.entry.clone())], &t);
        g.funding_sk = self.ctx.sk;
        g.set_owner(&me);
        g.set_jackpot(10 * KAS as i64);
        g.set_vault(60 * KAS as i64);
        g.controller.deadline = deadline as i64;
        for s in &mut g.shards {
            s.deadline = deadline as i64;
        }
        let gf = g.build_with(t.clone()).finish();
        g.fee = lottery_fee(&gf);
        let gf = g.build_with(t.clone()).finish();
        gf.run().map_err(|(i, e)| format!("local engine rejects the probe genesis at input {i}: {e:?}"))?;
        let cov = covenant_id(&gf);
        let gs = sent_of(&gf);
        let start = cases::sink(&self.ctx).await?;
        self.n_inst += 1;
        let n = self.n_inst;
        if !self.ctx.submit(&format!("wallet-probe instance {n} genesis, K = {K}"), &gs, true).await? {
            return Err("probe genesis rejected".into());
        }
        let gid = gs.tx.id();
        let gn = gs.tx.outputs.len() - 1;
        let gchange = cases::merged(&self.ctx, start, Op::new(gid, gn as u32), &gs.tx.outputs[gn].script_public_key).await?.0;
        let gdaa = gchange.entry.block_daa_score;
        let m = g.controller.m as usize;
        let slots = (0..K)
            .map(|i| Slot {
                state: g.shards[i].clone(),
                op: Op::new(gid, GenesisCase::shard_output(i) as u32),
                value: g.shard_value(i),
                daa: gdaa,
                start: None,
                tree: Tree::new(m),
                used: false,
            })
            .collect();
        self.log.ev(
            "instance",
            json!({"summary": format!("instance {n}: genesis {gid} at DAA {gdaa}, covenant id {cov}, T = {deadline} (close() possible about {} s after genesis)", deadline.saturating_sub(gdaa) / 10),
                   "instance": n, "genesis_txid": gid.to_string(), "covenant_id": cov.to_string(), "genesis_daa": gdaa, "deadline": deadline,
                   "fee_sompi": gs.fee, "masses": format!("{:?}", gs.m)}),
        );
        self.inst = Some(Inst { n, t, g, cov, gid, gdaa, deadline, slots, closed: false });
        Ok(())
    }

    /// An instance with a free shard and no close yet.
    async fn ensure_open(&mut self) -> R<()> {
        let ok = self.inst.as_ref().is_some_and(|i| !i.closed && i.slots.iter().any(|s| !s.used));
        if !ok {
            self.genesis().await?;
        }
        Ok(())
    }

    /// `/api/connect`: `{wallet, address, network, publicKey?}`.
    pub async fn connect(&mut self, v: &Value) -> R<Value> {
        let kind = v["wallet"].as_str().unwrap_or("").to_string();
        let net = v["network"].as_str().unwrap_or("");
        let want = testnet_name(&kind)?;
        let addr_s = v["address"].as_str().unwrap_or("");
        let res = (|| {
            if net != want {
                return Err(format!("{kind} reports network `{net}`, expected `{want}` (testnet-10); refusing"));
            }
            parse_addr(addr_s)
        })();
        let (addr, payout) = match res {
            Ok(x) => x,
            Err(e) => {
                self.log.ev("connect-refused", json!({"summary": e, "wallet": kind, "network": net, "address": addr_s}));
                return Err(e);
            }
        };
        let utxos = crate::net::utxos(&self.ctx.client, &addr).await?;
        let bal: u64 = utxos.iter().map(|u| u.utxo_entry.amount).sum();
        let spk = payout.spk();
        if spk != kaspa_txscript::pay_to_address_script(&addr) {
            return Err(format!("{addr}: payout script differs from the address script"));
        }
        self.log.ev(
            "connect",
            json!({"summary": format!("{kind} connected: {addr} on {net}; {} UTXOs, {:.8} KAS", utxos.len(), bal as f64 / 1e8),
                   "wallet": kind, "address": addr.to_string(), "network": net, "publicKey": v["publicKey"], "utxos": utxos.len(), "balance_sompi": bal}),
        );
        self.wallet = Some(Wallet { kind, addr: addr.clone(), payout, spk });
        Ok(json!({"ok": true, "address": addr.to_string(), "balance_kas": format!("{:.8}", bal as f64 / 1e8), "utxos": utxos.len()}))
    }

    /// The wallet's confirmed UTXOs not locked, largest first, enough for `need` (at most
    /// MAX_FUNDING inputs).
    async fn wallet_funding(&self, w: &Wallet, need: u64) -> R<Vec<(Op, UtxoEntry)>> {
        let mut v: Vec<(Op, UtxoEntry)> = crate::net::utxos(&self.ctx.client, &w.addr)
            .await?
            .into_iter()
            .map(|e| (Op::from(e.outpoint), UtxoEntry::from(e.utxo_entry)))
            .filter(|(op, e)| !self.locked.contains(op) && e.covenant_id.is_none() && e.block_daa_score != u64::MAX && e.script_public_key == w.spk)
            .collect();
        v.sort_by_key(|(_, e)| std::cmp::Reverse(e.amount));
        let mut out = vec![];
        let mut sum = 0;
        for x in v.into_iter().take(MAX_FUNDING) {
            sum += x.1.amount;
            out.push(x);
            if sum >= need {
                return Ok(out);
            }
        }
        Err(format!(
            "the wallet's free confirmed UTXOs ({} in the largest {MAX_FUNDING}, {:.8} KAS) do not cover {:.8} KAS; fund it (`tn10 send`) or wait a few seconds for change to confirm",
            out.len(),
            sum as f64 / 1e8,
            need as f64 / 1e8
        ))
    }

    /// Replace the builder's funding inputs (from `first`) and its last output (change or
    /// caller output) with the wallet's entries and script; set the outpoints.
    fn patch(b: &mut lottery::buy::Built, first: usize, funding: &[(Op, UtxoEntry)], spk: &ScriptPublicKey, mut ops: Vec<Op>) -> R<()> {
        for (k, (op, e)) in funding.iter().enumerate() {
            let i = first + k;
            if !matches!(b.inputs.get(i).map(|x| &x.kind), Some(InKind::Funding)) {
                return Err(format!("input {i} is not a funding input"));
            }
            b.inputs[i].entry = e.clone();
            ops.push(*op);
        }
        if b.inputs.len() != first + funding.len() {
            return Err(format!("{} inputs, expected {}", b.inputs.len(), first + funding.len()));
        }
        let last = b.outputs.len() - 1;
        b.outputs[last].script_public_key = spk.clone();
        b.outpoints = ops;
        Ok(())
    }

    /// `/api/build`: `{kind: buy|close, variant: indices|nolist|push}`.
    pub async fn build(&mut self, v: &Value) -> R<Value> {
        let kind = v["kind"].as_str().unwrap_or("").to_string();
        let variant = v["variant"].as_str().unwrap_or("").to_string();
        if !["indices", "nolist", "push"].contains(&variant.as_str()) {
            return Err(format!("unknown variant `{variant}`"));
        }
        let w = self.wallet.clone().ok_or("no wallet connected")?;
        // A build abandoned for 10 minutes (page reloaded mid-signature) frees its shard and
        // funding, so it cannot block a close.
        let old: Vec<u64> = self.pending.iter().filter(|(_, p)| p.created.elapsed().as_secs() > 600).map(|(k, _)| *k).collect();
        for k in old {
            let p = self.pending.remove(&k).unwrap();
            self.release(&p);
            self.log.ev("expired", json!({"summary": format!("#{k}: no wallet answer within 10 minutes; released"), "id": k}));
        }
        let rate = fees::estimate(&self.ctx.client).await?;
        let start = cases::sink(&self.ctx).await?;
        let (what, fin, sign, funding, label) = match kind.as_str() {
            "buy" => self.build_buy(&w, rate.sompi_per_gram).await?,
            "close" => self.build_close(&w, rate.sompi_per_gram).await?,
            o => return Err(format!("unknown kind `{o}`")),
        };
        // Every covenant input must already pass in the local engine (the funding inputs
        // fail until signed).
        for i in 0..fin.tx.inputs.len() {
            if !sign.contains(&i) {
                fin.run_input(i).map_err(|e| format!("local engine rejects covenant input {i} before signing: {e:?}"))?;
            }
        }
        let json = pskt::to_safe_json(&fin.tx, &fin.entries, &sign)?;
        let sent = pskt::blanked(&fin.tx, &sign)?;
        let id = self.next_id;
        self.next_id += 1;
        let m = harness::masses(&fin.tx, &fin.entries);
        let inst = self.inst.as_ref().map(|i| i.n).unwrap_or(0);
        self.log.ev(
            "build",
            json!({"summary": format!("#{id} {kind}/{variant} for {} ({label}): txid {}, {} inputs, {} outputs, sign {sign:?}, fee {:.8} KAS, masses {m:?}, JSON {} B",
                    w.kind, fin.tx.id(), fin.tx.inputs.len(), fin.tx.outputs.len(), fin.fee() as f64 / 1e8, json.len()),
                   "id": id, "kind": kind, "variant": variant, "wallet": w.kind, "instance": inst, "txid": fin.tx.id().to_string(), "sign": sign,
                   "fee_sompi": fin.fee(), "feerate": rate.sompi_per_gram, "masses": format!("{m:?}"), "unsigned_json": json}),
        );
        for op in &funding {
            self.locked.insert(*op);
        }
        let what_note = label.clone();
        self.pending.insert(id, Pending { what, variant: variant.clone(), inst, sent, fin, sign: sign.clone(), start, funding, created: std::time::Instant::now() });
        let sign_inputs: Vec<Value> = sign.iter().map(|&i| json!({"index": i, "sighashType": 1})).collect();
        Ok(json!({"ok": true, "id": id, "txJsonString": json, "signInputs": sign_inputs, "txid": self.pending[&id].fin.tx.id().to_string(), "note": what_note}))
    }

    async fn build_buy(&mut self, w: &Wallet, rate: u64) -> R<(What, Final, Vec<usize>, Vec<Op>, String)> {
        self.ensure_open().await?;
        let busy: HashSet<usize> = self.pending.values().filter_map(|p| if let What::Buy { slot, .. } = p.what { Some(slot) } else { None }).collect();
        self.combo += 1;
        let k = self.combo;
        let inst = self.inst.as_ref().unwrap();
        let si = (0..K).find(|i| !inst.slots[*i].used && !busy.contains(i)).ok_or("every shard of this instance is used or pending")?;
        let slot = inst.slots[si].clone();
        let t = inst.t.clone();
        let m = inst.g.controller.m as u32;
        let combo = Combo::new(unrank((k * 7_919 + now_ms() as u32 % 1_000) % RANKS), 1 + (k % m) as u8);
        let mut bc = BuyCase::new(t.b_max).with_tree(slot.tree.clone());
        bc.shard = slot.state.clone();
        bc.reserve = inst.g.reserve();
        bc.shard_daa = slot.daa;
        bc.cov = inst.cov;
        bc.bundle = vec![combo];
        bc.payout = w.payout;
        bc.with_ref = false;
        let need = bc.gross() + bc.b() * bc.shard.d as u64 + KAS;
        let funding = self.wallet_funding(w, need).await?;
        bc.funding = funding.iter().map(|(_, e)| e.amount).collect();
        let ops = vec![slot.op];
        let mk = |fee: u64| -> R<Final> {
            let mut c = bc.clone();
            c.fee = fee;
            let mut b = c.build_with(t.clone());
            Self::patch(&mut b, 1, &funding, &w.spk, ops.clone())?;
            Ok(b.finish_unsigned()?.0)
        };
        let fin = fees::with_fee(rate, mk)?;
        bc.fee = fin.fee();
        let mut b = bc.build_with(t.clone());
        Self::patch(&mut b, 1, &funding, &w.spk, ops.clone())?;
        let (fin, sign) = b.finish_unsigned()?;
        let label = format!("instance {} shard {si}, combination {:?} + {}, ticket pays the wallet", inst.n, combo.main, combo.bonus);
        Ok((What::Buy { slot: si, bc }, fin, sign, funding.iter().map(|x| x.0).collect(), label))
    }

    async fn build_close(&mut self, w: &Wallet, rate: u64) -> R<(What, Final, Vec<usize>, Vec<Op>, String)> {
        if self.inst.as_ref().is_none_or(|i| i.closed) {
            self.genesis().await?;
        }
        if self.pending.values().any(|p| p.inst == self.inst.as_ref().unwrap().n) {
            return Err("a transaction of this instance is still waiting for the wallet; answer or reject it in the wallet first (it expires after 10 minutes)".into());
        }
        let now = self.daa().await?;
        let (deadline, n) = {
            let i = self.inst.as_ref().unwrap();
            (i.deadline, i.n)
        };
        if now <= deadline + 10 {
            return Err(format!("close() of instance {n} opens at DAA {deadline} (+10); now {now}: about {} s to wait", (deadline + 10 - now) / 10 + 1));
        }
        // The node's creation DAA score of every bought shard (chain-merged, O-S4-4).
        let used: Vec<usize> = self.inst.as_ref().unwrap().slots.iter().enumerate().filter(|(_, s)| s.used && s.start.is_some()).map(|(i, _)| i).collect();
        for i in used {
            let (start, op, spk) = {
                let inst = self.inst.as_ref().unwrap();
                let s = &inst.slots[i];
                (s.start.unwrap(), s.op, inst.t.shard_spk(&s.state))
            };
            let u = cases::merged(&self.ctx, start, op, &spk).await?.0;
            let inst = self.inst.as_mut().unwrap();
            if u.entry.amount != inst.slots[i].value {
                return Err(format!("shard {i}: node value {} != tracked {}", u.entry.amount, inst.slots[i].value));
            }
            inst.slots[i].daa = u.entry.block_daa_score;
            inst.slots[i].start = None;
        }
        let inst = self.inst.as_ref().unwrap();
        let t = inst.t.clone();
        let mut c = CloseCase::new_with(K, &t);
        c.controller = inst.g.controller.clone();
        c.controller_daa = inst.gdaa;
        c.pot = inst.g.pot;
        c.params = inst.g.params.clone();
        c.params_daa = inst.gdaa;
        c.shards = inst.slots.iter().map(|s| ShardIn { state: s.state.clone(), daa: s.daa }).collect();
        c.funder = w.payout;
        c.cov = inst.cov;
        c.lock_time = inst.deadline;
        let need = inst.g.reserve() + lottery::close::MIN_CALLER_OUTPUT + KAS;
        let funding = self.wallet_funding(w, need).await?;
        c.funding = funding.iter().map(|(_, e)| FundingIn { value: e.amount, daa: e.block_daa_score }).collect();
        let mut ops = vec![Op::new(inst.gid, 0), Op::new(inst.gid, 1)];
        ops.extend(inst.slots.iter().map(|s| s.op));
        let mk = |fee: u64| -> R<Final> {
            let mut c = c.clone();
            c.fee = fee;
            let mut b = c.build_with(t.clone());
            Self::patch(&mut b, K + 2, &funding, &w.spk, ops.clone())?;
            Ok(b.finish_unsigned()?.0)
        };
        let fin = fees::with_fee(rate, mk)?;
        c.fee = fin.fee();
        let mut b = c.build_with(t.clone());
        Self::patch(&mut b, K + 2, &funding, &w.spk, ops.clone())?;
        let (fin, sign) = b.finish_unsigned()?;
        let sold = inst.slots.iter().filter(|s| s.used).count();
        let label = format!("instance {} close(), K = {K}, {sold} shard(s) sold into, caller output and marker funder = the wallet", inst.n);
        Ok((What::Close, fin, sign, funding.iter().map(|x| x.0).collect(), label))
    }

    fn release(&mut self, p: &Pending) {
        for op in &p.funding {
            self.locked.remove(op);
        }
    }

    /// After an accepted submission: the funding stays locked (spent), the instance moves on.
    fn applied(&mut self, p: Pending, txid: Hash) {
        let n = p.inst;
        let Some(inst) = self.inst.as_mut().filter(|i| i.n == n) else { return };
        match p.what {
            What::Buy { slot, bc } => {
                let s = &mut inst.slots[slot];
                s.state = bc.successor();
                s.tree = bc.proofs().2;
                s.op = Op::new(txid, 0);
                s.value = p.fin.tx.outputs[0].value;
                s.start = Some(p.start);
                s.used = true;
            }
            What::Close => inst.closed = true,
        }
    }

    /// `/api/signed`: `{id, signed, submit?}`. Parse, post-sign check, engine, submit.
    pub async fn signed(&mut self, v: &Value) -> R<Value> {
        let id = v["id"].as_u64().ok_or("id")?;
        let submit = v["submit"].as_bool().unwrap_or(true);
        let raw = match &v["signed"] {
            Value::String(s) => s.clone(),
            Value::Null => return Err("no signed transaction".into()),
            other => other.to_string(),
        };
        let p = self.pending.remove(&id).ok_or(format!("no pending build #{id}"))?;
        self.log.ev("signed-raw", json!({"summary": format!("#{id}: wallet returned {} B", raw.len()), "id": id, "signed_json": raw}));
        let (got, ge, form) = match pskt::from_json(&raw) {
            Ok(x) => x,
            Err(e) => {
                self.log.ev("parse-refused", json!({"summary": format!("#{id}: returned JSON does not parse: {e}"), "id": id, "error": e}));
                self.release(&p);
                return Ok(json!({"ok": false, "stage": "parse", "error": e}));
            }
        };
        let c = pskt::post_sign(&p.sent, &p.fin.entries, &p.sign, &got, &ge);
        self.log.ev(
            "post-sign",
            json!({"summary": format!("#{id}: {} ({form} JSON); gained {:?}, lost {:?}{}{}", if c.pass() { "PASS" } else { "REFUSED" }, c.gained, c.lost,
                    if c.pass() { String::new() } else { format!("; {}", c.refusals.join("; ")) },
                    if c.notes.is_empty() { String::new() } else { format!("; notes: {}", c.notes.join("; ")) }),
                   "id": id, "form": form, "pass": c.pass(), "refusals": c.refusals, "gained": c.gained, "lost": c.lost, "notes": c.notes}),
        );
        if !c.pass() {
            self.release(&p);
            return Ok(json!({"ok": false, "stage": "post-sign", "refusals": c.refusals, "gained": c.gained, "lost": c.lost}));
        }
        let tx = pskt::assemble(&p.sent, &p.sign, &got);
        let f = Final { tx: tx.clone(), entries: p.fin.entries.clone(), units: p.fin.units.clone(), sim: None };
        let eng: Vec<String> = f.run_all().iter().enumerate().map(|(i, r)| match r {
            Ok(u) => format!("input {i}: ok ({u} units)"),
            Err(e) => format!("input {i}: REJECTED {e:?}"),
        }).collect();
        let eng_ok = f.run().is_ok();
        self.log.ev("engine", json!({"summary": format!("#{id}: local engine {}", if eng_ok { "accepts every input" } else { "REJECTS" }), "id": id, "ok": eng_ok, "inputs": eng}));
        if !eng_ok {
            self.release(&p);
            return Ok(json!({"ok": false, "stage": "engine", "inputs": eng}));
        }
        if !submit {
            let txid = tx.id();
            self.pending.insert(id, p);
            return Ok(json!({"ok": true, "checked": true, "txid": txid.to_string()}));
        }
        match self.ctx.client.submit_transaction(RpcTransaction::from(&tx), false).await {
            Ok(r) => {
                self.log.ev("submit", json!({"summary": format!("#{id}: ACCEPTED by the node, txid {r}"), "id": id, "txid": r.to_string(), "variant": p.variant}));
                self.applied(p, tx.id());
                let next = self.after_close().await;
                Ok(json!({"ok": true, "txid": r.to_string(), "next": next}))
            }
            Err(e) => {
                let e = e.to_string();
                self.log.ev("submit", json!({"summary": format!("#{id}: node REJECTED: {e}"), "id": id, "error": e}));
                self.release(&p);
                Ok(json!({"ok": false, "stage": "submit", "error": e}))
            }
        }
    }

    /// After a close, start the next instance at once (its T is a minute away).
    async fn after_close(&mut self) -> Value {
        if self.inst.as_ref().is_some_and(|i| i.closed) {
            let r = match self.genesis().await {
                Ok(()) => format!("instance {} ready; its close() opens at DAA {}", self.n_inst, self.inst.as_ref().unwrap().deadline),
                Err(e) => format!("next instance failed: {e}"),
            };
            self.log.ev("next-instance", json!({"summary": r}));
            json!(r)
        } else {
            Value::Null
        }
    }

    /// `/api/pushed`: `{id, result}` where `result` is what pushPskt / pushTx returned.
    pub async fn pushed(&mut self, v: &Value) -> R<Value> {
        let id = v["id"].as_u64().ok_or("id")?;
        let p = self.pending.remove(&id).ok_or(format!("no pending build #{id}"))?;
        let res = &v["result"];
        let ours = p.fin.tx.id();
        let txid = txid_of(res);
        self.log.ev("push-result", json!({"summary": format!("#{id}: wallet push returned {}", short(&res.to_string())), "id": id, "result": res, "txid": txid.map(|t| t.to_string()), "built_txid": ours.to_string()}));
        let Some(txid) = txid else {
            self.release(&p);
            return Ok(json!({"ok": false, "stage": "push", "error": "no transaction id in the wallet's answer"}));
        };
        if txid != ours {
            // The wallet sent something else; the instance state is unknown from here.
            self.log.ev("push-mismatch", json!({"summary": format!("#{id}: the wallet submitted {txid}, not the built {ours} (the id covers every field but signatures and the mass commit): retiring the instance"), "id": id}));
            if let Some(i) = self.inst.as_mut().filter(|i| i.n == p.inst) {
                i.closed = true;
            }
            return Ok(json!({"ok": false, "stage": "push", "error": format!("txid {txid} != built {ours}")}));
        }
        // What reached the node, with its signature scripts: the mempool entry, else the
        // accepted transaction from the virtual chain (Full verbosity).
        let seen = self.fetch_submitted(txid, p.start).await;
        let check = match &seen {
            Ok((tx, src)) => {
                let ge: Vec<Option<UtxoEntry>> = p.fin.entries.iter().cloned().map(Some).collect();
                let c = pskt::post_sign(&p.sent, &p.fin.entries, &p.sign, tx, &ge);
                let f = Final { tx: tx.clone(), entries: p.fin.entries.clone(), units: p.fin.units.clone(), sim: None };
                let eng = f.run().map(|_| "accepts every input".to_string()).unwrap_or_else(|(i, e)| format!("REJECTS input {i}: {e:?}"));
                self.log.ev(
                    "push-check",
                    json!({"summary": format!("#{id}: submitted tx from the {src}: post-sign {} (after the fact), engine {eng}; gained {:?}{}", if c.pass() { "PASS" } else { "MISMATCH" }, c.gained,
                            if c.pass() { String::new() } else { format!("; {}", c.refusals.join("; ")) }),
                           "id": id, "source": src, "pass": c.pass(), "refusals": c.refusals, "notes": c.notes, "gained": c.gained, "engine": eng, "submitted_json": pskt::reserialize(tx, &p.fin.entries).unwrap_or_default()}),
                );
                json!({"pass": c.pass(), "refusals": c.refusals})
            }
            Err(e) => {
                self.log.ev("push-check", json!({"summary": format!("#{id}: could not read the submitted transaction back: {e}"), "id": id, "error": e}));
                json!({"pass": null, "error": e})
            }
        };
        self.applied(p, txid);
        let next = self.after_close().await;
        Ok(json!({"ok": true, "txid": txid.to_string(), "check": check, "next": next}))
    }

    async fn fetch_submitted(&self, txid: Hash, start: RpcHash) -> R<(Transaction, &'static str)> {
        let t0 = std::time::Instant::now();
        while t0.elapsed() < std::time::Duration::from_secs(60) {
            if let Ok(e) = self.ctx.client.get_mempool_entry(txid, false, false).await {
                let mut tx = Transaction::try_from(e.transaction).map_err(|e| e.to_string())?;
                tx.finalize();
                return Ok((tx, "mempool"));
            }
            if let Ok(r) = self.ctx.client.get_virtual_chain_from_block_v2(start, Some(RpcDataVerbosityLevel::Full), None).await {
                if let Ok(b) = crate::indexer::batch_of(r) {
                    for blk in b.added {
                        for ct in blk.txs {
                            if ct.tx.id() == txid {
                                return Ok((ct.tx, "virtual chain"));
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err(format!("{txid} neither in the mempool nor on the virtual chain within 60 s"))
    }

    /// `/api/wallet-error`: `{id, error, raw}`.
    pub fn wallet_error(&mut self, v: &Value) -> R<Value> {
        let id = v["id"].as_u64().unwrap_or(0);
        if let Some(p) = self.pending.remove(&id) {
            self.release(&p);
        }
        self.log.ev("wallet-error", json!({"summary": format!("#{id}: wallet error: {}", v["error"].as_str().unwrap_or("")), "id": id, "error": v["error"], "raw": v["raw"], "call": v["call"]}));
        Ok(json!({"ok": true}))
    }

    /// `/api/note`: free text from the page (e.g. a provider missing, a refusal).
    pub fn note(&mut self, v: &Value) -> R<Value> {
        self.log.ev("page", json!({"summary": v["text"].as_str().unwrap_or(""), "detail": v}));
        Ok(json!({"ok": true}))
    }

    /// `/api/state`.
    pub async fn state(&self) -> R<Value> {
        let daa = self.daa().await?;
        Ok(match &self.inst {
            Some(i) => json!({"instance": i.n, "covenant_id": i.cov.to_string(), "deadline": i.deadline, "daa": daa, "closed": i.closed,
                               "close_in_s": if daa > i.deadline + 10 { 0 } else { (i.deadline + 10 - daa) / 10 + 1 },
                               "shards_used": i.slots.iter().filter(|s| s.used).count(),
                               "wallet": self.wallet.as_ref().map(|w| format!("{} {}", w.kind, w.addr)), "log": self.log.path.display().to_string()}),
            None => json!({"instance": null, "daa": daa, "log": self.log.path.display().to_string()}),
        })
    }
}

fn short(s: &str) -> String {
    if s.len() > 200 { format!("{}...", &s[..200]) } else { s.to_string() }
}

/// A transaction id in a wallet's push answer: a 64-hex string, `{txid|id|transactionId}`,
/// or a JSON transaction (its id recomputed).
fn txid_of(v: &Value) -> Option<Hash> {
    let h = |s: &str| -> Option<Hash> {
        let s = s.trim().trim_matches('"');
        if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
            let b: Vec<u8> = (0..32).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect();
            return Some(Hash::from_bytes(b.try_into().ok()?));
        }
        None
    };
    match v {
        Value::String(s) => h(s).or_else(|| pskt::from_json(s).ok().map(|(t, _, _)| t.id())),
        Value::Object(o) => ["txid", "txId", "transactionId", "id"].iter().find_map(|k| o.get(*k).and_then(Value::as_str).and_then(h)),
        _ => None,
    }
}

// ------------------------------------------------------------------------ HTTP

async fn route(p: &tokio::sync::Mutex<Probe>, method: &str, path: &str, body: &str) -> (u16, &'static str, String) {
    if method == "GET" && (path == "/" || path.starts_with("/?")) {
        return (200, "text/html; charset=utf-8", PAGE.to_string());
    }
    if method == "GET" && path == "/api/state" {
        let r = p.lock().await.state().await;
        return reply(r);
    }
    if method != "POST" {
        return (404, "text/plain", "not found".into());
    }
    let v: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return (400, "application/json", json!({"ok": false, "error": format!("bad JSON: {e}")}).to_string()),
    };
    let mut g = p.lock().await;
    let r = match path {
        "/api/connect" => g.connect(&v).await,
        "/api/build" => g.build(&v).await,
        "/api/signed" => g.signed(&v).await,
        "/api/pushed" => g.pushed(&v).await,
        "/api/wallet-error" => g.wallet_error(&v),
        "/api/note" => g.note(&v),
        _ => return (404, "text/plain", "not found".into()),
    };
    if let Err(e) = &r {
        g.log.ev("error", json!({"summary": format!("{path}: {e}"), "path": path, "error": e}));
    }
    reply(r)
}

fn reply(r: R<Value>) -> (u16, &'static str, String) {
    match r {
        Ok(v) => (200, "application/json", v.to_string()),
        Err(e) => (200, "application/json", json!({"ok": false, "error": e}).to_string()),
    }
}

async fn handle(mut s: tokio::net::TcpStream, p: Arc<tokio::sync::Mutex<Probe>>, port: u16) -> R<()> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 16_384];
    let head_end = loop {
        let n = s.read(&mut tmp).await.map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > 65_536 {
            return Err("headers too large".into());
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut rl = lines.next().unwrap_or("").split(' ');
    let (method, path) = (rl.next().unwrap_or("").to_string(), rl.next().unwrap_or("").to_string());
    let mut len = 0usize;
    let mut origin = None;
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => len = v.trim().parse().unwrap_or(0),
                "origin" => origin = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    if len > 16 * 1024 * 1024 {
        return Err("body too large".into());
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < len {
        let n = s.read(&mut tmp).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    // Only this page may drive the probe (a foreign page in the same browser may not).
    let allowed = [format!("http://127.0.0.1:{port}"), format!("http://localhost:{port}")];
    let (code, ctype, out) = if method == "POST" && origin.as_ref().is_some_and(|o| !allowed.contains(o)) {
        (403, "text/plain", format!("origin {} refused", origin.unwrap_or_default()))
    } else {
        route(&p, &method, &path, &String::from_utf8_lossy(&body)).await
    };
    let resp = format!(
        "HTTP/1.1 {code} {}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        if code == 200 { "OK" } else { "ERR" },
        out.len()
    );
    s.write_all(resp.as_bytes()).await.map_err(|e| e.to_string())?;
    s.write_all(out.as_bytes()).await.map_err(|e| e.to_string())?;
    let _ = s.shutdown().await;
    Ok(())
}

/// `wallet-probe serve`: first instance, then the page.
pub async fn serve(mut p: Probe, port: u16) -> R<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await.map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))?;
    p.ensure_open().await?;
    p.log.ev("serve", json!({"summary": format!("open http://localhost:{port}/ (or http://127.0.0.1:{port}/) in Chrome; log {}", p.log.path.display()), "port": port}));
    let p = Arc::new(tokio::sync::Mutex::new(p));
    loop {
        let (sock, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let p = p.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(sock, p, port).await {
                eprintln!("http: {e}");
            }
        });
    }
}

// ------------------------------------------------------------------------ self-test

/// The stand-in signer: parse the Safe JSON as a wallet would (kaspa-wasm's
/// deserializeFromSafeJSON), sign `list` (or, with `None`, every input paying our P2PK script)
/// with SIG_HASH_ALL, and return Safe JSON.
pub fn standin_sign(json: &str, list: Option<&[usize]>, sk: &[u8; 32]) -> R<String> {
    let (mut tx, entries, _) = pskt::from_json(json)?;
    let entries: Vec<UtxoEntry> = entries.into_iter().enumerate().map(|(i, e)| e.ok_or(format!("input {i}: no entry"))).collect::<R<_>>()?;
    let me = lottery::buy::spk_of(sk);
    let list: Vec<usize> = match list {
        Some(l) => l.to_vec(),
        None => (0..tx.inputs.len()).filter(|&i| entries[i].script_public_key == me).collect(),
    };
    let unsigned = tx.clone();
    for i in list {
        tx.inputs[i].signature_script = sign_input(&PopulatedTransaction::new(&unsigned, entries.clone()), i, sk, SIG_HASH_ALL);
    }
    pskt::reserialize(&tx, &entries)
}

fn indices(b: &Value) -> Vec<usize> {
    b["signInputs"].as_array().map(|a| a.iter().filter_map(|x| x["index"].as_u64().map(|i| i as usize)).collect()).unwrap_or_default()
}

/// `wallet-probe selftest`.
pub async fn selftest(mut p: Probe) -> R<()> {
    let sk = p.ctx.sk;
    let addr = p.ctx.addr.to_string();
    p.log.ev("selftest", json!({"summary": format!("stand-in wallet = the TN10 key ({addr}); log {}", p.log.path.display())}));
    // Refusals: another network, another address prefix.
    for (net, a, why) in [("kaspa_testnet_11", addr.clone(), "network"), ("kaspa_testnet_10", addr.replacen("kaspatest:", "kaspadev:", 1), "prefix")] {
        let r = p.connect(&json!({"wallet": "standin", "address": a, "network": net})).await;
        if r.is_ok() {
            return Err(format!("connect with a wrong {why} was accepted"));
        }
        println!("selftest: wrong {why} refused: {}", r.unwrap_err());
    }
    p.connect(&json!({"wallet": "standin", "address": addr, "network": "kaspa_testnet_10"})).await?;
    p.ensure_open().await?;

    // Altered signed JSON is refused before anything is sent.
    type Edit = fn(&mut Value);
    let tampers: [(&str, Edit); 5] = [
        ("output value", |v| v["outputs"][1]["value"] = "1".into()),
        ("covenant signature script", |v| v["inputs"][0]["signatureScript"] = "00".into()),
        ("extra output", |v| {
            let o = v["outputs"][0].clone();
            v["outputs"].as_array_mut().unwrap().push(o)
        }),
        ("lock time", |v| v["lockTime"] = "1".into()),
        ("change address", |v| v["outputs"][2]["scriptPublicKey"] = format!("0000{}", hex(lottery::buy::spk_of(&[9u8; 32]).script())).into()),
    ];
    for (name, edit) in tampers {
        let b = p.build(&json!({"kind": "buy", "variant": "indices"})).await?;
        let signed = standin_sign(b["txJsonString"].as_str().unwrap(), Some(&indices(&b)), &sk)?;
        let mut v: Value = serde_json::from_str(&signed).map_err(|e| e.to_string())?;
        edit(&mut v);
        let r = p.signed(&json!({"id": b["id"], "signed": v.to_string()})).await?;
        if r["ok"].as_bool() != Some(false) || r["stage"] != "post-sign" {
            return Err(format!("tampered `{name}` was not refused at the post-sign check: {r}"));
        }
        println!("selftest: tampered `{name}` refused: {}", r["refusals"]);
    }

    // Honest buys: listed indices, then no list.
    let mut txids = vec![];
    for variant in ["indices", "nolist"] {
        let b = p.build(&json!({"kind": "buy", "variant": variant})).await?;
        let list = indices(&b);
        let signed = standin_sign(b["txJsonString"].as_str().unwrap(), if variant == "indices" { Some(&list) } else { None }, &sk)?;
        let r = p.signed(&json!({"id": b["id"], "signed": signed})).await?;
        if r["ok"].as_bool() != Some(true) {
            return Err(format!("buy/{variant} failed: {r}"));
        }
        txids.push(format!("buy/{variant} {}", r["txid"].as_str().unwrap_or("")));
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }

    // close(): wait for T, then the same round trip.
    let deadline = p.inst.as_ref().unwrap().deadline;
    cases::wait_daa(&p.ctx, deadline + 12, "probe deadline T").await?;
    let b = p.build(&json!({"kind": "close", "variant": "indices"})).await?;
    let signed = standin_sign(b["txJsonString"].as_str().unwrap(), Some(&indices(&b)), &sk)?;
    let r = p.signed(&json!({"id": b["id"], "signed": signed})).await?;
    if r["ok"].as_bool() != Some(true) {
        return Err(format!("close failed: {r}"));
    }
    txids.push(format!("close/indices {}", r["txid"].as_str().unwrap_or("")));
    p.log.ev("selftest-done", json!({"summary": format!("PASS: {}", txids.join(", ")), "txids": txids}));
    Ok(())
}

/// `send <kaspatest:address> <KAS> [--parts n]`: pay `kas` KAS from TN10_PRIVATE_KEY to a
/// testnet address, split into `parts` equal outputs.
pub async fn send(ctx: &mut Ctx, to: &str, kas: f64, parts: usize) -> R<()> {
    use crate::probe::{p2pk_in, plain};
    if !to.starts_with("kaspatest:") {
        return Err(format!("`{to}` is not a kaspatest: address; refusing"));
    }
    let a = Address::try_from(to).map_err(|e| format!("`{to}`: {e}"))?;
    crate::net::check_testnet(&a)?;
    if !(0.2..=1_000.0).contains(&kas) || parts == 0 || parts > 10 {
        return Err("amount must be 0.2..1000 KAS and parts 1..10".into());
    }
    let total = (kas * 1e8).round() as u64;
    let spk = kaspa_txscript::pay_to_address_script(&a);
    let outs: Vec<_> = (0..parts).map(|i| plain(total / parts as u64 + if i == 0 { total % parts as u64 } else { 0 }, &spk)).collect();
    let f = ctx.funds().await?.into_iter().find(|u| u.entry.amount > total + KAS).ok_or("no TN10 UTXO large enough")?;
    let s = ctx.build(&[p2pk_in(&f)], &outs, &[])?;
    if !ctx.submit(&format!("send {kas} KAS to {to} in {parts} output(s)"), &s, true).await? {
        return Err("send rejected".into());
    }
    println!("sent {kas} KAS to {to}: txid {}", s.tx.id());
    Ok(())
}
