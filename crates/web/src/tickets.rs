//! My tickets (spec frontend S2, 4 "Ticket card" and "Claim row", 7 "Claims", "Deposits",
//! "Voids"; front end F4): every figure a ticket card, a claim row or a deposit return shows,
//! computed here from chain objects, so the page holds no prize, tier or deadline arithmetic.
//!
//! - `ticket_card`: the ticket (the node's entry and its state bytes), the live pools of its
//!   round and the round's purchase lists give each combination's tier, its matched numbers,
//!   and for a claimable one the claim the chain would take now: prize = floor(slice / h), h
//!   from all k frozen trees, and the mode (PAY, ACCRUE below P_MIN, FINAL with the ticket's
//!   value; D56). Also when the ticket's value comes back (with the last claim, or from the
//!   date `return_deposit()` is valid) and whether `return_deposit()` is valid now (path b
//!   alone from T + D_RET, path a with an expired pool of the round; `ticket.sil` A5).
//! - `claim_figures` / `return_figures`: the figures of a built `claim()` / `return_deposit()`,
//!   each checked against the transaction (`built_mismatch` otherwise), as `buyFigures` is.
//!
//! The objects are checked as every builder checks them (`cases::live`: the state against the
//! UTXO's script under the instance's covenant ID); that an entry is live is the page's (it reads
//! each from a public node, F1 re-review N1).

use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
use lottery::claim::payout_spk;
use lottery::game::{ClaimMode, D_RET, P_MIN, tier_of};
use lottery::index::State;
use lottery::state::{PoolState, TicketState};
use lottery::web::build::{Coin, Funds as LFunds};
use lottery::web::cases::{self, Obj};
use lottery::web::{Code, Error, Result, verify};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::api::{cov_of, purchase_lists, unsigned_of};
use crate::convert::{self, Net, dec, hex, obj};
use crate::dto::{Address, BuiltTx, Combination, CovUtxo, Dec, Hex, TicketOut};

/// The mode a claim takes (D56).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ClaimModeName {
    /// The prize is paid now; the ticket keeps its value.
    Pay,
    /// The prize is below P_MIN: it accrues in the ticket and is paid with its last claim.
    Accrue,
    /// The ticket's last unclaimed index: the prize and the ticket's value are paid now.
    Final,
}

impl From<ClaimMode> for ClaimModeName {
    fn from(m: ClaimMode) -> Self {
        match m {
            ClaimMode::Pay => ClaimModeName::Pay,
            ClaimMode::Accrue => ClaimModeName::Accrue,
            ClaimMode::Final => ClaimModeName::Final,
        }
    }
}

/// `ticketCard`.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TicketCardInput {
    pub covenant_id: Hex,
    /// The ticket: a public node's entry for its outpoint, with its state bytes.
    pub ticket: CovUtxo,
    /// The live pools of the ticket's round (the node's entries, with the pools' state bytes
    /// from the round result); empty before the draw, after a void, or once every pool is gone.
    #[serde(default)]
    pub pools: Vec<CovUtxo>,
    /// The round's purchase lists (all k, shard order, checked against the frozen roots): the
    /// counts behind h. Needed when a pool is given.
    #[serde(default)]
    pub purchases: Option<Vec<Hex>>,
    /// The round's winning numbers as the stored round result has them (checked against a
    /// pool when it was taken), for a drawn round whose pools are gone. Must equal the pools'.
    #[serde(default)]
    pub winning: Option<Combination>,
    /// The stored claim deadline of the round (with `winning`).
    #[serde(default)]
    pub claim_deadline: Option<Dec>,
    /// The round ended in a void (no winners, design 10, D96).
    #[serde(default)]
    pub void: bool,
    /// The node's virtual DAA score.
    pub virtual_daa_score: Dec,
}

/// Where the ticket's round is, as far as the card goes.
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CardStage {
    /// Not drawn yet (open or closing).
    Open,
    Drawn,
    /// No draw: void (D96). The ticket can still get its value back by `return_deposit()`.
    Void,
}

/// A combination's claim, as the chain would take it now.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaimView {
    pub tier: u8,
    /// The tier's name: matched main numbers, "+ bonus" when the bonus matches ("4 + bonus").
    pub tier_name: String,
    /// The combination's slice: slice_t, G2 or J (sompi).
    pub slice: Dec,
    /// Holdings of this exact combination over all k shards.
    pub h: Dec,
    /// h - 1: the other holdings it is shared with.
    pub shared_with: Dec,
    /// floor(slice / h) (sompi).
    pub prize: Dec,
    pub mode: ClaimModeName,
    /// The prize is below P_MIN (it accrues unless this is the ticket's last claim).
    pub below_p_min: bool,
}

/// What a combination can still do.
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ComboStatus {
    /// Before the draw.
    Open,
    /// Drawn: it wins no tier (or the round was void).
    NoWin,
    /// It wins a tier whose pool is live: the claim can be made now.
    Claimable,
    /// Claimed already (its bit is set in the ticket).
    Claimed,
    /// It wins a tier, but no live pool of that tier is left (absorbed after the claim deadline).
    Expired,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComboCard {
    pub index: u32,
    pub combination: Combination,
    /// After the draw: whether each of the five main numbers is a winning number.
    pub matched_main: Option<[bool; 5]>,
    /// After the draw: whether the bonus is the winning bonus.
    pub bonus_matched: Option<bool>,
    pub tier: Option<u8>,
    /// The tier's name ("4 + bonus"), when it wins one.
    pub tier_name: Option<String>,
    pub status: ComboStatus,
    pub claim: Option<ClaimView>,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReturnPathName {
    /// `return_deposit_after()`: the ticket alone, from T + D_RET.
    Alone,
    /// `return_deposit_with_pool()`: with a pool of the round past its claim deadline.
    WithPool,
}

/// `return_deposit()` is valid now: which path, and the pool for path a.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Returnable {
    pub path: ReturnPathName,
    pub pool: Option<CovUtxo>,
}

/// A ticket card (spec S2, 4).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TicketCard {
    pub round: Dec,
    /// T, the round's sales deadline (DAA score).
    pub deadline: Dec,
    pub b: u32,
    /// The ticket's value (sompi): its deposits b x d plus the prizes accrued in it; what its
    /// last claim or `return_deposit()` pays back.
    pub value: Dec,
    pub payout: Address,
    pub stage: CardStage,
    pub winning: Option<Combination>,
    /// D_claim (DAA score): the claims are guaranteed until then (design 4.5).
    pub claim_deadline: Option<Dec>,
    pub combos: Vec<ComboCard>,
    /// Indices claimed so far.
    pub claimed: u32,
    /// The value comes back with the last claim (an unclaimed index wins a tier with a live pool).
    pub back_with_last_claim: bool,
    /// From this DAA score `return_deposit()` is valid: T + D_RET, or earlier with a pool of the
    /// round once its claim deadline has passed.
    pub return_from: Dec,
    pub returnable: Option<Returnable>,
}

fn combination_at(t: &TicketState, idx: usize) -> Option<Combination> {
    let s = idx.checked_mul(6)?;
    let c = t.bundle.get(s..s + 6)?;
    Some(Combination { main: [c[0], c[1], c[2], c[3], c[4]], bonus: c[5] })
}

fn bytes6(c: &Combination) -> [u8; 6] {
    [c.main[0], c.main[1], c.main[2], c.main[3], c.main[4], c.bonus]
}

fn win_of(w: [u8; 6]) -> Combination {
    Combination { main: [w[0], w[1], w[2], w[3], w[4]], bonus: w[5] }
}

/// The ticket and the round's live pools, checked against their UTXOs.
fn checked(cov: &kaspa_hashes::Hash, ticket: &CovUtxo, pools: &[CovUtxo]) -> Result<(Obj, TicketState, Vec<(Obj, PoolState)>)> {
    let to = obj("ticket", ticket)?;
    let t = cases::ticket(&to, cov)?;
    verify::check_ticket(&t, lottery::buy::b_max())?;
    if pools.len() > 64 {
        return Err(Error::new(Code::InputRange, format!("{} pools", pools.len())));
    }
    let mut ps = vec![];
    for p in pools {
        let o = obj("pools", p)?;
        let s = cases::pool(&o, cov)?;
        verify::check_pool(&s)?;
        if s.round != t.round {
            return Err(Error::new(Code::ChainMismatch, format!("a pool of round {} given for a ticket of round {}", s.round, t.round)));
        }
        ps.push((o, s));
    }
    if let Some((_, first)) = ps.first() {
        if ps.iter().any(|(_, p)| p.win != first.win || p.deadline != first.deadline || p.roots != first.roots) {
            return Err(Error::new(Code::ChainMismatch, "the round's pools disagree on the result"));
        }
    }
    Ok((to, t, ps))
}

/// Funds for the model only (no transaction is built from them).
fn model_funds() -> LFunds {
    let spk = lottery::buy::spk_of(&[0x6d; 32]);
    LFunds { coins: vec![Coin { outpoint: TransactionOutpoint::new(Default::default(), 0), entry: UtxoEntry::new(10 * lottery::buy::KAS, spk.clone(), 0, false, None) }], change: spk, fee: 0 }
}

/// A tier's name from its shape (design 4.2): "5 + bonus", "5", "4 + bonus", ...
pub fn tier_name(tier: u8) -> Option<String> {
    if tier > 6 {
        return None;
    }
    let (matched, bonus) = lottery::claim::tier_shape(tier);
    Some(if bonus { format!("{matched} + bonus") } else { matched.to_string() })
}

fn claim_view(x: &lottery::claim::ClaimExpected, pool: &PoolState) -> ClaimView {
    ClaimView {
        tier: pool.tier,
        tier_name: tier_name(pool.tier).unwrap_or_default(),
        slice: pool.slice.to_string(),
        h: x.h.to_string(),
        shared_with: (x.h - 1).max(0).to_string(),
        prize: x.prize.to_string(),
        mode: x.mode.into(),
        below_p_min: x.prize < P_MIN,
    }
}

/// Spec S2: the ticket card.
pub fn ticket_card(net: Net, i: &TicketCardInput) -> Result<TicketCard> {
    let cov = cov_of(&i.covenant_id)?;
    let (to, t, pools) = checked(&cov, &i.ticket, &i.pools)?;
    let now = dec("virtualDaaScore", &i.virtual_daa_score)?;
    let pool_win = pools.first().map(|(_, p)| p.win);
    if let (Some(pw), Some(w)) = (pool_win, i.winning.as_ref()) {
        if pw != bytes6(w) {
            return Err(Error::new(Code::ChainMismatch, "the stored winning numbers are not the pools'"));
        }
    }
    let win = pool_win.or(i.winning.as_ref().map(bytes6));
    if i.void && win.is_some() {
        return Err(Error::new(Code::ChainMismatch, "a void round with winning numbers"));
    }
    let stage = if i.void {
        CardStage::Void
    } else if win.is_some() {
        CardStage::Drawn
    } else {
        CardStage::Open
    };
    let purchases = match &i.purchases {
        Some(p) => Some(purchase_lists(p)?),
        None => None,
    };
    let claimed_bits = t.claimed.first().copied().unwrap_or(0);
    let pool_objs: Vec<Obj> = pools.iter().map(|(o, _)| o.clone()).collect();
    let mut combos = vec![];
    let mut back_with_last_claim = false;
    for idx in 0..t.b.clamp(0, 8) as usize {
        let combination = combination_at(&t, idx).ok_or_else(|| Error::new(Code::StateRange, format!("index {idx} outside the bundle")))?;
        let claimed = claimed_bits & (1u8 << idx) != 0;
        let (mut matched_main, mut bonus_matched, mut tier) = (None, None, None);
        let mut status = ComboStatus::Open;
        let mut claim = None;
        if let Some(w) = win {
            matched_main = Some(combination.main.map(|n| w[..5].contains(&n)));
            bonus_matched = Some(combination.bonus == w[5]);
            tier = tier_of(&bytes6(&combination), &w);
            status = if claimed {
                ComboStatus::Claimed
            } else if let Some(tr) = tier {
                match pools.iter().find(|(_, p)| p.tier == tr) {
                    Some((_, p)) => {
                        let lists = purchases.as_ref().ok_or_else(|| Error::new(Code::StoreIncomplete, "the round's purchase lists are needed for a claim"))?;
                        let (case, _) = cases::claim_case(&cov, &to, &pool_objs, lists, Some(idx as i64), &model_funds())?;
                        claim = Some(claim_view(&case.expected(), p));
                        back_with_last_claim = true;
                        ComboStatus::Claimable
                    }
                    None => ComboStatus::Expired,
                }
            } else {
                ComboStatus::NoWin
            };
        } else if i.void {
            status = ComboStatus::NoWin;
        }
        combos.push(ComboCard { index: idx as u32, combination, matched_main, bonus_matched, tier, tier_name: tier.and_then(tier_name), status, claim });
    }
    let alone = t.deadline.checked_add(D_RET).ok_or_else(|| Error::new(Code::Overflow, "T + D_RET"))?;
    let earliest_pool = pools.iter().min_by_key(|(_, p)| p.deadline);
    let return_from = earliest_pool.map(|(_, p)| p.deadline.min(alone)).unwrap_or(alone);
    // The node takes a transaction whose lock time is below its DAA score.
    let returnable = if (alone as u64) < now {
        Some(Returnable { path: ReturnPathName::Alone, pool: None })
    } else {
        earliest_pool
            .filter(|(_, p)| (p.deadline as u64) < now)
            .map(|(o, _)| Returnable { path: ReturnPathName::WithPool, pool: Some(convert::obj_dto(o)) })
    };
    Ok(TicketCard {
        round: t.round.to_string(),
        deadline: t.deadline.to_string(),
        b: t.b as u32,
        value: to.entry.amount.to_string(),
        payout: net.payout_address(t.payout_class, &t.payout_key).ok_or_else(|| Error::new(Code::WalletScript, "the ticket's payout has no address"))?,
        stage,
        winning: win.map(win_of),
        claim_deadline: pools.first().map(|(_, p)| p.deadline.to_string()).or_else(|| i.claim_deadline.clone()),
        combos,
        claimed: claimed_bits.count_ones(),
        back_with_last_claim,
        return_from: return_from.to_string(),
        returnable,
    })
}

// ------------------------------------------------------------------------------ claim figures

/// `claimFigures`: the `buildClaim` input fields that fix the claim, and the `BuiltTx` it gave.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaimFiguresInput {
    pub covenant_id: Hex,
    pub ticket: CovUtxo,
    pub pools: Vec<CovUtxo>,
    pub purchases: Vec<Hex>,
    pub index: u32,
    pub built: BuiltTx,
}

/// The pool a built claim leaves (output 0: the same state, the prize less).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoolOut {
    pub index: u32,
    pub value: Dec,
    pub state: Hex,
    pub script_public_key: Hex,
}

/// The claim row's figures of a built `claim()` (spec S2, 7 "Claims"), each checked against the
/// transaction: input 0 spends the given ticket, input 1 the pool of the claimed combination's
/// tier; output 0 is that pool less the prize; FINAL: output 1 pays prize + the ticket's value to
/// its payout; PAY: output 1 is the ticket with the index claimed, output 2 pays the prize;
/// ACCRUE: output 1 is the ticket with the index claimed holding its value + the prize; every
/// other output comes back to the wallet, which pays exactly the fee.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaimFigures {
    pub round: Dec,
    pub index: u32,
    pub combination: Combination,
    pub claim: ClaimView,
    /// The ticket's payout address (where the prize goes).
    pub payout: Address,
    /// Paid to the payout now (sompi): the prize (PAY), prize + the ticket's value (FINAL), 0
    /// (ACCRUE).
    pub paid_now: Dec,
    /// The ticket's value paid with it (FINAL; else 0): its deposits and accrued prizes.
    pub value_back: Dec,
    /// The prize kept in the ticket (ACCRUE; else 0).
    pub accrued: Dec,
    /// The network fee (sompi): all the wallet pays.
    pub fee: Dec,
    /// The fee is larger than the prize (spec 7).
    pub fee_exceeds_prize: bool,
    pub claim_deadline: Dec,
    /// The ticket after the claim (output 1), unless FINAL.
    pub ticket_next: Option<TicketOut>,
    pub pool_next: PoolOut,
}

fn bound(what: &str) -> Error {
    Error::new(Code::BuiltMismatch, format!("claim figures: {what}"))
}

/// The rest of the outputs (from `from` on) come back to the wallet, which loses exactly the
/// fee. The payout output `paid` is the ticket's payout, part of the claim: when it pays the
/// wallet's own address (the usual case: the player claims their own ticket) the summary counts
/// it as coming back, so it is taken out first.
fn wallet_pays_fee(b: &BuiltTx, from: usize, paid: Option<usize>, fee: u64) -> Result<()> {
    for (n, o) in b.summary.outputs.iter().enumerate().skip(from) {
        if !o.to_wallet {
            return Err(Error::new(Code::BuiltMismatch, format!("output {n} does not come back to the wallet")));
        }
    }
    let win = dec("summary.walletIn", &b.summary.wallet_in)?;
    let mut back = dec("summary.walletBack", &b.summary.wallet_back)?;
    if let Some(o) = paid.and_then(|n| b.summary.outputs.get(n)).filter(|o| o.to_wallet) {
        back = back.checked_sub(dec("summary.outputs.value", &o.value)?).ok_or_else(|| Error::new(Code::BuiltMismatch, "the payout is not among what comes back"))?;
    }
    if win.checked_sub(back) != Some(fee) {
        return Err(Error::new(Code::BuiltMismatch, "what the wallet pays is not the fee"));
    }
    Ok(())
}

fn spends(u: &lottery::web::sign::Unsigned, i: usize, o: &Obj) -> bool {
    u.tx.inputs.get(i).map(|x| x.previous_outpoint) == Some(o.outpoint) && u.entries.get(i) == Some(&o.entry)
}

fn ticket_out(net: Net, n: u32, s: &TicketState) -> Result<(TicketOut, ScriptPublicKey)> {
    let st = State::Ticket(s.clone());
    let spk = verify::state_spk(&st)?;
    let address = net.address(&spk).ok_or_else(|| bound("the ticket has no address"))?;
    Ok((TicketOut { index: n, state: hex(&verify::state_bytes(&st)?), script_public_key: convert::spk_hex(&spk), address }, spk))
}

/// Spec S2, 7: a built claim's figures, bound to its transaction.
pub fn claim_figures(net: Net, i: &ClaimFiguresInput) -> Result<ClaimFigures> {
    let cov = cov_of(&i.covenant_id)?;
    let u = unsigned_of(net, &i.built)?;
    let (to, t, pools) = checked(&cov, &i.ticket, &i.pools)?;
    let lists = purchase_lists(&i.purchases)?;
    let pool_objs: Vec<Obj> = pools.iter().map(|(o, _)| o.clone()).collect();
    let (case, pool_obj) = cases::claim_case(&cov, &to, &pool_objs, &lists, Some(i.index as i64), &model_funds())?;
    let x = case.expected();
    let pool = &case.pool;
    if !spends(&u, 0, &to) || !spends(&u, 1, pool_obj) {
        return Err(bound("inputs 0 and 1 are not the ticket and the pool"));
    }
    let out = |n: usize| u.tx.outputs.get(n).ok_or_else(|| bound(&format!("no output {n}")));
    let pspk = verify::state_spk(&State::Pool(pool.clone()))?;
    let o0 = out(0)?;
    if o0.script_public_key != pspk || o0.covenant.is_none() || Some(&o0.value) != x.values.first() {
        return Err(bound("output 0 is not the pool less the prize"));
    }
    let payout = payout_spk(t.payout_class, &t.payout_key);
    let tv = to.entry.amount;
    let prize = u64::try_from(x.prize).map_err(|_| bound("a negative prize"))?;
    let (paid_now, value_back, accrued, ticket_next, rest, paid_at) = match x.mode {
        ClaimMode::Final => {
            let o1 = out(1)?;
            if o1.script_public_key != payout || o1.covenant.is_some() || Some(o1.value) != prize.checked_add(tv) {
                return Err(bound("output 1 is not prize + the ticket's value to its payout"));
            }
            (o1.value, tv, 0, None, 2, Some(1))
        }
        ClaimMode::Pay | ClaimMode::Accrue => {
            let next = x.ticket.as_ref().ok_or_else(|| bound("no ticket successor"))?;
            let (tout, tspk) = ticket_out(net, 1, next)?;
            let o1 = out(1)?;
            let want = if x.mode == ClaimMode::Pay { Some(tv) } else { tv.checked_add(prize) };
            if o1.script_public_key != tspk || o1.covenant.is_none() || Some(o1.value) != want {
                return Err(bound("output 1 is not the ticket with the index claimed"));
            }
            if x.mode == ClaimMode::Pay {
                let o2 = out(2)?;
                if o2.script_public_key != payout || o2.covenant.is_some() || o2.value != prize {
                    return Err(bound("output 2 is not the prize to the ticket's payout"));
                }
                (prize, 0, 0, Some(tout), 3, Some(2))
            } else {
                (0, 0, prize, Some(tout), 2, None)
            }
        }
    };
    let fee = u.fee;
    wallet_pays_fee(&i.built, rest, paid_at, fee)?;
    let combination = combination_at(&t, i.index as usize).ok_or_else(|| bound("index outside the bundle"))?;
    let pstate = verify::state_bytes(&State::Pool(pool.clone()))?;
    Ok(ClaimFigures {
        round: t.round.to_string(),
        index: i.index,
        combination,
        claim: claim_view(&x, pool),
        payout: net.payout_address(t.payout_class, &t.payout_key).ok_or_else(|| bound("the payout has no address"))?,
        paid_now: paid_now.to_string(),
        value_back: value_back.to_string(),
        accrued: accrued.to_string(),
        fee: fee.to_string(),
        fee_exceeds_prize: fee > prize,
        claim_deadline: pool.deadline.to_string(),
        ticket_next,
        pool_next: PoolOut { index: 0, value: o0.value.to_string(), state: hex(&pstate), script_public_key: convert::spk_hex(&pspk) },
    })
}

// ------------------------------------------------------------------------------ return figures

#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReturnFiguresInput {
    pub covenant_id: Hex,
    pub ticket: CovUtxo,
    #[serde(default)]
    pub pool: Option<CovUtxo>,
    pub built: BuiltTx,
}

/// A built `return_deposit()`'s figures (spec S2, 7 "Deposits"), checked against the
/// transaction: input 0 spends the ticket (input 1 the pool, path a, recreated unchanged as
/// output 0); the payout output pays the ticket's whole value to its payout; the lock time is
/// T + D_RET (path b) or the pool's claim deadline (path a); the wallet pays exactly the fee.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReturnFigures {
    pub round: Dec,
    pub path: ReturnPathName,
    /// The ticket's value, paid to its payout.
    pub amount: Dec,
    pub payout: Address,
    pub fee: Dec,
    /// The transaction's lock time (DAA score).
    pub lock_time: Dec,
}

pub fn return_figures(net: Net, i: &ReturnFiguresInput) -> Result<ReturnFigures> {
    let cov = cov_of(&i.covenant_id)?;
    let u = unsigned_of(net, &i.built)?;
    let to = obj("ticket", &i.ticket)?;
    let t = cases::ticket(&to, &cov)?;
    let bad = |w: &str| Error::new(Code::BuiltMismatch, format!("return figures: {w}"));
    if !spends(&u, 0, &to) {
        return Err(bad("input 0 is not the ticket"));
    }
    let payout = payout_spk(t.payout_class, &t.payout_key);
    let (path, lock, pay_at) = match &i.pool {
        None => (ReturnPathName::Alone, t.deadline.checked_add(D_RET).ok_or_else(|| Error::new(Code::Overflow, "T + D_RET"))?, 0),
        Some(p) => {
            let po = obj("pool", p)?;
            let ps = cases::pool(&po, &cov)?;
            if ps.round != t.round || !spends(&u, 1, &po) {
                return Err(bad("input 1 is not a pool of the ticket's round"));
            }
            let o0 = u.tx.outputs.first().ok_or_else(|| bad("no output 0"))?;
            if o0.script_public_key != po.entry.script_public_key || o0.value != po.entry.amount || o0.covenant.is_none() {
                return Err(bad("output 0 is not the pool unchanged"));
            }
            (ReturnPathName::WithPool, ps.deadline, 1)
        }
    };
    let o = u.tx.outputs.get(pay_at).ok_or_else(|| bad("no payout output"))?;
    if o.script_public_key != payout || o.value != to.entry.amount || o.covenant.is_some() {
        return Err(bad("the payout output is not the ticket's value to its payout"));
    }
    if u.tx.lock_time != lock as u64 {
        return Err(bad("the lock time is not the path's"));
    }
    wallet_pays_fee(&i.built, pay_at + 1, Some(pay_at), u.fee)?;
    Ok(ReturnFigures {
        round: t.round.to_string(),
        path,
        amount: to.entry.amount.to_string(),
        payout: net.payout_address(t.payout_class, &t.payout_key).ok_or_else(|| bad("the payout has no address"))?,
        fee: u.fee.to_string(),
        lock_time: lock.to_string(),
    })
}
