//! Shared helpers for the M2 tests (spec M2 A7).
//!
//! Every attack changes one thing relative to a valid case and passes only on a *script*
//! error from the named input, never on a budget error.

use std::sync::Arc;

use harness::{Hash, TransactionOutpoint, TxScriptError, UtxoEntry};
use lottery::buy::InKind;
use lottery::buy::{BuyCase, Built, Final, KAS, Sources, Templates, b_max, funding_spk};
use lottery::claim::{ClaimCase, RefreshCase, RefreshObj, ReturnCase, ReturnPath};
use lottery::close::{CloseCase, FundingIn, ShardIn};
use lottery::draw::{DrawCase, PoolIn};
use lottery::genesis::{GenesisCase, covenant_id};
use lottery::ledger::Ledger;
use lottery::params::SetParamsCase;
use lottery::smt::Tree;
use lottery::state::{
    ControllerState, DrawerState, MarkerState, ParamsState, PoolState, PositionState, RecordState, RequestState, ShardState, TicketState, VaultState,
};

use super::accept;

/// A covenant ID that is not the instance's.
pub const OTHER_COV: Hash = Hash::from_bytes([0x99; 32]);

/// A rejection that counts for an attack: anything but a budget error, and not a
/// covenant-context error (those reject every input before any script runs).
pub fn is_script_rejection(e: &TxScriptError) -> bool {
    !matches!(e, TxScriptError::ExceededCommittedScriptUnits { .. } | TxScriptError::CovenantsError(_))
}

/// Input `input` of `b` must reject with a script error; returns the error.
pub fn reject_at(b: &Built, input: usize) -> TxScriptError {
    let f = b.finish();
    match f.run_input(input) {
        Ok(_) => panic!("ATTACK ACCEPTED by input {input}"),
        Err(e) => {
            assert!(is_script_rejection(&e), "input {input}: expected a script error, got {e:?}");
            e
        }
    }
}

/// M2 attack keyed on the input index: `mutate(base)` is rejected by input `input` with a
/// script error, and `base` itself is accepted by every input. The attack half runs first,
/// so until Stage 3 (stub bodies) these tests fail only at the base check
/// ("valid transaction rejected at input 0: VerifyError").
pub fn attack_at(base: &Built, input: usize, mutate: impl FnOnce(&mut Built)) {
    let mut m = base.clone();
    mutate(&mut m);
    reject_at(&m, input);
    accept(base);
}

/// As `attack_at` for an attack that is a different case: `bad` is rejected by `input`,
/// `good` is accepted.
pub fn attack_pair(good: &Built, bad: &Built, input: usize) {
    reject_at(bad, input);
    accept(good);
}

// ------------------------------------------------------------------------ M4 helpers

/// M4 acceptance of a claim, return or refresh (`step` names it): every input under its
/// committed budget, input 0 first. On a rejection the panic names the step and lists every
/// input's result, so a test that fails shows which M4 transaction (not a genesis, buy,
/// close or draw before it) the scripts rejected, and why.
pub fn accept_m4(step: &str, b: &Built) -> Final {
    let f = b.finish();
    if let Err((i, e)) = f.run() {
        panic!("M4 step `{step}`: valid transaction rejected at input {i}: {e:?}; all inputs: {:?}", f.run_all());
    }
    f
}

// ------------------------------------------------------------------------ M3 helpers

/// M3 acceptance of a valid draw: every input except the leader runs first, then the
/// drawer (input 0), each under its committed budget.
pub fn accept_draw(b: &Built) -> Final {
    let f = b.finish();
    let n = f.tx.inputs.len();
    for i in (1..n).chain([0]) {
        if let Err(e) = f.run_input(i) {
            panic!("valid draw rejected at input {i}: {e:?}");
        }
    }
    f
}

/// M3 attack keyed on the input index (spec M3 A8): `bad` is rejected by input `input` with
/// a script error, `good` passes that same input, and `good` is accepted by every input.
/// The middle step makes an attack count only when the targeted input tells the two
/// transactions apart.
pub fn attack3_pair(good: &Built, bad: &Built, input: usize) {
    strike(good, bad, input);
    accept_draw(good);
}

/// Steps 1 and 2 of `attack3_pair` only (`bad` rejected by `input`, `good` passes `input`).
/// A test that probes several targets calls `strike` for each (leg targets first) and
/// `accept_draw` on the base once at the end.
pub fn strike(good: &Built, bad: &Built, input: usize) {
    reject_at(bad, input);
    let f = good.finish();
    if let Err(e) = f.run_input(input) {
        panic!("BASE REJECTED at target input {input}: {e:?}");
    }
    if input == 0 && attrib_enabled() {
        let label = first_failing_check(bad);
        println!("ATTRIB {} : check {label}", std::thread::current().name().unwrap_or("?"));
    }
}

// ------------------------------------------------------------------------ check attribution

/// DIAGNOSTIC INSTRUMENT, not a test result on its own: with `M3_ATTRIB` set, every
/// drawer-targeted `strike` also prints the first A4 check that rejects the attack
/// ("ATTRIB <test> : check <N>"). The engine reports only VerifyError (require messages are
/// not compiled), so the check is found by truncation: drawer.sil is cut just before each
/// "// ==== check N" line (open blocks closed), compiled, swapped in as the leader of the
/// attack transaction, and only input 0 is run; the first truncation that rejects names the
/// check (a truncation includes every earlier check, so rejection is monotone in N).
/// Run: `M3_ATTRIB=1 cargo test -p lottery --release --test m3_attacks -- --nocapture
/// --test-threads=1 | grep ATTRIB`.
pub fn attrib_enabled() -> bool {
    std::env::var_os("M3_ATTRIB").is_some()
}

/// (label, truncated drawer template) for every check marker, in order; the last entry is the
/// whole source.
fn drawer_truncations() -> &'static Vec<(String, Arc<lottery::buy::Template>)> {
    static T: std::sync::OnceLock<Vec<(String, Arc<lottery::buy::Template>)>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let src = std::fs::read_to_string(harness::covenant_path("drawer.sil")).expect("drawer.sil").replace("

", "
");
        let lines: Vec<&str> = src.lines().collect();
        let marks: Vec<(usize, String)> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.trim_start().strip_prefix("// ==== check ").map(|r| (i, r.split(':').next().unwrap().trim().to_string())))
            .collect();
        let ctrl = tpl().controller.hash;
        marks
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
                        assert!(depth >= 2, "truncation before check {label}: brace depth {depth}");
                        kept.join("
") + "
" + &"}
".repeat(depth as usize)
                    }
                };
                (label.clone(), Arc::new(lottery::draw::compile_drawer(&body, ctrl)))
            })
            .collect()
    })
}

/// `b` with the leader (input 0) running template `t`: the same state bytes and arguments,
/// the input's P2SH script rebuilt; output 0 (drawer') rebuilt under `t` when it is the
/// genuine successor (round + 1, all else equal). Returns whether output 0 was rebuilt.
fn with_leader(b: &Built, t: &Arc<lottery::buy::Template>) -> (Built, bool) {
    use kaspa_txscript::pay_to_script_hash_script;
    let mut m = b.clone();
    let (old, redeem) = match &b.inputs[0].kind {
        InKind::Entry { template, redeem, .. } => (template.clone(), redeem.clone()),
        _ => panic!("input 0 is not an entry input"),
    };
    let state = redeem[old.prefix.len()..redeem.len() - old.suffix.len()].to_vec();
    // Drawer state encoding: role (0x01 0x0a), then round as an 8-byte push (0x08 + LE).
    assert_eq!(&state[..3], &[0x01, 0x0a, 0x08], "drawer state layout");
    let mut next = state.clone();
    let round = i64::from_le_bytes(next[3..11].try_into().unwrap());
    next[3..11].copy_from_slice(&(round + 1).to_le_bytes());
    let genuine = pay_to_script_hash_script(&[old.prefix.as_slice(), &next, &old.suffix].concat()) == b.outputs[0].script_public_key;
    let new_redeem = [t.prefix.as_slice(), &state, &t.suffix].concat();
    m.set_entry_redeem(0, new_redeem);
    if let InKind::Entry { template, .. } = &mut m.inputs[0].kind {
        *template = t.clone();
    }
    if genuine {
        m.outputs[0].script_public_key = pay_to_script_hash_script(&[t.prefix.as_slice(), &next, &t.suffix].concat());
    }
    (m, genuine)
}

/// The label of the first A4 check (truncation) that rejects `bad` at input 0.
pub fn first_failing_check(bad: &Built) -> String {
    let tr = drawer_truncations();
    let rejects = |j: usize| -> (bool, bool) {
        let (m, genuine) = with_leader(bad, &tr[j].1);
        (m.finish().run_input(0).is_err(), genuine)
    };
    // Sanity: the whole-source truncation is the real drawer and must reject.
    let (last, genuine) = rejects(tr.len() - 1);
    assert!(last, "attribution: the full drawer source accepts the attack");
    let (mut lo, mut hi) = (0usize, tr.len() - 1);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if rejects(mid).0 { hi = mid } else { lo = mid + 1 }
    }
    let label = tr[lo].0.clone();
    // Output 0's script edited (not the genuine successor): it cannot be rebuilt under a
    // truncated template, so every truncation from check 22 on rejects it. A label of 22 is
    // still exact: checks 23-25 do not read output 0, and checks <= 21 accepted.
    if !genuine {
        return format!("{label} (output 0 script edited)");
    }
    label
}

/// As `attack3_pair` with `bad` = `mutate(base)`.
pub fn attack3(base: &Built, input: usize, mutate: impl FnOnce(&mut Built)) {
    let mut m = base.clone();
    mutate(&mut m);
    attack3_pair(base, &m, input);
}

// ------------------------------------------------------------------------ fixtures

/// Committed templates.
pub fn tpl() -> Arc<Templates> {
    Templates::get(b_max())
}

/// The M2 attack base (spec A7): `CloseCase::new_with(4, t)` (a valid close of round 1 with
/// a fabricated drawn controller, D33) with sales in shards 0 and 2 (P_R > 0 in shard 0)
/// and distinct shard roots, so S, P_R, C_close and the marker roots are all non-trivial.
pub fn close_base(t: &Templates) -> CloseCase {
    let mut c = CloseCase::new_with(4, t);
    c.set_sales(0, 120 * KAS as i64, 5 * KAS as i64);
    c.set_sales(2, 37 * KAS as i64, 0);
    for (i, s) in c.shards.iter_mut().enumerate() {
        s.state.root = [0x10 + i as u8; 32];
    }
    c
}

/// Point the controller's route table at `t`.
pub fn set_route(c: &mut CloseCase, t: &Templates) {
    c.controller.shard_tpl = t.shard.hash;
    c.controller.marker_tpl = t.marker.hash;
    c.controller.params_tpl = t.params.hash;
}

// ------------------------------------------------------------------------ mutants and look-alikes
//
// Mechanism (spec A7 mutants). Stage 3 puts a tag comment `// mut:<ID>` at the end of each
// named `require` line in the .sil sources (several IDs on one line: `// mut:A,B`). A
// mutant removes every line carrying one of the given IDs and recompiles. A tagged line must
// hold only that one `require(...)` statement, so removing it leaves valid code. The IDs
// Stage 3 must place are listed in docs/specs/2-handoff.md.

#[derive(Clone, Copy, Debug)]
pub enum Sil {
    Controller,
    Params,
    Shard,
    Marker,
    // M3 (D38).
    Vault,
    Pool,
    Drawer,
    // M4 (stage 2b): the ticket compiles after the pool (ticket(pool), D54), so a ticket
    // mutant or look-alike changes the ticket and shard hashes only.
    Ticket,
    // M5 (D74): the record has no dependency; position(vault); request(record, position).
    // A record or position mutant also changes the request hash; a vault mutant changes
    // the position and request hashes.
    Record,
    Position,
    Request,
}

fn line_tags(line: &str) -> Vec<&str> {
    match line.find("// mut:") {
        Some(p) => line[p + 7..].split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).collect(),
        None => vec![],
    }
}

/// `src` without every line tagged with one of `ids`. Panics if an ID tags no line.
pub fn strip_tags(src: &str, ids: &[&str]) -> String {
    for id in ids {
        assert!(src.lines().any(|l| line_tags(l).contains(id)), "mutant tag `// mut:{id}` not found (Stage 3 must place it)");
    }
    src.lines().map(|l| if line_tags(l).iter().any(|t| ids.contains(t)) { "" } else { l }).collect::<Vec<_>>().join("\n")
}

fn source_mut(src: &mut Sources, which: Sil) -> &mut String {
    match which {
        Sil::Controller => &mut src.controller,
        Sil::Params => &mut src.params,
        Sil::Shard => &mut src.shard,
        Sil::Marker => &mut src.marker,
        Sil::Vault => &mut src.vault,
        Sil::Pool => &mut src.pool,
        Sil::Drawer => &mut src.drawer,
        Sil::Ticket => &mut src.ticket,
        Sil::Record => &mut src.record,
        Sil::Position => &mut src.position,
        Sil::Request => &mut src.request,
    }
}

fn hash_of(t: &Templates, which: Sil) -> [u8; 32] {
    match which {
        Sil::Controller => t.controller.hash,
        Sil::Params => t.params.hash,
        Sil::Shard => t.shard.hash,
        Sil::Marker => t.marker.hash,
        Sil::Vault => t.vault.hash,
        Sil::Pool => t.pool.hash,
        Sil::Drawer => t.drawer.hash,
        Sil::Ticket => t.ticket.hash,
        Sil::Record => t.record.hash,
        Sil::Position => t.position.hash,
        Sil::Request => t.request.hash,
    }
}

/// Templates compiled with the tagged lines of one covenant removed. Compile order is
/// ticket -> controller -> drawer -> marker, vault, pool -> shard, params (M3 D38), so a
/// controller mutant also changes every other hash except the ticket's, and a drawer
/// mutant changes the marker, vault and pool hashes: build cases with `CloseCase::new_with(k, &t)` and
/// `build_with(t)`.
pub fn mutant(which: Sil, ids: &[&str]) -> Arc<Templates> {
    let mut src = Sources::committed(b_max());
    let s = source_mut(&mut src, which);
    *s = strip_tags(s, ids);
    let t = Templates::compile_sources(b_max(), &src);
    assert_ne!(hash_of(&t, which), hash_of(&tpl(), which), "mutant {which:?} {ids:?} compiled to the real template");
    Arc::new(t)
}

/// A look-alike source: the real covenant plus a permissive `backdoor()` entry (different
/// template hash, same state layout).
pub fn lookalike_source(src: &str) -> String {
    let end = src.rfind('}').expect("closing brace");
    format!("{}\n    entry backdoor() {{\n        require(this.activeInputIndex >= 0);\n    }}\n}}\n", &src[..end])
}

/// Templates in which one covenant is its look-alike, the others committed. A look-alike
/// controller also changes the shard and params hashes (see `mutant`).
pub fn lookalike(which: Sil) -> Arc<Templates> {
    let mut src = Sources::committed(b_max());
    let s = source_mut(&mut src, which);
    *s = lookalike_source(s);
    let t = Templates::compile_sources(b_max(), &src);
    assert_ne!(hash_of(&t, which), hash_of(&tpl(), which));
    Arc::new(t)
}

// ------------------------------------------------------------------------ chained ledger runs

/// A deployed instance followed through a test ledger (D33): genesis, buys, set_params and
/// closes are run (every input must pass) and then applied, so later transactions spend the
/// real outpoints.
#[derive(Clone)]
pub struct Chain {
    pub ledger: Ledger,
    pub t: Arc<Templates>,
    pub cov: Hash,
    /// (outpoint, state, creation DAA).
    pub controller: (TransactionOutpoint, ControllerState, u64),
    /// Boost pot O of the current controller UTXO.
    pub pot: u64,
    pub params: (TransactionOutpoint, ParamsState, u64),
    pub shards: Vec<(TransactionOutpoint, ShardState, u64)>,
    /// The last marker created.
    pub marker: Option<(TransactionOutpoint, MarkerState)>,
    /// The vault and the drawer (genesis outputs k + 2 and k + 3, D50).
    pub vault: (TransactionOutpoint, VaultState, u64),
    pub drawer: (TransactionOutpoint, DrawerState, u64),
    /// The counter tree of each current shard (updated by `buy`, reset by `close`).
    pub trees: Vec<Tree>,
    /// The trees of the round frozen in `marker` (the draw's count proofs come from them).
    pub marker_trees: Vec<Tree>,
    /// Live prize pools: (outpoint, state, remaining = value - R, creation DAA).
    pub pools: Vec<(TransactionOutpoint, PoolState, u64, u64)>,
    /// Creation DAA of the marker in `marker` (its close; a `refresh()` moves it, M4).
    pub marker_daa: u64,
    /// Live tickets (M4): (outpoint, state, value = d b + accrued, creation DAA).
    pub tickets: Vec<(TransactionOutpoint, TicketState, u64, u64)>,
    /// The shard trees of every drawn round, by round (claims prove counts against them).
    pub drawn_trees: std::collections::BTreeMap<i64, Vec<Tree>>,
    /// M5: live share positions (outpoint, state, value = R, creation DAA); the deployer's
    /// genesis position first (S1-10).
    pub positions: Vec<(TransactionOutpoint, PositionState, u64, u64)>,
    /// M5: live vault requests (outpoint, state, value = R + accrued, creation DAA).
    pub requests: Vec<(TransactionOutpoint, RequestState, u64, u64)>,
    /// M5: live settlement records (outpoint, state, value, creation DAA).
    pub records: Vec<(TransactionOutpoint, RecordState, u64, u64)>,
}

impl Chain {
    /// Genesis of `GenesisCase::new(k)` (J_boost = 0, pot 0), applied at DAA 200.
    pub fn genesis(k: usize) -> Self {
        Self::genesis_with(k, |_| {}, true)
    }

    /// Genesis of `GenesisCase::new(k)` changed by `edit`. With `checked = false` the
    /// builder's `validate()` is bypassed (TEST-ONLY `build_unchecked_for_tests`), so an
    /// attack can deploy an instance the builder refuses.
    pub fn genesis_with(k: usize, edit: impl FnOnce(&mut GenesisCase), checked: bool) -> Self {
        let t = tpl();
        let mut ledger = Ledger::new();
        let op = ledger.fund(lottery::genesis::GENESIS_FUNDING, funding_spk(), 100);
        let entry = ledger.get(&op).unwrap().clone();
        let mut g = GenesisCase::new_with(k, vec![(op, entry)], &t);
        edit(&mut g);
        let b = if checked { g.build_with(t.clone()) } else { g.build_unchecked_for_tests(t.clone()) };
        let f = accept(&b);
        let cov = covenant_id(&f);
        let daa = 200;
        let outs = ledger.apply(&f.tx, &f.entries, daa).expect("genesis applies");
        Self {
            ledger,
            t,
            cov,
            controller: (outs[0], g.controller.clone(), daa),
            pot: g.pot,
            params: (outs[1], g.params.clone(), daa),
            shards: (0..k).map(|i| (outs[2 + i], g.shards[i].clone(), daa)).collect(),
            marker: None,
            vault: (outs[g.vault_output()], g.vault.clone(), daa),
            drawer: (outs[g.drawer_output()], g.drawer.clone(), daa),
            trees: (0..k).map(|_| Tree::new(g.controller.m as usize)).collect(),
            marker_trees: vec![],
            pools: vec![],
            marker_daa: 0,
            tickets: vec![],
            drawn_trees: Default::default(),
            positions: vec![(outs[g.position_output()], g.position.clone(), g.reserve(), daa)],
            requests: vec![],
            records: vec![],
        }
    }

    /// A buy case on shard `i` funded from the ledger (100 KAS at DAA 0, as `BuyCase`
    /// builds its funding entries), with the outpoints to set on the `Built`.
    pub fn buy_case(&mut self, i: usize) -> (BuyCase, Vec<TransactionOutpoint>) {
        let (op, s, daa) = self.shards[i].clone();
        let mut c = BuyCase::new(self.t.b_max).with_tree(self.trees[i].clone());
        assert_eq!(c.shard.root, s.root, "buy_case: the tracked tree matches the shard root");
        c.shard = s;
        c.reserve = self.controller.1.reserve as u64;
        c.shard_daa = daa;
        c.cov = self.cov;
        c.funding = vec![100 * KAS];
        let fop = self.ledger.fund(100 * KAS, funding_spk(), 0);
        (c, vec![op, fop])
    }

    pub fn buy_built(&self, c: &BuyCase, ops: Vec<TransactionOutpoint>) -> Built {
        let mut b = c.build_with(self.t.clone());
        b.outpoints = ops;
        b
    }

    /// Run and apply a buy on shard `i` at `daa`; the shard moves to its successor.
    pub fn buy(&mut self, i: usize, c: &BuyCase, ops: Vec<TransactionOutpoint>, daa: u64) -> Final {
        let f = accept(&self.buy_built(c, ops));
        let outs = self.ledger.apply(&f.tx, &f.entries, daa).expect("buy applies");
        self.shards[i] = (outs[0], c.successor(), daa);
        self.trees[i] = c.proofs().2;
        self.tickets.push((outs[1], c.ticket(), f.tx.outputs[1].value, daa));
        f
    }

    /// A close case over the chain's current objects, lock time = T, with one funding input
    /// of 100 KAS created at `funding_daa`; and the outpoints in input order.
    pub fn close_case(&mut self, funding_daa: u64) -> (CloseCase, Vec<TransactionOutpoint>) {
        let k = self.shards.len();
        let mut c = CloseCase::new_with(k, &self.t);
        c.controller = self.controller.1.clone();
        c.controller_daa = self.controller.2;
        c.pot = self.pot;
        c.params = self.params.1.clone();
        c.params_daa = self.params.2;
        c.shards = self.shards.iter().map(|(_, s, d)| ShardIn { state: s.clone(), daa: *d }).collect();
        c.cov = self.cov;
        c.lock_time = c.controller.deadline as u64;
        let fop = self.ledger.fund(100 * KAS, lottery::buy::spk_of(&c.funding_sk), funding_daa);
        c.funding = vec![FundingIn { value: 100 * KAS, daa: funding_daa }];
        let mut ops = vec![self.controller.0, self.params.0];
        ops.extend(self.shards.iter().map(|s| s.0));
        ops.push(fop);
        (c, ops)
    }

    pub fn close_built(&self, c: &CloseCase, ops: Vec<TransactionOutpoint>) -> Built {
        let mut b = c.build_with(self.t.clone());
        b.outpoints = ops;
        b
    }

    /// Run and apply a close at `daa`; the chain moves to round r+1's objects.
    pub fn close(&mut self, c: &CloseCase, ops: Vec<TransactionOutpoint>, daa: u64) -> Final {
        let x = c.expected();
        let f = accept(&self.close_built(c, ops));
        let outs = self.ledger.apply(&f.tx, &f.entries, daa).expect("close applies");
        let k = c.k();
        self.controller = (outs[0], x.controller.clone(), daa);
        self.pot = x.values[0] - x.controller.acc as u64 - x.controller.reserve as u64;
        self.params = (outs[1], x.params.clone(), daa);
        self.shards = (0..k).map(|i| (outs[2 + i], x.shards[i].clone(), daa)).collect();
        self.marker = Some((outs[k + 2], x.marker.clone()));
        self.marker_daa = daa;
        let fresh = (0..k).map(|_| Tree::new(x.m_next as usize)).collect();
        self.marker_trees = std::mem::replace(&mut self.trees, fresh);
        f
    }

    /// Run and apply a `set_params` at `daa` on the chain's params (funding from the ledger).
    pub fn set_params(&mut self, sp: &mut SetParamsCase, daa: u64) -> Final {
        sp.params = self.params.1.clone();
        sp.value = self.controller.1.reserve as u64;
        sp.params_daa = self.params.2;
        sp.cov = self.cov;
        sp.nonce = sp.params.nonce + 1;
        let fops: Vec<TransactionOutpoint> = sp.funding.iter().map(|&v| self.ledger.fund(v, funding_spk(), 0)).collect();
        let mut b = sp.build_with(self.t.clone());
        b.outpoints = std::iter::once(self.params.0).chain(fops).collect();
        let f = accept(&b);
        let outs = self.ledger.apply(&f.tx, &f.entries, daa).expect("set_params applies");
        self.params = (outs[0], sp.successor(), daa);
        f
    }

    /// The winning numbers the TEST-ONLY seed gives the current controller's round (call it
    /// before `close()`: the marker takes the controller's round and m).
    pub fn winning(&self, seed: &[u8; 32]) -> lottery::game::SeedNumbers {
        let c = &self.controller.1;
        lottery::game::seed_numbers(&self.cov.as_bytes(), c.round, seed, c.m)
    }

    /// A draw case over the chain's drawer, marker, vault and controller, absorbing the live
    /// pools at `absorb` (indices into `self.pools`), lock time max(seed_ref, deadlines),
    /// one funding input of 100 KAS created at `funding_daa`; and the outpoints in input order.
    pub fn draw_case(&mut self, seed: [u8; 32], absorb: &[usize], funding_daa: u64) -> (DrawCase, Vec<TransactionOutpoint>) {
        let (mop, mk) = self.marker.clone().expect("a closed round to draw");
        let mut c = DrawCase::new_with(self.marker_trees.len(), &self.t);
        c.drawer = self.drawer.1.clone();
        c.drawer_daa = self.drawer.2;
        c.marker = mk;
        // The marker was created by the close that created the current controller (or by
        // a later `refresh()`, M4).
        c.marker_daa = self.marker_daa;
        c.vault = self.vault.1.clone();
        c.vault_daa = self.vault.2;
        c.controller = self.controller.1.clone();
        c.pot = self.pot;
        c.controller_daa = self.controller.2;
        c.trees = self.marker_trees.clone();
        c.test_only_seed = seed;
        c.cov = self.cov;
        c.absorbed = absorb.iter().map(|&i| PoolIn { state: self.pools[i].1.clone(), remaining: self.pools[i].2, daa: self.pools[i].3 }).collect();
        c.fix_lock_time();
        let fop = self.ledger.fund(100 * KAS, lottery::buy::spk_of(&c.funding_sk), funding_daa);
        c.funding = vec![FundingIn { value: 100 * KAS, daa: funding_daa }];
        let mut ops = vec![self.drawer.0, mop, self.vault.0, self.controller.0];
        ops.extend(absorb.iter().map(|&i| self.pools[i].0));
        ops.push(fop);
        (c, ops)
    }

    pub fn draw_built(&self, c: &DrawCase, ops: Vec<TransactionOutpoint>) -> Built {
        let mut b = c.build_with(self.t.clone());
        b.outpoints = ops;
        b
    }

    /// Run and apply a draw at `daa`: the drawer, vault and controller advance, the
    /// absorbed pools are removed and the new pools recorded, the marker is spent.
    pub fn draw(&mut self, c: &DrawCase, ops: Vec<TransactionOutpoint>, daa: u64) -> Final {
        let x = c.expected();
        let absorbed: Vec<TransactionOutpoint> = ops[4..4 + c.a()].to_vec();
        // M5: a draw that converts runs through `accept_draw5` (M5_PREFLIGHT, common::m5);
        // one with nothing pending is the M4 draw, accepted as before.
        let f = if x.rec { super::m5::accept_draw5("draw (conversion)", c, &self.draw_built(c, ops)) } else { accept_draw(&self.draw_built(c, ops)) };
        let outs = self.ledger.apply(&f.tx, &f.entries, daa).expect("draw applies");
        self.drawer = (outs[0], x.drawer.clone(), daa);
        self.vault = (outs[1], x.vault.clone(), daa);
        self.controller = (outs[2], x.controller.clone(), daa);
        self.pot = x.o_prime as u64;
        self.pools.retain(|p| !absorbed.contains(&p.0));
        let r = c.reserve();
        for (p, s) in x.new_pools.iter().enumerate() {
            let o = x.pool_output(p);
            self.pools.push((outs[o], s.clone(), x.values[o] - r, daa));
        }
        if let (Some(i), Some(rc)) = (x.record_output(), &x.record) {
            self.records.push((outs[i], rc.clone(), x.values[i], daa));
        }
        self.marker = None;
        self.drawn_trees.insert(c.marker.round, std::mem::take(&mut self.marker_trees));
        f
    }

    /// Draw the closed round with `seed` and no absorption: one fresh funding input created
    /// at seed_ref + DRAW_WINDOW + 1,000, applied at lock_time + 10 (as DP16).
    pub fn draw_round(&mut self, seed: [u8; 32]) -> Final {
        let seed_ref = self.marker.as_ref().expect("a closed round to draw").1.seed_ref;
        let (c, ops) = self.draw_case(seed, &[], (seed_ref + lottery::game::DRAW_WINDOW + 1_000) as u64);
        self.draw(&c, ops, c.lock_time + 10)
    }

    // -------------------------------------------------------------------- M4 steps

    /// As `buy_case` with one funding input of `funding` sompi (buys at a higher price).
    pub fn buy_case_funded(&mut self, i: usize, funding: u64) -> (BuyCase, Vec<TransactionOutpoint>) {
        let (mut c, mut ops) = self.buy_case(i);
        c.funding = vec![funding];
        ops[1] = self.ledger.fund(funding, funding_spk(), 0);
        (c, ops)
    }

    /// A ticket fabricated directly as a UTXO under the instance covenant ID (D63): the
    /// ledger holds it, `tickets` tracks it. Returns its index in `tickets`.
    pub fn fabricate_ticket(&mut self, state: TicketState, value: u64, daa: u64) -> usize {
        let e = UtxoEntry::new(value, self.t.ticket_spk(&state), daa, false, Some(self.cov));
        let op = self.ledger.insert(e);
        self.tickets.push((op, state, value, daa));
        self.tickets.len() - 1
    }

    /// A pool fabricated directly as a UTXO under the instance covenant ID (D63), value
    /// `remaining` + state.reserve. Returns its index in `pools`.
    pub fn fabricate_pool(&mut self, state: PoolState, remaining: u64, daa: u64) -> usize {
        let e = UtxoEntry::new(remaining + state.reserve as u64, self.t.pool_spk(&state), daa, false, Some(self.cov));
        let op = self.ledger.insert(e);
        self.pools.push((op, state, remaining, daa));
        self.pools.len() - 1
    }

    /// The index in `pools` of the live pool of `round` and `tier`.
    pub fn pool_of(&self, round: i64, tier: u8) -> usize {
        self.pools
            .iter()
            .position(|p| p.1.round == round && p.1.tier == tier)
            .unwrap_or_else(|| panic!("no live pool of round {round} tier {tier}"))
    }

    fn fund_one(&mut self, funding_daa: u64) -> (FundingIn, TransactionOutpoint) {
        let fop = self.ledger.fund(10 * KAS, funding_spk(), funding_daa);
        (FundingIn { value: 10 * KAS, daa: funding_daa }, fop)
    }

    /// A claim of index `idx` of ticket `ti` against pool `pi`, with one funding input of
    /// 10 KAS created at `funding_daa`; and the outpoints in input order.
    pub fn claim_case(&mut self, ti: usize, pi: usize, idx: i64, funding_daa: u64) -> (ClaimCase, Vec<TransactionOutpoint>) {
        let (top, ticket, tv, tdaa) = self.tickets[ti].clone();
        let (pop, pool, rem, pdaa) = self.pools[pi].clone();
        let trees = self.drawn_trees.get(&pool.round).unwrap_or_else(|| panic!("no trees of round {}", pool.round)).clone();
        let mut c = ClaimCase::new(trees.len());
        c.b_max = self.t.b_max;
        c.ticket = ticket;
        c.ticket_value = tv;
        c.ticket_daa = tdaa;
        c.pool_value = rem + pool.reserve as u64;
        c.pool = pool;
        c.pool_daa = pdaa;
        c.trees = trees;
        c.idx = idx;
        c.cov = self.cov;
        let (fi, fop) = self.fund_one(funding_daa);
        c.funding = vec![fi];
        (c, vec![top, pop, fop])
    }

    pub fn claim_built(&self, c: &ClaimCase, ops: Vec<TransactionOutpoint>) -> Built {
        let mut b = c.build_with(self.t.clone());
        b.outpoints = ops;
        b
    }

    /// Run and apply a claim at `daa`: the pool moves to pool' (remaining - prize); the
    /// ticket moves to ticket' or, on FINAL, is removed.
    pub fn claim(&mut self, c: &ClaimCase, ops: Vec<TransactionOutpoint>, daa: u64) -> Final {
        let x = c.expected();
        assert!(x.rejects.is_empty(), "Chain::claim of a case the reference rejects: {:?}", x.rejects);
        let f = accept_m4("claim", &self.claim_built(c, ops.clone()));
        let outs = self.ledger.apply(&f.tx, &f.entries, daa).expect("claim applies");
        let pi = self.pools.iter().position(|p| p.0 == ops[1]).expect("claimed pool tracked");
        self.pools[pi] = (outs[0], x.pool.clone(), self.pools[pi].2 - x.prize as u64, daa);
        let ti = self.tickets.iter().position(|t| t.0 == ops[0]).expect("claiming ticket tracked");
        match &x.ticket {
            Some(s) => self.tickets[ti] = (outs[1], s.clone(), x.values[1], daa),
            None => {
                self.tickets.remove(ti);
            }
        }
        f
    }

    /// `return_deposit_after` of ticket `ti` at lock_time T + D_RET.
    pub fn return_after_case(&mut self, ti: usize, funding_daa: u64) -> (ReturnCase, Vec<TransactionOutpoint>) {
        let (top, ticket, tv, tdaa) = self.tickets[ti].clone();
        let mut c = ReturnCase::after_of(ticket, tv, tdaa);
        c.b_max = self.t.b_max;
        c.cov = self.cov;
        let (fi, fop) = self.fund_one(funding_daa);
        c.funding = vec![fi];
        (c, vec![top, fop])
    }

    /// `return_deposit_with_pool` of ticket `ti` with pool `pi` at lock_time = its deadline.
    pub fn return_with_pool_case(&mut self, ti: usize, pi: usize, funding_daa: u64) -> (ReturnCase, Vec<TransactionOutpoint>) {
        let (top, ticket, tv, tdaa) = self.tickets[ti].clone();
        let (pop, pool, rem, pdaa) = self.pools[pi].clone();
        let pv = rem + pool.reserve as u64;
        let mut c = ReturnCase::with_pool_of(ticket, tv, tdaa, pool, pv, pdaa);
        c.b_max = self.t.b_max;
        c.cov = self.cov;
        let (fi, fop) = self.fund_one(funding_daa);
        c.funding = vec![fi];
        (c, vec![top, pop, fop])
    }

    /// Run and apply a return at `daa`: the ticket is removed; a co-spent pool moves to its
    /// byte-identical successor.
    pub fn return_deposit(&mut self, c: &ReturnCase, ops: Vec<TransactionOutpoint>, daa: u64) -> Final {
        let mut b = c.build_with(self.t.clone());
        b.outpoints = ops.clone();
        let f = accept_m4("return_deposit", &b);
        let outs = self.ledger.apply(&f.tx, &f.entries, daa).expect("return applies");
        if let ReturnPath::WithPool { .. } = c.path {
            let pi = self.pools.iter().position(|p| p.0 == ops[1]).expect("co-spent pool tracked");
            self.pools[pi].0 = outs[0];
            self.pools[pi].3 = daa;
        }
        let ti = self.tickets.iter().position(|t| t.0 == ops[0]).expect("returned ticket tracked");
        self.tickets.remove(ti);
        f
    }

    /// `refresh()` of the current marker, at lock_time = its creation + D_IDLE.
    pub fn refresh_marker_case(&mut self, funding_daa: u64) -> (RefreshCase, Vec<TransactionOutpoint>) {
        let (mop, mk) = self.marker.clone().expect("a closed round's marker");
        let value = self.ledger.get(&mop).expect("marker UTXO").amount;
        let daa = self.marker_daa;
        self.refresh_case(RefreshObj::Marker(mk), mop, value, daa, funding_daa)
    }

    /// `refresh()` of pool `pi`.
    pub fn refresh_pool_case(&mut self, pi: usize, funding_daa: u64) -> (RefreshCase, Vec<TransactionOutpoint>) {
        let (op, s, rem, daa) = self.pools[pi].clone();
        let value = rem + s.reserve as u64;
        self.refresh_case(RefreshObj::Pool(s), op, value, daa, funding_daa)
    }

    /// `refresh()` of ticket `ti`.
    pub fn refresh_ticket_case(&mut self, ti: usize, funding_daa: u64) -> (RefreshCase, Vec<TransactionOutpoint>) {
        let (op, s, value, daa) = self.tickets[ti].clone();
        self.refresh_case(RefreshObj::Ticket(s), op, value, daa, funding_daa)
    }

    fn refresh_case(&mut self, obj: RefreshObj, op: TransactionOutpoint, value: u64, daa: u64, funding_daa: u64) -> (RefreshCase, Vec<TransactionOutpoint>) {
        let mut c = RefreshCase::new(obj, value, daa);
        c.b_max = self.t.b_max;
        c.cov = self.cov;
        let (fi, fop) = self.fund_one(funding_daa);
        c.funding = vec![fi];
        (c, vec![op, fop])
    }

    /// Run and apply a refresh at `daa`: the object moves to its byte-identical successor,
    /// created at `daa`.
    pub fn refresh(&mut self, c: &RefreshCase, ops: Vec<TransactionOutpoint>, daa: u64) -> Final {
        let mut b = c.build_with(self.t.clone());
        b.outpoints = ops.clone();
        let step = match c.obj {
            RefreshObj::Marker(_) => "refresh (marker)",
            RefreshObj::Pool(_) => "refresh (pool)",
            RefreshObj::Ticket(_) => "refresh (ticket)",
            _ => panic!("Chain::refresh is the M4 step; M5 objects use Chain::refresh5 (common::m5)"),
        };
        let f = accept_m4(step, &b);
        let outs = self.ledger.apply(&f.tx, &f.entries, daa).expect("refresh applies");
        match &c.obj {
            RefreshObj::Marker(mk) => {
                self.marker = Some((outs[0], mk.clone()));
                self.marker_daa = daa;
            }
            RefreshObj::Pool(_) => {
                let pi = self.pools.iter().position(|p| p.0 == ops[0]).expect("refreshed pool tracked");
                self.pools[pi].0 = outs[0];
                self.pools[pi].3 = daa;
            }
            RefreshObj::Ticket(_) => {
                let ti = self.tickets.iter().position(|t| t.0 == ops[0]).expect("refreshed ticket tracked");
                self.tickets[ti].0 = outs[0];
                self.tickets[ti].3 = daa;
            }
            _ => unreachable!(),
        }
        f
    }
}
