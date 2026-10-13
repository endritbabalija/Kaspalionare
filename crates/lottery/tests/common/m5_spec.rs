//! M5 Stage 2: an executable model of the spec M5 checks, evaluated on a finished
//! transaction as the scripts see it (inputs' UTXO entries and signature-script arguments,
//! outputs with their covenant bindings, lock time). It is the oracle of `M5_PREFLIGHT`
//! (`common::m5`): before the M5 scripts exist it shows that every attack is rejected at its
//! named check and that every valid M5 transaction passes every modelled check; once a
//! script exists, the preflight also requires the script and the model to agree.
//!
//! Modelled entries and their check labels (each label is `<file>:<check>`; Stages 3-5 put a
//! `// ==== check <check>: ...` marker before each check, in this order, so `M5_ATTRIB`
//! can name the same labels from the scripts):
//! - vault.sil `request_deposit` D1..D9 (spec A4.1 checks 1-9), then D10 (the vault age,
//!   M5 Stage 7, user decision U-4: tx.daa >= OpTxInputDaaScore(0) + VAULT_REQ_DELAY);
//! - vault.sil `request_withdraw` W1..W11 (A4.2 checks 1-11), then W12 (the vault age, U-4);
//!   spec M6 (D86) adds W4b (the withdrawal minimum, with the remainder rule of Stage 5
//!   RV-1) between W4 and W5;
//! - position.sil `vault_leg` PO1..PO4; record.sil `request_leg` RL1..RL4;
//! - request.sil `settle` S1..S13 (A6.1 checks 1-13);
//! - position.sil, request.sil, record.sil `refresh` A7.1..A7.7 (M4 A6 checks 1-7, D76);
//! - drawer.sil `draw`: only the checks M5 and M7 add or change (spec M5 A5, M7 A4): 1 (the
//!   anchor length, M7), 3 (the controller's role, round and drawn = 0; M7 Stage 1b, SA8), 5,
//!   7 (mk_daa, D and the lock, M7), 8 (the anchor proof and the seed, or the void's zero
//!   win5 and empty proofs, M7), 9 (the leaves and siblings at the seed's rank, draw mode; M7
//!   Stage 1b, SA6), 13, 14, 17, C1, C2, 20, 21, 22, 23, 24,
//!   25, 26 in that order (C3 computes and has no require; the unchanged M3 checks are
//!   assumed to pass, which holds for M5 attacks: they change only the vault, the record and
//!   the conversion). Check 8's `OpChainblockSeqCommit` call on X.hash is modelled on the
//!   `ChainSim` the transaction runs against (`Final::sim`); check 1 includes the void floor
//!   draw_window >= VOID_MIN (M7 Stage 5 fix A).
//!
//! Interpretations the model makes (handoff S2-*, O-S2-*): the request, position and
//! settle entries take the template bytes of their foreign outputs as arguments (S2-1); the
//! request's own value range is 0 <= value <= TOTAL_MAX (O-S2-3, user decision U-3);
//! V' <= ASSETS_MAX is enforced by check 14's guard tightened to
//! `hAmt <= ASSETS_MAX - vt.total_assets` (O-S2-2, user decision U-2; the drawer change itself
//! is Stage 4).

use std::collections::BTreeMap;
use std::sync::Arc;

use harness::{ANCHOR_LEN, ArtifactValue, Hash, ScriptPublicKey, pay_to_script_hash_script};
use lottery::anchor::AnchorProof;
use lottery::buy::{Built, Final, InKind, Template, Templates};
use lottery::claim::{SIG_REFRESH, payout_spk};
use lottery::draw::{
    ARG_ANCHOR, ARG_LEAVES, ARG_MK_DAA, ARG_RECORD_PREFIX, ARG_RECORD_SUFFIX, ARG_SIBLINGS, ARG_WC, ARG_WIN5, ARG_X1, ARG_X2, ARG_Y, DrawCase, Lie, SIG_DRAW,
};
use lottery::game::{
    ASSETS_MAX, D_IDLE, DAA_MAX, MIN_DEPOSIT, P_MIN, SHARES_MAX, TOTAL_MAX, VAULT_REQ_DELAY, guarded_add, record_in_range, request_in_range, wc_witness_ok,
    witness_ok,
};
use lottery::state::{KIND_DEPOSIT, PositionState, RecordState, RequestState, VaultState};
use lottery::vault::*;

pub type State = BTreeMap<String, ArtifactValue>;
/// Err(label) of the first failing check.
pub type Verdict = Result<(), String>;

const ZERO: [u8; 32] = [0; 32];
const F_MAX: usize = 8;
const RESERVE_MAX: i64 = 10_000_000_000;
const SCRIPT_MAX: i64 = 1_000_000;

/// The transaction as the scripts see it.
pub struct Tx<'a> {
    pub b: &'a Built,
    pub f: &'a Final,
}

impl<'a> Tx<'a> {
    pub fn new(b: &'a Built, f: &'a Final) -> Self {
        Self { b, f }
    }
    fn t(&self) -> &Templates {
        &self.b.templates
    }
    fn n_in(&self) -> usize {
        self.f.tx.inputs.len()
    }
    fn n_out(&self) -> usize {
        self.f.tx.outputs.len()
    }
    fn in_id(&self, i: usize) -> [u8; 32] {
        self.f.entries[i].covenant_id.map(|h| h.as_bytes()).unwrap_or(ZERO)
    }
    /// OpTxInputAmount: values >= 2^63 fail the u64 -> i64 conversion (rusty-kaspa v2.1.0
    /// crypto/txscript/src/opcodes/mod.rs:1100-1107).
    fn in_value(&self, i: usize) -> Option<i64> {
        i64::try_from(self.f.entries[i].amount).ok()
    }
    fn out_value(&self, o: usize) -> Option<i64> {
        self.f.tx.outputs.get(o).and_then(|x| i64::try_from(x.value).ok())
    }
    fn out_id(&self, o: usize) -> [u8; 32] {
        self.f.tx.outputs[o].covenant.map(|c| c.covenant_id.as_bytes()).unwrap_or(ZERO)
    }
    fn out_spk(&self, o: usize) -> Option<&ScriptPublicKey> {
        self.f.tx.outputs.get(o).map(|x| &x.script_public_key)
    }
    fn in_spk(&self, i: usize) -> &ScriptPublicKey {
        &self.f.entries[i].script_public_key
    }
    /// OpAuthOutputIdx(i, 0..): the outputs authorized by input i, in order.
    fn auth(&self, i: usize) -> Vec<usize> {
        self.f.tx.outputs.iter().enumerate().filter(|(_, o)| o.covenant.is_some_and(|c| c.authorizing_input as usize == i)).map(|(n, _)| n).collect()
    }
    fn cov_in_count(&self, cov: [u8; 32]) -> usize {
        (0..self.n_in()).filter(|&i| self.in_id(i) == cov).count()
    }
    fn cov_out_count(&self, cov: [u8; 32]) -> usize {
        (0..self.n_out()).filter(|&o| self.out_id(o) == cov).count()
    }
    /// OpTxInputDaaScore: u64::MAX (unaccepted parent) is -1 (DECISIONS fact O-S4-4).
    fn daa(&self, i: usize) -> i64 {
        self.f.entries[i].block_daa_score as i64
    }
    /// `require(tx.daa >= x)` compiles to OpCheckLockTimeVerify (M4 DECISIONS fact row).
    fn tx_daa(&self) -> i64 {
        self.f.tx.lock_time as i64
    }
    pub fn entry(&self, i: usize) -> Option<(&Arc<Template>, &str, &[ArtifactValue], &[u8])> {
        match &self.b.inputs.get(i)?.kind {
            InKind::Entry { template, entry, args, redeem } => Some((template, entry.as_str(), args.as_slice(), redeem.as_slice())),
            _ => None,
        }
    }
    fn arg_int(&self, i: usize, n: usize) -> Option<i64> {
        match self.entry(i)?.2.get(n)? {
            ArtifactValue::Int(x) => Some(*x),
            _ => None,
        }
    }
    fn arg_bytes(&self, i: usize, n: usize) -> Option<Vec<u8>> {
        match self.entry(i)?.2.get(n)? {
            ArtifactValue::Bytes(x) => Some(x.clone()),
            ArtifactValue::Byte(x) => Some(vec![*x]),
            _ => None,
        }
    }
    /// The input's own state, decoded with its own template.
    fn own_state(&self, i: usize) -> Option<State> {
        let (t, _, _, redeem) = self.entry(i)?;
        if redeem.len() < t.prefix.len() + t.suffix.len() || !redeem.starts_with(&t.prefix) || !redeem.ends_with(&t.suffix) {
            return None;
        }
        decode(t, &redeem[t.prefix.len()..redeem.len() - t.suffix.len()])
    }
    /// readInputStateWithTemplate(i, pl, sl, hash) decoded with `layout` (the reader's
    /// struct): the template hash of the claimed prefix and suffix, the input's P2SH
    /// commitment, then the decode.
    fn read_input(&self, i: usize, pl: i64, sl: i64, hash: [u8; 32], layout: &Template) -> Option<State> {
        let (pl, sl) = (usize::try_from(pl).ok()?, usize::try_from(sl).ok()?);
        let (_, _, _, redeem) = self.entry(i)?;
        if pl + sl > redeem.len() || silverscript_abi::template_hash(&redeem[..pl], &redeem[redeem.len() - sl..]) != hash {
            return None;
        }
        if pay_to_script_hash_script(redeem) != *self.in_spk(i) {
            return None;
        }
        decode(layout, &redeem[pl..redeem.len() - sl])
    }
    /// The template bytes (prefix, suffix) of input i's redeem split at pl, sl.
    fn input_template(&self, i: usize, pl: i64, sl: i64) -> Option<(Vec<u8>, Vec<u8>)> {
        let (pl, sl) = (usize::try_from(pl).ok()?, usize::try_from(sl).ok()?);
        let (_, _, _, redeem) = self.entry(i)?;
        (pl + sl <= redeem.len()).then(|| (redeem[..pl].to_vec(), redeem[redeem.len() - sl..].to_vec()))
    }
    /// validateOutputStateWithTemplate(o, state, prefix, suffix, hash), the state encoded
    /// with `layout`.
    fn output_with_template(&self, o: usize, layout: &Template, state: &State, prefix: &[u8], suffix: &[u8], hash: [u8; 32]) -> bool {
        silverscript_abi::template_hash(prefix, suffix) == hash
            && state_bytes(layout, state).is_some_and(|s| self.out_spk(o) == Some(&pay_to_script_hash_script(&[prefix, &s, suffix].concat())))
    }
}

pub fn decode(t: &Template, state: &[u8]) -> Option<State> {
    let c = t.art.contracts.values().next().unwrap();
    silverscript_abi::decode_runtime_state_script(&t.art, &c.runtime_state, state).ok()
}

pub fn state_bytes(t: &Template, values: &State) -> Option<Vec<u8>> {
    let c = t.art.contracts.values().next().unwrap();
    silverscript_abi::encode_runtime_state_script(&t.art, &c.runtime_state, values).ok()
}

fn int(s: &State, f: &str) -> Option<i64> {
    match s.get(f)? {
        ArtifactValue::Int(x) => Some(*x),
        _ => None,
    }
}
fn byte(s: &State, f: &str) -> Option<u8> {
    match s.get(f)? {
        ArtifactValue::Byte(x) => Some(*x),
        _ => None,
    }
}
fn b32(s: &State, f: &str) -> Option<[u8; 32]> {
    match s.get(f)? {
        ArtifactValue::Bytes(x) if x.len() == 32 => x.as_slice().try_into().ok(),
        _ => None,
    }
}
fn b33(s: &State, f: &str) -> Option<[u8; 33]> {
    match s.get(f)? {
        ArtifactValue::Bytes(x) if x.len() == 33 => x.as_slice().try_into().ok(),
        _ => None,
    }
}

pub fn vault_of(s: &State) -> Option<(u8, VaultState)> {
    Some((
        byte(s, "role")?,
        VaultState {
            round: int(s, "round")?,
            total_assets: int(s, "total_assets")?,
            total_shares: int(s, "total_shares")?,
            pend_dep: int(s, "pend_dep")?,
            pend_wsh: int(s, "pend_wsh")?,
            liability: int(s, "liability")?,
            record_tpl: b32(s, "record_tpl")?,
            request_tpl: b32(s, "request_tpl")?,
            position_tpl: b32(s, "position_tpl")?,
        },
    ))
}
pub fn position_of(s: &State) -> Option<(u8, PositionState)> {
    Some((byte(s, "role")?, PositionState { round: int(s, "round")?, shares: int(s, "shares")?, owner_class: byte(s, "owner_class")?, owner_key: b33(s, "owner_key")? }))
}
pub fn request_of(s: &State) -> Option<(u8, RequestState)> {
    Some((
        byte(s, "role")?,
        RequestState { round: int(s, "round")?, kind: byte(s, "kind")?, amount: int(s, "amount")?, owner_class: byte(s, "owner_class")?, owner_key: b33(s, "owner_key")? },
    ))
}
pub fn record_of(s: &State) -> Option<(u8, RecordState)> {
    Some((
        byte(s, "role")?,
        RecordState {
            round: int(s, "round")?,
            request_tpl: b32(s, "request_tpl")?,
            refund: byte(s, "refund")?,
            dep_rem: int(s, "dep_rem")?,
            mint_rem: int(s, "mint_rem")?,
            wsh_rem: int(s, "wsh_rem")?,
            burn_rem: int(s, "burn_rem")?,
            out_rem: int(s, "out_rem")?,
            funder_class: byte(s, "funder_class")?,
            funder_key: b33(s, "funder_key")?,
        },
    ))
}

/// A3.1 vault ranges (spec M5) and R = value - total_assets - pend_dep in [0, RESERVE_MAX];
/// returns R.
fn vault_ranges(role: u8, v: &VaultState, value: Option<i64>) -> Option<i64> {
    let ok = role == 0x06
        && (0..TOTAL_MAX).contains(&v.round)
        && (0..=ASSETS_MAX).contains(&v.total_assets)
        && (0..=SHARES_MAX).contains(&v.total_shares)
        && v.pend_dep >= 0
        && v.pend_dep <= TOTAL_MAX - v.total_assets
        && (0..=v.total_shares).contains(&v.pend_wsh)
        && (0..=TOTAL_MAX).contains(&v.liability);
    let r = value? as i128 - v.total_assets as i128 - v.pend_dep as i128;
    (ok && (0..=RESERVE_MAX as i128).contains(&r)).then_some(r as i64)
}

/// D11 class and key padding (class != 1: key[32] = 0).
fn owner_ok(class: u8, key: &[u8; 33]) -> bool {
    class <= 2 && (class == 1 || key[32] == 0)
}

fn position_ranges(role: u8, p: &PositionState) -> bool {
    role == 0x0B && (1..=SHARES_MAX).contains(&p.shares) && owner_ok(p.owner_class, &p.owner_key)
}

fn check(cond: bool, label: &str) -> Verdict {
    if cond { Ok(()) } else { Err(label.to_string()) }
}

fn need<T>(x: Option<T>, label: &str) -> Result<T, String> {
    x.ok_or_else(|| label.to_string())
}

/// `n` covenant outputs 0..n-1 authorized by input `i`, and n outputs under `cov`.
fn cov_outputs(tx: &Tx, i: usize, n: usize, cov: [u8; 32]) -> bool {
    tx.auth(i) == (0..n).collect::<Vec<_>>() && tx.cov_out_count(cov) == n
}

/// Plain inputs from `from` on have the zero ID.
fn zero_inputs(tx: &Tx, from: usize) -> bool {
    (from..tx.n_in()).all(|i| tx.in_id(i) == ZERO)
}

fn zero_outputs(tx: &Tx, from: usize) -> bool {
    (from..tx.n_out()).all(|o| tx.out_id(o) == ZERO)
}

// ---------------------------------------------------------------- dispatch

/// The model's verdict for input `i` of the transaction (`None`: the entry at that input is
/// not modelled; the real script decides). `case` is needed for the drawer (its truth).
pub fn check_input(tx: &Tx, i: usize, draw: Option<&DrawCase>) -> Option<Verdict> {
    let (t, entry, _, _) = tx.entry(i)?;
    let ts = tx.t();
    let is = |x: &Arc<Template>| Arc::ptr_eq(t, x) || t.hash == x.hash;
    let name = t.name().to_string();
    Some(match entry {
        e if e == SIG_REQUEST_DEPOSIT => deposit(tx, i),
        e if e == SIG_REQUEST_WITHDRAW => withdraw(tx, i),
        e if e == SIG_VAULT_LEG => vault_leg(tx, i),
        e if e == SIG_SETTLE => settle(tx, i),
        e if e == SIG_REQUEST_LEG => request_leg(tx, i),
        e if e == SIG_REFRESH && (is(&ts.position) || is(&ts.request) || is(&ts.record) || ["Position", "Request", "Record"].contains(&name.as_str())) => {
            refresh(tx, i, &name.to_lowercase())
        }
        e if e == SIG_DRAW => drawer(tx, i, draw?),
        _ => return None,
    })
}

/// Every modelled input of the transaction: Ok, or the first (input, label) that fails.
pub fn check_all(tx: &Tx, draw: Option<&DrawCase>) -> Result<(), (usize, String)> {
    for i in 0..tx.n_in() {
        if let Some(Err(l)) = check_input(tx, i, draw) {
            return Err((i, l));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- vault entries (A4)

fn deposit(tx: &Tx, i: usize) -> Verdict {
    let l = |n: u8| format!("vault:D{n}");
    // D1: leader; own A3.1 ranges; R from the value.
    check(i == 0, &l(1))?;
    let (role, vt) = need(tx.own_state(0).as_ref().and_then(vault_of), &l(1))?;
    let r = need(vault_ranges(role, &vt, tx.in_value(0)), &l(1))?;
    let cov = tx.in_id(0);
    // D2
    check(tx.cov_in_count(cov) == 1 && (2..=1 + F_MAX).contains(&tx.n_in()) && zero_inputs(tx, 1), &l(2))?;
    // D3
    check(cov_outputs(tx, 0, 2, cov), &l(3))?;
    // D4
    check((2..=3).contains(&tx.n_out()) && zero_outputs(tx, 2), &l(4))?;
    // D5: D = out0 - in0 in [MIN_DEPOSIT, TOTAL_MAX - total_assets - pend_dep].
    let d = need(tx.out_value(0), &l(5))? as i128 - need(tx.in_value(0), &l(5))? as i128;
    check(d >= MIN_DEPOSIT as i128 && d <= (TOTAL_MAX - vt.total_assets - vt.pend_dep) as i128, &l(5))?;
    let d = d as i64;
    // D6: owner class and padding.
    let class = need(tx.arg_bytes(0, DEP_ARG_CLASS), &l(6))?;
    let key = need(tx.arg_bytes(0, DEP_ARG_KEY), &l(6))?;
    check(class.len() == 1 && key.len() == 33, &l(6))?;
    let key: [u8; 33] = key.try_into().unwrap();
    check(owner_ok(class[0], &key), &l(6))?;
    // D7
    check(tx.out_value(1) == Some(r), &l(7))?;
    // D8: vault' with pend_dep + D.
    let (own, _, _, _) = tx.entry(0).unwrap();
    let next = VaultState { pend_dep: vt.pend_dep + d, ..vt.clone() };
    check(tx.out_spk(0) == Some(&pay_to_script_hash_script(&own.redeem(&next.values()))), &l(8))?;
    // D9: the request under request_tpl.
    let rq = RequestState { round: vt.round + 1, kind: KIND_DEPOSIT, amount: d, owner_class: class[0], owner_key: key };
    let (pre, suf) = (need(tx.arg_bytes(0, DEP_ARG_REQ_PREFIX), &l(9))?, need(tx.arg_bytes(0, DEP_ARG_REQ_SUFFIX), &l(9))?);
    check(tx.output_with_template(1, &tx.t().request, &rq.values(), &pre, &suf, vt.request_tpl), &l(9))?;
    // D10 (U-4): the vault's age.
    check(vault_aged(tx), &l(10))
}

/// D10 / W12 (M5 Stage 7, U-4): 0 <= creation = OpTxInputDaaScore(0) < DAA_MAX and tx.daa
/// >= creation + VAULT_REQ_DELAY (as the refresh idle check, A7.3).
fn vault_aged(tx: &Tx) -> bool {
    let creation = tx.daa(0);
    (0..DAA_MAX).contains(&creation) && tx.tx_daa() >= creation + VAULT_REQ_DELAY
}

fn withdraw(tx: &Tx, i: usize) -> Verdict {
    let l = |n: u8| format!("vault:W{n}");
    // W1
    check(i == 0, &l(1))?;
    let (role, vt) = need(tx.own_state(0).as_ref().and_then(vault_of), &l(1))?;
    let r = need(vault_ranges(role, &vt, tx.in_value(0)), &l(1))?;
    let w = need(tx.arg_int(0, WD_ARG_W), &l(1))?;
    let owner_idx = need(tx.arg_int(0, WD_ARG_OWNER_IDX), &l(1))?;
    let (pl, sl) = (need(tx.arg_int(0, WD_ARG_POS_PL), &l(1))?, need(tx.arg_int(0, WD_ARG_POS_SL), &l(1))?);
    check((0..=SCRIPT_MAX).contains(&pl) && (0..=SCRIPT_MAX).contains(&sl), &l(1))?;
    let cov = tx.in_id(0);
    // W2
    check(tx.cov_in_count(cov) == 2 && tx.n_in() >= 2 && tx.in_id(1) == cov && (3..=2 + F_MAX).contains(&tx.n_in()) && zero_inputs(tx, 2), &l(2))?;
    // W3: the position under position_tpl; A3.4 ranges; value R.
    let (prole, ps) = need(tx.read_input(1, pl, sl, vt.position_tpl, &tx.t().position).as_ref().and_then(position_of), &l(3))?;
    check(position_ranges(prole, &ps) && tx.in_value(1) == Some(r), &l(3))?;
    // W4
    check(w >= 1 && w <= ps.shares && w <= vt.total_shares - vt.pend_wsh, &l(4))?;
    // W4b (spec M6 A4, D86; remainder rule Stage 5 RV-1, O-M6-2): the whole position, or
    // w >= wmin of the own state and the remainder shares - w >= wmin.
    let wmin = lottery::game::w_min(vt.total_assets, vt.total_shares);
    check(w == ps.shares || (w >= wmin && ps.shares - w >= wmin), "vault:W4b")?;
    // W5: the owner input (D67).
    check(owner_idx >= 2 && (owner_idx as usize) < tx.n_in() && *tx.in_spk(owner_idx as usize) == payout_spk(ps.owner_class, &ps.owner_key), &l(5))?;
    // W6
    let partial = w < ps.shares;
    let n_cov = 2 + partial as usize;
    check(cov_outputs(tx, 0, n_cov, cov), &l(6))?;
    // W7
    check((n_cov..=n_cov + 1).contains(&tx.n_out()) && zero_outputs(tx, n_cov), &l(7))?;
    // W8
    check(tx.out_value(0) == tx.in_value(0) && tx.out_value(1) == Some(r) && (!partial || tx.out_value(2) == Some(r)), &l(8))?;
    // W9
    let (own, _, _, _) = tx.entry(0).unwrap();
    let next = VaultState { pend_wsh: vt.pend_wsh + w, ..vt.clone() };
    check(tx.out_spk(0) == Some(&pay_to_script_hash_script(&own.redeem(&next.values()))), &l(9))?;
    // W10
    let rq = RequestState { round: vt.round + 1, kind: 1, amount: w, owner_class: ps.owner_class, owner_key: ps.owner_key };
    let (pre, suf) = (need(tx.arg_bytes(0, WD_ARG_REQ_PREFIX), &l(10))?, need(tx.arg_bytes(0, WD_ARG_REQ_SUFFIX), &l(10))?);
    check(tx.output_with_template(1, &tx.t().request, &rq.values(), &pre, &suf, vt.request_tpl), &l(10))?;
    // W11: position' from input 1's template.
    if partial {
        let (pre, suf) = need(tx.input_template(1, pl, sl), &l(11))?;
        let pn = PositionState { shares: ps.shares - w, ..ps.clone() };
        check(tx.output_with_template(2, &tx.t().position, &pn.values(), &pre, &suf, vt.position_tpl), &l(11))?;
    }
    // W12 (U-4): the vault's age.
    check(vault_aged(tx), &l(12))
}

fn vault_leg(tx: &Tx, i: usize) -> Verdict {
    check(i == 1, "position:PO1")?;
    check(tx.in_id(0) == tx.in_id(1), "position:PO2")?;
    check(tx.auth(1).is_empty(), "position:PO3")?;
    let (pl, sl) = (need(tx.arg_int(1, 0), "position:PO4")?, need(tx.arg_int(1, 1), "position:PO4")?);
    check((0..=SCRIPT_MAX).contains(&pl) && (0..=SCRIPT_MAX).contains(&sl), "position:PO4")?;
    // VAULT_TPL is the position's constructor constant: the vault hash of the position's own
    // compile (position(vault), D74).
    let vt = tx.read_input(0, pl, sl, tx.t().vault.hash, &tx.t().vault);
    check(vt.as_ref().and_then(vault_of).is_some_and(|(role, _)| role == 0x06), "position:PO4")
}

// ---------------------------------------------------------------- settlement (A6.1)

fn settle(tx: &Tx, i: usize) -> Verdict {
    let l = |n: u8| format!("request:S{n}");
    // S1: leader; own A3.2 ranges (value: O-S2-3); argument ranges.
    check(i == 0, &l(1))?;
    let (role, rq) = need(tx.own_state(0).as_ref().and_then(request_of), &l(1))?;
    let in0 = need(tx.in_value(0), &l(1))?;
    check(role == 0x07 && request_in_range(&rq) && (0..=TOTAL_MAX).contains(&in0), &l(1))?;
    let (pl, sl) = (need(tx.arg_int(0, SETTLE_ARG_REC_PL), &l(1))?, need(tx.arg_int(0, SETTLE_ARG_REC_SL), &l(1))?);
    check((0..=SCRIPT_MAX).contains(&pl) && (0..=SCRIPT_MAX).contains(&sl), &l(1))?;
    let cov = tx.in_id(0);
    // S2
    check(tx.cov_in_count(cov) == 2 && tx.n_in() >= 2 && tx.in_id(1) == cov && (3..=2 + F_MAX).contains(&tx.n_in()) && zero_inputs(tx, 2), &l(2))?;
    // S3: the record under RECORD_TPL (the request's constant), its ranges, value, round.
    let (rrole, rc) = need(tx.read_input(1, pl, sl, tx.t().record.hash, &tx.t().record).as_ref().and_then(record_of), &l(3))?;
    check(rrole == 0x08 && record_in_range(&rc), &l(3))?;
    let in1 = need(tx.in_value(1), &l(3))? as i128;
    let r_rc = in1 - rc.out_rem as i128 - rc.refund as i128 * rc.dep_rem as i128;
    check((0..=RESERVE_MAX as i128).contains(&r_rc), &l(3))?;
    let r_rc = r_rc as i64;
    check(rc.round == rq.round, &l(3))?;
    // S4-S6: amounts, record', mode (game::settle is the reference for these checks).
    let st = need(lottery::game::settle(&rc, &rq), &l(4))?;
    use lottery::game::SettleMode::*;
    let s = rq.amount;
    let drained = st.drained;
    let cov2 = matches!(st.mode, DepositConvert | Pay | Accrue);
    // S7
    let n_cov = (!drained) as usize + cov2 as usize;
    check(cov_outputs(tx, 0, n_cov, cov), &l(7))?;
    // S8
    let payout = matches!(st.mode, DepositRefund | Final | Pay);
    let base = n_cov + payout as usize + drained as usize;
    check((base..=base + 1).contains(&tx.n_out()) && zero_outputs(tx, n_cov), &l(8))?;
    // S9: values.
    let nx = &st.record;
    if !drained {
        check(tx.out_value(0) == Some(nx.out_rem + nx.refund as i64 * nx.dep_rem + r_rc), &l(9))?;
    }
    let c2 = (!drained) as usize;
    match st.mode {
        DepositConvert | Pay => check(tx.out_value(c2) == Some(in0), &l(9))?,
        Accrue => check(guarded_add(in0, st.pay).is_some() && tx.out_value(c2) == guarded_add(in0, st.pay), &l(9))?,
        _ => {}
    }
    let po = n_cov;
    match st.mode {
        DepositRefund => check(guarded_add(s, in0).is_some() && tx.out_value(po) == guarded_add(s, in0), &l(9))?,
        Final => check(guarded_add(st.pay, in0).is_some() && tx.out_value(po) == guarded_add(st.pay, in0), &l(9))?,
        Pay => check(tx.out_value(po) == Some(st.pay), &l(9))?,
        _ => {}
    }
    let ro = n_cov + payout as usize;
    if drained {
        // M5 Stage 7 (F4): a drained record is empty (out' = 0; held' = refund x dep' = 0 and
        // mint' = burn' = 0 follow from the exact drain), so nothing is left for the fee.
        check(nx.out_rem == 0, &l(9))?;
        check(tx.out_value(ro) == Some(r_rc), &l(9))?;
    }
    // S10: scripts (D11).
    if payout {
        check(tx.out_spk(po) == Some(&payout_spk(rq.owner_class, &rq.owner_key)), &l(10))?;
    }
    if drained {
        check(tx.out_spk(ro) == Some(&payout_spk(rc.funder_class, &rc.funder_key)), &l(10))?;
    }
    // S11: record' from input 1's template.
    if !drained {
        let (pre, suf) = need(tx.input_template(1, pl, sl), &l(11))?;
        check(tx.output_with_template(0, &tx.t().record, &nx.values(), &pre, &suf, tx.t().record.hash), &l(11))?;
    }
    // S12: the position under POSITION_TPL.
    if st.mode == DepositConvert {
        let p = PositionState { round: rc.round, shares: st.sh, owner_class: rq.owner_class, owner_key: rq.owner_key };
        let (pre, suf) = (need(tx.arg_bytes(0, SETTLE_ARG_POS_PREFIX), &l(12))?, need(tx.arg_bytes(0, SETTLE_ARG_POS_SUFFIX), &l(12))?);
        check(tx.output_with_template(c2, &tx.t().position, &p.values(), &pre, &suf, tx.t().position.hash), &l(12))?;
    }
    // S13: request' under the own template.
    if matches!(st.mode, Pay | Accrue) {
        let (own, _, _, _) = tx.entry(0).unwrap();
        let next = RequestState { round: rq.round + 1, kind: 1, amount: st.left, ..rq.clone() };
        check(tx.out_spk(c2) == Some(&pay_to_script_hash_script(&own.redeem(&next.values()))), &l(13))?;
    }
    let _ = P_MIN;
    Ok(())
}

fn request_leg(tx: &Tx, i: usize) -> Verdict {
    check(i == 1, "record:RL1")?;
    check(tx.in_id(0) == tx.in_id(1), "record:RL2")?;
    check(tx.auth(1).is_empty(), "record:RL3")?;
    let (pl, sl) = (need(tx.arg_int(1, 0), "record:RL4")?, need(tx.arg_int(1, 1), "record:RL4")?);
    check((0..=SCRIPT_MAX).contains(&pl) && (0..=SCRIPT_MAX).contains(&sl), "record:RL4")?;
    let (_, own) = need(tx.own_state(1).as_ref().and_then(record_of), "record:RL4")?;
    let rq = tx.read_input(0, pl, sl, own.request_tpl, &tx.t().request);
    check(rq.as_ref().and_then(request_of).is_some_and(|(role, r)| role == 0x07 && r.round == own.round), "record:RL4")
}

// ---------------------------------------------------------------- refresh (A7)

fn refresh(tx: &Tx, i: usize, file: &str) -> Verdict {
    let l = |n: u8| format!("{file}:A7.{n}");
    check(i == 0, &l(1))?;
    let cov = tx.in_id(0);
    check(tx.cov_in_count(cov) == 1 && (2..=1 + F_MAX).contains(&tx.n_in()) && zero_inputs(tx, 1), &l(2))?;
    let creation = tx.daa(0);
    check((0..DAA_MAX).contains(&creation) && tx.tx_daa() >= creation + D_IDLE, &l(3))?;
    check(cov_outputs(tx, 0, 1, cov), &l(4))?;
    check((1..=2).contains(&tx.n_out()) && zero_outputs(tx, 1), &l(5))?;
    check(tx.out_value(0).is_some() && tx.out_value(0) == tx.in_value(0), &l(6))?;
    check(tx.out_spk(0) == Some(tx.in_spk(0)), &l(7))
}

// ---------------------------------------------------------------- the drawer (A5)

/// `OpChainblockSeqCommit(h)` on the transaction's chain (spec M7 A0): the commitment iff `h`
/// is a known selected-chain block within the window (blue_score + F > pov), else `None`
/// (the opcode fails: no accessor, unknown, not selected or too deep).
fn chainblock_seq_commit(tx: &Tx, h: [u8; 32]) -> Option<[u8; 32]> {
    let sim = tx.f.sim.as_deref()?;
    let h = Hash::from_bytes(h);
    let b = sim.block(h)?;
    if !sim.on_chain(h) || b.blue_score + sim.finality_depth() <= sim.pov_blue_score() {
        return None;
    }
    sim.commit_of(h).map(|c| c.as_bytes())
}

/// The drawer's M5 and M7 checks. The truth is the case without its `Lie` (fabricated case
/// edits stay); the claims are read from the transaction.
fn drawer(tx: &Tx, i: usize, case: &DrawCase) -> Verdict {
    let l = |n: &str| format!("drawer:{n}");
    if i != 0 {
        return Err(l("1"));
    }
    // Spec M7 check 1: draw_window >= VOID_MIN = 2F (the drawer's own state; M7 Stage 5 fix
    // A, D97); the anchor is empty (void) or ANCHOR_LEN bytes.
    let dw = need(tx.own_state(0).as_ref().and_then(|s| int(s, "draw_window")), &l("1"))?;
    check(dw >= lottery::game::VOID_MIN, &l("1"))?;
    let anchor = need(tx.arg_bytes(0, ARG_ANCHOR), &l("1"))?;
    check(anchor.is_empty() || anchor.len() == ANCHOR_LEN, &l("1"))?;
    let is_void = anchor.is_empty();
    let mut tc = case.clone();
    tc.lie = Lie::default();
    let t = tx.b.templates.clone();
    // Check 3 (M3 A4; added to the model in M7 Stage 1b for SA8): the round r + 1 controller
    // at input 3 with role 1, round = the drawer's round + 1 and drawn = 0 (a second draw or
    // void of a round finds drawn = 1, spec M7 A6 item 7).
    let ctrl = need(tx.own_state(3), &l("3"))?;
    let round = need(tx.own_state(0).as_ref().and_then(|s| int(s, "round")), &l("3"))?;
    check(byte(&ctrl, "role") == Some(1) && int(&ctrl, "round") == Some(round + 1) && byte(&ctrl, "drawn") == Some(0), &l("3"))?;
    // Check 5: the vault input, A3.1 ranges (extended) and value.
    let (role, vt) = need(tx.own_state(2).as_ref().and_then(vault_of), &l("5"))?;
    check(vault_ranges(role, &vt, tx.in_value(2)) == Some(tc.reserve() as i64) && vt.round == tc.drawer.round - 1, &l("5"))?;
    // Spec M7 check 7: mkDaa = OpTxInputDaaScore(1) in [0, DAA_MAX), the mk_daa argument
    // equal to it, D = mkDaa + seed_offset < DAA_MAX; draw: tx.daa >= D; void: tx.daa >= D +
    // draw_window (< DAA_MAX).
    let mk_daa = tx.daa(1);
    check((0..DAA_MAX).contains(&mk_daa) && tx.arg_int(0, ARG_MK_DAA) == Some(mk_daa), &l("7"))?;
    let seed_offset = need(tx.own_state(3).as_ref().and_then(|s| int(s, "seed_offset")), &l("7"))?;
    let d = mk_daa + seed_offset;
    check(d < DAA_MAX, &l("7"))?;
    if is_void {
        let v = d as i128 + tc.drawer.draw_window as i128;
        check(v < DAA_MAX as i128 && tx.tx_daa() as i128 >= v, &l("7"))?;
    } else {
        check(tx.tx_daa() >= d, &l("7"))?;
    }
    // Spec M7 check 8: draw: X's preimage recomputed and equal to the opcode's commitment of
    // X.hash, P's recomputed commitment equal to X's parent_commit (pc == x_pc; no opcode call
    // on P and no P.hash since M7 Stage 5 fix B), dX >= D, dP < D, win5 = the seed's main numbers
    // from X.hash (the rank and bonus are not separately modelled: the M3 checks 9-10 follow);
    // void: win5 = 0, no leaves and no siblings.
    let win5 = need(tx.arg_bytes(0, ARG_WIN5), &l("8"))?;
    if is_void {
        let empty = |n| tx.arg_bytes(0, n).is_some_and(|b| b.is_empty());
        check(win5 == [0u8; 5] && empty(ARG_LEAVES) && empty(ARG_SIBLINGS), &l("8"))?;
    } else {
        let a = need(AnchorProof::from_bytes(&anchor).ok(), &l("8"))?;
        check(chainblock_seq_commit(tx, a.x.hash) == Some(a.x.seq_commit()), &l("8"))?;
        check(a.p.seq_commit() == a.x.parent_commit, &l("8"))?;
        let (dx, dp) = (a.x.daa_score as i64, a.p.daa_score as i64);
        check(dx >= d && dp < d, &l("8"))?;
        let sn = lottery::game::seed_numbers(&tx.in_id(0), tc.marker.round, &a.x.hash, tc.marker.m);
        check(win5 == sn.main, &l("8"))?;
    }
    let x = tc.expected();
    // Check 9 (M3 A4; added to the model in M7 Stage 1b for SA6): in draw mode the leaves and
    // siblings are the K count leaves at the seed's rank with their proofs (the truth's).
    if !is_void {
        check(tx.arg_bytes(0, ARG_LEAVES).as_deref() == Some(&x.leaves[..]) && tx.arg_bytes(0, ARG_SIBLINGS).as_deref() == Some(&x.siblings[..]), &l("9"))?;
    }
    let rec = vt.pend_dep > 0 || vt.pend_wsh > 0;
    let n_cov = 3 + x.np() + rec as usize;
    let cov = tx.in_id(0);
    // Check 13.
    check(cov_outputs(tx, 0, n_cov, cov), &l("13"))?;
    // Check 14 (U-2): V + H <= ASSETS_MAX, so V' <= ASSETS_MAX before any price is formed.
    // Spec M7 D96: a void pays H to the boost pot (hAmt = 0).
    let h_amt = if is_void { 0 } else { x.amounts.h };
    check(vt.total_assets as i128 + h_amt as i128 <= ASSETS_MAX as i128, &l("14"))?;
    // Check 17: the witness with the rules of V' (J fixed before any conversion, D71).
    let (x1, y, x2) = (need(tx.arg_int(0, ARG_X1), &l("17"))?, need(tx.arg_int(0, ARG_Y), &l("17"))?, need(tx.arg_int(0, ARG_X2), &l("17"))?);
    check(witness_ok(&x.rule, x.v_prime, x.o_prime, tc.controller.j, x1, y, x2) && x1 <= TOTAL_MAX && x2 <= TOTAL_MAX, &l("17"))?;
    // C2: the sum guard V' + D <= TOTAL_MAX (drawer `M5C2_sum`, Stage 4 S4-2; added to the
    // model in Stage 7: Stage 6's "VA6 V' + D > TOTAL_MAX" otherwise reached check 20 here),
    // then the wc witness (the M5 argument; the M4-signature fallback was removed in M6, D89).
    check(vt.pend_dep as i128 <= TOTAL_MAX as i128 - x.v_prime as i128, &l("C2"))?;
    let wc = need(tx.arg_int(0, ARG_WC), &l("C2"))?;
    check(wc_witness_ok(x.v_prime, vt.total_shares, vt.pend_wsh, x.conv.slack, wc), &l("C2"))?;
    // The true transaction.
    let good = tc.build_with(t.clone()).finish();
    let gout = &good.tx.outputs;
    // Check 20: output count and values (the caller output is the remainder).
    check(tx.n_out() == x.n_outputs(), &l("20"))?;
    for o in 0..x.n_outputs() - 1 {
        check(tx.out_value(o) == i64::try_from(x.values[o]).ok(), &l("20"))?;
    }
    // Check 21: plain outputs: zero ID; fee and refund scripts.
    check(zero_outputs(tx, n_cov), &l("21"))?;
    for o in n_cov..x.n_outputs() - 1 {
        check(tx.out_spk(o) == Some(&gout[o].script_public_key), &l("21"))?;
    }
    // Checks 22-24: drawer', vault', controller'.
    for (o, n) in [(0usize, "22"), (1, "23"), (2, "24")] {
        check(tx.out_spk(o) == Some(&gout[o].script_public_key), &l(n))?;
    }
    // Check 25: the pools.
    for p in 0..x.np() {
        check(tx.out_spk(3 + p) == Some(&gout[3 + p].script_public_key), &l("25"))?;
    }
    // Check 26: the record under vt.record_tpl (record_prefix, record_suffix).
    if let (Some(o), Some(rc)) = (x.record_output(), &x.record) {
        let (pre, suf) = (need(tx.arg_bytes(0, ARG_RECORD_PREFIX), &l("26"))?, need(tx.arg_bytes(0, ARG_RECORD_SUFFIX), &l("26"))?);
        check(tx.output_with_template(o, &t.record, &rc.values(), &pre, &suf, vt.record_tpl), &l("26"))?;
    }
    Ok(())
}
