//! The decoded states as the page reads them (spec 6.1 "decode": every state). One view per
//! covenant role, generated from the state's own fields by `view!`: the conversion
//! destructures the `lottery::state` struct exhaustively, so a field added to a state and
//! not to its view is a compile error, never a silent omission. Field names are the
//! covenant's, in camelCase. i64 fields are decimal strings, byte fields hex, and each D11
//! (class, key) pair also gets its address on the page's network.

use lottery::index::State;
use lottery::state::{
    ControllerState, DrawerState, MarkerState, ParamsState, PoolState, PositionState, RecordState, RequestState, ShardState, TicketState, VaultState,
};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::convert::{Net, hex};
use crate::dto::{Address, Combination, Dec, Hex, Shares};

macro_rules! conv {
    (Dec, $v:expr) => { $v.to_string() };
    (u8, $v:expr) => { *$v };
    (Hex, $v:expr) => { hex(&$v[..]) };
    (Shares, $v:expr) => { $v.iter().map(|x| x.to_string()).collect() };
    (Combination, $v:expr) => { Combination { main: [$v[0], $v[1], $v[2], $v[3], $v[4]], bonus: $v[5] } };
}

macro_rules! view {
    (
        $(#[doc = $doc:tt])* $view:ident from $state:ident { $($f:ident: $kind:ident),* $(,)? }
        $(address $addr:ident = ($cls:ident, $key:ident);)*
        $(extra $x:ident: [$($xt:tt)+] = $xe:expr;)*
    ) => {
        $(#[doc = $doc])*
        #[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
        #[serde(rename_all = "camelCase")]
        pub struct $view {
            $(pub $f: $kind,)*
            $(pub $addr: Option<Address>,)*
            $(pub $x: $($xt)+,)*
        }

        impl $view {
            #[allow(unused_variables)]
            pub fn of(s: &$state, net: Net) -> Self {
                let $state { $($f),* } = s;
                Self { $($f: conv!($kind, $f),)* $($addr: net.payout_address(*$cls, $key),)* $($x: $xe,)* }
            }
        }
    };
}

view! {
    /// A shard of a round's sales (spec M1 A2).
    ShardView from ShardState { round: Dec, shard_index: Dec, m: Dec, price: Dec, d: Dec, s_r: Dec, deadline: Dec, root: Hex, sales: Dec, ref_paid: Dec }
}

fn combinations(bundle: &[u8], b: i64) -> Vec<Combination> {
    bundle.chunks_exact(6).take(b.clamp(0, 64) as usize).map(|c| Combination { main: [c[0], c[1], c[2], c[3], c[4]], bonus: c[5] }).collect()
}

fn claimed_bits(claimed: &[u8], b: i64) -> Vec<u32> {
    (0..b.clamp(0, 64) as u32).filter(|&i| claimed.get(i as usize / 8).is_some_and(|x| x & (1 << (i % 8)) != 0)).collect()
}

view! {
    /// A ticket (spec M1 A2, M4): its combinations and which indices are claimed.
    TicketView from TicketState { round: Dec, deadline: Dec, b: Dec, bundle: Hex, claimed: Hex, payout_class: u8, payout_key: Hex }
    address payout_address = (payout_class, payout_key);
    extra combinations: [Vec<Combination>] = combinations(bundle, *b);
    extra claimed_indices: [Vec<u32>] = claimed_bits(claimed, *b);
}

view! {
    /// The round controller (spec M2 A3.1).
    ControllerView from ControllerState {
        round: Dec, k: Dec, m: Dec, price: Dec, d: Dec, deadline: Dec, s_f: Dec, s_r: Dec, fee_class: u8, fee_key: Hex, jackpot: Dec,
        j_boost: Dec, g2: Dec, liability: Dec, v_rec: Dec, acc: Dec, drawn: u8, shard_tpl: Hex, marker_tpl: Hex, params_tpl: Hex, reserve: Dec,
        s_c: Dec, s_d: Dec, st: Shares, e: Dec, j: Dec, g: Dec, margin: Dec, m_min: Dec, m_max: Dec, cap: Dec, round_len: Dec, d_min: Dec, seed_offset: Dec,
    }
    address fee_address = (fee_class, fee_key);
}

view! {
    /// The round parameters (spec M2 A3.2).
    ParamsView from ParamsState { round: Dec, admin_hash: Hex, nonce: Dec, used: u8, next_s_f: Dec, next_s_r: Dec, next_fee_class: u8, next_fee_key: Hex }
    address next_fee_address = (next_fee_class, next_fee_key);
}

view! {
    /// The marker of a closed round (spec M2 A3.3): the frozen roots and the counts.
    MarkerView from MarkerState {
        round: Dec, k: Dec, roots: Hex, sales: Dec, ref_paid: Dec, m: Dec, jackpot: Dec, j_boost: Dec, g2: Dec, s_f: Dec, s_r: Dec, s_c: Dec, s_d: Dec,
        st: Shares, funder_class: u8, funder_key: Hex,
    }
    address funder_address = (funder_class, funder_key);
}

view! {
    /// The drawer (spec M3, M7).
    DrawerView from DrawerState { round: Dec, pool_tpl: Hex, vault_tpl: Hex, ticket_tpl: Hex, f_min: Dec, claim_window: Dec, draw_window: Dec }
}

view! {
    /// The backer vault (spec M5 A2).
    VaultView from VaultState {
        round: Dec, total_assets: Dec, total_shares: Dec, pend_dep: Dec, pend_wsh: Dec, liability: Dec, record_tpl: Hex, request_tpl: Hex, position_tpl: Hex,
    }
}

view! {
    /// A prize pool of a drawn round (spec M3 A9, M4).
    PoolView from PoolState {
        round: Dec, tier: u8, slice: Dec, win: Combination, m: Dec, k: Dec, roots: Hex, deadline: Dec, ticket_tpl: Hex, reserve: Dec, funder_class: u8, funder_key: Hex,
    }
    address funder_address = (funder_class, funder_key);
}

view! {
    /// A vault request (spec M5 A3.2): kind 0 = deposit, 1 = withdrawal.
    RequestView from RequestState { round: Dec, kind: u8, amount: Dec, owner_class: u8, owner_key: Hex }
    address owner_address = (owner_class, owner_key);
}

view! {
    /// A settlement record (spec M5 A3.3).
    RecordView from RecordState {
        round: Dec, request_tpl: Hex, refund: u8, dep_rem: Dec, mint_rem: Dec, wsh_rem: Dec, burn_rem: Dec, out_rem: Dec, funder_class: u8, funder_key: Hex,
    }
    address funder_address = (funder_class, funder_key);
}

view! {
    /// A backer's share position (spec M5 A2, D67).
    PositionView from PositionState { round: Dec, shares: Dec, owner_class: u8, owner_key: Hex }
    address owner_address = (owner_class, owner_key);
}

/// The role names (the indexer API's and `bundle/1`'s).
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RoleName {
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

impl RoleName {
    /// The name of a role (the inverse of `role`).
    pub fn of(r: lottery::index::Role) -> Self {
        use lottery::index::Role as R;
        match r {
            R::Controller => RoleName::Controller,
            R::Params => RoleName::Params,
            R::Shard => RoleName::Shard,
            R::Marker => RoleName::Marker,
            R::Drawer => RoleName::Drawer,
            R::Vault => RoleName::Vault,
            R::Pool => RoleName::Pool,
            R::Ticket => RoleName::Ticket,
            R::Position => RoleName::Position,
            R::Request => RoleName::Request,
            R::Record => RoleName::Record,
        }
    }

    pub fn role(self) -> lottery::index::Role {
        use lottery::index::Role as R;
        match self {
            RoleName::Controller => R::Controller,
            RoleName::Params => R::Params,
            RoleName::Shard => R::Shard,
            RoleName::Marker => R::Marker,
            RoleName::Drawer => R::Drawer,
            RoleName::Vault => R::Vault,
            RoleName::Pool => R::Pool,
            RoleName::Ticket => R::Ticket,
            RoleName::Position => R::Position,
            RoleName::Request => R::Request,
            RoleName::Record => R::Record,
        }
    }
}

/// A decoded state, tagged by its role.
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "role", rename_all = "camelCase")]
pub enum StateView {
    Controller(ControllerView),
    Params(ParamsView),
    Shard(ShardView),
    Marker(MarkerView),
    Drawer(DrawerView),
    Vault(VaultView),
    Pool(PoolView),
    Ticket(TicketView),
    Position(PositionView),
    Request(RequestView),
    Record(RecordView),
}

impl StateView {
    pub fn of(s: &State, net: Net) -> Self {
        match s {
            State::Controller(x) => StateView::Controller(ControllerView::of(x, net)),
            State::Params(x) => StateView::Params(ParamsView::of(x, net)),
            State::Shard(x) => StateView::Shard(ShardView::of(x, net)),
            State::Marker(x) => StateView::Marker(MarkerView::of(x, net)),
            State::Drawer(x) => StateView::Drawer(DrawerView::of(x, net)),
            State::Vault(x) => StateView::Vault(VaultView::of(x, net)),
            State::Pool(x) => StateView::Pool(PoolView::of(x, net)),
            State::Ticket(x) => StateView::Ticket(TicketView::of(x, net)),
            State::Position(x) => StateView::Position(PositionView::of(x, net)),
            State::Request(x) => StateView::Request(RequestView::of(x, net)),
            State::Record(x) => StateView::Record(RecordView::of(x, net)),
        }
    }
}
