# M8 handoff (delete when M8 closes; contents go to DECISIONS)

Spec: `docs/specs/8.md` (approved 2026-10-13). Plan: stages S1–S5 (spec "Staging").
Each stage agent reads this file first, works only on its stage, stops at a suite-green boundary,
and updates the sections below before it ends. Handoff completeness beats row count.

## Status
- S1 margin floor: todo
- S2 lottery::index, wallet store, offline builders: todo
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
- before the CLAUDE.md edit (D122);
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
  - Fixed fees to replace (D117): `probe.rs:159`, `genesis.rs:601`.
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
  7. D110 row and the `docs/costs.md` M8 section (before "Spike VDF+ZK").
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
  - **In parallel, simnet:** owner approves the LLVM/protoc install and the CLAUDE.md edit (D122); then `crates/simnet` (in-process node, simnet-only guard, on-demand miner, nonce grinder, SN1–SN5); results in the "Simnet" section of `docs/tn10-mass-check.md`.

  Commit "M8 S4".
- **S5:**
  - `cli-debugger` on one `draw()` and one `claim()`; kascov.io preflight per shape;
  - `covenant-reviewer` on the full M8 diff;
  - records: DECISIONS D110–D122 plus the review rows, `docs/costs.md` M8, milestone → done, CLAUDE.md commands;
  - delete this file.

  Commit "M8: testnet-10 end to end".

## Progress log
- 2026-10-13 design session: spec approved, plan approved, this handoff created. No code. Next: S1 in a fresh chat (`/milestone 8` finds the spec; start at step 4 for S1).
(newest at the bottom; one entry per working session: what was done, test evidence, commit)

## PENDING DECISIONS rows (drafts; copied into docs/DECISIONS.md at S5 or at the stage's commit)

## Open questions for the owner

## Known failing / skipped tests
