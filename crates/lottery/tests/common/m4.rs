//! M4 Stage 2b: the `claim()`, `return_deposit()` and `refresh()` attacks expressed as data
//! (`Atk`), shared by `m4_attacks.rs` (committed templates) and `m4_mutants.rs` (stage 4:
//! re-run under the mutant that lacks the `require` an attack targets), as `common::m3`.
//!
//! An `Atk` is a valid case (`good`: a claim, a return or a refresh), one case edit and one
//! transaction edit, aimed at one input, with the check the spec (M4 A9 "Check") expects to
//! reject it:
//! - `ticket:N`: A4 claim check N (1..15);
//! - `ticket:A5b.N` / `ticket:A5a.N`: A5 `return_deposit_after` / `return_deposit_with_pool`
//!   check N;
//! - `ticket:A6.N`, `pool:A6.N`, `marker:A6.N`: A6 `refresh()` check N of that object;
//! - `pool:PTn`: the pool's `ticket_leg` check PTn;
//! - `ledger` (structural, no script) and `budget` (KA16) are not `Atk`s (see m4_attacks.rs).
//!
//! `strike`: the bad transaction is rejected by `input` with a script error, the good one
//! passes `input`, and then the good one is accepted by every input (`accept_m4`).
//!
//! Two diagnostic switches (neither is a test result on its own):
//! - `M4_PREFLIGHT=1`: while the ticket, pool and marker lack the M4 entries (before stage 3),
//!   a good transaction rejected by `EarlyReturn` (dispatch found no entry) is reported as a
//!   `PREFLIGHT` line instead of failing, so every attack runs: it proves each attack builds
//!   (no builder panic), changes the transaction, and is rejected by a *script* error (not a
//!   covenant-context error, which would mean a malformed attack).
//! - `M4_ATTRIB=1`: every strike also prints the first check that rejects the attack
//!   (`ATTRIB ... | expected <check> | got <check> | OK|MISMATCH`), by truncating the target
//!   covenant source before each `// ==== check <label>` line (the drawer's M3 instrument,
//!   generalized; see `first_failing_check4`). Stage 3 places those lines.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use harness::{ArtifactValue, CovenantBinding, TransactionOutput, TxScriptError, UtxoEntry, outpoint, pay_to_script_hash_script};
use kaspa_consensus_core::tx::GenesisCovenantGroup;
use lottery::buy::{Built, In, InKind, KAS, Sources, Template, Templates, b_max, spk_of};
use lottery::claim::*;
use lottery::close::{FundingIn, TEST_DEADLINE};
use lottery::combo::Combo;
use lottery::game::{CLAIM_RESERVE_MAX, ClaimMode, DAA_MAX, P_MIN, TOTAL_MAX};
use lottery::smt::Tree;
use lottery::state::{K_MAX, Payout, TIER_LOWER0, TicketState};

use super::m2::*;

// ---------------------------------------------------------------- the case of an attack

/// The transaction an attack starts from.
#[derive(Clone)]
pub enum Case {
    Claim(ClaimCase),
    Return(ReturnCase),
    Refresh(RefreshCase),
}

impl From<ClaimCase> for Case {
    fn from(c: ClaimCase) -> Self {
        Case::Claim(c)
    }
}
impl From<ReturnCase> for Case {
    fn from(c: ReturnCase) -> Self {
        Case::Return(c)
    }
}
impl From<RefreshCase> for Case {
    fn from(c: RefreshCase) -> Self {
        Case::Refresh(c)
    }
}

impl Case {
    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        match self {
            Case::Claim(c) => c.build_with(t),
            Case::Return(c) => c.build_with(t),
            Case::Refresh(c) => c.build_with(t),
        }
    }

    pub fn build(&self) -> Built {
        self.build_with(tpl())
    }

    /// The step name `accept_m4` reports.
    pub fn step(&self) -> &'static str {
        match self {
            Case::Claim(_) => "claim (attack base)",
            Case::Return(r) => match r.path {
                ReturnPath::After => "return_deposit_after (attack base)",
                ReturnPath::WithPool { .. } => "return_deposit_with_pool (attack base)",
            },
            Case::Refresh(r) => match r.obj {
                RefreshObj::Pool(_) => "refresh pool (attack base)",
                RefreshObj::Ticket(_) => "refresh ticket (attack base)",
                RefreshObj::Position(_) => "refresh position (attack base)",
                RefreshObj::Request(_) => "refresh request (attack base)",
                RefreshObj::Record(_) => "refresh record (attack base)",
            },
        }
    }

    /// Point the case at templates `t`: every pool state carries `ticket_tpl` = the ticket
    /// hash of `t` (a ticket or pool mutant changes the ticket hash: ticket(pool), D54). No
    /// other M4 state holds a template hash.
    pub fn retarget(&mut self, t: &Templates) {
        match self {
            Case::Claim(c) => c.pool.ticket_tpl = t.ticket.hash,
            Case::Return(r) => {
                if let ReturnPath::WithPool { pool, .. } = &mut r.path {
                    pool.ticket_tpl = t.ticket.hash;
                }
            }
            Case::Refresh(r) => {
                if let RefreshObj::Pool(p) = &mut r.obj {
                    p.ticket_tpl = t.ticket.hash;
                }
            }
        }
    }

    pub fn claim(&mut self) -> &mut ClaimCase {
        match self {
            Case::Claim(c) => c,
            _ => panic!("not a claim case"),
        }
    }
    pub fn ret(&mut self) -> &mut ReturnCase {
        match self {
            Case::Return(c) => c,
            _ => panic!("not a return case"),
        }
    }
    pub fn refresh(&mut self) -> &mut RefreshCase {
        match self {
            Case::Refresh(c) => c,
            _ => panic!("not a refresh case"),
        }
    }
}

pub type CaseEdit = Box<dyn Fn(&mut Case)>;
pub type BuiltEdit = Box<dyn Fn(&mut Built, &Case)>;

/// One attack: `good` with `case` applied, built, then `built` applied, must be rejected by
/// input `input` (at `check`) while `good` passes it.
pub struct Atk {
    /// "KAn short description".
    pub id: String,
    pub input: usize,
    /// The expected rejecting check (module doc).
    pub check: &'static str,
    pub good: Case,
    pub case: CaseEdit,
    pub built: BuiltEdit,
}

impl Atk {
    pub fn new(id: impl Into<String>, input: usize, check: &'static str, good: impl Into<Case>) -> Self {
        Self { id: id.into(), input, check, good: good.into(), case: Box::new(|_| {}), built: Box::new(|_, _| {}) }
    }

    /// A claim-case edit.
    pub fn claim(mut self, f: impl Fn(&mut ClaimCase) + 'static) -> Self {
        self.case = Box::new(move |c| f(c.claim()));
        self
    }

    /// A return-case edit.
    pub fn ret(mut self, f: impl Fn(&mut ReturnCase) + 'static) -> Self {
        self.case = Box::new(move |c| f(c.ret()));
        self
    }

    /// A refresh-case edit.
    pub fn refresh(mut self, f: impl Fn(&mut RefreshCase) + 'static) -> Self {
        self.case = Box::new(move |c| f(c.refresh()));
        self
    }

    /// A transaction edit (after the build); it gets the (edited) case.
    pub fn built(mut self, f: impl Fn(&mut Built, &Case) + 'static) -> Self {
        self.built = Box::new(f);
        self
    }

    /// The same attack aimed at another input and check.
    pub fn at(mut self, input: usize, check: &'static str) -> Self {
        self.input = input;
        self.check = check;
        self
    }

    /// (good, bad, bad case) under templates `t`.
    pub fn pair_case(&self, t: &Arc<Templates>) -> (Built, Built, Case) {
        let mut good = self.good.clone();
        good.retarget(t);
        let mut bad = good.clone();
        (self.case)(&mut bad);
        let mut bb = bad.build_with(t.clone());
        (self.built)(&mut bb, &bad);
        (good.build_with(t.clone()), bb, bad)
    }

    /// (good, bad) transactions under templates `t` (stage 4 mutants).
    pub fn pair(&self, t: &Arc<Templates>) -> (Built, Built) {
        let (g, b, _) = self.pair_case(t);
        (g, b)
    }

    /// Committed templates: the bad transaction is rejected by `input` with a script error,
    /// the good one passes `input` and is then accepted by every input.
    pub fn strike(&self) {
        let (g, b, bc) = self.pair_case(&tpl());
        strike4(self, &g, &b, &bc);
        done4(&self.id, self.good.step(), &g);
    }
}

// ---------------------------------------------------------------- strike, done, diagnostics

pub fn preflight() -> bool {
    std::env::var_os("M4_PREFLIGHT").is_some()
}

pub fn attrib4_enabled() -> bool {
    std::env::var_os("M4_ATTRIB").is_some()
}

fn test_name() -> String {
    std::thread::current().name().unwrap_or("?").to_string()
}

/// Steps 1 and 2 of `Atk::strike` (as `common::m2::strike`, plus the M4 diagnostics).
pub fn strike4(atk: &Atk, good: &Built, bad: &Built, bad_case: &Case) {
    let input = atk.input;
    let gf = good.finish();
    let bf = bad.finish();
    // The transaction ID does not commit to signature scripts, so argument-only attacks
    // (leaves, siblings, template lengths) are compared by their signature scripts.
    let sigs = |f: &lottery::buy::Final| f.tx.inputs.iter().map(|i| i.signature_script.clone()).collect::<Vec<_>>();
    assert!(gf.tx.id() != bf.tx.id() || gf.entries != bf.entries || sigs(&gf) != sigs(&bf), "{}: the attack changed nothing", atk.id);
    let bad_err = match bf.run_input(input) {
        Ok(_) => panic!("ATTACK ACCEPTED ({}) by input {input}", atk.id),
        Err(e) => {
            assert!(is_script_rejection(&e), "{}: input {input}: expected a script error, got {e:?}", atk.id);
            e
        }
    };
    if let Err(e) = gf.run_input(input) {
        if preflight() && matches!(e, TxScriptError::EarlyReturn) {
            let rejects = match bad_case {
                Case::Claim(c) => format!("{:?}", c.expected().rejects),
                _ => "-".into(),
            };
            println!("PREFLIGHT {} | {} | expect input {input} {} | bad: {bad_err:?} | good: {e:?} | reference: {rejects}", test_name(), atk.id, atk.check);
            return;
        }
        panic!("BASE REJECTED at target input {input} ({}): {e:?}", atk.id);
    }
    if attrib4_enabled() {
        let got = first_failing_check4(bad, bad_case, atk.check, input);
        let verdict = match &got {
            Some(g) if g.split(' ').next() == Some(atk.check) => "OK",
            Some(_) => "MISMATCH",
            None => "NO MARKERS",
        };
        println!("ATTRIB {} | {} | expected {} | got {} | {verdict}", test_name(), atk.id, atk.check, got.as_deref().unwrap_or("-"));
    }
}

/// The good transaction is accepted by every input (`accept_m4`). Under `M4_PREFLIGHT` a
/// dispatch rejection (`EarlyReturn`, no such entry yet) is tolerated.
pub fn done4(id: &str, step: &str, good: &Built) {
    if preflight() {
        let f = good.finish();
        match f.run() {
            Ok(()) => {}
            Err((_, TxScriptError::EarlyReturn)) => {}
            Err((i, e)) => panic!("{id}: base rejected at input {i} by {e:?} (not dispatch)"),
        }
        return;
    }
    accept_m4(&format!("{step} [{id}]"), good);
}

// ---------------------------------------------------------------- check attribution (M4_ATTRIB)
//
// Contract for stage 3 (the covenant sources). Each check of an M4 entry starts with a line
//     // ==== check <label>: <text>
// inside the entry body, in check order, labels unique per file:
//     ticket.sil  claim                     1 .. 15            (A4 checks)
//                 return_deposit_after      A5b.1 .. A5b.6     (A5 path b)
//                 return_deposit_with_pool  A5a.1 .. A5a.8     (A5 path a)
//                 refresh                   A6.1 .. A6.7
//     pool.sil    ticket_leg                PT1 .. PT4
//                 refresh                   A6.1 .. A6.7
//     marker.sil  refresh                   A6.1 .. A6.7
// A label's group is its prefix ("" for the bare numbers, "A5b.", "A5a.", "A6.", "PT").
// For an attack aimed at `<file>:<label>`, the file is cut just before each later marker of
// the same group (open blocks closed), compiled, swapped in at the target input, and only
// that input is run; the last marker of the group uses the whole source. The first
// truncation that rejects names the check (a truncation contains every earlier check of the
// entry, so rejection is monotone).

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Obj {
    Ticket,
    Pool,
    Marker,
}

fn parse_check(check: &str) -> Option<(Obj, String)> {
    let (file, label) = check.split_once(':')?;
    let obj = match file {
        "ticket" => Obj::Ticket,
        "pool" => Obj::Pool,
        "marker" => Obj::Marker,
        _ => return None,
    };
    Some((obj, label.to_string()))
}

fn group_of(label: &str) -> String {
    if label.chars().all(|c| c.is_ascii_digit()) {
        String::new()
    } else if let Some(rest) = label.strip_prefix("PT") {
        let _ = rest;
        "PT".into()
    } else {
        match label.find('.') {
            Some(p) => label[..=p].to_string(),
            None => label.to_string(),
        }
    }
}

fn source_of(obj: Obj) -> String {
    let s = Sources::committed(b_max());
    match obj {
        Obj::Ticket => s.ticket,
        Obj::Pool => s.pool,
        Obj::Marker => s.marker,
    }
    .replace("\r\n", "\n")
}

fn compile_variant(obj: Obj, src: &str) -> Arc<Template> {
    let mut s = Sources::committed(b_max());
    match obj {
        Obj::Ticket => s.ticket = src.to_string(),
        Obj::Pool => s.pool = src.to_string(),
        Obj::Marker => s.marker = src.to_string(),
    }
    let t = Templates::compile_sources(b_max(), &s);
    match obj {
        Obj::Ticket => t.ticket.clone(),
        Obj::Pool => t.pool.clone(),
        Obj::Marker => t.marker.clone(),
    }
}

type Truncations = Vec<(String, Arc<Template>)>;

/// (label, truncated template) for every marker of `group` in `obj`'s source, in order; the
/// last one is the whole source. Empty if the source has no marker of the group.
fn truncations(obj: Obj, group: &str, src: &str) -> Arc<Truncations> {
    type Key = (Obj, String, String);
    static CACHE: OnceLock<Mutex<HashMap<Key, Arc<Truncations>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (obj, group.to_string(), src.to_string());
    if let Some(t) = cache.lock().unwrap().get(&key) {
        return t.clone();
    }
    // `lines()` also strips a CR before each LF.
    let src = src.to_string();
    let lines: Vec<&str> = src.lines().collect();
    let marks: Vec<(usize, String)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.trim_start().strip_prefix("// ==== check ").map(|r| (i, r.split(':').next().unwrap().trim().to_string())))
        .filter(|(_, label)| group_of(label) == group)
        .collect();
    let out: Truncations = marks
        .iter()
        .enumerate()
        .map(|(j, (_, label))| {
            let body = match marks.get(j + 1) {
                None => src.clone(),
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
            (label.clone(), compile_variant(obj, &body))
        })
        .collect();
    let out = Arc::new(out);
    cache.lock().unwrap().insert(key, out.clone());
    out
}

/// `b` with input `input` running template `t` (same state bytes and arguments, P2SH script
/// rebuilt). Genuine successors that the swapped template validates are rebuilt under `t`:
/// a refresh's output 0 (when it equals the spent script), a claim's ticket' at output 1
/// (when it is the spent ticket with some `claimed` byte). Returns whether such a successor
/// was present but edited by the attack (it cannot be rebuilt; only the whole source can
/// then judge it).
fn with_template(b: &Built, input: usize, t: &Arc<Template>, case: &Case, obj: Obj) -> (Built, bool) {
    let mut m = b.clone();
    let (old, redeem) = match &b.inputs[input].kind {
        InKind::Entry { template, redeem, .. } => (template.clone(), redeem.clone()),
        _ => panic!("input {input} is not an entry input"),
    };
    let state = redeem[old.prefix.len()..redeem.len() - old.suffix.len()].to_vec();
    let new_redeem = [t.prefix.as_slice(), &state, &t.suffix].concat();
    m.set_entry_redeem(input, new_redeem.clone());
    if let InKind::Entry { template, .. } = &mut m.inputs[input].kind {
        *template = t.clone();
    }
    let mut edited = false;
    if input == 0 {
        match case {
            Case::Refresh(_) if !b.outputs.is_empty() => {
                if b.outputs[0].script_public_key == b.inputs[0].entry.script_public_key {
                    m.outputs[0].script_public_key = pay_to_script_hash_script(&new_redeem);
                } else {
                    edited = true;
                }
            }
            Case::Claim(c) if obj == Obj::Ticket => {
                if b.outputs.len() > 1 && b.outputs[1].covenant.is_some() {
                    let found = (0..=255u8).find_map(|v| {
                        let mut s: TicketState = c.ticket.clone();
                        s.claimed[0] = v;
                        (pay_to_script_hash_script(&old.redeem(&s.values())) == b.outputs[1].script_public_key).then_some(s)
                    });
                    match found {
                        Some(s) => m.outputs[1].script_public_key = pay_to_script_hash_script(&t.redeem(&s.values())),
                        None => edited = true,
                    }
                }
            }
            _ => {}
        }
    }
    (m, edited)
}

/// The first check (`<file>:<label>`) that rejects `bad` at `input`, for an attack expected
/// at `check`; `None` if the target source has no marker of that group yet (before stage 3).
pub fn first_failing_check4(bad: &Built, case: &Case, check: &str, input: usize) -> Option<String> {
    first_failing_check_in(bad, case, check, input, None)
}

/// Whether input `input` of `b` passes when it runs the whole of `src` (the committed source
/// of `check`'s file if `None`) in place of its template (instrument self-test).
pub fn passes_under_source(b: &Built, case: &Case, check: &str, input: usize, src: Option<&str>) -> bool {
    let (obj, label) = parse_check(check).expect("a <file>:<label> check");
    let src = src.map(str::to_string).unwrap_or_else(|| source_of(obj));
    let tr = truncations(obj, &group_of(&label), &src);
    let (m, _) = with_template(b, input, &tr.last().expect("markers").1, case, obj);
    m.finish().run_input(input).is_ok()
}

/// As `first_failing_check4` with the target file's source given (`None`: the committed
/// one). The self-test in m4_attacks.rs runs the instrument on a synthetic ticket.
pub fn first_failing_check_in(bad: &Built, case: &Case, check: &str, input: usize, src: Option<&str>) -> Option<String> {
    let (obj, label) = parse_check(check)?;
    let file = match obj {
        Obj::Ticket => "ticket",
        Obj::Pool => "pool",
        Obj::Marker => "marker",
    };
    let src = src.map(str::to_string).unwrap_or_else(|| source_of(obj));
    let tr = truncations(obj, &group_of(&label), &src);
    if tr.is_empty() {
        return None;
    }
    let rejects = |j: usize| -> (bool, bool) {
        let (m, edited) = with_template(bad, input, &tr[j].1, case, obj);
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

// ---------------------------------------------------------------- Built edits

pub fn bytes_arg(v: &mut [ArtifactValue], i: usize) -> &mut Vec<u8> {
    match &mut v[i] {
        ArtifactValue::Bytes(b) => b,
        a => panic!("argument {i} is not bytes: {a:?}"),
    }
}

/// Fresh distinct outpoints after inputs were inserted or removed.
pub fn reindex(b: &mut Built) {
    b.outpoints = (0..b.inputs.len() as u32).map(outpoint).collect();
}

/// Input `i`'s UTXO entry with another covenant ID.
pub fn set_input_cov(b: &mut Built, i: usize, cov: Option<harness::Hash>) {
    let e = b.inputs[i].entry.clone();
    b.inputs[i].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, cov);
}

/// Input `i`'s UTXO entry with another creation DAA score.
pub fn set_input_daa(b: &mut Built, i: usize, daa: u64) {
    let e = b.inputs[i].entry.clone();
    b.inputs[i].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), daa, false, e.covenant_id);
}

/// A plain output of 1 KAS (paid out of the change, the last output) appended at the end.
pub fn extra_plain(b: &mut Built) {
    let last = b.outputs.len() - 1;
    b.outputs[last].value -= KAS;
    b.outputs.push(TransactionOutput::new(KAS, spk_of(&[9u8; 32])));
}

/// A copy of covenant output `o` worth 1 KAS (out of the change), authorized by input 0,
/// inserted before the change.
pub fn extra_cov(b: &mut Built, o: usize) {
    let last = b.outputs.len() - 1;
    b.outputs[last].value -= KAS;
    let mut x = b.outputs[o].clone();
    x.value = KAS;
    b.outputs.insert(last, x);
}

/// Another payout script (a fresh Schnorr key).
pub fn other_payout() -> harness::ScriptPublicKey {
    Payout::Schnorr([0x77; 32]).spk()
}

/// A state map with another role byte.
pub fn with_role(mut v: std::collections::BTreeMap<String, ArtifactValue>, role: u8) -> std::collections::BTreeMap<String, ArtifactValue> {
    v.insert("role".into(), ArtifactValue::Byte(role));
    v
}

pub const BAD_ROLE: u8 = 0x07;

// ---------------------------------------------------------------- Stage 4 edit helpers

/// A covenant-less copy of input `from` (same template, state, arguments and value, no
/// covenant ID) inserted at `at`, where a funding input would sit: its script runs with
/// activeInputIndex = `at` (the `*_active` / `RF1` / `PT1` attacks).
pub fn rogue_copy(b: &mut Built, from: usize, at: usize) {
    let mut i = b.inputs[from].clone();
    let e = i.entry.clone();
    i.entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, None);
    b.inputs.insert(at, i);
    reindex(b);
}

/// Output `out` becomes a genesis output of a fresh covenant authorized by funding input
/// `funding` (consensus-valid: the ID is recomputed from the outpoint, rusty-kaspa v2.1.0
/// crypto/txscript/src/covenants.rs:136-160). Its OpOutputCovenantId is then nonzero.
pub fn genesis_output(b: &mut Built, funding: usize, out: usize) {
    b.genesis = vec![GenesisCovenantGroup::new(funding as u16, vec![out as u32])];
}

/// Input `i` (a ticket, pool or marker entry) with role byte BAD_ROLE in its state.
pub fn input_role(b: &mut Built, i: usize) {
    let (t, redeem) = match &b.inputs[i].kind {
        InKind::Entry { template, redeem, .. } => (template.clone(), redeem.clone()),
        _ => panic!("input {i} is not an entry input"),
    };
    let mut state = redeem[t.prefix.len()..redeem.len() - t.suffix.len()].to_vec();
    // Every M4 state starts with the role byte as a 1-byte push (0x01 <role>).
    assert_eq!(state[0], 0x01, "role push");
    state[1] = BAD_ROLE;
    b.set_entry_redeem(i, [t.prefix.as_slice(), &state, &t.suffix].concat());
}

/// The pool at input `i` with role BAD_ROLE, and output `o` (pool') following it.
pub fn pool_role(b: &mut Built, i: usize, o: usize) {
    input_role(b, i);
    b.outputs[o].script_public_key = b.inputs[i].entry.script_public_key.clone();
}

/// The D11 class-3 payout script the ticket computes (`payoutSpk` falls through to P2SH of
/// the key's first 32 bytes).
pub fn class3_payout(key: &[u8; 33]) -> harness::ScriptPublicKey {
    Payout::P2sh(key[..32].try_into().unwrap()).spk()
}

/// Funding edits shared by claims, returns and refreshes.
pub fn no_funding(f: &mut Vec<FundingIn>, change: &mut bool, fee: &mut u64) {
    f.clear();
    *change = false;
    *fee = 0;
}
pub fn nine_funding(f: &mut Vec<FundingIn>) {
    let daa = f[0].daa;
    *f = (0..9).map(|_| FundingIn { value: KAS, daa }).collect();
}

/// The colex rank the ticket script computes for any 5 numbers (check 5; for a
/// non-canonical combination it is another combination's rank).
pub fn script_rank(c: &[u8; 6]) -> u32 {
    let n: Vec<i64> = c[..5].iter().map(|&x| x as i64).collect();
    let (x2, x3, x4, x5) = (n[1] - 1, n[2] - 1, n[3] - 1, n[4] - 1);
    let r = (n[0] - 1) + x2 * (x2 - 1) / 2 + x3 * (x3 - 1) * (x3 - 2) / 6 + x4 * (x4 - 1) * (x4 - 2) * (x4 - 3) / 24 + x5 * (x5 - 1) * (x5 - 2) * (x5 - 3) * (x5 - 4) / 120;
    u32::try_from(r).expect("rank >= 0")
}

/// A claim of the non-canonical combination `comb` at index 0 (fabricated, D63): shard 0
/// holds it at the script's rank, the pool's tier is the script's tier for it.
pub fn noncanonical(comb: [u8; 6]) -> ClaimCase {
    let mut c = ClaimCase::fabricated(4, 10, TIER_LOWER0, 2 * P_MIN, &[1]);
    let rank = script_rank(&comb);
    c.ticket.bundle[..6].copy_from_slice(&comb);
    c.trees[0].increment(rank, comb[5]);
    c.sync_roots();
    c.lie.rank = Some(rank);
    c.pool.tier = lottery::game::tier_of(&comb, &c.pool.win).expect("a prize tier");
    c
}

/// K = 1, m = 2, tier 2 (bonus 1) claim; `set_m` rebuilds its tree with another m.
pub fn m_base() -> ClaimCase {
    let c = ClaimCase::fabricated(1, 2, TIER_LOWER0, 2 * P_MIN, &[1]);
    assert!(c.expected().rejects.is_empty());
    c
}
pub fn set_m(c: &mut ClaimCase, m: i64) {
    let comb = comb_of(c);
    let mut t = Tree::new(m as usize);
    t.increment(comb.rank(), comb.bonus);
    c.trees = vec![t];
    c.pool.m = m;
    c.sync_roots();
}

fn with_pool_edit(f: impl Fn(&mut lottery::state::PoolState, &mut u64) + 'static) -> impl Fn(&mut ReturnCase) + 'static {
    move |c: &mut ReturnCase| {
        if let ReturnPath::WithPool { pool, pool_value, .. } = &mut c.path {
            f(pool, pool_value)
        }
    }
}

// ---------------------------------------------------------------- bases

pub const TKT: usize = ClaimCase::TICKET;
pub const POOL: usize = ClaimCase::POOL;

/// The standalone PAY claim (K = 4, m = 10, b = 2, idx 0 of tier 2 held once, slice
/// 0.2 KAS): outputs pool', ticket', payout, change.
pub fn pay() -> ClaimCase {
    let c = ClaimCase::new(4);
    let x = c.expected();
    assert!(x.rejects.is_empty() && x.mode == ClaimMode::Pay, "{:?}", x.rejects);
    c
}

/// The PAY base made FINAL (index 1 already claimed): outputs pool', payout, change.
pub fn fin() -> ClaimCase {
    let mut c = ClaimCase::new(4);
    c.make_final();
    let x = c.expected();
    assert!(x.rejects.is_empty() && x.mode == ClaimMode::Final, "{:?}", x.rejects);
    c
}

/// An ACCRUE claim (prize P_MIN - 1): outputs pool', ticket' (+ prize), change.
pub fn accrue() -> ClaimCase {
    let c = ClaimCase::fabricated(4, 10, TIER_LOWER0, P_MIN - 1, &[1]);
    let x = c.expected();
    assert!(x.rejects.is_empty() && x.mode == ClaimMode::Accrue, "{:?}", x.rejects);
    c
}

/// The claimed combination of a case.
pub fn comb_of(c: &ClaimCase) -> Combo {
    let b = c.comb();
    Combo::new(b[..5].try_into().unwrap(), b[5])
}

/// The KA2 base: K = 4, tier 2, slice 0.6 KAS; the combination held in shards 0, 1 and 3
/// (h = 3, a Z_0 leaf in non-empty shard 2); shard 1 also holds the same main numbers with
/// another bonus (a nonzero count elsewhere in its leaf); a decoy rank (another tier 2
/// combination, same bonus) held in shards 0 and 2. PAY of 0.2 KAS.
pub fn ka2_base() -> ClaimCase {
    let mut c = ClaimCase::fabricated(4, 10, TIER_LOWER0, 60_000_000, &[1, 1, 0, 1]);
    let comb = comb_of(&c);
    let other_b = if comb.bonus == 1 { 2 } else { 1 };
    c.trees[1].increment(comb.rank(), other_b);
    let decoy = combo_for_tier(&c.pool.win, TIER_LOWER0, 10, 1);
    assert_ne!(decoy.rank(), comb.rank());
    assert_eq!(decoy.bonus, comb.bonus);
    c.trees[0].increment(decoy.rank(), decoy.bonus);
    c.trees[2].increment(decoy.rank(), decoy.bonus);
    c.sync_roots();
    let x = c.expected();
    assert!(x.rejects.is_empty(), "{:?}", x.rejects);
    assert_eq!((x.h, x.prize, x.mode), (3, 20_000_000, ClaimMode::Pay));
    assert!(x.counts[2].iter().all(|&n| n == 0), "shard 2: Z_0 leaf at the rank");
    c
}

/// The real count strings of a case at its combination's rank.
pub fn real_counts(c: &ClaimCase) -> Vec<Vec<u16>> {
    let r = comb_of(c).rank();
    c.trees.iter().map(|t| t.counts(r)).collect()
}

/// A fabricated claim (K = 4, m = 10) whose ticket is [tier `tier` combination, `other`],
/// both held in shard 0 (the trees and pool roots follow); idx 0 against the tier pool.
pub fn claim_with_second(tier: u8, other: Combo) -> ClaimCase {
    let mut c = ClaimCase::fabricated(4, 10, tier, 2 * P_MIN, &[1]);
    let comb = comb_of(&c);
    let mut tr = Tree::new(10);
    tr.increment(comb.rank(), comb.bonus);
    tr.increment(other.rank(), other.bonus);
    c.trees[0] = tr;
    c.ticket = fabricated_ticket(&[comb, other], c.b_max);
    c.sync_roots();
    assert!(c.expected().rejects.is_empty());
    c
}

/// Standalone refresh cases (as KP15): ticket and pool of `ClaimCase::new(4)`, created at DAA
/// 1,500,000. Spec M7 D99: the marker has no `refresh()` (its case is removed; SP10).
pub fn refresh_ticket() -> RefreshCase {
    let c = ClaimCase::new(4);
    RefreshCase::new(RefreshObj::Ticket(c.ticket), c.ticket_value, 1_500_000)
}
pub fn refresh_pool() -> RefreshCase {
    let c = ClaimCase::new(4);
    RefreshCase::new(RefreshObj::Pool(c.pool), c.pool_value, 1_500_000)
}

/// The refresh bases with their object name (`ticket`, `pool`; spec M7 D99 removed the
/// marker's).
pub fn refresh_bases() -> Vec<(&'static str, RefreshCase)> {
    vec![("ticket", refresh_ticket()), ("pool", refresh_pool())]
}

/// A refresh's check label for object `obj` ("ticket:A6.N" etc.).
fn a6(obj: &str, n: u8) -> &'static str {
    match (obj, n) {
        ("ticket", 1) => "ticket:A6.1",
        ("ticket", 2) => "ticket:A6.2",
        ("ticket", 3) => "ticket:A6.3",
        ("ticket", 4) => "ticket:A6.4",
        ("ticket", 5) => "ticket:A6.5",
        ("ticket", 6) => "ticket:A6.6",
        ("ticket", 7) => "ticket:A6.7",
        ("pool", 1) => "pool:A6.1",
        ("pool", 2) => "pool:A6.2",
        ("pool", 3) => "pool:A6.3",
        ("pool", 4) => "pool:A6.4",
        ("pool", 5) => "pool:A6.5",
        ("pool", 6) => "pool:A6.6",
        ("pool", 7) => "pool:A6.7",
        ("marker", 1) => "marker:A6.1",
        ("marker", 2) => "marker:A6.2",
        ("marker", 3) => "marker:A6.3",
        ("marker", 4) => "marker:A6.4",
        ("marker", 5) => "marker:A6.5",
        ("marker", 6) => "marker:A6.6",
        ("marker", 7) => "marker:A6.7",
        _ => panic!("no A6 label {obj}:{n}"),
    }
}

// ---------------------------------------------------------------- KA1 (script half)

/// KA1: the same index claimed again on the ticket successor (bit already set; check 4).
pub fn ka1() -> Vec<Atk> {
    vec![
        Atk::new("KA1 index already claimed (PAY)", TKT, "ticket:4", pay()).claim(|c| c.ticket.claimed[0] |= c.bit()),
        Atk::new("KA1 index already claimed (ACCRUE)", TKT, "ticket:4", accrue()).claim(|c| c.ticket.claimed[0] |= c.bit()),
    ]
}

// ---------------------------------------------------------------- KA2

/// KA2 (check 7): forged or under-reported h.
pub fn ka2() -> Vec<Atk> {
    let c = ka2_base();
    let comb = comb_of(&c);
    let bi = comb.bonus as usize - 1;
    let oi = if comb.bonus == 1 { 1 } else { 0 };
    let decoy = combo_for_tier(&c.pool.win, TIER_LOWER0, 10, 1).rank();
    let counts = real_counts(&c);
    let lie_counts = move |f: Box<dyn Fn(&mut Vec<Vec<u16>>)>| {
        let mut v = counts.clone();
        f(&mut v);
        v
    };
    let stale_bonus = lie_counts(Box::new(move |v| v[1][bi] -= 1));
    let stale_else = lie_counts(Box::new(move |v| v[1][oi] -= 1));
    let z0 = lie_counts(Box::new(|v| v[3].iter_mut().for_each(|n| *n = 0)));
    vec![
        Atk::new("KA2 stale leaf: shard 1 count - 1 at the bonus", TKT, "ticket:7", c.clone()).claim(move |c| c.lie.counts = Some(stale_bonus.clone())),
        Atk::new("KA2 stale leaf: shard 1 count - 1 at another bonus", TKT, "ticket:7", c.clone()).claim(move |c| c.lie.counts = Some(stale_else.clone())),
        Atk::new("KA2 Z_0 for shard 3's non-empty leaf", TKT, "ticket:7", c.clone()).claim(move |c| c.lie.counts = Some(z0.clone())),
        Atk::new("KA2 k - 1 proofs", TKT, "ticket:7", c.clone()).claim(|c| c.lie.k_proofs = Some(3)),
        Atk::new("KA2 proofs of shards 0 and 3 swapped (equal leaves)", TKT, "ticket:7", c.clone()).claim(|c| c.lie.swap = Some((0, 3))),
        Atk::new("KA2 proofs of shards 0 and 1 swapped", TKT, "ticket:7", c.clone()).claim(|c| c.lie.swap = Some((0, 1))),
        Atk::new("KA2 leaves + 2 bytes", TKT, "ticket:7", c.clone()).built(|b, _| bytes_arg(b.entry_args_mut(TKT), CLAIM_ARG_LEAVES).extend([0, 0])),
        Atk::new("KA2 leaves - 2 bytes", TKT, "ticket:7", c.clone()).built(|b, _| {
            let l = bytes_arg(b.entry_args_mut(TKT), CLAIM_ARG_LEAVES);
            l.truncate(l.len() - 2);
        }),
        Atk::new("KA2 siblings + 1 byte", TKT, "ticket:7", c.clone()).built(|b, _| bytes_arg(b.entry_args_mut(TKT), CLAIM_ARG_SIBLINGS).push(0)),
        Atk::new("KA2 siblings - 1 byte", TKT, "ticket:7", c.clone()).built(|b, _| {
            bytes_arg(b.entry_args_mut(TKT), CLAIM_ARG_SIBLINGS).pop();
        }),
        Atk::new("KA2 another rank's leaf and path (a decoy held in shards 0, 2)", TKT, "ticket:7", c.clone()).claim(move |c| c.lie.rank = Some(decoy)),
        // Extra (S2b-3): h itself is not a script input; a false h with the true leaves
        // only changes the outputs (prize = slice / 2 instead of slice / 3).
        Atk::new("KA2 under-reported h with true leaves (outputs follow h = 2)", TKT, "ticket:12", c).claim(|c| c.lie.h = Some(2)),
    ]
}

// ---------------------------------------------------------------- KA3

/// KA3 (check 6): wrong tier.
pub fn ka3() -> Vec<Atk> {
    let mut v = vec![];
    for tier in 0..=6u8 {
        for nt in [tier as i32 - 1, tier as i32 + 1] {
            if (0..=6).contains(&nt) {
                let c = ClaimCase::fabricated(4, 10, tier, 2 * P_MIN, &[1]);
                v.push(Atk::new(format!("KA3 tier {tier} combination against the tier {nt} pool"), TKT, "ticket:6", c).claim(move |c| c.pool.tier = nt as u8));
            }
        }
    }
    // Non-winning combinations held at index 1 of a ticket whose index 0 wins tier 6.
    let wb = |c: &ClaimCase| c.pool.win;
    let base = ClaimCase::fabricated(4, 10, 6, 2 * P_MIN, &[1]);
    let w = wb(&base);
    let one = combo_matching(&w, 1, true, 10, 3);
    let two = combo_matching(&w, 2, false, 10, 3);
    for (name, other) in [("1 match with the bonus", one), ("2 matches without the bonus", two)] {
        assert_eq!(lottery::game::tier_of(&other.bytes(), &w), None);
        v.push(Atk::new(format!("KA3 non-winning combination ({name}) against the tier 6 pool"), TKT, "ticket:6", claim_with_second(6, other)).claim(|c| c.idx = 1));
    }
    v
}

// ---------------------------------------------------------------- KA4

/// KA4: a ticket of another round (check 3); a look-alike ticket from a fake shard under
/// a fresh covenant ID at the genuine pool (PT2) and as the ticket sees it (check 2).
pub fn ka4() -> Vec<Atk> {
    // The ticket (and its successor) under OTHER_COV; every covenant output bound to it.
    let fresh_id = |b: &mut Built, _: &Case| {
        set_input_cov(b, TKT, Some(OTHER_COV));
        for o in b.outputs.iter_mut().filter(|o| o.covenant.is_some()) {
            o.covenant = Some(CovenantBinding::new(0, OTHER_COV));
        }
    };
    vec![
        Atk::new("KA4 round 2 ticket against a round 1 pool", TKT, "ticket:3", pay()).claim(|c| c.ticket.round = 2),
        Atk::new("KA4 look-alike ticket under a fresh ID, at the pool", POOL, "pool:PT2", pay()).built(fresh_id),
        Atk::new("KA4 look-alike ticket under a fresh ID, at the ticket", TKT, "ticket:2", pay()).built(fresh_id),
    ]
}

// ---------------------------------------------------------------- KA5

/// KA5: payout redirected (13), off by one (12), at the wrong index, an extra plain output
/// (11).
pub fn ka5() -> Vec<Atk> {
    let mut v = vec![];
    for (name, base, po) in [("PAY", pay(), 2usize), ("FINAL", fin(), 1usize)] {
        v.push(Atk::new(format!("KA5 {name} payout to another script"), TKT, "ticket:13", base.clone()).built(move |b, _| b.outputs[po].script_public_key = other_payout()));
        v.push(
            Atk::new(format!("KA5 {name} payout to another class (P2SH of the key)"), TKT, "ticket:13", base.clone())
                .built(move |b, c| match c {
                    Case::Claim(c) => b.outputs[po].script_public_key = Payout::P2sh(c.ticket.payout_key[..32].try_into().unwrap()).spk(),
                    _ => unreachable!(),
                }),
        );
        v.push(Atk::new(format!("KA5 {name} payout + 1"), TKT, "ticket:12", base.clone()).built(move |b, _| b.shift_value(po, 1)));
        v.push(Atk::new(format!("KA5 {name} payout - 1"), TKT, "ticket:12", base.clone()).built(move |b, _| b.shift_value(po, -1)));
        v.push(Atk::new(format!("KA5 {name} extra plain output"), TKT, "ticket:11", base.clone()).built(|b, _| extra_plain(b)));
    }
    // Payout at the wrong index. PAY: payout and ticket' swapped (output 1 is then not
    // authorized: check 10). FINAL: payout and change swapped (check 12, out1 value).
    v.push(Atk::new("KA5 PAY payout at output 1 (ticket' at 2)", TKT, "ticket:10", pay()).built(|b, _| b.outputs.swap(1, 2)));
    v.push(Atk::new("KA5 FINAL payout at output 2 (change at 1)", TKT, "ticket:12", fin()).built(|b, _| b.outputs.swap(1, 2)));
    v
}

// ---------------------------------------------------------------- KA6

/// KA6: pool' value off by one (12); pool' state changed (14).
pub fn ka6() -> Vec<Atk> {
    let mut v = vec![
        Atk::new("KA6 pool' value + 1", TKT, "ticket:12", pay()).built(|b, _| b.shift_value(0, 1)),
        Atk::new("KA6 pool' value - 1", TKT, "ticket:12", pay()).built(|b, _| b.shift_value(0, -1)),
    ];
    type PoolEdit = fn(&mut lottery::state::PoolState);
    let edits: [(&str, PoolEdit); 6] = [
        ("slice + 1", |s| s.slice += 1),
        ("deadline + 1", |s| s.deadline += 1),
        ("roots byte flipped", |s| s.roots[0] ^= 1),
        ("ticket_tpl byte flipped", |s| s.ticket_tpl[0] ^= 1),
        ("reserve + 1", |s| s.reserve += 1),
        ("funder key byte flipped", |s| s.funder_key[0] ^= 1),
    ];
    for (name, f) in edits {
        v.push(Atk::new(format!("KA6 pool' {name}"), TKT, "ticket:14", pay()).built(move |b, c| match c {
            Case::Claim(c) => {
                let mut s = c.pool.clone();
                f(&mut s);
                b.outputs[0].script_public_key = b.templates.pool_spk(&s);
            }
            _ => unreachable!(),
        }));
    }
    v
}

// ---------------------------------------------------------------- KA7 (script half)

/// KA7: the same tier's pool of the next round (check 3). The absorbed-outpoint half is a
/// ledger test.
pub fn ka7() -> Vec<Atk> {
    vec![Atk::new("KA7 the next round's pool of the same tier", TKT, "ticket:3", pay()).claim(|c| c.pool.round = 2)]
}

// ---------------------------------------------------------------- KA8

/// KA8: mode cheating.
pub fn ka8() -> Vec<Atk> {
    vec![
        // PAY shape with prize < P_MIN: nOut = 4, the script expects 2 + ch.
        Atk::new("KA8 PAY with prize < P_MIN", TKT, "ticket:11", accrue()).claim(|c| c.lie.mode = Some(ClaimMode::Pay)),
        // ACCRUE shape with prize >= P_MIN: 3 outputs pass check 11 as PAY without change;
        // out1 = value + prize fails PAY's out1 = in0.value.
        Atk::new("KA8 ACCRUE with prize >= P_MIN", TKT, "ticket:12", pay()).claim(|c| c.lie.mode = Some(ClaimMode::Accrue)),
        Atk::new("KA8 PAY shape (ticket' kept) when final", TKT, "ticket:10", fin()).claim(|c| c.lie.mode = Some(ClaimMode::Pay)),
        Atk::new("KA8 ACCRUE shape (ticket' kept) when final", TKT, "ticket:10", fin()).claim(|c| c.lie.mode = Some(ClaimMode::Accrue)),
        Atk::new("KA8 FINAL (ticket consumed) with another index unclaimed", TKT, "ticket:10", pay()).claim(|c| c.lie.mode = Some(ClaimMode::Final)),
    ]
}

// ---------------------------------------------------------------- KA9

/// KA9: ticket' tampering (15), ticket' value (12).
pub fn ka9() -> Vec<Atk> {
    type TicketEdit = fn(&mut TicketState, u8);
    let edits: [(&str, TicketEdit); 8] = [
        ("bit not set", |s, bit| s.claimed[0] &= !bit),
        ("the wrong bit set", |s, bit| s.claimed[0] = (s.claimed[0] & !bit) | (bit << 1)),
        ("round + 1", |s, _| s.round += 1),
        ("T + 1", |s, _| s.deadline += 1),
        ("b - 1", |s, _| s.b -= 1),
        ("bundle byte changed", |s, _| s.bundle[0] ^= 1),
        ("payout key changed", |s, _| s.payout_key[0] ^= 1),
        ("payout class changed", |s, _| s.payout_class = 2),
    ];
    let mut v = vec![];
    for (mode, base) in [("PAY", pay()), ("ACCRUE", accrue())] {
        for (name, f) in edits {
            v.push(Atk::new(format!("KA9 {mode} ticket' {name}"), TKT, "ticket:15", base.clone()).built(move |b, c| match c {
                Case::Claim(c) => {
                    let x = c.expected();
                    let mut s = x.ticket.clone().unwrap();
                    f(&mut s, x.bit);
                    b.outputs[1].script_public_key = b.templates.ticket_spk(&s);
                }
                _ => unreachable!(),
            }));
        }
        v.push(Atk::new(format!("KA9 {mode} ticket' value - 1"), TKT, "ticket:12", base.clone()).built(|b, _| b.shift_value(1, -1)));
    }
    v.push(Atk::new("KA9 PAY ticket' value + prize (prize paid twice)", TKT, "ticket:12", pay()).built(|b, c| match c {
        Case::Claim(c) => b.shift_value(1, c.expected().prize),
        _ => unreachable!(),
    }));
    v
}

// ---------------------------------------------------------------- KA10

/// The second claim of KA10: the PAY base's ticket with another payout key.
fn second_claim() -> ClaimCase {
    let mut c = pay();
    c.ticket.payout_key[0] ^= 0x55;
    c
}

/// KA10: shape.
pub fn ka10() -> Vec<Atk> {
    // A parallel group: inputs [ticket A, pool A, ticket B, pool B, funding]; outputs
    // [A's pool', ticket', payout; B's pool', ticket' (authorized by input 2), payout; change].
    let parallel = |b: &mut Built, _: &Case| {
        let bb = second_claim().build_with(b.templates.clone());
        let ins: Vec<In> = bb.inputs[..2].to_vec();
        for (j, i) in ins.into_iter().enumerate() {
            b.inputs.insert(2 + j, i);
        }
        let last = b.outputs.len() - 1;
        for (j, o) in bb.outputs[..3].iter().enumerate() {
            let mut o = o.clone();
            if let Some(cb) = o.covenant {
                o.covenant = Some(CovenantBinding::new(2, cb.covenant_id));
            }
            b.outputs.insert(last + j, o);
        }
        reindex(b);
    };
    // Two tickets against one pool with one payout: ticket B (claim entry) at input 2.
    let two_tickets = |b: &mut Built, _: &Case| {
        let bb = second_claim().build_with(b.templates.clone());
        b.inputs.insert(2, bb.inputs[0].clone());
        reindex(b);
    };
    vec![
        Atk::new("KA10 parallel group (a second ticket and pool pair)", TKT, "ticket:2", pay()).built(parallel),
        Atk::new("KA10 two tickets, one pool, one payout: at ticket A", TKT, "ticket:2", pay()).built(two_tickets),
        Atk::new("KA10 two tickets, one pool, one payout: at ticket B (input 2)", 2, "ticket:1", pay()).built(two_tickets),
        Atk::new("KA10 extra covenant output (a ticket' copy)", TKT, "ticket:10", pay()).built(|b, _| extra_cov(b, 1)),
        Atk::new("KA10 extra covenant output on FINAL (a pool' copy)", TKT, "ticket:10", fin()).built(|b, _| extra_cov(b, 0)),
        Atk::new("KA10 funding input with a covenant ID", TKT, "ticket:2", pay()).built(|b, _| set_input_cov(b, ClaimCase::funding_input(0), Some(OTHER_COV))),
        Atk::new("KA10 ticket and pool swapped: the pool at 0", 0, "pool:PT1", pay()).built(|b, _| b.inputs.swap(0, 1)),
        Atk::new("KA10 ticket and pool swapped: the ticket at 1", 1, "ticket:1", pay()).built(|b, _| b.inputs.swap(0, 1)),
        Atk::new("KA10 pool leg authorizing pool' (at the pool)", POOL, "pool:PT3", pay()).built(|b, c| b.outputs[0].covenant = Some(CovenantBinding::new(1, cov_of(c)))),
        Atk::new("KA10 pool leg authorizing pool' (at the ticket)", TKT, "ticket:10", pay()).built(|b, c| b.outputs[0].covenant = Some(CovenantBinding::new(1, cov_of(c)))),
        // Stage 4 (mutant targets, one change each).
        Atk::new("KA10 a covenant-less copy of the ticket as funding (at the copy, input 2)", 2, "ticket:1", pay()).built(|b, _| rogue_copy(b, TKT, 2)),
        Atk::new("KA10 the pool under another covenant ID (pool' joins the ticket's)", TKT, "ticket:2", pay()).built(|b, _| set_input_cov(b, POOL, Some(OTHER_COV))),
        Atk::new("KA10 covenant-less ticket and pool (plain outputs)", TKT, "ticket:2", pay()).built(|b, _| {
            set_input_cov(b, TKT, None);
            set_input_cov(b, POOL, None);
            b.outputs.iter_mut().for_each(|o| o.covenant = None);
        }),
        Atk::new("KA10 no funding input (fee 0)", TKT, "ticket:2", pay()).claim(|c| no_funding(&mut c.funding, &mut c.change, &mut c.fee)),
        Atk::new("KA10 nine funding inputs", TKT, "ticket:2", pay()).claim(|c| nine_funding(&mut c.funding)),
        Atk::new("KA10 pool' and ticket' without covenant binding", TKT, "ticket:10", pay()).built(|b, _| {
            b.outputs[0].covenant = None;
            b.outputs[1].covenant = None;
        }),
        Atk::new("KA10 PAY without its payout and change outputs", TKT, "ticket:11", pay()).built(|b, _| b.outputs.truncate(2)),
        Atk::new("KA10 the change under a fresh genesis covenant ID", TKT, "ticket:11", pay()).built(|b, _| genesis_output(b, ClaimCase::funding_input(0), 3)),
    ]
}

fn cov_of(c: &Case) -> harness::Hash {
    match c {
        Case::Claim(c) => c.cov,
        Case::Return(c) => c.cov,
        Case::Refresh(c) => c.cov,
    }
}

// ---------------------------------------------------------------- KA11

/// Templates whose ticket is a look-alike (the real ticket plus a permissive `backdoor()`),
/// compiled once.
pub fn fake_ticket() -> Arc<Templates> {
    static T: OnceLock<Arc<Templates>> = OnceLock::new();
    T.get_or_init(|| lookalike(Sil::Ticket)).clone()
}

/// KA11 (PT4): what the pool leg accepts at input 0.
pub fn ka11() -> Vec<Atk> {
    vec![
        // The same state under a look-alike ticket template (spent through `backdoor`); the
        // leg's lengths are the look-alike's, so only the template hash can reject it.
        Atk::new("KA11 look-alike ticket template at input 0", POOL, "pool:PT4", pay()).built(|b, c| {
            let Case::Claim(c) = c else { unreachable!() };
            let fake = fake_ticket();
            let e = b.inputs[TKT].entry.clone();
            b.inputs[TKT] = In {
                entry: UtxoEntry::new(e.amount, fake.ticket_spk(&c.ticket), e.block_daa_score, false, e.covenant_id),
                kind: InKind::Entry { template: fake.ticket.clone(), entry: "backdoor".into(), args: vec![], redeem: fake.ticket.redeem(&c.ticket.values()) },
            };
            *b.entry_args_mut(POOL) = ticket_leg_args(&fake);
        }),
        // Another pool (role 0x04, pool template) at input 0, spent through its refresh;
        // the leg's lengths are the pool template's.
        Atk::new("KA11 another pool at input 0", POOL, "pool:PT4", pay()).built(|b, c| {
            let Case::Claim(c) = c else { unreachable!() };
            let t = b.templates.clone();
            let e = b.inputs[TKT].entry.clone();
            b.inputs[TKT] = In {
                entry: UtxoEntry::new(c.pool_value, t.pool_spk(&c.pool), e.block_daa_score, false, e.covenant_id),
                kind: InKind::Entry { template: t.pool.clone(), entry: SIG_REFRESH.into(), args: vec![], redeem: t.pool_redeem(&c.pool) },
            };
            *b.entry_args_mut(POOL) = vec![ArtifactValue::Int(t.pool.prefix.len() as i64), ArtifactValue::Int(t.pool.suffix.len() as i64)];
        }),
        // The real ticket template with role 0x07 at input 0.
        Atk::new("KA11 real ticket template with role 0x07 at input 0", POOL, "pool:PT4", pay()).built(|b, c| {
            let Case::Claim(c) = c else { unreachable!() };
            let r = b.templates.ticket.redeem(&with_role(c.ticket.values(), BAD_ROLE));
            b.set_entry_redeem(TKT, r);
        }),
        // A ticket of another round at input 0 (PT4: round = own round).
        Atk::new("KA11 round 2 ticket at the round 1 pool", POOL, "pool:PT4", pay()).claim(|c| c.ticket.round = 2),
        // Stage 4 (mutant targets).
        Atk::new("KA11 a covenant-less copy of the pool as funding (at the copy, input 2)", 2, "pool:PT1", pay()).built(|b, _| rogue_copy(b, POOL, 2)),
        Atk::new("KA11 ticket_pl = -1", POOL, "pool:PT4", pay()).built(|b, _| b.entry_args_mut(POOL)[0] = ArtifactValue::Int(-1)),
        Atk::new("KA11 ticket_pl = SCRIPT_MAX + 1", POOL, "pool:PT4", pay()).built(|b, _| b.entry_args_mut(POOL)[0] = ArtifactValue::Int(1_000_001)),
        Atk::new("KA11 ticket_sl = -1", POOL, "pool:PT4", pay()).built(|b, _| b.entry_args_mut(POOL)[1] = ArtifactValue::Int(-1)),
        Atk::new("KA11 ticket_sl = SCRIPT_MAX + 1", POOL, "pool:PT4", pay()).built(|b, _| b.entry_args_mut(POOL)[1] = ArtifactValue::Int(1_000_001)),
    ]
}

// ---------------------------------------------------------------- KA12

/// KA12: fabricated ranges (D63).
pub fn ka12() -> Vec<Atk> {
    use lottery::game::M_CLAIM;
    vec![
        Atk::new("KA12 idx = b", TKT, "ticket:1", pay()).claim(|c| c.idx = c.ticket.b),
        Atk::new("KA12 idx = -1", TKT, "ticket:1", pay()).claim(|c| c.idx = -1),
        Atk::new("KA12 ticket b = 0", TKT, "ticket:1", pay()).claim(|c| c.ticket.b = 0),
        Atk::new("KA12 ticket b = B_MAX + 1", TKT, "ticket:1", pay()).claim(|c| c.ticket.b = lottery::game::TICKET_B_MAX + 1),
        Atk::new("KA12 ticket payout_class = 3", TKT, "ticket:1", pay()).claim(|c| c.ticket.payout_class = 3),
        Atk::new("KA12 ticket round = 0", TKT, "ticket:1", pay()).claim(|c| c.ticket.round = 0),
        Atk::new("KA12 pool_pl = -1", TKT, "ticket:1", pay()).built(|b, _| b.entry_args_mut(TKT)[CLAIM_ARG_POOL_PL] = ArtifactValue::Int(-1)),
        Atk::new("KA12 pool_sl = SCRIPT_MAX + 1", TKT, "ticket:1", pay()).built(|b, _| b.entry_args_mut(TKT)[CLAIM_ARG_POOL_SL] = ArtifactValue::Int(1_000_001)),
        Atk::new("KA12 bonus = pool m + 1", TKT, "ticket:5", pay()).claim(|c| c.ticket.bundle[5] = c.pool.m as u8 + 1),
        Atk::new("KA12 non-canonical main numbers (n1 = n2)", TKT, "ticket:5", pay()).claim(|c| c.ticket.bundle[1] = c.ticket.bundle[0]),
        Atk::new("KA12 pool m = M_CLAIM + 1", TKT, "ticket:3", pay()).claim(|c| c.pool.m = M_CLAIM + 1),
        Atk::new("KA12 pool k = 0", TKT, "ticket:3", pay()).claim(|c| c.pool.k = 0),
        Atk::new("KA12 pool k = K_MAX + 1", TKT, "ticket:3", pay()).claim(|c| c.pool.k = K_MAX as i64 + 1),
        Atk::new("KA12 pool tier 7", TKT, "ticket:3", pay()).claim(|c| c.pool.tier = 7),
        Atk::new("KA12 pool reserve > value", TKT, "ticket:3", pay()).claim(|c| c.pool.reserve = c.pool_value as i64 + 1),
        Atk::new("KA12 prize > remaining (value = reserve + prize - 1)", TKT, "ticket:8", pay()).claim(|c| {
            let prize = c.expected().prize as u64;
            c.pool_value = c.pool.reserve as u64 + prize - 1;
        }),
        // Stage 4 (mutant targets; fabricated states, D63): each violates one range only, and
        // every other check of the claim holds for it.
        Atk::new("KA12 ticket role 0x07 (FINAL)", TKT, "ticket:1", fin()).built(|b, _| input_role(b, TKT)),
        Atk::new("KA12 ticket and pool round 0", TKT, "ticket:1", pay()).claim(|c| {
            c.ticket.round = 0;
            c.pool.round = 0;
        }),
        Atk::new("KA12 ticket and pool round TOTAL_MAX", TKT, "ticket:1", pay()).claim(|c| {
            c.ticket.round = TOTAL_MAX;
            c.pool.round = TOTAL_MAX;
        }),
        Atk::new("KA12 ticket deadline -1", TKT, "ticket:1", pay()).claim(|c| c.ticket.deadline = -1),
        Atk::new("KA12 ticket deadline DAA_MAX", TKT, "ticket:1", pay()).claim(|c| c.ticket.deadline = DAA_MAX),
        Atk::new("KA12 ticket payout_class 3, paid to its P2SH form", TKT, "ticket:1", pay())
            .claim(|c| c.ticket.payout_class = 3)
            .built(|b, c| {
                let Case::Claim(c) = c else { unreachable!() };
                b.outputs[2].script_public_key = class3_payout(&c.ticket.payout_key);
            }),
        Atk::new("KA12 idx = b with a winning combination in the bundle padding", TKT, "ticket:1", pay()).claim(|c| {
            let comb = c.comb();
            c.ticket.bundle[12..18].copy_from_slice(&comb);
            c.idx = 2;
        }),
        Atk::new("KA12 pool_pl = SCRIPT_MAX + 1", TKT, "ticket:1", pay()).built(|b, _| b.entry_args_mut(TKT)[CLAIM_ARG_POOL_PL] = ArtifactValue::Int(1_000_001)),
        Atk::new("KA12 pool_sl = -1", TKT, "ticket:1", pay()).built(|b, _| b.entry_args_mut(TKT)[CLAIM_ARG_POOL_SL] = ArtifactValue::Int(-1)),
        Atk::new("KA12 pool role 0x07 (pool' follows)", TKT, "ticket:3", pay()).built(|b, _| pool_role(b, POOL, 0)),
        Atk::new("KA12 non-winning combination against a tier 7 pool", TKT, "ticket:3", pay()).claim(|c| {
            c.idx = 1;
            c.pool.tier = 7;
        }),
        Atk::new("KA12 pool slice -1", TKT, "ticket:3", pay()).claim(|c| c.pool.slice = -1),
        Atk::new("KA12 pool slice TOTAL_MAX + 2 (h = 2)", TKT, "ticket:3", ClaimCase::fabricated(4, 10, TIER_LOWER0, 20_000_000, &[2])).claim(|c| {
            c.pool.slice = TOTAL_MAX + 2;
            c.pool_value = (TOTAL_MAX / 2 + 1) as u64 + c.pool.reserve as u64 + KAS;
        }),
        Atk::new("KA12 pool m = 1 (trees of m = 1)", TKT, "ticket:3", m_base()).claim(|c| set_m(c, 1)),
        Atk::new("KA12 pool m = M_CLAIM + 1 (trees of m = 51)", TKT, "ticket:3", m_base()).claim(|c| set_m(c, M_CLAIM + 1)),
        Atk::new("KA12 pool deadline -1", TKT, "ticket:3", pay()).claim(|c| c.pool.deadline = -1),
        Atk::new("KA12 pool deadline DAA_MAX", TKT, "ticket:3", pay()).claim(|c| c.pool.deadline = DAA_MAX),
        Atk::new("KA12 pool reserve -1", TKT, "ticket:3", pay()).claim(|c| c.pool.reserve = -1),
        Atk::new("KA12 pool reserve RESERVE_MAX + 1", TKT, "ticket:3", pay()).claim(|c| {
            c.pool.reserve = CLAIM_RESERVE_MAX + 1;
            c.pool_value = (CLAIM_RESERVE_MAX + 1) as u64 + c.pool.slice as u64 + KAS;
        }),
        Atk::new("KA12 n1 = 0 (the predecessor rank's leaf)", TKT, "ticket:5", pay()).claim(|c| *c = noncanonical([0, 7, 12, 19, 28, 4])),
        Atk::new("KA12 n1 = n2 (a jackpot by a repeated number)", TKT, "ticket:5", pay()).claim(|c| *c = noncanonical([7, 7, 12, 19, 28, 4])),
        Atk::new("KA12 n2 = n3 (a jackpot by a repeated number)", TKT, "ticket:5", pay()).claim(|c| *c = noncanonical([3, 12, 12, 19, 28, 4])),
        Atk::new("KA12 n3 = n4 (a jackpot by a repeated number)", TKT, "ticket:5", pay()).claim(|c| *c = noncanonical([3, 7, 19, 19, 28, 4])),
        Atk::new("KA12 n4 = n5 (a jackpot by a repeated number)", TKT, "ticket:5", pay()).claim(|c| *c = noncanonical([3, 7, 12, 28, 28, 4])),
        Atk::new("KA12 n5 = 31", TKT, "ticket:5", pay()).claim(|c| *c = noncanonical([3, 7, 12, 19, 31, 4])),
        // The bonus out of range against the tier its main numbers then win (4 matches, no
        // bonus match: tier 3), so only the bonus range and the count slice can reject it.
        // The proofs stay those of the combination's rank (the reference would fall back to
        // rank 0 for a non-canonical combination).
        Atk::new("KA12 bonus = 0 against the tier 3 pool", TKT, "ticket:5", pay()).claim(|c| {
            c.lie.rank = Some(comb_of(c).rank());
            c.ticket.bundle[5] = 0;
            c.pool.tier = 3;
        }),
        Atk::new("KA12 bonus = pool m + 1 against the tier 3 pool", TKT, "ticket:5", pay()).claim(|c| {
            c.lie.rank = Some(comb_of(c).rank());
            c.ticket.bundle[5] = c.pool.m as u8 + 1;
            c.pool.tier = 3;
        }),
        // k = 0 with no proofs (leaves and siblings empty, as 2m * 0 and 576 * 0 bytes).
        Atk::new("KA12 pool k = 0 with no proofs", TKT, "ticket:3", pay()).claim(|c| {
            c.pool.k = 0;
            c.lie.k_proofs = Some(0);
        }),
        Atk::new("KA12 a combination no shard holds (h = 0)", TKT, "ticket:8", pay()).claim(|c| {
            let comb = comb_of(c);
            c.trees[0].set(comb.rank(), vec![0; c.pool.m as usize]);
            c.sync_roots();
        }),
        // Shard 0's leaf with count bytes 0x00 0x80 (32,768 read unsigned) at the bonus; the
        // pool root commits to it, the outputs follow h = 32,768 (ACCRUE of 610 sompi).
        Atk::new("KA12 a count of 32,768 (sign bit) at the bonus", TKT, "ticket:7", pay())
            .claim(|c| {
                use lottery::smt::{h, num_bytes, root_from_path};
                let comb = comb_of(c);
                let r = comb.rank();
                let s = 2 * (comb.bonus as usize - 1);
                let mut counts = c.trees[0].counts_bytes(r);
                counts[s..s + 2].copy_from_slice(&[0x00, 0x80]);
                let leaf = h(&[&[0x00][..], &num_bytes(r as u64, 3), &counts].concat());
                let root = root_from_path(r, leaf, &c.trees[0].siblings(r));
                c.pool.roots[..32].copy_from_slice(&root);
                c.lie.h = Some(0x8000);
            })
            .built(|b, c| {
                let Case::Claim(c) = c else { unreachable!() };
                let s = 2 * (comb_of(c).bonus as usize - 1);
                bytes_arg(b.entry_args_mut(TKT), CLAIM_ARG_LEAVES)[s..s + 2].copy_from_slice(&[0x00, 0x80]);
            }),
    ]
}

// ---------------------------------------------------------------- KA13, KA14

/// KA13: `return_deposit_after` (A5 path b).
pub fn ka13() -> Vec<Atk> {
    let r = ReturnCase::after;
    vec![
        Atk::new("KA13 lock_time = T + D_RET - 1", TKT, "ticket:A5b.3", r()).ret(|c| c.lock_time -= 1),
        Atk::new("KA13 a co-spent covenant input (a second ticket)", TKT, "ticket:A5b.2", r()).built(|b, _| {
            let i = b.inputs[0].clone();
            b.inputs.insert(1, i);
            reindex(b);
        }),
        Atk::new("KA13 payout to another script", TKT, "ticket:A5b.6", r()).built(|b, _| b.outputs[0].script_public_key = other_payout()),
        Atk::new("KA13 payout + 1", TKT, "ticket:A5b.6", r()).built(|b, _| b.shift_value(0, 1)),
        Atk::new("KA13 payout - 1", TKT, "ticket:A5b.6", r()).built(|b, _| b.shift_value(0, -1)),
        Atk::new("KA13 extra plain output", TKT, "ticket:A5b.5", r()).built(|b, _| extra_plain(b)),
        Atk::new("KA13 payout under the covenant ID", TKT, "ticket:A5b.4", r()).built(|b, c| b.outputs[0].covenant = Some(CovenantBinding::new(0, cov_of(c)))),
        // Stage 4: fabricated own-state ranges (D63, A5b.1) and shape (mutant targets).
        Atk::new("KA13 ticket role 0x07", TKT, "ticket:A5b.1", r()).built(|b, _| input_role(b, TKT)),
        Atk::new("KA13 ticket round 0", TKT, "ticket:A5b.1", r()).ret(|c| c.ticket.round = 0),
        Atk::new("KA13 ticket round TOTAL_MAX", TKT, "ticket:A5b.1", r()).ret(|c| c.ticket.round = TOTAL_MAX),
        Atk::new("KA13 ticket deadline -1", TKT, "ticket:A5b.1", r()).ret(|c| c.ticket.deadline = -1),
        Atk::new("KA13 ticket deadline DAA_MAX", TKT, "ticket:A5b.1", r()).ret(|c| c.ticket.deadline = DAA_MAX),
        Atk::new("KA13 ticket b = 0", TKT, "ticket:A5b.1", r()).ret(|c| c.ticket.b = 0),
        Atk::new("KA13 ticket b = B_MAX + 1", TKT, "ticket:A5b.1", r()).ret(|c| c.ticket.b = lottery::game::TICKET_B_MAX + 1),
        Atk::new("KA13 ticket payout_class 3, paid to its P2SH form", TKT, "ticket:A5b.1", r()).ret(|c| c.ticket.payout_class = 3).built(|b, c| {
            let Case::Return(c) = c else { unreachable!() };
            b.outputs[0].script_public_key = class3_payout(&c.ticket.payout_key);
        }),
        Atk::new("KA13 a covenant-less copy of the ticket as funding (at the copy, input 1)", 1, "ticket:A5b.1", r()).built(|b, _| rogue_copy(b, TKT, 1)),
        Atk::new("KA13 a covenant-less ticket", TKT, "ticket:A5b.2", r()).built(|b, _| set_input_cov(b, TKT, None)),
        Atk::new("KA13 no funding input (fee 0)", TKT, "ticket:A5b.2", r()).ret(|c| no_funding(&mut c.funding, &mut c.change, &mut c.fee)),
        Atk::new("KA13 nine funding inputs", TKT, "ticket:A5b.2", r()).ret(|c| nine_funding(&mut c.funding)),
        Atk::new("KA13 funding input with a covenant ID", TKT, "ticket:A5b.2", r()).built(|b, _| set_input_cov(b, 1, Some(OTHER_COV))),
        Atk::new("KA13 the change under a fresh genesis covenant ID", TKT, "ticket:A5b.5", r()).built(|b, _| genesis_output(b, 1, 1)),
        Atk::new("KA13 no output", TKT, "ticket:A5b.5", r()).built(|b, _| b.outputs.clear()),
    ]
}

/// KA14: `return_deposit_with_pool` (A5 path a).
pub fn ka14() -> Vec<Atk> {
    let r = ReturnCase::with_pool;
    let pool_edit = |f: fn(&mut lottery::state::PoolState)| {
        move |c: &mut ReturnCase| {
            if let ReturnPath::WithPool { pool, .. } = &mut c.path {
                f(pool)
            }
        }
    };
    vec![
        Atk::new("KA14 lock_time = pool deadline - 1", TKT, "ticket:A5a.4", r()).ret(|c| c.lock_time -= 1),
        Atk::new("KA14 a pool of round 2", TKT, "ticket:A5a.3", r()).ret(pool_edit(|p| p.round = 2)),
        Atk::new("KA14 pool' value + 1", TKT, "ticket:A5a.7", r()).built(|b, _| b.shift_value(0, 1)),
        Atk::new("KA14 pool' value - 1", TKT, "ticket:A5a.7", r()).built(|b, _| b.shift_value(0, -1)),
        Atk::new("KA14 pool' slice changed", TKT, "ticket:A5a.7", r()).built(|b, c| {
            let Case::Return(c) = c else { unreachable!() };
            let ReturnPath::WithPool { pool, .. } = &c.path else { unreachable!() };
            let mut s = pool.clone();
            s.slice -= 1;
            b.outputs[0].script_public_key = b.templates.pool_spk(&s);
        }),
        Atk::new("KA14 payout to another script", TKT, "ticket:A5a.8", r()).built(|b, _| b.outputs[1].script_public_key = other_payout()),
        Atk::new("KA14 payout - 1", TKT, "ticket:A5a.8", r()).built(|b, _| b.shift_value(1, -1)),
        // Stage 4: fabricated own-state ranges (A5a.1), pool ranges (A5a.3), shape.
        Atk::new("KA14 ticket role 0x07", TKT, "ticket:A5a.1", r()).built(|b, _| input_role(b, TKT)),
        Atk::new("KA14 ticket and pool round 0", TKT, "ticket:A5a.1", r()).ret(|c| {
            c.ticket.round = 0;
            with_pool_edit(|p, _| p.round = 0)(c);
        }),
        Atk::new("KA14 ticket and pool round TOTAL_MAX", TKT, "ticket:A5a.1", r()).ret(|c| {
            c.ticket.round = TOTAL_MAX;
            with_pool_edit(|p, _| p.round = TOTAL_MAX)(c);
        }),
        Atk::new("KA14 ticket deadline -1", TKT, "ticket:A5a.1", r()).ret(|c| c.ticket.deadline = -1),
        Atk::new("KA14 ticket deadline DAA_MAX", TKT, "ticket:A5a.1", r()).ret(|c| c.ticket.deadline = DAA_MAX),
        Atk::new("KA14 ticket b = 0", TKT, "ticket:A5a.1", r()).ret(|c| c.ticket.b = 0),
        Atk::new("KA14 ticket b = B_MAX + 1", TKT, "ticket:A5a.1", r()).ret(|c| c.ticket.b = lottery::game::TICKET_B_MAX + 1),
        Atk::new("KA14 ticket payout_class 3, paid to its P2SH form", TKT, "ticket:A5a.1", r()).ret(|c| c.ticket.payout_class = 3).built(|b, c| {
            let Case::Return(c) = c else { unreachable!() };
            b.outputs[1].script_public_key = class3_payout(&c.ticket.payout_key);
        }),
        Atk::new("KA14 a covenant-less copy of the ticket as funding (at the copy, input 2)", 2, "ticket:A5a.1", r()).built(|b, _| rogue_copy(b, TKT, 2)),
        Atk::new("KA14 pool_pl = -1", TKT, "ticket:A5a.1", r()).built(|b, _| b.entry_args_mut(TKT)[0] = ArtifactValue::Int(-1)),
        Atk::new("KA14 pool_pl = SCRIPT_MAX + 1", TKT, "ticket:A5a.1", r()).built(|b, _| b.entry_args_mut(TKT)[0] = ArtifactValue::Int(1_000_001)),
        Atk::new("KA14 pool_sl = -1", TKT, "ticket:A5a.1", r()).built(|b, _| b.entry_args_mut(TKT)[1] = ArtifactValue::Int(-1)),
        Atk::new("KA14 pool_sl = SCRIPT_MAX + 1", TKT, "ticket:A5a.1", r()).built(|b, _| b.entry_args_mut(TKT)[1] = ArtifactValue::Int(1_000_001)),
        Atk::new("KA14 the pool under another covenant ID (pool' joins the ticket's)", TKT, "ticket:A5a.2", r()).built(|b, _| set_input_cov(b, 1, Some(OTHER_COV))),
        Atk::new("KA14 no funding input (fee 0)", TKT, "ticket:A5a.2", r()).ret(|c| no_funding(&mut c.funding, &mut c.change, &mut c.fee)),
        Atk::new("KA14 nine funding inputs", TKT, "ticket:A5a.2", r()).ret(|c| nine_funding(&mut c.funding)),
        Atk::new("KA14 funding input with a covenant ID", TKT, "ticket:A5a.2", r()).built(|b, _| set_input_cov(b, 2, Some(OTHER_COV))),
        Atk::new("KA14 pool role 0x07 (pool' follows)", TKT, "ticket:A5a.3", r()).built(|b, _| pool_role(b, 1, 0)),
        Atk::new("KA14 pool tier 7", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, _| p.tier = 7)),
        Atk::new("KA14 pool slice -1", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, _| p.slice = -1)),
        Atk::new("KA14 pool slice TOTAL_MAX + 1", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, _| p.slice = TOTAL_MAX + 1)),
        Atk::new("KA14 pool m = 1", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, _| p.m = 1)),
        Atk::new("KA14 pool m = M_CLAIM + 1", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, _| p.m = lottery::game::M_CLAIM + 1)),
        Atk::new("KA14 pool k = 0", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, _| p.k = 0)),
        Atk::new("KA14 pool k = K_MAX + 1", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, _| p.k = K_MAX as i64 + 1)),
        Atk::new("KA14 pool deadline -1", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, _| p.deadline = -1)),
        Atk::new("KA14 pool deadline DAA_MAX", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, _| p.deadline = DAA_MAX)),
        Atk::new("KA14 pool reserve -1", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, _| p.reserve = -1)),
        Atk::new("KA14 pool reserve RESERVE_MAX + 1", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, v| {
            p.reserve = CLAIM_RESERVE_MAX + 1;
            *v = p.reserve as u64 + KAS;
        })),
        Atk::new("KA14 pool reserve > value", TKT, "ticket:A5a.3", r()).ret(with_pool_edit(|p, v| p.reserve = *v as i64 + 1)),
        Atk::new("KA14 pool' without covenant binding", TKT, "ticket:A5a.5", r()).built(|b, _| b.outputs[0].covenant = None),
        Atk::new("KA14 only pool' (no payout)", TKT, "ticket:A5a.6", r()).built(|b, _| b.outputs.truncate(1)),
        Atk::new("KA14 extra plain output", TKT, "ticket:A5a.6", r()).built(|b, _| extra_plain(b)),
        Atk::new("KA14 the change under a fresh genesis covenant ID", TKT, "ticket:A5a.6", r()).built(|b, _| genesis_output(b, 2, 2)),
    ]
}

// ---------------------------------------------------------------- KA15

/// KA15: `refresh()` of each of the ticket and a pool (input 0; spec M7 D99 removed the
/// marker's `refresh()`, SP10).
pub fn ka15() -> Vec<Atk> {
    let mut v = vec![];
    for (obj, base) in refresh_bases() {
        v.push(Atk::new(format!("KA15 {obj}: lock_time = creation + D_IDLE - 1"), 0, a6(obj, 3), base.clone()).refresh(|c| c.lock_time -= 1));
        v.push(Atk::new(format!("KA15 {obj}: creation = -1 (unaccepted parent)"), 0, a6(obj, 3), base.clone()).built(|b, _| set_input_daa(b, 0, u64::MAX)));
        v.push(Atk::new(format!("KA15 {obj}: value + 1"), 0, a6(obj, 6), base.clone()).built(|b, _| b.shift_value(0, 1)));
        v.push(Atk::new(format!("KA15 {obj}: value - 1"), 0, a6(obj, 6), base.clone()).built(|b, _| b.shift_value(0, -1)));
        v.push(Atk::new(format!("KA15 {obj}: a second covenant input"), 0, a6(obj, 2), base.clone()).built(|b, _| {
            let i = b.inputs[0].clone();
            b.inputs.insert(1, i);
            reindex(b);
        }));
        v.push(Atk::new(format!("KA15 {obj}: extra covenant output"), 0, a6(obj, 4), base.clone()).built(|b, _| extra_cov(b, 0)));
        v.push(Atk::new(format!("KA15 {obj}: extra plain output"), 0, a6(obj, 5), base.clone()).built(|b, _| extra_plain(b)));
        // One field of the successor changed (byte-identity, check 7).
        let fields: Vec<(&str, Box<dyn Fn(&RefreshObj) -> RefreshObj>)> = match obj {
            "ticket" => vec![("claimed", Box::new(|o| match o {
                RefreshObj::Ticket(s) => {
                    let mut s = s.clone();
                    s.claimed[0] ^= 1;
                    RefreshObj::Ticket(s)
                }
                _ => unreachable!(),
            }))],
            "pool" => vec![
                ("deadline", Box::new(|o| match o {
                    RefreshObj::Pool(s) => {
                        let mut s = s.clone();
                        s.deadline += 1;
                        RefreshObj::Pool(s)
                    }
                    _ => unreachable!(),
                })),
                ("roots", Box::new(|o| match o {
                    RefreshObj::Pool(s) => {
                        let mut s = s.clone();
                        s.roots[0] ^= 1;
                        RefreshObj::Pool(s)
                    }
                    _ => unreachable!(),
                })),
            ],
            _ => unreachable!("refresh_bases: ticket and pool only"),
        };
        for (field, f) in fields {
            v.push(Atk::new(format!("KA15 {obj}: successor {field} changed"), 0, a6(obj, 7), base.clone()).built(move |b, c| {
                let Case::Refresh(c) = c else { unreachable!() };
                b.outputs[0].script_public_key = f(&c.obj).spk(&b.templates);
            }));
        }
        // Stage 4 (mutant targets).
        v.push(Atk::new(format!("KA15 {obj}: a covenant-less copy as funding (at the copy, input 1)"), 1, a6(obj, 1), base.clone()).built(|b, _| rogue_copy(b, 0, 1)));
        v.push(Atk::new(format!("KA15 {obj}: a covenant-less object, plain successor"), 0, a6(obj, 2), base.clone()).built(|b, _| {
            set_input_cov(b, 0, None);
            b.outputs[0].covenant = None;
        }));
        v.push(Atk::new(format!("KA15 {obj}: no funding input (fee 0)"), 0, a6(obj, 2), base.clone()).refresh(|c| no_funding(&mut c.funding, &mut c.change, &mut c.fee)));
        v.push(Atk::new(format!("KA15 {obj}: nine funding inputs"), 0, a6(obj, 2), base.clone()).refresh(|c| nine_funding(&mut c.funding)));
        v.push(Atk::new(format!("KA15 {obj}: funding input with a covenant ID"), 0, a6(obj, 2), base.clone()).built(|b, _| set_input_cov(b, 1, Some(OTHER_COV))));
        v.push(Atk::new(format!("KA15 {obj}: creation = DAA_MAX"), 0, a6(obj, 3), base.clone()).built(|b, _| set_input_daa(b, 0, DAA_MAX as u64)));
        v.push(Atk::new(format!("KA15 {obj}: successor without covenant binding"), 0, a6(obj, 4), base.clone()).built(|b, _| b.outputs[0].covenant = None));
        v.push(Atk::new(format!("KA15 {obj}: no output (A6.4 rejects first)"), 0, a6(obj, 4), base.clone()).built(|b, _| b.outputs.clear()));
        v.push(Atk::new(format!("KA15 {obj}: the change under a fresh genesis covenant ID"), 0, a6(obj, 5), base.clone()).built(|b, _| genesis_output(b, 1, 1)));
    }
    v
}

// ---------------------------------------------------------------- engine-only value bounds

/// Input values above TOTAL_MAX = 2^62 (and sums above it). Consensus caps every output and
/// every transaction's input total at MAX_SOMPI = 2.9 * 10^18 < 2^62 (rusty-kaspa v2.1.0
/// consensus/core/src/constants.rs:34; tx_validation_in_isolation.rs:154-166;
/// tx_validation_in_utxo_context.rs:101-110), so no UTXO can carry these values on chain.
/// The local engine does not apply those rules, so these attacks show the value ranges do
/// execute (Stage 4, S4-3).
pub fn engine_only() -> Vec<Atk> {
    let big = TOTAL_MAX as u64 + 1;
    vec![
        Atk::new("ENGINE claim: ticket value TOTAL_MAX + 1", TKT, "ticket:1", pay()).claim(move |c| c.ticket_value = big),
        Atk::new("ENGINE claim: pool value TOTAL_MAX + 1", TKT, "ticket:3", pay()).claim(move |c| c.pool_value = big),
        Atk::new("ENGINE claim: ticket value TOTAL_MAX - 10, ACCRUE (value + prize > TOTAL_MAX)", TKT, "ticket:12", accrue()).claim(|c| c.ticket_value = TOTAL_MAX as u64 - 10),
        Atk::new("ENGINE return_deposit_after: ticket value TOTAL_MAX + 1", TKT, "ticket:A5b.1", ReturnCase::after()).ret(move |c| c.ticket_value = big),
        Atk::new("ENGINE return_deposit_with_pool: ticket value TOTAL_MAX + 1", TKT, "ticket:A5a.1", ReturnCase::with_pool()).ret(move |c| c.ticket_value = big),
        Atk::new("ENGINE return_deposit_with_pool: pool value TOTAL_MAX + 1", TKT, "ticket:A5a.3", ReturnCase::with_pool()).ret(with_pool_edit(move |_, v| *v = big)),
        // 2^63: OpTxInputAmount itself fails (u64 -> i64 conversion, NumberTooBig; rusty-kaspa
        // v2.1.0 crypto/txscript/src/opcodes/mod.rs:1100-1107), so `value >= 0` can never fail.
        Atk::new("ENGINE claim: ticket value 2^63 (OpTxInputAmount fails)", TKT, "ticket:1", pay()).claim(|c| c.ticket_value = 1 << 63),
        Atk::new("ENGINE return_deposit_after: ticket value 2^63 (OpTxInputAmount fails)", TKT, "ticket:A5b.1", ReturnCase::after()).ret(|c| c.ticket_value = 1 << 63),
        Atk::new("ENGINE return_deposit_with_pool: ticket value 2^63 (OpTxInputAmount fails)", TKT, "ticket:A5a.1", ReturnCase::with_pool()).ret(|c| c.ticket_value = 1 << 63),
    ]
}

/// Every attack of the library (KA1-KA15 and the engine-only ones), for lookup by id.
pub fn all_atks() -> Vec<Atk> {
    let mut v = vec![];
    for f in [ka1, ka2, ka3, ka4, ka5, ka6, ka7, ka8, ka9, ka10, ka11, ka12, ka13, ka14, ka15, engine_only] {
        v.extend(f());
    }
    v
}

/// The attack with this exact id.
pub fn atk(id: &str) -> Atk {
    let v = all_atks();
    let n = v.iter().filter(|a| a.id == id).count();
    assert_eq!(n, 1, "attack id `{id}`: {n} matches");
    v.into_iter().find(|a| a.id == id).unwrap()
}

// ---------------------------------------------------------------- KA16

/// KA16 bases: the leader's committed budget - 1 must fail on the budget.
pub fn ka16_cases() -> Vec<(&'static str, Case)> {
    vec![
        ("claim PAY", pay().into()),
        ("claim FINAL", fin().into()),
        ("claim ACCRUE", accrue().into()),
        ("return_deposit_after", ReturnCase::after().into()),
        ("return_deposit_with_pool", ReturnCase::with_pool().into()),
        ("refresh ticket", refresh_ticket().into()),
        ("refresh pool", refresh_pool().into()),
    ]
}

// ---------------------------------------------------------------- chain fixture (ledger cases)

/// Genesis (K = 4, price 1 KAS, lower shares [400, 300, 300, 0, 0]), one buy in shard 0 of
/// `bundle(win)`, close and draw of round 1 with seed [1; 32].
pub fn chain_round1(bundle: impl FnOnce(&[u8; 6]) -> Vec<Combo>) -> Chain {
    let mut ch = Chain::genesis_with(4, |g| g.controller.st = [400, 300, 300, 0, 0], true);
    let seed = [1u8; 32];
    let w = ch.winning(&seed);
    let win = [w.main[0], w.main[1], w.main[2], w.main[3], w.main[4], w.bonus];
    let (mut c, ops) = ch.buy_case(0);
    c.bundle = bundle(&win);
    ch.buy(0, &c, ops, 300_000);
    let (cc, ops) = ch.close_case(900_000);
    ch.close(&cc, ops, TEST_DEADLINE as u64 + 10);
    ch.draw_round(seed);
    ch
}
