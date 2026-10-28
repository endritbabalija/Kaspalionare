//! The backup file (spec frontend 3 S8, 6.3, 8 "Local copy"; front end F4) and the re-check of a
//! kept round copy against the chain.
//!
//! **The file** (`kaspalionare-backup/2`, F6 security review): one JSON object with the
//! instance's `webstore/2` tickets and round copies (never a pending purchase, claim or return:
//! their signed transactions stay in this browser; never the quick wallet's key, which lives in
//! its own database and has its own export, spec S7), the network and instance it belongs to,
//! and a checksum: blake2b-256 of the file's compact JSON with the checksum field empty. The
//! checksum finds a damaged file; it is no seal (anyone can write a file with a matching
//! checksum), so the import also checks every record against the chain. Each ticket is filed
//! under its outpoint (`<transactionId>:<index>` of its ticket entry), so the key an imported
//! ticket is kept under is bound to the entry the page checks on the node, never a name the
//! file picks.
//!
//! **Format 1** (`kaspalionare-backup/1`, F4 to F6) still imports: its `pending` records (the
//! page always wrote none) are read for the checksum and never imported, and its ticket keys
//! (the purchase's outpoint) are replaced by each ticket's outpoint.
//!
//! **Reading** (`backup_read`), before anything is kept, in this order: the size cap
//! (`BACKUP_MAX`), strict JSON (`deny_unknown_fields` on every record), the format, this page's
//! network and instance, the checksum, then every record on its own (`check_records`): each
//! ticket's state bytes rebuild its stored entry's script under the instance's covenant ID
//! (`cases::live`) and say the same round, bundle and payout as the record; each round's lists
//! rebuild its stored roots, and the stored marker or pools carry those roots, the round, m and
//! k, and the result's numbers and deadline. Any failure is `backup_invalid` and nothing is read.
//!
//! **The chain check** (`check_copy`, also the local-copy box's re-check, S2): a closed or drawn
//! copy's lists against a live chain object (the stored marker or one of the stored pools, found
//! on a node by its script: a pool keeps its state through every claim); a disagreement is
//! `copy_mismatch` (the page reports `backup_mismatch` for an import). With no such object live
//! any more (the round's pools absorbed), there is nothing left to claim, and the copy is
//! `gone`. The page checks each ticket against the node's entry for its outpoint
//! (`verifyState`) before it keeps it.

use lottery::combo::Combo;
use lottery::game::{TOTAL_MAX, draw_amounts, pool_set, win_counts};
use lottery::index::{Role, State};
use lottery::state::{K_MAX, MarkerState, PoolState};
use lottery::web::{Code, Error, Result, cases, verify};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::api::{self, cov_of, purchase_lists};
use crate::convert::{self, Net, hex, unhex};
use crate::dto::{Combination, CovUtxo, Dec, Hex, RootsInput, Utxo};
use crate::views::RoleName;
use crate::webstore::{self, RoundCopyStage, StoredObject, StoredPending, StoredRound, StoredTicket};

/// The backup file's format name (the one the page writes).
pub const BACKUP_FORMAT: &str = "kaspalionare-backup/2";
/// The F4 format, still read (its pending records are not imported).
pub const BACKUP_FORMAT_V1: &str = "kaspalionare-backup/1";
/// The largest backup file read (bytes): about 130 rounds of 10,000 combinations. The writer
/// keeps every file it writes within it, and within `WORKER_ARGS_MAX` once escaped as the
/// `backupRead` argument (F4 review S7: anything the page writes, it can import).
pub const BACKUP_MAX: usize = 8 << 20;
/// The page's cap on one worker call's arguments, measured as JSON (`web/src/worker/client.ts`
/// `ARGS_MAX_BYTES`; a Vitest pins the two equal through `indexerLimits().workerArgs`).
pub const WORKER_ARGS_MAX: usize = 16 << 20;
/// At most this many records of each kind.
pub const RECORDS_MAX: usize = 4_096;

/// The backup file.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupFile {
    /// `kaspalionare-backup/2`.
    pub format: String,
    /// The record format (`webstore/2`).
    pub store: String,
    pub network: String,
    pub covenant_id: Hex,
    /// Unix milliseconds.
    pub created_ms: Dec,
    /// Each filed under its outpoint.
    pub tickets: Vec<StoredTicket>,
    pub rounds: Vec<StoredRound>,
    /// blake2b-256 of the file's compact JSON with this field empty (hex).
    pub checksum: Hex,
}

/// A `kaspalionare-backup/1` file (read only): format 2 plus the pending purchases, which are
/// never imported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupFileV1 {
    pub format: String,
    pub store: String,
    pub network: String,
    pub covenant_id: Hex,
    pub created_ms: Dec,
    pub tickets: Vec<StoredTicket>,
    pub rounds: Vec<StoredRound>,
    pub pending: Vec<StoredPending>,
    pub checksum: Hex,
}

/// `backupWrite`: the store's records of this page's instance.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupWriteInput {
    pub covenant_id: Hex,
    pub created_ms: Dec,
    pub tickets: Vec<StoredTicket>,
    pub rounds: Vec<StoredRound>,
}

/// `backupRead`: the file's text and this page's instance.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupReadInput {
    pub covenant_id: Hex,
    pub text: String,
}

/// What a backup holds for one round (spec S8: "what it contains per round").
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupRound {
    pub round: Dec,
    /// The copy's stage, if the file holds the round's copy.
    pub stage: Option<RoundCopyStage>,
    pub tickets: u32,
    /// Combinations on those tickets.
    pub combinations: u32,
    /// Combinations in the round's purchase lists (all shards).
    pub purchases: u32,
    pub winning: Option<Combination>,
}

/// A backup file that read and passed every record check (not yet the chain's).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupContents {
    pub created_ms: Dec,
    /// Each keyed by its outpoint (`<transactionId>:<index>` of its ticket entry).
    pub tickets: Vec<StoredTicket>,
    pub rounds: Vec<StoredRound>,
    /// Per round, newest first.
    pub summary: Vec<BackupRound>,
}

fn invalid(d: impl Into<String>) -> Error {
    Error::new(Code::BackupInvalid, d.into())
}

/// Any record fault, as `backup_invalid` with its own code in the detail.
fn as_invalid(e: Error) -> Error {
    if e.code == Code::BackupInvalid { e } else { invalid(format!("{}: {}", e.code.as_str(), e.detail)) }
}

fn checksum(f: &BackupFile) -> Result<String> {
    let mut c = f.clone();
    c.checksum = String::new();
    checksum_of(&c)
}

fn checksum_of<T: Serialize>(c: &T) -> Result<String> {
    let text = serde_json::to_string(c).map_err(|e| Error::new(Code::Encode, e.to_string()))?;
    Ok(hex(&crate::indexer::bundle_hash(text.as_bytes())))
}

/// The key a ticket is filed under in a backup: its ticket entry's outpoint.
pub fn outpoint_key(t: &StoredTicket) -> String {
    format!("{}:{}", t.ticket.utxo.transaction_id.to_ascii_lowercase(), t.ticket.utxo.index)
}

fn bundle_of(t: &lottery::state::TicketState) -> Vec<Combination> {
    t.bundle.chunks_exact(6).take(t.b.clamp(0, 8) as usize).map(|c| Combination { main: [c[0], c[1], c[2], c[3], c[4]], bonus: c[5] }).collect()
}

fn same_round(what: &str, s: &str, r: i64) -> Result<()> {
    if convert::dec_i64(what, s)? != r {
        return Err(invalid(format!("{what}: round {s}, not {r}")));
    }
    Ok(())
}

/// A stored object's state of `role`, whose script is the stored one.
fn stored_state(o: &StoredObject, role: Role) -> Result<State> {
    if o.role.role() != role {
        return Err(invalid(format!("a stored {:?} where a {role:?} belongs", o.role)));
    }
    let s = verify::decode_state(role, &unhex("state", &o.state)?)?;
    if convert::spk_hex(&verify::state_spk(&s)?) != o.script_public_key.to_ascii_lowercase() {
        return Err(invalid(format!("a stored {role:?}'s script is not its state's")));
    }
    Ok(s)
}

fn check_ticket(net: Net, cov: &kaspa_hashes::Hash, cov_hex: &str, t: &StoredTicket) -> Result<()> {
    if !t.covenant_id.eq_ignore_ascii_case(cov_hex) || t.network != net.name() {
        return Err(invalid(format!("ticket {}: of another instance or network", t.key)));
    }
    let (txid, n) = t.key.split_once(':').ok_or_else(|| invalid(format!("ticket key {}", t.key)))?;
    convert::hash32("key", txid)?;
    n.parse::<u32>().map_err(|_| invalid(format!("ticket key {}", t.key)))?;
    let s = cases::ticket(&convert::obj("ticket", &t.ticket)?, cov)?;
    verify::check_ticket(&s, lottery::buy::b_max())?;
    same_round("ticket round", &t.round, s.round)?;
    if bundle_of(&s) != t.bundle || net.payout_address(s.payout_class, &s.payout_key).as_deref() != Some(t.payout.as_str()) {
        return Err(invalid(format!("ticket {}: its bundle or payout is not its state's", t.key)));
    }
    Ok(())
}

fn check_round(net: Net, cov_hex: &str, r: &StoredRound) -> Result<()> {
    if !r.covenant_id.eq_ignore_ascii_case(cov_hex) || r.network != net.name() {
        return Err(invalid(format!("round {}: of another instance or network", r.round)));
    }
    let round = convert::dec_i64("round", &r.round)?;
    if r.key != webstore_round_key(cov_hex, &r.round) {
        return Err(invalid(format!("round key {}", r.key)));
    }
    let k = r.k as usize;
    if !(1..=K_MAX).contains(&k) || r.purchases.len() != k || !(2..=lottery::game::M_CLAIM).contains(&(r.m as i64)) {
        return Err(invalid(format!("round {round}: m = {}, k = {k}, {} lists", r.m, r.purchases.len())));
    }
    let trees = cases::trees(r.m as i64, &purchase_lists(&r.purchases)?)?;
    let rebuilt: Vec<String> = trees.iter().map(|t| hex(&t.root())).collect();
    if let Some(roots) = &r.roots {
        if roots.iter().map(|x| x.to_ascii_lowercase()).collect::<Vec<_>>() != rebuilt {
            return Err(invalid(format!("round {round}: the lists do not rebuild the stored roots")));
        }
    }
    let frozen = MarkerState::roots_bytes(&trees.iter().map(|t| t.root()).collect::<Vec<_>>());
    let marker = match &r.marker {
        Some(o) => {
            let State::Marker(x) = stored_state(o, Role::Marker)? else { return Err(invalid("not a marker")) };
            if x.round != round || x.m != r.m as i64 || x.k != k as i64 || x.roots != frozen {
                return Err(invalid(format!("round {round}: the kept marker has another round, m, k or roots")));
            }
            if r.stage == RoundCopyStage::Closed && r.against.as_ref() != Some(o) {
                return Err(invalid(format!("round {round}: a closed copy whose kept marker is not the one it was checked against")));
            }
            Some(x)
        }
        None => None,
    };
    if let Some(a) = &r.against {
        let (rr, m, kk, roots) = match stored_state(a, a.role.role())? {
            State::Marker(x) => (x.round, x.m, x.k, x.roots),
            State::Pool(x) => (x.round, x.m, x.k, x.roots),
            _ => return Err(invalid("the stored object is neither a marker nor a pool")),
        };
        if rr != round || m != r.m as i64 || kk != k as i64 || roots != frozen {
            return Err(invalid(format!("round {round}: the stored chain object has another round, m, k or roots")));
        }
    }
    if let Some(res) = &r.result {
        if !verify::canonical(&[res.winning.main[0], res.winning.main[1], res.winning.main[2], res.winning.main[3], res.winning.main[4], res.winning.bonus], r.m as i64) {
            return Err(invalid(format!("round {round}: the winning numbers are not a combination")));
        }
        let w = [res.winning.main[0], res.winning.main[1], res.winning.main[2], res.winning.main[3], res.winning.main[4], res.winning.bonus];
        let dl = convert::dec_i64("claimDeadline", &res.claim_deadline)?;
        if res.pools.len() > 7 {
            return Err(invalid(format!("round {round}: {} pools", res.pools.len())));
        }
        let mut states: Vec<PoolState> = vec![];
        for p in &res.pools {
            let State::Pool(x) = stored_state(p, Role::Pool)? else { return Err(invalid("not a pool")) };
            if x.round != round || x.win != w || x.deadline != dl || x.roots != frozen || x.m != r.m as i64 || x.k != k as i64 {
                return Err(invalid(format!("round {round}: a stored pool disagrees with the result or the lists")));
            }
            if states.iter().any(|y| y.tier == x.tier) {
                return Err(invalid(format!("round {round}: two stored pools of tier {}", x.tier)));
            }
            states.push(x);
        }
        if let (Some(mk), Some(any)) = (&marker, states.first()) {
            let want = round_pools(mk, any, &purchase_lists(&r.purchases)?).map_err(as_invalid)?;
            if want.len() != states.len() || want.iter().any(|x| !states.contains(x)) {
                return Err(invalid(format!("round {round}: the kept pools are not the set the kept marker's draw created")));
            }
        }
    }
    let ok = match r.stage {
        RoundCopyStage::Open => r.result.is_none(),
        RoundCopyStage::Closed => r.against.as_ref().is_some_and(|a| a.role == RoleName::Marker) && r.result.is_none(),
        RoundCopyStage::Drawn => r.result.is_some() && r.against.as_ref().is_some_and(|a| a.role == RoleName::Pool),
    };
    if !ok || (r.stage != RoundCopyStage::Open && r.roots.is_none()) {
        return Err(invalid(format!("round {round}: stage {:?} without what it needs", r.stage)));
    }
    Ok(())
}

fn webstore_round_key(cov: &str, round: &str) -> String {
    format!("{}:{round}", cov.to_ascii_lowercase())
}

/// Every record checked on its own (no chain): `backup_invalid` on the first fault. Each ticket
/// must be filed under its outpoint, once.
pub fn check_records(net: Net, covenant_id: &str, tickets: &[StoredTicket], rounds: &[StoredRound]) -> Result<()> {
    let cov = cov_of(covenant_id).map_err(as_invalid)?;
    let cov_hex = hex(&cov.as_bytes());
    if tickets.len() > RECORDS_MAX || rounds.len() > RECORDS_MAX {
        return Err(invalid("too many records"));
    }
    let mut keys = std::collections::BTreeSet::new();
    for t in tickets {
        check_ticket(net, &cov, &cov_hex, t).map_err(as_invalid)?;
        let k = outpoint_key(t);
        if !t.key.eq_ignore_ascii_case(&k) {
            return Err(invalid(format!("ticket {}: not filed under its outpoint {k}", t.key)));
        }
        if !keys.insert(k) {
            return Err(invalid(format!("ticket {} twice", t.key)));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for r in rounds {
        check_round(net, &cov_hex, r).map_err(as_invalid)?;
        if !seen.insert(r.key.clone()) {
            return Err(invalid(format!("round {} twice", r.round)));
        }
    }
    Ok(())
}

/// `backupWrite`'s answer: the file's text and the records left out of it.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupWritten {
    pub text: String,
    /// The keys of the records that did not pass their own check (e.g. an F3 record a later page
    /// cannot vouch for) and were left out (F4 review note: one bad record never stops a backup).
    pub skipped: Vec<String>,
}

/// Spec S8: the backup file of the store's records. Each record is checked first, so a page
/// never writes a file it would refuse to read; a record that fails is left out and named
/// (`skipped`). Pending purchases, claims and returns are not exported (their signed
/// transactions stay in this browser). Each ticket is filed under its outpoint (a ticket after a
/// claim keeps its purchase's key in the store; the file names its current entry); a second
/// record of the same outpoint is left out. The file is compact JSON within `BACKUP_MAX`, and within
/// `WORKER_ARGS_MAX` as the escaped `backupRead` argument, else `backup_invalid` (F4 review S7).
pub fn backup_write(net: Net, i: &BackupWriteInput) -> Result<BackupWritten> {
    let cov = cov_of(&i.covenant_id).map_err(as_invalid)?;
    let cov_hex = hex(&cov.as_bytes());
    if i.tickets.len() > RECORDS_MAX || i.rounds.len() > RECORDS_MAX {
        return Err(invalid(format!("{} tickets, {} rounds: at most {RECORDS_MAX} of each", i.tickets.len(), i.rounds.len())));
    }
    let mut skipped = vec![];
    let mut tickets = vec![];
    let mut keys = std::collections::BTreeSet::new();
    for t in &i.tickets {
        let filed = StoredTicket { pending: None, key: outpoint_key(t), ..t.clone() };
        if check_ticket(net, &cov, &cov_hex, &filed).is_ok() && keys.insert(filed.key.clone()) { tickets.push(filed) } else { skipped.push(t.key.clone()) }
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut rounds = vec![];
    for r in &i.rounds {
        if check_round(net, &cov_hex, r).is_ok() && seen.insert(r.key.clone()) { rounds.push(r.clone()) } else { skipped.push(r.key.clone()) }
    }
    let mut f = BackupFile {
        format: BACKUP_FORMAT.into(),
        store: webstore::FORMAT.into(),
        network: net.name().into(),
        covenant_id: cov_hex.clone(),
        created_ms: i.created_ms.clone(),
        tickets,
        rounds,
        checksum: String::new(),
    };
    f.checksum = checksum(&f)?;
    let text = serde_json::to_string(&f).map_err(|e| Error::new(Code::Encode, e.to_string()))?;
    fits(&text, &cov_hex)?;
    Ok(BackupWritten { text, skipped })
}

/// A backup text the page can read back: within `BACKUP_MAX`, and within `WORKER_ARGS_MAX` as the
/// escaped `backupRead` argument (the worker's cap is measured as JSON).
fn fits(text: &str, cov_hex: &str) -> Result<()> {
    let arg = serde_json::to_string(&BackupReadInput { covenant_id: cov_hex.into(), text: text.into() }).map_err(|e| Error::new(Code::Encode, e.to_string()))?;
    if text.len() > BACKUP_MAX || arg.len() > WORKER_ARGS_MAX {
        return Err(invalid(format!("the backup would be {} bytes ({} as the read argument): at most {BACKUP_MAX} ({WORKER_ARGS_MAX})", text.len(), arg.len())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// F4 review S7: the writer refuses what the reader or the worker would refuse.
    #[test]
    fn a_backup_is_written_only_within_what_the_page_reads() {
        let cov = "ab".repeat(32);
        assert!(fits(&"a".repeat(BACKUP_MAX - 200), &cov).is_ok());
        assert_eq!(fits(&"a".repeat(BACKUP_MAX + 1), &cov).unwrap_err().code, Code::BackupInvalid);
        // Escaping doubles a quote: a text within BACKUP_MAX can still be too large an argument.
        assert_eq!(fits(&"\"".repeat(BACKUP_MAX), &cov).unwrap_err().code, Code::BackupInvalid);
    }
}

fn summary(c: &BackupFile) -> Vec<BackupRound> {
    let mut rounds: std::collections::BTreeMap<i64, BackupRound> = Default::default();
    let entry = |m: &mut std::collections::BTreeMap<i64, BackupRound>, r: &str| -> Option<i64> {
        let n = r.parse::<i64>().ok()?;
        m.entry(n).or_insert(BackupRound { round: r.into(), stage: None, tickets: 0, combinations: 0, purchases: 0, winning: None });
        Some(n)
    };
    for t in &c.tickets {
        if let Some(n) = entry(&mut rounds, &t.round) {
            let x = rounds.get_mut(&n).expect("just inserted");
            x.tickets += 1;
            x.combinations += t.bundle.len() as u32;
        }
    }
    for r in &c.rounds {
        if let Some(n) = entry(&mut rounds, &r.round) {
            let x = rounds.get_mut(&n).expect("just inserted");
            x.stage = Some(r.stage);
            x.purchases = r.purchases.iter().map(|p| (p.len() / 12) as u32).sum();
            x.winning = r.result.as_ref().map(|res| res.winning);
        }
    }
    rounds.into_values().rev().collect()
}

/// Spec S8: a backup file read back, every record checked on its own (`backup_invalid`).
pub fn backup_read(net: Net, i: &BackupReadInput) -> Result<BackupContents> {
    if i.text.len() > BACKUP_MAX {
        return Err(invalid(format!("{} bytes (at most {BACKUP_MAX})", i.text.len())));
    }
    #[derive(Deserialize)]
    struct Head {
        format: String,
    }
    let not = |e: serde_json::Error| invalid(format!("not a backup file: {e}"));
    let head: Head = serde_json::from_str(&i.text).map_err(not)?;
    let f: BackupFile = match head.format.as_str() {
        BACKUP_FORMAT => serde_json::from_str(&i.text).map_err(not)?,
        BACKUP_FORMAT_V1 => {
            let v1: BackupFileV1 = serde_json::from_str(&i.text).map_err(not)?;
            if v1.pending.len() > RECORDS_MAX {
                return Err(invalid("too many records"));
            }
            if checksum_of(&BackupFileV1 { checksum: String::new(), ..v1.clone() })? != v1.checksum.to_ascii_lowercase() {
                return Err(invalid("the checksum does not hold: the file is damaged"));
            }
            // Format 1 filed a ticket under its purchase's outpoint; here each is filed under its own.
            // Its pending records are not imported.
            let tickets = v1.tickets.into_iter().map(|t| StoredTicket { key: outpoint_key(&t), ..t }).collect();
            let mut f = BackupFile { format: v1.format, store: v1.store, network: v1.network, covenant_id: v1.covenant_id, created_ms: v1.created_ms, tickets, rounds: v1.rounds, checksum: String::new() };
            f.checksum = checksum(&f)?;
            f
        }
        other => return Err(invalid(format!("format {other}"))),
    };
    let cov = cov_of(&i.covenant_id).map_err(as_invalid)?;
    if f.store != webstore::FORMAT {
        return Err(invalid(format!("format {} / {}", f.format, f.store)));
    }
    if f.network != net.name() || convert::hash32("covenantId", &f.covenant_id).map_err(as_invalid)? != cov.as_bytes() {
        return Err(invalid(format!("a backup of instance {} on {}", f.covenant_id, f.network)));
    }
    if checksum(&f)? != f.checksum.to_ascii_lowercase() {
        return Err(invalid("the checksum does not hold: the file is damaged"));
    }
    check_records(net, &i.covenant_id, &f.tickets, &f.rounds)?;
    if let Some(t) = f.tickets.iter().find(|t| t.pending.is_some()) {
        return Err(invalid(format!("ticket {}: a pending action in a backup", t.key)));
    }
    let summary = summary(&f);
    Ok(BackupContents { created_ms: f.created_ms, tickets: f.tickets, rounds: f.rounds, summary })
}

// ------------------------------------------------------------------------------ the round's pools

/// Every pool `draw()` created for a round (F4 review M2; D45, `game::pool_set`, built as
/// `lottery::draw` builds them): from the round's marker (J, G2, the sales and the shares fix the
/// slices), the k purchase lists (h_J and n2 at the winning numbers) and one of the round's pools
/// (the fields every pool of one draw shares: the winning numbers, the claim deadline, the ticket
/// template, R and the funder). `copy_mismatch` when the marker and the pool are not of one round,
/// or the pool is not one of the set.
pub fn round_pools(mk: &MarkerState, any: &PoolState, lists: &[Vec<u8>]) -> Result<Vec<PoolState>> {
    let bad = |d: String| Error::new(Code::CopyMismatch, d);
    verify::check_marker(mk)?;
    verify::check_pool(any)?;
    if any.round != mk.round || any.m != mk.m || any.k != mk.k || any.roots != mk.roots {
        return Err(bad(format!("round {}: the marker and the pool are not of one round", mk.round)));
    }
    let trees = cases::trees(mk.m, lists)?;
    if trees.len() != mk.k as usize || MarkerState::roots_bytes(&trees.iter().map(|t| t.root()).collect::<Vec<_>>()) != mk.roots {
        return Err(bad(format!("round {}: the lists do not rebuild the marker's roots", mk.round)));
    }
    let w = any.win;
    if !verify::canonical(&w, mk.m) {
        return Err(bad(format!("round {}: the winning numbers are not a combination", mk.round)));
    }
    let rank = Combo::new([w[0], w[1], w[2], w[3], w[4]], w[5]).rank();
    let counts: Vec<Vec<u16>> = trees.iter().map(|t| t.counts(rank)).collect();
    let (h_j, n2) = win_counts(&counts, w[5], mk.m);
    // pool_set multiplies n2 x G2 (the script's checked multiplication): refused here, never a panic.
    if n2.checked_mul(mk.g2).is_none_or(|v| v > TOTAL_MAX) {
        return Err(Error::new(Code::Overflow, "n2 x G2"));
    }
    let set = pool_set(h_j, n2, mk.jackpot, mk.g2, &draw_amounts(mk).b, mk.m);
    let pools: Vec<PoolState> = set.pools.iter().map(|p| PoolState { tier: p.tier, slice: p.slice, ..any.clone() }).collect();
    if !pools.contains(any) {
        return Err(bad(format!("round {}: pool tier {} is not one the marker's draw created", mk.round, any.tier)));
    }
    Ok(pools)
}

fn object_of(s: &State) -> Result<StoredObject> {
    Ok(StoredObject {
        role: RoleName::of(s.role()),
        state: hex(&verify::state_bytes(s)?),
        script_public_key: convert::spk_hex(&verify::state_spk(s)?),
    })
}

fn pool_states(res: Option<&webstore::StoredResult>) -> Result<Vec<PoolState>> {
    let mut out = vec![];
    for p in res.iter().flat_map(|res| &res.pools) {
        match stored_state(p, Role::Pool)? {
            State::Pool(x) => out.push(x),
            _ => return Err(invalid("not a pool")),
        }
    }
    Ok(out)
}

/// `mergeCopy`: a round copy just checked against the chain (`next`: from the indexer API through
/// `checkIndexerRound`, or a backup's through `checkCopy`) and the copy the store holds (`prev`).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MergeCopyInput {
    pub covenant_id: Hex,
    #[serde(default)]
    pub prev: Option<StoredRound>,
    pub next: StoredRound,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MergedCopy {
    /// The record to keep.
    pub round: StoredRound,
    /// It differs from `prev` (the page writes it only then).
    pub changed: bool,
}

fn stage_no(s: RoundCopyStage) -> u8 {
    match s {
        RoundCopyStage::Open => 0,
        RoundCopyStage::Closed => 1,
        RoundCopyStage::Drawn => 2,
    }
}

fn mismatch(round: &str, d: &str) -> Error {
    Error::new(Code::CopyMismatch, format!("round {round}: {d}"))
}

/// Spec 6.3, F4 review M2: the copy to keep. A copy never moves back a stage and never loses what
/// it holds: the closed copy's marker is kept into the drawn copy; a drawn copy's pools are the
/// union of both copies' (each was live when taken; an absorbed pool stays listed); with the
/// marker kept, the pools are the whole set its draw created (`round_pools`), so every pool is
/// listed whatever the indexer served. Frozen lists, roots, numbers or deadline that differ
/// between the two, or a marker that does not explain the pools, are `copy_mismatch`. A `prev`
/// that fails its own record check (an F3 record) is replaced.
pub fn merge_copy(net: Net, i: &MergeCopyInput) -> Result<MergedCopy> {
    let cov = cov_of(&i.covenant_id)?;
    let cov_hex = hex(&cov.as_bytes());
    let n = &i.next;
    check_round(net, &cov_hex, n).map_err(|e| mismatch(&n.round, &e.detail))?;
    let prev = i.prev.as_ref().filter(|p| check_round(net, &cov_hex, p).is_ok());
    let mut out = n.clone();
    if let Some(p) = prev {
        if p.key != n.key {
            return Err(mismatch(&n.round, "two copies of different rounds"));
        }
        if stage_no(p.stage) > stage_no(n.stage) {
            return Ok(MergedCopy { round: p.clone(), changed: false });
        }
        let frozen = |r: &StoredRound| r.stage != RoundCopyStage::Open;
        if frozen(p) && (p.purchases != n.purchases || p.roots != n.roots || p.m != n.m || p.k != n.k) {
            return Err(mismatch(&n.round, "the kept copy's frozen lists or roots differ from the new one's"));
        }
        let kept_marker = p.marker.clone().or_else(|| p.against.clone().filter(|a| p.stage == RoundCopyStage::Closed && a.role == RoleName::Marker));
        match (&out.marker, kept_marker) {
            (Some(a), Some(b)) if *a != b => return Err(mismatch(&n.round, "two different markers")),
            (None, Some(b)) => out.marker = Some(b),
            _ => {}
        }
        if let (Some(a), Some(b)) = (&p.result, &mut out.result) {
            if a.winning != b.winning || a.claim_deadline != b.claim_deadline {
                return Err(mismatch(&n.round, "the kept result's numbers or deadline differ from the new one's"));
            }
            for o in &a.pools {
                if !b.pools.iter().any(|x| x.script_public_key.eq_ignore_ascii_case(&o.script_public_key)) {
                    b.pools.push(o.clone());
                }
            }
        }
    }
    if out.stage == RoundCopyStage::Drawn {
        if let (Some(mk), Some(res)) = (&out.marker, &mut out.result) {
            let State::Marker(m) = stored_state(mk, Role::Marker).map_err(|e| mismatch(&n.round, &e.detail))? else { unreachable!() };
            let have = pool_states(Some(res))?;
            let any = have.first().ok_or_else(|| mismatch(&n.round, "a drawn copy without a pool"))?;
            let want = round_pools(&m, any, &purchase_lists(&n.purchases)?)?;
            if let Some(x) = have.iter().find(|x| !want.contains(x)) {
                return Err(mismatch(&n.round, &format!("pool tier {} is not one the kept marker's draw created", x.tier)));
            }
            res.pools = want.into_iter().map(|p| object_of(&State::Pool(p))).collect::<Result<_>>()?;
        }
    }
    check_round(net, &cov_hex, &out).map_err(|e| mismatch(&n.round, &e.detail))?;
    let changed = i.prev.as_ref() != Some(&out);
    Ok(MergedCopy { round: out, changed })
}

// ------------------------------------------------------------------------------ check a copy

/// `checkCopy`: a kept round copy and the node's entries at its chain objects' scripts.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CopyCheckInput {
    pub covenant_id: Hex,
    pub round: StoredRound,
    /// What a public node holds at the scripts of `round.against`, `round.marker` and
    /// `round.result`'s pools (`nodeScriptEntries`); others are ignored.
    pub live: Vec<Utxo>,
    /// The node's virtual DAA score: a listed pool not live after its claim deadline was absorbed
    /// (F4 review M2). Without it no pool counts as absorbed.
    #[serde(default)]
    pub virtual_daa_score: Option<Dec>,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CopyStatus {
    /// The lists rebuild the roots of a live chain object of the round.
    Verified,
    /// The round is still open: its copy cannot be checked against frozen roots yet.
    Open,
    /// Every pool the copy lists is absorbed (not live after its claim deadline): nothing is left
    /// to claim from it.
    Gone,
    /// The copy cannot be checked and must be taken again (F4 review M2, S4): a closed copy whose
    /// marker is spent (the round is drawn or void, its result unknown here), a drawn copy whose
    /// listed pools are not live before their claim deadline, or a record without what its stage
    /// needs (an F3 record).
    Incomplete,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CopyCheck {
    pub status: CopyStatus,
    /// The round's live pools (the node's entries with the stored states), ready for
    /// `ticketCard` and `buildClaim`.
    pub pools: Vec<CovUtxo>,
    /// The live object the lists were checked against.
    pub against: Option<CovUtxo>,
    /// The tiers of listed pools the chain shows absorbed (`ticketCard`'s `absorbed`).
    pub absorbed: Vec<u8>,
    /// The copy lists every pool `draw()` created (its marker kept; `ticketCard`'s `complete`).
    pub complete: bool,
}

/// The node's live entries at a stored object's script, as covenant objects of the instance.
fn live_at(cov: &kaspa_hashes::Hash, o: &StoredObject, live: &[Utxo]) -> Result<Vec<CovUtxo>> {
    let mut out = vec![];
    for u in live.iter().filter(|u| u.script_public_key.eq_ignore_ascii_case(&o.script_public_key)) {
        let c = CovUtxo { utxo: u.clone(), state: o.state.clone() };
        cases::live(o.role.role(), &convert::obj("live", &c)?, cov).map_err(|e| Error::new(Code::CopyMismatch, format!("{}: {}", e.code.as_str(), e.detail)))?;
        out.push(c);
    }
    Ok(out)
}

/// Spec 6.3, S2: a kept copy against the chain (`copy_mismatch` when its lists do not rebuild
/// the roots of the round's live marker or pool, or its record is not consistent in itself).
pub fn check_copy(net: Net, i: &CopyCheckInput) -> Result<CopyCheck> {
    let cov = cov_of(&i.covenant_id)?;
    let r = &i.round;
    if i.live.len() > 256 {
        return Err(Error::new(Code::InputRange, format!("{} entries", i.live.len())));
    }
    let none = |status| CopyCheck { status, pools: vec![], against: None, absorbed: vec![], complete: false };
    if r.stage == RoundCopyStage::Open {
        return Ok(none(CopyStatus::Open));
    }
    // A closed or drawn record without what its stage needs (an F3 copy) is taken again.
    let ready = match r.stage {
        RoundCopyStage::Closed => r.against.as_ref().is_some_and(|a| a.role == RoleName::Marker),
        _ => r.result.as_ref().is_some_and(|x| !x.pools.is_empty()),
    };
    if !ready || r.roots.is_none() {
        return Ok(none(CopyStatus::Incomplete));
    }
    check_round(net, &hex(&cov.as_bytes()), r).map_err(|e| Error::new(Code::CopyMismatch, format!("round {}: {}", r.round, e.detail)))?;
    let roots_of = |against: &CovUtxo| {
        api::check_roots(&RootsInput { covenant_id: i.covenant_id.clone(), against: against.clone(), purchases: r.purchases.clone() })
            .map_err(|e| Error::new(Code::CopyMismatch, format!("round {}: {}: {}", r.round, e.code.as_str(), e.detail)))
    };
    if r.stage == RoundCopyStage::Closed {
        let marker = r.against.as_ref().expect("ready");
        return Ok(match live_at(&cov, marker, &i.live)?.into_iter().next() {
            Some(against) => {
                roots_of(&against)?;
                CopyCheck { status: CopyStatus::Verified, pools: vec![], against: Some(against), absorbed: vec![], complete: false }
            }
            // The marker is spent: the round was drawn or voided; its result is not in this copy.
            None => none(CopyStatus::Incomplete),
        });
    }
    let now = i.virtual_daa_score.as_deref().map(|d| convert::dec("virtualDaaScore", d)).transpose()?;
    let (mut pools, mut absorbed, mut unknown) = (vec![], vec![], false);
    for (p, s) in r.result.iter().flat_map(|res| &res.pools).zip(pool_states(r.result.as_ref())?) {
        let l = live_at(&cov, p, &i.live)?;
        if !l.is_empty() {
            pools.extend(l);
        } else if now.is_some_and(|n| n > s.deadline as u64) {
            absorbed.push(s.tier);
        } else {
            unknown = true;
        }
    }
    // With the marker kept the copy lists every pool of the draw; a listed pool that is neither
    // live nor absorbed leaves its tier unknown, so the set is then not complete either.
    let complete = r.marker.is_some() && !unknown;
    let Some(against) = pools.first().cloned() else {
        let status = if unknown { CopyStatus::Incomplete } else { CopyStatus::Gone };
        return Ok(CopyCheck { status, pools, against: None, absorbed, complete });
    };
    roots_of(&against)?;
    Ok(CopyCheck { status: CopyStatus::Verified, pools, against: Some(against), absorbed, complete })
}
