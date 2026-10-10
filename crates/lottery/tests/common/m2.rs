//! Shared helpers for the M2 tests (spec M2 A7).
//!
//! Every attack changes one thing relative to a valid case and passes only on a *script*
//! error from the named input, never on a budget error.

use std::sync::Arc;

use harness::{Hash, TransactionOutpoint, TxScriptError};
use lottery::buy::InKind;
use lottery::buy::{BuyCase, Built, Final, KAS, Sources, Templates, b_max, funding_spk};
use lottery::close::{CloseCase, FundingIn, ShardIn};
use lottery::draw::{DrawCase, PoolIn};
use lottery::genesis::{GenesisCase, covenant_id};
use lottery::ledger::Ledger;
use lottery::params::SetParamsCase;
use lottery::smt::Tree;
use lottery::state::{ControllerState, DrawerState, MarkerState, ParamsState, PoolState, ShardState, VaultState};

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
        // The marker was created by the close that created the current controller.
        c.marker_daa = self.controller.2;
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
        let f = accept_draw(&self.draw_built(c, ops));
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
        self.marker = None;
        self.marker_trees = vec![];
        f
    }

    /// Draw the closed round with `seed` and no absorption: one fresh funding input created
    /// at seed_ref + DRAW_WINDOW + 1,000, applied at lock_time + 10 (as DP16).
    pub fn draw_round(&mut self, seed: [u8; 32]) -> Final {
        let seed_ref = self.marker.as_ref().expect("a closed round to draw").1.seed_ref;
        let (c, ops) = self.draw_case(seed, &[], (seed_ref + lottery::game::DRAW_WINDOW + 1_000) as u64);
        self.draw(&c, ops, c.lock_time + 10)
    }
}
