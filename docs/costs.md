# Costs

Sizes and masses per transition, using the mainnet mass parameters of rusty-kaspa v2.1.0.

**Limits per transaction** (the same as per block, and confirmed on testnet-10: feasibility Q7, `docs/tn10-mass-check.md`): compute 500,000, storage 500,000, transient 1,000,000.

**Fees.**
- Minimum relay fee: 100 sompi per gram of max(compute, transient / 2). It ignores storage mass.
- Fee for priority: the mempool ranks by fee per gram of max(compute, transient / 2, storage). Paying 100 sompi per gram of that is the floor priority under load (feasibility Q4).

Evidence and citations: `docs/feasibility.md`. Minimums are rounded up to 0.0001 KAS.

The numbers below come from M0 probes (`covenants/m0/`), not protocol scripts. Replace each row when the real script exists.

## Scripts (M0 probes)

| Script | Redeem B | Sigscript B | Script units | Compute budget | Source test |
|---|---|---|---|---|---|
| Merkle verify + update, depth 16, OpBlake2b | 1,911 | 2,439 | 40,169 | 4 | `m0_merkle` |
| Merkle verify + update, depth 23, OpBlake2b | 2,666 | 3,418 | 67,323 | 6 | `m0_merkle` |
| Controller close probe, MAX_K = K | ≈ 190 + 420·K | ≈ redeem + 4 | ≈ 1,550 + 3,840·K | — | `m0_mass::q5` |

## Transitions (M0 estimates)

| Transition | Compute mass | Transient mass | Storage mass | Relay floor | Fee for priority | Notes |
|---|---|---|---|---|---|---|
| `buy()`, 1 ticket, d = 0.0844 (minimum), price 0.1, fresh shard | 6,612 | 15,648 | 475,839–499,992 | 0.0078 KAS | ≈ 0.48–0.50 KAS | Shard sigscript 3,418 B (Merkle only), R = 1.44. Accepted on TN10 at 499,992. |
| `buy()`, 1 ticket, d = 5 KAS, steady state | 6,612 | 15,648 | 8,039 | 0.0078 KAS | 0.0080 KAS | Pot 1,000 KAS, large change. |
| `close()`, K = 16, S = 3,418 B | 74,575 | 257,420 | ≤ 500,000 at R min | 0.1287 KAS | ≈ 0.50 KAS | S = shard sigscript size. Accepted on TN10 (498,657). |
| `close()`, K = 32, S = 3,418 B | 146,362 | 512,088 | ≤ 500,000 at R min | 0.2560 KAS | ≈ 0.50 KAS | Accepted on TN10 (495,013). |
| `close()`, K = 62, S = 3,418 B (TN10 probe) | 280,926 | 989,544 | 479,758 | 0.4948 KAS | 0.4948 KAS | Largest K; accepted and mined on TN10. |
| `close()` probe, K = 8, real scripts, budgets committed | 8,997 | 21,348 | 952 | 0.0107 KAS | 0.0107 KAS | `m0_shapes`; controller 32,240 units, budget 3. Storage mass committed. OpTrue funding stand-in; not run through isolation checks. |
| `claim()`, last claim, prize 0, d = 0.0844, R = 1.44, worst funding | 6,921 (TN10) | 16,524 (TN10) | 146,283 | 0.0083 KAS | ≈ 0.15 KAS | Accepted on TN10 (146,259). |
| `claim()`, non-final on a bundle of 2, minimum prize 0.0426 | 7,378 (TN10) | 16,872 (TN10) | 499,976 | 0.0084 KAS | ≈ 0.50 KAS | Accepted on TN10. |

## Minimums

| Quantity | Value | Condition |
|---|---|---|
| Smallest output | 0.02 KAS plain; 0.08 KAS covenant-bound | Storage ≤ 500,000 with negligible relief (G3; TN10: 0.019 KAS plain rejected). |
| Ticket value (d × b) | 0.0844 KAS (K = 16, price 0.1); 0.0828 (price 1); 0.0842 / 0.0835 / 0.0828 for b = 2 / 5 / 10 | Every `buy()` valid without a referral output, worst case including a fresh shard at R. |
| Ticket value with a referral output | 0.1045 / 0.0871 / 0.0866 KAS (0.1 / 0.5 / 1 KAS referral, price 1); none for a 0.01 KAS referral, i.e. at price 0.1 the 10 % referral is never payable and goes to the boost pot | Every `buy()` valid; M_ref ≥ 0.02 KAS. |
| Ticket deposit at which storage stops being the block bottleneck | 5.0646 KAS | ≥ 63 buys per block, steady state. The worst-case floor of 35,974 storage per buy (13 per block) is independent of d. |
| Last-claim payout (prize + deposit) | 0.0212 KAS | Always met because d ≥ 0.0844. |
| Non-final claim prize on a bundle | 0.0426 / 0.0265 / 0.0236 KAS for b = 2 / 5 / 10 | Payout is the prize alone; prize 0 is invalid. |
| Reserve R per protocol object | 1.44 KAS (K = 16), 2.72 KAS (K = 32), 5.31 KAS (K = 64) | `close()` storage mass ≤ 500,000, worst case over sales, controller value O and caller funding. |
| Reserve R, `close()` within 10% of block storage | 17.04 KAS (K = 16), 33.06 KAS (K = 32) | Storage mass ≤ 50,000. |
| Largest K for `close()` | 62 / 37 / 23 | Shard sigscript 3,418 / 6,000 / 10,000 B (transient-mass bound, exact search); K = 62 accepted on TN10. |
