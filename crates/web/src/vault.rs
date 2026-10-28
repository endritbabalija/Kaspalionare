//! S5 Vault (spec frontend 3 S5, 4 "Vault position card", 7; front end F6 part A): every figure the
//! Vault page shows, computed here from node-checked chain objects, and the rows of a built
//! `request_deposit()`, `request_withdraw()` or `settle()`, each bound to its transaction. No rule
//! lives here: the price (A + VA) / (S + VS), a position's worth (`vault::position_worth`), wmin and
//! W4b (`game::w_min`, `game::w4b_ok`), the request age (VAULT_REQ_DELAY), MIN_DEPOSIT and every
//! settlement amount and mode (`game::settle`, `SettleCase::expected`) come from `lottery`.
//!
//! - `vault_view` (`vaultView`): the vault's figures (value, share price, liability L, pending
//!   totals, the round requests convert at, from when the vault takes the next request), the
//!   connected wallet's positions (shares, worth today, the withdrawals W4b allows) and requests
//!   (pending, settle due with what the settlement does, or unknown). A position or request of
//!   another owner is left out (counted), never shown as the wallet's.
//! - `withdraw_choice` (`withdrawChoice`): the shares a withdrawal of an amount (or of the whole
//!   position) takes, refused (`vault_amount`) unless the vault takes it (W4, W4b).
//! - `deposit_figures`, `withdraw_figures`, `settle_figures`: the rows of a built transaction, each
//!   checked against it (`built_mismatch` otherwise), as `buyFigures` and `claimFigures` are.
//!
//! The objects are checked as every builder checks them (`cases::live`: the state against the UTXO's
//! script under the instance's covenant ID); that an entry is live is the page's (it reads each from
//! a public node by outpoint, F1 re-review N1).

use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutput};
use lottery::game::{self, MIN_DEPOSIT, SettleMode, VA, VAULT_REQ_DELAY, VS};
use lottery::index::State;
use lottery::state::{KIND_DEPOSIT, KIND_WITHDRAW, Payout, PositionState, RecordState, RequestState, VaultState};
use lottery::vault::{SettleCase, position_worth};
use lottery::web::cases::{self, Obj};
use lottery::web::sign::Unsigned;
use lottery::web::{Code, Error, Result, verify};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::api::{cov_of, unsigned_of};
use crate::convert::{self, Net, dec, dec_i64, hex, obj};
use crate::dto::{Address, BuiltTx, CovUtxo, Dec, Hex};
use crate::rounds::Outpoint;

/// The most positions, requests or records one `vaultView` takes (the indexer's list cap).
pub const VAULT_OBJECTS_MAX: usize = crate::indexer::OBJECTS_MAX as usize;

// ------------------------------------------------------------------------------ helpers

fn amount_err(detail: impl Into<String>) -> Error {
    Error::new(Code::VaultAmount, detail.into())
}

/// A value minus the parts the state accounts for: the reserve R the UTXO carries on top
/// (`cases::request_deposit`'s derivation).
fn reserve(value: u64, accounted: i128, what: &str) -> Result<u64> {
    let r = value as i128 - accounted;
    if r < 1 || r > verify::RESERVE_MAX as i128 {
        return Err(Error::new(Code::ChainMismatch, format!("{what}: value {value} leaves reserve {r}")));
    }
    Ok(r as u64)
}

/// The live vault, its state and its reserve R.
fn vault_of(cov: &kaspa_hashes::Hash, c: &CovUtxo) -> Result<(Obj, VaultState, u64)> {
    let o = obj("vault", c)?;
    let v = cases::vault(&o, cov)?;
    verify::check_vault(&v)?;
    let r = reserve(o.entry.amount, v.total_assets as i128 + v.pend_dep as i128, "vault")?;
    Ok((o, v, r))
}

fn position_of(cov: &kaspa_hashes::Hash, c: &CovUtxo) -> Result<(Obj, PositionState)> {
    let o = obj("position", c)?;
    let p = cases::position(&o, cov)?;
    verify::check_position(&p)?;
    Ok((o, p))
}

fn request_of(cov: &kaspa_hashes::Hash, c: &CovUtxo) -> Result<(Obj, RequestState)> {
    let o = obj("request", c)?;
    let r = cases::request(&o, cov)?;
    verify::check_request(&r)?;
    Ok((o, r))
}

fn record_of(cov: &kaspa_hashes::Hash, c: &CovUtxo) -> Result<(Obj, RecordState)> {
    let o = obj("record", c)?;
    let r = cases::record(&o, cov)?;
    verify::check_record(&r)?;
    Ok((o, r))
}

/// What `shares` are worth at the vault's price now: floor(shares (A + VA) / (S + VS)).
fn worth(shares: i64, v: &VaultState) -> i64 {
    position_worth(shares, v.total_assets, v.total_shares)
}

/// (A + VA) / (S + VS) in sompi per share, 8 decimals (rounded down).
fn price(v: &VaultState) -> String {
    let num = (v.total_assets as u128 + VA as u128) * 100_000_000;
    let den = v.total_shares as u128 + VS as u128;
    let q = num / den;
    format!("{}.{:08}", q / 100_000_000, q % 100_000_000)
}

/// The lock time of a request on this vault: its creation DAA + VAULT_REQ_DELAY (vault checks D10,
/// W12; `DepositCase::at_age`). The node takes a transaction whose lock time is below its DAA score.
fn request_from(o: &Obj) -> u64 {
    o.entry.block_daa_score.saturating_add(VAULT_REQ_DELAY as u64)
}

fn outpoint_of(c: &CovUtxo) -> Outpoint {
    Outpoint::of(&c.utxo)
}

fn owner_of(net: Net, class: u8, key: &[u8; 33]) -> Result<Address> {
    net.payout_address(class, key).ok_or_else(|| Error::new(Code::WalletScript, "the owner has no address"))
}

/// The x-only key of a Schnorr P2PK owner (what the withdrawal's owner input names), else None.
fn schnorr_key(class: u8, key: &[u8; 33]) -> Option<Hex> {
    matches!(Payout::from_fields(class, key), Payout::Schnorr(_)).then(|| hex(&key[..32]))
}

// ------------------------------------------------------------------------------ vault view

/// `vaultView`: the live vault and, for the connected wallet, its positions, its requests and the
/// live records they settle against (each the node's entry with the indexer's state bytes).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultViewInput {
    pub covenant_id: Hex,
    pub vault: CovUtxo,
    #[serde(default)]
    pub positions: Vec<CovUtxo>,
    #[serde(default)]
    pub requests: Vec<CovUtxo>,
    #[serde(default)]
    pub records: Vec<CovUtxo>,
    /// The connected wallet's address: only its positions and requests are shown.
    #[serde(default)]
    pub owner: Option<Address>,
    /// The node's virtual DAA score.
    pub virtual_daa_score: Dec,
}

/// The vault's figures (spec S5), all from the live vault.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultSummary {
    /// The last round drawn (the vault's state).
    pub round: Dec,
    /// round + 1: a request made now converts at this round's draw, at its price.
    pub next_draw: Dec,
    /// The UTXO's value: total_assets + pending deposits + R.
    pub value: Dec,
    /// A, the backers' assets (sompi).
    pub total_assets: Dec,
    /// S, the shares in issue.
    pub total_shares: Dec,
    /// (A + VA) / (S + VS), sompi per share, 8 decimals rounded down.
    pub share_price: String,
    /// L, the liability the vault keeps for the round whose jackpot is fixed.
    pub liability: Dec,
    pub pending_deposits: Dec,
    /// Shares.
    pub pending_withdrawals: Dec,
    /// What the pending withdrawal shares are worth at today's price (the draw sets the real one).
    pub pending_withdrawals_worth: Dec,
    /// R, the reserve a request or a position carries.
    pub reserve: Dec,
    /// The lock time of a request now (DAA score): the vault's creation DAA + VAULT_REQ_DELAY.
    pub request_from: Dec,
    /// The node's clock is past `request_from`: the vault takes a request now.
    pub request_open: bool,
    /// MIN_DEPOSIT (sompi).
    pub min_deposit: Dec,
    /// wmin (shares) and its worth today.
    pub w_min: Dec,
    pub w_min_worth: Dec,
    pub source: Outpoint,
}

/// The partial withdrawals W4b allows on a position: w from `least` to `most` shares, and the amounts
/// (sompi) `withdrawChoice` turns into them (F6 part A review S1: every amount from `least_amount` to
/// `most_amount` is taken, and none outside).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PartRange {
    pub least: Dec,
    pub most: Dec,
    /// The smallest amount the vault takes: ceil(least (A + VA) / (S + VS)).
    pub least_amount: Dec,
    /// The largest amount the vault takes: its shares floor(a (S + VS) / (A + VA)) stay <= `most`.
    pub most_amount: Dec,
    /// What the page prints for them (exactly, never rounded again): `least_amount` rounded up and
    /// `most_amount` rounded down to a whole SHOWN_STEP (0.01 KAS) when both stay inside the range,
    /// else the exact amounts. Typed back, each is taken.
    pub least_shown: Dec,
    pub most_shown: Dec,
}

/// The page's KAS precision (2 decimals) in sompi: the step the Least and Most figures are rounded to.
pub const SHOWN_STEP: u64 = 1_000_000;

/// A share position of the wallet (spec 4 "Vault position card": position).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PositionCard {
    pub object: CovUtxo,
    /// The round of the draw that minted it (informational, A3.4).
    pub round: Dec,
    pub shares: Dec,
    /// floor(shares (A + VA) / (S + VS)): what it is worth at today's price.
    pub worth: Dec,
    pub owner: Address,
    /// The owner's x-only key when the owner is a Schnorr P2PK address (the only kind the page's
    /// signers sign; the withdrawal's owner input names it). None: not withdrawable from the page.
    pub owner_key: Option<Hex>,
    /// The partial withdrawals the vault takes besides the whole; None when shares < 2 wmin.
    pub partial: Option<PartRange>,
    pub source: Outpoint,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RequestKindName {
    Deposit,
    Withdrawal,
}

/// Where a request is (spec 4 "Vault position card": pending request, settle due).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RequestStatus {
    /// Its draw has not happened: it converts at that draw's price.
    Pending,
    /// Its draw happened and the live record of that draw is given: `settle()` can be made now.
    SettleDue,
    /// Its draw happened but no live record of it is given (the indexer did not list it).
    Unknown,
}

/// The five settlement modes (spec M5 A6.1, `game::SettleMode`).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SettleModeName {
    /// A deposit converted: a position of `shares` is minted for the owner.
    Convert,
    /// A deposit in refund mode: the deposit and the request's value are paid back.
    Refund,
    /// A withdrawal fully converted: the payout and the request's value are paid.
    Final,
    /// A withdrawal partly converted: the payout is paid, the rest stays pending for the next draw.
    Pay,
    /// A withdrawal partly converted with a payout below P_MIN: it accrues in the request.
    Accrue,
}

impl From<SettleMode> for SettleModeName {
    fn from(m: SettleMode) -> Self {
        match m {
            SettleMode::DepositConvert => SettleModeName::Convert,
            SettleMode::DepositRefund => SettleModeName::Refund,
            SettleMode::Final => SettleModeName::Final,
            SettleMode::Pay => SettleModeName::Pay,
            SettleMode::Accrue => SettleModeName::Accrue,
        }
    }
}

/// What `settle()` of a request against its record does now (`SettleCase::expected`, whose
/// reference is `game::settle`).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettleView {
    pub mode: SettleModeName,
    /// Convert: the shares minted into the new position.
    pub shares: Dec,
    /// Paid to the owner now (sompi): Refund the deposit + the request's value; Final the payout +
    /// the request's value; Pay the payout; 0 otherwise.
    pub payout: Dec,
    /// Withdrawal: the shares converted at this draw, and those still pending (for the next draw).
    pub converted: Dec,
    pub left: Dec,
    /// Accrue: the payout kept in the request (paid with its final settlement).
    pub accrued: Dec,
    /// The request's value (its reserve R and any accrued payout): paid back with the payout
    /// (Refund, Final), or moved into the new position (Convert) or the request (Pay, Accrue).
    pub request_value: Dec,
    /// This settlement drains the record: its reserve goes back to the record's funder.
    pub drained: bool,
    pub record_refund: Dec,
    pub record_funder: Option<Address>,
}

/// A request of the wallet (spec 4 "Vault position card": pending request, settle due).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestCard {
    pub object: CovUtxo,
    pub kind: RequestKindName,
    /// Deposit: sompi. Withdrawal: shares.
    pub amount: Dec,
    /// Deposit: the amount. Withdrawal: what the shares are worth at today's price.
    pub worth: Dec,
    /// The round whose draw converts it (at that draw's price).
    pub round: Dec,
    /// The UTXO's value: R and any accrued payout.
    pub value: Dec,
    pub owner: Address,
    pub status: RequestStatus,
    /// SettleDue: the live record it settles against, and what the settlement does.
    pub record: Option<CovUtxo>,
    pub settle: Option<SettleView>,
    pub source: Outpoint,
}

/// `vaultView`'s result.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultPage {
    pub vault: VaultSummary,
    pub positions: Vec<PositionCard>,
    pub requests: Vec<RequestCard>,
    /// Positions and requests given that the wallet does not own: left out.
    pub foreign: u32,
}

/// The shares `withdrawChoice` takes for an amount: floor(a (S + VS) / (A + VA)). `part_range`
/// inverts it, so the two must stay the same formula.
fn shares_for(a: u128, v: &VaultState) -> Option<u128> {
    a.checked_mul(v.total_shares as u128 + VS as u128).map(|x| x / (v.total_assets as u128 + VA as u128))
}

/// W4b's part range on a position, as amounts: w from wmin to min(shares - wmin, S - pend_wsh) (W4b,
/// W4), and the amounts whose `shares_for` lands in it. None when no amount does (a range narrower
/// than one sompi's worth of shares) or the position is below 2 wmin.
fn part_range(v: &VaultState, shares: i64) -> Option<PartRange> {
    let wm = game::w_min(v.total_assets, v.total_shares);
    let most = shares.checked_sub(wm)?.min(v.total_shares.checked_sub(v.pend_wsh)?);
    if wm < 1 || most < wm {
        return None;
    }
    let n = v.total_assets as u128 + VA as u128;
    let d = v.total_shares as u128 + VS as u128;
    // The smallest a with a d >= wm n, and the largest a with a d < (most + 1) n.
    let lo = (wm as u128 * n).div_ceil(d);
    let hi = ((most as u128 + 1) * n - 1) / d;
    if lo > hi || lo == 0 {
        return None;
    }
    debug_assert!(shares_for(lo, v) >= Some(wm as u128) && shares_for(hi, v) <= Some(most as u128));
    let step = SHOWN_STEP as u128;
    let (ls, ms) = (lo.div_ceil(step) * step, hi / step * step);
    let (ls, ms) = if ls <= ms { (ls, ms) } else { (lo, hi) };
    Some(PartRange {
        least: wm.to_string(),
        most: most.to_string(),
        least_amount: lo.to_string(),
        most_amount: hi.to_string(),
        least_shown: ls.to_string(),
        most_shown: ms.to_string(),
    })
}

fn settle_case(cov: &kaspa_hashes::Hash, ro: &Obj, rq: &RequestState, co: &Obj, rc: &RecordState) -> Result<SettleCase> {
    let t = verify::templates()?;
    let accounted = (rc.refund as i128).checked_mul(rc.dep_rem as i128).and_then(|x| x.checked_add(rc.out_rem as i128)).ok_or_else(|| Error::new(Code::Overflow, "record: refund x deposits"))?;
    let rr = reserve(co.entry.amount, accounted, "record")?;
    let mut c = SettleCase::new_with(&t, rc.clone(), rq.clone());
    c.request_value = ro.entry.amount;
    c.request_daa = ro.entry.block_daa_score;
    c.record_reserve = rr;
    c.record_daa = co.entry.block_daa_score;
    c.cov = *cov;
    Ok(c)
}

/// The settlement of `rq` against `rc` (None: the reference refuses it).
fn settle_view(net: Net, c: &SettleCase) -> Option<SettleView> {
    game::settle(&c.record, &c.request)?;
    let x = c.expected();
    x.reference.as_ref()?;
    let at = |i: Option<usize>| i.and_then(|i| x.values.get(i)).copied().unwrap_or(0);
    let accrued = if x.mode == SettleMode::Accrue { x.pay } else { 0 };
    Some(SettleView {
        mode: x.mode.into(),
        shares: x.sh.to_string(),
        payout: at(x.payout_output).to_string(),
        converted: x.conv.to_string(),
        left: x.left.to_string(),
        accrued: accrued.to_string(),
        request_value: c.request_value.to_string(),
        drained: x.drained,
        record_refund: at(x.refund_output).to_string(),
        record_funder: x.drained.then(|| net.payout_address(c.record.funder_class, &c.record.funder_key)).flatten(),
    })
}

/// Spec S5: the vault's figures and the wallet's positions and requests.
pub fn vault_view(net: Net, i: &VaultViewInput) -> Result<VaultPage> {
    let cov = cov_of(&i.covenant_id)?;
    if i.positions.len() > VAULT_OBJECTS_MAX || i.requests.len() > VAULT_OBJECTS_MAX || i.records.len() > VAULT_OBJECTS_MAX {
        return Err(Error::new(Code::InputRange, format!("{} positions, {} requests, {} records", i.positions.len(), i.requests.len(), i.records.len())));
    }
    let owner = i.owner.as_deref().map(|a| net.payout_of("owner", a)).transpose()?;
    if owner.is_none() && (!i.positions.is_empty() || !i.requests.is_empty()) {
        return Err(Error::new(Code::InputRange, "positions or requests without the owner they are shown for"));
    }
    let now = dec("virtualDaaScore", &i.virtual_daa_score)?;
    let (vo, v, r) = vault_of(&cov, &i.vault)?;
    let from = request_from(&vo);
    let wm = game::w_min(v.total_assets, v.total_shares);
    let vault = VaultSummary {
        round: v.round.to_string(),
        next_draw: (v.round + 1).to_string(),
        value: vo.entry.amount.to_string(),
        total_assets: v.total_assets.to_string(),
        total_shares: v.total_shares.to_string(),
        share_price: price(&v),
        liability: v.liability.to_string(),
        pending_deposits: v.pend_dep.to_string(),
        pending_withdrawals: v.pend_wsh.to_string(),
        pending_withdrawals_worth: worth(v.pend_wsh, &v).to_string(),
        reserve: r.to_string(),
        request_from: from.to_string(),
        request_open: from < now,
        min_deposit: MIN_DEPOSIT.to_string(),
        w_min: wm.to_string(),
        w_min_worth: worth(wm, &v).to_string(),
        source: outpoint_of(&i.vault),
    };
    let mine = |class: u8, key: &[u8; 33]| owner.as_ref().is_some_and(|o| *o == Payout::from_fields(class, key));
    let mut seen: Vec<Outpoint> = vec![];
    let mut once = |c: &CovUtxo| -> Result<()> {
        let o = outpoint_of(c);
        if seen.contains(&o) {
            return Err(Error::new(Code::InputRange, format!("{}:{} given twice", o.transaction_id, o.index)));
        }
        seen.push(o);
        Ok(())
    };
    let mut foreign = 0u32;
    let mut positions = vec![];
    for c in &i.positions {
        once(c)?;
        let (_, p) = position_of(&cov, c)?;
        if !mine(p.owner_class, &p.owner_key) {
            foreign += 1;
            continue;
        }
        positions.push(PositionCard {
            object: c.clone(),
            round: p.round.to_string(),
            shares: p.shares.to_string(),
            worth: worth(p.shares, &v).to_string(),
            owner: owner_of(net, p.owner_class, &p.owner_key)?,
            owner_key: schnorr_key(p.owner_class, &p.owner_key),
            partial: part_range(&v, p.shares),
            source: outpoint_of(c),
        });
    }
    let mut records: Vec<(Obj, RecordState, &CovUtxo)> = vec![];
    for c in &i.records {
        once(c)?;
        let (o, rc) = record_of(&cov, c)?;
        if records.iter().any(|(_, x, _)| x.round == rc.round) {
            return Err(Error::new(Code::ChainMismatch, format!("two live records of round {}", rc.round)));
        }
        records.push((o, rc, c));
    }
    let mut requests = vec![];
    for c in &i.requests {
        once(c)?;
        let (ro, rq) = request_of(&cov, c)?;
        if !mine(rq.owner_class, &rq.owner_key) {
            foreign += 1;
            continue;
        }
        let kind = if rq.kind == KIND_DEPOSIT { RequestKindName::Deposit } else { RequestKindName::Withdrawal };
        let worth_now = if rq.kind == KIND_WITHDRAW { worth(rq.amount, &v) } else { rq.amount };
        let (status, record, settle) = if rq.round > v.round {
            (RequestStatus::Pending, None, None)
        } else {
            match records.iter().find(|(_, rc, _)| rc.round == rq.round) {
                Some((co, rc, cc)) => match settle_view(net, &settle_case(&cov, &ro, &rq, co, rc)?) {
                    Some(s) => (RequestStatus::SettleDue, Some((*cc).clone()), Some(s)),
                    None => (RequestStatus::Unknown, None, None),
                },
                None => (RequestStatus::Unknown, None, None),
            }
        };
        requests.push(RequestCard {
            object: c.clone(),
            kind,
            amount: rq.amount.to_string(),
            worth: worth_now.to_string(),
            round: rq.round.to_string(),
            value: ro.entry.amount.to_string(),
            owner: owner_of(net, rq.owner_class, &rq.owner_key)?,
            status,
            record,
            settle,
            source: outpoint_of(c),
        });
    }
    Ok(VaultPage { vault, positions, requests, foreign })
}

// ------------------------------------------------------------------------------ withdraw choice

/// `withdrawChoice`: the shares a withdrawal from `position` takes: the whole position (`amount`
/// null), or the shares worth at most `amount` sompi at today's price.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WithdrawChoiceInput {
    pub covenant_id: Hex,
    pub vault: CovUtxo,
    pub position: CovUtxo,
    #[serde(default)]
    pub amount: Option<Dec>,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WithdrawChoice {
    pub shares: Dec,
    pub whole: bool,
    /// What the shares are worth at today's price (the draw sets the real one).
    pub worth: Dec,
    /// The position left (0 when whole) and its worth today.
    pub remainder: Dec,
    pub remainder_worth: Dec,
}

/// w is a withdrawal the vault takes from this position (W4 1 <= w <= shares and w <= S - pend_wsh;
/// W4b: whole, or w >= wmin and shares - w >= wmin).
fn w_ok(v: &VaultState, p: &PositionState, w: i64) -> Result<()> {
    if !(1..=p.shares).contains(&w) || v.pend_wsh > v.total_shares - w {
        return Err(amount_err(format!("withdraw {w} of {} shares (pending {} of {})", p.shares, v.pend_wsh, v.total_shares)));
    }
    if !game::w4b_ok(v.total_assets, v.total_shares, p.shares, w) {
        return Err(amount_err(format!("partial withdrawal {w} of {}: w and the remainder must both be >= wmin {} (D86, W4b)", p.shares, game::w_min(v.total_assets, v.total_shares))));
    }
    Ok(())
}

pub fn withdraw_choice(_net: Net, i: &WithdrawChoiceInput) -> Result<WithdrawChoice> {
    let cov = cov_of(&i.covenant_id)?;
    let (_, v, _) = vault_of(&cov, &i.vault)?;
    let (_, p) = position_of(&cov, &i.position)?;
    let w = match &i.amount {
        None => p.shares,
        Some(a) => {
            let a = dec("amount", a)?;
            // The shares worth at most `a` at today's price: floor(a (S + VS) / (A + VA)).
            let q = shares_for(a as u128, &v).ok_or_else(|| amount_err("amount out of range"))?;
            i64::try_from(q).map_err(|_| amount_err("amount out of range"))?
        }
    };
    w_ok(&v, &p, w)?;
    let rest = p.shares - w;
    Ok(WithdrawChoice { shares: w.to_string(), whole: rest == 0, worth: worth(w, &v).to_string(), remainder: rest.to_string(), remainder_worth: worth(rest, &v).to_string() })
}

// ------------------------------------------------------------------------------ figures

/// A covenant output a built transaction creates (to read from a node once accepted).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CovOut {
    pub index: u32,
    pub value: Dec,
    pub state: Hex,
    pub script_public_key: Hex,
    pub address: Address,
}

fn bound(what: &str) -> Error {
    Error::new(Code::BuiltMismatch, format!("vault figures: {what}"))
}

fn spends(u: &Unsigned, i: usize, o: &Obj) -> bool {
    u.tx.inputs.get(i).map(|x| x.previous_outpoint) == Some(o.outpoint) && u.entries.get(i) == Some(&o.entry)
}

fn out(u: &Unsigned, n: usize) -> Result<&TransactionOutput> {
    u.tx.outputs.get(n).ok_or_else(|| bound(&format!("no output {n}")))
}

/// Output `n` is the covenant output of `state` holding `value`.
fn cov_out(net: Net, u: &Unsigned, n: usize, state: &State, value: u64, what: &str) -> Result<CovOut> {
    let spk = verify::state_spk(state)?;
    let o = out(u, n)?;
    if o.script_public_key != spk || o.value != value || o.covenant.is_none() {
        return Err(bound(&format!("output {n} is not {what}")));
    }
    Ok(CovOut { index: n as u32, value: value.to_string(), state: hex(&verify::state_bytes(state)?), script_public_key: convert::spk_hex(&spk), address: net.address(&spk).ok_or_else(|| bound("no address"))? })
}

/// Output `n` is a plain output paying `value` to `spk`.
fn plain_out(u: &Unsigned, n: usize, spk: &ScriptPublicKey, value: u64, what: &str) -> Result<()> {
    let o = out(u, n)?;
    if &o.script_public_key != spk || o.value != value || o.covenant.is_some() {
        return Err(bound(&format!("output {n} is not {what}")));
    }
    Ok(())
}

/// Every output from `from` on comes back to the wallet (the change); what the wallet's coins lose
/// (wallet_in - wallet_back, from the summary `unsigned_of` rebuilt from the transaction).
fn wallet_net(b: &BuiltTx, from: usize) -> Result<i128> {
    for (n, o) in b.summary.outputs.iter().enumerate().skip(from) {
        if !o.to_wallet {
            return Err(bound(&format!("output {n} does not come back to the wallet")));
        }
    }
    Ok(dec("summary.walletIn", &b.summary.wallet_in)? as i128 - dec("summary.walletBack", &b.summary.wallet_back)? as i128)
}

/// Whether output `n` of the summary pays the wallet.
fn to_wallet(b: &BuiltTx, n: Option<usize>) -> bool {
    n.and_then(|n| b.summary.outputs.get(n)).is_some_and(|o| o.to_wallet)
}

/// `depositFigures`: the `buildRequestDeposit` input fields that fix the request, and its `BuiltTx`.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DepositFiguresInput {
    pub covenant_id: Hex,
    pub vault: CovUtxo,
    pub amount: Dec,
    pub owner: Address,
    pub built: BuiltTx,
}

/// The rows of a built `request_deposit()` (spec S5, 7), each checked against the transaction:
/// input 0 spends the given vault; output 0 is the vault with the deposit pending (its value + D);
/// output 1 is the request {next draw, deposit, D, owner} holding R; every other output comes back
/// to the wallet, which pays exactly D + R + fee; the lock time is the vault's creation DAA +
/// VAULT_REQ_DELAY.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DepositFigures {
    /// The round whose draw converts it, at that draw's price.
    pub round: Dec,
    pub amount: Dec,
    /// R, held in the request; back with its settlement (in the position, or with the refund).
    pub reserve: Dec,
    pub fee: Dec,
    /// amount + reserve + fee: what leaves the wallet.
    pub total: Dec,
    pub owner: Address,
    /// The transaction's lock time (DAA score).
    pub request_from: Dec,
    /// Requests are irrevocable (design 5.4): there is no cancellation.
    pub revocable: bool,
    /// A deposit can come back as a refund (refund mode, D70).
    pub refundable: bool,
    pub request: CovOut,
    pub vault_next: CovOut,
}

pub fn deposit_figures(net: Net, i: &DepositFiguresInput) -> Result<DepositFigures> {
    let cov = cov_of(&i.covenant_id)?;
    let u = unsigned_of(net, &i.built)?;
    let (vo, v, r) = vault_of(&cov, &i.vault)?;
    let d = dec_i64("amount", &i.amount)?;
    if d < MIN_DEPOSIT {
        return Err(amount_err(format!("deposit {d} below MIN_DEPOSIT")));
    }
    let owner = net.payout_of("owner", &i.owner)?;
    if !matches!(owner, Payout::Schnorr(_)) {
        return Err(Error::new(Code::OwnerType, "the page deposits for Schnorr P2PK owners only"));
    }
    if !spends(&u, 0, &vo) {
        return Err(bound("input 0 is not the given vault"));
    }
    let du = u64::try_from(d).map_err(|_| amount_err("negative deposit"))?;
    let next = VaultState { pend_dep: v.pend_dep.checked_add(d).ok_or_else(|| amount_err("pending deposits overflow"))?, ..v.clone() };
    let vault_value = vo.entry.amount.checked_add(du).ok_or_else(|| amount_err("vault value overflows"))?;
    let vault_next = cov_out(net, &u, 0, &State::Vault(next), vault_value, "the vault with the deposit pending")?;
    let rq = RequestState::new(v.round + 1, KIND_DEPOSIT, d, &owner);
    let request = cov_out(net, &u, 1, &State::Request(rq), r, "this deposit's request holding R")?;
    let from = request_from(&vo);
    if u.tx.lock_time != from {
        return Err(bound("the lock time is not the vault's creation DAA + VAULT_REQ_DELAY"));
    }
    let total = du.checked_add(r).and_then(|x| x.checked_add(u.fee)).ok_or_else(|| amount_err("total overflows"))?;
    if wallet_net(&i.built, 2)? != total as i128 {
        return Err(bound("what the wallet pays is not deposit + reserve + fee"));
    }
    Ok(DepositFigures {
        round: (v.round + 1).to_string(),
        amount: d.to_string(),
        reserve: r.to_string(),
        fee: u.fee.to_string(),
        total: total.to_string(),
        owner: i.owner.clone(),
        request_from: from.to_string(),
        revocable: false,
        refundable: true,
        request,
        vault_next,
    })
}

/// `withdrawFigures`: the `buildRequestWithdraw` input fields that fix the request, and its
/// `BuiltTx`.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WithdrawFiguresInput {
    pub covenant_id: Hex,
    pub vault: CovUtxo,
    pub position: CovUtxo,
    pub shares: Dec,
    pub built: BuiltTx,
}

/// The rows of a built `request_withdraw()` (spec S5, 7), each checked against the transaction:
/// inputs 0 and 1 spend the given vault and position; output 0 is the vault with the shares pending
/// (its value unchanged); output 1 is the request {next draw, withdrawal, w, the position's owner}
/// holding R; a partial withdrawal's output 2 is the position less w holding R; every other output
/// comes back to the wallet, which pays exactly the fee (+ R for a partial one: the whole position's
/// R becomes the request's).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WithdrawFigures {
    pub round: Dec,
    pub shares: Dec,
    pub position_shares: Dec,
    pub whole: bool,
    /// The position left (shares) and the worths at today's price (the draw sets the real price).
    pub remainder: Dec,
    pub worth: Dec,
    pub remainder_worth: Dec,
    /// R posted for the request (partial; 0 when whole); back with its final settlement.
    pub reserve: Dec,
    pub fee: Dec,
    /// fee + reserve: what leaves the wallet.
    pub total: Dec,
    pub owner: Address,
    pub request_from: Dec,
    pub revocable: bool,
    pub request: CovOut,
    pub position_next: Option<CovOut>,
    pub vault_next: CovOut,
}

pub fn withdraw_figures(net: Net, i: &WithdrawFiguresInput) -> Result<WithdrawFigures> {
    let cov = cov_of(&i.covenant_id)?;
    let u = unsigned_of(net, &i.built)?;
    let (vo, v, r) = vault_of(&cov, &i.vault)?;
    let (po, p) = position_of(&cov, &i.position)?;
    let w = dec_i64("shares", &i.shares)?;
    w_ok(&v, &p, w)?;
    if !spends(&u, 0, &vo) || !spends(&u, 1, &po) {
        return Err(bound("inputs 0 and 1 are not the given vault and position"));
    }
    let partial = w < p.shares;
    let next = VaultState { pend_wsh: v.pend_wsh + w, ..v.clone() };
    let vault_next = cov_out(net, &u, 0, &State::Vault(next), vo.entry.amount, "the vault with the shares pending")?;
    let rq = RequestState { round: v.round + 1, kind: KIND_WITHDRAW, amount: w, owner_class: p.owner_class, owner_key: p.owner_key };
    let request = cov_out(net, &u, 1, &State::Request(rq), r, "this withdrawal's request holding R")?;
    let position_next = if partial {
        let rest = PositionState { shares: p.shares - w, ..p.clone() };
        Some(cov_out(net, &u, 2, &State::Position(rest), r, "the position less the shares")?)
    } else {
        None
    };
    let from = request_from(&vo);
    if u.tx.lock_time != from {
        return Err(bound("the lock time is not the vault's creation DAA + VAULT_REQ_DELAY"));
    }
    let posted = if partial { r } else { 0 };
    let total = u.fee.checked_add(posted).ok_or_else(|| amount_err("total overflows"))?;
    if wallet_net(&i.built, 2 + partial as usize)? != total as i128 {
        return Err(bound("what the wallet pays is not the fee (+ R for a partial withdrawal)"));
    }
    let rest = p.shares - w;
    Ok(WithdrawFigures {
        round: (v.round + 1).to_string(),
        shares: w.to_string(),
        position_shares: p.shares.to_string(),
        whole: !partial,
        remainder: rest.to_string(),
        worth: worth(w, &v).to_string(),
        remainder_worth: worth(rest, &v).to_string(),
        reserve: posted.to_string(),
        fee: u.fee.to_string(),
        total: total.to_string(),
        owner: owner_of(net, p.owner_class, &p.owner_key)?,
        request_from: from.to_string(),
        revocable: false,
        request,
        position_next,
        vault_next,
    })
}

/// `settleFigures`: the `buildSettle` input fields and its `BuiltTx`.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettleFiguresInput {
    pub covenant_id: Hex,
    pub request: CovUtxo,
    pub record: CovUtxo,
    pub built: BuiltTx,
}

/// The rows of a built `settle()` (spec S5, 7), each checked against the transaction: inputs 0 and
/// 1 spend the given request and record; every output `SettleCase::expected` names is there with its
/// script and value (the record less this settlement unless drained, the new position or the
/// request left, the payout to the owner, the record's reserve to its funder when drained); every
/// other output comes back to the wallet. `total` = what leaves the wallet now (negative: it gains,
/// when the payout or the record's reserve is its own).
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettleFigures {
    /// The draw the request converted at.
    pub round: Dec,
    pub kind: RequestKindName,
    pub settle: SettleView,
    pub owner: Address,
    /// The payout goes to this wallet (the owner is the wallet's address).
    pub payout_to_wallet: bool,
    /// The record's reserve comes back to this wallet (it funded the record).
    pub record_refund_to_wallet: bool,
    pub fee: Dec,
    /// A signed decimal (sompi).
    pub total: String,
    pub record_next: Option<CovOut>,
    pub position_next: Option<CovOut>,
    pub request_next: Option<CovOut>,
    /// The payout output's index (Refund, Final, Pay).
    pub payout_index: Option<u32>,
}

pub fn settle_figures(net: Net, i: &SettleFiguresInput) -> Result<SettleFigures> {
    let cov = cov_of(&i.covenant_id)?;
    let u = unsigned_of(net, &i.built)?;
    let (ro, rq) = request_of(&cov, &i.request)?;
    let (co, rc) = record_of(&cov, &i.record)?;
    if !spends(&u, 0, &ro) || !spends(&u, 1, &co) {
        return Err(bound("inputs 0 and 1 are not the given request and record"));
    }
    let c = settle_case(&cov, &ro, &rq, &co, &rc)?;
    let view = settle_view(net, &c).ok_or_else(|| Error::new(Code::Refused, "the settlement reference refuses this request against this record"))?;
    let x = c.expected();
    let val = |n: usize| x.values.get(n).copied().ok_or_else(|| bound("no value"));
    let record_next = match x.record_output {
        Some(n) => Some(cov_out(net, &u, n, &State::Record(x.record.clone()), val(n)?, "the record less this settlement")?),
        None => None,
    };
    let (mut position_next, mut request_next) = (None, None);
    if let Some(n) = x.cov2_output {
        match (&x.position, &x.request) {
            (Some(p), _) => position_next = Some(cov_out(net, &u, n, &State::Position(p.clone()), val(n)?, "the minted position")?),
            (_, Some(r)) => request_next = Some(cov_out(net, &u, n, &State::Request(r.clone()), val(n)?, "the request left")?),
            _ => return Err(bound("a covenant output without its state")),
        }
    }
    let owner_spk = lottery::claim::payout_spk(rq.owner_class, &rq.owner_key);
    if let Some(n) = x.payout_output {
        plain_out(&u, n, &owner_spk, val(n)?, "the payout to the request's owner")?;
    }
    if let Some(n) = x.refund_output {
        plain_out(&u, n, &lottery::claim::payout_spk(rc.funder_class, &rc.funder_key), val(n)?, "the record's reserve to its funder")?;
    }
    let fixed = [x.record_output, x.cov2_output, x.payout_output, x.refund_output].iter().flatten().count();
    let total = wallet_net(&i.built, fixed)?;
    let payout_to_wallet = to_wallet(&i.built, x.payout_output);
    let record_refund_to_wallet = to_wallet(&i.built, x.refund_output);
    let mut gains: i128 = 0;
    if let (true, Some(n)) = (payout_to_wallet, x.payout_output) {
        gains += val(n)? as i128;
    }
    if let (true, Some(n)) = (record_refund_to_wallet, x.refund_output) {
        gains += val(n)? as i128;
    }
    if total != u.fee as i128 - gains {
        return Err(bound("what the wallet pays is not the fee less what comes back to it"));
    }
    Ok(SettleFigures {
        round: rq.round.to_string(),
        kind: if rq.kind == KIND_DEPOSIT { RequestKindName::Deposit } else { RequestKindName::Withdrawal },
        settle: view,
        owner: owner_of(net, rq.owner_class, &rq.owner_key)?,
        payout_to_wallet,
        record_refund_to_wallet,
        fee: u.fee.to_string(),
        total: total.to_string(),
        record_next,
        position_next,
        request_next,
        payout_index: x.payout_output.map(|n| n as u32),
    })
}
