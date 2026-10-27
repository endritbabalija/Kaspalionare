//! Spec M8 A5 (`tn10 campaign <start|resume|status>`): the testnet-10 campaign C0-C8, step
//! by step, resumable.
//!
//! - Every transaction is built by the `lottery` builders from the instance's rebuilt state
//!   (`lottery::index::Rebuild`, fed by the indexer from the node's virtual chain) or, for
//!   claims, from the wallet store; the offline builders `DrawCase::from_store` (every draw,
//!   from the index) and `ClaimCase::from_store` (C4/C6 from the wallet, C5c from the index,
//!   C8c from both, compared, the indexer-built one submitted)
//!   take no RPC type. Each one runs in the local v2.1.0 engine before submission.
//! - `campaign/1` (`tn10-data/<cov>/campaign.json`) is written after every action. An action
//!   is named ("C2/buy/17"); its txid is recorded before submission, and `resume` skips an
//!   action whose recorded transaction is chain-accepted (idempotent by txid). An expected
//!   refusal is recorded as a check and is not repeated.
//! - Expected refusals (A6 "testnet attack checks") are matched by the node's rejection text
//!   (`Refusal::matches`): a failed script is "failed to verify the signature script"
//!   (rusty-kaspa v2.1.0 `consensus/core/src/errors/tx.rs:81-82`), a spent input with orphans
//!   disallowed "is an orphan where orphan is disallowed" (`mining/errors/src/mempool.rs:77-78`)
//!   or "already spent by transaction ... in the mempool" (`:33-34`). A refusal the node does
//!   not give is a FINDING and stops the run; an unexpected rejection stops it too (D104).
//! - Fees: `fees::with_fee` at the node's normal-bucket rate (D120); never a fixed fee.
//! - `--dry-run` runs the same steps against `Local`: a ledger, the local engine (with the
//!   mempool's relay-floor and storage-mass checks), fabricated chain blocks fed to the same
//!   indexer code, `ChainSim::anchored` anchors and simulated time (DAA / 10 s). Nothing is
//!   sent anywhere.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use harness::ChainSim;
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::Hash;
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::sign::sign_input;
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::tx::{
    PopulatedTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_rpc_core::{RpcTransaction, api::rpc::RpcApi};
use kaspa_wrpc_client::KaspaRpcClient;
use lottery::anchor::AnchorProof;
use lottery::buy::{BuyCase, Final, OwnerKey, Templates, b_max, spk_of};
use lottery::claim::{ClaimCase, RefreshCase};
use lottery::close::{CloseCase, FundingIn, ShardIn};
use lottery::combo::{Combo, RANKS, unrank};
use lottery::draw::{DrawCase, RoundResult};
use lottery::game::{D_IDLE, SettleMode, VAULT_REQ_DELAY, anchor_daa, w_min, wc_witness_ok};
use lottery::genesis::GenesisCase;
use lottery::index::{Accepted, Event, Live, Role, State, classify, parse_settle, refresh_obj};
use lottery::ledger::Ledger;
use lottery::params::{ADMIN_SK, SetParamsCase, admin_hash, blake2b, xonly};
use lottery::state::{Payout, ShardState};
use lottery::store::{self, Campaign, CheckJson, CheckStatus, ClaimStore, Funding, Payer, StepJson, TxRef, Wallet};
use lottery::vault::{DepositCase, SettleCase, WithdrawCase};

use crate::indexer::{self, ChainBatch, ChainBlock, ChainTx, Indexer};
use crate::{anchor, fees, net};

type R<T> = Result<T, String>;

pub const KAS: u64 = 100_000_000;
pub const K: usize = 16;
/// The A5 steps in order. C8 is the soak in three sittings (owner note mode B).
pub const STEPS: [&str; 13] = ["C0", "C1", "C2", "C3", "C4", "C5", "C5b", "C5c", "C6", "C7", "C8a", "C8b", "C8c"];
const DONE: &str = "done";
/// A2: round length and d_min (10 min at 10 BPS).
pub const ROUND_LEN: i64 = 6_000;
/// A2: J(1) 10 KAS, V0 100 KAS, m(1) 3.
pub const J1: i64 = 10 * KAS as i64;
pub const V0: i64 = 100 * KAS as i64;
/// A5 C5: round 2's draw is held back 60 min (36,000 DAA at 10 BPS).
pub const LATE_DAA: u64 = 36_000;
/// A5 C8: the soak's timing (seconds after t0).
pub const H: u64 = 3_600;
/// Spare DAA after a deadline or lock time before a transaction that needs it final.
const SETTLE_DAA: u64 = 20;

// -------------------------------------------------------------------------- pure helpers

/// The kinds of expected refusal and how the node words them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A covenant `require` fails: `TxRuleError::SignatureInvalid` (consensus/core/src/errors/tx.rs:81-82).
    Script,
    /// An input is already spent: the request of a second `settle()`.
    Spent,
}

impl Refusal {
    pub fn matches(self, text: &str) -> bool {
        match self {
            Refusal::Script => text.contains("failed to verify the signature script"),
            Refusal::Spent => text.contains("is an orphan where orphan is disallowed") || (text.contains("already spent by transaction") && text.contains("in the mempool")),
        }
    }
}

/// The step after `s` (`done` after the last).
pub fn next_step(s: &str) -> String {
    match STEPS.iter().position(|x| *x == s) {
        Some(i) if i + 1 < STEPS.len() => STEPS[i + 1].to_string(),
        _ => DONE.to_string(),
    }
}

/// Whether step `s` comes after `until` (the run stops before it).
pub fn past(s: &str, until: &str) -> bool {
    if s == DONE {
        return true;
    }
    match (STEPS.iter().position(|x| *x == s), STEPS.iter().position(|x| *x == until)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// The campaign's record of action `action`: its txids, newest last.
pub fn action_txids(c: &Campaign, action: &str) -> Vec<[u8; 32]> {
    c.steps.iter().flat_map(|s| &s.txids).filter(|t| t.action == action).map(|t| t.txid).collect()
}

/// Done = one of its recorded transactions is chain-accepted (A3: idempotent by txid).
pub fn action_done(c: &Campaign, action: &str, accepted: impl Fn(&[u8; 32]) -> bool) -> bool {
    action_txids(c, action).iter().any(|t| accepted(t))
}

pub fn check_done(c: &Campaign, name: &str) -> bool {
    c.steps.iter().flat_map(|s| &s.checks).any(|k| k.name == name)
}

/// Owner note mode A: one long sitting (PC on, no sleep). Mode B: three sittings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoakMode {
    A,
    B,
}

/// The next soak sitting (owner note, spec C8 amended for mode B) given the campaign's
/// step, the soak's t0 (unix seconds, set by C8a) and now. Times are unix seconds.
pub fn next_sitting(step: &str, t0: Option<u64>, now: u64, mode: SoakMode) -> String {
    let at = |s: u64| format!("t0 + {:.1} h ({}, unix {s}, in {:.1} h)", (s - t0.unwrap_or(0)) as f64 / H as f64, utc(s), s.saturating_sub(now) as f64 / H as f64);
    match (step, t0) {
        ("C8b", Some(t0)) => {
            let (lo, hi) = (t0 + 8 * H, t0 + 24 * H);
            if now > hi {
                format!("C8b window missed: the indexer catch-up had to run by {}; its resume block may be below the pruning window (it will stop with an error, never skip)", at(hi))
            } else {
                format!("next sitting: C8b, indexer catch-up (about 5 min), between {} and {}", at(lo), at(hi))
            }
        }
        ("C8c", Some(t0)) => {
            let close = if mode == SoakMode::A { t0 + 20 * H } else { t0 + 29 * H + H / 2 };
            format!(
                "next sitting: C8c, close() at or after {} (mode {:?}), then the deletion evidence, draw() at or after {} and no later than the close's D + F (12 h), claim and refresh() (about 1.5 h)",
                at(close),
                mode,
                at(t0 + 30 * H)
            )
        }
        ("C8a", _) => "next sitting: C8a, the soak start (a round past its deadline is closed and drawn empty first, then the soak round's buys with the indexer running, about 30 min); any time".to_string(),
        (s, _) => format!("next step: {s}"),
    }
}

/// Unix seconds as a UTC wall-clock string, `YYYY-MM-DD HH:MM UTC` (civil-from-days,
/// H. Hinnant's algorithm; no time-zone crate in tn10).
pub fn utc(t: u64) -> String {
    let (days, secs) = ((t / 86_400) as i64, t % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", secs / 3_600, secs % 3_600 / 60)
}

/// S4: the open round cannot host the soak's buys (its deadline has passed or leaves
/// under d_min / 2 of sales), so C8a closes and draws it empty first.
pub fn needs_pre_soak_close(deadline: u64, d_min: u64, virtual_daa: u64) -> bool {
    virtual_daa + d_min / 2 > deadline
}

/// A claim's covenant inputs: ticket (0) and pool (1); funding from input 2 (A4 shape).
pub const CLAIM_FUNDING_FROM: usize = 2;

/// S4 owner note (dual claim, C8c): two builds of the same claim (wallet store and indexer
/// store) must be identical in every field except the funding inputs' signature scripts
/// (randomised Schnorr signatures), as IX2's `same_tx` (`m8_index.rs`) compares them: the
/// signature scripts of inputs >= `from` are cleared, then the whole transactions must be
/// equal. `Ok(detail)`: identical (the detail says whether the funding signatures differ);
/// `Err`: the first difference found.
pub fn same_except_funding_sigs(a: &Transaction, b: &Transaction, from: usize) -> R<String> {
    if a.inputs.len() != b.inputs.len() || a.inputs.len() < from {
        return Err(format!("input count {} vs {}", a.inputs.len(), b.inputs.len()));
    }
    let strip = |t: &Transaction| {
        let mut t = t.clone();
        for i in from..t.inputs.len() {
            t.inputs[i].signature_script.clear();
        }
        t.finalize();
        t
    };
    let (sa, sb) = (strip(a), strip(b));
    if sa != sb {
        let diff = (0..sa.inputs.len())
            .find(|&i| sa.inputs[i] != sb.inputs[i])
            .map(|i| format!("input {i}{}", if i < from { " (covenant)" } else { " (funding, outside its signature)" }))
            .or_else(|| {
                if sa.outputs.len() != sb.outputs.len() {
                    return Some(format!("output count {} vs {}", sa.outputs.len(), sb.outputs.len()));
                }
                (0..sa.outputs.len()).find(|&i| sa.outputs[i] != sb.outputs[i]).map(|i| format!("output {i}"))
            })
            .unwrap_or_else(|| "a transaction field (version, lock time, subnetwork, gas, payload or mass)".into());
        return Err(diff);
    }
    let sigs = (from..a.inputs.len()).filter(|&i| a.inputs[i].signature_script != b.inputs[i].signature_script).count();
    Ok(format!(
        "{} covenant inputs and all {} outputs identical; {sigs} of {} funding signature scripts differ (randomised Schnorr)",
        from,
        a.outputs.len(),
        a.inputs.len() - from
    ))
}

/// Dry-run hook (`TN10_DRY_C8_WIN=1`): the `Local` anchor of round `round` is ground until
/// one of `combos` wins a tier, so the C8c dual claim is exercised (the live run is luck).
#[derive(Clone, Debug)]
pub struct ForceWin {
    pub cov: [u8; 32],
    pub round: i64,
    pub m: i64,
    pub combos: Vec<[u8; 6]>,
}

/// The first `xh` (from `seed`, a counter appended) whose seed makes one of `f.combos`
/// win a tier; returns the hash and the number of tries.
pub fn grind_anchor(seed: &[u8], f: &ForceWin) -> ([u8; 32], u64) {
    for n in 0u64.. {
        let mut xh = blake2b(&[seed, &n.to_le_bytes()].concat());
        xh[0] |= 1;
        let s = lottery::game::seed_numbers(&f.cov, f.round, &xh, f.m);
        let win = [s.main[0], s.main[1], s.main[2], s.main[3], s.main[4], s.bonus];
        if f.combos.iter().any(|c| lottery::game::tier_of(c, &win).is_some()) {
            return (xh, n + 1);
        }
    }
    unreachable!()
}

/// Which store a claim is built from. `Both` (S4 owner note, C8c): from the wallet and from
/// the indexer, compared (`same_except_funding_sigs`), and the indexer-built one submitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimFrom {
    Wallet,
    Index,
    Both,
}

/// A panic in a builder (a reference that rejects) becomes an error.
fn guarded<T>(what: &str, f: impl FnOnce() -> T) -> R<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|p| {
        let m = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
        format!("{what}: the builder refuses: {m}")
    })
}

fn kas(v: u64) -> String {
    format!("{:.8} KAS", v as f64 / KAS as f64)
}

fn hex(b: &[u8]) -> String {
    store::to_hex(b)
}

// ------------------------------------------------------------------------------ backends

/// What the runner needs from a network. `Node` is a public testnet-10 node; `Local` is the
/// dry run.
#[allow(async_fn_in_trait)]
pub trait Net {
    fn node(&self) -> String;
    fn dry(&self) -> bool;
    /// Unix seconds (simulated in a dry run).
    async fn now(&mut self) -> R<u64>;
    async fn daa(&mut self) -> R<u64>;
    /// Wait until the virtual DAA score is above `target`; returns it.
    async fn wait_daa(&mut self, target: u64, what: &str) -> R<u64>;
    async fn sink(&mut self) -> R<(Hash, u64)>;
    async fn fee_rate(&mut self) -> R<fees::Rate>;
    /// Submit (orphans disallowed); `Err` is the node's rejection text.
    async fn submit(&mut self, f: &Final) -> Result<(), String>;
    /// The applied events and the number of transactions reverted by a chain reorg.
    async fn sync(&mut self, x: &mut Indexer) -> R<Synced>;
    async fn wait_accepted(&mut self, x: &mut Indexer, ids: &[TransactionId]) -> R<Synced>;
    /// Our spendable P2PK UTXOs, each with its chain-accepted creation DAA.
    async fn funds(&mut self, x: &Indexer) -> R<Vec<Funding>>;
    async fn utxo(&mut self, op: TransactionOutpoint, spk: &ScriptPublicKey) -> R<Option<UtxoEntry>>;
    /// The anchor proof for D (and the chain the local engine runs it against).
    async fn anchor(&mut self, start: Hash, d: u64) -> R<(AnchorProof, Option<Arc<ChainSim>>)>;
    /// Deletion evidence for block `h` (A4).
    async fn deleted(&mut self, h: Hash) -> R<Option<String>>;
    /// Before a DAA-sensitive transaction: the node's entries of its inputs must be the
    /// builder's (amount, creation DAA, covenant ID).
    async fn verify_inputs(&mut self, f: &Final, ops: &[TransactionOutpoint]) -> R<()>;
    /// Dry-run hook only (`ForceWin`); a live node ignores it.
    fn force_win(&mut self, _f: ForceWin) {}
}

/// A public testnet-10 node (`net::connect` guards the network).
pub struct Node {
    pub client: KaspaRpcClient,
    pub addr: Address,
    pub url: String,
    pub to: anchor::Timeouts,
    /// `TN10_WRPC_URL`, or `None` for the public resolver (used again on a reconnect).
    pub cfg_url: Option<String>,
}

/// Index catch-up retries after a node or connection failure (M8 S4).
pub const POLL_RETRIES: usize = 12;

/// The wait before retry `n` (1-based): 5 s doubling, at most 120 s.
pub fn retry_backoff(n: usize) -> u64 {
    (5u64 << n.saturating_sub(1).min(5)).min(120)
}

impl Node {
    /// A fresh connection (through the resolver unless `TN10_WRPC_URL` is set, so usually
    /// another public node); the network guard of `net::connect` applies again.
    pub async fn reconnect(&mut self) -> R<()> {
        let _ = self.client.disconnect().await;
        let c = net::connect(self.cfg_url.clone()).await?;
        self.url = c.url().unwrap_or_default();
        self.client = c;
        Ok(())
    }

    /// `indexer::poll` with bounded retries on node or connection failures (M8 S4): each
    /// batch is persisted as it is ingested, so a retry resumes from the last saved
    /// position on a new connection; the events of batches before the failure are kept.
    /// A non-transient error (a pruned resume block) stops at once.
    pub async fn poll_retry(&mut self, x: &mut Indexer) -> R<indexer::PollStats> {
        let mut st = indexer::PollStats::default();
        let mut n = 0;
        loop {
            match indexer::poll_into(&self.client, x, &mut st).await {
                Ok(()) => return Ok(st),
                Err(e) if indexer::transient(&e) && n < POLL_RETRIES => {
                    n += 1;
                    let w = retry_backoff(n);
                    println!("  indexer: {e}; retry {n}/{POLL_RETRIES} in {w} s on a new connection (resume block {}, DAA {})", x.last, x.last_daa);
                    tokio::time::sleep(std::time::Duration::from_secs(w)).await;
                    if let Err(c) = self.reconnect().await {
                        println!("  indexer: reconnect failed: {c}");
                    }
                }
                Err(e) => return Err(if n > 0 { format!("{e} (after {n} retries)") } else { e }),
            }
        }
    }
}

impl Net for Node {
    fn node(&self) -> String {
        self.url.clone()
    }
    fn dry(&self) -> bool {
        false
    }
    async fn now(&mut self) -> R<u64> {
        Ok(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs())
    }
    async fn daa(&mut self) -> R<u64> {
        Ok(self.client.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score)
    }
    async fn wait_daa(&mut self, target: u64, what: &str) -> R<u64> {
        let t0 = std::time::Instant::now();
        let mut said = 0;
        loop {
            let now = self.daa().await?;
            if now > target {
                return Ok(now);
            }
            let m = t0.elapsed().as_secs() / 60;
            if m >= said {
                println!("  waiting for {what}: virtual DAA {now}, target {target} (about {:.1} min)", (target - now) as f64 / 600.0);
                said = m + 5;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
    async fn sink(&mut self) -> R<(Hash, u64)> {
        let s = self.client.get_block_dag_info().await.map_err(|e| e.to_string())?.sink;
        let d = self.client.get_block(s, false).await.map_err(|e| e.to_string())?.header.daa_score;
        Ok((s, d))
    }
    async fn fee_rate(&mut self) -> R<fees::Rate> {
        fees::estimate(&self.client).await
    }
    async fn submit(&mut self, f: &Final) -> Result<(), String> {
        self.client.submit_transaction(RpcTransaction::from(&f.tx), false).await.map(|_| ()).map_err(|e| e.to_string())
    }
    async fn sync(&mut self, x: &mut Indexer) -> R<Synced> {
        let p = self.poll_retry(x).await?;
        Ok((p.events, p.reverted))
    }
    async fn wait_accepted(&mut self, x: &mut Indexer, ids: &[TransactionId]) -> R<Synced> {
        for id in ids {
            x.watch(*id);
        }
        let t0 = std::time::Instant::now();
        let mut evs = vec![];
        let mut reverted = 0;
        loop {
            let p = self.poll_retry(x).await?;
            evs.extend(p.events);
            reverted += p.reverted;
            let left: Vec<_> = ids.iter().filter(|i| x.accepted_at(i).is_none()).collect();
            if left.is_empty() {
                return Ok((evs, reverted));
            }
            if t0.elapsed() > std::time::Duration::from_secs(600) {
                return Err(format!("not chain-accepted within 600 s: {left:?}"));
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
    async fn funds(&mut self, x: &Indexer) -> R<Vec<Funding>> {
        let vdaa = self.daa().await?;
        let v = net::utxos(&self.client, &self.addr).await?;
        Ok(v.into_iter()
            .filter_map(|e| {
                let op: TransactionOutpoint = e.outpoint.into();
                let edaa = e.utxo_entry.block_daa_score;
                // The creation DAA a covenant reads (close T', draw check 19) is the accepting
                // chain block's: known for every transaction the runner waited for; an older
                // entry (a faucet payment) is settled.
                let daa = match x.accepted_at(&op.transaction_id) {
                    Some((_, d)) => d,
                    None if edaa.saturating_add(6_000) < vdaa => edaa,
                    None => return None,
                };
                Some(Funding { outpoint: op, value: e.utxo_entry.amount, daa })
            })
            .collect())
    }
    async fn utxo(&mut self, op: TransactionOutpoint, spk: &ScriptPublicKey) -> R<Option<UtxoEntry>> {
        let addr = kaspa_txscript::extract_script_pub_key_address(spk, Prefix::Testnet).map_err(|e| e.to_string())?;
        let v = net::utxos(&self.client, &addr).await?;
        Ok(v.into_iter().find(|e| TransactionOutpoint::from(e.outpoint.clone()) == op).map(|e| e.utxo_entry.into()))
    }
    async fn anchor(&mut self, start: Hash, d: u64) -> R<(AnchorProof, Option<Arc<ChainSim>>)> {
        let a = anchor::rpc_anchor(&self.client, start, d, &self.to).await?;
        anchor::wait_blue_score(&self.client, a.x.sim.blue_score + 50, &self.to).await?;
        let sim = ChainSim::from_real_chain(Hash::from_bytes(a.p.fields.parent_commit), &[a.p.sim, a.x.sim]);
        Ok((a.proof, Some(Arc::new(sim))))
    }
    async fn deleted(&mut self, h: Hash) -> R<Option<String>> {
        indexer::deleted_block(&self.client, h).await
    }
    async fn verify_inputs(&mut self, f: &Final, ops: &[TransactionOutpoint]) -> R<()> {
        for (i, op) in ops.iter().enumerate() {
            let want = &f.entries[i];
            let got = self.utxo(*op, &want.script_public_key).await?.ok_or_else(|| format!("input {i} {op}: not in the node's UTXO index"))?;
            if got.amount != want.amount || got.covenant_id != want.covenant_id || (want.covenant_id.is_some() && got.block_daa_score != want.block_daa_score) {
                return Err(format!(
                    "input {i} {op}: node entry (amount {}, DAA {}, cov {:?}) != builder entry (amount {}, DAA {}, cov {:?})",
                    got.amount, got.block_daa_score, got.covenant_id, want.amount, want.block_daa_score, want.covenant_id
                ));
            }
        }
        Ok(())
    }
}

/// What a sync returns: the applied events, and the count of transactions a reorg reverted.
pub type Synced = (Vec<(Accepted, Event)>, usize);

/// The dry run: a ledger, the local engine and fabricated chain blocks (one per accepted
/// transaction). Simulated time is DAA / 10 s; a block older than `retention` DAA is
/// "deleted" (the soak's evidence).
pub struct Local {
    ledger: Ledger,
    daa: u64,
    spk: ScriptPublicKey,
    mine: std::collections::HashMap<TransactionOutpoint, (u64, u64)>,
    pending: Vec<ChainBlock>,
    blocks: BTreeMap<Hash, u64>,
    n: u64,
    pub retention: u64,
    pub rate: fees::Rate,
    /// Every submission and its outcome (the dry-run report).
    pub submitted: usize,
    pub refused: usize,
    /// Test hook (`TN10_DRY_REORG=1`): reorg the chain block of the first synced buy once
    /// (removed, then its transactions re-added in a new block at the same DAA), as on
    /// testnet-10 in S3b.
    pub reorg_first_buy: bool,
    /// Test hook (`TN10_DRY_C8_WIN=1`, set by C8c): the next anchor is ground so that one of
    /// our soak-round combinations wins a tier.
    pub force: Option<ForceWin>,
}

impl Local {
    /// Funds `budget` sompi in four faucet-like UTXOs at DAA `start - 10,000`.
    pub fn new(spk: ScriptPublicKey, budget: u64, start: u64, rate: fees::Rate) -> Self {
        let mut me = Self {
            ledger: Ledger::new(),
            daa: start,
            spk: spk.clone(),
            mine: std::collections::HashMap::new(),
            pending: vec![],
            blocks: BTreeMap::new(),
            n: 0,
            retention: 30 * 36_000,
            rate,
            submitted: 0,
            refused: 0,
            reorg_first_buy: false,
            force: None,
        };
        for _ in 0..4 {
            let op = me.ledger.fund(budget / 4, spk.clone(), start - 10_000);
            me.mine.insert(op, (budget / 4, start - 10_000));
        }
        me
    }

    fn next_hash(&mut self, tag: &[u8]) -> Hash {
        self.n += 1;
        Hash::from_bytes(blake2b(&[tag, &self.n.to_le_bytes()].concat()))
    }

    pub fn balance(&self) -> u64 {
        self.mine.values().map(|v| v.0).sum()
    }
}

impl Net for Local {
    fn node(&self) -> String {
        "local dry run (no network)".into()
    }
    fn dry(&self) -> bool {
        true
    }
    async fn now(&mut self) -> R<u64> {
        Ok(1_800_000_000 + self.daa / 10)
    }
    async fn daa(&mut self) -> R<u64> {
        Ok(self.daa)
    }
    async fn wait_daa(&mut self, target: u64, _what: &str) -> R<u64> {
        if self.daa <= target {
            self.daa = target + 1;
        }
        Ok(self.daa)
    }
    async fn sink(&mut self) -> R<(Hash, u64)> {
        let h = self.next_hash(b"tn10 dry-run sink");
        self.blocks.insert(h, self.daa);
        Ok((h, self.daa))
    }
    async fn fee_rate(&mut self) -> R<fees::Rate> {
        Ok(self.rate)
    }
    async fn submit(&mut self, f: &Final) -> Result<(), String> {
        self.submitted += 1;
        let tx = &f.tx;
        let id = tx.id();
        let refused = |me: &mut Self, e: String| {
            me.refused += 1;
            Err(e)
        };
        // Mempool order (rusty-kaspa v2.1.0 `mining/src/mempool/validate_and_insert_transaction.rs`):
        // the entries first (a missing outpoint with orphans disallowed, `:61-63`), then the
        // standard checks and the scripts.
        let entries = match self.ledger.entries(tx) {
            Ok(e) => e,
            Err(_) => return refused(self, format!("transaction {id} is an orphan where orphan is disallowed")),
        };
        for (i, (s, b)) in entries.iter().zip(&f.entries).enumerate() {
            let daa_ok = s.covenant_id.is_none() || s.block_daa_score == b.block_daa_score;
            if s.amount != b.amount || s.script_public_key != b.script_public_key || s.covenant_id != b.covenant_id || !daa_ok {
                return Err(format!("LOCAL BUILDER ERROR: input {i}: the builder's entry differs from the ledger's"));
            }
        }
        if tx.lock_time >= self.daa {
            return refused(self, format!("transaction {id} lock time {} is not final at DAA {}", tx.lock_time, self.daa));
        }
        let m = harness::masses(tx, &entries);
        if tx.storage_mass() != m.storage {
            return refused(self, format!("transaction {id}: committed storage mass {} != {}", tx.storage_mass(), m.storage));
        }
        let fee = entries.iter().map(|e| e.amount).sum::<u64>().checked_sub(tx.outputs.iter().map(|o| o.value).sum::<u64>()).unwrap_or(0);
        if fee < lottery::buy::relay_floor_fee(&m) {
            return refused(self, format!("transaction {id} is not standard: fee {fee} below the relay floor"));
        }
        for i in 0..tx.inputs.len() {
            if let Err(e) = lottery::buy::run_script(tx, &entries, i, true, f.sim.as_deref()) {
                return refused(self, format!("failed to verify the signature script: {e} (input {i})"));
            }
        }
        let outs = self.ledger.apply(tx, &entries, self.daa).map_err(|e| format!("ledger: {e:?}"))?;
        for i in &tx.inputs {
            self.mine.remove(&i.previous_outpoint);
        }
        for (op, o) in outs.iter().zip(&tx.outputs) {
            if o.script_public_key == self.spk && o.covenant.is_none() {
                self.mine.insert(*op, (o.value, self.daa));
            }
        }
        let h = self.next_hash(b"tn10 dry-run block");
        self.blocks.insert(h, self.daa);
        let spent = entries.iter().map(|e| lottery::index::Spent { amount: e.amount, daa: e.block_daa_score }).collect();
        self.pending.push(ChainBlock { hash: h, daa: self.daa, txs: vec![ChainTx { tx: tx.clone(), spent }] });
        self.daa += 2;
        Ok(())
    }
    async fn sync(&mut self, x: &mut Indexer) -> R<Synced> {
        let added = std::mem::take(&mut self.pending);
        let g = x.ingest(ChainBatch { removed: vec![], added: added.clone() })?;
        let (mut events, mut reverted) = (g.events, g.reverted);
        if self.reorg_first_buy && events.iter().any(|(_, e)| matches!(e, Event::Buy(_))) {
            self.reorg_first_buy = false;
            let old = added.last().cloned().ok_or("dry-run reorg: no block")?;
            let h = self.next_hash(b"tn10 dry-run reorg");
            self.blocks.insert(h, old.daa);
            let g2 = x.ingest(ChainBatch { removed: vec![old.hash], added: vec![ChainBlock { hash: h, daa: old.daa, txs: old.txs }] })?;
            println!("  dry run: reorg of chain block {} ({} reverted, {} re-added)", old.hash, g2.reverted, g2.events.len());
            reverted += g2.reverted;
            events.extend(g2.events);
        }
        x.persist()?;
        Ok((events, reverted))
    }
    async fn wait_accepted(&mut self, x: &mut Indexer, ids: &[TransactionId]) -> R<Synced> {
        for id in ids {
            x.watch(*id);
        }
        let evs = self.sync(x).await?;
        if let Some(i) = ids.iter().find(|i| x.accepted_at(i).is_none()) {
            return Err(format!("dry run: {i} was not accepted"));
        }
        Ok(evs)
    }
    async fn funds(&mut self, _x: &Indexer) -> R<Vec<Funding>> {
        Ok(self.mine.iter().map(|(op, (v, d))| Funding { outpoint: *op, value: *v, daa: *d }).collect())
    }
    async fn utxo(&mut self, op: TransactionOutpoint, _spk: &ScriptPublicKey) -> R<Option<UtxoEntry>> {
        Ok(self.ledger.get(&op).cloned())
    }
    async fn anchor(&mut self, _start: Hash, d: u64) -> R<(AnchorProof, Option<Arc<ChainSim>>)> {
        self.wait_daa(d + 60, "X").await?;
        let seed = [b"tn10 dry-run anchor".as_slice(), &d.to_le_bytes()].concat();
        let mut xh = blake2b(&seed);
        xh[0] |= 1;
        if let Some(f) = self.force.take() {
            let (g, tries) = grind_anchor(&seed, &f);
            println!("  dry run: anchor ground in {tries} tries so that a round-{} combination of ours wins (TN10_DRY_C8_WIN)", f.round);
            xh = g;
        }
        let sim = ChainSim::anchored(d, Hash::from_bytes(xh));
        // `DrawCase::from_store` runs a given proof against `ChainSim::anchored(D, X.hash)`,
        // the same chain, so no sim is returned.
        Ok((AnchorProof::from_sim(&sim, d), None))
    }
    async fn deleted(&mut self, h: Hash) -> R<Option<String>> {
        Ok(match self.blocks.get(&h) {
            None => Some(format!("dry run: block {h} unknown")),
            Some(d) if d + self.retention < self.daa => Some(format!("dry run: block {h} (DAA {d}) is older than the simulated retention of {} DAA", self.retention)),
            Some(_) => None,
        })
    }
    async fn verify_inputs(&mut self, _f: &Final, _ops: &[TransactionOutpoint]) -> R<()> {
        Ok(())
    }
    fn force_win(&mut self, f: ForceWin) {
        self.force = Some(f);
    }
}

// ------------------------------------------------------------- genesis knobs (F6 part C)

/// The genesis knobs C1 applies (F6 part C: the beta instance). The default is the M8
/// campaign's (spec 8 A2): 10-minute rounds, m(1) = 3 in [2, 50], J(1) 10 KAS, V0 100 KAS,
/// the rest the library's defaults (`GenesisCase::new_with`: seed_offset `SEED_OFFSET`,
/// draw_window `DRAW_WINDOW` = VOID_MIN, claim_window `CLAIM_WINDOW`, s_C = s_D = 50).
#[derive(Clone, Debug)]
pub struct GenesisKnobs {
    /// A name for the report ("m8", "fast", "steady").
    pub profile: String,
    pub round_len: i64,
    pub d_min: i64,
    /// Round 1's sales: T(1) = the DAA at genesis + this.
    pub first_sales: i64,
    pub m: i64,
    pub m_min: i64,
    pub m_max: i64,
    pub j1: i64,
    pub v0: i64,
    pub s_c: Option<i64>,
    pub s_d: Option<i64>,
    pub seed_offset: Option<i64>,
    pub draw_window: Option<i64>,
    pub claim_window: Option<i64>,
    /// The protocol fee address (controller and params); `None`: the funding key's.
    pub fee_address: Option<Payout>,
}

impl Default for GenesisKnobs {
    fn default() -> Self {
        Self {
            profile: "m8".into(),
            round_len: ROUND_LEN,
            d_min: ROUND_LEN,
            first_sales: ROUND_LEN + 100,
            m: 3,
            m_min: 2,
            m_max: 50,
            j1: J1,
            v0: V0,
            s_c: None,
            s_d: None,
            seed_offset: None,
            draw_window: None,
            claim_window: None,
            fee_address: None,
        }
    }
}

/// 1 hour and 24 hours of DAA at 10 BPS.
pub const HOUR_DAA: i64 = 36_000;
pub const DAY_DAA: i64 = 24 * HOUR_DAA;

impl GenesisKnobs {
    /// The beta profiles (F6 part C, docs/launch/beta-plan.md): the floors for every window
    /// (seed_offset the design's 600, draw_window = VOID_MIN, claim_window = CLAIM_WINDOW),
    /// m(1) = 3 in [2, M_DRAW], and the round length: `fast` one hour, `steady` one day, with
    /// d_min half of it (the next deadline stays on the clock when the close comes within half a
    /// round of T).
    pub fn beta(profile: &str) -> R<Self> {
        let len = match profile {
            "fast" => HOUR_DAA,
            "steady" => DAY_DAA,
            "m8" => return Ok(Self::default()),
            p => return Err(format!("--profile {p}: fast, steady or m8")),
        };
        Ok(Self {
            profile: profile.into(),
            round_len: len,
            // T' = max(T + round_len, close + d_min): a close within half a round keeps the clock time.
            d_min: len / 2,
            first_sales: len,
            seed_offset: Some(lottery::game::SEED_OFFSET),
            draw_window: Some(lottery::game::VOID_MIN),
            claim_window: Some(lottery::game::CLAIM_WINDOW),
            ..Self::default()
        })
    }

    /// Apply to a genesis case whose round 1 ends at `t1` (C1 sets the funding, owner and
    /// admin hash itself). The caller runs `validate()`.
    pub fn apply(&self, g: &mut GenesisCase, t1: i64, fee: &Payout) {
        let fee = self.fee_address.as_ref().unwrap_or(fee);
        {
            let c = &mut g.controller;
            c.m = self.m;
            c.m_min = self.m_min;
            c.m_max = self.m_max;
            c.deadline = t1;
            c.round_len = self.round_len;
            c.d_min = self.d_min;
            c.fee_class = fee.class();
            c.fee_key = fee.key33();
            if let Some(v) = self.s_c {
                c.s_c = v;
            }
            if let Some(v) = self.s_d {
                c.s_d = v;
            }
            if let Some(v) = self.seed_offset {
                c.seed_offset = v;
            }
        }
        if let Some(v) = self.draw_window {
            g.drawer.draw_window = v;
        }
        if let Some(v) = self.claim_window {
            g.drawer.claim_window = v;
        }
        g.params.next_fee_class = fee.class();
        g.params.next_fee_key = fee.key33();
        for s in &mut g.shards {
            s.m = self.m;
            s.deadline = t1;
        }
        g.set_vault(self.v0);
        g.set_jackpot(self.j1);
    }
}

/// The admin key guard (F1 re-review N5, F6 part C): never the public default `ADMIN_SK`
/// (`lottery::params`, a key in the repository: anyone could call `set_params()`), checked on
/// the key and on its hash.
pub fn admin_guard(admin_sk: &[u8; 32]) -> R<()> {
    if *admin_sk == ADMIN_SK || admin_hash(admin_sk) == admin_hash(&ADMIN_SK) {
        return Err("the admin key is the public default ADMIN_SK (lottery::params): refusing; run `tn10 admin-keygen`".into());
    }
    Ok(())
}

// --------------------------------------------------------------------------------- runner

#[derive(Clone, Debug)]
pub struct Opts {
    /// Stop after this step (C7: the fast campaign; C8c: through the soak).
    pub until: String,
    pub soak: SoakMode,
    /// Balance needed before genesis (C0) and each round (sompi).
    pub budget: u64,
    /// Buys per full round (A5: 75).
    pub buys: usize,
    /// C6's plain rounds (A5: 40).
    pub plain_buys: usize,
    /// C5b's buys before the deadline (A5: 20).
    pub c5b_buys: usize,
    /// PENDING A5 amendment: a deposit (sompi) in C6's first round so the carried
    /// withdrawals can settle FINAL (0 = the approved A5 without it).
    pub c6_deposit: u64,
    /// C1's genesis knobs (F6 part C).
    pub genesis: GenesisKnobs,
}

impl Default for Opts {
    fn default() -> Self {
        Self { until: "C7".into(), soak: SoakMode::B, budget: 2_000 * KAS, buys: 75, plain_buys: 40, c5b_buys: 20, c6_deposit: 0, genesis: GenesisKnobs::default() }
    }
}

/// One buy of a wave.
#[derive(Clone, Debug)]
struct BuySpec {
    action: String,
    round: i64,
    shard: usize,
    n: u64,
    b: usize,
    with_ref: bool,
}

pub struct Runner<N: Net> {
    pub net: N,
    pub x: Option<Indexer>,
    pub c: Campaign,
    root: PathBuf,
    sk: [u8; 32],
    me: Payout,
    t: Arc<Templates>,
    wallets: BTreeMap<i64, Wallet>,
    pub opts: Opts,
    rate: u64,
    /// The claim modes seen (C4/C6 "partial" and "last" claims).
    modes: Vec<String>,
    /// The admin key of genesis (`set_params()`); `None`: the funding key (the M8 campaign).
    admin_sk: Option<[u8; 32]>,
}

impl<N: Net> Runner<N> {
    /// `root`: `tn10-data` (or `tn10-data/dry-run`); the instance directory is
    /// `root/<cov-hex>/` once genesis is built.
    pub fn new(net: N, root: PathBuf, sk: [u8; 32], opts: Opts) -> Self {
        Self {
            net,
            x: None,
            c: Campaign::new(),
            root,
            sk,
            me: Payout::Schnorr(xonly(&sk)),
            t: Templates::get(b_max()),
            wallets: BTreeMap::new(),
            opts,
            rate: 100,
            modes: vec![],
            admin_sk: None,
        }
    }

    /// A separate admin key for genesis (F6 part C: the beta's, from `TN10_ADMIN_KEY`).
    pub fn with_admin(mut self, sk: [u8; 32]) -> Self {
        self.admin_sk = Some(sk);
        self
    }

    fn cov(&self) -> R<Hash> {
        self.c.cov.map(Hash::from_bytes).ok_or_else(|| "no genesis yet".to_string())
    }

    fn dir(&self) -> R<PathBuf> {
        Ok(self.root.join(hex(&self.cov()?.as_bytes())))
    }

    fn x(&self) -> R<&Indexer> {
        self.x.as_ref().ok_or_else(|| "the indexer is not open (no genesis)".to_string())
    }

    fn ix(&self) -> R<&lottery::index::Rebuild> {
        Ok(&self.x()?.ix)
    }

    /// Open a saved campaign (`resume`): its file, the indexer (from the bundle's genesis
    /// states) and the wallets rebuilt from the index log.
    pub fn open(&mut self, cov: Hash) -> R<()> {
        let dir = self.root.join(hex(&cov.as_bytes()));
        let text = std::fs::read_to_string(dir.join("campaign.json")).map_err(|e| format!("{}: {e}", dir.display()))?;
        self.c = store::read(&text, store::CAMPAIGN)?;
        if self.c.cov != Some(cov.as_bytes()) {
            return Err("campaign.json names another covenant ID".into());
        }
        let b: lottery::bundle::Bundle = store::read(&std::fs::read_to_string(dir.join("bundle-genesis.json")).map_err(|e| e.to_string())?, store::BUNDLE)?;
        let v = lottery::bundle::verify(&b)?;
        let start = Hash::from_bytes(store::from_hex(self.var("start_hash")?.as_str())?.try_into().map_err(|_| "start_hash")?);
        let start_daa: u64 = self.var("start_daa")?.parse().map_err(|e| format!("start_daa: {e}"))?;
        self.x = Some(Indexer::open(Some(&dir), v.templates.clone(), cov, Some(v.genesis_states), start, start_daa)?);
        self.rebuild_wallets()?;
        Ok(())
    }

    fn var(&self, k: &str) -> R<String> {
        self.c.vars.get(k).cloned().ok_or_else(|| format!("campaign var {k} missing"))
    }

    fn set_var(&mut self, k: &str, v: impl ToString) {
        self.c.vars.insert(k.to_string(), v.to_string());
    }

    pub fn save(&self) -> R<()> {
        let Ok(dir) = self.dir() else { return Ok(()) };
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        indexer::write_atomic(&dir.join("campaign.json"), &store::write(&self.c, store::CAMPAIGN)?)?;
        if !self.net.dry() {
            indexer::write_atomic(&self.root.join("current"), &hex(&self.cov()?.as_bytes()))?;
        }
        Ok(())
    }

    async fn step_mut(&mut self, id: &str) -> R<usize> {
        if let Some(i) = self.c.steps.iter().position(|s| s.id == id) {
            return Ok(i);
        }
        let daa = self.net.daa().await?;
        let wall_time = self.net.now().await?;
        self.c.steps.push(StepJson { id: id.into(), txids: vec![], checks: vec![], daa, wall_time });
        Ok(self.c.steps.len() - 1)
    }

    async fn check(&mut self, step: &str, name: &str, ok: bool, detail: impl Into<String>) -> R<()> {
        self.record(step, CheckJson { name: name.into(), ok, detail: detail.into(), status: None }).await
    }

    /// Record a check (replacing an earlier one of the same name) and save.
    async fn record(&mut self, step: &str, k: CheckJson) -> R<()> {
        let i = self.step_mut(step).await?;
        println!("  [{}] {step} {}: {}", if !k.ok { "FAIL" } else if k.not_exercised() { "not exercised" } else { "ok" }, k.name, k.detail);
        self.c.steps[i].checks.retain(|x| x.name != k.name);
        self.c.steps[i].checks.push(k);
        self.save()
    }

    fn accepted(&self, txid: &[u8; 32]) -> bool {
        self.x.as_ref().is_some_and(|x| x.accepted_at(&Hash::from_bytes(*txid)).is_some())
    }

    fn done(&self, action: &str) -> bool {
        action_done(&self.c, action, |t| self.accepted(t))
    }

    /// The accepted transaction of a done action.
    fn done_tx(&self, action: &str) -> Option<Accepted> {
        let x = self.x.as_ref()?;
        let ids = action_txids(&self.c, action);
        x.ix.log().iter().rev().find(|a| ids.contains(&a.tx.id().as_bytes())).cloned()
    }

    // ---- wallet feed (A3: the runner feeds every chain-accepted buy of the round)

    fn wallet_mut(&mut self, round: i64, m: i64) -> R<&mut Wallet> {
        let cov = self.cov()?;
        Ok(self.wallets.entry(round).or_insert_with(|| Wallet::new(cov, b_max(), round, m, K)))
    }

    fn feed(&mut self, evs: &[(Accepted, Event)]) -> R<()> {
        let t = self.t.clone();
        let me = self.me;
        let mut touched = std::collections::BTreeSet::new();
        for (a, ev) in evs {
            match ev {
                Event::Buy(b) => {
                    let w = self.wallet_mut(b.shard.round, b.shard.m)?;
                    w.record_buy(&t, &a.tx, a.daa, &[me]).map_err(|e| format!("wallet: {e}"))?;
                    touched.insert(b.shard.round);
                }
                Event::Close(c) => {
                    let w = self.wallet_mut(c.controller.round, c.controller.m)?;
                    w.freeze(&c.frozen_roots).map_err(|e| format!("wallet round {}: {e}", c.controller.round))?;
                    touched.insert(c.controller.round);
                }
                Event::Draw(d) => {
                    let w = self.wallet_mut(d.result.round, d.result.m)?;
                    w.set_draw(&t, &d.result, a.daa)?;
                    touched.insert(d.result.round);
                }
                Event::Claim(c) => {
                    if let Some(w) = self.wallets.get_mut(&c.ticket.round) {
                        let top: store::OutpointJson = a.tx.inputs[0].previous_outpoint.into();
                        let pop: store::OutpointJson = a.tx.inputs[1].previous_outpoint.into();
                        if w.tickets.iter().any(|x| x.outpoint == top) {
                            w.apply_claim(&t, &a.tx, a.daa)?;
                        } else if let Some(p) = w.pools.iter_mut().find(|p| p.outpoint == pop) {
                            // A third party's claim moves the pool too.
                            p.outpoint = TransactionOutpoint::new(a.tx.id(), 0).into();
                            p.value = a.tx.outputs[0].value;
                            p.daa = a.daa;
                        }
                        touched.insert(c.ticket.round);
                    }
                }
                _ => {}
            }
        }
        if let Ok(dir) = self.dir() {
            for r in touched {
                indexer::write_atomic(&dir.join(format!("wallet-r{r}.json")), &store::write(&self.wallets[&r], store::WALLET)?)?;
            }
        }
        Ok(())
    }

    /// The wallets from the accepted transactions in the index log (deterministic; `resume`).
    fn rebuild_wallets(&mut self) -> R<()> {
        self.wallets.clear();
        let cov = self.cov()?;
        let evs: Vec<(Accepted, Event)> =
            self.ix()?.log().iter().filter_map(|a| classify(&a.tx, &self.t, cov).map(|e| (a.clone(), e))).collect();
        self.feed(&evs)
    }

    async fn sync(&mut self) -> R<()> {
        let x = self.x.as_mut().ok_or("no indexer")?;
        let s = self.net.sync(x).await?;
        self.apply_synced(s)
    }

    async fn wait_txs(&mut self, ids: &[TransactionId]) -> R<()> {
        let x = self.x.as_mut().ok_or("no indexer")?;
        let s = self.net.wait_accepted(x, ids).await?;
        self.apply_synced(s)
    }

    /// Feed new events to the wallets; after a chain reorg (the index reverted transactions,
    /// which may come back in other chain blocks) rebuild them from the index log instead,
    /// since `feed` only adds (S3b: a reorg re-added a round-4 buy and the wallet refused
    /// the second ingestion, "spent root is not the wallet's tree root").
    fn apply_synced(&mut self, (evs, reverted): Synced) -> R<()> {
        if reverted > 0 {
            println!("  chain reorg: {reverted} instance transaction(s) reverted; wallets rebuilt from the index log");
            self.rebuild_wallets()
        } else {
            self.feed(&evs)
        }
    }

    // ---- submission

    async fn record_tx(&mut self, step: &str, action: &str, id: TransactionId) -> R<()> {
        let i = self.step_mut(step).await?;
        let node = self.net.node();
        self.c.steps[i].txids.push(TxRef { txid: id.as_bytes(), node, action: action.into() });
        self.save()
    }

    /// Run locally, record, submit (not waiting).
    async fn send(&mut self, step: &str, action: &str, f: &Final) -> R<TransactionId> {
        f.run().map_err(|(i, e)| format!("{action}: the local engine rejects input {i}: {e:?}; not submitted"))?;
        let id = f.tx.id();
        self.record_tx(step, action, id).await?;
        if let Some(x) = self.x.as_mut() {
            x.watch(id);
        }
        self.net.submit(f).await.map_err(|e| format!("{action}: REJECTED by {}: {e} (stop rule D104)", self.net.node()))?;
        println!("  {step} {action}: submitted {id} ({})", kas(f.fee()));
        Ok(id)
    }

    async fn send_wait(&mut self, step: &str, action: &str, f: &Final) -> R<TransactionId> {
        let id = self.send(step, action, f).await?;
        self.wait_txs(&[id]).await?;
        Ok(id)
    }

    /// An expected refusal: the local engine must reject it (at `at_input`, for a script
    /// refusal), then the node must reject it with the matching reason.
    async fn refuse(&mut self, step: &str, name: &str, f: &Final, kind: Refusal, at_input: Option<usize>) -> R<()> {
        let local = match kind {
            Refusal::Script => match f.run() {
                Ok(()) => return Err(format!("FINDING: {name}: the local engine ACCEPTS it; not submitted")),
                Err((i, e)) => {
                    if at_input.is_some_and(|w| w != i) {
                        return Err(format!("{name}: the local engine rejects input {i}, expected input {}: {e:?}", at_input.unwrap()));
                    }
                    format!("local engine rejects input {i}: {e}")
                }
            },
            Refusal::Spent => "built against the spent request's last state (valid there)".into(),
        };
        match self.net.submit(f).await {
            Ok(()) => {
                self.check(step, name, false, format!("the node ACCEPTED {} ({local})", f.tx.id())).await?;
                Err(format!("FINDING: {name}: the node accepted {}", f.tx.id()))
            }
            Err(text) => {
                let ok = kind.matches(&text);
                self.check(step, name, ok, format!("node {}: {text}; {local}", self.net.node())).await?;
                if ok { Ok(()) } else { Err(format!("{name}: refused for another reason: {text}")) }
            }
        }
    }

    // ---- funding

    async fn funds(&mut self) -> R<Vec<Funding>> {
        let x = self.x.as_ref().ok_or("no indexer")?;
        let mut v = self.net.funds(x).await?;
        v.sort_by_key(|f| std::cmp::Reverse(f.value));
        Ok(v)
    }

    async fn one_fund(&mut self, min: u64) -> R<Funding> {
        self.ensure_funding(1, min).await?;
        Ok(self.funds().await?.into_iter().next().expect("ensured"))
    }

    /// At least `n` of our UTXOs of `min` sompi or more; else a fan-out from the largest ones.
    async fn ensure_funding(&mut self, n: usize, min: u64) -> R<()> {
        let v = self.funds().await?;
        let have = v.iter().filter(|f| f.value >= min).count();
        if have >= n {
            return Ok(());
        }
        let k = n - have + 2;
        let each = (30 * KAS).max(min);
        // As few of the largest UTXOs as cover the outputs (the others stay usable).
        let need = k as u64 * each + KAS;
        let mut ins: Vec<Funding> = vec![];
        let mut total = 0u64;
        for f in v.iter().take(20) {
            if total >= need {
                break;
            }
            ins.push(*f);
            total += f.value;
        }
        if total < need {
            return Err(format!("insufficient funds: need {k} x {} for the next wave, have {} in our 20 largest UTXOs", kas(each), kas(total)));
        }
        let f = self.plain(&ins, &vec![each; k])?;
        let id = f.tx.id();
        println!("  fan-out {id}: {k} x {}", kas(each));
        if let Some(x) = self.x.as_mut() {
            x.watch(id);
        }
        self.net.submit(&f).await.map_err(|e| format!("fan-out REJECTED: {e}"))?;
        self.wait_txs(&[id]).await
    }

    /// A funding UTXO of `min` sompi created and chain-accepted just before use. close()'s
    /// next deadline is T' = max(T + round_len, own + d_min) with `own` = the newest input's
    /// creation DAA (`controller.sil:473-484`), not the close's own DAA: a close spending only
    /// old inputs, sent more than d_min after the newest, opens a round whose deadline has
    /// already passed, so no shard can sell (`shard.sil` check 6; testnet-10 round 3, S3b).
    async fn fresh_fund(&mut self, min: u64) -> R<Funding> {
        self.ensure_funding(1, min + KAS).await?;
        let src = self.funds().await?.into_iter().next().ok_or("fresh funding: no funds")?;
        let f = self.plain(&[src], &[min])?;
        let id = f.tx.id();
        if let Some(x) = self.x.as_mut() {
            x.watch(id);
        }
        self.net.submit(&f).await.map_err(|e| format!("fresh funding REJECTED: {e}"))?;
        println!("  fresh funding {id}: {}", kas(min));
        self.wait_txs(&[id]).await?;
        let op = TransactionOutpoint::new(id, 0);
        self.funds().await?.into_iter().find(|f| f.outpoint == op).ok_or_else(|| format!("fresh funding {op}: not among our UTXOs after acceptance"))
    }

    /// A P2PK -> P2PK transaction from `ins` to outputs of `outs` sompi plus change.
    fn plain(&self, ins: &[Funding], outs: &[u64]) -> R<Final> {
        let spk = spk_of(&self.sk);
        let entries: Vec<UtxoEntry> = ins.iter().map(|f| UtxoEntry::new(f.value, spk.clone(), f.daa, false, None)).collect();
        let total: u64 = ins.iter().map(|f| f.value).sum();
        fees::with_fee(self.rate, |fee| {
            let change = total.checked_sub(outs.iter().sum::<u64>() + fee).ok_or("plain: insufficient input")?;
            let mut o: Vec<TransactionOutput> = outs.iter().map(|v| TransactionOutput::new(*v, spk.clone())).collect();
            o.push(TransactionOutput::new(change, spk.clone()));
            let inputs = ins.iter().map(|f| TransactionInput::new_with_compute_budget(f.outpoint, vec![], 0, harness::budget_for(100_000))).collect();
            let mut tx = Transaction::new(1, inputs, o, 0, SubnetworkId::default(), 0, vec![]);
            harness::commit_storage_mass(&tx, &entries);
            for i in 0..ins.len() {
                tx.inputs[i].signature_script = sign_input(&PopulatedTransaction::new(&tx, entries.clone()), i, &self.sk, SIG_HASH_ALL);
            }
            tx.finalize();
            Ok(Final { tx, entries: entries.clone(), units: vec![], sim: None })
        })
    }

    async fn refresh_rate(&mut self) -> R<()> {
        let r = self.net.fee_rate().await?;
        self.rate = r.sompi_per_gram;
        Ok(())
    }

    fn payer(&self, fee: u64) -> Payer {
        Payer { funding_sk: self.sk, fee }
    }

    // ---- state from the index

    fn single(&self, role: Role) -> R<(Live, State)> {
        let (l, s) = self.ix()?.singleton(role)?;
        Ok((l.clone(), s.clone()))
    }

    fn controller(&self) -> R<(Live, lottery::state::ControllerState)> {
        match self.single(Role::Controller)? {
            (l, State::Controller(c)) => Ok((l, c)),
            _ => unreachable!(),
        }
    }

    fn lives(&self, role: Role) -> R<Vec<(Live, State)>> {
        let mut v: Vec<(Live, State)> = self.ix()?.live().values().filter(|l| l.role == role).filter_map(|l| l.state.clone().map(|s| (l.clone(), s))).collect();
        v.sort_by_key(|(l, _)| (l.daa, l.outpoint.transaction_id.as_bytes(), l.outpoint.index));
        Ok(v)
    }

    fn shards(&self, round: i64) -> R<Vec<(Live, ShardState)>> {
        let mut v: Vec<(Live, ShardState)> = self
            .lives(Role::Shard)?
            .into_iter()
            .filter_map(|(l, s)| match s {
                State::Shard(s) if s.round == round => Some((l, s)),
                _ => None,
            })
            .collect();
        v.sort_by_key(|(_, s)| s.shard_index);
        if v.len() != K || v.iter().enumerate().any(|(i, (_, s))| s.shard_index != i as i64) {
            return Err(format!("round {round}: {} live shards, expected {K}", v.len()));
        }
        Ok(v)
    }

    fn combos(&self, round: i64, n: u64, b: usize, m: i64) -> R<Vec<Combo>> {
        let cov = self.cov()?.as_bytes();
        Ok((0..b as u64)
            .map(|j| {
                let h = blake2b(&[cov.as_slice(), &round.to_le_bytes(), &n.to_le_bytes(), &j.to_le_bytes()].concat());
                let rank = u32::from_le_bytes(h[..4].try_into().unwrap()) % RANKS;
                Combo::new(unrank(rank), 1 + (h[4] as i64 % m) as u8)
            })
            .collect())
    }

    // ---- builders (every one from the index's live states, fee at the current rate)

    fn build_buy(&self, s: &BuySpec, fund: &Funding) -> R<Final> {
        let shards = self.shards(s.round)?;
        let (l, st) = shards[s.shard].clone();
        let tree = match self.wallets.get(&s.round) {
            Some(w) => w.shard_tree(s.shard)?,
            None => lottery::smt::Tree::new(st.m as usize),
        };
        if tree.root() != st.root {
            return Err(format!("{}: the wallet's shard {} tree root differs from the live shard's (a purchase it did not see)", s.action, s.shard));
        }
        let bundle = self.combos(s.round, s.n, s.b, st.m)?;
        let (_, ctrl) = self.controller()?;
        fees::with_fee(self.rate, |fee| {
            let mut c = BuyCase::new(b_max()).with_tree(tree.clone());
            c.shard = st.clone();
            c.reserve = ctrl.reserve as u64;
            c.shard_daa = l.daa;
            c.cov = self.cov()?;
            c.bundle = bundle.clone();
            c.with_ref = s.with_ref;
            c.referrer = self.me;
            c.payout = self.me;
            c.funding = vec![fund.value];
            c.funding_sk = self.sk;
            c.fee = fee;
            if c.shard_value() != l.value {
                return Err(format!("{}: shard value {} != live value {}", s.action, c.shard_value(), l.value));
            }
            let mut b = guarded("buy", || c.build_with(self.t.clone()))?;
            b.outpoints = vec![l.outpoint, fund.outpoint];
            guarded("buy finish", || b.finish())
        })
    }

    fn build_close(&self, fund: &Funding) -> R<(Final, CloseCase, Vec<TransactionOutpoint>)> {
        let (cl, ctrl) = self.controller()?;
        let (pl, ps) = match self.single(Role::Params)? {
            (l, State::Params(p)) => (l, p),
            _ => unreachable!(),
        };
        let shards = self.shards(ctrl.round)?;
        let mut keep = None;
        let f = fees::with_fee(self.rate, |fee| {
            let mut c = CloseCase::new_with(K, &self.t);
            c.controller = ctrl.clone();
            c.controller_daa = cl.daa;
            c.pot = cl.value.checked_sub(ctrl.acc as u64 + ctrl.reserve as u64).ok_or("controller value below acc + R")?;
            c.params = ps.clone();
            c.params_daa = pl.daa;
            c.shards = shards.iter().map(|(l, s)| ShardIn { state: s.clone(), daa: l.daa }).collect();
            c.funding = vec![FundingIn { value: fund.value, daa: fund.daa }];
            c.funder = self.me;
            c.funding_sk = self.sk;
            c.fee = fee;
            c.cov = self.cov()?;
            c.lock_time = ctrl.deadline as u64;
            let mut ops = vec![cl.outpoint, pl.outpoint];
            ops.extend(shards.iter().map(|(l, _)| l.outpoint));
            ops.push(fund.outpoint);
            let mut b = guarded("close", || c.build_with(self.t.clone()))?;
            b.outpoints = ops.clone();
            let f = guarded("close finish", || b.finish())?;
            keep = Some((c, ops));
            Ok(f)
        })?;
        let (c, ops) = keep.expect("built");
        Ok((f, c, ops))
    }

    fn build_draw(&self, proof: Option<AnchorProof>, sim: Option<Arc<ChainSim>>, fund: &Funding) -> R<(Final, DrawCase, Vec<TransactionOutpoint>)> {
        let mut keep = None;
        let f = fees::with_fee(self.rate, |fee| {
            let (mut c, ops) = DrawCase::from_store(self.ix()?, proof, &[*fund], self.me, &self.payer(fee))?;
            if sim.is_some() {
                c.sim = sim.clone();
            }
            let mut b = guarded("draw", || c.build_with(self.t.clone()))?;
            b.outpoints = ops.clone();
            let f = guarded("draw finish", || b.finish())?;
            keep = Some((c, ops));
            Ok(f)
        })?;
        let (c, ops) = keep.expect("built");
        Ok((f, c, ops))
    }

    fn build_claim<S: ClaimStore + ?Sized>(&self, store: &S, ticket: TransactionOutpoint, fund: &Funding, tamper: bool) -> R<(Final, ClaimCase)> {
        let mut keep = None;
        let f = fees::with_fee(self.rate, |fee| {
            let (mut c, ops) = ClaimCase::from_store(store, ticket, &[*fund], &self.payer(fee))?;
            if !c.roots_match() {
                return Err(format!("claim of {ticket}: the store's trees do not give the pool's frozen roots (no submission)"));
            }
            if tamper {
                // C4: one leaf count of the claimed combination changed by one (lowered when
                // the combination is held twice or more, else raised): an untrusted copy.
                let x = guarded("claim reference", || c.expected())?;
                let (rank, bonus) = (Combo::new(x.comb[..5].try_into().unwrap(), x.comb[5]).rank(), x.comb[5]);
                let s = c.trees.iter().position(|t| t.counts(rank)[bonus as usize - 1] > 0).ok_or("no shard holds the claimed combination")?;
                let mut counts = c.trees[s].counts(rank);
                if x.h >= 2 { counts[bonus as usize - 1] -= 1 } else { counts[bonus as usize - 1] += 1 }
                c.trees[s].set(rank, counts);
            }
            let mut b = guarded("claim", || c.build_with(self.t.clone()))?;
            b.outpoints = ops;
            let f = guarded("claim finish", || b.finish())?;
            keep = Some(c);
            Ok(f)
        })?;
        Ok((f, keep.expect("built")))
    }

    fn vault(&self) -> R<(Live, lottery::state::VaultState)> {
        match self.single(Role::Vault)? {
            (l, State::Vault(v)) => Ok((l, v)),
            _ => unreachable!(),
        }
    }

    fn build_deposit(&self, d: i64, fund: &Funding) -> R<(Final, DepositCase)> {
        let (vl, vs) = self.vault()?;
        let (_, ctrl) = self.controller()?;
        let mut keep = None;
        let f = fees::with_fee(self.rate, |fee| {
            let mut c = DepositCase::new_with(&self.t);
            c.vault = vs.clone();
            c.reserve = ctrl.reserve as u64;
            c.at_age(vl.daa);
            c.d = d;
            c.owner = self.me;
            c.funding = vec![FundingIn { value: fund.value, daa: fund.daa }];
            c.cov = self.cov()?;
            c.funding_sk = self.sk;
            c.fee = fee;
            if c.vault_value() != vl.value {
                return Err("deposit: vault value differs from the live one".into());
            }
            let mut b = guarded("deposit", || c.build_with(self.t.clone()))?;
            b.outpoints = vec![vl.outpoint, fund.outpoint];
            let f = guarded("deposit finish", || b.finish())?;
            keep = Some(c);
            Ok(f)
        })?;
        Ok((f, keep.expect("built")))
    }

    fn build_withdraw(&self, pos: &(Live, State), w: i64, fund: &Funding) -> R<(Final, WithdrawCase)> {
        let (vl, vs) = self.vault()?;
        let (_, ctrl) = self.controller()?;
        let State::Position(ps) = &pos.1 else { return Err("not a position".into()) };
        let mut keep = None;
        let f = fees::with_fee(self.rate, |fee| {
            let mut c = WithdrawCase::new_with(&self.t, OwnerKey::Schnorr(self.sk));
            c.vault = vs.clone();
            c.reserve = ctrl.reserve as u64;
            c.at_age(vl.daa);
            c.position = ps.clone();
            c.position_daa = pos.0.daa;
            c.w = w;
            c.owner_value = fund.value;
            c.owner_daa = fund.daa;
            c.funding = vec![];
            c.cov = self.cov()?;
            c.funding_sk = self.sk;
            c.fee = fee;
            let mut b = guarded("withdraw", || c.build_with(self.t.clone()))?;
            b.outpoints = vec![vl.outpoint, pos.0.outpoint, fund.outpoint];
            let f = guarded("withdraw finish", || b.finish())?;
            keep = Some(c);
            Ok(f)
        })?;
        Ok((f, keep.expect("built")))
    }

    /// A settlement of request `rq` (live, or its last state for the refused second
    /// settle) against the live record of its round (or the given one).
    fn build_settle(&self, rq: (TransactionOutpoint, lottery::state::RequestState, u64, u64), rc: Option<(TransactionOutpoint, lottery::state::RecordState, u64)>, fund: &Funding) -> R<(Final, SettleCase)> {
        let (_, ctrl) = self.controller()?;
        let (rop, rcs, rdaa) = match rc {
            Some(x) => x,
            None => {
                let v: Vec<(Live, State)> =
                    self.lives(Role::Record)?.into_iter().filter(|(_, s)| matches!(s, State::Record(r) if r.round == rq.1.round)).collect();
                match v.as_slice() {
                    [(l, State::Record(r))] => (l.outpoint, r.clone(), l.daa),
                    _ => return Err(format!("{} live records of round {}", v.len(), rq.1.round)),
                }
            }
        };
        let mut keep = None;
        let f = fees::with_fee(self.rate, |fee| {
            let mut c = SettleCase::new_with(&self.t, rcs.clone(), rq.1.clone());
            c.request_value = rq.2;
            c.request_daa = rq.3;
            c.record_daa = rdaa;
            c.record_reserve = ctrl.reserve as u64;
            c.funding = vec![FundingIn { value: fund.value, daa: fund.daa }];
            c.cov = self.cov()?;
            c.funding_sk = self.sk;
            c.fee = fee;
            let mut b = guarded("settle", || c.build_with(self.t.clone()))?;
            b.outpoints = vec![rq.0, rop, fund.outpoint];
            let f = guarded("settle finish", || b.finish())?;
            keep = Some(c);
            Ok(f)
        })?;
        Ok((f, keep.expect("built")))
    }

    fn build_set_params(&self, s_r: i64, fund: &Funding) -> R<Final> {
        let (pl, ps) = match self.single(Role::Params)? {
            (l, State::Params(p)) => (l, p),
            _ => unreachable!(),
        };
        fees::with_fee(self.rate, |fee| {
            let mut c = SetParamsCase::new();
            c.b_max = b_max();
            c.params = ps.clone();
            c.value = pl.value;
            c.params_daa = pl.daa;
            c.s_f = ps.next_s_f;
            c.s_r = s_r;
            c.set_fee(&self.me);
            c.nonce = ps.nonce + 1;
            c.admin_sk = self.sk;
            c.signer_sk = self.sk;
            c.funding = vec![fund.value];
            c.cov = self.cov()?;
            c.funding_sk = self.sk;
            c.fee = fee;
            let mut b = guarded("set_params", || c.build_with(self.t.clone()))?;
            b.outpoints = vec![pl.outpoint, fund.outpoint];
            guarded("set_params finish", || b.finish())
        })
    }

    fn build_refresh(&self, obj: &(Live, State), fund: &Funding) -> R<Final> {
        let o = refresh_obj(&obj.1).ok_or("not a refreshable object")?;
        fees::with_fee(self.rate, |fee| {
            let mut c = RefreshCase::new(o.clone(), obj.0.value, obj.0.daa);
            c.b_max = b_max();
            c.cov = self.cov()?;
            c.funding = vec![FundingIn { value: fund.value, daa: fund.daa }];
            c.funding_sk = self.sk;
            c.fee = fee;
            c.lock_time = obj.0.daa + D_IDLE as u64;
            let mut b = guarded("refresh", || c.build_with(self.t.clone()))?;
            b.outpoints = vec![obj.0.outpoint, fund.outpoint];
            guarded("refresh finish", || b.finish())
        })
    }

    // ---- composite actions

    /// Buys in waves (one per shard per wave, each with its own funding UTXO), each wave
    /// submitted together and waited for.
    async fn buys(&mut self, step: &str, specs: Vec<BuySpec>) -> R<()> {
        let todo: Vec<BuySpec> = specs.into_iter().filter(|s| !self.done(&s.action)).collect();
        let mut waves: Vec<Vec<BuySpec>> = vec![];
        for s in todo {
            match waves.iter_mut().find(|w| w.iter().all(|x| x.shard != s.shard) && w.len() < K) {
                Some(w) => w.push(s),
                None => waves.push(vec![s]),
            }
        }
        for w in waves {
            let (_, ctrl) = self.controller()?;
            let now = self.net.daa().await?;
            if now + 300 > ctrl.deadline as u64 && w.iter().any(|s| s.round == ctrl.round && !s.action.contains("post-deadline")) {
                return Err(format!("{step}: round {}'s deadline {} is too close (virtual DAA {now}) for {} more buys", ctrl.round, ctrl.deadline, w.len()));
            }
            self.refresh_rate().await?;
            self.ensure_funding(w.len(), 8 * KAS).await?;
            let funds = self.funds().await?;
            let mut ids = vec![];
            for (s, f) in w.iter().zip(funds.iter()) {
                let fin = self.build_buy(s, f)?;
                ids.push(self.send(step, &s.action, &fin).await?);
            }
            self.wait_txs(&ids).await?;
        }
        Ok(())
    }

    fn round_specs(&self, prefix: &str, round: i64, count: usize, b: usize, ref_first: bool) -> Vec<BuySpec> {
        (0..count)
            .map(|n| BuySpec { action: format!("{prefix}/buy/{n}"), round, shard: n % K, n: n as u64, b, with_ref: ref_first && n == 0 })
            .collect()
    }

    /// The referral output of a done buy: (value, paid to us).
    fn referral_of(&self, action: &str) -> Option<(u64, bool)> {
        let a = self.done_tx(action)?;
        let o = a.tx.outputs.get(2).filter(|o| o.covenant.is_none() && a.tx.outputs.len() == 4)?;
        Some((o.value, o.script_public_key == self.me.spk()))
    }

    async fn wait_deadline(&mut self) -> R<()> {
        let (_, ctrl) = self.controller()?;
        self.net.wait_daa(ctrl.deadline as u64 + SETTLE_DAA, &format!("round {} deadline T", ctrl.round)).await?;
        self.sync().await
    }

    /// close() of the open round (after its deadline); checks m' and the wallet's roots.
    async fn close(&mut self, step: &str, action: &str) -> R<()> {
        let (_, ctrl) = self.controller()?;
        let r = ctrl.round;
        if !self.done(action) {
            self.wait_deadline().await?;
            self.refresh_rate().await?;
            // Fresh input (S3b fix): T' takes `own` from the newest input, so the close
            // spends a funding UTXO accepted just before it.
            let fund = self.fresh_fund(10 * KAS).await?;
            let (s, _) = self.net.sink().await?;
            self.set_var(&format!("close_start.r{r}"), hex(&s.as_bytes()));
            let (f, c, ops) = self.build_close(&fund)?;
            // The library's stale-input guard (M8 S5, `lottery::close::refuse_stale_inputs`;
            // D24, controller.sil:473-484), at the virtual DAA just before submission.
            let now = self.net.daa().await?;
            c.check_fresh(now).map_err(|e| format!("{action}: not submitted: round {}: {e}", r + 1))?;
            self.net.verify_inputs(&f, &ops).await?;
            self.send_wait(step, action, &f).await?;
        }
        let (_, c2) = self.controller()?;
        let shards = self.shards(r + 1)?;
        let m_ok = shards.iter().all(|(_, s)| s.m == c2.m && s.s_r == c2.s_r);
        self.check(step, &format!("{action}: round {} m' = {} and s_R = {} in its controller and every shard", r + 1, c2.m, c2.s_r), m_ok, format!("controller m {} s_R {}", c2.m, c2.s_r)).await?;
        let close = self.ix()?.round(r).and_then(|x| x.close.clone()).ok_or("close not indexed")?;
        let w_ok = self.wallets.get(&r).map(|w| w.frozen_roots == close.frozen_roots && w.trees(r).is_ok()).unwrap_or(false);
        let i_ok = self.ix()?.trees(r).map(|t| t.iter().map(|t| t.root()).collect::<Vec<_>>() == close.frozen_roots).unwrap_or(false);
        self.check(step, &format!("{action}: wallet and indexer roots = the frozen roots of round {r}"), w_ok && i_ok, format!("wallet {w_ok}, indexer {i_ok}")).await
    }

    /// draw() of the closed round, from the index (offline builder), `late` DAA after D.
    async fn draw(&mut self, step: &str, action: &str, late: u64) -> R<lottery::draw::DrawExpected> {
        let (ml, ms) = self.single(Role::Marker)?;
        let State::Marker(mk) = ms else { unreachable!() };
        let (_, ctrl) = self.controller()?;
        let d = anchor_daa(ml.daa as i64, ctrl.seed_offset) as u64;
        let r = mk.round;
        if self.done(action) {
            return Err(format!("{action} is done; its expectation is only computed before submission"));
        }
        if late > 0 {
            self.net.wait_daa(d + late, &format!("round {r}'s draw held back to D + {late}")).await?;
        }
        let start = self.var(&format!("close_start.r{r}")).ok().and_then(|h| store::from_hex(&h).ok()).and_then(|b| <[u8; 32]>::try_from(b).ok()).map(Hash::from_bytes);
        let start = match start {
            Some(s) => s,
            None => self.net.sink().await?.0,
        };
        let (proof, sim) = self.net.anchor(start, d).await?;
        self.net.wait_daa(d + SETTLE_DAA, "D").await?;
        self.sync().await?;
        self.refresh_rate().await?;
        let fund = self.one_fund(10 * KAS).await?;
        let (f, c, ops) = self.build_draw(Some(proof), sim, &fund)?;
        let x = guarded("draw reference", || c.expected())?;
        self.net.verify_inputs(&f, &ops).await?;
        self.send_wait(step, action, &f).await?;
        // D124: the round result rebuilt from the transaction bytes equals the model.
        let rr = RoundResult::from_tx(&f.tx, &self.t)?;
        let rec_ok = match (&rr.record, &x.record) {
            (Some(a), Some(b)) => &a.state == b,
            (None, None) => true,
            _ => false,
        };
        let conv = lottery::game::convert(x.conv.v_prime, x.conv.s, x.conv.d, x.conv.w, x.conv.l_next);
        self.check(
            step,
            &format!("{action}: conversion and record = game::convert on the decoded state (D124)"),
            rec_ok && conv == Some(x.conv),
            format!("rec {}; D {} mint_c {} refund {}; W {} wc {} out_c {}; L' {}", x.rec, x.conv.d, x.conv.mint_c, x.conv.refund, x.conv.w, x.conv.wc, x.conv.out_c, x.conv.l_next),
        )
        .await?;
        let fee_ok = match x.fee_output() {
            Some(i) => f.tx.outputs[i].script_public_key == Payout::from_fields(c.controller.fee_class, &c.controller.fee_key).spk(),
            None => true,
        };
        self.check(step, &format!("{action}: fee paid to round {}'s fee address", r + 1), fee_ok, format!("fee output {:?}, paid {}", x.fee_output(), x.fee.paid)).await?;
        println!("  round {r}: win {:?}, pools {:?}, void {}", rr.win, rr.pools.iter().map(|p| (p.tier, p.value)).collect::<Vec<_>>(), rr.void);
        Ok(x)
    }

    /// Claims of every winning holding of round `r` (wallet, index or both stores); returns
    /// how many were accepted. `tamper_once`: first build the C4 untrusted-copy refusal.
    /// `ClaimFrom::Both`: each claim is built from both stores, both pass `roots_match`, the
    /// two must be identical except the funding signatures (a check, recorded with both
    /// txids before submission), and the indexer-built one is submitted.
    async fn claims(&mut self, step: &str, prefix: &str, r: i64, from: ClaimFrom, tamper_once: bool, max: usize) -> R<usize> {
        let from_index = from != ClaimFrom::Wallet;
        let mut n = 0;
        let tickets: Vec<TransactionOutpoint> = self.wallets.get(&r).map(|w| w.tickets.iter().map(|t| t.outpoint.outpoint()).collect()).unwrap_or_default();
        for mut op in tickets {
            for idx in 0..8 {
                if n >= max {
                    return Ok(n);
                }
                let action = format!("{prefix}/claim/{}:{}/{idx}", op.transaction_id, op.index);
                if self.done(&action) {
                    // Follow the ticket to its successor (output 1) if it continues.
                    if let Some(a) = self.done_tx(&action) {
                        op = TransactionOutpoint::new(a.tx.id(), 1);
                    }
                    n += 1;
                    continue;
                }
                self.refresh_rate().await?;
                let fund = self.one_fund(10 * KAS).await?;
                let w = self.wallets.get(&r).cloned();
                let mut dual = None;
                let built = if from == ClaimFrom::Both {
                    let ix = &self.x()?.ix;
                    if ix.live().get(&op).is_none() { break }
                    let Some(w) = w.as_ref() else { break };
                    if w.tickets.iter().all(|t| t.outpoint.outpoint() != op) { break }
                    match (self.build_claim(w, op, &fund, false), self.build_claim(ix, op, &fund, false)) {
                        (Ok((fw, _)), Ok((fi, ci))) => {
                            dual = Some((fw.tx.id(), fi.tx.id(), same_except_funding_sigs(&fw.tx, &fi.tx, CLAIM_FUNDING_FROM)));
                            Ok((fi, ci))
                        }
                        (Err(a), Err(b)) if a.contains("no unclaimed index wins") && b.contains("no unclaimed index wins") => break,
                        (Err(e), _) => return Err(format!("dual claim, wallet store: {e}")),
                        (_, Err(e)) => return Err(format!("dual claim, indexer store: {e}")),
                    }
                } else if from_index {
                    let ix = &self.x()?.ix;
                    if ix.live().get(&op).is_none() { break }
                    self.build_claim(ix, op, &fund, false)
                } else {
                    let Some(w) = w.as_ref() else { break };
                    if w.tickets.iter().all(|t| t.outpoint.outpoint() != op) { break }
                    self.build_claim(w, op, &fund, false)
                };
                let (f, c) = match built {
                    Ok(x) => x,
                    Err(e) if e.contains("no unclaimed index wins") => break,
                    Err(e) => return Err(e),
                };
                let name = format!("{step}: a claim with one leaf count changed by 1 refused (untrusted copy)");
                if tamper_once && !check_done(&self.c, &name) {
                    let (tf, _) = match &w {
                        Some(w) if !from_index => self.build_claim(w, op, &fund, true)?,
                        _ => self.build_claim(&self.x()?.ix, op, &fund, true)?,
                    };
                    self.refuse(step, &name, &tf, Refusal::Script, Some(0)).await?;
                }
                let x = guarded("claim reference", || c.checked_expected())??;
                if let Some((idw, idi, cmp)) = dual {
                    let name = format!("{step}: wallet- and indexer-built claims identical except funding signatures ({action})");
                    let ok = cmp.is_ok();
                    let detail = format!(
                        "wallet-built txid {idw}, indexer-built txid {idi} (the txid excludes signature scripts); both stores' trees give the pool's frozen roots (roots_match); {}; {}",
                        match &cmp {
                            Ok(d) => d.clone(),
                            Err(d) => format!("DIFFER at {d}"),
                        },
                        if ok { "submitting the indexer-built one" } else { "nothing submitted" }
                    );
                    self.check(step, &name, ok, detail).await?;
                    if let Err(d) = cmp {
                        return Err(format!("{action}: the wallet- and indexer-built claims differ at {d} (no submission)"));
                    }
                }
                let id = self.send_wait(step, &action, &f).await?;
                let mode = format!("{:?}", x.mode);
                println!("  claim {id}: tier {:?}, h {}, prize {}, mode {mode}", x.tier, x.h, kas(x.prize as u64));
                self.modes.push(mode.clone());
                n += 1;
                for (nm, m) in [("a partial claim (one index of a bundle; PAY or ACCRUE)", ["Pay", "Accrue"].contains(&mode.as_str())), ("a last claim (FINAL)", mode == "Final")] {
                    let cn = format!("claims: {nm}");
                    if m && !check_done(&self.c, &cn) {
                        self.check(step, &cn, true, format!("{id} (round {r}, mode {mode})")).await?;
                    }
                }
                if mode == "Final" {
                    break;
                }
                op = TransactionOutpoint::new(id, 1);
            }
        }
        Ok(n)
    }

    async fn deposit(&mut self, step: &str, action: &str, d: i64, refused: bool) -> R<()> {
        if (refused && check_done(&self.c, action)) || (!refused && self.done(action)) {
            return Ok(());
        }
        let (vl, _) = self.vault()?;
        self.net.wait_daa(vl.daa + VAULT_REQ_DELAY as u64 + SETTLE_DAA, "vault age (VAULT_REQ_DELAY)").await?;
        self.refresh_rate().await?;
        let fund = self.one_fund((d as u64) + 5 * KAS).await?;
        let (f, _) = self.build_deposit(d, &fund)?;
        if refused { self.refuse(step, action, &f, Refusal::Script, Some(0)).await } else { self.send_wait(step, action, &f).await.map(|_| ()) }
    }

    fn positions(&self) -> R<Vec<(Live, State)>> {
        Ok(self.lives(Role::Position)?.into_iter().filter(|(_, s)| matches!(s, State::Position(p) if p.owner_key == self.me.key33())).collect())
    }

    async fn withdraw(&mut self, step: &str, action: &str, genesis: bool, w_of: impl Fn(i64, i64) -> i64, refused: bool) -> R<()> {
        if (refused && check_done(&self.c, action)) || (!refused && self.done(action)) {
            return Ok(());
        }
        let pos = self
            .positions()?
            .into_iter()
            .find(|(_, s)| matches!(s, State::Position(p) if (p.round == 0) == genesis))
            .ok_or_else(|| format!("{action}: no live {} position", if genesis { "genesis" } else { "depositor" }))?;
        let (vl, vs) = self.vault()?;
        self.net.wait_daa(vl.daa + VAULT_REQ_DELAY as u64 + SETTLE_DAA, "vault age (VAULT_REQ_DELAY)").await?;
        self.refresh_rate().await?;
        let State::Position(p) = &pos.1 else { unreachable!() };
        let w = w_of(p.shares, w_min(vs.total_assets, vs.total_shares));
        let fund = self.one_fund(5 * KAS).await?;
        let (f, c) = self.build_withdraw(&pos, w, &fund)?;
        println!("  {action}: w = {w} of {} shares (wmin {})", p.shares, c.w_min());
        if refused { self.refuse(step, action, &f, Refusal::Script, Some(0)).await } else { self.send_wait(step, action, &f).await.map(|_| ()) }
    }

    /// Settle our live requests of `round` (oldest first); checks the mode and the model.
    /// Settle our live requests of kind `kind` targeting the drawn round `round` (oldest
    /// first; one action per request outpoint, so a resume finds them by name).
    async fn settle_all(&mut self, step: &str, prefix: &str, kind: u8, round: i64, want: &[SettleMode]) -> R<Vec<SettleMode>> {
        let rqs: Vec<(Live, State)> = self
            .lives(Role::Request)?
            .into_iter()
            .filter(|(_, s)| matches!(s, State::Request(q) if q.kind == kind && q.round == round && q.owner_key == self.me.key33()))
            .collect();
        let mut ids = vec![];
        for (l, s) in rqs {
            let State::Request(q) = s else { unreachable!() };
            let action = format!("{prefix}/settle/{}:{}", l.outpoint.transaction_id, l.outpoint.index);
            if self.done(&action) {
                continue;
            }
            self.refresh_rate().await?;
            let fund = self.one_fund(5 * KAS).await?;
            let (f, c) = self.build_settle((l.outpoint, q.clone(), l.value, l.daa), None, &fund)?;
            let x = guarded("settle reference", || c.expected())?;
            let reference = lottery::game::settle(&c.record, &c.request);
            let id = self.send_wait(step, &action, &f).await?;
            let values: Vec<u64> = f.tx.outputs.iter().map(|o| o.value).collect();
            let ok = x.reference.is_some() && x.reference == reference && values == x.values && want.contains(&x.mode);
            self.check(
                step,
                &format!("{action}: mode {:?} (want {want:?}), values = game::settle (D124)", x.mode),
                ok,
                format!("{id}: request round {} amount {}; sh {} conv {} pay {} left {}; carried request {:?}", q.round, q.amount, x.sh, x.conv, x.pay, x.left, x.request.as_ref().map(|r| (r.round, r.amount))),
            )
            .await?;
            ids.push(x.mode);
        }
        Ok(ids)
    }

    fn open_round(&self) -> R<i64> {
        Ok(self.controller()?.1.round)
    }

    // ---------------------------------------------------------------------------- steps

    /// C0: balance and fee estimate (the indexer starts in C1 with the covenant ID).
    pub async fn c0(&mut self) -> R<()> {
        let rate = self.net.fee_rate().await?;
        self.rate = rate.sompi_per_gram;
        let (s, sd) = self.net.sink().await?;
        self.set_var("start_hash", hex(&s.as_bytes()));
        self.set_var("start_daa", sd);
        let x0 = Indexer::open(None, self.t.clone(), Hash::from_bytes([0; 32]), None, s, sd)?;
        let bal: u64 = self.net.funds(&x0).await?.iter().map(|f| f.value).sum();
        self.c.vars.insert("balance_c0".into(), bal.to_string());
        let mut c0 = StepJson { id: "C0".into(), txids: vec![], checks: vec![], daa: sd, wall_time: self.net.now().await? };
        c0.checks.push(CheckJson { name: "balance >= budget".into(), ok: bal >= self.opts.budget, detail: format!("{} >= {}", kas(bal), kas(self.opts.budget)), status: None });
        c0.checks.push(CheckJson { name: "fee rate >= 100 sompi/gram".into(), ok: rate.sompi_per_gram >= 100, detail: format!("normal bucket {} -> {} sompi/gram", rate.raw, rate.sompi_per_gram), status: None });
        for k in &c0.checks {
            println!("  [{}] C0 {}: {}", if k.ok { "ok" } else { "FAIL" }, k.name, k.detail);
        }
        let ok = c0.checks.iter().all(|k| k.ok);
        self.c.steps.push(c0);
        if !ok {
            return Err("C0 checks failed".into());
        }
        Ok(())
    }

    /// C1: genesis (A2) and the launch bundle.
    pub async fn c1(&mut self) -> R<()> {
        if self.c.cov.is_none() {
            let fund = self.funds_raw().await?.into_iter().next().ok_or("no funds")?;
            let daa = self.net.daa().await?;
            let knobs = self.opts.genesis.clone();
            let t1 = daa as i64 + knobs.first_sales;
            let spk = spk_of(&self.sk);
            let mut g = GenesisCase::new_with(K, vec![(fund.outpoint, UtxoEntry::new(fund.value, spk, fund.daa, false, None))], &self.t);
            g.funding_sk = self.sk;
            g.set_owner(&self.me);
            knobs.apply(&mut g, t1, &self.me);
            let admin = self.admin_sk.unwrap_or(self.sk);
            admin_guard(&admin)?;
            g.params.admin_hash = admin_hash(&admin);
            let f = fees::with_fee(self.rate, |fee| {
                g.fee = fee;
                g.validate()?;
                Ok(g.build_with(self.t.clone()).finish())
            })?;
            let cov = lottery::genesis::covenant_id(&f);
            let b = lottery::bundle::Bundle::new(&g, &f, lottery::bundle::genesis_live(&g, &f, 0), None)?;
            let v = lottery::bundle::verify(&b)?;
            self.c.cov = Some(cov.as_bytes());
            self.c.genesis = (0..(K + 5) as u32).map(|i| TransactionOutpoint::new(f.tx.id(), i).into()).collect();
            let dir = self.dir()?;
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            indexer::write_atomic(&dir.join("bundle-genesis.json"), &store::write(&b, store::BUNDLE)?)?;
            let start = Hash::from_bytes(store::from_hex(&self.var("start_hash")?)?.try_into().map_err(|_| "start")?);
            let sd: u64 = self.var("start_daa")?.parse().map_err(|e| format!("{e}"))?;
            self.x = Some(Indexer::open(Some(&dir), v.templates.clone(), cov, Some(v.genesis_states.clone()), start, sd)?);
            self.check("C1", "covenant ID recomputed (bundle::verify before submission)", v.cov == cov, format!("{cov}")).await?;
            let bundle_admin = v.genesis_states.iter().find_map(|s| if let State::Params(p) = s { Some(p.admin_hash) } else { None });
            self.check(
                "C1",
                "the bundle's admin hash is the configured admin key's, not the public default key's",
                bundle_admin == Some(admin_hash(&admin)) && bundle_admin != Some(admin_hash(&ADMIN_SK)),
                format!("admin hash {}", bundle_admin.map(|h| hex(&h)).unwrap_or_default()),
            )
            .await?;
            let locked: u64 = f.tx.outputs.iter().filter(|o| o.covenant.is_some()).map(|o| o.value).sum();
            let ctl = &g.controller;
            self.set_var("genesis_locked", locked);
            self.set_var("genesis_fee", g.fee);
            self.check(
                "C1",
                &format!("genesis knobs ({})", knobs.profile),
                true,
                format!(
                    "T(1) = {t1}; round_len {} / d_min {} DAA; m {} in [{}, {}]; J(1) {} G2(1) {} L(1) {}; V0 {}; price {} deposit {}; s_C {} s_D {} s_R {} s_F {}; seed_offset {}; draw_window {}; claim_window {}; margin {}; reserve {}; locked in genesis outputs {}; fee {}",
                    ctl.round_len, ctl.d_min, ctl.m, ctl.m_min, ctl.m_max, kas(ctl.jackpot as u64), kas(ctl.g2 as u64), kas(ctl.liability as u64), kas(ctl.v_rec as u64),
                    kas(ctl.price as u64), kas(ctl.d as u64), ctl.s_c, ctl.s_d, ctl.s_r, ctl.s_f, ctl.seed_offset, g.drawer.draw_window, g.drawer.claim_window, ctl.margin, kas(ctl.reserve as u64), kas(locked), kas(g.fee)
                ),
            )
            .await?;
            self.send_wait("C1", "C1/genesis", &f).await?;
        }
        let cov = self.cov()?;
        let gid = Hash::from_bytes(self.c.genesis[0].txid);
        self.sync().await?;
        let (blk, daa) = self.x()?.accepted_at(&gid).ok_or_else(|| {
            format!("genesis {gid} is not chain-accepted; if it was never sent (no node accepted it), remove tn10-data/current and the instance directory and start again")
        })?;
        // The bundle with the genesis outputs' creation DAA and the snapshot.
        let dir = self.dir()?;
        let mut b: lottery::bundle::Bundle = store::read(&std::fs::read_to_string(dir.join("bundle-genesis.json")).map_err(|e| e.to_string())?, store::BUNDLE)?;
        for l in &mut b.live_utxos {
            l.daa = daa;
        }
        b.snapshot = Some(lottery::bundle::Snapshot { daa, chain_block: blk.as_bytes() });
        let v = lottery::bundle::verify(&b)?;
        indexer::write_atomic(&dir.join("bundle.json"), &store::write(&b, store::BUNDLE)?)?;
        let mismatches = crate::bundle::compare_live(&mut self.net, &b).await?;
        self.check("C1", "bundle-verify passes (local verify and the node's live UTXOs)", v.cov == cov && mismatches.is_empty(), format!("{} live UTXOs; mismatches {mismatches:?}", b.live_utxos.len())).await
    }

    async fn funds_raw(&mut self) -> R<Vec<Funding>> {
        let x0 = Indexer::open(None, self.t.clone(), Hash::from_bytes([0; 32]), None, Hash::from_bytes([0; 32]), 0)?;
        let mut v = self.net.funds(self.x.as_ref().unwrap_or(&x0)).await?;
        v.sort_by_key(|f| std::cmp::Reverse(f.value));
        Ok(v)
    }

    async fn balance_check(&mut self, step: &str, what: &str) -> R<()> {
        let bal: u64 = self.funds().await?.iter().map(|f| f.value).sum();
        let need = 250 * KAS;
        self.check(step, &format!("balance before {what}"), bal >= need, format!("{} (need {} per round)", kas(bal), kas(need))).await?;
        if bal < need { Err(format!("balance {} below {} before {what}", kas(bal), kas(need))) } else { Ok(()) }
    }

    /// C2: round 1 (m = 3).
    pub async fn c2(&mut self) -> R<()> {
        let s = "C2";
        self.sync().await?;
        let r = self.open_round()?;
        self.balance_check(s, &format!("round {r}")).await?;
        let specs = self.round_specs("C2", r, self.opts.buys, 4, true);
        self.buys(s, specs).await?;
        if let Some((v, ours)) = self.referral_of("C2/buy/0") {
            self.check(s, "referral paid at s_R 1,000 (buy 0, b = 4)", v == 40_000_000 && ours, kas(v)).await?;
        }
        self.deposit(s, "C2: request_deposit 9.99 KAS refused (MIN_DEPOSIT)", 999_000_000, true).await?;
        self.deposit(s, "C2/deposit 20 KAS", 20 * KAS as i64, false).await?;
        if !self.done("C2/set_params s_R 500") {
            self.refresh_rate().await?;
            let fund = self.one_fund(5 * KAS).await?;
            let f = self.build_set_params(500, &fund)?;
            self.send_wait(s, "C2/set_params s_R 500", &f).await?;
        }
        let name = "C2: a second set_params in round 1 refused (used = 1)";
        if !check_done(&self.c, name) {
            let fund = self.one_fund(5 * KAS).await?;
            let f = self.build_set_params(400, &fund)?;
            self.refuse(s, name, &f, Refusal::Script, Some(0)).await?;
        }
        let n = self.opts.buys;
        let spec = BuySpec { action: "C2/buy after set_params".into(), round: r, shard: n % K, n: n as u64, b: 4, with_ref: true };
        self.buys(s, vec![spec]).await?;
        let (v, ours) = self.referral_of("C2/buy after set_params").ok_or("no referral output")?;
        self.check(s, "referral after set_params still at s_R 1,000 (not live in round 1)", v == 40_000_000 && ours, kas(v)).await
    }

    /// C3: close and draw round 1; settle the deposit twice.
    pub async fn c3(&mut self) -> R<()> {
        let s = "C3";
        self.sync().await?;
        let r = if let Ok(v) = self.var("c3_round") { v.parse().unwrap_or(1) } else { self.open_round()? };
        self.set_var("c3_round", r);
        let close = format!("C3/close r{r}");
        self.close(s, &close).await?;
        let draw = format!("C3/draw r{r}");
        if !self.done(&draw) {
            self.draw(s, &draw, 0).await?;
        }
        // Settle the deposit, then the same request again.
        self.settle_all(s, "C3/deposit", lottery::state::KIND_DEPOSIT, r, &[SettleMode::DepositConvert, SettleMode::DepositRefund]).await?;
        let first = self.c.steps.iter().flat_map(|x| &x.txids).find(|t| t.action.starts_with("C3/deposit/settle/")).map(|t| t.action.clone());
        if let Some(first) = first {
            let name = "C3: a second settle() of the spent request refused";
            if !check_done(&self.c, name) {
                let a = self.done_tx(&first).ok_or("first settle not accepted")?;
                let ev = parse_settle(&a.tx, &self.t)?;
                let rq = (a.tx.inputs[0].previous_outpoint, ev.request, a.spent[0].amount, a.spent[0].daa);
                let rc = (a.tx.inputs[1].previous_outpoint, ev.record, a.spent[1].daa);
                let fund = self.one_fund(5 * KAS).await?;
                let (f, _) = self.build_settle(rq, Some(rc), &fund)?;
                self.refuse(s, name, &f, Refusal::Spent, None).await?;
            }
        }
        Ok(())
    }

    /// C4: claims of round 1 from the wallet (luck: up to two extra rounds).
    pub async fn c4(&mut self) -> R<()> {
        let s = "C4";
        let mut r: i64 = self.var("c3_round")?.parse().map_err(|e| format!("{e}"))?;
        for retry in 0..=2u32 {
            if retry > 0 {
                // An extra round of buys, closed and drawn (A5 luck rule).
                let open = self.open_round()?;
                self.balance_check(s, &format!("retry round {open}")).await?;
                let specs = self.round_specs(&format!("C4/retry{retry}"), open, self.opts.buys, 4, false);
                self.buys(s, specs).await?;
                self.close(s, &format!("C4/retry{retry}/close r{open}")).await?;
                let draw = format!("C4/retry{retry}/draw r{open}");
                if !self.done(&draw) {
                    self.draw(s, &draw, 0).await?;
                }
                r = open;
            }
            let n = self.claims(s, &format!("C4/r{r}"), r, ClaimFrom::Wallet, true, usize::MAX).await?;
            if n > 0 {
                return self.check(s, &format!("claims of every winning holding of round {r} from the wallet store, offline"), true, format!("{n} claims")).await;
            }
            println!("  C4: no winning holding in round {r} (retry {retry})");
        }
        self.record(s, luck_check("claims from the wallet store", 0, String::new(), "not exercised: no winning holding after two extra rounds (A5 luck rule)".into())).await
    }

    /// C5: round 2 (m = 2) with the referral threshold, the withdrawals and the close.
    pub async fn c5(&mut self) -> R<()> {
        let s = "C5";
        self.sync().await?;
        let r = self.open_round()?;
        self.balance_check(s, &format!("round {r}")).await?;
        let specs = self.round_specs("C5", r, self.opts.buys, 4, false);
        self.buys(s, specs).await?;
        let n = self.opts.buys as u64;
        let name = "C5: b = 1 with a 0.05 KAS referral refused (below M_REF)";
        if !check_done(&self.c, name) {
            self.ensure_funding(1, 8 * KAS).await?;
            let fund = self.funds().await?[0];
            let f = self.build_buy(&BuySpec { action: name.into(), round: r, shard: 0, n, b: 1, with_ref: true }, &fund)?;
            self.refuse(s, name, &f, Refusal::Script, Some(0)).await?;
        }
        self.buys(
            s,
            vec![
                BuySpec { action: "C5/b=1 without referral".into(), round: r, shard: 1, n: n + 1, b: 1, with_ref: false },
                BuySpec { action: "C5/b=2 with 0.1 KAS referral".into(), round: r, shard: 2, n: n + 2, b: 2, with_ref: true },
            ],
        )
        .await?;
        let (v, ours) = self.referral_of("C5/b=2 with 0.1 KAS referral").ok_or("no referral output")?;
        self.check(s, "b = 2 referral paid at s_R 500 (0.1 KAS = M_REF)", v == 10_000_000 && ours, kas(v)).await?;
        self.withdraw(s, "C5: depositor's partial withdrawal leaving less than wmin refused", false, |sh, wm| (sh - wm + 1).max(1), true).await?;
        self.withdraw(s, "C5/depositor whole withdrawal", false, |sh, _| sh, false).await?;
        self.withdraw(s, "C5/genesis backer full withdrawal", true, |sh, _| sh, false).await?;
        self.close(s, &format!("C5/close r{r}")).await?;
        self.set_var("c5_round", r);
        Ok(())
    }

    /// C5b (A5 amendment, owner 2026-10-17: the sales-pause test shifted by one round).
    /// Round 3 opened past its deadline on testnet-10 (a late close with stale inputs), so:
    /// draw round 2 now (late) with the liability-lock check, settle both withdrawals (PAY,
    /// carried), close round 3 with no sales (fresh input), then round 4 sells while draw 3
    /// is held back: 20 buys before its deadline, one per shard after it where allowed, a
    /// further buy refused (sales pause), close() of round 4 before draw 3 refused.
    pub async fn c5b(&mut self) -> R<()> {
        let s = "C5b";
        self.sync().await?;
        let r2: i64 = self.var("c5_round")?.parse().map_err(|e| format!("{e}"))?;
        let draw = format!("C5b/draw r{r2} (late)");
        if !self.done(&draw) {
            let x = self.draw(s, &draw, 0).await?;
            let c = x.conv;
            let total = c.v_prime + c.d_c - c.out_c;
            let ok = c.wc < c.w && total >= c.l_next && wc_witness_ok(c.v_prime, c.s, c.w, c.slack, c.wc);
            self.check(s, "draw 2 converts only wc < W shares and leaves exactly L' (liability lock, D124)", ok, format!("W {} wc {}; vault after {} vs L' {} (slack left {})", c.w, c.wc, total, c.l_next, total - c.l_next)).await?;
        }
        self.settle_all(s, "C5b/withdrawals", lottery::state::KIND_WITHDRAW, r2, &[SettleMode::Pay]).await?;
        // Round 3: no sales (on testnet-10 its deadline had passed before it opened).
        let r3 = match self.var("c5b_empty_round") {
            Ok(v) => v.parse().map_err(|e| format!("{e}"))?,
            Err(_) => {
                let r = self.open_round()?;
                self.set_var("c5b_empty_round", r);
                r
            }
        };
        if self.open_round()? == r3 {
            let name = format!("round {r3} closed with no sales (A5 amendment: one-round shift)");
            if !check_done(&self.c, &name) {
                let (_, ctrl) = self.controller()?;
                let created = self.shards(r3)?.iter().map(|(l, _)| l.daa).min().unwrap_or(0);
                let dead = created >= ctrl.deadline as u64;
                self.check(s, &name, true, format!("deadline {}, shards created at DAA {created}: {}", ctrl.deadline, if dead { "opened past its deadline (stale close inputs, controller.sil:473-484), no buy possible" } else { "sales were possible; none made by plan" })).await?;
            }
            self.close(s, &format!("C5b/close r{r3} (no sales)")).await?;
        }
        // Round 4: the sales pause while draw 3 is held back.
        let r = match self.var("c5b_round") {
            Ok(v) => v.parse().map_err(|e| format!("{e}"))?,
            Err(_) => {
                let r = self.open_round()?;
                self.set_var("c5b_round", r);
                r
            }
        };
        let specs: Vec<BuySpec> = self.round_specs("C5b", r, self.opts.c5b_buys, 1, false);
        self.buys(s, specs).await?;
        self.wait_deadline().await?;
        let (_, ctrl) = self.controller()?;
        let t3 = ctrl.deadline as u64;
        let allowed: Vec<usize> = self.shards(r)?.iter().filter(|(l, _)| l.daa < t3).map(|(_, st)| st.shard_index as usize).collect();
        let n0 = self.opts.c5b_buys as u64;
        let specs = allowed
            .iter()
            .map(|&i| BuySpec { action: format!("C5b/post-deadline buy shard {i}"), round: r, shard: i, n: n0 + i as u64, b: 1, with_ref: false })
            .collect();
        self.buys(s, specs).await?;
        self.post_deadline_check().await?;
        let name = "C5b: a further buy on a post-deadline shard refused (sales pause)";
        if !check_done(&self.c, name) {
            let i = self.shards(r)?.iter().find(|(l, _)| l.daa >= t3).map(|(_, st)| st.shard_index as usize).ok_or("no post-deadline shard")?;
            self.ensure_funding(1, 8 * KAS).await?;
            let fund = self.funds().await?[0];
            let f = self.build_buy(&BuySpec { action: name.into(), round: r, shard: i, n: n0 + 100, b: 1, with_ref: false }, &fund)?;
            self.refuse(s, name, &f, Refusal::Script, Some(0)).await?;
        }
        let name = format!("C5b: close() of round {r} before round {}'s draw refused (strict order)", r - 1);
        if !check_done(&self.c, &name) {
            let fund = self.one_fund(10 * KAS).await?;
            let (f, _, _) = self.build_close(&fund)?;
            self.refuse(s, &name, &f, Refusal::Script, Some(0)).await?;
        }
        Ok(())
    }

    /// The C5b check from the accepted post-deadline buy actions (one per shard whose UTXO
    /// predated the deadline). Computed from what was accepted, not from the shards still
    /// predating it: after a resume those were already bought (S3b: a resume recorded
    /// "0 shards" although all 16 had been accepted).
    async fn post_deadline_check(&mut self) -> R<()> {
        let shards: Vec<usize> = (0..K).filter(|i| self.done(&format!("C5b/post-deadline buy shard {i}"))).collect();
        self.check("C5b", "one buy per shard whose UTXO predates the deadline accepted after it", !shards.is_empty(), format!("{} shards accepted ({shards:?})", shards.len())).await
    }

    /// C5c (shifted by one round): draw 3 held back 60 min (inside 12 h), the carried
    /// withdrawals settled after it, round 4 closed and drawn from the index offline, one
    /// round-4 claim from the index.
    pub async fn c5c(&mut self) -> R<()> {
        let s = "C5c";
        self.sync().await?;
        // Re-evaluate a C5b check a resume recorded wrongly (from the accepted actions).
        if self.c.steps.iter().flat_map(|x| &x.checks).any(|k| k.name == "one buy per shard whose UTXO predates the deadline accepted after it" && !k.ok) {
            self.post_deadline_check().await?;
        }
        let r3: i64 = self.var("c5b_empty_round")?.parse().map_err(|e| format!("{e}"))?;
        let draw = format!("C5c/draw r{r3} (60 min late)");
        if !self.done(&draw) {
            self.draw(s, &draw, LATE_DAA).await?;
        }
        let carried = [SettleMode::Final, SettleMode::Pay, SettleMode::Accrue];
        self.settle_all(s, &format!("C5c/carried r{r3}"), lottery::state::KIND_WITHDRAW, r3, &carried).await?;
        let r4: i64 = self.var("c5b_round")?.parse().map_err(|e| format!("{e}"))?;
        if self.open_round()? == r4 {
            self.close(s, &format!("C5c/close r{r4}")).await?;
        }
        let draw4 = format!("C5c/draw r{r4} (indexer store)");
        if !self.done(&draw4) {
            self.draw(s, &draw4, 0).await?;
        }
        self.set_var("c5c_round", r4);
        let n = self.claims(s, &format!("C5c/r{r4} index"), r4, ClaimFrom::Index, false, 1).await?;
        self.record(s, luck_check(&format!("one round-{r4} claim from the indexer store"), n, "exercised".to_string(), format!("not exercised: no winning round-{r4} holding of ours (luck)"))).await
    }

    /// C6: settle the carried requests; the next two rounds, plain (rounds 5 and 6 after
    /// the one-round shift of C5b/C5c).
    pub async fn c6(&mut self) -> R<()> {
        let s = "C6";
        self.sync().await?;
        // The carried requests settle against each new record until FINAL (a vault still
        // capped by the liability lock pays part and carries the rest again, PAY/ACCRUE).
        let carried = [SettleMode::Final, SettleMode::Pay, SettleMode::Accrue];
        let r3: i64 = self.var("c5c_round")?.parse().map_err(|e| format!("{e}"))?;
        let mut modes = self.settle_all(s, &format!("C6/carried r{r3}"), lottery::state::KIND_WITHDRAW, r3, &carried).await?;
        for k in 0..2 {
            let r = match self.var(&format!("c6_round{k}")) {
                Ok(v) => v.parse().map_err(|e| format!("{e}"))?,
                Err(_) => {
                    let r = self.open_round()?;
                    self.set_var(&format!("c6_round{k}"), r);
                    r
                }
            };
            if self.open_round()? == r {
                self.balance_check(s, &format!("round {r}")).await?;
                let specs = self.round_specs(&format!("C6/r{r}"), r, self.opts.plain_buys, 4, false);
                self.buys(s, specs).await?;
                if k == 0 && self.opts.c6_deposit > 0 {
                    // PENDING A5 amendment (S3a dry run): the liability lock keeps L' in the
                    // vault, so the last backers' carried requests convert only the slack each
                    // draw (PAY again, never FINAL). A new deposit converted in the same draw
                    // adds the slack that lets them convert fully.
                    let d = self.opts.c6_deposit as i64;
                    self.deposit(s, &format!("C6/deposit {} (backs the carried withdrawals)", kas(d as u64)), d, false).await?;
                }
                self.close(s, &format!("C6/close r{r}")).await?;
            }
            let draw = format!("C6/draw r{r}");
            if !self.done(&draw) {
                self.draw(s, &draw, 0).await?;
            }
            let (_, c) = self.controller()?;
            self.check(s, &format!("m stays 2 after round {r}"), c.m == 2, format!("m {}", c.m)).await?;
            modes.extend(self.settle_all(s, &format!("C6/carried r{r}"), lottery::state::KIND_WITHDRAW, r, &carried).await?);
            self.settle_all(s, &format!("C6/deposit r{r}"), lottery::state::KIND_DEPOSIT, r, &[SettleMode::DepositConvert, SettleMode::DepositRefund]).await?;
            self.claims(s, &format!("C6/r{r}"), r, ClaimFrom::Wallet, false, usize::MAX).await?;
        }
        let left = self.lives(Role::Request)?.iter().filter(|(_, st)| matches!(st, State::Request(q) if q.kind == lottery::state::KIND_WITHDRAW)).count();
        self.check(
            s,
            "the carried withdrawal requests settle FINAL (A5 C6)",
            modes.contains(&SettleMode::Final) && left == 0,
            format!("modes in C6 {modes:?}; live requests left {left}"),
        )
        .await
    }

    /// C7: the fast-campaign report.
    pub async fn c7(&mut self) -> R<()> {
        let text = self.report()?;
        let dir = self.dir()?;
        indexer::write_atomic(&dir.join("report.md"), &text)?;
        println!("{text}");
        let all_ok = self.c.steps.iter().flat_map(|s| &s.checks).all(|k| k.ok);
        self.check("C7", "fast-campaign report written; every check of C0-C6 ok", all_ok, dir.join("report.md").display().to_string()).await
    }

    /// C8a: the soak round's buys (t0).
    pub async fn c8a(&mut self) -> R<()> {
        let s = "C8a";
        self.sync().await?;
        // S4: the round left open by C6 (round 7 on testnet-10) may be past its deadline
        // when the soak starts (no buy possible, shard check 6). Close it (fresh input)
        // and draw it with no sales first; the fresh close opens the soak round with
        // d_min of sales.
        if self.var("soak_round").is_err() {
            // Test hook (`TN10_DRY_SOAK_LATE=1`, dry run only): let the open round's
            // deadline pass before the soak, as on testnet-10 after S3b.
            if self.net.dry() && std::env::var("TN10_DRY_SOAK_LATE").is_ok_and(|v| v == "1") && self.var("c8_empty_round").is_err() {
                let (_, ctrl) = self.controller()?;
                self.net.wait_daa(ctrl.deadline as u64 + 3_600, "open round's deadline passed (dry-run hook)").await?;
                self.sync().await?;
            }
            let pre = match self.var("c8_empty_round") {
                Ok(v) => Some(v.parse::<i64>().map_err(|e| format!("{e}"))?),
                Err(_) => {
                    let (_, ctrl) = self.controller()?;
                    let now = self.net.daa().await?;
                    if needs_pre_soak_close(ctrl.deadline as u64, ctrl.d_min as u64, now) {
                        self.set_var("c8_empty_round", ctrl.round);
                        Some(ctrl.round)
                    } else {
                        None
                    }
                }
            };
            if let Some(r0) = pre {
                if self.open_round()? == r0 {
                    let name = format!("round {r0} past its deadline before the soak: closed and drawn with no sales");
                    if !check_done(&self.c, &name) {
                        let (_, ctrl) = self.controller()?;
                        let sales: i64 = self.shards(r0)?.iter().map(|(_, st)| st.sales).sum();
                        let now = self.net.daa().await?;
                        self.check(s, &name, sales == 0, format!("deadline {}, virtual DAA {now}, sales {sales}", ctrl.deadline)).await?;
                    }
                    self.close(s, &format!("C8a/close r{r0} (no sales, before the soak)")).await?;
                }
                let draw = format!("C8a/draw r{r0} (no sales, before the soak)");
                if !self.done(&draw) {
                    self.draw(s, &draw, 0).await?;
                }
            }
        }
        let r = match self.var("soak_round") {
            Ok(v) => v.parse().map_err(|e| format!("{e}"))?,
            Err(_) => {
                let r = self.open_round()?;
                self.set_var("soak_round", r);
                let t0 = self.net.now().await?;
                self.set_var("soak_t0", t0);
                r
            }
        };
        self.balance_check(s, &format!("soak round {r}")).await?;
        let (_, ctrl) = self.controller()?;
        let now = self.net.daa().await?;
        if ctrl.round == r && !self.done(&format!("C8/buy/{}", self.opts.buys - 1)) && now + ctrl.d_min as u64 / 2 > ctrl.deadline as u64 {
            return Err(format!("{s}: soak round {r}'s deadline {} is too close (virtual DAA {now}) for the soak buys", ctrl.deadline));
        }
        println!("  soak round {r}: deadline {} (virtual DAA {now}, {} DAA of sales left)", ctrl.deadline, (ctrl.deadline as u64).saturating_sub(now));
        let specs = self.round_specs("C8", r, self.opts.buys, 4, false);
        self.buys(s, specs).await
    }

    /// C8b: one indexer catch-up inside the pruning window (mode B sitting 2).
    pub async fn c8b(&mut self) -> R<()> {
        let t0: u64 = self.var("soak_t0")?.parse().map_err(|e| format!("{e}"))?;
        let now = self.net.now().await?;
        if self.opts.soak == SoakMode::B && now < t0 + 8 * H && !self.net.dry() {
            return Err(format!("too early for C8b: {}", next_sitting("C8b", Some(t0), now, self.opts.soak)));
        }
        if self.net.dry() {
            let d = self.net.daa().await?;
            self.net.wait_daa(d + 10 * 36_000, "t0 + 10 h (dry run)").await?;
        }
        self.sync().await?;
        let x = self.x()?;
        self.check("C8b", "indexer caught up inside the pruning window", true, format!("last chain block {} (DAA {})", x.last, x.last_daa)).await
    }

    /// C8c: close, deletion evidence, late draw from the index, the dual claim (built from
    /// the wallet and from the index, compared, the indexer-built one submitted), refresh of
    /// an idle ticket.
    pub async fn c8c(&mut self) -> R<()> {
        let s = "C8c";
        let t0: u64 = self.var("soak_t0")?.parse().map_err(|e| format!("{e}"))?;
        let r: i64 = self.var("soak_round")?.parse().map_err(|e| format!("{e}"))?;
        let close_at = if self.opts.soak == SoakMode::A { t0 + 20 * H } else { t0 + 29 * H + H / 2 };
        if self.net.dry() {
            let now = self.net.now().await?;
            let d = self.net.daa().await?;
            self.net.wait_daa(d + (close_at.saturating_sub(now)) * 10, "soak close time (dry run)").await?;
        } else if self.net.now().await? < close_at {
            return Err(format!("too early for C8c: {}", next_sitting("C8c", Some(t0), self.net.now().await?, self.opts.soak)));
        }
        self.sync().await?;
        self.close(s, &format!("C8/close r{r}")).await?;
        // Deletion evidence: the block of round r's earliest buy is no longer served.
        let blk = self.x()?.earliest_buy_blocks().into_iter().find(|(rr, _)| *rr == r).map(|(_, b)| b).ok_or("no indexed buy of the soak round")?;
        let name = format!("C8: a public node no longer returns the block of round {r}'s first buy");
        if !check_done(&self.c, &name) {
            let mut ev = None;
            for _ in 0..240 {
                if let Some(e) = self.net.deleted(blk).await? {
                    ev = Some(e);
                    break;
                }
                if self.net.dry() {
                    let d = self.net.daa().await?;
                    self.net.wait_daa(d + 36_000, "simulated hour").await?;
                } else {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            }
            let ev = ev.ok_or("the block is still served after 4 h of waiting")?;
            self.check(s, &name, true, ev).await?;
        }
        let draw_at = t0 + 30 * H;
        if self.net.dry() {
            let now = self.net.now().await?;
            let d = self.net.daa().await?;
            self.net.wait_daa(d + draw_at.saturating_sub(now) * 10, "t0 + 30 h (dry run)").await?;
        } else {
            while self.net.now().await? < draw_at {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        }
        let draw = format!("C8/draw r{r} (indexer store, after deletion)");
        if !self.done(&draw) {
            // Dry-run hook (`TN10_DRY_C8_WIN=1`): grind the simulated anchor so that one of
            // our round-r combinations wins, exercising the dual claim below.
            if self.net.dry() && std::env::var("TN10_DRY_C8_WIN").is_ok_and(|v| v == "1") {
                let w = self.wallets.get(&r).ok_or("no wallet of the soak round")?;
                let combos = w.tickets.iter().flat_map(|t| t.bundle.chunks_exact(6).map(|c| <[u8; 6]>::try_from(c).unwrap())).collect();
                let f = ForceWin { cov: self.cov()?.as_bytes(), round: r, m: w.m, combos };
                self.net.force_win(f);
            }
            self.draw(s, &draw, 0).await?;
        }
        // S4 owner note: the claim is built from the wallet store and from the indexer store,
        // compared, and the indexer-built one is submitted (A7: both items).
        // Owner decision 2026-10-25 (round 8 closed past the U6 bound, D102): every winning
        // index of ours is claimed in this sitting, before the tickets' return_deposit_after
        // opens (T + D_RET), each with the dual build and comparison.
        let n = self.claims(s, &format!("C8/r{r}"), r, ClaimFrom::Both, false, usize::MAX).await?;
        let how = |what: &str| format!("exercised ({what}; see the dual-claim check)");
        let not = format!("not exercised: no winning round-{r} holding of ours (luck)");
        self.record(s, luck_check("A7: a claim from wallet-kept trees > 30 h later", n, how("built from the wallet store, identical to the submitted one"), not.clone())).await?;
        self.record(s, luck_check("A7: a claim from the indexer copy", n, how("the indexer-built claim was submitted"), not)).await?;
        if !self.done("C8/refresh ticket") {
            // The oldest live ticket of the soak round (bought at t0, idle since).
            // (min by creation DAA: a ticket' left by the claim above is new, not idle.)
            let tk = self
                .lives(Role::Ticket)?
                .into_iter()
                .filter(|(_, s)| matches!(s, State::Ticket(t) if t.round == r && t.payout_key == self.me.key33()))
                .min_by_key(|(l, _)| l.daa)
                .ok_or("no live soak-round ticket of ours")?;
            self.net.wait_daa(tk.0.daa + D_IDLE as u64 + SETTLE_DAA, "ticket idle D_IDLE (24 h)").await?;
            self.refresh_rate().await?;
            let fund = self.one_fund(5 * KAS).await?;
            let f = self.build_refresh(&tk, &fund)?;
            self.send_wait(s, "C8/refresh ticket", &f).await?;
        }
        Ok(())
    }

    /// M8 S4: log a re-anchor (`tn10 campaign reanchor`) as a check of the current step.
    pub async fn record_reanchor(&mut self, l: &store::ReanchorLine, u: &indexer::Unchanged) -> R<()> {
        let step = self.c.step.clone();
        let detail = format!(
            "old block {} (DAA {}) -> new block {} (DAA {}) on {}; {} live instance UTXOs unchanged, no unknown instance-bound UTXO ({} node entries, {} not instance-bound); argument: {}",
            hex(&l.old_block),
            l.old_daa_text(),
            hex(&l.new_block),
            l.new_daa,
            l.node,
            u.live_checked,
            u.node_entries,
            u.foreign,
            l.argument
        );
        self.check(&step, &format!("indexer re-anchored after the pruning window (DAA {})", l.new_daa), true, detail).await
    }

    /// One index catch-up to the node's sink (with the wallet feed).
    pub async fn catch_up(&mut self) -> R<()> {
        self.sync().await?;
        let x = self.x()?;
        println!("indexer caught up: last chain block {} (DAA {})", x.last, x.last_daa);
        Ok(())
    }

    /// The report (C7): every step, its transactions and checks.
    pub fn report(&self) -> R<String> {
        let mut t = format!("# M8 campaign report ({})\n\ncovenant ID `{}`; node {}\n\n", if self.net.dry() { "DRY RUN, nothing submitted" } else { "testnet-10" }, hex(&self.cov()?.as_bytes()), self.net.node());
        if let Some(x) = self.x.as_ref().filter(|x| !x.reanchors().is_empty()) {
            t.push_str("## Index re-anchors (M8 S4)\n\n");
            for r in x.reanchors() {
                t.push_str(&format!(
                    "- after {} log lines: `{}` (DAA {}) -> `{}` (DAA {}) on {}, {} live instance UTXOs unchanged, wall {}\n",
                    r.at,
                    hex(&r.old_block),
                    r.old_daa_text(),
                    hex(&r.new_block),
                    r.new_daa,
                    r.node,
                    r.live_checked,
                    r.wall_time
                ));
            }
            t.push('\n');
        }
        for st in &self.c.steps {
            t.push_str(&format!("## {} (DAA {}, wall {})\n\n", st.id, st.daa, st.wall_time));
            for k in &st.checks {
                t.push_str(&check_line(k));
                t.push('\n');
            }
            if !st.txids.is_empty() {
                t.push_str(&format!("\n{} transactions:\n", st.txids.len()));
                for x in &st.txids {
                    t.push_str(&format!("- {} `{}`\n", x.action, hex(&x.txid)));
                }
            }
            t.push('\n');
        }
        Ok(t)
    }

    /// Run from the current step until `opts.until`.
    pub async fn run(&mut self) -> R<()> {
        loop {
            let step = self.c.step.clone();
            if past(&step, &self.opts.until) {
                println!("campaign: stopping before {step} (--until {})", self.opts.until);
                return Ok(());
            }
            println!("== {step} ==");
            match step.as_str() {
                "C0" => self.c0().await?,
                "C1" => self.c1().await?,
                "C2" => self.c2().await?,
                "C3" => self.c3().await?,
                "C4" => self.c4().await?,
                "C5" => self.c5().await?,
                "C5b" => self.c5b().await?,
                "C5c" => self.c5c().await?,
                "C6" => self.c6().await?,
                "C7" => self.c7().await?,
                "C8a" => self.c8a().await?,
                "C8b" => self.c8b().await?,
                "C8c" => self.c8c().await?,
                other => return Err(format!("unknown step {other}")),
            }
            self.c.step = next_step(&step);
            self.save()?;
        }
    }
}

/// The next round's deadline a close() with inputs created at `input_daas` writes:
/// T' = max(T + round_len, own + d_min), own = the newest input's creation DAA
/// (`controller.sil:473-484`, check 20).
/// A check under the A5 luck rule: a pass when `n` > 0 items were exercised, otherwise
/// status `not_exercised` with that detail (M8 S5: not recorded as a pass).
pub fn luck_check(name: &str, n: usize, exercised: String, not_exercised: String) -> CheckJson {
    if n > 0 {
        CheckJson { name: name.into(), ok: true, detail: exercised, status: None }
    } else {
        CheckJson { name: name.into(), ok: true, detail: not_exercised, status: Some(CheckStatus::NotExercised) }
    }
}

/// One check as a `report.md` line (no newline): `[x]` pass, `[ ]` failure, `[-]` ... NOT
/// EXERCISED.
pub fn check_line(k: &CheckJson) -> String {
    if !k.ok {
        format!("- [ ] {}: {}", k.name, k.detail)
    } else if k.not_exercised() {
        format!("- [-] {} (NOT EXERCISED): {}", k.name, k.detail)
    } else {
        format!("- [x] {}: {}", k.name, k.detail)
    }
}

/// `campaign status`: the step, the checks and the next soak sitting.
pub fn status(root: &Path, cov: Option<Hash>, mode: SoakMode) -> R<()> {
    let cov_hex = match cov {
        Some(c) => hex(&c.as_bytes()),
        None => std::fs::read_to_string(root.join("current")).map_err(|_| format!("no current campaign in {}", root.display()))?.trim().to_string(),
    };
    let c: Campaign = store::read(&std::fs::read_to_string(root.join(&cov_hex).join("campaign.json")).map_err(|e| e.to_string())?, store::CAMPAIGN)?;
    println!("campaign {cov_hex}: next step {}", c.step);
    for s in &c.steps {
        let fails: Vec<&CheckJson> = s.checks.iter().filter(|k| !k.ok).collect();
        let ne: Vec<&CheckJson> = s.checks.iter().filter(|k| k.ok && k.not_exercised()).collect();
        println!("  {}: {} transactions, {} checks ({} failed, {} not exercised), DAA {}, wall {}", s.id, s.txids.len(), s.checks.len(), fails.len(), ne.len(), s.daa, s.wall_time);
        for k in fails {
            println!("    FAIL {}: {}", k.name, k.detail);
        }
        for k in ne {
            println!("    NOT EXERCISED {}: {}", k.name, k.detail);
        }
    }
    let t0 = c.vars.get("soak_t0").and_then(|v| v.parse().ok());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    println!("{}", next_sitting(&c.step, t0, now, mode));
    if mode == SoakMode::B {
        println!("C8 amendment (mode B): the close moves from t0 + 20 h to the last sitting (about t0 + 29.5 h); the draw must be >= t0 + 30 h (deletion) and <= the close's D + F (12 h).");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// F6 part C: every beta profile passes genesis `validate()` (the floors: claim window
    /// >= 7 days, draw_window >= VOID_MIN, the windows within D_RET, margin >= 1.5, the edge
    /// and whale rules, L(1) under the vault cap) and keeps the windows at their floors.
    #[test]
    fn beta_profiles_pass_genesis_validation() {
        let t = Templates::get(b_max());
        for p in ["fast", "steady", "m8"] {
            let k = GenesisKnobs::beta(p).unwrap();
            let mut g = GenesisCase::new_with(K, vec![], &t);
            k.apply(&mut g, 2_000_000, &Payout::Schnorr([7; 32]));
            g.params.admin_hash = admin_hash(&[0x55; 32]);
            g.validate().unwrap_or_else(|e| panic!("{p}: {e}"));
            if p != "m8" {
                assert_eq!((g.drawer.claim_window, g.drawer.draw_window, g.controller.seed_offset), (lottery::game::CLAIM_WINDOW, lottery::game::VOID_MIN, lottery::game::SEED_OFFSET), "{p}");
            }
        }
        assert_eq!(GenesisKnobs::beta("fast").unwrap().round_len, 36_000);
        assert_eq!(GenesisKnobs::beta("steady").unwrap().round_len, 864_000);
        assert!(GenesisKnobs::beta("hourly").is_err());
        // Negative controls: a claim window one DAA below the floor, a draw window below VOID_MIN.
        let mut k = GenesisKnobs::beta("fast").unwrap();
        k.claim_window = Some(lottery::game::CLAIM_WINDOW - 1);
        let mut g = GenesisCase::new_with(K, vec![], &t);
        k.apply(&mut g, 2_000_000, &Payout::Schnorr([7; 32]));
        assert!(g.validate().unwrap_err().contains("CLAIM_WINDOW"));
        let mut k = GenesisKnobs::beta("fast").unwrap();
        k.draw_window = Some(lottery::game::VOID_MIN - 1);
        let mut g = GenesisCase::new_with(K, vec![], &t);
        k.apply(&mut g, 2_000_000, &Payout::Schnorr([7; 32]));
        assert!(g.validate().unwrap_err().contains("VOID_MIN"));
    }

    /// The admin guard refuses the public default key (and accepts another).
    #[test]
    fn admin_guard_refuses_the_public_default_key() {
        assert!(admin_guard(&ADMIN_SK).unwrap_err().contains("ADMIN_SK"));
        assert!(admin_guard(&[0x55; 32]).is_ok());
    }

    use super::*;
    use lottery::close::close_deadline;

    #[test]
    fn refusal_reason_matching() {
        let script = "RPC Server (remote error) -> Rejected transaction 1234: failed to verify the signature script: script ran, but verification failed";
        let orphan = "Rejected transaction ab: transaction ab is an orphan where orphan is disallowed";
        let mempool = "output cd:0 already spent by transaction ef in the mempool";
        assert!(Refusal::Script.matches(script));
        assert!(!Refusal::Script.matches(orphan));
        assert!(!Refusal::Script.matches("transaction 1 is not standard: fee below"));
        assert!(Refusal::Spent.matches(orphan));
        assert!(Refusal::Spent.matches(mempool));
        assert!(!Refusal::Spent.matches(script));
        assert!(!Refusal::Spent.matches("output cd:0 already spent by transaction ef"), "the mempool form needs both parts");
    }

    #[test]
    fn steps_order_and_until() {
        assert_eq!(next_step("C0"), "C1");
        assert_eq!(next_step("C5"), "C5b");
        assert_eq!(next_step("C7"), "C8a");
        assert_eq!(next_step("C8c"), DONE);
        assert!(!past("C7", "C7"));
        assert!(past("C8a", "C7"));
        assert!(past(DONE, "C8c"));
        assert!(!past("C2", "C7"));
    }

    fn camp(actions: &[(&str, u8)]) -> Campaign {
        let mut c = Campaign::new();
        c.steps.push(StepJson {
            id: "C2".into(),
            txids: actions.iter().map(|(a, t)| TxRef { txid: [*t; 32], node: "n".into(), action: a.to_string() }).collect(),
            checks: vec![CheckJson { name: "refused X".into(), ok: true, detail: String::new(), status: None }],
            daa: 1,
            wall_time: 1,
        });
        c
    }

    /// Resume: an action is done when one of its recorded txids is accepted; a recorded
    /// but unaccepted one (rejected, or replaced by a rebuild) is redone; checks are done
    /// once recorded; the JSON round-trips with the new fields.
    #[test]
    fn resume_is_idempotent_by_txid() {
        let c = camp(&[("C2/buy/0", 1), ("C2/buy/1", 2), ("C2/buy/1", 3)]);
        let acc = |t: &[u8; 32]| t[0] == 1 || t[0] == 3;
        assert!(action_done(&c, "C2/buy/0", acc));
        assert!(action_done(&c, "C2/buy/1", acc), "the rebuilt second txid is accepted");
        assert!(!action_done(&c, "C2/buy/1", |t: &[u8; 32]| t[0] == 1));
        assert!(!action_done(&c, "C2/buy/2", acc));
        assert_eq!(action_txids(&c, "C2/buy/1"), vec![[2; 32], [3; 32]]);
        assert!(check_done(&c, "refused X"));
        assert!(!check_done(&c, "refused Y"));
        let mut c2 = c.clone();
        c2.vars.insert("soak_t0".into(), "1800000000".into());
        let j = store::write(&c2, store::CAMPAIGN).unwrap();
        assert_eq!(store::read::<Campaign>(&j, store::CAMPAIGN).unwrap(), c2);
        // A campaign/1 file written before the S3 fields still reads (serde defaults).
        let old = j.replace(",\n      \"action\": \"C2/buy/0\"", "");
        assert!(store::read::<Campaign>(&old, store::CAMPAIGN).is_ok());
    }

    /// S3b reproduction (testnet-10, 2026-10-17): round 2's deadline T = 598,556,019, its
    /// newest close input about 598,556,019, the close accepted at 598,564,744. Without a
    /// fresh input T' = 598,562,019 < the close: round 3 could not sell. A funding input
    /// accepted just before the close moves T' to d_min after it.
    #[test]
    fn stale_close_inputs_give_a_past_deadline_fresh_input_fixes_it() {
        let (t, len, d_min, close_at) = (598_556_019u64, 6_000, 6_000, 598_564_744u64);
        let stale = close_deadline(t, len, [598_556_019, 598_552_725, 598_543_977], d_min);
        assert_eq!(stale, 598_562_019);
        assert!(stale < close_at, "the next round opens past its deadline");
        let fresh = close_deadline(t, len, [598_556_019, close_at - 10], d_min);
        assert_eq!(fresh, close_at - 10 + d_min);
        assert!(fresh >= close_at + d_min / 2, "the guard in close() lets it through");
        // A timely close is unchanged in kind: T + round_len still wins when inputs are older.
        assert_eq!(close_deadline(t, len, [t - 100], d_min), t + len);
        // M8 S5: the library guard the runner now calls refuses the S3b close and passes the
        // one with a fresh input.
        let guard = |daas: &[u64]| lottery::close::refuse_stale_inputs(t as i64, len as i64, d_min as i64, daas.iter().copied(), close_at);
        assert!(guard(&[598_556_019, 598_552_725, 598_543_977]).unwrap_err().contains("stale inputs"));
        assert_eq!(guard(&[598_556_019, close_at - 10]), Ok(fresh));
    }

    /// M8 S5 report honesty: a luck-rule check that did not run is recorded as
    /// `not_exercised` in campaign/1 and report.md (not as a pass); an exercised one is a
    /// plain pass; campaign/1 files written before the field still read (no status).
    #[test]
    fn not_exercised_checks_are_not_passes() {
        let ne = luck_check("A7: a claim from the indexer copy", 0, "exercised".into(), "not exercised: no winning round-8 holding of ours (luck)".into());
        let mut c = camp(&[]);
        c.steps[0].checks = vec![ne.clone()];
        let j = store::write(&c, store::CAMPAIGN).unwrap();
        assert!(j.contains(r#""status": "not_exercised""#), "{j}");
        assert!(ne.ok, "not a failure: the campaign's ok gates are unchanged");
        let line = check_line(&ne);
        assert!(!line.starts_with("- [x]"), "{line}");
        assert!(line.starts_with("- [-]") && line.contains("NOT EXERCISED"), "{line}");
        let ex = luck_check("A7: a claim from the indexer copy", 2, "exercised".into(), "not exercised".into());
        c.steps[0].checks = vec![ex.clone()];
        assert!(!store::write(&c, store::CAMPAIGN).unwrap().contains("status"), "an exercised check writes no status");
        assert_eq!(check_line(&ex), "- [x] A7: a claim from the indexer copy: exercised");
        let fail = CheckJson { ok: false, ..ex.clone() };
        assert!(check_line(&fail).starts_with("- [ ]"));
        // A campaign/1 file written before S5 (no status field) still reads, its checks as
        // before; and the not-exercised record round-trips.
        let back: Campaign = store::read(&j, store::CAMPAIGN).unwrap();
        assert_eq!(back.steps[0].checks, vec![ne]);
        let old = store::write(&camp(&[]), store::CAMPAIGN).unwrap();
        assert!(!old.contains("status"));
        let k = &store::read::<Campaign>(&old, store::CAMPAIGN).unwrap().steps[0].checks[0];
        assert!(check_line(k).starts_with("- [x] refused X"));
    }

    #[test]
    fn soak_status_windows() {
        let t0 = 1_800_000_000;
        assert!(next_sitting("C8a", None, t0, SoakMode::B).contains("C8a"));
        let b = next_sitting("C8b", Some(t0), t0 + H, SoakMode::B);
        assert!(b.contains("t0 + 8.0 h") && b.contains("t0 + 24.0 h"), "{b}");
        assert!(next_sitting("C8b", Some(t0), t0 + 25 * H, SoakMode::B).contains("missed"));
        let c = next_sitting("C8c", Some(t0), t0 + 10 * H, SoakMode::B);
        assert!(c.contains("t0 + 29.5 h") && c.contains("t0 + 30.0 h"), "{c}");
        assert!(next_sitting("C8c", Some(t0), t0, SoakMode::A).contains("t0 + 20.0 h"));
        assert_eq!(next_sitting("C5", Some(t0), t0, SoakMode::B), "next step: C5");
        // S4: the windows carry a wall-clock date (UTC).
        assert!(b.contains(&utc(t0 + 8 * H)) && b.contains(&utc(t0 + 24 * H)), "{b}");
        assert!(c.contains(&utc(t0 + 29 * H + H / 2)), "{c}");
    }

    #[test]
    fn utc_wall_clock() {
        assert_eq!(utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc(951_782_400), "2000-02-29 00:00 UTC");
        // S3b's C7 step wall time (2026-10-17 02:21:47 local, UTC+2).
        assert_eq!(utc(1_792_196_507), "2026-10-17 00:21 UTC");
    }

    #[test]
    fn pre_soak_close_when_the_open_round_cannot_sell() {
        // S3b left round 7 open with its deadline at about DAA 598,625,800.
        assert!(needs_pre_soak_close(598_625_800, 6_000, 598_700_000));
        assert!(needs_pre_soak_close(598_625_800, 6_000, 598_623_000));
        assert!(!needs_pre_soak_close(598_625_800, 6_000, 598_622_000));
    }

    /// S4 dual claim: two builds of the same claim (randomised funding signatures) compare
    /// equal; any change to a covenant input, a funding input outside its signature, an
    /// output or the input count is a difference.
    #[test]
    fn dual_claim_comparison() {
        let c = ClaimCase::new(2);
        let a = c.build().finish().tx;
        let b = c.build().finish().tx;
        assert_eq!(a.inputs.len(), 3);
        let ok = same_except_funding_sigs(&a, &b, CLAIM_FUNDING_FROM).expect("two builds of one claim");
        assert!(ok.contains("2 covenant inputs and all 4 outputs identical"), "{ok}");
        assert_eq!(a.id(), b.id(), "the txid excludes signature scripts");
        // Only a funding signature differs (forced): still identical.
        let mut f = a.clone();
        f.inputs[2].signature_script[1] ^= 1;
        f.finalize();
        assert!(same_except_funding_sigs(&a, &f, CLAIM_FUNDING_FROM).unwrap().contains("1 of 1 funding signature scripts differ"));
        let diff = |m: &dyn Fn(&mut Transaction)| {
            let mut t = b.clone();
            m(&mut t);
            t.finalize();
            same_except_funding_sigs(&a, &t, CLAIM_FUNDING_FROM).expect_err("must differ")
        };
        assert_eq!(diff(&|t| t.outputs[0].value += 1), "output 0");
        assert_eq!(diff(&|t| t.outputs[3].value -= 1), "output 3");
        assert_eq!(diff(&|t| t.inputs[0].signature_script[4] ^= 1), "input 0 (covenant)");
        assert_eq!(diff(&|t| t.inputs[1].signature_script.push(0x51)), "input 1 (covenant)");
        assert_eq!(diff(&|t| t.inputs[2].previous_outpoint.index += 1), "input 2 (funding, outside its signature)");
        assert_eq!(diff(&|t| t.inputs[2].sequence += 1), "input 2 (funding, outside its signature)");
        assert!(diff(&|t| { t.outputs.pop(); }).starts_with("output count"));
        assert!(diff(&|t| { t.inputs.pop(); }).starts_with("input count"));
        assert!(diff(&|t| t.lock_time += 1).starts_with("a transaction field"));
        // With from = 3 (no funding exempt), the forced funding-signature change differs.
        assert_eq!(same_except_funding_sigs(&a, &f, 3).unwrap_err(), "input 2 (covenant)");
    }

    /// The dry-run forced win: the ground anchor makes one of the combinations win a tier.
    #[test]
    fn grind_anchor_makes_a_combination_win() {
        let combos: Vec<[u8; 6]> = (0..300u32).map(|i| Combo::new(unrank(i * 401), 1 + (i % 2) as u8).bytes()).collect();
        let f = ForceWin { cov: [9; 32], round: 8, m: 2, combos: combos.clone() };
        let (xh, tries) = grind_anchor(b"seed", &f);
        let s = lottery::game::seed_numbers(&f.cov, 8, &xh, 2);
        let win = [s.main[0], s.main[1], s.main[2], s.main[3], s.main[4], s.bonus];
        assert!(combos.iter().any(|c| lottery::game::tier_of(c, &win).is_some()), "after {tries} tries");
        assert_eq!(xh[0] & 1, 1);
    }

    /// M8 S4: the catch-up retries are bounded (12, about 17 min of waits in all).
    #[test]
    fn poll_retry_backoff_is_bounded() {
        let w: Vec<u64> = (1..=POLL_RETRIES).map(retry_backoff).collect();
        assert_eq!(&w[..7], &[5, 10, 20, 40, 80, 120, 120]);
        assert!(w.iter().all(|&s| s <= 120));
        assert_eq!(w.iter().sum::<u64>(), 5 + 10 + 20 + 40 + 80 + 120 * 7);
    }
}
