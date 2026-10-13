# M8 handoff (delete when M8 closes; contents go to DECISIONS)

Spec: `docs/specs/8.md` (approved 2026-10-13). Plan: stages S1–S5 (spec "Staging").
Each stage agent reads this file first, works only on its stage, stops at a suite-green boundary,
and updates the sections below before it ends. Handoff completeness beats row count.

## Status
- S1 margin floor: done (commit "M8 S1: whale-margin floor 1.5"; D113)
- S2 lottery::index, wallet store, offline builders: todo (also: the genesis round-1 whale check, review SF1, see below)
- S3 tn10 indexer, campaign runner, bundle, fast campaign: todo
- S4 soak + simnet: todo
- S5 review and records: todo

## Approved plan (owner, 2026-10-13; copied from the design session's plan file)

**How the work runs:** the implementation session orchestrates. Each stage runs in a fresh agent that reads this file, stops suite-green and updates it. `covenant-reviewer` runs as its own fresh agent after S1 and in S5.

**Stop for the owner:**
- S1 reviewer owner decisions;
- before the first testnet submission of S3 (balance, about 2,000 tKAS);
- before any `git push`;
- before installing LLVM/clang and protoc;
- before the CLAUDE.md edit (D125);
- before the first kascov.io upload.

**Facts found while planning (verify line numbers before editing):**
- **State visibility:**
  - Outputs are P2SH only; a UTXO's state appears only in the redeem of the transaction that spends it.
  - The indexer must rebuild each successor state from the decoded inputs plus the entry args with the existing models (`CloseCase::expected`, `DrawCase::expected`, `SettleCase::expected`, the buy check-20 ticket state, `RoundResult::from_tx` at `draw.rs:1074`).
  - Each rebuilt state is checked by hashing it to the output SPK (state → redeem → P2SH). A mismatch is an error.
- **Private decoders:** `pushes()`, `state_values()` and `script_num()` are private in `crates/lottery/src/draw.rs:1251-1299`; move them to a `pub(crate)` decode module. `state.rs` has encoders only.
- **No transaction log:** `Chain` (`tests/common/m2.rs:390`) and `Ledger` (`src/ledger.rs`) keep none; IX1 needs `Chain` to record every `Final.tx`.
- **IX1's local game:**
  - its pieces come from `vp14_three_draws` (`m5_vault.rs:539`) for deposits, withdrawals and capped carry-over; `cp4`/`cp5` (`m2_close.rs:185,218`) for set_params and two rounds; `sell_close_draw`/`chain_claim` (`m4_claim.rs:153,194`) for claims;
  - the M5 helpers are in `tests/common/m5.rs` (deposit_case :596, withdraw_case :637, settle_case :682).
- **Mutants:**
  - The M3 pin `every_m3_tag_is_tested` asserts 85 (`m3_mutants.rs:726`). It drops `M5*/M6*/M7*` drawer tags and keeps only `CL*` controller tags.
  - The new drawer tag `D_margin_min` makes it 86.
  - M2 has no pin test: cover `C2_margin_min` with an explicit check in `m8_margin.rs`.
  - Do not prefix the new tags with M5/M6/M7.
- **Margin fixtures:**
  - The source is `close.rs:143` (11_700); genesis inherits it via `genesis.rs:109`.
  - Re-derive: `game.rs:1301,1344`, `m3_game.rs:186`, `m2_attacks.rs:154`, `m3_attacks.rs:243-261`.
  - `buy.rs:446` is a dummy compile fixture.
  - `spike_vdf.rs:904,913` keeps its own margins.
- **Workspace:**
  - `crates/Cargo.toml` has `default-members = ["harness","lottery"]`; `tn10` and the new `simnet` stay out of the default `cargo test`.
  - serde 1.0.229 and serde_json 1.0.151 are in Cargo.lock but are not yet a direct dependency of any crate.
- **Network coupling:**
  - `tn10/src/anchor.rs` is network-agnostic except for `probe::Ctx` (`probe.rs:49-54`) and its fixed timeouts.
  - The network guard is `tn10/src/net.rs:13-61`.
  - Fixed fees to replace (D120): `probe.rs:159`, `genesis.rs:601`.
- **Simnet facts (rusty-kaspa v2.1.0):**
  - `skip_proof_of_work` (`params.rs:737`; skipped at `pre_ghostdag_validation.rs:102-105`);
  - the header hash covers nonce and timestamp (`hashing/header.rs:7-35`);
  - `--enable-unsynced-mining` (`kaspad/src/args.rs:339`; without it, submit_block returns IsInIBD, `rpc/service/src/service.rs:306-311`);
  - an in-process node with mining in `testing/integration/src/daemon_integration_tests.rs:98-134`;
  - a CPU nonce loop in `bridge/src/rkstratum_cpu_miner.rs:191-245`;
  - Windows build needs protoc and LLVM/clang with `LIBCLANG_PATH` (README:71-78);
  - default simnet wRPC Borsh port 17510;
  - the mainnet hook passes kaspad commands only with `--simnet`/`--devnet`/`--testnet` (`.claude/hooks/block-mainnet.js:19-30`).
- **Indexer RPC:**
  - `get_virtual_chain_from_block_v2` (`rpc/core/src/api/rpc.rs:507`; `message.rs:2893-2899`) returns the full accepted transactions per chain block. v1 (`rpc.rs:259`) returns txids only, in batches of 2,480 blocks at 10 BPS (`service.rs:737-743`).
  - Check that the public nodes support v2 (server version) before relying on it; the fallback is v1 plus `get_block`.

**Stages:**
- **S1, margin floor:**
  1. `tests/m8_margin.rs` MF1–MF5 first (MF1–MF3 must fail because 14,999 is still accepted).
  2. `controller.sil:236` and `drawer.sil:407` → `MARGIN_MIN` with tags; `game.rs` `pub const MARGIN_MIN: i64 = 15_000`; `genesis.rs:295` range.
  3. Fixtures to 1.5, with each re-derived expectation logged (old → new); M3 pin 86.
  4. `cargo test` green.
  5. `m7_measure --release`: worst `close()`/`draw()` masses and the stack headroom at check 24 (≥ 4, D104).
  6. `covenant-reviewer` on the S1 diff (attack the MF5 cost claim, "which rule is off-chain only", negative controls).
  7. D113 row and the `docs/costs.md` M8 section (before "Spike VDF+ZK").
  8. Commit "M8 S1: whale-margin floor 1.5".
- **S2, `crates/lottery/src/index.rs`:**
  - `Event`/`classify`/`parse_*`;
  - `Rebuild` with apply and revert;
  - the serde types `index/1`, `wallet/1`, `campaign/1`, `bundle/1`;
  - `fee_for`, `bundle::verify`;
  - `ClaimCase::from_store` and `DrawCase::from_store`, with no RPC type in their signatures.

  Tests `tests/m8_index.rs` IX1–IX9 first. Commit "M8 S2".
- **S3, tn10:**
  - `indexer.rs`, `campaign.rs` (C0–C8, state file, idempotent resume, rejection-reason matching), `bundle.rs`, `fees.rs` (`get_fee_estimate`; verify in refs), `anchor.rs` generic over `RpcApi`;
  - subcommands in `main.rs`; `tn10-data/` git-ignored;
  - dry run (the indexer replays the M7 testnet instances read-only; `campaign start --dry-run`);
  - then the fast campaign C0–C7 (after the owner OKs the budget);
  - results to `docs/tn10-mass-check.md` "M8"; bundle `docs/launch/tn10-m8.json` plus `bundle-verify`.

  Commit "M8 S3".
- **S4:**
  - **Soak C8:** unattended about 31 h; the PC stays on; check in at the close (t0 + 20 h) and the draw (t0 + 30.5 h).
  - **In parallel, simnet:** owner approves the LLVM/protoc install and the CLAUDE.md edit (D125); then `crates/simnet` (in-process node, simnet-only guard, on-demand miner, nonce grinder, SN1–SN5); results in the "Simnet" section of `docs/tn10-mass-check.md`.

  Commit "M8 S4".
- **S5:**
  - `cli-debugger` on one `draw()` and one `claim()`; kascov.io preflight per shape;
  - `covenant-reviewer` on the full M8 diff;
  - records: DECISIONS D113–D125 plus the review rows, `docs/costs.md` M8, milestone → done, CLAUDE.md commands;
  - delete this file.

  Commit "M8: testnet-10 end to end".

## Progress log
- 2026-10-13 design session: spec approved, plan approved, this handoff created. No code. Next: S1 in a fresh chat (`/milestone 8` finds the spec; start at step 4 for S1).
(newest at the bottom; one entry per working session: what was done, test evidence, commit)
- 2026-10-13/14 S1 session (stopped early: low memory killed the third `cargo test`; no commit).
  - **Tests first.** `crates/lottery/tests/m8_margin.rs`: MF0 (constant); MF1 `close()` 14,999 refused / 15,000 accepted at input 0 (also 20,000 accepted, 20,001 refused); MF2 `draw()` 14,999 refused at input 0, 15,000 accepted, attribution by `first_failing_check` = drawer check 3; MF3 genesis 14,999/15,000/20,000/20,001 -> Err/Ok/Ok/Err (the error names "margin"); MF4 both mutants (`C2_margin_min` via `mutant(Sil::Controller)`, `D_margin_min` via `mutant(Sil::Drawer)`): the 14,999 strike is accepted by input 0 and the 15,000 base by all inputs; MF5 (3 tests, below). Shared attack `common/m3.rs` `ctrl_margin_below_floor()`; `m3_mutants.rs` `d_margin_min` kills `D_margin_min` (passed in run 2). The controller has no truncation-attribution instrument, so for close() "refused at C2_margin_min" is shown by MF1 + MF4 together.
  - **Fails before** (scripts unchanged, Rust constant only): MF1, MF2 and both MF4s panic "ATTACK ACCEPTED by input 0"; MF3 "margin 14999: expected ok = false, got Ok(())". MF5 passed (pure Rust).
  - **Implemented:** `covenants/controller.sil` and `covenants/drawer.sil`: `int constant MARGIN_MIN = 15000;` after `BP` (with a comment) and `require(margin >= MARGIN_MIN); // mut:C2_margin_min` / `require(ctrl.margin >= MARGIN_MIN); // mut:D_margin_min`; `game.rs` `pub const MARGIN_MIN: i64 = 15_000`; `genesis.rs:295` `range("margin", c.margin, MARGIN_MIN, 2 * BP)` and its unit-test strike `BP - 1` -> `MARGIN_MIN - 1`.
  - **Fixtures:** `close.rs:143` and `buy.rs:446` (dummy) 11,700 -> 15,000. `spike_vdf.rs` untouched.
  - **MF5 result (holds at 1.5; verified in run 2 and standalone):** real-magnitude grid, exhaustive over m' in {2, 10, 50} and V', O' in {0} U {10^k KAS, k = 0..10}: 432 points per margin; price 1 KAS, fixture shares, e 15 %, j 10 %, g 1 %, cap 20 %. Whale-binding points (one more sompi of J_house fails the whale check alone): 16 at 1.17, **154 at 1.5**. At each: (x2, y) passes; x2 + 1 and y + 1 fail `pass` and `pass_exact`; witness_ok accepts and rejects x2 + 1; a 2,000-sompi local scan finds nothing larger; W - (J' + G2' (m' - 1)) is in [0, m') (worst 0 sompi at 1.17, **27 sompi at 1.5**), i.e. jackpot money per ticket within 1/N sompi below q = floor(p w / margin) (q/p = 0.6752 at 1.17, 0.5267 at 1.5). Worked point V' 1,000,000 KAS, m' 10, O' 10^10 KAS at 1.5: J_house 100,000 KAS, J_boost 588,561.09 KAS, money = W = 750,531.59 KAS exactly. `fix_jackpot_brute` equals `fix_jackpot` exactly on a scaled copy (price 3 sompi, m' in {2, 10}, V' in {0, 10^4, 10^6, 10^7}, O' in {0, 10^4, 3x10^5, 2x10^6} sompi, 32 points per margin; whale-binding 2 at 1.17, 5 at 1.5); at full magnitude brute force is infeasible (linear scans over 10^14 sompi). **New fact:** at 1.5 the edge rule binds nowhere on the grid (q = 0.5267 p < a = 0.629 p): every point the edge bound at 1.17 is whale-bound at 1.5, including base lowering (x1 < x_b). The test asserts this.
  - **Full runs.** Run 1 (fail-fast) stopped at m2_attacks ca4. Run 2 (`cargo test -j 4 --no-fail-fast`, debug): every binary green except m2_game (cp8, 1 failure), m3_attacks (da4, 1), m3_mutants (pin, 1; 91 passed incl. d_margin_min) and spike_vdf (STACK_OVERFLOW, pre-existing, see Known failing). Passed per binary: harness lib 8, covenant_smoke 3, m0_limits 3, m0_mass 5, m0_merkle 2, m0_shapes 6, m7_chain 8, lottery lib 42, m1_attacks 25, m1_buy 14, m1_measure 4, m1_mutants 11, m2_attacks 39, m2_close 8, m2_game 1 (+1 F), m2_measure 5, m2_mutants 25, m2_params 17, m3_attacks 18 (+1 F), m3_draw 21, m3_game 13, m3_measure 7, m3_mutants 91 (+1 F), m4_attacks 21, m4_claim 23, m4_game 4, m4_measure 8, m4_mutants 193 (1 ignored, pre-existing), m5_attacks 21, m5_game 10, m5_measure 5, m5_mutants 261, m5_vault 15, m6_attacks 8, m6_conflict 3, m6_limits 8, m6_measure 10, m6_mutants 9, m7_attacks 19, m7_measure 7, m7_mutants 19, m7_seed 15, **m8_margin 9**. The three failures were then re-derived (see the draft row) but are **not yet re-run**. Run 3 first hit a stale-artifact compile error in m3_game (`MARGIN_MIN` not found, caused by building a HEAD worktree into the shared target dir; m3_game compiled and passed in run 2); after `cargo clean -p lottery -p harness` the rerun was killed for low memory during compilation.
  - **Next (the S1 resume point):** from `crates/`: `cargo test -j 2 --no-fail-fast` (expect everything green except the spike_vdf debug overflow). If da4's edge regime (e = 3,000) does not give [true, false, false], recompute it: any e with p s_H (1 - e) < q, i.e. e > 2,883 bp, and a vault where the edge rule, not the cap, binds. Then `cargo check -p lottery --no-default-features --features mainnet`. Then step 5: `cargo test -p lottery --release --test m7_measure -- --nocapture --test-threads=1`, and `cargo test -p lottery --release --test spike_vdf -- --nocapture --test-threads=1` (spike_vdf runs only in release). On masses: the margin literal changes from 10000 to 15000, and both are 2-byte script numbers, so redeem sizes should not change. Confirm by measurement; do not assume.

- 2026-10-14 orchestrator (S1 close-out): resumed the S1 agent's handoff. `cargo test -j 2 --no-fail-fast` green in every binary (m2_game 2, m3_attacks 19, m3_mutants 92, m8_margin 9, …) except the pre-existing debug `spike_vdf` overflow; `spike_vdf --release` 14/14; mainnet `cargo check` builds; release `m7_measure` re-measured (D113). Owner decisions: floor 1.5 now (RSD separate), M8 rows renumbered D113–D125, review SF1 fixed in S2. covenant-reviewer: no must-fix; SF2 fixed (cp8 edge-bound point, mutant `s1 = qAllow·N − g2Hat` killed by hand), SF3 records written, notes applied. Records: DECISIONS rows (renumbering, D113, design 4.6 proposal, S1 review), `docs/costs.md` "M8" section. Next: S2 in a fresh agent.
  - **S2 added task (review SF1, owner 2026-10-14):** genesis `validate()` must also require `J + G2(m − 1) <= q N m` and `J_house + G2(m − 1) <= a N m` for round 1 (round-1 shares, e, margin); write the attack test first (the default fixture, J(1) 1,000,000 KAS at m 10, is 45 % over the limit and must be refused), then give tests that need a large J explicit fixture values rather than loosening anything. IX8 (bundle-verify) runs the same validation.

## PENDING DECISIONS rows (drafts; copied into docs/DECISIONS.md at S5 or at the stage's commit)
- D113 and the S1 review row were moved into `docs/DECISIONS.md` at the S1 commit (2026-10-14). Nothing pending.

## Owner notes
- 2026-10-13, soak scheduling. The owner's laptop need not stay on for 30 h. Offer two modes:
  - **(A)** plugged in with sleep off;
  - **(B)** three sittings:
    1. t0: the round-6 buys, with the indexer running (about 15 min);
    2. one indexer catch-up between about t0 + 8 h and t0 + 24 h, so that its resume block is never older than the pruning window (about 5 min);
    3. at about t0 + 29.5 h: `close()`, wait for the deletion evidence (about t0 + 30.5 h), then `draw()`, claim and `refresh()` (about 1.5 h).

  In (B) the close moves from spec C8's "about t0 + 20 h" to the last sitting. Constraint: the draw must be ≥ t0 + 30 h (deletion) and ≤ close DAA + 600 + F (12 h). Record this as a C8 amendment.
  - **Verify in S3:** does `get_virtual_chain_from_block` fail for a start hash below the pruning point?
  - **The runner must support (B):** `campaign status` prints the next sitting's time window.

## Open questions for the owner
- (S1, RESOLVED 2026-10-14) Numbering: the owner renumbered the M8 decisions to D113–D125 (spec 8, milestones and this file updated).
- (S1, RESOLVED 2026-10-14) Floor ordering versus research row D112: the owner chose to keep the 1.5 floor now as D109 planned; it stays provisional (one constant per script) and RSD (D111, D112) remains a separate release-gate decision.
- (S1, DONE 2026-10-14) design.md 4.6 updated to the 1.5 floor at the owner's request (DECISIONS row "Design 4.6 edit applied").

## Known failing / skipped tests
- `spike_vdf` `sv_measure_masses_and_stack` overflows its stack in a debug build (STATUS_STACK_OVERFLOW, 0xc00000fd). **Pre-existing:** reproduced at HEAD 93d5413 in a temporary worktree (2026-10-14). CLAUDE.md runs spike_vdf with `--release`. Not fixed (outside S1).
- `m4_mutants`: 1 ignored (pre-existing).
- Re-derived after run 2, not yet re-run: `m2_game` cp8, `m3_attacks` da4, `m3_mutants` every_m3_tag_is_tested.
