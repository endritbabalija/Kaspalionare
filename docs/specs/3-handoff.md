# M3 handoff (temporary; delete when M3 closes)

Plan: `C:\Users\endri\.claude\plans\wobbly-painting-reef.md` (approved). Spec: `docs/specs/3.md`.
Branch: `m3-draw`.

## Stage status
- [x] Stage 1 — scaffolding, reference functions, m3_game
- [x] Stage 2 — tests first (DrawCase, m3_draw, m3_attacks, DP22)
- [x] Stage 3 — implement drawer.sil (all M0-M3 tests green, debug and release)
- [x] Stage 4 — mutants, measurements, tn10 probe (no stop trigger; open items O-S4-1, O-S4-4 security-relevant)
- [x] Stage 5 — adversarial review and fixes (decisions A, B; review items 2-7; open item O-S5-1)
- [ ] Stage 6 — record and commit

## Stage 1: files changed
- `crates/lottery/Cargo.toml`: features `default = ["test-seed"]`, `test-seed`, `mainnet`.
- `crates/lottery/src/lib.rs`: D43 `compile_error!` gate; `#[cfg(feature = "test-seed")] pub mod draw;`.
- `crates/lottery/src/draw.rs` (new, stub): `compile_drawer(src, controller_hash)` (the only place
  drawer.sil is compiled, D43) and the placeholder drawer state. Stage 2 adds `DrawCase`,
  `DrawExpected`, `RoundResult`.
- `crates/lottery/src/state.rs`: `ROLE_POOL` 0x04, `ROLE_VAULT` 0x06, `ROLE_DRAWER` 0x0A,
  `TIER_JACKPOT`/`TIER_SECOND`/`TIER_LOWER0`; `DrawerState`, `VaultState`, `PoolState`
  (fields()/values()/ctor(hash)); `MarkerState::ctor(drawer_hash)` (constructor constant first).
- `crates/lottery/src/buy.rs`: `Sources` gains `vault`, `pool`, `drawer` (cfg test-seed);
  `Templates` gains `vault`, `pool`, `drawer` (cfg test-seed) and `drawer_hash()`; compile order
  ticket -> controller -> drawer(ctrl.hash) -> marker, vault, pool(drawer.hash) -> shard, params
  (ctrl.hash). Helpers `marker_redeem`, `vault_redeem/spk`, `pool_redeem/spk`,
  `drawer_redeem/spk`; dummy vault/pool states. `Template::from` and `read_covenant` are now
  `pub(crate)`.
- `crates/lottery/src/game.rs`: M3 constants (M_DRAW 50, A_MAX 5, F_MIN 1 KAS, F_MIN_MAX 10^10,
  CLAIM_WINDOW 6,048,000, DRAW_WINDOW 432,000, SEED_TAG) and `Shares::of_marker`, `draw_amounts`,
  `tier_w`, `win_counts`, `pool_set` (D45; `PoolSet`, `NewPool`), `guarded_add`, `vault_settle`,
  `boost_after`, `fee_split` (`FeeSplit`), `JackpotRule::{new, pass, pass_exact}`, `fix_jackpot`
  (bisection), `fix_jackpot_brute`, `witness_ok` (the A5 checks as the script will apply them),
  `JackpotFix`, `u56_le`, `seed_numbers` (D42; `SeedNumbers`), `claim_deadline` (D39).
- `crates/lottery/src/genesis.rs`: `GenesisCase` gains `vault`, `drawer`, `network`; outputs
  vault at k+2 and drawer at k+3 (M2 indices kept), genesis group 0..k+4; `validate()` adds the
  D49/D50/D43 checks; `set_vault`, `set_jackpot`, `vault_value`, output-index helpers;
  `GENESIS_FUNDING` (10,100,000 KAS) for fixtures; unit test extended.
- `crates/lottery/src/close.rs`: `genesis_and_ledger` unit test updated (9 outputs, vault and
  drawer checked).
- `covenants/drawer.sil` (new): state A3.1, constants A2, ctor constant controllerTemplateHash,
  entry `draw_test_seed(...)` with the full A4 argument list; ALWAYS REJECTS
  (`require(this.activeInputIndex < 0)`).
- `covenants/vault.sil` (new): state A3.2, ctor constant drawerTemplateHash, real `draw_leg`.
- `covenants/pool.sil` (new): state A3.3, real `absorb_leg`, rejecting `claim()`.
- `covenants/marker.sil`: ctor constant drawerTemplateHash; `draw_leg` replaces `locked()`.
- `covenants/controller.sil`: `draw_leg(marker_pl, marker_sl)` appended; `close()` unchanged.
- `crates/lottery/tests/common/m2.rs`: `Sil::{Vault, Pool, Drawer}`, `source_mut`/`hash_of`,
  `Chain.vault` / `Chain.drawer` from genesis outputs; genesis funded with `GENESIS_FUNDING`.
- `crates/lottery/tests/m2_mutants.rs`: `route_control` match covers the new `Sil` variants.
- `crates/lottery/tests/m3_game.rs` (new): DP17 (brute force, large values, monotone, design 4.6
  worked example), DP18 Rust side, unit tests.
- `crates/tn10/src/cases.rs`: `close_m2` scales J(1) = 10 KAS and V0 = 60 KAS (the fixture's
  10,000,000 KAS vault is not fundable on testnet); outpoint mapping unchanged and commented.

## Stage 2: files changed
- `crates/lottery/src/draw.rs`: `DrawCase` (inputs and DAA scores, `absorbed: Vec<PoolIn>`,
  funding, `lock_time`, `trees`, `test_only_seed`, `funder`, `lie: Lie`), `DrawExpected`
  (every output state and value from `game`, output-index helpers `pool_output`,
  `fee_output`, `marker_refund`, `pool_refund`, `caller_output`, `n_outputs`), `args()`
  (A4 order; `ARG_*` index constants), `drawer_leg_args`/`controller_leg_args`/`leg_args`,
  `build()`/`build_with()`, `RoundResult` (+ `ResultPool`, `pool_state`). Fixtures:
  `DrawCase::new(k)` (round 1), `at_round(k, r)`, `hold`/`hold_jackpot`/`hold_at`,
  `other_bonus`, `set_sales`, `absorb(n)`, `fix_lock_time`, `sync_roots`, `caller_formula`.
- `crates/lottery/src/smt.rs`: `proofs_at(trees, rank)` (K count strings, K sibling strings).
- `crates/lottery/src/state.rs`: `Payout::from_fields(class, key33)`.
- `crates/lottery/tests/common/m2.rs`: `accept_draw`, `strike`, `attack3`, `attack3_pair`,
  `stage2_probe`; `Chain` is `Clone` and tracks `trees`, `marker_trees`, `pools`;
  `Chain::winning`, `draw_case`, `draw_built`, `draw`; `buy_case` uses the tracked tree
  (repeat buys on a shard now work; the root-equality assert is kept).
- `crates/lottery/tests/m3_draw.rs` (new): DP1-DP16, DP17/DP18 script halves, DP19, DP20,
  DP22, `a6_g2_guard_skipped_without_second_pool`.
- `crates/lottery/tests/m3_attacks.rs` (new): DA1-DA16, `a6_g2_guard_with_second_pool`.

## Stage 3: files changed
- `covenants/drawer.sil`: `draw_test_seed` implements A4 checks 1-25 and A5 in spec order.
  Each check starts at a `// ==== check N` line (used by the attribution instrument); requires
  carry `// mut:D<check>_<what>` tags for Stage 4 (D2_a, D2_fund, D2_cov, D2_zero, D3_role,
  D3_round, D3_drawn, D3_mdraw, D4_role, D4_round, D4_k, D4_mdraw, D4_value, D5_role,
  D5_round, D5_value, D6_drawer, D6_pot, D7, D8_canon, D8_rank, D9_len, D9_root, D10_range,
  D10_n2, D13, D14_jp, D14_g2guard, D14_sp, D14_nonneg, D15_jboost, D15_lower, D15_q, D16,
  D17_x1_base, D17_x1_below, D17_x1_pass, D17_x1_max, D17_y_pot, D17_y_pass, D17_y_max,
  D17_x2_ge, D17_x2_pass, D17_x2_max, D18_role, D18_round, D18_deadline, D18_class,
  D19_own, D19_max, D20_count, D20_drawer, D20_vault, D20_ctrl, D20_pool, D20_fee, D20_refund,
  D21_zero, D21_fee, D21_mrefund, D21_prefund, D25_class, D25_pad). Checks 22-25 are
  validateOutputState* calls (no require to strip). Functions `muldiv`, `pass` (A5),
  `payoutSpk` (D11 scripts). Structs ControllerState, MarkerState, VaultState, PoolState.
- `covenants/drawer.sil` (S3-9): new last argument `int own_daa`; check 19 requires
  `own_daa == own` (the maximum OpTxInputDaaScore, after `own < DAA_MAX`; tag D19_own) and
  computes D_claim from `own_daa`.
- `crates/lottery/src/draw.rs`: `compile_drawer` is now `pub` (attribution instrument);
  `ARG_OWN` = 19 and `args()` passes `x.own`; `Lie.own` (DA17: a consistent false own, D_claim
  follows it); `RoundResult::from_tx(tx, templates) -> Result<RoundResult, String>` from the
  transaction bytes only, plus private `pushes` (push-only script parser), `script_num` and
  `state_values` (redeem -> state via `silverscript_abi::decode_runtime_state_script`).
- `crates/lottery/tests/common/m2.rs`: `M3_STAGE2_PROBE`/`stage2_probe` deleted (`accept_draw`
  and `strike` are strict); attribution instrument `M3_ATTRIB` (`attrib_enabled`,
  `drawer_truncations`, `with_leader`, `first_failing_check`), called from `strike` for input-0
  strikes only when the env var is set; `fabricate_draw` deleted; `Chain::draw_round(seed)`.
- `crates/lottery/tests/m2_close.rs`: cp5 and cp7 run a real `draw()` (`draw_round`).
- `crates/lottery/tests/m3_draw.rs`: DP1 asserts `RoundResult::from_tx(..) == RoundResult::new(..)`;
  Stage 2 status header removed.
- `crates/lottery/tests/m3_attacks.rs`: DA17 `da17_own_argument` (own_daa = own - 1, own + 1,
  the drawer input's DAA; each a consistent claim that moves D_claim) and its negative
  control `da17_negative_control_without_own_check` (drawer mutant without D19_own, case
  retargeted to the mutant's pool/vault/marker hashes: every claim accepted by every input).
  Header updated; DA10 gains one strike (look-alike
  pool prefix/suffix with the REAL template lengths, which reaches check 25; the existing
  variant that also changes pool_pl/pool_sl is caught first by check 18 and is kept).

## Stage 3: decisions (not yet in DECISIONS.md; one row each in Stage 6)
- S3-1 (A4 check 10): counts are read unsigned (2 bytes || 0x00) and required <= 32,767,
  checked on the bitwise OR of the K count strings (one m-loop instead of K x m): a slot of
  the OR is <= 32,767 iff no shard's count has its sign bit set, and is nonzero iff some
  shard's count is >= 1 (n2). h_J stays an exact sum over the K shards at the bonus slot.
  Stricter than the spec's signed [0, 32,767] only on non-canonical encodings (negative
  zero 0x0080), which a genuine leaf never holds (buy writes `(c + 1) as byte[2]`).
- S3-2 (A3/A4 ranges, as M2's Stage 3 deviation row): extra checks that reject no state the
  ranges allow and later checks accept: check 3 ctrl.m >= 2, k in [1, K_MAX], price, shares,
  e, j, g, margin, cap, acc, reserve as M2 A3; check 4 seed_ref in [0, DAA_MAX), mk.m >= 2,
  S - P_R - C_close >= 0 and J_boost <= TOTAL_MAX - (S - P_R - C_close) before the input-1
  sum; check 5 pend_dep <= TOTAL_MAX - total_assets before the input-2 sum; check 17
  0 <= x1 <= TOTAL_MAX, y >= 0, x2 <= TOTAL_MAX (so x1 + 1, y + 1, x2 + 1 cannot overflow).
- S3-3 (A6 "pool amount + R"): not guarded, cannot overflow: J, n2 x G2 (<= 49 x G2_MAX) and
  B_t are <= TOTAL_MAX = 2^62 and R <= 10^10. Every other A6 guard is written as listed.
- S3-4 (A4 check 21, absorbed-pool refunds): check 18 records blake2b(rebuilt refund
  scriptPubKey) per pool (fixed 32-byte slots, no concatenation ambiguity); check 21 compares
  blake2b(output script) with it. Avoids reading the pool states twice.
- S3-5 (A4 check 21, fee): fee_class <= 2 is checked only when the fee is paid (the
  scriptPubKey is rebuilt only then).
- S3-6 (SilverScript limit): the compiler enforces the 244-item consensus stack limit at
  compile time ("live stack bindings" / "combined stack items"). Every struct field read is a
  binding (controller 39, marker 22, vault 7) plus 19 arguments and 7 own fields, so
  temporaries live in braced blocks, which drop their locals
  (`refs/silverscript/silverscript-lang/src/compiler/compile/statement.rs:742-757`), and
  aliases were removed. Checks 8-10 and 20-21 share one block each. Headroom measured with a
  padding probe: 16 extra live ints compile, 24 do not (at check 24/25). A later change that
  adds live locals must stay under that.
- S3-7 (A4 check 1 "argument lengths in [0, 1,000,000]"): applied to the eight template
  length ints (as M2's legs). pool_prefix/pool_suffix are not length-checked (as close()'s
  marker_prefix/suffix); a wrong one fails check 25's template hash.
- S3-8 (A9): `RoundResult::from_tx` uses the transaction bytes only (S3-9 makes D_claim
  derivable); R is read from output 0 (drawer' = R, check 20).
- S3-9 USER-APPROVED (2026-10-10), SPEC CORRECTION of A4 check 19 and A9: `draw_test_seed`
  takes `own_daa` (last argument). Check 19 requires `own_daa` to equal the maximum
  OpTxInputDaaScore over all inputs (exact equality; the `own < DAA_MAX` guard kept), and
  D_claim = max(own_daa, seed_ref + draw_window) + claim_window. The round result (A9) is
  then derived from the draw() transaction bytes alone. Tests: DA17 and its negative
  control. Cost: drawer +10 B; stack headroom now 18 spare live ints (pad 18 compiles, 20
  does not).

## Stage 3: open items
- O1 RESOLVED by the user decision S3-9 (own as an argument). Stage 6: write the spec
  corrections for A4 check 19 (argument list and the equality) and A9 into the spec's
  corrections section and a DECISIONS row.
- O2 (attribution, not a test change): DA12 "bonus not from h" (a consistent `Lie.bonus`) is
  rejected by check 25, not 8: the script derives the bonus from h in check 8, so a different
  claimed bonus only appears in the new pools' win byte, which check 25 validates. With the
  base fixture the pool set and values are identical under both bonuses (h_J 1 vs 2 both give
  jp; n2 = 1 either way). Spec A8 lists DA12 under checks 7, 8.
- O3 (attribution): the pre-existing DA10 variant "look-alike pool prefix, suffix AND lengths"
  is caught by check 18 (pool_pl/pool_sl also locate the absorbed pool's template), not 25.
  Added the variant with the real lengths, which reaches 25. Both are kept.
- O4 (Stage 4 measurement flag): drawer redeem = 42,045 B after S3-9 (prefix 1 B, suffix
  41,940 B, state 104 B; 42,035 B before). Leader script units: 851,315 (K = 4, m = 10, a = 0,
  `DrawCase::new(4)`); before S3-9, 1,289,192 at K = 16, m = 10, one holding (accepted under
  committed budgets). The draw transaction also
  carries the controller redeem (~26 kB) at input 3: M3-b/M3-d must check transient and
  compute mass at K = 16, m = M_DRAW, a = A_MAX. Check 9 (16 x 18 unrolled levels) is ~20 kB
  of the drawer; a one-hash-per-level variant was tried and was larger (+3 kB), reverted.
- O5 (tests): DP1-DP22 and DA1-DA16 unchanged except the additive DA10 strike (O3) and the
  DP1 `from_tx` assertion; DA17 + negative control added (S3-9). No test was weakened.
- Stack headroom: 18 spare live ints (S3-6, S3-9). Stage 5 fixes that add live locals must
  stay under it or scope them in blocks.

## Stage 3: attack attribution (first failing A4 check, input 0)
Command: `M3_ATTRIB=1 cargo test -p lottery --release --test m3_attacks -- --nocapture
--test-threads=1 | grep ATTRIB` (196 lines). How it works (`common::m2::first_failing_check`):
drawer.sil is cut before each `// ==== check N` line (open blocks closed), compiled, swapped in
as the leader of the attack transaction (same state bytes and arguments; output 0 rebuilt when
it is the genuine successor), only input 0 runs, and a bisection finds the first truncation
that rejects. The engine error is `VerifyError` for every failed require (require messages
are not compiled), so the check number below is the evidence, not error text. DA15's error is
`ExceededCommittedScriptUnits` (asserted in the test). Strikes in test order:

| Test | Input | Strike -> first failing check |
|---|---|---|
| DA1 | 0 | all 12 (count +1/-1 at the bonus and elsewhere x5, another rank's leaf, Z_0 for a non-empty leaf, shards 1/2 swapped, k - 1 proofs, length +2/-2, sibling flipped) -> 9 |
| DA2 | 0 | jackpot pool when h_J = 0 -> 13; none when h_J = 1 -> 13; (n2 + 1) G2 -> 20; no second pool -> 13 |
| DA3 | 0 | vault' total_assets - 1 -> 23; vault value -1 / +1 -> 20, 20; vault pays J_house unheld -> 20 |
| DA4 | 0 | x1 +-1, y +-1, x2 +-1 -> 17 (x6); edge / whale / cap regimes x2 + 1 -> 17 (x3); ctrl' g2 +-1, liability +-1, jackpot +1, j_boost +1 -> 24 (x6); vault' liability +-1 -> 23 (x2) |
| DA5 | 0 | fee script -> 21; fee value +1 / -1 -> 20, 20; paid below F_MIN -> 20; carried above -> 20 |
| DA6 | 4 then 0 | pool L5 (lock = deadline - 1) and pool L4 (round r, r + 1) at input 4; drawer: lock = deadline - 1 -> 18; pool round r, r + 1 -> 18, 18; Q not in O' -> 20 |
| DA7 | 0 | unheld J_boost to the vault -> 20; unpaid B_3 to the vault -> 20 |
| DA8 | 1 then 0 | marker L3 at input 1; drawer: parallel auth (pool output by input 1) -> 13; extra covenant output -> 13; extra plain output -> 20; caller output with a covenant ID -> 21; funding input with a covenant ID -> 2; pools swapped -> 20; marker refund missing -> 20; pool refund missing -> 20; no caller output -> 20; a = 6 -> 2; 9 funding inputs -> 2 |
| DA9 | 0 | marker refund script -> 21; pool refund script -> 21; refunds swapped -> 21; each refund +1 / -1 -> 20 (x4) |
| DA10 | 1, 3, 2 then 0 | leg strikes as in Stage 2; drawer: marker round r - 1 -> 4; controller round r + 2 -> 3; drawn = 1 -> 3; vault round r -> 5; look-alike pool input -> 18; look-alike pool output (real prefix) -> 25; look-alike prefix/suffix with real lengths (added) -> 25; look-alike prefix/suffix and lengths -> 18 (O3); look-alike controller -> 3 |
| DA11 | 1-4 | leg strikes only (L1-L5), no drawer strike; each rejects at the named leg input |
| DA12 | 0 | other rank -> 8; reversed win5 -> 8; win5 byte flipped -> 8; bonus not from h -> 25 (O2); other seed -> 8; lock = seed_ref - 1 -> 7 |
| DA13 | 0 | per pool (5 pools): slice +-1, tier, win[0], win[5], m, k, roots, round, deadline +-1, funder -> 25 (x12); value +-1 -> 20 (x2); funder class 3 -> 25; key padding -> 25 |
| DA14 | 0 | controller' x14 (drawn, v_rec, acc, round, m, price, k, deadline, s_f, fee_key, marker_tpl, shard_tpl, cap, reserve) -> 24; drawer' x7 (round -1/+1, pool_tpl, vault_tpl, f_min, claim_window, draw_window) -> 22 (output 0 edited: exact, checks 23-25 do not read output 0); vault' x4 (total_shares, pend_dep, pend_wsh, round) -> 23; outputs 0-2 value +-1 -> 20 (x6) |
| DA15 | 0 | committed budget - 1 -> `ExceededCommittedScriptUnits` |
| DA16 | 0 | marker value +-1 -> 4, 4; vault value +-1 -> 5, 5; drawer value +1 -> 6 |
| DA17 | 0 | own_daa = own - 1, own + 1, the drawer input's DAA -> 19, 19, 19. Before the D19_own require existed (argument only) the same three were caught only at 25 (wrong D_claim in the pools); with the require stripped (mutant D19_own) all three are ACCEPTED by every input (negative control test) |
| A6 G2 guard | 0 | G2 = TOTAL_MAX/254 + 1 with n2 = 1 -> 14 |

## Stage 3: test runs (from `crates/`)
Final run, after S3-9:
- `cargo build --tests` (debug): OK, only the two pre-existing m0_mass unused-variable warnings.
- `cargo test --no-fail-fast` (debug, full workspace, exit 0): harness covenant_smoke 3,
  m0_limits 3, m0_mass 5, m0_merkle 2, m0_shapes 6; lottery unit 19, m1_attacks 25, m1_buy 14,
  m1_measure 4, m1_mutants 11, m2_attacks 40, m2_close 8, m2_game 2, m2_measure 5,
  m2_mutants 25, m2_params 17, m3_attacks 19, m3_draw 22 (incl. dp22), m3_game 13. 0 failed.
- `M3_ATTRIB=1 cargo test -p lottery --release --test m3_attacks -- --nocapture
  --test-threads=1`: 19 passed; attribution table above (unchanged except DA17).
Earlier in Stage 3 (before S3-9): the same suite in debug and `cargo test -p lottery
--release` were green (m3_attacks 17).
- Not run in Stage 3: tn10 probes; m3_mutants and m3_measure (done in Stage 4, below).

## Stage 4: files changed
- `crates/lottery/tests/common/m3.rs` (new): `Atk` (a valid draw, one case edit, one
  transaction edit; `pair(t)` builds both under any templates after `retarget`, `strike()`
  runs them against the committed templates), `retarget` (drawer pool/vault_tpl,
  controller marker/shard/params_tpl), the m3_attacks bases (`base`, `base_no_jp`,
  `base_no_absorb`, `base_funded`), every attack the mutants use (existing DA strikes
  re-expressed, plus the Stage 4 additions), the A5 `regime` search and witness refits.
- `crates/lottery/tests/common/mod.rs`: `pub mod m3;`.
- `crates/lottery/tests/m3_attacks.rs`: `use common::m3 as s4;`. 36 strikes were appended,
  each marked `(Stage 4 addition, mutant gap)`, to DA1, DA2, DA3, DA4, DA7, DA8, DA9,
  DA10, DA11, DA12, DA13 and DA16. No existing strike was changed or removed. Test count
  unchanged (19).
- `crates/lottery/tests/m3_mutants.rs` (new): 89 tests (below).
- `crates/lottery/tests/m3_measure.rs` (new): DP21, M3-a to M3-f (7 tests).
- `crates/tn10/src/cases.rs`: `wait_daa`, `ours`, `draw_m3` (with the node-entry check before
  the draw). `crates/tn10/src/main.rs`: `probe draw-m3`, the usage string, `info` lists UTXOs
  with creation DAA scores.
- `docs/costs.md`: M3 section (M3-a to M3-f, limits cited to refs); the M2-c line about
  `draw()` now points to M3-d.
- `docs/tn10-mass-check.md`: M3-g section.
- `CLAUDE.md`: M3 test, measurement and attribution commands; `close-m2` and `draw-m3` in
  the probe list.

## Stage 4: mutants (`m3_mutants.rs`, 89 tests: 85 tags plus extras)
Mechanism as M2. Each `killed` test does two things:
- it runs the attack against the committed templates (`Atk::strike`: rejected by the named
  input with a script error, the base passes that input and every input of the base);
- it rebuilds the base and the attack under the mutant, which is compiled in memory with
  only the tagged lines removed, so the sources on disk are untouched. The mutant accepts
  the base, and the named input ACCEPTS the attack. So the m3_attacks strike named below
  fails under the mutant.

Variants:
- `killed_assign`: an assignment-line mutant changes what the script computes. Under it
  the VALID draw is rejected by the named input (every positive test fails), and the
  attack, which matches the mutant's computation, is accepted.
- `weakened_*` (D16, which tags a declaration): the line is replaced by `bool paid =
  true;` or `= false;`.
- `redundant_*`: the mutant still rejects the attack at the named input, because another
  check of the same script implies the removed one. The argument for each is below.
- `every_m3_tag_is_tested`: every `// mut:` tag in drawer, marker, vault, pool and the
  controller's CL* tags has a test, and the count is 85 (63 drawer + 22 legs).

| Tag | Attack (`common::m3`) | Killed by (m3_attacks test) | Kind |
|---|---|---|---|
| D2_a | `a_negative`: a = -1 (controller without the ID, a = 0 base, caller output dropped) | da8_shape (S4) | killed |
| D2_a (upper half a <= A_MAX) | `a_above_max` (a = 6) | — | redundant: the loop guards `require(end - start <= max)` (silverscript-lang `compiler/for.rs:136-150`) of `for(ci, 1, 4 + a, 3 + A_MAX)` and the A_MAX loops |
| D2_fund | `no_funding` (n_fund = 0; sales 100,000 KAS so C_draw covers the reserves) | da8_shape (S4) | killed |
| D2_fund (upper half) | `nine_funding` | — | redundant: the funding loop is bounded by F_MAX |
| D2_cov | `marker_other_cov` | — | redundant: a := OpCovInputCount(cov) - 4 and D2_zero force all of inputs 0..3 + a to carry cov (pigeonhole) |
| D2_zero | `funding_other_cov` | da8_shape | killed |
| D3_role | `ctrl_role` (input and controller' role 0x07) | da10 (S4) | killed |
| D3_round | `ctrl_round` | da10 | killed |
| D3_drawn | `ctrl_drawn` | da10 | killed |
| D3_mdraw | `ctrl_m_above` (m' = 51) | da10 (S4) | killed |
| D4_role | `marker_role` | da10 (S4) | killed |
| D4_round | `marker_round_field` (round field only; the case-level DA10 strike also moves the seed, which check 8 catches) | da10 (S4) | killed |
| D4_k | `marker_k` | da10 (S4) | killed |
| D4_mdraw | `marker_m_above` (m = 51 with consistent trees) | — | redundant: the check 10 loop `for(b, 0, mk.m, M_DRAW)` guard |
| D4_value | `input_value(MK, +1)` | da16 | killed |
| D5_role | `vault_role` (input and vault' role 0x07) | da10 (S4) | killed |
| D5_round | `vault_round` | da10 | killed |
| D5_value | `input_value(VLT, -1)` | da16 | killed |
| D6_drawer | `input_value(DRW, +1)` | da16 | killed |
| D6_pot | `pot_negative` (controller worth acc + R - 1) | da16 (S4) | killed |
| D7 | `early_lock` | da12 | killed |
| D8_canon | `win5_noncanonical` ((n1 + C(n2-1, 2), 1, n3, n4, n5), same colex rank) | da12 (S4) | killed |
| D8_rank | `win5_other_rank` (canonical win5 of rank + 1, genuine proofs of the seed's rank) | da12 (S4) | killed |
| D9_len | `leaves_longer` | da1 | killed |
| D9_root | `forged_leaf` (shard 0 claims one more second-prize bonus, roots genuine) | da1 (S4) | killed |
| D10_range | `count_sign_bit` (leaf bytes 0x00 0x80 with a matching forged root) | da2 (S4) | killed |
| D10_n2 | `n2_zero_claim` | da2 | killed_assign |
| D13 | `parallel_auth` | da8 | killed |
| D14_jp | `vault_keeps_jhouse` | da3 (S4) | killed_assign |
| D14_g2guard | `g2_guard` | a6_g2_guard_with_second_pool | killed |
| D14_sp | `vault_keeps_second` | da3 (S4) | killed_assign |
| D14_nonneg | `vault_negative` (V' = -1: vault 1 sompi short of an exact-0 settlement) | da3 (S4) | killed |
| D15_jboost | `jboost_to_caller` | da7 (S4) | killed_assign |
| D15_lower | `lower_to_caller` | da7 (S4) | killed_assign |
| D15_q | `q_to_caller` | da6 | killed_assign |
| D16 (weakened `paid = true`) | `fee_paid_below` | da5 | killed_assign |
| D16 (weakened `paid = false`) | `fee_carried_above` | da5 | killed_assign |
| D17_x1_base | `x1_below_base` (x_b passes, x1 = x_b - 1, y and x2 refit) | da4 (S4) | killed |
| D17_x1_below | `x1_at_failing_base` | — | redundant: pass is non-increasing in x, so x1 >= x_b fails pass (D17_x1_pass) |
| D17_x1_pass | `x1_failing` | — | redundant: pass(x1, y) (D17_y_pass) implies pass(x1, 0) |
| D17_x1_max | `x1_not_largest` (refit) | da4 (S4) | killed |
| D17_y_pot | `y_above_pot` (largest passing y > O') | da4 (S4) | killed |
| D17_y_pass | `y_failing` | — | redundant: x2 >= x1 and pass(x2, y) imply pass(x1, y) |
| D17_y_max | `y_not_largest` (refit x2) | da4 (S4) | killed |
| D17_x2_ge | `x2_below_x1` | — | redundant: x2 < x1 with pass(x1, y) gives pass(x2 + 1, y), which D17_x2_max rejects |
| D17_x2_pass | `x2_plus_one` | da4 | killed |
| D17_x2_max | `x2_minus_one` (regime with x2 > x1) | da4 (S4 regime) | killed |
| D18_role | `pool_role` | da10 (S4) | killed |
| D18_round | `pool_round_r` | da6 | killed |
| D18_deadline | `before_deadline` | da6 | killed |
| D18_class | `pool_class3` (refund to the P2SH form payoutSpk gives class 3) | da9 (S4) | killed |
| D19_own | `own_lie` | da17 (and its negative control) | killed |
| D19_max | `deadline_without_draw_window` (old inputs only; pools at own + CW) | da13 (S4) | killed_assign |
| D20_count | `extra_plain` | da8 | killed |
| D20_drawer / vault / ctrl | `output_value` 0 / 1 / 2 | da14 / da3 / da14 | killed |
| D20_pool | `output_value` of the jackpot, second and a lower pool | da13 | killed (x3) |
| D20_fee | `output_value(fee)` | da5 | killed |
| D20_refund | marker refund, pool refund | da9 | killed (x2) |
| D21_zero | `caller_with_cov` | da8 | killed |
| D21_fee / mrefund / prefund | `output_script` | da5 / da9 / da9 | killed |
| D25_class / D25_pad | `new_pool_class3` / `new_pool_padding` | da13 | killed |
| ML1 / VL1 | marker at input 2 / vault at input 1 (swap) | da11 | killed |
| CL1 / PL1 | controller at input 4 / pool at input 3 (swap) | da11 | killed |
| ML2 VL2 CL2 PL2 | `other_leader` | da11 | killed |
| ML3 VL3 CL3 PL3 | `leg_authorizes` | da11 | killed |
| ML4_role VL4_role PL4_role | `drawer_role` (real drawer template, role 0x07) | da11 (S4) | killed |
| CL4_role | `marker_role().at(CTL)` | da11 (S4) | killed |
| ML4_round / CL4_round | `marker_round` at MK / CTL | da10 | killed |
| VL4_round | `vault_round` at VLT | da10 | killed |
| PL4_round / PL5 | `pool_round_r` / `before_deadline` at POOL0 | da6 | killed |
| CL5 | `ctrl_drawn` at CTL | da10 | killed |

Summary:
- All 85 tags are killed, or killed in one half with the other half shown redundant
  (D2_a, D2_fund), except 6 tags that are fully redundant: D2_cov, D4_mdraw,
  D17_x1_below, D17_x1_pass, D17_y_pass and D17_x2_ge.
- The redundant checks were kept as defense in depth (rule: never delete a check because
  its mutant survives).
- Stage 5 should re-check the redundancy arguments. Each rests on the loop guards or on
  pass being non-increasing in x and in y (DP17 tests monotonicity).

Attribution of the Stage 4 strikes (`M3_ATTRIB=1`, input 0 only), first failing check:

| Test | Checks |
|---|---|
| da1 | forged leaf: 9 |
| da2 | count 0x8000: 10 |
| da3 | keeps J_house: 20; keeps n2 G2: 20; V' = -1: 14 |
| da4 | all nine witness strikes: 17 |
| da7 | J_boost to the caller: 20; B_t to the caller: 20 |
| da8 | a = -1, no funding, marker with another ID: 2, 2, 2 |
| da9 | pool class 3: 18 |
| da10 | ctrl role 3, ctrl m' 3, marker role 4, marker round field 4, marker k 4, marker m 4, vault role 5, pool role 18 |
| da12 | non-canonical: 8; other rank: 8 |
| da13 | D19_max case: 25 |
| da16 | O = -1: 6 |

The DA11 additions target legs (inputs 1, 2, 3, 4), so no attribution applies.

## Stage 4: measurements (`m3_measure.rs`, recorded in docs/costs.md M3)
Limits verified in refs (rusty-kaspa v2.1.0): `consensus/core/src/config/params.rs:626`
(mainnet) and `:683` (testnet): compute 500,000, storage 500,000, transient 1,000,000.
Enforced per block in `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:88-95`.
KIP-9 at `consensus/core/src/mass/mod.rs:439-512`, plurality `:83-99`, C at
`consensus/core/src/constants.rs:26`. Zero-value output invalid at
`consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs:150-151`.

| ID | Result | % of limit (compute / transient / storage) |
|---|---|---|
| M3-a | drawer 42,045 B; marker 1,326 (was 756); vault 594; pool 1,228; controller 27,694 (was 26,321) | — |
| M3-b | worst row (np 7, K 16, m 50, a 5): drawer 1,426,348 units, sigscript 53,611 B; compute 134,134, transient 365,816, storage 59,175 | 26.8 / 36.6 / 11.8 |
| DP21 | K 16, m 50, a 5, np 5 / 7, 1 or 8 funding inputs: worst compute 141,974, transient 369,176, storage 59,175 | 28.4 / 36.9 / 11.8 |
| M3-c | drawer B by K_MAX x M_DRAW: (16, 50) = 42,045; (16, 100) = 46,145; (4, 10) = 23,928. Worst shape at m = 50: 28.4 / 36.9 / 11.8 %; m_max = M_DRAW = 50 stands | — |
| M3-d | storage supremum 292,182 at a = 5 (10 covenant outputs at R, 6 refunds at R, fee at F_MIN, caller 1 KAS); grid worst 281,979 | storage 58.4 (sup) / 56.4 (grid) |
| M3-e | priority fee 0.1522 (K 4) to 0.1846 KAS (K 16 worst); C_draw (50 bp) covers it from S = 30.4 to 36.9 KAS; storage worst 0.2922 KAS (S = 58.4 KAS) | — |
| M3-f | K = 16 close(): compute 293,841, transient 864,604 (+7,780 over M2), storage 402,301 | 58.8 / 86.5 / 80.5 |

No stop trigger fired:
- M3-f is 86.5 % (below 90 %);
- M3-d is 58.4 % (below 90 %);
- m_max did not drop (it stays at 50).

R stays 1.69 KAS (draw alone needs R >= 1.07 KAS).

## Stage 4: decisions (not yet in DECISIONS.md; one row each in Stage 6)
- S4-1 (spec ambiguity, measured both): the spec's DP21/M3-b/M3-d say "np = 5 / 5 fresh
  pools", but the script allows np = 7 (jackpot, second prize, 5 lower tiers; check 13's
  loop bound 10 = 3 + 7). The fixture funds only 3 tiers (st = 400, 300, 300, 0, 0). Both
  were measured; np = 7 (`ST_ALL` = 400, 300, 300, 200, 200) is the worst and is used for
  M3-c, M3-d (supremum 10·4C/R) and the DP21 assertion. The spec's corrections section
  should say np <= 7.
- S4-2 (m_max, M3-c): unchanged at M_DRAW = 50. The worst draw at m = 50 is 36.9 %
  transient. M_DRAW = 100 is not runnable with the committed drawer (128-byte zero buffer
  covers 2m <= 128); an estimate is 39.2 % transient.
- S4-3 (R, M3-d): unchanged at 1.69 KAS (close() sets it). The draw supremum assumes the
  caller output >= 1 KAS (MIN_CALLER_OUTPUT as a builder policy; DrawCase does not enforce
  it, the caller's own output only harms the caller).
- S4-4 (test structure): Stage 4 attacks live in `common::m3` and are shared by
  m3_attacks (real templates) and m3_mutants (mutants), so a mutant test runs exactly the
  strike it names.
- S4-5 (tn10 probe): draw-m3 sets T = virtual DAA + 600, not below it as close-m2 does.
  buy() requires the shard created before T (shard.sil:151), and close() needs T final,
  so the probe waits for T, then for seed_ref (+600). The first attempt with T below the
  current DAA had its buy rejected by the LOCAL engine before submission; only its
  genesis went to testnet (about 95 KAS stay locked there, see tn10-mass-check.md).

## Stage 4: open items
- O-S4-1 RESOLVED in Stage 5 by user decision A (S5-1). Original note:
- O-S4-1 (SECURITY-RELEVANT, liveness; user decision): the genesis builder and the
  drawer's A3.1 ranges accept 0 <= f_min <= 10^10, which causes two problems.
  - With f_min = 0, a round with T_F = acc + F = 0 (no sales, acc 0) must pay a 0-sompi
    fee output (check 20: paid = T_F >= f_min). A zero-value output is consensus-invalid
    (tx_validation_in_isolation.rs:150-151), so that round can never be drawn, and every
    later round is blocked (close() waits for draw()).
  - With 0 < f_min < 0.0596 KAS, a T_F in [f_min, 0.0596 KAS) can push draw() storage
    above 90 %. Below about 0.0072 KAS (C / T_F > 500,000 − 292,182 + 10,000), the draw
    exceeds the limit and is impossible for the same reason.
  - Proposed: genesis (and the A3.1 range) requires f_min >= a floor, e.g.
    f_min >= 0.06 KAS at R = 1.69 KAS (M3-d), or simply f_min = F_MIN = 1 KAS fixed.
  - Not changed in Stage 4 (design/spec decision). Stage 5 reviewer: confirm.
- O-S4-2 (spec, not security; correction listed in Stage 5): "np = 5" in DP21/M3-b/M3-d vs the script maximum 7 (S4-1).
- O-S4-3 (tooling): M_DRAW > 64 would need a larger zero buffer in drawer.sil (check 9's
  `zeros`, 128 B). Not relevant while M_DRAW = 50.

## Stage 4: test runs (from `crates/`)
- `cargo test --no-fail-fast` (debug, full workspace, exit 0):
  - harness: covenant_smoke 3, m0_limits 3, m0_mass 5, m0_merkle 2, m0_shapes 6;
  - lottery: unit 19, m1_attacks 25, m1_buy 14, m1_measure 4, m1_mutants 11, m2_attacks 40,
    m2_close 8, m2_game 2, m2_measure 5, m2_mutants 25, m2_params 17, m3_attacks 19,
    m3_draw 22 (incl. dp22), m3_game 13, **m3_measure 7, m3_mutants 89** (41 s in debug).
  - 0 failed.
- `cargo test -p lottery --release --test m3_mutants`: 89 passed (8.4 s).
- `cargo test -p lottery --release --test m3_attacks`: 19 passed. The 36 new strikes are
  rejected by the committed scripts.
- `M3_ATTRIB=1 cargo test -p lottery --release --test m3_attacks -- --nocapture --test-threads=1`:
  19 passed, 226 ATTRIB lines; the Stage 4 rows are in the table above.
- `cargo test -p lottery --release --test m3_measure -- --nocapture --test-threads=1`:
  7 passed; output as recorded in docs/costs.md M3.
- `cargo build -p tn10`: OK. `cargo run -p tn10 -- probe draw-m3` was run three times,
  on testnet-10 only, with the `.env` key (never printed). Node network `testnet-10`.

## Stage 4: testnet-10 (M3-g, docs/tn10-mass-check.md)
- Run 3 was ACCEPTED: genesis `1737e827…1c86`, buy `ad8dc585…2d60`, close
  `90da213e…8b3e` (K = 16: 293,841 / 864,604 / 426,540; matches M3-f) and **draw
  `459791070e5462e3256d7cab594f2b581105248b100de4636d977c941f901206`** (compute 107,048,
  transient 332,712, storage 130,534, fee 0.1665 KAS; np = 4, jackpot held).
  - `RoundResult::from_tx` on the mined draw: win (3, 19, 22, 25, 27, 7), D_claim
    599,651,609.
  - The node's UTXO entries of all 5 inputs equal the builder's.
- Run 1: the buy was rejected by the LOCAL engine (T below the current DAA, S4-5); only
  its genesis was submitted.
- Run 2: genesis, buy and close were accepted, but the draw `6a17386a…cf7` was REJECTED
  by the node with VerifyError although the local engine accepted it. See O-S4-4.
- Probe additions after run 2:
  - an entry check: fetch each input's UTXO entry from the node, compare it with the
    builder's, and re-run the local engine on the node's entries;
  - `info` now lists each UTXO with its creation DAA score.
- Balance after: 8,648.65 testnet KAS.

## Stage 4: open item O-S4-4 (SECURITY-RELEVANT, liveness) — EXPLAINED and handled in Stage 5 (S5-5)
- **What happened.** Run 2's draw was rejected on testnet-10 by a failed `require`, but
  the identical transaction passed the local v2.1.0 engine on the builder's entries. The
  cause is unknown.
- **Observation.** Run 2's close was read (0.4 s after the caller output appeared in the
  UTXO index) with creation DAA 593,168,541, which is below its own seed_ref 593,168,551.
  Run 3's close read 593,171,615, above its seed_ref 593,171,609.
- **Hypothesis.** The young close outputs' creation DAA score changed after the read, for
  example through a virtual-chain reorganisation that changed the merging block. Check 19
  then fails, because it requires `own_daa` to EQUAL the maximum input DAA score (S3-9),
  and D_claim in the pools depends on it too.
- **Could not verify.** The run 2 caller output was spent as run 3's genesis funding
  before it could be re-read.
- **If confirmed:**
  - A draw whose youngest input is very recent can fail, but it does not lose funds; the
    caller retries.
  - Builder policy: build draw() from inputs that are several seconds or confirmations
    deep, and re-read their DAA scores just before signing (run 3 did this).
  - No consensus or security impact on funds is known.
- **Stage 5:** confirm by spending only deep inputs, or by repeating with an immediate
  draw after close.

## Stage 5: review fixes (covenant reviewer findings + user decisions A, B)

### Files changed
- `covenants/drawer.sil`: constant `F_MIN_FLOOR = 10000000`; check 1 `require(f_min >= F_MIN_FLOOR); // mut:D1_fmin` (was `>= 0`); tags `// mut:D12_jp`, `// mut:D12_sp` on the check 12 declarations (no logic change). Redeem 42,045 -> 42,049 B.
- `crates/lottery/src/game.rs`: `F_MIN_FLOOR` (0.1 KAS) and `G2_MAX` (TOTAL_MAX / 254).
- `crates/lottery/src/genesis.rs`: `validate()` adds f_min >= F_MIN_FLOOR, drawn == 1, j_boost <= jackpot, g2 <= G2_MAX; the share-room sum and the L(1) formula are i128; `set_jackpot` uses checked arithmetic. New unit test `genesis_refuses_stage5_review_instances`.
- `crates/lottery/tests/common/m3.rs`: attacks `y_pot_x2_below_x1`, `f_min_below_floor`; positive cases `zero_jackpot_held`, `zero_g2_held`.
- `crates/lottery/tests/m3_attacks.rs`: `da18_f_min_below_floor`, `da4_out_of_order_witness` (21 tests).
- `crates/lottery/tests/m3_draw.rs`: `dp23_zero_sales_round_at_f_min_floor`, `dp24_zero_prizes_held` (24 tests).
- `crates/lottery/tests/m3_mutants.rs`: `killed_positive`; `d1_fmin`, `weakened_d12_jp`, `weakened_d12_sp`, `d17_y_pass_and_x2_ge_together`; tag count 85 -> 88 (93 tests).
- `crates/lottery/tests/m3_measure.rs`: M3-d also measured at f_min = F_MIN_FLOOR (asserts <= 90 % per shape and for the supremum); M3-e prints the supremum fee at the floor.
- `crates/tn10/src/cases.rs`: `sink`, `merged` (chain-merged node entries); `draw_m3` builds every DAA-dependent transaction from them, re-reads the draw inputs before submission and rebuilds on change; `ours` removed.
- `docs/costs.md`: M3 numbers re-run (drawer +4 B), M3-d floor table, M3-e row. `docs/tn10-mass-check.md`: O-S4-4 explanation, run 4.

### Naming note (decision A)
The user called the floor "F_MIN = 0.1 KAS". The code already had `F_MIN` = 1 KAS (spec D41: the default fee threshold every fixture uses). The floor is named `F_MIN_FLOOR` (10,000,000 sompi) in `game.rs` and `drawer.sil`; `F_MIN` (1 KAS) stays the default f_min. If the user meant to lower the default to 0.1 KAS as well, that is a one-line change plus DP12/DP13/M3-d fixture updates (not done).

### Item -> test -> result
| Item | Fix | Test(s) | Failing first (right reason) | After |
|---|---|---|---|---|
| A f_min floor, drawer | check 1 `f_min >= F_MIN_FLOOR` (D1_fmin) | m3_attacks `da18_f_min_below_floor` (f_min = floor - 1, 0, -1 rejected; floor accepted) | "ATTACK ACCEPTED by input 0" | pass; ATTRIB check 1 (x3) |
| A mutant / negative control | — | m3_mutants `d1_fmin` (guard removed in memory: the attack is accepted by input 0) | — | pass (killed) |
| A f_min floor, genesis | `range("f_min", d.f_min, F_MIN_FLOOR, F_MIN_MAX)` | unit `genesis_refuses_stage5_review_instances` (floor - 1 and 0 refused, floor accepted) | "f_min = F_MIN_FLOOR - 1: bad instance accepted" | pass |
| A other paths | none needed: f_min is set only at genesis and copied unchanged into drawer' (check 22); `set_params` writes only next s_F, s_R and fee address (`params.sil:159-162`); marker/vault/pool legs read only the drawer's role and round | — | — | — |
| A positive, end to end | — | m3_draw `dp23_zero_sales_round_at_f_min_floor`: genesis f_min = floor, acc 0 -> close (0 sales) -> draw (fee carried, no fee output, every output > 0, drawer' keeps f_min) -> round 2 close. Rust side: the same round at f_min = 0 needs a 0-sompi fee output | (positive) | pass |
| A storage | — | m3_measure `m3d_reserve_recheck` at T_F = F_MIN_FLOOR: worst measured 371,979 (74.4 %), supremum 382,182 (76.4 %) <= 90 % | — | pass |
| B G2 genesis | `g2 <= G2_MAX` | unit test: G2_MAX accepted, G2_MAX + 1 refused (L(1) consistent, vault TOTAL_MAX) | "G2 = G2_MAX + 1: bad instance accepted" | pass |
| 2 j_boost <= jackpot | genesis check | unit test (J 1 KAS, J_boost 2 KAS, pot 2 KAS, L(1) consistent) | "j_boost > jackpot: bad instance accepted" | pass |
| 3 overflow | i128 room and L(1) | unit test (G2 = 2^62, m = 5: 2^64). Negative control with the old i64 code and the G2 check off: debug "attempt to multiply with overflow" (validate panicked), release "bad instance accepted" | yes (both profiles) | pass |
| 4 drawn == 1 | genesis check | unit test (drawn = 0) | "drawn = 0: bad instance accepted" | pass |
| 5 D45 zero prizes | tags only | m3_draw `dp24_zero_prizes_held` (J = 0 with h_J >= 1: no jackpot pool; G2 = 0 with n2 >= 1: no second pool); m3_mutants `weakened_d12_jp` (`bool jp = hJ >= 1;`), `weakened_d12_sp` (`bool sp = n2 >= 1;`) | — | pass; both mutants reject the valid draw at input 0 (VerifyError): killed |
| 6 tn10 O-S4-4 | `merged` + rebuild loop | `cargo run -p tn10 -- probe draw-m3` (run 4) | — | ACCEPTED: draw `8b9d0a0c…8305`; all 5 node entries = builder entries, each DAA = its merging chain block's |
| 7 D17 pair | none (both kept) | m3_attacks `da4_out_of_order_witness` (y = O' above the largest passing y, x2 = the largest x passing with O', below x1); m3_mutants `d17_y_pass_and_x2_ge_together`: either one removed still rejects (redundant), BOTH removed accepts (killed) | — | pass; ATTRIB check 17 |

### Item 7 note (mutant table)
D17_y_pass and D17_x2_ge are each redundant only because the other is present (the Stage 4 redundancy arguments for them lean on each other). With both removed, the witness (x1, O', x2 < x1) in the capped regime passes every remaining A5 check. Both are kept and back each other up; the pair mutant is killed by `da4_out_of_order_witness`.

### Decision B: admin paths
No admin path can change g, G2, the m range or f_min: `set_params` (params.sil) writes only next_s_f, next_s_r and the next fee address. G2 changes only in draw(): G2' = muldiv(x2 + y, g) (check 17), which the covenant does not bound by G2_MAX (open item O-S5-1).

### Stage 5 decisions (not yet in DECISIONS.md; one row each in Stage 6)
- S5-1 USER-APPROVED (decision A): f_min >= F_MIN_FLOOR = 0.1 KAS in the drawer's A3.1 range check and in genesis `validate()` (named F_MIN_FLOOR; the F_MIN = 1 KAS default is kept, see the naming note).
- S5-2 USER-APPROVED (decision B): the covenant bound stays G2 <= TOTAL_MAX / 254 (only when a second-prize pool is created); genesis also requires G2(1) <= G2_MAX.
- S5-3 (review 2-4): genesis requires J_boost <= J and drawn = 1; `validate()` arithmetic in i128.
- S5-4 (review 5): tags D12_jp / D12_sp and the `killed_positive` mutant kind.
- S5-5 (review 6, O-S4-4): builder policy: DAA-dependent transactions (close, draw) spend only inputs a chain block has merged, with the node's entry; re-read before submission, rebuild on change.
- S5-6 (review 7): the D17_y_pass / D17_x2_ge pair is recorded as mutual backup; both kept.

### Spec corrections to write in Stage 6 (docs/specs/3.md corrections section)
- O-S4-2: DP21, M3-b and M3-d say np = 5; the script maximum is np <= 7 (jackpot, second prize, five lower tiers).
- A3.1 drawer ranges: F_MIN_FLOOR <= f_min <= 10^10 (was 0 <= f_min), F_MIN_FLOOR = 0.1 KAS (decision A).
- D50 / genesis: G2(1) <= TOTAL_MAX / 254, J_boost(1) <= J(1), controller drawn = 1, f_min >= F_MIN_FLOOR.
- Still to write from earlier stages: S3-9 (own_daa in A4 check 19 and A9); the A6 G2 guard applies only when a second-prize pool is created.

### Stage 5 open items
- O-S5-1 (liveness at very large scale; not changed, decision B keeps the covenant bound): draw() check 17 sets G2' = muldiv(x2 + y, g) without capping it at G2_MAX. A later round with G2 > G2_MAX and n2 >= 1 would be rejected by the A6 guard forever. Reaching it needs x2 + y >= 10 x G2_MAX (g <= 10 %), about 1.8 x 10^9 KAS of jackpot, with the vault backing G2 (m' - 1) <= cap x V' (V' >= 1.8 x 10^8 KAS). W <= PRICE_MAX x N x M_DRAW ~ 7.1 x 10^17 sompi does not exclude it. Unreachable in practice but not excluded by an invariant. Options: cap G2' in check 17 (G2' <= G2_MAX as part of `pass`), or accept and record. Needs a design decision; not implemented.
- O-S4-1: RESOLVED by decision A (S5-1).
- O-S4-4: EXPLAINED (virtual-time vs chain-block DAA score of young outputs, see tn10-mass-check.md) and handled in the probe (S5-5). Run 4 was accepted. No entry moved during run 4, so the explanation rests on the source reading, not on an observed change.

### Stage 5 test runs (from `crates/`)
- `cargo test --no-fail-fast` (debug, full workspace, exit 0, 0 failed): harness covenant_smoke 3, m0_limits 3, m0_mass 5, m0_merkle 2, m0_shapes 6; lottery unit 20, m1_attacks 25, m1_buy 14, m1_measure 4, m1_mutants 11, m2_attacks 40, m2_close 8, m2_game 2, m2_measure 5, m2_mutants 25, m2_params 17, m3_attacks 21, m3_draw 24, m3_game 13, m3_measure 7, m3_mutants 93.
- `cargo test -p lottery --release --test m3_measure -- --nocapture --test-threads=1`: 7 passed; numbers in docs/costs.md M3 (drawer 42,049 B; DP21 worst 141,978 / 369,192 / 59,175; M3-d supremum 292,182 at f_min = 1 KAS, 382,182 = 76.4 % at F_MIN_FLOOR; M3-f unchanged).
- `M3_ATTRIB=1 cargo test -p lottery --release --test m3_attacks -- da18 da4_out --nocapture --test-threads=1`: da18 -> check 1 (x3); da4_out_of_order_witness -> check 17.
- Failing-first runs: genesis unit test failed on each new check in turn (f_min, j_boost, drawn, G2) before each fix; da18 failed with "ATTACK ACCEPTED by input 0" before the drawer change; overflow negative control as in the table.
- `cargo build -p tn10` clean; `cargo run -p tn10 -- probe draw-m3` once (run 4, testnet-10 only, key from `.env`, never printed): genesis `a315cdcd…36ac`, buy `893559d2…81cc`, close `3ca43680…95fd`, draw `8b9d0a0c913e371db9bebe12ab9fc6e0c07e181504a1a910c2d2dd4d26db8305` ACCEPTED (107,052 / 332,728 / 130,534).
- Not run in Stage 5: the full release suite (only m3_measure and the m3_attacks attribution in release).

## Decisions not yet in DECISIONS.md
- USER-APPROVED (2026-10-10, spec A6 "n2·G2"): the guard G2 <= TOTAL_MAX/254 applies ONLY
  when a second-prize pool is created (n2 > 0, i.e. sp). A draw with G2 = TOTAL_MAX/254 + 1
  and n2 = 0 must succeed (`m3_draw::a6_g2_guard_skipped_without_second_pool`); with n2 > 0
  it must be rejected and G2 = TOTAL_MAX/254 accepted (`m3_attacks::a6_g2_guard_with_second_pool`).
  The spec's corrections section must record this (A6 currently reads as unconditional).
- (Stage 2, A8) Attack helper semantics: `strike(good, bad, i)` = bad rejected by input i with
  a script error AND good passes input i; every attack test ends with `accept_draw(base)`.
  Leg-targeted strikes are placed before drawer-targeted ones in each test.
- (Stage 2) `Lie`: TEST-ONLY consistent false claims (counts, rank, win5, bonus, witness,
  paid, v_delta, o_delta). The builder follows the claim everywhere (pools, J fixing,
  values), so the attack is one change and only the check catching the claim can reject.
- (Stage 2) The caller output is computed in `DrawExpected` as the remainder (inputs - other
  outputs - fee); `check_draw` asserts it equals A4's funding - np·R + C_draw - fee. So a
  `Lie` that moves money keeps the transaction balanced.
- (Stage 2) Standalone fixture: round r draw, seed_ref = 1,000,600 + (r - 1)·10^6, vault
  10,000,000 KAS (round r - 1), pot 100,000 KAS, acc 2 KAS, marker J 1,000,000 / J_boost
  50,000 / G2 10,000 KAS, m = 10, funding 100 KAS created after seed_ref + DRAW_WINDOW
  (fresh), lock_time = seed_ref; absorbed pools (round r - 1) have deadline seed_ref -
  100,000 and remaining 3 + i KAS.
- (Stage 2) Chain: the marker's creation DAA is taken as the controller's (both made by the
  same close).
- (DP22) Implemented as a real test that shells out to `$CARGO check` with
  `--target-dir crates/target/dp22` (avoids this run's build lock). First run ~35 s, cached
  afterwards. Checks both the gate failure message and the clean mainnet-only build.
- (DP17/DP18) Their script halves are in m3_draw.rs (`dp17_script_accepts_reference_witnesses`
  over V' x O' x price x m' = 3x3x2x2; `dp18_script_seed_mapping` with seeds 70,922 / 36,808 /
  15 / 2).
- (A2/A6, D43) Without `test-seed`, `Templates` has no drawer; marker, vault and pool are compiled
  with a zero drawer hash, so their legs can never pass; `GenesisCase::validate()` refuses and
  `build_unchecked_for_tests` panics ("no drawer in a build without the test-seed feature").
  `cargo check -p lottery --no-default-features --features mainnet` builds clean; with
  `mainnet,test-seed` it fails with the gate message (ran it; DP22 test itself is Stage 2).
- (D43) The genesis builder accepts only `NetworkType::Testnet` and `Devnet` (Mainnet and Simnet
  refused); `GenesisCase.network` defaults to Testnet.
- (D50) Extra genesis checks beyond the spec list: controller L(1) = J − J_boost + G2(m − 1);
  vault.liability = controller.liability; vault.round = controller.round − 1; drawer.round =
  controller.round; vault total_shares/pending = 0; drawer pool_tpl/vault_tpl must equal the
  compiled templates (checked in `build_with`).
- (Fixtures) Genesis fixtures keep V0 = v_rec = 10,000,000 KAS and are funded with
  `GENESIS_FUNDING` = 10,100,000 KAS in the local ledger. tn10 `close_m2` uses J(1) = 10 KAS,
  V0 = 60 KAS (L(1) = 10.9 KAS <= 12 KAS cap).
- (A4 legs) Leg arguments are range-checked to [0, SCRIPT_MAX] as in M2. Vault L4 written
  `drw.round == round + 1`; controller L4 `mk.round == round - 1`. Mutant tags placed now:
  ML1 ML2 ML3 ML4_role ML4_round (marker), VL1 VL2 VL3 VL4_role VL4_round (vault), CL1 CL2 CL3
  CL4_role CL4_round CL5 (controller draw_leg), PL1 PL2 PL3 PL4_role PL4_round PL5 (pool).
- (A4 legs) Pool L2 uses `OpInputCovenantId(this.activeInputIndex)`; L3
  `OpAuthOutputCount(this.activeInputIndex) == 0` (the index is >= 4, fixed only by the drawer's
  check 2).
- (D42) bin2num decoding verified in source: OpBin2Num pops an i64 and accepts non-minimal
  encodings up to 8 bytes, LE sign-magnitude (rusty-kaspa v2.1.0
  `crypto/txscript/src/opcodes/mod.rs:1310-1316`, `crypto/txscript/src/data_stack.rs:164-178`),
  so h[0..7] || 0x00 decodes to the 56-bit LE value (`game::u56_le`, test
  `dp18_u56_matches_script_number`). round is encoded with `to_le_bytes` as in D34.

## Open items / gotchas for the next stage
- RESOLVED (Stage 3): the A6 G2 guard is inside the second-prize branch of check 14
  (`mut:D14_g2guard`). The Rust `pool_set` has no such guard (checked_mul only).
- RESOLVED (Stage 3): cp5/cp7 use the real draw (`Chain::draw_round`); `fabricate_draw`
  deleted. (`close_base` still uses `CloseCase`'s own fabricated drawn controller: standalone
  M2 cases, not the Chain.)
- RESOLVED (Stage 3), see Stage 3 open item O1: `RoundResult::from_tx` implemented.
- Spec ambiguity (not security-relevant, interpreted): DA1 "a leaf length 2m ± 2" is
  implemented as the whole leaves argument 2 bytes longer / shorter (check 9's length
  equality); a per-shard re-split with the total kept is covered by "proofs swapped" and
  "count ± 1".
- DA4 regimes (asserted on the Rust side before any script runs): x2 + 1 fails only the
  edge rule at V = 10,000,000 KAS / pot 0; only the whale check at V = 4,000,000 / pot
  600,000; only the cap at V = 2,000,000 / pot 0 (all on the attack base).
- DP8 note: with a large vault x1 sits on the edge rule, and y loads the edge rule too
  (G2 = muldiv(x + y, g)), so y ~ 0 there; DP8 uses a 1,000,000 KAS vault to get y = O'.
- DELETED (Stage 3): the `M3_STAGE2_PROBE` instrument.
- Leg strikes already meaningful in strict mode (they run before the input-0 strikes):
  DA6 pool L5 (lock_time = deadline - 1) and pool L4 (pool round r, r + 1) at input 4; DA8
  marker L3 (pool output authorized by input 1); DA10 marker L4 and controller L4 (marker of
  round r - 1), controller L4 (controller round r + 2), controller L5 (drawn = 1), vault L4
  (vault round r); DA11 all of: look-alike drawer -> marker/vault/pool L4, look-alike marker
  -> controller L4, legs at wrong index (L1, inputs 1-4), legs authorizing an output (L3,
  inputs 1-4), leader of another covenant ID (L2, inputs 1-4).
- Dummy states: compile-time placeholders live in `buy.rs` (`dummy_vault`, `dummy_pool`, etc.)
  and `draw.rs` (`dummy_drawer`); template hashes do not depend on state values.
- Chain mapping: `Chain::genesis_with` sets `vault = outs[g.vault_output()]` (k+2) and
  `drawer = outs[g.drawer_output()]` (k+3); DAA 200. `Chain::draw` (Stage 2) should advance
  `drawer`, `vault`, `controller`, record the new pools and spend `marker`.
- A controller mutant/look-alike now changes every hash except the ticket's (drawer depends on
  the controller hash, marker/vault/pool on the drawer's). A drawer mutant changes the marker,
  vault and pool hashes, so the genesis `drawer.pool_tpl/vault_tpl` must come from the same
  `Templates` (the genesis builder enforces it).
- `game::witness_ok` is the Rust mirror of A5's three witness checks; Stage 3's script must
  match it. `fix_jackpot` reproduces the design 4.6 worked example (top-up to 1,040,421 KAS).
- DP18 seeds (cov = [0x4c; 32], round 1, m = 10, seed = i as 8 LE bytes then 24 zero bytes):
  rank 0 at i = 70,922; rank 142,505 at i = 36,808; bonus 1 at i = 15; bonus 10 at i = 2. Note
  Chain tests use the genesis covenant ID, not COV, so search again for that cov if needed.
- SilverScript: the `.sil` files compile with unused constants and unused entry arguments
  (drawer skeleton). Bash heredocs in this environment break on long inputs; use the Write tool.
- Marker redeem grew (draw_leg + DrawerState struct): M3-f (K = 16 close re-measure) is due in
  Stage 4.

## Stage 2 test run (from `crates/`; superseded by the Stage 3 run above)
- `cargo build --tests`: OK (only the two pre-existing m0_mass unused-variable warnings).
- `cargo test --no-fail-fast`: harness covenant_smoke 3, m0_limits 3, m0_mass 5, m0_merkle 2,
  m0_shapes 6; lottery unit 19, m1_attacks 25, m1_buy 14, m1_measure 4, m1_mutants 11,
  m2_attacks 40, m2_close 8, m2_game 2, m2_measure 5, m2_mutants 25, m2_params 17, m3_game 13:
  all pass. m3_draw: 1 passed (dp22), 21 failed; m3_attacks: 0 passed, 17 failed (as
  intended, table below).
- `M3_STAGE2_PROBE=1 cargo test --no-fail-fast -p lottery --test m3_draw --test m3_attacks -- --skip dp22`:
  m3_draw 21/21, m3_attacks 16/17 (DA15 fails: "expected a budget error, got VerifyError").
- Per-test failure lines: `cargo test --no-fail-fast -p lottery --test m3_attacks --test m3_draw -- --test-threads=1`.

| Test | Strict result | Fails where |
|---|---|---|
| dp1-dp20 (21 tests incl. dp17/dp18 script halves), a6_g2_guard_skipped_without_second_pool | FAIL | "valid draw rejected at input 0" (all other inputs accepted first); DP14/DP16 at the round 1 draw |
| dp22_seed_gate | PASS | — |
| da11_legs | FAIL | only at the final `accept_draw`: input 0; all 16 leg strikes (inputs 1-4) hit |
| da6, da8, da10 | FAIL | leg strikes hit (listed in Open items), then "BASE REJECTED at target input 0" |
| da1-5, da7, da9, da12-14, da16, a6_g2_guard_with_second_pool | FAIL | "BASE REJECTED at target input 0" (drawer-targeted, not yet meaningful) |
| da15 | FAIL | "valid draw rejected at input 0" (budget test needs the real drawer) |

## Stage 1 test run (command + summary)
From `crates/`, end of Stage 1:
- `cargo test --release`: all green. harness: covenant_smoke 3, m0_limits 3, m0_mass 5,
  m0_merkle 2, m0_shapes 6; lottery: unit 19, m1_attacks 25, m1_buy 14, m1_measure 4,
  m1_mutants 11, m2_attacks 40, m2_close 8, m2_game 2, m2_measure 5, m2_mutants 25, m2_params 17,
  m3_game 13. 0 failed.
- `cargo build --tests` (debug): OK (only the two pre-existing m0_mass unused-variable warnings).
- `cargo check -p tn10`: OK.
- `cargo check -p lottery --no-default-features --features mainnet`: OK, no warnings.
- `cargo check -p lottery --no-default-features --features mainnet,test-seed`: fails with
  "TEST-ONLY seed cannot be built for mainnet" (as intended, D43).
- Not run: the tn10 probes (no network work in Stage 1).
