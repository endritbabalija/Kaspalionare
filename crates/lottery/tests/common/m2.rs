//! Shared helpers for the M2 tests (spec M2 A7).
//!
//! Every attack changes one thing relative to a valid case and passes only on a *script*
//! error from the named input, never on a budget error.

use std::sync::Arc;

use harness::{Hash, TransactionOutpoint, TxScriptError};
use lottery::buy::{BuyCase, Built, Final, KAS, Sources, Templates, b_max, funding_spk};
use lottery::close::{CloseCase, FundingIn, ShardIn};
use lottery::genesis::{GenesisCase, covenant_id};
use lottery::ledger::Ledger;
use lottery::params::SetParamsCase;
use lottery::state::{ControllerState, MarkerState, ParamsState, ShardState};

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
    }
}

fn hash_of(t: &Templates, which: Sil) -> [u8; 32] {
    match which {
        Sil::Controller => t.controller.hash,
        Sil::Params => t.params.hash,
        Sil::Shard => t.shard.hash,
        Sil::Marker => t.marker.hash,
    }
}

/// Templates compiled with the tagged lines of one covenant removed. Compile order is
/// ticket -> controller -> shard -> params -> marker, so a controller mutant also changes
/// the shard and params hashes: build cases with `CloseCase::new_with(k, &t)` and
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
        let op = ledger.fund(10_000 * KAS, funding_spk(), 100);
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
        }
    }

    /// A buy case on shard `i` funded from the ledger (100 KAS at DAA 0, as `BuyCase`
    /// builds its funding entries), with the outpoints to set on the `Built`.
    pub fn buy_case(&mut self, i: usize) -> (BuyCase, Vec<TransactionOutpoint>) {
        let (op, s, daa) = self.shards[i].clone();
        let mut c = BuyCase::new(self.t.b_max).with_tree(lottery::smt::Tree::new(s.m as usize));
        assert_eq!(c.shard.root, s.root, "buy_case expects an empty shard");
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

    /// D33 fixture "draw" of the current round: a drawn controller (drawn = 1, J fields set,
    /// L = J + G2 (m - 1)) is fabricated as a new UTXO with the same value; the undrawn one
    /// stays unused. `j_boost` must not exceed the pot.
    pub fn fabricate_draw(&mut self, jackpot: i64, j_boost: i64, g2: i64) {
        assert!(j_boost as u64 <= self.pot);
        let (_, mut c, daa) = self.controller.clone();
        c.drawn = 1;
        c.jackpot = jackpot;
        c.j_boost = j_boost;
        c.g2 = g2;
        c.liability = jackpot + g2 * (c.m - 1);
        let value = self.pot + c.acc as u64 + c.reserve as u64;
        let op = self.ledger.insert(harness::UtxoEntry::new(value, self.t.controller_spk(&c), daa + 1, false, Some(self.cov)));
        self.controller = (op, c, daa + 1);
    }
}
