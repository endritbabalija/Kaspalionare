//! M4 mutation tests (spec M4 A9 "Mutants", negative controls as M2/M3). For every
//! `// mut:<ID>` tag in ticket.sil (claim checks 1-15, return_deposit_after A5b,
//! return_deposit_with_pool A5a, refresh A6), in pool.sil `ticket_leg` (PT*) and `refresh`
//! (RF*), and in marker.sil `refresh` (RF*):
//! - `killed`: with the committed sources the named input rejects the attack (the strike of
//!   m4_attacks.rs, shared through `common::m4`); with only that `require` removed (compiled
//!   in memory, the sources on disk are untouched), the named input ACCEPTS it, and the
//!   valid base transaction is still accepted by every input.
//! - `redundant`: the mutant still rejects the attack at the named input, because another
//!   check (or a compiler-inserted loop bound, or an engine error) implies the removed one.
//!   The argument is in each test's comment and in docs/DECISIONS.md ("Mutant results" row, M4). If a
//!   later change removes the implying check, the test fails and the gap is noticed.
//!
//! Granularity is the line: a tag that marks several `require` lines (a range's two halves)
//! is tested line by line (`"file:TAG#n"` = the n-th line carrying TAG, 0-based, in file
//! order). `"file:TAG"` removes every line of the tag. Groups whose members are each
//! redundant because of the others are also removed together and killed (as M3 D17).
//!
//! Template bindings (inside `readInputStateWithTemplate` / `validateOutputState*`, not
//! removable as a line) get look-alike negative controls (M2 test-method row): the same
//! transaction is accepted when the trusted hash is the look-alike's and rejected under the
//! committed one.
//!
//! `every_m4_tag_is_tested` checks that every tag line is named here (or listed as
//! equivalent with its proof).

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::m2::{Sil, accept_m4, lookalike, reject_at, tpl};
use common::m4::*;
use harness::{TxScriptError, pay_to_script_hash_script};
use lottery::buy::{Built, InKind, Sources, Template, Templates, b_max};
use lottery::claim::*;

// ---------------------------------------------------------------- mutant mechanism

/// "file:TAG" or "file:TAG#n".
fn parse(t: &str) -> (Sil, &str, Option<usize>) {
    let (file, rest) = t.split_once(':').expect("file:TAG");
    let sil = match file {
        "ticket" => Sil::Ticket,
        "pool" => Sil::Pool,
        "marker" => Sil::Marker,
        f => panic!("no M4 file {f}"),
    };
    match rest.split_once('#') {
        Some((tag, n)) => (sil, tag, Some(n.parse().expect("line index"))),
        None => (sil, rest, None),
    }
}

fn line_tags(line: &str) -> Vec<&str> {
    match line.find("// mut:") {
        Some(p) => line[p + 7..].split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).collect(),
        None => vec![],
    }
}

/// `src` without the `nth` line carrying `tag` (every such line if `None`).
fn strip(src: &str, tag: &str, nth: Option<usize>) -> String {
    let mut k = 0;
    let mut removed = 0;
    let out: Vec<&str> = src
        .lines()
        .map(|l| {
            if line_tags(l).contains(&tag) {
                let this = k;
                k += 1;
                if nth.is_none_or(|n| n == this) {
                    removed += 1;
                    return "";
                }
            }
            l
        })
        .collect();
    assert!(removed > 0 && nth.is_none_or(|_| removed == 1), "tag {tag} line {nth:?} not found");
    out.join("\n")
}

fn field(s: &mut Sources, which: Sil) -> &mut String {
    match which {
        Sil::Ticket => &mut s.ticket,
        Sil::Pool => &mut s.pool,
        Sil::Marker => &mut s.marker,
        _ => unreachable!(),
    }
}

fn hash(t: &Templates, which: Sil) -> [u8; 32] {
    match which {
        Sil::Ticket => t.ticket.hash,
        Sil::Pool => t.pool.hash,
        Sil::Marker => t.marker.hash,
        _ => unreachable!(),
    }
}

/// Templates compiled with the given tag lines removed (one or more files).
fn mutant4(tags: &[&str]) -> Arc<Templates> {
    let mut src = Sources::committed(b_max());
    let mut files = vec![];
    for t in tags {
        let (sil, tag, n) = parse(t);
        let s = field(&mut src, sil);
        *s = strip(s, tag, n);
        files.push(sil);
    }
    let t = Templates::compile_sources(b_max(), &src);
    for sil in files {
        assert_ne!(hash(&t, sil), hash(&tpl(), sil), "mutant {tags:?} compiled to the committed template");
    }
    Arc::new(t)
}

/// The attack is rejected with the committed templates (strike), the base is accepted
/// under the mutant, and the mutant's named input accepts the attack.
fn killed(tags: &[&str], id: &str) {
    let a = atk(id);
    a.strike();
    let t = mutant4(tags);
    let (g, b) = a.pair(&t);
    accept_m4(&format!("base under mutant {tags:?}"), &g);
    if let Err(e) = b.finish().run_input(a.input) {
        panic!("mutant {tags:?}: input {} still rejects `{id}`: {e:?}", a.input);
    }
}

/// Redundant line: the mutant still rejects the attack at the named input (not on the
/// budget), and still accepts the base.
fn redundant(tags: &[&str], id: &str) {
    let a = atk(id);
    a.strike();
    let t = mutant4(tags);
    let (g, b) = a.pair(&t);
    accept_m4(&format!("base under mutant {tags:?}"), &g);
    match b.finish().run_input(a.input) {
        Ok(_) => panic!("mutant {tags:?} accepts `{id}` at input {}: the line is not redundant", a.input),
        Err(e) => {
            assert!(!matches!(e, TxScriptError::ExceededCommittedScriptUnits { .. }), "budget error: {e:?}");
            println!("redundant {tags:?} `{id}`: still rejected at input {} by {e:?}", a.input);
        }
    }
}

macro_rules! killed {
    ($name:ident, [$($tag:expr),+], $id:expr) => {
        #[test]
        fn $name() {
            killed(&[$($tag),+], $id);
        }
    };
}

macro_rules! redundant {
    ($name:ident, [$($tag:expr),+], $id:expr) => {
        #[test]
        fn $name() {
            redundant(&[$($tag),+], $id);
        }
    };
}

/// Equivalent lines with no transaction that tells them apart, and why (the proof is also
/// run as a `redundant` test with the closest attack where one exists).
const EQUIVALENT: &[(&str, &str)] = &[
    (
        "ticket:K1_value#0",
        "v0 = OpTxInputAmount(0) >= 0 always: the opcode converts the u64 amount to i64 and fails with NumberTooBig above i64::MAX (rusty-kaspa v2.1.0 crypto/txscript/src/opcodes/mod.rs:1100-1107); run: k1_value_0_redundant",
    ),
    ("ticket:RB1_value#0", "as K1_value#0; run: rb1_value_0_redundant"),
    ("ticket:RA1_value#0", "as K1_value#0; run: ra1_value_0_redundant"),
];

// ================================================================ ticket claim: check 1

killed!(k1_active, ["ticket:K1_active"], "KA10 a covenant-less copy of the ticket as funding (at the copy, input 2)");
killed!(k1_role, ["ticket:K1_role"], "KA12 ticket role 0x07 (FINAL)");
killed!(k1_round_0, ["ticket:K1_round#0"], "KA12 ticket and pool round 0");
killed!(k1_round_1, ["ticket:K1_round#1"], "KA12 ticket and pool round TOTAL_MAX");
killed!(k1_deadline_0, ["ticket:K1_deadline#0"], "KA12 ticket deadline -1");
killed!(k1_deadline_1, ["ticket:K1_deadline#1"], "KA12 ticket deadline DAA_MAX");
// b >= 1 follows from 0 <= idx < b (K1_idx).
redundant!(k1_b_0_redundant, ["ticket:K1_b#0"], "KA12 ticket b = 0");
// b <= B_MAX: check 9's `for(fq, 0, b, B_MAX)` carries the compiler's require(b - 0 <= B_MAX)
// (silverscript-lang/src/compiler/for.rs:135-155).
redundant!(k1_b_1_redundant, ["ticket:K1_b#1"], "KA12 ticket b = B_MAX + 1");
killed!(k1_class, ["ticket:K1_class"], "KA12 ticket payout_class 3, paid to its P2SH form");
redundant!(k1_value_0_redundant, ["ticket:K1_value#0"], "ENGINE claim: ticket value 2^63 (OpTxInputAmount fails)");
// Consensus-equivalent (values <= MAX_SOMPI < 2^62), killed in the engine (S4-3).
killed!(k1_value_1, ["ticket:K1_value#1"], "ENGINE claim: ticket value TOTAL_MAX + 1");
// Template lengths: the template hash commits to len(prefix) || prefix || len(suffix) ||
// suffix (silverscript-lang/src/compiler/compile/state.rs:140-167), so only the true lengths
// read the pool; out-of-range lengths fail the script-sig substring (OpSubstr-family index
// errors). Defense in depth.
redundant!(k1_tpl_0_redundant, ["ticket:K1_tpl#0"], "KA12 pool_pl = -1");
redundant!(k1_tpl_1_redundant, ["ticket:K1_tpl#1"], "KA12 pool_pl = SCRIPT_MAX + 1");
redundant!(k1_tpl_2_redundant, ["ticket:K1_tpl#2"], "KA12 pool_sl = -1");
redundant!(k1_tpl_3_redundant, ["ticket:K1_tpl#3"], "KA12 pool_sl = SCRIPT_MAX + 1");
// idx >= 0: idx = -1 gives co = -6 and bundle.slice(-6, -5) fails (OpSubstr rejects a
// negative index, rusty-kaspa v2.1.0 crypto/txscript/src/opcodes/mod.rs:502-507).
redundant!(k1_idx_0_redundant, ["ticket:K1_idx#0"], "KA12 idx = -1");
killed!(k1_idx_1, ["ticket:K1_idx#1"], "KA12 idx = b with a winning combination in the bundle padding");

// ================================================================ ticket claim: check 2

// OpCovInputCount(cov) = 2 and OpInputCovenantId(1) = cov each follow from the other plus
// K2_zero (inputs >= 2 carry the zero ID, input 0 carries cov): pigeonhole. If input 0
// carries no covenant (cov = zero hash), no output is authorized by it (covenants.rs:124-131)
// and K10 rejects. Removed together, a pool of another covenant ID is accepted.
redundant!(k2_count_redundant, ["ticket:K2_count"], "KA10 two tickets, one pool, one payout: at ticket A");
redundant!(k2_count_no_covenant_redundant, ["ticket:K2_count"], "KA10 covenant-less ticket and pool (plain outputs)");
redundant!(k2_pool_redundant, ["ticket:K2_pool"], "KA10 the pool under another covenant ID (pool' joins the ticket's)");
killed!(k2_count_and_pool, ["ticket:K2_count", "ticket:K2_pool"], "KA10 the pool under another covenant ID (pool' joins the ticket's)");
killed!(k2_fund_0, ["ticket:K2_fund#0"], "KA10 no funding input (fee 0)");
// <= 2 + F_MAX: the K2_zero loop `for(fi, 2, n, F_MAX)` bounds n - 2 <= F_MAX (for.rs:135-155).
redundant!(k2_fund_1_redundant, ["ticket:K2_fund#1"], "KA10 nine funding inputs");
killed!(k2_zero, ["ticket:K2_zero"], "KA10 funding input with a covenant ID");

// ================================================================ ticket claim: check 3

killed!(k3_role, ["ticket:K3_role"], "KA12 pool role 0x07 (pool' follows)");
killed!(k3_tier, ["ticket:K3_tier"], "KA12 non-winning combination against a tier 7 pool");
killed!(k3_slice_0, ["ticket:K3_slice#0"], "KA12 pool slice -1");
killed!(k3_slice_1, ["ticket:K3_slice#1"], "KA12 pool slice TOTAL_MAX + 2 (h = 2)");
killed!(k3_m_0, ["ticket:K3_m#0"], "KA12 pool m = 1 (trees of m = 1)");
killed!(k3_m_1, ["ticket:K3_m#1"], "KA12 pool m = M_CLAIM + 1 (trees of m = 51)");
// k >= 1: with k = 0 no proof runs, h = 0 and K8_h rejects.
redundant!(k3_k_0_redundant, ["ticket:K3_k#0"], "KA12 pool k = 0 with no proofs");
// k <= K_MAX: the check 7 loop `for(i, 0, pl.k, K_MAX)` bounds it (for.rs:135-155).
redundant!(k3_k_1_redundant, ["ticket:K3_k#1"], "KA12 pool k = K_MAX + 1");
killed!(k3_deadline_0, ["ticket:K3_deadline#0"], "KA12 pool deadline -1");
killed!(k3_deadline_1, ["ticket:K3_deadline#1"], "KA12 pool deadline DAA_MAX");
killed!(k3_reserve_0, ["ticket:K3_reserve#0"], "KA12 pool reserve -1");
killed!(k3_reserve_1, ["ticket:K3_reserve#1"], "KA12 pool reserve RESERVE_MAX + 1");
// reserve <= v1: K8 requires 0 <= prize <= v1 - reserve (prize = slice / h >= 0 since
// slice >= 0, h >= 1).
redundant!(k3_value_0_redundant, ["ticket:K3_value#0"], "KA12 pool reserve > value");
killed!(k3_value_1, ["ticket:K3_value#1"], "ENGINE claim: pool value TOTAL_MAX + 1");
killed!(k3_round, ["ticket:K3_round"], "KA7 the next round's pool of the same tier");

// ================================================================ ticket claim: checks 4-8

killed!(k4_claimed, ["ticket:K4_claimed"], "KA1 index already claimed (PAY)");
killed!(k5_canon_0, ["ticket:K5_canon#0"], "KA12 n1 = 0 (the predecessor rank's leaf)");
killed!(k5_canon_1, ["ticket:K5_canon#1"], "KA12 n1 = n2 (a jackpot by a repeated number)");
killed!(k5_canon_2, ["ticket:K5_canon#2"], "KA12 n2 = n3 (a jackpot by a repeated number)");
killed!(k5_canon_3, ["ticket:K5_canon#3"], "KA12 n3 = n4 (a jackpot by a repeated number)");
killed!(k5_canon_4, ["ticket:K5_canon#4"], "KA12 n4 = n5 (a jackpot by a repeated number)");
killed!(k5_canon_5, ["ticket:K5_canon#5"], "KA12 n5 = 31");
// bonus >= 1 and bonus <= pl.m: the count slice counts.slice(2(bonus - 1), 2 bonus) leaves
// the 2m-byte leaf otherwise (OpSubstr index error, opcodes/mod.rs:502-507).
redundant!(k5_bonus_0_redundant, ["ticket:K5_bonus#0"], "KA12 bonus = 0 against the tier 3 pool");
redundant!(k5_bonus_1_redundant, ["ticket:K5_bonus#1"], "KA12 bonus = pool m + 1 against the tier 3 pool");
killed!(k6_tier, ["ticket:K6_tier"], "KA3 tier 2 combination against the tier 3 pool");
killed!(k7_len_0, ["ticket:K7_len#0"], "KA2 leaves + 2 bytes");
killed!(k7_len_1, ["ticket:K7_len#1"], "KA2 siblings + 1 byte");
killed!(k7_root, ["ticket:K7_root"], "KA2 stale leaf: shard 1 count - 1 at the bonus");
killed!(k7_count, ["ticket:K7_count"], "KA12 a count of 32,768 (sign bit) at the bonus");
// h >= 1: h = 0 makes slice / h fail (OpDiv by zero, opcodes/mod.rs:656-661).
redundant!(k8_h_redundant, ["ticket:K8_h"], "KA12 a combination no shard holds (h = 0)");
killed!(k8_remaining, ["ticket:K8_remaining"], "KA12 prize > remaining (value = reserve + prize - 1)");

// ================================================================ ticket claim: checks 10-13

// Check 10's three requires are each implied by the other two plus K11_zero (outputs >=
// nCov carry the zero ID): input 0's authorized outputs are continuation outputs under cov
// (covenants.rs:124-131), so they lie in 0..nCov-1; OpAuthOutputIdx fails past the count.
// Removed together, pool' and ticket' may leave the covenant (no binding).
redundant!(k10_auth_redundant, ["ticket:K10_auth"], "KA10 extra covenant output (a ticket' copy)");
redundant!(k10_idx_redundant, ["ticket:K10_idx"], "KA10 pool leg authorizing pool' (at the ticket)");
redundant!(k10_cov_redundant, ["ticket:K10_cov"], "KA10 extra covenant output (a ticket' copy)");
killed!(k10_all, ["ticket:K10_auth", "ticket:K10_idx", "ticket:K10_cov"], "KA10 pool' and ticket' without covenant binding");
// ch >= 0: a missing payout output makes check 12 read past the outputs (index error).
redundant!(k11_count_0_redundant, ["ticket:K11_count#0"], "KA10 PAY without its payout and change outputs");
killed!(k11_count_1, ["ticket:K11_count#1"], "KA5 PAY extra plain output");
killed!(k11_zero, ["ticket:K11_zero"], "KA10 the change under a fresh genesis covenant ID");
killed!(k12_pool, ["ticket:K12_pool"], "KA6 pool' value + 1");
killed!(k12_pay_ticket, ["ticket:K12_pay_ticket"], "KA9 PAY ticket' value - 1");
killed!(k12_pay_payout, ["ticket:K12_pay_payout"], "KA5 PAY payout + 1");
// Consensus-equivalent (value + prize > 2^62 > MAX_SOMPI), killed in the engine (S4-3).
killed!(k12_guard, ["ticket:K12_guard"], "ENGINE claim: ticket value TOTAL_MAX - 10, ACCRUE (value + prize > TOTAL_MAX)");
killed!(k12_sum_accrue, ["ticket:K12_sum"], "KA9 ACCRUE ticket' value - 1");
killed!(k12_sum_final, ["ticket:K12_sum"], "KA5 FINAL payout + 1");
killed!(k13_spk_pay, ["ticket:K13_spk"], "KA5 PAY payout to another script");
killed!(k13_spk_final, ["ticket:K13_spk"], "KA5 FINAL payout to another class (P2SH of the key)");

// ================================================================ ticket return_deposit_after (A5b)

killed!(rb1_active, ["ticket:RB1_active"], "KA13 a covenant-less copy of the ticket as funding (at the copy, input 1)");
killed!(rb1_role, ["ticket:RB1_role"], "KA13 ticket role 0x07");
killed!(rb1_round_0, ["ticket:RB1_round#0"], "KA13 ticket round 0");
killed!(rb1_round_1, ["ticket:RB1_round#1"], "KA13 ticket round TOTAL_MAX");
killed!(rb1_deadline_0, ["ticket:RB1_deadline#0"], "KA13 ticket deadline -1");
// deadline < DAA_MAX: A5b.3's tx.daa >= deadline + D_RET compiles to 0 <= x <
// LOCK_TIME_THRESHOLD = DAA_MAX before OpCheckLockTimeVerify
// (silverscript-lang/src/compiler/compile/statement.rs:375-388; rusty-kaspa v2.1.0
// crypto/txscript/src/lib.rs:84), so a deadline >= DAA_MAX can never pass A5b.3.
redundant!(rb1_deadline_1_redundant, ["ticket:RB1_deadline#1"], "KA13 ticket deadline DAA_MAX");
killed!(rb1_b_0, ["ticket:RB1_b#0"], "KA13 ticket b = 0");
killed!(rb1_b_1, ["ticket:RB1_b#1"], "KA13 ticket b = B_MAX + 1");
killed!(rb1_class, ["ticket:RB1_class"], "KA13 ticket payout_class 3, paid to its P2SH form");
redundant!(rb1_value_0_redundant, ["ticket:RB1_value#0"], "ENGINE return_deposit_after: ticket value 2^63 (OpTxInputAmount fails)");
killed!(rb1_value_1, ["ticket:RB1_value#1"], "ENGINE return_deposit_after: ticket value TOTAL_MAX + 1");
killed!(rb2_count, ["ticket:RB2_count"], "KA13 a covenant-less ticket");
killed!(rb2_fund_0, ["ticket:RB2_fund#0"], "KA13 no funding input (fee 0)");
redundant!(rb2_fund_1_redundant, ["ticket:RB2_fund#1"], "KA13 nine funding inputs");
killed!(rb2_zero, ["ticket:RB2_zero"], "KA13 funding input with a covenant ID");
killed!(rb3, ["ticket:RB3"], "KA13 lock_time = T + D_RET - 1");
// A5b.4's two requires imply each other (input 0 is the only input under cov, so the outputs
// it authorizes are exactly the outputs under cov, covenants.rs:124-131), and A5b.5's zero-ID
// loop implies both. All three removed, a payout under cov is accepted.
redundant!(rb4_auth_redundant, ["ticket:RB4_auth"], "KA13 payout under the covenant ID");
redundant!(rb4_cov_redundant, ["ticket:RB4_cov"], "KA13 payout under the covenant ID");
killed!(rb4_and_rb5_zero, ["ticket:RB4_auth", "ticket:RB4_cov", "ticket:RB5_zero"], "KA13 payout under the covenant ID");
// >= 1 output: A5b.6 reads output 0 (index error without it).
redundant!(rb5_count_0_redundant, ["ticket:RB5_count#0"], "KA13 no output");
// <= 2 outputs: the RB5_zero loop `for(zo, 0, n, 2)` bounds n <= 2 (for.rs:135-155).
redundant!(rb5_count_1_redundant, ["ticket:RB5_count#1"], "KA13 extra plain output");
killed!(rb5_zero, ["ticket:RB5_zero"], "KA13 the change under a fresh genesis covenant ID");
killed!(rb6_value, ["ticket:RB6_value"], "KA13 payout + 1");
killed!(rb6_spk, ["ticket:RB6_spk"], "KA13 payout to another script");

// ================================================================ ticket return_deposit_with_pool (A5a)

killed!(ra1_active, ["ticket:RA1_active"], "KA14 a covenant-less copy of the ticket as funding (at the copy, input 2)");
killed!(ra1_role, ["ticket:RA1_role"], "KA14 ticket role 0x07");
killed!(ra1_round_0, ["ticket:RA1_round#0"], "KA14 ticket and pool round 0");
killed!(ra1_round_1, ["ticket:RA1_round#1"], "KA14 ticket and pool round TOTAL_MAX");
killed!(ra1_deadline_0, ["ticket:RA1_deadline#0"], "KA14 ticket deadline -1");
killed!(ra1_deadline_1, ["ticket:RA1_deadline#1"], "KA14 ticket deadline DAA_MAX");
killed!(ra1_b_0, ["ticket:RA1_b#0"], "KA14 ticket b = 0");
killed!(ra1_b_1, ["ticket:RA1_b#1"], "KA14 ticket b = B_MAX + 1");
killed!(ra1_class, ["ticket:RA1_class"], "KA14 ticket payout_class 3, paid to its P2SH form");
redundant!(ra1_value_0_redundant, ["ticket:RA1_value#0"], "ENGINE return_deposit_with_pool: ticket value 2^63 (OpTxInputAmount fails)");
killed!(ra1_value_1, ["ticket:RA1_value#1"], "ENGINE return_deposit_with_pool: ticket value TOTAL_MAX + 1");
// As K1_tpl.
redundant!(ra1_tpl_0_redundant, ["ticket:RA1_tpl#0"], "KA14 pool_pl = -1");
redundant!(ra1_tpl_1_redundant, ["ticket:RA1_tpl#1"], "KA14 pool_pl = SCRIPT_MAX + 1");
redundant!(ra1_tpl_2_redundant, ["ticket:RA1_tpl#2"], "KA14 pool_sl = -1");
redundant!(ra1_tpl_3_redundant, ["ticket:RA1_tpl#3"], "KA14 pool_sl = SCRIPT_MAX + 1");
// As K2_count / K2_pool (pigeonhole with RA2_zero; a covenant-less ticket fails RA5).
redundant!(ra2_count_redundant, ["ticket:RA2_count"], "KA14 the pool under another covenant ID (pool' joins the ticket's)");
redundant!(ra2_pool_redundant, ["ticket:RA2_pool"], "KA14 the pool under another covenant ID (pool' joins the ticket's)");
killed!(ra2_count_and_pool, ["ticket:RA2_count", "ticket:RA2_pool"], "KA14 the pool under another covenant ID (pool' joins the ticket's)");
killed!(ra2_fund_0, ["ticket:RA2_fund#0"], "KA14 no funding input (fee 0)");
redundant!(ra2_fund_1_redundant, ["ticket:RA2_fund#1"], "KA14 nine funding inputs");
killed!(ra2_zero, ["ticket:RA2_zero"], "KA14 funding input with a covenant ID");
killed!(ra3_role, ["ticket:RA3_role"], "KA14 pool role 0x07 (pool' follows)");
killed!(ra3_tier, ["ticket:RA3_tier"], "KA14 pool tier 7");
killed!(ra3_slice_0, ["ticket:RA3_slice#0"], "KA14 pool slice -1");
killed!(ra3_slice_1, ["ticket:RA3_slice#1"], "KA14 pool slice TOTAL_MAX + 1");
killed!(ra3_m_0, ["ticket:RA3_m#0"], "KA14 pool m = 1");
killed!(ra3_m_1, ["ticket:RA3_m#1"], "KA14 pool m = M_CLAIM + 1");
killed!(ra3_k_0, ["ticket:RA3_k#0"], "KA14 pool k = 0");
killed!(ra3_k_1, ["ticket:RA3_k#1"], "KA14 pool k = K_MAX + 1");
// pool deadline in [0, DAA_MAX): A5a.4's tx.daa >= pl.deadline compiles to 0 <= x <
// LOCK_TIME_THRESHOLD = DAA_MAX before CLTV (statement.rs:375-388; lib.rs:84).
redundant!(ra3_deadline_0_redundant, ["ticket:RA3_deadline#0"], "KA14 pool deadline -1");
redundant!(ra3_deadline_1_redundant, ["ticket:RA3_deadline#1"], "KA14 pool deadline DAA_MAX");
killed!(ra3_reserve_0, ["ticket:RA3_reserve#0"], "KA14 pool reserve -1");
killed!(ra3_reserve_1, ["ticket:RA3_reserve#1"], "KA14 pool reserve RESERVE_MAX + 1");
killed!(ra3_value_0, ["ticket:RA3_value#0"], "KA14 pool reserve > value");
killed!(ra3_value_1, ["ticket:RA3_value#1"], "ENGINE return_deposit_with_pool: pool value TOTAL_MAX + 1");
killed!(ra3_round, ["ticket:RA3_round"], "KA14 a pool of round 2");
killed!(ra4, ["ticket:RA4"], "KA14 lock_time = pool deadline - 1");
// A5a.5's three requires are each implied by the other two plus RA6_zero (outputs >= 1
// carry the zero ID); OpAuthOutputIdx(0, 0) fails when input 0 authorizes nothing.
// Removed together, pool' may leave the covenant.
redundant!(ra5_auth_redundant, ["ticket:RA5_auth"], "KA14 pool' without covenant binding");
redundant!(ra5_idx_redundant, ["ticket:RA5_idx"], "KA14 pool' without covenant binding");
redundant!(ra5_cov_redundant, ["ticket:RA5_cov"], "KA14 pool' without covenant binding");
killed!(ra5_all, ["ticket:RA5_auth", "ticket:RA5_idx", "ticket:RA5_cov"], "KA14 pool' without covenant binding");
// >= 2 outputs: A5a.8 reads output 1. <= 3: the RA6_zero loop `for(zo, 1, n, 2)`.
redundant!(ra6_count_0_redundant, ["ticket:RA6_count#0"], "KA14 only pool' (no payout)");
redundant!(ra6_count_1_redundant, ["ticket:RA6_count#1"], "KA14 extra plain output");
killed!(ra6_zero, ["ticket:RA6_zero"], "KA14 the change under a fresh genesis covenant ID");
killed!(ra7_value, ["ticket:RA7_value"], "KA14 pool' value + 1");
killed!(ra8_value, ["ticket:RA8_value"], "KA14 payout - 1");
killed!(ra8_spk, ["ticket:RA8_spk"], "KA14 payout to another script");

// ================================================================ refresh (A6) of ticket, pool, marker

macro_rules! refresh_mutants {
    ($m:ident, $file:literal, $obj:literal) => {
        mod $m {
            use super::*;
            killed!(rf1, [concat!($file, ":RF1")], concat!("KA15 ", $obj, ": a covenant-less copy as funding (at the copy, input 1)"));
            // A6.2 count is implied by RF2_zero (pigeonhole) or, for a covenant-less object,
            // by A6.4 (nothing authorized). Removed with RF2_zero, a second covenant input
            // is accepted.
            redundant!(rf2_count_redundant, [concat!($file, ":RF2_count")], concat!("KA15 ", $obj, ": a second covenant input"));
            redundant!(rf2_count_no_covenant_redundant, [concat!($file, ":RF2_count")], concat!("KA15 ", $obj, ": a covenant-less object, plain successor"));
            killed!(rf2_count_and_zero, [concat!($file, ":RF2_count"), concat!($file, ":RF2_zero")], concat!("KA15 ", $obj, ": a second covenant input"));
            killed!(rf2_fund_0, [concat!($file, ":RF2_fund#0")], concat!("KA15 ", $obj, ": no funding input (fee 0)"));
            // <= 1 + F_MAX: the RF2_zero loop `for(fi, 1, n, F_MAX)`.
            redundant!(rf2_fund_1_redundant, [concat!($file, ":RF2_fund#1")], concat!("KA15 ", $obj, ": nine funding inputs"));
            killed!(rf2_zero, [concat!($file, ":RF2_zero")], concat!("KA15 ", $obj, ": funding input with a covenant ID"));
            killed!(rf3_creation_0, [concat!($file, ":RF3_creation#0")], concat!("KA15 ", $obj, ": creation = -1 (unaccepted parent)"));
            // creation < DAA_MAX: creation + D_IDLE >= LOCK_TIME_THRESHOLD fails the compiled
            // range before CLTV (statement.rs:375-388); a score >= 2^63 is pushed negative
            // (opcodes/mod.rs:1124-1130) and fails RF3_creation#0.
            redundant!(rf3_creation_1_redundant, [concat!($file, ":RF3_creation#1")], concat!("KA15 ", $obj, ": creation = DAA_MAX"));
            killed!(rf3_idle, [concat!($file, ":RF3_idle")], concat!("KA15 ", $obj, ": lock_time = creation + D_IDLE - 1"));
            // A6.4's three requires are each implied by the other two plus RF5_zero (output 1
            // carries the zero ID). Removed together, the successor may leave the covenant.
            redundant!(rf4_auth_redundant, [concat!($file, ":RF4_auth")], concat!("KA15 ", $obj, ": extra covenant output"));
            redundant!(rf4_idx_redundant, [concat!($file, ":RF4_idx")], concat!("KA15 ", $obj, ": successor without covenant binding"));
            redundant!(rf4_cov_redundant, [concat!($file, ":RF4_cov")], concat!("KA15 ", $obj, ": extra covenant output"));
            killed!(rf4_all, [concat!($file, ":RF4_auth"), concat!($file, ":RF4_idx"), concat!($file, ":RF4_cov")], concat!("KA15 ", $obj, ": successor without covenant binding"));
            // >= 1 output: A6.4 (one authorized output) and A6.6 (reads output 0).
            redundant!(rf5_count_0_redundant, [concat!($file, ":RF5_count#0")], concat!("KA15 ", $obj, ": no output (A6.4 rejects first)"));
            // <= 2 outputs: the RF5_zero loop `for(zo, 1, n, 1)`.
            redundant!(rf5_count_1_redundant, [concat!($file, ":RF5_count#1")], concat!("KA15 ", $obj, ": extra plain output"));
            killed!(rf5_zero, [concat!($file, ":RF5_zero")], concat!("KA15 ", $obj, ": the change under a fresh genesis covenant ID"));
            killed!(rf6, [concat!($file, ":RF6")], concat!("KA15 ", $obj, ": value + 1"));
        }
    };
}

refresh_mutants!(ticket_refresh, "ticket", "ticket");
refresh_mutants!(pool_refresh, "pool", "pool");
refresh_mutants!(marker_refresh, "marker", "marker");

// ================================================================ pool ticket_leg (PT)

killed!(pt1, ["pool:PT1"], "KA11 a covenant-less copy of the pool as funding (at the copy, input 2)");
killed!(pt2, ["pool:PT2"], "KA4 look-alike ticket under a fresh ID, at the pool");
killed!(pt3, ["pool:PT3"], "KA10 pool leg authorizing pool' (at the pool)");
// As K1_tpl (template hash over length-prefixed prefix and suffix).
redundant!(pt4_tpl_0_redundant, ["pool:PT4_tpl#0"], "KA11 ticket_pl = -1");
redundant!(pt4_tpl_1_redundant, ["pool:PT4_tpl#1"], "KA11 ticket_pl = SCRIPT_MAX + 1");
redundant!(pt4_tpl_2_redundant, ["pool:PT4_tpl#2"], "KA11 ticket_sl = -1");
redundant!(pt4_tpl_3_redundant, ["pool:PT4_tpl#3"], "KA11 ticket_sl = SCRIPT_MAX + 1");
killed!(pt4_role, ["pool:PT4_role"], "KA11 real ticket template with role 0x07 at input 0");
killed!(pt4_round, ["pool:PT4_round"], "KA11 round 2 ticket at the round 1 pool");

// ================================================================ template bindings (look-alike controls)

/// Input `i` running template `t` (same state bytes and arguments, P2SH rebuilt).
fn swap_template(b: &mut Built, i: usize, t: &Arc<Template>) {
    let (old, redeem) = match &b.inputs[i].kind {
        InKind::Entry { template, redeem, .. } => (template.clone(), redeem.clone()),
        _ => panic!("input {i} is not an entry input"),
    };
    let state = redeem[old.prefix.len()..redeem.len() - old.suffix.len()].to_vec();
    b.set_entry_redeem(i, [t.prefix.as_slice(), &state, &t.suffix].concat());
    if let InKind::Entry { template, .. } = &mut b.inputs[i].kind {
        *template = t.clone();
    }
}

/// Checks 3 and 14 (POOL_TPL in readInputStateWithTemplate and
/// validateOutputStateWithInputTemplate): a ticket compiled with a look-alike pool's hash as
/// POOL_TPL claims against that look-alike pool (accepted); the committed ticket, same state
/// and arguments, rejects the same look-alike pool at input 0.
#[test]
fn template_claim_reads_pool_under_pool_tpl() {
    let fake = lookalike(Sil::Pool);
    let mut good: Case = pay().into();
    good.retarget(&fake);
    let g = good.build_with(fake.clone());
    accept_m4("claim with a look-alike pool, ticket POOL_TPL = its hash", &g);
    let real = tpl();
    let mut b = g.clone();
    swap_template(&mut b, ClaimCase::TICKET, &real.ticket);
    // ticket' under the committed ticket template, so only the pool binding differs.
    let Case::Claim(c) = &good else { unreachable!() };
    b.outputs[1].script_public_key = real.ticket_spk(c.expected().ticket.as_ref().unwrap());
    let e = reject_at(&b, ClaimCase::TICKET);
    println!("committed ticket rejects a look-alike pool: {e:?}");
}

/// Check 14 (output half): pool' under a look-alike pool template (same state) while input
/// 1 is the committed pool is rejected. The positive half (pool' follows input 1's
/// template) is the look-alike claim accepted above.
#[test]
fn template_claim_pool_successor_under_input_template() {
    let fake = lookalike(Sil::Pool);
    let c = pay();
    let mut b = c.build();
    b.outputs[0].script_public_key = fake.pool_spk(&c.expected().pool);
    reject_at(&b, ClaimCase::TICKET);
    accept_m4("claim (base)", &c.build());
}

/// Check 15 (validateOutputState: the ticket's own template): a look-alike ticket claims
/// into its own look-alike successor (accepted); the committed ticket rejects ticket' under
/// the look-alike template.
#[test]
fn template_claim_ticket_successor_is_own_template() {
    let fake = lookalike(Sil::Ticket);
    let mut good: Case = pay().into();
    good.retarget(&fake);
    accept_m4("look-alike ticket claims into its own successor", &good.build_with(fake.clone()));
    let c = pay();
    let mut b = c.build();
    b.outputs[1].script_public_key = fake.ticket_spk(c.expected().ticket.as_ref().unwrap());
    reject_at(&b, ClaimCase::TICKET);
}

/// A5a.3 / A5a.7: as the claim controls, for return_deposit_with_pool.
#[test]
fn template_return_reads_pool_under_pool_tpl() {
    let fake = lookalike(Sil::Pool);
    let mut good: Case = ReturnCase::with_pool().into();
    good.retarget(&fake);
    let g = good.build_with(fake.clone());
    accept_m4("return with a look-alike pool, ticket POOL_TPL = its hash", &g);
    let mut b = g.clone();
    swap_template(&mut b, ReturnCase::TICKET, &tpl().ticket);
    reject_at(&b, ReturnCase::TICKET);
    // pool' under a look-alike template while input 1 is the committed pool.
    let r = ReturnCase::with_pool();
    let mut b = r.build();
    b.outputs[0].script_public_key = fake.pool_spk(r.expected().pool.as_ref().unwrap());
    reject_at(&b, ReturnCase::TICKET);
}

/// A6.7 (validateOutputState): each object refreshes into its own template only. A
/// look-alike object refreshes into its look-alike successor (accepted); the committed
/// object rejects a successor under the look-alike template.
#[test]
fn template_refresh_successor_is_own_template() {
    for (name, base) in refresh_bases() {
        let which = match base.obj {
            RefreshObj::Ticket(_) => Sil::Ticket,
            RefreshObj::Pool(_) => Sil::Pool,
            RefreshObj::Marker(_) => Sil::Marker,
            _ => unreachable!("M4 refresh bases only"),
        };
        let fake = lookalike(which);
        let mut good: Case = base.clone().into();
        good.retarget(&fake);
        accept_m4(&format!("look-alike {name} refresh"), &good.build_with(fake.clone()));
        let mut b = base.build();
        b.outputs[0].script_public_key = base.obj.spk(&fake);
        reject_at(&b, 0);
    }
}

/// PT4 (readInputStateWithTemplate under the pool's ticket_tpl): a pool whose ticket_tpl is
/// a look-alike ticket's hash serves that look-alike (accepted); with the committed ticket
/// hash in its state it rejects the same look-alike at input 1.
#[test]
fn template_pool_leg_reads_ticket_under_ticket_tpl() {
    let fake = lookalike(Sil::Ticket);
    let mut good: Case = pay().into();
    good.retarget(&fake);
    let g = good.build_with(fake.clone());
    accept_m4("pool with ticket_tpl = a look-alike ticket's hash", &g);
    let Case::Claim(c) = &good else { unreachable!() };
    let mut pool = c.pool.clone();
    pool.ticket_tpl = tpl().ticket.hash;
    let mut b = g.clone();
    b.set_entry_redeem(ClaimCase::POOL, fake.pool_redeem(&pool));
    b.outputs[0].script_public_key = pay_to_script_hash_script(&fake.pool_redeem(&pool));
    reject_at(&b, ClaimCase::POOL);
}

// ================================================================ coverage

/// Every `// mut:` tag line of the M4 code (ticket.sil: all; pool.sil: PT*, RF*;
/// marker.sil: RF*) is named above: a single-line tag as "file:TAG", each line of a
/// multi-line tag as "file:TAG#n".
#[test]
fn every_m4_tag_is_tested() {
    let me = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/m4_mutants.rs")).expect("own source");
    let (mut tags, mut lines) = (0, 0);
    for (f, file) in [("ticket.sil", "ticket"), ("pool.sil", "pool"), ("marker.sil", "marker")] {
        let src = std::fs::read_to_string(harness::covenant_path(f)).expect("source");
        let mut count: BTreeMap<String, usize> = BTreeMap::new();
        // The header comment's "`// mut:<ID>` tags ..." is not a tag.
        for l in src.lines().filter(|l| !l.contains("// mut:<")) {
            for t in line_tags(l) {
                let m4 = file == "ticket" || t.starts_with("PT") || t.starts_with("RF");
                if m4 {
                    *count.entry(t.to_string()).or_default() += 1;
                }
            }
        }
        // The refresh tests are generated per file by `refresh_mutants!` (tags written
        // concat!($file, ":TAG")), so an RF tag counts when the file's instance exists.
        let generated = me.contains(&format!("refresh_mutants!({file}_refresh, \"{file}\""));
        for (t, n) in count {
            tags += 1;
            lines += n;
            let named = |s: &str| me.contains(&format!("\"{file}:{s}\"")) || (generated && t.starts_with("RF") && me.contains(&format!("\":{s}\"")));
            if n == 1 {
                assert!(named(&t), "{f}: tag {t} has no mutant test");
            } else {
                for i in 0..n {
                    assert!(named(&format!("{t}#{i}")), "{f}: line {i} of tag {t} has no mutant test");
                }
            }
        }
    }
    for e in EQUIVALENT {
        assert!(me.contains(&format!("\"{}\"", e.0)));
    }
    println!("M4 tags: {tags}, tagged lines: {lines}");
    assert_eq!((tags, lines), (EXPECTED_TAGS, EXPECTED_LINES), "M4 tag count changed: update this test and the handoff table");
}

const EXPECTED_TAGS: usize = 131;
const EXPECTED_LINES: usize = 187;

/// DIAGNOSTIC (ignored; not a test result on its own): for every `redundant!` line above,
/// the first check that rejects the attack under that mutant, found by the M4 attribution
/// instrument run on the mutant source. It backs the proofs in the comments. Run:
/// `cargo test -p lottery --release --test m4_mutants redundancy_attribution -- --ignored --nocapture --test-threads=1`.
#[test]
#[ignore]
fn redundancy_attribution() {
    let me = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/m4_mutants.rs")).expect("own source");
    let mut rows: Vec<(String, String)> = vec![];
    for l in me.lines().map(str::trim).filter(|l| l.starts_with("redundant!(")) {
        let q: Vec<&str> = l.split('"').collect();
        if l.contains("concat!") {
            // refresh_mutants!: q = [.., ":TAG", .., "KA15 ", .., ": rest", ..]
            for obj in ["ticket", "pool", "marker"] {
                rows.push((format!("{obj}{}", q[1]), format!("KA15 {obj}{}", q[5])));
            }
        } else {
            rows.push((q[1].to_string(), q[3].to_string()));
        }
    }
    for (tag, id) in rows {
        let (sil, t, n) = parse(&tag);
        let mut src = Sources::committed(b_max());
        let s = field(&mut src, sil);
        let mutant_src = strip(s, t, n);
        let a = atk(&id);
        let (_, b, c) = a.pair_case(&tpl());
        let got = first_failing_check_in(&b, &c, a.check, a.input, Some(&mutant_src));
        println!("REDUNDANT {tag} | {id} | input {} | first rejecting check under the mutant: {}", a.input, got.as_deref().unwrap_or("-"));
    }
}
