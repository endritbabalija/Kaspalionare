//! The served state: the M8 index (`index/1`) rebuilt from the log an indexer writes, and
//! followed as the log grows.
//!
//! The source of truth is `index-log.jsonl` (an append-only log: a header line, then one
//! add or remove line per accepted transaction, and re-anchor lines; `lottery::store`). The
//! follower reads it from the start, applies each add line to a `lottery::index::Rebuild`
//! (each rebuilt state checked against the transaction's outputs, as the indexer itself
//! does) and reverts on each remove line. Then it polls the file: new complete lines are
//! applied; a file that shrank (rewritten) is read again from the start. A line that does
//! not apply stops the follower at that line: the last good state stays served and
//! `/v1/status` reports the error. `index.json` (the indexer's snapshot) is read only for
//! its `last_chain_block` and `last_chain_daa` (how far the indexer has followed).
//!
//! The genesis states come from the launch bundle (`bundle/1`, verified locally with
//! `lottery::bundle::verify`), whose covenant ID must be the configured one. The server
//! writes nothing anywhere.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash;
use lottery::buy::Templates;
use lottery::draw::RoundResult;
use lottery::index::{Event, Rebuild, Role, State};
use lottery::state::{MarkerState, TicketState};
use lottery::store::{self, IndexLogHeader, IndexLogLine, LogKind, REANCHOR};
use serde::Deserialize;
use web::convert::Net;

pub const LOG_FILE: &str = "index-log.jsonl";
pub const SNAPSHOT_FILE: &str = "index.json";
/// The default bundle file name in the store directory (what `tn10` writes).
pub const BUNDLE_FILE: &str = "bundle.json";

/// A live covenant UTXO with a known state.
#[derive(Clone, Debug)]
pub struct LiveObj {
    pub role: Role,
    pub outpoint: TransactionOutpoint,
    pub value: u64,
    pub daa: u64,
    pub spk: ScriptPublicKey,
    pub state: State,
    pub state_bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct BuySnap {
    pub txid: TransactionId,
    pub shard_index: i64,
    pub combos: Vec<[u8; 6]>,
    pub ticket_index: u32,
    pub ticket: TicketState,
    pub referral: u64,
}

#[derive(Clone, Debug)]
pub struct CloseSnap {
    pub txid: TransactionId,
    pub marker_daa: u64,
    pub marker: MarkerState,
    pub frozen_roots: Vec<[u8; 32]>,
    pub m_next: i64,
    /// T and seed_offset of the round's controller as `close()` spent it.
    pub deadline: Option<i64>,
    pub seed_offset: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct DrawSnap {
    pub txid: TransactionId,
    pub result: RoundResult,
    /// The 272-byte anchor proof argument of the draw (empty bytes for a void's).
    pub anchor: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct RoundSnap {
    pub round: i64,
    pub m: Option<i64>,
    pub k: usize,
    pub buys: Vec<BuySnap>,
    pub close: Option<CloseSnap>,
    pub draw: Option<DrawSnap>,
    pub incomplete: Option<String>,
    pub gaps: Vec<String>,
    /// Shards the index cannot follow (a missed purchase), with the reason.
    pub broken: BTreeMap<i64, String>,
}

impl RoundSnap {
    /// Shard `i`'s purchases, in its purchase order.
    pub fn purchases(&self, i: i64) -> Vec<[u8; 6]> {
        self.buys.iter().filter(|b| b.shard_index == i).flat_map(|b| b.combos.iter().copied()).collect()
    }
}

/// What the routes read: an immutable picture of the index, replaced whole on each change.
#[derive(Clone)]
pub struct Snapshot {
    pub cov: Hash,
    pub net: Net,
    pub t: Arc<Templates>,
    /// The bundle file's bytes, served verbatim (the page hashes them).
    pub bundle: Arc<Vec<u8>>,
    pub rounds: BTreeMap<i64, RoundSnap>,
    /// Sorted by outpoint.
    pub live: Vec<LiveObj>,
    pub gaps: Vec<String>,
    pub log_lines: u64,
    pub last_chain: Option<([u8; 32], u64)>,
    pub error: Option<String>,
    pub updated_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// The fields of `index.json` the status reads (serde skips the rest).
#[derive(Deserialize)]
struct SnapHead {
    format: String,
    cov: String,
    last_chain_block: Option<String>,
    #[serde(default)]
    last_chain_daa: Option<u64>,
}

/// The bundle at `path`, verified, for instance `cov`: (bytes, network, templates, genesis
/// states).
pub fn read_bundle(path: &Path, cov: Hash) -> Result<(Vec<u8>, Net, Arc<Templates>, Vec<State>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() > web::indexer::LIMITS.instance as usize {
        return Err(format!("{}: {} bytes, above the instance response cap", path.display(), bytes.len()));
    }
    let text = std::str::from_utf8(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let b: lottery::bundle::Bundle = store::read(text, store::BUNDLE).map_err(|e| format!("{}: {e}", path.display()))?;
    let net = Net::parse(&b.network).map_err(|e| format!("{}: {e}", path.display()))?;
    let v = lottery::bundle::verify(&b).map_err(|e| format!("{}: bundle verify: {e}", path.display()))?;
    if v.cov != cov {
        return Err(format!("{}: the bundle is instance {}, not the configured {cov}", path.display(), v.cov));
    }
    Ok((bytes, net, v.templates, v.genesis_states))
}

/// The index of one instance, following its log.
pub struct Follower {
    dir: PathBuf,
    cov: Hash,
    net: Net,
    t: Arc<Templates>,
    genesis: Vec<State>,
    bundle: Arc<Vec<u8>>,
    ix: Rebuild,
    /// Bytes of the log consumed (up to the last complete line applied).
    offset: u64,
    header: bool,
    lines: u64,
    foreign: Vec<[u8; 32]>,
    closes: HashMap<TransactionId, (i64, i64)>,
    anchors: HashMap<TransactionId, Vec<u8>>,
    snap_mtime: Option<SystemTime>,
    last_chain: Option<([u8; 32], u64)>,
    error: Option<String>,
    /// The log length at the error (retried only when the file changes).
    stuck_at: Option<u64>,
}

impl Follower {
    /// The index in `dir` (its `index-log.jsonl`) of the instance the bundle at `bundle`
    /// describes, which must be `cov`; read in full.
    pub fn open(dir: &Path, bundle: &Path, cov: Hash) -> Result<Self, String> {
        let (bytes, net, t, genesis) = read_bundle(bundle, cov)?;
        let mut f = Follower {
            dir: dir.to_path_buf(),
            cov,
            net,
            ix: Rebuild::new(t.clone(), cov, Some(genesis.clone())),
            t,
            genesis,
            bundle: Arc::new(bytes),
            offset: 0,
            header: false,
            lines: 0,
            foreign: vec![],
            closes: HashMap::new(),
            anchors: HashMap::new(),
            snap_mtime: None,
            last_chain: None,
            error: None,
            stuck_at: None,
        };
        let log = f.dir.join(LOG_FILE);
        if !log.exists() {
            return Err(format!("{}: no index log (start the indexer first)", log.display()));
        }
        f.poll()?;
        if let Some(e) = &f.error {
            return Err(format!("{}: {e}", log.display()));
        }
        Ok(f)
    }

    fn reset(&mut self) {
        self.ix = Rebuild::new(self.t.clone(), self.cov, Some(self.genesis.clone()));
        self.offset = 0;
        self.header = false;
        self.lines = 0;
        self.foreign.clear();
        self.error = None;
        self.stuck_at = None;
    }

    /// Read what the files gained since the last poll. Returns whether anything changed.
    pub fn poll(&mut self) -> Result<bool, String> {
        let path = self.dir.join(LOG_FILE);
        let len = std::fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?.len();
        let mut changed = self.read_snapshot_head();
        if len < self.offset {
            // Rewritten: read again from the start.
            self.reset();
            changed = true;
        }
        if self.stuck_at == Some(len) {
            return Ok(changed);
        }
        if len > self.offset {
            let mut file = std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            file.seek(SeekFrom::Start(self.offset)).map_err(|e| e.to_string())?;
            let mut buf = Vec::with_capacity((len - self.offset) as usize);
            file.take(len - self.offset).read_to_end(&mut buf).map_err(|e| e.to_string())?;
            // Complete lines only (the writer may be in the middle of one).
            let Some(end) = buf.iter().rposition(|&b| b == b'\n') else { return Ok(changed) };
            let text = std::str::from_utf8(&buf[..=end]).map_err(|e| format!("{}: {e}", path.display()))?;
            let mut consumed = 0usize;
            for line in text.split_inclusive('\n') {
                let l = line.trim();
                if !l.is_empty() {
                    if let Err(e) = self.apply_line(l) {
                        self.error = Some(format!("log line {} (byte {}): {e}", self.lines, self.offset as usize + consumed));
                        self.stuck_at = Some(len);
                        self.offset += consumed as u64;
                        return Ok(true);
                    }
                }
                consumed += line.len();
                changed = true;
            }
            self.offset += consumed as u64;
            self.error = None;
            self.stuck_at = None;
        }
        Ok(changed)
    }

    fn read_snapshot_head(&mut self) -> bool {
        let path = self.dir.join(SNAPSHOT_FILE);
        let Ok(mtime) = std::fs::metadata(&path).and_then(|m| m.modified()) else { return false };
        if self.snap_mtime == Some(mtime) {
            return false;
        }
        self.snap_mtime = Some(mtime);
        let Ok(text) = std::fs::read_to_string(&path) else { return false };
        let Ok(h) = serde_json::from_str::<SnapHead>(&text) else { return false };
        if h.format != store::INDEX || h.cov != store::to_hex(&self.cov.as_bytes()) {
            return false;
        }
        let block = h.last_chain_block.and_then(|b| store::from_hex(&b).ok()).and_then(|b| <[u8; 32]>::try_from(b).ok());
        let next = block.map(|b| (b, h.last_chain_daa.unwrap_or(0)));
        let changed = next != self.last_chain;
        self.last_chain = next;
        changed
    }

    fn apply_line(&mut self, line: &str) -> Result<(), String> {
        let v: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if !self.header {
            let h: IndexLogHeader = serde_json::from_value(v).map_err(|e| format!("header: {e}"))?;
            if h.format != store::INDEX {
                return Err(format!("header: format {:?}, expected {:?}", h.format, store::INDEX));
            }
            if h.cov != self.cov.as_bytes() {
                return Err(format!("header: covenant ID {} is not the configured {}", store::to_hex(&h.cov), self.cov));
            }
            self.header = true;
            return Ok(());
        }
        if v.get("kind").and_then(serde_json::Value::as_str) == Some(REANCHOR) {
            // A re-anchor line changes no index state (M8 S4).
            return Ok(());
        }
        store::check_safe(&v, "$")?;
        let l: IndexLogLine = serde_json::from_value(v).map_err(|e| e.to_string())?;
        if l.seq != self.lines {
            return Err(format!("seq {} where {} is expected", l.seq, self.lines));
        }
        match l.kind {
            LogKind::Add => {
                let a = l.accepted()?;
                match self.ix.apply(&a)? {
                    None => self.foreign.push(l.txid),
                    Some(Event::Close(c)) => {
                        self.closes.insert(a.tx.id(), (c.controller.deadline, c.controller.seed_offset));
                    }
                    Some(Event::Draw(d)) => {
                        if !d.void() {
                            self.anchors.insert(a.tx.id(), d.anchor.clone());
                        }
                    }
                    Some(_) => {}
                }
            }
            LogKind::Remove => {
                if let Some(i) = self.foreign.iter().rposition(|x| *x == l.txid) {
                    self.foreign.remove(i);
                } else {
                    self.ix.revert(Hash::from_bytes(l.txid))?;
                }
            }
        }
        self.lines += 1;
        Ok(())
    }

    /// The current picture of the index.
    pub fn snapshot(&self) -> Snapshot {
        let t = &self.t;
        let rounds = self
            .ix
            .rounds()
            .iter()
            .map(|(&r, x)| {
                let close = x.close.as_ref().map(|c| {
                    let (deadline, seed_offset) = self.closes.get(&c.txid).map(|&(d, s)| (Some(d), Some(s))).unwrap_or((None, None));
                    CloseSnap { txid: c.txid, marker_daa: c.marker_daa, marker: c.marker.clone(), frozen_roots: c.frozen_roots.clone(), m_next: c.m_next, deadline, seed_offset }
                });
                let draw = x.draw.as_ref().map(|d| DrawSnap { txid: d.txid, result: d.result.clone(), anchor: self.anchors.get(&d.txid).cloned() });
                let buys = x
                    .buys
                    .iter()
                    .map(|b| BuySnap {
                        txid: b.txid,
                        shard_index: b.shard_index,
                        combos: b.combos.clone(),
                        ticket_index: b.ticket_outpoint.index,
                        ticket: b.ticket.clone(),
                        referral: b.referral,
                    })
                    .collect();
                let broken = x.shards.iter().filter_map(|(&i, s)| s.broken.clone().map(|b| (i, b))).collect();
                (r, RoundSnap { round: r, m: x.m, k: x.shards.len(), buys, close, draw, incomplete: x.incomplete.clone(), gaps: x.gaps.clone(), broken })
            })
            .collect();
        let mut live: Vec<LiveObj> = self
            .ix
            .live()
            .values()
            .filter_map(|l| {
                let s = l.state.clone()?;
                let state_bytes = s.try_state_bytes(t).ok()?;
                Some(LiveObj { role: l.role, outpoint: l.outpoint, value: l.value, daa: l.daa, spk: l.spk.clone(), state: s, state_bytes })
            })
            .collect();
        live.sort_by_key(|l| (l.outpoint.transaction_id.as_bytes(), l.outpoint.index));
        Snapshot {
            cov: self.cov,
            net: self.net,
            t: self.t.clone(),
            bundle: self.bundle.clone(),
            rounds,
            live,
            gaps: self.ix.gaps().to_vec(),
            log_lines: self.lines,
            last_chain: self.last_chain,
            error: self.error.clone(),
            updated_ms: now_ms(),
        }
    }
}
