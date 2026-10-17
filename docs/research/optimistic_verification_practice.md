# Optimistic / fraud-proof verification in production: what RSD should copy (status as of 2026-10-17)

Scope note: research for the refereed sequential delay (RSD, `docs/research/seed-fairness-findings.md` section 4, `docs/specs/seed-delay-outline.md`). Primary sources (project docs, specs, papers) are preferred; secondary sources are flagged "(secondary)". "Not found" means I searched and did not find it; nothing is guessed. Kaspa facts are marked "to verify in refs/". Pages are cached at fetch time; dates are given where the source gives them.

## Headline summary

- Nobody in production uses a window anywhere near RSD's 2 h. Every system found uses days: Arbitrum One about 6.4 days per challenge period (45,818 parent blocks default), Optimism withdrawal delay 7 days, Alpen's BitVM2 bridge 1,064 blocks, BitVM2 paper "1-2 weeks". The only reasoning I found for the length is censorship and social response, not "time to run the game". RSD's W_c of 2 h with about 7 sequential moves is the biggest outlier and the biggest risk.
- The sequential move count drives the window. Berger et al. (Offchain Labs, 2025) show the attacker's required bribe budget scales with (window rounds) / (moves the defender must win). Fewer sequential moves buy shorter windows. BitVM2 cut the original BitVM's "up to 70 on-chain transactions" to 3 for exactly this reason. RSD's refutation could be made shallower (fan-out/fan-in, section "Implications").
- Bonds in practice are not sized to the attack gain but to (a) refuter on-chain cost at a high gas price (Optimism: 200 gwei assumption, about 2x the cost of a response), (b) a spam deterrent for the top-level claim (Arbitrum: 3,600 ETH), and (c) a resource ratio (Arbitrum: malicious side pays 10x the honest side's marginal cost). Honest gas is refunded or the loser's bond is paid to the honest side; Arbitrum adds a 1% defender's bounty.
- Watchers: no primary data on how many exist or how many real challenges happened (Arbitrum, Optimism): not found. Optimism's own spec says that without the bond reward "the FDG will be too costly for honest players to participate in". Arbitrum refunds honest gas out of band via the Foundation. Lightning altruist watchtowers exist but reward towers were still "undergoing further testing" in LND docs. Everything supports: pay the watcher, do not rely on altruism.
- Optimism's permissionless fault proofs shipped 2024-06-10 and were rolled back to a permissioned fallback about two months later (reported 2024-08-17) after audits found high-severity bugs; the dispute game and MIPS contracts had been outside audit scope. Lesson for RSD: the refutation machinery is the highest-risk code, and RSD has no guardian to fall back on.
- Closest analogue (BitVM2 and bridges built on it) splits one huge verifier into chunks of at most 4 MB, makes the operator commit to every intermediate value, and lets anyone execute one chunk on chain to disprove. That is structurally RSD (checkpoints plus one-segment recomputation). Its practical pains: non-standard transaction sizes, challenger collateral against griefing, and capital lockup.

---

## Q1. Arbitrum BoLD

### Takeaway
BoLD (live on Arbitrum One and Nova from Feb 12, 2025) bounds delay by design: disputes confirm within a fixed window of two challenge periods plus a grace period, no matter how many Sybils, and one honest party suffices. It does so with large bonds (3,600 ETH top level) and a 6.4-day base period that is explained by censorship/social-response arguments, not by game length. Number of active validators: not found.

### Cited Findings
- Go-live: "Arbitrum One 09:00 ET on Feb 12, 2025". [Arbitrum docs, choose-bold via search](https://docs.arbitrum.io/launch-arbitrum-chain/features/common/validation-and-security/choose-bold); [The Block](https://www.theblock.co/post/340278/offchain-labs-releases-arbitrum-bold-on-mainnet-for-permissionless-validation) (secondary, says disputes resolve in "about 12 days").
- Challenge period "~6.4 days"; BoLD disputes resolve "within a fixed time window of 2 challenge periods (~6.4 days each), 2 day grace period for the security council to intervene" plus a small computation delta; grace period raised from 1 to 2 days in an April 2024 update. [Arbitrum AIP forum post](https://forum.arbitrum.foundation/t/aip-bold-permissionless-validation-for-arbitrum/23232)
- Default config: 45,818 parent-chain blocks (about 1 week) challenge period plus 200 blocks (about 40 min) extra for "last-minute challenges"; 30,000 blocks (about 4.5 days) is given as a faster example, not a minimum. Stated aim: "sufficient time for validators to detect and challenge fraudulent assertions". [Arbitrum docs, choose challenge period](https://docs.arbitrum.io/launch-arbitrum-chain/features/common/validation-and-security/choose-challenge-period)
- The page does not explain why 6.4 days. [Arbitrum BoLD FAQ](https://docs.arbitrum.io/how-arbitrum-works/bold/bold-faq). The general rationale for about 7 days in optimistic rollups, per Offchain Labs authors, is a social response to a 51% censorship attack: detect within 24 h, prepare and activate a fork in up to 5 days, leave about 1 day of clock; "not" the interaction time of the game. [ethresear.ch 21721](https://ethresear.ch/t/optimistic-rollups-the-challenge-period-and-strong-censorship-attacks/21721) (forum post by Arbitrum-affiliated authors; commenters call 7 days "somewhat arbitrary").
- Delay-attack bound: "The BoLD delay is bounded, and additional challenges would not increase the delay of a particular assertion"; the assertion bond deters delay attacks. [BoLD FAQ](https://docs.arbitrum.io/how-arbitrum-works/bold/bold-faq)
- Bonds (Arbitrum One): top-level assertion 3,600 ETH; sub-challenge bonds 555/79 ETH per level (an earlier version: 1000/100/10); "1,110 ETH to fully resolve a dispute, refunded on confirmation"; "a malicious party pays 10 times the honest party's marginal costs". 3,600 ETH was kept because it "acts as a deterrent to open a challenge and spam the protocol". Derivation deferred to an "Economics of Disputes" PDF (not read). [BoLD FAQ](https://docs.arbitrum.io/how-arbitrum-works/bold/bold-faq); [AIP thread](https://forum.arbitrum.foundation/t/aip-bold-permissionless-validation-for-arbitrum/23232); [search digest of AIP page 4](https://forum.arbitrum.foundation/t/aip-bold-permissionless-validation-for-arbitrum/23232?page=4)
- Honest side: gas "100% refundable" by the Arbitrum Foundation; malicious bonds confiscated to a DAO-controlled address; 1% "defender's bounty" to honest bond-posters. Off-chain compute is not reimbursed: about USD 2.50 per sub-challenge on an AWS r5.4xlarge (about USD 1/hour, 2.5 h). Hardware goal: a laptop in the average case; minimum still "being researched". [BoLD FAQ](https://docs.arbitrum.io/how-arbitrum-works/bold/bold-faq)
- "Only a single honest party is required"; "any number of honest, anonymous parties". [BoLD gentle introduction](https://docs.arbitrum.io/how-arbitrum-works/bold/gentle-introduction). Counter-view (interested party, Cartesi author): BoLD's fully parallel design lets a funded adversary force the hero to spend about 150,000 ETH against a 1M ETH attack. [Dave on ethresear.ch](https://ethresear.ch/t/the-dave-fraud-proof-algorithm/21844)
- Censorship in a chain without sequencer: each move may need a 24 h force-inclusion wait, so a 50-move challenge could take 50 days; a Delay Buffer reduces this. [BoLD FAQ](https://docs.arbitrum.io/how-arbitrum-works/bold/bold-faq)
- Validator count: "no total count is given"; "very few proposers" expected; only 1 needed for liveness. Not found as a number. [BoLD FAQ](https://docs.arbitrum.io/how-arbitrum-works/bold/bold-faq), [AIP](https://forum.arbitrum.foundation/t/aip-bold-permissionless-validation-for-arbitrum/23232). Number of disputes that actually occurred: not found.

### Inferences
- Arbitrum treats the window as a censorship budget and the bond as a spam/delay price; the game itself takes far less than the window.
- The bond is large because Arbitrum's honest side must be able to pay up to about 1,110 ETH in parallel; this is capital, refunded on win.

### Gaps
Economics of Disputes PDF not read; actual dispute history and active validator count not found.

---

## Q2. Optimism permissionless fault proofs (Cannon)

### Takeaway
Launched 2024-06-10, rolled back to a permissioned fallback within about 10 weeks after audits found bugs in the dispute game and in the interaction between clock extension and the large-preimage period. The design uses chess clocks, escalating gas-scaled bonds and a Guardian backstop.

### Cited Findings
- Live 2024-06-10; reverted to the permissioned fallback (reported 2024-08-17); "None of the vulnerabilities have been exploited"; dispute game and MIPS contracts were outside audit scope. [The Block](https://www.theblock.co/post/311702/optimism-foundation-disables-permissionless-fraud-proofs-plans-hard-fork-following-security-audits) (secondary news).
- Granite proposal (2024-08-16, activation 2024-09-11 16:00:01 UTC): Cantina 3.1.1 (High, mmap bounds), Code4rena H-01 (High: counter a valid claim using a trace one block past the disputed block), H-02 (Medium: the large-preimage challenge period was longer than the clock extension, "so honest challengers could lose bonds"), H-05, Spearbit 5.1.1 (precompile out-of-gas overwrote a valid preimage), plus clock-extension edge cases (zero extension at depth 1, extension above half the max duration reverts moves). The Deputy Guardian activated the permissioned fallback; all pending withdrawals were invalidated and had to be re-proven. [Optimism governance, Granite](https://gov.optimism.io/t/upgrade-proposal-10-granite-network-upgrade/8733)
- Later (date not seen): a medium-severity off-chain blob-preimage bug can make games resolve incorrectly; the team chose not to downgrade because the Security Council can blacklist wrongly resolved games from bridge withdrawals. [Optimism governance disclosure](https://gov.optimism.io/t/vulnerability-disclosure-incorrect-blob-preimages/9833)
- Guardian can pause, blacklist specific games, and switch the respected game type to a permissioned game; DelayedWETH withdrawal delay is 7 days on OP Mainnet. [Optimism docs, fp-security](https://docs.optimism.io/stack/fault-proofs/fp-security) ("honest challengers can lose bonds if the Guardian does not step in").
- Clock rules: chess clocks per team, a claim inherits its grandparent's clock, CLOCK_EXTENSION is a flat credit when little time remains, extended by 2x for the first execution-trace claim "to cover the off-chain FPVM run". Values are per-implementation constants. [OP spec, fault-dispute-game](https://raw.githubusercontent.com/ethereum-optimism/specs/main/specs/fault-proof/stage-one/fault-dispute-game.md). Base's spec (secondary) lists max clock 3.5 days and extension 3 hours; OP mainnet values not confirmed. [Base spec search result](https://docs.base.org/base-chain/specs/reference/configurability)
- Bonds: gas-scaled per depth, `400_000 * 1.09493^N` gas up to `300_000_000` gas (MAX_DEPTH 73), priced at a fixed 200 gwei; the baseline is "approximately double the cost to respond at the top level"; the maximum covers about double the largest large-preimage proposal; the loser's bond goes to the leftmost counter-claimant or to whoever called `step`. "Without the bond reward incentive, the FDG will be too costly for honest players to participate in." [OP spec, bond-incentives](https://specs.optimism.io/fault-proof/stage-one/bond-incentives.html)
- Max depth 73; recommended initial bond 0.08 ETH; "A 7-day dispute window requires roughly 14 ETH" of proposal bonds at hourly proposals. Challenges are permissionless via `op-challenger`. [OP docs, fault-proofs explainer](https://docs.optimism.io/stack/fault-proofs/explainer)

### Inferences
- The H-02 class of bug (a side-process longer than the honest side's remaining clock) is directly relevant: any RSD step whose duration can exceed the refuter's remaining window lets the attacker win by timing.
- OP depends on a human Guardian for safety; RSD, by the project's rules, has none.

### Gaps
Count of real disputes on OP Mainnet: not found. OP mainnet clock values: not confirmed. The hacken.io write-up returned 403 and was not read.

---

## Q3. Cartesi Dave / PRT

### Takeaway
PRT/Dave put Sybil resistance in the structure: matching claims group, bonds are per claim, and the honest side's hardware and stake stay constant while settlement delay grows only logarithmically in the number of Sybils.

### Cited Findings
- PRT: claims are Merkle trees over states; a bracket-style tournament halves claims per stage; sparse then dense recommitment until a one-step proof; honest hero needs one stake, workload "only logarithmically in the number of Sybils"; settlement in "logarithm squared"; no durations given. [Cartesi docs, PRT intro](https://docs.cartesi.io/fraud-proofs/prt/prt-introduction/)
- Dave (author post, date not shown): delay O(HP + log Sybils) with HP = 21 hitpoints and an 8 h grace per timeout; with a one-week censorship assumption it takes 21.75 days at 2^4 Sybils, 35.25 at 2^20, 42.75 at 2^32 ("2 to 5 challenge periods"); match delay 2 h (about 1 h inclusion + about 1 h validity proof); 450,000 gas per match, about 0.05 ETH at 100 gwei; suggested bond 3 ETH; "a mispriced bond does not compromise security, and even a system with no bonds is secure"; hero expenses 7 ETH against a 1M ETH attack; comparison table claims BoLD hero cost 150,000 ETH and OPFP 1,000,000 ETH (the post is advocacy, treat as interested). [ethresear.ch, The Dave fraud-proof algorithm](https://ethresear.ch/t/the-dave-fraud-proof-algorithm/21844)
- Repo: README says a general recursive delay theorem is not yet established; repo has no releases; mainnet status not stated. [cartesi/dave on GitHub](https://github.com/cartesi/dave)

### Inferences
Dave trades liveness (weeks) for constant honest cost. RSD can take a cheaper path because it only needs the honest result to finalize, not a bounded-delay fight; see Implications 5.

### Gaps
PRT/Dave papers not read; mainnet deployment and timings not confirmed.

---

## Q4. BitVM / BitVM2 and bridges (closest analogue)

### Takeaway
BitVM2 is an optimistic scheme on a chain with no VM: a program too big for any transaction is split into chunks (each at most 4 MB of script); the operator posts a claim and, if challenged, commits to all intermediate chunk outputs on chain; anyone can disprove by running one chunk. Trust: "existential honesty" (1-of-n) at setup plus one honest challenger. Bitcoin-specific costs (non-standard sizes, timelock pre-signing, capital lockup) shape everything.

### Cited Findings
- BitVM2 paper: verifier split "into sub-program chunks, each at most 4MB"; operator commits during setup; challenge forces the operator to "reveal all intermediary outputs"; "anyone can find the incorrect intermediary result and prove that the operator cheated by executing the corresponding sub-program chunk on-chain"; slashes "(some) limited amount of collateral (reimbursing on-chain costs)"; "3 on-chain transactions ... with a delay of no more than 2-3 weeks"; original BitVM "up to 70 on-chain transactions (over a period of multiple weeks or even months in practice)". Permissionless challenging for anyone with a full node. [BitVM2 paper](https://bitvm.org/bitvm_bridge.pdf) (read from the PDF text; paper date not extracted).
- Griefing: a challenger must post collateral c_B "parameterized such that it covers the transaction fees of Assert"; challenge collateral can be crowdfunded with SIGHASH_SINGLE|ANYONECANPAY. Same paper, section 5.4.
- Practical limits: Assert and Disprove transactions "exceed the 400kB standardness rule ... will not be relayed by non-modified Bitcoin full nodes", so delivery needs direct links to miners/pools; up to 960 sub-programs of 4 MB in a single 4 MB Assert; operators must front BTC (capital cost); deposits are fixed-size. Same paper, section 8.
- Sizes (Alpen, vendor blog): Disprove 3.5-4 MB, Assert about 4.8 MB, full Groth16 verifier about 1.2 GB; challenger on-chain commitment about 4 MB; the Challenge transaction is assumed altruistic and "must compensate at least the prover's cost of submitting the Assert data"; stake must exceed the Disprove cost. [Alpen blog](https://www.alpen.org/blog/state-of-snark-verification-with-bitvm2) (secondary, vendor)
- Alpen bridge docs: challenge period 1,064 blocks (the page says "approximately 1.4 weeks"; 1,064 Bitcoin blocks at 10 min is about 7.4 days by my arithmetic, so the doc's "1.4 weeks" is inconsistent; treat the block count as authoritative); operator reassignment 504 blocks; a slashable bond of unstated amount; testnet denomination 2 BTC; "requires only a single functional operator"; mainnet values "will be announced at a later stage". [Alpen docs](https://docs.alpen.org/how-alpen-works/bitcoin-bridge)
- Clementine (Citrea): security rests on "at least one honest Watchtower in a permissioned set"; a single challenge per operator can slash multiple misbehaviours. Timelock and collateral numbers not found. [ePrint 2025/776](https://eprint.iacr.org/2025/776). Secondary claims of mainnet since 2026-01-27 and "roughly two weeks" settlement [Phemex](https://phemex.com/academy/what-is-bitvm-smart-contracts-on-bitcoin) are unverified.
- Bitlayer: BitVM bridge announced live on mainnet (YBTC); challenge period and stake not found. [bitcoinnews.com](https://bitcoinnews.com/p/bitlayer-bitvm-bridge-live-on-mainnet) (secondary)
- Known hole: no deployed BitVM-class bridge has an on-chain disprove path for one class of "fixed-lie" assertions (equivocation is slashable, a single consistent false value is not). [ethresear.ch post](https://ethresear.ch/t/tee-as-verifier-for-bitvm-style-bridges-collapsing-the-canonical-label-distribution-problem/24783) (single-author forum post, secondary)
- Economics (secondary commentary): bridge user deposits are "insured up to the value of the Operator's collateral"; at 10% collateral the last withdrawal in a queue gets roughly 10 challenge periods of guarantee. [Red Velvet Zip](https://redvelvetzip.substack.com/p/are-bitvm-bridges-economically-secure)

### Inferences
- RSD's chunk size is far smaller (about 200,000 Blake3 steps per transaction) because Kaspa limits are different; the Kaspa transaction mass/standardness limits are "to verify in refs/" and the spike (S1/S2) must measure them. BitVM2 shows that at the size ceiling transactions are not relayed by default nodes; RSD's "near-full" claim batches have the same class of risk.
- RSD needs no signer committee (KIP-20 covenants replace pre-signed transactions), so BitVM's existential-honesty setup risk disappears; its weak point is replaced by the "one honest watcher" assumption.
- The "fixed lie" gap has an RSD counterpart: any claim field that no refutation path can contradict (for example a wrong completion DAA, a wrong claimant script) must be fixed by construction at genesis. The outline's genesis checks cover c_0 = x and the position of c_N = y; the spec should list every claim field against a refutation or a genesis check.

### Gaps
Clementine whitepaper body (timelocks, bond sizes) not read; Bitlayer figures not found; BitVM2 paper date not extracted.

---

## Q5. Watcher incentives in practice

### Takeaway
Theory says unpaid verification collapses (verifier's dilemma). Practice answers are: pay refuters from the loser's bond (OP, BitVM, Arbitrum), refund honest gas and add a bounty (Arbitrum), run altruist watchers (Lightning towers), or force errors so honest checking pays (Truebit). Empirical counts of real challenges and watchers: not found.

### Cited Findings
- Verifier's dilemma: with costly verification, rational nodes accept unvalidated results; the paper (ACM CCS 2015, Test-of-Time Award 2025) proposes an incentive fix for outsourced computation. [IACR ePrint 2015/702](https://eprint.iacr.org/2015/702); [NUS note](https://www.comp.nus.edu.sg/features/the-dilemma-that-shook-the-blockchain-world-a-decade-on/) (secondary).
- Truebit: periodic forced errors with jackpot payouts so verifiers who check every task eventually win; forced-error rate set to about 1 per 1,000 tasks; jackpot about fair task pay times that rate; reward falls exponentially with the number of challengers (Sybil resistance). Critique (Koch and Reitwiessner 2018): profitability is "very hard to predict". [Truebit paper PDF](https://people.cs.uchicago.edu/teutsch/papers/truebit.pdf), [arXiv 1806.11476](https://arxiv.org/pdf/1806.11476) (as summarised by the search tool; authors not fully confirmed from results). Front-running critique: [ethresear.ch 1215](https://ethresear.ch/t/truebit-protocol-front-running-vulnerability/1215) (forum, secondary).
- Arbitrum: honest gas refunded by the Foundation and a 1% defender's bounty; off-chain compute not refunded. [BoLD FAQ](https://docs.arbitrum.io/how-arbitrum-works/bold/bold-faq)
- Optimism: bond reward needed or honest play is "too costly". [OP spec](https://specs.optimism.io/fault-proof/stage-one/bond-incentives.html)
- Lightning: LND watchtowers are altruist ("returns all of the victim's funds (minus on-chain fees) without taking a cut"); reward towers "will be enabled in a subsequent release" (doc undated); the tower learns nothing until a breach. [LND watchtower docs](https://github.com/lightningnetwork/lnd/blob/master/docs/watchtower.md) Usage statistics: not found.
- Felten reportedly said few fraud proofs were attempted because malicious validators risk their stake (secondary, unverified, no date). [spark.money glossary via search](https://www.spark.money/glossary/optimistic-rollup)
- "Bystander effect" among fraud-proof challengers: I could not find the claimed Bunz et al. source. The cited study on the bystander effect in fraud reporting is human-subjects work (Brink, Eller, Gan 2015), only loosely analogous. Plasma watcher data: not found.
- Empirical challenge counts for Arbitrum or Optimism and active validator numbers: not found.

### Inferences
- A system whose attack never happens pays watchers nothing in equilibrium. Fixed per-round rewards or a poster-as-watcher design are the practical fixes.
- RSD has a structural advantage: the claimant has already computed y (1.8 h CPU), so every honest claimant is also a watcher with a reward at stake in rival claims.

### Gaps
No quantified watcher populations anywhere; Bunz et al. reference unresolved (the findings file mentions it; verify the citation in `seed-fairness-findings.md` section 6).

---

## Q6. Censorship of challenges and window length

### Takeaway
Window length is chosen against censorship, not against the compute time of the game. The best quantitative treatment (Berger, Felten, Mamageishvili, Sudakov) ties the safe window to the number of sequential moves and the defender/attacker budgets. It is derived for Ethereum proposers; a PoW DAG changes the numbers.

### Cited Findings
- Berger et al., submitted 2025-02-27: models where an attacker bribes proposers to exclude the defender's moves. Base game: attacker wins iff (T-N+1)/N * D <= A (T rounds in the window, N rounds the defender must win, D defender budget, A attacker budget). Special rounds (non-rational proposers, attacker must outbid by factor k) raise the bound; with m independent builders the attacker needs about m times the budget (Theorem 4) and the defender wins with prob >= 1-2^(-Theta(N)) if A < (T-4N+1)m/N * D (Theorem 5). Numeric example: one week about T = 50,000 blocks of 12 s, about 2% non-rational proposers (s about 1,000), k about 60, n about 60; with a USD 10B attacker payoff the defender needs at least USD 12M without special rounds, about USD 5.6M with them. The paper does not itself conclude that one week is right. [arXiv 2502.20334 (PDF text read)](https://arxiv.org/pdf/2502.20334); [abstract](https://arxiv.org/abs/2502.20334)
- Same family: for "soft" censorship 99.99% inclusion needs "at least 3 hours per transaction"; with about 35 steps per player that gives about 4.5 days; with about 70 steps and 7-day clocks an attacker can delay each move about 5 h. [ethresear.ch 21721](https://ethresear.ch/t/optimistic-rollups-the-challenge-period-and-strong-censorship-attacks/21721)
- Arbitrum L3 example: 24 h forced-inclusion wait per move means a 50-move challenge could take 50 days without a Delay Buffer. [BoLD FAQ](https://docs.arbitrum.io/how-arbitrum-works/bold/bold-faq)
- Bitcoin side: BitVM2's disprove needs 3 transactions in "1-2 weeks"; Alpen 1,064 blocks. (Q4)

### Inferences
- On Kaspa, a block producer can only censor a transaction in its own blocks; whether an attacker with fraction alpha of hashrate can delay inclusion depends on how the 10 blocks per second DAG picks transactions (to verify in refs/). The Berger result that multiple independent builders multiply the attacker's cost by about m carries over qualitatively. This is an inference, not a measured Kaspa fact.
- The Ethereum "3 h per transaction for 99.99%" figure comes from slot-based proposers and should not be copied; it shows only that practice adds hours per move, not seconds. RSD needs its own measured p99.99 inclusion latency per hop on testnet-10, under a simulated attacker miner.
- RSD's attacker is the anchor miner with alpha up to about 0.45 in the findings table: a miner with a large share who also gains from a wrong y. Berger's bribe model prices censorship; here the censor is already a large block producer and pays no bribe, so the bribe-based safety margin is smaller than in the paper.

### Gaps
No source on PoW censorship in fraud proofs; no Kaspa-specific inclusion data.

---

## Q7. Bond sizing in practice

### Takeaway
Three rules recur: the bond covers the counterparty's on-chain response at a high feerate with a margin of about 2x (Optimism), the top-level bond is a spam/delay deterrent far above gas cost (Arbitrum), and the loser's bond is the honest side's reward. Dave argues the exact bond is not security-critical.

### Cited Findings
- Optimism: gas-scaled per depth, 200 gwei assumption, baseline about 2x top-level response cost. (Q2)
- Arbitrum: 3,600 ETH top-level, 555/79 per level, 10x resource ratio, honest gas refunded, confiscated bonds go to a DAO-controlled address, not burned; 1% defender's bounty. Lower bonds "reduce the costs of delay grieving attacks". (Q1)
- BitVM2: challenger collateral c_B at least the Assert fee; operator stake greater than the Disprove cost (Alpen); crowdfunding challenges. (Q4)
- Dave: 3 ETH suggested; "a mispriced bond does not compromise security". (Q3, interested source)
- Burn vs pay: OP pays the leftmost counter-claimant or the `step` caller; Arbitrum confiscates to the DAO and pays 1% bounty; BitVM burns the operator's connector (the operator loses access to funds) (secondary: Red Velvet Zip). No source found comparing burning with paying on griefing grounds.

### Inferences
- A pay-the-refuter share is necessary (OP: too costly otherwise); burning part of it is the standard defence against the claimant refuting itself, which RSD already handles (outline M1).

### Gaps
Arbitrum's derivation (Economics of Disputes PDF) not read.

---

## Implications for RSD

1. **Window W_c (Q6, Q1, Q2).** Do not treat 2 h as safe. All production systems use days, and the closest numeric model (Berger) ties safety to window rounds divided by sequential moves. Choose W_c from a formula, not from the happy-path hop count: W_c >= (sequential refutation hops + 1) x (measured p99.99 inclusion latency under an attacker miner) + honest recompute margin. By the Ethereum benchmark (3 h per move) this would be about 24 h for 7 hops; Kaspa will be far better, but the number must be measured on testnet-10 (S2), not assumed. The cost of a longer W_c is latency: the round is 8+ h already; adding 12 to 24 h is an owner decision to present, with the formula.
2. **Cut sequential hops, not just steps (Q4, Q6).** BitVM2 went from about 70 transactions to 3; Berger's bound scales with N. Fan-out / fan-in: the refuter commits the true intermediate values in one transaction, five chunk-verification transactions run in parallel (about 200,000 steps each), and one final transaction consumes all five plus the claim. That makes about 3 sequential hops instead of about 7 and costs more fee in parallel. This is an inference from BitVM2's disprove pattern; it needs a soundness check in the spec.
3. **Clock semantics, with an extension rule (Q2).** Copy Optimism's lesson (H-02): no refutation step may be able to outlast the remaining window. Define the finalize bound as max(completion + W_c, last refute-chain activity + W_ext) so that a claim completed or a verify chain started near the deadline still leaves the refuter a full chain duration. The outline's finalize bound (completion DAA + W_c) already covers completion; check the verify genesis at the window edge (M5) against this rule.
4. **Bond B (Q7).** Size as B >= 2 x (cost of one full refutation chain at the maximum assumed feerate, plus a failed-attempt margin) x (number of simultaneous false claims the honest side must kill), with half burned and half to the refuter. The 2x margin mirrors Optimism's rule; the multiplier covers the Sybil case below. A top-level deterrent far above gas (Arbitrum's 3,600 ETH) is a separate lever; RSD's analogue is the claim's near-full upload cost (18 transactions) versus the 5-7 transactions of a refutation, which review M2 already requires.
5. **Sybil claims are RSD's Dave problem (Q1, Q3).** One honest claim does not stop a wrong claim finalizing; the honest side must refute every wrong claim before its own deadline, each needing its own verify chain. k wrong claims cost the attacker k bonds plus k uploads and the honest side k refutations (paid half a bond each, so net positive if B/2 exceeds one refutation). The risk is block space and capital for k parallel chains inside W_c. Mitigations to specify: a claim-count cap or rising bond per outstanding claim (BoLD per-level bonds, PRT bonds per claim), and a test on testnet with k = 50 and 500 false claims. Whether one verified segment can kill several claims sharing the same first wrong checkpoint is an open design question (a covenant UTXO is spent once).
6. **Watchers: pay and align them (Q5).** Altruism is not enough (Lightning reward towers unfinished, Optimism "too costly", Arbitrum refunds gas). RSD's best asset is that the claimant has already computed y. Make the finalize reward go to the claimant whose claim finalizes, so every rational honest party computes y and posts a competing correct claim; each is then motivated to refute rivals. Keep the refuter payout, and consider a fixed per-round watcher fee from the draw reward for any refutation attempt that is later shown correct. Truebit-style forced errors (a protocol-funded false claim) are a possible extra but need a funder and reduce predictability (critiqued by Koch and Reitwiessner); do not add them in the first spec.
7. **Number of posters (Q1, Q5).** Specify no cap on claims (permissionless, as BoLD "any number of honest parties"); do specify that a correct claim and the commit reward are rewarded once, with the bond refund on finalization. Do not rely on a single poster-watcher: the project front-end, an indexer and large ticket holders should each run the watcher by default, and the spec should state the required watcher uptime (about the honest compute time plus W_c per round).
8. **Test the dispute game as the main audit target (Q2).** Optimism's rollback came from bugs in dispute-game edge cases outside audit scope. The RSD attack table (spec outline section 3) is right; add: refutation chain steps that run past window end, claim completing within one block of the deadline, zero-extension edge cases, wrong-length segments (T not equal to N times s), and the BitVM "fixed-lie" analogue for every claim field. Require `covenant-reviewer` plus a second independent review before testnet; no Guardian exists, so a rollback path must be decided up front (void after a long backstop, outline M3).
9. **Standardness and relay (Q4).** BitVM2's large transactions were not relayed by default nodes. S2 must establish whether near-full Kaspa transactions are accepted and propagate at the relay floor under load; "to verify in refs/". If not, claim batches must shrink, which raises the transaction count and lowers the bond-versus-refutation cost ratio of item 4.
10. **Griefing the refuter's side (Q4, Q7).** BitVM2 makes challengers post collateral to stop forced-expensive assertions. RSD's refutation is cheap and bonded on the claimant side; check whether a false refutation attempt (against a correct claim) can force the claimant to spend anything. If not, no challenger collateral is needed; say so explicitly in the spec.

### Risks RSD does not yet address
- **W_c too short (Q6, Q1).** The 2 h window has no precedent and no censorship model; a block-producing attacker (the anchor miner) is the realistic censor, not a briber.
- **Sybil claim flood inside the window (item 5).** The outline covers claim flood vs block space (M2) but not k parallel honest refutation chains against each claim's own deadline.
- **No fallback if the machinery has a bug (Q2).** Optimism needed a Guardian; RSD cannot have one. A refutation bug is either a stuck round (void) or a stolen draw.
- **Honest-watcher liveness (Q5).** No primary data shows that challengers exist in practice; the whole safety argument rests on at least one. The watcher's own capital and key availability during the window (item 6) are unspecified.
- **Interaction between attacker speed and reward timing.** If an attacker with a fast sequential computer finishes y first, they can post a wrong claim and a correct one, or time claims to the edge; the honest watcher has computed nothing until 1.8 h after commit (T_honest_bound). Check that refutation does not require the honest party to have finished its own computation before a claim completes.
- **Kaspa-specific numbers are missing**: inclusion latency, transaction standardness limits and feerates are all "to verify in refs/" and in the S1/S2 spike.
