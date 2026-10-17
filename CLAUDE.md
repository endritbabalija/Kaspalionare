# Kaspa L1 Lottery

## Goal
A KAS-only lottery on Kaspa L1 built with Toccata covenants (SilverScript). Players `buy()` tickets and `claim()` prizes; anyone can call `close()` and `draw()`. Every lottery rule is enforced by L1 covenants. Off-chain code (our Rust transaction builder) only assembles transactions and Merkle proofs that the covenants verify. No keeper bots, no off-chain lottery logic.

- Design: `docs/design.md` — read the relevant section before any covenant work. Do not import it here; open it when needed.
- Kaspa best practices and open checks: docs/research/kaspa-best-practices.md — read before covenant work.
- Build order and current milestone: `docs/milestones.md`
- Decisions made while building: `docs/DECISIONS.md`
- Per-milestone specs: `docs/specs/`

## Non-negotiables
- Kaspa model: a transaction states its outputs; covenant scripts only verify. A covenant cannot read another UTXO without spending it.
- R1 Full state bytes: successor state is built from bytes present in the transaction, never stored only as a hash of data supplied elsewhere.
- R2 Enumerated shapes: every transition fixes input/output counts, output indices and covenant bindings; extra outputs are rejected.
- R3 Absolute time: deadlines and the seed reference are absolute DAA scores stored in state. Only lower-bound time checks exist; never assume transaction expiry.
- R4 Prize pools carry only data reconstructible from the round result record and their UTXO value.
- Solvency: holders of one winning combination receive at most its slice in total; no pool can be overdrawn; the vault always holds the liability J_house + G2 × (m − 1) of the round whose jackpot is fixed, until that round is drawn.
- `close()` consumes all shards of the round, so counts are frozen before the seed reference. Rounds are strictly ordered: `close()` waits for the previous `draw()`.
- Seed (M7): the seed is the hash of the anchor block X, the first selected-chain block with DAA ≥ D = the marker's creation DAA + seed_offset, authenticated in `draw()` with `OpChainblockSeqCommit` and a 272-byte KIP-21 preimage proof; a missed window ends in a void (no winners). The anchor miner's withholding bias is OPEN on the release gate (D100, amended): genesis refuses mainnet. The only TEST-ONLY path left is the short-void drawer variant for testnet `void-m7`, compiled out of a `mainnet` build.

## Gotchas
- Never state Kaspa limits, opcodes or fees from memory. Verify them against the sources in `refs/` (the pinned rusty-kaspa release, the KIPs, SilverScript) and cite the file.
- The solvency argument (design 4.4) depends on the exact form `floor(B_t / W_t)` then `floor(slice / h)`, on second-prize pools of n2 × G2, and on the vault keeping L ≤ cap × V. Re-check it whenever prize or vault arithmetic changes.
- "Solved in model" in the design means solved on paper only. Nothing counts as implemented until compiled attack tests pass and the reviewer has checked it.

## Safety
- Local script-engine tests, a private simnet node on this PC with no peers (`crates/simnet`, D125), and testnet-10 only. Never use mainnet nodes, mainnet addresses (`kaspa:`) or mainnet keys. A hook blocks this.
- Never commit private keys, seeds, mnemonics or node credentials. Personal settings go in `CLAUDE.local.md` (git-ignored).

## Stack and layout
- `covenants/` SilverScript sources (`shard.sil`, `ticket.sil`, `controller.sil`, `marker.sil`, `params.sil`, `drawer.sil`, `vault.sil`, `pool.sil`, `request.sil`, `record.sil`, `position.sil`; `m0/`, `m1/` are measurement probes). `crates/` Rust workspace: `harness` (compile, engine, mass, `ChainSim` fabricated chain in `chain.rs`), `lottery` (combinations, Merkle tree, state encoders, transaction builder, M1+ tests), `tn10` (testnet probes). `refs/` cloned reference repos (git-ignored, read-only).
- Covenants are tested first in the local rusty-kaspa script engine, then on testnet-10.
- rusty-kaspa: pinned v2.1.0 (`01b532e8…`), SilverScript v1.0.0 (design section 2, `docs/DECISIONS.md`). Use those releases.
- Development machine is Windows. Prefer commands that work in Git Bash and PowerShell.
- Commands (run from `crates/`; same in Git Bash and PowerShell; `crates/rust-toolchain.toml` pins Rust 1.94.0 and rustup installs it on first use; the first build fetches rusty-kaspa v2.1.0 and SilverScript v1.0.0 from git, ~2 min):
  - Build: `cargo build --tests`
  - All tests: `cargo test`
  - One test file with printed measurements: `cargo test --test m0_mass -- --nocapture --test-threads=1` (also `covenant_smoke`, `m0_merkle`, `m0_shapes`, `m0_limits`; `m0_mass` takes about 4 min in debug builds; add `--release` for about 1 min)
  - M1 (crate `lottery`): `cargo test -p lottery` (unit tests plus `m1_buy`, `m1_attacks`, `m1_mutants`, `m1_measure`); measurements: `cargo test -p lottery --release --test m1_measure -- --nocapture --test-threads=1`
  - M3 (crate `lottery`): `cargo test -p lottery --test m3_game --test m3_draw --test m3_attacks --test m3_mutants`; measurements: `cargo test -p lottery --release --test m3_measure -- --nocapture --test-threads=1`; first failing A4 check per attack: `M3_ATTRIB=1 cargo test -p lottery --release --test m3_attacks -- --nocapture --test-threads=1` (lines starting `ATTRIB`)
  - M4 (crate `lottery`): `cargo test -p lottery --test m4_game --test m4_claim --test m4_attacks --test m4_mutants`; measurements: `cargo test -p lottery --release --test m4_measure -- --nocapture --test-threads=1`; first failing check per attack: `M4_ATTRIB=1 cargo test -p lottery --release --test m4_attacks -- --nocapture --test-threads=1` (lines `ATTRIB`; use `grep -oE "ATTRIB .*"`)
  - M5 (crate `lottery`): `cargo test -p lottery --test m5_game --test m5_vault --test m5_attacks --test m5_mutants`; measurements: `cargo test -p lottery --release --test m5_measure -- --nocapture --test-threads=1`; first failing check per attack: `M5_ATTRIB=1 cargo test -p lottery --release --test m5_attacks -- --nocapture --test-threads=1` (lines `ATTRIB`); executable-spec model as oracle (each strike rejected by the model at exactly its named check): `M5_PREFLIGHT=1 cargo test -p lottery --release --test m5_attacks --test m5_vault -- --nocapture --test-threads=1`
  - M6 (crate `lottery`): `cargo test -p lottery --test m6_limits --test m6_attacks --test m6_mutants`; measurements: `cargo test -p lottery --release --test m6_measure --test m6_conflict -- --nocapture --test-threads=1` (`m6_conflict` takes `M6_TAU_CHAIN`, `M6_TAU_MERGE` and `M6_RHO` in seconds to override the measured τ and the rebuild delay); `M5_ATTRIB=1` and `M5_PREFLIGHT=1` also cover `m6_attacks` (add `--test m6_attacks` to the M5 commands)
  - M7 (crate `lottery`): `cargo test -p lottery --test m7_seed --test m7_attacks --test m7_mutants`; measurements: `cargo test -p lottery --release --test m7_measure -- --nocapture --test-threads=1`; `M5_ATTRIB=1` and `M5_PREFLIGHT=1` also cover `m7_attacks` (add `--test m7_attacks`); `cargo test -p harness` covers `m7_chain` (KIP-21 pins). Seed gate (SP12, run by `m7_seed`): `cargo check -p lottery --no-default-features --features mainnet` builds, `--features mainnet,test-seed` fails as an unknown feature, and a `mainnet` build has no short-void variant symbol
  - Spike VDF+ZK (crate `lottery`, test-only drawer variants, `docs/specs/spike-vdf.md`): `cargo test -p lottery --release --test spike_vdf -- --nocapture --test-threads=1` (masses, stack, prover timing, margin-floor table)
  - Low memory (Windows paging file, os error 1455): add `-j 1` or `-j 2` to any cargo command.
  - Covenant sources are compiled at test time from `covenants/` by `harness::compile_file`; there is no separate compile step.
  - Testnet-10 probes (not part of `cargo test`): `cargo run -p tn10 -- info`, `cargo run -p tn10 -- anchor-check [n]` (rebuilds a fresh anchor proof from RPC and runs every self-check without submitting; with n also the next n chain blocks) and `cargo run -p tn10 -- probe <all|baseline|storage|transient|dust|buy|buy-m1|claim|close-m2|draw-m7|void-m7|draw-m3|claim-m4|vault-m5|conflict-m6|close16|close32|close62>` (`buy-m1` submits the real shard script, b = 4 and b = 1, each with a referral; `draw-m7` runs genesis, buy, close, waits for the anchor X (first chain block with DAA ≥ D) and 50 blue score on it, rebuilds its 272-byte proof from RPC and submits a K = 16 `draw()` with the real scripts; `draw-m3` is an alias of `draw-m7`, not in `all`; `void-m7` runs genesis with the TEST-ONLY short-void drawer variant (draw_window = 1,200, testnet only), close and a void about 2 minutes later; `claim-m4` runs the `draw-m7` sequence and claims only if its fixed ticket happens to win (the outcome is unknown before close), otherwise it records "claim() not exercised"; `vault-m5` runs genesis, a request below the vault age window (must be rejected), `request_deposit()`, `request_withdraw()`, buy, close, a converting `draw()` on the real anchor with the settlement record and both settlements, about 4 minutes of DAA waits; run it with `--release`, the public resolver sometimes times out, retry; `conflict-m6` runs 10 shard races, replacements and chained buys and measures τ_chain and τ_merge; it is not part of `all` and locks about 160 KAS). The key is `TN10_PRIVATE_KEY` in the git-ignored `.env` (create it with `cargo run -p tn10 -- keygen` and fund it from the faucet). The endpoint is `TN10_WRPC_URL`, or the public resolver if unset. Results go in `docs/tn10-mass-check.md`.

## How we work
- One milestone per session; use the `/milestone` skill. Start in plan mode and get the plan approved before writing code.
- For every covenant transition, write the attack tests first, then implement, then run all tests. Show the test output as evidence.
- After each milestone, run the `covenant-reviewer` subagent on the changes. Fix correctness and requirement gaps; ignore style.
- Record every decision or deviation from the design as one row in `docs/DECISIONS.md`. If the design doc and the code disagree, stop and ask.
- Do not edit `docs/design.md` unless asked; propose changes in `docs/DECISIONS.md`.
- Use `/clear` between unrelated tasks.

## Compaction
When compacting, keep: the current milestone, modified files, test commands, failing tests and any decision not yet written to `docs/DECISIONS.md`.
