# Sequential hashing hardware limit and hash-chain delay designs (status as of 17 October 2026)

Scope: research for the RSD candidate (y = H^T(X.hash), wrong y refuted on chain). Complements `seed-fairness-findings.md` and `production_vdf_deployments.md`; facts already cited there (Chia ASIC 3-4x, Autonomys 600 ps/round and 125-135 ps bound, VDF Alliance FPGA/ASIC, Unicorn, Bunz et al. read) are only referenced. Labels: **[P]** primary source, **[S]** secondary, **[M]** measured here today, **[E]** engineering estimate (mine, not sourced). Kaspa facts are quoted from `refs/` and marked "verified in refs/".

## Headline summary

- **No published latency figure exists for one SHA-256, Blake2b or Blake3 evaluation on a latency-optimised ASIC.** Every hash-hardware paper and every Bitcoin chip reports throughput. The A_max for sequential Blake3 therefore cannot be measured from literature; it can only be bounded by analogy to the one sourced logic-depth bound we have (AES round, Supranational for Autonomys).
- Derived range (inference, [E]): sequential Blake3 on a well-funded ASIC runs **about 3-10 ns per step, A_max about 5-18 against the 53 ns reference CPU**. The current planning value of 1 ns per step (A_max 50) would need one Blake3 half-round (4 serial 32-bit additions plus 4 XORs) in about 70 ps, which is below the 125-135 ps theoretical bound for one AES round. A_max 50 is therefore a very conservative ceiling; **A_max 25 is a defensible planning value with 1.4x margin over the high end of the estimate**.
- Honest compute time is simply **delta_min x A_max** whatever the hash (same CPU/ASIC ratio logic): 120 s x 50 = 100 min; x 25 = 50 min; x 10 = 20 min.
- Hash choice: Blake3 and SHA-256 cost the same on chain (1 script unit per input byte, verified in refs/); Blake2b costs 2x. SHA-256 has the smallest structural gap to hardware on the honest side (SHA-NI: 39 ns measured, a dedicated round unit already in the CPU), but it is the function with the largest existing ASIC industry. No clear winner on evidence; Blake3 recommended, SHA-256 as the shorter-delay alternative if the project accepts a SHA-NI assumption for watchers.
- Economics: Shang and Chen's rule T >= (delta/c) E[V] cannot be satisfied for a large jackpot (hundreds of thousands of seconds to years). RSD does not rely on it; its security is the private-race argument (findings 4.1). The paper's "s = 100" does not exist as an adversary bound; the s = 100 in the Boneh et al. survey is a processor count for proof generation.
- A STARK of the chain is no replacement for refutation today: Kaspa verifies only RISC Zero succinct and Groth16 proofs (verified in refs/), and the chain is about 1.2e11 hashes. It is a plausible later optional fast path (section 5).

---

## Question 1: Latency of one hash on CPU, GPU, FPGA, ASIC; A_max for sequential Blake3

### Takeaway
Latency data is scarce and almost never published for hashes. What exists: CPU latency (measured below), one low-cost FPGA Blake3 result (slower than a CPU), ASIC throughput-only numbers, and the AES critical-path bound. GPUs and FPGAs are dismissed for sequential work by every production project that looked (Autonomys). The only credible attacker is a custom ASIC; its step time is bounded below by logic depth, which we can estimate by analogy, not measure.

### Cited findings
**CPU, measured today [M]** (i7-13650HX, Rust release, `target-cpu=native`, 20 M sequential 32-byte steps; crates blake3 1.8, sha2 0.10 with SHA-NI auto-detection, blake2b_simd 1.0, aes 0.8 with AES-NI; scratch program in the session scratchpad, not in the repo):

| Function | ns per step | Per round |
|---|---|---|
| Blake3 (`blake3::hash`, 32 B) | 60.8 (earlier run in findings: 53) | 7 rounds: 7.6-8.7 ns |
| SHA-256 (SHA-NI) | 38.9 | 64 rounds: 0.61 ns |
| Blake2b-256 | 96.7 (findings: 92) | 12 rounds: about 8 ns |
| AES-128 block (AES-NI) | 7.23 | 10 rounds: 0.72 ns |

AES at 0.72 ns per round is consistent with the Supranational figure of about 600 ps per round at 5 GHz. [P] [Autonomys forum, Why AES for PoT, 12 Jul 2025](https://forum.autonomys.xyz/t/why-did-we-choose-aes-for-our-proof-of-time-pot/4864) (ASIC prototype 2.5-3x, lower bound 125-135 ps, 4-5x at best; already in `production_vdf_deployments.md`).

**FPGA, Blake3 [P]:** Sugier, Int. J. Electronics and Telecommunications 70(1), 2024, doi 10.24425/ijet.2023.147710 ([PDF](https://czasopisma.pan.pl/Content/130689)): Spartan-7 (low-cost), iterative Blake3 with one round instance: 44.4 MHz, 45 logic levels per round (one round per clock), so about 22.5 ns per round and about 158 ns per compression (core only), roughly 3x slower than the CPU. Pipelining the round raises Fmax (P2 91.9 MHz, P6 about 194 MHz) but a round then takes 2 to 6 cycles: pipelining raises latency per step (P6: about 31 ns per round). The paper states the Blake3 change from 10 to 7 rounds is the main speed gain and the fixed message schedule removes 16:1 multiplexers (about 40% area). It reports no ASIC.

**ASIC, throughput only [P]/[S]:** Henzen et al. BLAKE cores: 38.0 kGE at 15 Gbps and 79.0 kGE at 19 Gbps in 90 nm (BLAKE-32 and BLAKE-64) ([Aumasson BLAKE slides](https://www.aumasson.jp/blake/blake_slides2.pdf)); no latency. Bitcoin ASICs: BM1370 (5 nm) default about 525 MHz, about 1.0-1.2 TH/s per chip, per a community/reseller page ([solomining.de](https://solomining.de/en/blogs/news/bm1370-asic-chip-technical-specifications), [S]); Bitmain publishes no datasheet; pipeline depth and per-hash latency not found. A glossary ([D-Central](https://d-central.tech/from-concept-to-chip-the-art-and-science-of-asic-design-in-bitcoin-mining/), [S]) describes unrolled engines as a pipeline of more than 100 stages with one hash per clock: latency = depth x clock period, throughput decoupled. One hash per round-stage at about 0.5 GHz gives an upper bound of about 2 ns per round in shipping miners, which are tuned for J/TH, not latency (inference).

**Other chains' hardware experience (already cited):** Chia ASIC timelords 3-4x over the best CPU; Supranational/Autonomys AES ASIC 2.5-3x (prototype), at most 4-5x (bound).

**Solana PoH [S]:** sequential SHA-256 chain; verification parallel on a 4,000-core GPU ([Anza docs](https://docs.anza.xyz/consensus/synchronization)); a secondary page puts 12,500 to 12,800 hashes per tick at 160 ticks per second, about 2 million hashes per second, or 500 ns per hash ([GridPlus](https://docs.gridplus.io/solana/proof-of-history)); I could not confirm `DEFAULT_HASHES_PER_TICK` in the crate docs. The Solana whitepaper considers a faster ASIC only as an attack on finality timeouts by a colluding PoH generator, and finds it no worse than shared identity ([whitepaper section 5.13.5](https://solana.com/solana-whitepaper.pdf)). If the 2 M/s figure is right, PoH runs about 13x slower than the 39 ns SHA-NI step: it uses the chain as a clock with large headroom, not as a delay against a faster adversary.

### Inferences (all [E])
Logic-depth analogy. The only sourced critical-path bound is AES: 125-135 ps for one round (SubBytes, MixColumns, AddRoundKey), prototype ASIC 2.5-3x faster than the CPU's 600-720 ps. A Blake3 half-round (a column or diagonal layer of 4 G functions) has a critical path of 4 serial 32-bit adds (plus the message add, merged by carry-save) and 4 XORs, with rotations free in wiring: roughly 1.5-2x the gate depth of an AES round. Hence per half-round about 0.2-0.3 ns at the AES bound and about 0.5-0.7 ns at prototype grade (AES prototype ratio 2.5x worse than its bound). Fourteen half-rounds give:

| Scenario | Blake3 step | A_max vs 53 ns | vs 61 ns |
|---|---|---|---|
| Theoretical bound (AES-bound technology, exotic design) | 3-4 ns | 13-18 | 15-20 |
| Prototype/production-grade ASIC | 7-10 ns | 5-8 | 6-9 |
| Current planning assumption | 1 ns | 53 | 61 |

Cross-check from the CPU side: 53 ns / 14 half-rounds = 3.8 ns, about 18 cycles per half-round, which is the dependency chain of the G function at roughly one cycle per dependent operation. An ASIC removes the per-operation cycle quantisation (adder about 0.1 ns versus a 0.2 ns cycle) and the SIMD shuffles, which yields the 5-18x above; nothing in the structure gives 50x. The assumption this rests on: a Blake3 half-round cannot be much faster relative to an AES round than its gate depth suggests, i.e. no new circuit trick (for example redundant-representation arithmetic across the XOR) beats the 4-adder chain. Carry-save representations do not help across XORs because XOR needs resolved bits. This is the weakest link; it should be replaced by a synthesis estimate (section Gaps).

Why A_max 25 and not 18: (1) the AES lower bound itself is theoretical and Supranational quoted 8-26 M USD for 3-4x on AES, so an attacker for a multi-million-USD jackpot could afford one design; (2) cryogenic or overclocked operation (8 GHz CPU is 375 ps per AES round, 1.9x over stock) is not covered by the logic-depth argument. 25 leaves 1.4x over the bound at 18. The unmeasured 50 leaves 2.8x.

### Gaps
No published ASIC, FPGA or CPU single-hash latency figure for Blake3, Blake2b or SHA-256 beyond the Spartan-7 row; no Bitcoin-miner pipeline depth; no Supranational AES report text (attached zip in the Autonomys post not read); no synthesis result for Blake3 in any open PDK. Not found: any Blake3 ASIC literature (Alephium miners use Blake3 but I found no ASIC latency data).

---

## Question 2: Which hash minimises the attacker's advantage

### Takeaway
By structure, the function whose honest-side implementation is already close to a custom round circuit gives the smallest ratio: AES (AES-NI) and SHA-256 (SHA-NI). Between the functions Kaspa script can evaluate, SHA-256 has the smaller estimated A_max, Blake3 the smaller existing ASIC base. Kaspa script has no AES opcode.

### Cited findings
- Opcodes verified in refs/: `OpSHA256` (0xa8), `OpBlake2b` (0xaa), `OpBlake2bWithKey` (0xa7), `OpBlake3` (0xd9), `OpBlake3WithKey` (0xda) in `refs/rusty-kaspa-v2.1.0/crypto/txscript/src/opcodes/mod.rs` (lines 775-800, 1456-1470). Pricing `HashOpcodePricing`: Blake3 1, SHA-256 1, Blake2b 2 script units per input byte (same file, lines 55-69). A grep for "aes", "keccak", "sha3" in that file finds nothing: AES and Keccak are not available.
- Round counts: Blake3 7 rounds, Blake2s 10 per Sugier (Blake2b has 12, standard), SHA-256 64.
- Measured CPU steps above: Blake3 53-61, SHA-256 39 (SHA-NI), Blake2b 92-97 ns.

### Inferences [E]
| Hash | CPU step (honest) | ASIC step estimate | A_max range | On-chain cost per step |
|---|---|---|---|---|
| SHA-256 | 39 ns (with SHA-NI; several times slower without, not measured here) | 64 rounds x 0.12-0.25 ns = 8-16 ns (one adder plus carry-save per round) | 2.5-5 | 1 unit/byte |
| Blake3 | 53-61 ns | 3-10 ns | 5-18 | 1 unit/byte |
| Blake2b | 92-97 ns | 24 half-rounds with 64-bit adders: 7-20 ns | 5-13 | 2 units/byte |
| AES (not available) | 7.2 ns | 2-3 ns | 2.5-4.5 [P] | n/a |

- Blake2b has no advantage in ratio and costs 2x on chain: discard.
- SHA-256: the lowest estimated A_max, so the shortest honest time for a given delta_min (delta_min x A_max: 120 s x 8 = 16 min). Costs: (a) watchers need SHA-NI (Intel since Ice Lake client and Alder Lake, AMD Zen; not verified here, software SHA-256 is slower, not measured); (b) the world's best-funded ASIC design teams and fabs already build SHA-256 cores at 3-5 nm, which lowers the attacker's design cost; my estimate of 8-16 ns could be pessimistic for them. Bitcoin chips are throughput-tuned, so this is a risk, not a measurement.
- Blake3: larger gap on paper, but a fresh design, no mass-market ASIC base found. The attacker's cost of a bespoke chip (8-26 M USD per Supranational for AES-class 3-4x) is the same order as any hash.
- The honest time does not depend on the hash beyond A_max: honest = delta_min x A_max.

### Gaps
No synthesis comparison of the three round circuits at one node exists in what I found. SHA-NI latency per `sha256rnds2` not verified in vendor tables. Blake3 in an unrolled single-compress implementation (fixed flags, no API overhead) might beat 53 ns on a CPU; unmeasured.

---

## Question 3: Production hash-chain delays and how they handle faster hardware

### Takeaway
Everybody who uses a sequential function for security (not as a clock) either keeps the delay far above the bounded hardware advantage, injects fresh entropy so speed gains do not accumulate, or publishes the design so that the fastest hardware is common. None uses a one-shot ceiling like RSD's A_max without a recalibration mechanism.

### Cited findings
- **Solana PoH:** a SHA-256 chain used as a clock; generation sequential on one core, verification parallel; the doc says the chain "does not serve as a good source of randomness" ([Anza docs](https://docs.anza.xyz/consensus/synchronization), as quoted in `production_vdf_deployments.md`). Parameter (hashes per tick) is a cluster constant; I found no recalibration mechanism and no ASIC-speedup analysis beyond the whitepaper note above.
- **Autonomys proof-of-time [P]** ([Academy](https://academy.autonomys.xyz/autonomys-network/consensus/proof-of-time), fetched 17 Oct 2026): 183 M AES iterations per slot targeting about 1 s on high-end CPUs; "continuously monitor hardware capabilities and adjust the target"; the delay function "should be benchmarked on the best available hardware" so nobody evaluates it meaningfully faster; PoT progress is monitored against wall-clock time to detect faster timekeepers; 8 checkpoints for 7x parallel verification and 4x less power; entropy injection from a deep consensus header every 50 blocks (about 5 min), which resets a faster timekeeper's lead; lag r = 4 slots; no numeric safety margin published. No recalibration algorithm is described.
- **Bunz, Goldfeder, Bonneau, "Proofs-of-delay and randomness beacons in Ethereum", IEEE S&B 2017 [P]** (full text read in the earlier session, not re-fetched): iterated-hash delay with an interactive challenge game; its own timing remark for 2^40 Keccak is inconsistent with its Table I (see findings section 6). Not re-verified here: its hardware assumption. Parameters chosen by an assumed ratio between commodity and ASIC speed.
- **Ethereum minimal VDF beacon [P]:** A_max = 10 epochs justified by a 20-30 M USD ASIC budget; open-sourced hardware (already cited).
- **Unicorn (Lenstra, Wesolowski 2015) [P]:** ties the delay to a wall-clock guarantee of 2 min (overclocked 9 GHz case: about 3 min); no recalibration (already cited).
- **Chia:** relies on the same open-hardware argument (ASIC timelords, 3-4x), with security parameters expressed as a speed ratio (42.7% / 33.3% netspace thresholds).

### Inferences
Autonomys's two defences map onto RSD as follows: entropy injection does not apply (RSD runs one chain once per round, so there is nothing to accumulate), and recalibration is a governance action. RSD can write A_max and T into the round parameters (params covenant) so that each round states T absolutely and the owner or later versions re-set it as hardware evidence changes; a round's T is absolute attacker-time bound, not a speed ratio, which is the safe direction.

### Gaps
No source gives a recalibration procedure for any deployed hash-chain delay. Bunz et al.'s adversary-speed assumption was not re-read. Solana's current hashes per tick not verified.

---

## Question 4: Proofs of sequential work and what they give (unique output?)

### Takeaway
None of the proof-of-sequential-work (PoSW) constructions gives the lottery what it needs. They prove that sequential time was spent on a statement; they do not make the final label unique, and in the random oracle model a VDF with a cheap verifier and a unique output provably cannot exist. RSD's optimistic refutation is consistent with that: its worst-case verifier cost equals the work of one segment, and cheapness comes from incentives, not from the function.

### Cited findings
- **Mahmoody, Moran, Vadhan, "Publicly verifiable proofs of sequential work", ITCS 2013** ([Harvard](https://salil.seas.harvard.edu/publications/publicly-verifiable-proofs-sequential-work), [ePrint 2011/553](https://eprint.iacr.org/2011/553)) [P/S]: built on depth-robust graphs, about N space; the proof shows time was spent on a statement chi; no unique output. Same authors showed (CRYPTO 2011) that time-lock puzzles whose generator also computes the solution are impossible in the random oracle model (per the search summary [S]).
- **Cohen, Pietrzak, "Simple Proofs of Sequential Work", Eurocrypt 2018 (best paper)** ([ePrint 2018/183](https://eprint.iacr.org/2018/183)) [P/S]: Merkle-tree labelling graph (labels hash the left siblings on the path, statement mixed in), log N space prover possible, verifier spot-checks openings. The abstract (as returned) states that MMV's open problem of unique proofs, where even a cheating prover can only produce one accepting proof, is not solved. The compressed-oracle paper ([arXiv 2010.11658](https://arxiv.org/pdf/2010.11658), [S]) notes the labelling itself is unique and takes N sequential queries.
- **Guan, Riazanov, Yuan, "Breaking Verifiable Delay Functions in the Random Oracle Model", CRYPTO 2025** ([ePrint 2024/766](https://eprint.iacr.org/2024/766)) [P]: VDFs with imperfect completeness and non-adaptive computational uniqueness cannot exist in the pure random oracle model; follow-up Abusalah et al., "Impossibility of VDFs in the ROM: The Complete Picture" ([ePrint 2025/1773](https://eprint.iacr.org/2025/1773), Eurocrypt 2026) closes the private-coin setup case. Earlier: Mahmoody, Smith, Wu (ICALP 2020) ruled out perfect completeness and adaptive perfect uniqueness.
- Continuous VDFs (Ephraim, Freitag, Komargodski, Pass, Eurocrypt 2020): constructed from repeated squaring, not hashes; not read here.

### Inferences
- PoSW gives soundness: an accepting proof implies about N(1 - small) sequential work. A prover who corrupts a small fraction of labels still passes with high probability, so many different roots are accepted; if the root feeds the seed, the prover grinds among them. The root is not a unique output. This is an inference from the sampling structure of the spot-check verifier; I did not find the sentence in the paper.
- A hash-only unique output with an efficient (poly-log) verifier is ruled out by the ROM results for the computational-uniqueness setting; any scheme must either verify by recomputation (RSD: refuting a segment costs s steps), use structured assumptions (class groups, RSA: Chia, Tezos) or a SNARK/STARK (section 5). RSD is the first kind with fraud proofs, as Bunz et al.
- What PoSW could add to RSD: nothing for uniqueness; possibly cheap evidence that a claimant did the work, but the claim is already bonded and refutable.

### Gaps
Cohen-Pietrzak full text not re-read; the grinding argument above is mine.

---

## Question 5: SNARK/STARK proving of long hash chains

### Takeaway
Off-chain proving of 1.2e11 hashes is now cheap in cloud terms (hundreds of core-hours with the newest binary-field provers), but the proof systems that reach those rates are not what Kaspa verifies. Kaspa L1 accepts RISC Zero succinct and Groth16 only, and the SHA-256 path through RISC Zero has no sourced throughput.

### Cited findings
All throughputs are for batches of independent hashes, self-reported, Apple M4 Max.

| System | Rate | Hash | Proof, verifier | Source |
|---|---|---|---|---|
| Flock (Bunz, Rothblum, Wang, arXiv 2607.27491, 29 Jul 2026) | 82k compressions/s on 1 core; over 660k on 10 cores; SHA-256 about 42k/s per core | Blake3 compression | under 450 KB per 2^30 AND gates; few ms | [arXiv](https://arxiv.org/html/2607.27491) |
| BinarySpartan (Setty, ePrint 2026/1656, 11 Aug, rev. 3 Sep 2026) | Blake3 872k/s; SHA-256 401k/s; Keccak-f 287k/s on 12 P-cores, incl. witness | Blake3 | polylog; not stated | [ePrint](https://eprint.iacr.org/2026/1656) |
| Osadnik, Seiler (ePrint 2026/2103, Sep 2026) | over 67k compressions/s per core | Blake3 | under 100 KiB with LaBRADOR; verify under 40 ms | [ePrint](https://eprint.iacr.org/2026/2103) |
| Stwo (StarkWare, 2024) | 500k-600k Poseidon2 hashes/s (4-core i7; 12-core M3 Pro); Blake2s only as commitment hash; third-party figure of 2.9k Blake2s compressions/s single thread quoted in Flock [S] | Poseidon2 | n/a | [StarkWare](https://starkware.co/blog/starkware-new-proving-record/) |
| RISC Zero | SHA-256 accelerator about 68 cycles per 64-byte block (+6 setup); no proving cycles/s found | SHA-256 | succinct/Groth16 | [RISC Zero optimization guide](https://www.mintlify.com/risc0/risc0/advanced/optimization) ([S]) |

Kaspa verification, verified in refs/: `OpZkPrecompile` (0xa6) with tags `Groth16 = 0x20` and `R0Succinct = 0x21` only (`refs/rusty-kaspa-v2.1.0/crypto/txscript/src/zk_precompiles/tags.rs`; `refs/kips/kip-0016.md` lines 35, 91, 101). Priced at `Gram(1000*140)` and `Gram(1000*250)` respectively (`tags.rs`). The KIP states Stwo was judged not mature enough and is "the most promising candidate" for a later tag (`kip-0016.md` line 71); SP1 was deferred. KIP-16 benchmark: Groth16 1.65 ms, STARK 9.08 ms per verification (line 121).

### Inferences (arithmetic on the cited rates; [E] where marked)
For T = 1.2e11 (or 5.7e10 at A_max 25):
- Binary-field provers: 1.2e11 / 872k per s = 1.4e5 s = 38 h on one M4 Max (12 cores); at 82k/s per core, 1.46e6 core-seconds = about 406 core-hours. At 67k/s: about 497 core-hours. With 100 machines: well under an hour to a few hours. Cloud price about tens of USD at typical core-hour prices [E, unsourced rate].
- A chain is sequential but the prover can split it into segments between published checkpoints (the honest evaluator has them), each a public-input/public-output batch instance; this makes the batch benchmarks applicable [E]. One would still need to aggregate roughly 1e5 segment proofs.
- Kaspa side: none of the three fast systems has an L1 verifier. A route is to run the segment verifier inside RISC Zero and submit a succinct receipt (verifier cost inside the zkVM not found). Directly proving SHA-256 steps in RISC Zero: at least 68 cycles per step plus loop overhead, i.e. at least 8e12 cycles for T = 1.2e11, about three orders of magnitude above typical zkVM jobs; no proving cycles/s figure found, so no cost can be stated. (At 1 M cycles/s per GPU, a placeholder, it would be 93 GPU-days; at 10 M, 9 days.)
- STARK proof does not shorten the draw: someone still computes the chain (about delta_min x A_max) and then proves, so the draw cannot come earlier than the honest compute time. It removes the "one honest watcher" assumption and the refutation window W_c.

### Gaps
No benchmark of a sequential (dependent) chain proof; no RISC Zero GPU cycles/s for a SHA-256-heavy guest; no Kaspa script cost for verifying a Binius/Flock/BinarySpartan proof (no tag). Whether `R0Succinct` pricing allows a proof in a transaction under the mass limits: to verify in refs/.

---

## Question 6: Economic sizing (Shang and Chen, Boneh et al. s = 100)

### Takeaway
Shang and Chen's rule makes the required delay grow linearly with the value at stake. Applied to a lottery with a large jackpot it demands impractical delays, so RSD cannot, and does not, rest on it. The "s = 100" is a processor count in a proof-generation remark of Boneh et al., not a security margin.

### Cited findings
**Shang, Chen, "Economic Security of VDF-Based Randomness Beacons: Models, Thresholds, and Design Guidelines"**, arXiv 2604.04744v1, 6-7 Apr 2026, CCS '26 ([PDF](https://arxiv.org/pdf/2604.04744), [HTML](https://arxiv.org/html/2604.04744v1)) [P]. Formulas below are from an automated extraction of the HTML; I confirmed the text of Corollary 5.2 and the case-study numbers in the PDF text, but math glyphs were lost in `pdftotext`, so check against the paper before quoting.
- Model: attacker speedup delta >= 1 (the same sequential work takes T/delta), cost rate c USD per second, reward V; profit = 1_success x V minus integral of a_t c dt; one full evaluation costs c T / delta. Economic security (Def. 4.2): E[profit] <= 0 for every admissible strategy.
- Corollary 5.2 (linear threshold): secure iff **T >= (delta / c) E[V]**.
- Theorem 5.4 (interval-robust): delta in [delta_min, delta_max], c in [c_min, c_max], V <= V_max gives **T >= (delta_max / c_min) V_max**. Theorem 5.5 (Chebyshev): T >= (delta_max / c_min)(mu_max + sigma_max / sqrt(eps)).
- Composition: m protocols sharing one beacon, T >= (delta/c) sum_j mu_j (Theorem 5.6); n rounds, T >= (delta/c) max_k (1/k) E[sum of k rewards] (5.7).
- Extensions: grinding over G parallel candidates T >= delta E[V_max] / (c G) (6.1); selective abort with leverage p, T >= (delta/c) E[V]/(1-p) approximately (6.2); coalition of m players sharing hardware cost: m times the bound.
- Case study: delta = 3, c = 0.05 USD/s gives T* approximately 60 E[V] s: E[V] = 10 USD needs 600 s, 100 USD needs 6,000 s (about 100 min); the Ethereum-style case delta = 2.5, c about 0.00046 USD/s, median MEV 50 USD gives about 271,739 s (about 3.1 days), 10,000 USD about 629 days. Their conclusion: delays of "a few seconds" are economically insecure; step 5 of the guidelines: periodically recalibrate T. ESDP = Economically Secure Delay Parameter (Def. 8.1).
- Limit of the model (their own simplification): the attacker's alternative is "finish before the honest evaluation or not at all"; proofs of Theorems 5.1 and 5.5 are sketches.

**Boneh et al. s = 100 [P/S]:** the phrase "Taking s = 100 seems reasonable in practice" appears in the Boneh, Bunz, Fisch survey ([A Survey of Two Verifiable Delay Functions Using Proof of Exponentiation](https://crypto.stanford.edu/~dabo/papers/VDFsurvey.pdf), Stanford PDF, date not in the extracted text) where s is the number of processors (stored group elements) used to speed up Wesolowski proof generation by a factor s. I did not find an adversary-speed parameter "s = 100" in it, and could not read the CRYPTO 2018 paper (eprint.iacr.org blocks automated fetch, ePrint 2018/601): not found.

### Inferences
- Mapping to the lottery: the attacker who learns y early gains V = value of choosing between the two outcomes (order of its ticket stake gain, up to the jackpot scale). Applying T_att = T/delta >= E[V]/c to V = 100,000 USD and c = 0.05 USD/s needs an attacker-side time of 2e6 s (23 days) for a purely compute-priced attack; for V = 1 M USD 2e7 s. This is unreachable with an hours-long T. Two points rescue RSD: (i) the compute cost is not the barrier; the attacker must hold block X privately for delta_min while racing the honest chain (findings 4.1: catch-up probability falls from 0.53 at alpha = 0.49 with 120 s to 2.4e-5 at alpha = 0.45); (ii) the rent-an-ASIC model assumes a seller; an ASIC owner pays c only on use. So the correct statement of security is the race plus an honest-majority bound, not the economic inequality. The economic inequality does say something real in one regime: a hash-chain delay cannot protect jackpots against an attacker that can hold X for free, e.g. one that need not race (a non-PoW anchor); for a PoW anchor the free hold is limited to about 12 s (findings section 3).
- Scaling rule worth keeping: delta_min must exceed the longest private hold the attacker can afford without losing the race, and that depends on alpha, not on V. V enters only through the choice of alpha-bound (which attackers are worth worrying about). Recalibrate T when the evidence on A_max changes, as Shang-Chen step 5 and Autonomys suggest.
- Theorem 6.1 (grinding) does not apply directly: each X admits one chain; an attacker with several private candidate blocks X_i has G chains in parallel, each needing its own ASIC; V_max and G enter the same way, so the linear inequality becomes G-fold easier for the attacker only if it can hold G blocks, which the race argument also prices.

### Gaps
Authors' text for Theorem 5.4 conditions beyond the extraction not verified; the PDF text was not read end to end. The CRYPTO 2018 Boneh et al. paper not read.

---

## Implications for RSD

1. **A_max.** Recommend **planning A_max = 25 for sequential Blake3 (range of evidence 5-18 against the 53 ns reference CPU)**. Evidence: (a) logic-depth analogy to the sourced AES bound of 125-135 ps per round, 4-5x over a 600 ps CPU round, 2.5-3x for a real ASIC prototype ([Autonomys forum](https://forum.autonomys.xyz/t/why-did-we-choose-aes-for-our-proof-of-time-pot/4864)); (b) Chia timelord ASICs 3-4x; (c) the Spartan-7 FPGA row showing FPGAs are slower than CPUs; (d) the CPU 18-cycle dependency chain per half-round. It rests on one assumption: no circuit trick makes a Blake3 half-round cheaper than about 4 serial 32-bit adds in gate depth. A_max = 50 (current) is a safe ceiling, A_max = 10 is the evidence centre; going below 25 should wait for a synthesis result.
2. **Hash.** Keep **Blake3**: on-chain cost equal to SHA-256 (1 unit per byte) and half of Blake2b, 7 rounds, no mass-market ASIC base found. SHA-256 is the shortest-honest-time alternative (estimated A_max 2.5-5 with SHA-NI, honest 39 ns) but concentrates risk where ASIC design capacity is largest, and assumes SHA-NI for every watcher; decide only after a synthesis comparison. Blake2b: no.
3. **T and honest time at delta_min = 120 s** (reference step 53 ns, attacker step = 53/A_max ns, T = 120 / attacker step, honest = delta_min x A_max):

| A_max | Attacker step | T | Honest CPU time |
|---|---|---|---|
| 10 | 5.3 ns | 2.3e10 | 20 min |
| 18 | 2.9 ns | 4.1e10 | 36 min |
| **25** | **2.1 ns** | **5.7e10** | **50 min** |
| 50 (current) | 1.06 ns | 1.1e11 to 1.2e11 | 100 min |

   Recommended: T = 5.7e10 (about 50 min honest, refutation segments unchanged, N = T/s checkpoints about halved to 5.7e4 at s = 1e6, about 1.8 MB). This brings the draw from about 5.6 h after close to roughly 3-4 h (the 1.8 h chain term becomes 0.8 h; other terms W_c, bounds unchanged: re-derive per findings 4.4). If the owner prefers to keep 50, the cost is 50 extra minutes. If the honest reference used 61 ns, scale honest time by 61/53.
4. **STARK later?** Not as a replacement for refutation in the near term. Off-chain proving rates are good enough (about 400-500 core-hours for 1.2e11 Blake3 steps at the newest published rates), but Kaspa L1 verifies only RISC Zero succinct and Groth16 (`refs/rusty-kaspa-v2.1.0/crypto/txscript/src/zk_precompiles/tags.rs`, KIP-16), the fast provers have no L1 verifier, and the RISC Zero route for a 1e11-step chain has no sourced cost. A STARK-backed fast path is realistic later as an optional finalisation route (a verified receipt finalizes at once; unproved claims keep the refutation window), if Kaspa adds a Stwo/Binius tag. It would remove the honest-watcher assumption but not the honest compute time. Revisit when (a) a new tag appears or (b) a RISC Zero cycles/s figure for SHA-256 chains is published.
5. **Next measurements (cheap):** (i) synthesise a Blake3 compress core, a SHA-256 core and a Blake2b core with an open PDK at one predicted advanced node (for example ASAP7 with OpenROAD) and report critical path in ps: turns the 5-18 estimate into a number; (ii) benchmark an unrolled fixed-flags Blake3 compress on the CPU to fix the reference step; (iii) read the Supranational AES report (zip on the Autonomys forum post) for the exact critical-path method; (iv) read Bunz et al. again for their adversary assumption.
