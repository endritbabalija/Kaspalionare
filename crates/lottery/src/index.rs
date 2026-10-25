//! `lottery::index` (spec M8 A4, D115): turns accepted transactions back into round data.
//! Pure: no I/O, no RPC, no clock. The indexer (`tn10`, S3) feeds it the accepted
//! transactions of the selected chain in order, with the amount and creation DAA score of
//! every spent entry (`Accepted`).
//!
//! - `classify(tx, templates, cov)` names the transition by the covenant role of input 0
//!   (its redeem script decoded against the committed templates) and the entry its dispatch
//!   tag selects, and parses it (`parse_buy`, `parse_close`, `parse_draw`, ...). A
//!   transaction with an output bound to the instance covenant ID that cannot be parsed is
//!   `Event::Unknown`, which `Rebuild::apply` reports as an error: it never skips one.
//! - `Rebuild::apply` keeps, per round, every shard's counter tree (`smt::Tree`) in the
//!   order its shard UTXO chain was spent, the rounds (close, frozen roots, draw, round
//!   result) and the live UTXO set with each UTXO's state. Every successor state is rebuilt
//!   from the spent states (decoded from the redeem scripts in the transaction) and the
//!   entry arguments with the builders' own models (`BuyCase` arithmetic, `CloseCase`,
//!   `DrawCase`, `DepositCase`, `WithdrawCase`, `SettleCase`, `game::claim_*`), then checked
//!   by hashing it to the output's script (state -> redeem -> P2SH) and its value; a
//!   mismatch is an error and nothing is applied.
//! - At `Close`, every rebuilt root is compared with the frozen root (the shard input's
//!   root, which the marker carries). If one differs, or a shard UTXO of the round was
//!   spent without the index having seen it created, the round is marked incomplete and
//!   `Rebuild::trees` refuses to hand out its trees, so no proof is built from it.
//! - `Rebuild::revert(txid)` undoes the most recently applied transaction (a reorg removes
//!   chain blocks newest first) by replaying the remaining log.
//!
//! Absorbing an expired pool has no transaction of its own: `pool.sil` has only
//! `absorb_leg` (inputs 4.. of `draw()`), `ticket_leg` and `refresh`. The A4 event list's
//! "Absorb" is therefore `Event::Draw` with a non-empty `absorbed` list.
//!
//! Outputs are P2SH only, so a UTXO's state is known when this index built it (every
//! non-genesis covenant output) or when a later transaction spends it (its redeem). The
//! genesis outputs' states are not in the genesis transaction; `Rebuild::new` takes them
//! optionally (from the launch bundle) and checks them against the genesis outputs.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use harness::{ArtifactValue, Hash, ScriptPublicKey, Transaction, TransactionId, TransactionOutpoint, UtxoEntry, pay_to_script_hash_script};

use crate::buy::Templates;
use crate::claim::RefreshObj;
use crate::close::{CloseCase, FundingIn, ShardIn, z18};
use crate::combo::Combo;
use crate::decode::{arg_bool, arg_byte, arg_bytes, arg_int, byte, bytes, bytes_n, entry_args, int, pushes, state_values};
use crate::draw::{ARG_ANCHOR, ARG_LEAVES, ARG_MK_DAA, ARG_WC, ARG_X1, ARG_X2, ARG_Y, DrawCase, PoolIn, RoundResult};
use crate::game::{ClaimMode, claim_amount, claim_mode, win_counts};
use crate::smt::{PATH_BYTES, Tree, leaf_hash, root_from_path};
use crate::state::{
    ControllerState, DrawerState, K_MAX, MarkerState, ParamsState, Payout, PoolState, PositionState, RecordState, RequestState, ShardState,
    TicketState, VaultState,
};
use crate::vault::{DepositCase, SettleCase, WithdrawCase};

// ------------------------------------------------------------------------------ states

/// The covenant role of a UTXO (the template its redeem script is an instance of).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Controller,
    Params,
    Shard,
    Marker,
    Drawer,
    Vault,
    Pool,
    Ticket,
    Position,
    Request,
    Record,
}

/// A decoded covenant state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Controller(ControllerState),
    Params(ParamsState),
    Shard(ShardState),
    Marker(MarkerState),
    Drawer(DrawerState),
    Vault(VaultState),
    Pool(PoolState),
    Ticket(TicketState),
    Position(PositionState),
    Request(RequestState),
    Record(RecordState),
}

impl State {
    pub fn role(&self) -> Role {
        match self {
            State::Controller(_) => Role::Controller,
            State::Params(_) => Role::Params,
            State::Shard(_) => Role::Shard,
            State::Marker(_) => Role::Marker,
            State::Drawer(_) => Role::Drawer,
            State::Vault(_) => Role::Vault,
            State::Pool(_) => Role::Pool,
            State::Ticket(_) => Role::Ticket,
            State::Position(_) => Role::Position,
            State::Request(_) => Role::Request,
            State::Record(_) => Role::Record,
        }
    }

    /// The state's `round` field.
    pub fn round(&self) -> i64 {
        match self {
            State::Controller(s) => s.round,
            State::Params(s) => s.round,
            State::Shard(s) => s.round,
            State::Marker(s) => s.round,
            State::Drawer(s) => s.round,
            State::Vault(s) => s.round,
            State::Pool(s) => s.round,
            State::Ticket(s) => s.round,
            State::Position(s) => s.round,
            State::Request(s) => s.round,
            State::Record(s) => s.round,
        }
    }

    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        match self {
            State::Controller(s) => s.values(),
            State::Params(s) => s.values(),
            State::Shard(s) => s.values(),
            State::Marker(s) => s.values(),
            State::Drawer(s) => s.values(),
            State::Vault(s) => s.values(),
            State::Pool(s) => s.values(),
            State::Ticket(s) => s.values(),
            State::Position(s) => s.values(),
            State::Request(s) => s.values(),
            State::Record(s) => s.values(),
        }
    }

    /// The redeem script: the role's template with this state.
    pub fn redeem(&self, t: &Templates) -> Vec<u8> {
        template_of(t, self.role()).redeem(&self.values())
    }

    /// `redeem` with the encoder's error instead of a panic (front-end F1 part B).
    pub fn try_redeem(&self, t: &Templates) -> Result<Vec<u8>, String> {
        template_of(t, self.role()).try_redeem(&self.values())
    }

    /// The state of `role` with these field values (the inverse of `values`; the `role`
    /// byte is not read). Front-end F1 part B: the W3 property test mutates one field of any
    /// state through it.
    pub fn from_values(role: Role, v: &BTreeMap<String, ArtifactValue>) -> Result<State, String> {
        typed(role, v)
    }

    /// `state_bytes` with the encoder's error instead of a panic (front-end F1 part B).
    pub fn try_state_bytes(&self, t: &Templates) -> Result<Vec<u8>, String> {
        template_of(t, self.role()).try_state_bytes(&self.values())
    }

    /// The encoded state alone, without the template's prefix and suffix (M8 `bundle/1`).
    pub fn state_bytes(&self, t: &Templates) -> Vec<u8> {
        template_of(t, self.role()).state_bytes(&self.values())
    }

    /// The state of `role` whose encoded state bytes are `bytes` (M8 `bundle/1`): the redeem
    /// script prefix || bytes || suffix of the role's committed template is decoded and must
    /// re-encode to the same bytes. Returns the state and its redeem script. A full redeem
    /// script given as `bytes` (the pre-fix bundle shape) is refused by name.
    pub fn from_state_bytes(t: &Templates, role: Role, bytes: &[u8]) -> Result<(State, Vec<u8>), String> {
        let tpl = template_of(t, role);
        if bytes.len() >= tpl.prefix.len() + tpl.suffix.len() && bytes.starts_with(&tpl.prefix) && bytes.ends_with(&tpl.suffix) {
            return Err(format!("{role:?} state holds a full redeem script, not the state bytes (old bundle shape)"));
        }
        let redeem = [tpl.prefix.as_slice(), bytes, &tpl.suffix].concat();
        let v = state_values(tpl, &redeem)?;
        let s = typed(role, &v)?;
        // F1 part B: a value the decoder accepts but the encoder refuses is an error.
        if s.try_redeem(t)? != redeem {
            return Err(format!("{role:?} state bytes do not re-encode to themselves"));
        }
        Ok((s, redeem))
    }

    /// P2SH of the redeem script (what a covenant output carries).
    pub fn spk(&self, t: &Templates) -> ScriptPublicKey {
        pay_to_script_hash_script(&self.redeem(t))
    }

    /// The state of a redeem script, by the committed template whose prefix and suffix it
    /// carries. The decoding is checked by re-encoding: the state must give the same
    /// redeem script back, byte for byte.
    pub fn decode(t: &Templates, redeem: &[u8]) -> Result<State, String> {
        for role in ROLES {
            let tpl = template_of(t, role);
            let Ok(v) = state_values(tpl, redeem) else { continue };
            let s = typed(role, &v)?;
            if s.try_redeem(t)? != redeem {
                return Err(format!("{role:?} state does not re-encode to its redeem script"));
            }
            return Ok(s);
        }
        Err("redeem script is not an instance of a committed template".into())
    }
}

pub const ROLES: [Role; 11] =
    [Role::Controller, Role::Params, Role::Shard, Role::Marker, Role::Drawer, Role::Vault, Role::Pool, Role::Ticket, Role::Position, Role::Request, Role::Record];

pub fn template_of(t: &Templates, role: Role) -> &crate::buy::Template {
    match role {
        Role::Controller => &t.controller,
        Role::Params => &t.params,
        Role::Shard => &t.shard,
        Role::Marker => &t.marker,
        Role::Drawer => &t.drawer,
        Role::Vault => &t.vault,
        Role::Pool => &t.pool,
        Role::Ticket => &t.ticket,
        Role::Position => &t.position,
        Role::Request => &t.request,
        Role::Record => &t.record,
    }
}

fn st(v: &BTreeMap<String, ArtifactValue>) -> Result<[i64; 5], String> {
    Ok([int(v, "st1")?, int(v, "st2")?, int(v, "st3")?, int(v, "st4")?, int(v, "st5")?])
}

/// The typed state of decoded fields (the inverse of each state's `values()`).
pub(crate) fn typed(role: Role, v: &BTreeMap<String, ArtifactValue>) -> Result<State, String> {
    Ok(match role {
        Role::Controller => State::Controller(ControllerState {
            round: int(v, "round")?,
            k: int(v, "k")?,
            m: int(v, "m")?,
            price: int(v, "price")?,
            d: int(v, "d")?,
            deadline: int(v, "deadline")?,
            s_f: int(v, "s_f")?,
            s_r: int(v, "s_r")?,
            fee_class: byte(v, "fee_class")?,
            fee_key: bytes_n(v, "fee_key")?,
            jackpot: int(v, "jackpot")?,
            j_boost: int(v, "j_boost")?,
            g2: int(v, "g2")?,
            liability: int(v, "liability")?,
            v_rec: int(v, "v_rec")?,
            acc: int(v, "acc")?,
            drawn: byte(v, "drawn")?,
            shard_tpl: bytes_n(v, "shard_tpl")?,
            marker_tpl: bytes_n(v, "marker_tpl")?,
            params_tpl: bytes_n(v, "params_tpl")?,
            reserve: int(v, "reserve")?,
            s_c: int(v, "s_c")?,
            s_d: int(v, "s_d")?,
            st: st(v)?,
            e: int(v, "e")?,
            j: int(v, "j")?,
            g: int(v, "g")?,
            margin: int(v, "margin")?,
            m_min: int(v, "m_min")?,
            m_max: int(v, "m_max")?,
            cap: int(v, "cap")?,
            round_len: int(v, "round_len")?,
            d_min: int(v, "d_min")?,
            seed_offset: int(v, "seed_offset")?,
        }),
        Role::Params => State::Params(ParamsState {
            round: int(v, "round")?,
            admin_hash: bytes_n(v, "admin_hash")?,
            nonce: int(v, "nonce")?,
            used: byte(v, "used")?,
            next_s_f: int(v, "next_s_f")?,
            next_s_r: int(v, "next_s_r")?,
            next_fee_class: byte(v, "next_fee_class")?,
            next_fee_key: bytes_n(v, "next_fee_key")?,
        }),
        Role::Shard => State::Shard(ShardState {
            round: int(v, "round")?,
            shard_index: int(v, "shard_index")?,
            m: int(v, "m")?,
            price: int(v, "price")?,
            d: int(v, "d")?,
            s_r: int(v, "s_r")?,
            deadline: int(v, "deadline")?,
            root: bytes_n(v, "root")?,
            sales: int(v, "sales")?,
            ref_paid: int(v, "ref_paid")?,
        }),
        Role::Marker => State::Marker(MarkerState {
            round: int(v, "round")?,
            k: int(v, "k")?,
            roots: bytes(v, "roots")?,
            sales: int(v, "sales")?,
            ref_paid: int(v, "ref_paid")?,
            m: int(v, "m")?,
            jackpot: int(v, "jackpot")?,
            j_boost: int(v, "j_boost")?,
            g2: int(v, "g2")?,
            s_f: int(v, "s_f")?,
            s_r: int(v, "s_r")?,
            s_c: int(v, "s_c")?,
            s_d: int(v, "s_d")?,
            st: st(v)?,
            funder_class: byte(v, "funder_class")?,
            funder_key: bytes_n(v, "funder_key")?,
        }),
        Role::Drawer => State::Drawer(DrawerState {
            round: int(v, "round")?,
            pool_tpl: bytes_n(v, "pool_tpl")?,
            vault_tpl: bytes_n(v, "vault_tpl")?,
            ticket_tpl: bytes_n(v, "ticket_tpl")?,
            f_min: int(v, "f_min")?,
            claim_window: int(v, "claim_window")?,
            draw_window: int(v, "draw_window")?,
        }),
        Role::Vault => State::Vault(VaultState {
            round: int(v, "round")?,
            total_assets: int(v, "total_assets")?,
            total_shares: int(v, "total_shares")?,
            pend_dep: int(v, "pend_dep")?,
            pend_wsh: int(v, "pend_wsh")?,
            liability: int(v, "liability")?,
            record_tpl: bytes_n(v, "record_tpl")?,
            request_tpl: bytes_n(v, "request_tpl")?,
            position_tpl: bytes_n(v, "position_tpl")?,
        }),
        Role::Pool => State::Pool(PoolState {
            round: int(v, "round")?,
            tier: byte(v, "tier")?,
            slice: int(v, "slice")?,
            win: bytes_n(v, "win")?,
            m: int(v, "m")?,
            k: int(v, "k")?,
            roots: bytes(v, "roots")?,
            deadline: int(v, "deadline")?,
            ticket_tpl: bytes_n(v, "ticket_tpl")?,
            reserve: int(v, "reserve")?,
            funder_class: byte(v, "funder_class")?,
            funder_key: bytes_n(v, "funder_key")?,
        }),
        Role::Ticket => State::Ticket(TicketState {
            round: int(v, "round")?,
            deadline: int(v, "deadline")?,
            b: int(v, "b")?,
            bundle: bytes(v, "bundle")?,
            claimed: bytes(v, "claimed")?,
            payout_class: byte(v, "payout_class")?,
            payout_key: bytes_n(v, "payout_key")?,
        }),
        Role::Position => State::Position(PositionState {
            round: int(v, "round")?,
            shares: int(v, "shares")?,
            owner_class: byte(v, "owner_class")?,
            owner_key: bytes_n(v, "owner_key")?,
        }),
        Role::Request => State::Request(RequestState {
            round: int(v, "round")?,
            kind: byte(v, "kind")?,
            amount: int(v, "amount")?,
            owner_class: byte(v, "owner_class")?,
            owner_key: bytes_n(v, "owner_key")?,
        }),
        Role::Record => State::Record(RecordState {
            round: int(v, "round")?,
            request_tpl: bytes_n(v, "request_tpl")?,
            refund: byte(v, "refund")?,
            dep_rem: int(v, "dep_rem")?,
            mint_rem: int(v, "mint_rem")?,
            wsh_rem: int(v, "wsh_rem")?,
            burn_rem: int(v, "burn_rem")?,
            out_rem: int(v, "out_rem")?,
            funder_class: byte(v, "funder_class")?,
            funder_key: bytes_n(v, "funder_key")?,
        }),
    })
}

/// A D11 payout of decoded (class, key) fields; class > 2 is an error, not a panic.
fn payout(class: u8, key: &[u8]) -> Result<Payout, String> {
    let k: [u8; 33] = key.try_into().map_err(|_| format!("payout key of {} bytes", key.len()))?;
    if class > 2 {
        return Err(format!("payout class {class} is not a D11 standard script"));
    }
    Ok(Payout::from_fields(class, &k))
}

// ------------------------------------------------------------------------------ events

/// A decoded covenant input: its index, the outpoint it spends, the entry it calls and
/// its state (from the redeem script).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CovIn {
    pub index: usize,
    pub outpoint: TransactionOutpoint,
    pub entry: String,
    pub state: State,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenesisEvent {
    /// K, from the genesis layout (D50, M5 A9): k + 5 protocol outputs.
    pub k: usize,
}

/// `buy()` (spec M1): the shard input, the arguments, and the successor and ticket rebuilt
/// from them alone (the successor root by folding each slot's new leaf up its siblings, as
/// the shard script does).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuyEvent {
    pub shard: ShardState,
    pub b: i64,
    pub bundle: Vec<Combo>,
    pub leaves: Vec<u8>,
    pub siblings: Vec<u8>,
    pub with_ref: bool,
    /// r = floor(b p s_R / 10^4) if a referral is paid, else 0 (`BuyCase::referral`).
    pub referral: u64,
    pub payout: Payout,
    pub successor: ShardState,
    pub ticket: TicketState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetParamsEvent {
    pub params: ParamsState,
    pub successor: ParamsState,
}

/// `close()` (spec M2 A4): the round r objects, the frozen roots (the shard inputs' roots,
/// shard order) and the arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloseEvent {
    pub controller: ControllerState,
    pub params: ParamsState,
    pub shards: Vec<ShardState>,
    pub frozen_roots: Vec<[u8; 32]>,
    pub m_next: i64,
    pub funder: Payout,
}

/// `draw()` (spec M3 A4, M7): the round result rebuilt from the transaction bytes
/// (`RoundResult::from_tx`), the spent states, the absorbed pools (inputs 4..4 + a) and the
/// arguments that are not in the round result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawEvent {
    pub drawer: DrawerState,
    pub marker: MarkerState,
    pub vault: VaultState,
    pub controller: ControllerState,
    pub absorbed: Vec<PoolState>,
    pub result: RoundResult,
    pub anchor: Vec<u8>,
    pub leaves: Vec<u8>,
    pub witness: (i64, i64, i64),
    pub wc: i64,
    pub mk_daa: i64,
}

impl DrawEvent {
    pub fn void(&self) -> bool {
        self.result.void
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimEvent {
    pub ticket: TicketState,
    pub pool: PoolState,
    pub idx: i64,
    pub leaves: Vec<u8>,
    pub siblings: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReturnEvent {
    pub ticket: TicketState,
    /// `return_deposit_with_pool`: the co-spent pool (recreated unchanged).
    pub pool: Option<PoolState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepositEvent {
    pub vault: VaultState,
    pub owner: Payout,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithdrawEvent {
    pub vault: VaultState,
    pub position: PositionState,
    pub w: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettleEvent {
    pub request: RequestState,
    pub record: RecordState,
}

/// One transition of the instance (spec M8 A4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Genesis(GenesisEvent),
    Buy(BuyEvent),
    SetParams(SetParamsEvent),
    Close(CloseEvent),
    /// A draw or a void (`DrawEvent::void`); with absorbed pools it is also the A4 "Absorb".
    Draw(Box<DrawEvent>),
    Claim(ClaimEvent),
    RequestDeposit(DepositEvent),
    RequestWithdraw(WithdrawEvent),
    Settle(SettleEvent),
    /// `refresh()` of a pool, ticket, position, request or record: the object as spent.
    Refresh(State),
    ReturnDeposit(ReturnEvent),
    /// A covenant-bound transaction that cannot be parsed (an error, never skipped).
    Unknown(String),
}

impl Event {
    /// A short name ("buy", "close", "draw", "void", ...).
    pub fn name(&self) -> &'static str {
        match self {
            Event::Genesis(_) => "genesis",
            Event::Buy(_) => "buy",
            Event::SetParams(_) => "set_params",
            Event::Close(_) => "close",
            Event::Draw(d) if d.void() => "void",
            Event::Draw(_) => "draw",
            Event::Claim(_) => "claim",
            Event::RequestDeposit(_) => "request_deposit",
            Event::RequestWithdraw(_) => "request_withdraw",
            Event::Settle(_) => "settle",
            Event::Refresh(_) => "refresh",
            Event::ReturnDeposit(r) if r.pool.is_some() => "return_deposit_with_pool",
            Event::ReturnDeposit(_) => "return_deposit_after",
            Event::Unknown(_) => "unknown",
        }
    }
}

/// Decode input `i` as a covenant input: (entry, arguments, state). `Err` if its redeem is
/// not an instance of a committed template or its arguments do not decode.
fn cov_input(tx: &Transaction, t: &Templates, i: usize) -> Result<(CovIn, Vec<ArtifactValue>), String> {
    let input = tx.inputs.get(i).ok_or_else(|| format!("no input {i}"))?;
    let p = pushes(&input.signature_script)?;
    let redeem = p.last().ok_or_else(|| format!("input {i}: empty signature script"))?;
    let state = State::decode(t, redeem).map_err(|e| format!("input {i}: {e}"))?;
    let (entry, args, _) = entry_args(template_of(t, state.role()), &input.signature_script).map_err(|e| format!("input {i}: {e}"))?;
    Ok((CovIn { index: i, outpoint: input.previous_outpoint, entry, state }, args))
}

/// Input `i` decoded and required to be `role` calling `entry`.
fn leg(tx: &Transaction, t: &Templates, i: usize, role: Role, entry: &str) -> Result<State, String> {
    let (c, _) = cov_input(tx, t, i)?;
    if c.state.role() != role || c.entry != entry {
        return Err(format!("input {i}: {:?}.{}, expected {role:?}.{entry}", c.state.role(), c.entry));
    }
    Ok(c.state)
}

macro_rules! as_state {
    ($s:expr, $v:ident) => {
        match $s {
            State::$v(x) => x,
            other => return Err(format!("expected a {} state, got {:?}", stringify!($v), other.role())),
        }
    };
}

/// The per-shard leaf counts of a K-shard count-proof string (2m bytes per shard,
/// little-endian u16 counts), as `RoundResult::from_tx` reads them.
pub fn leaf_counts(leaves: &[u8], m: i64) -> Result<Vec<Vec<u16>>, String> {
    // S2c should-fix 3: m is node-chosen (a decoded state); range first, then the width.
    if !(1..=255).contains(&m) {
        return Err(format!("leaves for m = {m}"));
    }
    let w = 2 * m as usize;
    if leaves.len() % w != 0 {
        return Err(format!("leaves of {} bytes for m = {m}", leaves.len()));
    }
    Ok(leaves.chunks(w).map(|c| c.chunks(2).map(|x| u16::from_le_bytes([x[0], x[1]])).collect()).collect())
}

/// `buy()` from its transaction: the shard input (input 0, entry `buy`), its arguments
/// (b, bundle, leaves, siblings, with_ref, payout class and key, ticket prefix and suffix),
/// the successor shard and the ticket rebuilt from them.
pub fn parse_buy(tx: &Transaction, t: &Templates) -> Result<BuyEvent, String> {
    let (c, args) = cov_input(tx, t, 0)?;
    if c.entry != "buy" {
        return Err(format!("input 0 calls {}, not buy", c.entry));
    }
    let shard = as_state!(c.state, Shard);
    let b = arg_int(&args, 0)?;
    let bundle_b = arg_bytes(&args, 1)?;
    let leaves = arg_bytes(&args, 2)?.to_vec();
    let siblings = arg_bytes(&args, 3)?.to_vec();
    let with_ref = arg_bool(&args, 4)?;
    let pay = payout(arg_byte(&args, 5)?, arg_bytes(&args, 6)?)?;
    if !(1..=t.b_max as i64).contains(&b) || bundle_b.len() != 6 * b as usize || !(2..=255).contains(&shard.m) {
        return Err(format!("buy: b = {b}, bundle of {} bytes, m = {}", bundle_b.len(), shard.m));
    }
    let bundle: Vec<Combo> = bundle_b.chunks(6).map(|c| Combo::new(c[..5].try_into().unwrap(), c[5])).collect();
    let m = shard.m as usize;
    if leaves.len() != b as usize * 2 * m || siblings.len() != b as usize * PATH_BYTES {
        return Err(format!("buy: {} leaf bytes and {} sibling bytes for b = {b}, m = {m}", leaves.len(), siblings.len()));
    }
    // The successor root: each slot's leaf with its bonus incremented, folded up that slot's
    // siblings (the proofs are sequential, so the last fold is the new root); the first
    // slot's old leaf must fold to the spent shard's root.
    let mut root = shard.root;
    for (s, c) in bundle.iter().enumerate() {
        if !c.is_canonical(shard.m.min(255) as u8) {
            return Err(format!("buy: slot {s} is not canonical"));
        }
        let rank = c.rank();
        let mut counts = leaf_counts(&leaves[s * 2 * m..(s + 1) * 2 * m], shard.m)?.remove(0);
        let sib = &siblings[s * PATH_BYTES..(s + 1) * PATH_BYTES];
        if root_from_path(rank, leaf_hash(rank, &counts), sib) != root {
            return Err(format!("buy: slot {s}'s proof does not fold to the running root"));
        }
        let slot = &mut counts[c.bonus as usize - 1];
        *slot = slot.checked_add(1).ok_or("buy: count overflow")?;
        root = root_from_path(rank, leaf_hash(rank, &counts), sib);
    }
    // S2c should-fix 3 (D115): the spent state is node-chosen; checked arithmetic, an error
    // (never a panic or a wrapped value) outside the ranges the shard script allows.
    let (gross, referral) = buy_money(&shard, b, with_ref)?;
    let mut successor = shard.clone();
    successor.root = root;
    successor.sales = shard.sales.checked_add(gross as i64).ok_or("buy: sales overflow")?;
    successor.ref_paid = shard.ref_paid.checked_add(referral as i64).ok_or("buy: ref_paid overflow")?;
    let ticket = TicketState::new(&shard, &bundle, &pay, t.b_max);
    Ok(BuyEvent { shard, b, bundle, leaves, siblings, with_ref, referral, payout: pay, successor, ticket })
}

/// (P, r) of a buy: P = b x price and r = floor(P s_R / 10^4) with a referral, else 0
/// (`BuyCase::referral`), in checked u64 arithmetic; both fit an i64 (state fields).
fn buy_money(shard: &ShardState, b: i64, with_ref: bool) -> Result<(u64, u64), String> {
    let price = u64::try_from(shard.price).map_err(|_| format!("buy: price {}", shard.price))?;
    let s_r = u64::try_from(shard.s_r).map_err(|_| format!("buy: s_R {}", shard.s_r))?;
    let b = u64::try_from(b).map_err(|_| format!("buy: b {b}"))?;
    let gross = b.checked_mul(price).filter(|&g| g <= i64::MAX as u64).ok_or("buy: b x price overflows")?;
    let referral = if with_ref { gross.checked_mul(s_r).ok_or("buy: P x s_R overflows")? / 10_000 } else { 0 };
    Ok((gross, referral))
}

/// `set_params()`: the params input and the successor from the arguments (s_F, s_R, fee
/// class, fee key, nonce; `SetParamsCase::successor`).
pub fn parse_set_params(tx: &Transaction, t: &Templates) -> Result<SetParamsEvent, String> {
    let (c, args) = cov_input(tx, t, 0)?;
    let params = as_state!(c.state, Params);
    let fee = payout(arg_byte(&args, 2)?, arg_bytes(&args, 3)?)?;
    let successor = ParamsState {
        nonce: arg_int(&args, 4)?,
        used: 1,
        next_s_f: arg_int(&args, 0)?,
        next_s_r: arg_int(&args, 1)?,
        next_fee_class: fee.class(),
        next_fee_key: fee.key33(),
        ..params.clone()
    };
    Ok(SetParamsEvent { params, successor })
}

/// `close()`: the controller (input 0, entry `close`), the params (input 1) and the k
/// shards (inputs 2..k + 2, `close_leg`), the frozen roots (the shard inputs' roots, which
/// `close()` copies into the marker) and the arguments m' and the marker funder.
pub fn parse_close(tx: &Transaction, t: &Templates) -> Result<CloseEvent, String> {
    let (c, args) = cov_input(tx, t, 0)?;
    if c.entry != "close" {
        return Err(format!("input 0 calls {}, not close", c.entry));
    }
    let controller = as_state!(c.state, Controller);
    let k = controller.k;
    if !(1..=K_MAX as i64).contains(&k) {
        return Err(format!("close: k = {k}"));
    }
    let params = as_state!(leg(tx, t, 1, Role::Params, "close_leg")?, Params);
    let mut shards = vec![];
    for i in 0..k as usize {
        let s = as_state!(leg(tx, t, 2 + i, Role::Shard, "close_leg")?, Shard);
        if s.shard_index != i as i64 || s.round != controller.round {
            return Err(format!("close: input {} is shard {} of round {}", 2 + i, s.shard_index, s.round));
        }
        shards.push(s);
    }
    let frozen_roots = shards.iter().map(|s| s.root).collect();
    let funder = payout(arg_byte(&args, 1)?, arg_bytes(&args, 2)?)?;
    Ok(CloseEvent { controller, params, shards, frozen_roots, m_next: arg_int(&args, 0)?, funder })
}

/// `draw()` or a void: `RoundResult::from_tx` plus the spent drawer, marker, vault and
/// controller (inputs 0-3), the absorbed pools (inputs 4.. calling `absorb_leg`) and the
/// arguments the round result does not keep.
pub fn parse_draw(tx: &Transaction, t: &Templates) -> Result<DrawEvent, String> {
    let (c, args) = cov_input(tx, t, 0)?;
    if c.entry != "draw" {
        return Err(format!("input 0 calls {}, not draw", c.entry));
    }
    let drawer = as_state!(c.state, Drawer);
    let marker = as_state!(leg(tx, t, 1, Role::Marker, "draw_leg")?, Marker);
    let vault = as_state!(leg(tx, t, 2, Role::Vault, "draw_leg")?, Vault);
    let controller = as_state!(leg(tx, t, 3, Role::Controller, "draw_leg")?, Controller);
    let mut absorbed = vec![];
    for i in 4..tx.inputs.len() {
        match cov_input(tx, t, i) {
            Ok((ci, _)) if ci.state.role() == Role::Pool && ci.entry == "absorb_leg" => absorbed.push(as_state!(ci.state, Pool)),
            Ok((ci, _)) => return Err(format!("input {i}: {:?}.{} in a draw", ci.state.role(), ci.entry)),
            Err(_) => break,
        }
    }
    // S2c should-fix 3: `from_tx` computes the pools from the decoded marker; a panic on a
    // node-chosen state is an error.
    let result = model("draw result", || RoundResult::from_tx(tx, t))??;
    Ok(DrawEvent {
        drawer,
        marker,
        vault,
        controller,
        absorbed,
        result,
        anchor: arg_bytes(&args, ARG_ANCHOR)?.to_vec(),
        leaves: arg_bytes(&args, ARG_LEAVES)?.to_vec(),
        witness: (arg_int(&args, ARG_X1)?, arg_int(&args, ARG_Y)?, arg_int(&args, ARG_X2)?),
        wc: arg_int(&args, ARG_WC)?,
        mk_daa: arg_int(&args, ARG_MK_DAA)?,
    })
}

/// The transition `tx` makes on the instance `cov`, or `None` if it does not touch it.
///
/// Input 0's redeem script names the role (the committed template it is an instance of)
/// and its dispatch tag the entry. A transaction whose input 0 is not an instance of a
/// committed template is the instance's genesis if its covenant-bound outputs are exactly
/// outputs 0..k + 5 (K + 5 protocol outputs, D50) and the KIP-20 covenant ID of input 0's
/// outpoint and those outputs is `cov` (rusty-kaspa v2.1.0
/// `consensus/core/src/hashing/covenant_id.rs:16-31`); otherwise, if it has an output bound
/// to `cov`, it is `Unknown` (for example a look-alike template), else `None`. A
/// transaction whose bound outputs all carry another covenant ID is not this instance's
/// (`None`); one with outputs bound to both is `Unknown`.
/// `classify` cannot tell from the bytes alone which instance a spent template instance
/// belongs to when no output is bound (a FINAL claim, `return_deposit_after`, a drained
/// settlement): `Rebuild::apply` checks the spent outpoints against its live set.
pub fn classify(tx: &Transaction, t: &Templates, cov: Hash) -> Option<Event> {
    let bound: Vec<usize> = tx.outputs.iter().enumerate().filter(|(_, o)| o.covenant.is_some_and(|b| b.covenant_id == cov)).map(|(i, _)| i).collect();
    if tx.outputs.iter().any(|o| o.covenant.is_some_and(|b| b.covenant_id != cov)) {
        // Another instance's transition binds its outputs to its own ID; next to an
        // output bound to this instance it is not a transition of either: report it.
        if bound.is_empty() {
            return None;
        }
        return Some(Event::Unknown("outputs bound to another covenant ID next to the instance's".into()));
    }
    let first = tx.inputs.first()?;
    let redeem = pushes(&first.signature_script).ok().and_then(|p| p.last().cloned());
    let state = redeem.as_ref().and_then(|r| State::decode(t, r).ok());
    let Some(state) = state else {
        if bound.is_empty() {
            return None;
        }
        return Some(match genesis_k(tx, t, cov, &bound) {
            Ok(k) => Event::Genesis(GenesisEvent { k }),
            Err(e) => Event::Unknown(e),
        });
    };
    let entry = match entry_args(template_of(t, state.role()), &first.signature_script) {
        Ok((e, _, _)) => e,
        Err(e) => return Some(Event::Unknown(format!("input 0 ({:?}): {e}", state.role()))),
    };
    let r = match (state.role(), entry.as_str()) {
        (Role::Shard, "buy") => parse_buy(tx, t).map(Event::Buy),
        (Role::Params, "set_params") => parse_set_params(tx, t).map(Event::SetParams),
        (Role::Controller, "close") => parse_close(tx, t).map(Event::Close),
        (Role::Drawer, "draw") => parse_draw(tx, t).map(|d| Event::Draw(Box::new(d))),
        (Role::Ticket, "claim") => parse_claim(tx, t).map(Event::Claim),
        (Role::Ticket, "return_deposit_after") => Ok(Event::ReturnDeposit(ReturnEvent { ticket: as_ticket(state), pool: None })),
        (Role::Ticket, "return_deposit_with_pool") => {
            leg(tx, t, 1, Role::Pool, "ticket_leg").map(|p| Event::ReturnDeposit(ReturnEvent { ticket: as_ticket(state), pool: Some(as_pool(p)) }))
        }
        (Role::Pool | Role::Ticket | Role::Position | Role::Request | Role::Record, "refresh") => Ok(Event::Refresh(state)),
        (Role::Vault, "request_deposit") => parse_deposit(tx, t).map(Event::RequestDeposit),
        (Role::Vault, "request_withdraw") => parse_withdraw(tx, t).map(Event::RequestWithdraw),
        (Role::Request, "settle") => parse_settle(tx, t).map(Event::Settle),
        (role, e) => Err(format!("input 0 is {role:?}.{e}: not a transition's leader")),
    };
    Some(r.unwrap_or_else(Event::Unknown))
}

fn as_ticket(s: State) -> TicketState {
    match s {
        State::Ticket(x) => x,
        _ => unreachable!("role checked"),
    }
}

fn as_pool(s: State) -> PoolState {
    match s {
        State::Pool(x) => x,
        _ => unreachable!("role checked"),
    }
}

/// K of a genesis transaction, after checking its layout and covenant ID.
fn genesis_k(tx: &Transaction, t: &Templates, cov: Hash, bound: &[usize]) -> Result<usize, String> {
    let n = bound.len();
    if bound.iter().enumerate().any(|(i, &o)| i != o) || n < 6 || n - 5 > K_MAX {
        return Err(format!("covenant-bound outputs {bound:?}: not a genesis layout (outputs 0..k + 5)"));
    }
    if tx.outputs[..n].iter().any(|o| o.covenant.is_some_and(|b| b.authorizing_input != 0)) {
        return Err("genesis outputs not authorized by input 0".into());
    }
    if tx.inputs.iter().any(|i| pushes(&i.signature_script).ok().and_then(|p| p.last().cloned()).is_some_and(|r| State::decode(t, &r).is_ok())) {
        return Err("a genesis spends no template instance".into());
    }
    let id = kaspa_consensus_core::hashing::covenant_id::covenant_id(tx.inputs[0].previous_outpoint, tx.outputs[..n].iter().enumerate().map(|(i, o)| (i as u32, o)));
    if id != cov {
        return Err(format!("covenant ID of input 0 and outputs 0..{n} is {id}, not the instance's {cov}"));
    }
    Ok(n - 5)
}

pub fn parse_claim(tx: &Transaction, t: &Templates) -> Result<ClaimEvent, String> {
    let (c, args) = cov_input(tx, t, 0)?;
    let ticket = as_state!(c.state, Ticket);
    let pool = as_state!(leg(tx, t, 1, Role::Pool, "ticket_leg")?, Pool);
    Ok(ClaimEvent { ticket, pool, idx: arg_int(&args, 0)?, leaves: arg_bytes(&args, 1)?.to_vec(), siblings: arg_bytes(&args, 2)?.to_vec() })
}

pub fn parse_deposit(tx: &Transaction, t: &Templates) -> Result<DepositEvent, String> {
    let (c, args) = cov_input(tx, t, 0)?;
    let vault = as_state!(c.state, Vault);
    Ok(DepositEvent { vault, owner: payout(arg_byte(&args, 0)?, arg_bytes(&args, 1)?)? })
}

pub fn parse_withdraw(tx: &Transaction, t: &Templates) -> Result<WithdrawEvent, String> {
    let (c, args) = cov_input(tx, t, 0)?;
    let vault = as_state!(c.state, Vault);
    let position = as_state!(leg(tx, t, 1, Role::Position, "vault_leg")?, Position);
    Ok(WithdrawEvent { vault, position, w: arg_int(&args, 0)? })
}

pub fn parse_settle(tx: &Transaction, t: &Templates) -> Result<SettleEvent, String> {
    let (c, _) = cov_input(tx, t, 0)?;
    let request = as_state!(c.state, Request);
    let record = as_state!(leg(tx, t, 1, Role::Record, "request_leg")?, Record);
    Ok(SettleEvent { request, record })
}

// ------------------------------------------------------------------------------ rebuild

/// The amount and creation DAA score of a spent entry (an RPC node returns them with the
/// accepted transaction's inputs: `RpcOptionalTransactionInputVerboseData::utxo_entry`,
/// rusty-kaspa v2.1.0 `rpc/core/src/model/optional/tx.rs:18-28, 306-307`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spent {
    pub amount: u64,
    pub daa: u64,
}

/// One accepted transaction: the transaction, the DAA score its outputs are created at
/// (the accepting chain block's), and the entries it spent, in input order.
#[derive(Clone, Debug)]
pub struct Accepted {
    pub tx: Transaction,
    pub daa: u64,
    pub spent: Vec<Spent>,
}

impl Accepted {
    pub fn new(tx: Transaction, daa: u64, entries: &[UtxoEntry]) -> Self {
        Self { tx, daa, spent: entries.iter().map(|e| Spent { amount: e.amount, daa: e.block_daa_score }).collect() }
    }
}

/// A live UTXO of the instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Live {
    pub outpoint: TransactionOutpoint,
    pub role: Role,
    pub value: u64,
    /// Creation DAA score.
    pub daa: u64,
    pub spk: ScriptPublicKey,
    /// The state, when known (rebuilt by this index, or given for a genesis output).
    pub state: Option<State>,
}

/// One shard's counter tree in a round. `tree` is `None` once the index cannot follow the
/// shard (a spent root that differs from the rebuilt one), with the reason in `broken`.
#[derive(Clone)]
pub struct ShardTrack {
    pub tree: Option<Tree>,
    pub broken: Option<String>,
}

/// A purchase as indexed (A3 `buys[]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuyRecord {
    pub txid: TransactionId,
    pub shard_index: i64,
    pub combos: Vec<[u8; 6]>,
    pub ticket_outpoint: TransactionOutpoint,
    pub ticket: TicketState,
    /// The referral amount paid (0 without a referral output).
    pub referral: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloseInfo {
    pub txid: TransactionId,
    /// The marker's creation DAA score (D = marker_daa + seed_offset).
    pub marker_daa: u64,
    pub marker: MarkerState,
    pub frozen_roots: Vec<[u8; 32]>,
    pub m_next: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawInfo {
    pub txid: TransactionId,
    pub result: RoundResult,
}

/// A round as indexed.
#[derive(Clone)]
pub struct Round {
    pub round: i64,
    /// m of the round's shards, once known (genesis states are hidden behind P2SH).
    pub m: Option<i64>,
    pub shards: BTreeMap<i64, ShardTrack>,
    pub buys: Vec<BuyRecord>,
    pub close: Option<CloseInfo>,
    pub draw: Option<DrawInfo>,
    /// Why the round is incomplete, if it is (set at its close).
    pub incomplete: Option<String>,
    /// Shard UTXOs of the round spent without the index having seen them created.
    pub gaps: Vec<String>,
}

impl Round {
    fn new(round: i64, k: usize, m: Option<i64>) -> Self {
        let shards = (0..k as i64).map(|i| (i, ShardTrack { tree: m.map(|m| Tree::new(m as usize)), broken: None })).collect();
        Self { round, m, shards, buys: vec![], close: None, draw: None, incomplete: None, gaps: vec![] }
    }
}

#[derive(Clone, Default)]
struct Core {
    genesis_txid: Option<TransactionId>,
    rounds: BTreeMap<i64, Round>,
    live: HashMap<TransactionOutpoint, Live>,
    /// S2c should-fix 4: instance-wide gaps, missed transitions of the controller, params,
    /// drawer, vault, positions, requests or records (round objects go to their round).
    gaps: Vec<String>,
}

/// The rebuilt instance (spec M8 A4).
pub struct Rebuild {
    t: Arc<Templates>,
    cov: Hash,
    genesis_states: Option<Vec<State>>,
    core: Core,
    log: Vec<Accepted>,
}

/// Run a model that panics where its reference rejects; a panic becomes an error.
pub(crate) fn model<T>(what: &str, f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|p| {
        let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
        format!("{what}: the model rejects the spent states: {msg}")
    })
}

impl Rebuild {
    /// An empty index of instance `cov` under templates `t`. `genesis` are the genesis
    /// outputs' states in output order (controller, params, k shards, vault, drawer,
    /// position), if known; they are checked against the genesis outputs.
    pub fn new(t: Arc<Templates>, cov: Hash, genesis: Option<Vec<State>>) -> Self {
        Self { t, cov, genesis_states: genesis, core: Core::default(), log: vec![] }
    }

    pub fn cov(&self) -> Hash {
        self.cov
    }

    pub fn templates(&self) -> &Arc<Templates> {
        &self.t
    }

    /// The one live UTXO of `role` with a known state (the controller, params, marker,
    /// vault and drawer are singletons). `Err` if there is none, several, or its state is
    /// unknown (a genesis output without the genesis states).
    pub fn singleton(&self, role: Role) -> Result<(&Live, &State), String> {
        let mut it = self.core.live.values().filter(|l| l.role == role);
        let l = it.next().ok_or_else(|| format!("no live {role:?}"))?;
        if it.next().is_some() {
            return Err(format!("several live {role:?} UTXOs"));
        }
        let s = l.state.as_ref().ok_or_else(|| format!("the live {role:?}'s state is unknown (a genesis output: give the genesis states)"))?;
        Ok((l, s))
    }

    pub fn genesis_txid(&self) -> Option<TransactionId> {
        self.core.genesis_txid
    }

    pub fn rounds(&self) -> &BTreeMap<i64, Round> {
        &self.core.rounds
    }

    pub fn round(&self, r: i64) -> Option<&Round> {
        self.core.rounds.get(&r)
    }

    /// The live UTXOs, by outpoint.
    pub fn live(&self) -> &HashMap<TransactionOutpoint, Live> {
        &self.core.live
    }

    /// The applied transactions, in order.
    pub fn log(&self) -> &[Accepted] {
        &self.log
    }

    /// Instance-wide gaps (S2c should-fix 4): a spent template instance of the controller,
    /// params, drawer, vault, a position, a request or a record that the index never saw
    /// created (it missed that object's transition; the stale predecessor stays live). A
    /// missed shard, ticket, pool or marker transition is a gap of its round instead
    /// (`Round::gaps`, and the round is incomplete).
    pub fn gaps(&self) -> &[String] {
        &self.core.gaps
    }

    /// The frozen shard trees of a closed round (shard order), for count proofs. Refused
    /// when the round is not closed or is incomplete (spec M8 A4: no proof is offered from
    /// an incomplete round), and when the index has an instance-wide gap (S2c should-fix
    /// 4: its live set is not the chain's, so nothing is built from it).
    pub fn trees(&self, r: i64) -> Result<Vec<Tree>, String> {
        if !self.core.gaps.is_empty() {
            return Err(format!("the index has missed transitions: {}", self.core.gaps.join("; ")));
        }
        let round = self.core.rounds.get(&r).ok_or_else(|| format!("round {r} is not indexed"))?;
        if round.close.is_none() {
            return Err(format!("round {r} is not closed"));
        }
        if let Some(why) = &round.incomplete {
            return Err(format!("round {r} is incomplete: {why}"));
        }
        round.shards.values().map(|s| s.tree.clone().ok_or_else(|| format!("round {r}: a shard tree is missing"))).collect()
    }

    /// Apply one accepted transaction. `Ok(None)`: not this instance's. `Ok(Some(event))`:
    /// applied. `Err`: an `Unknown` transaction or a rebuilt state that does not match the
    /// transaction; nothing is applied.
    pub fn apply(&mut self, a: &Accepted) -> Result<Option<Event>, String> {
        let txid = a.tx.id();
        if a.spent.len() != a.tx.inputs.len() {
            return Err(format!("tx {txid}: {} spent entries for {} inputs", a.spent.len(), a.tx.inputs.len()));
        }
        let bound = a.tx.outputs.iter().any(|o| o.covenant.is_some_and(|b| b.covenant_id == self.cov));
        let spends_live = a.tx.inputs.iter().any(|i| self.core.live.contains_key(&i.previous_outpoint));
        let Some(ev) = classify(&a.tx, &self.t, self.cov) else {
            if spends_live {
                return Err(format!("tx {txid}: spends a live instance UTXO but is not a transition of the instance"));
            }
            return Ok(None);
        };
        if !bound && !spends_live {
            return Ok(None);
        }
        if let Event::Unknown(why) = &ev {
            return Err(format!("tx {txid}: Unknown covenant-bound transaction: {why}"));
        }
        let mut next = self.core.clone();
        // S2c should-fix 3 (D115): every input is node-chosen. The arithmetic below is
        // checked; a panic left anywhere (a model, a decoder) is still an error, never a
        // crash of the indexer, and `next` is dropped.
        let (t, cov, gs) = (&self.t, self.cov, self.genesis_states.as_deref());
        model("apply", || next.apply(t, cov, gs, a, &ev))?.map_err(|e| format!("tx {txid} ({}): {e}", ev.name()))?;
        self.core = next;
        self.log.push(a.clone());
        Ok(Some(ev))
    }

    /// Undo the most recently applied transaction `txid` (a reorg removes chain blocks
    /// newest first): the remaining log is replayed from an empty index.
    pub fn revert(&mut self, txid: TransactionId) -> Result<(), String> {
        match self.log.last() {
            Some(a) if a.tx.id() == txid => {}
            Some(a) => return Err(format!("revert {txid}: the last applied transaction is {}", a.tx.id())),
            None => return Err(format!("revert {txid}: nothing applied")),
        }
        let mut log = std::mem::take(&mut self.log);
        log.pop();
        self.core = Core::default();
        for a in &log {
            self.apply(a).map_err(|e| format!("revert {txid}: replay failed: {e}"))?;
        }
        Ok(())
    }
}

/// An expected covenant output: index, state, value.
type Out = (usize, State, u64);

impl Core {
    fn round_mut(&mut self, r: i64) -> &mut Round {
        self.rounds.entry(r).or_insert_with(|| Round::new(r, 0, None))
    }

    fn apply(&mut self, t: &Templates, cov: Hash, genesis: Option<&[State]>, a: &Accepted, ev: &Event) -> Result<(), String> {
        let tx = &a.tx;
        let txid = tx.id();
        // Spend: every live input must match its recorded entry and script; a template
        // instance input the index has not seen created is a gap of its round.
        for (i, input) in tx.inputs.iter().enumerate() {
            let redeem = pushes(&input.signature_script).ok().and_then(|p| p.last().cloned());
            match self.live.remove(&input.previous_outpoint) {
                Some(l) => {
                    if (l.value, l.daa) != (a.spent[i].amount, a.spent[i].daa) {
                        return Err(format!("input {i}: spent entry {:?} differs from the indexed UTXO ({}, {})", a.spent[i], l.value, l.daa));
                    }
                    if redeem.as_ref().map(|r| pay_to_script_hash_script(r)) != Some(l.spk.clone()) {
                        return Err(format!("input {i}: the redeem script does not hash to the indexed UTXO's script"));
                    }
                    if let (Some(known), Some(r)) = (&l.state, &redeem) {
                        if &known.redeem(t) != r {
                            return Err(format!("input {i}: the spent state differs from the rebuilt one"));
                        }
                    }
                }
                None => {
                    // S2c should-fix 4: any template instance spent without the index having
                    // seen it created is a missed transition of its object, never accepted
                    // silently. Round objects (shard, ticket, pool, marker) make their round
                    // incomplete; every other role is an instance-wide gap (`gaps`).
                    if let Some(s) = redeem.as_ref().and_then(|r| State::decode(t, r).ok()) {
                        let what = match &s {
                            State::Shard(sh) => format!("shard {} of round {}", sh.shard_index, sh.round),
                            other => format!("{:?} of round {}", other.role(), other.round()),
                        };
                        let why = format!("{what} spent at {} was never seen created (a missed transition)", input.previous_outpoint);
                        match s.role() {
                            Role::Shard | Role::Ticket | Role::Pool | Role::Marker => {
                                let round = self.round_mut(s.round());
                                if round.close.is_some() {
                                    round.incomplete = Some(match round.incomplete.take() {
                                        Some(w) => format!("{w}; {why}"),
                                        None => why.clone(),
                                    });
                                }
                                round.gaps.push(why);
                            }
                            _ => self.gaps.push(why),
                        }
                    }
                }
            }
        }
        let outs: Vec<Out> = match ev {
            Event::Genesis(g) => return self.genesis(t, genesis, a, g.k),
            Event::Buy(b) => self.buy(a, b)?,
            Event::SetParams(p) => vec![(0, State::Params(p.successor.clone()), a.spent[0].amount)],
            Event::Close(c) => self.close(t, a, c)?,
            Event::Draw(d) => self.draw(t, cov, a, d)?,
            Event::Claim(c) => claim_outputs(a, c)?,
            Event::ReturnDeposit(r) => match &r.pool {
                Some(p) => vec![(0, State::Pool(p.clone()), a.spent[1].amount)],
                None => vec![],
            },
            Event::Refresh(s) => vec![(0, s.clone(), a.spent[0].amount)],
            Event::RequestDeposit(d) => deposit_outputs(t, a, d)?,
            Event::RequestWithdraw(w) => withdraw_outputs(t, a, w)?,
            Event::Settle(s) => settle_outputs(t, a, s)?,
            Event::Unknown(why) => return Err(format!("Unknown: {why}")),
        };
        // R2: exactly the expected covenant outputs, each the rebuilt state's P2SH and value.
        let bound: Vec<usize> = tx.outputs.iter().enumerate().filter(|(_, o)| o.covenant.is_some()).map(|(i, _)| i).collect();
        let want: Vec<usize> = outs.iter().map(|o| o.0).collect();
        if bound != want {
            return Err(format!("covenant outputs {bound:?}, the rebuilt transition has {want:?}"));
        }
        for (i, s, v) in outs {
            let o = &tx.outputs[i];
            if o.covenant.map(|b| b.covenant_id) != Some(cov) {
                return Err(format!("output {i} is not bound to the instance"));
            }
            let spk = s.spk(t);
            if o.script_public_key != spk {
                return Err(format!("output {i}: the rebuilt {:?} state does not hash to the output script", s.role()));
            }
            if o.value != v {
                return Err(format!("output {i}: value {}, the rebuilt {:?} has {v}", o.value, s.role()));
            }
            let op = TransactionOutpoint::new(txid, i as u32);
            self.live.insert(op, Live { outpoint: op, role: s.role(), value: v, daa: a.daa, spk, state: Some(s) });
        }
        Ok(())
    }

    fn genesis(&mut self, t: &Templates, states: Option<&[State]>, a: &Accepted, k: usize) -> Result<(), String> {
        if self.genesis_txid.is_some() {
            return Err("a second genesis".into());
        }
        let txid = a.tx.id();
        let mut roles = vec![Role::Controller, Role::Params];
        roles.extend(std::iter::repeat_n(Role::Shard, k));
        roles.extend([Role::Vault, Role::Drawer, Role::Position]);
        if let Some(s) = states {
            if s.len() != roles.len() {
                return Err(format!("{} genesis states for {} outputs", s.len(), roles.len()));
            }
        }
        let mut m = None;
        for (i, role) in roles.iter().enumerate() {
            let o = &a.tx.outputs[i];
            let state = match states {
                Some(s) => {
                    let st = s[i].clone();
                    if st.role() != *role || st.spk(t) != o.script_public_key {
                        return Err(format!("genesis output {i}: the given {:?} state does not hash to the output script", st.role()));
                    }
                    if let State::Controller(c) = &st {
                        m = Some(c.m);
                    }
                    Some(st)
                }
                None => None,
            };
            let op = TransactionOutpoint::new(txid, i as u32);
            self.live.insert(op, Live { outpoint: op, role: *role, value: o.value, daa: a.daa, spk: o.script_public_key.clone(), state });
        }
        self.genesis_txid = Some(txid);
        self.rounds.insert(1, Round::new(1, k, m));
        Ok(())
    }

    fn buy(&mut self, a: &Accepted, b: &BuyEvent) -> Result<Vec<Out>, String> {
        let txid = a.tx.id();
        let s = &b.shard;
        let round = self.round_mut(s.round);
        round.m.get_or_insert(s.m);
        let track = round.shards.entry(s.shard_index).or_insert(ShardTrack { tree: None, broken: None });
        if track.broken.is_none() {
            if track.tree.is_none() {
                if s.root == z18() {
                    track.tree = Some(Tree::new(s.m as usize));
                } else {
                    track.broken = Some(format!("shard {} first seen with a non-empty root", s.shard_index));
                }
            }
            if let Some(tree) = &track.tree {
                if tree.root() != s.root {
                    track.broken = Some(format!("shard {}: the spent root differs from the rebuilt tree's (a purchase is missing)", s.shard_index));
                    track.tree = None;
                }
            }
            if let Some(tree) = &mut track.tree {
                for c in &b.bundle {
                    tree.increment(c.rank(), c.bonus);
                }
                if tree.root() != b.successor.root {
                    return Err("the rebuilt tree's root differs from the proofs' successor root".into());
                }
            }
        }
        round.buys.push(BuyRecord {
            txid,
            shard_index: s.shard_index,
            combos: b.bundle.iter().map(|c| c.bytes()).collect(),
            ticket_outpoint: TransactionOutpoint::new(txid, 1),
            ticket: b.ticket.clone(),
            referral: b.referral,
        });
        if b.with_ref && a.tx.outputs.get(2).map(|o| o.value) != Some(b.referral) {
            return Err("the referral output is not r".into());
        }
        let (gross, _) = buy_money(s, b.b, b.with_ref)?;
        let shard_value = a.spent[0].amount.checked_add(gross).and_then(|v| v.checked_sub(b.referral)).ok_or("buy: shard value overflows")?;
        let d = u64::try_from(s.d).map_err(|_| format!("buy: d {}", s.d))?;
        let ticket_value = (b.b as u64).checked_mul(d).ok_or("buy: b x d overflows")?;
        Ok(vec![(0, State::Shard(b.successor.clone()), shard_value), (1, State::Ticket(b.ticket.clone()), ticket_value)])
    }

    fn close(&mut self, t: &Templates, a: &Accepted, c: &CloseEvent) -> Result<Vec<Out>, String> {
        let k = c.shards.len();
        let ctrl = &c.controller;
        let mut cc = CloseCase::new_with(k, t);
        cc.controller = ctrl.clone();
        cc.controller_daa = a.spent[0].daa;
        cc.pot = a.spent[0].amount.checked_sub(acc_plus_r(ctrl)?).ok_or("controller value below acc + R")?;
        cc.params = c.params.clone();
        cc.params_daa = a.spent[1].daa;
        cc.shards = c.shards.iter().enumerate().map(|(i, s)| ShardIn { state: s.clone(), daa: a.spent[2 + i].daa }).collect();
        cc.funding = a.spent[k + 2..].iter().map(|s| FundingIn { value: s.amount, daa: s.daa }).collect();
        cc.funder = c.funder;
        cc.fee = 0;
        cc.lock_time = a.tx.lock_time;
        cc.m_next = Some(c.m_next);
        let x = model("close", || cc.expected())?;
        // The frozen roots against the rebuilt trees.
        let r = ctrl.round;
        let txid = a.tx.id();
        let round = self.round_mut(r);
        round.m.get_or_insert(ctrl.m);
        let mut why: Vec<String> = round.gaps.clone();
        for (i, root) in c.frozen_roots.iter().enumerate() {
            let track = round.shards.entry(i as i64).or_insert(ShardTrack { tree: None, broken: None });
            if track.tree.is_none() && track.broken.is_none() && *root == z18() {
                track.tree = Some(Tree::new(ctrl.m as usize));
            }
            match (&track.tree, &track.broken) {
                (_, Some(b)) => why.push(b.clone()),
                (Some(tree), None) if tree.root() == *root => {}
                (Some(_), None) => why.push(format!("shard {i}: the rebuilt root differs from the frozen root")),
                (None, None) => why.push(format!("shard {i}: the frozen root is not empty but the index saw no purchase")),
            }
        }
        round.incomplete = (!why.is_empty()).then(|| why.join("; "));
        round.close = Some(CloseInfo { txid, marker_daa: a.daa, marker: x.marker.clone(), frozen_roots: c.frozen_roots.clone(), m_next: x.m_next });
        let mut next = Round::new(r + 1, k, Some(x.m_next));
        if let Some(old) = self.rounds.remove(&(r + 1)) {
            // Purchases of round r + 1 cannot precede its shards' creation; keep any gaps.
            next.gaps = old.gaps;
        }
        self.rounds.insert(r + 1, next);
        let mut outs: Vec<Out> = vec![(0, State::Controller(x.controller.clone()), x.values[0]), (1, State::Params(x.params.clone()), x.values[1])];
        for (i, s) in x.shards.iter().enumerate() {
            outs.push((2 + i, State::Shard(s.clone()), x.values[2 + i]));
        }
        outs.push((k + 2, State::Marker(x.marker.clone()), x.values[k + 2]));
        Ok(outs)
    }

    fn draw(&mut self, t: &Templates, cov: Hash, a: &Accepted, d: &DrawEvent) -> Result<Vec<Out>, String> {
        let mk = &d.marker;
        // S2c should-fix 3: k and m size the trees and the case; bound them first
        // (`RoundResult::from_tx` checks the same ranges).
        if !(1..=K_MAX as i64).contains(&mk.k) || !(2..=crate::game::M_DRAW).contains(&mk.m) {
            return Err(format!("marker k {} or m {} out of range", mk.k, mk.m));
        }
        let k = mk.k as usize;
        let ctrl = &d.controller;
        if a.spent[1].daa as i64 != d.mk_daa {
            return Err(format!("mk_daa {} is not the marker's creation DAA score {}", d.mk_daa, a.spent[1].daa));
        }
        let mut c = DrawCase::new_with(k, t);
        c.drawer = d.drawer.clone();
        c.drawer_daa = a.spent[0].daa;
        c.marker = mk.clone();
        c.marker_daa = a.spent[1].daa;
        c.vault = d.vault.clone();
        c.vault_daa = a.spent[2].daa;
        c.controller = ctrl.clone();
        c.controller_daa = a.spent[3].daa;
        c.pot = a.spent[3].amount.checked_sub(acc_plus_r(ctrl)?).ok_or("controller value below acc + R")?;
        c.trees = (0..k).map(|_| Tree::new(mk.m as usize)).collect();
        c.absorbed = d
            .absorbed
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let s = a.spent[4 + i];
                let r = u64::try_from(p.reserve).map_err(|_| "pool reserve below 0")?;
                s.amount.checked_sub(r).map(|remaining| PoolIn { state: p.clone(), remaining, daa: s.daa }).ok_or("pool value below R")
            })
            .collect::<Result<_, _>>()?;
        c.funding = a.spent[4 + d.absorbed.len()..].iter().map(|s| FundingIn { value: s.amount, daa: s.daa }).collect();
        c.funder = d.result.funder;
        c.fee = 0;
        c.lock_time = a.tx.lock_time;
        c.cov = cov;
        c.sim = None;
        c.x_hash = [1; 32];
        c.void = d.void();
        if !c.void {
            // The anchor as in the transaction: X.hash = its bytes 0..32 (D94), the counts from
            // the leaves argument (the proofs were checked on chain against the frozen roots).
            c.lie.anchor = Some(d.anchor.clone());
            let counts = leaf_counts(&d.leaves, mk.m)?;
            c.lie.counts = Some(win_counts(&counts, d.result.win[5], mk.m));
        }
        c.lie.witness = Some(d.witness);
        let x = model("draw", || c.expected())?;
        if x.conv.wc != d.wc {
            return Err(format!("wc argument {} is not the conversion's {}", d.wc, x.conv.wc));
        }
        let r = mk.round;
        let txid = a.tx.id();
        self.round_mut(r).draw = Some(DrawInfo { txid, result: d.result.clone() });
        let mut outs: Vec<Out> = vec![
            (0, State::Drawer(x.drawer.clone()), x.values[0]),
            (1, State::Vault(x.vault.clone()), x.values[1]),
            (2, State::Controller(x.controller.clone()), x.values[2]),
        ];
        for (p, s) in x.new_pools.iter().enumerate() {
            outs.push((x.pool_output(p), State::Pool(s.clone()), x.values[x.pool_output(p)]));
        }
        if let (Some(i), Some(rc)) = (x.record_output(), &x.record) {
            outs.push((i, State::Record(rc.clone()), x.values[i]));
        }
        Ok(outs)
    }
}

/// claim(): pool' is the pool unchanged at value - prize; ticket' (PAY, ACCRUE) has the
/// index's bit set (`game::claim_amount`, `game::claim_mode`, as `ClaimCase::expected`).
fn claim_outputs(a: &Accepted, c: &ClaimEvent) -> Result<Vec<Out>, String> {
    let tk = &c.ticket;
    let p = &c.pool;
    let i = c.idx;
    // S2c should-fix 3: the ticket state and the index are node-chosen; i is bounded by
    // TICKET_B_MAX before any index arithmetic.
    if !(0..tk.b.min(crate::game::TICKET_B_MAX)).contains(&i) || tk.bundle.len() < 6 * (i as usize + 1) || tk.claimed.is_empty() {
        return Err(format!("claim index {i}"));
    }
    let comb = &tk.bundle[6 * i as usize..6 * i as usize + 6];
    let counts = leaf_counts(&c.leaves, p.m)?;
    let bonus = comb[5] as usize;
    let h: i64 = counts.iter().map(|c| if (1..=c.len()).contains(&bonus) { c[bonus - 1] as i64 } else { 0 }).sum();
    let prize = claim_amount(p.slice, h).ok_or("claim: h < 1")?;
    let mode = claim_mode(tk.claimed[0], tk.b, i, prize).ok_or("claim: index already claimed or out of range")?;
    let prize_u = u64::try_from(prize).map_err(|_| format!("claim: prize {prize}"))?;
    let pool_value = a.spent[1].amount.checked_sub(prize_u).ok_or("claim: prize above the pool value")?;
    let mut outs: Vec<Out> = vec![(0, State::Pool(p.clone()), pool_value)];
    if mode != ClaimMode::Final {
        let mut next = tk.clone();
        next.claimed[0] |= 1u8.checked_shl(i as u32).ok_or("claim: index bit")?;
        let v = if mode == ClaimMode::Pay { a.spent[0].amount } else { a.spent[0].amount.checked_add(prize_u).ok_or("claim: ticket value overflows")? };
        outs.push((1, State::Ticket(next), v));
    }
    Ok(outs)
}

/// request_deposit(): `DepositCase::expected` with D = vault' value - vault value and R =
/// vault value - total_assets - pend_dep.
fn deposit_outputs(t: &Templates, a: &Accepted, d: &DepositEvent) -> Result<Vec<Out>, String> {
    let v_in = a.spent[0].amount;
    let v_out = a.tx.outputs.first().ok_or("no output 0")?.value;
    let dep = i64::try_from(v_out.checked_sub(v_in).ok_or("vault' value below the vault's")?).map_err(|_| "deposit above i64")?;
    let mut c = DepositCase::new_with(t);
    c.vault = d.vault.clone();
    c.reserve = v_in.checked_sub(held_by_vault(&d.vault)?).ok_or("vault value below total_assets + pend_dep")?;
    c.d = dep;
    c.owner = d.owner;
    c.change = false;
    let x = model("request_deposit", || c.expected())?;
    Ok(vec![(0, State::Vault(x.vault), x.values[0]), (1, State::Request(x.request), x.values[1])])
}

/// request_withdraw(): `WithdrawCase::expected` (R = vault value - total_assets - pend_dep).
fn withdraw_outputs(t: &Templates, a: &Accepted, w: &WithdrawEvent) -> Result<Vec<Out>, String> {
    let v_in = a.spent[0].amount;
    let mut c = WithdrawCase::new_with(t, crate::vault::owner());
    c.vault = w.vault.clone();
    c.reserve = v_in.checked_sub(held_by_vault(&w.vault)?).ok_or("vault value below total_assets + pend_dep")?;
    c.position = w.position.clone();
    c.w = w.w;
    c.change = false;
    let x = model("request_withdraw", || c.expected())?;
    let mut outs: Vec<Out> = vec![(0, State::Vault(x.vault), x.values[0]), (1, State::Request(x.request), x.values[1])];
    if let Some(p) = x.position {
        outs.push((2, State::Position(p), x.values[2]));
    }
    Ok(outs)
}

/// settle(): `SettleCase::expected` (R_rc = record value - out_rem - refund x dep_rem).
fn settle_outputs(t: &Templates, a: &Accepted, s: &SettleEvent) -> Result<Vec<Out>, String> {
    let rc = &s.record;
    let mut c = SettleCase::new_with(t, rc.clone(), s.request.clone());
    c.request_value = a.spent[0].amount;
    let held = rc.out_rem as i128 + rc.refund as i128 * rc.dep_rem as i128;
    c.record_reserve = u64::try_from(a.spent[1].amount as i128 - held).map_err(|_| "record value below its remainders")?;
    c.change = false;
    c.funding = vec![];
    c.fee = 0;
    let x = model("settle", || c.expected())?;
    if x.reference.is_none() {
        return Err("the settlement reference rejects the spent request and record".into());
    }
    let mut outs: Vec<Out> = vec![];
    if let Some(i) = x.record_output {
        outs.push((i, State::Record(x.record.clone()), x.values[i]));
    }
    if let Some(i) = x.cov2_output {
        match (&x.position, &x.request) {
            (Some(p), _) => outs.push((i, State::Position(p.clone()), x.values[i])),
            (None, Some(r)) => outs.push((i, State::Request(r.clone()), x.values[i])),
            _ => return Err("settle: a second covenant output with no state".into()),
        }
    }
    Ok(outs)
}

/// acc + R of a decoded controller, checked (S2c should-fix 3).
fn acc_plus_r(c: &ControllerState) -> Result<u64, String> {
    c.acc.checked_add(c.reserve).and_then(|x| u64::try_from(x).ok()).ok_or_else(|| format!("controller acc {} + R {}", c.acc, c.reserve))
}

/// total_assets + pend_dep of a decoded vault, checked (S2c should-fix 3).
fn held_by_vault(v: &VaultState) -> Result<u64, String> {
    v.total_assets.checked_add(v.pend_dep).and_then(|x| u64::try_from(x).ok()).ok_or_else(|| format!("vault total_assets {} + pend_dep {}", v.total_assets, v.pend_dep))
}

/// The refresh object of a decoded state (for callers that build a refresh from the index).
pub fn refresh_obj(s: &State) -> Option<RefreshObj> {
    Some(match s {
        State::Pool(x) => RefreshObj::Pool(x.clone()),
        State::Ticket(x) => RefreshObj::Ticket(x.clone()),
        State::Position(x) => RefreshObj::Position(x.clone()),
        State::Request(x) => RefreshObj::Request(x.clone()),
        State::Record(x) => RefreshObj::Record(x.clone()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buy::{BuyCase, b_max};

    /// Every committed template's state decodes and re-encodes; a buy parses to the
    /// builder's own successor and ticket.
    #[test]
    fn decode_and_parse_buy() {
        let t = Templates::get(b_max());
        let mut c = BuyCase::new(b_max());
        c.bundle = vec![Combo::new([1, 2, 3, 4, 5], 2), Combo::new([1, 2, 3, 4, 5], 2), Combo::new([6, 7, 8, 9, 10], 1)];
        c.with_ref = true;
        let f = c.build().finish();
        let b = parse_buy(&f.tx, &t).unwrap();
        assert_eq!(b.successor, c.successor());
        assert_eq!(b.ticket, c.ticket());
        assert_eq!(b.referral, c.referral());
        assert_eq!(State::decode(&t, &t.shard_redeem(&c.shard)).unwrap(), State::Shard(c.shard.clone()));
        assert!(State::decode(&t, &[0x51]).is_err());
    }
}
