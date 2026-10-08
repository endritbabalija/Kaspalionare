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
