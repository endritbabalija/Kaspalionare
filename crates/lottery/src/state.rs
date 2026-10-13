//! Protocol states as SilverScript runtime-state values: shard and ticket (spec M1
//! section 4), controller, params and marker (spec M2 A3), drawer, vault and pool (spec M3
//! A3), and the vault request, settlement record and share position (spec M5 A3).

use std::collections::BTreeMap;

use harness::{ArtifactValue, ScriptPublicKey};

use crate::combo::{Combo, bundle_bytes};

pub const ROLE_CONTROLLER: u8 = 0x01;
pub const ROLE_SHARD: u8 = 0x02;
pub const ROLE_MARKER: u8 = 0x03;
pub const ROLE_POOL: u8 = 0x04;
pub const ROLE_TICKET: u8 = 0x05;
pub const ROLE_VAULT: u8 = 0x06;
pub const ROLE_REQUEST: u8 = 0x07;
pub const ROLE_RECORD: u8 = 0x08;
pub const ROLE_PARAMS: u8 = 0x09;
pub const ROLE_DRAWER: u8 = 0x0A;
/// Share position (spec M5 A2, D67; new role).
pub const ROLE_POSITION: u8 = 0x0B;
/// Request kinds (spec M5 A2, A3.2).
pub const KIND_DEPOSIT: u8 = 0;
pub const KIND_WITHDRAW: u8 = 1;
/// Largest K (spec M2 A2); the marker stores 32 x K_MAX root bytes.
pub const K_MAX: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardState {
    pub round: i64,
    pub shard_index: i64,
    pub m: i64,
    pub price: i64,
    pub d: i64,
    pub s_r: i64,
    pub deadline: i64,
    pub root: [u8; 32],
    pub sales: i64,
    pub ref_paid: i64,
}

impl ShardState {
    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        use ArtifactValue::*;
        BTreeMap::from([
            ("role".into(), Byte(ROLE_SHARD)),
            ("round".into(), Int(self.round)),
            ("shard_index".into(), Int(self.shard_index)),
            ("m".into(), Int(self.m)),
            ("price".into(), Int(self.price)),
            ("d".into(), Int(self.d)),
            ("s_r".into(), Int(self.s_r)),
            ("deadline".into(), Int(self.deadline)),
            ("root".into(), Bytes(self.root.to_vec())),
            ("sales".into(), Int(self.sales)),
            ("ref_paid".into(), Int(self.ref_paid)),
        ])
    }

    /// Constructor arguments: the ticket and controller template hashes (constants), then
    /// the state.
    pub fn ctor(&self, ticket_hash: [u8; 32], controller_hash: [u8; 32]) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        vec![
            Bytes(ticket_hash.to_vec()),
            Bytes(controller_hash.to_vec()),
            Int(self.round),
            Int(self.shard_index),
            Int(self.m),
            Int(self.price),
            Int(self.d),
            Int(self.s_r),
            Int(self.deadline),
            Bytes(self.root.to_vec()),
            Int(self.sales),
            Int(self.ref_paid),
        ]
    }
}

/// A standard payout or referral script (spec M1 D11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payout {
    Schnorr([u8; 32]),
    Ecdsa([u8; 33]),
    P2sh([u8; 32]),
}

impl Payout {
    pub fn class(&self) -> u8 {
        match self {
            Payout::Schnorr(_) => 0,
            Payout::Ecdsa(_) => 1,
            Payout::P2sh(_) => 2,
        }
    }

    /// 33-byte key field: 32-byte payloads are followed by one zero byte.
    pub fn key33(&self) -> [u8; 33] {
        let mut k = [0u8; 33];
        match self {
            Payout::Schnorr(x) | Payout::P2sh(x) => k[..32].copy_from_slice(x),
            Payout::Ecdsa(x) => k.copy_from_slice(x),
        }
        k
    }

    pub fn script(&self) -> Vec<u8> {
        match self {
            Payout::Schnorr(x) => [&[0x20][..], x, &[0xac]].concat(),
            Payout::Ecdsa(x) => [&[0x21][..], x, &[0xab]].concat(),
            Payout::P2sh(x) => [&[0xaa, 0x20][..], x, &[0x87]].concat(),
        }
    }

    pub fn spk(&self) -> ScriptPublicKey {
        ScriptPublicKey::new(0, self.script().into())
    }

    /// The payout a (class, 33-byte key) state field pair describes (D11). Panics on a
    /// class above 2.
    pub fn from_fields(class: u8, key: &[u8; 33]) -> Self {
        let k32: [u8; 32] = key[..32].try_into().unwrap();
        match class {
            0 => Payout::Schnorr(k32),
            1 => Payout::Ecdsa(*key),
            2 => Payout::P2sh(k32),
            c => panic!("payout class {c} is not a D11 standard script"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TicketState {
    pub round: i64,
    pub deadline: i64,
    pub b: i64,
    /// 6 x B_MAX bytes.
    pub bundle: Vec<u8>,
    /// ceil(B_MAX / 8) bytes.
    pub claimed: Vec<u8>,
    pub payout_class: u8,
    pub payout_key: [u8; 33],
}

impl TicketState {
    pub fn new(shard: &ShardState, bundle: &[Combo], payout: &Payout, b_max: usize) -> Self {
        let mut bytes = bundle_bytes(bundle);
        bytes.resize(6 * b_max, 0);
        Self {
            round: shard.round,
            deadline: shard.deadline,
            b: bundle.len() as i64,
            bundle: bytes,
            claimed: vec![0; b_max.div_ceil(8)],
            payout_class: payout.class(),
            payout_key: payout.key33(),
        }
    }

    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        use ArtifactValue::*;
        BTreeMap::from([
            ("role".into(), Byte(ROLE_TICKET)),
            ("round".into(), Int(self.round)),
            ("deadline".into(), Int(self.deadline)),
            ("b".into(), Int(self.b)),
            ("bundle".into(), Bytes(self.bundle.clone())),
            ("claimed".into(), Bytes(self.claimed.clone())),
            ("payout_class".into(), Byte(self.payout_class)),
            ("payout_key".into(), Bytes(self.payout_key.to_vec())),
        ])
    }

    /// Constructor arguments: the pool template hash (constant POOL_TPL, spec M4 A2, D54),
    /// then the state.
    pub fn ctor(&self, pool_hash: [u8; 32]) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        vec![
            Bytes(pool_hash.to_vec()),
            Int(self.round),
            Int(self.deadline),
            Int(self.b),
            Bytes(self.bundle.clone()),
            Bytes(self.claimed.clone()),
            Byte(self.payout_class),
            Bytes(self.payout_key.to_vec()),
        ]
    }
}

/// The five lower-tier shares s_t in the fixed slot order (4+b, 4, 3+b, 3, 2+b) (D18).
pub type TierShares = [i64; 5];

/// Round controller, role 0x01 (spec M2 A3.1). Field order = byte order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControllerState {
    pub round: i64,
    pub k: i64,
    pub m: i64,
    pub price: i64,
    pub d: i64,
    pub deadline: i64,
    pub s_f: i64,
    pub s_r: i64,
    pub fee_class: u8,
    pub fee_key: [u8; 33],
    pub jackpot: i64,
    pub j_boost: i64,
    pub g2: i64,
    pub liability: i64,
    pub v_rec: i64,
    pub acc: i64,
    pub drawn: u8,
    pub shard_tpl: [u8; 32],
    pub marker_tpl: [u8; 32],
    pub params_tpl: [u8; 32],
    pub reserve: i64,
    pub s_c: i64,
    pub s_d: i64,
    pub st: TierShares,
    pub e: i64,
    pub j: i64,
    pub g: i64,
    pub margin: i64,
    pub m_min: i64,
    pub m_max: i64,
    pub cap: i64,
    pub round_len: i64,
    pub d_min: i64,
    pub seed_offset: i64,
}

impl ControllerState {
    pub fn set_fee(&mut self, p: &Payout) {
        self.fee_class = p.class();
        self.fee_key = p.key33();
    }

    /// (name, value) pairs in state order, role first.
    fn fields(&self) -> Vec<(&'static str, ArtifactValue)> {
        use ArtifactValue::*;
        vec![
            ("role", Byte(ROLE_CONTROLLER)),
            ("round", Int(self.round)),
            ("k", Int(self.k)),
            ("m", Int(self.m)),
            ("price", Int(self.price)),
            ("d", Int(self.d)),
            ("deadline", Int(self.deadline)),
            ("s_f", Int(self.s_f)),
            ("s_r", Int(self.s_r)),
            ("fee_class", Byte(self.fee_class)),
            ("fee_key", Bytes(self.fee_key.to_vec())),
            ("jackpot", Int(self.jackpot)),
            ("j_boost", Int(self.j_boost)),
            ("g2", Int(self.g2)),
            ("liability", Int(self.liability)),
            ("v_rec", Int(self.v_rec)),
            ("acc", Int(self.acc)),
            ("drawn", Byte(self.drawn)),
            ("shard_tpl", Bytes(self.shard_tpl.to_vec())),
            ("marker_tpl", Bytes(self.marker_tpl.to_vec())),
            ("params_tpl", Bytes(self.params_tpl.to_vec())),
            ("reserve", Int(self.reserve)),
            ("s_c", Int(self.s_c)),
            ("s_d", Int(self.s_d)),
            ("st1", Int(self.st[0])),
            ("st2", Int(self.st[1])),
            ("st3", Int(self.st[2])),
            ("st4", Int(self.st[3])),
            ("st5", Int(self.st[4])),
            ("e", Int(self.e)),
            ("j", Int(self.j)),
            ("g", Int(self.g)),
            ("margin", Int(self.margin)),
            ("m_min", Int(self.m_min)),
            ("m_max", Int(self.m_max)),
            ("cap", Int(self.cap)),
            ("round_len", Int(self.round_len)),
            ("d_min", Int(self.d_min)),
            ("seed_offset", Int(self.seed_offset)),
        ]
    }

    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        self.fields().into_iter().map(|(n, v)| (n.to_string(), v)).collect()
    }

    /// Constructor arguments: every field after the role, in order.
    pub fn ctor(&self) -> Vec<ArtifactValue> {
        self.fields().into_iter().skip(1).map(|(_, v)| v).collect()
    }
}

/// Parameter UTXO, role 0x09 (spec M2 A3.2, D16).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamsState {
    pub round: i64,
    /// blake2b of the 32-byte x-only admin pubkey (D34).
    pub admin_hash: [u8; 32],
    pub nonce: i64,
    pub used: u8,
    pub next_s_f: i64,
    pub next_s_r: i64,
    pub next_fee_class: u8,
    pub next_fee_key: [u8; 33],
}

impl ParamsState {
    pub fn set_next_fee(&mut self, p: &Payout) {
        self.next_fee_class = p.class();
        self.next_fee_key = p.key33();
    }

    fn fields(&self) -> Vec<(&'static str, ArtifactValue)> {
        use ArtifactValue::*;
        vec![
            ("role", Byte(ROLE_PARAMS)),
            ("round", Int(self.round)),
            ("admin_hash", Bytes(self.admin_hash.to_vec())),
            ("nonce", Int(self.nonce)),
            ("used", Byte(self.used)),
            ("next_s_f", Int(self.next_s_f)),
            ("next_s_r", Int(self.next_s_r)),
            ("next_fee_class", Byte(self.next_fee_class)),
            ("next_fee_key", Bytes(self.next_fee_key.to_vec())),
        ]
    }

    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        self.fields().into_iter().map(|(n, v)| (n.to_string(), v)).collect()
    }

    /// Constructor arguments: the controller template hash (constant), then the state.
    pub fn ctor(&self, controller_hash: [u8; 32]) -> Vec<ArtifactValue> {
        let mut v = vec![ArtifactValue::Bytes(controller_hash.to_vec())];
        v.extend(self.fields().into_iter().skip(1).map(|(_, v)| v));
        v
    }
}

/// Frozen marker, role 0x03 (spec M2 A3.3; layout unchanged in M3, A3.4; spec M7 A3/D99: the
/// `seed_ref` field between st5 and funder_class is removed). Its code has the `draw_leg`
/// delegate and the drawer template hash as a constructor constant (D38).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkerState {
    pub round: i64,
    pub k: i64,
    /// 32 x K_MAX bytes; roots i >= k are all zero.
    pub roots: Vec<u8>,
    pub sales: i64,
    pub ref_paid: i64,
    pub m: i64,
    pub jackpot: i64,
    pub j_boost: i64,
    pub g2: i64,
    pub s_f: i64,
    pub s_r: i64,
    pub s_c: i64,
    pub s_d: i64,
    pub st: TierShares,
    pub funder_class: u8,
    pub funder_key: [u8; 33],
}

impl MarkerState {
    /// Root bytes for the given shard roots: concatenated, zero-padded to 32 x K_MAX.
    pub fn roots_bytes(roots: &[[u8; 32]]) -> Vec<u8> {
        Self::roots_bytes_k(roots, K_MAX)
    }

    /// As `roots_bytes` for templates compiled with another K_MAX (spec M6 A5 M6-b: the
    /// in-memory K_MAX variants, whose marker and pool hold `byte[32 x k_max]` roots).
    pub fn roots_bytes_k(roots: &[[u8; 32]], k_max: usize) -> Vec<u8> {
        assert!(roots.len() <= k_max, "at most K_MAX roots");
        let mut b: Vec<u8> = roots.concat();
        b.resize(32 * k_max, 0);
        b
    }

    pub fn set_funder(&mut self, p: &Payout) {
        self.funder_class = p.class();
        self.funder_key = p.key33();
    }

    fn fields(&self) -> Vec<(&'static str, ArtifactValue)> {
        use ArtifactValue::*;
        vec![
            ("role", Byte(ROLE_MARKER)),
            ("round", Int(self.round)),
            ("k", Int(self.k)),
            ("roots", Bytes(self.roots.clone())),
            ("sales", Int(self.sales)),
            ("ref_paid", Int(self.ref_paid)),
            ("m", Int(self.m)),
            ("jackpot", Int(self.jackpot)),
            ("j_boost", Int(self.j_boost)),
            ("g2", Int(self.g2)),
            ("s_f", Int(self.s_f)),
            ("s_r", Int(self.s_r)),
            ("s_c", Int(self.s_c)),
            ("s_d", Int(self.s_d)),
            ("st1", Int(self.st[0])),
            ("st2", Int(self.st[1])),
            ("st3", Int(self.st[2])),
            ("st4", Int(self.st[3])),
            ("st5", Int(self.st[4])),
            ("funder_class", Byte(self.funder_class)),
            ("funder_key", Bytes(self.funder_key.to_vec())),
        ]
    }

    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        self.fields().into_iter().map(|(n, v)| (n.to_string(), v)).collect()
    }

    /// Constructor arguments: the drawer template hash (constant, D38), then the state.
    pub fn ctor(&self, drawer_hash: [u8; 32]) -> Vec<ArtifactValue> {
        let mut v = vec![ArtifactValue::Bytes(drawer_hash.to_vec())];
        v.extend(self.fields().into_iter().skip(1).map(|(_, v)| v));
        v
    }
}

/// Drawer singleton, role 0x0A (spec M3 A3.1, D37, D49). Value R.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawerState {
    /// The round this drawer draws next.
    pub round: i64,
    pub pool_tpl: [u8; 32],
    pub vault_tpl: [u8; 32],
    /// The ticket template hash (spec M4 A3.2, D54): an instance constant set at genesis,
    /// copied unchanged by draw() and written into every new pool.
    pub ticket_tpl: [u8; 32],
    pub f_min: i64,
    pub claim_window: i64,
    pub draw_window: i64,
}

impl DrawerState {
    fn fields(&self) -> Vec<(&'static str, ArtifactValue)> {
        use ArtifactValue::*;
        vec![
            ("role", Byte(ROLE_DRAWER)),
            ("round", Int(self.round)),
            ("pool_tpl", Bytes(self.pool_tpl.to_vec())),
            ("vault_tpl", Bytes(self.vault_tpl.to_vec())),
            ("ticket_tpl", Bytes(self.ticket_tpl.to_vec())),
            ("f_min", Int(self.f_min)),
            ("claim_window", Int(self.claim_window)),
            ("draw_window", Int(self.draw_window)),
        ]
    }

    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        self.fields().into_iter().map(|(n, v)| (n.to_string(), v)).collect()
    }

    /// Constructor arguments: the controller template hash (constant, D38), then the state.
    pub fn ctor(&self, controller_hash: [u8; 32]) -> Vec<ArtifactValue> {
        let mut v = vec![ArtifactValue::Bytes(controller_hash.to_vec())];
        v.extend(self.fields().into_iter().skip(1).map(|(_, v)| v));
        v
    }
}

/// House vault, role 0x06 (spec M3 A3.2, design 6, D40; extended by spec M5 A3.1, D74).
/// Value total_assets + pend_dep + R.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultState {
    /// The last drawn round (0 at genesis).
    pub round: i64,
    pub total_assets: i64,
    pub total_shares: i64,
    pub pend_dep: i64,
    pub pend_wsh: i64,
    pub liability: i64,
    /// Instance constants (D74): the record, request and position template hashes, set at
    /// genesis and copied unchanged.
    pub record_tpl: [u8; 32],
    pub request_tpl: [u8; 32],
    pub position_tpl: [u8; 32],
}

impl VaultState {
    fn fields(&self) -> Vec<(&'static str, ArtifactValue)> {
        use ArtifactValue::*;
        vec![
            ("role", Byte(ROLE_VAULT)),
            ("round", Int(self.round)),
            ("total_assets", Int(self.total_assets)),
            ("total_shares", Int(self.total_shares)),
            ("pend_dep", Int(self.pend_dep)),
            ("pend_wsh", Int(self.pend_wsh)),
            ("liability", Int(self.liability)),
            ("record_tpl", Bytes(self.record_tpl.to_vec())),
            ("request_tpl", Bytes(self.request_tpl.to_vec())),
            ("position_tpl", Bytes(self.position_tpl.to_vec())),
        ]
    }

    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        self.fields().into_iter().map(|(n, v)| (n.to_string(), v)).collect()
    }

    /// Constructor arguments: the drawer template hash (constant, D38), then the state.
    pub fn ctor(&self, drawer_hash: [u8; 32]) -> Vec<ArtifactValue> {
        let mut v = vec![ArtifactValue::Bytes(drawer_hash.to_vec())];
        v.extend(self.fields().into_iter().skip(1).map(|(_, v)| v));
        v
    }
}

/// Tier codes (spec M3 A2): 0 jackpot, 1 second prize, 2..6 the lower slots
/// (4+b, 4, 3+b, 3, 2+b).
pub const TIER_JACKPOT: u8 = 0;
pub const TIER_SECOND: u8 = 1;
/// Tier code of lower slot 0 (4+b); slot t has code TIER_LOWER0 + t.
pub const TIER_LOWER0: u8 = 2;

/// Prize pool, role 0x04 (spec M3 A3.3, with ticket_tpl and reserve from spec M4 A3.3).
/// Value remaining + R.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolState {
    pub round: i64,
    pub tier: u8,
    /// J, G2 or slice_t.
    pub slice: i64,
    /// n1..n5, bonus.
    pub win: [u8; 6],
    pub m: i64,
    pub k: i64,
    /// 32 x K_MAX bytes, as the marker.
    pub roots: Vec<u8>,
    pub deadline: i64,
    /// The ticket template hash, copied from the drawer state by draw() (D54).
    pub ticket_tpl: [u8; 32],
    /// R (= ctrl.reserve), written by draw() (D55).
    pub reserve: i64,
    pub funder_class: u8,
    pub funder_key: [u8; 33],
}

impl PoolState {
    pub fn set_funder(&mut self, p: &Payout) {
        self.funder_class = p.class();
        self.funder_key = p.key33();
    }

    fn fields(&self) -> Vec<(&'static str, ArtifactValue)> {
        use ArtifactValue::*;
        vec![
            ("role", Byte(ROLE_POOL)),
            ("round", Int(self.round)),
            ("tier", Byte(self.tier)),
            ("slice", Int(self.slice)),
            ("win", Bytes(self.win.to_vec())),
            ("m", Int(self.m)),
            ("k", Int(self.k)),
            ("roots", Bytes(self.roots.clone())),
            ("deadline", Int(self.deadline)),
            ("ticket_tpl", Bytes(self.ticket_tpl.to_vec())),
            ("reserve", Int(self.reserve)),
            ("funder_class", Byte(self.funder_class)),
            ("funder_key", Bytes(self.funder_key.to_vec())),
        ]
    }

    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        self.fields().into_iter().map(|(n, v)| (n.to_string(), v)).collect()
    }

    /// Constructor arguments: the drawer template hash (constant, D38), then the state.
    pub fn ctor(&self, drawer_hash: [u8; 32]) -> Vec<ArtifactValue> {
        let mut v = vec![ArtifactValue::Bytes(drawer_hash.to_vec())];
        v.extend(self.fields().into_iter().skip(1).map(|(_, v)| v));
        v
    }
}

/// Vault request, role 0x07 (spec M5 A3.2, D67, D68). Value R + accrued (0 for deposits).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestState {
    /// The target draw (the vault's round + 1 at creation).
    pub round: i64,
    /// KIND_DEPOSIT or KIND_WITHDRAW.
    pub kind: u8,
    /// Deposit sompi or withdrawal shares.
    pub amount: i64,
    pub owner_class: u8,
    pub owner_key: [u8; 33],
}

impl RequestState {
    pub fn new(round: i64, kind: u8, amount: i64, owner: &Payout) -> Self {
        Self { round, kind, amount, owner_class: owner.class(), owner_key: owner.key33() }
    }

    fn fields(&self) -> Vec<(&'static str, ArtifactValue)> {
        use ArtifactValue::*;
        vec![
            ("role", Byte(ROLE_REQUEST)),
            ("round", Int(self.round)),
            ("kind", Byte(self.kind)),
            ("amount", Int(self.amount)),
            ("owner_class", Byte(self.owner_class)),
            ("owner_key", Bytes(self.owner_key.to_vec())),
        ]
    }

    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        self.fields().into_iter().map(|(n, v)| (n.to_string(), v)).collect()
    }

    /// Constructor arguments: the record and position template hashes (constants
    /// RECORD_TPL, POSITION_TPL, D74), then the state.
    pub fn ctor(&self, record_hash: [u8; 32], position_hash: [u8; 32]) -> Vec<ArtifactValue> {
        let mut v = vec![ArtifactValue::Bytes(record_hash.to_vec()), ArtifactValue::Bytes(position_hash.to_vec())];
        v.extend(self.fields().into_iter().skip(1).map(|(_, v)| v));
        v
    }
}

/// Settlement record, role 0x08 (spec M5 A3.3, D65, D72). Value out_rem + refund x dep_rem
/// + R.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordState {
    /// The draw that created it.
    pub round: i64,
    /// The request template hash, copied by draw() from the vault state (D74).
    pub request_tpl: [u8; 32],
    /// 1 = refund mode (D70): every deposit settlement repays its deposit.
    pub refund: u8,
    pub dep_rem: i64,
    pub mint_rem: i64,
    pub wsh_rem: i64,
    pub burn_rem: i64,
    pub out_rem: i64,
    pub funder_class: u8,
    pub funder_key: [u8; 33],
}

impl RecordState {
    pub fn set_funder(&mut self, p: &Payout) {
        self.funder_class = p.class();
        self.funder_key = p.key33();
    }

    fn fields(&self) -> Vec<(&'static str, ArtifactValue)> {
        use ArtifactValue::*;
        vec![
            ("role", Byte(ROLE_RECORD)),
            ("round", Int(self.round)),
            ("request_tpl", Bytes(self.request_tpl.to_vec())),
            ("refund", Byte(self.refund)),
            ("dep_rem", Int(self.dep_rem)),
            ("mint_rem", Int(self.mint_rem)),
            ("wsh_rem", Int(self.wsh_rem)),
            ("burn_rem", Int(self.burn_rem)),
            ("out_rem", Int(self.out_rem)),
            ("funder_class", Byte(self.funder_class)),
            ("funder_key", Bytes(self.funder_key.to_vec())),
        ]
    }

    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        self.fields().into_iter().map(|(n, v)| (n.to_string(), v)).collect()
    }

    /// Constructor arguments: the state (the record has no constructor constant, D74).
    pub fn ctor(&self) -> Vec<ArtifactValue> {
        self.fields().into_iter().skip(1).map(|(_, v)| v).collect()
    }
}

/// Share position, role 0x0B (spec M5 A3.4, D66, D67). Value R.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionState {
    /// The round of the draw that minted it (0 at genesis); informational.
    pub round: i64,
    pub shares: i64,
    pub owner_class: u8,
    pub owner_key: [u8; 33],
}

impl PositionState {
    pub fn new(round: i64, shares: i64, owner: &Payout) -> Self {
        Self { round, shares, owner_class: owner.class(), owner_key: owner.key33() }
    }

    fn fields(&self) -> Vec<(&'static str, ArtifactValue)> {
        use ArtifactValue::*;
        vec![
            ("role", Byte(ROLE_POSITION)),
            ("round", Int(self.round)),
            ("shares", Int(self.shares)),
            ("owner_class", Byte(self.owner_class)),
            ("owner_key", Bytes(self.owner_key.to_vec())),
        ]
    }

    pub fn values(&self) -> BTreeMap<String, ArtifactValue> {
        self.fields().into_iter().map(|(n, v)| (n.to_string(), v)).collect()
    }

    /// Constructor arguments: the vault template hash (constant VAULT_TPL, D74), then the
    /// state.
    pub fn ctor(&self, vault_hash: [u8; 32]) -> Vec<ArtifactValue> {
        let mut v = vec![ArtifactValue::Bytes(vault_hash.to_vec())];
        v.extend(self.fields().into_iter().skip(1).map(|(_, v)| v));
        v
    }
}
