//! The builders from chain objects (spec frontend 6.1 "build", 6.3; F1 part C).
//!
//! `web::build` takes each entry point's `*Case`. A page must not fill those by hand: their
//! fields are derived from the chain (the reserve from a UTXO's value, the boost pot, the lock
//! times, the trees from the purchases), and that derivation is lottery logic, so it lives
//! here, in Rust, next to the builders. Each function below takes the covenant UTXOs as the
//! chain shows them (`Obj`: outpoint, UTXO entry, and the state bytes the indexer API or the
//! local copy gives), the instance's covenant ID, the purchases (compact six-byte form, in
//! shard order) where a tree is needed, and the user's choices, and returns `web::build`'s
//! `Unsigned`.
//!
//! What is checked, for every covenant object (nothing from the API is trusted, 6.3):
//! - its UTXO carries the instance's covenant ID, is not a coinbase and is confirmed;
//! - its state bytes decode under the role's embedded template, re-encode to themselves, and
//!   their P2SH is the UTXO's script (`verify::verify_state`);
//! - after the build, the entry and outpoint the transaction spends for it are exactly the
//!   given ones (`chain_inputs`): every value the builder derived (reserves, the pot) gives
//!   back the chain's UTXO value. A mismatch is `ChainMismatch`.
//!
//! The fields each case takes from its objects mirror the native hosts that fill cases from
//! an index (`DrawCase::from_store_absorbing`, `ClaimCase::from_store_index`, and tn10's
//! campaign builders); the builders then apply `web::build`'s guards, run every covenant
//! input in the engine, and return the unsigned transaction.

use harness::{Hash, TransactionOutpoint, UtxoEntry};

use super::build::{self, Funds, OwnerCoin};
use super::sign::Unsigned;
use super::verify::{check_marker, rebuild_tree_bytes, templates, verify_state};
use super::{Code, Error, Result, WithCode, ensure};
use crate::anchor::AnchorProof;
use crate::buy::{BuyCase, M_REF, OwnerKey, Templates, b_max};
use crate::claim::{ClaimCase, ReturnCase};
use crate::close::{CloseCase, ShardIn};
use crate::combo::Combo;
use crate::draw::{DrawCase, PoolIn};
use crate::game::{BP, DAA_MAX};
use crate::index::{Role, State};
use crate::smt::Tree;
use crate::state::{
    ControllerState, DrawerState, K_MAX, MarkerState, ParamsState, Payout, PoolState, PositionState, RecordState, RequestState, ShardState,
    TicketState, VaultState,
};
use crate::vault::{DepositCase, SettleCase, WithdrawCase};

/// A covenant UTXO as the chain shows it, with its state bytes (the indexer API's or the local
/// copy's; checked here against the UTXO's script).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Obj {
    pub outpoint: TransactionOutpoint,
    pub entry: UtxoEntry,
    pub state: Vec<u8>,
}

/// The state of `o` as `role`, checked against its UTXO and the instance `cov`.
pub fn live(role: Role, o: &Obj, cov: &Hash) -> Result<State> {
    ensure(o.entry.covenant_id == Some(*cov), Code::ChainMismatch, || format!("{role:?} at {}: not a UTXO of this instance (covenant ID)", o.outpoint))?;
    ensure(!o.entry.is_coinbase, Code::ChainMismatch, || format!("{role:?} at {}: a coinbase UTXO", o.outpoint))?;
    ensure(o.entry.block_daa_score < DAA_MAX as u64, Code::CoinUnconfirmed, || format!("{role:?} at {}: not confirmed", o.outpoint))?;
    verify_state(role, &o.state, &o.entry.script_public_key)
}

macro_rules! typed_live {
    ($name:ident, $role:ident, $ty:ty) => {
        /// The live state of this role (`live`).
        pub fn $name(o: &Obj, cov: &Hash) -> Result<$ty> {
            match live(Role::$role, o, cov)? {
                State::$role(s) => Ok(s),
                _ => Err(Error::new(Code::NotTemplate, concat!("not a ", stringify!($role), " state"))),
            }
        }
    };
}

typed_live!(shard, Shard, ShardState);
typed_live!(ticket, Ticket, TicketState);
typed_live!(pool, Pool, PoolState);
typed_live!(controller, Controller, ControllerState);
typed_live!(params, Params, ParamsState);
typed_live!(marker, Marker, MarkerState);
typed_live!(drawer, Drawer, DrawerState);
typed_live!(vault, Vault, VaultState);
typed_live!(position, Position, PositionState);
typed_live!(request, Request, RequestState);
typed_live!(record, Record, RecordState);

/// After the build: input `i` spends exactly `objs[i]` (outpoint and entry), for each given
/// object. Every derived value (a reserve, the pot) must give back the chain's UTXO.
fn chain_inputs(u: Unsigned, objs: &[(&TransactionOutpoint, &UtxoEntry)]) -> Result<Unsigned> {
    for (i, (op, e)) in objs.iter().enumerate() {
        let inp = u.tx.inputs.get(i).ok_or_else(|| Error::new(Code::Encode, format!("no input {i}")))?;
        ensure(inp.previous_outpoint == **op, Code::ChainMismatch, || format!("input {i} spends {}, not {op}", inp.previous_outpoint))?;
        ensure(u.entries.get(i) == Some(*e), Code::ChainMismatch, || format!("input {i} ({op}): the built entry is not the chain's UTXO (value, script, DAA or covenant ID)"))?;
    }
    Ok(u)
}

fn of(o: &Obj) -> (&TransactionOutpoint, &UtxoEntry) {
    (&o.outpoint, &o.entry)
}

/// A value minus the parts the state accounts for: the reserve R a UTXO carries on top.
fn reserve_of(value: u64, accounted: i128, what: &str) -> Result<u64> {
    let r = value as i128 - accounted;
    ensure(r >= 1 && r <= super::verify::RESERVE_MAX as i128, Code::ChainMismatch, || format!("{what}: value {value} leaves reserve {r}"))?;
    Ok(r as u64)
}

/// The k shard trees of a round from their purchases (compact, shard order).
pub fn trees(m: i64, purchases: &[Vec<u8>]) -> Result<Vec<Tree>> {
    ensure((1..=K_MAX).contains(&purchases.len()), Code::InputRange, || format!("{} shard purchase lists (1..={K_MAX})", purchases.len()))?;
    purchases.iter().map(|p| rebuild_tree_bytes(m, p)).collect()
}

fn embedded() -> Result<std::sync::Arc<Templates>> {
    let t = templates()?;
    ensure(t.b_max == b_max(), Code::Templates, || "templates of another B_MAX".into())?;
    Ok(t)
}

// ------------------------------------------------------------------------------ buy

/// What a buyer chooses (spec S1).
#[derive(Clone, Debug)]
pub struct BuyChoice {
    /// 1 ..= B_MAX combinations.
    pub bundle: Vec<Combo>,
    /// The ticket's payout (the buyer's own address).
    pub payout: Payout,
    /// The referrer (spec 7). The referral output is paid only if r = floor(b x p x s_R /
    /// 10^4) >= M_REF; below that it is omitted and the share stays in the round.
    pub referrer: Option<Payout>,
}

/// The referral a bundle of `b` pays on `shard` (spec 7), and whether it is paid (r >= M_REF).
pub fn referral(shard: &ShardState, b: usize) -> Result<(u64, bool)> {
    let gross = (b as u64).checked_mul(shard.price as u64).ok_or_else(|| Error::new(Code::Overflow, "b x price"))?;
    let r = gross.checked_mul(shard.s_r as u64).ok_or_else(|| Error::new(Code::Overflow, "gross x s_R"))? / BP as u64;
    Ok((r, r >= M_REF))
}

/// `buy()` on the live shard `shard`, whose purchases so far are `purchases`.
pub fn buy(cov: &Hash, shard_obj: &Obj, purchases: &[u8], choice: &BuyChoice, funds: &Funds) -> Result<Unsigned> {
    let s = shard(shard_obj, cov)?;
    super::verify::check_shard(&s)?;
    let tree = rebuild_tree_bytes(s.m, purchases)?;
    ensure(tree.root() == s.root, Code::TreeMismatch, || "the purchases do not give the shard's root".into())?;
    let (_, paid) = referral(&s, choice.bundle.len())?;
    let mut c = BuyCase::new(b_max()).with_tree(tree);
    c.reserve = reserve_of(shard_obj.entry.amount, s.sales as i128 - s.ref_paid as i128, "shard")?;
    c.shard = s;
    c.shard_daa = shard_obj.entry.block_daa_score;
    c.cov = *cov;
    c.bundle = choice.bundle.clone();
    c.payout = choice.payout;
    c.with_ref = paid && choice.referrer.is_some();
    c.referrer = choice.referrer.unwrap_or(choice.payout);
    let u = build::buy(&c, shard_obj.outpoint, funds)?;
    chain_inputs(u, &[of(shard_obj)])
}

// ------------------------------------------------------------------------------ claim

/// A `ClaimStore` of chain objects: the ticket, the live pools of its round, the round's trees.
struct ChainStore {
    t: std::sync::Arc<Templates>,
    cov: Hash,
    ticket: (TransactionOutpoint, TicketState, u64, u64),
    pools: Vec<(TransactionOutpoint, PoolState, u64, u64)>,
    trees: Vec<Tree>,
}

impl crate::store::ClaimStore for ChainStore {
    fn templates(&self) -> std::result::Result<std::sync::Arc<Templates>, String> {
        Ok(self.t.clone())
    }
    fn cov(&self) -> Hash {
        self.cov
    }
    fn ticket(&self, op: &TransactionOutpoint) -> std::result::Result<(TicketState, u64, u64), String> {
        if *op == self.ticket.0 { Ok((self.ticket.1.clone(), self.ticket.2, self.ticket.3)) } else { Err(format!("ticket {op} not given")) }
    }
    fn pools(&self, round: i64) -> std::result::Result<Vec<(TransactionOutpoint, PoolState, u64, u64)>, String> {
        Ok(self.pools.iter().filter(|p| p.1.round == round).cloned().collect())
    }
    fn trees(&self, _round: i64) -> std::result::Result<Vec<Tree>, String> {
        Ok(self.trees.clone())
    }
}

/// `claim()` of the live ticket `ticket_obj` against a live pool of its round, from
/// `pools` (the round's live pools), the count proofs from the round's purchases
/// (`purchases`, one list per shard). `idx = None` claims the first unclaimed index that
/// wins a tier with a live pool; `Some(i)` claims index i.
pub fn claim(cov: &Hash, ticket_obj: &Obj, pools: &[Obj], purchases: &[Vec<u8>], idx: Option<i64>, funds: &Funds) -> Result<Unsigned> {
    funds.check()?;
    let (case, pool_obj) = claim_case(cov, ticket_obj, pools, purchases, idx, funds)?;
    let u = build::claim(&case, ticket_obj.outpoint, pool_obj.outpoint, funds)?;
    chain_inputs(u, &[of(ticket_obj), of(pool_obj)])
}

/// The claim `claim` builds, as the model (`ClaimCase`, every output from the `game`
/// reference), with the pool object it spends; no transaction is built (front end F4: the claim
/// row's figures and their binding to a built claim, `crates/web` `claimFigures`). The same
/// checks as `claim`: the ticket and every pool against their UTXOs, the purchases against
/// every live pool's frozen roots, and the reference's own refusals.
pub fn claim_case<'a>(cov: &Hash, ticket_obj: &Obj, pools: &'a [Obj], purchases: &[Vec<u8>], idx: Option<i64>, funds: &Funds) -> Result<(ClaimCase, &'a Obj)> {
    let t = embedded()?;
    let tk = ticket(ticket_obj, cov)?;
    ensure(pools.len() <= 64, Code::InputRange, || format!("{} pools", pools.len()))?;
    let mut ps = vec![];
    for o in pools {
        let p = pool(o, cov)?;
        super::verify::check_pool(&p)?;
        ps.push((o.outpoint, p, o.entry.amount, o.entry.block_daa_score));
    }
    let m = ps.iter().find(|p| p.1.round == tk.round).map(|p| p.1.m).ok_or_else(|| Error::new(Code::StoreIncomplete, format!("no live pool of round {}", tk.round)))?;
    // The purchases first (spec 6.3): every live pool of the round froze the round's roots, so
    // a missing or extra purchase is a tree mismatch before the reference looks at a count.
    let trees = trees(m, purchases)?;
    for p in ps.iter().filter(|p| p.1.round == tk.round) {
        super::verify::check_roots(&trees, &p.1.roots, p.1.k)?;
    }
    let store = ChainStore { t, cov: *cov, ticket: (ticket_obj.outpoint, tk, ticket_obj.entry.amount, ticket_obj.entry.block_daa_score), pools: ps, trees };
    let fund: Vec<crate::store::Funding> = funds.coins.iter().map(|c| crate::store::Funding { outpoint: c.outpoint, value: c.entry.amount, daa: c.entry.block_daa_score }).collect();
    // The builder replaces the funding key with its placeholder; this one signs nothing.
    let payer = crate::store::Payer { funding_sk: [0x6b; 32], fee: funds.fee };
    let (case, ops) = match idx {
        None => ClaimCase::from_store(&store, ticket_obj.outpoint, &fund, &payer),
        Some(i) => ClaimCase::from_store_index(&store, ticket_obj.outpoint, i, &fund, &payer),
    }
    .code(Code::Refused)?;
    let pool_op = *ops.get(1).ok_or_else(|| Error::new(Code::StoreIncomplete, "no pool outpoint"))?;
    let pool_obj = pools.iter().find(|o| o.outpoint == pool_op).ok_or_else(|| Error::new(Code::StoreIncomplete, "the pool is not one given"))?;
    Ok((case, pool_obj))
}

// ------------------------------------------------------------------------------ return

/// `return_deposit()` of the live ticket: path b alone (`pool = None`, lock time T + D_RET),
/// or path a with an expired pool of its round (lock time = the pool's deadline).
pub fn return_deposit(cov: &Hash, ticket_obj: &Obj, pool_obj: Option<&Obj>, funds: &Funds) -> Result<Unsigned> {
    let tk = ticket(ticket_obj, cov)?;
    super::verify::check_ticket(&tk, b_max())?;
    let (v, daa) = (ticket_obj.entry.amount, ticket_obj.entry.block_daa_score);
    let c = match pool_obj {
        None => {
            ensure(tk.deadline.checked_add(crate::game::D_RET).is_some_and(|l| l < DAA_MAX), Code::StateRange, || "T + D_RET out of range".into())?;
            ReturnCase::after_of(tk, v, daa)
        }
        Some(o) => {
            let p = pool(o, cov)?;
            ReturnCase::with_pool_of(tk, v, daa, p, o.entry.amount, o.entry.block_daa_score)
        }
    };
    let c = ReturnCase { cov: *cov, ..c };
    let u = build::return_deposit(&c, ticket_obj.outpoint, pool_obj.map(|o| o.outpoint), funds)?;
    match pool_obj {
        None => chain_inputs(u, &[of(ticket_obj)]),
        Some(o) => chain_inputs(u, &[of(ticket_obj), of(o)]),
    }
}

// ------------------------------------------------------------------------------ close

/// `close()` of the round of the live controller: `shards` are the round's k live shards in
/// shard order; the marker's reserve is funded by `funder` (the caller), the caller output
/// goes to `funds.change`. `now` is the virtual DAA score (the D24 stale-input guard).
pub fn close(cov: &Hash, controller_obj: &Obj, params_obj: &Obj, shard_objs: &[Obj], funder: Payout, funds: &Funds, now: u64) -> Result<Unsigned> {
    let t = embedded()?;
    let ctrl = controller(controller_obj, cov)?;
    super::verify::check_controller(&ctrl)?;
    let ps = params(params_obj, cov)?;
    let k = shard_objs.len();
    ensure((1..=K_MAX).contains(&k), Code::InputRange, || format!("{k} shards (1..={K_MAX})"))?;
    let mut c = CloseCase::new_with(k, &t);
    c.pot = (controller_obj.entry.amount as i128 - ctrl.acc as i128 - ctrl.reserve as i128)
        .try_into()
        .map_err(|_| Error::new(Code::ChainMismatch, "the controller value is below acc + R"))?;
    c.lock_time = ctrl.deadline.max(0) as u64;
    c.controller = ctrl;
    c.controller_daa = controller_obj.entry.block_daa_score;
    c.params = ps;
    c.params_daa = params_obj.entry.block_daa_score;
    c.shards = shard_objs.iter().map(|o| Ok(ShardIn { state: shard(o, cov)?, daa: o.entry.block_daa_score })).collect::<Result<_>>()?;
    c.funder = funder;
    c.cov = *cov;
    let mut ops = vec![controller_obj.outpoint, params_obj.outpoint];
    ops.extend(shard_objs.iter().map(|o| o.outpoint));
    let u = build::close(&c, &ops, funds, now)?;
    let mut objs = vec![of(controller_obj), of(params_obj)];
    objs.extend(shard_objs.iter().map(of));
    chain_inputs(u, &objs)
}

// ------------------------------------------------------------------------------ draw

/// The objects of a `draw()` (spec M3, M7).
pub struct DrawObjs<'a> {
    /// The drawer of round r (input 0).
    pub drawer: &'a Obj,
    /// The marker of round r (input 1).
    pub marker: &'a Obj,
    /// The vault of round r - 1 (input 2).
    pub vault: &'a Obj,
    /// The controller of round r + 1 (input 3).
    pub controller: &'a Obj,
    /// Expired pools to absorb (inputs 4..).
    pub absorbed: &'a [Obj],
}

/// `draw()` of the closed round of the live marker with the anchor proof `proof` (272
/// bytes), or its void (`proof = None`): the trees from the round's purchases (one list per
/// shard, checked against the marker's frozen roots); the new pools' reserves are funded by
/// `funder` (the caller), the caller output goes to `funds.change`.
pub fn draw(cov: &Hash, o: &DrawObjs, purchases: &[Vec<u8>], proof: Option<&[u8]>, funder: Payout, funds: &Funds) -> Result<Unsigned> {
    let t = embedded()?;
    let mk = marker(o.marker, cov)?;
    check_marker(&mk)?;
    let k = mk.k as usize;
    // F5 review S5: a void uses no leaves, siblings or counts (draw.rs `try_expected`), so it
    // needs no purchases: with none given, k empty trees fill the case's shape (the void's
    // transaction is the same whatever the trees: `build::draw` checks the roots of a draw
    // only). Purchases given for a void are still checked against the marker's roots.
    let void_without_purchases = proof.is_none() && purchases.is_empty();
    ensure((1..=K_MAX).contains(&k) && (void_without_purchases || purchases.len() == k), Code::StoreIncomplete, || format!("{} purchase lists for k = {k}", purchases.len()))?;
    let trees = if void_without_purchases { (0..k).map(|_| Tree::new(mk.m as usize)).collect() } else { trees(mk.m, purchases)? };
    if proof.is_none() && !void_without_purchases {
        super::verify::check_roots(&trees, &mk.roots, mk.k)?;
    }
    let ctrl = controller(o.controller, cov)?;
    let mut c = DrawCase::new_with(k, &t);
    c.drawer = drawer(o.drawer, cov)?;
    c.drawer_daa = o.drawer.entry.block_daa_score;
    c.vault = vault(o.vault, cov)?;
    c.vault_daa = o.vault.entry.block_daa_score;
    c.pot = (o.controller.entry.amount as i128 - ctrl.acc as i128 - ctrl.reserve as i128)
        .try_into()
        .map_err(|_| Error::new(Code::ChainMismatch, "the controller value is below acc + R"))?;
    c.controller = ctrl;
    c.controller_daa = o.controller.entry.block_daa_score;
    c.marker = mk;
    c.marker_daa = o.marker.entry.block_daa_score;
    c.trees = trees;
    c.cov = *cov;
    c.sim = None;
    c.absorbed = o
        .absorbed
        .iter()
        .map(|a| {
            let p = pool(a, cov)?;
            super::verify::check_pool(&p)?;
            let remaining = a.entry.amount.checked_sub(p.reserve as u64).ok_or_else(|| Error::new(Code::ChainMismatch, "absorbed pool: value below R"))?;
            Ok(PoolIn { state: p, remaining, daa: a.entry.block_daa_score })
        })
        .collect::<Result<_>>()?;
    match proof {
        None => c.set_void(true),
        Some(bytes) => {
            let p = AnchorProof::from_bytes(bytes).code(Code::AnchorInvalid)?;
            c.x_hash = p.x_hash();
            c.proof = Some(p);
            c.set_void(false);
        }
    }
    c.funder = funder;
    let mut ops = vec![o.drawer.outpoint, o.marker.outpoint, o.vault.outpoint, o.controller.outpoint];
    ops.extend(o.absorbed.iter().map(|a| a.outpoint));
    let u = build::draw(&c, &ops, funds)?;
    let mut objs = vec![of(o.drawer), of(o.marker), of(o.vault), of(o.controller)];
    objs.extend(o.absorbed.iter().map(of));
    chain_inputs(u, &objs)
}

// ------------------------------------------------------------------------------ vault

/// `request_deposit()` of `d` sompi into the live vault, for `owner` (lock time = the
/// vault's creation DAA + VAULT_REQ_DELAY, the earliest it accepts).
pub fn request_deposit(cov: &Hash, vault_obj: &Obj, d: i64, owner: Payout, funds: &Funds) -> Result<Unsigned> {
    let t = embedded()?;
    let v = vault(vault_obj, cov)?;
    super::verify::check_vault(&v)?;
    let mut c = DepositCase::new_with(&t);
    c.reserve = reserve_of(vault_obj.entry.amount, v.total_assets as i128 + v.pend_dep as i128, "vault")?;
    c.vault = v;
    c.at_age(vault_obj.entry.block_daa_score);
    c.d = d;
    c.owner = owner;
    c.cov = *cov;
    let u = build::request_deposit(&c, vault_obj.outpoint, funds)?;
    chain_inputs(u, &[of(vault_obj)])
}

/// `request_withdraw()` of `w` shares of the live position from the live vault. The owner
/// input is `owner` (a coin at the position owner's address and the owner's public key); its
/// wallet signs it. The lock time is the vault's creation DAA + VAULT_REQ_DELAY.
pub fn request_withdraw(cov: &Hash, vault_obj: &Obj, position_obj: &Obj, w: i64, owner: &OwnerCoin, funds: &Funds) -> Result<Unsigned> {
    let t = embedded()?;
    let v = vault(vault_obj, cov)?;
    super::verify::check_vault(&v)?;
    let ps = position(position_obj, cov)?;
    super::verify::check_position(&ps)?;
    // The placeholder owner key: `build::request_withdraw` replaces it with the owner's coin.
    let mut c = WithdrawCase::new_with(&t, OwnerKey::Schnorr([0x6c; 32]));
    c.reserve = reserve_of(vault_obj.entry.amount, v.total_assets as i128 + v.pend_dep as i128, "vault")?;
    c.vault = v;
    c.at_age(vault_obj.entry.block_daa_score);
    c.position = ps;
    c.position_daa = position_obj.entry.block_daa_score;
    c.w = w;
    c.funding = vec![];
    c.cov = *cov;
    let u = build::request_withdraw(&c, vault_obj.outpoint, position_obj.outpoint, owner, funds)?;
    chain_inputs(u, &[of(vault_obj), of(position_obj), (&owner.coin.outpoint, &owner.coin.entry)])
}

/// `settle()` of the live request against the live record of its round.
pub fn settle(cov: &Hash, request_obj: &Obj, record_obj: &Obj, funds: &Funds) -> Result<Unsigned> {
    let t = embedded()?;
    let rq = request(request_obj, cov)?;
    let rc = record(record_obj, cov)?;
    super::verify::check_record(&rc)?;
    let accounted = rc.out_rem as i128 + rc.refund as i128 * rc.dep_rem as i128;
    let record_reserve = reserve_of(record_obj.entry.amount, accounted, "record")?;
    let mut c = SettleCase::new_with(&t, rc, rq);
    c.request_value = request_obj.entry.amount;
    c.request_daa = request_obj.entry.block_daa_score;
    c.record_reserve = record_reserve;
    c.record_daa = record_obj.entry.block_daa_score;
    c.cov = *cov;
    let u = build::settle(&c, request_obj.outpoint, record_obj.outpoint, funds)?;
    chain_inputs(u, &[of(request_obj), of(record_obj)])
}
