//! Shard and ticket states (spec M1 section 4), as SilverScript runtime-state values.

use std::collections::BTreeMap;

use harness::{ArtifactValue, ScriptPublicKey};

use crate::combo::{Combo, bundle_bytes};

pub const ROLE_SHARD: u8 = 0x02;
pub const ROLE_TICKET: u8 = 0x05;

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

    /// Constructor arguments after the ticket template hash.
    pub fn ctor(&self, ticket_hash: [u8; 32]) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        vec![
            Bytes(ticket_hash.to_vec()),
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

    pub fn ctor(&self) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        vec![
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
