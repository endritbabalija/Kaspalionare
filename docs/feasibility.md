# M0 Feasibility

Date: 2026-10-09. Answers the six questions of milestone M0 (`docs/milestones.md`). Revised 2026-10-10 after a second covenant-reviewer pass: corrections are marked **(revised 2026-10-10)**, and Q7 adds the network mass-limit check.

**Sources.** Shallow clones in `refs/` (git-ignored). Paths below are relative to `refs/`.

| Repo | Ref | Commit |
|---|---|---|
| `rusty-kaspa-v2.1.0` | tag v2.1.0 (pinned, see Q6) | `01b532e8b553523216471682649693af92f0fd16` |
| `rusty-kaspa-v2.0.1` | tag v2.0.1 (design baseline) | `cfafeb4c093fa37a303f1b9f19c58f986b870ce3` |
| `silverscript` | tag v1.0.0 | `3ed973335b59269293564805cc2c58a14595ec03` |
| `kips` | master | `e4ae2332117b5cb68bd6188e065ef885b6d17939` |

**Measurements.** All numbers come from tests in `crates/harness/tests/`. They run in the local v2.1.0 script engine with the v2.1.0 mainnet mass calculator. Reproduce them from `crates/`:

```
cargo test -- --nocapture --test-threads=1
```

The covenants in `covenants/m0/` are probes for measurement, not protocol code.

**Mass limits used throughout.** Per-transaction mass limits equal the block limits: compute 500,000, storage 500,000, transient 1,000,000 (`rusty-kaspa-v2.1.0/consensus/core/src/config/params.rs:626`). The mempool applies them per transaction (`mining/src/mempool/check_transaction_limits.rs:14-29, 38-40`). There is no separate standard-transaction mass cap in v2.1.0.

---

## Q1. Can SilverScript express stateful covenants with multiple inputs and outputs and KIP-20 covenant IDs?

**Answer: yes, partly through the declarative layer.** Our transitions need handwritten checks.

**What SilverScript provides:**
- **State.** Contract fields form an implicit `State` (`silverscript/docs/DECL.md:13-16`).
- **Covenant-ID builtins.** `OpInputCovenantId`, `OpOutputCovenantId`, `OpCovInputCount/Idx`, `OpCovOutputCount/Idx` and `OpAuthOutputCount/Idx` are all exposed (`silverscript/silverscript-lang/src/compiler/compile/expression/builtin.rs:67-74`).
- **Cross-template state.** `readInputStateWithTemplate`, `validateOutputStateWithTemplate` and `validateOutputStateWithInputTemplate` read and write state across templates (`silverscript/docs/TUTORIAL.md:1207-1287`).

**What we measured** (`tests/m0_shapes.rs`). We used two templates in one covenant family:
- inputs: controller, K shards and the caller's funding input;
- outputs: controller successor, K fresh shards and an unbound caller output carrying reward plus change;
- every count and index checked in script.

The transaction conserves value and runs every input under the compute budget it commits. Its masses are within all limits, its inputs cover its outputs, and the test now commits the storage mass that consensus checks (`tx_validation_in_utxo_context.rs:126-133`). Its fee covers the minimum relay fee for its masses; `assert_valid_masses` checks both. **(revised 2026-10-10)** It is not shown to be fully consensus-valid: the consensus isolation checks, signatures and a real genesis lineage are not run here (they are on testnet-10, Q7). It is not relayable as built, because the caller's funding input uses an OpTrue script as a stand-in, which mempool standardness rejects (`mining/src/mempool/check_transaction_standard.rs:52-54`). Its compute mass also leaves out a real signature check. It passes for K = 1, 3 and 8. At K = 8 the controller uses 32,240 script units (budget 3), compute mass is 8,997 and storage mass is 952.

It rejects four attacks. Each test asserts that the controller (input 0) refuses with `VerifyError`:
- an extra output;
- a wrong shard successor state;
- a shard from another covenant family;
- a reward output bound into the family.

**Three limitations:**
- **Declarations assume one template per family.** The declarative `#[covenant(...)]` layer assumes every input and output in the family uses the same template and state layout (`silverscript/docs/DECL.md:519-536`). Our families mix controller, shard, marker, pool and ticket templates, so transitions are handwritten with the templated builtins.
- **Declarations allow extra outputs.** The singleton declaration counts only the outputs its own input authorizes (`silverscript/silverscript-lang/tests/covenant_declaration_ast_fixtures/lowers_singleton_sugar_transition_two_field_state.sil:22`). An extra plain output passes, and the test `singleton_sugar_does_not_reject_extra_plain_output` demonstrates it. Rule R2 is therefore hand-coded (`tx.inputs.length`, `tx.outputs.length`, `OpCovInputCount`, `OpCovOutputCount`).
- **`OpOutputAuthorizingInput` is not exposed** in SilverScript v1.0.0. An unbound output reports the zero covenant ID instead (`rusty-kaspa-v2.1.0/crypto/txscript/src/opcodes/mod.rs:1412`), and the probe checks for that.

**Impact on the design.** None on the rules. Implementation notes:
- Each transition is a handwritten entry using the templated state builtins.
- KIP-20 allows several authorization groups with the same covenant ID in one transaction (`kips/kip-0020.md:298`), and `OpCovOutputCount` counts across all of them. Shape checks must use the family-wide counts plus explicit indices, not one input's authorized outputs.

**Remaining risks:**
- SilverScript says it "is experimental and has not reached its first stable release" despite the v1.0.0 tag (`silverscript/SECURITY.md:5`). No audit was found.
- The compiler links its own rusty-kaspa revision `a41a333b`, which sits between v2.0.1 and v2.1.0 (`silverscript/Cargo.toml:24-31`). Our tests run its bytecode in the v2.1.0 engine.

## Q2. Can a script verify a sparse Merkle path and compute the updated root from the same path, within limits? Which BLAKE-family opcode?

**Answer: yes.**

**Available hash opcodes:**

| Opcode | Byte | Location in `opcodes/mod.rs` |
|---|---|---|
| `OpBlake2b` | 0xaa | `:797` |
| `OpBlake2bWithKey` | 0xa7 | — |
| `OpBlake3` | 0xd9 | `:1456` |
| `OpBlake3WithKey` | 0xda | — |
| `OpSHA256` | 0xa8 | — |

KIP-17 specifies the Blake3 and keyed opcodes (`kips/kip-0017.md:72-74`).

**Pricing** (`rusty-kaspa-v2.1.0/crypto/txscript/src/opcodes/mod.rs:61-73`):
- Hashing costs 2 script units per byte for Blake2b and 1 per byte for Blake3.
- 100 script units equal 1 gram, and one compute-budget unit equals 10,000 script units.
- Each input gets 9,999 script units free (`consensus/core/src/mass/units.rs:4-7, 22-24`).
- Script and element size limits are 1,000,000 bytes, and the stack limit is 244 items (`crypto/txscript/src/lib.rs:77-80`).

**Measured** (`tests/m0_merkle.rs`, probe `covenants/m0/merkle_counter.sil`). The probe hashes the leaf `H(0x00‖index‖count)` and nodes `H(0x01‖L‖R)`. It verifies the old leaf against the root, builds the new root with count + 1 along the same path, and runs as a singleton transition:

| Depth | Hash | Redeem B | Sigscript B | Script units | Budget | Compute mass | Transient mass |
|---|---|---|---|---|---|---|---|
| 16 | Blake2b | 1,911 | 2,439 | 40,169 | 4 | 3,444 | 10,696 |
| 23 | Blake2b | 2,666 | 3,418 | 67,323 | 6 | 4,623 | 14,612 |
| 16 | Blake3 | 1,911 | 2,439 | 38,063 | 3 | 3,344 | 10,696 |
| 23 | Blake3 | 2,666 | 3,418 | 64,307 | 6 | 4,623 | 14,612 |

A forged count (3, 5 or 0 instead of 4) is rejected at depth 23 with both hashes. The test commits the genuine call's budget and asserts that the failure is `VerifyError`, not a budget overrun. **(revised 2026-10-10)** The successor under test is now built from the forged count + 1, so it agrees with the claimed leaf and only the old-root check can reject. Before, the successor was built from the true count, and the successor-state check alone would also have rejected.

**Decision: `OpBlake2b`.**
- P2SH commitments already use Blake2b (`crypto/txscript/src/standard.rs:51`).
- Blake3 saves only about 4.5% of script units, and gives the same compute budget at depth 23.
- SilverScript's template hash uses Blake3 (`silverscript/silverscript-abi/src/lib.rs:109`), so both primitives are in the trust base regardless.

**Impact on the design.** Section 8.2 is confirmed. A depth-23 update uses about 1% of the per-transaction compute limit. Size, not compute, is the cost driver (see Q5).

**Remaining risks:**
- The probe omits the shard's other `buy()` checks: canonical combination, price, ticket output and `H(combination) mod K`. The real shard script will be larger.
- The empty-leaf convention (an absent key means count 0) is not yet specified. It belongs in the M1 spec.

## Q3. Can covenant scripts be tested locally with the rusty-kaspa script engine in Rust tests?

**Answer: yes.**

**How the harness runs a script** (`crates/harness/src/lib.rs`):
1. Compiles `.sil` with `silverscript_lang::compiler::compile_to_sil_abi_artifact` (`silverscript/silverscript-lang/src/compiler/abi.rs:107`).
2. Wraps the bytecode in P2SH.
3. Builds a v1 transaction.
4. Runs each input through `TxScriptEngine::from_transaction_input_with_script_units_limit` (`rusty-kaspa-v2.1.0/crypto/txscript/src/lib.rs:508`), using the input's committed compute budget and mainnet engine flags.

**Matches consensus.** The harness builds `CovenantsContext::from_tx`, which is exactly what consensus uses for KIP-20 covenant checks (`consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs:176-178`).

**Evidence.** `cargo test` passes all 19 tests: 3 in `tests/covenant_smoke.rs` and 16 measurement and probe tests (`m0_limits` 3, `m0_mass` 5, `m0_merkle` 2, `m0_shapes` 6; revised 2026-10-10, was 15). The probe tests commit the measured budget and run under it.

**Impact on the design.** None.

**Remaining risks:**
- The engine path does not run the other consensus checks: zero-value outputs, mass limits, lock time and fee. Tests check mass explicitly with `harness::masses`. Lock-time and DAA-score tests (M2) need the transaction's lock time and the UTXO DAA scores set by hand.
- Testnet-10 remains the final check (M7).

## Q4. What storage mass does a ticket output have, and what minimum deposit does that imply?

**Answer: storage mass per covenant output is 4·10¹² / value. In the worst case a ticket needs at least 0.0844 KAS to be valid (R for K = 16, price 0.1 KAS, no referral output). It needs about 5 KAS before storage mass stops being the block bottleneck in steady state.**

**Formula.**
- KIP-9 storage mass is C·(Σ_O p²/o − |I|/A(I))⁺ with C = 10¹² (`kips/kip-0009.md:29, 86`; `rusty-kaspa-v2.1.0/consensus/core/src/constants.rs:26`; `consensus/core/src/mass/mod.rs:439-513, 463`).
- The size factor p is ceil((63 + spk length + 32 if covenant-bound) / 100) (`mass/mod.rs:83-99`). A covenant-bound P2SH output (ticket, shard, controller, marker, pool) has p = 2. A plain P2PK or P2SH output has p = 1.
- Storage mass is not part of the relay fee floor (`mining/src/mempool/check_transaction_standard.rs:67-69`), but it is capped per transaction and per block at 500,000. **(revised 2026-10-10)** The mempool ranks transactions by fee per gram of the normalized maximum of compute, transient and storage mass (`mining/src/mempool/model/frontier/feerate_key.rs:86-98`). A storage-heavy transaction therefore needs a higher fee to be included under load. A worst-case `buy()` or `close()` near 500,000 storage needs about 0.5 KAS to rank at the minimum feerate, against a relay floor of 0.0078 / 0.13 KAS (`m0_mass::fee_for_priority_includes_storage`).
- Zero-value outputs are invalid (`consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs:150-151`).

**Ticket deposit** (`tests/m0_mass.rs::q4_minimum_ticket_deposit`). A one-ticket `buy()` has:
- inputs: shard and funding;
- outputs: shard successor, ticket and buyer change;
- the shard sigscript sized at the measured 3,418 B.

Its compute mass is 6,612 and its transient mass is 15,648, giving a minimum fee of 0.0078 KAS. Compute limits a block to 75 buys; transient mass limits it to 63.

The worst case covers two variables:
- **Shard pot:** 0 (the first buy on a fresh shard holding only R, so its successor is a small R + price output) or 1,000 KAS.
- **Buyer change:** 1 KAS (adds C/1 KAS) or 10⁷ KAS (removes input relief).

Minimums below are rounded up to 0.0001 KAS; exact sompi values are printed by the test.

Worst-case storage for one ticket, with R = 1.44 KAS (K = 16) and price 0.1 KAS:

| Deposit (KAS) | 0.01 | 0.08 | 0.09 | 0.1 | 0.5 | 1 | 2 | 5 | 10 |
|---|---|---|---|---|---|---|---|---|---|
| Worst storage mass | 4,025,974 (invalid) | 525,974 (invalid) | 470,418 | 425,974 | 105,974 | 65,974 | 45,974 | 33,974 | 32,804 |
| Buys per block by storage | 0 | 0 | 1 | 1 | 4 | 7 | 10 | 14 | 15 |

Minimum deposit so that every one-ticket `buy()` is valid (storage ≤ 500,000):

| K (sets R) | R (KAS) | Price 0.1 KAS | Price 1 KAS |
|---|---|---|---|
| 16 | 1.4400 | 0.0844 | 0.0828 |
| 32 | 2.7200 | 0.0824 | 0.0818 |
| 64 | 5.3058 | 0.0817 | 0.0817 |

**(revised 2026-10-10)** With bundles, one `buy()` creates one ticket holding d × b, so the floor applies to the ticket's value: 0.0844 / 0.0842 / 0.0835 / 0.0828 KAS for b = 1 / 2 / 5 / 10 (K = 16, price 0.1). The earlier figures for n separate ticket outputs (0.1683 / 0.4173 / 0.8312) describe a shape the design no longer has.

**Referral output (revised 2026-10-10).** The `buy()` shape with a plain referral output (design 7.1) adds C / referral. Minimum ticket value at K = 16:

| Referral output | Price 0.1 KAS | Price 1 KAS |
|---|---|---|
| none | 0.0844 | 0.0828 |
| 0.01 KAS | none exists | none exists |
| 0.1 KAS | 0.1075 | 0.1045 |
| 0.5 KAS | — | 0.0871 |
| 1 KAS | — | 0.0866 |

A referral output below 0.02 KAS can never be valid (G3, Q7), so M_ref ≥ 0.02 KAS, and the deposit floor depends on M_ref. A realistic referral is s_R × price (10 % now, at most 15 %). At price 0.1 KAS it is 0.01–0.015 KAS, below the 0.02 KAS floor, so it can never be paid as an output and always goes to the boost pot. The 0.1 KAS referral at price 0.1 is a mass test of the output shape only; the meaningful figure is 0.1045 KAS (price 1, s_R = 10 %).

**Throughput.**
- In the worst case every `buy()` carries a storage floor of 35,974 that no deposit removes: the fresh-shard successor at R + price (about 26,000) plus a 1 KAS change output (10,000). That limits such buys to 13 per block.
- A larger R and larger change outputs shrink the floor.
- For steady-state buys (shard pot 1,000 KAS, large change), the deposit at which storage stops being the block bottleneck (63 buys per block) is 5.0646 KAS.

**Protocol-object reserve R** (decision 2026-10-09, `tests/m0_mass.rs::q4_reserve_r_by_k`).
- Setup: `close()` creates the marker, the controller and K shards, each carrying R. The caller posts the marker's R and receives the reward in its own change output.
- The worst case is taken over gross sales of 0, 10⁴ and 10⁶ KAS, over the controller's own value O (boost pot plus fee accumulator, which stays in the next controller) of 0, 10⁴ and 10⁶ KAS, and over caller funding of 1, 10³ and 10⁷ KAS beyond the posted reserve. Large values remove the input relief term; a small caller output adds its own C/value.
- **(revised 2026-10-10)** The first version left O out. A large O removes input relief while the caller output stays small. The measured values now match the closed form R = max(4C(K+2)/L, 4C(K+1)/(L − 10,101)) to within 0.5%, and the test asserts this. The second term binds above about 3.96 KAS.

| K | Min R (KAS, rounded up), storage ≤ 500,000 | Min R (KAS, rounded up), storage ≤ 50,000 (10% of a block) |
|---|---|---|
| 1 | 0.2400 | 2.4000 |
| 4 | 0.4800 | 5.0126 |
| 8 | 0.8000 | 9.0212 |
| 16 | 1.4400 | 17.0358 |
| 32 | 2.7200 | 33.0579 |
| 64 | 5.3058 | 64.9351 |
| 128 | 10.5208 | 127.3886 |

(revised 2026-10-10: K = 64 and 128 in the first column, and K ≥ 4 in the second, rose once O was included.)

**(revised 2026-10-10)** `draw()` creates one fresh R-output per pool, plus the per-draw settlement record (design 5.4). It also pays one plain refund of exactly R to the funder of the marker and of each absorbed pool, plus the fee and caller outputs. Its condition is therefore 4C·(pools + 1)/R + C·(1 + absorbed)/R + C/fee payout + C/caller output − relief ≤ 500,000. R must satisfy whichever of `close()` and `draw()` is tighter. Check this again when the tier table is fixed (M3).

**Minimum claim payout** (`tests/m0_mass.rs::q4_minimum_claim_payout`). The test uses R = 1.44 KAS (K = 16) and d = 0.0844 KAS (the worst-case minimum), with the claim taking the pool down to exactly R. It takes the worst case over submitter funding of 10 KAS and 10⁷ KAS:

| Prize (KAS) | 0 | 0.001 | 0.01 | 1 | 100 |
|---|---|---|---|---|---|
| Storage mass | 146,283 | 144,895 | 133,727 | 36,998 | 27,876 |

On a ticket's **last** claim the payout is the prize plus the deposit. Its smallest valid value is 0.0212 KAS, which is below the minimum deposit, so the last claim is always valid; the test asserts this. Only R for K = 16 was measured; a larger R only lowers the pool term.

**Non-final bundle claims (revised 2026-10-10).** An earlier claim on a bundle pays the prize alone, and the deposit stays in the ticket successor (design 7.4), so the prize needs a floor. At R = 1.44 and d = 0.0844 per combination, the minimum prize is 0.0426 / 0.0265 / 0.0236 KAS for b = 2 / 5 / 10. A prize of 0 would be a zero-value output, which consensus rejects. The M4 spec must define what a non-final claim with floor(slice/h) below this floor does (for example, defer it to the last claim). The first version of this section said "the prize itself has no lower bound", which holds only for last claims.

**Impact on the design.** The reserve decision (DECISIONS 2026-10-09) changes §5.5, §6 and §7.2–7.4. The design update is pending, owned by the user.
- The ticket deposit becomes a game parameter with a floor of about 0.085 KAS per ticket. The exact floor depends on R and the ticket price; it is 0.0844 KAS for K = 16 and price 0.1 KAS.
- Whether to require about 5 KAS, so that storage mass is not the steady-state throughput limit, is an economic choice for 4.6/14.
- The first buy on each fresh shard carries a fixed storage cost from the R-valued shard successor. That argues for a larger R than the `close()` minimum (M6).

**Remaining risks:**
- The numbers assume one shard input and P2PK funding. Wallets with many small funding UTXOs raise the input relief term, which only lowers storage mass.
- Fee-market behaviour under full blocks is not modelled.

## Q5. Roughly how many shard inputs can one `close()` transaction consume?

**Answer: about 23–62, depending on the final shard script size. The limit is transient mass (serialized size × 4 ≤ 1,000,000, i.e. about 250 kB; `consensus/core/src/mass/mod.rs:367`, `constants.rs:31`), not compute and not the 1,000-input cap.**

**Setup** (`tests/m0_mass.rs::q5_largest_k_for_close`):
- The controller probe is compiled with its unrolled shard loop set to K.
- It is executed in the engine with K real shard inputs and the funding input, all under committed budgets.
- For mass, each shard input is sized as a shard sigscript of S bytes with compute budget 1.
- S = 3,418 B is the measured `buy()` sigscript of the Merkle probe (redeem plus arguments). At `close()` a shard sends its redeem script and a selector, not its siblings, so about 2.67 kB would be the Merkle-only figure. 3,418 B is conservative, not a lower bound (revised 2026-10-10). 6,000 and 10,000 B are allowances for the full shard script.
- The largest K is found by binary search over K = 1..128, compiling and running the probe at every probed K.

| K | Controller redeem B | Controller script units | S = 3,418: compute / transient | S = 6,000 | S = 10,000 |
|---|---|---|---|---|---|
| 8 | 3,548 | 32,240 | 38,683 / 130,092 | 59,339 / 212,716 | 91,339 / 340,716 |
| 16 | 6,908 | 62,928 | 74,575 / 257,420 | 115,887 / 422,668 | 179,887 / 678,668 |
| 32 | 13,630 | 124,316 | 146,362 / 512,088 | 228,986 / 842,584 | over: 1,354,584 |
| 48 | 20,350 | 185,692 | 218,146 / 766,744 | over: 1,262,488 | over |
| 64 | 27,070 | 247,068 | over: 1,021,400 | over | over |

Largest K that fits:

| Shard sigscript S | Largest K |
|---|---|
| 3,418 B | 62 |
| 6,000 B | 37 |
| 10,000 B | 23 |

The controller script grows by about 420 B per shard because the loop is unrolled.

**Impact on the design.** This is consistent with §8.3 ("probably 16–64"). The real shard script will exceed the Merkle-only 3,418 B, so expect K in the 16–37 range. A `close()` near the limit takes most of one block's transient budget. K is fixed in M6 once the real shard script size is known.

**Remaining risks:**
- The real shard and controller scripts are not written yet.
- Each shard input carries its whole redeem script, so the shard script size directly sets K.
- Keeping the close-leg logic in the controller, not the shard, helps.
- A transaction near the block limits may be slow to be mined.

## Q6. Which rusty-kaspa release should we pin?

**Answer: v2.1.0 (`01b532e8`), released 2026-09-22.**

**Comparison.** v2.0.1 → v2.1.0 touches 311 files (+9,450 / −12,145).
- **No new hard fork, opcode or limit change.** The opcode tables are identical. v2.1.0 is a post-Toccata cleanup: Toccata is always active, and the `toccata_activation` parameter is removed (a v2.1.0 unit test asserts the override is rejected as an unknown field: `consensus/core/src/config/params.rs:796-798`). The design's activation score of 474,165,565 exists only in v2.0.1 (`rusty-kaspa-v2.0.1/consensus/core/src/config/params.rs:724`).
- **API changes:**
  - `EngineFlags` loses `covenants_enabled`; only `sigop_script_units` remains (v2.1.0 `crypto/txscript/src/lib.rs:123-131`).
  - The pre/post-Toccata script constants become single values (`lib.rs:78-80`).
  - `MassCalculator` is unchanged.
- **Mempool:** the pre-Toccata 100,000 standard-mass cap and the legacy fee path are removed.

**Appendix A against both releases.** Every value matches:

| Property | Value | v2.1.0 location |
|---|---|---|
| Inputs / outputs per transaction | 1,000 | `params.rs:615-616` |
| Script-public-key length | 10,000 | `:621` |
| Signature-script length | 250,000 | `:25` |
| Combined stack items | 244 | `crypto/txscript/src/lib.rs:77` |
| Block compute / storage / transient mass | 500,000 / 500,000 / 1,000,000 | `params.rs:626` |
| Minimum fee | 100 sompi per gram of max(compute, transient/2) | `mining/src/mempool/config.rs:20`; transient is 4 grams per byte (`consensus/core/src/constants.rs:31`) |

The remaining Appendix A items were cross-checked by the reading agent:
- Pruning depth 1,080,000: `max(lower_bound, BPS × PRUNING_DURATION)` with PRUNING_DURATION = 108,000 s (`consensus/core/src/config/bps.rs:96-108`, `consensus/core/src/config/constants.rs:73`) and 10 BPS.
- Checked 64-bit arithmetic: `opcodes/mod.rs:592-651`.
- All listed opcodes exist: `opcodes/mod.rs:874-1421`.

**Reasons to pin v2.1.0:**
- It is the latest release, more than two weeks old.
- It runs the same consensus rules after activation as v2.0.1.
- It has a simpler engine API.

**Impact on the design.** §2 should cite v2.1.0 and keep v2.0.1 only as the source of the activation score. This is the user's edit.

**Remaining risks:**
- The SilverScript compiler pins an intermediate revision. If a future SilverScript release targets a newer rusty-kaspa, re-run the suite.
- The local Rust toolchain is pinned to 1.94.0 because SilverScript requires it (`silverscript/Cargo.toml:18`).

---

## Q7. Which mass limits does the network enforce per transaction? (research G1–G3, added 2026-10-10)

**Answer: the block limits, per transaction. Storage 500,000 and transient 1,000,000 were tested at the boundary on testnet-10 against v2.1.0 nodes. Compute 500,000 follows from the source (`check_transaction_limits.rs:21`) but was not pushed to its limit. The 100,000-gram standard limit in KIP-9's text is not applied. M0's minimum deposit, reserve R and the largest K for 3,418 B shard scripts (62) hold under relay. The largest K for 6,000 / 10,000 B (37 / 23) follows from the verified transient limit but was not submitted. No `draw()` shape was submitted (deferred to M3). Full results with transaction IDs are in `docs/tn10-mass-check.md`.**

**Source.**
- The mempool checks compute and transient mass in isolation, and storage mass in context, against `mempool_block_mass_limits` (`mining/src/mempool/check_transaction_limits.rs:15-44`).
- The node fills that value from the consensus block limits (`config.block_mass_limits`, node daemon `daemon.rs:648-652`; `mining/src/mempool/config.rs:92-123`).
- Testnet-10, devnet and simnet use the same limits as mainnet (`consensus/core/src/config/params.rs:626, 683, 732, 772`).
- Standardness adds no mass cap (`check_transaction_standard.rs:29-88`).
- There is no dust rule anywhere in `mining/src`.

**Testnet-10.**

| Check | Result |
|---|---|
| Storage 99,000 / 101,000 / 499,000 | accepted and mined |
| Storage 501,000 | rejected: "storage mass of 501000 is larger than max allowed size of 500000" |
| Transient 109,896 / 989,896 | accepted and mined |
| Transient 1,009,896 | rejected: "transient … mass of 1009896 is larger than max allowed size of 1000000" |
| Plain output 0.019 KAS (storage 526,315) | rejected |
| Plain 0.02 / 0.1 KAS | accepted |
| Covenant output 0.08 / 0.1 / 0.4 KAS (storage 500,000 / 400,000 / 100,000) | accepted |
| `buy()` at d = 0.0844 KAS without a referral output, and with a 0.1 KAS referral output and a 0.1075 KAS ticket (storage 499,992 each) | accepted, mined within 1.5 s |
| `claim()`, last (146,259) and non-final at the minimum prize (499,976) | accepted |
| `close()` at K = 16 / 32 / 62 with M0's minimum R and script sizes (K = 62: transient 989,544, storage 479,758) | accepted, mined within 1.5 s. K = 16 and 32 (storage 498,657 / 495,013) hit the profile that sets R; K = 62 used O = 0 with large change, not its R-setting profile (local 499,862). |

The covenant probes on testnet-10 use padded scripts sized to M0's measured sigscripts and budgets (see `docs/tn10-mass-check.md`), so they test masses, not protocol logic.

**Local analysis** (`tests/m0_limits.rs`).
- **G2.**
  - A covenant-bound output costs 4× a plain output of the same value (40,000 vs 10,000 for 1 KAS).
  - KIP-9 input relief offsets little in `close()`. At M0's minimum R, typical callers use 74–87% of the storage limit. A large controller value with a 1 KAS caller output uses 96–100%. The profile that sets R uses about 100% by construction: O = 0 and large funding at K < 48, large O at K ≥ 48.
  - If a 100,000 per-transaction limit applied, R would need to be 7.56 / 14.68 / 27.99 KAS at K = 16 / 32 / 62.
- **G3.** Smallest single output with negligible relief:

| Output | Under 500,000 | Under 100,000 | Under 10,000 |
|---|---|---|---|
| Plain | 0.02 KAS | 0.1 KAS | 1 KAS |
| Covenant-bound | 0.08 KAS | 0.4 KAS | 4 KAS |

**Still applies.**
- **Fee priority.** The mempool ranks by fee over the normalized maximum of compute, transient and storage mass (Q4). A transaction near 500,000 storage needs about 0.5 KAS to rank at the minimum feerate under load.
- **Block share.** A transaction near any limit fills that dimension of a block on its own.
- **Wallet SDK cap.** The wallet SDK's transaction generator caps max(compute, storage) at 100,000 (`wallet/core/src/tx/mass.rs:25, 299-319`; `wallet/core/src/tx/generator/generator.rs:739-745, 839`). This is a client limit on transactions the generator builds; our builder constructs protocol transactions itself.

**Impact on the design.**
- §12's open question ("which mass limit governs") is resolved: the block limits apply per transaction.
- M0's reserve and deposit figures stand, corrected by the Q4 revisions above. The largest K stands: 62 was submitted, and 37 / 23 are inferred from the verified transient limit.
- M_ref ≥ 0.02 KAS (G3). The relay floor understates the fee storage-heavy shapes need under load.

**Remaining risks:**
- Results are for v2.1.0 nodes. A node release that adds a standard mass cap would invalidate them, so re-run `cargo run -p tn10 -- probe all` on each rusty-kaspa bump.
- Testnet-10 was idle, so these runs say nothing about inclusion times under load.
