# Decisions

One row per decision. Newest at the bottom. If a decision changes the design, say which section of `docs/design-v0.4.md` it affects.

| Date | Decision | Why | Design section |
|---|---|---|---|
| 2026-10-08 | Claim-based design: `buy()`, `close()`, `draw()`, `claim()`; no keepers | All rules enforced on L1; no bots | 1, 3 |
| 2026-10-08 | One exact-combination counter per ticket, in K sharded sparse Merkle trees | Fair duplicate splitting without global state | 8 |
| 2026-10-08 | Prize = tier budget ÷ winning combinations, split among a combination's holders | Solvent by construction; instant claims | 4.2–4.4 |
| 2026-10-08 | Backers underwrite only the jackpot floor and are settled at `draw()` | No tier-wide winner totals needed | 5 |
| 2026-10-08 | Game size chosen at `close()` from a fixed ladder by target λ* | Meaningful slices at any sales volume | 4.6 |
| 2026-10-08 | Claim window of at least 7 days, enforced by when `draw()` may absorb a pool | Kaspa has no transaction expiry | 4.5 |
| 2026-10-08 | Rounds strictly ordered: `close()` waits for the previous `draw()` | Rolling pot and floor always reach the right round | 7.2 |
| 2026-10-08 | Randomness parked; `draw()` uses a test-only seed | Out of scope for now | 11 |
| 2026-10-08 | Rust for the transaction builder and tests; local script engine before testnet-10 | rusty-kaspa is Rust; fast feedback | Implementation choice |
| 2026-10-09 | Pin rusty-kaspa v2.1.0 (`01b532e8b553523216471682649693af92f0fd16`) instead of baseline v2.0.1 | Latest release; same post-Toccata consensus rules and Appendix A limits as v2.0.1; simpler engine API (feasibility Q6). Toccata activation DAA 474,165,565 is only in v2.0.1's params | 2, Appendix A (citation update pending) |
| 2026-10-09 | Merkle and protocol hashing use `OpBlake2b` (0xaa) | Same hash as the P2SH commitment; Blake3 saves only ~4.5 % script units and gives the same compute budget at depth 23 (feasibility Q2). SilverScript template hashes are Blake3 regardless | 8.2 |
| 2026-10-09 | SilverScript compiler pinned at tag v1.0.0 (`3ed97333…`), used as a Rust library; only bytecode crosses into the v2.1.0 engine | Compiler links its own rusty-kaspa rev `a41a333b` (between v2.0.1 and v2.1.0); keeping the boundary at bytecode avoids API coupling | Implementation choice |
| 2026-10-09 | KIPs read at master `e4ae2332…` | No tagged KIP releases | Sources |
| 2026-10-09 | Rust deps from git pinned by rev, not path deps into `refs/`; toolchain pinned to 1.94.0 in `crates/rust-toolchain.toml` | `refs/` is git-ignored; SilverScript requires rust-version 1.94.0 | Implementation choice |
| 2026-10-09 | Workspace layout: `crates/harness` (compile, engine run, mass); M0 probes in `covenants/m0/`, tests in `crates/harness/tests/m0_*.rs` | Probes are measurement only, not protocol code | Implementation choice |
| 2026-10-09 | M0 has no `docs/specs/M0.md` | M0 writes no protocol code; questions and done criteria are in `docs/milestones.md` | — |
| 2026-10-09 | R2 "no extra outputs" is hand-coded (`tx.inputs.length`, `tx.outputs.length`, `OpCovInputCount/OpCovOutputCount`, explicit indices) | SilverScript's covenant declarations count only authorized outputs and allow extra outputs (test `singleton_sugar_does_not_reject_extra_plain_output`); declarations also assume one template per covenant family | 6 (R2), 7 |
| 2026-10-09 | Every protocol UTXO (controller, shards, frozen marker, prize pools, vault) carries a fixed reserve R on top of its economic value; R moves with the object; conservation (5.5) is restated over economic value excluding reserves; a prize pool keeps R until a later `draw()` absorbs it | Zero-value outputs are invalid and small outputs exceed storage mass (feasibility Q4); the last claimant is never blocked | 5.5, 6, 7.2–7.4 (design update pending) |
| 2026-10-09 | Creator lends, creator gets it back: a transaction creating more protocol objects than it consumes has the caller post the extra reserves; each new object records the funder's payout script and repays R to it when finally consumed. `close()` funds the marker's R (refunded at `draw()`); `draw()` funds each pool's R (refunded when a later `draw()` absorbs it); controller, shard and vault reserves are posted once at deployment | Reserves need a funder without touching player or backer money | 5.5, 6, 7.2–7.4 (design update pending) |
| 2026-10-09 | No standalone tiny outputs: caller rewards are added to a caller-chosen output that also carries the caller's change; the protocol fee accumulates in the controller and is paid out only above a threshold | Small standalone outputs exceed storage mass (0.01 KAS plain output = 1,000,000) | 5.1, 5.5, 7.2, 7.3 (design update pending) |
| 2026-10-09 | M0 measured minimums: R ≥ 1.44 / 2.72 / 5.28 KAS for K = 16 / 32 / 64 (close() storage ≤ 500,000, worst case over sales and caller funding); ticket deposit ≥ 0.08 KAS per ticket (≈ 5 KAS so storage is not the block bottleneck); claim payout ≥ 0.0212 KAS, always met by the deposit; largest K for close() 62 / 37 / 23 for shard sigscripts of 3,418 / 6,000 / 10,000 B, bound by transient mass | Measured, see `docs/feasibility.md` Q4–Q5 and `docs/costs.md`; final R, d and K are set in M6 | 4.6, 8.3, 12 |
