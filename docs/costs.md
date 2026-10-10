# Costs

Sizes and masses per transition, using the mainnet mass parameters of rusty-kaspa v2.1.0.

**Limits per transaction** (the same as per block, and confirmed on testnet-10: feasibility Q7, `docs/tn10-mass-check.md`): compute 500,000, storage 500,000, transient 1,000,000.

**Fees.**
- Minimum relay fee: 100 sompi per gram of max(compute, transient / 2). It ignores storage mass.
- Fee for priority: the mempool ranks by fee per gram of max(compute, transient / 2, storage). Paying 100 sompi per gram of that is the floor priority under load (feasibility Q4).

Evidence and citations: `docs/feasibility.md`. Minimums are rounded up to 0.0001 KAS.

The numbers below come from M0 probes (`covenants/m0/`), not protocol scripts. Replace each row when the real script exists.

## M1: shard and `buy()` (real scripts)

Source: `crates/lottery/tests/m1_measure.rs` (`cargo test -p lottery --release --test m1_measure -- --nocapture --test-threads=1`), shard `covenants/shard.sil`, 2026-10-10. Script units include the engine's 1-unit-per-pushed-byte charge (`crypto/txscript/src/runtime_resource_meter.rs:47-49`), which dominates: the redeem script and every slice are pushes.

**M-a / M-b: shard script per bundle slot and the K it allows at `close()`.** Each slot adds about 1,961 B of redeem script (18 unrolled levels, each used twice). `close()` is modelled as in M0 Q5: an M0-sized controller (190 + 420·K B) plus K shard inputs whose sigscript is the shard's redeem push plus a 300 B allowance for the M2 close leg. Units are for the shard input.

| B_MAX | Redeem B | Close sigscript B | `buy()` b = 1, m = 10: units / sigscript B | `buy()` b = B_MAX, m = 10: units / sigscript B | `buy()` b = B_MAX, m = 100: units | Largest K at 90 % | Largest K at 100 % | K = 16 transient | K = 16 compute |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 3,306 | 3,609 | 52,165 / 3,988 | 52,165 / 3,988 | 55,056 | 53 | 59 | 269,632 | 76,028 |
| 2 | 5,267 | 5,570 | 64,019 / 5,949 | 91,119 / 6,551 | 97,077 | 36 | 40 | 395,136 | 107,404 |
| 3 | 7,228 | 7,531 | 75,871 / 7,910 | 131,352 / 9,114 | 140,737 | 27 | 30 | 520,640 | 138,780 |
| 4 | 9,189 | 9,492 | 87,723 / 9,871 | 172,861 / 11,678 | 186,033 | 22 | 24 | 646,144 | 171,756 |
| **5 (committed)** | **11,150** | **11,453** | **99,575 / 11,832** | **215,637 / 14,241** | **232,956** | **18** | **20** | **771,648** | **203,132** |
| 6 | 13,111 | 13,414 | 111,427 / 13,793 | 259,679 / 16,804 | 281,505 | 16 | 17 | 897,152 | 234,508 |
| 7 | 15,072 | 15,375 | 123,279 / 15,754 | 305,001 / 19,367 | 331,689 | 14 | 15 | 1,022,656 | 265,884 |
| 8 | 17,033 | 17,336 | 135,131 / 17,715 | 351,589 / 21,930 | 383,500 | 12 | 13 | 1,148,160 | 297,260 |

B_MAX = 5: K = 16 uses 77 % of transient mass; the largest K at 90 % is 18. B_MAX = 6 fits K = 16 at 89.7 % only against M0's controller probe (712 B of headroom), which the M2 controller will exceed (DECISIONS 2026-10-10).

**M-c: `buy()` masses** (d = 0.11 KAS, price 1 KAS, fresh shard at R = 1.44 KAS):

| Shape | Compute | Transient | Storage | Shard units | Fee for priority (KAS) |
|---|---|---|---|---|---|
| b = 1, no referral, large change | 15,326 | 49,304 | 379,245 | 99,575 | 0.3792 |
| b = 1, referral 0.1 KAS, large change | 15,738 | 49,512 | 479,946 | 99,859 | 0.4799 |
| b = 1, referral 0.1 KAS, change 1 KAS | 15,738 | 49,512 | 468,509 | 99,859 | 0.4685 |
| b = 5, referral 0.5 KAS, large change | 19,347 | 59,148 | 98,682 | 215,921 | 0.0987 |
| b = 5, referral 0.5 KAS, change 1 KAS | 19,347 | 59,148 | 98,862 | 215,921 | 0.0989 |
| b = 5, m = 100, referral, large change | 20,448 | 62,752 | 98,682 | 233,240 | 0.0987 |
| b = 5, referral, on testnet-10 (M-e) | 19,347 | 59,148 | 99,453 | — | 0.5 paid |

Smallest deposit d per combination for storage ≤ 500,000 (fresh shard at R = 1.44, worst of large and 1 KAS change):

| b | No referral (KAS) | With referral 0.1 × b KAS (KAS) |
|---|---|---|
| 1 | 0.0826 | 0.1043 |
| 2 | 0.0409 | 0.0456 |
| 3 | 0.0271 | 0.0291 |
| 4 | 0.0203 | 0.0214 |
| 5 | 0.0162 | 0.0169 |

M_ref = 0.1 KAS: a referral output at M_ref has storage mass 100,000 on its own. At d = 0.11 a b = 1 buy with a referral reaches 96 % of the storage limit; M6 revisits d.

**M-d: hash.** 16 paths of depth 18, leaf at m = 10, verify only (`covenants/m1/paths_bench.sil`): OpBlake2b 502,956 units, OpBlake3 483,804, a 3.8 % saving. Below the 25 % threshold (spec D12), so OpBlake2b stays. Pushes, not hashing, dominate the cost.

## Scripts (M0 probes)

| Script | Redeem B | Sigscript B | Script units | Compute budget | Source test |
|---|---|---|---|---|---|
| Merkle verify + update, depth 16, OpBlake2b | 1,911 | 2,439 | 40,169 | 4 | `m0_merkle` |
| Merkle verify + update, depth 23, OpBlake2b | 2,666 | 3,418 | 67,323 | 6 | `m0_merkle` |
| Controller close probe, MAX_K = K | ≈ 190 + 420·K | ≈ redeem + 4 | ≈ 1,550 + 3,840·K | — | `m0_mass::q5` |

## Transitions (M0 estimates)

| Transition | Compute mass | Transient mass | Storage mass | Relay floor | Fee for priority | Notes |
|---|---|---|---|---|---|---|
| `buy()` M0 probe (superseded by M1 above), 1 ticket, d = 0.0844 (minimum), price 0.1, fresh shard | 6,612 | 15,648 | 475,839–499,992 | 0.0078 KAS | ≈ 0.48–0.50 KAS | Shard sigscript 3,418 B (Merkle only), R = 1.44. Accepted on TN10 at 499,992. |
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
