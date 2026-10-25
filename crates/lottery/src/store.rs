//! Spec M8 A3 (D121): the off-chain store formats and the stores the offline builders read.
//!
//! - Every file is JSON (`serde_json`) whose first field is `"format": "<name>/1"`; a reader
//!   refuses any other format. Byte fields are lowercase hex (uppercase is refused); integers
//!   are JSON numbers, and every number in a file must be an integer of magnitude below 2^53
//!   (checked on write and on read, so a JavaScript reader loses nothing).
//! - `index/1`: the indexer's append-only log (`IndexLog`, `index-log.jsonl`: a header line,
//!   then one `IndexLogLine` per added or removed accepted transaction) and the snapshot
//!   rebuilt from it (`IndexSnapshot`, `index.json`). M8 S2 amendment to A3: each log line
//!   also carries the amount and creation DAA score of every spent entry (`spent`), which
//!   `close()`'s next deadline and the rebuilt successor values need and the transaction
//!   does not contain. M8 S2c amendment (review MF-C): the line's transaction is the hex of
//!   its Borsh serialization and each spent amount a decimal string, so a foreign u64 at or
//!   above 2^53 (a funding input's `sequence`, a foreign UTXO's value) never makes a line
//!   unwritable; the lottery's own numbers keep the 2^53 rule.
//! - `wallet/1`: one round's purchases as the buyer saw them (`Wallet`): the leaf updates
//!   per shard with the root after each (of every chain-accepted buy, the runner's and
//!   third parties', S2c), the frozen roots, the runner's own tickets, the round result and
//!   the live pools the wallet knows. It never reads `index/1`.
//! - `campaign/1`: the testnet campaign runner's state (`Campaign`).
//! - `bundle/1`: the launch bundle, in `crate::bundle`.
//!
//! `ClaimStore` is what `ClaimCase::from_store` reads; `Rebuild` (an index) and `Wallet`
//! implement it. Neither is trusted: what is built from them is checked on chain against
//! the frozen roots (D115).

use std::collections::BTreeMap;
use std::sync::Arc;

use harness::{Hash, ScriptPublicKey, Transaction, TransactionOutpoint};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::buy::Templates;
use crate::combo::Combo;
use crate::draw::{ResultPool, ResultRecord, RoundResult};
use crate::index::{Accepted, Rebuild, Role, Spent, State, parse_buy, parse_claim};
use crate::smt::Tree;
use crate::state::{Payout, PoolState, TicketState};

/// The largest integer a store file may hold: 2^53 - 1 (D121).
pub const SAFE_MAX: u64 = (1 << 53) - 1;

pub const INDEX: &str = "index/1";
pub const WALLET: &str = "wallet/1";
pub const CAMPAIGN: &str = "campaign/1";
pub const BUNDLE: &str = "bundle/1";

// ------------------------------------------------------------------------------ hex

/// Lowercase hex.
pub fn to_hex(b: &[u8]) -> String {
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(2 * b.len());
    for x in b {
        s.push(D[(x >> 4) as usize] as char);
        s.push(D[(x & 15) as usize] as char);
    }
    s
}

/// Lowercase hex only (D121); an odd length or any other character is an error.
pub fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    let nib = |c: u8| match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        _ => Err(format!("hex: {:?} is not a lowercase hex digit", c as char)),
    };
    let b = s.as_bytes();
    if b.len() % 2 != 0 {
        return Err(format!("hex: odd length {}", b.len()));
    }
    b.chunks(2).map(|p| Ok(nib(p[0])? << 4 | nib(p[1])?)).collect()
}

/// `#[serde(with = "hex")]` for `Vec<u8>` and `[u8; N]`.
pub mod hex {
    use super::*;
    use serde::de::Error as _;
    pub fn serialize<S: Serializer, T: AsRef<[u8]>>(v: &T, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&to_hex(v.as_ref()))
    }
    pub fn deserialize<'de, D: Deserializer<'de>, T: TryFrom<Vec<u8>>>(d: D) -> Result<T, D::Error> {
        let v = from_hex(&String::deserialize(d)?).map_err(D::Error::custom)?;
        let n = v.len();
        T::try_from(v).map_err(|_| D::Error::custom(format!("hex: wrong length {n}")))
    }
}

/// `#[serde(with = "hex_vec")]` for a list of byte strings.
pub mod hex_vec {
    use super::*;
    use serde::de::Error as _;
    #[allow(clippy::ptr_arg)]
    pub fn serialize<S: Serializer, T: AsRef<[u8]>>(v: &Vec<T>, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(v.iter().map(|x| to_hex(x.as_ref())))
    }
    pub fn deserialize<'de, D: Deserializer<'de>, T: TryFrom<Vec<u8>>>(d: D) -> Result<Vec<T>, D::Error> {
        Vec::<String>::deserialize(d)?
            .iter()
            .map(|s| {
                let v = from_hex(s).map_err(D::Error::custom)?;
                let n = v.len();
                T::try_from(v).map_err(|_| D::Error::custom(format!("hex: wrong length {n}")))
            })
            .collect()
    }
}

/// `#[serde(with = "hex_opt")]` for an optional byte string.
pub mod hex_opt {
    use super::*;
    use serde::de::Error as _;
    pub fn serialize<S: Serializer, T: AsRef<[u8]>>(v: &Option<T>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(x) => s.serialize_some(&to_hex(x.as_ref())),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>, T: TryFrom<Vec<u8>>>(d: D) -> Result<Option<T>, D::Error> {
        match Option::<String>::deserialize(d)? {
            None => Ok(None),
            Some(s) => {
                let v = from_hex(&s).map_err(D::Error::custom)?;
                let n = v.len();
                T::try_from(v).map(Some).map_err(|_| D::Error::custom(format!("hex: wrong length {n}")))
            }
        }
    }
}

/// `#[serde(with = "tx_borsh_hex")]` (M8 S2c, review MF-C, A3 amendment): a transaction as
/// the lowercase hex of its Borsh serialization under the pinned rusty-kaspa v2.1.0
/// (`kaspa_consensus_core::tx::Transaction` derives `BorshSerialize`/`BorshDeserialize`,
/// `consensus/core/src/tx.rs:253`; every field in declaration order, the cached `id`
/// included). Reversible byte for byte, and a JSON string has no 2^53 limit, so a foreign
/// input's `sequence` or a foreign output's value at any u64 is written. Reading it back
/// does not recompute the id: `checked_id` does (MF-B).
pub mod tx_borsh_hex {
    use super::*;
    use serde::de::Error as _;
    pub fn serialize<S: Serializer>(tx: &Transaction, s: S) -> Result<S::Ok, S::Error> {
        let b = borsh::to_vec(tx).map_err(serde::ser::Error::custom)?;
        s.serialize_str(&to_hex(&b))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Transaction, D::Error> {
        let b = from_hex(&String::deserialize(d)?).map_err(D::Error::custom)?;
        borsh::from_slice(&b).map_err(|e| D::Error::custom(format!("tx: not a Borsh transaction: {e}")))
    }
}

/// `#[serde(with = "u64_dec")]` (M8 S2c, review MF-C): a u64 that is not the lottery's own
/// (a foreign spent amount) as a decimal string, so it has no 2^53 limit. Digits only, no
/// sign, no leading zero (except "0"), at most u64::MAX.
pub mod u64_dec {
    use super::*;
    use serde::de::Error as _;
    pub fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        let t = String::deserialize(d)?;
        let canonical = !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit()) && (t == "0" || !t.starts_with('0'));
        if !canonical {
            return Err(D::Error::custom(format!("{t:?} is not a canonical decimal u64")));
        }
        t.parse().map_err(|e| D::Error::custom(format!("{t:?}: {e}")))
    }
}

// ------------------------------------------------------------------------------ files

/// Every number in `v` is an integer of magnitude at most 2^53 - 1 (D121).
pub fn check_safe(v: &Value, path: &str) -> Result<(), String> {
    match v {
        Value::Number(n) => {
            let ok = match (n.as_u64(), n.as_i64()) {
                (Some(u), _) => u <= SAFE_MAX,
                (None, Some(i)) => i >= -(SAFE_MAX as i64),
                _ => false,
            };
            if ok { Ok(()) } else { Err(format!("{path}: {n} is not an integer below 2^53")) }
        }
        Value::Array(a) => a.iter().enumerate().try_for_each(|(i, x)| check_safe(x, &format!("{path}[{i}]"))),
        Value::Object(o) => o.iter().try_for_each(|(k, x)| check_safe(x, &format!("{path}.{k}"))),
        _ => Ok(()),
    }
}

fn check_format(v: &Value, want: &str) -> Result<(), String> {
    match v.get("format").and_then(Value::as_str) {
        Some(f) if f == want => Ok(()),
        Some(f) => Err(format!("unknown format {f:?} (expected {want:?})")),
        None => Err(format!("no format field (expected {want:?})")),
    }
}

/// Serialize a store file of format `format` (its `format` field must say so); refused if
/// any number is not an integer below 2^53. Fields keep their declaration order, so
/// `format` comes first.
pub fn write<T: Serialize>(x: &T, format: &str) -> Result<String, String> {
    let v = serde_json::to_value(x).map_err(|e| e.to_string())?;
    check_format(&v, format)?;
    check_safe(&v, "$")?;
    serde_json::to_string_pretty(x).map_err(|e| e.to_string())
}

/// One JSON line (the index log), with the same checks as `write`.
fn write_line<T: Serialize>(x: &T) -> Result<String, String> {
    let v = serde_json::to_value(x).map_err(|e| e.to_string())?;
    check_safe(&v, "$")?;
    serde_json::to_string(x).map_err(|e| e.to_string())
}

/// Parse a store file; refused if its format is not `format` or a number is not an
/// integer below 2^53.
pub fn read<T: DeserializeOwned>(s: &str, format: &str) -> Result<T, String> {
    let v: Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
    check_format(&v, format)?;
    check_safe(&v, "$")?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

fn read_line<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    let v: Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
    check_safe(&v, "$")?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------------------ shared

/// M8 S2c (review MF-B): the id of a transaction read from a file, recomputed. serde (and
/// Borsh) read a `Transaction`'s `id` field verbatim and do not recompute it (rusty-kaspa
/// v2.1.0 `consensus/core/src/tx/serde_impl.rs:288-291, 401-402`), so the stored id proves
/// nothing about the stored fields. The consensus id is
/// `kaspa_consensus_core::hashing::tx::id` (`consensus/core/src/hashing/tx.rs:34-36`: v0 or
/// v1 by the version), what `Transaction::finalize` stores. It does not cover the
/// signature scripts (`TxEncodingFlags::EXCLUDE_SIGNATURE_SCRIPT`, `hashing/tx.rs:44-48,
/// 234`): the index checks those by rebuilding (each redeem against the spent UTXO's script,
/// each successor against its output script).
pub fn checked_id(tx: &Transaction) -> Result<Hash, String> {
    let id = kaspa_consensus_core::hashing::tx::id(tx);
    if id != tx.id() {
        return Err(format!("the stored transaction id {} is not its recomputed id {id}", tx.id()));
    }
    Ok(id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutpointJson {
    #[serde(with = "hex")]
    pub txid: [u8; 32],
    pub index: u32,
}

impl From<TransactionOutpoint> for OutpointJson {
    fn from(o: TransactionOutpoint) -> Self {
        Self { txid: o.transaction_id.as_bytes(), index: o.index }
    }
}

impl OutpointJson {
    pub fn outpoint(&self) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash::from_bytes(self.txid), self.index)
    }
}

/// A funding UTXO of an offline build (spec M8 A4): its outpoint, value and creation DAA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Funding {
    pub outpoint: TransactionOutpoint,
    pub value: u64,
    pub daa: u64,
}

/// Who pays for an offline build (spec M8 A4; S2c should-fix 2): the secret key of the
/// funding inputs (it signs them, and the change returns to it) and the fee in sompi
/// (`buy::fee_for`). A required argument of every `from_store` builder, so none falls back
/// to the builders' defaults (the public test key `buy::FUNDING_SK` = [7; 32] and a fixed
/// fee). No `Debug`: it holds a secret key.
#[derive(Clone, Copy)]
pub struct Payer {
    pub funding_sk: [u8; 32],
    pub fee: u64,
}

/// A round result (`RoundResult`) as stored in `index/1` and `wallet/1`. The record state
/// is its redeem script (decoded, and checked by re-encoding, against the templates).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultJson {
    pub round: i64,
    #[serde(with = "hex")]
    pub win: [u8; 6],
    pub m: i64,
    pub k: i64,
    #[serde(with = "hex_vec")]
    pub roots: Vec<[u8; 32]>,
    pub pools: Vec<ResultPoolJson>,
    pub d_claim: i64,
    pub funder_class: u8,
    #[serde(with = "hex")]
    pub funder_key: [u8; 33],
    #[serde(with = "hex")]
    pub ticket_tpl: [u8; 32],
    pub reserve: u64,
    pub record: Option<ResultRecordJson>,
    #[serde(with = "hex")]
    pub x_hash: [u8; 32],
    pub mk_daa: i64,
    pub void: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultPoolJson {
    pub tier: u8,
    pub slice: i64,
    pub value: u64,
    pub outpoint: OutpointJson,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultRecordJson {
    #[serde(with = "hex")]
    pub redeem: Vec<u8>,
    pub value: u64,
    pub outpoint: OutpointJson,
}

impl ResultJson {
    pub fn of(r: &RoundResult, t: &Templates) -> Self {
        Self {
            round: r.round,
            win: r.win,
            m: r.m,
            k: r.k,
            roots: r.roots.clone(),
            pools: r.pools.iter().map(|p| ResultPoolJson { tier: p.tier, slice: p.slice, value: p.value, outpoint: p.outpoint.into() }).collect(),
            d_claim: r.d_claim,
            funder_class: r.funder.class(),
            funder_key: r.funder.key33(),
            ticket_tpl: r.ticket_tpl,
            reserve: r.reserve,
            record: r.record.as_ref().map(|x| ResultRecordJson { redeem: t.record_redeem(&x.state), value: x.value, outpoint: x.outpoint.into() }),
            x_hash: r.x_hash,
            mk_daa: r.mk_daa,
            void: r.void,
        }
    }

    pub fn result(&self, t: &Templates) -> Result<RoundResult, String> {
        if self.funder_class > 2 {
            return Err(format!("round result: funder class {}", self.funder_class));
        }
        let record = match &self.record {
            None => None,
            Some(x) => match State::decode(t, &x.redeem)? {
                State::Record(state) => Some(ResultRecord { state, value: x.value, outpoint: x.outpoint.outpoint() }),
                s => return Err(format!("round result record: a {:?} state", s.role())),
            },
        };
        Ok(RoundResult {
            round: self.round,
            win: self.win,
            m: self.m,
            k: self.k,
            roots: self.roots.clone(),
            pools: self.pools.iter().map(|p| ResultPool { tier: p.tier, slice: p.slice, value: p.value, outpoint: p.outpoint.outpoint() }).collect(),
            d_claim: self.d_claim,
            funder: Payout::from_fields(self.funder_class, &self.funder_key),
            ticket_tpl: self.ticket_tpl,
            reserve: self.reserve,
            record,
            x_hash: self.x_hash,
            mk_daa: self.mk_daa,
            void: self.void,
        })
    }
}

// ------------------------------------------------------------------------------ index/1

/// The first line of `index-log.jsonl`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexLogHeader {
    pub format: String,
    #[serde(with = "hex")]
    pub cov: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogKind {
    Add,
    Remove,
}

/// A spent entry of a log line. `amount` is a decimal string (S2c MF-C): a transaction of
/// the instance may spend a foreign funding UTXO of any value, and the 2^53 rule (D121) is
/// for the lottery's own numbers. `daa` is a DAA score (below 2^53 for about 28 million
/// years at 10 BPS).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpentJson {
    #[serde(with = "u64_dec")]
    pub amount: u64,
    pub daa: u64,
}

/// One `index-log.jsonl` line (A3, with the M8 S2 `spent` amendment and the S2c `tx`
/// amendment). `remove` lines come from a chain block's removal
/// (`removed_chain_block_hashes`), newest transaction first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexLogLine {
    pub seq: u64,
    pub kind: LogKind,
    #[serde(with = "hex")]
    pub chain_block: [u8; 32],
    pub chain_daa: u64,
    #[serde(with = "hex")]
    pub txid: [u8; 32],
    /// The transaction as the hex of its Borsh serialization (`tx_borsh_hex`; S2c MF-C:
    /// its serde JSON has u64 numbers, a foreign input's `sequence` or a foreign output's
    /// value can be >= 2^53, and a line that cannot be written would stop the indexer). Its
    /// id is recomputed when the line is read (`accepted`, MF-B).
    #[serde(with = "tx_borsh_hex")]
    pub tx: Transaction,
    pub accepted_daa: u64,
    /// The spent entries in input order (amount, creation DAA score).
    pub spent: Vec<SpentJson>,
}

impl IndexLogLine {
    pub fn add(seq: u64, chain_block: Hash, chain_daa: u64, a: &Accepted) -> Self {
        Self {
            seq,
            kind: LogKind::Add,
            chain_block: chain_block.as_bytes(),
            chain_daa,
            txid: a.tx.id().as_bytes(),
            tx: a.tx.clone(),
            accepted_daa: a.daa,
            spent: a.spent.iter().map(|s| SpentJson { amount: s.amount, daa: s.daa }).collect(),
        }
    }

    pub fn remove(seq: u64, chain_block: Hash, chain_daa: u64, a: &Accepted) -> Self {
        Self { kind: LogKind::Remove, ..Self::add(seq, chain_block, chain_daa, a) }
    }

    /// This line as one JSON line (no newline), with the `write` checks (M8 S3: the tn10
    /// indexer appends lines to `index-log.jsonl` instead of rewriting the file).
    pub fn to_line(&self) -> Result<String, String> {
        write_line(self)
    }

    /// The accepted transaction; its id must be `txid`.
    pub fn accepted(&self) -> Result<Accepted, String> {
        let id = checked_id(&self.tx).map_err(|e| format!("log line {}: {e}", self.seq))?;
        if id.as_bytes() != self.txid {
            return Err(format!("log line {}: txid {} is not the transaction's id {id}", self.seq, to_hex(&self.txid)));
        }
        Ok(Accepted { tx: self.tx.clone(), daa: self.accepted_daa, spent: self.spent.iter().map(|s| Spent { amount: s.amount, daa: s.daa }).collect() })
    }
}

/// The `kind` of a re-anchor line.
pub const REANCHOR: &str = "reanchor";

/// A re-anchor line of `index-log.jsonl` (M8 S4, PENDING A4 amendment). The indexer's
/// resume block `old_block` had fallen below every node's pruning point, so the index was
/// moved to `new_block` after a check on `node` that each of the `live_checked` live
/// instance UTXOs was still unspent with the same script, value and creation DAA, and that
/// the node returned no instance-bound UTXO the index did not know (`argument` says why
/// that proves there was no instance transaction in the gap). It changes no index state;
/// `at` is the number of add/remove lines before it (their `seq` stays 0, 1, 2, ...). A
/// reader of an older `index/1` refuses the line (unknown `kind`) instead of skipping it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReanchorLine {
    pub kind: String,
    pub at: u64,
    #[serde(with = "hex")]
    pub old_block: [u8; 32],
    /// The old block's DAA score as the index recorded it (snapshots before M8 S4 did not
    /// store it: then the DAA of the newest logged transaction, a lower bound).
    pub old_daa: u64,
    #[serde(with = "hex")]
    pub new_block: [u8; 32],
    pub new_daa: u64,
    pub node: String,
    pub live_checked: u64,
    /// Unix seconds of the check.
    pub wall_time: u64,
    pub argument: String,
}

impl ReanchorLine {
    /// This line as one JSON line (no newline); `kind` must be `reanchor`.
    pub fn to_line(&self) -> Result<String, String> {
        if self.kind != REANCHOR {
            return Err(format!("re-anchor line: kind {:?} is not {REANCHOR:?}", self.kind));
        }
        write_line(self)
    }
}

/// `index-log.jsonl`: a header line, then the add/remove lines in `seq` order 0, 1, 2, ...,
/// with the re-anchor lines among them (each after the `at` add/remove lines before it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexLog {
    pub cov: Hash,
    pub lines: Vec<IndexLogLine>,
    pub reanchors: Vec<ReanchorLine>,
}

impl IndexLog {
    /// The header line of a log of instance `cov` (no newline).
    pub fn header_line(cov: Hash) -> Result<String, String> {
        write_line(&IndexLogHeader { format: INDEX.into(), cov: cov.as_bytes() })
    }

    pub fn to_jsonl(&self) -> Result<String, String> {
        let mut s = write_line(&IndexLogHeader { format: INDEX.into(), cov: self.cov.as_bytes() })?;
        s.push('\n');
        let mut ra = self.reanchors.iter().peekable();
        for (i, l) in self.lines.iter().enumerate() {
            while let Some(r) = ra.next_if(|r| r.at <= i as u64) {
                s.push_str(&r.to_line()?);
                s.push('\n');
            }
            s.push_str(&write_line(l)?);
            s.push('\n');
        }
        for r in ra {
            s.push_str(&r.to_line()?);
            s.push('\n');
        }
        Ok(s)
    }

    pub fn from_jsonl(s: &str) -> Result<Self, String> {
        let mut it = s.lines().filter(|l| !l.trim().is_empty());
        let head = it.next().ok_or("index log: empty")?;
        let v: Value = serde_json::from_str(head).map_err(|e| e.to_string())?;
        check_format(&v, INDEX)?;
        let h: IndexLogHeader = read_line(head)?;
        let mut lines: Vec<IndexLogLine> = vec![];
        let mut reanchors: Vec<ReanchorLine> = vec![];
        for l in it {
            let v: Value = serde_json::from_str(l).map_err(|e| e.to_string())?;
            if v.get("kind").and_then(Value::as_str) == Some(REANCHOR) {
                let r: ReanchorLine = read_line(l)?;
                if r.at != lines.len() as u64 {
                    return Err(format!("index log: a re-anchor line after {} add/remove lines says at = {}", lines.len(), r.at));
                }
                if reanchors.last().is_some_and(|p| p.at == r.at && p.new_block != r.old_block) {
                    return Err("index log: consecutive re-anchor lines do not chain (old block != the previous new block)".into());
                }
                reanchors.push(r);
            } else {
                lines.push(read_line(l)?);
            }
        }
        if let Some((i, l)) = lines.iter().enumerate().find(|(i, l)| l.seq != *i as u64) {
            return Err(format!("index log: line {i} has seq {}", l.seq));
        }
        Ok(Self { cov: Hash::from_bytes(h.cov), lines, reanchors })
    }

    /// The index the log describes: `add` lines applied in order, each `remove` line
    /// reverting its transaction (which must be the last applied one, a reorg removes
    /// blocks newest first). Lines of transactions that are not the instance's are skipped
    /// (and so are their removals). `cov` is the covenant ID the caller verified (the launch
    /// bundle's, `bundle::verify`); the header's is only a label and must equal it (S2d: a
    /// header naming another instance would otherwise skip every line as foreign and
    /// rebuild an empty index, silently). Re-anchor lines change no state: the index after
    /// one is the index before it (the line is the logged proof that no instance
    /// transaction happened in the skipped chain range; M8 S4).
    pub fn rebuild(&self, t: Arc<Templates>, cov: Hash, genesis: Option<Vec<State>>) -> Result<Rebuild, String> {
        if self.cov != cov {
            return Err(format!("index log: header covenant ID {} is not the verified instance's {cov}", self.cov));
        }
        let mut ix = Rebuild::new(t, cov, genesis);
        let mut foreign: Vec<[u8; 32]> = vec![];
        for l in &self.lines {
            match l.kind {
                LogKind::Add => {
                    if ix.apply(&l.accepted()?).map_err(|e| format!("log line {}: {e}", l.seq))?.is_none() {
                        foreign.push(l.txid);
                    }
                }
                LogKind::Remove => {
                    if let Some(i) = foreign.iter().rposition(|x| *x == l.txid) {
                        foreign.remove(i);
                    } else {
                        ix.revert(Hash::from_bytes(l.txid)).map_err(|e| format!("log line {}: {e}", l.seq))?;
                    }
                }
            }
        }
        Ok(ix)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuyJson {
    #[serde(with = "hex")]
    pub txid: [u8; 32],
    pub shard_index: i64,
    #[serde(with = "hex_vec")]
    pub combos: Vec<[u8; 6]>,
    pub ticket_outpoint: OutpointJson,
    #[serde(rename = "ref")]
    pub referral: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseJson {
    #[serde(with = "hex")]
    pub txid: [u8; 32],
    pub marker_daa: u64,
    #[serde(with = "hex_vec")]
    pub roots: Vec<[u8; 32]>,
    pub m_next: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawJson {
    #[serde(with = "hex")]
    pub txid: [u8; 32],
    pub result: ResultJson,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundJson {
    pub round: i64,
    pub m: Option<i64>,
    pub close: Option<CloseJson>,
    pub draw: Option<DrawJson>,
    pub buys: Vec<BuyJson>,
    /// Why the round is incomplete (no proof is offered from it), if it is.
    pub incomplete: Option<String>,
    pub gaps: Vec<String>,
}

/// A live UTXO of `index/1` (A3 `live_utxos[]`): the state is its redeem script, when known.
/// (`bundle/1` stores the state bytes only: `bundle::BundleLive`.)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveJson {
    pub outpoint: OutpointJson,
    pub role: Role,
    pub round: Option<i64>,
    pub value: u64,
    pub daa: u64,
    #[serde(with = "hex")]
    pub covenant_id: [u8; 32],
    pub spk_version: u16,
    #[serde(with = "hex")]
    pub spk: Vec<u8>,
    #[serde(with = "hex_opt")]
    pub state: Option<Vec<u8>>,
}

impl LiveJson {
    /// The state, decoded and checked: its P2SH must be the stored script and its role the
    /// stored role.
    pub fn decode(&self, t: &Templates) -> Result<Option<State>, String> {
        let Some(r) = &self.state else { return Ok(None) };
        let s = State::decode(t, r)?;
        if s.role() != self.role || s.spk(t) != ScriptPublicKey::from_vec(self.spk_version, self.spk.clone()) {
            return Err(format!("live UTXO {}:{}: the state does not match its role or script", to_hex(&self.outpoint.txid), self.outpoint.index));
        }
        Ok(Some(s))
    }
}

/// `index.json` (A3): a snapshot rebuilt from the log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexSnapshot {
    pub format: String,
    #[serde(with = "hex")]
    pub cov: [u8; 32],
    #[serde(with = "hex_opt")]
    pub genesis_txid: Option<[u8; 32]>,
    #[serde(with = "hex_opt")]
    pub last_chain_block: Option<[u8; 32]>,
    pub rounds: Vec<RoundJson>,
    pub live_utxos: Vec<LiveJson>,
    /// Instance-wide missed transitions (`Rebuild::gaps`, S2c should-fix 4); a round's own
    /// are in its `gaps`.
    pub gaps: Vec<String>,
    /// The log's re-anchor lines (M8 S4; absent from older snapshots).
    #[serde(default)]
    pub reanchors: Vec<ReanchorLine>,
    /// The DAA score of `last_chain_block` (M8 S4; absent from older snapshots).
    #[serde(default)]
    pub last_chain_daa: Option<u64>,
}

impl IndexSnapshot {
    pub fn of(ix: &Rebuild, last_chain_block: Option<Hash>) -> Self {
        let t = ix.templates();
        let rounds = ix
            .rounds()
            .values()
            .map(|r| RoundJson {
                round: r.round,
                m: r.m,
                close: r.close.as_ref().map(|c| CloseJson { txid: c.txid.as_bytes(), marker_daa: c.marker_daa, roots: c.frozen_roots.clone(), m_next: c.m_next }),
                draw: r.draw.as_ref().map(|d| DrawJson { txid: d.txid.as_bytes(), result: ResultJson::of(&d.result, t) }),
                buys: r
                    .buys
                    .iter()
                    .map(|b| BuyJson {
                        txid: b.txid.as_bytes(),
                        shard_index: b.shard_index,
                        combos: b.combos.clone(),
                        ticket_outpoint: b.ticket_outpoint.into(),
                        referral: b.referral,
                    })
                    .collect(),
                incomplete: r.incomplete.clone(),
                gaps: r.gaps.clone(),
            })
            .collect();
        let mut live: Vec<LiveJson> = ix
            .live()
            .values()
            .map(|l| LiveJson {
                outpoint: l.outpoint.into(),
                role: l.role,
                round: l.state.as_ref().map(State::round),
                value: l.value,
                daa: l.daa,
                covenant_id: ix.cov().as_bytes(),
                spk_version: l.spk.version(),
                spk: l.spk.script().to_vec(),
                state: l.state.as_ref().map(|s| s.redeem(t)),
            })
            .collect();
        live.sort_by_key(|l| (l.outpoint.txid, l.outpoint.index));
        Self {
            format: INDEX.into(),
            cov: ix.cov().as_bytes(),
            genesis_txid: ix.genesis_txid().map(|h| h.as_bytes()),
            last_chain_block: last_chain_block.map(|h| h.as_bytes()),
            rounds,
            live_utxos: live,
            gaps: ix.gaps().to_vec(),
            reanchors: vec![],
            last_chain_daa: None,
        }
    }
}

// ------------------------------------------------------------------------------ wallet/1

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeafUpdate {
    #[serde(with = "hex")]
    pub combo: [u8; 6],
    /// The count of (rank, bonus) after the update.
    pub count_after: u16,
    #[serde(with = "hex")]
    pub root_after: [u8; 32],
}

/// A ticket the wallet bought (its state's fields, value and creation DAA).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletTicket {
    pub outpoint: OutpointJson,
    pub deadline: i64,
    pub b: i64,
    #[serde(with = "hex")]
    pub bundle: Vec<u8>,
    #[serde(with = "hex")]
    pub claimed: Vec<u8>,
    pub payout_class: u8,
    #[serde(with = "hex")]
    pub payout_key: [u8; 33],
    pub value: u64,
    pub daa: u64,
}

/// A live prize pool of the round as the wallet knows it (its state is the round result's,
/// `RoundResult::pool_state`, R4; a claim moves only its outpoint and value).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletPool {
    pub tier: u8,
    pub outpoint: OutpointJson,
    pub value: u64,
    pub daa: u64,
}

/// `wallet-r<round>.json` (A3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wallet {
    pub format: String,
    #[serde(with = "hex")]
    pub cov: [u8; 32],
    pub b_max: u64,
    pub round: i64,
    pub m: i64,
    /// Per shard, the ordered leaf updates of the wallet's purchases.
    pub shards: Vec<Vec<LeafUpdate>>,
    /// The roots `close()` froze, once closed (checked equal to the rebuilt trees).
    #[serde(with = "hex_vec")]
    pub frozen_roots: Vec<[u8; 32]>,
    pub tickets: Vec<WalletTicket>,
    /// The round result, once drawn.
    pub record: Option<ResultJson>,
    pub pools: Vec<WalletPool>,
}

/// A file's B_MAX (the claim reference's and the ticket script's bound, `game::claim_mode`)
/// bounded to [1, TICKET_B_MAX] before any template is compiled for it: anything else would
/// not compile (0) or would compile without end (S2c, S2d).
pub fn checked_b_max(b_max: u64) -> Result<usize, String> {
    usize::try_from(b_max)
        .ok()
        .filter(|&n| (1..=crate::game::TICKET_B_MAX as usize).contains(&n))
        .ok_or_else(|| format!("b_max {b_max} outside [1, {}]", crate::game::TICKET_B_MAX))
}

impl Wallet {
    pub fn new(cov: Hash, b_max: usize, round: i64, m: i64, k: usize) -> Self {
        Self {
            format: WALLET.into(),
            cov: cov.as_bytes(),
            b_max: b_max as u64,
            round,
            m,
            shards: vec![vec![]; k],
            frozen_roots: vec![],
            tickets: vec![],
            record: None,
            pools: vec![],
        }
    }

    /// Shard `i`'s tree replayed from its leaf updates; each update's count and root must
    /// be what the replay gives.
    pub fn shard_tree(&self, i: usize) -> Result<Tree, String> {
        // S2d (N5): the file's m is bounded before it sizes the tree or is narrowed to u8:
        // [2, M_HARD], the range genesis and close() allow (shard.sil `m <= M_HARD`).
        if !(2..=crate::game::M_HARD).contains(&self.m) {
            return Err(format!("wallet: m {} outside [2, {}]", self.m, crate::game::M_HARD));
        }
        let mut tree = Tree::new(self.m as usize);
        for (n, u) in self.shards.get(i).ok_or_else(|| format!("wallet: no shard {i}"))?.iter().enumerate() {
            let c = Combo::new(u.combo[..5].try_into().unwrap(), u.combo[5]);
            if !c.is_canonical(self.m as u8) {
                return Err(format!("wallet shard {i} update {n}: combination not canonical"));
            }
            tree.increment(c.rank(), c.bonus);
            if tree.counts(c.rank())[c.bonus as usize - 1] != u.count_after || tree.root() != u.root_after {
                return Err(format!("wallet shard {i} update {n}: count or root differs from the replay"));
            }
        }
        Ok(tree)
    }

    /// Ingest a chain-accepted `buy()` of the round on one of the instance's shards (S2c:
    /// on public testnet anyone may buy, so the runner feeds every buy of the round, its
    /// own and third parties', in shard UTXO order). The leaf updates are always recorded
    /// (the spent shard's root must be the wallet's tree root, so the wallet saw every
    /// purchase of that shard before this one); the ticket (output 1) is recorded only if
    /// its payout is one of `own`. Returns whether the ticket was recorded.
    pub fn record_buy(&mut self, t: &Templates, tx: &Transaction, daa: u64, own: &[Payout]) -> Result<bool, String> {
        let b = parse_buy(tx, t)?;
        let s = &b.shard;
        if s.round != self.round || s.m != self.m {
            return Err(format!("wallet: a buy of round {} (m {}), the wallet is round {} (m {})", s.round, s.m, self.round, self.m));
        }
        if tx.outputs.first().and_then(|o| o.covenant).map(|c| c.covenant_id.as_bytes()) != Some(self.cov) {
            return Err("wallet: a buy of another instance (output 0 is not bound to the wallet's covenant ID)".into());
        }
        let i = usize::try_from(s.shard_index).ok().filter(|&i| i < self.shards.len()).ok_or_else(|| format!("wallet: shard index {}", s.shard_index))?;
        let mut tree = self.shard_tree(i)?;
        if tree.root() != s.root {
            return Err(format!("wallet: shard {i}'s spent root is not the wallet's tree root (a purchase the wallet did not see)"));
        }
        let mut ups = vec![];
        for c in &b.bundle {
            tree.increment(c.rank(), c.bonus);
            ups.push(LeafUpdate { combo: c.bytes(), count_after: tree.counts(c.rank())[c.bonus as usize - 1], root_after: tree.root() });
        }
        if tree.root() != b.successor.root {
            return Err("wallet: the rebuilt root is not the buy's successor root".into());
        }
        let out = tx.outputs.get(1).ok_or("buy: no output 1")?;
        if out.script_public_key != t.ticket_spk(&b.ticket) {
            return Err("buy: output 1 is not the ticket".into());
        }
        self.shards[i].extend(ups);
        if !own.contains(&b.payout) {
            return Ok(false);
        }
        self.tickets.push(WalletTicket {
            outpoint: TransactionOutpoint::new(tx.id(), 1).into(),
            deadline: b.ticket.deadline,
            b: b.ticket.b,
            bundle: b.ticket.bundle.clone(),
            claimed: b.ticket.claimed.clone(),
            payout_class: b.ticket.payout_class,
            payout_key: b.ticket.payout_key,
            value: out.value,
            daa,
        });
        Ok(true)
    }

    /// The roots `close()` froze: each must equal the wallet's rebuilt tree root.
    pub fn freeze(&mut self, roots: &[[u8; 32]]) -> Result<(), String> {
        if roots.len() != self.shards.len() {
            return Err(format!("wallet: {} frozen roots for {} shards", roots.len(), self.shards.len()));
        }
        for (i, r) in roots.iter().enumerate() {
            if self.shard_tree(i)?.root() != *r {
                return Err(format!("wallet: shard {i}'s rebuilt root differs from the frozen root (incomplete)"));
            }
        }
        self.frozen_roots = roots.to_vec();
        Ok(())
    }

    /// The round's `draw()`: the round result and its pools (created at `daa`).
    pub fn set_draw(&mut self, t: &Templates, r: &RoundResult, daa: u64) -> Result<(), String> {
        if r.round != self.round {
            return Err(format!("wallet: round result of round {}", r.round));
        }
        self.record = Some(ResultJson::of(r, t));
        self.pools = r.pools.iter().map(|p| WalletPool { tier: p.tier, outpoint: p.outpoint.into(), value: p.value, daa }).collect();
        Ok(())
    }

    /// An accepted `claim()` of one of the wallet's tickets (accepted at `daa`): the pool
    /// moves to output 0; the ticket moves to output 1 with its index's bit set, or is gone
    /// (FINAL).
    pub fn apply_claim(&mut self, t: &Templates, tx: &Transaction, daa: u64) -> Result<(), String> {
        let c = parse_claim(tx, t)?;
        let top: OutpointJson = tx.inputs[0].previous_outpoint.into();
        let pop: OutpointJson = tx.inputs[1].previous_outpoint.into();
        let ti = self.tickets.iter().position(|x| x.outpoint == top).ok_or("wallet: the claimed ticket is not the wallet's")?;
        let pi = self.pools.iter().position(|x| x.outpoint == pop).ok_or("wallet: the claimed pool is not known")?;
        let p0 = tx.outputs.first().ok_or("claim: no output 0")?;
        self.pools[pi].outpoint = TransactionOutpoint::new(tx.id(), 0).into();
        self.pools[pi].value = p0.value;
        self.pools[pi].daa = daa;
        match tx.outputs.get(1).filter(|o| o.covenant.is_some()) {
            Some(o) => {
                let tk = &mut self.tickets[ti];
                tk.claimed[0] |= 1u8 << c.idx;
                tk.outpoint = TransactionOutpoint::new(tx.id(), 1).into();
                tk.value = o.value;
                tk.daa = daa;
            }
            None => {
                self.tickets.remove(ti);
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------------------------ campaign/1

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxRef {
    #[serde(with = "hex")]
    pub txid: [u8; 32],
    /// The node the transaction was submitted to.
    pub node: String,
    /// M8 S3 (A3 amendment): the runner's action name ("C2/buy/17"), so `campaign resume`
    /// skips an action whose recorded transaction is chain-accepted (idempotent by txid).
    #[serde(default)]
    pub action: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckJson {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepJson {
    /// A5 step ID (C0 ... C8).
    pub id: String,
    pub txids: Vec<TxRef>,
    pub checks: Vec<CheckJson>,
    pub daa: u64,
    /// Unix seconds.
    pub wall_time: u64,
}

/// `campaign.json` (A3): `step` is the next step; `campaign resume` continues there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Campaign {
    pub format: String,
    #[serde(with = "hex_opt")]
    pub cov: Option<[u8; 32]>,
    pub step: String,
    pub genesis: Vec<OutpointJson>,
    pub steps: Vec<StepJson>,
    /// M8 S3 (A3 amendment): the runner's named values that a resume needs (start hash,
    /// the sink before each close, the soak's t0, the luck retries), as strings.
    #[serde(default)]
    pub vars: BTreeMap<String, String>,
}

impl Campaign {
    pub fn new() -> Self {
        Self { format: CAMPAIGN.into(), cov: None, step: "C0".into(), genesis: vec![], steps: vec![], vars: BTreeMap::new() }
    }
}

impl Default for Campaign {
    fn default() -> Self {
        Self::new()
    }
}

// ------------------------------------------------------------------------------ ClaimStore

/// What `ClaimCase::from_store` reads (spec M8 A4): an index (`Rebuild`) or a wallet.
pub trait ClaimStore {
    /// The instance's templates (a wallet file's `b_max` is checked first, S2d).
    fn templates(&self) -> Result<Arc<Templates>, String>;
    fn cov(&self) -> Hash;
    /// A live ticket: (state, value, creation DAA).
    fn ticket(&self, op: &TransactionOutpoint) -> Result<(TicketState, u64, u64), String>;
    /// The live prize pools of `round`: (outpoint, state, value, creation DAA).
    fn pools(&self, round: i64) -> Result<Vec<(TransactionOutpoint, PoolState, u64, u64)>, String>;
    /// The frozen shard trees of `round` (refused for an incomplete or open round).
    fn trees(&self, round: i64) -> Result<Vec<Tree>, String>;
}

impl ClaimStore for Rebuild {
    fn templates(&self) -> Result<Arc<Templates>, String> {
        Ok(Rebuild::templates(self).clone())
    }

    fn cov(&self) -> Hash {
        Rebuild::cov(self)
    }

    fn ticket(&self, op: &TransactionOutpoint) -> Result<(TicketState, u64, u64), String> {
        let l = self.live().get(op).ok_or_else(|| format!("ticket {op} is not live in the index"))?;
        match &l.state {
            Some(State::Ticket(s)) => Ok((s.clone(), l.value, l.daa)),
            _ => Err(format!("{op} is not a ticket of known state")),
        }
    }

    fn pools(&self, round: i64) -> Result<Vec<(TransactionOutpoint, PoolState, u64, u64)>, String> {
        let mut v: Vec<_> = self
            .live()
            .values()
            .filter_map(|l| match &l.state {
                Some(State::Pool(p)) if p.round == round => Some((l.outpoint, p.clone(), l.value, l.daa)),
                _ => None,
            })
            .collect();
        v.sort_by_key(|p| p.1.tier);
        Ok(v)
    }

    fn trees(&self, round: i64) -> Result<Vec<Tree>, String> {
        Rebuild::trees(self, round)
    }
}

impl ClaimStore for Wallet {
    fn templates(&self) -> Result<Arc<Templates>, String> {
        Ok(Templates::get(checked_b_max(self.b_max)?))
    }

    fn cov(&self) -> Hash {
        Hash::from_bytes(self.cov)
    }

    fn ticket(&self, op: &TransactionOutpoint) -> Result<(TicketState, u64, u64), String> {
        let o: OutpointJson = (*op).into();
        let w = self.tickets.iter().find(|x| x.outpoint == o).ok_or_else(|| format!("ticket {op} is not in the wallet"))?;
        let s = TicketState {
            round: self.round,
            deadline: w.deadline,
            b: w.b,
            bundle: w.bundle.clone(),
            claimed: w.claimed.clone(),
            payout_class: w.payout_class,
            payout_key: w.payout_key,
        };
        Ok((s, w.value, w.daa))
    }

    fn pools(&self, round: i64) -> Result<Vec<(TransactionOutpoint, PoolState, u64, u64)>, String> {
        if round != self.round {
            return Err(format!("the wallet is round {}, not {round}", self.round));
        }
        let rec = self.record.as_ref().ok_or("the wallet's round is not drawn")?.result(&*self.templates()?)?;
        self.pools
            .iter()
            .map(|p| {
                let i = rec.pools.iter().position(|x| x.tier == p.tier).ok_or_else(|| format!("no tier {} pool in the round result", p.tier))?;
                Ok((p.outpoint.outpoint(), rec.pool_state(i), p.value, p.daa))
            })
            .collect()
    }

    fn trees(&self, round: i64) -> Result<Vec<Tree>, String> {
        if round != self.round {
            return Err(format!("the wallet is round {}, not {round}", self.round));
        }
        if self.frozen_roots.is_empty() {
            return Err(format!("round {round} is not closed in the wallet"));
        }
        (0..self.shards.len())
            .map(|i| {
                let tree = self.shard_tree(i)?;
                if tree.root() != self.frozen_roots[i] {
                    return Err(format!("round {round} is incomplete in the wallet: shard {i}'s root differs from the frozen root"));
                }
                Ok(tree)
            })
            .collect()
    }
}

/// The parameters a stored `BTreeMap` of named integers carries (`bundle/1` `params`).
pub type Params = BTreeMap<String, i64>;
