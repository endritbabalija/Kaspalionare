//! M5 mutation tests (spec M5 A10 "Mutants", negative controls as M2-M4). For every
//! `// mut:<ID>` tag of M5 (vault.sil and drawer.sil: the tags starting `M5`; request.sil,
//! record.sil, position.sil: every tag):
//! - `killed`: with the committed sources the named input rejects the attack (the strike of
//!   m5_attacks.rs, shared through `common::m5`); with only that line removed (compiled in
//!   memory, the sources on disk are untouched), the named input ACCEPTS it, and the valid
//!   base transaction is still accepted by every input.
//! - `redundant`: the mutant still rejects the attack at the named input, because another
//!   check (a compiler-inserted loop bound, a template hash, an engine error) implies the
//!   removed one. The argument is in each test's comment and in the handoff (Stage 6). If a
//!   later change removes the implying check, the test fails and the gap is noticed.
//! - `killed_positive` (one assignment line, M5D13_rec): the mutant rejects a valid
//!   converting draw, so the positive tests (VP4-VP9, VP14) fail under it.
//!
//! Granularity is the line: a tag that marks several `require` lines is tested line by line
//! (`"file:TAG#n"` = the n-th line carrying TAG, 0-based, in file order); `"file:TAG"` removes
//! every line of the tag. Groups whose members are each redundant because of the others are
//! also removed together and killed (as M3 D17, M4).
//!
//! Template bindings (inside `readInputStateWithTemplate` / `validateOutputState*`, not
//! removable as a line: vault D8/D9, W3/W9/W10/W11, position PO4, settle S3/S11/S12/S13,
//! record RL4, drawer 26) get look-alike negative controls: the same transaction is accepted
//! when the trusted hash is the look-alike's and rejected under the committed one.
//!
//! `every_m5_tag_is_tested` checks that every M5 tag line is named here.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::m2::{Sil, accept_m4, lookalike, reject_at, tpl};
use common::m5::*;
use harness::{ArtifactValue, TxScriptError};
use lottery::buy::{Built, InKind, Sources, Template, Templates, b_max};
use lottery::draw::{ARG_RECORD_PREFIX, ARG_RECORD_SUFFIX};
use lottery::vault::*;

// ---------------------------------------------------------------- mutant mechanism

/// "file:TAG" or "file:TAG#n".
fn parse(t: &str) -> (Sil, &str, Option<usize>) {
    let (file, rest) = t.split_once(':').expect("file:TAG");
    let sil = match file {
        "vault" => Sil::Vault,
        "drawer" => Sil::Drawer,
        "request" => Sil::Request,
        "record" => Sil::Record,
        "position" => Sil::Position,
        f => panic!("no M5 file {f}"),
    };
    match rest.split_once('#') {
        Some((tag, n)) => (sil, tag, Some(n.parse().expect("line index"))),
        None => (sil, rest, None),
    }
}

/// The tags of a line (a header comment's `// mut:M5<ID>` is not a tag).
fn line_tags(line: &str) -> Vec<&str> {
    match line.find("// mut:") {
        Some(p) if line[p + 7..].starts_with("M5<") || line[p + 7..].starts_with('<') => vec![],
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
        Sil::Vault => &mut s.vault,
        Sil::Drawer => &mut s.drawer,
        Sil::Request => &mut s.request,
        Sil::Record => &mut s.record,
        Sil::Position => &mut s.position,
        _ => unreachable!(),
    }
}

fn hash(t: &Templates, which: Sil) -> [u8; 32] {
    match which {
        Sil::Vault => t.vault.hash,
        Sil::Drawer => t.drawer.hash,
        Sil::Request => t.request.hash,
        Sil::Record => t.record.hash,
        Sil::Position => t.position.hash,
        _ => unreachable!(),
    }
}

/// Templates compiled with the given tag lines removed (one or more files). Lines of the
/// same tag are given as distinct tags only (stripping shifts the line index).
fn mutant5(tags: &[&str]) -> Arc<Templates> {
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
    let a = atk5(id);
    a.strike();
    let t = mutant5(tags);
    let (g, b) = a.pair(&t);
    accept_m4(&format!("base under mutant {tags:?}"), &g);
    if let Err(e) = b.finish().run_input(a.input) {
        panic!("mutant {tags:?}: input {} still rejects `{id}`: {e:?}", a.input);
    }
}

/// Redundant line: the mutant still rejects the attack at the named input (not on the
/// budget), and still accepts the base.
fn redundant(tags: &[&str], id: &str) {
    let a = atk5(id);
    a.strike();
    let t = mutant5(tags);
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

/// Equivalent lines with no transaction that tells them apart, and why (each is also run as
/// a `redundant` test with the closest attack).
const EQUIVALENT: &[(&str, &str)] = &[
    (
        "request:M5S1_value#0",
        "v0 = OpTxInputAmount(0) >= 0 always: the opcode converts the u64 amount to i64 and fails with NumberTooBig above i64::MAX (rusty-kaspa v2.1.0 crypto/txscript/src/opcodes/mod.rs:1100-1107); run: s1_value_0_redundant",
    ),
    (
        "drawer:M5C2_slack",
        "L' <= V' + D_c whenever check 17 passes: L' = x2 + G2' (m - 1) passes Cv = muldiv(V', cap) with cap <= 10^4, so L' <= V' for V' >= 0, and V' < 0 fails D14_nonneg first. The only states that reach it (V' < 0) are killed with D14_nonneg and M5C2_fits removed together (m3_mutants.rs d14_nonneg, M5 Stage 4 S4-6); run: c2_slack_redundant",
    ),
];

// ================================================================ vault request_deposit (A4.1)

killed!(d1_active, ["vault:M5D1_active"], "VA9 a covenant-less copy of the vault as funding (at the copy, input 1)");
killed!(d1_role, ["vault:M5D1_role"], "VA6 request_deposit: vault role 0x07 (fabricated)");
killed!(d1_round_0, ["vault:M5D1_round#0"], "VA6 request_deposit: vault round -1 (fabricated)");
killed!(d1_round_1, ["vault:M5D1_round#1"], "VA6 request_deposit: vault round TOTAL_MAX (fabricated)");
killed!(d1_assets_0, ["vault:M5D1_assets#0"], "VA6 request_deposit: total_assets -1 (fabricated)");
// total_assets <= ASSETS_MAX: D5 bounds D <= TOTAL_MAX - total_assets - pend_dep with D >=
// MIN_DEPOSIT and pend_dep >= 0, so total_assets <= TOTAL_MAX - MIN_DEPOSIT < ASSETS_MAX.
redundant!(d1_assets_1_redundant, ["vault:M5D1_assets#1"], "VA6 request_deposit: total_assets ASSETS_MAX + 1 (fabricated, engine only)");
// total_shares >= 0: 0 <= pend_wsh <= total_shares (M5D1_wsh) implies it.
redundant!(d1_shares_0_redundant, ["vault:M5D1_shares#0"], "VA6 request_deposit: total_shares -1 (fabricated)");
killed!(d1_shares_1, ["vault:M5D1_shares#1"], "VA6 request_deposit: total_shares = SHARES_MAX + 1 (fabricated vault)");
killed!(d1_dep_0, ["vault:M5D1_dep#0"], "VA6 request_deposit: pend_dep -1 (fabricated)");
// pend_dep <= TOTAL_MAX - total_assets: D5 requires MIN_DEPOSIT <= D <= TOTAL_MAX -
// total_assets - pend_dep, which is negative above it.
redundant!(d1_dep_1_redundant, ["vault:M5D1_dep#1"], "VA6 request_deposit: pend_dep TOTAL_MAX - total_assets + 1 (fabricated, engine only)");
killed!(d1_wsh_0, ["vault:M5D1_wsh#0"], "VA6 request_deposit: pend_wsh -1 (fabricated)");
killed!(d1_wsh_1, ["vault:M5D1_wsh#1"], "VA6 request_deposit: pend_wsh total_shares + 1 (fabricated)");
killed!(d1_liab_0, ["vault:M5D1_liab#0"], "VA6 request_deposit: liability -1 (fabricated)");
killed!(d1_liab_1, ["vault:M5D1_liab#1"], "VA6 request_deposit: liability TOTAL_MAX + 1 (fabricated)");
// R >= 0: D7 requires the request output's value (>= 0, a u64) to equal R.
redundant!(d1_rsv_0_redundant, ["vault:M5D1_rsv#0"], "VA6 request_deposit: R = -1 (vault value below total_assets + pend_dep)");
killed!(d1_rsv_1, ["vault:M5D1_rsv#1"], "VA6 request_deposit: R = RESERVE_MAX + 1");
// OpCovInputCount(cov) = 1 follows from D2_zero (inputs >= 1 carry the zero ID; pigeonhole).
// Removed with D2_zero, a parallel group is accepted.
redundant!(d2_count_redundant, ["vault:M5D2_count"], "VA9 a parallel group (a second vault-like UTXO under the same ID)");
killed!(d2_count_and_zero, ["vault:M5D2_count", "vault:M5D2_zero"], "VA9 a parallel group (a second vault-like UTXO under the same ID)");
killed!(d2_fund_0, ["vault:M5D2_fund#0"], "VA9 no funding input");
// <= 1 + F_MAX: the D2_zero loop `for(fi, 1, n, F_MAX)` carries the compiler's
// require(n - 1 <= F_MAX) (silverscript-lang/src/compiler/for.rs:135-155).
redundant!(d2_fund_1_redundant, ["vault:M5D2_fund#1"], "VA9 nine funding inputs");
killed!(d2_zero, ["vault:M5D2_zero"], "VA9 a funding input with a covenant ID");
// D3's four requires are each implied by the others plus D4_zero (outputs >= 2 carry the
// zero ID): input 0's authorized outputs are continuation outputs under cov
// (crypto/txscript/src/covenants.rs:124-131), so they lie in 0..1; OpAuthOutputIdx fails past
// the count. Removed together, vault' may leave the covenant.
redundant!(d3_auth_redundant, ["vault:M5D3_auth"], "VA9 an extra covenant output (a request copy)");
redundant!(d3_idx_0_redundant, ["vault:M5D3_idx#0"], "VA9 an extra covenant output (a request copy)");
redundant!(d3_idx_1_redundant, ["vault:M5D3_idx#1"], "VA9 an extra covenant output (a request copy)");
redundant!(d3_cov_redundant, ["vault:M5D3_cov"], "VA9 an extra covenant output (a request copy)");
killed!(d3_all, ["vault:M5D3_auth", "vault:M5D3_idx", "vault:M5D3_cov"], "VA9 vault' without covenant binding");
// >= 2 outputs: D7 reads output 1 (index error without it).
redundant!(d4_count_0_redundant, ["vault:M5D4_count#0"], "VA9 only vault' (no request output)");
// <= 3 outputs: the D4_zero loop `for(zo, 2, n, 1)` bounds n - 2 <= 1 (for.rs:135-155).
redundant!(d4_count_1_redundant, ["vault:M5D4_count#1"], "VA9 an extra plain output");
killed!(d4_zero, ["vault:M5D4_zero"], "VA9 the change under a fresh genesis covenant ID");
killed!(d5_min, ["vault:M5D5_min"], "VA1 request_deposit D = MIN_DEPOSIT - 1");
killed!(d5_min_zero, ["vault:M5D5_min"], "VA1 request_deposit D = 0");
// Consensus-equivalent (values <= MAX_SOMPI < 2^62), killed in the engine (M4 S4-3).
killed!(d5_max, ["vault:M5D5_max"], "VA6 request_deposit D = TOTAL_MAX - total_assets - pend_dep + 1 (engine only)");
killed!(d6_class, ["vault:M5D6_class"], "VA9 owner class 3");
killed!(d6_pad, ["vault:M5D6_pad"], "VA9 owner key padding byte set (class 0)");
killed!(d7_plus, ["vault:M5D7"], "VA9 request value R + 1");
killed!(d7_minus, ["vault:M5D7"], "VA9 request value R - 1");

// ================================================================ vault request_withdraw (A4.2)

killed!(w1_active, ["vault:M5W1_active"], "VA10 a covenant-less copy of the vault as funding (at the copy, input 2)");
killed!(w1_role, ["vault:M5W1_role"], "VA6 request_withdraw: vault role 0x07 (fabricated)");
killed!(w1_round_0, ["vault:M5W1_round#0"], "VA6 request_withdraw: vault round -1 (fabricated)");
killed!(w1_round_1, ["vault:M5W1_round#1"], "VA6 request_withdraw: vault round TOTAL_MAX (fabricated)");
killed!(w1_assets_0, ["vault:M5W1_assets#0"], "VA6 request_withdraw: total_assets -1 (fabricated)");
killed!(w1_assets_1, ["vault:M5W1_assets#1"], "VA6 request_withdraw: total_assets ASSETS_MAX + 1 (fabricated, engine only)");
// total_shares >= 0: 0 <= pend_wsh <= total_shares (M5W1_wsh) implies it.
redundant!(w1_shares_0_redundant, ["vault:M5W1_shares#0"], "VA6 request_withdraw: total_shares -1 (fabricated)");
killed!(w1_shares_1, ["vault:M5W1_shares#1"], "VA6 request_withdraw: total_shares = SHARES_MAX + 1 (fabricated vault)");
killed!(w1_dep_0, ["vault:M5W1_dep#0"], "VA6 request_withdraw: pend_dep -1 (fabricated)");
killed!(w1_dep_1, ["vault:M5W1_dep#1"], "VA6 request_withdraw: pend_dep TOTAL_MAX - total_assets + 1 (fabricated, engine only)");
killed!(w1_wsh_0, ["vault:M5W1_wsh#0"], "VA6 request_withdraw: pend_wsh -1 (fabricated)");
// pend_wsh <= total_shares: W4 requires 1 <= w <= total_shares - pend_wsh.
redundant!(w1_wsh_1_redundant, ["vault:M5W1_wsh#1"], "VA6 request_withdraw: pend_wsh total_shares + 1 (fabricated)");
killed!(w1_liab_0, ["vault:M5W1_liab#0"], "VA6 request_withdraw: liability -1 (fabricated)");
killed!(w1_liab_1, ["vault:M5W1_liab#1"], "VA6 request_withdraw: liability TOTAL_MAX + 1 (fabricated)");
// R >= 0: W3 requires the position input's value (>= 0) to equal R.
redundant!(w1_rsv_0_redundant, ["vault:M5W1_rsv#0"], "VA6 request_withdraw: R = -1 (vault value below total_assets + pend_dep)");
killed!(w1_rsv_1, ["vault:M5W1_rsv#1"], "VA6 request_withdraw: R = RESERVE_MAX + 1");
// Template lengths: the template hash commits to len(prefix) || prefix || len(suffix) ||
// suffix (silverscript-lang/src/compiler/compile/state.rs:140-167), so only the true lengths
// read the position; out-of-range lengths fail the script-sig substring (OpSubstr-family
// index errors, rusty-kaspa v2.1.0 crypto/txscript/src/opcodes/mod.rs:502-507). Defense in
// depth (as M4 K1_tpl).
redundant!(w1_tpl_0_redundant, ["vault:M5W1_tpl#0"], "VA10 pos_pl = -1");
redundant!(w1_tpl_1_redundant, ["vault:M5W1_tpl#1"], "VA10 pos_pl = SCRIPT_MAX + 1");
redundant!(w1_tpl_2_redundant, ["vault:M5W1_tpl#2"], "VA10 pos_sl = -1");
redundant!(w1_tpl_3_redundant, ["vault:M5W1_tpl#3"], "VA10 pos_sl = SCRIPT_MAX + 1");
// OpCovInputCount(cov) = 2 and OpInputCovenantId(1) = cov each follow from the other plus
// W2_zero (pigeonhole, as M4 K2). Removed together, a position of another covenant is read.
redundant!(w2_count_redundant, ["vault:M5W2_count"], "VA10 the position under another covenant ID (at the vault)");
redundant!(w2_pos_redundant, ["vault:M5W2_pos"], "VA10 the position under another covenant ID (at the vault)");
killed!(w2_count_and_pos, ["vault:M5W2_count", "vault:M5W2_pos"], "VA10 the position under another covenant ID (at the vault)");
// >= 3 inputs: W5 requires 2 <= owner_idx < inputs.
redundant!(w2_fund_0_redundant, ["vault:M5W2_fund#0"], "VA10 no plain input (whole withdrawal, fee 0)");
// <= 2 + F_MAX: the W2_zero loop `for(fi, 2, n, F_MAX)` (for.rs:135-155).
redundant!(w2_fund_1_redundant, ["vault:M5W2_fund#1"], "VA10 nine plain inputs");
killed!(w2_zero, ["vault:M5W2_zero"], "VA10 the owner input with a covenant ID");
killed!(w3_role, ["vault:M5W3_role"], "VA10 a position with role 0x07 (fabricated, whole withdrawal)");
// shares >= 1: W4 requires 1 <= w <= shares.
redundant!(w3_shares_0_redundant, ["vault:M5W3_shares#0"], "VA10 a position with 0 shares (fabricated)");
killed!(w3_shares_1, ["vault:M5W3_shares#1"], "VA10 a position with SHARES_MAX + 1 shares (fabricated)");
killed!(w3_class, ["vault:M5W3_class"], "VA10 a position with owner class 3, the owner input its P2SH form (fabricated)");
killed!(w3_pad, ["vault:M5W3_pad"], "VA10 a position with the key padding byte set (class 0, fabricated)");
killed!(w3_value, ["vault:M5W3_value"], "VA10 the position's value R + 1");
killed!(w4_min, ["vault:M5W4_min"], "VA6 request_withdraw w = 0");
killed!(w4_shares, ["vault:M5W4_shares"], "VA6 request_withdraw w = shares + 1");
killed!(w4_total, ["vault:M5W4_total"], "VA6 request_withdraw pend_wsh + w > total_shares");
killed!(w5_idx_0, ["vault:M5W5_idx#0"], "VA10 owner_idx = 0 with a P2SH owner whose script hash is the vault's (fabricated)");
// owner_idx < inputs: OpTxInputSpk fails on an index past the inputs.
redundant!(w5_idx_1_redundant, ["vault:M5W5_idx#1"], "VA10 Schnorr: owner_idx = inputs.length");
killed!(w5_spk_schnorr, ["vault:M5W5_spk"], "VA10 Schnorr: no owner input (owner_idx at a funding input of another script)");
killed!(w5_spk_ecdsa, ["vault:M5W5_spk"], "VA10 ECDSA: no owner input (owner_idx at a funding input of another script)");
killed!(w5_spk_p2sh, ["vault:M5W5_spk"], "VA10 P2SH: no owner input (owner_idx at a funding input of another script)");
killed!(w5_spk_victim, ["vault:M5W5_spk"], "VA10 a position of another owner with the attacker's input");
// W6's three requires are each implied by the other two plus W7_zero (as D3). Removed
// together, position' may leave the covenant.
redundant!(w6_auth_redundant, ["vault:M5W6_auth"], "VA9 request_withdraw: an extra covenant output (a request copy)");
redundant!(w6_idx_redundant, ["vault:M5W6_idx"], "VA9 request_withdraw: an extra covenant output (a request copy)");
redundant!(w6_cov_redundant, ["vault:M5W6_cov"], "VA9 request_withdraw: an extra covenant output (a request copy)");
killed!(w6_all, ["vault:M5W6_auth", "vault:M5W6_idx", "vault:M5W6_cov"], "VA9 request_withdraw: position' without covenant binding");
// >= nCov outputs: W6 (nCov authorized outputs) and W8 (reads outputs 1 and 2).
redundant!(w7_count_0_redundant, ["vault:M5W7_count#0"], "VA9 request_withdraw: partial with only vault' and request");
// <= nCov + 1: the W7_zero loop `for(zo, nCov, n, 1)` (for.rs:135-155).
redundant!(w7_count_1_redundant, ["vault:M5W7_count#1"], "VA9 request_withdraw: an extra plain output");
killed!(w7_zero, ["vault:M5W7_zero"], "VA9 request_withdraw: the change under a fresh genesis covenant ID");
killed!(w8_vault, ["vault:M5W8_vault"], "VA9 request_withdraw: vault' value - 1");
killed!(w8_request, ["vault:M5W8_request"], "VA9 request_withdraw: request value R + 1");
killed!(w8_position, ["vault:M5W8_position"], "VA9 request_withdraw: position' value R - 1");

// ================================================================ position vault_leg (PO)

killed!(po1, ["position:PO1"], "VA10 a covenant-less copy of the position as funding (at the copy, input 2)");
killed!(po2, ["position:PO2"], "VA10 the position under another covenant ID (at the position)");
killed!(po3, ["position:PO3"], "VA10 the position leg authorizing position' (at the position)");
// As W1_tpl (template hash over length-prefixed prefix and suffix).
redundant!(po4_tpl_0_redundant, ["position:PO4_tpl#0"], "VA10 vault_pl = -1 at the position leg");
redundant!(po4_tpl_1_redundant, ["position:PO4_tpl#1"], "VA10 vault_pl = SCRIPT_MAX + 1 at the position leg");
redundant!(po4_tpl_2_redundant, ["position:PO4_tpl#2"], "VA10 vault_sl = -1 at the position leg");
redundant!(po4_tpl_3_redundant, ["position:PO4_tpl#3"], "VA10 vault_sl = SCRIPT_MAX + 1 at the position leg");
killed!(po4_role, ["position:PO4_role"], "VA10 a vault with role 0x07 (fabricated) at the position leg");

// ================================================================ record request_leg (RL)

killed!(rl1, ["record:M5RL1"], "VA12 a covenant-less copy of the record as funding (at the copy, input 2)");
killed!(rl2, ["record:M5RL2"], "VA3 a record under another covenant ID (at the record leg)");
killed!(rl3, ["record:M5RL3"], "VA12 the record leg authorizing record' (at the record)");
redundant!(rl4_tpl_0_redundant, ["record:M5RL4_tpl#0"], "VA12 req_pl = -1 at the record leg");
redundant!(rl4_tpl_1_redundant, ["record:M5RL4_tpl#1"], "VA12 req_pl = SCRIPT_MAX + 1 at the record leg");
redundant!(rl4_tpl_2_redundant, ["record:M5RL4_tpl#2"], "VA12 req_sl = -1 at the record leg");
redundant!(rl4_tpl_3_redundant, ["record:M5RL4_tpl#3"], "VA12 req_sl = SCRIPT_MAX + 1 at the record leg");
killed!(rl4_role, ["record:M5RL4_role"], "VA12 a request with role 0x08 (fabricated) at the record leg");
killed!(rl4_round, ["record:M5RL4_round"], "VA3 the record of t + 1 at the record leg (RL4: round = own round)");

// ================================================================ request settle (A6.1)

killed!(s1_active, ["request:M5S1_active"], "VA12 a covenant-less copy of the request as funding (at the copy, input 2)");
killed!(s1_role, ["request:M5S1_role"], "VA11 a request with role 0x08 (fabricated)");
// round >= 1 and rc.round >= 1 imply each other through S3's rc.round = round (and so do
// round < TOTAL_MAX and rc.round < TOTAL_MAX). Removed in pairs, a request and record of
// round 0 (or TOTAL_MAX) settle.
redundant!(s1_round_0_redundant, ["request:M5S1_round#0"], "VA11 request and record round 0 (fabricated)");
redundant!(s3_round_0_redundant, ["request:M5S3_round#0"], "VA11 request and record round 0 (fabricated)");
killed!(s1_s3_round_0, ["request:M5S1_round#0", "request:M5S3_round#0"], "VA11 request and record round 0 (fabricated)");
redundant!(s1_round_1_redundant, ["request:M5S1_round#1"], "VA11 request and record round TOTAL_MAX (fabricated)");
redundant!(s3_round_1_redundant, ["request:M5S3_round#1"], "VA11 request and record round TOTAL_MAX (fabricated)");
killed!(s1_s3_round_1, ["request:M5S1_round#1", "request:M5S3_round#1"], "VA11 request and record round TOTAL_MAX (fabricated)");
killed!(s1_kind, ["request:M5S1_kind"], "VA11 a withdrawal request of kind 2 (fabricated, FINAL)");
killed!(s1_amount_0, ["request:M5S1_amount#0"], "VA11 a deposit request below MIN_DEPOSIT (fabricated)");
killed!(s1_amount_1, ["request:M5S1_amount#1"], "VA11 a withdrawal request of SHARES_MAX + 1 shares (fabricated, FINAL)");
killed!(s1_class, ["request:M5S1_class"], "VA11 a request with owner class 3, paid to its P2SH form (fabricated)");
killed!(s1_pad, ["request:M5S1_pad"], "VA11 a request with the key padding byte set (class 0, fabricated)");
redundant!(s1_value_0_redundant, ["request:M5S1_value#0"], "VA11 request value 2^63 (engine only, OpTxInputAmount fails)");
// Consensus-equivalent (values <= MAX_SOMPI < 2^62), killed in the engine (M4 S4-3).
killed!(s1_value_1, ["request:M5S1_value#1"], "VA11 PAY: request value TOTAL_MAX + 1 (engine only)");
// As W1_tpl (the record's template hash commits to the lengths).
redundant!(s1_tpl_0_redundant, ["request:M5S1_tpl#0"], "VA12 rec_pl = -1 at the request");
redundant!(s1_tpl_1_redundant, ["request:M5S1_tpl#1"], "VA12 rec_pl = SCRIPT_MAX + 1 at the request");
redundant!(s1_tpl_2_redundant, ["request:M5S1_tpl#2"], "VA12 rec_sl = -1 at the request");
redundant!(s1_tpl_3_redundant, ["request:M5S1_tpl#3"], "VA12 rec_sl = SCRIPT_MAX + 1 at the request");
// As W2 (pigeonhole with S2_zero).
redundant!(s2_count_redundant, ["request:M5S2_count"], "VA3 a record under another covenant ID (at the request)");
redundant!(s2_rec_redundant, ["request:M5S2_rec"], "VA3 a record under another covenant ID (at the request)");
killed!(s2_count_and_rec, ["request:M5S2_count", "request:M5S2_rec"], "VA3 a record under another covenant ID (at the request)");
killed!(s2_fund_0, ["request:M5S2_fund#0"], "VA11 no funding input");
// <= 2 + F_MAX: the S2_zero loop `for(fi, 2, n, F_MAX)` (for.rs:135-155).
redundant!(s2_fund_1_redundant, ["request:M5S2_fund#1"], "VA11 nine funding inputs");
killed!(s2_zero, ["request:M5S2_zero"], "VA11 a funding input with a covenant ID");
killed!(s3_role, ["request:M5S3_role"], "VA11 a record with role 0x07 (fabricated, drained)");
killed!(s3_refund, ["request:M5S3_refund"], "VA11 a record with refund = 2 (fabricated, FINAL)");
killed!(s3_dep_0, ["request:M5S3_dep#0"], "VA11 a record with dep_rem -1 (fabricated, withdrawal)");
killed!(s3_dep_1, ["request:M5S3_dep#1"], "VA11 a record with dep_rem TOTAL_MAX + 1 (fabricated, withdrawal)");
killed!(s3_mint_0, ["request:M5S3_mint#0"], "VA11 a record with mint_rem -1 (fabricated, withdrawal)");
killed!(s3_mint_1, ["request:M5S3_mint#1"], "VA11 a record with mint_rem SHARES_MAX + 1 (fabricated, withdrawal)");
// wsh_rem >= 0: 0 <= burn_rem <= wsh_rem (M5S3_burn) implies it.
redundant!(s3_wsh_0_redundant, ["request:M5S3_wsh#0"], "VA11 a record with wsh_rem -1 (fabricated, deposit)");
killed!(s3_wsh_1, ["request:M5S3_wsh#1"], "VA11 a record with wsh_rem TOTAL_MAX + 1 (fabricated, deposit)");
killed!(s3_burn_0, ["request:M5S3_burn#0"], "VA11 a record with burn_rem -1 (fabricated, deposit)");
killed!(s3_burn_1, ["request:M5S3_burn#1"], "VA11 a record with burn_rem > wsh_rem (fabricated)");
killed!(s3_out_0, ["request:M5S3_out#0"], "VA11 a record with out_rem -1 (fabricated, deposit)");
killed!(s3_out_1, ["request:M5S3_out#1"], "VA11 a record with out_rem TOTAL_MAX + 1 (fabricated, deposit, engine only)");
killed!(s3_refmint, ["request:M5S3_refmint"], "VA11 a refund record with mint_rem > 0 (fabricated)");
killed!(s3_funder, ["request:M5S3_funder"], "VA11 a record with funder class 3, refunded to its P2SH form (fabricated, drained)");
// in1 >= out_rem: the next line requires in1 - out_rem >= held >= 0 (held = refund x dep_rem,
// dep_rem >= 0 by M5S3_dep).
redundant!(s3_value_0_redundant, ["request:M5S3_value#0"], "VA11 a record value below out_rem (fabricated)");
killed!(s3_value_1, ["request:M5S3_value#1"], "VA11 a refund record value with R_rc = -1 (fabricated)");
killed!(s3_value_2, ["request:M5S3_value#2"], "VA11 a record value with R_rc > RESERVE_MAX (fabricated)");
killed!(s3_draw_minus, ["request:M5S3_draw"], "VA3 request round t against the record of t - 1");
killed!(s3_draw_plus, ["request:M5S3_draw"], "VA3 request round t against the record of t + 1");
killed!(s3_draw_successor, ["request:M5S3_draw"], "VA4 request' (round t + 1) against record t");
killed!(s4_rem_dep, ["request:M5S4_rem"], "VA11 a deposit above the record's dep_rem");
killed!(s4_sh, ["request:M5S4_sh"], "VA1 settle: a fabricated record where sh = 0");
// S7's three requires are each implied by the other two plus S8_zero (as D3). Removed
// together, the position may leave the covenant.
redundant!(s7_auth_redundant, ["request:M5S7_auth"], "VA11 an extra covenant output (a position copy)");
redundant!(s7_idx_redundant, ["request:M5S7_idx"], "VA11 an extra covenant output (a position copy)");
redundant!(s7_cov_redundant, ["request:M5S7_cov"], "VA11 an extra covenant output (a position copy)");
killed!(s7_all, ["request:M5S7_auth", "request:M5S7_idx", "request:M5S7_cov"], "VA11 position without covenant binding");
// >= base outputs: S9 reads the payout output (index error without it).
redundant!(s8_count_0_redundant, ["request:M5S8_count#0"], "VA11 FINAL without its payout and change outputs");
killed!(s8_count_1, ["request:M5S8_count#1"], "VA11 an extra plain output");
killed!(s8_zero, ["request:M5S8_zero"], "VA11 the change under a fresh genesis covenant ID");
killed!(s9_record, ["request:M5S9_record"], "VA11 record' value + 1");
killed!(s9_cov2, ["request:M5S9_cov2"], "VA11 position value + 1");
// Consensus-equivalent guards (value + pay > 2^62 > MAX_SOMPI), killed in the engine.
killed!(s9_accrue_0, ["request:M5S9_accrue#0"], "VA11 ACCRUE: request value TOTAL_MAX - pay + 1 (engine only)");
killed!(s9_accrue_1, ["request:M5S9_accrue#1"], "VA11 ACCRUE request' value without the pay");
killed!(s9_refund_0, ["request:M5S9_refund#0"], "VA11 refund: request value TOTAL_MAX - s + 1 (engine only)");
killed!(s9_refund_1, ["request:M5S9_refund#1"], "VA11 refund payout + 1");
killed!(s9_final_0, ["request:M5S9_final#0"], "VA11 FINAL: request value TOTAL_MAX - pay + 1 (engine only)");
killed!(s9_final_1, ["request:M5S9_final#1"], "VA11 FINAL payout + 1");
killed!(s9_pay, ["request:M5S9_pay"], "VA11 PAY payout + 1");
killed!(s9_funder, ["request:M5S9_funder"], "VA11 the record's R refund - 1");
killed!(s10_owner_final, ["request:M5S10_owner"], "VA11 FINAL payout redirected");
killed!(s10_owner_refund, ["request:M5S10_owner"], "VA11 refund payout redirected");
killed!(s10_funder, ["request:M5S10_funder"], "VA11 the record's R refunded to another script");

// ================================================================ refresh (A7) of position, record, request

macro_rules! refresh_mutants {
    ($m:ident, $file:literal, $pre:literal, $obj:literal) => {
        mod $m {
            use super::*;
            killed!(rf1, [concat!($file, ":", $pre, "RF1")], concat!("VA13 ", $obj, ": a covenant-less copy as funding (at the copy, input 1)"));
            // A7.2 count is implied by RF2_zero (pigeonhole) or, for a covenant-less object,
            // by A7.4 (nothing authorized). Removed with RF2_zero, a second covenant input is
            // accepted.
            redundant!(rf2_count_redundant, [concat!($file, ":", $pre, "RF2_count")], concat!("VA13 ", $obj, ": a second covenant input"));
            redundant!(rf2_count_no_covenant_redundant, [concat!($file, ":", $pre, "RF2_count")], concat!("VA13 ", $obj, ": a covenant-less object, plain successor"));
            killed!(rf2_count_and_zero, [concat!($file, ":", $pre, "RF2_count"), concat!($file, ":", $pre, "RF2_zero")], concat!("VA13 ", $obj, ": a second covenant input"));
            killed!(rf2_fund_0, [concat!($file, ":", $pre, "RF2_fund#0")], concat!("VA13 ", $obj, ": no funding input"));
            // <= 1 + F_MAX: the RF2_zero loop `for(fi, 1, n, F_MAX)`.
            redundant!(rf2_fund_1_redundant, [concat!($file, ":", $pre, "RF2_fund#1")], concat!("VA13 ", $obj, ": nine funding inputs"));
            killed!(rf2_zero, [concat!($file, ":", $pre, "RF2_zero")], concat!("VA13 ", $obj, ": funding input with a covenant ID"));
            killed!(rf3_creation_0, [concat!($file, ":", $pre, "RF3_creation#0")], concat!("VA13 ", $obj, ": creation = -1 (unaccepted parent)"));
            // creation < DAA_MAX: creation + D_IDLE >= LOCK_TIME_THRESHOLD fails the compiled
            // range before CLTV (silverscript-lang/src/compiler/compile/statement.rs:375-388;
            // rusty-kaspa v2.1.0 crypto/txscript/src/lib.rs:84).
            redundant!(rf3_creation_1_redundant, [concat!($file, ":", $pre, "RF3_creation#1")], concat!("VA13 ", $obj, ": creation = DAA_MAX"));
            killed!(rf3_idle, [concat!($file, ":", $pre, "RF3_idle")], concat!("VA13 ", $obj, ": lock_time = creation + D_IDLE - 1"));
            // A7.4's three requires are each implied by the other two plus RF5_zero (output 1
            // carries the zero ID). Removed together, the successor may leave the covenant.
            redundant!(rf4_auth_redundant, [concat!($file, ":", $pre, "RF4_auth")], concat!("VA13 ", $obj, ": extra covenant output"));
            redundant!(rf4_idx_redundant, [concat!($file, ":", $pre, "RF4_idx")], concat!("VA13 ", $obj, ": successor without covenant binding"));
            redundant!(rf4_cov_redundant, [concat!($file, ":", $pre, "RF4_cov")], concat!("VA13 ", $obj, ": extra covenant output"));
            killed!(rf4_all, [concat!($file, ":", $pre, "RF4_auth"), concat!($file, ":", $pre, "RF4_idx"), concat!($file, ":", $pre, "RF4_cov")], concat!("VA13 ", $obj, ": successor without covenant binding"));
            // >= 1 output: A7.4 (one authorized output) and A7.6 (reads output 0).
            redundant!(rf5_count_0_redundant, [concat!($file, ":", $pre, "RF5_count#0")], concat!("VA13 ", $obj, ": no output (A7.4 rejects first)"));
            // <= 2 outputs: the RF5_zero loop `for(zo, 1, n, 1)`.
            redundant!(rf5_count_1_redundant, [concat!($file, ":", $pre, "RF5_count#1")], concat!("VA13 ", $obj, ": extra plain output"));
            killed!(rf5_zero, [concat!($file, ":", $pre, "RF5_zero")], concat!("VA13 ", $obj, ": the change under a fresh genesis covenant ID"));
            killed!(rf6_plus, [concat!($file, ":", $pre, "RF6")], concat!("VA13 ", $obj, ": value + 1"));
            killed!(rf6_minus, [concat!($file, ":", $pre, "RF6")], concat!("VA13 ", $obj, ": value - 1"));
        }
    };
}

refresh_mutants!(position_refresh, "position", "M5PO_", "position");
refresh_mutants!(record_refresh, "record", "M5RC_", "record");
refresh_mutants!(request_refresh, "request", "M5RQ_", "request");

// ================================================================ drawer (A5: checks 5, 13, 14, C2, 20)

// total_assets <= ASSETS_MAX: check 14's hAmt <= ASSETS_MAX - total_assets with hAmt >= 0.
// Removed together, a vault above ASSETS_MAX (no sales, V' = V) is drawn.
redundant!(dr5_assets_redundant, ["drawer:M5D5_assets"], "VA6 draw: total_assets = ASSETS_MAX + 1, no sales, nothing pending (fabricated vault, engine only)");
killed!(dr5_assets_and_14, ["drawer:M5D5_assets", "drawer:M5D14_assets"], "VA6 draw: total_assets = ASSETS_MAX + 1, no sales, nothing pending (fabricated vault, engine only)");
killed!(dr5_shares, ["drawer:M5D5_shares"], "VA6 draw: total_shares = SHARES_MAX + 1, nothing pending (fabricated vault)");
killed!(dr5_wsh, ["drawer:M5D5_wsh"], "VA6 draw: pend_wsh = total_shares + 1, wc = W (fabricated vault)");
killed!(dr14_assets, ["drawer:M5D14_assets"], "VA6 V' = ASSETS_MAX + 1, nothing pending (fabricated vault, engine only)");
killed!(c2_sum, ["drawer:M5C2_sum"], "VA6 V' + D > TOTAL_MAX (fabricated vault, engine only)");
redundant!(c2_slack_redundant, ["drawer:M5C2_slack"], "VA14 J fixed from V' + D");
// wc >= 0: with wc < W the next check requires out(wc + 1) > slack; out(0) = 0 and slack >= 0
// (C2_slack), so wc = -1 fails M5C2_max.
redundant!(c2_range_0_redundant, ["drawer:M5C2_range#0"], "VA5 wc = -1");
killed!(c2_range_1, ["drawer:M5C2_range#1"], "VA5 wc = W + 1 within the slack");
killed!(c2_fits, ["drawer:M5C2_fits"], "VA5 wc + 1 (vault' total_assets < L')");
killed!(c2_fits_capped, ["drawer:M5C2_fits"], "VA5 wc = W when capped");
killed!(c2_max, ["drawer:M5C2_max"], "VA5 a non-maximal wc (wc - 1)");
killed!(dr20_record, ["drawer:M5D20_record"], "VA8 record value + 1");

/// M5D13_rec is an assignment (nCov += 1 when something is pending): without it the drawer
/// rejects every valid converting draw (the record is an authorized covenant output that
/// nCov does not count), so every converting positive (VP4-VP9, VP12 grid, VP14) fails.
#[test]
fn dr13_rec_killed_positive() {
    let good = d_both();
    let gc: Case5 = good.clone().into();
    accept5("draw with conversion (committed)", &gc, &gc.build());
    let t = mutant5(&["drawer:M5D13_rec"]);
    let mut c = gc.clone();
    c.retarget(&t);
    let e = reject_at(&c.build_with(t), DRW5);
    println!("mutant M5D13_rec: valid converting draw rejected at the drawer: {e:?}");
}

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

/// `case` pointed at the look-alike templates `fake`, built and accepted by every input.
fn accept_under(name: &str, case: impl Into<Case5>, fake: &Arc<Templates>) -> Built {
    let mut c: Case5 = case.into();
    c.retarget(fake);
    let b = c.build_with(fake.clone());
    accept_m4(name, &b);
    b
}

/// D8 and W9 (validateOutputState: the vault's own template): a look-alike vault requests
/// into its own look-alike successor (accepted); the committed vault rejects vault' under
/// the look-alike template.
#[test]
fn template_vault_successor_is_own_template() {
    let fake = lookalike(Sil::Vault);
    accept_under("look-alike vault: request_deposit", dep(), &fake);
    accept_under("look-alike vault: request_withdraw", wd(owner()), &fake);
    let c = dep();
    let mut b = c.build();
    b.outputs[0].script_public_key = fake.vault_spk(&c.expected().vault);
    reject_at(&b, VLT0);
    let c = wd(owner());
    let mut b = c.build();
    b.outputs[0].script_public_key = fake.vault_spk(&c.expected().vault);
    reject_at(&b, VLT0);
}

/// D9 and W10 (validateOutputStateWithTemplate under the vault's request_tpl state field): a
/// vault whose request_tpl is a look-alike request's hash creates that look-alike request
/// (accepted); the committed vault state (request_tpl = the committed hash) rejects the same
/// look-alike request with the look-alike's prefix and suffix as arguments.
#[test]
fn template_vault_request_under_request_tpl() {
    let fake = lookalike(Sil::Request);
    accept_under("request_deposit with vault request_tpl = a look-alike request", dep(), &fake);
    accept_under("request_withdraw with vault request_tpl = a look-alike request", wd(owner()), &fake);
    let c = dep();
    let mut b = c.build();
    b.outputs[1].script_public_key = fake.request_spk(&c.expected().request);
    let a = b.entry_args_mut(VLT0);
    a[DEP_ARG_REQ_PREFIX] = ArtifactValue::Bytes(fake.request.prefix.clone());
    a[DEP_ARG_REQ_SUFFIX] = ArtifactValue::Bytes(fake.request.suffix.clone());
    reject_at(&b, VLT0);
    let c = wd(owner());
    let mut b = c.build();
    b.outputs[1].script_public_key = fake.request_spk(&c.expected().request);
    let a = b.entry_args_mut(VLT0);
    a[WD_ARG_REQ_PREFIX] = ArtifactValue::Bytes(fake.request.prefix.clone());
    a[WD_ARG_REQ_SUFFIX] = ArtifactValue::Bytes(fake.request.suffix.clone());
    reject_at(&b, VLT0);
}

/// W3 (readInputStateWithTemplate under the vault's position_tpl) and W11
/// (validateOutputStateWithInputTemplate from input 1): a vault whose position_tpl is a
/// look-alike position's hash withdraws from that look-alike, position' following it
/// (accepted); the committed vault rejects the look-alike position (the VA10 strike) and
/// position' under the look-alike template while input 1 is the committed position.
#[test]
fn template_vault_position_under_position_tpl() {
    let fake = lookalike(Sil::Position);
    accept_under("request_withdraw with vault position_tpl = a look-alike position", wd(owner()), &fake);
    atk5("VA10 a position under a look-alike position template").strike();
    let c = wd(owner());
    let mut b = c.build();
    b.outputs[2].script_public_key = fake.position_spk(c.expected().position.as_ref().unwrap());
    reject_at(&b, VLT0);
}

/// PO4 (readInputStateWithTemplate under VAULT_TPL): a position compiled with a look-alike
/// vault's hash as VAULT_TPL serves that look-alike vault (accepted); the committed position
/// rejects a look-alike vault at input 0 (the VA10 strike).
#[test]
fn template_position_leg_reads_vault_under_vault_tpl() {
    let fake = lookalike(Sil::Vault);
    accept_under("position VAULT_TPL = a look-alike vault's hash", wd(owner()), &fake);
    atk5("VA10 the position leg under a look-alike vault").strike();
}

/// S3 (readInputStateWithTemplate under RECORD_TPL) and S11 (record' under input 1's
/// template): a request compiled with a look-alike record's hash settles against that
/// look-alike, record' following it (accepted); the committed request rejects the
/// look-alike record (the VA3 strike) and record' under the look-alike template while input
/// 1 is the committed record.
#[test]
fn template_settle_reads_record_under_record_tpl() {
    let fake = lookalike(Sil::Record);
    accept_under("settle with RECORD_TPL = a look-alike record's hash", st_dep(), &fake);
    atk5("VA3 a look-alike record template").strike();
    let c = st_dep();
    let mut b = c.build();
    b.outputs[0].script_public_key = fake.record_spk(&c.expected().record);
    reject_at(&b, REQ);
}

/// S12 (validateOutputStateWithTemplate under POSITION_TPL): a request compiled with a
/// look-alike position's hash mints that look-alike position (accepted); the committed
/// request, same state and arguments (the look-alike's prefix and suffix), rejects it.
#[test]
fn template_settle_position_under_position_tpl() {
    let fake = lookalike(Sil::Position);
    let g = accept_under("settle with POSITION_TPL = a look-alike position's hash", st_dep(), &fake);
    let mut b = g.clone();
    swap_template(&mut b, REQ, &tpl().request);
    let e = reject_at(&b, REQ);
    println!("committed request rejects a look-alike position: {e:?}");
}

/// S13 (validateOutputState: the request's own template) and RL4 (the record reads the
/// request under its request_tpl state field): a look-alike request settles PAY into its own
/// look-alike request' against a record whose request_tpl is its hash (accepted); the
/// committed request rejects request' under the look-alike template, and the committed
/// record state rejects the look-alike request (the VA12 strike).
#[test]
fn template_settle_request_successor_and_record_leg() {
    let fake = lookalike(Sil::Request);
    accept_under("look-alike request PAY into its own successor, record request_tpl = its hash", st_pay(), &fake);
    accept_under("look-alike request deposit-convert, record request_tpl = its hash", st_dep(), &fake);
    let c = st_pay();
    let mut b = c.build();
    let x = c.expected();
    b.outputs[x.cov2_output.unwrap()].script_public_key = fake.request_spk(x.request.as_ref().unwrap());
    reject_at(&b, REQ);
    atk5("VA12 a look-alike request at input 0").strike();
}

/// Check 26 (validateOutputStateWithTemplate under the vault's record_tpl state field): a
/// vault whose record_tpl is a look-alike record's hash gets that look-alike record from a
/// converting draw (accepted); the committed vault state rejects the same look-alike record
/// with the look-alike's prefix and suffix as arguments.
#[test]
fn template_draw_record_under_record_tpl() {
    let fake = lookalike(Sil::Record);
    accept_under("draw with vault record_tpl = a look-alike record's hash", d_both(), &fake);
    let c = d_both();
    let x = c.expected();
    let mut b = c.build();
    b.outputs[x.record_output().unwrap()].script_public_key = fake.record_spk(x.record.as_ref().unwrap());
    let a = b.entry_args_mut(DRW5);
    a[ARG_RECORD_PREFIX] = ArtifactValue::Bytes(fake.record.prefix.clone());
    a[ARG_RECORD_SUFFIX] = ArtifactValue::Bytes(fake.record.suffix.clone());
    reject_at(&b, DRW5);
}

// ================================================================ coverage

/// Every M5 `// mut:` tag line (vault.sil and drawer.sil: tags starting `M5`; request.sil,
/// record.sil, position.sil: all) is named above: a single-line tag as "file:TAG", each line
/// of a multi-line tag as "file:TAG#n" (the refresh tags through `refresh_mutants!`).
#[test]
fn every_m5_tag_is_tested() {
    let me = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/m5_mutants.rs")).expect("own source");
    let (mut tags, mut lines) = (0, 0);
    for (f, file) in [("vault.sil", "vault"), ("drawer.sil", "drawer"), ("request.sil", "request"), ("record.sil", "record"), ("position.sil", "position")] {
        let src = std::fs::read_to_string(harness::covenant_path(f)).expect("source");
        let mut count: BTreeMap<String, usize> = BTreeMap::new();
        for l in src.lines() {
            for t in line_tags(l) {
                let m5 = !matches!(file, "vault" | "drawer") || t.starts_with("M5");
                if m5 {
                    *count.entry(t.to_string()).or_default() += 1;
                }
            }
        }
        for (t, n) in count {
            tags += 1;
            lines += n;
            // refresh_mutants!(<mod>, "file", "M5XX_", ..) writes concat!($file, ":", $pre, "RFn...").
            let rf = ["M5PO_", "M5RC_", "M5RQ_"].iter().find(|p| t.starts_with(**p)).map(|p| (*p, &t[p.len()..]));
            let generated = rf.is_some_and(|(p, _)| me.contains(&format!("_refresh, \"{file}\", \"{p}\"")));
            let named = |s: &str| {
                me.contains(&format!("\"{file}:{s}\"")) || (generated && me.contains(&format!("$pre, \"{}\")", &s[rf.unwrap().0.len()..])))
            };
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
    // The other direction: every "file:TAG" / "file:TAG#n" named in this file is an M5 tag
    // line of that file (so the mutant set equals the tag set).
    let mut all: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (f, file) in [("vault.sil", "vault"), ("drawer.sil", "drawer"), ("request.sil", "request"), ("record.sil", "record"), ("position.sil", "position")] {
        let src = std::fs::read_to_string(harness::covenant_path(f)).expect("source");
        for l in src.lines() {
            for t in line_tags(l) {
                *all.entry((file.to_string(), t.to_string())).or_default() += 1;
            }
        }
    }
    for chunk in me.split('"').skip(1).step_by(2) {
        let Some((file, rest)) = chunk.split_once(':') else { continue };
        if !["vault", "drawer", "request", "record", "position"].contains(&file) || rest.contains(' ') || rest.is_empty() {
            continue;
        }
        let (tag, n) = match rest.split_once('#') {
            Some((t, n)) => (t, n.parse::<usize>().ok()),
            None => (rest, None),
        };
        let c = all.get(&(file.to_string(), tag.to_string())).copied().unwrap_or(0);
        assert!(c > 0 && n.is_none_or(|n| n < c), "named mutant {chunk} is not a tag line of {file}.sil");
    }
    println!("M5 tags: {tags}, tagged lines: {lines}");
    assert_eq!((tags, lines), (EXPECTED_TAGS, EXPECTED_LINES), "M5 tag count changed: update this test and the handoff table");
}

const EXPECTED_TAGS: usize = 153;
const EXPECTED_LINES: usize = 212;
