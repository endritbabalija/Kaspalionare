//! Transaction builder for `close()` (spec M2 A4).
//!
//! Shape (A4):
//!   inputs : 0 controller r (leader, `close`); 1 params (`close_leg`); 2 + i shard i
//!            (`close_leg`), i < k; k + 2 .. caller funding (zero ID, 1..F_MAX)
//!   outputs: 0 controller r+1; 1 params'; 2 + i new shard i; k + 2 marker r (all
//!            authorized by input 0 under the instance covenant ID); k + 3 caller output
//!            (zero ID) = funding - R(marker) - fee + C_close.
//!
//! `CloseCase` describes a close with public fields; `expected()` derives every successor
//! state and value from the Rust reference (`game::close_math`); `build()` returns an
//! editable `Built`. Tests change one case field (input-side attacks) or one thing in the
//! `Built` (output-side attacks), then `finish()` and run.

use std::sync::Arc;

use harness::{ArtifactValue, CovenantBinding, Hash, TransactionOutput, UtxoEntry, outpoint};

use crate::buy::{Built, COV, FUNDING_SK, In, InKind, KAS, TEST_DEPOSIT, Templates, b_max, spk_of};
use crate::game::{CloseMath, close_math};
use crate::params::{ADMIN_SK, TEST_RESERVE, admin_hash, xonly};
use crate::smt::empty_hashes;
use crate::state::{ControllerState, K_MAX, MarkerState, ParamsState, Payout, ShardState};

/// Instance values of the tests (spec M2 D17-D23).
pub const TEST_ROUND_LEN: i64 = 864_000;
pub const TEST_D_MIN: i64 = 72_000;
pub const TEST_SEED_OFFSET: i64 = 600;
/// Round 1 deadline T in the fixtures.
pub const TEST_DEADLINE: i64 = 1_000_000;
/// The fixture fee address.
pub const TEST_FEE_ADDR: Payout = Payout::Schnorr([0x44; 32]);

/// Builder policy (M2 Stage 4, user decision): the caller output of `close()` is at least
/// 1 KAS. Not a script check: the script fixes the caller output as funding - R - fee +
/// C_close (D29, A4) and the caller chooses its funding. The policy keeps the plain caller
/// output's own storage term at or below C / 1 KAS = 10,000 grams, which the R bound
/// (`params::TEST_RESERVE`) assumes. `CloseCase::build*` refuses a case below it; attack
/// tests that need a smaller caller output edit the `Built` afterwards.
pub const MIN_CALLER_OUTPUT: u64 = KAS;

/// Root of the empty counter tree, Z_18 (spec M1 3; same for every m).
pub fn z18() -> [u8; 32] {
    empty_hashes()[crate::smt::DEPTH]
}

/// A round-r shard spent by `close()`.
#[derive(Clone, Debug)]
pub struct ShardIn {
    pub state: ShardState,
    /// Creation DAA score of the shard UTXO.
    pub daa: u64,
}

/// A caller funding input (zero ID, signed with `funding_sk`).
#[derive(Clone, Copy, Debug)]
pub struct FundingIn {
    pub value: u64,
    pub daa: u64,
}

/// Expected outputs of a valid close (A4 checks 13-24).
#[derive(Clone, Debug)]
pub struct CloseExpected {
    pub math: CloseMath,
    /// The m_next argument used (the case override or the reference).
    pub m_next: i64,
    pub controller: ControllerState,
    pub params: ParamsState,
    pub shards: Vec<ShardState>,
    pub marker: MarkerState,
    /// Output values in order: controller, params, k shards, marker, caller.
    pub values: Vec<u64>,
}

#[derive(Clone)]
pub struct CloseCase {
    pub b_max: usize,
    /// Controller of round r (input 0). `controller.k` must equal `shards.len()` for a
    /// valid case; attacks may break that.
    pub controller: ControllerState,
    /// Boost pot O; the controller input's value is O + acc + reserve.
    pub pot: u64,
    pub controller_daa: u64,
    /// Params of round r (input 1); value = controller.reserve.
    pub params: ParamsState,
    pub params_daa: u64,
    /// Shards of round r, inputs 2..k+1 in order; value S_i - P_R,i + reserve.
    pub shards: Vec<ShardIn>,
    pub funding: Vec<FundingIn>,
    /// Marker reserve funder (argument and marker field).
    pub funder: Payout,
    pub fee: u64,
    /// Transaction lock time; must be >= T (A4 check 7).
    pub lock_time: u64,
    /// m_next argument override; `None` uses the reference.
    pub m_next: Option<i64>,
    pub cov: Hash,
    pub funding_sk: [u8; 32],
    /// K_MAX of the templates the case is built against: the marker' roots are padded to
    /// 32 x k_max bytes. `state::K_MAX` (16) for the committed templates; the M6-b K_MAX
    /// variants (measurement only, D91) set it to the variant's K_MAX.
    pub k_max: usize,
}

impl CloseCase {
    /// A valid zero-sales close of round 1 with `k` fresh shards, a "drawn" controller
    /// fabricated per D33 (J = 1,000,000 KAS, J_boost = 50,000 KAS, boost pot 100,000 KAS,
    /// V_rec = 10,000,000 KAS), the D17-D23 instance values and one funding input.
    pub fn new(k: usize) -> Self {
        Self::new_with(k, &Templates::get(b_max()))
    }

    pub fn new_with(k: usize, t: &Templates) -> Self {
        let reserve = TEST_RESERVE as i64;
        let controller = ControllerState {
            round: 1,
            k: k as i64,
            m: 10,
            price: KAS as i64,
            d: TEST_DEPOSIT as i64,
            deadline: TEST_DEADLINE,
            s_f: 500,
            s_r: 1_000,
            fee_class: TEST_FEE_ADDR.class(),
            fee_key: TEST_FEE_ADDR.key33(),
            jackpot: 1_000_000 * KAS as i64,
            j_boost: 50_000 * KAS as i64,
            g2: 10_000 * KAS as i64,
            liability: 1_090_000 * KAS as i64,
            v_rec: 10_000_000 * KAS as i64,
            acc: 2 * KAS as i64,
            drawn: 1,
            shard_tpl: t.shard.hash,
            marker_tpl: t.marker.hash,
            params_tpl: t.params.hash,
            reserve,
            s_c: 50,
            s_d: 50,
            st: [400, 300, 300, 0, 0],
            e: 1_500,
            j: 1_000,
            g: 100,
            margin: 11_700,
            m_min: 2,
            m_max: 50,
            cap: 2_000,
            round_len: TEST_ROUND_LEN,
            d_min: TEST_D_MIN,
            seed_offset: TEST_SEED_OFFSET,
        };
        let params = ParamsState {
            round: 1,
            admin_hash: admin_hash(&ADMIN_SK),
            nonce: 0,
            used: 0,
            next_s_f: controller.s_f,
            next_s_r: controller.s_r,
            next_fee_class: controller.fee_class,
            next_fee_key: controller.fee_key,
        };
        let shards = (0..k)
            .map(|i| ShardIn {
                state: ShardState {
                    round: 1,
                    shard_index: i as i64,
                    m: controller.m,
                    price: controller.price,
                    d: controller.d,
                    s_r: controller.s_r,
                    deadline: controller.deadline,
                    root: z18(),
                    sales: 0,
                    ref_paid: 0,
                },
                daa: 5_000,
            })
            .collect();
        Self {
            b_max: t.b_max,
            controller,
            pot: 100_000 * KAS,
            controller_daa: 4_000,
            params,
            params_daa: 4_000,
            shards,
            funding: vec![FundingIn { value: 100 * KAS, daa: 900_000 }],
            funder: Payout::Schnorr(xonly(&FUNDING_SK)),
            fee: KAS / 2,
            lock_time: TEST_DEADLINE as u64,
            m_next: None,
            cov: COV,
            funding_sk: FUNDING_SK,
            k_max: K_MAX,
        }
    }

    pub fn k(&self) -> usize {
        self.shards.len()
    }

    // Input and output indices (A4 shape).
    pub const CONTROLLER: usize = 0;
    pub const PARAMS: usize = 1;
    pub fn shard_input(i: usize) -> usize {
        2 + i
    }
    pub fn funding_input(&self, n: usize) -> usize {
        self.k() + 2 + n
    }
    pub fn marker_output(&self) -> usize {
        self.k() + 2
    }
    pub fn caller_output(&self) -> usize {
        self.k() + 3
    }

    /// Set shard i's sales and referral paid (its root is not checked by `close()`).
    pub fn set_sales(&mut self, i: usize, sales: i64, ref_paid: i64) {
        self.shards[i].state.sales = sales;
        self.shards[i].state.ref_paid = ref_paid;
    }

    pub fn reserve(&self) -> u64 {
        self.controller.reserve as u64
    }

    /// V_c = O + A + R.
    pub fn controller_value(&self) -> u64 {
        self.pot + self.controller.acc as u64 + self.reserve()
    }

    /// S_i - P_R,i + R. i128: attack cases use out-of-range S and P_R.
    pub fn shard_value(&self, i: usize) -> u64 {
        let s = &self.shards[i].state;
        (s.sales as i128 - s.ref_paid as i128 + self.reserve() as i128) as u64
    }

    /// Max creation DAA score over all inputs (A4 check 20).
    pub fn own(&self) -> u64 {
        let mut own = self.controller_daa.max(self.params_daa);
        for s in &self.shards {
            own = own.max(s.daa);
        }
        for f in &self.funding {
            own = own.max(f.daa);
        }
        own
    }

    pub fn math(&self) -> CloseMath {
        let shards: Vec<(i64, i64)> = self.shards.iter().map(|s| (s.state.sales, s.state.ref_paid)).collect();
        close_math(&self.controller, &self.params, &shards, self.controller_value() as i64, self.own() as i64)
    }

    /// Every successor state and output value of a valid close (A4 checks 13-24).
    pub fn expected(&self) -> CloseExpected {
        let math = self.math();
        let m_next = self.m_next.unwrap_or(math.m_next);
        let c = &self.controller;
        let p = &self.params;
        let controller = ControllerState {
            round: c.round + 1,
            m: m_next,
            deadline: math.deadline,
            s_f: p.next_s_f,
            s_r: p.next_s_r,
            fee_class: p.next_fee_class,
            fee_key: p.next_fee_key,
            jackpot: 0,
            j_boost: 0,
            g2: 0,
            liability: 0,
            drawn: 0,
            ..c.clone()
        };
        let params = ParamsState { round: p.round + 1, used: 0, ..p.clone() };
        let shards = (0..self.k())
            .map(|i| ShardState {
                round: c.round + 1,
                shard_index: i as i64,
                m: m_next,
                price: c.price,
                d: c.d,
                s_r: p.next_s_r,
                deadline: math.deadline,
                root: z18(),
                sales: 0,
                ref_paid: 0,
            })
            .collect();
        let roots: Vec<[u8; 32]> = self.shards.iter().map(|s| s.state.root).collect();
        let mut marker = MarkerState {
            round: c.round,
            k: c.k,
            roots: MarkerState::roots_bytes_k(&roots, self.k_max),
            sales: math.sales,
            ref_paid: math.ref_paid,
            m: c.m,
            jackpot: c.jackpot,
            j_boost: c.j_boost,
            g2: c.g2,
            s_f: c.s_f,
            s_r: c.s_r,
            s_c: c.s_c,
            s_d: c.s_d,
            st: c.st,
            seed_ref: math.seed_ref,
            funder_class: 0,
            funder_key: [0; 33],
        };
        marker.set_funder(&self.funder);
        let r = self.reserve();
        let mut values = vec![math.controller_out as u64, r];
        values.extend(std::iter::repeat_n(r, self.k()));
        values.push(math.marker_value as u64);
        let funding: u64 = self.funding.iter().map(|f| f.value).sum();
        let caller = (funding as i128 - r as i128 - self.fee as i128 + math.amounts.c_close as i128) as u64;
        values.push(caller);
        CloseExpected { math, m_next, controller, params, shards, marker, values }
    }

    /// close(m_next, funder_class, funder_key, shard_pl, shard_sl, params_pl, params_sl,
    /// marker_prefix, marker_suffix).
    pub fn args(&self, t: &Templates, m_next: i64) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        vec![
            Int(m_next),
            Byte(self.funder.class()),
            Bytes(self.funder.key33().to_vec()),
            Int(t.shard.prefix.len() as i64),
            Int(t.shard.suffix.len() as i64),
            Int(t.params.prefix.len() as i64),
            Int(t.params.suffix.len() as i64),
            Bytes(t.marker.prefix.clone()),
            Bytes(t.marker.suffix.clone()),
        ]
    }

    /// close_leg(ctrl_pl, ctrl_sl) for the params and shard delegates.
    pub fn leg_args(t: &Templates) -> Vec<ArtifactValue> {
        vec![ArtifactValue::Int(t.controller.prefix.len() as i64), ArtifactValue::Int(t.controller.suffix.len() as i64)]
    }

    pub fn build(&self) -> Built {
        self.build_with(Templates::get(self.b_max))
    }

    /// Build against given templates (mutants compile one covenant without one check).
    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        let x = self.expected();
        let funding: u64 = self.funding.iter().map(|f| f.value).sum();
        let caller = funding as i128 - self.reserve() as i128 - self.fee as i128 + x.math.amounts.c_close as i128;
        assert!(
            caller >= MIN_CALLER_OUTPUT as i128,
            "builder policy: close() caller output {caller} sompi is below MIN_CALLER_OUTPUT = {MIN_CALLER_OUTPUT}"
        );
        let leg = Self::leg_args(&t);
        let cov = Some(self.cov);
        let mut inputs = vec![
            In {
                entry: UtxoEntry::new(self.controller_value(), t.controller_spk(&self.controller), self.controller_daa, false, cov),
                kind: InKind::Entry {
                    template: t.controller.clone(),
                    entry: "close".into(),
                    args: self.args(&t, x.m_next),
                    redeem: t.controller_redeem(&self.controller),
                },
            },
            In {
                entry: UtxoEntry::new(self.reserve(), t.params_spk(&self.params), self.params_daa, false, cov),
                kind: InKind::Entry {
                    template: t.params.clone(),
                    entry: "close_leg".into(),
                    args: leg.clone(),
                    redeem: t.params_redeem(&self.params),
                },
            },
        ];
        for (i, s) in self.shards.iter().enumerate() {
            inputs.push(In {
                entry: UtxoEntry::new(self.shard_value(i), t.shard_spk(&s.state), s.daa, false, cov),
                kind: InKind::Entry {
                    template: t.shard.clone(),
                    entry: "close_leg".into(),
                    args: leg.clone(),
                    redeem: t.shard_redeem(&s.state),
                },
            });
        }
        for f in &self.funding {
            inputs.push(In { entry: UtxoEntry::new(f.value, spk_of(&self.funding_sk), f.daa, false, None), kind: InKind::Funding });
        }

        let bind = Some(CovenantBinding::new(0, self.cov));
        let mut outputs = vec![
            TransactionOutput::with_covenant(x.values[0], t.controller_spk(&x.controller), bind),
            TransactionOutput::with_covenant(x.values[1], t.params_spk(&x.params), bind),
        ];
        for (i, s) in x.shards.iter().enumerate() {
            outputs.push(TransactionOutput::with_covenant(x.values[2 + i], t.shard_spk(s), bind));
        }
        outputs.push(TransactionOutput::with_covenant(x.values[self.marker_output()], t.marker_spk(&x.marker), bind));
        outputs.push(TransactionOutput::new(x.values[self.caller_output()], spk_of(&self.funding_sk)));

        let outpoints = (0..inputs.len() as u32).map(outpoint).collect();
        Built {
            templates: t,
            args: vec![],
            inputs,
            outputs,
            genesis: vec![],
            lock_time: self.lock_time,
            shard_budget: None,
            budget_delta: 0,
            outpoints,
            funding_sk: self.funding_sk,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Builder smoke tests (Stage 1): the shapes assemble, encode, commit storage mass and
    //! sign. They do not depend on the covenant bodies.
    use super::*;
    use crate::genesis::{GenesisCase, covenant_id};
    use crate::ledger::{Ledger, LedgerError};
    use crate::params::SetParamsCase;
    use harness::TxScriptError;

    fn funding_ok(f: &crate::buy::Final, from: usize) {
        for i in from..f.tx.inputs.len() {
            f.run_input(i).unwrap_or_else(|e| panic!("funding input {i}: {e:?}"));
        }
    }

    #[test]
    fn close_shape_assembles() {
        let mut c = CloseCase::new(4);
        c.set_sales(0, 10 * KAS as i64, KAS as i64 / 10);
        c.set_sales(2, 3 * KAS as i64, 0);
        let x = c.expected();
        let f = c.build().finish();
        assert_eq!(f.tx.inputs.len(), 4 + 2 + 1);
        assert_eq!(f.tx.outputs.len(), 4 + 4);
        assert_eq!(f.tx.lock_time, TEST_DEADLINE as u64);
        for (o, v) in f.tx.outputs.iter().zip(&x.values) {
            assert_eq!(o.value, *v);
        }
        assert!(f.tx.outputs[..c.caller_output()].iter().all(|o| o.covenant == Some(CovenantBinding::new(0, COV))));
        assert!(f.tx.outputs[c.caller_output()].covenant.is_none());
        // Conservation over economic value (A4): inputs - outputs = fee.
        assert_eq!(f.fee(), c.fee);
        funding_ok(&f, c.funding_input(0));
        // Covenant inputs reach their entry and fail by script (stubs in stage 1, real checks
        // later), never by an encoding panic.
        for i in 0..c.funding_input(0) {
            if let Err(e) = f.run_input(i) {
                assert!(!matches!(e, TxScriptError::InvalidSignature(_)), "input {i}: {e:?}");
            }
        }
        assert_eq!(x.marker.roots.len(), 512);
        assert_eq!(x.shards[3].root, z18());
    }

    #[test]
    fn set_params_shape_assembles() {
        let c = SetParamsCase::new();
        assert_eq!(c.honest_msg().bytes().len(), 113);
        let f = c.build().finish();
        assert_eq!(f.tx.outputs.len(), 2);
        assert_eq!(f.tx.outputs[0].value, c.value);
        funding_ok(&f, 1);
        // The signature verifies off-chain against the passed key.
        let kp = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &c.admin_sk).unwrap();
        let sig = secp256k1::schnorr::Signature::from_slice(&c.signature()).unwrap();
        let msg = secp256k1::Message::from_digest(c.honest_msg().digest());
        assert!(secp256k1::SECP256K1.verify_schnorr(&sig, &msg, &kp.x_only_public_key().0).is_ok());
    }

    #[test]
    fn genesis_and_ledger() {
        let mut ledger = Ledger::new();
        let spk = crate::buy::funding_spk();
        let op = ledger.fund(crate::genesis::GENESIS_FUNDING, spk, 100);
        let entry = ledger.get(&op).unwrap().clone();
        let g = GenesisCase::new(4, vec![(op, entry)]);
        let f = g.build().finish();
        funding_ok(&f, 0);
        let cov = covenant_id(&f);
        // Controller, params, 4 shards, vault, drawer (D50), deployer position (M5 D66),
        // then change.
        assert!(f.tx.outputs[..9].iter().all(|o| o.covenant.map(|b| b.covenant_id) == Some(cov)));
        assert!(f.tx.outputs[9].covenant.is_none());
        assert_eq!(f.tx.outputs[g.vault_output()].value, g.vault_value());
        assert_eq!(f.tx.outputs[g.vault_output()].script_public_key, g.build().templates.vault_spk(&g.vault));
        assert_eq!(f.tx.outputs[g.drawer_output()].value, g.reserve());
        assert_eq!(f.tx.outputs[g.drawer_output()].script_public_key, g.build().templates.drawer_spk(&g.drawer));
        assert_eq!(f.tx.outputs[g.position_output()].value, g.reserve());
        assert_eq!(f.tx.outputs[g.position_output()].script_public_key, g.build().templates.position_spk(&g.position));
        let outs = ledger.apply(&f.tx, &f.entries, 200).unwrap();
        assert_eq!(outs.len(), 10);
        assert_eq!(ledger.get(&outs[2]).unwrap().covenant_id, Some(cov));
        // Spending the funding outpoint again is rejected.
        assert_eq!(ledger.apply(&f.tx, &f.entries, 300), Err(LedgerError::Missing { index: 0, outpoint: op }));
    }
}
