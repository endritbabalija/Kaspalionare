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
- IMPORTANT: randomness is parked. `draw()` takes an explicit, clearly named TEST-ONLY seed. Do not design or implement a real seed source.

## Gotchas
- Never state Kaspa limits, opcodes or fees from memory. Verify them against the sources in `refs/` (the pinned rusty-kaspa release, the KIPs, SilverScript) and cite the file.
- The solvency argument (design 4.4) depends on the exact form `floor(B_t / W_t)` then `floor(slice / h)`, on second-prize pools of n2 × G2, and on the vault keeping L ≤ cap × V. Re-check it whenever prize or vault arithmetic changes.
- "Solved in model" in the design means solved on paper only. Nothing counts as implemented until compiled attack tests pass and the reviewer has checked it.

## Safety
- Local script-engine tests and testnet-10 only. Never use mainnet nodes, mainnet addresses (`kaspa:`) or mainnet keys. A hook blocks this.
- Never commit private keys, seeds, mnemonics or node credentials. Personal settings go in `CLAUDE.local.md` (git-ignored).

## Stack and layout
- `covenants/` SilverScript sources. `crates/` Rust workspace: transaction builder, Merkle tree, test harness. `refs/` cloned reference repos (git-ignored, read-only).
- Covenants are tested first in the local rusty-kaspa script engine, then on testnet-10.
- rusty-kaspa: pinned v2.1.0 (`01b532e8…`), SilverScript v1.0.0 (design section 2, `docs/DECISIONS.md`). Use those releases.
- Development machine is Windows. Prefer commands that work in Git Bash and PowerShell.
- Commands (run from `crates/`; same in Git Bash and PowerShell; `crates/rust-toolchain.toml` pins Rust 1.94.0 and rustup installs it on first use; the first build fetches rusty-kaspa v2.1.0 and SilverScript v1.0.0 from git, ~2 min):
  - Build: `cargo build --tests`
  - All tests: `cargo test`
  - One test file with printed measurements: `cargo test --test m0_mass -- --nocapture --test-threads=1` (also `covenant_smoke`, `m0_merkle`, `m0_shapes`; `m0_mass` takes about 1–2 min in debug builds)
  - Covenant sources are compiled at test time from `covenants/` by `harness::compile_file`; there is no separate compile step.

## How we work
- One milestone per session; use the `/milestone` skill. Start in plan mode and get the plan approved before writing code.
- For every covenant transition, write the attack tests first, then implement, then run all tests. Show the test output as evidence.
- After each milestone, run the `covenant-reviewer` subagent on the changes. Fix correctness and requirement gaps; ignore style.
- Record every decision or deviation from the design as one row in `docs/DECISIONS.md`. If the design doc and the code disagree, stop and ask.
- Do not edit `docs/design.md` unless asked; propose changes in `docs/DECISIONS.md`.
- Use `/clear` between unrelated tasks.

## Compaction
When compacting, keep: the current milestone, modified files, test commands, failing tests and any decision not yet written to `docs/DECISIONS.md`.
