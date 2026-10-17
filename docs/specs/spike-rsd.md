# Spike spec: refereed sequential delay (RSD) feasibility

Status: **approved 2026-10-17** with the answers in S6 (D128). Decision rows: D111 (mechanism), D127 (direction and the outline revisions). Outline: `docs/specs/seed-delay-outline.md`, sections 0 step 1 and 5.3. rusty-kaspa paths are relative to `refs/rusty-kaspa-v2.1.0`. Milestone entry in `docs/milestones.md` after approval.

Question: can RSD's on-chain parts run within D104's rule (every shape ≤ 90 % of each mass) with s ≈ 10⁶ steps per segment refuted in about 3 sequential hops, and how long does a near-full transaction take to be included on testnet-10, so that W_c is measured instead of assumed? Local engine plus testnet-10. No production covenant changes, no change to `docs/design.md` or the game rules.

## S0. Facts from the sources (checked 2026-10-17)

1. **Mass limits per transaction:** compute 500,000, storage 500,000, transient 1,000,000 (`docs/costs.md` header, confirmed on testnet-10 in `docs/tn10-mass-check.md`). These equal the per-block limits, so **a transaction near 90 % fills a block on its own**. Transient = 4 × size (`consensus/core/src/mass/mod.rs:367`, `consensus/core/src/constants.rs:31`), so 90 % means size ≤ 225,000 B.
2. **Compute mass** = size + 10 × output script bytes + committed budget (`consensus/core/src/mass/mod.rs:335-369`). One budget unit = 100 grams = 10,000 script units; the budget is a u16 (`consensus/core/src/mass/units.rs:4-7, 64`).
3. **Script limits:** signature script ≤ 250,000 B (`consensus/core/src/config/params.rs:25`), so the P2SH redeem and its arguments must fit; script ≤ 1,000,000 B, ≤ 1,000,000 opcodes, stack ≤ 244 items (`crypto/txscript/src/lib.rs:77-80`).
4. **Hash pricing:** `OpBlake3` and `OpSha256` 1 unit per input byte, `OpBlake2b` 2 (`crypto/txscript/src/opcodes/mod.rs:61-71`); pushed bytes are charged 1:1 (`crypto/txscript/src/runtime_resource_meter.rs`). One iterated step on 32 bytes ≈ 1 script byte and 64 units.
5. **No chaining inside a block:** a transaction cannot spend an output created in the same block (`consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:136-146`). Every sequential hop costs at least one block.
6. **Lineage:** KIP-20 covenant ids give non-forgeable lineage from a checked genesis (`refs/kips/kip-0020.md` §1–4).
7. **Relay fee:** 100 sompi per gram of max(compute, transient / 2); priority by max(compute, transient / 2, storage) (`docs/costs.md`; `crates/harness/src/lib.rs` `priority_fee`).

**Paper estimate (to be replaced by measurements).** A raw unrolled verifier of 200,000 `OpBlake3` steps: size ≈ 200 kB (redeem) + glue, transient ≈ 80–90 %; compute ≈ 225,000 (size) + 128,000 (12.8 M units) + 4,000 (P2SH re-hash) ≈ 71 %. So s = 10⁶ is about 5 chunk transactions. Each near-full transaction pays ≈ 0.45 KAS at the relay floor. A claim with N ≈ 5.7·10⁴ checkpoints (1.8 MB) is about 9–10 near-full transactions.

## S1. What the spike builds

All covenants are test-only probes in `covenants/spike_rsd/` (like `covenants/m0/`); nothing enters `crates/lottery/src` or a `mainnet` build.

1. **Verifier step, two forms** (`crates/lottery/tests/spike_rsd.rs`):
   - **R1 raw:** a hand-assembled script of n unrolled `OpBlake3` steps over a 32-byte value, checking the result against an argument. Find the largest n within 90 % on all three masses.
   - **R2 SilverScript:** the same as a SilverScript loop (or unrolled source), compiled with SilverScript v1.0.0. Record compile time, bytes per step, the per-iteration overhead against R1, and the largest n.
2. **Fan-out refutation prototype** (OV implication 2): four probe covenants.
   - `claim_stub` holds a root R over the checkpoints (index, value).
   - `ref_commit`: the refuter posts the true intermediate values m₀ = c_j, m₁ … m₄ and proves c_j at position j under R. It creates five chunk UTXOs bound to its own KIP-20 lineage.
   - `chunk`: verifies H^{s/5}(m_i) = m_{i+1} in one transaction (R1 or R2 form). It writes "chunk i ok" into a successor bound to the same commit.
   - `ref_final`: consumes the five ok-chunks and the claim stub. It checks they all come from one commit, that m₅ ≠ the claim's c_{j+1} at position j+1 under R, and pays out.
   - Three sequential hops: commit, the chunks in parallel, final.
   - **Attack tests, written first:**
     - a wrong chunk result;
     - a chunk from another commit;
     - a final with a missing or duplicated chunk;
     - a final where m₅ equals c_{j+1};
     - positions j and j+1 not adjacent, or under another claim's R;
     - a forged chunk UTXO outside the lineage.

     One mutant per check, killed by a test.
3. **Claim batch:** the largest number of 32-byte checkpoints per transaction when the script recomputes the batch's Merkle root from the bytes in the transaction (R1: a root without its data would let a claimant hide where it lied). Output: transactions per claim at the outline's N, and the claim-versus-refutation block-space ratio that review M2 requires (claims must cost more).
4. **CPU reference:** sequential Blake3 on 32 bytes with the `blake3` crate (portable and default backends) and an unrolled fixed-flags compress. Median of 5 runs on the development machine, recorded with load. This settles the 53 vs 61 ns gap and gives the honest-time figure.
5. **Testnet-10 probe `rsd-inclusion`** in `crates/tn10` (not in `all`). It submits plain P2SH spends whose redeem is the R1 verifier at the measured n, so no lottery covenant is needed:
   - single near-full transactions at the relay floor, 2× and 10× the floor: accepted, relayed, time from submit to acceptance (DAA and wall clock);
   - sequential chains of 3 and 7 hops: end-to-end time;
   - k near-full transactions submitted at once (self-made load): how inclusion time grows with k.

   Report p50, p99 and the maximum per case, with sample counts. Results go in `docs/tn10-mass-check.md`.
6. **Flood model** (OV implication 5): a table, not a covenant. For k = 5, 50 and 500 false claims it gives the attacker's cost (bonds, uploads), the honest side's cost (refutations), the near-full blocks each side needs, and the time the honest side needs at the measured inclusion rate. Testnet runs only the k of S6 Q2.

## S2. Measurements

Recorded in `docs/costs.md` as "Spike RSD":
- masses of R1, R2, `ref_commit`, `chunk`, `ref_final` and a claim batch;
- n per transaction for R1 and R2;
- compile time;
- stack headroom at the verify;
- fees at the floor and at priority;
- CPU ns per step;
- testnet inclusion latency per case;
- the flood table;
- W_c from the outline formula: (sequential hops + 1) × the measured worst-case inclusion latency + the honest recompute margin, alongside an analytic bound for an attacker miner with α = 0.45 that refuses to include refutations (each honest block includes them; time until enough honest blocks, at 10 BPS).

## S3. Verdict rule

- **Feasible:** every shape stays within 90 %, s = 10⁶ is refuted in ≤ 3 sequential hops, near-full transactions are relayed and included at the floor, and the measured W_c puts the draw at most 10 h after the commit (it must land before the next round's deadline in 24 h rounds, with room for the data window, design 9).
- **Feasible with changes:** one of these fails but a parameter change fixes it (smaller s, more chunks, priority fees, longer rounds). The spike names the change and its cost.
- **Not feasible:** near-full transactions are not relayed or included, or no s gives ≤ 90 % with a sound fan-out.

The verdict, the numbers and the resulting parameters (s, N, W_c, fees per round) become DECISIONS rows and update outline 5.1. The release gate is not closed by the spike.

## S4. Out of scope (spec or release gate)

- The flood defence itself: the claim registry with an open-claim count and rising bond (the "disagreeing claims block each other" rule). That is a spec item, and the spike only provides its costs.
- `commit_anchor()`, `finalize()`, drawer changes, the window re-derivation (D97, D98, D102), jackpot timing.
- Blake3 hardware synthesis (S6 Q3), the cryptographer review, audits.
- Any mainnet node or address.

## S5. Order of work

1. S1.4 CPU reference, S1.1 R1 and R2. These fix n, and everything else depends on n.
2. S1.2 attack tests first, then the probe covenants.
3. S1.3 claim batch.
4. S1.5 testnet probe.
5. S1.6 flood table.
6. `covenant-reviewer` on the probes and tests; DECISIONS rows; `docs/costs.md`.

## S6. Owner answers (2026-10-17)

- **Q1 Testnet budget: yes**, up to 250 KAS of faucet fees for about 200 near-full transactions (≈ 0.45 KAS each at the floor, some at 2× and 10×).
- **Q2 Flood on testnet: k = 5 real false claims**; 50 and 500 are modelled (S1.6).
- **Q3 Blake3 hardware synthesis: not in this spike.** A_max stays 25, provisional; the question goes to the cryptographer review on the release gate.
