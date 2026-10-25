//! The unsigned builders of the web-facing layer (spec frontend 6.1 "build", 6.2; F1 part
//! B), one per entry point: `buy`, `claim` (and `claim_from_store`), `return_deposit`,
//! `close`, `draw` (and `draw_from_store`; a void when there is no anchor proof),
//! `request_deposit`, `request_withdraw`, `settle`; plus the fee (`fee_of`, `with_fee`).
//!
//! Each takes the entry point's `*Case` (its states decoded and verified against the chain
//! by the page, `web::verify`), the outpoints of the covenant inputs, and `Funds`: the
//! wallet's coins, its change script and the fee. It
//! 1. checks the funds and every case field the builder's arithmetic reads, against the
//!    covenant's own ranges (`verify::check_*`) and the reference's preconditions, so the
//!    builder below runs inside its domain (no panic; a refusal is a typed error);
//! 2. builds with the library's own builder (`build_with`, the native code path: the bytes
//!    are the native builder's for the same case), the funding and change on a
//!    placeholder key;
//! 3. puts the wallet's coins and change script in place of the placeholder's (`bind`), and
//!    refuses if the placeholder key is left anywhere in the transaction;
//! 4. returns `sign::unsigned` (budgets and storage mass committed, every covenant input run
//!    in the engine, the inputs to sign, the masses, the fee).
//!
//! The fixture-only knobs of the cases (`Lie`s, budget deltas, the `m_next` override, other
//! K_MAX variants) must be at their defaults; anything else is `InputRange`.

use std::sync::Arc;

use harness::{ChainSim, Hash, Masses, ScriptPublicKey, SimBlock, TransactionOutpoint, UtxoEntry};

use super::sign::{Unsigned, unsigned};
use super::verify::{
    RESERVE_MAX, canonical, check_anchor, check_controller, check_drawer, check_marker, check_params, check_pool, check_position, check_record,
    check_request, check_roots, check_shard, check_ticket, check_vault,
};
use super::{Code, Error, Result, WithCode, ensure};
use crate::anchor::AnchorProof;
use crate::buy::{BuyCase, Built, InKind, M_REF, OwnerKey, OwnerPub, Templates, spk_of};
use crate::claim::{ClaimCase, ReturnCase, ReturnPath};
use crate::close::{CloseCase, FundingIn, MIN_CALLER_OUTPUT};
use crate::draw::DrawCase;
use crate::game::{A_MAX, BP, DAA_MAX, M_DRAW, MIN_DEPOSIT, TOTAL_MAX, round_amounts};
use crate::state::{K_MAX, Payout};
use crate::vault::{DepositCase, SettleCase, WithdrawCase};

/// The most funding inputs a builder takes (controller.sil, drawer.sil and the ticket's
/// `F_MAX`).
pub const F_MAX: usize = 8;
/// The highest fee rate accepted (sompi/gram), 10 x the 100 sompi/gram floor (as
/// `tn10::fees`, M8 S5, D115).
pub const MAX_RATE_SOMPI_PER_GRAM: u64 = 1_000;
/// The most any one transaction may pay in fees: the largest priority mass a block admits
/// (500,000 grams) at the maximum rate, 5 KAS (as `tn10::fees`).
pub const MAX_FEE_SOMPI: u64 = 500_000 * MAX_RATE_SOMPI_PER_GRAM;

/// The key the builders put on the funding inputs and the change while they build; its
/// script is replaced by the wallet's in `bind`, and a transaction that still carries it is
/// refused. It signs nothing.
const PLACEHOLDER_SK: [u8; 32] = [0x6b; 32];
/// The owner key of a withdrawal while it is built (replaced by the position owner's).
const PLACEHOLDER_OWNER_SK: [u8; 32] = [0x6c; 32];

/// A wallet coin: a confirmed, zero-ID UTXO locked to the wallet's Schnorr P2PK script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coin {
    pub outpoint: TransactionOutpoint,
    pub entry: UtxoEntry,
}

/// What pays for a transaction: the wallet's coins (funding inputs, in order), the script
/// the change (or the caller output of `close()` and `draw()`) goes to, and the fee.
#[derive(Clone, Debug)]
pub struct Funds {
    pub coins: Vec<Coin>,
    pub change: ScriptPublicKey,
    pub fee: u64,
}

/// The parsed standard (D11) script of a script public key: P2PK Schnorr, P2PK ECDSA or P2SH.
pub fn payout_of_spk(spk: &ScriptPublicKey) -> Option<Payout> {
    let s = spk.script();
    if spk.version() != 0 {
        return None;
    }
    match (s.len(), s.first(), s.last()) {
        (34, Some(0x20), Some(0xac)) => s[1..33].try_into().ok().map(Payout::Schnorr),
        (35, Some(0x21), Some(0xab)) => s[1..34].try_into().ok().map(Payout::Ecdsa),
        (35, Some(0xaa), Some(0x87)) if s[1] == 0x20 => s[2..34].try_into().ok().map(Payout::P2sh),
        _ => None,
    }
}

fn check_coin(n: usize, c: &Coin) -> Result<()> {
    ensure(c.entry.covenant_id.is_none(), Code::WalletScript, || format!("coin {n} carries a covenant ID"))?;
    ensure(matches!(payout_of_spk(&c.entry.script_public_key), Some(Payout::Schnorr(_))), Code::WalletScript, || format!("coin {n}: not a Schnorr P2PK script"))?;
    ensure(c.entry.block_daa_score != u64::MAX, Code::CoinUnconfirmed, || format!("coin {n} is not confirmed"))?;
    ensure(c.entry.block_daa_score < DAA_MAX as u64, Code::InputRange, || format!("coin {n}: DAA score {}", c.entry.block_daa_score))?;
    ensure(c.entry.amount > 0 && c.entry.amount <= TOTAL_MAX as u64, Code::InputRange, || format!("coin {n}: amount {}", c.entry.amount))?;
    ensure(!c.entry.is_coinbase, Code::WalletScript, || format!("coin {n} is a coinbase output"))
}

impl Funds {
    /// The coins (1 ..= F_MAX, each confirmed and the wallet's Schnorr P2PK), the change
    /// script (a D11 standard script) and the fee (at most MAX_FEE_SOMPI).
    pub fn check(&self) -> Result<()> {
        ensure((1..=F_MAX).contains(&self.coins.len()), Code::InputRange, || format!("{} funding coins (1..={F_MAX})", self.coins.len()))?;
        for (n, c) in self.coins.iter().enumerate() {
            check_coin(n, c)?;
        }
        ensure(payout_of_spk(&self.change).is_some(), Code::WalletScript, || "the change script is not a standard address script".into())?;
        ensure(self.fee <= MAX_FEE_SOMPI, Code::FeeRate, || format!("fee {} above {MAX_FEE_SOMPI}", self.fee))?;
        self.total().map(|_| ())
    }

    /// The coins' total (at most TOTAL_MAX).
    pub fn total(&self) -> Result<u64> {
        let t = self.coins.iter().try_fold(0u64, |a, c| a.checked_add(c.entry.amount)).ok_or_else(|| Error::new(Code::Overflow, "coin total overflows"))?;
        ensure(t <= TOTAL_MAX as u64, Code::Overflow, || format!("coin total {t}"))?;
        Ok(t)
    }

    fn funding_in(&self) -> Vec<FundingIn> {
        self.coins.iter().map(|c| FundingIn { value: c.entry.amount, daa: c.entry.block_daa_score }).collect()
    }

    fn values(&self) -> Vec<u64> {
        self.coins.iter().map(|c| c.entry.amount).collect()
    }
}

fn templates_for(b_max: usize) -> Result<Arc<Templates>> {
    ensure(b_max == crate::buy::b_max(), Code::InputRange, || format!("b_max {b_max}: only the committed B_MAX is embedded"))?;
    Templates::try_get(b_max).code(Code::Templates)
}

fn daa(what: &str, v: u64) -> Result<()> {
    ensure(v < DAA_MAX as u64, Code::InputRange, || format!("{what} DAA score {v}"))
}

fn value(what: &str, v: u64) -> Result<()> {
    ensure(v <= TOTAL_MAX as u64, Code::InputRange, || format!("{what} value {v}"))
}

fn sub(a: u64, b: u64, what: &str) -> Result<u64> {
    a.checked_sub(b).ok_or_else(|| Error::new(Code::FundingShort, format!("{what}: {a} < {b}")))
}

fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or_else(|| Error::new(Code::Overflow, format!("{a} + {b} overflows")))
}

fn mul(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b).ok_or_else(|| Error::new(Code::Overflow, format!("{a} x {b} overflows")))
}

/// Step 3: the wallet's coins on the funding inputs `first ..`, the outpoints in input order
/// (`cov` for the inputs before `first`, then the coins'), and the change script on output
/// `change` (if any). Refuses a transaction that still carries a placeholder script.
fn bind(b: &mut Built, first: usize, cov: &[TransactionOutpoint], funds: &Funds, change: Option<usize>) -> Result<()> {
    ensure(cov.len() == first, Code::InputRange, || format!("{} covenant outpoints, the transaction has {first} covenant inputs", cov.len()))?;
    ensure(b.inputs.len() == first + funds.coins.len(), Code::InputRange, || format!("{} inputs for {first} + {} coins", b.inputs.len(), funds.coins.len()))?;
    let mut ops = cov.to_vec();
    for (k, c) in funds.coins.iter().enumerate() {
        let inp = &mut b.inputs[first + k];
        ensure(matches!(inp.kind, InKind::Funding), Code::InputRange, || format!("input {} is not a funding input", first + k))?;
        // The value must be the coin's (the case was built from it); the DAA score is the
        // coin's for every case that reads it (close(): T'), and buy() builds with 0.
        ensure(inp.entry.amount == c.entry.amount, Code::InputRange, || format!("input {}: value differs from coin {k}", first + k))?;
        inp.entry = c.entry.clone();
        ops.push(c.outpoint);
    }
    b.outpoints = ops;
    if let Some(i) = change {
        let o = b.outputs.get_mut(i).ok_or_else(|| Error::new(Code::InputRange, format!("no change output {i}")))?;
        o.script_public_key = funds.change.clone();
    }
    no_placeholder(b)
}

fn no_placeholder(b: &Built) -> Result<()> {
    let ph = [spk_of(&PLACEHOLDER_SK), OwnerKey::Schnorr(PLACEHOLDER_OWNER_SK).spk()];
    let left = b.inputs.iter().any(|i| ph.contains(&i.entry.script_public_key)) || b.outputs.iter().any(|o| ph.contains(&o.script_public_key));
    ensure(!left, Code::Encode, || "a placeholder script is left in the transaction".into())
}

// ------------------------------------------------------------------------------ buy

/// `buy()` (spec M1) of `case.bundle` on the shard at `shard` (its state `case.shard`, value
/// `case.shard_value()`, the tree `case.tree` of its purchases), funded by `funds`; change
/// (if any) last.
pub fn buy(case: &BuyCase, shard: TransactionOutpoint, funds: &Funds) -> Result<Unsigned> {
    funds.check()?;
    let t = templates_for(case.b_max)?;
    let s = &case.shard;
    check_shard(s)?;
    ensure(case.reserve >= 1 && case.reserve <= RESERVE_MAX as u64, Code::StateRange, || format!("shard reserve {}", case.reserve))?;
    daa("shard", case.shard_daa)?;
    let b = case.bundle.len();
    ensure((1..=t.b_max).contains(&b), Code::InputRange, || format!("bundle of {b} combinations (1..={})", t.b_max))?;
    ensure(case.tree.m as i64 == s.m && case.tree.root() == s.root, Code::TreeMismatch, || "the tree is not the shard's (m or root)".into())?;
    let mut tree = case.tree.clone();
    for (n, c) in case.bundle.iter().enumerate() {
        ensure(canonical(&c.bytes(), s.m), Code::PurchaseInvalid, || format!("combination {n} is not canonical for m = {}", s.m))?;
        tree.try_increment(c.rank(), c.bonus).map_err(|e| Error::new(Code::PurchaseInvalid, format!("combination {n}: {e}")))?;
    }
    let gross = mul(b as u64, s.price as u64)?;
    let referral = if case.with_ref { mul(gross, s.s_r as u64)? / BP as u64 } else { 0 };
    ensure(!case.with_ref || referral >= M_REF, Code::Refused, || format!("referral {referral} below M_REF (shard check 9)"))?;
    ensure(add(s.sales as u64, gross)? <= TOTAL_MAX as u64, Code::Refused, || "sales would exceed TOTAL_MAX".into())?;
    let deposits = mul(b as u64, s.d as u64)?;
    let change = sub(funds.total()?, add(add(gross, deposits)?, funds.fee)?, "the coins do not cover price + deposits + fee")?;
    let mut c = case.clone();
    c.funding = funds.values();
    c.fee = funds.fee;
    c.change = change > 0;
    c.funding_sk = PLACEHOLDER_SK;
    let mut built = c.build_with(t);
    let last = built.outputs.len() - 1;
    bind(&mut built, 1, &[shard], funds, c.change.then_some(last))?;
    unsigned(&built)
}

// ------------------------------------------------------------------------------ claim

fn no_lie_claim(c: &ClaimCase) -> Result<()> {
    let l = &c.lie;
    let none = l.counts.is_none() && l.k_proofs.is_none() && l.swap.is_none() && l.rank.is_none() && l.h.is_none() && l.prize.is_none() && l.mode.is_none();
    ensure(none && c.budget_delta == 0, Code::InputRange, || "a TEST-ONLY claim lie is set".into())
}

/// `claim()` (spec M4) of index `case.idx` of the ticket at `ticket` against the pool at
/// `pool`, the count proofs from `case.trees` (checked against the pool's frozen roots).
pub fn claim(case: &ClaimCase, ticket: TransactionOutpoint, pool: TransactionOutpoint, funds: &Funds) -> Result<Unsigned> {
    funds.check()?;
    let t = templates_for(case.b_max)?;
    no_lie_claim(case)?;
    check_ticket(&case.ticket, t.b_max)?;
    check_pool(&case.pool)?;
    value("ticket", case.ticket_value)?;
    value("pool", case.pool_value)?;
    daa("ticket", case.ticket_daa)?;
    daa("pool", case.pool_daa)?;
    ensure((1..=K_MAX).contains(&case.trees.len()) && case.trees.iter().all(|tr| tr.m as i64 == case.pool.m), Code::TreeMismatch, || "the trees do not fit the pool (k or m)".into())?;
    check_roots(&case.trees, &case.pool.roots, case.pool.k)?;
    let change = sub(funds.total()?, funds.fee, "the coins do not cover the fee")?;
    let mut c = case.clone();
    c.funding = funds.funding_in();
    c.fee = funds.fee;
    c.change = change > 0;
    c.funding_sk = PLACEHOLDER_SK;
    let x = c.checked_expected().code(Code::Refused)?;
    let mut built = c.build_with(t);
    bind(&mut built, 2, &[ticket, pool], funds, x.change_output)?;
    unsigned(&built)
}

/// `claim()` of the live ticket `ticket` built from a store (an index or a `wallet/1`,
/// `ClaimCase::from_store`: its first unclaimed index that wins a tier with a live pool).
pub fn claim_from_store<S: crate::store::ClaimStore + ?Sized>(store: &S, ticket: TransactionOutpoint, funds: &Funds) -> Result<Unsigned> {
    funds.check()?;
    let fund: Vec<crate::store::Funding> = funds.coins.iter().map(|c| crate::store::Funding { outpoint: c.outpoint, value: c.entry.amount, daa: c.entry.block_daa_score }).collect();
    let payer = crate::store::Payer { funding_sk: PLACEHOLDER_SK, fee: funds.fee };
    let (case, ops) = ClaimCase::from_store(store, ticket, &fund, &payer).code(Code::StoreIncomplete)?;
    let (t, p) = (ops.first().copied(), ops.get(1).copied());
    claim(&case, t.ok_or_else(|| Error::new(Code::StoreIncomplete, "no ticket outpoint"))?, p.ok_or_else(|| Error::new(Code::StoreIncomplete, "no pool outpoint"))?, funds)
}

// ------------------------------------------------------------------------------ return

/// `return_deposit()` (spec M4 A5) of the ticket at `ticket`: path b alone (`pool = None`,
/// `ReturnPath::After`), or path a with the expired pool at `pool` (`ReturnPath::WithPool`).
pub fn return_deposit(case: &ReturnCase, ticket: TransactionOutpoint, pool: Option<TransactionOutpoint>, funds: &Funds) -> Result<Unsigned> {
    funds.check()?;
    let t = templates_for(case.b_max)?;
    ensure(case.budget_delta == 0, Code::InputRange, || "a TEST-ONLY budget delta is set".into())?;
    check_ticket(&case.ticket, t.b_max)?;
    value("ticket", case.ticket_value)?;
    daa("ticket", case.ticket_daa)?;
    let cov = match (&case.path, pool) {
        (ReturnPath::After, None) => vec![ticket],
        (ReturnPath::WithPool { pool: p, pool_value, pool_daa }, Some(op)) => {
            check_pool(p)?;
            value("pool", *pool_value)?;
            daa("pool", *pool_daa)?;
            ensure(p.round == case.ticket.round, Code::Refused, || "the pool is not of the ticket's round".into())?;
            vec![ticket, op]
        }
        _ => return Err(Error::new(Code::InputRange, "the pool outpoint does not match the return path")),
    };
    let change = sub(funds.total()?, funds.fee, "the coins do not cover the fee")?;
    let mut c = case.clone();
    c.funding = funds.funding_in();
    c.fee = funds.fee;
    c.change = change > 0;
    c.funding_sk = PLACEHOLDER_SK;
    let x = c.expected();
    let mut built = c.build_with(t);
    bind(&mut built, cov.len(), &cov, funds, x.change_output)?;
    unsigned(&built)
}

// ------------------------------------------------------------------------------ close

/// `close()` (spec M2) of the round of `case.controller`: `outpoints` are the controller,
/// the params and the k shards in order; the caller output (last) goes to `funds.change`.
/// `now` is the virtual DAA score (D24 stale-input guard, `CloseCase::check_fresh`).
pub fn close(case: &CloseCase, outpoints: &[TransactionOutpoint], funds: &Funds, now: u64) -> Result<Unsigned> {
    funds.check()?;
    let t = templates_for(case.b_max)?;
    ensure(case.m_next.is_none() && case.k_max == K_MAX, Code::InputRange, || "a TEST-ONLY close override is set".into())?;
    let c = &case.controller;
    check_controller(c)?;
    check_params(&case.params)?;
    let k = case.shards.len();
    ensure((1..=K_MAX).contains(&k) && c.k == k as i64, Code::Refused, || format!("{k} shards for controller k = {}", c.k))?;
    ensure(c.drawn == 1, Code::Refused, || "the previous round is not drawn (close() check 8)".into())?;
    ensure(case.params.round == c.round, Code::Refused, || "the params are of another round".into())?;
    value("boost pot", case.pot)?;
    daa("controller", case.controller_daa)?;
    daa("params", case.params_daa)?;
    let mut sales: i64 = 0;
    for (i, s) in case.shards.iter().enumerate() {
        check_shard(&s.state)?;
        daa("shard", s.daa)?;
        let st = &s.state;
        let same = st.round == c.round && st.shard_index == i as i64 && st.m == c.m && st.price == c.price && st.d == c.d && st.s_r == c.s_r && st.deadline == c.deadline;
        ensure(same, Code::Refused, || format!("shard {i} does not match the controller (close() check 10)"))?;
        ensure(st.sales <= TOTAL_MAX - sales, Code::Refused, || "the round's sales exceed TOTAL_MAX".into())?;
        sales += st.sales;
    }
    let ref_paid: i64 = case.shards.iter().map(|s| s.state.ref_paid).sum();
    let amounts = round_amounts(sales, ref_paid, &crate::game::Shares::of(c));
    ensure(amounts.b_r >= 0, Code::Refused, || "b_r < 0 (close() check 11)".into())?;
    add(add(case.pot, c.acc as u64)?, c.reserve as u64)?;
    ensure(c.j_boost as u64 <= case.pot, Code::Refused, || "the boost pot does not cover J_boost (close() check 12)".into())?;
    let mut x = case.clone();
    x.funding = funds.funding_in();
    x.fee = funds.fee;
    x.funding_sk = PLACEHOLDER_SK;
    x.check_fresh(now).code(Code::Policy)?;
    let caller = funds.total()? as i128 - case.reserve() as i128 - funds.fee as i128 + amounts.c_close as i128;
    ensure(caller >= MIN_CALLER_OUTPUT as i128, Code::Policy, || format!("the caller output {caller} is below MIN_CALLER_OUTPUT = {MIN_CALLER_OUTPUT}"))?;
    let mut built = x.build_with(t);
    bind(&mut built, k + 2, outpoints, funds, Some(x.caller_output()))?;
    unsigned(&built)
}

// ------------------------------------------------------------------------------ draw

fn no_lie_draw(c: &DrawCase) -> Result<()> {
    let l = &c.lie;
    let none = l.counts.is_none()
        && l.rank.is_none()
        && l.win5.is_none()
        && l.bonus.is_none()
        && l.witness.is_none()
        && l.paid.is_none()
        && l.v_delta == 0
        && l.o_delta == 0
        && l.mk_daa.is_none()
        && l.anchor.is_none()
        && l.void_pools.is_none()
        && l.wc.is_none()
        && l.conv_v.is_none()
        && l.j_v.is_none()
        && l.refund.is_none()
        && l.record.is_none()
        && l.out_delta == 0;
    ensure(none, Code::InputRange, || "a TEST-ONLY draw lie is set".into())
}

/// The chain the drawer's dry run sees for a real anchor proof: X alone, with its committed
/// parent commitment (`ChainSim::from_real_chain`), so `OpChainblockSeqCommit(X.hash)` gives
/// X's commitment as the node's accessor does (spec M7 A7; the drawer calls it on X only,
/// Stage 5 fix B). P's hash is not in the proof; a placeholder stands for it.
pub fn sim_from_proof(p: &AnchorProof) -> Result<ChainSim> {
    let x = &p.x;
    ensure(x.hash != [0; 32], Code::AnchorInvalid, || "anchor X.hash is zero".into())?;
    let parent = if x.hash == [1; 32] { Hash::from_bytes([2; 32]) } else { Hash::from_bytes([1; 32]) };
    let b = SimBlock {
        hash: Hash::from_bytes(x.hash),
        parent,
        blue_score: x.blue_score,
        daa_score: x.daa_score,
        sp_timestamp: x.sp_timestamp,
        activity_root: Hash::from_bytes(x.activity_root),
        miner_payload_root: Hash::from_bytes(x.miner_payload_root),
    };
    Ok(ChainSim::from_real_chain(Hash::from_bytes(x.parent_commit), &[b]))
}

/// `draw()` (spec M3, M7) of the closed round of `case.marker`, or its void when
/// `case.void` (no anchor): `outpoints` are the drawer, marker, vault, controller and the
/// absorbed pools in order; the caller output (last) goes to `funds.change`. A draw needs
/// `case.proof` (the anchor proof for D, checked here off-chain); the drawer's dry run
/// sees that proof's X (`sim_from_proof`).
pub fn draw(case: &DrawCase, outpoints: &[TransactionOutpoint], funds: &Funds) -> Result<Unsigned> {
    funds.check()?;
    let t = templates_for(case.b_max)?;
    no_lie_draw(case)?;
    check_drawer(&case.drawer)?;
    check_marker(&case.marker)?;
    check_vault(&case.vault)?;
    let ctrl = &case.controller;
    check_controller(ctrl)?;
    let mk = &case.marker;
    ensure(ctrl.drawn == 0 && ctrl.m <= M_DRAW && ctrl.k == mk.k, Code::Refused, || "the controller is not the next round's undrawn controller (drawer check 3)".into())?;
    ensure(mk.round == case.drawer.round && ctrl.round == mk.round + 1 && case.vault.round == mk.round - 1, Code::Refused, || "the drawer, marker, controller and vault rounds do not line up".into())?;
    for (w, v) in [("drawer", case.drawer_daa), ("marker", case.marker_daa), ("vault", case.vault_daa), ("controller", case.controller_daa)] {
        daa(w, v)?;
    }
    value("boost pot", case.pot)?;
    add(add(case.pot, ctrl.acc as u64)?, ctrl.reserve as u64)?;
    ensure(case.trees.iter().all(|tr| tr.m as i64 == mk.m), Code::TreeMismatch, || "a tree's m is not the marker's".into())?;
    check_roots(&case.trees, &mk.roots, mk.k)?;
    // drawer.sil check 4 (lines 454-461): S - P_R - C_close >= 0 and J_boost <= TOTAL_MAX - it.
    let held = mk.sales - mk.ref_paid - crate::game::muldiv(mk.sales, mk.s_c);
    ensure(held >= 0 && mk.j_boost <= TOTAL_MAX - held, Code::Refused, || "the marker's value is out of range (drawer check 4)".into())?;
    // The second-prize pool holds n2 x G2 with n2 <= m - 1 (`game::pool_set`, whose checked
    // product panics where the drawer's checked multiplication rejects): refused up front
    // when even the largest n2 overflows (G2 above about 1.9 x 10^17 sompi, more than the
    // supply could fund).
    ensure(mk.g2.checked_mul(mk.m - 1).is_some(), Code::Refused, || "G2 x (m - 1) overflows".into())?;
    ensure(case.absorbed.len() <= A_MAX, Code::InputRange, || format!("{} absorbed pools (at most {A_MAX})", case.absorbed.len()))?;
    for p in &case.absorbed {
        check_pool(&p.state)?;
        value("absorbed pool", p.remaining)?;
        daa("absorbed pool", p.daa)?;
    }
    let mut c = case.clone();
    match (&case.proof, case.void) {
        (Some(p), false) => {
            check_anchor(p, c.d())?;
            ensure(c.x_hash == p.x_hash(), Code::AnchorInvalid, || "x_hash is not the proof's X.hash".into())?;
            c.sim = Some(Arc::new(sim_from_proof(p)?));
        }
        (None, true) => {
            // A void calls no seqcommit opcode; its dry run needs no chain beyond a root.
            c.sim = Some(Arc::new(ChainSim::line(1, 1, 1)));
        }
        _ => return Err(Error::new(Code::InputRange, "a draw needs an anchor proof, a void none")),
    }
    c.funding = funds.funding_in();
    c.fee = funds.fee;
    c.funding_sk = PLACEHOLDER_SK;
    // F1 review: coins below the new reserves and the fee are a funding shortfall, not a
    // rule refusal.
    let x = c.try_expected().map_err(|d| Error::new(if d.starts_with(crate::draw::FUNDING_SHORT) { Code::FundingShort } else { Code::Refused }, d))?;
    let mut built = c.build_with(t);
    bind(&mut built, 4 + case.absorbed.len(), outpoints, funds, Some(x.caller_output()))?;
    unsigned(&built)
}

/// `draw()` of the closed round built from an index (`DrawCase::from_store`: its marker,
/// drawer, vault, controller and frozen trees); `anchor = None` builds the void.
pub fn draw_from_store(ix: &crate::index::Rebuild, anchor: Option<AnchorProof>, funder: Payout, funds: &Funds) -> Result<Unsigned> {
    funds.check()?;
    let fund: Vec<crate::store::Funding> = funds.coins.iter().map(|c| crate::store::Funding { outpoint: c.outpoint, value: c.entry.amount, daa: c.entry.block_daa_score }).collect();
    let payer = crate::store::Payer { funding_sk: PLACEHOLDER_SK, fee: funds.fee };
    let (case, ops) = DrawCase::from_store(ix, anchor, &fund, funder, &payer).code(Code::StoreIncomplete)?;
    let n = 4 + case.absorbed.len();
    draw(&case, ops.get(..n).ok_or_else(|| Error::new(Code::StoreIncomplete, "outpoints"))?, funds)
}

// ------------------------------------------------------------------------------ vault

/// `request_deposit()` (spec M5 A4.1) of `case.d` into the vault at `vault`.
pub fn request_deposit(case: &DepositCase, vault: TransactionOutpoint, funds: &Funds) -> Result<Unsigned> {
    funds.check()?;
    let t = templates_for(case.b_max)?;
    ensure(case.budget_delta == 0, Code::InputRange, || "a TEST-ONLY budget delta is set".into())?;
    check_vault(&case.vault)?;
    ensure(case.reserve >= 1 && case.reserve <= RESERVE_MAX as u64, Code::StateRange, || format!("vault reserve {}", case.reserve))?;
    daa("vault", case.vault_daa)?;
    ensure((MIN_DEPOSIT..=TOTAL_MAX).contains(&case.d), Code::Refused, || format!("deposit {} below MIN_DEPOSIT or above TOTAL_MAX (vault check 5)", case.d))?;
    ensure(case.vault.pend_dep <= TOTAL_MAX - case.d, Code::Refused, || "pending deposits would exceed TOTAL_MAX".into())?;
    let need = add(add(case.d as u64, case.reserve)?, funds.fee)?;
    let change = sub(funds.total()?, need, "the coins do not cover deposit + reserve + fee")?;
    let mut c = case.clone();
    c.funding = funds.funding_in();
    c.fee = funds.fee;
    c.change = change > 0;
    c.funding_sk = PLACEHOLDER_SK;
    let x = c.expected();
    let mut built = c.build_with(t);
    bind(&mut built, 1, &[vault], funds, x.change_output)?;
    unsigned(&built)
}

/// The owner input of a withdrawal: a coin of the position owner's address (any value), and
/// the owner's public key behind the position's owner script.
#[derive(Clone, Debug)]
pub struct OwnerCoin {
    pub coin: Coin,
    pub owner: OwnerPub,
}

/// `request_withdraw()` (spec M5 A4.2) of `case.w` shares of the position at `position`
/// from the vault at `vault`. The owner input (D67, signed by the owner's wallet) is
/// `owner.coin` and goes first among the plain inputs; `funds.coins` follow it.
pub fn request_withdraw(case: &WithdrawCase, vault: TransactionOutpoint, position: TransactionOutpoint, owner: &OwnerCoin, funds: &Funds) -> Result<Unsigned> {
    // The owner's coin may be the only plain input: `funds.coins` may be empty here.
    ensure(funds.coins.len() < F_MAX, Code::InputRange, || format!("{} funding coins", funds.coins.len()))?;
    for (n, c) in funds.coins.iter().enumerate() {
        check_coin(n, c)?;
    }
    ensure(payout_of_spk(&funds.change).is_some(), Code::WalletScript, || "the change script is not a standard address script".into())?;
    ensure(funds.fee <= MAX_FEE_SOMPI, Code::FeeRate, || format!("fee {}", funds.fee))?;
    let t = templates_for(case.b_max)?;
    ensure(case.budget_delta == 0, Code::InputRange, || "a TEST-ONLY budget delta is set".into())?;
    check_vault(&case.vault)?;
    check_position(&case.position)?;
    ensure(case.reserve >= 1 && case.reserve <= RESERVE_MAX as u64, Code::StateRange, || format!("vault reserve {}", case.reserve))?;
    daa("vault", case.vault_daa)?;
    daa("position", case.position_daa)?;
    let ps = &case.position;
    ensure((1..=ps.shares).contains(&case.w), Code::Refused, || format!("withdraw {} of {} shares", case.w, ps.shares))?;
    ensure(case.policy_ok(), Code::Policy, || "a partial withdrawal below wmin (D86, vault W4b)".into())?;
    ensure(case.vault.pend_wsh <= case.vault.total_shares - case.w, Code::Refused, || "pending withdrawals would exceed the shares".into())?;
    // The owner input's script must be the position owner's D11 script (vault check 5).
    let owner_payout = Payout::from_fields(ps.owner_class, &ps.owner_key);
    ensure(owner.owner.payout() == owner_payout, Code::WalletScript, || "the owner key is not the position's owner".into())?;
    ensure(owner.coin.entry.script_public_key == owner_payout.spk(), Code::WalletScript, || "the owner coin is not at the position owner's address".into())?;
    ensure(owner.coin.entry.covenant_id.is_none() && !owner.coin.entry.is_coinbase, Code::WalletScript, || "the owner coin is not a plain UTXO".into())?;
    ensure(owner.coin.entry.block_daa_score != u64::MAX, Code::CoinUnconfirmed, || "the owner coin is not confirmed".into())?;
    daa("owner coin", owner.coin.entry.block_daa_score)?;
    value("owner coin", owner.coin.entry.amount)?;
    let total = funds.coins.iter().try_fold(owner.coin.entry.amount, |a, c| a.checked_add(c.entry.amount)).ok_or_else(|| Error::new(Code::Overflow, "coin total"))?;
    value("plain inputs", total)?;
    let partial = case.w < ps.shares;
    let need = add(funds.fee, if partial { case.reserve } else { 0 })?;
    let change = sub(total, need, "the coins do not cover the fee and the request reserve")?;
    let mut c = case.clone();
    c.owner = OwnerKey::Schnorr(PLACEHOLDER_OWNER_SK);
    c.owner_value = owner.coin.entry.amount;
    c.owner_daa = owner.coin.entry.block_daa_score;
    c.owner_at = 0;
    c.funding = funds.funding_in();
    c.fee = funds.fee;
    c.change = change > 0;
    c.funding_sk = PLACEHOLDER_SK;
    let x = c.expected();
    let mut built = c.build_with(t);
    let oi = c.owner_input();
    // The owner input in place: the coin, the public owner (signed by the wallet).
    built.inputs[oi].entry = owner.coin.entry.clone();
    built.inputs[oi].kind = InKind::OwnerPub(owner.owner.clone());
    // Funding coins follow the owner input; `bind` sees the owner's outpoint as a covenant one.
    bind(&mut built, oi + 1, &[vault, position, owner.coin.outpoint], funds, x.change_output)?;
    unsigned(&built)
}

/// `settle()` (spec M5 A6.1) of the request at `request` against the record at `record`.
pub fn settle(case: &SettleCase, request: TransactionOutpoint, record: TransactionOutpoint, funds: &Funds) -> Result<Unsigned> {
    funds.check()?;
    let t = templates_for(case.b_max)?;
    let l = &case.lie;
    ensure(l.sh.is_none() && l.pay.is_none() && l.mode.is_none() && l.drained.is_none() && case.budget_delta == 0, Code::InputRange, || "a TEST-ONLY settle lie is set".into())?;
    check_request(&case.request)?;
    check_record(&case.record)?;
    value("request", case.request_value)?;
    ensure(case.record_reserve >= 1 && case.record_reserve <= RESERVE_MAX as u64, Code::StateRange, || format!("record reserve {}", case.record_reserve))?;
    daa("request", case.request_daa)?;
    daa("record", case.record_daa)?;
    crate::game::settle(&case.record, &case.request).ok_or_else(|| Error::new(Code::Refused, "the settlement reference refuses this request against this record"))?;
    let change = sub(funds.total()?, funds.fee, "the coins do not cover the fee")?;
    let mut c = case.clone();
    c.funding = funds.funding_in();
    c.fee = funds.fee;
    c.change = change > 0;
    c.funding_sk = PLACEHOLDER_SK;
    let x = c.expected();
    let mut built = c.build_with(t);
    bind(&mut built, 2, &[request, record], funds, x.change_output)?;
    unsigned(&built)
}

// ------------------------------------------------------------------------------ fee

/// The fee of a transaction of these masses at `rate` sompi/gram (D120: `fee_for(priority
/// mass, rate)`, the rate raised to the 100 sompi/gram floor); a rate above
/// MAX_RATE_SOMPI_PER_GRAM or a fee above MAX_FEE_SOMPI is an error.
pub fn fee_of(m: &Masses, rate: u64) -> Result<u64> {
    ensure(rate <= MAX_RATE_SOMPI_PER_GRAM, Code::FeeRate, || format!("rate {rate} sompi/gram above {MAX_RATE_SOMPI_PER_GRAM}"))?;
    let fee = crate::buy::fee_for(m.priority, rate).code(Code::Overflow)?;
    ensure(fee <= MAX_FEE_SOMPI, Code::FeeRate, || format!("fee {fee} above {MAX_FEE_SOMPI}"))?;
    Ok(fee)
}

/// Build a transaction whose fee covers its own exact mass at `rate`: `mk(fee)` builds it
/// with that fee (the change absorbs it). The mass depends on the fee only through the
/// change output's storage mass, so this settles in a few rounds (as `tn10::fees::with_fee`).
///
/// The loop never drops a change worth something: a change too small to pay for its own
/// output (its storage mass) makes it fail. F1 review: that failure is `ChangeDust` when the
/// same coins cover the transaction without a change output (`diagnose`), and stays
/// `FundingShort` (or the fee cap's `FeeRate`) when they do not. Owner decision (DECISIONS
/// 2026-10-26 (2)): when the transaction without a change output pays exactly the fee its
/// own mass needs (the leftover is exactly zero), that transaction is the result: no change
/// output and no extra fee. Any other leftover below dust is still refused (`ChangeDust`),
/// so the player never gives coins away as fee.
pub fn with_fee(rate: u64, mut mk: impl FnMut(u64) -> Result<Unsigned>) -> Result<Unsigned> {
    let mut fee = fee_of(&Masses { compute: 0, transient: 0, storage: 0, priority: 1_000 }, rate)?;
    let mut prev: Option<Unsigned> = None;
    for _ in 0..6 {
        let u = match mk(fee) {
            Ok(u) => u,
            Err(e) if e.code == Code::FundingShort => return diagnose(rate, prev.as_ref(), e, &mut mk),
            Err(e) => return Err(e),
        };
        let need = match fee_of(&u.masses, rate) {
            Ok(n) => n,
            Err(e) if e.code == Code::FeeRate => return diagnose(rate, Some(&u), e, &mut mk),
            Err(e) => return Err(e),
        };
        if u.fee >= need {
            return Ok(u);
        }
        fee = need.max(fee + 1);
        prev = Some(u);
    }
    Err(Error::new(Code::FeeRate, format!("the fee did not settle at {rate} sompi/gram after 6 builds")))
}

/// Why the fee loop failed after `prev` built (F1 review). `prev` had a change output of
/// value c at fee f; the builders leave the change out when it would be 0, so `mk(f + c)` is
/// the same transaction without change. If one of `prev`'s plain outputs, taken as c, gives a
/// build with fewer outputs whose fee covers its own mass, the coins cover the transaction
/// and only the change is too small. If that build's fee is exactly what its mass needs (the
/// leftover is exactly zero), it is returned (owner decision, DECISIONS 2026-10-26 (2)).
/// Otherwise it is discarded and the error is `ChangeDust`, with the extra fee dropping the
/// change would pay in the detail. Otherwise `e` stands.
fn diagnose(rate: u64, prev: Option<&Unsigned>, e: Error, mk: &mut impl FnMut(u64) -> Result<Unsigned>) -> Result<Unsigned> {
    let Some(p) = prev else { return Err(e) };
    for o in p.tx.outputs.iter().rev().filter(|o| o.covenant.is_none()) {
        let Some(f) = p.fee.checked_add(o.value).filter(|f| *f <= MAX_FEE_SOMPI) else { continue };
        let Ok(u) = mk(f) else { continue };
        let Ok(need) = fee_of(&u.masses, rate) else { continue };
        if u.tx.outputs.len() < p.tx.outputs.len() && u.fee >= need {
            if u.fee == need {
                return Ok(u);
            }
            return Err(Error::new(
                Code::ChangeDust,
                format!("{}; without the change output the fee would be {} sompi, {} above what its mass needs at {rate} sompi/gram", e.detail, u.fee, u.fee - need),
            ));
        }
    }
    Err(e)
}

/// The withdrawal minimum of a vault state (spec M6 A4, D86): `game::w_min`.
pub fn w_min(v: &crate::state::VaultState) -> Result<i64> {
    check_vault(v)?;
    Ok(crate::game::w_min(v.total_assets, v.total_shares))
}
