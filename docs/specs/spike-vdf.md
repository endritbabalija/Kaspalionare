# Spike spec: VDF+ZK seed feasibility

Status: **approved 2026-10-13** with the answers in S5. Milestone entry: `docs/milestones.md` "Spike — VDF+ZK seed feasibility (before M8)". Decision rows: DECISIONS 2026-10-13 "D100 amended" and "VDF+ZK feasibility spike". rusty-kaspa paths are relative to `refs/rusty-kaspa-v2.1.0`, SilverScript to `refs/silverscript`.

Question: does seed = H(VDF(X.hash)), with the VDF evaluation proven by `OpZkPrecompile`, fit inside the worst M7 `draw()` under D104 (every shape ≤ 90 % of each mass, drawer stack headroom ≥ 4 slots at check 24, the compiler accepts the drawer)? Local engine only, no testnet, no change to any production covenant.

## S0. Facts from the sources (checked for this spec)

1. **Opcode and price.** `OpZkPrecompile` (0xa6, `crypto/txscript/src/opcodes/mod.rs:759-771`) pops a tag byte, charges `tag.cost()`, verifies, pushes true. Groth16 (tag 0x20) costs 140,000 grams and R0 succinct (0x21) 250,000 (`zk_precompiles/tags.rs:29-32`). **Groth16 also charges 250,000 script units per verifying-key `gamma_abc_g1` element** (`zk_precompiles/groth16/mod.rs:18, 46-49`), i.e. 2,500 grams per element at `SCRIPT_UNITS_PER_GRAM = 100` (`consensus/core/src/mass/units.rs:6`), and there are n + 1 elements for n public inputs. So Groth16 costs **140,000 + 2,500 (n + 1)** grams. The "about 69 %" of the milestone entry omits this term.
2. **Stack interface.** Groth16 pops, from the top: VK (compressed), proof (compressed), n, then n public inputs as 32-byte field elements (`groth16/mod.rs:69-117`); trailing bytes are rejected. R0 succinct pops 8 items: claim, control index, control digests, seal, journal, image id, control id, hash-function id; only Poseidon2 is accepted (`risc0/mod.rs:80-136`).
3. **SilverScript builtins** (`silverscript-lang/src/compiler/builtin_types.rs:84-89`, `compile/expression/builtin.rs:296-335`): `g16.verify(vk, proof, inputs…)`, `r0.g16.verify(journal_hash, proof, image_id)` (RISC Zero receipt wrapped in Groth16; appends the zk-sdk fragment `append_r0_groth16_verifier_dynamic_image_id`, `crypto/txscript/zk-sdk/src/zk_to_script/fragments/groth16.rs:126-`, which rebuilds the receipt-claim digest with two `OpSHA256` and calls Groth16 with 5 public inputs), `r0.succinct.verify(…)`. Compiled examples verify in SilverScript's own tests (`tests/examples_tests.rs:371-412`).
4. **Sizes.** A compressed BN254 Groth16 proof is 128 B (the fixture in `zk_precompiles/tests/helpers.rs:257`). The R0 succinct test seal is **222,668 B** (`zk_precompiles/tests/data/succinct.seal.hex`). Transient mass = size × 4 (`consensus/core/src/constants.rs:31`, `mass/mod.rs:367`); limits compute 500,000, storage 500,000, transient 1,000,000 (`docs/costs.md`).
5. **No activation gate in the local engine:** the verifier lives in `kaspa-txscript`, which `crates/harness` already links.
6. **Baseline** (DECISIONS D104 / M7 measured row): worst `draw()` 206,523 / 527,012 / 71,009 = 41.3 / 52.7 / 14.2 % (compute / transient / storage); drawer redeem 64,629 B; 4 free stack slots at check 24 (at the floor).

**Lower bounds on paper (to be replaced by measurements):**

| Variant | Opcode grams | Worst `draw()` compute | Transient | Paper verdict |
|---|---|---|---|---|
| V1 plain Groth16, n = 4 (X.hash and y each split into two 16-byte halves) | 152,500 | ≥ 359,023 = 71.8 % | + ~2 kB × 4 (proof 128 B, VK ~392 B, glue) ≈ 53.6 % | fits on paper |
| V1′ plain Groth16, n = 2 (X.hash and y truncated to 31 B) | 147,500 | ≥ 70.8 % | as above | fits on paper |
| V2 RISC Zero over Groth16 (`r0.g16.verify`, n = 5) | 155,000 | ≥ 361,523 = 72.3 % | as above | fits on paper |
| V3 RISC Zero succinct | 250,000 | ≥ 456,523 = 91.3 % | ≥ 527,012 + 890,672 = **141.8 %** | does not fit, not even alone in a split `draw()` (transient ≥ 89 % from the seal) |

## S1. What the spike builds

1. **Drawer variants, test-only, by source patch at test time** (the mechanism of the short-void variant, `crates/lottery/src/buy.rs:134-139`): the production `drawer.sil` stays byte-for-byte unchanged. The patch (a) adds the verify call bound to X.hash, which the anchor proof already authenticates, and to the claimed output y passed in the sigscript; (b) replaces the seed input X.hash in D94's `blake2b("KLOT/seed" ‖ cov ‖ round ‖ ·)` by y. Variants V1 (n = 4, and n = 2 if V1 is tight), V2, V3. No variant enters a `mainnet` build (same `cfg` gate as the short-void variant, checked by SP12's symbol test).
2. **Proofs.**
   - V1: a real Groth16 proof generated locally with `ark-groth16` (the version rusty-kaspa v2.1.0 links) for a small **MinRoot**-style circuit over BN254 Fr (evaluation takes fifth roots, sequential; the circuit checks fifth powers, cheap). Public inputs = X.hash halves and y halves. The iteration count T is small in the spike: the proof, VK and on-chain cost of Groth16 do not depend on T. Setup is a local, insecure one (test only; see Q2).
   - V2: the RISC Zero Groth16 receipt fixture that SilverScript's tests use (`r0_groth16.rcpt.hex`). Its journal is not ours, so the binding test is skipped and only the mass and stack are measured (the cost does not depend on the journal contents). The script still computes `journal_hash = sha256(X.hash ‖ y)` so the glue is counted. RISC Zero proving needs its toolchain, which does not run natively on Windows; the spike does not produce RISC Zero proofs.
   - V3: the succinct fixture in `zk_precompiles/tests/data/` (mass only, same reason).
3. **Tests** in `crates/lottery/tests/spike_vdf.rs` (not part of a milestone suite):
   - SV1 the worst M7 `draw()` (DP21 with the anchor) under V1 executes in the local engine and its `RoundResult` uses the seed from y;
   - SV2 attacks on V1, each rejected: wrong y (proof valid for another y), proof for another X.hash, a VK swapped in by the caller (the VK is a template constant, never a sigscript argument), truncated or padded proof (trailing bytes), a public input ≥ the field modulus, n mismatched against the VK;
   - SV3 V2 and V3 inside the worst `draw()`: executes (V2) and mass recorded (both; V3 is expected over 100 % transient);
   - SV4 the typical `draw()` and the worst void under each variant (the void path must not require a proof; the verify sits only on the draw branch);
   - one mutant per V1 check (drop the verify, drop the y-binding, take the seed from X.hash again) that a test must kill.
4. **Measurements** (`spike_vdf.rs --release -- --nocapture`), recorded in `docs/costs.md` as a "Spike VDF" section: compute, transient and storage mass of the worst and typical `draw()` and the worst void under each variant; drawer redeem size; stack depth and free slots at check 24 and at the verify call (`MAX_STACK_SIZE = 244`, `crypto/txscript/src/lib.rs:77`); the opcode's share of compute separately from the glue.

## S2. Verdict rule

- **Fits as is**: some variant keeps every `draw()` and void shape ≤ 90 % on all three masses, ≥ 4 free slots at check 24, and the compiler accepts it.
- **Fits only split**: no variant fits as is, but a two-transaction `draw()` would (step 1 verifies the proof and writes y into a new seed UTXO bound to the round; step 2 is today's `draw()` consuming it). The spike then estimates, without building it: the extra transaction's masses, the new covenant and its R2 shape, the extra fee, and the second race/void window it opens (R3: a deadline for step 2, what happens if only step 1 lands).
- **Does not fit**: neither.

The verdict, the measured numbers and the chosen variant (if any) become DECISIONS rows. The release-gate row (D100 amended) is updated but **not closed** by the spike: it also needs the items in S4.

## S3. Margin-floor question (D100 amended)

The spike computes, with `game::round_amounts`, `allowance`, `whale_allowance` and `fix_jackpot` (the code D100 amended used), the minimum ε the game-size rule allows (ordinary / self-referring / self-referring plus both caller rewards) for margin floors 1.00, 1.05, 1.10, 1.17, 1.25 and 1.30 at the fixture shares, and records the table. It does **not** change the `margin >= BP` check in `close()` and genesis: the floor is handed to the owner as a decision with the table, framed as: with a working VDF layer the floor no longer protects against the miner (the miner cannot see the outcome in time), so it is only a price question; without it, the floor sets the ε that rentable burst hashrate must beat.

## S4. Out of scope (stays on the release gate)

- Choosing T (the delay) and showing that it beats the withholding horizon with a hardware-speedup margin; the prover's wall time for that T against the opcode window (about 12 h, X must stay within F of the point of view) and the draw window. The spike reports only prover time at its small T and the scaling per iteration (see Q1).
- The security of MinRoot as a VDF (literature check, never from memory); a production trusted setup (Q2).
- Measuring rentable burst hashrate.
- Any testnet run, any change to the production drawer, `docs/design.md` or the game rules.

## S5. Owner answers (2026-10-13)

- **Q1 (prover time): yes.** Time the V1 prover locally at small T (T = 2^10 … 2^16), extrapolate per iteration, **and state whether the proving cost fits within the C_draw caller reward** (C_draw = floor(S × s_D / 10^4), `game::round_amounts`, `crates/lottery/src/game.rs:103`; s_D = 50 bp in the fixture). The comparison is reported as the break-even round sales S at which C_draw covers the proving cost, with the cost inputs (machine, hourly price, KAS/USD) stated as parameters, never from memory. The spike still does not choose T (S4).
- **Q2 (setup): measure both V1 and V2; the choice stays on the release gate.**
- **Q3 (V3): measure succinct once (SV3).**

## S6. Implementation notes (2026-10-13; results in `docs/costs.md` "Spike VDF+ZK", decisions D106, D107 and the margin-floor row)

- **Proof transport.** The VDF output and proof follow the 272-byte anchor inside the existing `anchor` argument (check 1's length test becomes `ANCHOR_LEN + VDF_LEN`), not new entry parameters: a parameter is a live binding across check 24, where the drawer has exactly the D104 floor of 4 free slots. Measured headroom: 4 at check 24 for every variant (the D104 floor, unchanged), 121–125 at the verify (ints declared inside the verify block).
- **V1 public inputs.** n = 4 (X.hash as two zero-padded 16-byte halves, x_T, y_T); the seed source is yS = blake2b(x_T ‖ y_T). V1′ (n = 2) was not needed: V1 is at 72.1 %.
- **V3.** As S0 predicted, but for a reason S0 did not list: the signature script would be 301,908 B, over the consensus limit `MAX_SIGNATURE_SCRIPT_LEN` = 250,000 (`consensus/core/src/config/params.rs:25`), so V3 cannot be encoded inside `draw()` at all. "Measure once" (Q3) was done standalone (one input, one output): 95.6 % compute, 89.4 % transient, so a split `draw()` would not fit it either. V1 and V2 were measured the same way for comparison.
- **SV2 n mismatch** is a property of the source, not an attack: n and the VK are template constants (SV2f checks the source; the precompile's arity check is `groth16/mod.rs:42-44`).
- **Attack budget.** The test builder gives a failing dry run 2,000,000 units (`buy.rs:42`), below the units of the drawer with the precompile (more than 15.25 M; the precompile alone is 14,000,000 + 5 × 250,000). The SV2 attacks and the mutant check therefore run under the honest draw's committed budget, so a refusal comes from the script (asserted: `ZkIntegrity` or check 1's `VerifyError`), not from the budget.
- **Case templates.** Every case is built against its own variant's templates (the controller's marker_tpl and the marker's drawer hash follow the drawer), including the mutant runs.
- **Mutants.** SM1 no verify, SM2 seed from X.hash, SM3 X inputs taken from P's parent_commit bytes. SM3 replaces S1.3's "drop the y-binding" mutant: the y bytes are both the public inputs and the seed source, so any change to the seed's source is caught by SV1. A mutant counts as killed only when the lie is accepted or the honest case is refused by a script error under the unmutated variant's honest budget.
- **Gate.** The variants live only in a test target (`tests/spike_vdf.rs`); nothing is in the library, so no `cfg` gate as S1.1 said is needed (equally unreachable from a `mainnet` build).
- **V2 fragment.** `r0.g16.verify` is expanded by SilverScript, which links rusty-kaspa rev a41a333; the fragment that runs is that revision's, not `refs/rusty-kaspa-v2.1.0/crypto/txscript/zk-sdk/.../groth16.rs` as S0 item 3 cites. It executes on the v2.1.0 engine.
- **Margin floor (S3).** Computed with the closed form of D100 amended (a) and `game::whale_allowance`, not by running `fix_jackpot` per margin; it reproduces the recorded 1.17 and 1.00 values.
- **Prover timing.** Median of 5 runs per T; load matters (ratio 3.4 alone, 7.0 under a concurrent test suite), both recorded.
- **Fixtures** (copied into `crates/lottery/tests/fixtures/spike_vdf/`): rusty-kaspa v2.1.0 `crypto/txscript/src/zk_precompiles/tests/data/succinct.*.hex` and SilverScript `silverscript-lang/tests/fixtures/r0_groth16.rcpt.hex`. Dev-dependencies added to `crates/lottery` (test-only): `ark-*` 0.6.0 (the versions rusty-kaspa v2.1.0 links), `kaspa-txscript-zk-sdk` at SilverScript's rusty-kaspa rev a41a333, `risc0-zkvm` 3.0.4 without default features, `borsh`.
