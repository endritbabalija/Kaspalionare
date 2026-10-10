//! M4 attacks on `claim()`, `return_deposit()` and `refresh()`: KA1-KA16 (spec M4 A9; KA17
//! is in m3_attacks.rs, it attacks the drawer).
//!
//! As M3 A8: full transactions in the v2.1.0 engine with production flags and committed
//! budgets; one change per attack; `strike(good, bad, i)`: the named input rejects the bad
//! transaction with a script error and passes the good one; every strike then accepts the
//! good (base) transaction with every input. The attacks are data in `common::m4` (`Atk`),
//! so stage 4's m4_mutants.rs re-runs each one under the mutant without its `require`.
//!
//! Ledger cases (a double spend of the claimed ticket, a claim on an absorbed pool's
//! outpoint) are structural: the UTXO set (`Ledger`, D33) refuses the spend, as consensus
//! does; no script runs in them. Range attacks use fabricated UTXOs (D63).
//!
//! Diagnostics (see `common::m4`): `M4_PREFLIGHT=1` runs every attack while the M4 entries
//! do not exist yet (stage 2b gate); `M4_ATTRIB=1` prints the check that rejects each
//! strike once stage 3 has placed the `// ==== check` markers.

mod common;

use common::m4::*;
use harness::TxScriptError;
use lottery::claim::*;
use lottery::ledger::LedgerError;
use lottery::state::TIER_LOWER0;

fn run(atks: Vec<Atk>) {
    assert!(!atks.is_empty());
    for a in atks {
        a.strike();
    }
}

// ---------------------------------------------------------------- KA1

/// KA1 (check 4): the same index claimed again on the ticket successor.
#[test]
fn ka1_double_claim_same_index() {
    run(ka1());
}

/// KA1 (ledger, structural): after a claim spends the ticket, a second transaction
/// spending the original ticket outpoint is refused by the UTXO set (input 0 missing).
/// The first claim is applied without running scripts (its script acceptance is KP1-KP17).
#[test]
fn ka1_double_claim_ledger() {
    let mut ch = chain_round1(|w| vec![combo_for_tier(w, TIER_LOWER0, 10, 0), combo_matching(w, 0, false, 10, 1)]);
    let pi = ch.pool_of(1, TIER_LOWER0);
    let (c, ops) = ch.claim_case(0, pi, 0, 1_999_000);
    assert!(c.expected().rejects.is_empty(), "{:?}", c.expected().rejects);
    let f = ch.claim_built(&c, ops.clone()).finish();
    let outs = ch.ledger.apply(&f.tx, &f.entries, 2_000_000).expect("the first claim applies");
    assert!(!ch.ledger.contains(&ops[0]), "the ticket is spent");
    assert!(ch.ledger.contains(&outs[1]), "ticket' exists");
    // The same claim again from the original ticket (fresh funding).
    let (c2, ops2) = ch.claim_case(0, pi, 0, 1_999_500);
    assert_eq!(ops2[0], ops[0]);
    let f2 = ch.claim_built(&c2, ops2.clone()).finish();
    assert_eq!(ch.ledger.entries(&f2.tx), Err(LedgerError::Missing { index: 0, outpoint: ops[0] }));
    assert!(ch.ledger.apply(&f2.tx, &f2.entries, 2_000_100).is_err());
}

// ---------------------------------------------------------------- KA2 - KA12

/// KA2 (check 7): forged or under-reported h.
#[test]
fn ka2_forged_h() {
    run(ka2());
}

/// KA3 (check 6): wrong tier.
#[test]
fn ka3_wrong_tier() {
    run(ka3());
}

/// KA4 (3; PT2, 2): another round's ticket; a look-alike ticket under a fresh ID.
#[test]
fn ka4_other_round_and_family() {
    run(ka4());
}

/// KA5 (13, 12, 11 and 10): redirected, off-by-one, misplaced payouts; an extra output.
#[test]
fn ka5_payout() {
    run(ka5());
}

/// KA6 (12, 14): pool' value and state.
#[test]
fn ka6_pool_successor() {
    run(ka6());
}

/// KA7 (3): the same tier's pool of the next round.
#[test]
fn ka7_next_round_pool() {
    run(ka7());
}

/// KA7 (ledger, structural): a claim prepared on a round 1 pool, which round 2's `draw()`
/// then absorbs; the claim spends the absorbed outpoint and is refused by the UTXO set
/// (input 1 missing). The genesis, buys, closes and draws run the real scripts.
#[test]
fn ka7_absorbed_pool_ledger() {
    let mut ch = chain_round1(|w| vec![combo_for_tier(w, TIER_LOWER0, 10, 0), combo_matching(w, 0, false, 10, 1)]);
    let pi = ch.pool_of(1, TIER_LOWER0);
    let (c, ops) = ch.claim_case(0, pi, 0, 1_999_000);
    assert!(c.expected().rejects.is_empty(), "{:?}", c.expected().rejects);
    // Round 2: a buy, close at T2, draw absorbing the pool at its deadline (as KP12).
    let t2 = ch.controller.1.deadline as u64;
    let (b, bops) = ch.buy_case(3);
    ch.buy(3, &b, bops, t2 - 1_000);
    let (mut c2, cops) = ch.close_case(t2 - 500);
    c2.lock_time = t2;
    ch.close(&c2, cops, t2 + 10);
    let deadline = ch.pools[pi].1.deadline as u64;
    let (d, dops) = ch.draw_case([2u8; 32], &[pi], deadline);
    ch.draw(&d, dops, deadline + 10);
    assert!(!ch.ledger.contains(&ops[1]), "the pool was absorbed");
    let f = ch.claim_built(&c, ops.clone()).finish();
    assert_eq!(ch.ledger.entries(&f.tx), Err(LedgerError::Missing { index: 1, outpoint: ops[1] }));
}

/// KA8 (10, 11, 12): mode cheating.
#[test]
fn ka8_mode_cheating() {
    run(ka8());
}

/// KA9 (12, 15): ticket' tampering.
#[test]
fn ka9_ticket_successor() {
    run(ka9());
}

/// KA10 (2, 1, 10; PT1, PT3): shape.
#[test]
fn ka10_shape() {
    run(ka10());
}

/// KA11 (PT4): the pool leg's view of input 0.
#[test]
fn ka11_pool_leg_input0() {
    run(ka11());
}

/// KA12 (1, 3, 5, 8): fabricated ranges (D63).
#[test]
fn ka12_ranges() {
    run(ka12());
}

// ---------------------------------------------------------------- KA13 - KA15

/// KA13 (A5b 3, 2, 6, 5, 4): `return_deposit_after`.
#[test]
fn ka13_return_after() {
    run(ka13());
}

/// KA14 (A5a 4, 3, 7, 8): `return_deposit_with_pool`.
#[test]
fn ka14_return_with_pool() {
    run(ka14());
}

/// KA15 (A6 3, 7, 6, 2, 4, 5): `refresh()` of the ticket, a pool and a marker.
#[test]
fn ka15_refresh() {
    run(ka15());
}

/// Stage 4 (S4-3): input values and sums above TOTAL_MAX = 2^62. Consensus caps values at
/// MAX_SOMPI < 2^62, so these are reachable only in the local engine; they show the value
/// ranges (checks 1, 3, 12, A5b.1, A5a.1, A5a.3) execute.
#[test]
fn engine_only_value_bounds() {
    run(engine_only());
}

/// Every attack id is unique (m4_mutants.rs looks attacks up by id).
#[test]
fn attack_ids_are_unique() {
    let mut ids: Vec<String> = all_atks().into_iter().map(|a| a.id).collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate attack ids");
}

// ---------------------------------------------------------------- KA16

/// KA16: the leader's committed budget one unit short (claim in each mode, both returns,
/// each refresh): input 0 fails on the budget, after the base is accepted.
#[test]
fn ka16_leader_budget_minus_one() {
    for (name, case) in ka16_cases() {
        let b = case.build();
        done4(name, case.step(), &b);
        let mut m = b.clone();
        m.budget_delta = -1;
        let e = m.finish().run_input(0).expect_err("under-budget leader accepted");
        if preflight() && matches!(e, TxScriptError::EarlyReturn) {
            println!("PREFLIGHT ka16 | KA16 {name} | expect budget | bad: {e:?} (no entry yet)");
            continue;
        }
        assert!(matches!(e, TxScriptError::ExceededCommittedScriptUnits { .. }), "KA16 {name}: expected a budget error, got {e:?}");
    }
}

// ---------------------------------------------------------------- attribution instrument

/// Self-test of the `M4_ATTRIB` instrument (not an attack test): a synthetic ticket with the
/// committed state layout and a minimal `refresh()` carrying `// ==== check A6.N` markers
/// (one check inside a braced block). The instrument must name A6.2, A6.3 and A6.5 for
/// three refresh attacks, and the good refresh must pass the synthetic source with the
/// swapped template and its rebuilt output 0. This proves the truncation, compile, template
/// swap, successor rebuild and search before stage 3 relies on them.
#[test]
fn attribution_instrument_self_test() {
    let committed = lottery::buy::Sources::committed(lottery::buy::b_max()).ticket;
    let head_end = committed.find("byte[33] payout_key = init_payout_key;").expect("ticket state layout");
    let head = &committed[..head_end + "byte[33] payout_key = init_payout_key;".len()];
    let src = format!(
        "{head}\n\n    entry refresh() {{\n        // ==== check A6.1: leader\n        require(this.activeInputIndex == 0);\n        // ==== check A6.2: inputs\n        require(tx.inputs.length == 2);\n        // ==== check A6.3: idle\n        {{\n            int creation = OpTxInputDaaScore(0);\n            require(creation >= 0);\n        }}\n        // ==== check A6.5: outputs\n        require(tx.outputs.length == 2);\n    }}\n}}\n"
    );
    let good: Case = refresh_ticket().into();
    let gb = good.build();
    assert!(passes_under_source(&gb, &good, "ticket:A6.5", 0, Some(&src)), "the good refresh passes the synthetic ticket");
    let atks = ka15();
    let pick = |id: &str| atks.iter().find(|a| a.id == id).unwrap_or_else(|| panic!("no attack {id}"));
    for (id, want) in [
        ("KA15 ticket: a second covenant input", "ticket:A6.2"),
        ("KA15 ticket: creation = -1 (unaccepted parent)", "ticket:A6.3"),
        ("KA15 ticket: extra plain output", "ticket:A6.5"),
    ] {
        let (_, bad, bc) = pick(id).pair_case(&lottery::buy::Templates::get(lottery::buy::b_max()));
        let got = first_failing_check_in(&bad, &bc, want, 0, Some(&src));
        assert_eq!(got.as_deref(), Some(want), "{id}");
    }
}
