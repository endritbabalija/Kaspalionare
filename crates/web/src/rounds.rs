//! S3 Rounds and S4 Run the round (spec frontend 3, 4; front end F5): what the chain fixes of a
//! round, "Verify in this browser" (J6) item by item, and the caller panel's figures of a built
//! `close()`, `draw()` or void. No rule lives here: every value comes from `lottery`
//! (`game::anchor_daa`, `claim_deadline`, `void_lock`, `round_amounts`, `draw_amounts`) over
//! node-checked chain objects, and every check reuses `indexer::check_round` and
//! `lottery::web::verify`.
//!
//! - `facts` (inside `checkIndexerRound`): T, S, J, D, the claim deadline, the void lock, the k
//!   frozen roots, the caller reward (C_close or C_draw) and R, each with the live objects it is
//!   read from (spec 5 rule 1: every figure links to where it lives on-chain). A figure that no
//!   live object of the round fixes is absent; the page then shows the indexer's figure marked
//!   unverified, never as checked.
//! - `verify_round` (`verifyRound`): the J6 checks, each with its own result (match, mismatch, not
//!   checkable): each object's state against its live UTXO, the status the objects show, the
//!   indexer's frozen roots against the chain's, each shard's tree rebuilt from its purchases
//!   against the chain's root, the winning numbers recomputed from the anchor proof against the
//!   record (the pools), and X.hash against a public node's header (F5: after the draw too).
//! - `caller_figures` (`callerFigures`): the S4 rows of a built transaction (the reward, the fee,
//!   the reserves posted and when they come back, refunds, the total), bound to it: the caller
//!   output is the transaction's last output and must equal funding - posted - fee + reward
//!   (design 7.2, 7.3; `CloseCase`/`DrawCase::caller_formula`), with posted a whole number of R.

use lottery::game::{self, Shares};
use lottery::state::{K_MAX, MarkerState};
use lottery::web::{Code, Error, Result, verify};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::api::{cov_of, unsigned_of};
use crate::convert::{self, Net, dec, hex, unhex};
use crate::dto::{BuiltTx, CovUtxo, Dec, Hex, Utxo, WinningOut};
use crate::indexer::{self, ApiObject, ApiRound, Classified, RoundCheckInput};
use crate::views::RoleName;

// ------------------------------------------------------------------------------ facts

/// An outpoint (the explorer shows its transaction).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Outpoint {
    pub transaction_id: Hex,
    pub index: u32,
}

impl Outpoint {
    pub(crate) fn of(u: &Utxo) -> Self {
        Outpoint { transaction_id: u.transaction_id.to_ascii_lowercase(), index: u.index }
    }
}

/// A figure and the live objects it is read from.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fact {
    pub value: Dec,
    pub sources: Vec<Outpoint>,
}

fn fact(value: impl ToString, sources: &[&CovUtxo]) -> Fact {
    Fact { value: value.to_string(), sources: sources.iter().map(|c| Outpoint::of(&c.utxo)).collect() }
}

/// What the node-checked objects of a round fix (spec S3, S4). Absent: no live object fixes it.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoundFacts {
    /// T, the sales deadline: round r's controller (open).
    pub deadline: Option<Fact>,
    /// Open: whether round r - 1 is drawn (the controller's flag): `close()` waits for it.
    pub previous_drawn: Option<bool>,
    /// S: the sum over all k live shards (open), or the marker's (closing).
    pub sales: Option<Fact>,
    /// J: the controller's (open) or the marker's (closing).
    pub jackpot: Option<Fact>,
    /// D = the marker's creation DAA score + seed_offset (closing: the marker and round r + 1's
    /// controller), or the pool's claim deadline - draw_window - claim_window (drawn: a pool and
    /// the drawer).
    pub d: Option<Fact>,
    /// D + draw_window, from which only a void is valid (closing: with the drawer).
    pub void_from: Option<Fact>,
    /// D + draw_window + claim_window (closing: with the drawer; drawn: a pool's).
    pub claim_deadline: Option<Fact>,
    /// The k frozen roots, shard order (the marker's or a pool's).
    pub roots: Option<Vec<Hex>>,
    pub roots_source: Option<Outpoint>,
    /// The caller reward of the round's next transition: C_close (open: the controller and all k
    /// shards) or C_draw (closing: the marker; a void pays it too, D96).
    pub reward: Option<Fact>,
    /// R, the reserve a caller posts per object the transition adds (the controller's).
    pub reserve: Option<Fact>,
    /// The round's live prize pools (drawn), as the node holds them.
    pub pools: Vec<PoolFact>,
    /// F5 review S4: the transaction of the round's `close()`, read from a live object only it
    /// creates: the round's marker, or round r + 1's controller before the draw (drawn = 0).
    pub close_tx: Option<TxFact>,
    /// F5 review S4: the transaction of the round's `draw()` or void, read from a live object only
    /// it creates: the drawer of round r + 1 (the drawer moves on with the next draw). A pool or
    /// the record does not fix it: a claim or a settlement re-creates them in its own transaction.
    pub draw_tx: Option<TxFact>,
}

/// A transaction ID read from the outpoint of a live object that only that transaction creates.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TxFact {
    pub transaction_id: Hex,
    pub source: Outpoint,
}

fn tx_fact(c: &CovUtxo) -> TxFact {
    let source = Outpoint::of(&c.utxo);
    TxFact { transaction_id: source.transaction_id.clone(), source }
}

/// A live prize pool: its tier, the slice one winning combination is owed, and what is left in it.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoolFact {
    pub tier: u8,
    /// "5 + bonus", "5", "4 + bonus", ... (`tickets::tier_name`).
    pub tier_name: String,
    /// J, G2 or slice_t (the pool's state).
    pub slice: Dec,
    /// The UTXO's value, and the prize money left in it (the value less its reserve R).
    pub value: Dec,
    pub remaining: Dec,
    pub outpoint: Outpoint,
}

/// X of an anchor proof (its first 152 bytes: X.hash, ..., DAA score): what the page fetches from a
/// public node by hash (`nodeAnchorBlock`). Read here, not by slicing hex in TypeScript.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnchorX {
    pub anchor_hash: Hex,
    pub anchor_daa_score: Dec,
}

pub fn anchor_x(proof: &str) -> Result<AnchorX> {
    let p = verify::anchor_proof(&unhex("anchorProof", proof)?)?;
    Ok(AnchorX { anchor_hash: hex(&p.x_hash()), anchor_daa_score: p.x.daa_score.to_string() })
}

fn roots_of(field: &[u8], k: i64) -> Option<Vec<Hex>> {
    if field.len() != 32 * K_MAX || !(1..=K_MAX as i64).contains(&k) {
        return None;
    }
    Some(field.chunks_exact(32).take(k as usize).map(hex).collect())
}

/// C_draw of a marker (`game::draw_amounts`), when its shares and sales are in the script's ranges.
fn c_draw(mk: &MarkerState) -> Option<i64> {
    let ok = verify::check_marker(mk).is_ok() && Shares::of_marker(mk).s_h() >= 0 && mk.ref_paid <= mk.sales;
    ok.then(|| game::draw_amounts(mk).c_draw)
}

/// The facts of round `r` from its classified (node-checked) objects.
pub(crate) fn facts(r: i64, c: &Classified) -> Result<RoundFacts> {
    let mut f = RoundFacts::default();
    // Open: round r's controller (and, for S and C_close, every one of its k shards).
    if let Some((co, ct)) = c.controller.as_ref().filter(|(_, ct)| ct.round == r) {
        f.deadline = Some(fact(ct.deadline, &[co]));
        f.previous_drawn = Some(ct.drawn == 1);
        f.jackpot = Some(fact(ct.jackpot, &[co]));
        f.reserve = Some(fact(ct.reserve, &[co]));
        let k = ct.k.clamp(0, K_MAX as i64) as usize;
        let mut by_index: Vec<Option<&(CovUtxo, lottery::state::ShardState)>> = vec![None; k];
        let mut whole = ct.k >= 1 && ct.k <= K_MAX as i64;
        for s in &c.shards {
            match by_index.get_mut(s.1.shard_index.max(0) as usize) {
                Some(slot) if slot.is_none() && s.1.shard_index >= 0 => *slot = Some(s),
                _ => whole = false,
            }
        }
        if whole && by_index.iter().all(Option::is_some) && verify::check_controller(ct).is_ok() {
            let shards: Vec<&(CovUtxo, lottery::state::ShardState)> = by_index.into_iter().flatten().collect();
            let sales = shards.iter().try_fold(0i64, |a, s| a.checked_add(s.1.sales).filter(|x| *x <= game::TOTAL_MAX));
            let ref_paid = shards.iter().try_fold(0i64, |a, s| a.checked_add(s.1.ref_paid));
            if let (Some(sales), Some(ref_paid)) = (sales, ref_paid) {
                let mut src: Vec<&CovUtxo> = vec![co];
                src.extend(shards.iter().map(|s| &s.0));
                f.sales = Some(fact(sales, &shards.iter().map(|s| &s.0).collect::<Vec<_>>()));
                let sh = Shares::of(ct);
                if sh.s_h() >= 0 && ref_paid <= sales {
                    f.reward = Some(fact(game::round_amounts(sales, ref_paid, &sh).c_close, &src));
                }
            }
        }
    }
    // The close's and the draw's transactions (S4): only from objects that transaction alone creates.
    if let Some((mo, _)) = &c.marker {
        f.close_tx = Some(tx_fact(mo));
    } else if let Some((co, _)) = c.controller.as_ref().filter(|(_, ct)| ct.round == r + 1 && ct.drawn == 0) {
        f.close_tx = Some(tx_fact(co));
    }
    if let Some((dro, _)) = c.drawer.as_ref().filter(|(_, dr)| dr.round == r + 1) {
        f.draw_tx = Some(tx_fact(dro));
    } else if let Some((co, _)) = c.controller.as_ref().filter(|(_, ct)| ct.round == r + 1 && ct.drawn == 1) {
        f.draw_tx = Some(tx_fact(co));
    }
    // Closing: the marker, round r + 1's controller and the drawer.
    if let Some((mo, mk)) = &c.marker {
        f.sales = Some(fact(mk.sales, &[mo]));
        f.jackpot = Some(fact(mk.jackpot, &[mo]));
        f.roots = roots_of(&mk.roots, mk.k);
        f.roots_source = Some(Outpoint::of(&mo.utxo));
        if let Some(cd) = c_draw(mk) {
            f.reward = Some(fact(cd, &[mo]));
        }
        if let Some((co, ct)) = c.controller.as_ref().filter(|(_, ct)| ct.round == r + 1) {
            f.reserve = Some(fact(ct.reserve, &[co]));
            let mdaa = convert::dec_i64("marker.blockDaaScore", &mo.utxo.block_daa_score)?;
            let d = game::anchor_daa(mdaa, ct.seed_offset);
            f.d = Some(fact(d, &[mo, co]));
            if let Some((dro, dr)) = c.drawer.as_ref().filter(|(_, dr)| dr.round == r) {
                if let Some(v) = game::void_lock(d, dr.draw_window) {
                    f.void_from = Some(fact(v, &[mo, co, dro]));
                }
                if let Some(cd) = game::claim_deadline(d, dr.draw_window, dr.claim_window) {
                    f.claim_deadline = Some(fact(cd, &[mo, co, dro]));
                }
            }
        }
    }
    for (po, pl) in &c.pools {
        let value = convert::dec("pool.amount", &po.utxo.amount)?;
        let Some(remaining) = u64::try_from(pl.reserve).ok().and_then(|r| value.checked_sub(r)) else { continue };
        f.pools.push(PoolFact {
            tier: pl.tier,
            tier_name: crate::tickets::tier_name(pl.tier).unwrap_or_default(),
            slice: pl.slice.to_string(),
            value: value.to_string(),
            remaining: remaining.to_string(),
            outpoint: Outpoint::of(&po.utxo),
        });
    }
    // Drawn: a pool (they agree on the result: `check_round` compares their numbers) and the drawer.
    if let Some((po, pl)) = c.pools.first() {
        f.claim_deadline = Some(fact(pl.deadline, &[po]));
        f.roots = roots_of(&pl.roots, pl.k);
        f.roots_source = Some(Outpoint::of(&po.utxo));
        if let Some((dro, dr)) = c.drawer.as_ref().filter(|(_, dr)| dr.round > r) {
            let d = pl.deadline.checked_sub(dr.draw_window).and_then(|x| x.checked_sub(dr.claim_window));
            if let Some(d) = d.filter(|d| game::claim_deadline(*d, dr.draw_window, dr.claim_window) == Some(pl.deadline)) {
                f.d = Some(fact(d, &[po, dro]));
            }
        }
    }
    Ok(f)
}

// ------------------------------------------------------------------------------ verify (J6)

/// The result of one check (spec 4, "Round row and round detail": match, mismatch, not checkable).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VerifyResult {
    Match,
    Mismatch,
    NotCheckable,
}

/// What a check compares.
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VerifyKind {
    /// One object's state bytes against its live UTXO (the script they rebuild, the instance).
    State,
    /// The round's status as the live objects show it, against the indexer's.
    Status,
    /// The indexer's frozen roots against the chain's (the marker's or a pool's).
    Roots,
    /// One shard's tree, rebuilt from the indexer's purchases, against the chain's root.
    Tree,
    /// The winning numbers recomputed from the anchor proof, against the record (the pools).
    Numbers,
    /// The anchor block X.hash against a public node's header and selected chain.
    Anchor,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifyItem {
    pub kind: VerifyKind,
    pub result: VerifyResult,
    /// `state`: the object's role and outpoint.
    pub role: Option<RoleName>,
    pub outpoint: Option<Outpoint>,
    /// `tree`: the shard's index.
    pub shard: Option<u32>,
    /// A mismatch's or a not-checkable item's code (the error table's name), and its detail: for
    /// logs and agents, never page copy.
    pub code: Option<String>,
    pub detail: String,
}

/// `verifyRound`: every J6 check of a round, each with its own result.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoundVerify {
    pub round: Dec,
    pub items: Vec<VerifyItem>,
    /// No item is a mismatch.
    pub consistent: bool,
    /// The numbers recomputed from the anchor proof, when they were (`final` against the pools,
    /// `provisional` before the draw).
    pub winning: Option<WinningOut>,
    /// X.hash bound to a public node's header and chain (`CheckedRound.anchorBound`).
    pub anchor_bound: bool,
}

fn item(kind: VerifyKind, result: VerifyResult, e: Option<&Error>) -> VerifyItem {
    VerifyItem { kind, result, role: None, outpoint: None, shard: None, code: e.map(|e| e.code.as_str().to_string()), detail: e.map(|e| e.detail.clone()).unwrap_or_default() }
}

fn not_checkable(kind: VerifyKind, why: &str) -> VerifyItem {
    VerifyItem { code: None, detail: why.to_string(), ..item(kind, VerifyResult::NotCheckable, None) }
}

/// Spec J6, item by item. The input is `checkIndexerRound`'s (the round as the indexer serves it,
/// the node's entries for its objects, its purchases, its anchor proof and, for X, the anchor
/// block a public node reports by X.hash). Each check runs on its own, so one lie shows as one
/// mismatch:
/// - each object: its entry on the node (none: not checkable, it is spent) and its state against
///   it (`check_object`);
/// - the status, from the objects that matched (the indexer's draw and close left out);
/// - the indexer's frozen roots, with its close;
/// - each shard's tree from its purchases, against the marker's or a pool's root (or, open, the
///   live shard's);
/// - the numbers from the proof, against the pools and the indexer's draw (`final`), or bound to
///   the node's anchor block before the draw (`provisional`: not checkable until `draw()`);
/// - X.hash against the node's anchor block (closing, and drawn when the node still has X).
pub fn verify_round(net: Net, i: &RoundCheckInput) -> Result<RoundVerify> {
    use VerifyKind as K;
    use VerifyResult as V;
    let cov = cov_of(&i.covenant_id)?;
    let a = &i.round;
    let r = convert::dec_i64("round.round", &a.round)?;
    if a.objects.len() > 64 || i.utxos.len() > 256 {
        return Err(Error::new(Code::InputRange, format!("{} objects, {} UTXOs", a.objects.len(), i.utxos.len())));
    }
    let mut items = vec![];
    // Each object on its own.
    let mut live = vec![];
    for o in &a.objects {
        let op = Outpoint { transaction_id: o.transaction_id.to_ascii_lowercase(), index: o.index };
        let one = indexer::classify(net, &cov, r, std::slice::from_ref(o), &i.utxos);
        let has_entry = i.utxos.iter().any(|u| u.transaction_id.eq_ignore_ascii_case(&o.transaction_id) && u.index == o.index);
        let mut it = match (&one, has_entry) {
            (_, false) => not_checkable(K::State, "the node has no live UTXO at this outpoint (spent, or never created)"),
            (Ok(_), true) => item(K::State, V::Match, None),
            (Err(e), true) => item(K::State, V::Mismatch, Some(e)),
        };
        it.role = Some(o.role);
        it.outpoint = Some(op);
        if one.is_ok() {
            live.push(o.clone());
        }
        items.push(it);
    }
    let input = |round: ApiRound, with_purchases: bool, with_proof: bool, with_block: bool| RoundCheckInput {
        covenant_id: i.covenant_id.clone(),
        round,
        utxos: i.utxos.clone(),
        purchases: if with_purchases { i.purchases.clone() } else { None },
        anchor_proof: if with_proof { i.anchor_proof.clone() } else { None },
        anchor_block: if with_block { i.anchor_block.clone() } else { None },
    };
    let base = ApiRound { objects: live.clone(), draw: None, close: None, ..a.clone() };
    // The status.
    let st = indexer::check_round(net, &input(base.clone(), false, false, false));
    let cl = indexer::classify(net, &cov, r, &live, &i.utxos).ok();
    let (has_marker, has_pool) = cl.as_ref().map(|c| (c.marker.is_some(), !c.pools.is_empty())).unwrap_or((false, false));
    let base_ok = st.is_ok();
    items.push(match &st {
        Ok(c) if c.status_checked => item(K::Status, V::Match, None),
        Ok(c) if c.void_supported => not_checkable(K::Status, "a void leaves no live object of the round; the chain does not contradict it"),
        Ok(_) => not_checkable(K::Status, "no live object of the round shows its status"),
        Err(e) => item(K::Status, V::Mismatch, Some(e)),
    });
    let blocked = |kind: K| not_checkable(kind, "the round's objects do not match the chain");
    // The indexer's frozen roots against the chain's.
    let with_close = ApiRound { close: a.close.clone(), ..base.clone() };
    if a.close.is_some() {
        items.push(if !base_ok {
            blocked(K::Roots)
        } else if !(has_marker || has_pool) {
            not_checkable(K::Roots, "no live marker or pool of the round holds the frozen roots")
        } else {
            match indexer::check_round(net, &input(with_close.clone(), false, false, false)) {
                Ok(_) => item(K::Roots, V::Match, None),
                Err(e) => item(K::Roots, V::Mismatch, Some(&e)),
            }
        });
    }
    // Each shard's tree.
    if let Some(p) = &i.purchases {
        items.extend(trees(r, a, p, cl.as_ref().filter(|_| base_ok)));
    }
    // The winning numbers, then X.hash.
    let (mut winning, mut anchor_bound) = (None, false);
    let full = ApiRound { draw: a.draw.clone(), ..with_close.clone() };
    let drawn = a.draw.is_some() || has_pool;
    if i.anchor_proof.is_none() {
        if drawn || has_marker {
            items.push(not_checkable(K::Numbers, "the indexer serves no anchor proof for the round"));
            items.push(not_checkable(K::Anchor, "the indexer serves no anchor proof for the round"));
        }
    } else if !base_ok {
        items.push(blocked(K::Numbers));
        items.push(blocked(K::Anchor));
    } else if has_pool || !has_marker {
        // Drawn (or nothing fixes the numbers): the proof's numbers against the pools.
        match indexer::check_round(net, &input(full.clone(), false, true, false)) {
            Ok(c) => match c.winning {
                Some(w) => {
                    items.push(item(K::Numbers, V::Match, None));
                    winning = Some(w);
                    if i.anchor_block.is_none() {
                        items.push(not_checkable(K::Anchor, "the node does not report the anchor block X"));
                    } else {
                        match indexer::check_round(net, &input(full.clone(), false, true, true)) {
                            Ok(c) if c.anchor_bound => {
                                items.push(item(K::Anchor, V::Match, None));
                                anchor_bound = true;
                            }
                            Ok(_) => items.push(not_checkable(K::Anchor, "no live pool and drawer to bind the anchor")),
                            Err(e) => items.push(item(K::Anchor, V::Mismatch, Some(&e))),
                        }
                    }
                }
                None => {
                    items.push(not_checkable(K::Numbers, "no live pool of the round and the drawer: the record is gone"));
                    items.push(not_checkable(K::Anchor, "no live pool of the round and the drawer: the record is gone"));
                }
            },
            Err(e) => {
                items.push(item(K::Numbers, V::Mismatch, Some(&e)));
                items.push(not_checkable(K::Anchor, "the numbers do not match"));
            }
        }
    } else {
        // Closing: provisional numbers, X bound to the node's anchor block.
        if i.anchor_block.is_none() {
            items.push(not_checkable(K::Numbers, "provisional until draw(); the node does not report the anchor block X"));
            items.push(not_checkable(K::Anchor, "the node does not report the anchor block X"));
        } else {
            match indexer::check_round(net, &input(full.clone(), false, true, true)) {
                Ok(c) => match c.winning {
                    Some(w) => {
                        items.push(not_checkable(K::Numbers, "provisional until draw()"));
                        items.push(item(K::Anchor, V::Match, None));
                        anchor_bound = c.anchor_bound;
                        winning = Some(w);
                    }
                    None => {
                        items.push(not_checkable(K::Numbers, "the round's marker and next controller are not both live"));
                        items.push(not_checkable(K::Anchor, "the round's marker and next controller are not both live"));
                    }
                },
                Err(e) => {
                    items.push(item(K::Numbers, V::Mismatch, Some(&e)));
                    items.push(item(K::Anchor, V::Mismatch, Some(&e)));
                }
            }
        }
    }
    let consistent = items.iter().all(|x| x.result != V::Mismatch);
    Ok(RoundVerify { round: r.to_string(), items, consistent, winning, anchor_bound })
}

/// One `tree` item per shard: its purchases rebuilt (`verify::rebuild_tree_bytes`) against the
/// chain's root i (the marker's or a pool's frozen roots; open: the live shard's, with round r's
/// controller fixing m and k).
fn trees(r: i64, a: &ApiRound, p: &indexer::ApiPurchases, cl: Option<&Classified>) -> Vec<VerifyItem> {
    use VerifyKind as K;
    use VerifyResult as V;
    let k = a.k as usize;
    let at = |n: usize, mut it: VerifyItem| {
        it.shard = Some(n as u32);
        it
    };
    let Some(cl) = cl else {
        return (0..k.min(K_MAX)).map(|n| at(n, not_checkable(K::Tree, "the round's objects do not match the chain"))).collect();
    };
    if k == 0 || k > K_MAX || p.shards.len() != k || convert::dec_i64("purchases.round", &p.round).ok() != Some(r) || Some(p.m) != a.m || p.k != a.k {
        let e = Error::new(Code::ChainMismatch, format!("round {r}: the purchases' round, m or k differ from the round's"));
        return (0..k.min(K_MAX)).map(|n| at(n, item(K::Tree, V::Mismatch, Some(&e)))).collect();
    }
    // The chain's (m, root per shard).
    let chain: Option<(i64, Vec<Option<[u8; 32]>>)> = if let Some((_, mk)) = &cl.marker {
        Some((mk.m, root_list(&mk.roots, mk.k, k)))
    } else if let Some((_, pl)) = cl.pools.first() {
        Some((pl.m, root_list(&pl.roots, pl.k, k)))
    } else if let Some((_, ct)) = cl.controller.as_ref().filter(|(_, c)| c.round == r) {
        let mut v = vec![None; k];
        if ct.k as usize == k && Some(ct.m) == a.m.map(i64::from) {
            for (_, s) in &cl.shards {
                if let Some(slot) = v.get_mut(s.shard_index.max(0) as usize) {
                    *slot = Some(s.root);
                }
            }
        }
        Some((ct.m, v))
    } else {
        None
    };
    let Some((m, roots)) = chain else {
        return (0..k).map(|n| at(n, not_checkable(K::Tree, "no live marker, pool or open controller of the round holds its roots"))).collect();
    };
    (0..k)
        .map(|n| {
            let Some(root) = roots.get(n).copied().flatten() else {
                return at(n, not_checkable(K::Tree, "the shard is not live on the node"));
            };
            let rebuilt = unhex("purchases.shards", &p.shards[n]).and_then(|b| verify::rebuild_tree_bytes(m, &b));
            at(
                n,
                match rebuilt {
                    Ok(t) if t.root() == root => item(K::Tree, V::Match, None),
                    Ok(_) => item(K::Tree, V::Mismatch, Some(&Error::new(Code::TreeMismatch, format!("shard {n}'s purchases do not rebuild the chain's root")))),
                    Err(e) => item(K::Tree, V::Mismatch, Some(&e)),
                },
            )
        })
        .collect()
}

fn root_list(field: &[u8], chain_k: i64, k: usize) -> Vec<Option<[u8; 32]>> {
    if field.len() != 32 * K_MAX || chain_k as usize != k {
        return vec![None; k];
    }
    field.chunks_exact(32).take(k).map(|c| c.try_into().ok()).collect()
}

// ------------------------------------------------------------------------------ expired pools (S4, F5 review S1)

/// `absorbablePools`: the expired pools a `draw()` (or void) of the marker's round may absorb,
/// from candidate objects (the indexer's `GET /v1/pools`, which only says where to look) and the
/// node's entries for their outpoints.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AbsorbInput {
    pub covenant_id: Hex,
    /// The round's marker, round r + 1's controller and the drawer: the objects the transition
    /// spends (D, the void lock and R come from them).
    pub marker: CovUtxo,
    pub controller: CovUtxo,
    pub drawer: CovUtxo,
    /// Void (true) or draw.
    pub void: bool,
    /// The node's virtual DAA score: only a pool whose claim deadline is below it is absorbed
    /// (the transaction's lock time is at least every absorbed deadline).
    pub virtual_daa_score: Dec,
    pub objects: Vec<ApiObject>,
    /// The node's entries for the objects' outpoints (an object without one is spent: skipped).
    pub utxos: Vec<Utxo>,
}

/// The pools to absorb, in input order (`draw.rs` inputs 4..), at most A_MAX.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Absorbable {
    pub pools: Vec<CovUtxo>,
    /// Expired pools found beyond A_MAX (a later draw absorbs them).
    pub more: u32,
    /// Candidates left out: not live on the node, or not an absorbable pool of this instance (the
    /// indexer's word is never used; a wrong candidate costs nothing but its place). For logs.
    pub skipped: u32,
}

/// Spec S4, design 7.3 ("any prize pools past their deadline"), drawer.sil check 18 and pool.sil
/// `absorb_leg`: a candidate is absorbed when its node entry and state bytes rebuild a pool of
/// this instance (`check_object`), of an earlier round than the marker's, with its claim
/// deadline below the node's virtual DAA score and, for a draw, below the void lock (so the
/// draw's lock time max(D, deadlines) stays a draw's), its R the controller's (check 15 takes
/// Q = value - R with the controller's R) and its value at least R. Oldest deadline first (then
/// round, tier, outpoint); at most A_MAX; each outpoint once.
pub fn absorbable_pools(net: Net, i: &AbsorbInput) -> Result<Absorbable> {
    use lottery::web::cases;
    let cov = cov_of(&i.covenant_id)?;
    if i.objects.len() > indexer::OBJECTS_MAX as usize || i.utxos.len() > 2 * indexer::OBJECTS_MAX as usize {
        return Err(Error::new(Code::InputRange, format!("{} objects, {} UTXOs", i.objects.len(), i.utxos.len())));
    }
    let mk = cases::marker(&convert::obj("marker", &i.marker)?, &cov)?;
    let co = convert::obj("controller", &i.controller)?;
    let ct = cases::controller(&co, &cov)?;
    let dr = cases::drawer(&convert::obj("drawer", &i.drawer)?, &cov)?;
    verify::check_controller(&ct)?;
    verify::check_drawer(&dr)?;
    if ct.round != mk.round + 1 || dr.round != mk.round {
        return Err(Error::new(Code::ChainMismatch, "the controller and drawer are not the marker's round's"));
    }
    let now = convert::dec_i64("virtualDaaScore", &i.virtual_daa_score)?;
    let mdaa = convert::dec_i64("marker.blockDaaScore", &i.marker.utxo.block_daa_score)?;
    let d = game::anchor_daa(mdaa, ct.seed_offset);
    let void_from = game::void_lock(d, dr.draw_window).ok_or_else(|| Error::new(Code::InputRange, "no void lock"))?;
    let below = if i.void { now } else { now.min(void_from) };
    let mut found: Vec<(i64, i64, u8, String, u32, CovUtxo)> = vec![];
    let mut skipped = 0u32;
    for o in &i.objects {
        let ok = indexer::check_object(net, &cov, o, &i.utxos).ok().and_then(|(c, s)| match s {
            lottery::index::State::Pool(p) => Some((c.object, p)),
            _ => None,
        });
        let Some((obj, p)) = ok else {
            skipped += 1;
            continue;
        };
        let value = convert::dec("pool.amount", &obj.utxo.amount)?;
        let fits = verify::check_pool(&p).is_ok()
            && p.round < mk.round
            && p.deadline < below
            && p.reserve == ct.reserve
            && u64::try_from(p.reserve).is_ok_and(|r| value >= r);
        let key = (obj.utxo.transaction_id.to_ascii_lowercase(), obj.utxo.index);
        if !fits || found.iter().any(|f| (f.3.as_str(), f.4) == (key.0.as_str(), key.1)) {
            skipped += u32::from(!fits);
            continue;
        }
        found.push((p.deadline, p.round, p.tier, key.0, key.1, obj));
    }
    found.sort_by(|a, b| (a.0, a.1, a.2, &a.3, a.4).cmp(&(b.0, b.1, b.2, &b.3, b.4)));
    let more = found.len().saturating_sub(game::A_MAX) as u32;
    Ok(Absorbable { pools: found.into_iter().take(game::A_MAX).map(|f| f.5).collect(), more, skipped })
}

// ------------------------------------------------------------------------------ caller figures (S4)

/// The transition a caller panel builds.
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CallerKind {
    Close,
    Draw,
    Void,
}

/// `callerFigures`: a built `close()`, `draw()` or void and the chain objects its reward and
/// reserve come from (the same objects `buildClose`/`buildDraw` took).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallerFiguresInput {
    pub covenant_id: Hex,
    /// close: round r's controller; draw and void: round r + 1's (the one `draw()` spends).
    pub controller: CovUtxo,
    /// close: the round's k shards.
    #[serde(default)]
    pub shards: Vec<CovUtxo>,
    /// draw and void: the round's marker, the drawer and the vault (whether a settlement
    /// record is posted: anything pending in it, spec M5 A5 C3).
    #[serde(default)]
    pub marker: Option<CovUtxo>,
    #[serde(default)]
    pub drawer: Option<CovUtxo>,
    #[serde(default)]
    pub vault: Option<CovUtxo>,
    /// draw and void: the expired pools it absorbs (`absorbablePools`), in input order.
    #[serde(default)]
    pub absorbed: Vec<CovUtxo>,
    pub built: BuiltTx,
}

/// The S4 rows of a built caller transaction (spec S4, 7), every one bound to it. `total` =
/// `reserve` + `fee` - `reward` - `refund`: what leaves the wallet now (negative: it gains).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallerFigures {
    /// close, draw or void (a void: `draw()` with the empty anchor, its lock time at
    /// max(D + draw_window, the absorbed deadlines); a draw's at max(D, the absorbed deadlines)).
    pub kind: CallerKind,
    pub round: Dec,
    /// C_close or C_draw.
    pub reward: Dec,
    pub fee: Dec,
    /// The reserves posted for the objects the transaction adds (design 5.5: each repays R to
    /// this wallet's address when it is finally consumed), and how many.
    pub reserve: Dec,
    pub reserves: u32,
    /// close: the marker's R comes back with the round's `draw()` or void.
    pub reserve_back_with_draw: bool,
    /// draw: how many new prize pools it creates (one R each), and from when their R come back:
    /// a later `draw()` absorbs them from their claim deadline (DAA score; F5 review S1: the
    /// page's draws absorb expired pools, oldest first, A_MAX at a time).
    pub pools: u32,
    pub reserve_back_from: Option<Dec>,
    /// draw or void: a settlement record is posted (one R), back to this wallet once the
    /// round's settlements drain it (record.sil).
    pub record: bool,
    /// draw or void: the expired pools absorbed (their prize money to the boost pot, their R to
    /// their funders).
    pub absorbed: u32,
    /// Reserves paid back to this wallet in this transaction (it funded the marker or an absorbed pool).
    pub refund: Dec,
    /// The caller output (the last output: the reward, the change and nothing else).
    pub caller_output: u32,
    /// What leaves the wallet now, sompi (a signed decimal).
    pub total: String,
    /// The fee is larger than the reward (design 12 liveness note; shown as a warning,
    /// `fee_above_reward`).
    pub fee_above_reward: bool,
}

fn bound(what: &str) -> Error {
    Error::new(Code::BuiltMismatch, format!("caller figures: {what}"))
}

/// Spec S4, 7: the caller panel's figures of a built `close()`, `draw()` or void.
pub fn caller_figures(net: Net, i: &CallerFiguresInput) -> Result<CallerFigures> {
    use lottery::web::cases;
    let cov = cov_of(&i.covenant_id)?;
    let u = unsigned_of(net, &i.built)?;
    let input_at = |c: &CovUtxo| -> Result<usize> {
        let o = convert::obj("object", c)?;
        u.tx.inputs
            .iter()
            .zip(&u.entries)
            .position(|(inp, e)| inp.previous_outpoint == o.outpoint && *e == o.entry)
            .ok_or_else(|| bound("a given object is not an input of the transaction"))
    };
    let spent = |c: &CovUtxo| input_at(c).map(|_| ());
    let co = convert::obj("controller", &i.controller)?;
    let ct = cases::controller(&co, &cov)?;
    verify::check_controller(&ct)?;
    spent(&i.controller)?;
    let r_res = u64::try_from(ct.reserve).map_err(|_| bound("a negative reserve"))?;
    let n_cov = u.tx.outputs.iter().filter(|o| o.covenant.is_some()).count() as u64;
    let (kind, round, reward, back_from, rec) = match (&i.marker, &i.drawer) {
        (None, None) => {
            if i.vault.is_some() || !i.absorbed.is_empty() {
                return Err(bound("close: a vault or absorbed pools given"));
            }
            // close(): the reward from the controller and all k shards (design 7.2).
            if i.shards.len() != ct.k as usize || i.shards.is_empty() || i.shards.len() > K_MAX {
                return Err(bound("close: not the controller's k shards"));
            }
            let (mut sales, mut ref_paid) = (0i64, 0i64);
            let mut seen = vec![false; i.shards.len()];
            for s in &i.shards {
                let so = convert::obj("shards", s)?;
                let st = cases::shard(&so, &cov)?;
                verify::check_shard(&st)?;
                let slot = seen.get_mut(st.shard_index.max(0) as usize).ok_or_else(|| bound("close: a shard index outside k"))?;
                if *slot || st.round != ct.round || st.shard_index < 0 {
                    return Err(bound("close: a shard twice, or of another round"));
                }
                *slot = true;
                spent(s)?;
                sales = sales.checked_add(st.sales).filter(|x| *x <= game::TOTAL_MAX).ok_or_else(|| bound("close: sales above TOTAL_MAX"))?;
                ref_paid = ref_paid.checked_add(st.ref_paid).ok_or_else(|| bound("close: referrals overflow"))?;
            }
            let sh = Shares::of(&ct);
            if sh.s_h() < 0 || ref_paid > sales {
                return Err(bound("close: shares or referrals out of range"));
            }
            (CallerKind::Close, ct.round, game::round_amounts(sales, ref_paid, &sh).c_close, None, false)
        }
        (Some(m), Some(d)) => {
            // draw() or void: C_draw from the marker (design 7.3; D96: a void pays it too).
            let mo = convert::obj("marker", m)?;
            let mk = cases::marker(&mo, &cov)?;
            let dro = convert::obj("drawer", d)?;
            let dr = cases::drawer(&dro, &cov)?;
            verify::check_drawer(&dr)?;
            spent(m)?;
            spent(d)?;
            if ct.round != mk.round + 1 || dr.round != mk.round {
                return Err(bound("draw: the controller and drawer are not the marker's round's"));
            }
            let vt = i.vault.as_ref().ok_or_else(|| bound("draw: the vault is not given"))?;
            let vs = cases::vault(&convert::obj("vault", vt)?, &cov)?;
            verify::check_vault(&vs)?;
            spent(vt)?;
            let rec = vs.pend_dep > 0 || vs.pend_wsh > 0;
            // The absorbed pools: inputs 4.. in the given order (drawer.sil check 18).
            if i.absorbed.len() > game::A_MAX {
                return Err(bound("draw: more absorbed pools than A_MAX"));
            }
            let mut last_deadline: Option<i64> = None;
            for (n, a) in i.absorbed.iter().enumerate() {
                let p = cases::pool(&convert::obj("absorbed", a)?, &cov)?;
                verify::check_pool(&p)?;
                if input_at(a)? != 4 + n || p.round >= mk.round {
                    return Err(bound("draw: an absorbed pool not at its input, or not of an earlier round"));
                }
                last_deadline = Some(last_deadline.map_or(p.deadline, |x| x.max(p.deadline)));
            }
            // Every covenant input is named: drawer, marker, vault, controller and the absorbed
            // pools; the rest are the wallet's coins (the inputs it signs).
            if u.tx.inputs.len() != 4 + i.absorbed.len() + u.sign.len() {
                return Err(bound("draw: the transaction spends covenant inputs not given"));
            }
            let cd = c_draw(&mk).ok_or_else(|| bound("draw: the marker's shares are out of range"))?;
            let dd = game::anchor_daa(mo.entry.block_daa_score as i64, ct.seed_offset);
            let void_from = game::void_lock(dd, dr.draw_window).ok_or_else(|| bound("draw: no void lock"))?;
            // The kind from the lock time (draw.rs `fix_lock_time`): a draw's is max(D, the
            // absorbed deadlines), a void's max(D + draw_window, the absorbed deadlines). Where
            // both are the same (an absorbed deadline at or past the void lock) the lock time does
            // not fix the kind: refused.
            let with = |x: i64| last_deadline.map_or(x, |l| l.max(x)) as u64;
            let void = match (u.tx.lock_time == with(void_from), u.tx.lock_time == with(dd)) {
                (true, false) => true,
                (false, true) => false,
                _ => return Err(bound("draw: the lock time is not one draw's or one void's")),
            };
            let cdl = game::claim_deadline(dd, dr.draw_window, dr.claim_window).ok_or_else(|| bound("draw: no claim deadline"))?;
            (if void { CallerKind::Void } else { CallerKind::Draw }, mk.round, cd, (!void).then_some(cdl), rec)
        }
        _ => return Err(bound("a draw needs both its marker and the drawer")),
    };
    // The caller output: the last output, plain, paying a script the wallet signs for.
    let signed_spks: Vec<_> = u.sign.iter().map(|s| u.entries[s.index].script_public_key.clone()).collect();
    if signed_spks.is_empty() {
        return Err(bound("the wallet signs no input"));
    }
    let last = u.tx.outputs.len().checked_sub(1).ok_or_else(|| bound("no outputs"))?;
    let to_wallet = |o: &kaspa_consensus_core::tx::TransactionOutput| o.covenant.is_none() && signed_spks.contains(&o.script_public_key);
    let caller = &u.tx.outputs[last];
    if !to_wallet(caller) {
        return Err(bound("the last output does not pay the wallet"));
    }
    let wallet_in: u64 = u.sign.iter().try_fold(0u64, |a, s| a.checked_add(u.entries[s.index].amount)).ok_or_else(|| bound("funding overflows"))?;
    let refund: u64 = u.tx.outputs[..last].iter().filter(|o| to_wallet(o)).try_fold(0u64, |a, o| a.checked_add(o.value)).ok_or_else(|| bound("refunds overflow"))?;
    let reward_u = u64::try_from(reward).map_err(|_| bound("a negative reward"))?;
    // funding - posted - fee + reward = caller output.
    let posted = (wallet_in as i128) + (reward_u as i128) - (u.fee as i128) - (caller.value as i128);
    if posted < 0 || r_res == 0 || posted % (r_res as i128) != 0 {
        return Err(bound("the caller output is not funding - n R - fee + reward"));
    }
    let n = (posted / r_res as i128) as u64;
    // close: the marker's R (n = 1). draw and void: one R per new pool and one for the record,
    // which with drawer', vault' and controller' are the covenant outputs (n_cov = 3 + np + rec,
    // spec M5 A5 check 13); a void creates no pool.
    let pools = n.checked_sub(rec as u64);
    let n_ok = match kind {
        CallerKind::Close => n == 1,
        CallerKind::Draw => pools.is_some_and(|p| p <= 7) && n_cov == 3 + n,
        CallerKind::Void => pools == Some(0) && n_cov == 3 + n,
    };
    if !n_ok {
        return Err(bound(&format!("{n} reserves posted for a {kind:?} ({n_cov} covenant outputs, record {rec})")));
    }
    let pools = if kind == CallerKind::Close { 0 } else { pools.unwrap_or(0) };
    // Cross-check with the summary the page shows (`unsigned_of` already rebuilt it from the tx).
    let back = dec("summary.walletBack", &i.built.summary.wallet_back)?;
    if dec("summary.walletIn", &i.built.summary.wallet_in)? != wallet_in || back != caller.value + refund {
        return Err(bound("the summary is not the funding and what comes back"));
    }
    let total = wallet_in as i128 - back as i128;
    Ok(CallerFigures {
        kind,
        round: round.to_string(),
        reward: reward.to_string(),
        fee: u.fee.to_string(),
        reserve: (n * r_res).to_string(),
        reserves: n as u32,
        reserve_back_with_draw: kind == CallerKind::Close,
        pools: pools as u32,
        reserve_back_from: back_from.filter(|_| pools > 0).map(|x| x.to_string()),
        record: rec,
        absorbed: i.absorbed.len() as u32,
        refund: refund.to_string(),
        caller_output: last as u32,
        total: total.to_string(),
        fee_above_reward: u.fee > reward_u,
    })
}
