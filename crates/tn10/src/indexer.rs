//! Spec M8 A4 (`tn10 indexer`, D115): an untrusted, file-based index of one lottery
//! instance, read from a public node's virtual chain.
//!
//! RPC facts (rusty-kaspa v2.1.0, `refs/rusty-kaspa-v2.1.0`; details and the full citation
//! list in `docs/specs/8-handoff.md`, S3a step 1):
//! - `get_virtual_chain_from_block_v2(start, Some(Full), None)` (`rpc/core/src/api/rpc.rs:507-518`)
//!   returns `removed_chain_block_hashes`, `added_chain_block_hashes` and, per added chain
//!   block in order, `{chain_block_header, accepted_transactions}`
//!   (`rpc/core/src/model/message.rs:2891-2900`, `rpc/core/src/model/tx.rs:658-663`).
//! - One call returns a prefix of the chain: the batch is bounded by 10 x the mergeset size
//!   limit in merged blocks and `added` is truncated to what fit
//!   (`rpc/service/src/service.rs:1381, 1411`), so `poll` repeats until nothing is added.
//! - Full verbosity is needed to rebuild a transaction at all (version, lock_time,
//!   subnetwork_id and gas are Full, `rpc/core/src/convert/verbosity.rs:147-150`;
//!   `TryFrom<RpcOptionalTransaction>`, `rpc/core/src/convert/tx.rs:208-232`) and for the
//!   spent entries' creation DAA (`verbosity.rs:93-96`). `chain_tx` refuses a transaction
//!   without them, so a node that does not return them stops the indexer.
//! - A start hash below the node's retention root is refused by the node ("the queried hash
//!   does not have retention root on its chain", `consensus/src/consensus/mod.rs:858-863`;
//!   an unknown hash `HeaderNotFound`, `:437-446`): `poll` then stops with an error and
//!   never skips ahead.
//! - `get_block` returns a header-only (pruned) block with no transactions and an unknown
//!   one as an error (`rpc/service/src/service.rs:493-503`, `consensus/src/consensus/mod.rs:1427-1439`):
//!   `deleted_block` treats both as deletion evidence, the error only when it is the node's
//!   "cannot find header <hash>" (or the retention-root refusal); a timeout or another error
//!   is not evidence (M8 S5).
//!
//! What it keeps (A4): every accepted transaction that creates an output bound to the
//! instance's covenant ID or spends a live instance UTXO, as `index/1` add lines; chain
//! blocks the node removes become remove lines (newest first). Each kept transaction is
//! applied to a `lottery::index::Rebuild`; an `Err` (an `Unknown` transaction, a rebuilt
//! state that does not match) stops the indexer: nothing is ever skipped silently.

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kaspa_consensus_core::Hash;
use kaspa_consensus_core::tx::{Transaction, TransactionId};
use kaspa_rpc_core::{RpcDataVerbosityLevel, RpcOptionalTransaction, api::rpc::RpcApi};
use lottery::buy::Templates;
use lottery::index::{Accepted, Event, Rebuild, Spent, State};
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint};
use lottery::index::Live;
use lottery::store::{self, IndexLog, IndexLogLine, IndexSnapshot, LogKind, REANCHOR, ReanchorLine};

pub const LOG_FILE: &str = "index-log.jsonl";
pub const SNAPSHOT_FILE: &str = "index.json";

/// The repo-root data directory `tn10-data/` (git-ignored).
pub fn data_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tn10-data")
}

/// `tn10-data/<cov-hex>/`.
pub fn instance_dir(cov: Hash) -> PathBuf {
    data_root().join(store::to_hex(&cov.as_bytes()))
}

/// One accepted transaction as the node reports it.
#[derive(Clone, Debug)]
pub struct ChainTx {
    pub tx: Transaction,
    pub spent: Vec<Spent>,
}

/// One added chain block: its hash, DAA score and accepted transactions in order.
#[derive(Clone, Debug)]
pub struct ChainBlock {
    pub hash: Hash,
    pub daa: u64,
    pub txs: Vec<ChainTx>,
}

/// One `get_virtual_chain_from_block_v2` answer, converted and checked.
#[derive(Clone, Debug, Default)]
pub struct ChainBatch {
    pub removed: Vec<Hash>,
    pub added: Vec<ChainBlock>,
}

/// A transaction from the node's Full-verbosity form, with its spent entries. Errors name
/// the missing field: the indexer needs every one of them.
pub fn chain_tx(rtx: RpcOptionalTransaction) -> Result<ChainTx, String> {
    let claimed = rtx.verbose_data.as_ref().and_then(|v| v.transaction_id);
    let mut spent = Vec::with_capacity(rtx.inputs.len());
    for (i, inp) in rtx.inputs.iter().enumerate() {
        let e = inp
            .verbose_data
            .as_ref()
            .and_then(|v| v.utxo_entry.as_ref())
            .ok_or_else(|| format!("input {i}: the node returned no spent UTXO entry (Full verbosity required)"))?;
        let amount = e.amount.ok_or_else(|| format!("input {i}: the node returned no spent amount (Full verbosity required)"))?;
        let daa = e.block_daa_score.ok_or_else(|| format!("input {i}: the node returned no spent creation DAA score (Full verbosity required)"))?;
        spent.push(Spent { amount, daa });
    }
    let tx = Transaction::try_from(rtx).map_err(|e| format!("transaction fields missing (Full verbosity required): {e}"))?;
    if let Some(id) = claimed {
        if id != tx.id() {
            return Err(format!("the node's transaction id {id} is not the rebuilt transaction's {}", tx.id()));
        }
    }
    Ok(ChainTx { tx, spent })
}

/// Convert a v2 answer; every added chain block must come with its header hash, DAA score
/// and accepted transactions, in the order of `added_chain_block_hashes`.
pub fn batch_of(r: kaspa_rpc_core::GetVirtualChainFromBlockV2Response) -> Result<ChainBatch, String> {
    let added_hashes = r.added_chain_block_hashes.as_ref().clone();
    let acc = Arc::try_unwrap(r.chain_block_accepted_transactions).unwrap_or_else(|a| (*a).clone());
    if acc.len() != added_hashes.len() {
        return Err(format!("v2: {} added chain blocks but {} acceptance entries", added_hashes.len(), acc.len()));
    }
    let mut added = Vec::with_capacity(acc.len());
    for (h, a) in added_hashes.into_iter().zip(acc) {
        let hh = a.chain_block_header.hash.ok_or_else(|| format!("chain block {h}: no header hash"))?;
        if hh != h {
            return Err(format!("v2: acceptance entry header {hh} != added chain block {h}"));
        }
        let daa = a.chain_block_header.daa_score.ok_or_else(|| format!("chain block {h}: no DAA score (verbosity)"))?;
        let txs = a.accepted_transactions.into_iter().map(chain_tx).collect::<Result<Vec<_>, _>>().map_err(|e| format!("chain block {h}: {e}"))?;
        added.push(ChainBlock { hash: h, daa, txs });
    }
    Ok(ChainBatch { removed: r.removed_chain_block_hashes.as_ref().clone(), added })
}

/// A log entry currently in effect (an add line not removed).
#[derive(Clone, Debug)]
struct Active {
    txid: TransactionId,
    chain_block: Hash,
    chain_daa: u64,
    accepted: Accepted,
    /// Whether the index applied it (an instance transaction) or it was foreign.
    applied: bool,
}

/// What one `ingest` did.
#[derive(Clone, Debug, Default)]
pub struct Ingested {
    pub blocks: usize,
    pub removed_blocks: usize,
    pub kept: usize,
    pub reverted: usize,
    /// Instance transactions applied, with their events, in order.
    pub events: Vec<(Accepted, Event)>,
}

/// The index of one instance plus its files.
pub struct Indexer {
    dir: Option<PathBuf>,
    pub t: Arc<Templates>,
    pub cov: Hash,
    genesis: Option<Vec<State>>,
    pub ix: Rebuild,
    lines: Vec<IndexLogLine>,
    persisted: usize,
    active: Vec<Active>,
    /// The chain block the next poll starts from.
    pub last: Hash,
    pub last_daa: u64,
    /// Whether `last_daa` is `last`'s own DAA (from an ingested chain block, a re-anchor, the
    /// caller's start block, or the log line of that block) or only a lower bound (M8 S5).
    last_daa_exact: bool,
    /// Transactions the caller waits for (any transaction, instance or not), and the chain
    /// block and DAA that accepted them (dropped again if that block is removed).
    watched: HashSet<TransactionId>,
    accepted: HashMap<TransactionId, (Hash, u64)>,
    /// The log's re-anchor lines (M8 S4) and how many of them are on disk.
    reanchors: Vec<ReanchorLine>,
    persisted_ra: usize,
}

impl Indexer {
    /// A new index starting at chain block `start` (the chain block before genesis), or the
    /// one in `dir` (resumed at its `last_chain_block`). `dir = None`: in memory only.
    pub fn open(dir: Option<&Path>, t: Arc<Templates>, cov: Hash, genesis: Option<Vec<State>>, start: Hash, start_daa: u64) -> Result<Self, String> {
        let mut me = Self {
            dir: dir.map(Path::to_path_buf),
            ix: Rebuild::new(t.clone(), cov, genesis.clone()),
            t,
            cov,
            genesis,
            lines: vec![],
            persisted: 0,
            active: vec![],
            last: start,
            last_daa: start_daa,
            last_daa_exact: true,
            watched: HashSet::new(),
            accepted: HashMap::new(),
            reanchors: vec![],
            persisted_ra: 0,
        };
        let Some(dir) = dir else { return Ok(me) };
        let log_path = dir.join(LOG_FILE);
        if !log_path.exists() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            return Ok(me);
        }
        let text = fs::read_to_string(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
        let log = IndexLog::from_jsonl(&text)?;
        me.ix = log.rebuild(me.t.clone(), cov, me.genesis.clone())?;
        // Replay the lines once more to know which adds are in effect and which applied.
        let mut probe = Rebuild::new(me.t.clone(), cov, me.genesis.clone());
        for l in &log.lines {
            let a = l.accepted()?;
            match l.kind {
                LogKind::Add => {
                    let applied = probe.apply(&a).map_err(|e| format!("log line {}: {e}", l.seq))?.is_some();
                    me.active.push(Active { txid: a.tx.id(), chain_block: Hash::from_bytes(l.chain_block), chain_daa: l.chain_daa, accepted: a, applied });
                }
                LogKind::Remove => {
                    let i = me.active.iter().rposition(|x| x.txid.as_bytes() == l.txid).ok_or(format!("log line {}: remove of a transaction not in effect", l.seq))?;
                    let x = me.active.remove(i);
                    if x.applied {
                        probe.revert(x.txid)?;
                    }
                }
            }
        }
        me.lines = log.lines;
        me.persisted = me.lines.len();
        me.reanchors = log.reanchors;
        me.persisted_ra = me.reanchors.len();
        let snap_path = dir.join(SNAPSHOT_FILE);
        if snap_path.exists() {
            let s: IndexSnapshot = store::read(&fs::read_to_string(&snap_path).map_err(|e| e.to_string())?, store::INDEX)?;
            if s.cov != cov.as_bytes() {
                return Err(format!("{}: covenant ID {} is not {cov}", snap_path.display(), store::to_hex(&s.cov)));
            }
            if let Some(h) = s.last_chain_block {
                me.last = Hash::from_bytes(h);
            }
            if let Some(d) = s.last_chain_daa {
                me.last_daa = me.last_daa.max(d);
            }
        }
        if let Some(a) = me.active.last() {
            me.last_daa = me.last_daa.max(a.chain_daa);
        }
        // A re-anchor with no add/remove line after it: the resume block is its new block,
        // also when a crash came between the log line and the snapshot (old block saved).
        if let Some(r) = me.reanchors.last() {
            if r.at == me.lines.len() as u64 && (me.last.as_bytes() == r.old_block || me.last.as_bytes() == r.new_block) {
                me.last = Hash::from_bytes(r.new_block);
            }
            me.last_daa = me.last_daa.max(r.new_daa);
        }
        // Exact only when the resume block's own DAA is on record: the chain block of the
        // newest log line, or the newest re-anchor's new block. A snapshot block after those
        // (an empty chain block) has only the snapshot's figure, taken as a lower bound.
        let from_line = me.active.last().is_some_and(|a| a.chain_block == me.last && a.chain_daa == me.last_daa);
        let from_ra = me.reanchors.last().is_some_and(|r| r.new_block == me.last.as_bytes() && r.new_daa == me.last_daa);
        me.last_daa_exact = from_line || from_ra;
        Ok(me)
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Wait list: `accepted_at` reports these once a chain block accepts them.
    pub fn watch(&mut self, txid: TransactionId) {
        self.watched.insert(txid);
    }

    /// The chain block and DAA that accepted `txid`: an instance transaction in effect in
    /// the log, or a watched one seen since `open`.
    pub fn accepted_at(&self, txid: &TransactionId) -> Option<(Hash, u64)> {
        if let Some(a) = self.active.iter().rev().find(|a| &a.txid == txid) {
            return Some((a.chain_block, a.chain_daa));
        }
        self.accepted.get(txid).copied()
    }

    /// The chain block that accepted an instance transaction in effect.
    pub fn chain_block_of(&self, txid: &TransactionId) -> Option<Hash> {
        self.active.iter().rev().find(|a| &a.txid == txid).map(|a| a.chain_block)
    }

    pub fn lines(&self) -> &[IndexLogLine] {
        &self.lines
    }

    pub fn reanchors(&self) -> &[ReanchorLine] {
        &self.reanchors
    }

    /// The re-anchor line from this index's resume block to `new_block` (see `reanchor`).
    pub fn reanchor_line(&self, new_block: Hash, new_daa: u64, node: &str, live_checked: u64, wall_time: u64) -> ReanchorLine {
        ReanchorLine {
            kind: REANCHOR.into(),
            at: self.lines.len() as u64,
            old_block: self.last.as_bytes(),
            old_daa: self.last_daa,
            old_daa_is_lower_bound: !self.last_daa_exact,
            new_block: new_block.as_bytes(),
            new_daa,
            node: node.into(),
            live_checked,
            wall_time,
            argument: REANCHOR_ARGUMENT.into(),
        }
    }

    /// Record a re-anchor (checked by `check_unchanged` on a node, see `reanchor`): the
    /// line goes into the log after every add/remove line so far, the resume block becomes
    /// its new block, and both files are written at once.
    pub fn apply_reanchor(&mut self, r: ReanchorLine) -> Result<(), String> {
        if r.kind != REANCHOR || r.old_block != self.last.as_bytes() || r.at != self.lines.len() as u64 {
            return Err(format!(
                "re-anchor: the line (kind {:?}, old block {}, at {}) does not continue this index (resume block {}, {} lines)",
                r.kind,
                store::to_hex(&r.old_block),
                r.at,
                self.last,
                self.lines.len()
            ));
        }
        if r.new_daa <= self.last_daa {
            return Err(format!("re-anchor: new block DAA {} is not above the resume block's {}", r.new_daa, self.last_daa));
        }
        self.last = Hash::from_bytes(r.new_block);
        self.last_daa = r.new_daa;
        self.last_daa_exact = true;
        self.reanchors.push(r);
        self.persist()
    }

    fn relevant(&self, tx: &Transaction) -> bool {
        tx.outputs.iter().any(|o| o.covenant.is_some_and(|b| b.covenant_id == self.cov)) || tx.inputs.iter().any(|i| self.ix.live().contains_key(&i.previous_outpoint))
    }

    fn push_line(&mut self, kind: LogKind, chain_block: Hash, chain_daa: u64, a: &Accepted) {
        let seq = self.lines.len() as u64;
        self.lines.push(match kind {
            LogKind::Add => IndexLogLine::add(seq, chain_block, chain_daa, a),
            LogKind::Remove => IndexLogLine::remove(seq, chain_block, chain_daa, a),
        });
    }

    /// Apply one batch: removals first (each removed chain block's transactions reverted,
    /// newest first; they must be the newest in effect), then each added chain block's kept
    /// transactions in order. Stops at the first error (nothing of that block is kept as
    /// "done": `last` still names the previous block, so a retry hits the same error).
    pub fn ingest(&mut self, b: ChainBatch) -> Result<Ingested, String> {
        let mut out = Ingested { removed_blocks: b.removed.len(), ..Default::default() };
        if !b.removed.is_empty() {
            let gone: HashSet<Hash> = b.removed.iter().copied().collect();
            self.accepted.retain(|_, (h, _)| !gone.contains(h));
            let n = self.active.iter().filter(|a| gone.contains(&a.chain_block)).count();
            let tail = self.active.len() - n;
            if self.active[tail..].iter().any(|a| !gone.contains(&a.chain_block)) {
                return Err("reorg: the removed chain blocks' transactions are not the newest in the log".into());
            }
            while self.active.len() > tail {
                let a = self.active.pop().expect("tail");
                if a.applied {
                    self.ix.revert(a.txid).map_err(|e| format!("reorg: {e}"))?;
                }
                self.push_line(LogKind::Remove, a.chain_block, a.chain_daa, &a.accepted);
                out.reverted += 1;
            }
        }
        for blk in b.added {
            for ct in blk.txs {
                if !self.relevant(&ct.tx) {
                    let id = ct.tx.id();
                    if self.watched.contains(&id) {
                        self.accepted.insert(id, (blk.hash, blk.daa));
                    }
                    continue;
                }
                let id = ct.tx.id();
                // A crash between appending the log and writing the snapshot replays the
                // same block: an add already in effect for this block is not added twice.
                if self.active.iter().any(|a| a.txid == id && a.chain_block == blk.hash) {
                    continue;
                }
                let a = Accepted { tx: ct.tx, daa: blk.daa, spent: ct.spent };
                let ev = self.ix.apply(&a).map_err(|e| format!("chain block {} (DAA {}): {e}", blk.hash, blk.daa))?;
                self.push_line(LogKind::Add, blk.hash, blk.daa, &a);
                self.active.push(Active { txid: id, chain_block: blk.hash, chain_daa: blk.daa, accepted: a.clone(), applied: ev.is_some() });
                if self.watched.contains(&id) {
                    self.accepted.insert(id, (blk.hash, blk.daa));
                }
                out.kept += 1;
                if let Some(ev) = ev {
                    out.events.push((a, ev));
                }
            }
            self.last = blk.hash;
            self.last_daa = blk.daa;
            self.last_daa_exact = true;
            out.blocks += 1;
        }
        Ok(out)
    }

    /// Append the new log lines and rewrite `index.json` (log first: a crash in between is
    /// repaired by the duplicate check in `ingest`).
    pub fn persist(&mut self) -> Result<(), String> {
        let Some(dir) = self.dir.clone() else { return Ok(()) };
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let log_path = dir.join(LOG_FILE);
        let mut text = String::new();
        if !log_path.exists() {
            text.push_str(&IndexLog::header_line(self.cov)?);
            text.push('\n');
        }
        // Add/remove lines and re-anchor lines in log order (a re-anchor line follows the
        // `at` add/remove lines before it).
        let mut ra = self.reanchors[self.persisted_ra..].iter().peekable();
        for l in &self.lines[self.persisted..] {
            while let Some(r) = ra.next_if(|r| r.at <= l.seq) {
                text.push_str(&r.to_line()?);
                text.push('\n');
            }
            text.push_str(&l.to_line()?);
            text.push('\n');
        }
        for r in ra {
            text.push_str(&r.to_line()?);
            text.push('\n');
        }
        if !text.is_empty() {
            let mut f = OpenOptions::new().create(true).append(true).open(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
            f.write_all(text.as_bytes()).map_err(|e| format!("{}: {e}", log_path.display()))?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        self.persisted = self.lines.len();
        self.persisted_ra = self.reanchors.len();
        let mut snap = IndexSnapshot::of(&self.ix, Some(self.last));
        snap.reanchors = self.reanchors.clone();
        snap.last_chain_daa = Some(self.last_daa);
        write_atomic(&dir.join(SNAPSHOT_FILE), &store::write(&snap, store::INDEX)?)
    }

    /// The oldest round whose earliest buy's accepting chain block is in effect in the log,
    /// as (round, chain block), for the deletion check (A4 pruning evidence).
    pub fn earliest_buy_blocks(&self) -> Vec<(i64, Hash)> {
        self.ix
            .rounds()
            .values()
            .filter_map(|r| {
                let b = r.buys.first()?;
                Some((r.round, self.chain_block_of(&b.txid)?))
            })
            .collect()
    }
}

/// Write `path` through a temporary file and a rename.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// What one `poll` did.
#[derive(Clone, Debug, Default)]
pub struct PollStats {
    pub calls: usize,
    pub blocks: usize,
    pub removed_blocks: usize,
    pub txs_seen: usize,
    pub kept: usize,
    pub reverted: usize,
    pub events: Vec<(Accepted, Event)>,
    /// Inputs whose spent entry the node returned (the Full-verbosity self-check).
    pub spent_entries: usize,
}

/// Poll the node until the index reaches its sink: v2 calls from `last` with Full
/// verbosity, each batch ingested and persisted. A refused start hash (pruned or unknown)
/// is an error that names it; nothing is skipped.
pub async fn poll<C: RpcApi + ?Sized>(c: &C, x: &mut Indexer) -> Result<PollStats, String> {
    let mut st = PollStats::default();
    poll_into(c, x, &mut st).await?;
    Ok(st)
}

/// `poll`, adding to `st`. After an error `st` still holds every batch already ingested and
/// persisted (their events included), so a caller that retries (`campaign::Node`, M8 S4)
/// loses none of them.
pub async fn poll_into<C: RpcApi + ?Sized>(c: &C, x: &mut Indexer, st: &mut PollStats) -> Result<(), String> {
    loop {
        let r = c.get_virtual_chain_from_block_v2(x.last, Some(RpcDataVerbosityLevel::Full), None).await.map_err(|e| {
            let e = e.to_string();
            if transient(&e) {
                format!("get_virtual_chain_from_block_v2 from {} (DAA {}): {e} (node or connection failure; the position is saved)", x.last, x.last_daa)
            } else {
                format!("get_virtual_chain_from_block_v2 from {} (DAA {}): {e}; the resume block is unknown to this node or below its retention root: stopping, nothing skipped", x.last, x.last_daa)
            }
        })?;
        st.calls += 1;
        let b = batch_of(r)?;
        let n_added = b.added.len();
        st.txs_seen += b.added.iter().map(|k| k.txs.len()).sum::<usize>();
        st.spent_entries += b.added.iter().flat_map(|k| &k.txs).map(|t| t.spent.len()).sum::<usize>();
        let g = x.ingest(b)?;
        x.persist()?;
        st.blocks += g.blocks;
        st.removed_blocks += g.removed_blocks;
        st.kept += g.kept;
        st.reverted += g.reverted;
        st.events.extend(g.events);
        // A call from the sink adds nothing: the index has caught up.
        if n_added == 0 {
            return Ok(());
        }
    }
}

/// A node or connection failure worth retrying on another connection (M8 S4: public nodes
/// timed out on long catch-up calls and dropped WebSockets in sitting 3). Anything else
/// (a pruned or unknown resume block, a malformed answer, an index error) is not: the
/// indexer stops on it, nothing skipped.
pub fn transient(e: &str) -> bool {
    let e = e.to_lowercase();
    if e.contains("cannot find header") || e.contains("retention root") {
        return false;
    }
    ["timeout", "timed out", "websocket", "disconnect", "not connected", "connection", "closed", "reset by peer", "broken pipe"].iter().any(|k| e.contains(k))
}

/// The argument a re-anchor line carries (M8 S4, PENDING A4 amendment).
pub const REANCHOR_ARGUMENT: &str = "KIP-20 covenant binding (refs/kips/kip-0020.md:25-27, 33): an output can carry this instance's covenant ID only if its authorizing input spends a UTXO with that ID, or as the genesis of an ID that hashes the spent genesis outpoint (outpoints never repeat). So every instance transaction spends at least one instance UTXO (the indexer keeps exactly the transactions that spend a live instance UTXO or create an instance-bound output, and the latter needs the former). Every live instance UTXO of the index was still unspent on the node, with the same script, value and creation DAA, after the new block was chosen, and the node returned no instance-bound UTXO the index did not know: no instance transaction was accepted between the old and the new block.";

/// A UTXO as the node's utxoindex returns it (`get_utxos_by_addresses`, rusty-kaspa v2.1.0
/// `rpc/core/src/api/rpc.rs:363`; `RpcUtxoEntry.covenant_id`, `rpc/core/src/model/tx.rs:30`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeUtxo {
    pub outpoint: TransactionOutpoint,
    pub amount: u64,
    pub spk: ScriptPublicKey,
    pub daa: u64,
    pub covenant_id: Option<Hash>,
}

impl NodeUtxo {
    pub fn of(e: &kaspa_rpc_core::RpcUtxosByAddressesEntry) -> Self {
        Self {
            outpoint: e.outpoint.clone().into(),
            amount: e.utxo_entry.amount,
            spk: e.utxo_entry.script_public_key.clone(),
            daa: e.utxo_entry.block_daa_score,
            covenant_id: e.utxo_entry.covenant_id,
        }
    }
}

/// What `check_unchanged` saw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unchanged {
    pub live_checked: usize,
    /// Entries the node returned at the queried addresses.
    pub node_entries: usize,
    /// Of those, entries not bound to the instance (anyone may pay to a P2SH address).
    pub foreign: usize,
}

/// The re-anchor proof's check (M8 S4): every live instance UTXO of the index is unspent on
/// the node with the same script, value, creation DAA and covenant ID, and no entry the node
/// returned is bound to `cov` without being known to the index. `node` = the node's entries
/// at the addresses of every live UTXO's script (plus any others queried). Every problem is
/// listed in the error.
pub fn check_unchanged(live: &HashMap<TransactionOutpoint, Live>, cov: Hash, node: &[NodeUtxo]) -> Result<Unchanged, String> {
    let by_op: HashMap<TransactionOutpoint, &NodeUtxo> = node.iter().map(|n| (n.outpoint, n)).collect();
    let mut bad: Vec<String> = vec![];
    let mut ops: Vec<&TransactionOutpoint> = live.keys().collect();
    ops.sort_by_key(|o| (o.transaction_id, o.index));
    for op in ops {
        let l = &live[op];
        match by_op.get(op) {
            None => bad.push(format!("live {op} ({:?}): spent or unknown to the node", l.role)),
            Some(n) => {
                if n.amount != l.value || n.spk != l.spk || n.daa != l.daa || n.covenant_id != Some(cov) {
                    bad.push(format!(
                        "live {op} ({:?}): node (value {}, DAA {}, cov {:?}, script {}) != index (value {}, DAA {}, cov {cov})",
                        l.role,
                        n.amount,
                        n.daa,
                        n.covenant_id,
                        if n.spk == l.spk { "same" } else { "differs" },
                        l.value,
                        l.daa
                    ));
                }
            }
        }
    }
    let mut foreign = 0;
    for n in node {
        if live.contains_key(&n.outpoint) {
            continue;
        }
        if n.covenant_id == Some(cov) {
            bad.push(format!("node UTXO {} (value {}, DAA {}) is bound to the instance but unknown to the index", n.outpoint, n.amount, n.daa));
        } else {
            foreign += 1;
        }
    }
    if !bad.is_empty() {
        let n = bad.len();
        bad.truncate(20);
        return Err(format!("re-anchor refused: {n} problem(s): {}{}", bad.join("; "), if n > 20 { "; ..." } else { "" }));
    }
    Ok(Unchanged { live_checked: live.len(), node_entries: node.len(), foreign })
}

/// Re-anchor the index (M8 S4, PENDING A4 amendment) after its resume block fell below the
/// node's pruning point: (1) the node must no longer know the resume block (else: resume
/// normally); (2) the new block is the node's sink, chosen first; (3) then every live
/// instance UTXO and every entry at their addresses (plus `extra`) is read from the
/// utxoindex and checked by `check_unchanged` (a UTXO spent after (2) is missing, so the
/// check is conservative). `write`: record the line in the index (`apply_reanchor`), else
/// only return it (a read-only check).
pub async fn reanchor<C: RpcApi + ?Sized>(c: &C, x: &mut Indexer, node_url: &str, extra: &[kaspa_addresses::Address], now: u64, write: bool) -> Result<(ReanchorLine, Unchanged), String> {
    match c.get_block(x.last, false).await {
        Ok(b) => return Err(format!("re-anchor refused: the node still knows the resume block {} (DAA {}); resume normally", x.last, b.header.daa_score)),
        // M8 S5: only the node's own "unknown block" answer counts, never a timeout.
        Err(e) if says_block_unknown(x.last, &e.to_string()) => {}
        Err(e) => return Err(format!("re-anchor refused: get_block({}) failed with \"{e}\", which is not the node saying it no longer has the block; retry", x.last)),
    }
    let sink = c.get_block_dag_info().await.map_err(|e| e.to_string())?.sink;
    let new_daa = c.get_block(sink, false).await.map_err(|e| e.to_string())?.header.daa_score;
    let mut addrs: Vec<kaspa_addresses::Address> = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for l in x.ix.live().values() {
        let a = kaspa_txscript::extract_script_pub_key_address(&l.spk, kaspa_addresses::Prefix::Testnet).map_err(|e| format!("live {}: {e}", l.outpoint))?;
        if seen.insert(a.to_string()) {
            addrs.push(a);
        }
    }
    for a in extra {
        if seen.insert(a.to_string()) {
            addrs.push(a.clone());
        }
    }
    if let Some(a) = addrs.iter().find(|a| a.prefix != kaspa_addresses::Prefix::Testnet) {
        return Err(format!("{a} is not a testnet address; refusing"));
    }
    let mut node: Vec<NodeUtxo> = vec![];
    for chunk in addrs.chunks(50) {
        let v = c.get_utxos_by_addresses(chunk.to_vec()).await.map_err(|e| format!("get_utxos_by_addresses: {e}"))?;
        node.extend(v.iter().map(NodeUtxo::of));
    }
    let u = check_unchanged(x.ix.live(), x.cov, &node)?;
    let line = x.reanchor_line(sink, new_daa, node_url, u.live_checked as u64, now);
    if write {
        x.apply_reanchor(line.clone())?;
    }
    Ok((line, u))
}

/// Deletion evidence for block `h` (A4): `Some(what the node returned)` if the node says it
/// no longer has the block or its body, `None` if it serves the body, and an error for any
/// other answer (a timeout, a disconnect, another node error, an inconsistent block): that
/// is not evidence, and the caller stops (M8 S5). See `deletion_evidence`.
pub async fn deleted_block<C: RpcApi + ?Sized>(c: &C, h: Hash) -> Result<Option<String>, String> {
    let a = c.get_block(h, true).await.map(|b| BlockAnswer { daa: b.header.daa_score, txs: b.transactions.len(), header_only: b.verbose_data.as_ref().map(|v| v.is_header_only) }).map_err(|e| e.to_string());
    deletion_evidence(h, a)
}

/// What `get_block(h, true)` returned, reduced to what the deletion check reads.
#[derive(Clone, Copy, Debug)]
pub struct BlockAnswer {
    pub daa: u64,
    pub txs: usize,
    /// `verbose_data.is_header_only`, if the node sent verbose data.
    pub header_only: Option<bool>,
}

/// The node's error text for a block it does not have: `ConsensusError::HeaderNotFound`,
/// `#[error("cannot find header {0}")]` (rusty-kaspa v2.1.0
/// `consensus/core/src/errors/consensus.rs:13-14`), returned by `get_block` through
/// `get_block_even_if_header_only` (`consensus/src/consensus/mod.rs:1427-1432`; RPC
/// `rpc/service/src/service.rs:493-503`, wrapped transparently by `RpcError::ConsensusError`,
/// `rpc/core/src/error.rs:108-109`). Testnet-10 sent it as "RPC Server (remote error) ->
/// cannot find header <hash>" (C8c, 25 Oct).
pub const HEADER_NOT_FOUND: &str = "cannot find header";
/// The node's refusal of a hash below its retention root: `ConsensusError::General("the
/// queried hash does not have retention root on its chain")`
/// (`consensus/src/consensus/mod.rs:858-863`). `get_block` itself does not raise it (it
/// answers `HeaderNotFound` once the header is gone); accepted as the same evidence.
pub const BELOW_RETENTION_ROOT: &str = "the queried hash does not have retention root on its chain";

/// Whether a node error says the node no longer has block `h`: `HEADER_NOT_FOUND` naming
/// `h` itself, or `BELOW_RETENTION_ROOT`.
pub fn says_block_unknown(h: Hash, e: &str) -> bool {
    e.contains(&format!("{HEADER_NOT_FOUND} {h}")) || e.contains(BELOW_RETENTION_ROOT)
}

/// Classify a `get_block(h, true)` answer as deletion evidence (see `deleted_block`).
/// Evidence: the error text `HEADER_NOT_FOUND` naming `h` itself, `BELOW_RETENTION_ROOT`, or
/// a block whose verbose data says `is_header_only` (set from the block status,
/// `rpc/service/src/converter/consensus.rs:78`) with no transactions (the body is pruned,
/// `consensus/src/consensus/mod.rs:1433-1437`). Served: a block with transactions that is not
/// header-only. Anything else is an error, never evidence: a timeout, a disconnect, another
/// node error, or an empty block without the header-only flag (a block always has its
/// coinbase transaction).
pub fn deletion_evidence(h: Hash, a: Result<BlockAnswer, String>) -> Result<Option<String>, String> {
    match a {
        Err(e) if says_block_unknown(h, &e) => Ok(Some(format!("get_block({h}) failed: {e}"))),
        Err(e) => Err(format!("get_block({h}) failed: {e}; not deletion evidence (only \"{HEADER_NOT_FOUND} {h}\", the retention-root refusal or a header-only block count)")),
        Ok(b) if b.header_only == Some(true) && b.txs == 0 => {
            Ok(Some(format!("get_block({h}) returned a header-only block (DAA {}) with no transactions (body pruned)", b.daa)))
        }
        Ok(b) if b.header_only == Some(false) && b.txs > 0 => Ok(None),
        Ok(b) => Err(format!("get_block({h}): inconsistent answer ({} transactions, header-only {:?}); not deletion evidence", b.txs, b.header_only)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::subnets::SubnetworkId;
    use kaspa_consensus_core::tx::{TransactionInput, TransactionOutpoint, TransactionOutput, ScriptPublicKey};

    fn plain_tx(n: u8) -> Transaction {
        let mut tx = Transaction::new(
            1,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_bytes([n; 32]), 0), vec![], 0, 0)],
            vec![TransactionOutput::new(1_000, ScriptPublicKey::from_vec(0, vec![0x51]))],
            0,
            SubnetworkId::default(),
            0,
            vec![],
        );
        tx.finalize();
        tx
    }

    fn block(n: u8, daa: u64, txs: Vec<Transaction>) -> ChainBlock {
        ChainBlock { hash: Hash::from_bytes([n; 32]), daa, txs: txs.into_iter().map(|tx| ChainTx { spent: vec![Spent { amount: 2_000, daa: 1 }; tx.inputs.len()], tx }).collect() }
    }

    /// Foreign transactions are not kept; a watched one is reported with its block, and
    /// forgotten when that block is removed; `last` follows the added blocks.
    #[test]
    fn foreign_and_watched() {
        let t = Templates::get(lottery::buy::b_max());
        let mut x = Indexer::open(None, t, Hash::from_bytes([9; 32]), None, Hash::from_bytes([1; 32]), 10).unwrap();
        let (a, b) = (plain_tx(1), plain_tx(2));
        x.watch(b.id());
        let g = x.ingest(ChainBatch { removed: vec![], added: vec![block(2, 20, vec![a.clone()]), block(3, 30, vec![b.clone()])] }).unwrap();
        assert_eq!((g.blocks, g.kept), (2, 0));
        assert_eq!(x.last, Hash::from_bytes([3; 32]));
        assert_eq!(x.accepted_at(&b.id()), Some((Hash::from_bytes([3; 32]), 30)));
        assert_eq!(x.accepted_at(&a.id()), None, "not watched");
        x.ingest(ChainBatch { removed: vec![Hash::from_bytes([3; 32])], added: vec![block(4, 31, vec![])] }).unwrap();
        assert_eq!(x.accepted_at(&b.id()), None, "its block was removed");
        assert!(x.lines().is_empty());
    }

    // ---- M8 S4 re-anchor

    const COV: [u8; 32] = [9; 32];

    fn live_set() -> HashMap<TransactionOutpoint, Live> {
        (1..=3u8)
            .map(|n| {
                let op = TransactionOutpoint::new(Hash::from_bytes([n; 32]), n as u32);
                let l = Live { outpoint: op, role: lottery::index::Role::Ticket, value: 400_000_000 + n as u64, daa: 598_642_000 + n as u64, spk: ScriptPublicKey::from_vec(0, vec![0xaa, 0x20, n, 0x87]), state: None };
                (op, l)
            })
            .collect()
    }

    fn node_of(live: &HashMap<TransactionOutpoint, Live>) -> Vec<NodeUtxo> {
        live.values().map(|l| NodeUtxo { outpoint: l.outpoint, amount: l.value, spk: l.spk.clone(), daa: l.daa, covenant_id: Some(Hash::from_bytes(COV)) }).collect()
    }

    /// Unchanged live set: accepted (a foreign payment to an instance address is counted,
    /// not refused). One live UTXO spent, one with another DAA, value or script, or an
    /// instance-bound UTXO the index does not know: refused.
    #[test]
    fn reanchor_check_unchanged_missing_changed_unknown() {
        let cov = Hash::from_bytes(COV);
        let live = live_set();
        let mut node = node_of(&live);
        let first = live.values().next().unwrap().clone();
        node.push(NodeUtxo { outpoint: TransactionOutpoint::new(Hash::from_bytes([7; 32]), 0), amount: 1_000, spk: first.spk.clone(), daa: 600_000_000, covenant_id: None });
        let u = check_unchanged(&live, cov, &node).unwrap();
        assert_eq!(u, Unchanged { live_checked: 3, node_entries: 4, foreign: 1 });

        let mut spent = node_of(&live);
        spent.retain(|n| n.outpoint != first.outpoint);
        let e = check_unchanged(&live, cov, &spent).unwrap_err();
        assert!(e.contains("spent or unknown to the node") && e.contains("1 problem"), "{e}");

        for edit in [
            (|n: &mut NodeUtxo| n.daa += 1) as fn(&mut NodeUtxo),
            |n: &mut NodeUtxo| n.amount -= 1,
            |n: &mut NodeUtxo| n.spk = ScriptPublicKey::from_vec(0, vec![0x51]),
            |n: &mut NodeUtxo| n.covenant_id = None,
        ] {
            let mut changed = node_of(&live);
            edit(&mut changed[0]);
            let e = check_unchanged(&live, cov, &changed).unwrap_err();
            assert!(e.contains("!= index"), "{e}");
        }

        let mut unknown = node_of(&live);
        unknown.push(NodeUtxo { outpoint: TransactionOutpoint::new(Hash::from_bytes([8; 32]), 1), amount: 5, spk: first.spk.clone(), daa: 605_000_000, covenant_id: Some(cov) });
        let e = check_unchanged(&live, cov, &unknown).unwrap_err();
        assert!(e.contains("bound to the instance but unknown to the index"), "{e}");
    }

    fn line(x: &Indexer, new: u8, new_daa: u64) -> ReanchorLine {
        ReanchorLine {
            kind: REANCHOR.into(),
            at: x.lines().len() as u64,
            old_block: x.last.as_bytes(),
            old_daa: x.last_daa,
            old_daa_is_lower_bound: false,
            new_block: [new; 32],
            new_daa,
            node: "wss://example.invalid/testnet-10".into(),
            live_checked: 3,
            wall_time: 1_792_900_000,
            argument: REANCHOR_ARGUMENT.into(),
        }
    }

    /// M8 S5: a re-anchor line says whether its `old_daa` is the old block's own DAA (known
    /// from an ingested chain block or a re-anchor) or only a lower bound (an index reopened
    /// from disk whose resume block is not the chain block of its newest logged line).
    #[test]
    fn reanchor_line_says_whether_old_daa_is_exact() {
        let dir = std::env::temp_dir().join(format!("tn10-reanchor-bound-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let t = Templates::get(lottery::buy::b_max());
        let cov = Hash::from_bytes(COV);
        let flag = |x: &Indexer| x.reanchor_line(Hash::from_bytes([60; 32]), 9_000, "n", 0, 0).to_line().unwrap();
        let mut x = Indexer::open(Some(&dir), t.clone(), cov, None, Hash::from_bytes([1; 32]), 10).unwrap();
        x.ingest(ChainBatch { removed: vec![], added: vec![block(2, 20, vec![plain_tx(1)])] }).unwrap();
        x.persist().unwrap();
        assert!(flag(&x).contains(r#""old_daa":20,"#) && flag(&x).contains(r#""old_daa_is_lower_bound":false"#), "{}", flag(&x));
        // Reopened: the transaction was foreign (not logged), so only the snapshot's figure
        // is on record for the resume block: a lower bound.
        let y = Indexer::open(Some(&dir), t.clone(), cov, None, Hash::from_bytes([1; 32]), 10).unwrap();
        assert_eq!((y.last, y.last_daa), (Hash::from_bytes([2; 32]), 20));
        assert!(flag(&y).contains(r#""old_daa_is_lower_bound":true"#), "{}", flag(&y));
        // An empty chain block after it: in process the DAA is the block's own ...
        x.ingest(ChainBatch { removed: vec![], added: vec![block(3, 30, vec![])] }).unwrap();
        x.persist().unwrap();
        assert!(flag(&x).contains(r#""old_daa":30,"#) && flag(&x).contains(r#""old_daa_is_lower_bound":false"#), "{}", flag(&x));
        // ... but an index reopened from disk only has a lower bound for it.
        let z = Indexer::open(Some(&dir), t.clone(), cov, None, Hash::from_bytes([1; 32]), 10).unwrap();
        assert_eq!(z.last, Hash::from_bytes([3; 32]));
        assert!(flag(&z).contains(r#""old_daa_is_lower_bound":true"#), "{}", flag(&z));
        // After a re-anchor the new block's DAA is on record (in process and reopened).
        let mut w = z;
        w.apply_reanchor(line(&w, 50, 5_000)).unwrap();
        assert!(flag(&w).contains(r#""old_daa":5000,"#) && flag(&w).contains(r#""old_daa_is_lower_bound":false"#), "{}", flag(&w));
        let v = Indexer::open(Some(&dir), t.clone(), cov, None, Hash::from_bytes([1; 32]), 10).unwrap();
        assert!(flag(&v).contains(r#""old_daa_is_lower_bound":false"#), "{}", flag(&v));
        let _ = fs::remove_dir_all(&dir);
    }

    /// The re-anchor line is written to both files; a reopened index resumes from the new
    /// block (also when the snapshot still names the old one, a crash between the two
    /// writes); a line that does not continue the index is refused.
    #[test]
    fn reanchor_persists_and_resumes_from_the_new_block() {
        let dir = std::env::temp_dir().join(format!("tn10-reanchor-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let t = Templates::get(lottery::buy::b_max());
        let cov = Hash::from_bytes(COV);
        let mut x = Indexer::open(Some(&dir), t.clone(), cov, None, Hash::from_bytes([1; 32]), 10).unwrap();
        x.ingest(ChainBatch { removed: vec![], added: vec![block(2, 20, vec![plain_tx(1)])] }).unwrap();
        x.persist().unwrap();
        assert!(x.apply_reanchor(ReanchorLine { old_block: [5; 32], ..line(&x, 50, 5_000) }).unwrap_err().contains("does not continue"));
        assert!(x.apply_reanchor(line(&x, 50, 20)).unwrap_err().contains("not above"));
        x.apply_reanchor(line(&x, 50, 5_000)).unwrap();
        assert_eq!((x.last, x.last_daa), (Hash::from_bytes([50; 32]), 5_000));
        let text = fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(text.lines().nth(1).unwrap().starts_with("{\"kind\":\"reanchor\""), "{text}");
        let snap: IndexSnapshot = store::read(&fs::read_to_string(dir.join(SNAPSHOT_FILE)).unwrap(), store::INDEX).unwrap();
        assert_eq!((snap.last_chain_block, snap.last_chain_daa, snap.reanchors.len()), (Some([50; 32]), Some(5_000), 1));
        let y = Indexer::open(Some(&dir), t.clone(), cov, None, Hash::from_bytes([1; 32]), 10).unwrap();
        assert_eq!((y.last, y.last_daa, y.reanchors().len()), (Hash::from_bytes([50; 32]), 5_000, 1));
        // A crash after the log line, before the snapshot: the snapshot names the old block.
        let mut old = snap.clone();
        old.last_chain_block = Some([2; 32]);
        old.last_chain_daa = Some(20);
        old.reanchors.clear();
        write_atomic(&dir.join(SNAPSHOT_FILE), &store::write(&old, store::INDEX).unwrap()).unwrap();
        let z = Indexer::open(Some(&dir), t, cov, None, Hash::from_bytes([1; 32]), 10).unwrap();
        assert_eq!((z.last, z.last_daa), (Hash::from_bytes([50; 32]), 5_000));
        let _ = fs::remove_dir_all(&dir);
    }

    /// M8 S5: only the node's own answers for a deleted block count as deletion evidence:
    /// "cannot find header <h>" (ConsensusError::HeaderNotFound), the retention-root refusal,
    /// or an explicit header-only block. A timeout, a disconnect or another error is not
    /// evidence (an error: the caller stops), and neither is an inconsistent answer.
    #[test]
    fn deletion_evidence_needs_the_nodes_answer() {
        let h = Hash::from_bytes([0xe0; 32]);
        let other = Hash::from_bytes([0x11; 32]);
        // Not evidence: connection failures, other errors, the header text for another hash.
        for e in [
            "RPC request timeout".to_string(),
            "WebSocket disconnected".to_string(),
            "not connected".to_string(),
            "websocket error: Connection reset by peer".to_string(),
            "RPC Server (remote error) -> cannot find full block 1111".to_string(),
            format!("RPC Server (remote error) -> cannot find header {other}"),
            "cannot find header".to_string(),
        ] {
            let r = deletion_evidence(h, Err(e.clone()));
            assert!(r.is_err(), "{e:?} counted as deletion evidence: {r:?}");
        }
        // The real testnet-10 text (C8c, 25 Oct; campaign.json of instance e9f4...).
        let real = format!("RPC Server (remote error) -> cannot find header {h}");
        assert!(deletion_evidence(h, Err(real)).unwrap().unwrap().contains("cannot find header"));
        let ret = "RPC Server (remote error) -> the queried hash does not have retention root on its chain".to_string();
        assert!(deletion_evidence(h, Err(ret)).unwrap().is_some());
        let ho = BlockAnswer { daa: 605_000_000, txs: 0, header_only: Some(true) };
        assert!(deletion_evidence(h, Ok(ho)).unwrap().unwrap().contains("header-only"));
        // Served: a full block.
        assert_eq!(deletion_evidence(h, Ok(BlockAnswer { daa: 1, txs: 3, header_only: Some(false) })).unwrap(), None);
        // Not evidence: no transactions without the explicit header-only flag (a block always
        // has its coinbase), or a body served with the flag set.
        for a in [BlockAnswer { daa: 1, txs: 0, header_only: None }, BlockAnswer { daa: 1, txs: 0, header_only: Some(false) }, BlockAnswer { daa: 1, txs: 2, header_only: Some(true) }] {
            assert!(deletion_evidence(h, Ok(a)).is_err(), "{a:?}");
        }
    }

    #[test]
    fn transient_node_failures_are_retried_pruning_is_not() {
        for e in ["RPC request timeout", "WebSocket disconnected", "websocket error: Connection reset by peer", "not connected", "connection closed"] {
            assert!(transient(e), "{e}");
        }
        for e in ["RPC Server (remote error) -> cannot find header e0a6", "the queried hash does not have retention root on its chain", "v2: 3 added chain blocks but 2 acceptance entries", "log line 4: rebuilt state differs"] {
            assert!(!transient(e), "{e}");
        }
    }
}
