//! M7 attacks SA1-SA10 (spec M7 A8 "Attacks"): the anchor proof, the caller-supplied
//! reference, the seed, the void mode, re-rolls, shape and delegates on the anchor fixture,
//! and `close()`'s marker output without `seed_ref`.
//!
//! As M5: full transactions in the v2.1.0 engine with production flags and committed budgets,
//! each run against a `harness::ChainSim` (`execute_on_chain`); one change per attack; the
//! named input rejects the bad transaction with a script error and passes the good one; the
//! good transaction is then accepted by every input. The modelled attacks are data in
//! `common::m7` (`Atk5`, struck by `run5`), so `m7_mutants.rs` re-runs them under the mutants,
//! `M5_PREFLIGHT=1` checks each against the spec model (`common::m5_spec`, which names the
//! failing check) and `M5_ATTRIB=1` names the script's first failing check (the M3
//! truncation instrument on drawer.sil). SA3 (only the chain's point of view or the accessor
//! changes), the leg attacks of SA9, SA8's marker refresh and SA10 (`close()`) are direct
//! strikes (`common::m2::strike`, `M3_ATTRIB` for the drawer).
//!
//! M7 Stage 1b (tests first, scripts untouched): every draw and void strike stops at "BASE
//! REJECTED ... EarlyReturn" (the committed drawer has only `draw_test_seed`, the builder
//! calls `draw`), and SA10 at the close base (controller check 24 still builds the marker
//! with `seed_ref`). `sa_model_rejects_each_attack_at_its_check`, `attack_ids_are_unique` and
//! `attacks_are_well_formed` run without the scripts and pass.

mod common;

use std::sync::Arc;

use common::m2::*;
use common::m5::{Case5, accept5, preflight5, run5};
use common::m5_spec::{Tx, check_input};
use common::m7::*;
use harness::{ArtifactValue, CovenantBinding, TxScriptError, UtxoEntry, pay_to_script_hash_script};
use lottery::buy::{Built, In, InKind, Template};
use lottery::draw::*;
use lottery::state::MarkerState;

const VLT: usize = DrawCase::VAULT;
const CTL: usize = DrawCase::CONTROLLER;
const POOL0: usize = 4;

// ---------------------------------------------------------------- SA1, SA2, SA4-SA9 (modelled)

#[test]
fn sa1_neighbouring_chain_block() {
    run5(sa1());
}

#[test]
fn sa2_non_chain_block() {
    run5(sa2());
}

#[test]
fn sa4_wrong_preimage() {
    run5(sa4());
}

#[test]
fn sa5_caller_supplied_reference() {
    run5(sa5());
}

#[test]
fn sa6_seed() {
    run5(sa6());
}

#[test]
fn sa7_void_misuse() {
    run5(sa7());
}

#[test]
fn sa8_re_roll_after_draw_or_void() {
    run5(sa8());
}

#[test]
fn sa9_shape_modelled() {
    run5(sa9());
}

// ---------------------------------------------------------------- SA3

/// The finished `good` transaction run against another chain (`sim`; `None`: no accessor):
/// the same transaction, the same committed budgets, only the point of view differs. Input 0
/// must reject it with an error `want` accepts and pass `good`, and `good` is accepted by
/// every input. Under `M5_PREFLIGHT` the model must reject it at drawer:8 and accept `good`.
fn chain_strike(id: &str, c: &DrawCase, good: &Built, sim: Option<Arc<harness::ChainSim>>, want: impl Fn(&TxScriptError) -> bool) {
    let gf = good.finish();
    let bf = lottery::buy::Final { tx: gf.tx.clone(), entries: gf.entries.clone(), units: gf.units.clone(), sim };
    let real_bad = bf.run_input(DRW);
    if preflight5() {
        let mb = check_input(&Tx::new(good, &bf), DRW, Some(c));
        let mg = check_input(&Tx::new(good, &gf), DRW, Some(c));
        println!("PREFLIGHT sa3 | {id} | model bad {mb:?} good {mg:?} | script bad {:?}", real_bad.as_ref().err());
        assert_eq!((mb, mg), (Some(Err("drawer:8".to_string())), Some(Ok(()))), "PREFLIGHT MISMATCH ({id})");
    }
    let e = match real_bad {
        Ok(_) => panic!("ATTACK ACCEPTED ({id}) by input 0"),
        Err(e) => e,
    };
    if let Err(g) = gf.run_input(DRW) {
        panic!("BASE REJECTED at target input 0 ({id}): {g:?}");
    }
    assert!(want(&e), "{id}: unexpected rejection {e:?}");
    accept5(id, &Case5::Draw(c.clone()), good);
}

/// SA3 (check 8, the opcode): the same draw seen from a point of view at X.blue_score + F
/// (BlockIsTooDeep: the accessor admits X iff X.bs + F > pov, rusty-kaspa v2.1.0
/// `consensus/src/model/services/seq_commit_accessor.rs:8-10, 36-44`), and through
/// `harness::execute` with no accessor (InvalidOpcode, `crypto/txscript/src/opcodes/mod.rs:
/// 1389-1393`). On the attack line and on a minimal anchored chain. The last admissible
/// point of view (X.bs + F - 1) is SP2.
#[test]
fn sa3_outside_the_window_and_opcode_disabled() {
    for c in [sa_base(), noncanonical_base().0] {
        let good = c.build();
        let x = anchor_x(&c);
        let mut deep = (*c.chain()).clone();
        deep.set_pov(x.blue_score + deep.finality_depth());
        chain_strike("SA3 point of view at X.bs + F: BlockIsTooDeep (M7A_xc)", &c, &good, Some(Arc::new(deep)), |e| matches!(e, TxScriptError::BlockIsTooDeep(_)));
        chain_strike("SA3 the same draw without an accessor: InvalidOpcode (M7A_xc)", &c, &good, None, |e| matches!(e, TxScriptError::InvalidOpcode(_)));
    }
}

// ---------------------------------------------------------------- SA8 (marker refresh)

/// SA8 / SP10 (spec M7 D99): the marker has no `refresh()`. In a valid draw, input 1 (the
/// marker) calling `refresh()` instead of `draw_leg` is rejected at dispatch (EarlyReturn),
/// while `draw_leg` passes.
#[test]
fn sa8_marker_refresh_fails_at_dispatch() {
    let c = sa_base();
    let good = c.build();
    let mut bad = good.clone();
    match &mut bad.inputs[MK].kind {
        InKind::Entry { entry, args, .. } => {
            // The full signature: refresh() is no longer in the ABI, so it is encoded by hand
            // (`planned_sig_script`) as the pre-M7 dispatch would have read it.
            *entry = "refresh()".into();
            args.clear();
        }
        _ => unreachable!(),
    }
    strike(&good, &bad, MK);
    let e = bad.finish().run_input(MK).unwrap_err();
    assert!(matches!(e, TxScriptError::EarlyReturn), "marker refresh(): expected a dispatch failure (no such entry), got {e:?} (marker.sil still has refresh())");
    accept_draw(&good);
}

/// SA8 (structural, as VA4): after a void the round's marker is spent and the round r + 1
/// controller has drawn = 1, so neither a second void nor a draw can spend them again (the
/// ledger refuses the spent marker; a fabricated drawn = 1 controller is `sa8_re_roll_*`).
#[test]
fn sa8_void_spends_the_marker() {
    let mut ch = Chain::genesis(4);
    let t = ch.controller.1.deadline as u64;
    let (mut cc, ops) = ch.close_case(t - 500);
    cc.lock_time = t;
    ch.close(&cc, ops, t + 10);
    let mop = ch.marker.clone().unwrap().0;
    let d = ch.anchor_d();
    let (c, ops) = ch.void_case(&[], (d + lottery::game::DRAW_WINDOW + 1_000) as u64);
    let again = ops.clone();
    ch.draw(&c, ops, c.lock_time + 10);
    assert!(!ch.ledger.contains(&mop), "the marker is spent by the void");
    assert_eq!(ch.controller.1.drawn, 1, "the round r + 1 controller is drawn");
    let mut b = c.build_with(ch.t.clone());
    b.outpoints = again;
    let f = b.finish();
    assert!(matches!(ch.ledger.entries(&f.tx), Err(lottery::ledger::LedgerError::Missing { .. })), "a second void of the same objects is refused by the UTXO set");
}

// ---------------------------------------------------------------- SA9 (legs, budget, values)

/// SA9 (as M3 DA8-DA11, DA15, DA16) on the anchor line: delegates under a look-alike leader,
/// legs at the wrong index, a leg authorizing an output, a leader of another covenant,
/// look-alike pool and controller templates, input values +- 1, the leader one budget unit
/// short. (The modelled shape, DA13 and DA14 attacks are `sa9_shape_modelled`.)
#[test]
fn sa9_legs_templates_values_budget() {
    let c = sa_base();
    let b0 = c.build();
    let hit = |input: usize, f: &dyn Fn(&mut Built)| {
        let mut m = b0.clone();
        f(&mut m);
        strike(&b0, &m, input);
    };
    let cov = c.cov;
    // DA8: a pool output authorized by the marker: marker L3.
    hit(MK, &|b| b.outputs[3].covenant = Some(CovenantBinding::new(MK as u16, cov)));
    // DA11: a look-alike drawer at input 0 (same state, permissive entry); the legs get its
    // template lengths. Marker, vault and pool L4 reject.
    let fake = lookalike(Sil::Drawer);
    let ds = c.drawer.clone();
    let fleg = DrawCase::drawer_leg_args(&fake);
    let fake_leader = |b: &mut Built| {
        let e = b.inputs[0].entry.clone();
        b.inputs[0] = In {
            entry: UtxoEntry::new(e.amount, fake.drawer_spk(&ds), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fake.drawer.clone(), entry: "backdoor".into(), args: vec![], redeem: fake.drawer_redeem(&ds) },
        };
        for i in [MK, VLT, POOL0] {
            *b.entry_args_mut(i) = fleg.clone();
        }
    };
    for i in [MK, VLT, POOL0] {
        hit(i, &fake_leader);
    }
    // Controller L4: a look-alike marker at input 1.
    let fm = lookalike(Sil::Marker);
    let ms = c.marker.clone();
    hit(CTL, &|b| {
        let e = b.inputs[MK].entry.clone();
        b.inputs[MK] = In {
            entry: UtxoEntry::new(e.amount, fm.marker_spk(&ms), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fm.marker.clone(), entry: "backdoor".into(), args: vec![], redeem: fm.marker_redeem(&ms) },
        };
        *b.entry_args_mut(CTL) = DrawCase::controller_leg_args(&fm);
    });
    // L1: legs at the wrong index.
    hit(2, &|b| b.inputs.swap(MK, VLT));
    hit(1, &|b| b.inputs.swap(MK, VLT));
    hit(3, &|b| b.inputs.swap(CTL, POOL0));
    hit(4, &|b| b.inputs.swap(CTL, POOL0));
    // L3: a leg authorizing an output.
    hit(MK, &|b| b.outputs[3].covenant = Some(CovenantBinding::new(MK as u16, cov)));
    hit(VLT, &|b| b.outputs[1].covenant = Some(CovenantBinding::new(VLT as u16, cov)));
    hit(CTL, &|b| b.outputs[2].covenant = Some(CovenantBinding::new(CTL as u16, cov)));
    hit(POOL0, &|b| b.outputs[4].covenant = Some(CovenantBinding::new(POOL0 as u16, cov)));
    // L2: the real drawer at input 0 under another covenant ID.
    let other_leader = |b: &mut Built| {
        let e = b.inputs[0].entry.clone();
        b.inputs[0].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, Some(OTHER_COV));
        for o in b.outputs.iter_mut().filter(|o| o.covenant.is_some()) {
            o.covenant = Some(CovenantBinding::new(0, OTHER_COV));
        }
    };
    for i in [MK, VLT, CTL, POOL0] {
        hit(i, &other_leader);
    }
    // DA10: a look-alike pool at input 4 (check 18), a new pool under the look-alike pool
    // template (check 25), a look-alike controller at input 3 (check 3).
    let fp = lookalike(Sil::Pool);
    let ps = c.absorbed[0].state.clone();
    hit(DRW, &|b| {
        let e = b.inputs[POOL0].entry.clone();
        b.inputs[POOL0] = In {
            entry: UtxoEntry::new(e.amount, fp.pool_spk(&ps), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fp.pool.clone(), entry: "backdoor".into(), args: vec![], redeem: fp.pool_redeem(&ps) },
        };
    });
    let np0 = c.expected().new_pools[0].clone();
    let spk = fp.pool_spk(&np0);
    hit(DRW, &|b| b.outputs[3].script_public_key = spk.clone());
    let fc = lookalike(Sil::Controller);
    let cs = c.controller.clone();
    hit(DRW, &|b| {
        let e = b.inputs[CTL].entry.clone();
        b.inputs[CTL] = In {
            entry: UtxoEntry::new(e.amount, fc.controller_spk(&cs), e.block_daa_score, false, e.covenant_id),
            kind: InKind::Entry { template: fc.controller.clone(), entry: "backdoor".into(), args: vec![], redeem: fc.controller_redeem(&cs) },
        };
    });
    // DA16: input values +- 1 (the vault's are modelled, `sa9_shape_modelled`).
    for i in [DRW, MK, CTL, POOL0] {
        hit(DRW, &|b| b.inputs[i].entry.amount += 1);
        hit(DRW, &|b| b.inputs[i].entry.amount -= 1);
    }
    accept_draw(&b0);
    // DA15: the leader's committed budget one unit short.
    let mut b = b0.clone();
    b.budget_delta = -1;
    let e = b.finish().run_input(DRW).expect_err("under-budget drawer accepted");
    assert!(matches!(e, TxScriptError::ExceededCommittedScriptUnits { .. }), "expected a budget error, got {e:?}");
    // The same for a void.
    let v = void_base().build();
    accept_draw(&v);
    let mut b = v.clone();
    b.budget_delta = -1;
    let e = b.finish().run_input(DRW).expect_err("under-budget void accepted");
    assert!(matches!(e, TxScriptError::ExceededCommittedScriptUnits { .. }), "expected a budget error, got {e:?}");
}

// ---------------------------------------------------------------- SA10 (close)

/// The pre-M7 marker template (with `seed_ref`, spec M3 A3.4 / M6), compiled from the test
/// fixture `tests/fixtures/marker_pre_m7.sil` (a copy of `covenants/marker.sil` at commit
/// ab35582) against the committed drawer hash; its constructor has `init_seed_ref` before
/// funder_class.
fn old_marker(drawer_hash: [u8; 32], m: &MarkerState) -> Template {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/marker_pre_m7.sil")).expect("fixture");
    assert!(src.contains("int init_seed_ref"), "the fixture is the pre-M7 marker");
    let mut ctor = m.ctor(drawer_hash);
    let at = ctor.len() - 2;
    ctor.insert(at, ArtifactValue::Int(0));
    Template::from(harness::compile_source(&src, &ctor))
}

/// SA10 (close check 24): the marker output of a valid `close()` (controller input 0)
/// carrying an extra field (the old `seed_ref` = own + seed_offset, 9 bytes before
/// funder_class) under the committed template, or built from the old template with the old
/// layout.
#[test]
fn sa10_close_marker_output() {
    let t = tpl();
    let c = close_base(&t);
    let x = c.expected();
    let b0 = c.build();
    let mo = c.marker_output();
    let seed_ref = x.math.own + c.controller.seed_offset;
    // An extra field under the committed template.
    let redeem = t.marker_redeem(&x.marker);
    let state = &redeem[t.marker.prefix.len()..redeem.len() - t.marker.suffix.len()];
    let n = state.len();
    assert_eq!((state[n - 36], state[n - 34]), (0x01, 0x21), "marker state ends with funder_class (1-byte push) and funder_key (33-byte push)");
    let mut extra = state[..n - 36].to_vec();
    extra.push(0x08);
    extra.extend_from_slice(&seed_ref.to_le_bytes());
    extra.extend_from_slice(&state[n - 36..]);
    let spk = pay_to_script_hash_script(&[t.marker.prefix.as_slice(), &extra, &t.marker.suffix].concat());
    attack_at(&b0, 0, |b| b.outputs[mo].script_public_key = spk);
    // The old template with the old layout.
    let old = old_marker(t.drawer.hash, &x.marker);
    assert_ne!(old.hash, t.marker.hash, "the committed marker no longer has seed_ref (Stage 2)");
    let mut v = x.marker.values();
    v.insert("seed_ref".into(), ArtifactValue::Int(seed_ref));
    let spk = pay_to_script_hash_script(&old.redeem(&v));
    attack_at(&b0, 0, |b| b.outputs[mo].script_public_key = spk);
}

// ---------------------------------------------------------------- structure (no scripts)

/// Every modelled attack is rejected by the spec model (`common::m5_spec`, the `M5_PREFLIGHT`
/// oracle) at exactly its named check, and its good transaction passes the model. Runs
/// without the scripts (the model reads the built transaction and the `ChainSim`).
#[test]
fn sa_model_rejects_each_attack_at_its_check() {
    let mut bad = vec![];
    for a in all_atks7() {
        let (g, b, bc) = a.pair_case(&tpl());
        let (gf, bf) = (g.finish(), b.finish());
        let mut good = a.good.clone();
        good.retarget(&tpl());
        let mb = check_input(&Tx::new(&b, &bf), a.input, bc.draw_case());
        let mg = check_input(&Tx::new(&g, &gf), a.input, good.draw_case());
        if mb != Some(Err(a.check.to_string())) || mg != Some(Ok(())) {
            bad.push(format!("{} (expect {}): model bad {mb:?}, good {mg:?}", a.id, a.check));
        }
    }
    assert!(bad.is_empty(), "model verdicts differ from the labels:\n{}", bad.join("\n"));
}

#[test]
fn attack_ids_are_unique() {
    let mut ids: Vec<String> = all_atks7().into_iter().map(|a| a.id).collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate attack ids");
}

/// Every attack changes the transaction and names a drawer label.
#[test]
fn attacks_are_well_formed() {
    for a in all_atks7() {
        assert!(a.check.starts_with("drawer:") && a.input == DRW, "{}: {} at input {}", a.id, a.check, a.input);
        let (g, b) = a.pair(&tpl());
        let (gf, bf) = (g.finish(), b.finish());
        let sigs = |f: &lottery::buy::Final| f.tx.inputs.iter().map(|i| i.signature_script.clone()).collect::<Vec<_>>();
        assert!(gf.tx.id() != bf.tx.id() || gf.entries != bf.entries || sigs(&gf) != sigs(&bf), "{}: the attack changed nothing", a.id);
    }
}
