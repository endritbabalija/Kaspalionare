//! Transaction builder for `set_params()` (spec M2 A5) and admin signing (D34).
//!
//! Shape: input 0 = params; inputs 1..n = admin funding (zero ID); output 0 = params
//! successor (authorized by input 0); output 1 = optional change (zero ID).
//!
//! The admin signs Schnorr over blake2b(message), where message =
//! "KLOT/set_params" (15 B) || cov (32) || target round (8 LE) || s_F (8 LE) || s_R (8 LE)
//! || fee_class (1) || fee_key (33) || nonce (8 LE) = 113 bytes. The script verifies it with
//! `checkMsgSig` (OpCheckSigFromStack, rusty-kaspa v2.1.0 `crypto/txscript/src/opcodes/mod.rs:1438`,
//! which verifies a BIP-340 signature over the 32-byte digest as given). The stored key is
//! blake2b(32-byte x-only pubkey).

use std::sync::Arc;

use harness::{ArtifactValue, CovenantBinding, Hash, TransactionOutput, UtxoEntry, outpoint};
use secp256k1::{Keypair, Message, SECP256K1};

use crate::buy::{Built, COV, FUNDING_SK, In, InKind, KAS, Templates, b_max, spk_of};
use crate::state::{ParamsState, Payout};

/// Domain tag of the admin message (D34).
pub const DOMAIN: &[u8; 15] = b"KLOT/set_params";
/// Test admin key. Test-only; never used on any network.
pub const ADMIN_SK: [u8; 32] = [0x0a; 32];
/// Reserve R of the instance and every test fixture: 1.69 KAS (M2 Stage 4, M2-c).
///
/// R >= 4C(K + 3) / (0.9 L) with C = 10^12, L = 500,000 and K = K_MAX = 16, i.e.
/// 168,888,889 sompi, rounded up to 0.01 KAS. That is the supremum of a `close()`'s storage
/// mass over every state A3 allows (any sales, boost pot, J_boost, acc and caller output of at
/// least `close::MIN_CALLER_OUTPUT`): at most K + 3 covenant outputs (plurality 2) sit at R
/// and the input relief is positive (KIP-9, rusty-kaspa v2.1.0
/// `consensus/core/src/mass/mod.rs:439-512`). It keeps every `close()` at or below 90 % of the
/// storage limit; larger R only lowers the mass of `buy()` and `set_params()`.
pub const TEST_RESERVE: u64 = 169_000_000;

/// 32-byte x-only public key of a secret key.
pub fn xonly(sk: &[u8; 32]) -> [u8; 32] {
    Keypair::from_seckey_slice(SECP256K1, sk).expect("valid key").x_only_public_key().0.serialize()
}

/// blake2b-256 (unkeyed), as `OpBlake2b` (rusty-kaspa v2.1.0 `opcodes/mod.rs:797-801`).
pub fn blake2b(data: &[u8]) -> [u8; 32] {
    let h = blake2b_simd::Params::new().hash_length(32).hash(data);
    h.as_bytes().try_into().expect("32 bytes")
}

/// The stored admin key: blake2b(x-only pubkey) (D34).
pub fn admin_hash(sk: &[u8; 32]) -> [u8; 32] {
    blake2b(&xonly(sk))
}

/// The signed fields of a `set_params()` call (D34).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParamsMsg {
    pub cov: Hash,
    /// Round the values apply to: the params' round + 1.
    pub target_round: i64,
    pub s_f: i64,
    pub s_r: i64,
    pub fee_class: u8,
    pub fee_key: [u8; 33],
    pub nonce: i64,
}

impl ParamsMsg {
    pub fn bytes(&self) -> Vec<u8> {
        let mut m = Vec::with_capacity(113);
        m.extend_from_slice(DOMAIN);
        m.extend_from_slice(&self.cov.as_bytes());
        m.extend_from_slice(&self.target_round.to_le_bytes());
        m.extend_from_slice(&self.s_f.to_le_bytes());
        m.extend_from_slice(&self.s_r.to_le_bytes());
        m.push(self.fee_class);
        m.extend_from_slice(&self.fee_key);
        m.extend_from_slice(&self.nonce.to_le_bytes());
        debug_assert_eq!(m.len(), 113);
        m
    }

    pub fn digest(&self) -> [u8; 32] {
        blake2b(&self.bytes())
    }

    /// 64-byte BIP-340 signature over the digest (deterministic, no aux randomness).
    pub fn sign(&self, sk: &[u8; 32]) -> [u8; 64] {
        let kp = Keypair::from_seckey_slice(SECP256K1, sk).expect("valid key");
        SECP256K1.sign_schnorr_no_aux_rand(&Message::from_digest(self.digest()), &kp).serialize()
    }
}

/// A `set_params()` call, before it becomes a transaction.
#[derive(Clone)]
pub struct SetParamsCase {
    pub b_max: usize,
    /// The spent params state.
    pub params: ParamsState,
    /// Value of the params input (R).
    pub value: u64,
    /// Creation DAA score of the params UTXO.
    pub params_daa: u64,
    // Arguments.
    pub s_f: i64,
    pub s_r: i64,
    pub fee_class: u8,
    pub fee_key: [u8; 33],
    pub nonce: i64,
    /// Key whose x-only pubkey is passed as `admin_pk`.
    pub admin_sk: [u8; 32],
    /// Key that signs (SA4 uses another key).
    pub signer_sk: [u8; 32],
    /// Signed message override (SA7-SA9); `None` signs the message matching the arguments.
    pub msg: Option<ParamsMsg>,
    /// Admin funding input values (zero ID).
    pub funding: Vec<u64>,
    /// Whether output 1 (change) exists; without it funding must equal the fee.
    pub change: bool,
    pub fee: u64,
    pub cov: Hash,
    pub funding_sk: [u8; 32],
}

impl SetParamsCase {
    /// A valid change in round 1: s_F 7 %, s_R 12 %, new fee address, nonce stored + 1,
    /// with change.
    pub fn new() -> Self {
        let params = ParamsState {
            round: 1,
            admin_hash: admin_hash(&ADMIN_SK),
            nonce: 0,
            used: 0,
            next_s_f: 500,
            next_s_r: 1_000,
            next_fee_class: 0,
            next_fee_key: Payout::Schnorr([0x44; 32]).key33(),
        };
        let fee = Payout::Schnorr([0x55; 32]);
        Self {
            b_max: b_max(),
            nonce: params.nonce + 1,
            params,
            value: TEST_RESERVE,
            params_daa: 5_000,
            s_f: 700,
            s_r: 1_200,
            fee_class: fee.class(),
            fee_key: fee.key33(),
            admin_sk: ADMIN_SK,
            signer_sk: ADMIN_SK,
            msg: None,
            funding: vec![10 * KAS],
            change: true,
            fee: KAS / 10,
            cov: COV,
            funding_sk: FUNDING_SK,
        }
    }

    pub fn set_fee(&mut self, p: &Payout) {
        self.fee_class = p.class();
        self.fee_key = p.key33();
    }

    /// The message matching the arguments (D34), target round = params round + 1.
    pub fn honest_msg(&self) -> ParamsMsg {
        ParamsMsg {
            cov: self.cov,
            target_round: self.params.round + 1,
            s_f: self.s_f,
            s_r: self.s_r,
            fee_class: self.fee_class,
            fee_key: self.fee_key,
            nonce: self.nonce,
        }
    }

    pub fn signature(&self) -> [u8; 64] {
        self.msg.unwrap_or_else(|| self.honest_msg()).sign(&self.signer_sk)
    }

    /// Expected successor (A5 check 8): nonce, used = 1 and next_* = the arguments.
    pub fn successor(&self) -> ParamsState {
        ParamsState {
            nonce: self.nonce,
            used: 1,
            next_s_f: self.s_f,
            next_s_r: self.s_r,
            next_fee_class: self.fee_class,
            next_fee_key: self.fee_key,
            ..self.params.clone()
        }
    }

    /// set_params(s_f, s_r, fee_class, fee_key, nonce, admin_pk, sig).
    pub fn args(&self) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        vec![
            Int(self.s_f),
            Int(self.s_r),
            Byte(self.fee_class),
            Bytes(self.fee_key.to_vec()),
            Int(self.nonce),
            Bytes(xonly(&self.admin_sk).to_vec()),
            Bytes(self.signature().to_vec()),
        ]
    }

    pub fn build(&self) -> Built {
        self.build_with(Templates::get(self.b_max))
    }

    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        let redeem = t.params_redeem(&self.params);
        let mut inputs = vec![In {
            entry: UtxoEntry::new(self.value, t.params_spk(&self.params), self.params_daa, false, Some(self.cov)),
            kind: InKind::Entry { template: t.params.clone(), entry: "set_params".into(), args: self.args(), redeem },
        }];
        for &v in &self.funding {
            inputs.push(In { entry: UtxoEntry::new(v, spk_of(&self.funding_sk), 0, false, None), kind: InKind::Funding });
        }
        let mut outputs =
            vec![TransactionOutput::with_covenant(self.value, t.params_spk(&self.successor()), Some(CovenantBinding::new(0, self.cov)))];
        let funding: u64 = self.funding.iter().sum();
        let change = funding.checked_sub(self.fee).expect("funding covers the fee");
        if self.change {
            outputs.push(TransactionOutput::new(change, spk_of(&self.funding_sk)));
        } else {
            assert_eq!(change, 0, "no-change case needs exact funding");
        }
        let outpoints = (0..inputs.len() as u32).map(outpoint).collect();
        Built {
            templates: t,
            args: vec![],
            inputs,
            outputs,
            genesis: vec![],
            lock_time: 0,
            shard_budget: None,
            budget_delta: 0,
            outpoints,
            funding_sk: self.funding_sk,
            sim: None,
        }
    }
}

impl Default for SetParamsCase {
    fn default() -> Self {
        Self::new()
    }
}
