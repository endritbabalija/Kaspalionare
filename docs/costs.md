# Costs

Sizes and masses per transition. Mainnet mass parameters of rusty-kaspa v2.1.0. Limits per transaction (= per block): compute 500,000, storage 500,000, transient 1,000,000. Minimum relay fee: 100 sompi per gram of max(compute, transient / 2). Evidence and citations: `docs/feasibility.md`.

Numbers below are from M0 probes (`covenants/m0/`), not protocol scripts. Replace each row when the real script exists.

## Scripts (M0 probes)

| Script | Redeem B | Sigscript B | Script units | Compute budget | Source test |
|---|---|---|---|---|---|
| Merkle verify + update, depth 16, OpBlake2b | 1,911 | 2,439 | 40,169 | 4 | `m0_merkle` |
| Merkle verify + update, depth 23, OpBlake2b | 2,666 | 3,418 | 67,323 | 6 | `m0_merkle` |
| Controller close probe, MAX_K = K | ≈ 190 + 420·K | ≈ redeem + 4 | ≈ 1,550 + 3,840·K | — | `m0_mass::q5` |

## Transitions (M0 estimates)

| Transition | Compute mass | Transient mass | Storage mass | Min fee | Notes |
|---|---|---|---|---|---|
| `buy()`, 1 ticket, d = 1 KAS | 6,612 | 15,648 | 40,007 | 0.0078 KAS | shard sigscript 3,418 B (Merkle only) |
| `close()`, K = 16, S = 3,418 B | 74,575 | 257,420 | see R row | 0.1287 KAS | S = shard sigscript size |
| `close()`, K = 32, S = 3,418 B | 146,362 | 512,088 | see R row | 0.2560 KAS | |
| `close()` probe, K = 8, real scripts, budgets committed | 8,997 | 21,348 | 952 | 0.0107 KAS | `m0_shapes`; controller 32,240 units, budget 3 |
| `claim()`, prize 0, d = 0.08, R = 1.44, worst funding | — | — | 152,775 | — | storage only measured |

## Minimums

| Quantity | Value | Condition |
|---|---|---|
| Ticket deposit d | 0.08 KAS per ticket in the `buy()` | storage mass ≤ 500,000 |
| Ticket deposit, storage not the block bottleneck | 5.04 KAS | ≥ 63 buys per block |
| Claim payout (prize + deposit) | 0.0212 KAS | always met because d ≥ 0.08 |
| Reserve R per protocol object | 1.44 KAS (K = 16), 2.72 KAS (K = 32), 5.28 KAS (K = 64) | `close()` storage mass ≤ 500,000, worst case over sales and caller funding |
| Reserve R, `close()` within 10 % of block storage | 14.4 KAS (K = 16), 27.2 KAS (K = 32) | storage mass ≤ 50,000 |
| Largest K for `close()` | 62 / 37 / 23 | shard sigscript 3,418 / 6,000 / 10,000 B (transient mass bound, exact search) |
