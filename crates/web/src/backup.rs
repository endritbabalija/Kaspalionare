//! The backup file (spec frontend 3 S8, 6.3, 8 "Local copy"; front end F4) and the re-check of a
//! kept round copy against the chain.
//!
//! **The file** (`kaspalionare-backup/1`): one JSON object with the whole local store of one
//! instance (its `webstore/2` tickets, rounds and pending purchases; never the quick wallet's
//! key, which lives in its own database and has its own export, spec S7), the network and
//! instance it belongs to, and a checksum: blake2b-256 of the file's compact JSON with the
//! checksum field empty. The checksum finds a damaged file; it is no seal (anyone can write a
//! file with a matching checksum), so the import also checks every record against the chain.
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

use lottery::index::{Role, State};
use lottery::state::{K_MAX, MarkerState};
use lottery::web::{Code, Error, Result, cases, verify};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::api::{self, cov_of, purchase_lists};
use crate::convert::{self, Net, hex, unhex};
use crate::dto::{Combination, CovUtxo, Dec, Hex, RootsInput, Utxo};
use crate::views::RoleName;
use crate::webstore::{self, RoundCopyStage, StoredObject, StoredPending, StoredRound, StoredTicket};

/// The backup file's format name.
pub const BACKUP_FORMAT: &str = "kaspalionare-backup/1";
/// The largest backup file read (bytes): about 260 rounds of 10,000 combinations.
pub const BACKUP_MAX: usize = 16 << 20;
/// At most this many records of each kind.
pub const RECORDS_MAX: usize = 4_096;

/// The backup file.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupFile {
    /// `kaspalionare-backup/1`.
    pub format: String,
    /// The record format (`webstore/2`).
    pub store: String,
    pub network: String,
    pub covenant_id: Hex,
    /// Unix milliseconds.
    pub created_ms: Dec,
    pub tickets: Vec<StoredTicket>,
    pub rounds: Vec<StoredRound>,
    pub pending: Vec<StoredPending>,
    /// blake2b-256 of the file's compact JSON with this field empty (hex).
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
    #[serde(default)]
    pub pending: Vec<StoredPending>,
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
    pub tickets: Vec<StoredTicket>,
    pub rounds: Vec<StoredRound>,
    pub pending: Vec<StoredPending>,
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
    let text = serde_json::to_string(&c).map_err(|e| Error::new(Code::Encode, e.to_string()))?;
    Ok(hex(&crate::indexer::bundle_hash(text.as_bytes())))
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
        for p in &res.pools {
            let State::Pool(x) = stored_state(p, Role::Pool)? else { return Err(invalid("not a pool")) };
            if x.round != round || x.win != w || x.deadline != dl || x.roots != frozen || x.m != r.m as i64 || x.k != k as i64 {
                return Err(invalid(format!("round {round}: a stored pool disagrees with the result or the lists")));
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

fn check_pending(net: Net, cov_hex: &str, p: &StoredPending) -> Result<()> {
    if !p.covenant_id.eq_ignore_ascii_case(cov_hex) || p.network != net.name() || !p.signed.id.eq_ignore_ascii_case(&p.key) {
        return Err(invalid(format!("pending {}: of another instance, network or transaction", p.key)));
    }
    let s = verify::decode_state(Role::Ticket, &unhex("ticket.state", &p.ticket.state)?)?;
    if convert::spk_hex(&verify::state_spk(&s)?) != p.ticket.script_public_key.to_ascii_lowercase() {
        return Err(invalid(format!("pending {}: its ticket's script is not its state's", p.key)));
    }
    Ok(())
}

/// Every record checked on its own (no chain): `backup_invalid` on the first fault.
pub fn check_records(net: Net, covenant_id: &str, tickets: &[StoredTicket], rounds: &[StoredRound], pending: &[StoredPending]) -> Result<()> {
    let cov = cov_of(covenant_id).map_err(as_invalid)?;
    let cov_hex = hex(&cov.as_bytes());
    if tickets.len() > RECORDS_MAX || rounds.len() > RECORDS_MAX || pending.len() > RECORDS_MAX {
        return Err(invalid("too many records"));
    }
    for t in tickets {
        check_ticket(net, &cov, &cov_hex, t).map_err(as_invalid)?;
    }
    let mut seen = std::collections::BTreeSet::new();
    for r in rounds {
        check_round(net, &cov_hex, r).map_err(as_invalid)?;
        if !seen.insert(r.key.clone()) {
            return Err(invalid(format!("round {} twice", r.round)));
        }
    }
    for p in pending {
        check_pending(net, &cov_hex, p).map_err(as_invalid)?;
    }
    Ok(())
}

/// Spec S8: the backup file of the store's records (checked first, so a page never writes a
/// file it would refuse to read).
pub fn backup_write(net: Net, i: &BackupWriteInput) -> Result<String> {
    check_records(net, &i.covenant_id, &i.tickets, &i.rounds, &i.pending)?;
    let mut f = BackupFile {
        format: BACKUP_FORMAT.into(),
        store: webstore::FORMAT.into(),
        network: net.name().into(),
        covenant_id: i.covenant_id.to_ascii_lowercase(),
        created_ms: i.created_ms.clone(),
        tickets: i.tickets.clone(),
        rounds: i.rounds.clone(),
        pending: i.pending.clone(),
        checksum: String::new(),
    };
    f.checksum = checksum(&f)?;
    serde_json::to_string_pretty(&f).map_err(|e| Error::new(Code::Encode, e.to_string()))
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
    let f: BackupFile = serde_json::from_str(&i.text).map_err(|e| invalid(format!("not a backup file: {e}")))?;
    let cov = cov_of(&i.covenant_id).map_err(as_invalid)?;
    if f.format != BACKUP_FORMAT || f.store != webstore::FORMAT {
        return Err(invalid(format!("format {} / {}", f.format, f.store)));
    }
    if f.network != net.name() || convert::hash32("covenantId", &f.covenant_id).map_err(as_invalid)? != cov.as_bytes() {
        return Err(invalid(format!("a backup of instance {} on {}", f.covenant_id, f.network)));
    }
    if checksum(&f)? != f.checksum.to_ascii_lowercase() {
        return Err(invalid("the checksum does not hold: the file is damaged"));
    }
    check_records(net, &i.covenant_id, &f.tickets, &f.rounds, &f.pending)?;
    let summary = summary(&f);
    Ok(BackupContents { created_ms: f.created_ms, tickets: f.tickets, rounds: f.rounds, pending: f.pending, summary })
}

// ------------------------------------------------------------------------------ check a copy

/// `checkCopy`: a kept round copy and the node's entries at its chain objects' scripts.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CopyCheckInput {
    pub covenant_id: Hex,
    pub round: StoredRound,
    /// What a public node holds at the scripts of `round.against` and `round.result`'s pools
    /// (`nodeScriptEntries`); others are ignored.
    pub live: Vec<Utxo>,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CopyStatus {
    /// The lists rebuild the roots of a live chain object of the round.
    Verified,
    /// The round is still open: its copy cannot be checked against frozen roots yet.
    Open,
    /// No chain object of the round is live any more (its pools are gone): nothing is left to
    /// claim from it.
    Gone,
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
/// the roots of the round's live marker or pool).
pub fn check_copy(i: &CopyCheckInput) -> Result<CopyCheck> {
    let cov = cov_of(&i.covenant_id)?;
    let r = &i.round;
    if i.live.len() > 256 {
        return Err(Error::new(Code::InputRange, format!("{} entries", i.live.len())));
    }
    if r.stage == RoundCopyStage::Open {
        return Ok(CopyCheck { status: CopyStatus::Open, pools: vec![], against: None });
    }
    let mut pools = vec![];
    for p in r.result.iter().flat_map(|res| &res.pools) {
        pools.extend(live_at(&cov, p, &i.live)?);
    }
    let against = match &r.against {
        Some(a) if a.role == RoleName::Marker => live_at(&cov, a, &i.live)?.into_iter().next(),
        _ => None,
    }
    .or_else(|| pools.first().cloned());
    let Some(against) = against else { return Ok(CopyCheck { status: CopyStatus::Gone, pools, against: None }) };
    api::check_roots(&RootsInput { covenant_id: i.covenant_id.clone(), against: against.clone(), purchases: r.purchases.clone() })
        .map_err(|e| Error::new(Code::CopyMismatch, format!("round {}: {}: {}", r.round, e.code.as_str(), e.detail)))?;
    Ok(CopyCheck { status: CopyStatus::Verified, pools, against: Some(against) })
}
