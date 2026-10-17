# Seed fairness: findings of the research session

Session of 2026-10-13, brief `docs/research/seed-fairness-brief.md`. No covenant or design changes. One adversarial review pass (general-purpose red team) and one literature check were run; their findings are folded in below. Decisions: D110–D112 in `docs/DECISIONS.md`. Spec outline: `docs/specs/seed-delay-outline.md`.

## 1. Answer

1. **No mechanism built only from public block data, without a delay or a secret, removes the anchor miner's option.** This is a known theorem, not only our failure to find one: Bentov, Gabizon, Zuckerman, "Bitcoin Beacon" (arXiv:1605.04559, 2016), Theorem 3.5: for *any* extractor over block data there is a "p-resettable" source (the attacker mines the next block with probability p and may discard it once) giving bias at least p/12. Every variant we tried confirms it (section 3).
2. **A delay removes it, and it can be checked on Kaspa L1 without ZK, without a ceremony, without a house and without anything the owner runs.** Seed = H(… ‖ y) with y = H^T(X.hash), T sequential Blake3 steps, so that nobody can know y while X's chain position can still change. y is posted by anyone with a bond together with on-chain checkpoints, and any wrong claim is refuted by a short permissionless chain of transactions that recomputes one segment on chain (section 4). This is the "proof-of-delay" construction of Bünz, Goldfeder, Bonneau (IEEE S&B 2017), with their interactive challenge game replaced by a non-interactive, slot-free refutation that Kaspa's script costs make possible (about 200,000 Blake3 steps per transaction).
3. **What it buys:** the anchor bias disappears for any attacker below about 45 % of hashrate (Kaspa's own security bound), whatever its ticket stake; the margin floor no longer has to price miner bias, so the ordinary player's expected loss stays about 27 % instead of the 37 % that the 1.5 floor costs.
4. **What it costs:** one new assumption (at least one honest party per round recomputes the hash chain, about 2 h of one CPU core, and refutes a wrong claim in time); one hardware assumption (nobody computes sequential Blake3 more than about 50 times faster than a desktop CPU); a draw about 5 h after the anchor instead of minutes; a few near-full transactions per round; and a new milestone of covenant work. It does not survive an attacker near or above 50 % of hashrate; neither does anything else on a PoW chain.

## 2. Facts (re-verified in `refs/`, 2026-10-13)

- k = 124 at 10 BPS (`refs/rusty-kaspa-v2.1.0/consensus/core/src/config/bps.rs:40`); blue rule `consensus/src/processes/ghostdag/protocol.rs:113-118`. A red block's subsidy and fees go to the merging block's miner (`consensus/src/processes/coinbase.rs:117-131`); a blue block is paid to the script in its own coinbase payload (`coinbase.rs:107-114`). Finality 43,200 s (`consensus/core/src/config/constants.rs:70`).
- KIP-21 (`refs/kips/kip-0021.md` §6.5.2–6.7): `SeqCommit(B)` also commits `MinerPayloadRoot(B)`, a Merkle root over every mergeset block X of `H(X.hash ‖ blue_work(X) ‖ H(coinbase payload of X))`, selected parent first; the payload carries the payout script (`coinbase.rs:144-160`). A covenant can therefore authenticate the hash and the payout script of every block a chain block merged (used in the anchor-bounty variant of section 3).
- Script costs: `OpBlake3` 1 unit per input byte, `OpBlake2b` 2 (`crypto/txscript/src/opcodes/mod.rs:61-71`); pushed bytes 1:1 (`crypto/txscript/src/runtime_resource_meter.rs`, `charge_newly_pushed_bytes`); no per-opcode base charge, only ≤ 1,000,000 opcodes (`crypto/txscript/src/lib.rs:80`); 100 units per gram (`consensus/core/src/mass/units.rs:6`). Compute mass = size + 10 × output script bytes + committed budget; transient = 4 × size (`consensus/core/src/mass/mod.rs:335-369`, `consensus/core/src/constants.rs:31`). One iterated `OpBlake3` step on 32 bytes is 1 script byte and 64 units; the transient limit binds, so about 200,000 steps fit one transaction under the 90 % rule, before the P2SH re-hash of the redeem (2 units per byte) and SilverScript's per-iteration overhead (unmeasured, review S1). Transactions cannot chain inside one block (`consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:136-146`). No mempool cap on signature-script size (`mining/src/mempool/check_transaction_standard.rs:17` caps only P2SH sig-ops).
- Measured on the development machine (i7-13650HX, Rust `blake3` 1.8, `blake2b_simd` 1.0, release): sequential Blake3 on 32 bytes **53 ns per step**, Blake2b-256 **92 ns**. No published latency measurement for a sequential-hash ASIC was found; the literature check's engineering estimate is 3–15 ns per step on a latency-optimised ASIC (10–30× a CPU, perhaps 50×). We assume at least 1 ns per step for any attacker (A_max ≈ 50); this is an assumption, not a measurement.

## 3. Result 1: without a delay or a secret the option cannot be removed

Whatever function of public blocks defines the seed, some block completes it, and its miner can evaluate the function before publishing; holding a block is free for about 12 s (k = 124) and costs one subsidy after. Bentov–Gabizon–Zuckerman prove the general bound (bias ≥ p/12 for any extractor); Bonneau–Clark–Goldfeder (ePrint 2015/1015) show that discarding blocks is the only way to bias a PoW beacon and that its price is the forgone reward, which on Kaspa is zero for 12 s; Pierrot–Wesolowski (Cryptogr. Commun. 2018) show that hashing the last n blocks does not help. Variants examined here:

| Variant | Effect | Why it fails |
|---|---|---|
| Several anchors / hash over a window | none | the last block's miner keeps the last word (D108 confirmed) |
| XOR over all blocks merged in a window | worse | the closing chain block's miner chooses which tips are merged: a free menu |
| Minimum-hash election over a long window | same α/(1−α) | the attacker drops its own bad candidates; cost ≈ α·ln N subsidies, negligible against a large stake |
| Rare "super-block" anchor, or any anchor at an unpredictable time | same | removes burst rental, not a sustained share |
| Anchor bounty ρ·sales paid to X's payout script (authenticable via KIP-21 §6.5.2) | partial | a drop now costs ρ·sales, so only holders of more than ρ/(1−ε) of the round re-roll; for a large holder the threshold α > ε is unchanged; costs players ρ |
| More EV in frequent prize tiers | small | for a large holder only rare-tier EV is re-rollable; moves the threshold a few points and shrinks the jackpot |
| Commit-reveal by players or the closer | none | the last revealer drops its reveal (D108 confirmed) |
| VDF with a SNARK proof | works | declined: ceremony and prover (D106–D108 confirmed) |
| House secret, drand, void penalty | — | D108 confirmed, unchanged |

## 4. Result 2: the refereed sequential delay (RSD)

### 4.1 Why a delay is enough

X's miner can change X's chain position only by withholding it (free for about 12 s) or by out-racing the honest chain from X's parent. If nobody can compute y within δ_min, the only way to act on y is a private race of length δ_min. The review computed the catch-up probability (Nakamoto race with z = 10·δ_min·(1−α) honest blocks, minus the k = 124 blue allowance, racing until finality):

| δ_min (attacker's time to compute y) | α = 0.40 | α = 0.45 | α = 0.49 |
|---|---|---|---|
| 30 s | 1.1·10⁻² | 0.34 | 0.90 |
| 60 s | 2·10⁻⁸ | 1.3·10⁻² | 0.73 |
| 120 s | ≈ 0 | 2.4·10⁻⁵ | 0.53 |
| 600 s | ≈ 0 | ≈ 0 | 0.065 |

With δ_min = 120 s at 1 ns per step, T = 1.2·10¹¹ and an honest CPU needs about 1.8 h; δ_min = 300 s gives T = 3·10¹¹ and about 4.4 h. If the real attacker advantage is the estimated 10–30× rather than 50×, the effective δ_min is 2–5 times larger. The parent-omission lever (Am-5) and the D94 menu caveat stop mattering for the same reason: no decision that can still change the anchor can depend on y. Precomputation is impossible (X.hash is unknown until its PoW is found).

### 4.2 Transitions (outline; the spec outline has the checks)

1. **`commit_anchor()`** (anyone; the M7 272-byte proof via `OpChainblockSeqCommit`, unchanged): records x = X.hash and its commit DAA in the drawer state. The only step bound by the 12 h opcode window. Pays a commit reward.
2. **`claim()`** (anyone, any number of independent claim UTXOs, each with bond B): posts y and N = T/s checkpoints c₀ = x, c_{j+1} = H^s(c_j), c_N = y, over a chain of transactions; each batch is committed by a Merkle root over (index, value) and the batch roots by a top root R in the claim state. Lineage: a claim UTXO is a KIP-20 covenant whose genesis is checked (template, cov, round, c₀ = x).
3. **`refute(j)`** (anyone): a verification UTXO, itself KIP-20-bound to a genesis state "start = c_j at index j under R, steps done 0", advanced about 200,000 steps per transaction by anyone, s steps in all; its last transaction consumes the claim, checks R, the lineage of both UTXOs, the position j+1 under R, and that the result differs from c_{j+1}. Half the bond is burned, half paid to the refuter. Parallel verification chains do not touch the claim until their last step; a losing chain refunds after a lower-bound timeout.
4. **`finalize()`** (anyone): consumes the drawer and one unrefuted, complete claim with c₀ = x, at DAA ≥ max(commit DAA + T_honest_bound + W_c, claim completion DAA + W_c). The first bound lets every watcher finish its own honest computation before any claim can finalize, so checking a claim is a comparison. `draw()` then runs as today from y. Refutation is valid while the claim is unspent (lower-bound checks only, R3).
5. **Void:** no `commit_anchor()` by D + draw_window (D96, unchanged), and after a commit only a long backstop (days) for a lost round, so that nobody can force a void once y is known (review M3).

### 4.3 Parameters (to measure in a spike)

| Quantity | Proposal | Basis |
|---|---|---|
| T | 1.2·10¹¹ (δ_min 120 s at 1 ns) or 3·10¹¹ (300 s) | 4.1; owner trades latency against margin |
| s (segment) | about 10⁶ steps, refuted in about 5 transactions | ≈ 200,000 steps per transaction, to measure (S1) |
| N = T/s | 1.2·10⁵ checkpoints, 3.8 MB, about 18 near-full claim transactions | claims must cost more block space than refutations (review M2) |
| W_c | 2 h | refutation needs about 5 acceptance hops |
| T_honest_bound | 2× the reference CPU time | watchers start at commit |
| B | covers a refutation at a high feerate, plus the burned half | review M1, S2, S3 |

Per round: commit, about 18 claim transactions, finalize: a few KAS at current feerates (to measure), paid from the draw reward.

### 4.4 Assumptions added and what they mean in practice

- **One honest watcher.** If a wrong claim goes unrefuted, the claimant chooses the outcome: worse than today's bias, so this assumption must hold in practice. The natural watchers are holders of large ticket stakes in the round (they are the victims), the project front-end and any indexer; it is a laptop running for 2 h, not a prover service. Bünz et al. also note free-riding among challengers ("bystander effect"); the burned half of the bond and the per-round reward model are the first answers, forced errors (Truebit) a possible later one.
- **Hardware bound A_max ≈ 50** for sequential Blake3 (unmeasured; literature estimate 10–30×).
- **Honest majority** (< about 45 % for any attacker or collusion), Kaspa's own.
- **Latency:** the draw comes about 5 h after the anchor; design §9 margins (6 h) and the windows D97, D98, D102 must be re-derived (review M6, S5); rounds cannot be shorter than about 8 h.

## 5. Residual without the mechanism (plan A)

Attacker gain per round as a fraction of its ticket stake, α/(1−α)·(1−ε) − ε, ε = 1 − 0.79/margin − returned shares (`docs/costs.md` "Margin floor" closed form):

| Floor | Attacker | ε | α = 0.28 | α = 0.34 | α = 0.45 | α = 0.55 |
|---|---|---|---|---|---|---|
| 1.000 | worst (all shares) | 0.000 | +0.389 | +0.515 | +0.818 | +1.222 |
| 1.256 | worst | 0.161 | +0.165 | +0.271 | +0.525 | +0.864 |
| 1.500 | worst | 0.263 | +0.023 | +0.116 | +0.339 | +0.637 |
| 1.500 | ordinary | 0.373 | −0.130 | −0.051 | +0.139 | +0.393 |
| 1.760 | worst | 0.341 | −0.085 | −0.002 | +0.198 | +0.464 |

With RSD these gains are about 0 for α below about 0.45 at any floor.

## 6. Sources (literature check, 2026-10-13)

- Bonneau, Clark, Goldfeder, "On Bitcoin as a public randomness source", ePrint 2015/1015 (full text read).
- Bünz, Goldfeder, Bonneau, "Proofs-of-delay and randomness beacons in Ethereum", IEEE S&B 2017 (full text read; its 1-hour timing for 2⁴⁰ Keccak is inconsistent with its own Table I).
- Pierrot, Wesolowski, "Malleability of the blockchain's entropy", Cryptography and Communications 10 (2018) 211–233 (full text read).
- Boneh, Bonneau, Bünz, Fisch, "Verifiable Delay Functions", CRYPTO 2018, ePrint 2018/601 (full text read).
- Bentov, Gabizon, Zuckerman, "Bitcoin Beacon", arXiv:1605.04559 (full text read; Theorem 3.5).
- Bonneau, Bünz, Christ, Efron, "Good Things Come to Those Who Wait", EUROCRYPT 2025, ePrint 2024/1711 (abstract only).
- Luu et al., "Demystifying Incentives in the Consensus Computer", CCS 2015; Teutsch, Reitwießner, Truebit whitepaper (arXiv:1908.04756); Alvarez et al., "BoLD", AFT 2024 (arXiv:2404.10491); Berger et al., "Economic Censorship Games in Fraud Proofs", arXiv:2502.20334 (abstracts or partial reads).
- No Kaspa- or GHOSTDAG-specific literature on randomness or withholding was found.
