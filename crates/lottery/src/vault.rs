//! Spec M5 builders: the vault's `request_deposit()` and `request_withdraw()` (A4) and the
//! request's `settle()` (A6.1), with the reference for every output. `refresh()` of a
//! position, request or record is `claim::RefreshCase` with the M5 `RefreshObj`s (A7).
//!
//! Shapes (spec M5, D75):
//!   request_deposit (A4.1)  inputs : 0 vault (`request_deposit`); 1 .. n funding (zero ID)
//!                           outputs: 0 vault' (value + D); 1 request (deposit, value R);
//!                                    [2 change]
//!   request_withdraw (A4.2) inputs : 0 vault (`request_withdraw`); 1 position (`vault_leg`);
//!                                    2 .. funding, one of them the owner input (zero ID)
//!                           outputs: 0 vault' (value unchanged); 1 request (withdrawal,
//!                                    value R); partial: 2 position' (shares - w, value R);
//!                                    [change]
//!   settle (A6.1)           inputs : 0 request (`settle`); 1 record (`request_leg`);
//!                                    2 .. funding (zero ID)
//!                           outputs: record' (unless drained); position (deposit-convert)
//!                                    or request' (PAY, ACCRUE); then plain: the payout
//!                                    (refund, FINAL, PAY); the record's R to its funder (if
//!                                    drained); [change]
//!
//! Every covenant output is authorized by input 0 under the instance covenant ID. The
//! caller pays the fee from funding (change = funding - fee, less D + R for a deposit and R
//! for a partial withdrawal's request).
//!
//! As `claim.rs`: a `*Case` has public fields; `expected()` derives every output from the
//! `game` reference (`settle` for settlements), never from the script; `build()` returns an
//! editable `Built`. Entries are called by full dispatch signature (`SIG_*`, M4 S2-1): until
//! M5 Stages 3 and 5 write them, the vault, position, request and record placeholders reject
//! these transactions at dispatch.
//!
//! Builder policy (S5-5): requests, settlements and refreshes spend chain-merged inputs. The
//! input DAA scores read are refresh's idle check and, since M5 Stage 7 (U-4), the vault
//! request age: a request's lock_time must be >= the vault's creation DAA + VAULT_REQ_DELAY
//! (`DepositCase::at_age` / `WithdrawCase::at_age`; `new_with` sets exactly that).

use std::sync::Arc;

use harness::{ArtifactValue, CovenantBinding, Hash, TransactionOutput, UtxoEntry, outpoint};

use crate::buy::{Built, COV, FUNDING_SK, In, InKind, KAS, OwnerKey, Templates, b_max, spk_of};
use crate::claim::payout_spk;
use crate::close::FundingIn;
use crate::game::{
    MIN_DEPOSIT, P_MIN, SHARE_UNIT, SettleMode, TOTAL_MAX, VA, VAULT_REQ_DELAY, VS, convert, mulfrac,
};
use crate::params::TEST_RESERVE;
use crate::state::{KIND_DEPOSIT, KIND_WITHDRAW, Payout, PositionState, RecordState, RequestState, VaultState};

// ------------------------------------------------------------------------ entry contract

/// Vault `request_deposit(byte owner_class, byte[33] owner_key, byte[] req_prefix, byte[]
/// req_suffix)` (A4.1). S2-1 (PROVISIONAL, open item O-S2-1): SilverScript v1.0.0's
/// `validateOutputStateWithTemplate` takes the template prefix and suffix bytes
/// (silverscript-lang/src/compiler/static_check.rs:811-825, docs/TUTORIAL.md:1218-1229), and
/// no input of a deposit carries the request template, so A4.1 check 9 needs them as
/// arguments, as the drawer's check 26 (record_prefix, record_suffix) and check 25
/// (pool_prefix, pool_suffix). Appended after the spec's arguments.
pub const SIG_REQUEST_DEPOSIT: &str = "request_deposit(byte,byte[33],byte[],byte[])";
/// Vault `request_withdraw(int w, int owner_idx, int pos_pl, int pos_sl, byte[] req_prefix,
/// byte[] req_suffix)` (A4.2; the request template bytes for check 10, S2-1).
pub const SIG_REQUEST_WITHDRAW: &str = "request_withdraw(int,int,int,int,byte[],byte[])";
/// Position `vault_leg(int vault_pl, int vault_sl)` (A4 PO1-PO4).
pub const SIG_VAULT_LEG: &str = "vault_leg(int,int)";
/// Request `settle(int rec_pl, int rec_sl, byte[] pos_prefix, byte[] pos_suffix)` (A6.1; the
/// position template bytes for check 12, S2-1; ignored unless deposit-convert, as the
/// drawer ignores record_prefix and record_suffix when nothing is pending).
pub const SIG_SETTLE: &str = "settle(int,int,byte[],byte[])";
/// Record `request_leg(int req_pl, int req_sl)` (A6.1 RL1-RL4).
pub const SIG_REQUEST_LEG: &str = "request_leg(int,int)";

pub const DEP_ARG_CLASS: usize = 0;
pub const DEP_ARG_KEY: usize = 1;
pub const DEP_ARG_REQ_PREFIX: usize = 2;
pub const DEP_ARG_REQ_SUFFIX: usize = 3;
pub const WD_ARG_W: usize = 0;
pub const WD_ARG_OWNER_IDX: usize = 1;
pub const WD_ARG_POS_PL: usize = 2;
pub const WD_ARG_POS_SL: usize = 3;
pub const WD_ARG_REQ_PREFIX: usize = 4;
pub const WD_ARG_REQ_SUFFIX: usize = 5;
pub const SETTLE_ARG_REC_PL: usize = 0;
pub const SETTLE_ARG_REC_SL: usize = 1;
pub const SETTLE_ARG_POS_PREFIX: usize = 2;
pub const SETTLE_ARG_POS_SUFFIX: usize = 3;

// ------------------------------------------------------------------------ fixtures

/// The fixture backer's key (positions and requests of the standalone cases).
pub const OWNER_SK: [u8; 32] = [0x31; 32];
/// A second backer.
pub const OWNER2_SK: [u8; 32] = [0x32; 32];
/// Fixture vault V (10,000,000 KAS, as `draw::TEST_VAULT`).
pub const FIXTURE_VAULT: i64 = 10_000_000 * KAS as i64;
/// Fixture liability (as the draw fixture's).
pub const FIXTURE_LIABILITY: i64 = 1_040_000 * KAS as i64;

/// The fixture backer's owner key (Schnorr P2PK).
pub fn owner() -> OwnerKey {
    OwnerKey::Schnorr(OWNER_SK)
}

/// A fabricated vault (D79) of round 1 after its draw: V = FIXTURE_VAULT, S = V / SHARE_UNIT,
/// nothing pending, the fixture liability, the templates of `t`.
pub fn fixture_vault(t: &Templates) -> VaultState {
    VaultState {
        round: 1,
        total_assets: FIXTURE_VAULT,
        total_shares: FIXTURE_VAULT / SHARE_UNIT,
        pend_dep: 0,
        pend_wsh: 0,
        liability: FIXTURE_LIABILITY,
        record_tpl: t.record.hash,
        request_tpl: t.request.hash,
        position_tpl: t.position.hash,
    }
}

fn funding_inputs(funding: &[FundingIn], sk: &[u8; 32]) -> Vec<In> {
    funding.iter().map(|f| In { entry: UtxoEntry::new(f.value, spk_of(sk), f.daa, false, None), kind: InKind::Funding }).collect()
}

fn built(t: Arc<Templates>, inputs: Vec<In>, outputs: Vec<TransactionOutput>, lock_time: u64, budget_delta: i32, sk: [u8; 32]) -> Built {
    let outpoints = (0..inputs.len() as u32).map(outpoint).collect();
    Built { templates: t, args: vec![], inputs, outputs, genesis: vec![], lock_time, shard_budget: None, budget_delta, outpoints, funding_sk: sk }
}

fn total(funding: &[FundingIn]) -> u64 {
    funding.iter().map(|f| f.value).sum()
}

/// Leg arguments (prefix and suffix length of a template).
fn lens(t: &crate::buy::Template) -> Vec<ArtifactValue> {
    vec![ArtifactValue::Int(t.prefix.len() as i64), ArtifactValue::Int(t.suffix.len() as i64)]
}

/// i128 -> u64 for output values (attack cases may go out of range; the script rejects).
fn val(x: i128) -> u64 {
    x.clamp(0, u64::MAX as i128) as u64
}

// ------------------------------------------------------------------------ request_deposit

#[derive(Clone)]
pub struct DepositCase {
    pub b_max: usize,
    /// The vault of round r (input 0); value total_assets + pend_dep + R.
    pub vault: VaultState,
    /// R (the controller's reserve; the vault derives it from its value).
    pub reserve: u64,
    pub vault_daa: u64,
    /// The deposit D (i64: range attacks use 0 and out-of-range values).
    pub d: i64,
    /// The request's owner (any D11 script, D77).
    pub owner: Payout,
    pub funding: Vec<FundingIn>,
    pub change: bool,
    pub fee: u64,
    /// The transaction lock time; must be >= vault_daa + VAULT_REQ_DELAY (D10, U-4).
    pub lock_time: u64,
    pub cov: Hash,
    pub funding_sk: [u8; 32],
    pub budget_delta: i32,
}

/// The reference outputs of a deposit request (A4.1 checks 5-9).
#[derive(Clone, Debug)]
pub struct DepositExpected {
    /// vault' (pend_dep + D, every other field unchanged).
    pub vault: VaultState,
    /// The request {round + 1, deposit, D, owner}.
    pub request: RequestState,
    /// Output values (change last, if present).
    pub values: Vec<u64>,
    pub change_output: Option<usize>,
}

impl DepositCase {
    pub const VAULT: usize = 0;
    pub fn funding_input(n: usize) -> usize {
        1 + n
    }

    /// A valid deposit request of 10 KAS by the fixture backer into the fixture vault, with
    /// one funding input of 20 KAS and change.
    pub fn new() -> Self {
        Self::new_with(&Templates::get(b_max()))
    }

    pub fn new_with(t: &Templates) -> Self {
        Self {
            b_max: t.b_max,
            vault: fixture_vault(t),
            reserve: TEST_RESERVE,
            vault_daa: 1_000_000,
            d: 10 * KAS as i64,
            owner: owner().payout(),
            funding: vec![FundingIn { value: 20 * KAS, daa: 1_100_000 }],
            change: true,
            fee: KAS / 10,
            lock_time: 1_000_000 + VAULT_REQ_DELAY as u64,
            cov: COV,
            funding_sk: FUNDING_SK,
            budget_delta: 0,
        }
    }

    pub fn vault_value(&self) -> u64 {
        val(self.vault.total_assets as i128 + self.vault.pend_dep as i128 + self.reserve as i128)
    }

    /// The earliest lock time the vault accepts: its creation DAA + VAULT_REQ_DELAY (D10).
    pub fn earliest(&self) -> u64 {
        self.vault_daa + VAULT_REQ_DELAY as u64
    }

    /// Set the vault's creation DAA to `daa` and the lock time to the earliest accepted one.
    pub fn at_age(&mut self, daa: u64) -> &mut Self {
        self.vault_daa = daa;
        self.lock_time = self.earliest();
        self
    }

    pub fn expected(&self) -> DepositExpected {
        let vault = VaultState { pend_dep: self.vault.pend_dep.saturating_add(self.d), ..self.vault.clone() };
        let request = RequestState::new(self.vault.round + 1, KIND_DEPOSIT, self.d, &self.owner);
        let mut values = vec![val(self.vault_value() as i128 + self.d as i128), self.reserve];
        let change_output = self.change.then(|| {
            values.push(val(total(&self.funding) as i128 - self.d as i128 - self.reserve as i128 - self.fee as i128));
            values.len() - 1
        });
        DepositExpected { vault, request, values, change_output }
    }

    pub fn args(&self, t: &Templates) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        vec![Byte(self.owner.class()), Bytes(self.owner.key33().to_vec()), Bytes(t.request.prefix.clone()), Bytes(t.request.suffix.clone())]
    }

    pub fn build(&self) -> Built {
        self.build_with(Templates::get(self.b_max))
    }

    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        let x = self.expected();
        let cov = Some(self.cov);
        let mut inputs = vec![In {
            entry: UtxoEntry::new(self.vault_value(), t.vault_spk(&self.vault), self.vault_daa, false, cov),
            kind: InKind::Entry { template: t.vault.clone(), entry: SIG_REQUEST_DEPOSIT.into(), args: self.args(&t), redeem: t.vault_redeem(&self.vault) },
        }];
        inputs.extend(funding_inputs(&self.funding, &self.funding_sk));
        let bind = Some(CovenantBinding::new(0, self.cov));
        let mut outputs = vec![
            TransactionOutput::with_covenant(x.values[0], t.vault_spk(&x.vault), bind),
            TransactionOutput::with_covenant(x.values[1], t.request_spk(&x.request), bind),
        ];
        if let Some(i) = x.change_output {
            outputs.push(TransactionOutput::new(x.values[i], spk_of(&self.funding_sk)));
        }
        built(t, inputs, outputs, self.lock_time, self.budget_delta, self.funding_sk)
    }
}

impl Default for DepositCase {
    fn default() -> Self {
        Self::new()
    }
}

// ------------------------------------------------------------------------ request_withdraw

#[derive(Clone)]
pub struct WithdrawCase {
    pub b_max: usize,
    pub vault: VaultState,
    pub reserve: u64,
    pub vault_daa: u64,
    /// The position (input 1); value R.
    pub position: PositionState,
    pub position_daa: u64,
    /// Shares to withdraw (i64: range attacks).
    pub w: i64,
    /// The owner input's key (its script must be the position owner's, D67) and UTXO.
    pub owner: OwnerKey,
    pub owner_value: u64,
    pub owner_daa: u64,
    /// The owner input's place among the plain inputs (index 2 + owner_at).
    pub owner_at: usize,
    /// The other plain inputs (signed with `funding_sk`).
    pub funding: Vec<FundingIn>,
    pub change: bool,
    pub fee: u64,
    /// The transaction lock time; must be >= vault_daa + VAULT_REQ_DELAY (W12, U-4).
    pub lock_time: u64,
    pub cov: Hash,
    pub funding_sk: [u8; 32],
    pub budget_delta: i32,
}

/// The reference outputs of a withdrawal request (A4.2 checks 4-11).
#[derive(Clone, Debug)]
pub struct WithdrawExpected {
    pub partial: bool,
    /// vault' (pend_wsh + w).
    pub vault: VaultState,
    /// The request {round + 1, withdrawal, w, the position's owner}.
    pub request: RequestState,
    /// position' {round, shares - w, owner} (partial only).
    pub position: Option<PositionState>,
    pub values: Vec<u64>,
    pub change_output: Option<usize>,
}

impl WithdrawExpected {
    pub fn n_cov(&self) -> usize {
        2 + self.partial as usize
    }
}

impl WithdrawCase {
    pub const VAULT: usize = 0;
    pub const POSITION: usize = 1;

    /// A valid partial withdrawal of 1,000,000 of a 5,000,000-share position of the
    /// fixture backer (Schnorr owner input of 10 KAS, no other funding, change).
    pub fn new() -> Self {
        Self::new_with(&Templates::get(b_max()), owner())
    }

    /// As `new` with the owner `key` (P2PK Schnorr, P2PK ECDSA or P2SH).
    pub fn new_with(t: &Templates, key: OwnerKey) -> Self {
        Self {
            b_max: t.b_max,
            vault: fixture_vault(t),
            reserve: TEST_RESERVE,
            vault_daa: 1_000_000,
            position: PositionState::new(1, 5_000_000, &key.payout()),
            position_daa: 900_000,
            w: 1_000_000,
            owner: key,
            owner_value: 10 * KAS,
            owner_daa: 1_100_000,
            owner_at: 0,
            funding: vec![],
            change: true,
            fee: KAS / 10,
            lock_time: 1_000_000 + VAULT_REQ_DELAY as u64,
            cov: COV,
            funding_sk: FUNDING_SK,
            budget_delta: 0,
        }
    }

    /// The whole position (w = shares).
    pub fn whole(mut self) -> Self {
        self.w = self.position.shares;
        self
    }

    pub fn vault_value(&self) -> u64 {
        val(self.vault.total_assets as i128 + self.vault.pend_dep as i128 + self.reserve as i128)
    }

    /// The earliest lock time the vault accepts: its creation DAA + VAULT_REQ_DELAY (W12).
    pub fn earliest(&self) -> u64 {
        self.vault_daa + VAULT_REQ_DELAY as u64
    }

    /// Set the vault's creation DAA to `daa` and the lock time to the earliest accepted one.
    pub fn at_age(&mut self, daa: u64) -> &mut Self {
        self.vault_daa = daa;
        self.lock_time = self.earliest();
        self
    }

    pub fn owner_input(&self) -> usize {
        2 + self.owner_at
    }

    pub fn n_inputs(&self) -> usize {
        3 + self.funding.len()
    }

    pub fn expected(&self) -> WithdrawExpected {
        let ps = &self.position;
        let partial = self.w < ps.shares;
        let vault = VaultState { pend_wsh: self.vault.pend_wsh.saturating_add(self.w), ..self.vault.clone() };
        let request = RequestState { round: self.vault.round + 1, kind: KIND_WITHDRAW, amount: self.w, owner_class: ps.owner_class, owner_key: ps.owner_key };
        let position = partial.then(|| PositionState { shares: ps.shares - self.w, ..ps.clone() });
        let mut values = vec![self.vault_value(), self.reserve];
        if partial {
            values.push(self.reserve);
        }
        let plain = self.owner_value + total(&self.funding);
        let change_output = self.change.then(|| {
            // Whole: the position's R becomes the request's; partial: the funding posts it.
            values.push(val(plain as i128 - self.fee as i128 - if partial { self.reserve as i128 } else { 0 }));
            values.len() - 1
        });
        WithdrawExpected { partial, vault, request, position, values, change_output }
    }

    pub fn args(&self, t: &Templates) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        vec![
            Int(self.w),
            Int(self.owner_input() as i64),
            Int(t.position.prefix.len() as i64),
            Int(t.position.suffix.len() as i64),
            Bytes(t.request.prefix.clone()),
            Bytes(t.request.suffix.clone()),
        ]
    }

    pub fn build(&self) -> Built {
        self.build_with(Templates::get(self.b_max))
    }

    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        let x = self.expected();
        let cov = Some(self.cov);
        let mut inputs = vec![
            In {
                entry: UtxoEntry::new(self.vault_value(), t.vault_spk(&self.vault), self.vault_daa, false, cov),
                kind: InKind::Entry { template: t.vault.clone(), entry: SIG_REQUEST_WITHDRAW.into(), args: self.args(&t), redeem: t.vault_redeem(&self.vault) },
            },
            In {
                entry: UtxoEntry::new(self.reserve, t.position_spk(&self.position), self.position_daa, false, cov),
                kind: InKind::Entry { template: t.position.clone(), entry: SIG_VAULT_LEG.into(), args: lens(&t.vault), redeem: t.position_redeem(&self.position) },
            },
        ];
        let mut plain = funding_inputs(&self.funding, &self.funding_sk);
        let owner_in = In { entry: UtxoEntry::new(self.owner_value, self.owner.spk(), self.owner_daa, false, None), kind: InKind::Owner(self.owner.clone()) };
        plain.insert(self.owner_at.min(plain.len()), owner_in);
        inputs.extend(plain);
        let bind = Some(CovenantBinding::new(0, self.cov));
        let mut outputs = vec![
            TransactionOutput::with_covenant(x.values[0], t.vault_spk(&x.vault), bind),
            TransactionOutput::with_covenant(x.values[1], t.request_spk(&x.request), bind),
        ];
        if let Some(p) = &x.position {
            outputs.push(TransactionOutput::with_covenant(x.values[2], t.position_spk(p), bind));
        }
        if let Some(i) = x.change_output {
            outputs.push(TransactionOutput::new(x.values[i], spk_of(&self.funding_sk)));
        }
        built(t, inputs, outputs, self.lock_time, self.budget_delta, self.funding_sk)
    }
}

impl Default for WithdrawCase {
    fn default() -> Self {
        Self::new()
    }
}

// ------------------------------------------------------------------------ settle

/// TEST-ONLY consistent false claims (attacks, as `ClaimLie`): each set field replaces one
/// value the reference derives; every output follows it.
#[derive(Clone, Debug, Default)]
pub struct SettleLie {
    /// Deposit: shares minted instead of the reference's (VA1 sh = 0, VA7 sh +- 1).
    pub sh: Option<i64>,
    /// Withdrawal: the payout instead of the reference's (VA7 pay +- 1).
    pub pay: Option<i64>,
    /// The mode (and so the shape) instead of the reference's (VA11).
    pub mode: Option<SettleMode>,
    /// Keep a record' when drained (`Some(true)`) or drop it when not (`Some(false)`) (VA11).
    pub drained: Option<bool>,
}

/// The raw settlement amounts (spec M5 A6.1 checks 4-6), computed without failing (attack
/// cases); `reference` is `game::settle` (None = the script rejects).
#[derive(Clone, Debug)]
pub struct SettleExpected {
    pub reference: Option<crate::game::Settlement>,
    pub mode: SettleMode,
    pub sh: i64,
    pub conv: i64,
    pub pay: i64,
    pub left: i64,
    pub drained: bool,
    /// record' (the new remainders).
    pub record: RecordState,
    /// The position minted (deposit-convert) or request' (PAY, ACCRUE).
    pub position: Option<PositionState>,
    pub request: Option<RequestState>,
    /// Output indices.
    pub record_output: Option<usize>,
    pub cov2_output: Option<usize>,
    pub payout_output: Option<usize>,
    pub refund_output: Option<usize>,
    pub change_output: Option<usize>,
    pub values: Vec<u64>,
}

impl SettleExpected {
    pub fn n_cov(&self) -> usize {
        self.record_output.is_some() as usize + self.cov2_output.is_some() as usize
    }
}

#[derive(Clone)]
pub struct SettleCase {
    pub b_max: usize,
    /// The request (input 0); value R + accrued.
    pub request: RequestState,
    pub request_value: u64,
    pub request_daa: u64,
    /// The record (input 1); value out_rem + refund x dep_rem + R_rc.
    pub record: RecordState,
    /// R_rc (the record's reserve, posted by the draw caller).
    pub record_reserve: u64,
    pub record_daa: u64,
    pub funding: Vec<FundingIn>,
    pub change: bool,
    pub fee: u64,
    pub lock_time: u64,
    pub lie: SettleLie,
    pub cov: Hash,
    pub funding_sk: [u8; 32],
    pub budget_delta: i32,
}

/// mulfrac without failing (attack cases outside the domain): floor(a r / c) in i128.
fn mf(a: i64, r: i64, c: i64) -> i64 {
    mulfrac(a, r, c).unwrap_or_else(|| if c == 0 { 0 } else { ((a as i128 * r as i128) / c as i128) as i64 })
}

/// The fixture record of the draw of round 2 (D79): the conversion of the fixture vault at
/// V' = FIXTURE_VAULT with 3 x 10 KAS deposited and 3,000,000 shares to withdraw, L' =
/// the fixture liability (nothing capped); funder = the funding key.
pub fn fixture_record(t: &Templates) -> RecordState {
    let c = convert(FIXTURE_VAULT, FIXTURE_VAULT / SHARE_UNIT, 30 * KAS as i64, 3_000_000, FIXTURE_LIABILITY).expect("fixture conversion");
    c.record(2, t.request.hash, &Payout::Schnorr(crate::params::xonly(&FUNDING_SK))).unwrap()
}

impl SettleCase {
    pub const REQUEST: usize = 0;
    pub const RECORD: usize = 1;
    pub fn funding_input(n: usize) -> usize {
        2 + n
    }

    /// A settlement of `request` (value R) against `record` (R_rc = TEST_RESERVE), one
    /// funding input of 10 KAS, change.
    pub fn new(record: RecordState, request: RequestState) -> Self {
        Self::new_with(&Templates::get(b_max()), record, request)
    }

    pub fn new_with(t: &Templates, record: RecordState, request: RequestState) -> Self {
        Self {
            b_max: t.b_max,
            request,
            request_value: TEST_RESERVE,
            request_daa: 1_000_000,
            record,
            record_reserve: TEST_RESERVE,
            record_daa: 1_200_000,
            funding: vec![FundingIn { value: 10 * KAS, daa: 1_300_000 }],
            change: true,
            fee: KAS / 10,
            lock_time: 0,
            lie: SettleLie::default(),
            cov: COV,
            funding_sk: FUNDING_SK,
            budget_delta: 0,
        }
    }

    /// A deposit settlement of 10 KAS against the fixture record (deposit-convert, not
    /// drained).
    pub fn deposit() -> Self {
        let t = Templates::get(b_max());
        let rc = fixture_record(&t);
        Self::new_with(&t, rc, RequestState::new(2, KIND_DEPOSIT, 10 * KAS as i64, &owner().payout()))
    }

    /// A withdrawal settlement of 1,000,000 shares against the fixture record (all of the
    /// record's withdrawals converted: FINAL, not drained).
    pub fn withdrawal() -> Self {
        let t = Templates::get(b_max());
        let rc = fixture_record(&t);
        Self::new_with(&t, rc, RequestState::new(2, KIND_WITHDRAW, 1_000_000, &owner().payout()))
    }

    /// out_rem + refund x dep_rem + R_rc (i128: attack records may be out of range).
    pub fn record_value(&self) -> u64 {
        let rc = &self.record;
        val(rc.out_rem as i128 + rc.refund as i128 * rc.dep_rem as i128 + self.record_reserve as i128)
    }

    pub fn owner(&self) -> Payout {
        Payout::from_fields(self.request.owner_class.min(2), &self.request.owner_key)
    }

    pub fn expected(&self) -> SettleExpected {
        let rc = &self.record;
        let rq = &self.request;
        let reference = crate::game::settle(rc, rq);
        let s = rq.amount;
        let mut next = rc.clone();
        let (mut sh, mut conv, mut pay, mut left) = (0, 0, 0, 0);
        let mode;
        if rq.kind == KIND_DEPOSIT {
            if rc.refund == 1 {
                mode = SettleMode::DepositRefund;
            } else {
                mode = SettleMode::DepositConvert;
                sh = if s == rc.dep_rem { rc.mint_rem } else { mf(rc.mint_rem, s, rc.dep_rem) };
            }
            if let Some(x) = self.lie.sh {
                sh = x;
            }
            next.dep_rem = rc.dep_rem - s;
            next.mint_rem = rc.mint_rem - sh;
        } else {
            conv = if s == rc.wsh_rem { rc.burn_rem } else { mf(rc.burn_rem, s, rc.wsh_rem) };
            pay = if rc.burn_rem == 0 {
                0
            } else if conv == rc.burn_rem {
                rc.out_rem
            } else {
                mf(rc.out_rem, conv, rc.burn_rem)
            };
            if let Some(x) = self.lie.pay {
                pay = x;
            }
            left = s - conv;
            mode = if left == 0 {
                SettleMode::Final
            } else if pay >= P_MIN {
                SettleMode::Pay
            } else {
                SettleMode::Accrue
            };
            next.wsh_rem = rc.wsh_rem - s;
            next.burn_rem = rc.burn_rem - conv;
            next.out_rem = rc.out_rem - pay;
        }
        let mode = self.lie.mode.unwrap_or(mode);
        let drained = self.lie.drained.map(|keep| !keep).unwrap_or(next.dep_rem == 0 && next.wsh_rem == 0);

        let position = (mode == SettleMode::DepositConvert).then(|| PositionState { round: rc.round, shares: sh, owner_class: rq.owner_class, owner_key: rq.owner_key });
        let request = matches!(mode, SettleMode::Pay | SettleMode::Accrue)
            .then(|| RequestState { round: rq.round + 1, kind: KIND_WITHDRAW, amount: left, owner_class: rq.owner_class, owner_key: rq.owner_key });

        let iv = self.request_value as i128;
        let mut values = vec![];
        let record_output = (!drained).then(|| {
            values.push(val(next.out_rem as i128 + next.refund as i128 * next.dep_rem as i128 + self.record_reserve as i128));
            values.len() - 1
        });
        let cov2_output = (position.is_some() || request.is_some()).then(|| {
            values.push(match mode {
                SettleMode::DepositConvert | SettleMode::Pay => self.request_value,
                _ => val(iv + pay as i128),
            });
            values.len() - 1
        });
        let payout_output = match mode {
            SettleMode::DepositRefund => Some(val(s as i128 + iv)),
            SettleMode::Final => Some(val(pay as i128 + iv)),
            SettleMode::Pay => Some(val(pay as i128)),
            _ => None,
        }
        .map(|v| {
            values.push(v);
            values.len() - 1
        });
        let refund_output = drained.then(|| {
            values.push(self.record_reserve);
            values.len() - 1
        });
        let change_output = self.change.then(|| {
            values.push(val(total(&self.funding) as i128 - self.fee as i128));
            values.len() - 1
        });
        if !self.change {
            assert_eq!(total(&self.funding), self.fee, "a settlement without change needs funding = fee");
        }
        SettleExpected {
            reference,
            mode,
            sh,
            conv,
            pay,
            left,
            drained,
            record: next,
            position,
            request,
            record_output,
            cov2_output,
            payout_output,
            refund_output,
            change_output,
            values,
        }
    }

    pub fn args(&self, t: &Templates) -> Vec<ArtifactValue> {
        let mut v = lens(&t.record);
        v.extend([ArtifactValue::Bytes(t.position.prefix.clone()), ArtifactValue::Bytes(t.position.suffix.clone())]);
        v
    }

    pub fn build(&self) -> Built {
        self.build_with(Templates::get(self.b_max))
    }

    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        let x = self.expected();
        let cov = Some(self.cov);
        let mut inputs = vec![
            In {
                entry: UtxoEntry::new(self.request_value, t.request_spk(&self.request), self.request_daa, false, cov),
                kind: InKind::Entry { template: t.request.clone(), entry: SIG_SETTLE.into(), args: self.args(&t), redeem: t.request_redeem(&self.request) },
            },
            In {
                entry: UtxoEntry::new(self.record_value(), t.record_spk(&self.record), self.record_daa, false, cov),
                kind: InKind::Entry { template: t.record.clone(), entry: SIG_REQUEST_LEG.into(), args: lens(&t.request), redeem: t.record_redeem(&self.record) },
            },
        ];
        inputs.extend(funding_inputs(&self.funding, &self.funding_sk));
        let bind = Some(CovenantBinding::new(0, self.cov));
        let mut outputs = vec![];
        if let Some(i) = x.record_output {
            outputs.push(TransactionOutput::with_covenant(x.values[i], t.record_spk(&x.record), bind));
        }
        if let Some(i) = x.cov2_output {
            let spk = match (&x.position, &x.request) {
                (Some(p), _) => t.position_spk(p),
                (_, Some(r)) => t.request_spk(r),
                _ => unreachable!(),
            };
            outputs.push(TransactionOutput::with_covenant(x.values[i], spk, bind));
        }
        if let Some(i) = x.payout_output {
            outputs.push(TransactionOutput::new(x.values[i], payout_spk(self.request.owner_class, &self.request.owner_key)));
        }
        if let Some(i) = x.refund_output {
            outputs.push(TransactionOutput::new(x.values[i], payout_spk(self.record.funder_class, &self.record.funder_key)));
        }
        if let Some(i) = x.change_output {
            outputs.push(TransactionOutput::new(x.values[i], spk_of(&self.funding_sk)));
        }
        built(t, inputs, outputs, self.lock_time, self.budget_delta, self.funding_sk)
    }
}

/// The price-check helper of VP8: the assets a position of `shares` is worth at the vault
/// state (A, S): floor(shares (A + VA) / (S + VS)) (the withdrawal payout, C2).
pub fn position_worth(shares: i64, total_assets: i64, total_shares: i64) -> i64 {
    ((shares as i128 * (total_assets as i128 + VA as i128)) / (total_shares as i128 + VS as i128)) as i64
}

/// MIN_DEPOSIT and TOTAL_MAX re-exported for the tests' range attacks.
pub const DEPOSIT_MIN: i64 = MIN_DEPOSIT;
pub const VALUE_MAX: i64 = TOTAL_MAX;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::SettleMode;

    #[test]
    fn fixtures_are_valid_on_the_rust_side() {
        let d = DepositCase::new();
        let x = d.expected();
        assert_eq!(x.vault.pend_dep, d.vault.pend_dep + d.d);
        assert_eq!(d.build().outputs.len(), 3);
        let w = WithdrawCase::new();
        let x = w.expected();
        assert!(x.partial);
        assert_eq!(w.build().outputs.len(), 4);
        assert!(!w.clone().whole().expected().partial);
        let s = SettleCase::deposit();
        let x = s.expected();
        assert_eq!(x.mode, SettleMode::DepositConvert);
        assert!(x.reference.is_some() && !x.drained);
        let s = SettleCase::withdrawal();
        let x = s.expected();
        assert_eq!(x.mode, SettleMode::Final, "{x:?}");
        assert!(x.reference.is_some() && !x.drained);
        for k in [OwnerKey::Schnorr(OWNER_SK), OwnerKey::Ecdsa(OWNER_SK), OwnerKey::P2sh(OWNER_SK)] {
            assert_eq!(k.payout().spk(), k.spk());
        }
    }
}
