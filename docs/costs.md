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

(Superseded in M2: the shard now carries the real `close_leg`, and B_MAX = 4 under D32. See the M2 section below for the rerun of this table.) B_MAX = 5: K = 16 uses 77 % of transient mass; the largest K at 90 % is 18. B_MAX = 6 fits K = 16 at 89.7 % only against M0's controller probe (712 B of headroom), which the M2 controller will exceed (DECISIONS 2026-10-10).

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

## M2: controller, params and `close()` (real scripts)

Source: `crates/lottery/tests/m2_measure.rs` (`cargo test -p lottery --release --test m2_measure -- --nocapture --test-threads=1`) and the `m1_measure` rerun, scripts `covenants/controller.sil`, `params.sil`, `shard.sil` (with `close_leg`) and `marker.sil` (D28 placeholder), 2026-10-10. Mass limits as above; masses from the v2.1.0 calculator with mainnet parameters.

**B_MAX = 4** (D32 applied). At B_MAX = 5 a K = 16 `close()` used 98.2 % of transient mass (982,252) and so broke the 90 % target. K stays 16.

**M2-a: redeem sizes** (B_MAX = 4). Re-measured after the review fix R2 (the share-room `require` in `close()` check 2): the controller grew by 19 B, from 26,302 B. The other scripts are unchanged; the params and shard legs use 133 more units each because they read the longer controller script. K = 16 `close()`: compute +19, transient +76; storage unchanged.

| Object | Redeem B |
|---|---|
| controller | 26,321 |
| params | 2,918 |
| shard (`buy` + `close_leg`) | 11,361 |
| marker (placeholder) | 756 |

**M2-a: `close()` units per input and masses.** Every shard carries 1,000 KAS of sales and a non-trivial root (the worst script path). R = 1.69 KAS.

| K | Controller units / sigscript B | Params units / sigscript B | Shard units (max) / sigscript B | Compute | Transient | Storage | Priority |
|---|---|---|---|---|---|---|---|
| 4 | 1,015,364 / 26,417 | 190,525 / 2,930 | 207,413 / 11,373 | 99,988 | 304,152 | 118,411 | 152,076 |
| 16 | 3,358,075 / 26,417 | 190,525 / 2,930 | 207,413 / 11,373 | 289,996 | 856,824 | 402,301 | 428,412 |

Each shard adds about 15,800 compute and 46,056 transient mass. `set_params()` uses 120,376 params units. With change it costs compute 6,404, transient 13,896 and storage 16,980 (fee for priority 0.0170 KAS). Without change it costs 5,992, 13,688 and 0 (0.0068 KAS).

**M2-b: largest K** (B_MAX = 4, real scripts, sales in every shard):

| Limit | K = 16 mass | % of limit | Largest K at 90 % | Largest K at 100 % |
|---|---|---|---|---|
| compute | 289,996 | 58.0 | 16 (K_MAX) | 16 (K_MAX) |
| transient | 856,824 | 85.7 | 16 (K_MAX) | 16 (K_MAX) |
| storage (this shape) | 402,301 | 80.5 | 16 (K_MAX) | 16 (K_MAX) |
| storage (worst case, M2-c) | 449,692 | 89.9 | 16 | 16 |

K_MAX = 16 is the layout bound (the marker holds 16 roots). Transient mass sets the real ceiling: extrapolating +46,056 per shard, it reaches 90 % at K = 16 and 100 % at K = 19.

M1's model rerun (`m1_measure`, shard redeem with the real leg plus a 300 B allowance and an M0-sized controller) gives these results at B_MAX = 4: largest K 18 at 90 % and 20 at 100 %, K = 16 transient 785,152. At B_MAX = 5 the same model gives a largest K of 15 at 90 %.

**M2-c: reserve R = 1.69 KAS.** `close()` has K + 3 fresh covenant outputs: controller', params', K shards and the marker. The KIP-9 storage mass (rusty-kaspa v2.1.0 `consensus/core/src/mass/mod.rs:439-512`; C = 10¹², `constants.rs:26`) is Σ C·p²/v over the outputs minus the input relief |I|²·C/Σ inputs, where p is the plurality. Covenant P2SH outputs have p = 2 and the plain caller output has p = 1 (`mass/mod.rs:83-99`).
- The output values are:
  - controller', O − J_boost + A + R;
  - params', R;
  - each new shard, R;
  - the marker, S − P_R − C_close + J_boost + R;
  - the caller output, funding − R − fee + C_close.
- Because 0 ≤ J_boost ≤ O and A ≥ 0 (A3, A4 check 12), every covenant output is ≥ R.
- All K + 3 covenant outputs sit at R only when O = J_boost = A = 0 and there are no sales. The relief then tends to 0 as the caller's funding grows. A large pot instead takes one output off R: the controller' when J_boost = 0, or the marker when J_boost = O.
- Builder policy: the caller output is at least 1 KAS (`close::MIN_CALLER_OUTPUT`). This is not a script check. It caps the caller term at C/1 KAS = 10,000, which is below one covenant output at R (4C/R = 23,668). So the output a large pot takes off R always outweighs the caller term. With O = 0 and a small caller output, the relief exceeds the caller term.

**Supremum: storage ≤ 4C(K + 3)/R. So R = 4C(K + 3)/(0.9 L) keeps every `close()` within 90 % of L = 500,000.** At K = 16 that is 168,888,889 sompi, rounded up to **R = 1.69 KAS**. At K = 32 it would be 3.12 KAS. This supersedes the M0 formula max(4C(K + 2)/L, 4C(K + 1)/(L − 10,101)) for `close()`. That formula counted K + 2 outputs at R and allowed 100 %.

Measured worst case over a grid, at R = 1.69:
- grid: sales per shard ∈ {0, 10, 10,000 KAS}; O ∈ {0, 10⁵, 10⁷ KAS}; J_boost ∈ {0, O/2, O}; A ∈ {0, 2 KAS}; caller ∈ {1, 10, 10³, 10⁶, 10⁸ KAS};
- `m2c_reserve_recheck` asserts ≤ 90 % for every K from 1 to 16.

| K | Caller output | Worst storage | % of limit | Worst shape |
|---|---|---|---|---|
| 4 | 1 KAS | 152,008 | 30.4 | zero sales, O = 10⁷ KAS, J_boost = 0 |
| 4 | ≥ 10⁶ KAS | 165,676 | 33.1 | zero sales, O = A = 0 (= supremum) |
| 16 | 1 KAS | 436,024 | 87.2 | zero sales, O = 10⁷ KAS, J_boost = 0 |
| 16 | 10 KAS | 427,024 | 85.4 | same |
| 16 | 1,000 KAS | 436,456 | 87.3 | zero sales, O = A = 0 |
| 16 | ≥ 10⁶ KAS | **449,692** | **89.9** | zero sales, O = A = 0 (= supremum) |

At the old R = 1.44 KAS the same K = 16 worst case is 527,763 (105.6 %), which is over the limit.

**R in the other shapes.** A larger R only lowers storage mass, because R appears in outputs (C/R terms) and in inputs (relief).
- M1 `buy()` reruns at R = 1.69, d = 0.11 KAS, price 1:
  - b = 1, no referral, large change: storage 377,721;
  - b = 1, referral 0.1 KAS: storage 478,296 with large change and 468,152 with 1 KAS change;
  - b = 4, referral: storage 122,690.
  The b = 1 referral shape is still close to the limit, but that comes from d and the referral output, not R (M6 revisits d).
- Smallest d per combination: 0.0823 / 0.0408 / 0.0271 / 0.0203 KAS for b = 1..4 without a referral, and 0.1038 / 0.0455 / 0.0291 / 0.0214 KAS with one.
- `set_params()`: 16,980.
- Genesis at K = 16: 413,196 (testnet-10 below).
- `draw()` (M3) re-checked R against its own outputs: M3-d below (supremum 58.4 % at f_min = 1 KAS, 76.4 % at the floor f_min = 0.1 KAS; at A_MAX = 7, M4: 60.8 % / 78.8 %).

**M2-d: priority fee against C_close** (C_close = s_C · S, s_C = 50 bp; fee = 100 sompi per gram of max(compute, transient/2, storage)):

| K | Priority mass | Fee for priority (KAS) | Sales S at which C_close covers it (KAS) |
|---|---|---|---|
| 4 | 152,076 | 0.1521 | 30.42 |
| 16 | 428,412 | 0.4284 | 85.68 |
| 16, storage worst case (M2-c) | 449,692 | 0.4497 | 89.94 |

Below about 90 KAS of round sales at K = 16, the caller pays part of the fee out of pocket. This is the D19 liveness note. The relay floor alone (max(compute, transient/2)) is 0.4284 KAS at K = 16.

**M2-e** (testnet-10): genesis plus a real K = 16 `close()` were accepted and mined. Storage was 435,348 and the fee 0.4354 KAS (`docs/tn10-mass-check.md`).

## M3: drawer, vault, pool and `draw()` with a TEST-ONLY seed (real scripts)

Source: `crates/lottery/tests/m3_measure.rs` (`cargo test -p lottery --release --test m3_measure -- --nocapture --test-threads=1`), scripts `covenants/drawer.sil`, `vault.sil`, `pool.sil`, `marker.sil` (with `draw_leg`) and `controller.sil` (with `draw_leg`), 2026-10-10. B_MAX = 4, R = 1.69 KAS. Re-run after the M3 Stage 5 review fixes (drawer: f_min ≥ F_MIN_FLOOR = 0.1 KAS, +4 B; every row below is the re-run).

**Limits, re-verified against the pinned rusty-kaspa v2.1.0 (`refs/rusty-kaspa-v2.1.0`):**
- compute 500,000, storage 500,000, transient 1,000,000 per block: `consensus/core/src/config/params.rs:626` (MAINNET_PARAMS) and `:683` (TESTNET_PARAMS). They are enforced per block in `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:88-95`, so a transaction above one of them can never be mined.
- Storage mass (KIP-9): `consensus/core/src/mass/mod.rs:439-512`. Plurality `:83-99`: ⌈(63 + script length + 32 if a covenant ID) / 100⌉, so a covenant P2SH output has p = 2 (130 B) and a standard P2PK or P2SH output p = 1. C = STORAGE_MASS_PARAMETER = 10¹² (`consensus/core/src/constants.rs:26`).
- A zero-value output is invalid: `consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs:150-151`.

**M3-a: redeem sizes.**

| Object | Prefix B | Suffix B | State B | Redeem B |
|---|---|---|---|---|
| drawer (`draw_test_seed`) | 1 | 41,944 | 104 | 42,049 |
| marker (`draw_leg`; M2: 756 B placeholder) | 1 | 610 | 715 | 1,326 |
| vault (`draw_leg`) | 1 | 537 | 56 | 594 |
| pool (`absorb_leg` + placeholder `claim`) | 1 | 620 | 607 | 1,228 |
| controller (`close` + `draw_leg`; M2: 26,321 B) | 1 | 27,266 | 427 | 27,694 |

**M3-b: `draw()` script units per input and masses.** Worst proofs: every shard's leaf at the winning rank is non-empty with every bonus held (h_J = K, n2 = m − 1). Fee paid, one funding input. np = 5 is the spec's shape (the fixture funds three lower tiers). np = 7 is the script maximum (jackpot, second prize and five lower tiers, all tier shares > 0).

| np | K | m | a | Drawer units / sigscript B | Marker | Vault | Controller | Pool (max) | Compute | Transient | Storage | Priority | % c / t / s |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 5 | 4 | 10 | 0 | 926,979 / 45,182 | 297,464 | 296,001 | 63,734 | — | 96,756 | 304,464 | 29,588 | 152,232 | 19.4 / 30.4 / 5.9 |
| 5 | 4 | 10 | 5 | 971,005 / 45,182 | 297,464 | 296,001 | 63,734 | 297,287 | 120,291 | 331,404 | 59,173 | 165,702 | 24.1 / 33.1 / 11.8 |
| 5 | 4 | 50 | 0 | 939,026 / 45,503 | 297,464 | 296,001 | 63,734 | — | 97,177 | 305,748 | 29,588 | 152,874 | 19.4 / 30.6 / 5.9 |
| 5 | 4 | 50 | 5 | 983,052 / 45,503 | 297,464 | 296,001 | 63,734 | 297,287 | 120,712 | 332,688 | 59,173 | 166,344 | 24.1 / 33.3 / 11.8 |
| 5 | 16 | 10 | 0 | 1,306,904 / 52,335 | 297,464 | 296,001 | 63,734 | — | 107,709 | 333,076 | 29,588 | 166,538 | 21.5 / 33.3 / 5.9 |
| 5 | 16 | 10 | 5 | 1,350,930 / 52,335 | 297,464 | 296,001 | 63,734 | 297,287 | 131,244 | 360,016 | 59,173 | 180,008 | 26.2 / 36.0 / 11.8 |
| 5 | 16 | 50 | 0 | 1,352,558 / 53,615 | 297,464 | 296,001 | 63,734 | — | 109,489 | 338,196 | 29,588 | 169,098 | 21.9 / 33.8 / 5.9 |
| 5 | 16 | 50 | 5 | 1,396,584 / 53,615 | 297,464 | 296,001 | 63,734 | 297,287 | 132,924 | 365,136 | 59,173 | 182,568 | 26.6 / 36.5 / 11.8 |
| 7 | 16 | 50 | 5 | 1,426,372 / 53,615 | 297,464 | 296,001 | 63,734 | 297,287 | 134,138 | 365,832 | 59,175 | 182,916 | 26.8 / 36.6 / 11.8 |

The np = 7 rows at K = 4 and the other K = 16 shapes are 0.1–0.3 points above their np = 5 rows (the full table is printed by `m3b_draw_units_and_masses`). Each delegate leg reads the drawer's 42 kB script once (about 296,000 units, mostly the 1-unit-per-pushed-byte charge), so the legs, not the K proofs, dominate compute. Transient mass is about 4 grams per byte (the drawer and controller redeems are pushed once each).

**DP21** (K = 16, m = M_DRAW = 50, a = A_MAX = 5, worst proofs; accepted, value conserved). *M4: A_MAX is now 7 and the pool and marker grew; see M4-e below (worst 34.0 / 43.7 / 14.2 %) and DECISIONS U5.*

| np | Funding inputs | Compute | Transient | Storage | % c / t / s |
|---|---|---|---|---|---|
| 5 | 1 | 132,924 | 365,136 | 59,173 | 26.6 / 36.5 / 11.8 |
| 5 | 8 (F_MAX) | 140,764 | 368,496 | 59,173 | 28.2 / 36.8 / 11.8 |
| 7 | 1 | 134,138 | 365,832 | 59,175 | 26.8 / 36.6 / 11.8 |
| 7 | 8 (F_MAX) | 141,978 | 369,192 | 59,175 | 28.4 / 36.9 / 11.8 |

**M3-c: drawer redeem B by the template constants K_MAX and M_DRAW** (state 104 B; each variant compiled from drawer.sil with only those two constants changed):

| K_MAX | M_DRAW 10 | 25 | 50 | 100 |
|---|---|---|---|---|
| 4 | 23,932 | 25,165 | 27,215 | 31,315 |
| 8 | 28,876 | 30,109 | 32,159 | 36,261 |
| **16** | 38,766 | 39,999 | **42,049 (committed)** | 46,149 |

Each shard of K_MAX adds about 1,236 B (one unrolled leaf and 18 levels). Each 1 of M_DRAW adds about 82 B (the unrolled check 10 loop).

Worst shape at K = 16 (a = 5, np = 7, F_MAX funding inputs) by m with the committed drawer: m = 10: 28.1 / 36.4 / 11.8 %; m = 25: 28.2 / 36.6 / 11.8 %; m = 50: 28.4 / 36.9 / 11.8 % (compute / transient / storage). All are within 90 %, so **m_max stays at M_DRAW = 50** (D49; genesis requires m_max ≤ M_DRAW). This is not the stop case (m_max below 50).

M_DRAW = 100 cannot run as committed: the drawer's 128-byte zero buffer covers only 2m ≤ 128. An estimate from +4,100 B of script and +1,600 B of leaves at 4 grams per byte gives transient ≈ 391,992 (39.2 %). Script units were not estimated.

**M3-d: R re-check for `draw()` storage mass.** Outputs:
- drawer' R;
- vault' V' + pend + R ≥ R;
- controller' O' + A' + R ≥ R;
- up to 7 new pools, each amount + R > R;
- these 3 + 7 are covenant outputs with p = 2;
- the fee T_F ≥ f_min, the marker refund R and a pool refunds R, all with p = 1;
- the caller output, which is not bounded by the script.

The input relief only lowers the mass. So the supremum is 10·4C/R + (1 + a)·C/R + C/f_min + C/caller. At R = 1.69 KAS, f_min = F_MIN = 1 KAS, caller ≥ 1 KAS (`close::MIN_CALLER_OUTPUT`, used here as a builder policy), a = 5:

| a | Caller output (KAS) | Storage (every covenant output at or near R, V' = 0, T_F = f_min) | % of limit | Supremum |
|---|---|---|---|---|
| 0 | 1 | 224,018 | 44.8 | 262,597 |
| 0 | ≥ 10⁶ | 252,394 | 50.5 | 262,597 |
| 5 | 1 | 169,847 | 34.0 | 292,182 |
| 5 | 1,000 | 278,493 | 55.7 | 292,182 |
| 5 | ≥ 10⁶ | 281,979 | 56.4 | 292,182 |

**Supremum at a = A_MAX: 292,182 (58.4 %), which is within 90 %, so R stays at 1.69 KAS.** *(a = 5. M4 raised A_MAX to 7: the supremum is 304,016 (60.8 %), see M4-e below and DECISIONS U5.)* `draw()` alone would need R ≥ 1.07 KAS at f_min = 1 KAS (about 1.353 KAS at f_min = F_MIN_FLOOR), so `close()` (M2-c) sets R.

The fee term C/T_F depends on f_min:
- At R = 1.69 KAS the supremum stays within 90 % only for f_min ≥ 5,958,837 sompi (0.0596 KAS).
- f_min = 0 with T_F = 0 (no sales and acc = 0) would require a zero-value fee output, which is invalid, so that round could never be drawn.

**Stage 5 (user decision A): f_min ≥ F_MIN_FLOOR = 0.1 KAS** is now required by the drawer (A3.1 range, check 1, `drawer.sil`) and by genesis `validate()`; f_min is copied unchanged into drawer' and no admin path sets it. The worst state therefore has T_F = F_MIN_FLOOR. Measured on the same worst shapes (`m3d_reserve_recheck`, f_min = T_F = F_MIN_FLOOR):

| a | Caller output (KAS) | Storage | % of limit | Supremum |
|---|---|---|---|---|
| 0 | 1 | 312,308 | 62.5 | 352,597 |
| 0 | ≥ 10⁶ | 342,394 | 68.5 | 352,597 |
| 5 | 1 | 256,009 | 51.2 | 382,182 |
| 5 | 1,000 | 368,493 | 73.7 | 382,182 |
| 5 | ≥ 10⁶ | 371,979 | 74.4 | 382,182 |

**Supremum at a = A_MAX and f_min = F_MIN_FLOOR: 382,182 (76.4 %), within 90 %.** *(a = 5; at A_MAX = 7 (M4) it is 394,016 (78.8 %), see M4-e below.)* R stays at 1.69 KAS. A zero-sales round at f_min = F_MIN_FLOOR carries the fee (T_F = 0 < f_min, no fee output) and is drawn end to end (`m3_draw::dp23_zero_sales_round_at_f_min_floor`).

**M3-e: priority fee of `draw()` against C_draw = s_D · S** (s_D = 50 bp; fee = 100 sompi per gram of max(compute, transient/2, storage)):

| Shape | Priority mass | Fee for priority (KAS) | Sales S at which C_draw covers it (KAS) |
|---|---|---|---|
| K = 4, m = 10, a = 0, np = 5 | 152,232 | 0.1522 | 30.45 |
| K = 16, m = 50, a = 5, np = 5 | 182,568 | 0.1826 | 36.51 |
| K = 16, m = 50, a = 5, np = 7, 8 funding inputs | 184,596 | 0.1846 | 36.92 |
| storage supremum (M3-d, f_min = 1 KAS) | 292,182 | 0.2922 | 58.44 |
| storage supremum (M3-d, f_min = F_MIN_FLOOR = 0.1 KAS) | 382,182 | 0.3822 | 76.44 |

Below about 37 KAS of round sales (58 KAS in the storage worst case at f_min = 1 KAS, 76 KAS at f_min = F_MIN_FLOOR), the caller pays part of the fee out of pocket. This is the same liveness note as D19 for `close()`.

**M3-f: K = 16 `close()` re-measured** with the M3 controller (27,694 B, +1,373 B for `draw_leg`) and marker (1,326 B, +570 B). Same shape as M2-a: 1,000 KAS sales per shard, distinct roots.

| K | Controller units | Compute | Transient | Storage | % c / t / s |
|---|---|---|---|---|---|
| 4 | 1,029,308 | 102,533 | 311,932 | 118,411 | 20.5 / 31.2 / 23.7 |
| 16 | 3,372,019 | 293,841 | **864,604** | 402,301 | 58.8 / **86.5** / 80.5 |

Transient mass rose by 7,780 (856,824 → 864,604) and stays within 90 % (D32), with 35,396 grams of headroom. The M2-c storage worst case (449,692, 89.9 %) is unchanged, because close()'s outputs did not change. Not the stop case.

## M4: ticket `claim()`, `return_deposit()`, `refresh()` (real scripts)

Source: `crates/lottery/tests/m4_measure.rs` (`cargo test -p lottery --release --test m4_measure -- --nocapture --test-threads=1`), scripts `covenants/ticket.sil` (`claim`, `return_deposit_after`, `return_deposit_with_pool`, `refresh`), `pool.sil` (`absorb_leg`, `ticket_leg`, `refresh`), `marker.sil` (`draw_leg`, `refresh`) and `drawer.sil` (ticket_tpl in state, A_MAX = 7), 2026-10-11. B_MAX = 4, R = 1.69 KAS, d = 0.11 KAS, P_MIN = 0.05 KAS. Re-run after the M4 review fixes (U3 genesis rule, U5 A_MAX = 7); only the drawer changed, so M4-a (drawer row), M4-e and the M3-d supremum below carry the A_MAX = 7 figures and every other table is unchanged. Percentages are of compute 500,000 / transient 1,000,000 / storage 500,000.

**M4-a: redeem sizes** (B_MAX = 4).

| Object | Prefix B | Suffix B | State B | Redeem B |
|---|---|---|---|---|
| ticket (`claim`, two returns, `refresh`; M1: placeholder) | 1 | 25,359 | 92 | 25,452 |
| pool (`absorb_leg`, `ticket_leg`, `refresh`; M3: 1,228) | 1 | 1,831 | 649 | 2,481 |
| marker (`draw_leg`, `refresh`; M3: 1,326) | 1 | 1,388 | 715 | 2,104 |
| drawer (`draw_test_seed`, A_MAX = 7; M3: 42,049; 42,914 with ticket_tpl at A_MAX = 5) | 1 | 45,328 | 137 | 45,466 |
| vault (`draw_leg`) | 1 | 610 | 56 | 667 |
| controller (`close`, `draw_leg`) | 1 | 27,266 | 427 | 27,694 |
| shard (`buy`, `close_leg`) | 1 | 11,244 | 116 | 11,361 |

The drawer suffix is redeem − prefix − state. The drawer grew by 865 B for `ticket_tpl` and the larger `PoolState` (Stage 1), then to 45,466 B for A_MAX = 7.

Ticket redeem B by the template constants K_MAX (rows) and M_CLAIM (columns):

| K_MAX | M_CLAIM 10 | 25 | 50 |
|---|---|---|---|
| 4 | 10,834 | 10,836 | 10,836 |
| 8 | 15,706 | 15,708 | 15,708 |
| **16** | 25,450 | 25,452 | **25,452 (committed)** |

Each unit of K_MAX adds about 1.22 kB (the proof loop is unrolled K_MAX times); M_CLAIM barely matters (the 128-byte zero buffer is built by doubling).

**M4-b: `claim()` units per input and masses.** Worst proofs: every shard holds the combination once, so h = K and every leaf is a full hash plus 18 levels (as DP21). PAY slice 2·P_MIN·K, ACCRUE slice K (prize 1 sompi), FINAL = PAY with the other index already claimed. One funding input, change. Proof bytes = k·(576 + 2m).

| Mode | K | m | Proof B | Ticket units / sigscript B | Pool leg units | Compute | Transient | Storage | Priority | % c / t / s |
|---|---|---|---|---|---|---|---|---|---|---|
| PAY | 4 | 10 | 2,384 | 288,826 / 27,854 | 183,627 | 38,007 | 123,788 | 277,425 | 277,425 | 7.6 / 12.4 / 55.5 |
| PAY | 4 | 50 | 2,704 | 293,399 / 28,175 | 183,627 | 38,428 | 125,072 | 277,425 | 277,425 | 7.7 / 12.5 / 55.5 |
| PAY | 16 | 10 | 9,536 | 667,302 / 35,007 | 183,651 | 48,960 | 152,400 | 275,149 | 275,149 | 9.8 / 15.2 / 55.0 |
| PAY | 16 | 50 | 10,816 | 700,682 / 36,287 | 183,651 | 50,640 | 157,520 | 275,149 | 275,149 | 10.1 / 15.8 / 55.0 |
| ACCRUE | 4 | 10 | 2,384 | 288,322 / 27,854 | 183,627 | 37,595 | 123,580 | 178,337 | 178,337 | 7.5 / 12.4 / 35.7 |
| ACCRUE | 4 | 50 | 2,704 | 292,895 / 28,175 | 183,627 | 38,016 | 124,864 | 178,337 | 178,337 | 7.6 / 12.5 / 35.7 |
| ACCRUE | 16 | 10 | 9,536 | 666,798 / 35,007 | 183,651 | 48,548 | 152,192 | 178,337 | 178,337 | 9.7 / 15.2 / 35.7 |
| ACCRUE | 16 | 50 | 10,816 | 700,178 / 36,287 | 183,651 | 50,228 | 157,312 | 178,337 | 178,337 | 10.0 / 15.7 / 35.7 |
| FINAL | 4 | 10 | 2,384 | 186,107 / 27,854 | 183,627 | 36,550 | 123,440 | 26,857 | 61,720 | 7.3 / 12.3 / 5.4 |
| FINAL | 4 | 50 | 2,704 | 190,680 / 28,175 | 183,627 | 36,971 | 124,724 | 26,857 | 62,362 | 7.4 / 12.5 / 5.4 |
| FINAL | 16 | 10 | 9,536 | 564,580 / 35,007 | 183,651 | 47,503 | 152,052 | 24,581 | 76,026 | 9.5 / 15.2 / 4.9 |
| FINAL | 16 | 50 | 10,816 | 597,960 / 36,287 | 183,651 | 49,083 | 157,172 | 24,581 | 78,586 | 9.8 / 15.7 / 4.9 |

PAY at K = 16, m = 50 with F_MAX = 8 funding inputs: ticket 701,172 units, pool 183,651; compute 58,480, transient 160,880, storage 177,525 (11.7 / 16.1 / 35.5 %). Storage in PAY and ACCRUE comes from the small ticket' (and payout) outputs, not the script. Testnet-10 accepted a K = 16, m = 10 FINAL claim with the same compute and transient (M4-i).

**M4-c: P_MIN.** Worst PAY: b = 2, d = 0.11 KAS (ticket' 0.22 KAS), pool' at R = 1.69 KAS, one funding input. Storage by payout P:

| Change | P = 0.03 KAS | 0.04 | 0.0426 | 0.05 (P_MIN) | 0.1 | Smallest P ≤ 90 % (sompi) | Rounded up to 0.01 KAS |
|---|---|---|---|---|---|---|---|
| 1 KAS | 466,584 | 383,521 | 368,332 | 333,791 | 235,101 | 3,157,513 (0.031575 KAS) | 0.04 KAS |
| 1,000 KAS | 538,584 | 455,251 | 439,992 | 405,251 | 305,251 | 4,085,802 (0.040858 KAS) | 0.05 KAS |
| 1,000,000 KAS | 538,819 | 455,486 | 440,227 | 405,486 | 305,486 | 4,089,729 (0.040897 KAS) | 0.05 KAS |

The worst case is a very large change (the KIP-9 input relief shrinks as the input total grows; the 1,000,000 KAS row is the supremum within about 0.1 %), not the 1 KAS change the spec named. **Measured P_MIN = 0.05 KAS (5,000,000 sompi): D57 confirmed, the constant is unchanged.** At P_MIN the worst PAY storage is 405,486 (81.1 %). The spec's 0.0426 KAS estimate is 0.0409 KAS measured; both round up to 0.05. The bound assumes d ≥ D_CLAIM_FLOOR = 0.11 KAS (genesis rule).

**M4-d: `buy()` with the M4 ticket template** (TICKET_TEMPLATE_HASH changed; shard units include pushing the ticket's 25,452 B redeem; d = 0.11 KAS, price 1 KAS, m = 10, R = 1.69 KAS):

| Shape | Shard units | Shard sigscript B | Compute | Transient | Storage | % c / t / s |
|---|---|---|---|---|---|---|
| b = 1, no referral, large change | 354,091 | 37,378 | 43,472 | 151,488 | 377,721 | 8.7 / 15.1 / 75.5 |
| b = 1, referral 0.1 KAS, large change | 354,375 | 37,378 | 43,884 | 151,696 | 478,296 | 8.8 / 15.2 / 95.7 |
| b = 1, referral 0.1 KAS, change 1 KAS | 354,375 | 37,378 | 43,884 | 151,696 | 468,152 | 8.8 / 15.2 / 93.6 |
| b = 4, referral 0.4 KAS, large change | 439,513 | 39,185 | 46,491 | 158,924 | 122,690 | 9.3 / 15.9 / 24.5 |
| b = 4, referral 0.4 KAS, change 1 KAS | 439,513 | 39,185 | 46,491 | 158,924 | 121,677 | 9.3 / 15.9 / 24.3 |

Compared with M3 (placeholder ticket), compute rose about 2.8× and transient about 3×, both far below 90 % (D64). Storage is unchanged by the script: the b = 1 referral buy at 95.7 % / 93.6 % is the pre-existing D15 figure, accepted until M6 sets d (DECISIONS U2); `m4_measure` asserts ≤ 100 % for it. The largest K at 90 % for `close()` stays 16 (M2-b) and 18 in M1's model at B_MAX = 4.

**M4-e: DP21 with the M4 pool (2,481 B) and marker, A_MAX = 7** (K = 16, m = M_DRAW = 50, a = 7, worst proofs; accepted, value conserved). Measured before raising A_MAX; every mass ≤ 90 %, so A_MAX = 7 was kept (DECISIONS U5, supersedes D44). Pool leg units at most 323,673.

| np | Funding inputs | Compute | Transient | Storage | % c / t / s |
|---|---|---|---|---|---|
| 5 | 1 | 160,788 | 432,912 | 71,007 | 32.2 / 43.3 / 14.2 |
| 5 | 8 (F_MAX) | 168,628 | 436,272 | 71,007 | 33.7 / 43.6 / 14.2 |
| 7 | 1 | 162,202 | 433,608 | 71,009 | 32.4 / 43.4 / 14.2 |
| **7** | **8 (F_MAX)** | **170,042** | **436,968** | **71,009** | **34.0 / 43.7 / 14.2** |

The worst row is 7 pools created and 7 absorbed: 19 inputs, 20 outputs. At A_MAX = 5 with the same M4 scripts the worst was 153,270 / 405,960 / 59,175 (30.7 / 40.6 / 11.8 %; drawer 1,565,812 units, pool leg 305,809). M3-c worst by m at a = 7 (np 7, F_MAX): 34.0 / 43.7 / 14.2 % at m = 50, so m_max stays 50.

**M3-d at A_MAX = 7** (R re-check for `draw()` storage, same method as M3-d): the supremum 10·4C/R + (1 + a)·C/R + C/f_min + C/caller is **304,016 (60.8 %)** at f_min = 1 KAS and **394,016 (78.8 %)** at f_min = F_MIN_FLOOR (was 292,182 / 382,182 at a = 5); the measured worst shapes reach 58.8 % and 76.8 %. R stays 1.69 KAS; `draw()` alone would need R ≥ 1.116 KAS at f_min = 1 KAS.

**M4-f: K = 16 `close()` with the M4 marker (2,104 B, +705 B for `refresh`, +73 B for the drawer struct).** Same shape as M2-a / M3-f.

| K | Controller units | Compute | Transient | Storage | % c / t / s |
|---|---|---|---|---|---|
| 4 | 1,037,088 | 103,411 | 315,044 | 118,411 | 20.7 / 31.5 / 23.7 |
| 16 | 3,379,799 | 294,619 | **867,716** | 402,301 | 58.9 / **86.8** / 80.5 |

Transient rose by 3,112 from M3-f (864,604) and stays within 90 % (D32, D64). **Headroom below 90 %: 32,284 grams, the tightest margin of any shape (about 4 grams per marker byte).** A_MAX does not affect `close()`. The M2-c storage worst case is unchanged at 449,692 (89.9 %). Testnet-10 accepted the same `close()` (M4-i).

**M4-g: `return_deposit()` and `refresh()`** (one funding input, change):

| Transaction | Leader units / sigscript B | Pool leg units | Compute | Transient | Storage | % c / t / s |
|---|---|---|---|---|---|---|
| `return_deposit_after` (b = 2 ticket) | 51,819 / 25,460 | — | 28,052 | 103,328 | 37,659 | 5.6 / 10.3 / 7.5 |
| `return_deposit_with_pool` (b = 2 ticket, K = 1 pool) | 96,636 / 25,464 | 183,576 | 33,260 | 113,880 | 41,239 | 6.7 / 11.4 / 8.2 |
| `refresh` ticket | 153,991 / 25,460 | — | 29,097 | 103,468 | 174,023 | 5.8 / 10.3 / 34.8 |
| `refresh` pool (K = 4) | 22,647 / 2,489 | — | 4,826 | 11,584 | 7,869 | 1.0 / 1.2 / 1.6 |
| `refresh` marker (K = 4) | 20,046 / 2,112 | — | 4,449 | 10,076 | 310 | 0.9 / 1.0 / 0.1 |
| `refresh` marker (K = 16) | 20,046 / 2,112 | — | 4,449 | 10,076 | 310 | 0.9 / 1.0 / 0.1 |

**M4-h: `claim()` priority fee** (100 sompi per gram of priority mass, `harness::priority_fee`; informational):

| Shape | Priority mass | Fee for priority (KAS) | % of P_MIN | % of 0.1 KAS | % of 1 KAS |
|---|---|---|---|---|---|
| PAY, K = 4, m = 10 | 277,425 | 0.2774 | 554.9 | 277.4 | 27.74 |
| PAY, K = 16, m = 50 | 275,149 | 0.2751 | 550.3 | 275.1 | 27.51 |
| ACCRUE, K = 16, m = 50 | 178,337 | 0.1783 | 356.7 | 178.3 | 17.83 |
| FINAL, K = 16, m = 50 | 78,586 | 0.0786 | 157.2 | 78.6 | 7.86 |

At the worst PAY storage at P = P_MIN (405,486, M4-c) the fee for priority is about 0.405 KAS (8 × P_MIN). So under load (full blocks), a PAY claim of a small prize costs several times the prize, because storage mass (C / payout, C / ticket') dominates. The relay floor ignores storage mass, so the fee is far lower when blocks are not full. Open for M6 (P_MIN, d, batching claims; DECISIONS "OPEN for M6" row).

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
| `claim()` M0 probe (superseded by M4-b), last claim, prize 0, d = 0.0844, R = 1.44, worst funding | 6,921 (TN10) | 16,524 (TN10) | 146,283 | 0.0083 KAS | ≈ 0.15 KAS | Accepted on TN10 (146,259). |
| `claim()` M0 probe (superseded by M4-b, M4-c), non-final on a bundle of 2, minimum prize 0.0426 | 7,378 (TN10) | 16,872 (TN10) | 499,976 | 0.0084 KAS | ≈ 0.50 KAS | Accepted on TN10. |

## Minimums

| Quantity | Value | Condition |
|---|---|---|
| Smallest output | 0.02 KAS plain; 0.08 KAS covenant-bound | Storage ≤ 500,000 with negligible relief (G3; TN10: 0.019 KAS plain rejected). |
| Ticket value (d × b) | 0.0844 KAS (K = 16, price 0.1); 0.0828 (price 1); 0.0842 / 0.0835 / 0.0828 for b = 2 / 5 / 10 | Every `buy()` valid without a referral output, worst case including a fresh shard at R. |
| Ticket value with a referral output | 0.1045 / 0.0871 / 0.0866 KAS (0.1 / 0.5 / 1 KAS referral, price 1); none for a 0.01 KAS referral, i.e. at price 0.1 the 10 % referral is never payable and goes to the boost pot | Every `buy()` valid; M_ref ≥ 0.02 KAS. |
| Ticket deposit at which storage stops being the block bottleneck | 5.0646 KAS | ≥ 63 buys per block, steady state. The worst-case floor of 35,974 storage per buy (13 per block) is independent of d. |
| Last-claim payout (prize + deposit) | 0.0212 KAS | Always met because d ≥ 0.0844. |
| Non-final claim prize on a bundle | 0.0426 / 0.0265 / 0.0236 KAS for b = 2 / 5 / 10 (M0 estimate) | Payout is the prize alone; prize 0 is invalid. |
| P_MIN, smallest PAY payout (M4) | **0.05 KAS** (measured 0.040897 KAS at the worst change, rounded up to 0.01 KAS) | Worst PAY claim (b = 2, d = 0.11 KAS, pool' at R) ≤ 90 % of storage; smaller prizes accrue into the ticket (D56, D57). M4-c above. |
| Reserve R per protocol object | **1.69 KAS (K = 16, M2)**: R = 4C(K + 3)/(0.9 × 500,000), rounded up to 0.01 KAS; 3.12 KAS at K = 32. M0 figures, now superseded: 1.44 / 2.72 / 5.31 KAS (K = 16 / 32 / 64) | `close()` storage mass ≤ 90 % of 500,000, worst case over every A3 state and caller output ≥ 1 KAS (builder policy). M2-c above. |
| Reserve R, `close()` within 10% of block storage | 17.04 KAS (K = 16), 33.06 KAS (K = 32) | Storage mass ≤ 50,000. |
| Largest K for `close()` | 62 / 37 / 23 | Shard sigscript 3,418 / 6,000 / 10,000 B (transient-mass bound, exact search); K = 62 accepted on TN10. |
