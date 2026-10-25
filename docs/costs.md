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

## M5: house vault shares (requests, conversion in `draw()`, settlement; real scripts)

Source: `crates/lottery/tests/m5_measure.rs` (`cargo test -p lottery --release --test m5_measure -- --nocapture --test-threads=1`), scripts `covenants/vault.sil` (`draw_leg`, `request_deposit`, `request_withdraw`), `request.sil` (`settle`, `refresh`), `record.sil` (`request_leg`, `refresh`), `position.sil` (`vault_leg`, `refresh`) and `drawer.sil` (conversion C1–C3, check 26), 2026-10-12, after the M5 review fixes (vault age window D10/W12, drained-record check). B_MAX = 4, R = 1.69 KAS, MIN_DEPOSIT = 1 KAS, P_MIN = 0.05 KAS. Percentages are of compute 500,000 / transient 1,000,000 / storage 500,000. Every D78 stop rule holds (all ≤ 90 %).

**M5-a: redeem sizes** (B_MAX = 4).

| Object | Prefix B | Suffix B | State B | Redeem B |
|---|---|---|---|---|
| vault (`draw_leg`, `request_deposit`, `request_withdraw`; M4: 667) | 1 | 3,224 | 155 | 3,380 |
| request (`settle`, `refresh`; 2 mulfrac copies) | 1 | 15,420 | 58 | 15,479 |
| record (`request_leg`, `refresh`) | 1 | 1,026 | 127 | 1,154 |
| position (`vault_leg`, `refresh`) | 1 | 1,137 | 56 | 1,194 |
| drawer (`draw_test_seed`, 3 mulfrac copies; M4: 45,466) | 1 | 65,096 | 137 | 65,234 |
| marker (`draw_leg`, `refresh`; size unchanged, DRAWER_TPL value changed) | 1 | 1,388 | 715 | 2,104 |

**One inlined `mulfrac` call costs 6,117 B (drawer) / 6,118 B (request)**, measured as the code-size difference to a variant with one call replaced by a plain expression. The drawer's +19,768 B over M4 are mostly its three calls.

**M5-b: DP21 with the conversion and the record (D78a)** (K = 16, m = 50, a = A_MAX = 7, worst proofs; accepted, value conserved). Drawer sigscript 79,043–79,049 B; vault leg 463,829 units.

| Pending | np | Funding inputs | Drawer units | Outputs | Compute | Transient | Storage | % c / t / s |
|---|---|---|---|---|---|---|---|---|
| nothing | 5 | 1 | 1,731,668 | 18 | 198,601 | 526,964 | 71,007 | 39.7 / 52.7 / 14.2 |
| nothing | 7 | 8 | 1,785,377 | 20 | 207,855 | 531,020 | 71,009 | 41.6 / 53.1 / 14.2 |
| both sides converted (2 mulfrac runs) | 5 | 1 | 1,748,509 | 19 | 199,161 | 527,324 | 72,358 | 39.8 / 52.7 / 14.5 |
| both sides converted | 7 | 8 | 1,802,341 | 21 | 208,515 | 531,380 | 72,366 | 41.7 / 53.1 / 14.5 |
| cap binding, wc < W (3 runs) | 5 | 1 | 1,755,191 | 19 | 199,264 | 527,336 | 71,007 | 39.9 / 52.7 / 14.2 |
| **cap binding** | **7** | **8** | **1,808,641** | **21** | **208,518** | **531,392** | 71,009 | **41.7 / 53.1** / 14.2 |
| refund by price + cap binding (3 runs) | 5 | 1 | 1,751,449 | 19 | 199,261 | 527,324 | 71,007 | 39.9 / 52.7 / 14.2 |
| refund by price + cap binding | 7 | 8 | 1,805,238 | 21 | 208,515 | 531,380 | 71,009 | 41.7 / 53.1 / 14.2 |

Worst: compute 208,518 (41.7 %), transient 531,392 (53.1 %), storage 72,366 (14.5 %). Against M4-e (34.0 / 43.7 / 14.2 %) almost all of the increase is the larger drawer and vault redeems pushed by every draw; the conversion's run time adds at most about 24,000 drawer units (about 700 compute grams), the record arguments 3–6 sigscript bytes. A refund before the mint guard skips mulfrac and is cheaper than the refund by price measured here.

**M5-c: `draw()` storage with the record at R** (D78a; W = 1 share at the smallest V', so wc = 1 and out_c = 0; method as M3-d). Supremum = 11 × 4C/R + (1 + a) × C/R + C/f_min + C/caller_min:

| f_min | Measured worst (a = 7) | Supremum | M4 (no record) |
|---|---|---|---|
| F_MIN = 1 KAS | 317,481 (63.5 %) | **327,684 (65.5 %)** | 304,016 (60.8 %) |
| F_MIN_FLOOR = 0.1 KAS | 407,481 (81.5 %) | **417,684 (83.5 %)** | 394,016 (78.8 %) |

R = 1.69 KAS still covers `draw()`; `close()` (M2-c) still sets R.

**M5-d: K = 16 `close()` with the M5 marker** (D78b; same shape as M4-f):

| K | Controller units | Compute | Transient | Storage | % c / t / s |
|---|---|---|---|---|---|
| 4 | 1,037,088 | 103,411 | 315,044 | 118,411 | 20.7 / 31.5 / 23.7 |
| 16 | 3,379,799 | 294,619 | **867,716** | 402,301 | 58.9 / **86.8** / 80.5 |

Identical to M4-f: only the marker's DRAWER_TPL bytes changed, not its size. Headroom below 90 % is still 32,284 grams, the tightest of any shape.

**M5-e: request entries, `settle()` and `refresh()`** (one funding input, change; covenant outputs at R):

| Transaction | Leader units / sigscript B | Leg units | Outputs | Compute | Transient | Storage | % c / t / s |
|---|---|---|---|---|---|---|---|
| `request_deposit` | 177,858 / 18,849 | — | 3 | 23,143 | 77,372 | 24,886 | 4.6 / 7.7 / 5.0 |
| `request_withdraw` whole (Schnorr owner) | 187,066 / 18,822 | 26,392 | 3 | 24,676 | 82,304 | 24,678 | 4.9 / 8.2 / 4.9 |
| `request_withdraw` partial (Schnorr owner) | 199,389 / 18,822 | 26,392 | 4 | 25,233 | 82,652 | 48,554 | 5.0 / 8.3 / 9.7 |
| `request_withdraw` partial (ECDSA owner) | 199,326 / 18,822 | 26,392 | 4 | 25,233 | 82,652 | 48,554 | 5.0 / 8.3 / 9.7 |
| `request_withdraw` partial (P2SH owner) | 199,255 / 18,822 | 26,392 | 4 | 25,268 | 82,792 | 48,554 | 5.1 / 8.3 / 9.7 |
| `settle` deposit-convert | 67,258 / 16,633 | 111,202 | 3 | 22,147 | 73,388 | 20,180 | 4.4 / 7.3 / 4.0 |
| `settle` deposit-convert, drained | 52,984 / 16,633 | 111,202 | 3 | 22,002 | 73,248 | 11,915 | 4.4 / 7.3 / 2.4 |
| `settle` deposit-refund | 56,138 / 16,633 | 111,202 | 3 | 22,002 | 73,248 | 0 | 4.4 / 7.3 / 0.0 |
| `settle` deposit-refund, drained | 42,418 / 16,633 | 111,202 | 3 | 21,857 | 73,108 | 0 | 4.4 / 7.3 / 0.0 |
| `settle` withdrawal FINAL | 59,817 / 16,633 | 111,202 | 3 | 22,002 | 73,248 | 0 | 4.4 / 7.3 / 0.0 |
| `settle` withdrawal FINAL, drained | 42,443 / 16,633 | 111,202 | 3 | 21,857 | 73,108 | 0 | 4.4 / 7.3 / 0.0 |
| `settle` withdrawal PAY | 121,822 / 16,633 | 111,202 | 4 | 23,159 | 73,596 | 21,294 | 4.6 / 7.4 / 4.3 |
| `settle` withdrawal PAY, drained | 104,957 / 16,633 | 111,202 | 4 | 22,914 | 73,456 | 22,456 | 4.6 / 7.3 / 4.5 |
| `settle` withdrawal ACCRUE | 121,314 / 16,633 | 111,202 | 3 | 22,747 | 73,388 | 27,869 | 4.5 / 7.3 / 5.6 |
| `settle` withdrawal ACCRUE, drained | 104,470 / 16,633 | 111,202 | 3 | 22,502 | 73,248 | 11,300 | 4.5 / 7.3 / 2.3 |
| `refresh` position | 8,044 / 1,202 | — | 2 | 3,339 | 6,436 | 16,980 | 0.7 / 0.6 / 3.4 |
| `refresh` request | 93,809 / 15,487 | — | 2 | 18,524 | 63,576 | 16,980 | 3.7 / 6.4 / 3.4 |
| `refresh` record | 8,651 / 1,162 | — | 2 | 3,299 | 6,276 | 115 | 0.7 / 0.6 / 0.0 |

Leg units are the position's `vault_leg` (withdrawals) and the record's `request_leg` (settlements; it reads the request, the largest share of a settle's compute). The age-window checks cost +465 leader units per request and +64 B of vault redeem; the drained-record check +6 B of request redeem and 11–37 units per settle. The position and record refreshes stay under the free per-input allowance of 9,999 units, so their committed compute budget is 0 (spec 5 Corrections, VA15). Every transaction is ≤ 9.7 % of every mass. The priority fee at 100 sompi per gram is about 0.039–0.048 KAS for a request and 0.037–0.042 KAS for a settlement (transient / 2 dominates; testnet-10 below).

**M5-f: testnet-10** (`docs/tn10-mass-check.md` M5): `probe vault-m5` with the real scripts was accepted end to end; its masses equal M5-e (request_deposit 23,143 / 77,372; request_withdraw partial 25,233 / 82,652; settle deposit-convert 22,147 / 73,388; settle FINAL drained 21,857 / 73,108) and the converting `draw()` (K = 16, a = 0, np = 4, record) measured 142,504 / 449,056 / 148,708.

## M6: K shards and limits (final constants; real scripts)

**K = 16, R = 1.69 KAS and m_max = 50 are final per M7** (the M7 section below re-runs the D83 rows with the final `draw()` and `close()`); the "provisional" marks in this section are the M6 record.

Source: `crates/lottery/tests/m6_measure.rs` (`cargo test -p lottery --release --test m6_measure -- --nocapture --test-threads=1`), all eleven templates at the M6 constants, 2026-10-13. Constants: K = 16 *(provisional, D83)*, B_MAX = 4, m_max = M_DRAW = M_CLAIM = 50 *(provisional, D83)*, R = 1.69 KAS *(provisional, D83)*, d = D_FLOOR = 0.13 KAS, M_REF = 0.1 KAS, P_MIN = 0.05 KAS, F_MIN = 1 KAS, F_MIN_FLOOR = 0.1 KAS, MIN_DEPOSIT = W_MIN_VALUE = 10 KAS, SHARE_UNIT = 1, V0_MIN = 1 KAS. Masses from the v2.1.0 calculator with mainnet parameters (`harness::masses`). Percentages are of compute 500,000 / transient 1,000,000 / storage 500,000 (`refs/rusty-kaspa-v2.1.0/consensus/core/src/config/params.rs:626`). Relay floor fee = 100 sompi per gram of max(compute, ⌈transient / 2⌉) (`lottery::buy::relay_floor_fee`; `mining/src/mempool/check_transaction_standard.rs:71-76, 92-100`); priority fee = 100 sompi per gram of the priority mass (`harness::priority_fee`). Every row is measured unless marked "estimate".

**Stop rule (D90) and genesis (user decision U-M6-1).** Every non-genesis shape is asserted ≤ 90 % of each mass, and the committed (K, B_MAX, m_max) is asserted within 90 % for every transition; all pass. Genesis is **exempt from the 90 % margin** (its masses depend only on deployer-chosen output values; a genesis over a limit only means "cannot deploy"). It is asserted ≤ 100 %, and the genesis builder now refuses a genesis whose exact compute, transient or storage mass exceeds 100 % of the mainnet limits (`lottery::genesis::genesis_mass_check`, called by `GenesisCase::try_build_with` / `build_with`; per-transaction limits in the mempool at `mining/src/mempool/check_transaction_limits.rs:19-26, 37-41`, per block at `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:88-96`).

**M6-a / M6-i: redeem sizes.**

| Object | Prefix B | Suffix B | State B | Redeem B |
|---|---|---|---|---|
| shard (`buy`, `close_leg`) | 1 | 11,244 | 116 | 11,361 |
| ticket (`claim`, two returns, `refresh`) | 1 | 25,359 | 92 | 25,452 |
| controller (`close`, `draw_leg`) | 1 | 27,266 | 427 | 27,694 |
| params (`set_params`, `close_leg`) | 1 | 2,808 | 109 | 2,918 |
| marker (`draw_leg`, `refresh`) | 1 | 1,388 | 715 | 2,104 |
| drawer (`draw_test_seed`, TEST-ONLY seed) | 1 | 65,096 | 137 | 65,234 |
| **vault** (`draw_leg`, `request_deposit`, `request_withdraw` with W4b incl. the RV-1 remainder rule; M5: 3,380) | 1 | 3,276 | 155 | **3,432** |
| pool (`absorb_leg`, `ticket_leg`, `refresh`) | 1 | 1,831 | 649 | 2,481 |
| record (`request_leg`, `refresh`) | 1 | 1,026 | 127 | 1,154 |
| position (`vault_leg`, `refresh`) | 1 | 1,137 | 56 | 1,194 |
| request (`settle`, `refresh`) | 1 | 15,420 | 58 | 15,479 |

Only the vault changed size (+52 B, check W4b: +42 B for wmin, then +10 B for the remainder rule after review RV-1, O-M6-2). MIN_DEPOSIT 10⁸ → 10⁹ is a 4-byte script number either way, so the drawer and request sizes are unchanged.

**M6-a / M6-h / M6-i: every transition** (one funding input unless stated; "leg" = the units of the named delegate input):

| Transition | Input 0 units / sigscript B | Leg units | Outputs | Compute | Transient | Storage | % c / t / s | Relay floor (KAS) | Priority fee (KAS) |
|---|---|---|---|---|---|---|---|---|---|
| genesis K = 16, fixture (V0 = 10,000,000 KAS, acc 2 KAS, large change) — **exempt (U-M6-1)** | 100,000 / 66 | — | 22 | 11,223 | 8,372 | 460,532 | 2.2 / 0.8 / **92.1** | 0.01122 | 0.46053 |
| genesis worst under `validate()` + U-M6-1, no change — **exempt** | 100,000 / 66 | — | 21 | 10,811 | 8,164 | 487,956 | 2.2 / 0.8 / 97.6 | 0.01081 | 0.48796 |
| genesis worst, change 1,000,000 KAS — **exempt** | 100,000 / 66 | — | 22 | 11,223 | 8,372 | 488,229 | 2.2 / 0.8 / **97.6** | 0.01122 | 0.48823 |
| genesis worst, smallest change the builder accepts (83,063,378 sompi = 0.8306 KAS; 83,063,377 refused) — **exempt** | 100,000 / 66 | — | 22 | 11,223 | 8,372 | 500,000 | 2.2 / 0.8 / 100.0 | 0.01122 | 0.50000 |
| `buy()` b = 1, no referral, large change | 354,091 / 37,378 | — | 3 | 43,472 | 151,488 | 321,777 | 8.7 / 15.1 / 64.4 | 0.07574 | 0.32178 |
| `buy()` b = 1, no referral, change 1 KAS | 354,091 / 37,378 | — | 3 | 43,472 | 151,488 | 311,729 | 8.7 / 15.1 / 62.3 | 0.07574 | 0.31173 |
| `buy()` b = 1, referral 0.1 KAS, large change | 354,375 / 37,378 | — | 4 | 43,884 | 151,696 | **422,352** | 8.8 / 15.2 / **84.5** | 0.07585 | 0.42235 |
| `buy()` b = 1, referral 0.1 KAS, change 1 KAS | 354,375 / 37,378 | — | 4 | 43,884 | 151,696 | 412,304 | 8.8 / 15.2 / 82.5 | 0.07585 | 0.41230 |
| `buy()` b = 4, no referral, large change | 439,229 / 39,185 | — | 3 | 46,079 | 158,716 | 83,172 | 9.2 / 15.9 / 16.6 | 0.07936 | 0.08317 |
| `buy()` b = 4, no referral, change 1 KAS | 439,229 / 39,185 | — | 3 | 46,079 | 158,716 | 82,279 | 9.2 / 15.9 / 16.5 | 0.07936 | 0.08228 |
| `buy()` b = 4, referral 0.4 KAS, large change | 439,513 / 39,185 | — | 4 | 46,491 | 158,924 | 108,704 | 9.3 / 15.9 / 21.7 | 0.07946 | 0.10870 |
| `buy()` b = 4, referral 0.4 KAS, change 1 KAS | 439,513 / 39,185 | — | 4 | 46,491 | 158,924 | 107,811 | 9.3 / 15.9 / 21.6 | 0.07946 | 0.10781 |
| `set_params()`, change | 120,376 / 3,067 | — | 2 | 6,404 | 13,896 | 16,980 | 1.3 / 1.4 / 3.4 | 0.00695 | 0.01698 |
| `set_params()`, no change | 120,310 / 3,067 | — | 1 | 5,992 | 13,688 | 0 | 1.2 / 1.4 / 0.0 | 0.00684 | 0.00684 |
| `close()` K = 16, sales in every shard (M2-a shape) | 3,379,799 / 29,140 | — | 20 | 294,619 | **867,716** | 402,301 | 58.9 / **86.8** / 80.5 | 0.43386 | 0.43386 |
| `close()` K = 16, storage worst case (M2-c grid: zero sales, O = J_boost = A = 0, caller 1,000,000 KAS) | 3,378,742 / 29,140 | — | 20 | 294,619 | 867,716 | **449,692** | 58.9 / 86.8 / **89.9** | 0.43386 | 0.44969 |
| `draw()` worst (DP21: m = 50, a = 7, np = 7, 8 funding inputs, cap binding, record) | 1,810,876 / 79,050 | 463,933 (vault) | 21 | 208,671 | 531,604 | 71,009 | 41.7 / 53.2 / 14.2 | 0.26580 | 0.26580 |
| `draw()` typical (m = 10, a = 0, np = 3, no record) | 1,507,674 / 77,763 | 463,933 (vault) | 9 | 141,239 | 448,556 | 93,028 | 28.2 / 44.9 / 18.6 | 0.22428 | 0.22428 |
| `claim()` PAY, K = 16, m = 50, h = K | 700,682 / 36,287 | 183,651 (pool) | 4 | 50,640 | 157,520 | 247,222 | 10.1 / 15.8 / 49.4 | 0.07876 | 0.24722 |
| `claim()` ACCRUE, K = 16, m = 50, h = K | 700,178 / 36,287 | 183,651 | 3 | 50,228 | 157,312 | 150,420 | 10.0 / 15.7 / 30.1 | 0.07866 | 0.15042 |
| `claim()` FINAL, K = 16, m = 50, h = K | 597,960 / 36,287 | 183,651 | 3 | 49,083 | 157,172 | 21,153 | 9.8 / 15.7 / 4.2 | 0.07859 | 0.07859 |
| `claim()` PAY, K = 16, m = 50, 8 funding inputs | 701,172 / 36,287 | 183,651 | 4 | 58,480 | 160,880 | 149,925 | 11.7 / 16.1 / 30.0 | 0.08044 | 0.14993 |
| `claim()` PAY at P = P_MIN, storage worst case (b = 2, pool' at R, change 1,000,000 KAS) | 288,417 / 27,854 | 183,627 | 4 | 38,007 | 123,788 | 377,514 | 7.6 / 12.4 / 75.5 | 0.06189 | 0.37751 |
| `return_deposit_after` (b = 2 ticket) | 51,819 / 25,460 | — | 2 | 28,052 | 103,328 | 30,702 | 5.6 / 10.3 / 6.1 | 0.05166 | 0.05166 |
| `return_deposit_with_pool` (b = 2 ticket, K = 1 pool) | 96,636 / 25,464 | 183,576 | 3 | 33,260 | 113,880 | 34,301 | 6.7 / 11.4 / 6.9 | 0.05694 | 0.05694 |
| `refresh` ticket | 153,991 / 25,460 | — | 2 | 29,097 | 103,468 | 146,087 | 5.8 / 10.3 / 29.2 | 0.05173 | 0.14609 |
| `refresh` pool (K = 4) | 22,647 / 2,489 | — | 2 | 4,826 | 11,584 | 7,869 | 1.0 / 1.2 / 1.6 | 0.00579 | 0.00787 |
| `refresh` marker (K = 4 and K = 16, identical) | 20,046 / 2,112 | — | 2 | 4,449 | 10,076 | 310 | 0.9 / 1.0 / 0.1 | 0.00504 | 0.00504 |
| `refresh` position | 8,044 / 1,202 | — | 2 | 3,339 | 6,436 | 16,980 | 0.7 / 0.6 / 3.4 | 0.00334 | 0.01698 |
| `refresh` request | 93,809 / 15,487 | — | 2 | 18,524 | 63,576 | 16,980 | 3.7 / 6.4 / 3.4 | 0.03179 | 0.03179 |
| `refresh` record | 8,651 / 1,162 | — | 2 | 3,299 | 6,276 | 115 | 0.7 / 0.6 / 0.0 | 0.00330 | 0.00330 |
| `request_deposit` (D = 10 KAS = MIN_DEPOSIT) | 178,170 / 18,901 | — | 3 | 23,195 | 77,580 | 24,886 | 4.6 / 7.8 / 5.0 | 0.03879 | 0.03879 |
| `request_withdraw` whole (Schnorr owner) | 187,456 / 18,876 | 26,756 (position) | 3 | 24,730 | 82,520 | 24,678 | 4.9 / 8.3 / 4.9 | 0.04126 | 0.04126 |
| `request_withdraw` partial (Schnorr owner) | 199,819 / 18,876 | 26,756 | 4 | 25,287 | 82,868 | 48,554 | 5.1 / 8.3 / 9.7 | 0.04143 | 0.04855 |
| `request_withdraw` partial (ECDSA owner) | 199,756 / 18,876 | 26,756 | 4 | 25,287 | 82,868 | 48,554 | 5.1 / 8.3 / 9.7 | 0.04143 | 0.04855 |
| `request_withdraw` partial (P2SH owner) | 199,685 / 18,876 | 26,756 | 4 | 25,322 | 83,008 | 48,554 | 5.1 / 8.3 / 9.7 | 0.04150 | 0.04855 |
| **M6-i** `request_withdraw` partial at the W4b boundary, w = wmin = 1,000,000,000 (Schnorr owner) | 199,805 / 18,875 | 26,756 | 4 | 25,286 | 82,864 | 48,554 | 5.1 / 8.3 / 9.7 | 0.04143 | 0.04855 |
| `settle` deposit-convert | 67,258 / 16,633 | 111,202 (record) | 3 | 22,147 | 73,388 | 20,180 | 4.4 / 7.3 / 4.0 | 0.03669 | 0.03669 |
| `settle` deposit-convert, drained | 52,984 / 16,633 | 111,202 | 3 | 22,002 | 73,248 | 11,915 | 4.4 / 7.3 / 2.4 | 0.03662 | 0.03662 |
| `settle` deposit-refund | 56,138 / 16,633 | 111,202 | 3 | 22,002 | 73,248 | 0 | 4.4 / 7.3 / 0.0 | 0.03662 | 0.03662 |
| `settle` deposit-refund, drained | 42,418 / 16,633 | 111,202 | 3 | 21,857 | 73,108 | 0 | 4.4 / 7.3 / 0.0 | 0.03655 | 0.03655 |
| `settle` withdrawal FINAL | 59,817 / 16,633 | 111,202 | 3 | 22,002 | 73,248 | 0 | 4.4 / 7.3 / 0.0 | 0.03662 | 0.03662 |
| `settle` withdrawal FINAL, drained | 42,443 / 16,633 | 111,202 | 3 | 21,857 | 73,108 | 0 | 4.4 / 7.3 / 0.0 | 0.03655 | 0.03655 |
| `settle` withdrawal PAY | 121,822 / 16,633 | 111,202 | 4 | 23,159 | 73,596 | 21,294 | 4.6 / 7.4 / 4.3 | 0.03680 | 0.03680 |
| `settle` withdrawal PAY, drained | 104,957 / 16,633 | 111,202 | 4 | 22,914 | 73,456 | 22,456 | 4.6 / 7.3 / 4.5 | 0.03673 | 0.03673 |
| `settle` withdrawal ACCRUE | 121,314 / 16,633 | 111,202 | 3 | 22,747 | 73,388 | 27,869 | 4.5 / 7.3 / 5.6 | 0.03669 | 0.03669 |
| `settle` withdrawal ACCRUE, drained | 104,470 / 16,633 | 111,202 | 3 | 22,502 | 73,248 | 11,300 | 4.5 / 7.3 / 2.3 | 0.03662 | 0.03662 |

**Tightest margins (genesis excluded, U-M6-1):** compute 58.9 % and transient **86.8 %** (`close()` K = 16, M2-a shape; 32,284 grams of transient headroom, unchanged since M4-f), storage **89.9 %** (`close()` K = 16 storage worst case), then 84.5 % (`buy()` b = 1 with a 0.1 KAS referral at d = 0.13). Against M5: the vault's +52 B adds 52 compute and 208 transient grams to every request (+4 grams per byte) and +153 / +212 to the worst `draw()`; request leader units rose by 312 (`request_deposit`, the longer vault redeem) to 430 (partial `request_withdraw`, the redeem plus W4b's two divisions and two comparisons), and the position leg by 364; storage of every `buy()`, `claim()`, return and ticket refresh fell with d = 0.11 → 0.13 KAS (for example PAY at P_MIN 405,486 → 377,514). Every other row equals M4/M5.

**Genesis (U-M6-1).** Genesis has K + 4 covenant outputs at R (controller at acc + pot + R, params, K shards, drawer, position) plus the vault at V0 + R and the change. The worst instance `validate()` allows (K = 16, acc = pot = 0, J = 0 so that V0 = V0_MIN passes the cap check, V0 = V0_MIN, R = R_MIN, fee = its relay floor 1,122,300 sompi) measures **488,229 (97.6 %)** with large change and 487,956 with no change. A change output adds C / change: the builder accepts at least 83,063,378 sompi (0.8306 KAS) of change at this instance, which gives exactly 500,000; one sompi less is refused with a "U-M6-1" error. The fixture genesis (V0 = 10,000,000 KAS, acc 2 KAS) is 460,532 (92.1 %). M3's genesis (no position output) was 437,512 on testnet-10.

**Minimums (M6).**

| Quantity | Value | Condition |
|---|---|---|
| Ticket deposit d | **0.13 KAS (13,000,000 sompi) = D_FLOOR**, genesis refuses d < D_FLOOR (D80, D81) | b = 1 `buy()` with a referral output at M_REF ≤ 90 % of storage: 422,352 (84.5 %) at d = 0.13; the measured floor is 0.1193 KAS (M6-e) |
| M_REF, smallest referral output | 0.1 KAS (shard) | bounds the referral output's storage term at C / M_REF = 100,000 grams (plain output, p = 1; M1); the M6-e shape puts the referral at exactly M_REF |
| F_MIN_FLOOR / default F_MIN | 0.1 KAS / 1 KAS | `draw()` storage supremum with the record 83.5 % / 65.5 % (M6-g) |
| P_MIN, smallest PAY payout | **0.05 KAS (unchanged, D82)** | measured floor 3,669,900 sompi (0.036699 KAS) at the worst change, rounded up to 0.04 KAS; P_MIN = 0.05 gives 377,514 (75.5 %) (M6-f) |
| MIN_DEPOSIT / W_MIN_VALUE | 10 KAS / 10 KAS (D85, D86) | vault D5, drawer C1, request settle range; vault W4b |
| V0_MIN, SHARE_UNIT | 1 KAS, 1 sompi (D84) | genesis only |
| R | 1.69 KAS *(provisional, D83)* | M6-g |

**M6-b: K frontier for `close()`** *(K provisional, D83).* In-memory K_MAX variants (controller K_MAX line and every `byte[512]` roots array → `byte[32·K_MAX]` in controller, marker, drawer, pool and ticket; D91), `close()` at K = K_MAX with the M2-a shape (1,000 KAS sales and 50 KAS referral per shard, distinct roots). The K = 16 variant is hash-identical to the committed templates (asserted). The run stops at the first K whose transient exceeds 100 %.

| K_MAX = K | Controller redeem B | Controller units / sigscript B | Compute | Transient | Storage at R = 1.69 | % c / t / s | R(K) = 4C(K + 3)/(0.9 L), 0.01 KAS up | Storage at R(K) |
|---|---|---|---|---|---|---|---|---|
| **16** | 27,694 | 3,379,799 / 29,140 | 294,619 | 867,716 | 402,301 | 58.9 / **86.8** / 80.5 | 1.69 | 402,301 (80.5 %) |
| 17 | 29,035 | 3,583,953 / 30,481 | 313,744 | 919,136 | 425,961 | 62.7 / **91.9** / 85.2 | 1.78 | 404,415 (80.9 %) |
| 18 | 30,371 | 3,788,141 / 31,817 | 333,064 | 970,536 | 449,622 | 66.6 / 97.1 / 89.9 | 1.87 | 406,340 (81.3 %) |
| 19 | 31,707 | 3,992,396 / 33,153 | 352,684 | 1,021,936 | 473,282 | 70.5 / **102.2** / 94.7 | 1.96 | 408,082 (81.6 %) |

| Limit | Largest K within 90 % | Largest K within 100 % |
|---|---|---|
| transient (binding) | **16** | **18** |
| compute | ≥ 19 (largest measured) | ≥ 19 |
| storage, this shape at R = 1.69 | 18 | 19 |
| storage, worst case | — (R is set from K: (K + 3)·4C/R(K) ≤ 90 % by construction; R(K) above) | — |

Each shard adds about 51,400 transient grams (the shard redeem pushed once, plus about 1,336 B of controller and 32 B of marker roots per K_MAX). Transient sets the ceiling: **K = 16 is the largest K within 90 %, K = 18 the largest within 100 %**.

`draw()` and `claim()` at K_MAX variants — **estimates** (transient only, 4 grams per byte, `refs/rusty-kaspa-v2.1.0/consensus/core/src/constants.rs:31`; extra bytes = Δdrawer + Δcontroller + Δmarker code + 32ΔK + A_MAX·(Δpool code + 32ΔK) + Δpool code + ΔK·(576 + 2m) for `draw()`; Δticket + Δpool code + 32ΔK + ΔK·(576 + 2m) for `claim()`; compute not estimated):

| K_MAX | Drawer code B (Δ) | Ticket code B (Δ) | `draw()` extra B | Worst `draw()` transient (estimate) | % | `claim()` PAY F_MAX extra B | `claim()` transient (estimate) | % |
|---|---|---|---|---|---|---|---|---|
| 16 | 65,097 (+0) | 25,360 (+0) | 0 | 531,604 (measured) | 53.2 | 0 | 160,880 (measured) | 16.1 |
| 17 | 66,335 (+1,238) | 26,581 (+1,221) | 3,511 | 545,648 (estimate) | 54.6 | 1,929 | 168,596 (estimate) | 16.9 |
| 18 | 67,571 (+2,474) | 27,799 (+2,439) | 7,015 | 559,664 (estimate) | 56.0 | 3,855 | 176,300 (estimate) | 17.6 |
| 19 | 68,807 (+3,710) | 29,017 (+3,657) | 10,519 | 573,680 (estimate) | 57.4 | 5,781 | 184,004 (estimate) | 18.4 |

`draw()` and `claim()` are far from binding K; `close()` transient is.

**M6-c: B_MAX frontier at K = 16** (shard, ticket and pool resized, `Templates::get(b)`):

| B_MAX | Shard redeem B | Ticket redeem B | `close()` K = 16: compute | transient | storage | % c / t / s | `buy()` b = B_MAX, referral, large change: compute | transient | storage | % c / t / s |
|---|---|---|---|---|---|---|---|---|---|---|
| 3 | 9,400 | 25,446 | 257,943 | 742,212 | 402,301 | 51.6 / 74.2 / 80.5 | 43,527 | 148,668 | 144,227 | 8.7 / 14.9 / 28.8 |
| **4** | 11,361 | 25,452 | 294,619 | 867,716 | 402,301 | 58.9 / **86.8** / 80.5 | 46,491 | 158,924 | 108,704 | 9.3 / 15.9 / 21.7 |
| 5 | 13,322 | 25,458 | 332,995 | 993,220 | 402,301 | 66.6 / **99.3** / 80.5 | 49,554 | 169,176 | 87,221 | 9.9 / 16.9 / 17.4 |

**B_MAX = 4 is final** (D83). B_MAX = 5 puts K = 16 `close()` at 99.3 % of transient (M2: 98.2 %; the controller and marker have grown since).

**M6-d: m frontier at K = 16** *(m_max provisional, D83).* `buy()` b = 4 with referral; worst `draw()` (a = 7, np = 7, 8 funding inputs, cap binding, record); worst `claim()` (h = K).

| Transition | m | Compute | Transient | Storage | % c / t / s |
|---|---|---|---|---|---|
| `buy()` b = 4 | 10 | 46,491 | 158,924 | 108,704 | 9.3 / 15.9 / 21.7 |
| `buy()` b = 4 | 25 | 46,711 | 159,404 | 108,704 | 9.3 / 15.9 / 21.7 |
| `buy()` b = 4 | **50** | 46,912 | 160,208 | 108,704 | 9.4 / 16.0 / 21.7 |
| `draw()`, drawer M_DRAW = 50 (committed) | 10 | 206,891 | 526,484 | 71,009 | 41.4 / 52.6 / 14.2 |
| `draw()`, drawer M_DRAW = 50 (committed) | 25 | 207,571 | 528,404 | 71,009 | 41.5 / 52.8 / 14.2 |
| `draw()`, drawer M_DRAW = 50 (committed) | **50** | 208,671 | 531,604 | 71,009 | 41.7 / 53.2 / 14.2 |
| `draw()`, drawer M_DRAW = 25 (variant) | 10 | 202,941 | 518,284 | 71,009 | 40.6 / 51.8 / 14.2 |
| `draw()`, drawer M_DRAW = 25 (variant) | 25 | 203,521 | 520,204 | 71,009 | 40.7 / 52.0 / 14.2 |
| `claim()` PAY, 8 funding inputs | 10 | 56,800 | 155,760 | 149,925 | 11.4 / 15.6 / 30.0 |
| `claim()` PAY, 8 funding inputs | 25 | 57,480 | 157,680 | 149,925 | 11.5 / 15.8 / 30.0 |
| `claim()` PAY, 8 funding inputs | **50** | 58,480 | 160,880 | 149,925 | 11.7 / 16.1 / 30.0 |
| `claim()` ACCRUE | 10 | 48,548 | 152,192 | 150,420 | 9.7 / 15.2 / 30.1 |
| `claim()` ACCRUE | 25 | 49,128 | 154,112 | 150,420 | 9.8 / 15.4 / 30.1 |
| `claim()` ACCRUE | **50** | 50,228 | 157,312 | 150,420 | 10.0 / 15.7 / 30.1 |
| `claim()` FINAL | 10 | 47,503 | 152,052 | 21,153 | 9.5 / 15.2 / 4.2 |
| `claim()` FINAL | 25 | 48,083 | 153,972 | 21,153 | 9.6 / 15.4 / 4.2 |
| `claim()` FINAL | **50** | 49,083 | 157,172 | 21,153 | 9.8 / 15.7 / 4.2 |

m = 50 is within 90 % for every transition (asserted). Drawer code: 63,047 B at M_DRAW = 25, 65,097 B at M_DRAW = 50.

**M6-e: d frontier** (fresh shard at R = 1.69 KAS, worst of large and 1 KAS change; with a referral, the price is set so that the referral output is exactly M_REF = 0.1 KAS for every b (price ⌈1 KAS / b⌉, s_R = 10 %), the largest referral storage term; M-c/M2 used 0.1·b KAS at price 1, so the b ≥ 2 referral columns are stricter than M2's):

| b | No referral: d at 90 % | d at 100 % | Referral at M_REF: d at 90 % | d at 100 % |
|---|---|---|---|---|
| 1 | 9,176,080 (0.0918 KAS) | 8,231,876 (0.0823) | **11,928,157 (0.1193)** | 10,380,417 (0.1038) |
| 2 | 4,546,034 (0.0455) | 4,082,100 (0.0408) | 5,964,079 (0.0596) | 5,190,209 (0.0519) |
| 3 | 3,014,852 (0.0301) | 2,708,624 (0.0271) | 3,976,053 (0.0398) | 3,460,139 (0.0346) |
| 4 | 2,253,506 (0.0225) | 2,025,305 (0.0203) | 2,982,040 (0.0298) | 2,595,105 (0.0260) |

The D80 shape, `buy()` b = 1 with a referral at M_REF (scripts run):

| d (KAS) | Change | Compute | Transient | Storage | % of limit |
|---|---|---|---|---|---|
| 0.11 | large | 43,884 | 151,696 | 478,296 | 95.7 |
| 0.11 | 1 KAS | 43,884 | 151,696 | 468,152 | 93.6 |
| 0.12 | large | 43,884 | 151,696 | 447,993 | 89.6 |
| 0.12 | 1 KAS | 43,884 | 151,696 | 437,897 | 87.6 |
| **0.13** | large | 43,884 | 151,696 | **422,352** | **84.5** |
| 0.13 | 1 KAS | 43,884 | 151,696 | 412,304 | 82.5 |

D80's predictions (447,993 at d = 0.12, 422,352 at d = 0.13) are measured exactly. **d = 0.13 KAS is final; U2 is closed.**

**M6-f: P_MIN at d = 0.13.** Worst PAY: b = 2, d = D_FLOOR (ticket' 0.26 KAS), pool' at R, one funding input. Storage by payout P:

| Change | P = 0.03 KAS | 0.0367 | 0.04 | 0.05 (P_MIN) | 0.1 | Smallest P ≤ 90 % (sompi) | Rounded up to 0.01 KAS |
|---|---|---|---|---|---|---|---|
| 1 KAS | 439,682 | 379,003 | 356,609 | 306,869 | 208,149 | 2,909,709 (0.029097 KAS) | 0.03 KAS |
| 1,000 KAS | 510,612 | 449,758 | 427,279 | 377,279 | 277,279 | 3,666,738 (0.036667 KAS) | 0.04 KAS |
| 1,000,000 KAS | 510,847 | 449,993 | 427,514 | **377,514** | 277,514 | **3,669,900 (0.036699 KAS)** | 0.04 KAS |

The floor is 0.0367 KAS as D82 predicted (rounded up: 0.04 KAS). **P_MIN = 0.05 KAS is kept** (stricter; lowering it would change the ticket template): 377,514 (75.5 %).

**M6-g: R re-check** *(R provisional, D83)* at R = 1.69 KAS, K = 16, d = 0.13 KAS:

| Transition | Measured worst | % | Supremum | % |
|---|---|---|---|---|
| `close()` (M2-c grid; worst: zero sales, O = J_boost = A = 0, caller 1,000,000 KAS) | 449,692 | 89.9 | 449,692 = (K + 3)·4C/R | **89.9** |
| `draw()` with the record (M5-c method), f_min = F_MIN = 1 KAS | 317,481 | 63.5 | 327,684 = 11·4C/R + (1 + a)·C/R + C/f_min + C/caller | 65.5 |
| `draw()` with the record, f_min = F_MIN_FLOOR = 0.1 KAS | 407,481 | 81.5 | 417,684 (same formula) | 83.5 |
| genesis — **exempt (U-M6-1)** | 488,229 (worst `validate()` instance, large change) | 97.6 | (K + 4)·4C/R + 4C/(V0 + R) + C/change | — |

**Binding transition: `close()`.** R = 4C(K + 3)/(0.9 L) = 168,888,889 sompi at K = 16, rounded up to **1.69 KAS**; `draw()` alone needs R ≥ 152,941,177 sompi (1.529 KAS) at f_min = F_MIN_FLOOR. Genesis is a third storage transition whose fresh outputs sit at R (K + 4 of them, one more than `close()`), so at R = 1.69 its worst instance exceeds 90 %; it is exempt from the margin by U-M6-1 and bounded at 100 % by the builder check, so it does not set R.

**M6-h: fees.** Claims against prizes (relay floor and priority fee as above):

| Claim shape | Priority mass | Relay floor (KAS) | Priority fee (KAS) | Priority fee % of P_MIN | % of 0.1 KAS | % of 1 KAS | Relay floor % of P_MIN |
|---|---|---|---|---|---|---|---|
| PAY, K = 16, m = 50, h = K | 247,222 | 0.07876 | 0.24722 | 494.4 | 247.2 | 24.72 | 157.5 |
| ACCRUE, K = 16, m = 50, h = K | 150,420 | 0.07866 | 0.15042 | 300.8 | 150.4 | 15.04 | 157.3 |
| FINAL, K = 16, m = 50, h = K | 78,586 | 0.07859 | 0.07859 | 157.2 | 78.6 | 7.86 | 157.2 |
| PAY at P = P_MIN, storage worst case (b = 2, pool' at R, change 1,000,000 KAS) | 377,514 | 0.06189 | **0.37751** | **755.0** | 377.5 | 37.75 | 123.8 |
| PAY at P = P_MIN, change 1 KAS | 306,869 | 0.06189 | 0.30687 | 613.7 | 306.9 | 30.69 | 123.8 |

Under load a PAY claim at P_MIN costs about 7.55 × P_MIN at the priority floor; even the relay floor (no load) is 1.24–1.58 × P_MIN, because the K = 16 proofs dominate transient mass. Documented, not tuned (D82; closes the M4-h OPEN row as documented).

Round sales S at which C_close = s_C·S and C_draw = s_D·S (s_C = s_D = 50 bp) cover the transaction's fee:

| Transaction | Priority mass | Priority fee (KAS) | S covering it (KAS) | Relay floor (KAS) | S covering the relay floor (KAS) |
|---|---|---|---|---|---|
| `close()` K = 16 (M2-a shape) | 433,858 | 0.43386 | 86.77 | 0.43386 | 86.77 |
| `close()` K = 16, storage worst case | 449,692 | 0.44969 | 89.94 | 0.43386 | 86.77 |
| `draw()` worst (M6-a) | 265,802 | 0.26580 | 53.16 | 0.26580 | 53.16 |
| `draw()` typical (M6-a) | 224,278 | 0.22428 | 44.86 | 0.22428 | 44.86 |

The `draw()` storage supremum with the record at F_MIN_FLOOR (417,684, M6-g) means a priority fee of at most 0.41768 KAS when storage dominates; C_draw covers it from S = 83.54 KAS. Below these sales the caller pays part of the fee (D19 liveness note).

**M6-i: vault request sizes.** Vault redeem 3,432 B (+52 B for W4b vs M5: +42 B for wmin, +10 B for the remainder rule after review RV-1, O-M6-2). `request_withdraw` partial at the W4b boundary (w = wmin = 10⁹ at the fixture vault, remainder 4.9 × 10¹⁰ ≥ wmin): leader 199,805 units, sigscript 18,875 B, position leg 26,756 units; 25,286 / 82,864 / 48,554 (5.1 / 8.3 / 9.7 %), the same masses as any partial withdrawal (rows above).

**M6-j: drawer sensitivity and byte budget, for the seed milestone.** Drawer-only variants run as full worst draws (K = 16, a = 7, np = 7, 8 funding inputs, cap binding, record), with marker, vault and pool committing to the variant drawer's hash:

| Pair | m | Drawer redeem B (a / b) | Δ B | Compute (a / b) | Transient (a / b) | Δ compute per B | Δ transient per B |
|---|---|---|---|---|---|---|---|
| M_DRAW 25 vs 50 | 25 | 63,184 / 65,234 | 2,050 | 203,521 / 207,571 | 520,204 / 528,404 | 1.976 | 4.000 |
| K_MAX 16 vs 17 | 50 | 65,234 / 66,472 | 1,238 | 208,671 / 210,711 | 531,604 / 536,564 | 1.648 | 4.006 |

Worst `draw()` at the committed drawer (65,234 B): compute 208,671 (41.7 %), transient 531,604 (53.2 %). With the larger slope of each (1.976 compute and 4.006 transient grams per drawer byte), **the drawer can grow by about 91,950 B before the worst `draw()` reaches 90 % of transient** (122,154 B for compute); transient binds. **Stack-slot budget (not a mass):** the drawer has 4 free slots of the 244-item combined stack limit at check 24 (DECISIONS 2026-10-12 Fact S4-1; M6 changed only the drawer's MIN_DEPOSIT constant, so the drawer compiles unchanged). For the seed milestone the 4 stack slots are the scarcer drawer budget; the byte budget is large.

**A5a: which M6 measurements depend on the drawer or `close()`** (rows marked "re-run" must be repeated at the end of the real-seed milestone before K, R or m_max count as final):

| Measurement | Depends on drawer size | Depends on the `draw()` shape | Depends on `close()` size or shape | After the seed milestone |
|---|---|---|---|---|
| M6-a `draw()` rows (worst and typical) | **yes**: compute (leg reads) and transient (push) | yes | no | **re-run** |
| M6-a `close()` rows | no (the marker holds DRAWER_TPL as 32 B) | no | **yes** (R1 fix) | **re-run** if `close()` changes |
| M6-a genesis rows (exempt, U-M6-1) | no (storage counts outputs) | no | no; depends on the K + 4 outputs at R | re-run if genesis gains an output |
| M6-a `buy()`, `claim()`, returns, refreshes, vault requests, settlements, `set_params()` | no (hashes, not drawer bytes) | no | no | valid |
| M6-b K frontier for `close()` | no | no | **yes** | **re-run** if `close()` changes |
| M6-b `draw()` estimates at K_MAX variants | **yes** | yes | no | **re-run** |
| M6-c B_MAX frontier | no | no | **yes** | **re-run** if `close()` changes |
| M6-d `draw()` rows (M_DRAW variants) | **yes** | yes | no | **re-run** (sets m_max) |
| M6-d `buy()` and `claim()` rows | no | no | no | valid |
| M6-e d frontier, M6-f P_MIN | no | no | no | valid |
| M6-g R: `close()` storage worst case | no | no | **yes** (K + 3 outputs) | **re-run** if `close()` gains or loses an output |
| M6-g R: `draw()` storage supremum | no | **yes** (output count and values) | no | **re-run** if `draw()` gains an output |
| M6-h fees | follows M6-a | follows M6-a | follows M6-a | re-run the dependent rows |
| M6-j drawer sensitivity and byte budget | is the measurement | yes | no | the seed milestone's budget input |

**K = 16, R = 1.69 KAS and m_max = 50 are provisional (D83)** until the seed milestone re-confirms them with these rows; d = 0.13 KAS and B_MAX = 4 are final.

### Purchase conflict rate (M6, D88)

**τ measured on testnet-10** (`probe conflict-m6`, 2026-10-13, public node, real shard script; results in `docs/tn10-mass-check.md` M6): **τ_chain = 0.919 s** (median, n = 10: start of the parent buy's submission → a buy chained on its unaccepted successor shard accepted by the mempool; mean 0.920, max 0.925; that is two RPC round trips to the public node, the node accepted every chained buy on the first attempt) and **τ_merge = 1.655 s** (median, n = 10: start of the submission → a chain block accepted the buy; mean 1.650, max 1.962; the virtual chain was polled about every 0.6 s, so this is an upper bound by one poll). Source: `crates/lottery/tests/m6_conflict.rs` (`cargo test -p lottery --release --test m6_conflict -- --nocapture --test-threads=1`, `TAU_CHAIN` / `TAU_MERGE` = these medians), a seeded discrete-event model (no network), 2026-10-13.

Model: K shards; buys arrive as a Poisson process at rate λ and pick a shard uniformly (research F1). An accepted buy makes its shard busy for τ (τ_chain: until a chained buy on the unaccepted successor is accepted; τ_merge: until the successor is merged by a chain block). A buy that finds its shard busy conflicts (the mempool rejects it as a double spend; it does not restart the busy time) and retries after a rebuild delay ρ on a shard drawn uniformly from the other K − 1. Reported: first-attempt conflict rate, mean attempts, P(≥ 3 attempts), saturation throughput K/τ. With a = λτ/K:
- **Correction to spec A7 and A8 (S3b-2):** as ρ → ∞ each shard is a one-server loss system, so the first-attempt rate is **a / (1 + a)** (Erlang B with one server). The spec's 1 − e^(−a) holds only if every arrival, rejected ones included, restarts the busy time; under the protocol semantics it misses by more than 0.01 in 26 of 45 grid cells. Sanity grid (K ∈ {4, 8, 16}, λ ∈ {0.1, 0.5, 1, 2, 5}/s, τ ∈ {1, 2, 5} s): the simulation is within 0.00107 of a/(1 + a) (≤ 2.15 SE) and, with restart semantics, within 0.00136 of 1 − e^(−a).
- **At a finite ρ below saturation (S3b-5)** every buy eventually succeeds, so a fresh buy finds its shard busy with probability exactly **a** (shard utilisation), independent of ρ and the retry rule (max deviation 0.00337). This is the practical first-attempt rate.
- λ ≥ K/τ has no steady state; those cells run as a 600 s burst from idle and are marked SAT (their numbers depend on the burst length).

Table at the measured τ, K = 16, ρ = 1 s (K = 4 and 8 are printed by the test):

| τ (measured median) | λ (buys/s) | K/τ (/s) | a = λτ/K | First-attempt conflict (sim) | a/(1 + a) | 1 − e^(−a) | Mean attempts | P(≥ 3) | Max attempts | Regime |
|---|---|---|---|---|---|---|---|---|---|---|
| τ_chain = 0.919 s | 0.1 | 17.41 | 0.006 | 0.0057 | 0.0057 | 0.0057 | 1.006 | 0.0000 | 3 | steady |
| τ_chain = 0.919 s | 0.5 | 17.41 | 0.029 | 0.0286 | 0.0279 | 0.0283 | 1.029 | 0.0008 | 4 | steady |
| τ_chain = 0.919 s | 1.0 | 17.41 | 0.057 | 0.0560 | 0.0543 | 0.0558 | 1.060 | 0.0033 | 5 | steady |
| τ_chain = 0.919 s | 2.0 | 17.41 | 0.115 | 0.1149 | 0.1030 | 0.1085 | 1.130 | 0.0138 | 7 | steady |
| τ_chain = 0.919 s | 5.0 | 17.41 | 0.287 | 0.2861 | 0.2231 | 0.2496 | 1.404 | 0.0827 | 11 | steady |
| τ_merge = 1.655 s | 0.1 | 9.668 | 0.010 | 0.0104 | 0.0102 | 0.0103 | 1.011 | 0.0001 | 3 | steady |
| τ_merge = 1.655 s | 0.5 | 9.668 | 0.052 | 0.0502 | 0.0492 | 0.0504 | 1.053 | 0.0026 | 5 | steady |
| τ_merge = 1.655 s | 1.0 | 9.668 | 0.103 | 0.1042 | 0.0937 | 0.0983 | 1.116 | 0.0109 | 6 | steady |
| τ_merge = 1.655 s | 2.0 | 9.668 | 0.207 | 0.2070 | 0.1714 | 0.1869 | 1.264 | 0.0444 | 9 | steady |
| τ_merge = 1.655 s | 5.0 | 9.668 | 0.517 | 0.5171 | 0.3409 | 0.4038 | 2.099 | 0.2702 | 20 | steady |

Reading: τ_chain applies to wallets that build on the pending successor (the probe shows the mempool accepts such a buy at once, research F3); τ_merge to wallets that wait for a chain block. Saturation is K/τ = 17.4 buys/s (chained) or 9.7 buys/s (waiting for the merge) at K = 16. The probe also measured that a racing buy on the same shard state is always rejected (10/10), and that a higher-feerate replacement through `submit_transaction_replacement` did not displace a pending buy on chain in any attempt (D87 as amended by O-M6-1: the fee auction is not a mitigation; `docs/tn10-mass-check.md` M6).

## M7: real seed (anchor proof, void; D83 re-confirmation)

Source: `crates/lottery/tests/m7_measure.rs` (`cargo test -p lottery --release --test m7_measure -- --nocapture --test-threads=1`; 7 passed), real scripts after M7 Stage 5 fix (drawer `draw()` with the **272-byte** anchor proof and the void mode, `draw_window >= VOID_MIN` in check 1; marker without `seed_ref` and `refresh()`, controller check 20 without the seed reference), 2026-10-13. Same constants, limits, mass calculator and fee formulas as M6 (above). Every row is measured unless marked "estimate"; the M6 column is the value in the M6 section that the M7 value replaces. Coverage is the milestone M7 entry ("K, R and m_max are re-confirmed") = the M6 A5a rows marked "re-run", plus the void rows (spec M7-b).

**Stage 5 fix (review should-fix 1 and S1b-1), re-measured.** (A) drawer check 1 requires `draw_window >= VOID_MIN = 864,000` (replaces `draw_window >= 0`; one constant, no new local). (B) the anchor proof drops P.hash (304 → **272 B**: X 152 B ‖ P's preimage fields 120 B) and the opcode call on P.hash; P is authenticated by `pc == x_pc` alone. Effect against the Stage 3 values (commit 6818bee), which the tables below replace: drawer redeem 64,643 → **64,629 B** (−14); drawer sigscript in the worst `draw()` 78,733 → 78,687 B (−46: −14 redeem, −32 anchor); input 0 1,809,407 → 1,808,410 units (−997: one opcode call, its slice and compare); worst `draw()` 206,569 / 527,196 → **206,523 / 527,012** (−46 / −184 grams); typical 139,937 / 444,148 → 139,891 / 443,964; void worst 185,144 / 480,256 → 185,130 / 480,200 (−14 / −56, redeem only); void typical 123,120 / 403,720 → 123,106 / 403,664; the vault leg 459,796 → 459,698 units. Every percentage is unchanged at one decimal. `close()`, the K / B_MAX frontiers, M7-e (R) and the stack headroom (4) are unchanged.

**Stop rules (D104): none fires.** Every final shape is ≤ 90 % of compute, transient and storage (genesis exempt, U-M6-1); `close()` at K = 16 is ≤ 90 %; the drawer's stack headroom at check 24 is 4 slots (floor 4) and the drawer compiles. The tests assert all of these.

**Rows not re-run here are unchanged.** `m6_measure` on the same tree (10 passed) gives byte-identical masses for every row M6 A5a marks "valid": genesis, `buy()`, `set_params()`, `claim()`, returns, ticket/pool/position/request/record refreshes, vault requests and settlements, the M6-d `buy()` and `claim()` rows, M6-e and M6-f. The `refresh` marker row is gone (the marker has no `refresh()`, D99).

**M7-a: redeem sizes.**

| Object | Prefix B | Suffix B | State B | Redeem B | M6 redeem B | Δ |
|---|---|---|---|---|---|---|
| drawer (`draw`: 272-byte anchor proof and void, void floor; M6: `draw_test_seed`) | 1 | 64,491 | 137 | **64,629** | 65,234 | −605 (Stage 3: 64,643) |
| marker (`draw_leg` only, no `seed_ref`; M6: `draw_leg`, `refresh`, `seed_ref`) | 1 | 682 | 706 | **1,389** | 2,104 | −715 (state −9, code −706) |
| controller (`close`, `draw_leg`) | 1 | 27,196 | 427 | **27,624** | 27,694 | −70 |

The drawer shrank although it gained the anchor proof: check 19's unrolled 19-input `own` loop is gone (D98), and it was larger than the new check 7/8 code. Every other template is unchanged.

**M7-a / M7-b / M7-c: `draw()`, void and `close()`** (fees at 100 sompi per gram; "leg" = the vault's units, input 2):

| Transition | Input 0 units / sigscript B | Leg units | Outputs | Compute | Transient | Storage | % c / t / s | Relay floor (KAS) | Priority fee (KAS) | M6 % c / t / s |
|---|---|---|---|---|---|---|---|---|---|---|
| `draw()` worst (DP21: K = 16, m = 50, a = 7, np = 7, 8 funding inputs, cap binding, record, anchor 272 B) | 1,808,410 / 78,687 | 459,698 | 21 | 206,523 | 527,012 | 71,009 | **41.3 / 52.7 / 14.2** | 0.26351 | 0.26351 | 41.7 / 53.2 / 14.2 |
| `draw()` typical (K = 16, m = 10, a = 0, np = 3, no record) | 1,504,124 / 77,400 | 459,698 | 9 | 139,891 | 443,964 | 93,028 | 28.0 / 44.4 / 18.6 | 0.22198 | 0.22198 | 28.2 / 44.9 / 18.6 |
| void worst (K = 16, a = 7, 8 funding inputs, cap binding, record, fee paid) — **new** | 1,096,944 / 67,593 | 459,698 | 14 | 185,130 | 480,200 | 71,005 | **37.0 / 48.0 / 14.2** | 0.24010 | 0.24010 | — |
| void typical (K = 16, a = 0, nothing pending) — **new** | 949,375 / 67,586 | 459,698 | 6 | 123,106 | 403,664 | 33,685 | 24.6 / 40.4 / 6.7 | 0.20183 | 0.20183 | — |
| `close()` K = 16, sales in every shard (M2-a shape) | 3,372,030 / 28,364 | — | 20 | 293,743 | 864,612 | 402,301 | 58.7 / **86.5** / 80.5 | 0.43231 | 0.43231 | 58.9 / 86.8 / 80.5 |
| `close()` K = 16, storage worst case (M2-c grid: zero sales, O = J_boost = A = 0, caller 1,000,000 KAS) | 3,370,973 / 28,364 | — | 20 | 293,743 | 864,612 | **449,692** | 58.7 / 86.5 / **89.9** | 0.43231 | 0.44969 | 58.9 / 86.8 / 89.9 |

Against M6: the worst `draw()` is −2,148 compute and −4,592 transient grams (−1,148 B: the drawer's sigscript −363 B, that is its redeem −605 B and the 272-byte anchor in place of the 32-byte test seed; the marker input −715 B; the controller input −70 B); the vault leg runs 4,235 fewer units. `close()` is −876 compute / −3,104 transient (−776 B in the controller's sigscript: its redeem −70 B and the marker template code it carries −706 B) with the same 20 outputs and storage; its transient headroom to 90 % is now 35,388 grams (M6: 32,284). A void has no pools, leaves or siblings, so it is below the draw on every mass; with the same 8 funding inputs and record its storage (71,005) is 4 grams below the worst draw's.

**Tightest margins after M7 (genesis excluded, U-M6-1):** compute 58.7 % and transient **86.5 %** (`close()` K = 16, M2-a), storage **89.9 %** (`close()` K = 16 storage worst case, unchanged: it depends only on output count and values), then 84.5 % (`buy()` b = 1, referral 0.1 KAS, unchanged).

**M7-a / M7-c (M6-h rows): fees.** Round sales S at which C_close = s_C·S and C_draw = s_D·S (s_C = s_D = 50 bp) cover the transaction's fee:

| Transaction | Priority mass | Priority fee (KAS) | S covering it (KAS) | Relay floor (KAS) | S covering the relay floor (KAS) | M6: S covering the priority fee |
|---|---|---|---|---|---|---|
| `close()` K = 16 (M2-a shape) | 432,306 | 0.43231 | 86.46 | 0.43231 | 86.46 | 86.77 |
| `close()` K = 16, storage worst case | 449,692 | 0.44969 | 89.94 | 0.43231 | 86.46 | 89.94 |
| `draw()` worst | 263,506 | 0.26351 | 52.70 | 0.26351 | 52.70 | 53.16 |
| `draw()` typical | 221,982 | 0.22198 | 44.40 | 0.22198 | 44.40 | 44.86 |
| void worst — new | 240,100 | 0.24010 | 48.02 | 0.24010 | 48.02 | — |
| void typical — new | 201,832 | 0.20183 | 40.37 | 0.20183 | 40.37 | — |

The `draw()` storage supremum with the record at F_MIN_FLOOR (417,684, M7-e) still means a priority fee of at most 0.41768 KAS when storage dominates; C_draw covers it from S = 83.54 KAS (unchanged). The claim fee rows of M6-h are unchanged (valid rows).

**M7-c (M6-b): K frontier for `close()`** (in-memory K_MAX variants of controller, marker, drawer, pool and ticket, D91; M2-a shape; the K = 16 variant is hash-identical to the committed templates, asserted):

| K_MAX = K | Controller redeem B | Controller units / sigscript B | Compute | Transient | Storage at R = 1.69 | % c / t / s | R(K) (KAS, 0.01 up) | Storage at R(K) | M6 % c / t / s |
|---|---|---|---|---|---|---|---|---|---|
| **16** | 27,624 | 3,372,030 / 28,364 | 293,743 | 864,612 | 402,301 | 58.7 / **86.5** / 80.5 | 1.69 | 402,301 (80.5 %) | 58.9 / 86.8 / 80.5 |
| 17 | 28,965 | 3,576,184 / 29,705 | 312,868 | 916,032 | 425,961 | 62.6 / **91.6** / 85.2 | 1.78 | 404,415 (80.9 %) | 62.7 / 91.9 / 85.2 |
| 18 | 30,301 | 3,780,372 / 31,041 | 332,288 | 967,432 | 449,622 | 66.5 / 96.7 / 89.9 | 1.87 | 406,340 (81.3 %) | 66.6 / 97.1 / 89.9 |
| 19 | 31,637 | 3,984,624 / 32,377 | 351,808 | 1,018,832 | 473,282 | 70.4 / **101.9** / 94.7 | 1.96 | 408,082 (81.6 %) | 70.5 / 102.2 / 94.7 |

| Limit | Largest K within 90 % | Largest K within 100 % | M6 |
|---|---|---|---|
| transient (binding) | **16** | **18** | 16 / 18 |
| compute | ≥ 19 (largest measured) | ≥ 19 | same |
| storage, this shape at R = 1.69 | 18 | 19 | same |

Each shard still adds about 51,400 transient grams; K = 17 misses 90 % by 16,032 grams. **K = 16 stays the largest K within 90 %.**

**M7-c (M6-c): B_MAX frontier at K = 16:**

| B_MAX | `close()` K = 16: compute | transient | storage | % c / t / s | `buy()` b = B_MAX, referral: % c / t / s | M6 `close()` % c / t / s |
|---|---|---|---|---|---|---|
| 3 | 256,967 | 739,108 | 402,301 | 51.4 / 73.9 / 80.5 | 8.7 / 14.9 / 28.8 | 51.6 / 74.2 / 80.5 |
| **4** | 293,743 | 864,612 | 402,301 | 58.7 / **86.5** / 80.5 | 9.3 / 15.9 / 21.7 | 58.9 / 86.8 / 80.5 |
| 5 | 332,019 | 990,116 | 402,301 | 66.4 / **99.0** / 80.5 | 9.9 / 16.9 / 17.4 | 66.6 / 99.3 / 80.5 |

The `buy()` columns are unchanged (the shard does not change). **B_MAX = 4 stays final**; 5 would put `close()` at 99.0 % of transient.

**M7-d (M6-b, M6-d): `draw()` with the anchor at the drawer variants** (worst draw: a = 7, np = 7, 8 funding inputs, cap binding, record; drawer-only variants, marker/vault/pool commit to the variant's hash):

| Drawer | K | m | Compute | Transient | Storage | % c / t / s | M6 % c / t / s |
|---|---|---|---|---|---|---|---|
| committed (K_MAX 16, M_DRAW 50) | 16 | 10 | 204,843 | 521,892 | 71,009 | 41.0 / 52.2 / 14.2 | 41.4 / 52.6 / 14.2 |
| committed (K_MAX 16, M_DRAW 50) | 16 | 25 | 205,423 | 523,812 | 71,009 | 41.1 / 52.4 / 14.2 | 41.5 / 52.8 / 14.2 |
| committed (K_MAX 16, M_DRAW 50) | 16 | **50** | 206,523 | 527,012 | 71,009 | **41.3 / 52.7 / 14.2** | 41.7 / 53.2 / 14.2 |
| M_DRAW 25 (variant) | 16 | 10 | 201,768 | 513,992 | 71,009 | 40.4 / 51.4 / 14.2 | 40.6 / 51.8 / 14.2 |
| M_DRAW 25 (variant) | 16 | 25 | 202,448 | 515,912 | 71,009 | 40.5 / 51.6 / 14.2 | 40.7 / 52.0 / 14.2 |
| K_MAX 17 (variant, K = 16 drawn) | 16 | 50 | 208,760 | 531,960 | 71,009 | 41.8 / 53.2 / 14.2 | — |

Drawer code: 62,517 B at M_DRAW = 25, 64,492 B at M_DRAW = 50 (Stage 3: 62,531 / 64,506; M6: 63,047 / 65,097). **m = 50 is within 90 % for `draw()` (asserted)**; with the unchanged `buy()` and `claim()` rows of M6-d it stays m_max.

`draw()` and `claim()` at K_MAX variants — **estimates** (M6-b method: transient only, 4 grams per byte, `refs/rusty-kaspa-v2.1.0/consensus/core/src/constants.rs:31`; baselines measured at K = 16 with the M7 drawer):

| K_MAX | Drawer code B (Δ) | Ticket code B (Δ) | `draw()` extra B | Worst `draw()` transient | % | `claim()` PAY F_MAX extra B | `claim()` transient | % | M6 `draw()` % |
|---|---|---|---|---|---|---|---|---|---|
| 16 | 64,492 (+0) | 25,360 (+0) | 0 | 527,012 (measured) | 52.7 | 0 | 160,880 (measured) | 16.1 | 53.2 |
| 17 | 65,727 (+1,235) | 26,581 (+1,221) | 3,508 | 541,044 (estimate) | 54.1 | 1,929 | 168,596 (estimate) | 16.9 | 54.6 |
| 18 | 66,960 (+2,468) | 27,799 (+2,439) | 7,009 | 555,048 (estimate) | 55.5 | 3,855 | 176,300 (estimate) | 17.6 | 56.0 |
| 19 | 68,193 (+3,701) | 29,017 (+3,657) | 10,510 | 569,052 (estimate) | 56.9 | 5,781 | 184,004 (estimate) | 18.4 | 57.4 |

`draw()` and `claim()` remain far from binding K; `close()` transient binds.

**M7-e (M6-g): R re-check** at R = 1.69 KAS, K = 16 (`close()` keeps K + 3 outputs; the anchor is an argument, not an output; a void has fewer outputs than a draw, and the measured worst below is the maximum over both modes):

| Transition | Measured worst | % | Supremum | % | M6 |
|---|---|---|---|---|---|
| `close()` (M2-c grid; worst: zero sales, O = J_boost = A = 0, caller 1,000,000 KAS) | 449,692 | 89.9 | 449,692 = (K + 3)·4C/R | **89.9** | 449,692 (89.9 %) |
| `draw()` / void with the record, f_min = F_MIN = 1 KAS | 317,481 | 63.5 | 327,684 | 65.5 | 327,684 (65.5 %) |
| `draw()` / void with the record, f_min = F_MIN_FLOOR = 0.1 KAS | 407,481 | 81.5 | 417,684 | 83.5 | 417,684 (83.5 %) |

R = 4C(K + 3)/(0.9 L) = 168,888,889 sompi at K = 16, rounded up to **1.69 KAS**; `draw()` alone needs R ≥ 152,941,177 sompi (1.529 KAS). Binding transition: `close()` (89.9 %, 308 grams below 90 %). **R = 1.69 KAS stays.**

**M7-f (M6-j): drawer sensitivity, byte budget and stack headroom.**

| Pair | m | Drawer redeem B (a / b) | Δ B | Compute (a / b) | Transient (a / b) | Δ compute per B | Δ transient per B | M6 slopes |
|---|---|---|---|---|---|---|---|---|
| M_DRAW 25 vs 50 | 25 | 62,654 / 64,629 | 1,975 | 202,448 / 205,423 | 515,912 / 523,812 | 1.506 | 4.000 | 1.976 / 4.000 |
| K_MAX 16 vs 17 | 50 | 64,629 / 65,864 | 1,235 | 206,523 / 208,760 | 527,012 / 531,960 | 1.811 | 4.006 | 1.648 / 4.006 |

Worst `draw()` at the committed drawer (64,629 B): compute 206,523 (41.3 %), transient 527,012 (52.7 %). With the larger slope of each (1.811 compute, 4.006 transient grams per drawer byte) **the drawer can grow by about 93,096 B before the worst `draw()` reaches 90 % of transient** (134,418 B for compute); transient binds (Stage 3: 93,050 / 134,393 B; M6: 91,950 B / 122,154 B).

**Stack headroom at check 24 (measured, replaces M6's fixed sentence):** drawer variants with n extra `int` locals declared before check 1 and used after check 24 compile for n ≤ **4**; n = 5 fails with "entrypoint 'draw' reaches 245 combined stack items … exceeding the consensus limit of 244". **4 free slots, exactly the D104 floor** (M5 Fact S4-1 and M6: 4; re-measured after the Stage 5 fix: still 4, the void-floor require keeps nothing live and the removed P opcode block was braced). The new `isVoid` bool costs one slot; dropping `seed_ref` from the marker struct and the `own` loop freed one. Any further drawer value live across check 24 fires D104.

**D83 re-confirmation (milestone M7 criterion; spec 7 A10 item 8).** Every row of the milestone list is re-run with the final M7 `draw()` and `close()` and stays within 90 %: **K = 16, m_max = M_DRAW = M_CLAIM = 50, B_MAX = 4 and R = 1.69 KAS are final.** Tightest: `close()` storage worst case 89.9 % (sets R), `close()` K = 16 transient 86.5 % (sets K), the drawer stack at its 4-slot floor (sets how much more can go into `draw()`).

## M8: testnet-10 end to end (S1: whale-margin floor 1.5, D113)

**S1 re-measure (release `m7_measure`, 2026-10-14).** The floor changes one constant and one comparison operand in `controller.sil` and `drawer.sil`; both redeem scripts keep their size (controller 27,624 B, drawer 64,629 B). The fixtures now run at margin 1.5.

| transaction | compute | transient | storage | % c / t / s | M7 | Δ |
|---|---|---|---|---|---|---|
| `close()` K = 16, sales in every shard (M2-a shape) | 293,744 | 864,616 | 402,301 | 58.7 / **86.5** / 80.5 | 293,743 / 864,612 / 402,301 | +1 / +4 / 0 |
| `close()` K = 16, storage worst case | 293,743 | 864,612 | **449,692** | 58.7 / 86.5 / **89.9** | unchanged | 0 |
| `draw()` worst (DP21 with the anchor) | 206,522 | 527,008 | 71,009 | 41.3 / 52.7 / 14.2 | 206,523 / 527,012 / 71,009 | −1 / −4 / 0 |

The ±1 / ±4 grams are one signature-script byte (a minimally encoded argument of the re-derived fixture changing length): 1 compute gram and 4 transient grams per byte. Drawer stack headroom at check 24: **4** (5 fails to compile, 245 > 244 combined stack items), the D104 floor, with no slack. K = 16, m_max = 50, B_MAX = 4 and R = 1.69 KAS stay final (D83).

**Margin floor note closed (MF5).** At 1.5, wherever neither the edge rule nor the cap binds, `fix_jackpot` gives the largest jackpot the whale check allows, and jackpot money per ticket is within 1/N sompi below q = ⌊p w / margin⌋ (q / p = 0.5267 at the fixture shares; 0.6752 at 1.17). At 1.5 the edge rule is never the binding check (q < a = 0.629 p).

**Testnet-10 fees (S3b–S4, instance `e9f4f810…88d6`, production templates).** The fee rate was the node's normal bucket, which was 100 sompi/gram throughout, so every fee is the priority mass × 100 sompi (D120). The per-shape masses and fees of all 300 C0–C7 instance transactions are in `docs/tn10-mass-check.md` M8, "Masses and fees per shape" (`tn10 masses`; the computed storage mass equals the committed one for every transaction). In brief:

| | fee per transaction (KAS) |
|---|---|
| `close()` K = 16 | 0.4323 |
| `draw()`, real anchor | 0.2210–0.2217 |
| buy b = 4 (b = 1: higher storage mass from smaller change outputs) | 0.079–0.084 (b = 1: 0.309–0.323) |
| claim (tier 4, ACCRUE) | 0.080–0.083 |
| request / settle | 0.037–0.041 |
| `refresh()` of a ticket (soak) | 0.0769 |
| genesis (K = 16) | 0.4609 |

- **Instance fees C0–C7:** 35.56 tKAS for 300 transactions. The soak's sitting 3 (close, draw, four claims, refresh, fresh funding) cost about 1.05 tKAS.
- **Spent or locked** (fees plus money left in the instance): 556.78 tKAS at C7, peak 843.73 (after round 2's buys, before the late draw and the withdrawal settles paid back), 890.54 after the soak. A campaign budget must state the peak as well as the end figure (DECISIONS, M8 owner amendments).
- The `close()` masses equal M7's (293,743 / 864,612). The real-anchor draws are within a few hundred grams of M7's.

**Drawer stack peak confirmed on a real spend (S5, sil-debug).** On round 8's `draw()` (`a08b156f…ed6c`), the patched `cli-debugger` (`tools/sil-debug/`) counts a runtime peak of **240 of 244** combined stack items, at `drawer.sil:994` (check 24). That leaves 4 free slots, the D104 floor measured at compile time above and in M7. The claim (`43fb4cf3…b37f`) peaks at 88. Caveat: the debugger links rusty-kaspa `a41a333b`, not v2.1.0.

**kascov storage mass (note).** kascov.io's preflight agrees with our compute and transient mass on all 14 shapes. Its storage mass is 9 to 25,017 grams above the committed storage mass of each accepted transaction (buy +1,576, close +25,017, the others +9 to +108); the committed mass equals ours. Its fee estimate (100 sompi × compute grams + 200,000) is its own formula, not the node's rate on the priority mass. So do not size fees from kascov (`docs/tn10-mass-check.md` "M8 S5: review tools").

## Spike VDF+ZK: the seed proof inside `draw()` (local engine; spec `docs/specs/spike-vdf.md`)

Measured 2026-10-13 by `cargo test -p lottery --release --test spike_vdf -- --nocapture --test-threads=1`. Test-only drawer variants, compiled in memory from a patch of the committed `drawer.sil` (unchanged). The VDF output and proof follow the 272-byte anchor inside the `anchor` argument, so the variants add no entry parameter. V1 = plain Groth16 (`g16.verify`, 4 public inputs: the X.hash halves and the MinRoot output (x_T, y_T); real proof from a local `ark-groth16` 0.6 setup, T = 64). V2 = RISC Zero receipt wrapped in Groth16 (`r0.g16.verify`, SilverScript's fixture receipt; the journal binding sha256(X.hash || y) is computed but the fixture's journal is verified). V3 = RISC Zero succinct (rusty-kaspa's fixture receipt, seal 222,668 B).

**Masses** (limits 500,000 / 1,000,000 / 500,000; D104 90 %):

| shape | compute | transient | storage | % c / t / s | ≤ 90 % |
|---|---|---|---|---|---|
| M7 worst draw (baseline) | 206,523 | 527,012 | 71,009 | 41.3 / 52.7 / 14.2 | yes |
| M7 typical draw (baseline) | 139,891 | 443,964 | 93,028 | 28.0 / 44.4 / 18.6 | yes |
| M7 worst void (baseline) | 185,130 | 480,200 | 71,005 | 37.0 / 48.0 / 14.2 | yes |
| V1 worst draw (sigscript 79,378 B) | 360,614 | 529,776 | 71,009 | **72.1** / 53.0 / 14.2 | yes |
| V1 typical draw | 293,282 | 446,728 | 93,028 | 58.7 / 44.7 / 18.6 | yes |
| V1 worst void | 186,429 | 482,196 | 71,005 | 37.3 / 48.2 / 14.2 | yes |
| V2 worst draw (sigscript 79,766 B) | 363,602 | 531,328 | 71,009 | **72.7** / 53.1 / 14.2 | yes |
| V2 typical draw | 296,270 | 448,280 | 93,028 | 59.3 / 44.8 / 18.6 | yes |
| V2 worst void | 187,049 | 483,876 | 71,005 | 37.4 / 48.4 / 14.2 | yes |
| V3 worst / typical draw | not encodable: signature script 301,908 / 300,621 B > `MAX_SIGNATURE_SCRIPT_LEN` = 250,000 (rusty-kaspa v2.1.0 `consensus/core/src/config/params.rs:25`, every network) | | | | no |
| V3 worst void | 185,457 | 481,108 | 71,005 | 37.1 / 48.1 / 14.2 | yes |

Delta of the worst draw over M7: V1 +154,091 compute / +2,764 transient (the opcode is 140,000 + 5 × 2,500 = 152,500 grams, `zk_precompiles/tags.rs:29-32`, `groth16/mod.rs:18, 46-49`; glue about 1,600); V2 +157,079 / +4,316 (155,000 for 5 public inputs plus two SHA-256). Storage unchanged (no new output). Drawer redeem: M7 64,629 B; V1 65,128 B (+499, the 392-byte VK constant); V2 65,546 B; V3 64,856 B.

**Verifier alone** (one input, one output; a floor for step 1 of a split `draw()`):

| variant | sigscript B | script units | compute | transient | % c / t | ≤ 90 % |
|---|---|---|---|---|---|---|
| V1 | 733 | 15,252,582 | 153,804 | 3,736 | 30.8 / 0.4 | yes |
| V2 | 1,121 | 15,503,630 | 156,692 | 5,288 | 31.3 / 0.5 | yes |
| V3 | 223,260 | 25,446,723 | **478,231** | **893,844** | **95.6 / 89.4** | **no** |

**Stack** (extra live int locals that still compile; D104 needs ≥ 4 at check 24; the instrument asserts the first failure is the stack limit): at check 24, M7 4 and V1 / V2 / V3 4 each; at the verify call (ints declared inside the verify block, so they die with it) V1 125, V2 122, V3 121. The proof bytes are slices of `anchor`, not new parameters, so check 24 stays the binding point and the verify is not.

**Prover (V1, MinRoot over BN254 Fr, `ark-groth16` 0.6, this machine, 20 threads; median of 5 runs per T, [min, max]; the test binary run alone):**

| T | constraints | eval (s) | setup (s) | prove (s) | proving key (MB) | prove / eval |
|---|---|---|---|---|---|---|
| 2^10 | 3,074 | 0.006 [0.006, 0.006] | 0.02 | 0.04 [0.04, 0.04] | 0.6 | 7.1 |
| 2^12 | 12,290 | 0.023 [0.023, 0.023] | 0.07 | 0.11 [0.11, 0.11] | 2.5 | 4.7 |
| 2^14 | 49,154 | 0.091 [0.090, 0.091] | 0.23 | 0.35 [0.34, 0.38] | 10.0 | 3.9 |
| 2^16 | 196,610 | 0.364 [0.363, 0.374] | 0.86 | 1.29 [1.27, 1.29] | 39.8 | 3.5 |

Slopes (2^14 → 2^16, medians): evaluation 5.56 µs per iteration (sequential), proving 18.94 µs per iteration (parallel; ratio 3.4; 3.5 with the fastest evaluation and slowest proof), proving key 608 B per iteration. **The timing depends on load:** the reviewer's run, made while the full `lottery` suite was running on the same machine, measured 7.61 / 53.31 µs per iteration (ratio 7.0). Both are recorded; plan with the higher ratio.

The draw's own fee and the proving cost come out of the **same** reward C_draw = floor(S × s_D / 10⁴) (s_D = 50 bp, `game::round_amounts`), so the break-even round sales are S = (fee + prove hours × price) × 10⁴ / s_D. The V1 worst draw's priority fee is 0.36061 KAS (72.1 % compute), which C_draw covers from S = 72.12 KAS on its own. **Linear extrapolation far outside the measured range** (×10³ to ×10⁴; FFT cost is n log n and a single monolithic circuit this large needs the proving key in memory, so these are optimistic):

| honest evaluation delay on this machine | T | prove (h) at ratio 3.4 | prove (h) at ratio 7.0 | evaluation + proof (h) at ratio 7.0 | proving key (GB) | break-even S at 1 KAS per machine-hour (KAS), ratio 3.4 / 7.0 |
|---|---|---|---|---|---|---|
| 10 s | 1.8 × 10⁶ | 0.01 | 0.02 | 0.02 | 1.1 | 74 / 76 |
| 1 min | 1.1 × 10⁷ | 0.06 | 0.12 | 0.13 | 6.6 | 83 / 96 |
| 10 min | 1.1 × 10⁸ | 0.57 | 1.17 | 1.33 | 66 | 186 / 305 |
| 1 h | 6.5 × 10⁸ | 3.41 | 7.0 | 8.0 | 394 | 754 / 1,473 |

The proving part of S scales with the machine price in KAS per hour (not stated here: no price from memory); the fee part (72.12 KAS) does not. Against the opcode window (about 12 h, during which X must stay within F of the point of view, after which the round voids), a 1-hour delay at ratio 7.0 already uses 8 h of evaluation plus proving: **time is comfortable only for delays of minutes, and the proving key (6.6 GB per minute of delay) limits a monolithic circuit first.**

**Margin floor** (S3; minimum ε where the whale check binds, q = p w / margin, fixture shares, the player's own lower-tier share counted as returned; reproduces D100 amended at 1.17 and 1.00):

| margin | q / p | ε ordinary | ε self-referring | ε self-referring + both caller rewards |
|---|---|---|---|---|
| 1.00 | 0.7900 | 0.1100 | 0.0100 | 0.0000 |
| 1.05 | 0.7524 | 0.1476 | 0.0476 | 0.0376 |
| 1.10 | 0.7182 | 0.1818 | 0.0818 | 0.0718 |
| 1.17 | 0.6752 | 0.2248 | 0.1248 | 0.1148 |
| 1.25 | 0.6320 | 0.2680 | 0.1680 | 0.1580 |
| 1.30 | 0.6077 | 0.2923 | 0.1923 | 0.1823 |

Margin at which ε reaches the edge-rule ordinary value 0.271: ordinary 1.256, self-referring 1.493, self-referring with both caller rewards 1.522. Method: the closed form of D100 amended (a), ε = 1 − w / margin − (s_t [+ s_R [+ s_C + s_D]]) / 10⁴ with `game::whale_allowance`; it assumes `fix_jackpot` reaches EV = q at every margin, which D100 amended showed at 1.17 only (the table reproduces its 1.17 and 1.00 values).

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
