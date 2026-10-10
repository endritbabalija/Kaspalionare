//! M2 positive tests for `close()` (spec M2 A7: CP1-CP7, CP10).
//!
//! Every transaction runs every input in the v2.1.0 engine under committed budgets
//! (`common::accept`). CP10 (caller output = funding - R - fee + C_close, and value
//! conservation) is checked by `check_close` in every positive close.

mod common;

use common::m2::*;
use common::{accept, shard_error};
use harness::TxScriptError;
use lottery::buy::{Final, KAS};
use lottery::close::{CloseCase, FundingIn, TEST_D_MIN, TEST_DEADLINE, TEST_ROUND_LEN, z18};
use lottery::game::{Shares, round_amounts};
use lottery::params::SetParamsCase;
use lottery::state::{ControllerState, MarkerState, ParamsState, Payout, ShardState};

/// CP10 and conservation (A4 "Conservation"): every output value as derived, the caller
/// output = funding - R(marker) - fee + C_close, and inputs - outputs = fee.
fn check_close(c: &CloseCase, f: &Final) {
    let x = c.expected();
    let k = c.k();
    assert_eq!(f.tx.outputs.len(), k + 4, "A4: exactly k + 4 outputs");
    for (n, (o, v)) in f.tx.outputs.iter().zip(&x.values).enumerate() {
        assert_eq!(o.value, *v, "output {n} value");
    }
    let funding: u64 = c.funding.iter().map(|x| x.value).sum();
    let r = c.reserve();
    let caller = f.tx.outputs[c.caller_output()].value;
    assert_eq!(caller as i128, funding as i128 - r as i128 - c.fee as i128 + x.math.amounts.c_close as i128, "CP10 caller output");
    assert_eq!(f.fee(), c.fee, "conservation: inputs - outputs = fee");
    // Economic value (A4): controller (O + A) -> controller' (O - J_boost + A) + marker
    // (S - P_R - C_close + J_boost); reserves: K + 2 in, K + 2 out, plus the marker R from the caller.
    let o = c.pot as i128;
    let a = c.controller.acc as i128;
    let jb = c.controller.j_boost as i128;
    assert_eq!(x.values[0] as i128 - r as i128, o - jb + a);
    let (s, p) = (x.math.sales as i128, x.math.ref_paid as i128);
    assert_eq!(x.values[k + 2] as i128 - r as i128, s - p - x.math.amounts.c_close as i128 + jb);
}

/// Run a close case built on the committed templates; returns the finished transaction.
fn run_close(c: &CloseCase) -> Final {
    let f = accept(&c.build());
    check_close(c, &f);
    f
}

// ---------------------------------------------------------------- CP1

/// CP1: genesis (K = 4) -> buys in shards 0 and 2 -> close(). Every output field and value
/// is checked against A4 written out independently here (not through `expected()`), plus
/// conservation (CP10).
#[test]
fn cp1_genesis_buys_close() {
    let mut ch = Chain::genesis(4);
    let (mut b0, ops0) = ch.buy_case(0);
    b0.with_ref = true;
    ch.buy(0, &b0, ops0, 300_000);
    let (mut b2, ops2) = ch.buy_case(2);
    b2.bundle = common::distinct(3, 10);
    ch.buy(2, &b2, ops2, 400_000);

    let (c, ops) = ch.close_case(900_000);
    let ctrl = c.controller.clone();
    let params = c.params.clone();
    let old: Vec<ShardState> = ch.shards.iter().map(|s| s.1.clone()).collect();
    let f = ch.close(&c, ops, TEST_DEADLINE as u64 + 10);
    check_close(&c, &f);
    let t = tpl();
    let k = 4;

    // Sales and referral sums from the shards (A4 check 10).
    let sales: i64 = old.iter().map(|s| s.sales).sum();
    let paid: i64 = old.iter().map(|s| s.ref_paid).sum();
    assert_eq!(sales, (1 + 3) * KAS as i64);
    assert_eq!(paid, b0.referral() as i64);
    let am = round_amounts(sales, paid, &Shares::of(&ctrl));
    let own = 900_000i64; // the funding input is the newest input
    let t_next = (ctrl.deadline + ctrl.round_len).max(own + ctrl.d_min);
    assert_eq!(t_next, TEST_DEADLINE + TEST_ROUND_LEN);
    let m_next = c.expected().math.rule.choose_brute();

    // Output 0: controller r+1 (check 21).
    let c1 = ControllerState {
        round: 2,
        m: m_next,
        deadline: t_next,
        s_f: params.next_s_f,
        s_r: params.next_s_r,
        fee_class: params.next_fee_class,
        fee_key: params.next_fee_key,
        jackpot: 0,
        j_boost: 0,
        g2: 0,
        liability: 0,
        drawn: 0,
        ..ctrl.clone()
    };
    assert_eq!(f.tx.outputs[0].script_public_key, t.controller_spk(&c1));
    let r = ctrl.reserve as u64;
    assert_eq!(f.tx.outputs[0].value, c.pot + ctrl.acc as u64 + r - ctrl.j_boost as u64);
    // Output 1: params' (check 22).
    let p1 = ParamsState { round: 2, used: 0, ..params.clone() };
    assert_eq!(f.tx.outputs[1].script_public_key, t.params_spk(&p1));
    assert_eq!(f.tx.outputs[1].value, r);
    // Outputs 2..k+1: new shards (check 23).
    for i in 0..k {
        let s = ShardState {
            round: 2,
            shard_index: i as i64,
            m: m_next,
            price: ctrl.price,
            d: ctrl.d,
            s_r: params.next_s_r,
            deadline: t_next,
            root: z18(),
            sales: 0,
            ref_paid: 0,
        };
        assert_eq!(f.tx.outputs[2 + i].script_public_key, t.shard_spk(&s), "new shard {i}");
        assert_eq!(f.tx.outputs[2 + i].value, r);
    }
    // Output k+2: marker r (checks 16, 24).
    let roots: Vec<[u8; 32]> = old.iter().map(|s| s.root).collect();
    assert_ne!(roots[0], z18());
    assert_eq!(roots[1], z18());
    let mut mk = MarkerState {
        round: 1,
        k: 4,
        roots: MarkerState::roots_bytes(&roots),
        sales,
        ref_paid: paid,
        m: ctrl.m,
        jackpot: ctrl.jackpot,
        j_boost: ctrl.j_boost,
        g2: ctrl.g2,
        s_f: ctrl.s_f,
        s_r: ctrl.s_r,
        s_c: ctrl.s_c,
        s_d: ctrl.s_d,
        st: ctrl.st,
        seed_ref: own + ctrl.seed_offset,
        funder_class: 0,
        funder_key: [0; 33],
    };
    mk.set_funder(&c.funder);
    assert_eq!(f.tx.outputs[k + 2].script_public_key, t.marker_spk(&mk));
    assert_eq!(f.tx.outputs[k + 2].value as i64, sales - paid - am.c_close + ctrl.j_boost + r as i64);
    // Output k+3: caller (check 6, CP10).
    assert!(f.tx.outputs[k + 3].covenant.is_none());
    assert_eq!(f.tx.outputs[k + 3].value as i64, 100 * KAS as i64 - r as i64 - c.fee as i64 + am.c_close);
    // Covenant outputs 0..=k+2 are authorized by input 0 under the instance ID (check 5).
    for o in &f.tx.outputs[..k + 3] {
        assert_eq!(o.covenant, Some(harness::CovenantBinding::new(0, ch.cov)));
    }
}

// ---------------------------------------------------------------- CP2, CP3

/// CP2: zero-sales close: the marker holds J_boost + R.
#[test]
fn cp2_zero_sales_marker_holds_boost_and_reserve() {
    let c = CloseCase::new(4);
    let f = run_close(&c);
    assert_eq!(f.tx.outputs[c.marker_output()].value, c.controller.j_boost as u64 + c.reserve());
    assert_eq!(c.expected().math.amounts.c_close, 0);
}

/// CP3: close at tx.daa = T exactly (lock time = T), with F_MAX = 8 funding inputs.
#[test]
fn cp3_lock_time_at_deadline_and_f_max_funding() {
    let mut c = close_base(&tpl());
    c.lock_time = c.controller.deadline as u64;
    c.funding = vec![FundingIn { value: 15 * KAS, daa: 900_000 }; 8];
    let f = run_close(&c);
    assert_eq!(f.tx.lock_time, TEST_DEADLINE as u64);
    assert_eq!(f.tx.inputs.len(), 4 + 2 + 8);
}

// ---------------------------------------------------------------- CP4

/// CP4: set_params in round 1, then close: round 2's controller and shards carry the new
/// values, the round 1 marker the old ones; params' keeps the pending values with used reset.
#[test]
fn cp4_set_params_then_close() {
    let mut ch = Chain::genesis(4);
    let mut sp = SetParamsCase::new();
    let new_fee = Payout::Schnorr([0x55; 32]);
    sp.set_fee(&new_fee);
    ch.set_params(&mut sp, 300_000);
    assert_eq!(ch.params.1.used, 1);
    let (c, ops) = ch.close_case(900_000);
    let old = c.controller.clone();
    let f = ch.close(&c, ops, TEST_DEADLINE as u64 + 10);
    check_close(&c, &f);
    let x = c.expected();
    // Round 2 controller: new s_F, s_R and fee address.
    assert_eq!((x.controller.s_f, x.controller.s_r), (700, 1_200));
    assert_eq!((x.controller.fee_class, x.controller.fee_key), (new_fee.class(), new_fee.key33()));
    // Round 2 shards: new s_R.
    assert!(x.shards.iter().all(|s| s.s_r == 1_200));
    // Round 1 marker: the old shares.
    assert_eq!((x.marker.s_f, x.marker.s_r), (old.s_f, old.s_r));
    assert_eq!((x.marker.s_f, x.marker.s_r), (500, 1_000));
    // params': pending kept, used reset, round advanced, nonce kept.
    assert_eq!(x.params.used, 0);
    assert_eq!(x.params.round, 2);
    assert_eq!((x.params.next_s_f, x.params.next_s_r, x.params.nonce), (700, 1_200, 1));
    assert_eq!(f.tx.outputs[1].script_public_key, ch.t.params_spk(&x.params));
    assert_eq!(f.tx.outputs[0].script_public_key, ch.t.controller_spk(&x.controller));
}

// ---------------------------------------------------------------- CP5

/// CP5: two consecutive rounds, with a real draw() of round 1 between them (M3 Stage 3
/// replaced the D33 fixture draw).
#[test]
fn cp5_two_consecutive_rounds() {
    let mut ch = Chain::genesis(4);
    let (b, ops) = ch.buy_case(1);
    ch.buy(1, &b, ops, 300_000);
    let (c1, ops) = ch.close_case(900_000);
    let f1 = ch.close(&c1, ops, TEST_DEADLINE as u64 + 10);
    check_close(&c1, &f1);
    // draw() of round 1: the round 2 controller becomes drawn with J(2), J_boost(2), G2(2).
    ch.draw_round([5u8; 32]);
    assert_eq!(ch.controller.1.drawn, 1);
    let t2 = ch.controller.1.deadline;
    let (b, ops) = ch.buy_case(3);
    ch.buy(3, &b, ops, t2 as u64 - 1_000);
    let (mut c2, ops) = ch.close_case(t2 as u64 - 500);
    c2.lock_time = t2 as u64;
    assert_eq!(c2.controller.round, 2);
    let f2 = ch.close(&c2, ops, t2 as u64 + 10);
    check_close(&c2, &f2);
    let x = c2.expected();
    assert_eq!(x.controller.round, 3);
    assert_eq!(x.marker.round, 2);
    assert_eq!(x.params.round, 3);
    assert_eq!(x.math.sales, KAS as i64);
}

// ---------------------------------------------------------------- CP6, CP7

/// CP6: honest late close: own DAA >> T, so T' = own + D_MIN, and a buy on a new shard
/// created at own is accepted.
#[test]
fn cp6_honest_late_close_opens_a_selling_round() {
    let mut ch = Chain::genesis(4);
    let own = TEST_DEADLINE as u64 + 2_000_000;
    let (c, ops) = ch.close_case(own);
    let x = c.expected();
    assert_eq!(x.math.deadline, own as i64 + TEST_D_MIN);
    assert!(x.math.deadline > TEST_DEADLINE + TEST_ROUND_LEN);
    let f = ch.close(&c, ops, own);
    check_close(&c, &f);
    let (b, ops) = ch.buy_case(0);
    assert_eq!(b.shard_daa, own);
    ch.buy(0, &b, ops, own + 10);
}

/// CP7: griefing late close (D24): the caller uses only old inputs, so T' = T + round_len;
/// the close is included after T'. Its new shards reject buys (buy check 6); values are
/// conserved; the next close works.
#[test]
fn cp7_griefing_late_close() {
    let mut ch = Chain::genesis(4);
    let (c, ops) = ch.close_case(900_000);
    let t_next = c.expected().math.deadline;
    assert_eq!(t_next, TEST_DEADLINE + TEST_ROUND_LEN);
    let late = t_next as u64 + 50_000;
    let f = ch.close(&c, ops, late);
    check_close(&c, &f);
    // A buy on a new shard (created at `late` >= T') is rejected by the shard's deadline check.
    let (b, ops) = ch.buy_case(2);
    assert!(b.shard_daa >= b.shard.deadline as u64);
    let e = shard_error(&ch.buy_built(&b, ops));
    assert!(matches!(e, TxScriptError::VerifyError), "{e:?}");
    // The next close works (after the real draw() of round 1).
    ch.draw_round([7u8; 32]);
    let (mut c2, ops) = ch.close_case(late + 100);
    c2.lock_time = late + 100;
    let f2 = ch.close(&c2, ops, late + 200);
    check_close(&c2, &f2);
    assert_eq!(c2.expected().controller.round, 3);
}

// ---------------------------------------------------------------- CP10 (extra shapes)

/// CP10 over more shapes: sales in every shard with referrals, a large boost being
/// released, and a non-zero caller output after C_close.
#[test]
fn cp10_caller_output_in_varied_closes() {
    let t = tpl();
    let mut c = close_base(&t);
    for i in 0..4 {
        c.set_sales(i, (1_000 + 77 * i as i64) * KAS as i64, (50 + i as i64) * KAS as i64);
    }
    c.controller.j_boost = 90_000 * KAS as i64;
    let f = run_close(&c);
    assert!(c.expected().math.amounts.c_close > 0);
    assert!(f.tx.outputs[c.caller_output()].value > 0);
    // Two funding inputs and a P2SH marker funder.
    let mut c = close_base(&t);
    c.funding = vec![FundingIn { value: 3 * KAS, daa: 1_000 }, FundingIn { value: 40 * KAS, daa: 950_000 }];
    c.funder = Payout::P2sh([0x77; 32]);
    run_close(&c);
}
