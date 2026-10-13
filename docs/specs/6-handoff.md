# M6 handoff (working file; deleted at M6 recording)

Spec: `docs/specs/6.md` (approved 2026-10-13). Plan: six stages; see "Stages" below. Each stage agent reads this file first and appends to "Log" before finishing: what it changed, decisions (S<stage>-<n> IDs), test commands run and results, open items for the next stage.

## Rules for every stage
- Read `CLAUDE.md`, the spec, and this file. Follow the M5 test method (one change per attack; `strike` = named input rejects with a script error, good tx accepted).
- Never weaken an existing test to make it pass; if a fixture must change because of the M6 constants, change values, keep intent, and log it (it becomes a spec Correction).
- Never state Kaspa facts from memory: cite `refs/` file:line.
- Stop rule D90: if any final shape exceeds 90 % of any mass, or the drawer hits the stack limit, STOP and report; do not restructure.
- Low memory on Windows: add `-j 2` to cargo if you see os error 1455.
- Do not commit. Do not touch `docs/design.md`.

## Stages
1. Tests first (scripts untouched): Rust reference constants + fixture rescaling + new tests; show new tests failing for the right reason.
2. Implement scripts, genesis, draw.rs cleanup, test tooling; full `cargo test` green; ATTRIB/PREFLIGHT runs.
3. `m6_measure`, `m6_conflict`, K_MAX variant plumbing, `docs/costs.md` M6 draft.
4. tn10 `conflict-m6` + re-runs (orchestrator decides who runs it).
5. Review (orchestrator).
6. Record (orchestrator).

## Explorer findings (2026-10-13, verified in source)

### Constants and fixtures that change
- `game.rs:739` SHARE_UNIT 1_000 → 1; `:744` MIN_DEPOSIT 100_000_000 → 1_000_000_000 (doc :743); add W_MIN_VALUE = 1_000_000_000 and `w_min(total_assets, total_shares)`.
- `genesis.rs:49-53` D_CLAIM_FLOOR → D_FLOOR = 13_000_000; check `:224-226`; unit test kp19 `:731-773` (needle "D_CLAIM_FLOOR" at :768); SHARE_UNIT at `:93, 159-160, 297-300` (`:297` range("vault total_assets", SHARE_UNIT, …) becomes V0_MIN = 100_000_000).
- `buy.rs:32` TEST_DEPOSIT = D_FLOOR (doc :29-31). `buy.rs:200, 355` dummy states (hash ignores values; leave).
- SHARE_UNIT also used: `draw.rs:330`, `vault.rs:108, 534`, `tests/common/m5.rs:2115` (S_UNIT, unused), `m5_game.rs:556-632`, tn10 `cases.rs` vault_m5.
- Breaks numerically:
  - `m5_game.rs:207-210` vp12_convert_boundaries (convert(10^12−1, 0, MIN_DEPOSIT, 0, 0) now converts with mint 1).
  - `common/m5.rs:1091-1103` `d_refund_price` (V = 2,000 KAS, S = 199, D = 3 KAS now converts; also D = 3 KAS < MIN_DEPOSIT). Feeds `m5_vault.rs:346` (VP9), `:444` (VP12), VA1 at `common/m5.rs:1117`.
  - `m5_measure.rs:174-187` RefundCap (S = 1,000,000, D = 30 KAS: likely converts now).
  - `m5_game.rs:582-585` (V0 = 10^6+999 → S0 asserts), `:604` (V0 = SHARE_UNIT − 1), `:628` (V0 = SHARE_UNIT accepted).
  - `m5_vault.rs:552` deposit 5 KAS (< 10 KAS); `:417`, `:477` withdraw_case(0, 1_000, …) (below wmin).
  - `vault.rs:323-340` `WithdrawCase::new_with`: position 5,000,000 shares, w = 1,000,000 — below wmin (10^9 at fixture V = S = 10^15). Base of ~80 `wd(...)` attacks; rescale (position 5·10^10, w = 10^10). Unit test `vault.rs:773`.
  - `m5_vault.rs:322-329` VP8 tolerance 2 × price — re-verify at price ≈ 1.
  - `common/m5.rs:2032-2035` d_big_dep 10^11 shares is now ~10 KAS; re-verify the J fixes still differ.
  - tn10 `cases.rs:948-967` vault_m5: d = 2 KAS, w = s0/100 → both below the new minimums. V0 = 60 KAS: wmin = 1,000,000,166 shares.
- Labels "d = 0.11 KAS": `m1_measure.rs:190`, `m4_measure.rs:15,171,191,245`, tn10 `cases.rs:208`. `m4_measure.rs:177` asserts ticket_value = 2 × D_CLAIM_FLOOR. `m4_measure.rs:258` M4-d b = 1 referral asserted only ≤ 100 % → tighten to ≤ 90 %. `m4_claim.rs:600-617` D_CLAIM_FLOOR rename/needle. `m3_game.rs:43` literal d 11_000_000 (cosmetic).
- No test asserts exact masses.

### Vault / SilverScript
- `vault.sil` W1 bounds total_assets ≤ ASSETS_MAX (:260-261), total_shares ≤ SHARES_MAX (:262-263). VS, VA NOT declared in vault.sil (drawer.sil:73-74 has them). W4 at :303-306, W5 at :308; header label list :23-26.
- `require(a || b)` works (OpBoolOr, no short-circuit; `refs/silverscript/silverscript-lang/src/compiler/compile/expression.rs:408-411`; examples `refs/silverscript/silverscript-lang/tests/apps/chess/chess_castle.sil:56`).
- Attribution label parsing: `strip_prefix("// ==== check ").split(':').next().trim()` (`common/m5.rs:452`) → write the marker as `// ==== check W4b: …`. `group5` (`common/m5.rs:401-406`) trims trailing digits only, so "W4b" would be its own group: fix it to strip a trailing lowercase suffix first, so W4b truncation sits after W4 in the W group.
- Preflight model `common/m5_spec.rs` `withdraw()` :367-411, W4 at :384, W5 at :386: add `check(w >= wmin || w == shares, "vault:W4b")`.
- `m3_mutants.rs:717-719` keeps vault tags not starting with "M5" → add `&& !t.starts_with("M6")`; count 88 at :733 unchanged.
- `m5_mutants.rs`: killed/redundant/mutant5/strip/line_tags private (:42-178); `every_m5_tag_is_tested` :688-750 counts M5* only (158/219).

### Vault builders
- `vault.rs`: `DepositCase` :144-260 (d public, default 10 KAS), `WithdrawCase` :272-447 (w, position public; `whole()` :354; `expected()` :383). No builder policy today. Settlement Lie `SettleLie` :461-470.
- Atk5 infra `common/m5.rs`: `Atk5::new(id, input, check, good).wd(|c| …)` (:174, example :1287), `run5`, `all_atks5()` :2079, `atk5(id)` :2087, bases `dep()`, `wd(key)`, `wd_whole(key)` :952-963, `VLT0 = 0`, `POS = 1`.

### draw.rs dead M4 branch (D89)
- Module doc :16-19; `ARG_OWN_M4` :85; `SIG_DRAW_M4` :88; `drawer_is_m5` :96-106; `args` :738-746 falls through to M4; `args_m4` :749-773 is ALSO the base of the M5 args → inline, don't delete; `build_with` :804 else-branch bare "draw_test_seed"; `from_tx` 22-push arm :977-984 and :1076-1078.
- Callers: `m5_vault.rs:170-176`, `common/m5.rs:266-271` (`not_yet_written`), `common/m5_spec.rs:306, 569-570`.

### Measurement infra
- `Templates::compile_sources(b_max, &Sources)` `buy.rs:194-239` (full chain); `Sources::committed(b_max)` :133-149; `Templates::get(b_max)` caches by b_max only; `resize_ticket_arrays` :95-99; `draw::compile_drawer` draw.rs:46; M3-c `drawer_variant` m3_measure.rs:187-194; M4-a `ticket_variant` m4_measure.rs:67-75.
- K_MAX = 16 hard-coded: `state.rs:27`; `state.rs:370-374` `roots_bytes` panics for K > 16 (callers close.rs:291, draw.rs:398, 1111, claim.rs:307); dummy roots `buy.rs:396, 459`; `draw.rs:289, 452, 996-1000`; `close.rs:458`. .sil: `controller.sil:55`, `drawer.sil:37`, `ticket.sil:35` constant lines; `byte[512]` at controller.sil:144,570; marker.sil:15,40; drawer.sil:136,180; pool.sil:23,72; ticket.sil:71. Controller zero buffer 512 B (`controller.sil:317-324`) fine at k = K_MAX.
- `CloseCase::new_with(k, t)` close.rs:110-191, `build_with(Arc<Templates>)` :345.
- Mass helpers: `harness::masses`, `priority_fee` (harness/src/lib.rs:129-160); relay floor exists only test-locally (harness/tests/common/mod.rs:118; tn10 probe.rs:248).
- Reuse: M2-c `m2_measure.rs:151-272` (`close_case_c`, `close_storage`, `worst_storage`, `storage_sup`, `r_bound`); M5-c `m5_measure.rs:239-310`; DP21 `m3_measure.rs:62` `worst`, `m4_measure.rs:270`, `m5_measure.rs:121`.

### tn10 / mempool
- Probes: `cases.rs` fns per probe; `round1_drawn_with(ctx, tag, Option<VaultRequests>)` :461; dispatch `main.rs:105-141`; submit `probe.rs:184` (`submit_transaction(.., false)` = RbfPolicy::Forbidden). Replacement: `RpcApi::submit_transaction_replacement` (`refs/rusty-kaspa-v2.1.0/rpc/core/src/api/rpc.rs:217-227`), RbfPolicy::Mandatory, needs exactly one double spend and strictly higher feerate (`mining/src/mempool/replace_by_fee.rs:118-168`); evicts descendants (:124-129).
- Chained spends: mempool populates in-pool parents with UNACCEPTED_DAA_SCORE (`mining/src/mempool/populate_entries_and_try_validate.rs:26-41`) → script sees −1 (`crypto/txscript/src/opcodes/mod.rs:1124-1130`); shard check 6 (`shard.sil:151`) has no ≥ 0 guard, so chained buys pass in the mempool; vault D10/W12 require creation ≥ 0.
- `merged()` cases.rs:362-398 (prints elapsed, does not return it); `wait_mined` probe.rs:213-229.

## Log

### Stage 1 (2026-10-13): tests first, scripts untouched

No `covenants/*.sil` file was edited. Nothing committed.

**Files changed**
- `crates/lottery/src/game.rs`: SHARE_UNIT 1_000 -> 1; MIN_DEPOSIT 10^8 -> 10^9; new `W_MIN_VALUE = 10^9` and `pub fn w_min(total_assets, total_shares) -> i64` (exact A4 W4b mirror); unit test `lp4_w_min_grid_and_value_bound` (grid + 100,000 random (A, S, w >= wmin) in u128, existing splitmix `Rng`, no new deps).
- `crates/lottery/src/genesis.rs`: `D_CLAIM_FLOOR = 11_000_000` -> `D_FLOOR = 13_000_000` (message now `d = .. < D_FLOOR = .. (D81)`); new `V0_MIN = 100_000_000`; kp19 needles renamed (also refuses d = 11,000,000); new unit test `la6_genesis_below_the_floors`; the G2_MAX boundary case `set_vault(ASSETS_MAX)` -> `set_vault(SHARES_MAX)` (fixture 8). `validate` still has NO V0_MIN / SHARES_MAX refusal (Stage 2).
- `crates/lottery/src/buy.rs`: `TEST_DEPOSIT = crate::genesis::D_FLOOR as u64` (was 11_000_000).
- `crates/lottery/src/vault.rs`: `WithdrawCase::new_with` and `fixture_record` (fixtures 1, 2).
- `crates/lottery/tests/common/m6.rs` (new): M6 bases (`dep_min`, `wd_at_wmin`, `wd_whole_small`, `wd_q1_whole`, `wd_q0_one`, `wd_extreme_at_wmin`, `d_at_price`, `d_guard_edge`, `la4_bases`), attacks `la1()..la5()` as `Atk5` data, `all_atks6`, `atk6(id)` (for Stage 2 `m6_mutants.rs`), `la3_no_arith`, `verdict`. `common/mod.rs`: `pub mod m6;`.
- `crates/lottery/tests/m6_attacks.rs` (new): LA1-LA5. `crates/lottery/tests/m6_limits.rs` (new): LP1-LP8.
- Fixture and label edits: `common/m5.rs`, `m5_vault.rs`, `m5_game.rs`, `m5_measure.rs`, `m4_measure.rs`, `m4_claim.rs`, `m3_game.rs`, `m1_measure.rs`, `crates/tn10/src/cases.rs` (label only).

**Fixture changes (old -> new, why; these become spec Corrections)**
1. `vault.rs` `WithdrawCase::new_with`: position 5,000,000 -> 5 x 10^10 shares, w 1,000,000 -> 10^10 (wmin = 10^9 at the fixture vault V = S = 10^15; value 10 of 50 KAS -> 100 of 500 KAS).
2. `vault.rs` `fixture_record`: S argument `FIXTURE_VAULT / SHARE_UNIT` -> `FIXTURE_VAULT / 1_000`, which keeps the M5 record exactly (price about 1,000 sompi per share, mint_rem 3,000,000 for 30 KAS). Why: at SHARE_UNIT = 1 the record had mint_rem = dep_rem, so the mutant test `m5_mutants::s4_rem_dep` (VA11 deposit = dep_rem + 1 under the mutant without `s <= c1`) was rejected by a later check (part = a1 + 1 > mint_rem), failing with "input 0 still rejects ... VerifyError". The explorers did not list it. NOT re-run after the fix (see Commands).
3. `common/m5.rs` `d_refund_price`: V 2,000 -> 20,000 KAS, D 3 KAS -> 3 x MIN_DEPOSIT = 30 KAS (S = 199 kept): mint floor(1.80) = 1 < 3, refund. At the old values D < MIN_DEPOSIT and mint 1 = ceil(D / 10^9) would convert. Feeds the VA1 draw attack, VP9, VP12 and LA5.
4. `m5_measure.rs` RefundCap: total_shares 1,000,000 -> 500,000, pend_wsh 950,000 -> 475,000 (mint floor(1.5) = 1 < 3; at 1,000,000 the mint 3 = ceil(30 KAS / 10 KAS) would convert). Passed in run 1.
5. `m5_game.rs` vp12_convert_boundaries: `convert(10^12 - 1, 0, MIN_DEPOSIT, ..)` -> `convert(10^13 - 1, ..)` (keeps the price at 10 x MIN_DEPOSIT per share); the exact-price case's comment is generalised (its code already used MIN_DEPOSIT).
6. `m5_game.rs` vp1_genesis_shares_position_templates: "S0 rounds down: V0 = 10^6 + 999 -> 1,000" -> "S0 = V0: V0 = V0_MIN + 999 -> V0_MIN + 999" (no rounding at SHARE_UNIT = 1).
7. `m5_game.rs` vp1_genesis_refusals: refusal "V0 = SHARE_UNIT - 1" -> "V0 = V0_MIN - 1" (needle "total_assets": Stage 2's message must contain it); accepted boundaries `[SHARE_UNIT, ASSETS_MAX]` -> `[V0_MIN, SHARES_MAX]` (S0 = V0 <= SHARES_MAX binds below ASSETS_MAX).
8. `genesis.rs` unit test, G2_MAX boundary: `set_vault(ASSETS_MAX)` -> `set_vault(SHARES_MAX)` (same reason as 7; it would break in Stage 2).
9. `m5_vault.rs`: vp11 and s7_request_at_vault_age `withdraw_case(0, 1_000, ..)` -> `withdraw_case(0, 10_000_000_000, ..)` (>= wmin, about 10^9); vp14 deposit 5 KAS -> 15 KAS (>= MIN_DEPOSIT).
10. `m4_measure.rs` M4-d: every `buy()` shape is now asserted <= 90 % storage (`buy_report(.., true)`, was `!(b == 1 && with_ref)`); header doc updated; M4-c `D_CLAIM_FLOOR` -> `D_FLOOR` (ticket' 0.26 KAS). Passed in run 1 at d = 0.13.
11. `m4_claim.rs` kp19: `D_CLAIM_FLOOR` -> `D_FLOOR` (import and needle). `m3_game.rs:43` d 11,000,000 -> 13,000,000 (cosmetic).
12. Labels: the `m1_measure.rs` M-c title prints `TEST_DEPOSIT` instead of "0.11"; the tn10 `cases.rs` buy-m1 label prints `c.shard.d`; the `common/m5.rs` d_big_dep doc (10^11 shares are now about 10 KAS, not about 10,000 KAS; VA14 still passes, so the three J fixes still differ).
Not changed, verified passing in run 1: VP8 tolerance 2 x price (m5_vault 15/15 at price about 1), d_big_dep values, settle bases, VP12 grid. tn10 `vault_m5` values are left for Stage 4.

**Decisions**
- S1-1: `w_min` returns 0 when q > S + VS (one share is worth more than W_MIN_VALUE, e.g. A = ASSETS_MAX, S = 0). That is the exact spec formula; W4's `w >= 1` is then the floor and the value bound still holds (the unit test asserts `wm >= 0` and `w_min(ASSETS_MAX, 0) == 0`). Reviewer: confirm this is intended.
- S1-2: the genesis d-floor refusal got the new constant and now names D81 (a rename, not a new refusal). So LP1's "d = D_FLOOR - 1 refused (D81)" and LA6's d part ALREADY PASS; LP1 and LA6 fail only on V0 < V0_MIN and S0 > SHARES_MAX.
- S1-3 (spec deviation, for Corrections): LA2's third case "at q = 1, w = S + VS - 1 < shares" cannot reach W4b. At q = 1, wmin = S + VS > total_shares, and W4 (`w <= ps.shares`, `w <= total_shares - pend_wsh`) rejects any w > S first. LA2(c) therefore uses the largest partial withdrawal W4 accepts, w = shares - 1 = S - 1 (expected `vault:W4b`). The literal case is kept as a separate attack expected at `vault:W4`, which passes now. LP3's q = 1 positive is the whole position.
- S1-4: the LA3 base is a fabricated vault with A = ASSETS_MAX, S = SHARES_MAX and the fixture position (5 x 10^10 shares): w = wmin = 10^9 (good) and wmin - 1 (bad). The value is > MAX_SOMPI, so it runs in the engine only (as VA6). `la3_no_arith` checks both transactions' input 0 for `is_engine_arithmetic`, and checks that the Rust mirror makes the same decision.
- S1-5: LP8 is a constant-mirror test: MIN_DEPOSIT in vault.sil, drawer.sil and request.sil; W_MIN_VALUE, VS and VA in vault.sil; VS and VA in drawer.sil, each compared with `game`. The "every M1-M5 test passes" part is the full `cargo test`. Stage 2 must declare `int constant VS = 1000;`, `int constant VA = 1;` and `int constant W_MIN_VALUE = 1000000000;` in vault.sil in exactly the `int constant NAME = N;` form.
- S1-6: LA4 covers both settle modes (convert `st_dep`, refund `st_refund`), each at MIN_DEPOSIT - 1 and at 1 KAS. LA5 has two bases: `d_guard_edge` (S = 199, V' + VA = 10^12, D = MIN_DEPOSIT + 1: mint 1 < 2) and `d_refund_price`.
- S1-7: LP5 uses `d_at_price(m, s, d)` (V is adjusted so that V' + VA = m exactly; asserted) with S = 4,000: m = MIN_DEPOSIT x 5,000 - 1 and the exact value give mint 1 (convert); + 1 gives mint 0 (refund). Each case is settled: a 1-share position, or a refund of s + R.
- S1-8: LP7 covers 19 standalone draw shapes (the M3 bases, round 1, the M5 conversion bases, the mint guard on both sides, d_big_dep, d_no_pools, d_guard_edge) plus two chain draws (nothing pending, then a deposit and a withdrawal pending). It passes now; it is the D89 regression for Stage 2.

**Commands run (from `crates/`) and results**
- `cargo test -p lottery -p tn10 --release --no-run -j 4`: builds (before the m6 files and the tn10 label edit).
- Run 1, `cargo test -p lottery -p harness --release --no-fail-fast -j 4`. It ran after the constant and fixture edits but BEFORE fixtures 2 and 8, the new unit tests and the m6 files. Every binary was ok except m5_attacks 19/21 (va1_zero_share_deposit, va11_settlement_outputs), m5_game 9/10 (vp1_genesis_refusals) and m5_mutants 258/261 (d5_min, s1_amount_0, s4_rem_dep). Counts: harness 0 + 3 + 3 + 5 + 2 + 6; lib 31; m1_attacks 25, m1_buy 14, m1_measure 4, m1_mutants 11, m2_attacks 40, m2_close 8, m2_game 2, m2_measure 5, m2_mutants 25, m2_params 17, m3_attacks 22, m3_draw 24, m3_game 13, m3_measure 7, m3_mutants 94, m4_attacks 21, m4_claim 25, m4_game 4, m4_measure 8, m4_mutants 211 (+1 ignored), m5_measure 5, m5_vault 15.
- `cargo test -p lottery --release --no-fail-fast --test m6_limits --test m6_attacks -j 4`: m6_attacks 1/5, m6_limits 4/8 (details below). `cargo test -p lottery --release --lib ...`: lib 32/33 (la6 fails, as expected; the lp4 unit test passes).
- `M5_ATTRIB=1 cargo test -p lottery --release --test m6_attacks la5 -- --nocapture --test-threads=1`: both LA5 attacks `expected drawer:20 | got drawer:20 | model Some(Err("drawer:20")) | OK`.
- A throw-away per-attack status test (since deleted) gave each Atk5's verdict: every LA1, LA2 (a, b, c), LA3 and LA4 (4 cases) attack gave "ATTACK ACCEPTED ... by input 0"; the literal LA2 q = 1 case and both LA5 attacks were STRUCK.
- Run 2, the full workspace `cargo test --release --no-fail-fast -j 4`: STOPPED by Claude Code for low system memory after harness (all ok), the lottery lib (32/33, la6 only) and m1_attacks (25/25). Not restarted, per instruction. So these were NOT re-verified after the last edits: `m5_mutants::s4_rem_dep` with fixture 2; the other binaries that use `fixture_record` (m5_attacks, m5_vault, m5_mutants); m5_game after fixture 7; and the tn10 compile after the label edit (tn10 compiled before it; the edit uses `c.shard.d`, a BuyCase field). Genesis after fixture 8 is verified: the lib run includes `genesis_refuses_bad_instances` passing.

**Expected to fail now (must pass after Stage 2), with current messages**
- m6_attacks `la1_deposit_below_min_deposit`: "ATTACK ACCEPTED (LA1 request_deposit D = MIN_DEPOSIT - 1) by input 0" (vault.sil MIN_DEPOSIT = 10^8).
- m6_attacks `la2_partial_withdrawal_below_wmin`: "ATTACK ACCEPTED (LA2 request_withdraw w = wmin - 1 (q >= 1)) by input 0" (no W4b).
- m6_attacks `la3_w4b_range_edges`: "ATTACK ACCEPTED (LA3 request_withdraw w = wmin - 1 at total_assets = ASSETS_MAX, total_shares = SHARES_MAX) by input 0"; `la3_no_arith` passed (no arithmetic error in either transaction).
- m6_attacks `la4_settle_deposit_below_min_deposit`: "ATTACK ACCEPTED (LA4 settle (deposit-convert): deposit request amount = MIN_DEPOSIT - 1 (fabricated)) by input 0" (request.sil MIN_DEPOSIT = 10^8).
- m6_limits `lp1_genesis_floors`: `LP1: genesis accepts ["V0 = V0_MIN - 1", "S0 = V0 = SHARES_MAX + 1"]` (the d part is refused correctly, S1-2).
- m6_limits `lp4_w_min_matches_the_script`: "3 of 90 grid points differ", all "expected rejected, the vault accepts": (A = S = 10^15, w = 999,999,999); (A = ASSETS_MAX, S = 10^15, w = 216,839, wmin 216,840); (A = ASSETS_MAX, S = SHARES_MAX, w = 999,999,999). No arithmetic errors.
- m6_limits `lp5_lock_out_headroom`: "M5 step `draw (price just below MIN_DEPOSIT per share)`: valid transaction rejected at input 0: VerifyError; model: Ok(())" (the old drawer C1 guard ceil(D / 10^8) = 10 > mint 1).
- m6_limits `lp8_constants_mirror_the_scripts`: MIN_DEPOSIT = 10^8 in vault.sil, drawer.sil and request.sil; W_MIN_VALUE, VS and VA = None in vault.sil.
- lib `genesis::tests::la6_genesis_below_the_floors`: `LA6: genesis accepts ["V0 = V0_MIN - 1", "V0 = 1", "S0 = V0 = SHARES_MAX + 1", "S0 = V0 = ASSETS_MAX"]`.
- m5_attacks `va1_zero_share_deposit` ("ATTACK ACCEPTED (VA1 request_deposit D = MIN_DEPOSIT - 1) by input 0") and `va11_settlement_outputs` ("ATTACK ACCEPTED (VA11 a deposit request below MIN_DEPOSIT (fabricated)) by input 0"): Rust MIN_DEPOSIT 10^9 vs scripts 10^8.
- m5_mutants `d5_min`, `s1_amount_0`: the same two attacks (killed! runs the strike first).
- m5_game `vp1_genesis_refusals`: "V0 = V0_MIN - 1: bad instance accepted" (fixture 7; Stage 2's V0 floor).
- Passing now by design: LA5 (the old guard is stricter), LP2, LP3, LP6, LP7 and `game::tests::lp4_*`.

**Open items for Stage 2**
1. First, re-run what run 2 did not reach: `cargo test -p lottery --release --no-fail-fast -j 2` (for memory). Confirm that `m5_mutants::s4_rem_dep` passes with fixture 2 and that nothing outside the list above fails. Then `cargo build -p tn10`.
2. `genesis::validate`: refuse V0 < V0_MIN and S0 > SHARES_MAX. Both messages must contain "D84" (LP1, LA6), and the V0 floor message must also contain "total_assets" (m5_game vp1 needle). S0 = V0 / SHARE_UNIT.
3. vault.sil: constants exactly as in S1-5; the W4b block with the marker `// ==== check W4b: ...` and the tag `M6W4b_min`. drawer.sil and request.sil: MIN_DEPOSIT = 1000000000.
4. `common/m5_spec.rs` withdraw(): add `check(w >= wmin || w == shares, "vault:W4b")` after W4 (PREFLIGHT for LA2 and LA3); the `group5` W4b suffix fix (handoff).
5. Spec Correction for S1-3 (the literal LA2 q = 1 case is W4's) and the fixture list above.
6. m6_mutants: `killed!(.., ["vault:M6W4b_min"], "LA2 request_withdraw w = wmin - 1 (q >= 1)")` plus the shares - 1 case; the ids are in `common::m6::la2()`; use `atk6(id)`.

### Stage 2 (2026-10-13): scripts, genesis, builder policy, D89 cleanup, test tooling

Nothing committed. `docs/design.md` untouched. No D90 stop: every script compiles (vault, drawer, request), no stack-limit error.

**Files changed**
- `covenants/vault.sil`: MIN_DEPOSIT 10^8 -> 10^9; new `int constant VS = 1000;`, `VA = 1;`, `W_MIN_VALUE = 1000000000;`; check W4b after W4, before W5, exactly as spec A4, marker `// ==== check W4b: w >= wmin or the whole position (M6, D86)`, tag `// mut:M6W4b_min`; header label list and constant comments updated; the `||` comment cites `refs/silverscript/silverscript-lang/src/compiler/compile/expression.rs:408-410` (OpBoolOr, both operands evaluated).
- `covenants/drawer.sil`: MIN_DEPOSIT -> 10^9; C1 comment "D + MIN_DEPOSIT - 1 <= 2^62 + 10^9 < 2^63 (check 5; M6 D85)"; constant comment. `covenants/request.sil`: MIN_DEPOSIT -> 10^9; comment "minimum deposit (10 KAS, M6 D85)".
- `crates/lottery/src/genesis.rs` `validate_with`: refuses V0 < V0_MIN ("vault total_assets V0 = .. < V0_MIN = .. (D84)") before the old range check, and S0 = V0 / SHARE_UNIT > SHARES_MAX ("vault S0 = .. > SHARES_MAX = .. (D84)") before the S0 equality check. Imports SHARES_MAX.
- `crates/lottery/src/vault.rs`: `WithdrawCase::w_min()`, `policy_ok()` (w >= game::w_min or w == position.shares), `checked() -> Result<Self, String>` (error names D86 and W4b); `build_with` does NOT refuse (tests force bad shapes through the public `w`). Module doc paragraph. Unit test `withdraw_policy_mirrors_w4b`.
- `crates/lottery/src/draw.rs` (D89): `args_m4` inlined into `args` (23 arguments: wc, record_prefix, record_suffix before own_daa); removed `ARG_OWN_M4`, `SIG_DRAW_M4`, `drawer_is_m5`, the bare `draw_test_seed` entry branch, the 22-push arm of `RoundResult::from_tx` and its "M4 with something pending" error; module and `from_tx` docs updated.
- Callers: `tests/m5_vault.rs` (vp4: `from_tx` asserted unconditionally); `tests/common/m5.rs` `not_yet_written` (drawer rejections always count; same behaviour as before, since `drawer_is_m5` was always true); `tests/common/m5_spec.rs` (dispatch only on `SIG_DRAW_M5`; wc and record prefix/suffix always read from the arguments; the M4 fallbacks removed).
- Test tooling: `common/m5.rs` `group5` strips a trailing lowercase suffix before the digits ("W4b" -> "W"); `common/m5_spec.rs` `withdraw()` adds `check(w >= w_min(A, S) || w == shares, "vault:W4b")` between W4 and W5 (+ header line); `m3_mutants.rs` M3 pin also drops `M6*` tags (count 88 unchanged).
- New `tests/common/mutants.rs`: `parse`, `line_tags`, `strip`, `field`, `hash`, `mutant5`, `killed` / `redundant` (by M5 id) and `killed_atk` / `redundant_atk` (by `&Atk5`), moved from m5_mutants.rs (only `pub` added and the `&Atk5` split). `common/mod.rs`: `pub mod mutants;`. `m5_mutants.rs` uses it (macros stay in the file; unused imports removed).
- New `tests/m6_mutants.rs`: `killed6!(m6w4b_min, ["vault:M6W4b_min"], "LA2 request_withdraw w = wmin - 1 (q >= 1)")`, `killed6!(m6w4b_min_whole_small, ..., "LA2 request_withdraw w = shares - 1 < wmin (whole position below wmin)")` (both via `atk6`), and `every_m6_tag_is_tested` (scans every `covenants/*.sil`: exactly 1 M6 tag on 1 line, each named).
- `tests/common/m6.rs`: `wd_wmin0` base and `m5_rebased()` (S2-1), added to `all_atks6`; `tests/m6_attacks.rs`: test `m5_attacks_rebased_for_w4b` (strike), header note.

**Decisions**
- S2-1 (Correction, M5 mutant tests): with W4b in place, two M5 mutants stopped being killed at the fixture base because W4b also rejects the attack there: `m5_mutants::w4_min` (VA6 w = 0: 0 < wmin = 10^9) and `m5_mutants::w1_shares_1` (VA6 S = SHARES_MAX + 1: S + VS inflates wmin to about 4.6 x 10^12 > w = 10^10). Neither line became redundant: W4's `w >= 1` is the only floor when wmin = 0 (S1-1, q > S + VS), and W1's S <= SHARES_MAX is the only guard for a whole-position withdrawal (W4b's second clause). Both mutant tests are re-targeted (same test names, same tags) to one-change attacks on bases where W4b passes: "VA6 request_withdraw w = 0 at wmin = 0 (M6 rebase)" (fixture A = 10^15, S = 500,000, position 100,000, good w = 10,000, wmin = 0) and "VA6 request_withdraw whole position: total_shares = SHARES_MAX + 1 (fabricated vault, M6 rebase)" (base `wd_whole`). The original VA6 attacks stay in m5_attacks unchanged and are still struck at W4 / W1 (ATTRIB OK). Both rebased attacks are struck in m6_attacks (expected vault:W4, vault:W1; ATTRIB and PREFLIGHT OK). m5_mutants stays at 261 tests, all passing.
- S2-2: the genesis V0 floor is checked BEFORE `range("vault total_assets", ..)` and S0 <= SHARES_MAX AFTER it, so V0 = 0 and V0_MIN - 1 get the D84 message (contains "total_assets"), V0 = ASSETS_MAX + 1 still gets the range message (contains "total_assets"), and V0 in (SHARES_MAX, ASSETS_MAX] gets the S0 message (D84).
- S2-3: `policy_ok` checks only the W4b rule, not W4's other bounds (w >= 1, w <= shares, w <= S - pend_wsh); `checked()` is the D86 policy helper (plan Correction: a helper, not a refusal in `build_with`). tn10 does not use it yet (Stage 4 rescales vault_m5).

**Commands run (from `crates/`) and results**
- `cargo test --workspace --release --no-fail-fast -j 2` (release, all crates): exit 0. harness lib 0, covenant_smoke 3, m0_limits 3, m0_mass 5, m0_merkle 2, m0_shapes 6; lottery lib 34 (33 + the policy unit test; la6 passes); m1_attacks 25, m1_buy 14, m1_measure 4, m1_mutants 11, m2_attacks 40, m2_close 8, m2_game 2, m2_measure 5, m2_mutants 25, m2_params 17, m3_attacks 22, m3_draw 24, m3_game 13, m3_measure 7, m3_mutants 94, m4_attacks 21, m4_claim 25, m4_game 4, m4_measure 8, m4_mutants 211 (+1 ignored), m5_attacks 21, m5_game 10, m5_measure 5, m5_mutants 261, m5_vault 15, m6_attacks 6, m6_limits 8, m6_mutants 3; tn10 0. (An earlier full lottery run, before S2-1, had m5_mutants 259/261: w1_shares_1, w4_min.)
- `M5_ATTRIB=1 cargo test -p lottery --release -j 2 --test m5_attacks --test m6_attacks -- --nocapture --test-threads=1`: 21 + 6 passed; 344 ATTRIB lines, 0 not OK. W4b: LA2 (3 attacks) and LA3 `expected vault:W4b | got vault:W4b | model Some(Err("vault:W4b")) | OK`; the literal LA2 q = 1 case `expected vault:W4 | got vault:W4 | OK`; LA1 and VA1 `vault:D5` OK; LA4 (4) and VA11 "below MIN_DEPOSIT" `request:S1` OK; LA5 `drawer:20` OK.
- `M5_PREFLIGHT=1 cargo test -p lottery --release -j 2 --test m5_attacks --test m5_vault --test m6_attacks -- --nocapture --test-threads=1`: 21 + 15 + 6 passed; 344 PREFLIGHT lines, 0 not OK (model and script agree, W4b included).
- `M3_ATTRIB=1 ... --test m3_attacks` (262 lines) and `M4_ATTRIB=1 ... --test m4_attacks` (274 lines, all OK): identical (sorted) to a baseline run in a scratch git worktree with HEAD's covenants (= the Stage 1 scripts) and the current crates: 0 differing lines. Worktree removed.
- `cargo check -p lottery --no-default-features --features mainnet,test-seed`: FAILS as required, exit 101, `error: TEST-ONLY seed cannot be built for mainnet` at `lottery\src\lib.rs:7:1`.
- `cargo build -p tn10`: ok.
- `cargo test -p lottery --release --test m5_measure m5a_redeem_sizes -- --nocapture`: vault redeem **3,422 B** (prefix 1, suffix 3,266, state 155), was 3,380 (costs.md M5-a): +42 B. Drawer 65,234 and request 15,479 unchanged (10^8 and 10^9 are both 4-byte script numbers).

**Open items for Stage 3**
1. costs.md M6-a / M6-i: vault redeem 3,422 B; measure `request_withdraw` partial at the W4b boundary (the W4b block adds two divisions to the vault's execution).
2. Spec Corrections to record (Stage 6): S2-1 (two M5 mutant tests re-targeted), S2-3 (builder policy is a helper, not a refusal in `build_with`), plus Stage 1's list.
3. tn10 `vault_m5` still uses d = 2 KAS and w = s0 / 100 (below both minimums); Stage 4 rescales it and can use `WithdrawCase::checked()`.
4. Reviewer (Stage 5): W4's `w >= 1` and W1's `S <= SHARES_MAX` are no longer the only guards at typical states (W4b overlaps them); S2-1 names the states where each still is.

### Stage 3a (2026-10-13): `m6_measure` and plumbing — **STOPPED by D90 (genesis storage 92.1 %)**

Nothing committed. `docs/design.md` and `covenants/` untouched. `docs/costs.md` NOT written (stop rule). No parameter changed.

**D90 STOP (measured).** `m6a_every_transition_within_90`: genesis at K = 16 (fixture `GenesisCase::new_with(16, ..)`: V0 = 10,000,000 KAS, acc 2 KAS, pot 0, R = 1.69 KAS, one funding input of GENESIS_FUNDING with change) has **storage 460,532 = 92.1 %** of 500,000 (compute 11,223, transient 8,372; 22 outputs). Every other M6-a shape is ≤ 90 % (table below). Output: scratchpad `m6a.txt` (session scratchpad of the orchestrator, `.../95cd1dd8-.../scratchpad/m6a.txt`).

**Why (reasoned from KIP-9, not run):** genesis creates K + 4 covenant outputs at about R (params, K shards, drawer, position; plurality 2, 4C/R = 23,669 each at R = 1.69) plus the controller at R + acc + pot. M3's genesis (no position) was 437,512 on testnet-10 (`docs/tn10-mass-check.md:146`); M5 added the position output at R (+23,669), which gives the measured 460,532. Genesis was not re-measured in M5 (the M5 tn10 genesis was accepted because the consensus limit is 100 %). The worst genesis allowed by `validate()` is higher than the fixture: with acc = pot = 0 (controller at R) and V0 = V0_MIN (vault' at 2.69 KAS) it is about 20 × 23,669 + 14,870 = 488,000 (97.6 %), and a small change output adds C / change on top (≥ 100 % below about 1.2 KAS of change: a deployer-side liveness issue only, the transaction would be refused). R keeping that worst genesis ≤ 90 %: about 1.84 KAS (the fixture shape: 1.74 KAS). These figures are arithmetic, not measured. Options for the user (not chosen here): (a) accept genesis as a one-off exempt from the 90 % margin (it is ≤ 100 % for builder-chosen values); (b) a genesis builder policy (e.g. acc/pot/V0/change floors) that keeps it ≤ 90 %; (c) raise R_MIN / R; (d) split genesis. (b)–(d) change parameters or structure: user decision.

**M6-a measured (the rest, all ≤ 90 %; % c / t / s; relay floor / priority fee KAS):**
- buy b = 1: no referral 8.7 / 15.1 / 64.4 (large change; 62.3 % at 1 KAS); referral 0.1 KAS 8.8 / 15.2 / **84.5** (storage 422,352, large change; 412,304 at 1 KAS change) — matches D80's 422,352. buy b = 4: 9.2–9.3 / 15.9 / 16.5–21.7. Relay floor ≈ 0.076–0.079, priority 0.083–0.422.
- set_params: 1.3 / 1.4 / 3.4 (change), 1.2 / 1.4 / 0.0 (none).
- close K = 16 M2-a shape: 58.9 / **86.8** / 80.5 (294,619 / 867,716 / 402,301; unchanged from M5-d); close K = 16 storage worst case (M2-c grid: sales 0, O 0, J_boost 0, A 0, caller 1,000,000 KAS; scripts run): 58.9 / 86.8 / **89.9** (449,692). Relay floor 0.43386, priority 0.43386 / 0.44969.
- draw worst (DP21 K = 16, m = 50, a = 7, np = 7, 8 funding, cap binding, record): 41.7 / 53.2 / 14.2 (208,661 / 531,564 / 71,009; drawer units 1,810,706, sigscript 79,050; vault leg 463,913); typical draw (K = 16, m = 10, a = 0, np = 3, no record): 28.2 / 44.9 / 18.6 (141,229 / 448,516 / 93,028). Relay floor = priority: 0.26578 / 0.22426.
- claim K = 16, m = 50, h = K: PAY 10.1 / 15.8 / 49.4; ACCRUE 10.0 / 15.7 / 30.1; FINAL 9.8 / 15.7 / 4.2; PAY 8 funding 11.7 / 16.1 / 30.0; **PAY at P = P_MIN, storage worst case** (b = 2, pool' at R, change 1,000,000 KAS; run) 7.6 / 12.4 / **75.5** (377,514): priority fee 0.37751 KAS (7.55 × P_MIN), relay floor 0.06189 KAS. ACCRUE priority 0.15042 KAS.
- returns 5.6 / 10.3 / 6.1 and 6.7 / 11.4 / 6.9; refreshes ≤ 5.8 / 10.3 / 29.2 (ticket); marker refresh K = 4 and K = 16 identical (roots always 512 B).
- vault (M6-i): request_deposit (10 KAS) 4.6 / 7.8 / 5.0 (leader 178,110 units, sigscript 18,891 B); request_withdraw whole 4.9 / 8.2 / 4.9; partial (Schnorr/ECDSA/P2SH) 5.1 / 8.3 / 9.7; **partial at the W4b boundary w = wmin = 10^9** 5.1 / 8.3 / 9.7 (leader 199,723 units, sigscript 18,865 B, position leg 26,686); every settle mode ≤ 4.6 / 7.4 / 5.6. Request entries +252 to +347 leader units vs M5-e (W4b's two divisions on the withdraw path; the deposit path also changed by the +42 B vault redeem).
- Redeem sizes (B): shard 11,361; ticket 25,452; controller 27,694; params 2,918; marker 2,104; drawer 65,234; **vault 3,422** (+42, W4b); pool 2,481; record 1,154; position 1,194; request 15,479.
- Tightest margins: compute 58.9 % (close K = 16), transient 86.8 % (close K = 16), storage 92.1 % (genesis, STOP), next 89.9 % (close storage worst case), 84.5 % (b = 1 referral buy).

**Not measured (stopped before running with output):** M6-b … M6-j. The tests are written and compile (`m6b_k_frontier`, `m6c_b_max_frontier`, `m6d_m_frontier`, `m6e_d_frontier`, `m6f_p_min`, `m6g_r_recheck`, `m6h_fees`, `m6j_drawer_sensitivity`); the full-suite run below executed them without `--nocapture` (pass/fail only, tables not inspected or recorded). After the user's D90 decision: re-run `cargo test -p lottery --release -j 2 --test m6_measure -- --nocapture --test-threads=1`, check the frontier tables, then write the costs.md M6 section.

**Files changed**
- `crates/lottery/src/state.rs`: `MarkerState::roots_bytes_k(roots, k_max)`; `roots_bytes` = `roots_bytes_k(.., K_MAX)` (same behaviour).
- `crates/lottery/src/buy.rs`: `pub fn source_k_max(controller_src)` (the `int constant K_MAX = n;` line, else K_MAX); `compile_sources` sizes the dummy marker/pool constructor roots from it (`dummy_marker(k_max)`, `dummy_pool(k_max)`; 16 for committed sources, so the committed templates are byte-identical); `pub fn relay_floor_fee(&Masses)`.
- `crates/lottery/src/close.rs`: `CloseCase.k_max` (default `state::K_MAX`); `expected()` pads the marker' roots to 32 × k_max.
- `crates/lottery/tests/common/measure.rs` (new) + `common/mod.rs` (`pub mod measure;`): moved unchanged from m1_measure (`buy_case`, `buy_storage`), m2_measure (`close_case_c`, `close_storage`, SALES/POTS/ACCS/CALLERS, `worst_storage`, `storage_sup` → `close_storage_sup`, `r_bound`), m4_measure (`worst_claim`, `pay_storage`, now `pay_case` + `pay_storage`), m5_measure (`ST_ALL_DRAW`, `dp21` + new `dp21_with(t, ..)`, `Pending`, `with_pending`, `storage_case_rec`, `storage_sup_rec`, `st`, `settle_shapes`); new `k_sources(k)`, `drawer_sources(k_max, m_draw)`, `variant(key, ..)` (cached in-memory compile; compile failure returned as Err, e.g. a stack-limit error).
- `m1_measure.rs`, `m2_measure.rs` (imports `close_storage_sup as storage_sup`), `m4_measure.rs`, `m5_measure.rs`: local copies removed, imports added, unused imports removed. Behaviour identical.
- `crates/lottery/tests/m6_measure.rs` (new): M6-a … M6-j as spec A5.

**Decisions**
- S3-1: `relay_floor_fee` lives in `lottery::buy` (lottery-side, as the plan says). It calls the mempool's own formula: `NonContextualMasses::normalized_max` with `MAINNET_PARAMS.block_mass_limits.cofactors()` × 100 sompi/gram. Verified in refs: fee mass = max(compute, normalized transient) (`mining/src/mempool/check_transaction_standard.rs:71-76`), fee = mass × minimum_relay_transaction_fee / 1000 with DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE = 100,000 (`:92-100`, `mining/src/mempool/config.rs:20`), normalized transient = ceil(transient × L_compute / L_transient) (`consensus/core/src/mass/mod.rs:178-188, 256-259`), the node passes its block mass limits to the mempool (`daemon.rs:648-652` in the node crate), mainnet limits 500,000 / 1,000,000 (`params.rs:626`). For mainnet this equals max(compute, ceil(transient / 2)) × 100, the test-local versions (`harness/tests/common/mod.rs:118`, `tn10/src/probe.rs:248`) — those are left as they are.
- S3-2: K_MAX variants (D91): substitution of the exact `    int constant K_MAX = 16;` line (controller, drawer, ticket) and every `byte[512]` (controller, marker, drawer, pool, ticket), compiled through `compile_sources`; `CloseCase.k_max` selects the marker' root padding. The K = 16 variant is asserted hash-identical to the committed controller, marker, drawer, pool and ticket.
- S3-3: M6-e puts the referral output at exactly M_REF for every b (price = ceil(1 KAS / b), s_R = 10 %), the largest referral storage term; M-c/M4-d used 0.1 × b KAS at price 1.
- S3-4: M6-b draw()/claim() at K > 16 are transient-only estimates (4 g/B, `constants.rs:31`): draw extra bytes = Δdrawer code + Δcontroller code + (Δmarker code + 32 ΔK) + A_MAX × (Δpool code + 32 ΔK) + Δpool code (drawer's pool prefix/suffix arguments) + ΔK (576 + 2m); claim = Δticket code + (Δpool code + 32 ΔK) + ΔK (576 + 2m). Compute is not estimated.
- S3-5: M6-j uses drawer-only variants (M_DRAW 25 vs 50 at m = 25; drawer K_MAX 16 vs 17 at m = 50, marker/pool unchanged as M3-c), run as full draws (the chain recompiled so marker, vault and pool commit to the variant drawer's hash); slopes are Δmass / Δdrawer redeem B; the byte budget uses the larger slope per mass. The stack-slot note cites DECISIONS 2026-10-12 Fact S4-1 (4 free slots at check 24, line 257) and the D78 row (line 249).
- S3-6: the M6-a genesis row uses the fixture genesis (above). Its worst case under `validate()` was not measured (stop).

**Commands (from `crates/`)**
- `cargo build -p lottery --release --tests -j 2`: ok, no warnings after the import clean-up.
- `cargo test -p lottery --release -j 2 --test m6_measure m6a -- --nocapture --test-threads=1`: `m6a_redeem_sizes` ok; `m6a_every_transition_within_90` FAILED (D90: genesis 460,532).
- `cargo test --workspace --release -j 2 --no-fail-fast` (to check the library and helper moves): exit 101, the only failure `m6_measure::m6a_every_transition_within_90` (D90, above). Every other binary ok: harness 0 + 3 + 3 + 5 + 2 + 6; lottery lib 34; m1_attacks 25, m1_buy 14, m1_measure 4, m1_mutants 11, m2_attacks 40, m2_close 8, m2_game 2, m2_measure 5, m2_mutants 25, m2_params 17, m3_attacks 22, m3_draw 24, m3_game 13, m3_measure 7, m3_mutants 94, m4_attacks 21, m4_claim 25, m4_game 4, m4_measure 8, m4_mutants 211 (+1 ignored), m5_attacks 21, m5_game 10, m5_measure 5, m5_mutants 261, m5_vault 15, m6_attacks 6, m6_conflict 3 (Stage 3b's file), m6_limits 8, m6_mutants 3; m6_measure 9 passed / 1 failed; tn10 0. So the M6-b … M6-j asserts (K = 16 close ≤ 90 %, B_MAX = 4, m = 50, b = 1 referral buy at D_FLOOR, P_MIN, both R checks) passed in that run, but their tables were captured and not inspected. Full log: scratchpad `full_suite.txt`.

**Open items**
1. USER DECISION (D90): genesis storage 92.1 % (fixture), about 97.6 % worst case (reasoned). Options above.
2. After the decision: run M6-b … M6-j with `--nocapture`, check every assert, write the costs.md M6 section (incl. the "Purchase conflict rate (M6, D88)" placeholder for Stage 3b/4) and the remaining Stage 3a numbers here.
3. The A5 table should list genesis under A5a ("depends on K + 4 outputs at R": the R re-check M6-g should name genesis as a third storage transition next to close and draw).

### Stage 3b (2026-10-13, merged by the orchestrator): `m6_conflict` model
File: `crates/lottery/tests/m6_conflict.rs` only (self-contained, splitmix64 inline). `cargo test -p lottery --release --test m6_conflict -j 2 -- --nocapture --test-threads=1`: 3 passed (1.2 s). Output: scratchpad `m6_conflict.txt`.
- S3b-1: a conflicting buy retries after ρ on a shard drawn uniformly from the other K − 1 (excludes only the shard it just lost; research F1).
- S3b-2 (**spec Correction**, A8 / D88): only an accepted buy makes a shard busy; a conflicting buy is a double spend the mempool rejects (no successor, timer not restarted). With a = λτ/K and ρ → ∞ each shard is a one-server loss system: first-attempt rate = **a / (1 + a)** (Erlang B, one server; insensitive to the service distribution). The spec's 1 − exp(−a) holds only if every arrival (rejected ones included) restarts the busy timer; under the protocol semantics it misses by > 0.01 in 26 of 45 cells. The test asserts a/(1+a) within 0.01 (max deviation 0.00107, ≤ 2.15 SE; tolerance ≈ 10 SE) and, as a simulator cross-check, the restart semantics against 1 − exp(−a) (max 0.00136). Both formulas are printed.
- S3b-3: placeholders TAU_CHAIN = 1.0 s, TAU_MERGE = 5.0 s, RHO_DEFAULT = 1.0 s; env overrides `M6_TAU_CHAIN`, `M6_TAU_MERGE`, `M6_RHO`. Stage 4 replaces τ with the probe's measured values.
- S3b-4: λ ≥ K/τ has no steady state: run as a 600 s burst from idle, then drained; flagged SAT (numbers depend on the burst length).
- S3b-5: at finite ρ below saturation every buy eventually succeeds, so a fresh buy finds its shard busy with probability exactly a = λτ/K (shard utilisation, PASTA), independent of ρ and the retry rule; asserted within 0.01 (max 0.00337). This, not a/(1+a), is the practical first-attempt rate.
- Table at placeholders (ρ = 1 s), K = 16: τ = 1 s: λ = 0.1/0.5/1/2/5 → first-conflict 0.006/0.031/0.063/0.123/0.312, mean attempts 1.006–1.458. τ = 5 s: 0.031/0.159/0.313/0.624/SAT (K/τ = 3.2/s).

### User decision U-M6-1 (2026-10-13, answer to the Stage 3a D90 stop)
**Genesis is exempt from the 90 % margin.** Reason: genesis storage depends only on deployer-chosen output values (not scripts); a genesis over the limit only means "cannot deploy" (no funds locked, no round frozen); the 90 % margin protects covenant-forced transitions (close, draw, claim, …). Implement: (1) a genesis builder check refusing a genesis whose exact storage mass (and compute, transient) exceeds 100 % of the mainnet limits, e.g. a small change output (failing-first unit test); (2) M6-a asserts genesis ≤ 100 % (not 90 %) and every other shape ≤ 90 %; (3) measure the worst genesis `validate()` allows (acc = pot = 0, V0 = V0_MIN, smallest change the builder accepts) and record it; (4) becomes a DECISIONS row (Stage 6) and an A5a/M6-g note (genesis is a third storage transition with K + 4 outputs at R, exempt). R and templates unchanged.

### Stage 3c (2026-10-13): U-M6-1, M6-b … M6-j with output, `docs/costs.md` M6 section

Nothing committed. `docs/design.md`, `covenants/` and `docs/DECISIONS.md` untouched. No parameter or template changed. No D90 stop: every non-genesis shape and every frontier assert is within 90 %.

**U-M6-1 implementation**
- `crates/lottery/src/genesis.rs`: new `pub fn genesis_mass_check(&harness::Masses) -> Result<(), String>` (each of compute, transient, storage must be ≤ `harness::MAINNET_PARAMS.block_mass_limits`; error text "genesis <name> mass <v> > the mainnet limit <l> (U-M6-1: genesis is exempt from the 90 % margin, not from the limit)"); new `GenesisCase::try_build_with(t) -> Result<Built, String>` = `validate_with` → `build_unchecked_for_tests` → `finish()` → `harness::masses` (the same v2.1.0 calculator with mainnet parameters as every measurement; `lottery` already depends on `harness`) → `genesis_mass_check`; `build_with` now panics "genesis refused: …" on either error. Module doc updated. Citations (verified in refs): limits `refs/rusty-kaspa-v2.1.0/consensus/core/src/config/params.rs:626` (compute 500,000, storage 500,000, transient 1,000,000); mempool per-transaction checks `mining/src/mempool/check_transaction_limits.rs:19-26` (compute, transient) and `:37-41` (storage); block checks `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:88-96`; all use `>`, so exactly 100 % is allowed.
- Unit test `genesis::tests::u_m6_1_genesis_refuses_mass_over_the_limit` (fixture genesis at K = K_MAX = 16, change shrunk by raising the fee): change 0.2 KAS and 1,000 sompi must be refused with a message containing "U-M6-1" and "storage" (`validate()` alone accepts both); change 2 KAS and no change (exact funding) are accepted.
  - **Failing first** (before the implementation; `cargo test -p lottery --release -j 2 --lib u_m6_1`): `test genesis::tests::u_m6_1_genesis_refuses_mass_over_the_limit ... FAILED`, `panicked at lottery\src\genesis.rs:868:26: U-M6-1: builder accepted a genesis with 20000000 sompi of change (storage mass over 100 % of the limit)`; `test result: FAILED. 0 passed; 1 failed`.
  - **Passing after** (`cargo test -p lottery --release -j 2 --lib genesis`): 10 passed, including `u_m6_1_genesis_refuses_mass_over_the_limit ... ok` and every existing genesis test.
- `crates/lottery/tests/m6_measure.rs`: `Row.exempt` (genesis rows asserted ≤ 100 %, others ≤ 90 %; the last column prints "false; exempt (U-M6-1), <= 100 %: true" for genesis); the "Tightest" line excludes genesis and "Genesis, exempt …" lines follow; new `genesis_outs`, `worst_genesis_case(t, fee, change)`, `worst_genesis_rows(t)` (fee = relay floor of the change shape, 1,122,300 sompi; binary search for the smallest change `try_build_with` accepts, asserting that change − 1 is refused with a "U-M6-1 … storage" error). M6-a now has four genesis rows.

**Worst genesis (measured, M6-a):** K = 16, acc = pot = 0, J = 0 (L(1) = 0, so V0 = V0_MIN passes the cap check), V0 = V0_MIN, R = R_MIN: **488,229 (97.6 %)** with change 1,000,000 KAS; 487,956 (97.6 %) with no change; the smallest change the builder accepts is **83,063,378 sompi (0.8306 KAS)**, giving exactly 500,000 (100.0 %); 83,063,377 is refused. Compute 11,223 / transient 8,372 (2.2 / 0.8 %). Fixture genesis 460,532 (92.1 %). Stage 3a's reasoned 97.6 % is now measured.

**Key M6-b … M6-j numbers** (full output: orchestrator scratchpad `m6_measure.txt`; all tables in `docs/costs.md` M6):
- M6-a: tightest non-genesis compute 58.9 % / transient 86.8 % (close K = 16), storage 89.9 % (close storage worst case), then 84.5 % (b = 1 referral buy at d = 0.13, = D80 exactly). Vault redeem 3,422 B; requests +42 compute / +168 transient vs M5 (4 g/B of redeem), worst draw +143 / +172.
- M6-b: close() transient 86.8 / 91.9 / 97.1 / 102.2 % at K = 16 / 17 / 18 / 19 → largest K at 90 % = **16**, at 100 % = **18** (transient binds; compute 70.5 % at K = 19, so "≥ 19"). About 51,400 transient per shard. Draw/claim at K = 17..19 are estimates (draw 54.6–57.4 %, claim 16.9–18.4 % transient).
- M6-c: B_MAX = 5 close K = 16 transient 993,220 (99.3 %; M2's 98.2 % plus the M3/M4 controller and marker growth of 10,892 ≈ 993,144); B_MAX = 4 86.8 %; B_MAX = 3 74.2 %.
- M6-d: m = 50: buy b = 4 9.4 / 16.0 / 21.7; draw 41.7 / 53.2 / 14.2; claim PAY F_MAX 11.7 / 16.1 / 30.0. M_DRAW = 25 variant at m = 25: 40.7 / 52.0 / 14.2.
- M6-e: b = 1 referral at M_REF floor 0.1193 KAS (90 %), 0.1038 (100 %); D80 shape 447,993 (d = 0.12), 422,352 (d = 0.13, 84.5 %).
- M6-f: P floor 3,669,900 sompi (0.036699 KAS; D82 ≈ 0.0367) → 0.04 KAS; P_MIN = 0.05: 377,514 (75.5 %).
- M6-g: close 449,692 = (K + 3)4C/R (89.9 %), **binding**; draw with the record 327,684 (65.5 %) / 417,684 (83.5 %) at F_MIN / F_MIN_FLOOR (= M5-c); draw alone needs R ≥ 1.529 KAS. Genesis recorded as the exempt third storage transition (K + 4 outputs at R).
- M6-h: PAY at P_MIN priority fee 0.37751 KAS (755 % of P_MIN), relay floor 0.06189 (124 %); ACCRUE 0.15042; FINAL 0.07859; C_close covers its fee from S = 86.77 KAS (89.94 storage worst case), C_draw from 53.16 (worst) / 44.85 (typical) / 83.54 (storage supremum).
- M6-j: slopes 1.976 compute / 4.000 transient per drawer byte (M_DRAW pair), 1.648 / 4.006 (K_MAX pair); drawer byte budget to 90 %: **91,960 B** (transient binds; compute 122,159 B); 4 free stack slots at check 24 (Fact S4-1).

**Plausibility (reasoned against M2–M5; no concern found):** b = 1 buys and every claim / return / ticket-refresh storage fell by 4C/(b × 0.11 KAS) − 4C/(b × 0.13 KAS) (e.g. PAY at P_MIN 405,486 → 377,514 = −27,972 = 4C/0.22 − 4C/0.26 KAS); D82's 153,846 + C/P + 23,669 = 377,515 vs 377,514 measured; close, settle, set_params and the pool/marker/position/request/record refreshes are identical to M4/M5; M6-e's no-referral b = 1 100 % floor 0.0823 = M2's; M6-e's b ≥ 2 referral floors are higher than M2's because S3-3 puts the referral at M_REF for every b (a stricter shape, intended); M6-g's draw suprema equal M5-c (storage does not depend on d). Note: M6-b's compute column ends at K = 19 because the run stops once transient passes 100 %.

**Decisions**
- S3c-1: the mass check lives in `try_build_with` / `build_with` (it needs the finished transaction), not in `validate()` (no transaction there). `validate()` is unchanged; `build_unchecked_for_tests` stays unchecked. tn10's genesis paths (`cases.rs:251-265`, `:485-496`, V0 = 60 KAS, acc 2 KAS) now pass through the check; it refuses only what the network would refuse.
- S3c-2: the worst-genesis rows pay fee = the relay floor of the change shape (1,122,300 sompi; the no-change shape's own floor is 1,081,100), so every row pays at least its relay floor; the fee moves storage by a few grams at most.
- S3c-3: the costs.md M6 section sits after M5 (end of the milestone sections, before the M0 "Scripts / Transitions / Minimums" tables), as M1–M5 do; the old M0 "Minimums" table is not edited (M6 has its own minimums table).
- S3c-4: the costs.md conflict subsection lists only the K = 16 rows of the Stage 3b table (the committed K); K = 4 and 8 stay in the test output.

**Commands (from `crates/`) and results**
- `cargo test -p lottery --release -j 2 --lib u_m6_1` (before the implementation): FAILED as above. After: `cargo test -p lottery --release -j 2 --lib genesis`: 10 passed.
- `cargo test -p lottery --release -j 2 --test m6_measure -- --nocapture --test-threads=1`: **10 passed** (11.15 s).
- `cargo test --workspace --release -j 2 --no-fail-fast`: **exit 0, 40 test binaries ok, 0 failed** (log: orchestrator scratchpad `full_3c.txt`). harness lib 0, covenant_smoke 3, m0_limits 3, m0_mass 5, m0_merkle 2, m0_shapes 6; lottery lib **35** (34 + `u_m6_1_genesis_refuses_mass_over_the_limit`); m1_attacks 25, m1_buy 14, m1_measure 4, m1_mutants 11, m2_attacks 40, m2_close 8, m2_game 2, m2_measure 5, m2_mutants 25, m2_params 17, m3_attacks 22, m3_draw 24, m3_game 13, m3_measure 7, m3_mutants 94, m4_attacks 21, m4_claim 25, m4_game 4, m4_measure 8, m4_mutants 211 (+1 ignored), m5_attacks 21, m5_game 10, m5_measure 5, m5_mutants 261, m5_vault 15, m6_attacks 6, m6_conflict 3, m6_limits 8, m6_measure **10** (was 9 + 1 D90 failure in Stage 3a), m6_mutants 3; tn10 0. Only warnings: two pre-existing unused variables in `harness/tests/m0_mass.rs:459` (untouched).

**Open items for Stage 4 (tn10)**
1. τ_chain and τ_merge from `probe conflict-m6` replace the placeholders (1 s / 5 s) in `m6_conflict` and in the costs.md "Purchase conflict rate" table (marked as placeholders there).
2. The tn10 genesis now runs `genesis_mass_check`; expected storage is about the fixture's (≈ 92 %); a `fund()` change below about 1 KAS would be refused with "U-M6-1" (the network would refuse it too).
3. Re-runs `buy-m1` (d = 0.13), `claim-m4`, `vault-m5` (rescaled d ≥ 10 KAS, w ≥ wmin via `WithdrawCase::checked()`); masses should equal the M6-a rows (`request_deposit` 23,185 / 77,540; `request_withdraw` partial 25,277 / 82,828).
4. Stage 6: DECISIONS rows for U-M6-1 and S3c-1 … S3c-4; the A5a / M6-g genesis note is in costs.md.

### Stage 4 (2026-10-13): tn10 `conflict-m6`, re-runs, measured τ

Nothing committed. `docs/design.md`, `covenants/`, `docs/DECISIONS.md` and `CLAUDE.md` untouched. No key or `.env` content printed (every run log was checked for "PRIVATE": 0 hits).

**Files changed**
- `crates/tn10/src/probe.rs`:
  - `Sent` derives `Clone`;
  - the row printing moved into `Ctx::row` (`submit` uses it);
  - new `Ctx::submit_raw` (`submit_transaction`, orphans disallowed, no row);
  - new `Ctx::submit_replacement_raw` (`submit_transaction_replacement`, which the v2.1.0 wRPC client routes at `rpc/wrpc/client/src/client.rs:665`; returns the replaced transaction, or errors if the accepted id is not the submitted one).
- `crates/tn10/src/cases.rs`:
  - `merged()` returns `(Utxo, f64 seconds)` and keeps its print; new `merged_poll(.., poll)`; the 11 callers take `.0`.
  - New `stats`, `watch_accepted` (virtual-chain polling for a set of ids), `submit_until` (resubmit alternatives until one is accepted) and `conflict_m6`.
  - `buy_m1` now runs b = B_MAX and b = 1, each with a referral, on separate genesis shards (`buy_m1_b`).
  - `vault_m5`: D = 12 KAS, w = `game::w_min(V0, S0)` = 1,000,000,166, `WithdrawCase::checked()` inside `round1_drawn_with`; the doc comment and labels are updated.
- `crates/tn10/src/main.rs`: `probe conflict-m6` (not in `all`), usage string.
- `crates/lottery/tests/m6_conflict.rs`: `TAU_CHAIN = 0.919`, `TAU_MERGE = 1.655` (measured medians), module/const docs, and the table header prints "measured median, tn10 conflict-m6 2026-10-13, n = 10".
- `docs/costs.md` "Purchase conflict rate (M6, D88)": placeholder header and table replaced by the measured-τ table and a reading note.
- `docs/tn10-mass-check.md`: new section "M6: purchase conflicts, replacement and chained buys, and the re-runs at the final templates".

**Decisions**
- S4-1: `conflict-m6` is not part of `probe all`; it locks about 160 KAS.
- S4-2 (probe design change after run 2): the node's acceptance of a replacement does not decide which buy is accepted on chain. Run 2: the node accepted B as a replacement of A, but A was accepted on chain and B was dropped. So:
  - part R reads the chain winner of A and B from the virtual chain;
  - it submits C on the node's winner first, then on the other candidate;
  - a part F (P, then Q chained on P at once, on the shard's merged tip, no RBF in between) gives clean τ_chain / τ_merge.

  10 repetitions on genesis shards 0–9, each followed by part F on that shard's successor (the brief's "rotate across shards / successors"); fan-out of 5 × 10 funding outputs.
- S4-3: τ fed to the model = the part F medians: τ_chain = 0.919 s (P submit start → Q accepted) and τ_merge = 1.655 s (P submit start → seen chain-accepted). Part R's τ_merge (median 2.262 s) is biased upward, because its polling starts after the chain step at about 1.8 s; it is reported, not used.
- S4-4: B pays 2 × A's fee (feerate 199.61 vs 100.32 sompi/gram; the probe asserts B's feerate is strictly higher before the race). The same B transaction is used for the race and the replacement. There is no reject cache: `accepted_transactions` only holds ids accepted in blocks (`mining/src/mempool/validate_and_insert_transaction.rs:143-144`).
- S4-5: `vault-m5` withdraws exactly wmin, the W4b boundary on testnet. Slack check: the conversion's slack V' + D_c − L' ≈ 50 KAS, far above about 10 KAS, so wc = W (`game::convert`, `game.rs:920-954`). This was confirmed: wc = W = 1,000,000,166.
- S4-6: `buy-m1` gained the b = 1 referral case, because spec A6 names "b = 1 with referral" and the probe only did b = B_MAX.

**Runs (from `crates/`, `--release`, the binary copied to the scratchpad and run from there so cargo could rebuild meanwhile; logs in the orchestrator scratchpad `conflict-m6_run{1,2,3}.txt`, `buy-m1.txt`, `buy-m1_run2.txt`, `claim-m4.txt`, `vault-m5.txt`, `info_*.txt`)**
- `info`: 8,022.88 KAS before; 7,542.77 KAS after all runs (480.11 KAS locked or paid in fees). No top-up needed.
- `probe conflict-m6`:
  - **Run 1:** killed by me after about 1 minute during the resolver connection (my mistake, not a network failure); nothing submitted, balance unchanged.
  - **Run 2:** first probe version. Genesis `049d5aa3…` and fan-out accepted. Rep 0: A `e5ec7c27…` accepted; B `e0d26f2b…` rejected "already spent by transaction e5ec7c27… in the mempool", then **accepted by `submit_transaction_replacement`, which returned A as the replaced transaction**. C on B was an orphan for 60 s. The probe errored after 300 s waiting for B's merge. A was found on chain (change at DAA 595,200,941) and B's funding was unspent.
  - **Run 3 (recorded):** complete, exit 0. Genesis `424df434081eaad0b08ebe52621cec8a06da87a38907a6bac9f35e968e1e2455`, fan-out `c87cdbcfdeee65497e183de38a3eac50cfb165177f74bdb976b794ce0b7ebec6`; per-rep txids in tn10-mass-check.md.
- `probe buy-m1`:
  - run 1 (b = 4 only): `40bfedae…` accepted, 46,491 / 158,924 / 109,476.
  - run 2: b = 4 `978f08b6…` 46,491 / 158,924 / 109,473 accepted; **b = 1 referral 0.1 KAS `2263d66c…` 43,884 / 151,696 / 423,125 (84.6 %) accepted**. Compute and transient equal M6-a; storage 423,125 vs M6-a 422,352 (change ≈ 7,700 KAS vs 1,000,000 KAS).
- `probe claim-m4`: all accepted. Claim FINAL `a4b593bd…` 47,503 / 152,052 / 24,626, payout 10.13 KAS = J + d; buy 43,472 / 151,488 and close 294,619 / 867,716 = M6-a; draw 141,986 / 448,864 (np = 4; M6-a typical np = 3: 141,229 / 448,516).
- `probe vault-m5`: all accepted, and the negative case was REJECTED by the node (D10 CLTV, "595211570 > 595211569").
  - request_deposit D = 12 KAS `eb865a3b…`: 23,185 / 77,540 = M6-a.
  - request_withdraw w = wmin = 1,000,000,166 `74d3a57d…`: 25,276 / 82,824 = M6-i (the W4b boundary row).
  - Draw `1a004c02…` 142,547 / 449,228 (M5 + 43 / + 172: vault +42 B and a 1 B longer W push); mint 1,418,999,053, wc = W, out_c 8.4567 KAS.
  - settle deposit-convert 22,147 / 73,388 and withdrawal FINAL drained 21,857 / 73,108 = M6-a.

**Race / replace / chain facts (run 3, n = 10)**
- Race: B rejected 10/10. 4 as "output … already spent by transaction <A> in the mempool"; 6 as "transaction <B> is an orphan where orphan is disallowed": 0.46–0.69 s after A's submission started, A was already block-included and out of the mempool.
- **Replacement: accepted by the node 1/10 (rep 5; it returned A as replaced). In 9/10 it was rejected with "replace by fee found no double spending transaction in the mempool". In rep 5, as in run 2, A was accepted on chain, not B. So: 0 of 11 replacements displaced the pending buy on chain.** Mechanism (refs):
  - the RPC replacement is broadcast (`protocol/flows/src/flow_context.rs:676-690`);
  - peers insert relayed transactions with RbfPolicy::Allowed (`protocol/flows/src/v10/txrelay/flow.rs:233`);
  - a block holding A removes A's double spends (`mining/src/mempool/handle_new_block_transactions.rs`, `remove_double_spends`).

  At 10 BPS and a ~0.46 s RPC round trip, A is in a block before B can get there.
- Chain (F3): chained buys accepted 20/20 (C 9 at the first attempt + 1 at the second after "C on B" was an orphan; Q 10/10 at the first attempt, each accepted before its parent was seen chain-accepted). All 20 were merged on chain afterwards.
- τ (s): τ_chain F median 0.919 / mean 0.920 / max 0.925; τ_merge F median 1.655 / mean 1.650 / max 1.962; τ_merge R median 2.262 / mean 2.351 / max 2.914; pooled median 2.103. Q accepted → chain-accepted median 1.047, max 1.684. Local build time 0.005 s per buy (excluded).

**Tests**
- `cargo build -p tn10 --release -j 2`: ok, no warnings (after each change).
- `cargo test --workspace --release -j 2`: exit 0; 40 binaries, 0 failed. harness 0/3/3/5/2/6; lottery lib 35; m1 25/14/4/11; m2 40/8/2/5/25/17; m3 22/24/13/7/94; m4 21/25/4/8/211; m5 21/10/5/261/15; m6_attacks 6, m6_conflict 3, m6_limits 8, m6_measure 10, m6_mutants 3; tn10 0. Only the two pre-existing `m0_mass` unused-variable warnings. It ran while the `m6_conflict.rs` τ edit and the `buy_m1` b = 1 edit were being made, so it may have compiled either version. Both were re-checked afterwards: `cargo build -p tn10` ok and the m6_conflict re-run below.
- `cargo test -p lottery --release --test m6_conflict -j 2 -- --nocapture --test-threads=1`: 3 passed. Header "tau_chain = 0.919 s (measured median, tn10 conflict-m6 2026-10-13, n = 10), tau_merge = 1.655 s (…)". K = 16 table at τ_chain: first-attempt conflict 0.0057 / 0.0286 / 0.0560 / 0.1149 / 0.2861 for λ = 0.1 / 0.5 / 1 / 2 / 5; at τ_merge: 0.0104 / 0.0502 / 0.1042 / 0.2070 / 0.5171. No SAT cells (K/τ = 17.41 and 9.668 /s).

**Open items for the review (Stage 5) and recording (Stage 6)**
1. **D87 input (flag): the fee-auction argument did not hold on testnet-10.** The mempool accepts a higher-feerate replacement of a covenant spend (so RBF is not refused for covenant spends), but the pending transaction was already block-included in 9/10 attempts, and the 2 node-accepted replacements still lost on chain. At 10 BPS an attacker's pending request is mined in under ~0.5–0.7 s, so "an honest request outbids the attacker's pending request" has no practical window through a public node. Not redesigned; for the user / reviewer to decide how D87 is worded (e.g. the griefing bound rests on MIN_DEPOSIT / W_MIN_VALUE and δ, not on RBF).
2. F3 is closed as a measurement: chained buys on an unaccepted successor are accepted at once and merge (20/20). The model's τ_chain (0.919 s) is two public-node round trips, i.e. the client latency, not a protocol limit.
3. The W4b-boundary withdrawal (wmin shares, 10 KAS at the request-time price π ≈ 1) converted to out_c = 8.4567 KAS: the round's jackpot payout lowered the share price before conversion. D86's "≥ W_MIN_VALUE − π" bound is at request time, as specified; worth a sentence in D87's "each slot locks ≈ 10 KAS".
4. CLAUDE.md's probe list does not name `conflict-m6`, and the buy-m1 description there does not mention the b = 1 case. I did not edit CLAUDE.md; suggest adding them at recording.
5. Spec Corrections (Stage 6):
   - the A6 probe has a part F (S4-2);
   - τ is measured as part F medians (S4-3);
   - `buy-m1` covers b = B_MAX and b = 1 (S4-6);
   - `merged()` returns the elapsed time;
   - `conflict-m6` is not in `all` (S4-1).
6. Locked on testnet: conflict-m6 runs 2 and 3 instances (about 96 + 160 KAS), plus the claim-m4 / vault-m5 / buy-m1 objects.

### Orchestrator decision O-M6-1 (2026-10-13, picked under user delegation: "pick the recommended one")
**D87 is amended: the fee auction is not a mitigation.** Stage 4 measured 0/11 effective replacements on testnet-10 (the attacker's request is in a block 0.46–0.69 s after submission; a node-accepted replacement was still dropped when the original was mined). The node does accept RBF for covenant spends (fact, kept). Request griefing (U-6) is mitigated by cost only: each slot locks ≥ min(MIN_DEPOSIT, W_MIN_VALUE − π) + R ≈ 11.7 KAS until the next settlement and pays the request fee (D85, D86); it is liveness of requests only (no funds at risk). The residual — an attacker who wins the race at every window opening can delay honest requests indefinitely at ≈ 16,800 KAS locked and ≈ 70 KAS/day in fees — is recorded **OPEN on the release gate** (candidate fixes: request batching, a per-round request queue; out of M6 scope). No code change in M6. Builders still submit through the replacement path when the vault outpoint is spent in the mempool (harmless, occasionally useful).

### Stage 5 review (covenant-reviewer on 06c131f vs 8f46436), summary by the orchestrator
No must-fix. **Should-fix RV-1:** W4b bounds the withdrawal, not the remainder: a partial w = wmin from a position of wmin + 1 leaves a 1-share position, which the whole-position clause later withdraws at R per slot (≈ 2,434 KAS/day for 1,440 slots, the M5 U-6 figure), so D87/O-M6-1's ≈ 11.7 KAS per slot is false for a prepared attacker. Reviewer test `rv_dust_split` (scratchpad `review/rv_m6.rs`): both steps accepted. Proposed fix (tried in scratch, M5/M6 suites pass): `require(w == ps.shares || (w >= wmin && ps.shares - w >= wmin));`. Settlements cannot create sub-wmin positions (a deposit mints ≥ 1 share worth ≈ D, at least D/2).
Notes: RV-N1 while total_assets + 1 < W_MIN_VALUE (q = 0, e.g. genesis at V0_MIN = 1 KAS) W4b gives no protection (wmin = 1): document in D86/D87. RV-N2: D87 should say the value bound holds at request time (testnet: a wmin withdrawal converted to 8.46 KAS). RV-N3: design.md:365 still says SHARE_UNIT = 1,000 and U-5 open → DECISIONS proposal at recording.
Checked sound (with negative controls): W4b arithmetic (400-point engine sweep vs game::w_min, 0 mismatches; W4b removed → 64/64 expected rejects accepted), S2-1 retargets genuine, constants consistent, C1 bound, D84 numbers, smallest genesis runs a full round, 11 template hashes unchanged by crate code, U-M6-1 (`>` per refs), M6-a/M6-b spot checks, D89, D92 conservation, request griefing liveness-only (draw_leg has no age check; settlements never spend the vault).

### Orchestrator decision O-M6-2 (picked under user delegation): apply RV-1's fix
W4b becomes `w == ps.shares || (w >= wmin && ps.shares - w >= wmin)` (the remainder of a partial withdrawal is also ≥ wmin). No lock-in: a whole withdrawal is always allowed. RV-N1/N2/N3 are documentation (Stage 6).

### Stage 5 fix (RV-1, 2026-10-13): W4b remainder rule (O-M6-2)

Nothing committed. `docs/design.md`, `docs/DECISIONS.md` and `CLAUDE.md` untouched. No D90 stop: the vault compiles, no stack-limit error, every M6-a shape still within 90 %. **Re-review range: the uncommitted working-tree changes vs `06c131f`** in the files listed below (the `docs/figures/**` changes and the earlier sections of this handoff in the working tree are not part of this fix).

**Script (`covenants/vault.sil`, `request_withdraw`, check W4b).** The one-line `require(w >= wmin || w == ps.shares); // mut:M6W4b_min` became:
```
if (w != ps.shares) {
    require(w >= wmin); // mut:M6W4b_min
    // 1 <= w <= ps.shares (W4), so 0 <= ps.shares - w < ps.shares <= SHARES_MAX.
    require(ps.shares - w >= wmin); // mut:M6W4b_rest
}
```
This equals `w == ps.shares || (w >= wmin && ps.shares - w >= wmin)` (O-M6-2). The marker is now `// ==== check W4b: the whole position, or w >= wmin and a remainder >= wmin (M6, D86)` (label still `W4b`, outside any if body; ATTRIB truncation unchanged). The comment block states the value bound, the remainder rule, RV-1 / O-M6-2, "no lock-in" and the mirror `game::w4b_ok`; the `||` / OpBoolOr citation was removed (no `||` left). Header label paragraph updated. No underflow: W4 (before W4b) gives 1 <= w <= ps.shares; even without W4, w <= S - pend_wsh <= SHARES_MAX keeps `ps.shares - w` far from overflow.

**Files changed (this fix)**
- `covenants/vault.sil` (above). Vault redeem 3,422 -> **3,432 B** (+10).
- `crates/lottery/src/game.rs`: new `pub fn w4b_ok(total_assets, total_shares, shares, w) -> bool` (the full W4b decision; `w_min` unchanged); unit test `rv1_w4b_ok_remainder_rule`.
- `crates/lottery/src/vault.rs`: `WithdrawCase::policy_ok()` = `game::w4b_ok`; the `checked()` error names "D86, RV-1, vault W4b" and the remainder; module doc; unit test `withdraw_policy_mirrors_w4b` extended (dust split refused, remainder exactly wmin ok, wmin - 1 refused, q = 0 partial leaving 1 share ok).
- `crates/lottery/tests/common/m5_spec.rs` `withdraw()`: `check(w == ps.shares || (w >= wmin && ps.shares - w >= wmin), "vault:W4b")`, plus the header line.
- `crates/lottery/tests/common/m6.rs`: bases `wd_whole_wmin_plus_1`, `wd_rest_at_wmin` (2 wmin, w = wmin), `wd_rest_at_wmin_large` (5 x 10^10, w = shares - wmin); `la7()` (LA7a/b/c, expected `vault:W4b`), added to `all_atks6`; module doc.
- `crates/lottery/tests/m6_attacks.rs`: `la7_partial_withdrawal_remainder_below_wmin` (run5(la7)) and `la7_dust_split_two_steps` (the reviewer's `rv_dust_split`: step 1 must be rejected by input 0, not by an arithmetic error; step 2, the whole withdrawal of a 1-share position, is still accepted: no lock-in); header.
- `crates/lottery/tests/m6_limits.rs`: LP3 gains 5 RV-1 positives (remainder exactly wmin with w = wmin and w = shares - wmin; whole positions of wmin + 1 and 5 x 10^10 shares; q = 0 partial leaving 1 share), mirror assert via `w4b_ok`. LP4's grid gains the remainder dimension (sizes {5 x 10^10, wmin - 1, wmin + 1, 2 wmin - 1, 2 wmin}, w in {wmin - 1, wmin, shares - wmin, shares - wmin + 1, shares}, clamped and deduplicated; 206 points; asserts that at least one point is rejected by the remainder rule alone); expectation via `w4b_ok`.
- `crates/lottery/tests/m6_mutants.rs`: see "Mutants"; pin (2 tags, 2 lines).
- `crates/lottery/tests/m5_mutants.rs`: `w4_shares` -> `w4_shares_redundant` plus an EQUIVALENT entry (S5-1).
- `docs/costs.md` M6 rows: vault redeem, the four request rows and M6-i, both M6-a draw rows, the M6-b K = 16..19 draw estimates, the M6-d draw rows, the M6-h draw fee rows, the M6-i text, the M6-j slopes table and budget text, the "Against M5" deltas, "Only the vault changed size".
- `docs/tn10-mass-check.md`: the first M6 `vault-m5` run and `claim-m4`'s draw marked SUPERSEDED; new sections for both RV-1 re-runs.

**Failing first** (scripts as at 06c131f, Rust mirrors already updated): `cargo test -p lottery --release -j 2 --no-fail-fast --test m6_attacks --test m6_limits -- --test-threads=1`:
- `la7_partial_withdrawal_remainder_below_wmin ... FAILED`: "ATTACK ACCEPTED (LA7a request_withdraw w = wmin from a position of wmin + 1 (remainder 1 < wmin)) by input 0";
- `la7_dust_split_two_steps ... FAILED`: "ATTACK ACCEPTED (rv_dust_split step 1: w = wmin = 1000000000 from wmin + 1 shares leaves a 1-share position') by input 0";
- `lp4_w_min_matches_the_script ... FAILED`: "LP4: 206 grid points, 15 rejected by the remainder rule alone, 15 mismatches", all "expected rejected, the vault accepts" (e.g. A = S = 10^15, shares 1,000,000,001, w = 10^9; also at A = ASSETS_MAX and S = SHARES_MAX);
- everything else in both files passed (LP3's new positives are accepted shapes, so they pass on the old script too).

**Mutants (one tag per line)**
- `M6W4b_min` (`require(w >= wmin)`): `m6w4b_min` killed by LA2 "w = wmin - 1 (q >= 1)" (unchanged).
- `M6W4b_rest` (new): killed by LA7a, LA7b and LA7c (`m6w4b_rest_dust_split`, `m6w4b_rest_small_w`, `m6w4b_rest_large_w`).
- LA2 "w = shares - 1 < wmin (whole position below wmin)" leaves a 1-share remainder, so each line alone rejects it. `m6w4b_min_whole_small` is now killed with BOTH tags removed, plus a `redundant6!` test for each tag alone (`..._min_only`, `..._rest_only`; both print "still rejected at input 0 by VerifyError").
- `m5w4_shares_with_m6w4b_rest`: M5's "VA6 request_withdraw w = shares + 1" is killed with `M5W4_shares` and `M6W4b_rest` removed together.
- `every_m6_tag_is_tested`: `M6 tags: {"vault:M6W4b_min", "vault:M6W4b_rest"}, tagged lines: 2`; pin (2, 2).

**Decisions**
- S5-1: W4b's remainder rule implies W4's `w <= ps.shares` (a partial needs ps.shares - w >= wmin >= 0, and `w_min` >= 0 on the W1 ranges). So `m5_mutants::w4_shares` (killed in M5 and Stage 2 by "VA6 w = shares + 1") was no longer killed. The full suite before this decision had m5_mutants 260/261: "mutant ["vault:M5W4_shares"]: input 0 still rejects `VA6 request_withdraw w = shares + 1`: VerifyError". It is reclassified as `redundant!(w4_shares_redundant, ..)`, which asserts that the mutant still rejects by a `require` (not an arithmetic error) and accepts the base, with an EQUIVALENT entry giving the proof. The pair kill (`m5w4_shares_with_m6w4b_rest`) shows the two lines together still guard it. The W4 line stays in the script: it keeps W4b's subtraction non-negative and keeps W4 first in attribution (ATTRIB for VA6 w = shares + 1 is still `vault:W4`). m5_mutants stays at 261 tests. Reviewer: confirm this reclassification (a Correction for Stage 6).
- S5-2: each LA7 attack makes one change from a valid base. LA7a: whole position of wmin + 1 -> w = wmin. LA7b: 2 wmin, w = wmin -> wmin + 1. LA7c: fixture 5 x 10^10, w = shares - wmin -> + 1. LA7a is exactly the reviewer's step 1.
- S5-3: the Rust mirror is a new `game::w4b_ok`. The PREFLIGHT model (`m5_spec`) restates the rule inline, not via `w4b_ok`, so it stays an independent statement of A4.
- S5-4: `claim-m4` was re-run as well as `vault-m5`, because its `draw()` spends the vault (`draw_leg`) and so carries the vault redeem. `buy-m1` and `conflict-m6` were not re-run: genesis and `buy()` carry no vault script, and the vault hash feeds only the position and request templates (`Templates::compile_sources`), so the shard, ticket, controller and params scripts are byte-identical.
- Spec text to correct at Stage 6 (not edited here): A4's W4b block and the D86 wording (remainder rule, two tags); A8 LA7 and the LP3/LP4 additions; "Mutants" (two tags; S5-1); RV-N1/N2/N3 from the review.

**Commands (from `crates/`) and results**
- `cargo test --workspace --release -j 2 --no-fail-fast` (final, after every edit): **exit 0**, 0 failed. harness 0/3/3/5/2/6; lottery lib **36** (+ `rv1_w4b_ok_remainder_rule`); m1 25/14/4/11; m2 40/8/2/5/25/17; m3 22/24/13/7/94; m4 21/25/4/8/211 (+1 ignored); m5_attacks 21, m5_game 10, m5_measure 5, m5_mutants 261, m5_vault 15; m6_attacks **8**, m6_conflict 3, m6_limits 8, m6_measure 10, m6_mutants **9**; tn10 0. Only the two pre-existing `m0_mass.rs:459` warnings. Log: scratchpad `full_rv1_final.txt`. An earlier run, `full_rv1.txt`, had only the S5-1 failure.
- `M5_ATTRIB=1 cargo test -p lottery --release -j 2 --test m5_attacks --test m6_attacks -- --nocapture --test-threads=1`: 21 + 8 passed; **347 ATTRIB lines (344 + 3 LA7), 0 not OK**. LA7a/b/c: `expected vault:W4b | got vault:W4b | model Some(Err("vault:W4b")) | OK`. LA2 (3) and LA3 are still `vault:W4b` OK; VA6 w = shares + 1 is still `vault:W4` OK. Log `attrib_rv1.txt`.
- `M5_PREFLIGHT=1 cargo test -p lottery --release -j 2 --test m5_attacks --test m5_vault --test m6_attacks -- --nocapture --test-threads=1`: 21 + 15 + 8 passed; **347 PREFLIGHT lines, 0 not OK** (no model/script disagreement). Log `preflight_rv1.txt`.
- `cargo test -p lottery --release -j 2 --test m6_mutants --test m5_mutants`: 9 + 261 passed.
- `cargo build -p tn10 --release -j 2`: ok.
- `cargo test -p lottery --release -j 2 --test m6_measure -- --nocapture --test-threads=1`: 10 passed; log `m6_measure_rv1.txt`.

**New masses** (diff against `m6_measure.txt`; nothing else changed)
- Vault redeem 3,432 B (suffix 3,276).
- `request_deposit`: 178,170 units, 18,901 B sigscript; 23,195 / 77,580 / 24,886.
- `request_withdraw` whole: 187,456 / 18,876, position leg 26,756; 24,730 / 82,520.
- `request_withdraw` partial: Schnorr 199,819 units, 25,287 / 82,868 / 48,554; ECDSA 199,756 units; P2SH 199,685 units, 25,322 / 83,008.
- **M6-i**, partial at w = wmin: 199,805 units, 18,875 B; 25,286 / 82,864 / 48,554 (5.1 / 8.3 / 9.7 %).
- Draw worst: 208,671 / 531,604 / 71,009 (vault leg 463,933; priority 265,802, 0.26580 KAS). Typical: 141,239 / 448,556 (0.22428 KAS; C_draw covers it from S = 44.86 KAS).
- M6-b draw estimates: +40 transient each (545,648 / 559,664 / 573,680 at K = 17 / 18 / 19; percentages unchanged).
- M6-j drawer byte budget to 90 %: transient **91,950 B** (was 91,960), compute 122,154 B; slopes unchanged.
- Every percentage column is unchanged except `request_withdraw` whole transient, 8.2 -> 8.3 %. The tightest margins are unchanged (close K = 16: 86.8 % transient, 89.9 % storage). The D83 row in DECISIONS (Stage 6) should use 91,950 B.

**Testnet-10** (from `crates/`, `--release`; logs `vault-m5_rv1.txt`, `claim-m4_rv1.txt`; "PRIVATE" grep: 0 hits in both; no retries needed)
- `probe vault-m5` (node vector-10.kaspa.green): all accepted; the negative D10 case was REJECTED ("595239511 > 595239510").
  - genesis `7722bd3818521fde39d718b9ed49241187902632cadf0f953aa530802447fa2d`;
  - request_deposit 23,195 / 77,580 `952e46398dc7cc4129f846c62e4b19c6754f481b5fcfa16af906e20df05bce88`;
  - request_withdraw w = wmin = 1,000,000,166 of 6,000,000,000 shares (remainder 4,999,999,834 >= wmin; passes `checked()`) 25,286 / 82,864 `d5cea0c1de398fcb49eab623741d9a303f58f0a2385260b019f55ebf22d85ec6`;
  - draw 142,557 / 449,268 `fa2980bea369105b22fa3f2418b7c9067e9e36376688aca79952763ade7c0839` (mint 1,418,999,053, wc = W, out_c 8.4567 KAS);
  - settle deposit-convert `e476a247b41b39a0044a77af1378625f73161c35b087d3696d5d58643a1b27c5`; withdrawal FINAL `ff94bba0b31da4eef984f9e437ecfd285598afc21db21543406e7c5568763a22`.

  The requests equal the new M6-a / M6-i rows; the draw is +10 / +40 against the first run.
- `probe claim-m4` (node electron-10.kaspa.blue): all accepted.
  - genesis `db4851b9f461b5199e8945ada67fe0db4f280ae377ba2531d31eda15f015b210`;
  - buy `7c542d4efca73f6e0c5f8e9557c4c44cc2d1f77ac5eddf97ce0fe543ab85754d`;
  - close `36d369250fb01ee68edea4e7dd13ec04d48bbd3a31d41a9ca373e85f50d96e92`;
  - draw 141,996 / 448,904 (+10 / +40) `005e5f10336dc832211a1ddc9049e7c9829363ca6eb9fd9d1faf53d3ef4c60a8`;
  - claim FINAL 47,503 / 152,052 `4a01521f648dbdb14dce63945716f7f3e5b9f9664fe638a9bc2d8771825f251b`.
- Balance 7,542.77 -> 7,333.81 testnet KAS. The probes also wrote the git-ignored `crates/tn10/results-vault-m5.md` and `results-claim-m4.md`.

**Open items for the re-review and Stage 6**
1. Reviewer: the new W4b block, S5-1 (M5W4_shares is now redundant/equivalent), the LA7 bases, the LP4 remainder dimension.
2. Stage 6:
   - DECISIONS rows for O-M6-2 / RV-1 and S5-1 … S5-4;
   - D86/D87 text: the remainder rule, so the per-slot cost now holds for prepared positions too; the RV-N1 q = 0 caveat; RV-N2, the request-time price;
   - the spec Corrections listed above;
   - the D83 byte budget, 91,950 B.
