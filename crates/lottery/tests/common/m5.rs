//! M5 Stage 2: the vault-share tests' shared library: the M5 cases (`Case5`), acceptance
//! and attack strikes with the two M5 diagnostics, the `Chain` M5 steps, the standalone
//! bases (D79) and the attacks VA1-VA15 as data (`Atk5`, as `common::m4::Atk`), shared by
//! `m5_vault.rs`, `m5_attacks.rs` and Stage 6's `m5_mutants.rs`.
//!
//! An `Atk5` is a valid case (`good`) with one case edit and one transaction edit, aimed at
//! one input, with the check the spec (M5 A10 "Check") expects to reject it, as a
//! `<file>:<label>` of `common::m5_spec` (vault:D1..D9, vault:W1..W11, position:PO1..PO4,
//! request:S1..S13, record:RL1..RL4, <obj>:A7.1..A7.7, drawer:5/13/17/C1/C2/20..26).
//! `strike`: the bad transaction is rejected by `input` with a script error, the good one
//! passes `input`, and the good one is then accepted by every input (`accept5`).
//!
//! Diagnostics (neither is a test result on its own; both off by default):
//! - `M5_PREFLIGHT=1`: the spec model (`m5_spec`) stands in for the M5 scripts that do not
//!   exist yet. A strike then requires the model to reject the bad transaction at the
//!   target input with exactly the named check and to accept the good one (so every attack
//!   is shown to reach its named check, not an earlier unrelated one); `accept5` tolerates
//!   a placeholder rejection (`EarlyReturn`: dispatch found no entry) or the M4 drawer's
//!   rejection of a converting draw when the model accepts that input, and applies the
//!   transaction, so the positive chains run end to end. Where a script exists, its verdict
//!   must agree with the model's (accept/reject) at every modelled input. Lines `PREFLIGHT`.
//! - `M5_ATTRIB=1`: every strike also prints the first check that rejects the attack in
//!   the script (`ATTRIB ... | expected <check> | got <check> | model <check> | OK|MISMATCH`)
//!   by truncating the target source before each `// ==== check <label>` marker (the M4
//!   instrument, extended to vault.sil, request.sil, record.sil and position.sil; the
//!   drawer uses the M3 instrument, `common::m2::first_failing_check`). Stages 3-5 place the
//!   markers with the `m5_spec` labels.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use harness::{ArtifactValue, CovenantBinding, TransactionOutpoint, TransactionOutput, TxScriptError, UtxoEntry, pay_to_script_hash_script};
use lottery::buy::{Built, FUNDING_SK, Final, In, InKind, KAS, OwnerKey, Sources, Template, Templates, b_max, funding_spk};
use lottery::claim::{RefreshCase, RefreshObj, SIG_REFRESH};
use lottery::close::FundingIn;
use lottery::draw::DrawCase;
use lottery::game::{DAA_MAX, MIN_DEPOSIT, P_MIN, SHARE_UNIT, SHARES_MAX, SettleMode, TOTAL_MAX, VAULT_REQ_DELAY, convert};
use lottery::params::xonly;
use lottery::state::{KIND_DEPOSIT, KIND_WITHDRAW, Payout, PositionState, RecordState, RequestState};
use lottery::vault::*;

use super::m2::*;
use super::m4::{extra_cov, extra_plain, genesis_output, other_payout, reindex, rogue_copy, set_input_cov, set_input_daa};
use super::m5_spec::{Tx, check_all, check_input, state_bytes};

// ---------------------------------------------------------------- the case of an attack

#[derive(Clone)]
pub enum Case5 {
    Deposit(DepositCase),
    Withdraw(WithdrawCase),
    Settle(SettleCase),
    Refresh(RefreshCase),
    Draw(DrawCase),
}

impl From<DepositCase> for Case5 {
    fn from(c: DepositCase) -> Self {
        Case5::Deposit(c)
    }
}
impl From<WithdrawCase> for Case5 {
    fn from(c: WithdrawCase) -> Self {
        Case5::Withdraw(c)
    }
}
impl From<SettleCase> for Case5 {
    fn from(c: SettleCase) -> Self {
        Case5::Settle(c)
    }
}
impl From<RefreshCase> for Case5 {
    fn from(c: RefreshCase) -> Self {
        Case5::Refresh(c)
    }
}
impl From<DrawCase> for Case5 {
    fn from(c: DrawCase) -> Self {
        Case5::Draw(c)
    }
}

impl Case5 {
    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        match self {
            Case5::Deposit(c) => c.build_with(t),
            Case5::Withdraw(c) => c.build_with(t),
            Case5::Settle(c) => c.build_with(t),
            Case5::Refresh(c) => c.build_with(t),
            Case5::Draw(c) => c.build_with(t),
        }
    }

    pub fn build(&self) -> Built {
        self.build_with(tpl())
    }

    pub fn step(&self) -> String {
        match self {
            Case5::Deposit(_) => "request_deposit".into(),
            Case5::Withdraw(c) => format!("request_withdraw ({})", if c.expected().partial { "partial" } else { "whole" }),
            Case5::Settle(c) => format!("settle ({:?})", c.expected().mode),
            Case5::Refresh(c) => format!("refresh ({})", c.obj.name()),
            Case5::Draw(c) => if c.pending() { "draw (conversion)".into() } else { "draw (nothing pending)".into() },
        }
    }

    pub fn draw_case(&self) -> Option<&DrawCase> {
        match self {
            Case5::Draw(c) => Some(c),
            _ => None,
        }
    }

    /// Point the case at templates `t`: the vault's template fields, the record's
    /// request_tpl and the draw's template hashes (a mutant or look-alike changes them).
    pub fn retarget(&mut self, t: &Templates) {
        let vt = |v: &mut lottery::state::VaultState| {
            v.record_tpl = t.record.hash;
            v.request_tpl = t.request.hash;
            v.position_tpl = t.position.hash;
        };
        match self {
            Case5::Deposit(c) => vt(&mut c.vault),
            Case5::Withdraw(c) => vt(&mut c.vault),
            Case5::Settle(c) => c.record.request_tpl = t.request.hash,
            Case5::Refresh(c) => {
                if let RefreshObj::Record(r) = &mut c.obj {
                    r.request_tpl = t.request.hash;
                }
            }
            Case5::Draw(c) => {
                *c = super::m3::retarget(c.clone(), t);
                vt(&mut c.vault);
            }
        }
    }

    pub fn dep(&mut self) -> &mut DepositCase {
        match self {
            Case5::Deposit(c) => c,
            _ => panic!("not a deposit case"),
        }
    }
    pub fn wd(&mut self) -> &mut WithdrawCase {
        match self {
            Case5::Withdraw(c) => c,
            _ => panic!("not a withdrawal case"),
        }
    }
    pub fn st(&mut self) -> &mut SettleCase {
        match self {
            Case5::Settle(c) => c,
            _ => panic!("not a settle case"),
        }
    }
    pub fn rf(&mut self) -> &mut RefreshCase {
        match self {
            Case5::Refresh(c) => c,
            _ => panic!("not a refresh case"),
        }
    }
    pub fn dr(&mut self) -> &mut DrawCase {
        match self {
            Case5::Draw(c) => c,
            _ => panic!("not a draw case"),
        }
    }
}

pub type CaseEdit5 = Box<dyn Fn(&mut Case5)>;
pub type BuiltEdit5 = Box<dyn Fn(&mut Built, &Case5)>;

pub struct Atk5 {
    /// "VAn short description".
    pub id: String,
    pub input: usize,
    /// The expected rejecting check (`<file>:<label>`, module doc).
    pub check: &'static str,
    pub good: Case5,
    pub case: CaseEdit5,
    pub built: BuiltEdit5,
}

impl Atk5 {
    pub fn new(id: impl Into<String>, input: usize, check: &'static str, good: impl Into<Case5>) -> Self {
        Self { id: id.into(), input, check, good: good.into(), case: Box::new(|_| {}), built: Box::new(|_, _| {}) }
    }
    pub fn dep(mut self, f: impl Fn(&mut DepositCase) + 'static) -> Self {
        self.case = Box::new(move |c| f(c.dep()));
        self
    }
    pub fn wd(mut self, f: impl Fn(&mut WithdrawCase) + 'static) -> Self {
        self.case = Box::new(move |c| f(c.wd()));
        self
    }
    pub fn st(mut self, f: impl Fn(&mut SettleCase) + 'static) -> Self {
        self.case = Box::new(move |c| f(c.st()));
        self
    }
    pub fn rf(mut self, f: impl Fn(&mut RefreshCase) + 'static) -> Self {
        self.case = Box::new(move |c| f(c.rf()));
        self
    }
    pub fn dr(mut self, f: impl Fn(&mut DrawCase) + 'static) -> Self {
        self.case = Box::new(move |c| f(c.dr()));
        self
    }
    pub fn built(mut self, f: impl Fn(&mut Built, &Case5) + 'static) -> Self {
        self.built = Box::new(f);
        self
    }
    pub fn at(mut self, input: usize, check: &'static str) -> Self {
        self.input = input;
        self.check = check;
        self
    }

    /// (good, bad, bad case) under templates `t`.
    pub fn pair_case(&self, t: &Arc<Templates>) -> (Built, Built, Case5) {
        let mut good = self.good.clone();
        good.retarget(t);
        let mut bad = good.clone();
        (self.case)(&mut bad);
        let mut bb = bad.build_with(t.clone());
        (self.built)(&mut bb, &bad);
        (good.build_with(t.clone()), bb, bad)
    }

    pub fn pair(&self, t: &Arc<Templates>) -> (Built, Built) {
        let (g, b, _) = self.pair_case(t);
        (g, b)
    }

    /// Committed templates: strike, then accept the good transaction (`accept5`).
    pub fn strike(&self) {
        let mut good = self.good.clone();
        good.retarget(&tpl());
        let (g, b, bc) = self.pair_case(&tpl());
        strike5(self, &g, &b, &good, &bc);
        accept5(&format!("{} [{}]", good.step(), self.id), &good, &g);
    }
}

pub fn run5(atks: Vec<Atk5>) {
    assert!(!atks.is_empty());
    for a in atks {
        a.strike();
    }
}

// ---------------------------------------------------------------- diagnostics switches

pub fn preflight5() -> bool {
    std::env::var_os("M5_PREFLIGHT").is_some()
}

pub fn attrib5_enabled() -> bool {
    std::env::var_os("M5_ATTRIB").is_some()
}

fn test_name() -> String {
    std::thread::current().name().unwrap_or("?").to_string()
}

/// A rejection that only says the M5 script is not written yet: the placeholder's dispatch
/// (`EarlyReturn`) of a non-drawer input. The drawer has only the M5 entry since M6 (D89:
/// the M4-signature case was removed), so a drawer rejection always counts.
fn not_yet_written(e: &TxScriptError, b: &Built, i: usize) -> bool {
    let drawer = matches!(&b.inputs[i].kind, InKind::Entry { template, .. } if template.name() == "Drawer");
    !drawer && matches!(e, TxScriptError::EarlyReturn)
}

// ---------------------------------------------------------------- acceptance

/// Every input of `b` (the transaction of `case`) under its committed budget: legs first,
/// then input 0 (as `accept_draw`). Under `M5_PREFLIGHT` a not-yet-written M5 script is
/// tolerated where the model accepts that input, and the model must agree with every
/// script that exists. Panics name the step, the input and every input's result.
pub fn accept5(step: &str, case: &Case5, b: &Built) -> Final {
    let f = b.finish();
    let tx = Tx::new(b, &f);
    let dc = case.draw_case();
    let n = f.tx.inputs.len();
    let mut tolerated = vec![];
    for i in (1..n).chain([0]) {
        let model = check_input(&tx, i, dc);
        match f.run_input(i) {
            Ok(_) => {
                if preflight5()
                    && let Some(Err(l)) = &model
                {
                    panic!("M5 step `{step}`: MODEL DISAGREES at input {i}: the script accepts, the model rejects at {l}");
                }
            }
            Err(e) => {
                if preflight5() && not_yet_written(&e, b, i) && model == Some(Ok(())) {
                    tolerated.push((i, format!("{e:?}")));
                    continue;
                }
                panic!(
                    "M5 step `{step}`: valid transaction rejected at input {i}: {e:?}; model: {:?}; all inputs: {:?}",
                    check_all(&tx, dc),
                    f.run_all()
                );
            }
        }
    }
    if !tolerated.is_empty() {
        println!("PREFLIGHT accept | {} | {step} | not yet written, model accepts: {tolerated:?}", test_name());
    }
    f
}

/// `accept5` for a draw (Chain::draw with something pending).
pub fn accept_draw5(step: &str, c: &DrawCase, b: &Built) -> Final {
    accept5(step, &Case5::Draw(c.clone()), b)
}

// ---------------------------------------------------------------- strike

pub fn strike5(atk: &Atk5, good: &Built, bad: &Built, good_case: &Case5, bad_case: &Case5) {
    let input = atk.input;
    let gf = good.finish();
    let bf = bad.finish();
    let sigs = |f: &Final| f.tx.inputs.iter().map(|i| i.signature_script.clone()).collect::<Vec<_>>();
    assert!(gf.tx.id() != bf.tx.id() || gf.entries != bf.entries || sigs(&gf) != sigs(&bf), "{}: the attack changed nothing", atk.id);
    let real_bad = bf.run_input(input);
    let real_good = gf.run_input(input);
    if preflight5() {
        let mb = check_input(&Tx::new(bad, &bf), input, bad_case.draw_case());
        let mg = check_input(&Tx::new(good, &gf), input, good_case.draw_case());
        let verdict = match (&mb, &mg) {
            // The good input is a plain one (a funding or owner input where the attack puts a
            // covenant input): not modelled; its own script must pass (checked below).
            (Some(Err(l)), Some(Ok(())) | None) if l == atk.check => "OK",
            _ => "MISMATCH",
        };
        println!(
            "PREFLIGHT {} | {} | input {input} expect {} | model bad {:?} good {:?} | script bad {:?} good {:?} | {verdict}",
            test_name(),
            atk.id,
            atk.check,
            mb,
            mg,
            real_bad.as_ref().err(),
            real_good.as_ref().err()
        );
        assert_eq!(verdict, "OK", "PREFLIGHT MISMATCH ({}): expected {} at input {input}, the model gives bad {mb:?}, good {mg:?}", atk.id, atk.check);
        let written = match &real_good {
            Ok(_) => true,
            Err(e) => !not_yet_written(e, good, input),
        };
        if !written {
            return;
        }
    }
    match &real_bad {
        Ok(_) => panic!("ATTACK ACCEPTED ({}) by input {input}", atk.id),
        Err(e) => assert!(is_script_rejection(e), "{}: input {input}: expected a script error, got {e:?}", atk.id),
    }
    if let Err(e) = &real_good {
        panic!("BASE REJECTED at target input {input} ({}): {e:?}", atk.id);
    }
    if attrib5_enabled() {
        let got = first_failing_check5(bad, bad_case, atk.check, input);
        let model = check_input(&Tx::new(bad, &bf), input, bad_case.draw_case());
        let verdict = match &got {
            Some(g) if g.split(' ').next() == Some(atk.check) => "OK",
            Some(_) => "MISMATCH",
            None => "NO MARKERS",
        };
        println!("ATTRIB {} | {} | expected {} | got {} | model {model:?} | {verdict}", test_name(), atk.id, atk.check, got.as_deref().unwrap_or("-"));
    }
}

// ---------------------------------------------------------------- check attribution (M5_ATTRIB)

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Obj5 {
    Vault,
    Request,
    Record,
    Position,
}

fn parse5(check: &str) -> Option<(Option<Obj5>, String)> {
    let (file, label) = check.split_once(':')?;
    let obj = match file {
        "vault" => Some(Obj5::Vault),
        "request" => Some(Obj5::Request),
        "record" => Some(Obj5::Record),
        "position" => Some(Obj5::Position),
        "drawer" => None,
        _ => return None,
    };
    Some((obj, label.to_string()))
}

/// A label's group: through the first '.' ("A7.3" -> "A7."), else the leading letters
/// ("D3" -> "D", "PO2" -> "PO", "S12" -> "S", "13" -> ""). A trailing lowercase suffix is
/// stripped first, so an inserted check sorts into its letter group ("W4b" -> "W", spec M6
/// W4b between W4 and W5).
pub fn group5(label: &str) -> String {
    match label.find('.') {
        Some(p) => label[..=p].to_string(),
        None => label.trim_end_matches(|c: char| c.is_ascii_lowercase()).trim_end_matches(|c: char| c.is_ascii_digit()).to_string(),
    }
}

fn source5(obj: Obj5) -> String {
    let s = Sources::committed(b_max());
    match obj {
        Obj5::Vault => s.vault,
        Obj5::Request => s.request,
        Obj5::Record => s.record,
        Obj5::Position => s.position,
    }
    .replace("\r\n", "\n")
}

fn compile5(obj: Obj5, src: &str) -> Arc<Template> {
    let mut s = Sources::committed(b_max());
    match obj {
        Obj5::Vault => s.vault = src.to_string(),
        Obj5::Request => s.request = src.to_string(),
        Obj5::Record => s.record = src.to_string(),
        Obj5::Position => s.position = src.to_string(),
    }
    let t = Templates::compile_sources(b_max(), &s);
    match obj {
        Obj5::Vault => t.vault.clone(),
        Obj5::Request => t.request.clone(),
        Obj5::Record => t.record.clone(),
        Obj5::Position => t.position.clone(),
    }
}

type Truncations = Vec<(String, Arc<Template>)>;

/// (label, truncated template) for every marker of `group` in `src`, in order; the last is
/// the whole source (as `common::m4::truncations`).
pub fn truncations5(obj: Obj5, group: &str, src: &str) -> Arc<Truncations> {
    type Key = (Obj5, String, String);
    static CACHE: OnceLock<Mutex<HashMap<Key, Arc<Truncations>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (obj, group.to_string(), src.to_string());
    if let Some(t) = cache.lock().unwrap().get(&key) {
        return t.clone();
    }
    let lines: Vec<&str> = src.lines().collect();
    let marks: Vec<(usize, String)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.trim_start().strip_prefix("// ==== check ").map(|r| (i, r.split(':').next().unwrap().trim().to_string())))
        .filter(|(_, label)| group5(label) == group)
        .collect();
    let out: Truncations = marks
        .iter()
        .enumerate()
        .map(|(j, (_, label))| {
            let body = match marks.get(j + 1) {
                None => src.to_string(),
                Some(&(end, _)) => {
                    let kept = &lines[..end];
                    let depth: i64 = kept
                        .iter()
                        .map(|l| l.split("//").next().unwrap())
                        .map(|c| c.matches('{').count() as i64 - c.matches('}').count() as i64)
                        .sum();
                    assert!(depth >= 2, "{obj:?} truncation before check {label}: brace depth {depth} (marker outside an entry?)");
                    kept.join("\n") + "\n" + &"}\n".repeat(depth as usize)
                }
            };
            (label.clone(), compile5(obj, &body))
        })
        .collect();
    let out = Arc::new(out);
    cache.lock().unwrap().insert(key, out.clone());
    out
}

/// The state bytes of input `i`'s redeem under its template.
fn input_state_bytes(b: &Built, i: usize) -> (Arc<Template>, Vec<u8>) {
    match &b.inputs[i].kind {
        InKind::Entry { template, redeem, .. } => (template.clone(), redeem[template.prefix.len()..redeem.len() - template.suffix.len()].to_vec()),
        _ => panic!("input {i} is not an entry input"),
    }
}

/// `b` with input `input` running template `t`; the input's own-template successor (a
/// refresh's output 0, vault' of a request entry, request' of a PAY or ACCRUE settlement)
/// rebuilt under `t` when it is the genuine one. Returns whether such a successor was
/// edited by the attack.
fn with_template5(b: &Built, input: usize, t: &Arc<Template>, case: &Case5) -> (Built, bool) {
    let mut m = b.clone();
    let (old, state) = input_state_bytes(b, input);
    m.set_entry_redeem(input, [t.prefix.as_slice(), &state, &t.suffix].concat());
    if let InKind::Entry { template, .. } = &mut m.inputs[input].kind {
        *template = t.clone();
    }
    if input != 0 {
        return (m, false);
    }
    let succ: Option<(usize, Vec<u8>)> = match case {
        Case5::Refresh(_) => Some((0, state.clone())),
        Case5::Deposit(c) => state_bytes(&old, &c.expected().vault.values()).map(|s| (0, s)),
        Case5::Withdraw(c) => state_bytes(&old, &c.expected().vault.values()).map(|s| (0, s)),
        Case5::Settle(c) => {
            let x = c.expected();
            match (x.cov2_output, &x.request) {
                (Some(o), Some(r)) => state_bytes(&old, &r.values()).map(|s| (o, s)),
                _ => None,
            }
        }
        Case5::Draw(_) => None,
    };
    let mut edited = false;
    if let Some((o, s)) = succ
        && o < b.outputs.len()
    {
        if b.outputs[o].script_public_key == pay_to_script_hash_script(&[old.prefix.as_slice(), &s, &old.suffix].concat()) {
            m.outputs[o].script_public_key = pay_to_script_hash_script(&[t.prefix.as_slice(), &s, &t.suffix].concat());
        } else {
            edited = true;
        }
    }
    (m, edited)
}

/// The first check (`<file>:<label>`) of the script that rejects `bad` at `input`, for an
/// attack expected at `check`; `None` before the target source has markers of that group.
pub fn first_failing_check5(bad: &Built, case: &Case5, check: &str, input: usize) -> Option<String> {
    first_failing_check5_in(bad, case, check, input, None)
}

pub fn first_failing_check5_in(bad: &Built, case: &Case5, check: &str, input: usize, src: Option<&str>) -> Option<String> {
    let (obj, label) = parse5(check)?;
    let Some(obj) = obj else {
        // The drawer: the M3 instrument (all markers of drawer.sil).
        return Some(format!("drawer:{}", first_failing_check(bad)));
    };
    let file = check.split(':').next().unwrap();
    let src = src.map(str::to_string).unwrap_or_else(|| source5(obj));
    let tr = truncations5(obj, &group5(&label), &src);
    if tr.is_empty() {
        return None;
    }
    let rejects = |j: usize| -> (bool, bool) {
        let (m, edited) = with_template5(bad, input, &tr[j].1, case);
        (m.finish().run_input(input).is_err(), edited)
    };
    let (last, edited) = rejects(tr.len() - 1);
    assert!(last, "attribution: the whole {file} source accepts the attack at input {input}");
    let (mut lo, mut hi) = (0usize, tr.len() - 1);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if rejects(mid).0 { hi = mid } else { lo = mid + 1 }
    }
    let got = format!("{file}:{}", tr[lo].0);
    Some(if edited { format!("{got} (successor script edited)") } else { got })
}

/// Whether input `input` of `b` passes the whole of `src` (instrument self-test).
pub fn passes_under_source5(b: &Built, case: &Case5, check: &str, input: usize, src: &str) -> bool {
    let (obj, label) = parse5(check).expect("a <file>:<label> check");
    let tr = truncations5(obj.expect("not the drawer"), &group5(&label), src);
    let (m, _) = with_template5(b, input, &tr.last().expect("markers").1, case);
    m.finish().run_input(input).is_ok()
}

// ---------------------------------------------------------------- Chain: M5 steps

/// Plain funding and owner inputs of the M5 steps are created this many DAA before the step.
const FUND_LEAD: u64 = 100;

impl Chain {
    pub fn reserve(&self) -> u64 {
        self.controller.1.reserve as u64
    }

    /// The vault UTXO's value (total_assets + pend_dep + R).
    pub fn vault_value(&self) -> u64 {
        self.ledger.get(&self.vault.0).expect("vault UTXO").amount
    }

    /// The DAA at which a vault request asked for at `daa` runs: no earlier than the
    /// vault's creation + VAULT_REQ_DELAY (D10/W12, U-4), so chains wait out the age.
    pub fn request_daa(&self, daa: u64) -> u64 {
        daa.max(self.vault.2 + VAULT_REQ_DELAY as u64)
    }

    /// A deposit request of `d` sompi for `owner`, funded by one input of d + 10 KAS created
    /// at the step's DAA - FUND_LEAD; lock_time = the step's DAA = max(`daa`, vault creation
    /// + VAULT_REQ_DELAY) (`request_daa`); and the outpoints.
    pub fn deposit_case(&mut self, owner: &Payout, d: i64, daa: u64) -> (DepositCase, Vec<TransactionOutpoint>) {
        let daa = self.request_daa(daa);
        let mut c = DepositCase::new_with(&self.t);
        c.vault = self.vault.1.clone();
        c.at_age(self.vault.2);
        c.lock_time = daa;
        c.reserve = self.reserve();
        c.d = d;
        c.owner = *owner;
        c.cov = self.cov;
        let v = d as u64 + 10 * KAS;
        let fop = self.ledger.fund(v, funding_spk(), daa - FUND_LEAD);
        c.funding = vec![FundingIn { value: v, daa: daa - FUND_LEAD }];
        assert_eq!(c.vault_value(), self.vault_value(), "deposit_case: the tracked vault value");
        (c, vec![self.vault.0, fop])
    }

    /// Applied at max(`daa`, the case's lock_time).
    pub fn deposit(&mut self, c: &DepositCase, ops: Vec<TransactionOutpoint>, daa: u64) -> Final {
        let daa = daa.max(c.lock_time);
        let x = c.expected();
        let mut b = c.build_with(self.t.clone());
        b.outpoints = ops;
        let f = accept5("request_deposit", &Case5::Deposit(c.clone()), &b);
        check_deposit(c, &f);
        let outs = self.ledger.apply(&f.tx, &f.entries, daa).expect("deposit applies");
        self.vault = (outs[0], x.vault.clone(), daa);
        self.requests.push((outs[1], x.request.clone(), x.values[1], daa));
        f
    }

    /// Run and apply a deposit request in one step; returns the request's index.
    pub fn deposit_now(&mut self, owner: &Payout, d: i64, daa: u64) -> usize {
        let (c, ops) = self.deposit_case(owner, d, daa);
        self.deposit(&c, ops, daa);
        self.requests.len() - 1
    }

    /// A withdrawal of `w` shares of position `pi`, the owner input (10 KAS, `key`'s script)
    /// created at the step's DAA - FUND_LEAD; lock_time = the step's DAA (`request_daa`);
    /// and the outpoints.
    pub fn withdraw_case(&mut self, pi: usize, w: i64, key: OwnerKey, daa: u64) -> (WithdrawCase, Vec<TransactionOutpoint>) {
        let daa = self.request_daa(daa);
        let (pop, ps, pv, pdaa) = self.positions[pi].clone();
        let mut c = WithdrawCase::new_with(&self.t, key.clone());
        c.vault = self.vault.1.clone();
        c.at_age(self.vault.2);
        c.lock_time = daa;
        c.reserve = self.reserve();
        assert_eq!(pv, c.reserve, "position value = R");
        c.position = ps;
        c.position_daa = pdaa;
        c.w = w;
        c.cov = self.cov;
        let oop = self.ledger.fund(10 * KAS, key.spk(), daa - FUND_LEAD);
        c.owner_value = 10 * KAS;
        c.owner_daa = daa - FUND_LEAD;
        (c, vec![self.vault.0, pop, oop])
    }

    /// Applied at max(`daa`, the case's lock_time).
    pub fn withdraw(&mut self, c: &WithdrawCase, ops: Vec<TransactionOutpoint>, daa: u64) -> Final {
        let daa = daa.max(c.lock_time);
        let x = c.expected();
        let mut b = c.build_with(self.t.clone());
        b.outpoints = ops.clone();
        let f = accept5("request_withdraw", &Case5::Withdraw(c.clone()), &b);
        check_withdraw(c, &f);
        let outs = self.ledger.apply(&f.tx, &f.entries, daa).expect("withdraw applies");
        self.vault = (outs[0], x.vault.clone(), daa);
        self.requests.push((outs[1], x.request.clone(), x.values[1], daa));
        let pi = self.positions.iter().position(|p| p.0 == ops[1]).expect("withdrawn position tracked");
        self.positions.remove(pi);
        if let Some(p) = &x.position {
            self.positions.push((outs[2], p.clone(), x.values[2], daa));
        }
        f
    }

    /// The index in `records` of the live record of `round`.
    pub fn record_of(&self, round: i64) -> usize {
        self.records.iter().position(|r| r.1.round == round).unwrap_or_else(|| panic!("no live record of round {round}"))
    }

    /// A settlement of request `qi` against the live record of its round, one funding input
    /// of 10 KAS created at `daa` - FUND_LEAD; and the outpoints.
    pub fn settle_case(&mut self, qi: usize, daa: u64) -> (SettleCase, Vec<TransactionOutpoint>) {
        let (qop, rq, qv, qdaa) = self.requests[qi].clone();
        let ri = self.record_of(rq.round);
        let (rop, rc, rv, rdaa) = self.records[ri].clone();
        let mut c = SettleCase::new_with(&self.t, rc, rq);
        c.request_value = qv;
        c.request_daa = qdaa;
        c.record_daa = rdaa;
        c.record_reserve = self.reserve();
        assert_eq!(c.record_value(), rv, "settle_case: the tracked record value");
        c.cov = self.cov;
        let fop = self.ledger.fund(10 * KAS, funding_spk(), daa - FUND_LEAD);
        c.funding = vec![FundingIn { value: 10 * KAS, daa: daa - FUND_LEAD }];
        (c, vec![qop, rop, fop])
    }

    pub fn settle(&mut self, c: &SettleCase, ops: Vec<TransactionOutpoint>, daa: u64) -> Final {
        let x = c.expected();
        assert!(x.reference.is_some(), "Chain::settle of a settlement the reference rejects");
        let mut b = c.build_with(self.t.clone());
        b.outpoints = ops.clone();
        let f = accept5("settle", &Case5::Settle(c.clone()), &b);
        check_settle(c, &f);
        let outs = self.ledger.apply(&f.tx, &f.entries, daa).expect("settle applies");
        let ri = self.records.iter().position(|r| r.0 == ops[1]).expect("record tracked");
        match x.record_output {
            Some(o) => self.records[ri] = (outs[o], x.record.clone(), x.values[o], daa),
            None => {
                self.records.remove(ri);
            }
        }
        let qi = self.requests.iter().position(|q| q.0 == ops[0]).expect("request tracked");
        self.requests.remove(qi);
        if let Some(o) = x.cov2_output {
            if let Some(p) = &x.position {
                self.positions.push((outs[o], p.clone(), x.values[o], daa));
            }
            if let Some(r) = &x.request {
                self.requests.push((outs[o], r.clone(), x.values[o], daa));
            }
        }
        f
    }

    /// Settle request `qi` now; returns the settlement's reference.
    pub fn settle_now(&mut self, qi: usize, daa: u64) -> SettleExpected {
        let (c, ops) = self.settle_case(qi, daa);
        let x = c.expected();
        self.settle(&c, ops, daa);
        x
    }

    /// `refresh()` of an M5 object (`kind`: "position", "request" or "record", index `i`) at
    /// lock_time = its creation + D_IDLE; and the outpoints.
    pub fn refresh5_case(&mut self, kind: &str, i: usize, daa: u64) -> (RefreshCase, Vec<TransactionOutpoint>) {
        let (op, obj, value, odaa) = match kind {
            "position" => {
                let p = &self.positions[i];
                (p.0, RefreshObj::Position(p.1.clone()), p.2, p.3)
            }
            "request" => {
                let p = &self.requests[i];
                (p.0, RefreshObj::Request(p.1.clone()), p.2, p.3)
            }
            "record" => {
                let p = &self.records[i];
                (p.0, RefreshObj::Record(p.1.clone()), p.2, p.3)
            }
            _ => panic!("refresh5_case: {kind}"),
        };
        let mut c = RefreshCase::new(obj, value, odaa);
        c.b_max = self.t.b_max;
        c.cov = self.cov;
        let fop = self.ledger.fund(10 * KAS, funding_spk(), daa - FUND_LEAD);
        c.funding = vec![FundingIn { value: 10 * KAS, daa: daa - FUND_LEAD }];
        (c, vec![op, fop])
    }

    pub fn refresh5(&mut self, c: &RefreshCase, ops: Vec<TransactionOutpoint>, daa: u64) -> Final {
        let mut b = c.build_with(self.t.clone());
        b.outpoints = ops.clone();
        let f = accept5(&format!("refresh ({})", c.obj.name()), &Case5::Refresh(c.clone()), &b);
        let outs = self.ledger.apply(&f.tx, &f.entries, daa).expect("refresh applies");
        macro_rules! upd {
            ($v:expr) => {{
                let i = $v.iter().position(|p| p.0 == ops[0]).expect("refreshed object tracked");
                $v[i].0 = outs[0];
                $v[i].3 = daa;
            }};
        }
        match &c.obj {
            RefreshObj::Position(_) => upd!(self.positions),
            RefreshObj::Request(_) => upd!(self.requests),
            RefreshObj::Record(_) => upd!(self.records),
            _ => panic!("refresh5 of an M4 object"),
        }
        f
    }

    /// Close the current round at T with fresh funding, applied at T + 10 (as m4 KP12).
    pub fn close_now(&mut self) {
        let t = self.controller.1.deadline as u64;
        let (mut c, ops) = self.close_case(t - 500);
        c.lock_time = t;
        self.close(&c, ops, t + 10);
    }

    /// A draw case of the closed round (seed, no absorption), funding fresh after
    /// seed_ref + DRAW_WINDOW; applied by `draw` at lock_time + 10.
    pub fn draw_now(&mut self, seed: [u8; 32]) -> (DrawCase, Final) {
        let seed_ref = self.marker.as_ref().expect("a closed round to draw").1.seed_ref;
        let (c, ops) = self.draw_case(seed, &[], (seed_ref + lottery::game::DRAW_WINDOW + 1_000) as u64);
        let f = self.draw(&c, ops, c.lock_time + 10);
        (c, f)
    }

    /// VP14 share invariants (S2-9, the spec's "total_shares = sum positions + pend_wsh"
    /// generalized to undrained records): total_shares = sum positions + pend_wsh + sum of
    /// the live records' mint_rem (minted, not yet settled into positions), and the live
    /// withdrawal requests' shares = pend_wsh + sum of the records' burn_rem (converted,
    /// not yet settled). With every record drained they are the spec's form. Panics with
    /// the numbers if either fails.
    pub fn assert_share_invariants(&self, step: &str) {
        let v = &self.vault.1;
        let pos: i64 = self.positions.iter().map(|p| p.1.shares).sum();
        let wreq: i64 = self.requests.iter().filter(|q| q.1.kind == KIND_WITHDRAW).map(|q| q.1.amount).sum();
        let mint: i64 = self.records.iter().map(|r| r.1.mint_rem).sum();
        let burn: i64 = self.records.iter().map(|r| r.1.burn_rem).sum();
        assert_eq!(v.total_shares, pos + v.pend_wsh + mint, "{step}: total_shares = positions {pos} + pend_wsh {} + records' mint_rem {mint}", v.pend_wsh);
        assert_eq!(wreq, v.pend_wsh + burn, "{step}: withdrawal requests {wreq} = pend_wsh {} + records' burn_rem {burn}", v.pend_wsh);
    }
}

/// Genesis (K = 4, lower shares [400, 300, 300, 0, 0]) for the M5 chains.
pub fn chain5() -> Chain {
    Chain::genesis_with(4, |g| g.controller.st = [400, 300, 300, 0, 0], true)
}

/// The deployer's owner key (the genesis position's default owner, S1-5).
pub fn deployer() -> OwnerKey {
    OwnerKey::Schnorr(FUNDING_SK)
}

// ---------------------------------------------------------------- positive checks (VP13)

/// Every output of a deposit request as `DepositExpected`, the covenant bindings, the fee,
/// and conservation: vault + funding = vault' + request + change + fee, vault' = vault + D.
pub fn check_deposit(c: &DepositCase, f: &Final) {
    let x = c.expected();
    let t = tpl();
    assert_eq!(f.tx.outputs.len(), x.values.len(), "A4.1 output count");
    for (i, (o, v)) in f.tx.outputs.iter().zip(&x.values).enumerate() {
        assert_eq!(o.value, *v, "output {i} value");
    }
    assert_eq!(f.tx.outputs[0].script_public_key, t.vault_spk(&x.vault), "vault' = vault with pend_dep + D");
    assert_eq!(f.tx.outputs[1].script_public_key, t.request_spk(&x.request), "the request");
    assert_eq!(x.request, RequestState::new(c.vault.round + 1, KIND_DEPOSIT, c.d, &c.owner));
    assert_eq!(f.tx.outputs[0].value, c.vault_value() + c.d as u64, "vault' value + D");
    assert_eq!(f.tx.outputs[1].value, c.reserve, "request value R");
    for (i, o) in f.tx.outputs.iter().enumerate() {
        assert_eq!(o.covenant.is_some(), i < 2, "output {i} binding");
        if let Some(b) = o.covenant {
            assert_eq!(b, CovenantBinding::new(0, c.cov));
        }
    }
    assert_eq!(f.fee(), c.fee, "inputs - outputs = fee");
}

/// As `check_deposit` for a withdrawal request: vault' value unchanged, pend_wsh + w; the
/// request (R); position' (shares - w, R) when partial; conservation.
pub fn check_withdraw(c: &WithdrawCase, f: &Final) {
    let x = c.expected();
    let t = tpl();
    assert_eq!(f.tx.outputs.len(), x.values.len(), "A4.2 output count");
    for (i, (o, v)) in f.tx.outputs.iter().zip(&x.values).enumerate() {
        assert_eq!(o.value, *v, "output {i} value");
    }
    assert_eq!(f.tx.outputs[0].script_public_key, t.vault_spk(&x.vault));
    assert_eq!(x.vault.pend_wsh, c.vault.pend_wsh + c.w);
    assert_eq!(f.tx.outputs[0].value, c.vault_value(), "vault' value unchanged");
    assert_eq!(f.tx.outputs[1].script_public_key, t.request_spk(&x.request));
    assert_eq!((x.request.round, x.request.kind, x.request.amount), (c.vault.round + 1, KIND_WITHDRAW, c.w));
    assert_eq!((x.request.owner_class, x.request.owner_key), (c.position.owner_class, c.position.owner_key));
    if let Some(p) = &x.position {
        assert_eq!(f.tx.outputs[2].script_public_key, t.position_spk(p));
        assert_eq!(p.shares, c.position.shares - c.w);
    }
    for (i, o) in f.tx.outputs.iter().enumerate() {
        assert_eq!(o.covenant.is_some(), i < x.n_cov(), "output {i} binding");
    }
    assert_eq!(f.fee(), c.fee);
    let cov_in = c.vault_value() + c.reserve;
    let cov_out: u64 = x.values[..x.n_cov()].iter().sum();
    assert_eq!(cov_out, cov_in + if x.partial { c.reserve } else { 0 }, "A4.2 conservation (partial: the funding posts the request's R)");
}

/// Every output of a settlement as `SettleExpected` (= `game::settle`), the scripts, the
/// bindings, the fee, and conservation: request + record = record' + (position | request')
/// + payout + refund.
pub fn check_settle(c: &SettleCase, f: &Final) {
    let x = c.expected();
    let st = x.reference.clone().expect("a valid settlement");
    assert_eq!((x.mode, x.sh, x.conv, x.pay, x.left, x.drained), (st.mode, st.sh, st.conv, st.pay, st.left, st.drained), "builder = game::settle");
    let t = tpl();
    assert_eq!(f.tx.outputs.len(), x.values.len(), "A6.1 output count");
    for (i, (o, v)) in f.tx.outputs.iter().zip(&x.values).enumerate() {
        assert_eq!(o.value, *v, "output {i} value");
    }
    if let Some(o) = x.record_output {
        assert_eq!(f.tx.outputs[o].script_public_key, t.record_spk(&st.record));
    }
    if let Some(o) = x.payout_output {
        let owner = Payout::from_fields(c.request.owner_class, &c.request.owner_key);
        assert_eq!(f.tx.outputs[o].script_public_key, owner.spk());
        let want = match x.mode {
            SettleMode::DepositRefund => c.request.amount as u64 + c.request_value,
            SettleMode::Final => x.pay as u64 + c.request_value,
            _ => x.pay as u64,
        };
        assert_eq!(f.tx.outputs[o].value, want, "payout");
    }
    if let Some(o) = x.refund_output {
        let funder = Payout::from_fields(c.record.funder_class, &c.record.funder_key);
        assert_eq!(f.tx.outputs[o].script_public_key, funder.spk());
        assert_eq!(f.tx.outputs[o].value, c.record_reserve, "R_rc to the funder");
    }
    for (i, o) in f.tx.outputs.iter().enumerate() {
        assert_eq!(o.covenant.is_some(), i < x.n_cov(), "output {i} binding");
    }
    assert_eq!(f.fee(), c.fee);
    let n_fixed = x.values.len() - x.change_output.is_some() as usize;
    let out: u64 = x.values[..n_fixed].iter().sum();
    assert_eq!(out, c.request_value + c.record_value(), "A6.1 conservation");
}

/// The draw's M5 outputs against the reference (`game::convert` at V', VP4/VP13): vault'
/// (C3), the record (state and value), controller' v_rec = total_assets', and conservation
/// on the vault side: vault + R_caller = vault' + record (house settlement V -> V' aside).
pub fn check_draw5(c: &DrawCase, f: &Final) -> lottery::draw::DrawExpected {
    let x = c.expected();
    let t = tpl();
    let conv = convert(x.v_prime, c.vault.total_shares, c.vault.pend_dep, c.vault.pend_wsh, x.fix.liability).expect("an in-range conversion");
    assert_eq!(x.conv, conv, "the builder's conversion is game::convert at V' and L'");
    assert_eq!(x.vault, conv.vault_after(&c.vault, c.marker.round), "vault' = C3");
    assert_eq!(f.tx.outputs[1].script_public_key, t.vault_spk(&x.vault));
    assert_eq!(f.tx.outputs[1].value, conv.total_assets() as u64 + c.reserve(), "vault' value = total_assets' + R");
    assert_eq!(x.controller.v_rec, conv.total_assets(), "D71: v_rec = total_assets'");
    assert_eq!(f.tx.outputs[2].script_public_key, t.controller_spk(&x.controller));
    assert_eq!(x.rec, conv.rec());
    if let (Some(i), Some(rc)) = (x.record_output(), &x.record) {
        assert_eq!(rc, &conv.record(c.marker.round, c.vault.request_tpl, &c.funder).unwrap());
        assert_eq!(f.tx.outputs[i].script_public_key, t.record_spk(rc));
        assert_eq!(f.tx.outputs[i].value, conv.record_value(c.reserve() as i64) as u64);
        assert_eq!(f.tx.outputs[i].covenant, Some(CovenantBinding::new(0, c.cov)));
        assert_eq!(i, 3 + x.np(), "the record at 3 + np");
    }
    // Conservation (5.5, vault side): vault (V + D + R) + the caller's R = vault' + record +
    // (V - V') [the house settlement, unchanged from M3].
    let lhs = c.vault_value() as i128 + if x.rec { c.reserve() as i128 } else { 0 } + (x.v_prime as i128 - c.vault.total_assets as i128);
    let rhs = f.tx.outputs[1].value as i128 + x.record_output().map(|i| f.tx.outputs[i].value as i128).unwrap_or(0);
    assert_eq!(lhs, rhs, "A5 conservation, vault side");
    assert_eq!(f.tx.outputs.len(), x.n_outputs());
    x
}

// ---------------------------------------------------------------- standalone bases (D79)

pub const VLT0: usize = 0;
pub const POS: usize = WithdrawCase::POSITION;
pub const REQ: usize = SettleCase::REQUEST;
pub const REC: usize = SettleCase::RECORD;
pub const DRW5: usize = DrawCase::DRAWER;

pub fn dep() -> DepositCase {
    DepositCase::new()
}

/// The partial withdrawal base with owner `key`.
pub fn wd(key: OwnerKey) -> WithdrawCase {
    WithdrawCase::new_with(&tpl(), key)
}

pub fn wd_whole(key: OwnerKey) -> WithdrawCase {
    wd(key).whole()
}

pub fn keys() -> Vec<(&'static str, OwnerKey)> {
    vec![("Schnorr", OwnerKey::Schnorr(OWNER_SK)), ("ECDSA", OwnerKey::Ecdsa(OWNER_SK)), ("P2SH", OwnerKey::P2sh(OWNER_SK))]
}

fn funder() -> Payout {
    Payout::Schnorr(xonly(&FUNDING_SK))
}

/// A fabricated record (D79) of round 2 with the given remainders, request_tpl and funder of
/// the committed templates.
pub fn record(refund: u8, dep_rem: i64, mint_rem: i64, wsh_rem: i64, burn_rem: i64, out_rem: i64) -> RecordState {
    let mut r = RecordState { round: 2, request_tpl: tpl().request.hash, refund, dep_rem, mint_rem, wsh_rem, burn_rem, out_rem, funder_class: 0, funder_key: [0; 33] };
    r.set_funder(&funder());
    r
}

fn req(kind: u8, amount: i64) -> RequestState {
    RequestState::new(2, kind, amount, &owner().payout())
}

/// Deposit-convert, not drained (the fixture record, 10 of 30 KAS).
pub fn st_dep() -> SettleCase {
    let c = SettleCase::deposit();
    assert_mode(&c, SettleMode::DepositConvert, false);
    c
}

/// Deposit-convert by the last depositor (the record only has deposits): drained.
pub fn st_dep_last() -> SettleCase {
    let c = SettleCase::new(record(0, 10 * KAS as i64, 1_000_000, 0, 0, 0), req(KIND_DEPOSIT, 10 * KAS as i64));
    assert_mode(&c, SettleMode::DepositConvert, true);
    c
}

/// Deposit-refund (refund mode), not drained.
pub fn st_refund() -> SettleCase {
    let c = SettleCase::new(record(1, 30 * KAS as i64, 0, 0, 0, 0), req(KIND_DEPOSIT, 10 * KAS as i64));
    assert_mode(&c, SettleMode::DepositRefund, false);
    c
}

/// Withdrawal FINAL against the fixture record, not drained.
pub fn st_final() -> SettleCase {
    let c = SettleCase::withdrawal();
    assert_mode(&c, SettleMode::Final, false);
    c
}

/// Withdrawal PAY: a capped record (1.5 of 3 M shares converted for 15 KAS); 1 M shares
/// settle: conv 500,000, pay 5 KAS = P_MIN x 100 >= P_MIN, left 500,000.
pub fn st_pay() -> SettleCase {
    let c = SettleCase::new(record(0, 0, 0, 3_000_000, 1_500_000, 15 * KAS as i64), req(KIND_WITHDRAW, 1_000_000));
    assert_mode(&c, SettleMode::Pay, false);
    c
}

/// Withdrawal ACCRUE: as `st_pay` with out_rem = 3 (P_MIN - 1), so pay = P_MIN - 1.
pub fn st_accrue() -> SettleCase {
    let c = SettleCase::new(record(0, 0, 0, 3_000_000, 1_500_000, 3 * (P_MIN - 1)), req(KIND_WITHDRAW, 1_000_000));
    assert_mode(&c, SettleMode::Accrue, false);
    c
}

fn assert_mode(c: &SettleCase, mode: SettleMode, drained: bool) {
    let x = c.expected();
    assert!(x.reference.is_some(), "base rejected by game::settle: {:?}", c.record);
    assert_eq!((x.mode, x.drained), (mode, drained), "settle base");
}

pub fn settle_bases() -> Vec<(&'static str, SettleCase)> {
    vec![("deposit-convert", st_dep()), ("deposit-convert drained", st_dep_last()), ("deposit-refund", st_refund()), ("FINAL", st_final()), ("PAY", st_pay()), ("ACCRUE", st_accrue())]
}

/// Standalone refreshes of a position, a deposit request and the fixture record, created at
/// DAA 1,500,000.
pub fn rf_position() -> RefreshCase {
    RefreshCase::new(RefreshObj::Position(PositionState::new(1, 5_000_000, &owner().payout())), lottery::params::TEST_RESERVE, 1_500_000)
}
pub fn rf_request() -> RefreshCase {
    RefreshCase::new(RefreshObj::Request(req(KIND_DEPOSIT, 10 * KAS as i64)), lottery::params::TEST_RESERVE, 1_500_000)
}
pub fn rf_record() -> RefreshCase {
    let c = st_dep();
    RefreshCase::new(RefreshObj::Record(c.record.clone()), c.record_value(), 1_500_000)
}
pub fn refresh5_bases() -> Vec<(&'static str, RefreshCase)> {
    vec![("position", rf_position()), ("request", rf_request()), ("record", rf_record())]
}

/// The M3 attack base (round 2, sales 157 KAS, the jackpot held: V' != V) with `d` sompi
/// and `w` shares pending in the vault.
pub fn d_pending(d: i64, w: i64) -> DrawCase {
    let mut c = super::m3::base();
    c.vault.pend_dep = d;
    c.vault.pend_wsh = w;
    c
}

/// Deposits and withdrawals pending, within the slack (wc = W).
pub fn d_both() -> DrawCase {
    let c = d_pending(30 * KAS as i64, 3_000_000);
    let x = c.expected();
    assert!(x.rec && x.conv.ok && x.conv.wc == 3_000_000, "{:?}", x.conv);
    c
}

/// Deposits only.
pub fn d_dep() -> DrawCase {
    d_pending(30 * KAS as i64, 0)
}

/// Withdrawals only, within the slack.
pub fn d_wd() -> DrawCase {
    d_pending(0, 3_000_000)
}

/// The cap binds (VP6): 95 % of the shares pending withdrawal, 30 KAS deposited.
pub fn d_cap() -> DrawCase {
    let mut c = d_pending(30 * KAS as i64, 0);
    c.vault.pend_wsh = c.vault.total_shares / 100 * 95;
    let x = c.expected();
    assert!(x.conv.wc >= 1 && x.conv.wc < x.conv.w, "cap base: wc {} of W {}", x.conv.wc, x.conv.w);
    c
}

/// Refund mode by price (VP9 a): V' large against S, so mint < ceil(D / MIN_DEPOSIT).
pub fn d_refund_price() -> DrawCase {
    let mut c = DrawCase::new(4);
    // V' = V (no sales, no holders); price (V' + 1) / (S + 1000) = 2 x 10^12 / 1,199, about
    // 1.67 x 10^9 sompi per share. D = 3 MIN_DEPOSIT = 30 KAS mints floor(1.80) = 1 < 3.
    // M6 (D85): V was 2,000 KAS and D 3 KAS (at MIN_DEPOSIT = 1 KAS); both scale by 10.
    c.vault.total_assets = 20_000 * KAS as i64;
    c.vault.total_shares = 199;
    c.vault.liability = 0;
    c.controller.v_rec = c.vault.total_assets;
    c.vault.pend_dep = 3 * MIN_DEPOSIT;
    let x = c.expected();
    assert!(x.conv.refund, "price refund base: {:?}", x.conv);
    c
}

// ---------------------------------------------------------------- VA1

pub fn va1() -> Vec<Atk5> {
    let zero_sh = SettleCase::new(record(0, 3 * MIN_DEPOSIT, 2, 0, 0, 0), req(KIND_DEPOSIT, MIN_DEPOSIT));
    vec![
        Atk5::new("VA1 request_deposit D = MIN_DEPOSIT - 1", VLT0, "vault:D5", dep()).dep(|c| c.d = MIN_DEPOSIT - 1),
        Atk5::new("VA1 request_deposit D = 0", VLT0, "vault:D5", dep()).dep(|c| c.d = 0),
        // The record mints 2 shares for 3 MIN_DEPOSIT: a settler of MIN_DEPOSIT gets
        // floor(2 / 3) = 0 shares; the cheater writes a zero-share position (sh = 0 follows).
        Atk5::new("VA1 settle: a fabricated record where sh = 0", REQ, "request:S4", st_dep()).st(move |c| {
            *c = zero_sh.clone();
            c.lie.sh = Some(0);
        }),
        Atk5::new("VA1 draw claiming conversion (refund = 0) when mint < ceil(D / MIN_DEPOSIT)", DRW5, "drawer:20", d_refund_price()).dr(|c| c.lie.refund = Some(false)),
    ]
}

// ---------------------------------------------------------------- VA2

pub fn va2() -> Vec<Atk5> {
    vec![
        // The request alone with funding: its value R + the deposit claimed to the owner.
        Atk5::new("VA2 a request spent with no record (only funding)", REQ, "request:S2", st_refund()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.inputs.remove(REC);
            reindex(b);
            let payout = Payout::from_fields(c.request.owner_class, &c.request.owner_key).spk();
            let change = b.outputs.last().unwrap().clone();
            b.outputs = vec![TransactionOutput::new(c.request_value, payout), change];
        }),
        // A deposit request co-spent by a vault entry (request_deposit), as input 1 running
        // `settle`: the request rejects (not the leader) and so does the vault.
        Atk5::new("VA2 a request spent by a vault entry (at the request, input 1)", 1, "request:S1", dep()).built(|b, _| {
            let s = st_refund().build_with(b.templates.clone());
            b.inputs.insert(1, s.inputs[REQ].clone());
            reindex(b);
        }),
        Atk5::new("VA2 a request spent by a vault entry (at the vault)", VLT0, "vault:D2", dep()).built(|b, _| {
            let s = st_refund().build_with(b.templates.clone());
            b.inputs.insert(1, s.inputs[REQ].clone());
            reindex(b);
        }),
        // FINAL: the shares returned to a position (authorized by input 0) instead of the
        // payout.
        Atk5::new("VA2 a withdrawal request whose shares are returned to a position", REQ, "request:S7", st_final()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            let x = c.expected();
            let po = x.payout_output.unwrap();
            let p = PositionState { round: c.record.round, shares: c.request.amount, owner_class: c.request.owner_class, owner_key: c.request.owner_key };
            b.outputs[po] = TransactionOutput::with_covenant(c.request_value, b.templates.position_spk(&p), Some(CovenantBinding::new(0, c.cov)));
            let last = b.outputs.len() - 1;
            b.outputs[last].value += x.pay as u64;
        }),
    ]
}

// ---------------------------------------------------------------- VA3

/// Templates whose record is a look-alike (the real one plus `backdoor()`), compiled once.
pub fn fake_record() -> Arc<Templates> {
    static T: OnceLock<Arc<Templates>> = OnceLock::new();
    T.get_or_init(|| lookalike(Sil::Record)).clone()
}

pub fn fake_request() -> Arc<Templates> {
    static T: OnceLock<Arc<Templates>> = OnceLock::new();
    T.get_or_init(|| lookalike(Sil::Request)).clone()
}

pub fn fake_vault() -> Arc<Templates> {
    static T: OnceLock<Arc<Templates>> = OnceLock::new();
    T.get_or_init(|| lookalike(Sil::Vault)).clone()
}

pub fn va3() -> Vec<Atk5> {
    let other_cov = |b: &mut Built, _: &Case5| set_input_cov(b, REC, Some(OTHER_COV));
    vec![
        Atk5::new("VA3 request round t against the record of t - 1", REQ, "request:S3", st_dep()).st(|c| c.record.round -= 1),
        Atk5::new("VA3 request round t against the record of t + 1", REQ, "request:S3", st_dep()).st(|c| c.record.round += 1),
        Atk5::new("VA3 the record of t + 1 at the record leg (RL4: round = own round)", REC, "record:RL4", st_dep()).st(|c| c.record.round += 1),
        // The same record state under a look-alike record template (its record' too); the
        // settle lengths are the look-alike's, so only RECORD_TPL can reject it.
        Atk5::new("VA3 a look-alike record template", REQ, "request:S3", st_dep()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            let fake = fake_record();
            let e = b.inputs[REC].entry.clone();
            b.inputs[REC] = In {
                entry: UtxoEntry::new(e.amount, fake.record_spk(&c.record), e.block_daa_score, false, e.covenant_id),
                kind: InKind::Entry { template: fake.record.clone(), entry: "backdoor".into(), args: vec![], redeem: fake.record_redeem(&c.record) },
            };
            b.outputs[0].script_public_key = fake.record_spk(&c.expected().record);
            let a = b.entry_args_mut(REQ);
            a[SETTLE_ARG_REC_PL] = ArtifactValue::Int(fake.record.prefix.len() as i64);
            a[SETTLE_ARG_REC_SL] = ArtifactValue::Int(fake.record.suffix.len() as i64);
        }),
        Atk5::new("VA3 a record under another covenant ID (at the request)", REQ, "request:S2", st_dep()).built(other_cov),
        Atk5::new("VA3 a record under another covenant ID (at the record leg)", REC, "record:RL2", st_dep()).built(other_cov),
    ]
}

// ---------------------------------------------------------------- VA4 (script half)

/// VA4: request' (round t + 1, left) of a PAY settlement against record' of round t.
pub fn va4() -> Vec<Atk5> {
    vec![Atk5::new("VA4 request' (round t + 1) against record t", REQ, "request:S3", st_pay()).st(|c| {
        let x = c.expected();
        c.request = x.request.clone().unwrap();
        c.request_value = x.values[x.cov2_output.unwrap()];
        c.record = x.record.clone();
    })]
}

// ---------------------------------------------------------------- VA5

pub fn va5() -> Vec<Atk5> {
    vec![
        Atk5::new("VA5 wc + 1 (vault' total_assets < L')", DRW5, "drawer:C2", d_cap()).dr(|c| c.lie.wc = Some(c.expected().conv.wc + 1)),
        Atk5::new("VA5 a non-maximal wc (wc - 1)", DRW5, "drawer:C2", d_cap()).dr(|c| c.lie.wc = Some(c.expected().conv.wc - 1)),
        Atk5::new("VA5 out_c + 1 (vault' total_assets - 1, record out_rem + 1)", DRW5, "drawer:20", d_cap()).dr(|c| c.lie.out_delta = 1),
        Atk5::new("VA5 wc = W when capped", DRW5, "drawer:C2", d_cap()).dr(|c| c.lie.wc = Some(c.vault.pend_wsh)),
    ]
}

// ---------------------------------------------------------------- VA6

/// The deposit `c` with D = `d`, funded to cover it (engine-only values above MAX_SOMPI
/// allowed, S4-3). The vault (and its template fields) is kept, so the case also builds
/// under a mutant's templates (Stage 6).
fn fund_dep(c: &mut DepositCase, d: i64) {
    c.d = d;
    c.funding = vec![FundingIn { value: d as u64 + 10 * KAS, daa: 1_100_000 }];
}

/// The VA6 mint-guard base: V' = 10 KAS, S = SHARES_MAX / 2, so q = floor(N / M) is about
/// 2.3 x 10^9 and the pre-guard bound (SHARES_MAX - S) / q is about 10 KAS; D is set to
/// the bound (`above` = false) or the bound + 1.
pub fn d_mint_guard(above: bool) -> DrawCase {
    let mut c = DrawCase::new(4);
    c.vault.total_assets = 10 * KAS as i64;
    c.vault.liability = 0;
    c.controller.v_rec = c.vault.total_assets;
    c.vault.total_shares = SHARES_MAX / 2;
    let (s, n, m) = (c.vault.total_shares, c.vault.total_shares + lottery::game::VS, c.expected().v_prime + lottery::game::VA);
    let q = n / m;
    assert!(q >= 1);
    c.vault.pend_dep = (SHARES_MAX - s) / q + above as i64;
    c
}

/// Round 2 with sales 157 KAS and no holder (no pool is paid: V' = V + H), with `d` and `w`
/// pending.
pub fn d_no_pools(d: i64, w: i64) -> DrawCase {
    let mut c = DrawCase::at_round(4, 2);
    c.set_sales(157 * KAS as i64, 5 * KAS as i64);
    c.vault.pend_dep = d;
    c.vault.pend_wsh = w;
    c
}

pub fn va6() -> Vec<Atk5> {
    let flip = |c: &mut DrawCase| {
        let refund = c.expected().conv.refund;
        c.lie.refund = Some(!refund);
    };
    vec![
        Atk5::new("VA6 draw: total_shares = SHARES_MAX + 1 (fabricated vault)", DRW5, "drawer:5", d_both()).dr(|c| c.vault.total_shares = SHARES_MAX + 1),
        Atk5::new("VA6 request_deposit: total_shares = SHARES_MAX + 1 (fabricated vault)", VLT0, "vault:D1", dep()).dep(|c| c.vault.total_shares = SHARES_MAX + 1),
        Atk5::new("VA6 request_withdraw: total_shares = SHARES_MAX + 1 (fabricated vault)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.total_shares = SHARES_MAX + 1),
        // V' = V + H = TOTAL_MAX = ASSETS_MAX + 1 with V = TOTAL_MAX - H <= ASSETS_MAX
        // (engine only: values above MAX_SOMPI, S4-3). U-2: rejected by check 14's guard
        // hAmt <= ASSETS_MAX - vt.total_assets (the drawer change is Stage 4).
        Atk5::new("VA6 V' = ASSETS_MAX + 1 (fabricated vault, engine only)", DRW5, "drawer:14", d_no_pools(MIN_DEPOSIT, 0)).dr(|c| {
            let h = c.expected().amounts.h;
            assert!(h >= 1);
            c.vault.total_assets = TOTAL_MAX - h;
        }),
        Atk5::new("VA6 request_deposit D = TOTAL_MAX - total_assets - pend_dep + 1 (engine only)", VLT0, "vault:D5", dep())
            .dep(|c| {
                let d = TOTAL_MAX - c.vault.total_assets - c.vault.pend_dep + 1;
                fund_dep(c, d)
            }),
        Atk5::new("VA6 request_withdraw w = shares + 1", VLT0, "vault:W4", wd(owner())).wd(|c| c.w = c.position.shares + 1),
        Atk5::new("VA6 request_withdraw pend_wsh + w > total_shares", VLT0, "vault:W4", wd(owner())).wd(|c| c.vault.pend_wsh = c.vault.total_shares - c.w + 1),
        Atk5::new("VA6 request_withdraw w = 0", VLT0, "vault:W4", wd(owner())).wd(|c| c.w = 0),
        Atk5::new("VA6 mint guard: D = (SHARES_MAX - S) / q, decision flipped", DRW5, "drawer:20", d_mint_guard(false)).dr(flip),
        Atk5::new("VA6 mint guard: D = (SHARES_MAX - S) / q + 1, decision flipped", DRW5, "drawer:20", d_mint_guard(true)).dr(flip),
    ]
}

// ---------------------------------------------------------------- VA7

fn record_out(edit: fn(&mut RecordState)) -> impl Fn(&mut Built, &Case5) + 'static {
    move |b, c| {
        let Case5::Draw(c) = c else { unreachable!() };
        let x = c.expected();
        let mut r = x.record.clone().unwrap();
        edit(&mut r);
        b.outputs[x.record_output().unwrap()].script_public_key = b.templates.record_spk(&r);
    }
}

pub fn va7() -> Vec<Atk5> {
    vec![
        Atk5::new("VA7 deposits converted at V (pre-settlement)", DRW5, "drawer:23", d_dep()).dr(|c| c.lie.conv_v = Some(c.vault.total_assets)),
        Atk5::new("VA7 withdrawals paid at V (pre-settlement)", DRW5, "drawer:20", d_wd()).dr(|c| c.lie.conv_v = Some(c.vault.total_assets)),
        Atk5::new("VA7 deposits converted at V' + D (post-deposit)", DRW5, "drawer:23", d_dep()).dr(|c| c.lie.conv_v = Some(c.expected().v_prime + c.vault.pend_dep)),
        Atk5::new("VA7 withdrawals paid at V' + D (post-deposit)", DRW5, "drawer:20", d_both()).dr(|c| c.lie.conv_v = Some(c.expected().v_prime + c.vault.pend_dep)),
        Atk5::new("VA7 record mint_rem + 1", DRW5, "drawer:26", d_both()).built(record_out(|r| r.mint_rem += 1)),
        Atk5::new("VA7 record mint_rem - 1", DRW5, "drawer:26", d_both()).built(record_out(|r| r.mint_rem -= 1)),
        Atk5::new("VA7 record out_rem + 1", DRW5, "drawer:26", d_both()).built(record_out(|r| r.out_rem += 1)),
        Atk5::new("VA7 record out_rem - 1", DRW5, "drawer:26", d_both()).built(record_out(|r| r.out_rem -= 1)),
        Atk5::new("VA7 record burn_rem + 1", DRW5, "drawer:26", d_cap()).built(record_out(|r| r.burn_rem += 1)),
        Atk5::new("VA7 record burn_rem - 1", DRW5, "drawer:26", d_both()).built(record_out(|r| r.burn_rem -= 1)),
        Atk5::new("VA7 record out_rem + 1 with its value + 1", DRW5, "drawer:20", d_both()).built(|b, c| {
            record_out(|r| r.out_rem += 1)(b, c);
            let Case5::Draw(c) = c else { unreachable!() };
            b.shift_value(c.expected().record_output().unwrap(), 1);
        }),
        Atk5::new("VA7 settlement sh + 1 (record' follows)", REQ, "request:S11", st_dep()).st(|c| c.lie.sh = Some(c.expected().sh + 1)),
        Atk5::new("VA7 settlement sh - 1 (record' follows)", REQ, "request:S11", st_dep()).st(|c| c.lie.sh = Some(c.expected().sh - 1)),
        Atk5::new("VA7 settlement sh + 1 by the last depositor (drained)", REQ, "request:S12", st_dep_last()).st(|c| c.lie.sh = Some(c.expected().sh + 1)),
        Atk5::new("VA7 settlement sh - 1 by the last depositor (drained)", REQ, "request:S12", st_dep_last()).st(|c| c.lie.sh = Some(c.expected().sh - 1)),
        Atk5::new("VA7 settlement pay + 1 (PAY)", REQ, "request:S9", st_pay()).st(|c| c.lie.pay = Some(c.expected().pay + 1)),
        Atk5::new("VA7 settlement pay - 1 (PAY)", REQ, "request:S9", st_pay()).st(|c| c.lie.pay = Some(c.expected().pay - 1)),
        Atk5::new("VA7 settlement pay + 1 (FINAL)", REQ, "request:S9", st_final()).st(|c| c.lie.pay = Some(c.expected().pay + 1)),
    ]
}

// ---------------------------------------------------------------- VA8

fn vault_out(edit: fn(&mut lottery::state::VaultState, &DrawCase)) -> impl Fn(&mut Built, &Case5) + 'static {
    move |b, c| {
        let Case5::Draw(c) = c else { unreachable!() };
        let mut v = c.expected().vault;
        edit(&mut v, c);
        b.outputs[1].script_public_key = b.templates.vault_spk(&v);
    }
}

pub fn va8() -> Vec<Atk5> {
    vec![
        Atk5::new("VA8 vault' pend_dep not zeroed", DRW5, "drawer:23", d_both()).built(vault_out(|v, c| v.pend_dep = c.vault.pend_dep)),
        Atk5::new("VA8 vault' pend_wsh not reduced by wc", DRW5, "drawer:23", d_both()).built(vault_out(|v, c| v.pend_wsh = c.vault.pend_wsh)),
        Atk5::new("VA8 vault' total_shares without the mint", DRW5, "drawer:23", d_both()).built(vault_out(|v, c| v.total_shares = c.vault.total_shares - c.expected().conv.wc)),
        Atk5::new("VA8 a second record output", DRW5, "drawer:13", d_both()).built(|b, c| {
            let Case5::Draw(c) = c else { unreachable!() };
            extra_cov(b, c.expected().record_output().unwrap());
        }),
        Atk5::new("VA8 a record when nothing is pending", DRW5, "drawer:13", super::m3::base()).dr(|c| c.lie.record = Some(true)),
        Atk5::new("VA8 no record when something is pending", DRW5, "drawer:13", d_both()).dr(|c| c.lie.record = Some(false)),
        Atk5::new("VA8 the record without covenant binding", DRW5, "drawer:13", d_both()).built(|b, c| {
            let Case5::Draw(c) = c else { unreachable!() };
            b.outputs[c.expected().record_output().unwrap()].covenant = None;
        }),
    ]
}

// ---------------------------------------------------------------- VA9

fn dep_request(edit: fn(&mut RequestState)) -> impl Fn(&mut Built, &Case5) + 'static {
    move |b, c| {
        let Case5::Deposit(c) = c else { unreachable!() };
        let mut r = c.expected().request;
        edit(&mut r);
        b.outputs[1].script_public_key = b.templates.request_spk(&r);
    }
}

fn dep_vault(edit: fn(&mut lottery::state::VaultState)) -> impl Fn(&mut Built, &Case5) + 'static {
    move |b, c| {
        let Case5::Deposit(c) = c else { unreachable!() };
        let mut v = c.expected().vault;
        edit(&mut v);
        b.outputs[0].script_public_key = b.templates.vault_spk(&v);
    }
}

fn wd_request(edit: fn(&mut RequestState)) -> impl Fn(&mut Built, &Case5) + 'static {
    move |b, c| {
        let Case5::Withdraw(c) = c else { unreachable!() };
        let mut r = c.expected().request;
        edit(&mut r);
        b.outputs[1].script_public_key = b.templates.request_spk(&r);
    }
}

fn wd_position(edit: fn(&mut PositionState)) -> impl Fn(&mut Built, &Case5) + 'static {
    move |b, c| {
        let Case5::Withdraw(c) = c else { unreachable!() };
        let mut p = c.expected().position.unwrap();
        edit(&mut p);
        b.outputs[2].script_public_key = b.templates.position_spk(&p);
    }
}

pub fn va9() -> Vec<Atk5> {
    vec![
        Atk5::new("VA9 request round = vault round + 2", VLT0, "vault:D9", dep()).built(dep_request(|r| r.round += 1)),
        Atk5::new("VA9 request round = vault round", VLT0, "vault:D9", dep()).built(dep_request(|r| r.round -= 1)),
        Atk5::new("VA9 request kind 2", VLT0, "vault:D9", dep()).built(dep_request(|r| r.kind = 2)),
        Atk5::new("VA9 request kind 1 (a deposit recorded as a withdrawal)", VLT0, "vault:D9", dep()).built(dep_request(|r| r.kind = KIND_WITHDRAW)),
        Atk5::new("VA9 request amount D + 1", VLT0, "vault:D9", dep()).built(dep_request(|r| r.amount += 1)),
        Atk5::new("VA9 request owner other than the argument", VLT0, "vault:D9", dep()).built(dep_request(|r| r.owner_key[0] ^= 1)),
        Atk5::new("VA9 request value R + 1", VLT0, "vault:D7", dep()).built(|b, _| b.shift_value(1, 1)),
        Atk5::new("VA9 request value R - 1", VLT0, "vault:D7", dep()).built(|b, _| b.shift_value(1, -1)),
        Atk5::new("VA9 owner class 3", VLT0, "vault:D6", dep()).dep(|c| c.owner = Payout::Schnorr([0x22; 32])).built(|b, c| {
            let Case5::Deposit(c) = c else { unreachable!() };
            b.entry_args_mut(VLT0)[DEP_ARG_CLASS] = ArtifactValue::Byte(3);
            let mut r = c.expected().request;
            r.owner_class = 3;
            b.outputs[1].script_public_key = b.templates.request_spk(&r);
        }),
        Atk5::new("VA9 owner key padding byte set (class 0)", VLT0, "vault:D6", dep()).built(|b, c| {
            let Case5::Deposit(c) = c else { unreachable!() };
            let mut k = c.owner.key33();
            k[32] = 1;
            b.entry_args_mut(VLT0)[DEP_ARG_KEY] = ArtifactValue::Bytes(k.to_vec());
            let mut r = c.expected().request;
            r.owner_key = k;
            b.outputs[1].script_public_key = b.templates.request_spk(&r);
        }),
        Atk5::new("VA9 vault' total_assets + 1", VLT0, "vault:D8", dep()).built(dep_vault(|v| v.total_assets += 1)),
        Atk5::new("VA9 vault' round + 1", VLT0, "vault:D8", dep()).built(dep_vault(|v| v.round += 1)),
        Atk5::new("VA9 vault' liability - 1", VLT0, "vault:D8", dep()).built(dep_vault(|v| v.liability -= 1)),
        Atk5::new("VA9 vault' pend_wsh + 1", VLT0, "vault:D8", dep()).built(dep_vault(|v| v.pend_wsh += 1)),
        Atk5::new("VA9 vault' request_tpl byte flipped", VLT0, "vault:D8", dep()).built(dep_vault(|v| v.request_tpl[0] ^= 1)),
        Atk5::new("VA9 vault' pend_dep + D + 1 (value + D)", VLT0, "vault:D8", dep()).built(dep_vault(|v| v.pend_dep += 1)),
        Atk5::new("VA9 an extra covenant output (a request copy)", VLT0, "vault:D3", dep()).built(|b, _| extra_cov(b, 1)),
        Atk5::new("VA9 an extra plain output", VLT0, "vault:D4", dep()).built(|b, _| extra_plain(b)),
        Atk5::new("VA9 the change under a fresh genesis covenant ID", VLT0, "vault:D4", dep()).built(|b, _| genesis_output(b, 1, 2)),
        Atk5::new("VA9 a funding input with a covenant ID", VLT0, "vault:D2", dep()).built(|b, _| set_input_cov(b, 1, Some(OTHER_COV))),
        // No funding: the deposit is not covered (inputs < outputs); the engine still runs the
        // scripts, and D2 (2 <= inputs) is what rejects it.
        Atk5::new("VA9 no funding input", VLT0, "vault:D2", dep()).built(|b, _| {
            b.inputs.truncate(1);
            reindex(b);
            b.outputs.truncate(2);
        }),
        Atk5::new("VA9 nine funding inputs", VLT0, "vault:D2", dep()).dep(|c| c.funding = (0..9).map(|_| FundingIn { value: 3 * KAS, daa: 1_100_000 }).collect()),
        // A second vault-like UTXO (the same state) under the same ID, spent through the
        // same entry: a parallel group.
        Atk5::new("VA9 a parallel group (a second vault-like UTXO under the same ID)", VLT0, "vault:D2", dep()).built(|b, _| {
            let v = b.inputs[0].clone();
            b.inputs.insert(1, v);
            reindex(b);
        }),
        Atk5::new("VA9 vault' without covenant binding", VLT0, "vault:D3", dep()).built(|b, _| b.outputs[0].covenant = None),
        Atk5::new("VA9 request_withdraw: request amount w + 1", VLT0, "vault:W10", wd(owner())).built(wd_request(|r| r.amount += 1)),
        Atk5::new("VA9 request_withdraw: request kind 0", VLT0, "vault:W10", wd(owner())).built(wd_request(|r| r.kind = KIND_DEPOSIT)),
        Atk5::new("VA9 request_withdraw: request round + 1", VLT0, "vault:W10", wd(owner())).built(wd_request(|r| r.round += 1)),
        Atk5::new("VA9 request_withdraw: request to another owner", VLT0, "vault:W10", wd(owner())).built(wd_request(|r| r.owner_key[0] ^= 1)),
        Atk5::new("VA9 request_withdraw: position' shares + 1", VLT0, "vault:W11", wd(owner())).built(wd_position(|p| p.shares += 1)),
        Atk5::new("VA9 request_withdraw: position' to another owner", VLT0, "vault:W11", wd(owner())).built(wd_position(|p| p.owner_key[0] ^= 1)),
        Atk5::new("VA9 request_withdraw: position' round + 1", VLT0, "vault:W11", wd(owner())).built(wd_position(|p| p.round += 1)),
        Atk5::new("VA9 request_withdraw: request value R + 1", VLT0, "vault:W8", wd(owner())).built(|b, _| b.shift_value(1, 1)),
        Atk5::new("VA9 request_withdraw: position' value R - 1", VLT0, "vault:W8", wd(owner())).built(|b, _| b.shift_value(2, -1)),
        Atk5::new("VA9 request_withdraw: vault' value - 1", VLT0, "vault:W8", wd(owner())).built(|b, _| b.shift_value(0, -1)),
        Atk5::new("VA9 request_withdraw: vault' pend_wsh + w + 1", VLT0, "vault:W9", wd(owner())).built(|b, c| {
            let Case5::Withdraw(c) = c else { unreachable!() };
            let mut v = c.expected().vault;
            v.pend_wsh += 1;
            b.outputs[0].script_public_key = b.templates.vault_spk(&v);
        }),
        Atk5::new("VA9 request_withdraw: whole withdrawal with a position' kept", VLT0, "vault:W6", wd_whole(owner())).built(|b, c| {
            let Case5::Withdraw(c) = c else { unreachable!() };
            let p = PositionState { shares: 1, ..c.position.clone() };
            let last = b.outputs.len() - 1;
            b.outputs[last].value -= c.reserve;
            b.outputs.insert(2, TransactionOutput::with_covenant(c.reserve, b.templates.position_spk(&p), Some(CovenantBinding::new(0, c.cov))));
        }),
        Atk5::new("VA9 request_withdraw: partial withdrawal without position'", VLT0, "vault:W6", wd(owner())).built(|b, _| {
            let p = b.outputs.remove(2);
            let last = b.outputs.len() - 1;
            b.outputs[last].value += p.value;
        }),
        Atk5::new("VA9 request_withdraw: an extra plain output", VLT0, "vault:W7", wd(owner())).built(|b, _| extra_plain(b)),
    ]
}

// ---------------------------------------------------------------- VA10

pub fn va10() -> Vec<Atk5> {
    let mut v = vec![];
    for (name, key) in keys() {
        let k2 = key.clone();
        v.push(
            Atk5::new(format!("VA10 {name}: no owner input (owner_idx at a funding input of another script)"), VLT0, "vault:W5", wd(key.clone())).wd(move |c| {
                c.owner = OwnerKey::Schnorr([0x55; 32]);
                c.position = PositionState::new(c.position.round, c.position.shares, &k2.payout());
            }),
        );
        v.push(Atk5::new(format!("VA10 {name}: owner_idx = 0 (the vault)"), VLT0, "vault:W5", wd(key.clone())).built(|b, _| b.entry_args_mut(VLT0)[WD_ARG_OWNER_IDX] = ArtifactValue::Int(0)));
        v.push(Atk5::new(format!("VA10 {name}: owner_idx = 1 (the position)"), VLT0, "vault:W5", wd(key.clone())).built(|b, _| b.entry_args_mut(VLT0)[WD_ARG_OWNER_IDX] = ArtifactValue::Int(1)));
        v.push(Atk5::new(format!("VA10 {name}: owner_idx = inputs.length"), VLT0, "vault:W5", wd(key.clone())).built(|b, _| {
            let n = b.inputs.len() as i64;
            b.entry_args_mut(VLT0)[WD_ARG_OWNER_IDX] = ArtifactValue::Int(n);
        }));
        v.push(Atk5::new(format!("VA10 {name}: owner_idx = -1"), VLT0, "vault:W5", wd(key.clone())).built(|b, _| b.entry_args_mut(VLT0)[WD_ARG_OWNER_IDX] = ArtifactValue::Int(-1)));
    }
    // A position of another owner (the victim) with the attacker's own input.
    v.push(Atk5::new("VA10 a position of another owner with the attacker's input", VLT0, "vault:W5", wd(owner())).wd(|c| {
        c.position = PositionState::new(c.position.round, c.position.shares, &OwnerKey::Schnorr(OWNER2_SK).payout());
    }));
    // The owner input with the owner's key but another script class (ECDSA vs Schnorr).
    v.push(Atk5::new("VA10 an owner input of another class (ECDSA key, Schnorr owner)", VLT0, "vault:W5", wd(owner())).wd(|c| c.owner = OwnerKey::Ecdsa(OWNER_SK)));
    // The position leg under a look-alike vault at input 0 (spent through `backdoor`; the
    // leg's lengths are the look-alike's, so only VAULT_TPL can reject it).
    v.push(Atk5::new("VA10 the position leg under a look-alike vault", POS, "position:PO4", wd(owner())).built(|b, c| {
        let Case5::Withdraw(c) = c else { unreachable!() };
        let fake = fake_vault();
        let e = b.inputs[0].entry.clone();
        b.inputs[0] = In {
            entry: UtxoEntry::new(e.amount, fake.vault_spk(&c.vault), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fake.vault.clone(), entry: "backdoor".into(), args: vec![], redeem: fake.vault_redeem(&c.vault) },
        };
        *b.entry_args_mut(POS) = vec![ArtifactValue::Int(fake.vault.prefix.len() as i64), ArtifactValue::Int(fake.vault.suffix.len() as i64)];
    }));
    v.push(Atk5::new("VA10 a position at index 2 (at the position)", 2, "position:PO1", wd(owner())).built(|b, _| {
        b.inputs.swap(1, 2);
        reindex(b);
    }));
    v.push(Atk5::new("VA10 a position at index 2 (at the vault)", VLT0, "vault:W2", wd(owner())).built(|b, _| {
        b.inputs.swap(1, 2);
        reindex(b);
    }));
    v.push(Atk5::new("VA10 the position under another covenant ID (at the position)", POS, "position:PO2", wd(owner())).built(|b, _| set_input_cov(b, POS, Some(OTHER_COV))));
    v.push(Atk5::new("VA10 the position under another covenant ID (at the vault)", VLT0, "vault:W2", wd(owner())).built(|b, _| set_input_cov(b, POS, Some(OTHER_COV))));
    v.push(Atk5::new("VA10 the position leg authorizing position' (at the position)", POS, "position:PO3", wd(owner())).built(|b, c| {
        let Case5::Withdraw(c) = c else { unreachable!() };
        b.outputs[2].covenant = Some(CovenantBinding::new(1, c.cov));
    }));
    v.push(Atk5::new("VA10 the position leg authorizing position' (at the vault)", VLT0, "vault:W6", wd(owner())).built(|b, c| {
        let Case5::Withdraw(c) = c else { unreachable!() };
        b.outputs[2].covenant = Some(CovenantBinding::new(1, c.cov));
    }));
    v.push(Atk5::new("VA10 a covenant-less copy of the position as funding (at the copy, input 2)", 2, "position:PO1", wd(owner())).built(|b, _| rogue_copy(b, POS, 2)));
    v.push(Atk5::new("VA10 vault_pl = -1 at the position leg", POS, "position:PO4", wd(owner())).built(|b, _| b.entry_args_mut(POS)[0] = ArtifactValue::Int(-1)));
    v.push(Atk5::new("VA10 pos_pl = SCRIPT_MAX + 1", VLT0, "vault:W1", wd(owner())).built(|b, _| b.entry_args_mut(VLT0)[WD_ARG_POS_PL] = ArtifactValue::Int(1_000_001)));
    v.push(Atk5::new("VA10 a position with role 0x07 (fabricated)", VLT0, "vault:W3", wd(owner())).built(|b, c| {
        let Case5::Withdraw(c) = c else { unreachable!() };
        let mut vals = c.position.values();
        vals.insert("role".into(), ArtifactValue::Byte(0x07));
        b.set_entry_redeem(POS, b.templates.position.redeem(&vals));
    }));
    v.push(Atk5::new("VA10 a position under a look-alike position template", VLT0, "vault:W3", wd(owner())).built(|b, c| {
        let Case5::Withdraw(c) = c else { unreachable!() };
        let fake = lookalike(Sil::Position);
        let e = b.inputs[POS].entry.clone();
        b.inputs[POS] = In {
            entry: UtxoEntry::new(e.amount, fake.position_spk(&c.position), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fake.position.clone(), entry: "backdoor".into(), args: vec![], redeem: fake.position_redeem(&c.position) },
        };
        let a = b.entry_args_mut(VLT0);
        a[WD_ARG_POS_PL] = ArtifactValue::Int(fake.position.prefix.len() as i64);
        a[WD_ARG_POS_SL] = ArtifactValue::Int(fake.position.suffix.len() as i64);
    }));
    v.push(Atk5::new("VA10 the position's value R + 1", VLT0, "vault:W3", wd(owner())).built(|b, _| {
        let e = b.inputs[POS].entry.clone();
        b.inputs[POS].entry = UtxoEntry::new(e.amount + 1, e.script_public_key.clone(), e.block_daa_score, false, e.covenant_id);
        let last = b.outputs.len() - 1;
        b.outputs[last].value += 1;
    }));
    v
}

// ---------------------------------------------------------------- VA11

fn st_out(edit: fn(&mut SettleExpected, &SettleCase)) -> impl Fn(&mut Built, &Case5) + 'static {
    move |b, c| {
        let Case5::Settle(c) = c else { unreachable!() };
        let mut x = c.expected();
        edit(&mut x, c);
        let t = b.templates.clone();
        if let Some(o) = x.record_output {
            b.outputs[o].script_public_key = t.record_spk(&x.record);
        }
        if let (Some(o), Some(p)) = (x.cov2_output, &x.position) {
            b.outputs[o].script_public_key = t.position_spk(p);
        }
        if let (Some(o), Some(r)) = (x.cov2_output, &x.request) {
            b.outputs[o].script_public_key = t.request_spk(r);
        }
    }
}

pub fn va11() -> Vec<Atk5> {
    vec![
        Atk5::new("VA11 position to another owner", REQ, "request:S12", st_dep()).built(st_out(|x, _| x.position.as_mut().unwrap().owner_key[0] ^= 1)),
        Atk5::new("VA11 position shares + 1", REQ, "request:S12", st_dep()).built(st_out(|x, _| x.position.as_mut().unwrap().shares += 1)),
        Atk5::new("VA11 position shares - 1", REQ, "request:S12", st_dep()).built(st_out(|x, _| x.position.as_mut().unwrap().shares -= 1)),
        Atk5::new("VA11 position round + 1", REQ, "request:S12", st_dep()).built(st_out(|x, _| x.position.as_mut().unwrap().round += 1)),
        Atk5::new("VA11 position value + 1", REQ, "request:S9", st_dep()).built(|b, _| b.shift_value(1, 1)),
        Atk5::new("VA11 FINAL payout redirected", REQ, "request:S10", st_final()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.outputs[c.expected().payout_output.unwrap()].script_public_key = other_payout();
        }),
        Atk5::new("VA11 FINAL payout + 1", REQ, "request:S9", st_final()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.shift_value(c.expected().payout_output.unwrap(), 1);
        }),
        Atk5::new("VA11 FINAL payout - 1", REQ, "request:S9", st_final()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.shift_value(c.expected().payout_output.unwrap(), -1);
        }),
        Atk5::new("VA11 refund payout redirected", REQ, "request:S10", st_refund()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.outputs[c.expected().payout_output.unwrap()].script_public_key = other_payout();
        }),
        Atk5::new("VA11 refund payout + 1", REQ, "request:S9", st_refund()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.shift_value(c.expected().payout_output.unwrap(), 1);
        }),
        Atk5::new("VA11 PAY request' amount + 1", REQ, "request:S13", st_pay()).built(st_out(|x, _| x.request.as_mut().unwrap().amount += 1)),
        Atk5::new("VA11 PAY request' round unchanged", REQ, "request:S13", st_pay()).built(st_out(|x, _| x.request.as_mut().unwrap().round -= 1)),
        Atk5::new("VA11 ACCRUE request' amount - 1", REQ, "request:S13", st_accrue()).built(st_out(|x, _| x.request.as_mut().unwrap().amount -= 1)),
        Atk5::new("VA11 ACCRUE request' value without the pay", REQ, "request:S9", st_accrue()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            let x = c.expected();
            b.shift_value(x.cov2_output.unwrap(), -x.pay);
        }),
        Atk5::new("VA11 PAY below P_MIN (an ACCRUE settled as PAY)", REQ, "request:S8", st_accrue()).st(|c| c.lie.mode = Some(SettleMode::Pay)),
        Atk5::new("VA11 ACCRUE at or above P_MIN (a PAY settled as ACCRUE)", REQ, "request:S9", st_pay()).st(|c| c.lie.mode = Some(SettleMode::Accrue)),
        Atk5::new("VA11 record' dep_rem unchanged", REQ, "request:S11", st_dep()).built(st_out(|x, c| x.record.dep_rem = c.record.dep_rem)),
        Atk5::new("VA11 record' round + 1", REQ, "request:S11", st_dep()).built(st_out(|x, _| x.record.round += 1)),
        Atk5::new("VA11 record' request_tpl byte flipped", REQ, "request:S11", st_final()).built(st_out(|x, _| x.record.request_tpl[0] ^= 1)),
        Atk5::new("VA11 record' funder changed", REQ, "request:S11", st_final()).built(st_out(|x, _| x.record.funder_key[0] ^= 1)),
        Atk5::new("VA11 record' refund flag set", REQ, "request:S11", st_dep()).built(st_out(|x, _| x.record.refund = 1)),
        Atk5::new("VA11 record' value + 1", REQ, "request:S9", st_dep()).built(|b, _| b.shift_value(0, 1)),
        Atk5::new("VA11 the record's R refunded to another script", REQ, "request:S10", st_dep_last()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.outputs[c.expected().refund_output.unwrap()].script_public_key = other_payout();
        }),
        Atk5::new("VA11 the record's R refund - 1", REQ, "request:S9", st_dep_last()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.shift_value(c.expected().refund_output.unwrap(), -1);
        }),
        Atk5::new("VA11 a drained record recreated", REQ, "request:S7", st_dep_last()).st(|c| c.lie.drained = Some(true)),
        Atk5::new("VA11 a non-drained record consumed", REQ, "request:S7", st_dep()).st(|c| c.lie.drained = Some(false)),
        Atk5::new("VA11 an extra plain output", REQ, "request:S8", st_dep()).built(|b, _| extra_plain(b)),
        Atk5::new("VA11 the change under a fresh genesis covenant ID", REQ, "request:S8", st_dep()).built(|b, _| genesis_output(b, 2, 2)),
        Atk5::new("VA11 a funding input with a covenant ID", REQ, "request:S2", st_dep()).built(|b, _| set_input_cov(b, 2, Some(OTHER_COV))),
        Atk5::new("VA11 no funding input", REQ, "request:S2", st_dep()).built(|b, _| {
            b.inputs.truncate(2);
            reindex(b);
            b.outputs.pop();
        }),
        Atk5::new("VA11 a request with owner class 3 (fabricated)", REQ, "request:S1", st_final()).st(|c| c.request.owner_class = 3),
        Atk5::new("VA11 a deposit request below MIN_DEPOSIT (fabricated)", REQ, "request:S1", st_refund()).st(|c| c.request.amount = MIN_DEPOSIT - 1),
        Atk5::new("VA11 a record with burn_rem > wsh_rem (fabricated)", REQ, "request:S3", st_final()).st(|c| c.record.burn_rem = c.record.wsh_rem + 1),
        Atk5::new("VA11 a refund record with mint_rem > 0 (fabricated)", REQ, "request:S3", st_refund()).st(|c| c.record.mint_rem = 1),
        Atk5::new("VA11 a record value with R_rc > RESERVE_MAX (fabricated)", REQ, "request:S3", st_dep()).st(|c| c.record_reserve = 10_000_000_001),
        Atk5::new("VA11 a deposit above the record's dep_rem", REQ, "request:S4", st_dep()).st(|c| c.request.amount = c.record.dep_rem + 1),
        Atk5::new("VA11 a withdrawal above the record's wsh_rem", REQ, "request:S4", st_final()).st(|c| c.request.amount = c.record.wsh_rem + 1),
    ]
}

// ---------------------------------------------------------------- VA12

pub fn va12() -> Vec<Atk5> {
    vec![
        Atk5::new("VA12 a look-alike request at input 0", REC, "record:RL4", st_dep()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            let fake = fake_request();
            let e = b.inputs[REQ].entry.clone();
            b.inputs[REQ] = In {
                entry: UtxoEntry::new(e.amount, fake.request_spk(&c.request), e.block_daa_score, false, e.covenant_id),
                kind: InKind::Entry { template: fake.request.clone(), entry: "backdoor".into(), args: vec![], redeem: fake.request_redeem(&c.request) },
            };
            *b.entry_args_mut(REC) = vec![ArtifactValue::Int(fake.request.prefix.len() as i64), ArtifactValue::Int(fake.request.suffix.len() as i64)];
        }),
        Atk5::new("VA12 a deposit position at input 0 (role 0x0B, request lengths)", REC, "record:RL4", st_dep()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            let p = PositionState::new(c.record.round, 1, &owner().payout());
            let e = b.inputs[REQ].entry.clone();
            let t = b.templates.clone();
            b.inputs[REQ] = In {
                entry: UtxoEntry::new(e.amount, t.position_spk(&p), e.block_daa_score, false, e.covenant_id),
                kind: InKind::Entry { template: t.position.clone(), entry: SIG_REFRESH.into(), args: vec![], redeem: t.position_redeem(&p) },
            };
            *b.entry_args_mut(REC) = vec![ArtifactValue::Int(t.position.prefix.len() as i64), ArtifactValue::Int(t.position.suffix.len() as i64)];
        }),
        Atk5::new("VA12 the record at index 0 (at the record)", 0, "record:RL1", st_dep()).built(|b, _| b.inputs.swap(0, 1)),
        Atk5::new("VA12 the record at index 0 (at the request, input 1)", 1, "request:S1", st_dep()).built(|b, _| b.inputs.swap(0, 1)),
        Atk5::new("VA12 the record leg authorizing record' (at the record)", REC, "record:RL3", st_dep()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.outputs[0].covenant = Some(CovenantBinding::new(1, c.cov));
        }),
        Atk5::new("VA12 the record leg authorizing record' (at the request)", REQ, "request:S7", st_dep()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.outputs[0].covenant = Some(CovenantBinding::new(1, c.cov));
        }),
        Atk5::new("VA12 a covenant-less copy of the record as funding (at the copy, input 2)", 2, "record:RL1", st_dep()).built(|b, _| rogue_copy(b, REC, 2)),
        Atk5::new("VA12 req_pl = SCRIPT_MAX + 1 at the record leg", REC, "record:RL4", st_dep()).built(|b, _| b.entry_args_mut(REC)[0] = ArtifactValue::Int(1_000_001)),
        Atk5::new("VA12 rec_pl = -1 at the request", REQ, "request:S1", st_dep()).built(|b, _| b.entry_args_mut(REQ)[SETTLE_ARG_REC_PL] = ArtifactValue::Int(-1)),
    ]
}

// ---------------------------------------------------------------- VA13

fn a7(obj: &str, n: u8) -> &'static str {
    let s = format!("{obj}:A7.{n}");
    Box::leak(s.into_boxed_str())
}

fn successor_changed(obj: &RefreshObj) -> RefreshObj {
    match obj {
        RefreshObj::Position(s) => RefreshObj::Position(PositionState { shares: s.shares + 1, ..s.clone() }),
        RefreshObj::Request(s) => RefreshObj::Request(RequestState { amount: s.amount + 1, ..s.clone() }),
        RefreshObj::Record(s) => RefreshObj::Record(RecordState { mint_rem: s.mint_rem + 1, ..s.clone() }),
        _ => unreachable!(),
    }
}

pub fn va13() -> Vec<Atk5> {
    let mut v = vec![];
    for (obj, base) in refresh5_bases() {
        v.push(Atk5::new(format!("VA13 {obj}: lock_time = creation + D_IDLE - 1"), 0, a7(obj, 3), base.clone()).rf(|c| c.lock_time -= 1));
        v.push(Atk5::new(format!("VA13 {obj}: creation = -1 (unaccepted parent)"), 0, a7(obj, 3), base.clone()).built(|b, _| set_input_daa(b, 0, u64::MAX)));
        v.push(Atk5::new(format!("VA13 {obj}: creation = DAA_MAX"), 0, a7(obj, 3), base.clone()).built(|b, _| set_input_daa(b, 0, DAA_MAX as u64)));
        v.push(Atk5::new(format!("VA13 {obj}: value + 1"), 0, a7(obj, 6), base.clone()).built(|b, _| b.shift_value(0, 1)));
        v.push(Atk5::new(format!("VA13 {obj}: value - 1"), 0, a7(obj, 6), base.clone()).built(|b, _| b.shift_value(0, -1)));
        v.push(Atk5::new(format!("VA13 {obj}: a second covenant input"), 0, a7(obj, 2), base.clone()).built(|b, _| {
            let i = b.inputs[0].clone();
            b.inputs.insert(1, i);
            reindex(b);
        }));
        v.push(Atk5::new(format!("VA13 {obj}: extra covenant output"), 0, a7(obj, 4), base.clone()).built(|b, _| extra_cov(b, 0)));
        v.push(Atk5::new(format!("VA13 {obj}: extra plain output"), 0, a7(obj, 5), base.clone()).built(|b, _| extra_plain(b)));
        v.push(Atk5::new(format!("VA13 {obj}: successor with one field changed"), 0, a7(obj, 7), base.clone()).built(|b, c| {
            let Case5::Refresh(c) = c else { unreachable!() };
            b.outputs[0].script_public_key = successor_changed(&c.obj).spk(&b.templates);
        }));
        v.push(Atk5::new(format!("VA13 {obj}: a covenant-less copy as funding (at the copy, input 1)"), 1, a7(obj, 1), base.clone()).built(|b, _| rogue_copy(b, 0, 1)));
        v.push(Atk5::new(format!("VA13 {obj}: no funding input"), 0, a7(obj, 2), base.clone()).rf(|c| {
            c.funding.clear();
            c.change = false;
            c.fee = 0;
        }));
        v.push(Atk5::new(format!("VA13 {obj}: funding input with a covenant ID"), 0, a7(obj, 2), base.clone()).built(|b, _| set_input_cov(b, 1, Some(OTHER_COV))));
        v.push(Atk5::new(format!("VA13 {obj}: successor without covenant binding"), 0, a7(obj, 4), base.clone()).built(|b, _| b.outputs[0].covenant = None));
        v.push(Atk5::new(format!("VA13 {obj}: the change under a fresh genesis covenant ID"), 0, a7(obj, 5), base.clone()).built(|b, _| genesis_output(b, 1, 1)));
        // Stage 6 (mutant controls, as M4 KA15).
        v.push(Atk5::new(format!("VA13 {obj}: a covenant-less object, plain successor"), 0, a7(obj, 2), base.clone()).built(|b, _| {
            set_input_cov(b, 0, None);
            b.outputs[0].covenant = None;
        }));
        v.push(Atk5::new(format!("VA13 {obj}: nine funding inputs"), 0, a7(obj, 2), base.clone()).rf(|c| c.funding = (0..9).map(|_| FundingIn { value: KAS, daa: 1_400_000 }).collect()));
        v.push(Atk5::new(format!("VA13 {obj}: no output (A7.4 rejects first)"), 0, a7(obj, 4), base.clone()).built(|b, _| b.outputs.clear()));
    }
    v
}

// ---------------------------------------------------------------- Stage 6: boundary and shape attacks

/// Input `i` (an entry input) and, if given, output `o` (its genuine successor, state
/// `succ`) with role byte `role` in their state.
fn role_at(b: &mut Built, i: usize, vals: std::collections::BTreeMap<String, ArtifactValue>, succ: Option<(usize, std::collections::BTreeMap<String, ArtifactValue>)>, role: u8) {
    let t = match &b.inputs[i].kind {
        InKind::Entry { template, .. } => template.clone(),
        _ => panic!("input {i} is not an entry input"),
    };
    b.set_entry_redeem(i, t.redeem(&super::m4::with_role(vals, role)));
    if let Some((o, s)) = succ {
        b.outputs[o].script_public_key = pay_to_script_hash_script(&t.redeem(&super::m4::with_role(s, role)));
    }
}

/// The vault (input 0) of a deposit or withdrawal with role `role`, vault' following.
fn vault_role(role: u8) -> impl Fn(&mut Built, &Case5) + 'static {
    move |b, c| {
        let (v, x) = match c {
            Case5::Deposit(c) => (c.vault.values(), c.expected().vault.values()),
            Case5::Withdraw(c) => (c.vault.values(), c.expected().vault.values()),
            _ => unreachable!(),
        };
        role_at(b, 0, v, Some((0, x)), role);
    }
}

/// Input `i`'s value lowered by `by` and output `o`'s value lowered by the same amount.
fn lower_in_out(b: &mut Built, i: usize, o: Option<usize>, by: u64) {
    let e = b.inputs[i].entry.clone();
    b.inputs[i].entry = UtxoEntry::new(e.amount - by, e.script_public_key.clone(), e.block_daa_score, false, e.covenant_id);
    if let Some(o) = o {
        b.outputs[o].value -= by;
    }
}

/// A settlement against `rc` of a request of `kind` and `amount` (round 2, the fixture owner).
fn st_case(rc: RecordState, kind: u8, amount: i64) -> SettleCase {
    SettleCase::new(rc, req(kind, amount))
}

const RESERVE_MAX: i64 = 10_000_000_000;
const SCRIPT_MAX: i64 = 1_000_000;

/// Stage 6: boundary and shape attacks added for the mutants of m5_mutants.rs (one per
/// tagged range half or shape check no VA1-VA14 attack isolates). Ids carry the spec group
/// they belong to; the expected check is the `m5_spec` label (`M5_ATTRIB` confirms it).
pub fn stage6() -> Vec<Atk5> {
    use ArtifactValue::Int;
    let big = TOTAL_MAX as u64 + 1;
    let st_last = st_dep_last;
    vec![
        // ---- request_deposit (A4.1 D1-D5)
        Atk5::new("VA9 a covenant-less copy of the vault as funding (at the copy, input 1)", 1, "vault:D1", dep()).built(|b, _| rogue_copy(b, 0, 1)),
        Atk5::new("VA6 request_deposit: vault role 0x07 (fabricated)", VLT0, "vault:D1", dep()).built(vault_role(0x07)),
        Atk5::new("VA6 request_deposit: vault round -1 (fabricated)", VLT0, "vault:D1", dep()).dep(|c| c.vault.round = -1),
        Atk5::new("VA6 request_deposit: vault round TOTAL_MAX (fabricated)", VLT0, "vault:D1", dep()).dep(|c| c.vault.round = TOTAL_MAX),
        Atk5::new("VA6 request_deposit: total_assets -1 (fabricated)", VLT0, "vault:D1", dep()).dep(|c| c.vault.total_assets = -1),
        Atk5::new("VA6 request_deposit: total_assets ASSETS_MAX + 1 (fabricated, engine only)", VLT0, "vault:D1", dep()).dep(|c| c.vault.total_assets = TOTAL_MAX),
        Atk5::new("VA6 request_deposit: total_shares -1 (fabricated)", VLT0, "vault:D1", dep()).dep(|c| c.vault.total_shares = -1),
        Atk5::new("VA6 request_deposit: pend_dep -1 (fabricated)", VLT0, "vault:D1", dep()).dep(|c| c.vault.pend_dep = -1),
        Atk5::new("VA6 request_deposit: pend_dep TOTAL_MAX - total_assets + 1 (fabricated, engine only)", VLT0, "vault:D1", dep()).dep(|c| c.vault.pend_dep = TOTAL_MAX - c.vault.total_assets + 1),
        Atk5::new("VA6 request_deposit: pend_wsh -1 (fabricated)", VLT0, "vault:D1", dep()).dep(|c| c.vault.pend_wsh = -1),
        Atk5::new("VA6 request_deposit: pend_wsh total_shares + 1 (fabricated)", VLT0, "vault:D1", dep()).dep(|c| c.vault.pend_wsh = c.vault.total_shares + 1),
        Atk5::new("VA6 request_deposit: liability -1 (fabricated)", VLT0, "vault:D1", dep()).dep(|c| c.vault.liability = -1),
        Atk5::new("VA6 request_deposit: liability TOTAL_MAX + 1 (fabricated)", VLT0, "vault:D1", dep()).dep(|c| c.vault.liability = TOTAL_MAX + 1),
        // R = value - total_assets - pend_dep = -1: the vault's value and vault' lowered by R + 1.
        Atk5::new("VA6 request_deposit: R = -1 (vault value below total_assets + pend_dep)", VLT0, "vault:D1", dep()).built(|b, c| {
            let Case5::Deposit(c) = c else { unreachable!() };
            lower_in_out(b, 0, Some(0), c.reserve + 1);
        }),
        Atk5::new("VA6 request_deposit: R = RESERVE_MAX + 1", VLT0, "vault:D1", dep()).dep(|c| {
            c.reserve = RESERVE_MAX as u64 + 1;
            c.funding = vec![FundingIn { value: c.reserve + c.d as u64 + 10 * KAS, daa: 1_100_000 }];
        }),
        Atk5::new("VA9 only vault' (no request output)", VLT0, "vault:D3", dep()).built(|b, _| b.outputs.truncate(1)),
        // ---- request_withdraw (A4.2 W1-W7)
        Atk5::new("VA10 a covenant-less copy of the vault as funding (at the copy, input 2)", 2, "vault:W1", wd(owner())).built(|b, _| {
            rogue_copy(b, 0, 2);
            b.entry_args_mut(0)[WD_ARG_OWNER_IDX] = Int(3);
            b.entry_args_mut(2)[WD_ARG_OWNER_IDX] = Int(3);
        }),
        Atk5::new("VA6 request_withdraw: vault role 0x07 (fabricated)", VLT0, "vault:W1", wd(owner())).built(vault_role(0x07)),
        Atk5::new("VA6 request_withdraw: vault round -1 (fabricated)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.round = -1),
        Atk5::new("VA6 request_withdraw: vault round TOTAL_MAX (fabricated)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.round = TOTAL_MAX),
        Atk5::new("VA6 request_withdraw: total_assets -1 (fabricated)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.total_assets = -1),
        Atk5::new("VA6 request_withdraw: total_assets ASSETS_MAX + 1 (fabricated, engine only)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.total_assets = TOTAL_MAX),
        Atk5::new("VA6 request_withdraw: total_shares -1 (fabricated)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.total_shares = -1),
        Atk5::new("VA6 request_withdraw: pend_dep -1 (fabricated)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.pend_dep = -1),
        Atk5::new("VA6 request_withdraw: pend_dep TOTAL_MAX - total_assets + 1 (fabricated, engine only)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.pend_dep = TOTAL_MAX - c.vault.total_assets + 1),
        Atk5::new("VA6 request_withdraw: pend_wsh -1 (fabricated)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.pend_wsh = -1),
        Atk5::new("VA6 request_withdraw: pend_wsh total_shares + 1 (fabricated)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.pend_wsh = c.vault.total_shares + 1),
        Atk5::new("VA6 request_withdraw: liability -1 (fabricated)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.liability = -1),
        Atk5::new("VA6 request_withdraw: liability TOTAL_MAX + 1 (fabricated)", VLT0, "vault:W1", wd(owner())).wd(|c| c.vault.liability = TOTAL_MAX + 1),
        Atk5::new("VA6 request_withdraw: R = -1 (vault value below total_assets + pend_dep)", VLT0, "vault:W1", wd(owner())).built(|b, c| {
            let Case5::Withdraw(c) = c else { unreachable!() };
            lower_in_out(b, 0, Some(0), c.reserve + 1);
        }),
        Atk5::new("VA6 request_withdraw: R = RESERVE_MAX + 1", VLT0, "vault:W1", wd(owner())).wd(|c| {
            c.reserve = RESERVE_MAX as u64 + 1;
            c.owner_value = c.reserve + 10 * KAS;
        }),
        Atk5::new("VA10 pos_pl = -1", VLT0, "vault:W1", wd(owner())).built(|b, _| b.entry_args_mut(VLT0)[WD_ARG_POS_PL] = Int(-1)),
        Atk5::new("VA10 pos_sl = -1", VLT0, "vault:W1", wd(owner())).built(|b, _| b.entry_args_mut(VLT0)[WD_ARG_POS_SL] = Int(-1)),
        Atk5::new("VA10 pos_sl = SCRIPT_MAX + 1", VLT0, "vault:W1", wd(owner())).built(|b, _| b.entry_args_mut(VLT0)[WD_ARG_POS_SL] = Int(SCRIPT_MAX + 1)),
        Atk5::new("VA10 the owner input with a covenant ID", VLT0, "vault:W2", wd(owner())).built(|b, _| set_input_cov(b, 2, Some(OTHER_COV))),
        Atk5::new("VA10 nine plain inputs", VLT0, "vault:W2", wd(owner())).wd(|c| c.funding = (0..8).map(|_| FundingIn { value: KAS, daa: 1_100_000 }).collect()),
        // No plain input: a whole withdrawal pays its fee from nothing (fee 0); W2 (3 <= inputs).
        Atk5::new("VA10 no plain input (whole withdrawal, fee 0)", VLT0, "vault:W2", wd_whole(owner())).built(|b, _| {
            b.inputs.truncate(2);
            reindex(b);
            b.outputs.truncate(2);
        }),
        Atk5::new("VA10 a position with role 0x07 (fabricated, whole withdrawal)", VLT0, "vault:W3", wd_whole(owner())).built(|b, c| {
            let Case5::Withdraw(c) = c else { unreachable!() };
            role_at(b, POS, c.position.values(), None, 0x07);
        }),
        Atk5::new("VA10 a position with 0 shares (fabricated)", VLT0, "vault:W3", wd(owner())).wd(|c| {
            c.position.shares = 0;
            c.w = 1;
        }),
        Atk5::new("VA10 a position with SHARES_MAX + 1 shares (fabricated)", VLT0, "vault:W3", wd(owner())).wd(|c| c.position.shares = SHARES_MAX + 1),
        // Owner class 3: payoutSpk's default branch is the P2SH form of the key (vault.sil
        // payoutSpk), so the owner input is the P2SH owner's.
        Atk5::new("VA10 a position with owner class 3, the owner input its P2SH form (fabricated)", VLT0, "vault:W3", wd(OwnerKey::P2sh(OWNER_SK))).wd(|c| c.position.owner_class = 3),
        Atk5::new("VA10 a position with the key padding byte set (class 0, fabricated)", VLT0, "vault:W3", wd(owner())).wd(|c| c.position.owner_key[32] = 1),
        // owner_idx = 0 with a position owned by the P2SH script of the vault's own redeem:
        // only W5's owner_idx >= 2 rejects it (the vault input's script is that owner's).
        Atk5::new("VA10 owner_idx = 0 with a P2SH owner whose script hash is the vault's (fabricated)", VLT0, "vault:W5", wd(owner())).built(|b, c| {
            let Case5::Withdraw(c) = c else { unreachable!() };
            let t = b.templates.clone();
            let spk = t.vault_spk(&c.vault);
            let mut key = [0u8; 33];
            key[..32].copy_from_slice(&spk.script()[2..34]);
            let mut c2 = c.clone();
            c2.position.owner_class = 2;
            c2.position.owner_key = key;
            *b = c2.build_with(t);
            b.entry_args_mut(VLT0)[WD_ARG_OWNER_IDX] = Int(0);
        }),
        Atk5::new("VA9 request_withdraw: an extra covenant output (a request copy)", VLT0, "vault:W6", wd(owner())).built(|b, _| extra_cov(b, 1)),
        Atk5::new("VA9 request_withdraw: position' without covenant binding", VLT0, "vault:W6", wd(owner())).built(|b, _| b.outputs[2].covenant = None),
        Atk5::new("VA9 request_withdraw: partial with only vault' and request", VLT0, "vault:W6", wd(owner())).built(|b, _| b.outputs.truncate(2)),
        Atk5::new("VA9 request_withdraw: the change under a fresh genesis covenant ID", VLT0, "vault:W7", wd(owner())).built(|b, _| genesis_output(b, 2, 3)),
        // ---- position vault_leg (PO4)
        Atk5::new("VA10 vault_pl = SCRIPT_MAX + 1 at the position leg", POS, "position:PO4", wd(owner())).built(|b, _| b.entry_args_mut(POS)[0] = Int(SCRIPT_MAX + 1)),
        Atk5::new("VA10 vault_sl = -1 at the position leg", POS, "position:PO4", wd(owner())).built(|b, _| b.entry_args_mut(POS)[1] = Int(-1)),
        Atk5::new("VA10 vault_sl = SCRIPT_MAX + 1 at the position leg", POS, "position:PO4", wd(owner())).built(|b, _| b.entry_args_mut(POS)[1] = Int(SCRIPT_MAX + 1)),
        Atk5::new("VA10 a vault with role 0x07 (fabricated) at the position leg", POS, "position:PO4", wd(owner())).built(vault_role(0x07)),
        // ---- record request_leg (RL4)
        Atk5::new("VA12 req_pl = -1 at the record leg", REC, "record:RL4", st_dep()).built(|b, _| b.entry_args_mut(REC)[0] = Int(-1)),
        Atk5::new("VA12 req_sl = -1 at the record leg", REC, "record:RL4", st_dep()).built(|b, _| b.entry_args_mut(REC)[1] = Int(-1)),
        Atk5::new("VA12 req_sl = SCRIPT_MAX + 1 at the record leg", REC, "record:RL4", st_dep()).built(|b, _| b.entry_args_mut(REC)[1] = Int(SCRIPT_MAX + 1)),
        Atk5::new("VA12 a request with role 0x08 (fabricated) at the record leg", REC, "record:RL4", st_dep()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            role_at(b, REQ, c.request.values(), None, 0x08);
        }),
        // ---- settle (A6.1 S1-S9)
        Atk5::new("VA12 a covenant-less copy of the request as funding (at the copy, input 2)", 2, "request:S1", st_dep()).built(|b, _| rogue_copy(b, REQ, 2)),
        Atk5::new("VA11 a request with role 0x08 (fabricated)", REQ, "request:S1", st_dep()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            role_at(b, REQ, c.request.values(), None, 0x08);
        }),
        Atk5::new("VA11 request and record round 0 (fabricated)", REQ, "request:S1", st_dep()).st(|c| {
            c.request.round = 0;
            c.record.round = 0;
        }),
        Atk5::new("VA11 request and record round TOTAL_MAX (fabricated)", REQ, "request:S1", st_dep()).st(|c| {
            c.request.round = TOTAL_MAX;
            c.record.round = TOTAL_MAX;
        }),
        Atk5::new("VA11 a withdrawal request of kind 2 (fabricated, FINAL)", REQ, "request:S1", st_final()).st(|c| c.request.kind = 2),
        Atk5::new("VA11 a withdrawal request of SHARES_MAX + 1 shares (fabricated, FINAL)", REQ, "request:S1", st_final())
            .st(|c| *c = st_case(record(0, 0, 0, SHARES_MAX + 1, SHARES_MAX + 1, 15 * KAS as i64), KIND_WITHDRAW, SHARES_MAX + 1)),
        Atk5::new("VA11 a request with owner class 3, paid to its P2SH form (fabricated)", REQ, "request:S1", st_final()).st(|c| c.request.owner_class = 3).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.outputs[c.expected().payout_output.unwrap()].script_public_key = Payout::from_fields(2, &c.request.owner_key).spk();
        }),
        Atk5::new("VA11 a request with the key padding byte set (class 0, fabricated)", REQ, "request:S1", st_final()).st(|c| c.request.owner_key[32] = 1),
        Atk5::new("VA11 request value 2^63 (engine only, OpTxInputAmount fails)", REQ, "request:S1", st_final()).st(|c| c.request_value = 1 << 63),
        Atk5::new("VA11 PAY: request value TOTAL_MAX + 1 (engine only)", REQ, "request:S1", st_pay()).st(move |c| c.request_value = big),
        Atk5::new("VA12 rec_pl = SCRIPT_MAX + 1 at the request", REQ, "request:S1", st_dep()).built(|b, _| b.entry_args_mut(REQ)[SETTLE_ARG_REC_PL] = Int(SCRIPT_MAX + 1)),
        Atk5::new("VA12 rec_sl = -1 at the request", REQ, "request:S1", st_dep()).built(|b, _| b.entry_args_mut(REQ)[SETTLE_ARG_REC_SL] = Int(-1)),
        Atk5::new("VA12 rec_sl = SCRIPT_MAX + 1 at the request", REQ, "request:S1", st_dep()).built(|b, _| b.entry_args_mut(REQ)[SETTLE_ARG_REC_SL] = Int(SCRIPT_MAX + 1)),
        Atk5::new("VA11 nine funding inputs", REQ, "request:S2", st_dep()).st(|c| c.funding = (0..9).map(|_| FundingIn { value: KAS, daa: 1_300_000 }).collect()),
        Atk5::new("VA11 a record with role 0x07 (fabricated, drained)", REQ, "request:S3", st_last()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            role_at(b, REC, c.record.values(), None, 0x07);
        }),
        Atk5::new("VA11 a record with refund = 2 (fabricated, FINAL)", REQ, "request:S3", st_final())
            .st(|c| *c = st_case(record(2, 0, 0, 3_000_000, 3_000_000, 15 * KAS as i64), KIND_WITHDRAW, 1_000_000)),
        Atk5::new("VA11 a record with dep_rem -1 (fabricated, withdrawal)", REQ, "request:S3", st_final()).st(|c| c.record.dep_rem = -1),
        Atk5::new("VA11 a record with dep_rem TOTAL_MAX + 1 (fabricated, withdrawal)", REQ, "request:S3", st_final()).st(|c| c.record.dep_rem = TOTAL_MAX + 1),
        Atk5::new("VA11 a record with mint_rem -1 (fabricated, withdrawal)", REQ, "request:S3", st_final()).st(|c| c.record.mint_rem = -1),
        Atk5::new("VA11 a record with mint_rem SHARES_MAX + 1 (fabricated, withdrawal)", REQ, "request:S3", st_final()).st(|c| c.record.mint_rem = SHARES_MAX + 1),
        Atk5::new("VA11 a record with wsh_rem -1 (fabricated, deposit)", REQ, "request:S3", st_dep()).st(|c| {
            c.record.wsh_rem = -1;
            c.record.burn_rem = 0;
        }),
        Atk5::new("VA11 a record with wsh_rem TOTAL_MAX + 1 (fabricated, deposit)", REQ, "request:S3", st_dep()).st(|c| c.record.wsh_rem = TOTAL_MAX + 1),
        Atk5::new("VA11 a record with burn_rem -1 (fabricated, deposit)", REQ, "request:S3", st_dep()).st(|c| c.record.burn_rem = -1),
        Atk5::new("VA11 a record with out_rem -1 (fabricated, deposit)", REQ, "request:S3", st_dep()).st(|c| c.record.out_rem = -1),
        Atk5::new("VA11 a record with out_rem TOTAL_MAX + 1 (fabricated, deposit, engine only)", REQ, "request:S3", st_dep()).st(|c| c.record.out_rem = TOTAL_MAX + 1),
        Atk5::new("VA11 a record with funder class 3, refunded to its P2SH form (fabricated, drained)", REQ, "request:S3", st_last()).st(|c| c.record.funder_class = 3).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.outputs[c.expected().refund_output.unwrap()].script_public_key = Payout::from_fields(2, &c.record.funder_key).spk();
        }),
        // R_rc = in1 - out_rem - held = -1 with held = dep_rem > 0 (refund mode), so in1 >=
        // out_rem still holds: the record and record' both R_rc + 1 sompi lower.
        Atk5::new("VA11 a refund record value with R_rc = -1 (fabricated)", REQ, "request:S3", st_refund()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            lower_in_out(b, REC, Some(0), c.record_reserve + 1);
        }),
        Atk5::new("VA11 a record value below out_rem (fabricated)", REQ, "request:S3", st_dep()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            // record' cannot follow (its value would be negative); S3 rejects before S9 reads it.
            // in1 = out_rem - 1 (held = 0, not refund mode).
            lower_in_out(b, REC, None, c.record_reserve + 1);
        }),
        Atk5::new("VA11 an extra covenant output (a position copy)", REQ, "request:S7", st_dep()).built(|b, _| extra_cov(b, 1)),
        Atk5::new("VA11 position without covenant binding", REQ, "request:S7", st_dep()).built(|b, _| b.outputs[1].covenant = None),
        Atk5::new("VA11 FINAL without its payout and change outputs", REQ, "request:S8", st_final()).built(|b, _| b.outputs.truncate(1)),
        Atk5::new("VA11 ACCRUE: request value TOTAL_MAX - pay + 1 (engine only)", REQ, "request:S9", st_accrue()).st(|c| c.request_value = (TOTAL_MAX - c.expected().pay + 1) as u64),
        Atk5::new("VA11 refund: request value TOTAL_MAX - s + 1 (engine only)", REQ, "request:S9", st_refund()).st(|c| c.request_value = (TOTAL_MAX - c.request.amount + 1) as u64),
        Atk5::new("VA11 FINAL: request value TOTAL_MAX - pay + 1 (engine only)", REQ, "request:S9", st_final()).st(|c| c.request_value = (TOTAL_MAX - c.expected().pay + 1) as u64),
        Atk5::new("VA11 PAY payout + 1", REQ, "request:S9", st_pay()).built(|b, c| {
            let Case5::Settle(c) = c else { unreachable!() };
            b.shift_value(c.expected().payout_output.unwrap(), 1);
        }),
        // ---- draw (check 5, 14, C2, 20)
        Atk5::new("VA6 draw: total_shares = SHARES_MAX + 1, nothing pending (fabricated vault)", DRW5, "drawer:5", super::m3::base()).dr(|c| c.vault.total_shares = SHARES_MAX + 1),
        // S = 1,000, W = 1,001 > S, every share converted (wc = W, out(W) within the slack):
        // total_shares' = -1.
        Atk5::new("VA6 draw: pend_wsh = total_shares + 1, wc = W (fabricated vault)", DRW5, "drawer:5", d_pending(0, 1_001)).dr(|c| {
            c.vault.total_shares = 1_000;
            c.lie.wc = Some(1_001);
        }),
        // No sales (H = 0, so V' = V): V = ASSETS_MAX + 1 = TOTAL_MAX.
        Atk5::new("VA6 draw: total_assets = ASSETS_MAX + 1, no sales, nothing pending (fabricated vault, engine only)", DRW5, "drawer:5", d_no_pools(0, 0)).dr(|c| {
            c.set_sales(0, 0);
            c.vault.total_assets = TOTAL_MAX;
        }),
        Atk5::new("VA6 V' = ASSETS_MAX + 1, nothing pending (fabricated vault, engine only)", DRW5, "drawer:14", d_no_pools(0, 0)).dr(|c| {
            let h = c.expected().amounts.h;
            c.vault.total_assets = TOTAL_MAX - h;
        }),
        // V + H = ASSETS_MAX (check 14 passes), D = TOTAL_MAX - V (check 5 passes), so V' + D
        // = TOTAL_MAX + H > TOTAL_MAX: only C2's sum guard rejects. The deposits convert in the
        // script (q = 0, mint = floor(D N / M) >= ceil(D / MIN_DEPOSIT)); the reference refuses
        // the state (outside `convert`'s domain), so the builder is told to convert (`refund`
        // lie) and writes what the script computes.
        Atk5::new("VA6 V' + D > TOTAL_MAX (fabricated vault, engine only)", DRW5, "drawer:C2", d_no_pools(MIN_DEPOSIT, 0)).dr(|c| {
            let h = c.expected().amounts.h;
            c.vault.total_assets = lottery::game::ASSETS_MAX - h;
            c.vault.pend_dep = TOTAL_MAX - c.vault.total_assets;
            c.lie.refund = Some(false);
        }),
        Atk5::new("VA5 wc = -1", DRW5, "drawer:C2", d_both()).dr(|c| c.lie.wc = Some(-1)),
        Atk5::new("VA5 wc = W + 1 within the slack", DRW5, "drawer:C2", d_both()).dr(|c| c.lie.wc = Some(c.vault.pend_wsh + 1)),
        Atk5::new("VA8 record value + 1", DRW5, "drawer:20", d_both()).built(|b, c| {
            let Case5::Draw(c) = c else { unreachable!() };
            b.shift_value(c.expected().record_output().unwrap(), 1);
        }),
    ]
}

// ---------------------------------------------------------------- VA14

pub fn va14() -> Vec<Atk5> {
    vec![
        Atk5::new("VA14 J fixed from V' + D", DRW5, "drawer:17", d_big_dep()).dr(|c| c.lie.j_v = Some(c.expected().v_prime + c.vault.pend_dep)),
        Atk5::new("VA14 J fixed from vault' total_assets (after the conversions)", DRW5, "drawer:17", d_big_dep()).dr(|c| c.lie.j_v = Some(c.expected().vault.total_assets)),
    ]
}

/// VA14 base: a small vault (V = 100,000 KAS, so x_b = muldiv(V', j) passes and J follows
/// V') with 100,000 KAS deposited and 10^11 shares to withdraw, so V', V' + D and
/// total_assets' all differ, and so do the J fixes from them. M6 (D84): with S = 10^15
/// (SHARE_UNIT = 1) the 10^11 shares are worth about 10 KAS (about 10,000 KAS at
/// SHARE_UNIT = 1,000); the three values still differ (asserted below).
pub fn d_big_dep() -> DrawCase {
    let mut c = d_no_pools(100_000 * KAS as i64, 100_000_000_000);
    c.vault.total_assets = 100_000 * KAS as i64;
    let x = c.expected();
    for v in [x.v_prime + c.vault.pend_dep, x.vault.total_assets] {
        let mut alt = c.clone();
        alt.lie.j_v = Some(v);
        let a = alt.expected().fix;
        assert_ne!((a.x1, a.y, a.x2), (x.fix.x1, x.fix.y, x.fix.x2), "VA14 base: J fixed from {v} differs from J fixed from V'");
    }
    c
}

/// Every attack of the library (VA1-VA14), for lookup by id.
// ---------------------------------------------------------------- Stage 7 (review fixes)

/// M5 Stage 7 (review fixes): the vault request age (U-4, checks D10 and W12) and the empty
/// drained record (F4, settle check S9). Ids carry their spec group.
pub fn stage7() -> Vec<Atk5> {
    let aged = VAULT_REQ_DELAY as u64;
    // A drained deposit-convert (the last depositor of a deposits-only record) and a drained
    // ACCRUE (wc = 0: nothing burnt, the whole request carried) whose fabricated record
    // keeps out_rem > 0 with burn_rem = 0 (D79): before F4 the out_rem sompi went to the fee.
    let acc = st_case(record(0, 0, 0, 3_000_000, 0, 0), KIND_WITHDRAW, 3_000_000);
    assert_mode(&acc, SettleMode::Accrue, true);
    vec![
        Atk5::new("VA9 request_deposit at vault age VAULT_REQ_DELAY - 1", VLT0, "vault:D10", dep()).dep(|c| c.lock_time -= 1),
        Atk5::new("VA9 request_deposit at lock_time 0 (no age)", VLT0, "vault:D10", dep()).dep(|c| c.lock_time = 0),
        Atk5::new("VA9 request_deposit: vault creation = -1 (unaccepted parent)", VLT0, "vault:D10", dep()).built(|b, _| set_input_daa(b, VLT0, u64::MAX)),
        Atk5::new("VA9 request_deposit: vault creation = DAA_MAX", VLT0, "vault:D10", dep()).built(move |b, _| {
            set_input_daa(b, VLT0, DAA_MAX as u64);
            b.lock_time = DAA_MAX as u64 + aged;
        }),
        Atk5::new("VA10 request_withdraw (partial) at vault age VAULT_REQ_DELAY - 1", VLT0, "vault:W12", wd(owner())).wd(|c| c.lock_time -= 1),
        Atk5::new("VA10 request_withdraw (whole) at vault age VAULT_REQ_DELAY - 1", VLT0, "vault:W12", wd_whole(owner())).wd(|c| c.lock_time -= 1),
        Atk5::new("VA10 request_withdraw: vault creation = -1 (unaccepted parent)", VLT0, "vault:W12", wd(owner())).built(|b, _| set_input_daa(b, VLT0, u64::MAX)),
        Atk5::new("VA10 request_withdraw: vault creation = DAA_MAX", VLT0, "vault:W12", wd(owner())).built(move |b, _| {
            set_input_daa(b, VLT0, DAA_MAX as u64);
            b.lock_time = DAA_MAX as u64 + aged;
        }),
        Atk5::new("VA11 drained deposit-convert: record out_rem > 0 with burn_rem = 0 (fabricated)", REQ, "request:S9", st_dep_last()).st(|c| c.record.out_rem = 5 * KAS as i64),
        Atk5::new("VA11 drained ACCRUE: record out_rem > 0 with burn_rem = 0 (fabricated)", REQ, "request:S9", acc).st(|c| c.record.out_rem = 5 * KAS as i64),
    ]
}

pub fn all_atks5() -> Vec<Atk5> {
    let mut v = vec![];
    for f in [va1, va2, va3, va4, va5, va6, va7, va8, va9, va10, va11, va12, va13, va14, stage6, stage7] {
        v.extend(f());
    }
    v
}

pub fn atk5(id: &str) -> Atk5 {
    let v = all_atks5();
    let n = v.iter().filter(|a| a.id == id).count();
    assert_eq!(n, 1, "attack id `{id}`: {n} matches");
    v.into_iter().find(|a| a.id == id).unwrap()
}

// ---------------------------------------------------------------- VA15

/// VA15 bases: each new entry's leader with its committed budget - 1 must fail on the
/// budget.
pub fn va15_cases() -> Vec<(String, Case5)> {
    let mut v: Vec<(String, Case5)> = vec![
        ("request_deposit".into(), dep().into()),
        ("request_withdraw partial".into(), wd(owner()).into()),
        ("request_withdraw whole".into(), wd_whole(owner()).into()),
        ("draw with conversion".into(), d_both().into()),
    ];
    for (n, c) in settle_bases() {
        v.push((format!("settle {n}"), c.into()));
    }
    for (n, c) in refresh5_bases() {
        v.push((format!("refresh {n}"), c.into()));
    }
    v
}

/// The SHARE_UNIT re-export for the tests.
pub const S_UNIT: i64 = SHARE_UNIT;
