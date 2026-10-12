//! M2 mutation tests (spec M2 A7 "Mutants", negative controls). Each test proves that a
//! named `require` is the check that stops an attack: with the committed sources the named
//! input rejects the attack (and accepts the base); with only the lines tagged
//! `// mut:<ID>` removed from one covenant, the named input accepts it. Other inputs may
//! still reject (layered checks); the mutant claim is about the named input only.
//!
//! The tags are placed by Stage 3; the list is in docs/specs/2-handoff.md. Template
//! bindings live inside `readInputStateWithTemplate` / `validateOutputStateWithTemplate`
//! and the shard's constructor constant, so they cannot be removed as a line: their
//! negative controls swap the route table (or the shard's constant) to a look-alike instead.

mod common;

use std::sync::Arc;

use common::accept;
use common::m2::*;
use harness::{ArtifactValue, CovenantBinding, UtxoEntry};
use lottery::buy::{Built, In, InKind, Templates};
use lottery::close::CloseCase;
use lottery::params::SetParamsCase;

/// Input `input` rejects `mutate(base)` with the committed templates, and accepts it with
/// the lines tagged `ids` removed from `which`.
fn killed_by(which: Sil, ids: &[&str], input: usize, base: impl Fn(&Templates) -> CloseCase, mutate: impl Fn(&mut Built, &CloseCase)) {
    let t = tpl();
    let c = base(&t);
    attack_at(&c.build_with(t.clone()), input, |b| mutate(b, &c));
    let mt = mutant(which, ids);
    let mc = base(&mt);
    let mut mb = mc.build_with(mt.clone());
    accept(&mb);
    mutate(&mut mb, &mc);
    survives(&mb, input, ids);
}

/// As `killed_by` for a case-level attack.
fn case_killed_by(which: Sil, ids: &[&str], input: usize, base: impl Fn(&Templates) -> CloseCase, bad: impl Fn(&mut CloseCase)) {
    let t = tpl();
    let good = base(&t);
    let mut b = good.clone();
    bad(&mut b);
    attack_pair(&good.build_with(t.clone()), &b.build_with(t.clone()), input);
    let mt = mutant(which, ids);
    let good = base(&mt);
    accept(&good.build_with(mt.clone()));
    let mut b = good.clone();
    bad(&mut b);
    survives(&b.build_with(mt.clone()), input, ids);
}

fn survives(b: &Built, input: usize, ids: &[&str]) {
    let f = b.finish();
    if let Err(e) = f.run_input(input) {
        panic!("mutant without {ids:?}: input {input} still rejects the attack: {e:?}");
    }
}

/// set_params: input 0 rejects `bad` with the committed params and accepts it without `ids`.
fn sp_killed_by(ids: &[&str], bad: impl Fn(&mut SetParamsCase)) {
    let good = SetParamsCase::new();
    let mut b = good.clone();
    bad(&mut b);
    attack_pair(&good.build(), &b.build(), 0);
    let mt = mutant(Sil::Params, ids);
    accept(&good.build_with(mt.clone()));
    survives(&b.build_with(mt), 0, ids);
}

fn base(t: &Templates) -> CloseCase {
    close_base(t)
}

/// Shard `i`'s input state replaced by its values with `role` overridden.
fn shard_role(b: &mut Built, c: &CloseCase, i: usize, role: u8) {
    let mut v = c.shards[i].state.values();
    v.insert("role".into(), ArtifactValue::Byte(role));
    let redeem = b.templates.shard.redeem(&v);
    b.set_entry_redeem(CloseCase::shard_input(i), redeem);
}

// ---------------------------------------------------------------- controller (close)

#[test]
fn c2_share_room() {
    // Check 2 (share room, review finding R2): st = (4000, 3000, 600, 0, 0) gives
    // sum(st) + s_C + s_D + S_F_CAP + S_R_CAP = 10,200 > 10^4 while s_H = 800 >= 0, so no other
    // check rejects this close.
    case_killed_by(Sil::Controller, &["C2_share_room"], 0, base, |c| c.controller.st = [4_000, 3_000, 600, 0, 0]);
}

#[test]
fn c4_cov_input_count() {
    // Check 4: a shard-template UTXO without the instance ID in a shard slot (its state and
    // value are right, so check 10 passes; only OpCovInputCount = k + 2 sees it).
    killed_by(Sil::Controller, &["C4"], 0, base, |b, _| {
        let e = b.inputs[3].entry.clone();
        b.inputs[3].entry = UtxoEntry::new(e.amount, e.script_public_key.clone(), e.block_daa_score, false, None);
    });
}

#[test]
fn c5_auth_outputs() {
    // Check 5: the marker authorized by the params input (a parallel auth group).
    killed_by(Sil::Controller, &["C5"], 0, base, |b, c| {
        b.outputs[c.marker_output()].covenant = Some(CovenantBinding::new(1, c.cov));
    });
}

#[test]
fn c7_lock_time() {
    case_killed_by(Sil::Controller, &["C7"], 0, base, |c| c.lock_time = c.controller.deadline as u64 - 1);
}

#[test]
fn c8_drawn() {
    case_killed_by(Sil::Controller, &["C8"], 0, base, |c| c.controller.drawn = 0);
}

#[test]
fn c10_shard_role() {
    killed_by(Sil::Controller, &["C10_role"], 0, base, |b, c| shard_role(b, c, 1, 0x05));
}

#[test]
fn c10_shard_round() {
    case_killed_by(Sil::Controller, &["C10_round"], 0, base, |c| c.shards[1].state.round = 2);
}

#[test]
fn c10_shard_index() {
    case_killed_by(Sil::Controller, &["C10_index"], 0, base, |c| c.shards.swap(0, 1));
}

#[test]
fn c13_controller_value() {
    killed_by(Sil::Controller, &["C13"], 0, base, |b, _| b.shift_value(0, 1));
}

#[test]
fn c14_params_value() {
    killed_by(Sil::Controller, &["C14_out"], 0, base, |b, _| b.shift_value(1, 1));
}

#[test]
fn c15_new_shard_value() {
    killed_by(Sil::Controller, &["C15"], 0, base, |b, _| b.shift_value(3, 1));
}

#[test]
fn c16_marker_value() {
    killed_by(Sil::Controller, &["C16"], 0, base, |b, c| b.shift_value(c.marker_output(), 1));
}

#[test]
fn c19_m_next_closed_form() {
    // Check 19: m_next = the D25 closed form; m' + 1 carried consistently into the outputs.
    case_killed_by(Sil::Controller, &["C19"], 0, base, |c| c.m_next = Some(c.expected().m_next + 1));
}

// ---------------------------------------------------------------- template negative controls

/// Templates where `which` is a look-alike; a case built from them carries the look-alike
/// hash in its route table and is accepted by the controller; the same transaction with the
/// real hash in the route table is rejected (checks 9, 10, 24).
fn route_control(which: Sil) {
    let fake = lookalike(which);
    let good = base(&fake);
    let gb = good.build_with(fake.clone());
    accept(&gb);
    let real = tpl();
    let mut bad = good.clone();
    match which {
        Sil::Shard => bad.controller.shard_tpl = real.shard.hash,
        Sil::Params => bad.controller.params_tpl = real.params.hash,
        Sil::Marker => bad.controller.marker_tpl = real.marker.hash,
        Sil::Controller | Sil::Vault | Sil::Pool | Sil::Drawer | Sil::Ticket | Sil::Record | Sil::Position | Sil::Request => unreachable!(),
    }
    let mut bb = bad.build_with(fake.clone());
    // Keep the outputs of the good transaction: only the spent controller's route differs.
    bb.outputs = gb.outputs.clone();
    reject_at(&bb, 0);
}

#[test]
fn c10_template_control_route_table() {
    route_control(Sil::Shard);
}

#[test]
fn c9_template_control_route_table() {
    route_control(Sil::Params);
}

#[test]
fn c24_template_control_route_table() {
    route_control(Sil::Marker);
}

#[test]
fn l4_template_control_shard_constant() {
    // L4 (template): shards compiled with a look-alike controller's hash as their constant
    // accept being led by it; the real shards reject it (CA13).
    let fake: Arc<Templates> = lookalike(Sil::Controller);
    let c = base(&fake);
    let mut b = c.build_with(fake.clone());
    let e = b.inputs[0].entry.clone();
    b.inputs[0] = In {
        entry: e,
        kind: InKind::Entry { template: fake.controller.clone(), entry: "backdoor".into(), args: vec![], redeem: fake.controller_redeem(&c.controller) },
    };
    survives(&b, CloseCase::shard_input(0), &["(look-alike constant)"]);
}

// ---------------------------------------------------------------- shard close_leg

#[test]
fn l3_shard_leg_authorizes_no_output() {
    killed_by(Sil::Shard, &["L3"], 3, base, |b, c| b.outputs[3].covenant = Some(CovenantBinding::new(3, c.cov)));
}

#[test]
fn l4_shard_leg_leader_role() {
    // L4 (role): a controller-template UTXO whose role byte is not 0x01 at input 0.
    killed_by(Sil::Shard, &["L4_role"], CloseCase::shard_input(0), base, |b, c| {
        let mut v = c.controller.values();
        v.insert("role".into(), ArtifactValue::Byte(0x07));
        let redeem = b.templates.controller.redeem(&v);
        b.set_entry_redeem(0, redeem);
    });
}

#[test]
fn l4_shard_leg_leader_round() {
    // L4 (round): the leader is of round 2, the shards of round 1.
    case_killed_by(Sil::Shard, &["L4_round"], CloseCase::shard_input(0), base, |c| {
        c.controller.round = 2;
        c.params.round = 2;
        for s in &mut c.shards[1..] {
            s.state.round = 2;
        }
    });
}

#[test]
fn l5_shard_leg_cov_input_count() {
    // L5: a fifth shard of the family in a close whose controller says k = 4.
    case_killed_by(Sil::Shard, &["L5"], CloseCase::shard_input(4), base, |c| {
        let mut extra = c.shards[3].clone();
        extra.state.shard_index = 4;
        c.shards.push(extra);
    });
}

// ---------------------------------------------------------------- set_params

#[test]
fn p2_used_flag() {
    sp_killed_by(&["P2"], |c| c.params.used = 1);
}

#[test]
fn p4_nonce_increases() {
    sp_killed_by(&["P4"], |c| {
        c.params.nonce = 7;
        c.nonce = 7;
    });
}

#[test]
fn p5_admin_key_hash() {
    // A valid signature by another key, with that key passed.
    sp_killed_by(&["P5"], |c| {
        c.admin_sk = [0x0b; 32];
        c.signer_sk = [0x0b; 32];
    });
}

#[test]
fn p6_admin_signature() {
    // Another key signs; the admin's key is passed.
    sp_killed_by(&["P6"], |c| c.signer_sk = [0x0b; 32]);
}
