# How teams ship new randomness and fraud-proof mechanisms, and what a small team should copy (status as of 2026-10-17)

Scope note: research only, no code or design change. Primary sources preferred; secondary sources are flagged "(secondary)". Anything I could not find is marked "not found". Page fetches were summarised by a tool, so exact figures should be re-checked at the URL before they go into a decision. Kaspa facts are not stated here: anything about Kaspa limits is "to verify in refs/". The RSD mechanism is described in `docs/research/seed-fairness-findings.md`.

## Headline summary

- No team in this survey shipped a new randomness or fraud-proof mechanism on design review alone. The pattern is: public design or paper, then a long testnet, then one or more paid audits, then a public contest, then a guarded mainnet launch with a human fallback (guardian, security council, RANDAO fallback, permissioned mode). The fallback was used in anger at least once (Optimism, August 2024).
- Late findings are normal and come from outside the original team. Optimism's permissionless fault proofs went live on 2024-06-10 and were switched back to permissioned about two months later, after three outside audit streams (Spearbit, Cantina, Code4rena) found issues in the dispute game and MIPS VM. Arbitrum's Code4rena contest on BoLD found 2 High and 2 Medium after Trail of Bits had already audited it. Ethereum's VDF program was stopped by an outside cryptanalysis (three days of work) and by an earlier audit of the planned trusted-setup MPC.
- Sequentiality assumptions are the weak point, which is exactly RSD's new assumption (nobody computes sequential Blake3 more than ~50x faster than a CPU). MinRoot, Sloth++ and VeeDo were all broken or weakened by a parallel speed-up that their designers had not looked for. Cardano's Phalanx lists ASIC speed-up and amortisation as open risks and calls its parameters "best-effort".
- Formal methods at the big teams were mostly paper proofs plus audits; mechanised verification of the dispute protocol was not found for BoLD. A small team gets most of the value from a written executable model, property tests and an adversary-minded reviewer, which this project already has.
- Typical staging yardsticks: L2BEAT Stage 0/1/2 (Section 5) require a working proof system, a bounded override power and exit windows; they are a vocabulary for "training wheels", not a safety proof.

---

## Key Question 1: Tezos VDF in Kathmandu (2022)

### Takeaway
Tezos added a class-group VDF on top of its existing RANDAO-like scheme so that a missing or late VDF only costs the extra layer, never the seed. This is the cleanest published example of a "safe failure" design. I found no public audit of the VDF and no reviewer list.

### Cited Findings
- Kathmandu was announced as the 11th protocol proposal on 2022-07-13, listing "improved randomness with integration of VDFs" among its main features; a Kathmandunet testnet mirrored the proposal and the team asked bakers and builders to run it. [Nomadic Labs announcement](https://research-development.nomadic-labs.com/announcing-tezos-11th-protocol-upgrade-proposal-kathmandu.html)
- Kathmandu activated on mainnet 2022-09-23 at block 2,736,129. [Nomadic Labs, "Kathmandu is live"](https://research-development.nomadic-labs.com/kathmandu-is-live.html) (date repeated in [The Block](https://www.theblock.co/post/172590/terms-service), secondary). So proposal to activation was about ten weeks, including the on-chain voting periods.
- Design: the RANDAO output sets up the VDF challenge; the first baker to submit a correct solution before the cycle ends gets a small tip. If a valid solution is posted the seed is the hash of RANDAO output and VDF solution; if not, the seed is the RANDAO output alone. [Nomadic Labs, "Verifiable delay functions", 2022-11-21](https://research-development.nomadic-labs.com/verifiable-delay-functions.html)
- The implementation adapts the Chia VDF library through OCaml bindings (`class_group_vdf`, Nomadic Labs, MIT). [same page](https://research-development.nomadic-labs.com/verifiable-delay-functions.html); [opam listing](https://opam-5.ocaml.org/packages/class_group_vdf/class_group_vdf.0.0.2)
- Amendment history entry: Kathmandu "integrated VDFs into the random seed generation". [Tezos docs](https://docs.tezos.com/architecture/governance/amendment-history)

### Inferences
- The design makes the VDF a liveness-safe add-on: failure degrades to the old security level. RSD's fallback is different (a wrong unrefuted claim chooses the outcome, a void ends the round), so RSD has no equivalent graceful floor unless a void-or-old-seed rule is added. This is a design question for the spec, not a finding.
- The incentive for solving is a small tip, the same shape as RSD's commit reward.

### Gaps
- Audit of the VDF code, testnet duration, the start date of the VDF research, and reviewer names: not found.

---

## Key Question 2: Cardano CIP-0161 Ouroboros Phalanx and CPS-0021

### Takeaway
Cardano split the work in two public documents: a problem statement with a cost model (CPS-0021) and a solution (CIP-0161). The cost model, not a proof, is the case for the design. As of the sources found, it is "proposed", under technical validation, with no dates for implementation or testnet.

### Cited Findings
- CPS-0021 "Ouroboros Randomness Manipulation": authors Nicolas Henin and Raphael Toledo, status Open, created 2025-10-03. It frames randomness as the coin-flipping problem, states it is "a problem statement, not a solution", and asks open questions on detection, economic deterrents and whether grinding can be eliminated. It gives a cost model: small attacks about $56, large ones up to about $3.1 billion, with a feasibility table by grinding depth. [CPS-0021](https://github.com/cardano-foundation/CIPs/tree/master/CPS-0021)
- CIP-0161: status Proposed, created 2025-07-25, same authors; adds a Wesolowski VDF step to Praos; claims grinding cost rises roughly 10^10 for a 2-hour attacker budget (scenarios 2 h, 12 h, 5 days); assumes at least 51 % honest stake; recommends VDF parameters (128, 64) with a 4096-bit discriminant; lists as risks ASIC acceleration (Chia reported 3-5x), lookup-table amortisation, and the lack of formal non-amortisability guarantees, calling the recommendation "best-effort". It contains benchmarks but I found no dedicated simulation section and no reviewer list in the part I could read (about 100,000 of 156,000 characters). [CIP-0161](https://github.com/cardano-foundation/CIPs/tree/master/CIP-0161)
- IOG announcement, 2025-10-22: the proposal is "undergoing technical validation"; next steps are implementation and node integration, testnet trials, then a hard fork; no dates; no external reviewers named. [IOG](https://www.iog.io/news/introducing-ouroboros-phalanx-breaking-the-economics-of-grinding-attacks)
- A search for later Phalanx testnet or implementation news found none (Cardano's June 2026 hard fork found in the search is a different upgrade, protocol version 11). Not found.

### Inferences
- The useful template is the written cost model with named attacker tiers and a stated honest-stake assumption. The project already has an equivalent (margin floor, attacker gain table, section 5 of the findings).
- Cardano's open admission of "best-effort" parameters on a hardware assumption mirrors RSD's A_max ≈ 50 and should be copied: state the assumption, state who is allowed to break it, state what is measured.

### Gaps
- CIP-0161 sections beyond ~100,000 characters (Path to Active, reviewers) not read; no external security review found; the CPD cost-model document not read.

---

## Key Question 3: Ethereum's VDF program (2019-2023)

### Takeaway
The program ran about four years with a hardware alliance, a trusted-setup design that an audit broke, then a new function (MinRoot) that a short cryptanalysis effort weakened. The process that exposed the flaws was open, targeted cryptanalysis, not internal review. The conclusion was to stop, not patch.

### Cited Findings
- 2019: Protocol Labs and the Ethereum Foundation announced a collaboration on a secure VDF and on evaluating optimised ASICs; contests were planned. [Protocol Labs](https://protocol.ai/blog/collaboration-on-vdfs). The VDF Alliance (EF, Protocol Labs, Interchain Foundation, Supranational) worked on RSA VDFs and FPGA hardware. [VDF Alliance wiki](https://supranational.atlassian.net/wiki/spaces/VA/overview)
- EF statement 2023-09-18: the cryptography team had worked on a functional VDF since 2019; the first attempt was an RSA-based VDF needing a trusted setup, and "an audit by ZenGo found an attack on the planned secure multiparty computation for that setup"; the team then moved to MinRoot; a VDF workshop showed VDFs face attacks not thoroughly explored; a three-day cryptanalysis effort showed the round functions of MinRoot, Sloth++ and VeeDo can be parallelised by an index-calculus-style method (2^25 processors, 2^40 memory in a 256-bit field gives a speed-up depending on the latency model); wording: the team does "not recommend using VDFs within Ethereum" at present. [ethresear.ch](https://ethresear.ch/t/statement-regarding-the-public-report-on-the-analysis-of-minroot/16670)
- The statement says the report's security target was that no attacker computes the function more than c = 2 times faster than the reference; a reply notes that D. Bernstein argued in 2019 that residue number systems could give parallel speed-ups across VDF designs. [same thread, replies](https://ethresear.ch/t/statement-regarding-the-public-report-on-the-analysis-of-minroot/16670/2)
- The follow-up paper (Biryukov, Fisch, Herold, Khovratovich and others) was published at CRYPTO 2024: [ePrint 2024/873](https://eprint.iacr.org/2024/873)
- MinRoot design paper: [ePrint 2022/1626](https://eprint.iacr.org/2022/1626)
- A $15 million investment figure appears only in a Spanish secondary report (undated): not confirmed from a primary source.
- Ethereum still uses RANDAO; draft EIP-8321 (hash-chain RANDAO, created 2026-07-05) keeps the "reveal or withhold" bias model. [EIP-8321](https://eips.ethereum.org/EIPS/eip-8321)
- The competitions and bounties asked about: EF-side competition and bounty details not found in primary form here; Chia's own VDF competitions are covered in `production_vdf_deployments.md`.

### Inferences
- A sequential-hash assumption is a hardware-speed assumption; the attack that matters is a speed-up nobody tested for. RSD uses plain iterated Blake3, so the Bernstein/index-calculus family does not apply directly (no algebraic structure), but the question "is there any shortcut to H^T" should be asked in public, not only inside the team. Reasoning, not a finding.
- Cheap, short, focused cryptanalysis (three days) found what four years of design did not. A small paid workshop or open call before the testnet-10 soak is the analogue.

### Gaps
- EF bounty or competition amounts; the workshop date; the report authors list.

---

## Key Question 4: Aptos and Sui on-chain randomness

### Takeaway
Both ship randomness as a consensus-level feature with a threshold key from distributed key generation (DKG) per epoch, and both rely on a way to switch randomness off. Neither shows a public audit or a formal proof of the implementation in the sources found. Sui's May 2026 outage shows that the on/off and restart path is where the bug was.

### Cited Findings
- Aptos: AIP-41 specifies the randomness API; AIP-79 covers the implementation with a weighted DKG and weighted VUF and states the cryptography and its security argument are out of scope because they are in a separate paper. [AIP-79](https://github.com/aptos-foundation/AIPs/blob/main/aips/aip-79.md) (status Draft, 2024-02-12, via search summary; page itself not readable by me). Paper: [IACR news item 2024-02-12](https://iacr.org/news/item/22492)
- Aptos has an on-chain `RandomnessConfig` and a governance recovery path (`force_end_epoch`) and a per-validator local override for the rare case where a randomness bug halts consensus. [Aptos framework docs](https://aptos.dev/move-reference/mainnet/aptos-framework/randomness_config_seqnum)
- Aptos public audit of the randomness code: not found. A zkSecurity audit of the later encrypted mempool (March 2026) exists, a different system. [zkSecurity](https://reports.zksecurity.xyz/reports/aptos-encrypted-mempool). Mainnet enablement date: not found (the announcement post could not be fetched).
- Sui: native randomness available on testnet 2024-06-13, threshold cryptography and DKG at each epoch start; no audit or formal analysis statement and no testnet duration in the post. [Sui blog](https://www.sui.io/blog/secure-native-randomness-testnet)
- Sui incident, 2026-05-28/29: three mainnet outages (about 6.5 h, 3.5 h, 5.8 h). The third: after validators restarted, participation was too low so DKG correctly disabled randomness for that epoch, but a latent bug never saved that fact to disk; after later restarts validators did not know DKG had failed, randomness-dependent transactions could neither run nor be cancelled, and end-of-epoch waited indefinitely. Fixes: persist DKG status, and a mechanism letting validators close a stuck epoch. The post says no user funds were at risk and proposes invariant testing and "failure containment". [Sui postmortem, 2026-05-31](https://www.sui.io/blog/sui-mainnet-halts-resolved-after-major-upgrade)

### Inferences
- The lesson for RSD is about state that must survive restarts and the "failed round" path: Sui's bug was in the unhappy path (randomness off for an epoch), not the crypto. RSD's void and lost-round paths (findings 4.2 item 5) deserve the same attack-test weight as the happy path. On a UTXO chain the state lives in the UTXO, so persistence is by construction, but an unspendable stuck round is the equivalent failure.
- Both chains can force-close a stuck state through governance; a covenant lottery has no such lever, so every stuck state needs a permissionless timeout (consistent with the project's lower-bound-time rule R3).

### Gaps
- No primary source on audits of either randomness stack, on staging between devnet, testnet and mainnet, or on the Aptos rollout date.

---

## Key Question 5: Arbitrum BoLD and Optimism fault proofs

### Takeaway
These are the closest analogues to RSD's fraud-proof half. Both used paper proofs, repeated external audits, a public contest, a long testnet and a human override at launch. Both still had bugs found after launch or between audit rounds. Optimism's story is the most instructive: the fix hard fork was itself not re-audited.

### Cited Findings: Arbitrum BoLD
- Audit trail (Trail of Bits and Code4rena), from Arbitrum's list: initial audit 2023-08-02; fixes review 2024-05-02; Code4rena report 2024-06-17; further reviews 2024-08-05, 2024-10-07, 2024-10-30, and the upgrade payload 2024-12-26. [Arbitrum audit reports](https://docs.arbitrum.io/audit-reports)
- Code4rena contest 2024-05-10 to 05-27, prizes up to $300,500, 31 wardens: 2 High (an adversary can block honest stake retrieval; an edge in a dishonest tree can inherit a timer from an honest tree and confirm a wrong assertion), 2 Medium, 23 low/non-critical. [Code4rena report](https://code4rena.com/reports/2024-05-arbitrum-foundation) (search summary; check at URL)
- Governance proposal: public testnet of at least 8 weeks in the abstract (4 weeks in the timeline section; the two disagree), BoLD live on a Sepolia public testnet 2024-04-15, publication of "mathematical safety proofs and formal specifications", a Security Council "challenge grace period" raised from 1 to 2 days on 2024-04-25, and a Tally on-chain vote posted 2025-01-06 with execution windows after 2025-02-12. Original target was summer 2024, so the schedule slipped by about 6 to 8 months. [AIP](https://forum.arbitrum.foundation/t/aip-bold-permissionless-validation-for-arbitrum/23232); activation reported in [The Block](https://www.theblock.co/post/340278/offchain-labs-releases-arbitrum-bold-on-mainnet-for-permissionless-validation) (secondary)
- Paper with proofs of safety and constant completion time: [arXiv 2404.10491](https://arxiv.org/abs/2404.10491) (AFT 2024 per the findings file). A machine-checked proof (TLA+, Coq, Certora): not found.
- Bug bounty: Arbitrum Immunefi program, critical up to $2,000,000 (10 % of directly affected funds, capped), high $10,000 to $30,000, page updated 2026-09-24. [Immunefi Arbitrum](https://immunefi.com/bug-bounty/arbitrum/)

### Cited Findings: Optimism fault proofs
- Timeline: OP Goerli October 2023; OP Sepolia 2024-03-19; Sherlock contest started 2024-03-27 and found no critical bypass of safety mechanisms (Offchain Labs had reported two "chess clock" issues in `FaultDisputeGame` before it); mainnet permissionless on 2024-06-10 after a governance vote ending 2024-05-30. [Sherlock post, 2024-04-26](https://optimism.io/blog/initial-findings-from-the-fault-proof-program-sherlock-audit); [launch post](https://www.optimism.io/blog/permissionless-fault-proofs-and-stage-1-arrive-to-the-op-stack); dates for Goerli and Sepolia from secondary coverage ([coinacademy](https://coinacademy.fr/actu/op-optimism-integre-fault-proofs-mainnet-decentralisation-stage-1/), [Bankless](https://banklessdao.substack.com/p/optimisms-fault-proof-goes-live-on))
- Launch design: the Security Council can revert to a permissioned system; after a revert all pending withdrawals must be re-proven. [launch post](https://www.optimism.io/blog/permissionless-fault-proofs-and-stage-1-arrive-to-the-op-stack)
- Reversion: about two months later (announced August 2024) the system was moved back to permissioned operation after audits by Spearbit, Cantina and Code4rena found issues; none exploited. Granite upgrade proposed with execution 2024-09-11. [The Block, 2024-08-17](https://www.theblock.co/amp/post/311702/optimism-foundation-disables-permissionless-fraud-proofs-plans-hard-fork-following-security-audits) (secondary); [Granite proposal](https://gov.optimism.io/t/upgrade-proposal-10-granite-network-upgrade/8733)
- Findings fixed (from the proposal): Cantina high (MIPS `mmap` without bounds checks, judged infeasible to exploit); Code4rena H-01 (counter a valid claim with a trace one block longer), H-02 (large-preimage challenge period longer than the clock extension, so challengers could lose bonds), H-05 (a preimage-oracle path that skipped the challenge period); Spearbit medium (out-of-gas could overwrite a valid preimage); several lows. Several bugs are timing and clock bugs in the dispute game, the same class as RSD's window logic.
- Process gaps recorded in the same thread: the hard fork was not audited; the dispute game and MIPS contracts got no fix review ("liveness and reputational risk"); delegates objected that fixes were unreviewed; a stated rubric: if an exploit is costly, disclose and blacklist games; if cheap, activate the fallback before disclosure; the proposal says some fixed issues "were not identified by these audits". [Granite proposal](https://gov.optimism.io/t/upgrade-proposal-10-granite-network-upgrade/8733)
- Bug bounty: critical up to $2,000,042 (10 % of affected funds, capped), high $15,000 to $50,000, KYC required, page updated 2026-10-13. [Immunefi Optimism](https://immunefi.com/bug-bounty/optimism/)

### Inferences
- Even with three auditors and a contest, challenge-timer and bond logic was the bug-rich area. RSD's refutation chain (about 5 acceptance hops, windows W_c, bond burn/split) is the same kind of logic and should get the longest attack-test list and the most outside eyes.
- Fixes need a second review. Optimism's unreviewed fix fork is the anti-pattern; the reviewer-after-every-milestone rule already covers it if applied to bug-fix commits too.
- Contest prize sizes in the low hundreds of thousands of dollars were normal for chain-level protocols; a Kaspa lottery will not have that budget, so the scale-down is a bounded private review plus an open challenge (Section 7).

### Gaps
- Formal verification of either protocol; Trail of Bits report contents; Optimism's Cannon audits other than Sherlock; whether the Granite upgrade executed as scheduled (not confirmed here).

---

## Key Question 6: BitVM2 bridges on Bitcoin

### Takeaway
BitVM2 is the closest design family to a UTXO-covenant fraud proof (pre-signed transaction graph, permissionless challengers, large on-chain disputes). The public record is thin on audits and bugs. What exists is a peer-reviewed paper, partial audit announcements, and open forum critiques of liveness and challenge cost.

### Cited Findings
- Paper "Bridging Bitcoin to Second Layers via BitVM2" (USENIX Security 2026, [ePrint 2025/1158](https://eprint.iacr.org/2025/1158)); security model: existential honesty (1-of-n) at setup, one active rational operator for liveness, permissionless challengers. [USENIX page](https://www.usenix.org/conference/usenixsecurity26/presentation/woll)
- bitvm.org warns the design needs at least one honest operator or funds eventually become unspendable, and that liveness failures can be used for ransom attacks. [bitvm.org/bitvm2](https://bitvm.org/bitvm2)
- Critiques on Delving Bitcoin (informal): open challenges are a griefing vector (challengers should post collateral); basic-design challenge transactions are costly (section 8.5 refers to 4 MB transactions); optimistic payout concerns for Citrea's Clementine bridge (April 2026). [Delving Bitcoin, 2026-04-28 (mirror)](https://mirror.b10c.me/delvingbitcoin-org/2026-04-28-optimistic-payout-in-bitvm-implementations-like-citrea-id2465.html); a worst-case on-chain fraud proof estimated near $16,000 in fees in [Blockworks](https://blockworks.com/news/bitvm3-promises-cheaper-bitcoin-bridges) (secondary)
- Bitlayer's published audit plan: a shared audit of the core BitVM repository under the BitVM Alliance (in progress at the time of writing, page undated), a private audit of the transaction graph and contracts (completed, report pending release), and a node and zkVM audit (scheduled). No findings published. [Bitlayer hackmd](https://hackmd.io/@bitlayer/HJszUV7Lge)
- GOAT Network's testnet post lists an operator double-spend scenario (verifiable but fraudulent state from a forked chain); a project claim, not checked against the paper. [The Defiant](https://thedefiant.io/news/blockchains/goat-network-launches-bitvm2-testnet-marking-huge-step-toward-native-bitcoin-security) (secondary)
- Published bug disclosures or CVEs tied to a BitVM2 bridge: not found.

### Inferences
- The recurring weakness is not the fraud proof itself but who pays to run it (challenger collateral, griefing, free-riding). RSD's findings already name the bystander effect and the burn/refund answer; BitVM2 critiques support giving that point its own attack tests (spam refutations, parallel verification chains, bond-draining).
- Transaction-size and fee assumptions drive cost; for RSD those are Kaspa facts to verify in refs/ and on testnet-10, not from BitVM.

### Gaps
- Audit reports (none public), bug lists, testnet durations.

---

## Key Question 7: Practices for small teams

### Takeaway
Nobody in the survey had a method cheaper than "repeat outside review until findings stop", but a small team can substitute (a) an executable model as oracle, (b) property and differential testing, (c) one small modelled attacker-economics study, (d) a bounded value cap and a public bounty, and (e) a staged launch measured against a named ladder.

### Cited Findings
- Formal models: TLA+ has a documented industrial literature (systematic review of a decade of industrial practice, Bögli et al., IFM 2024; abstract only read). [arXiv 2411.13722](https://arxiv.org/abs/2411.13722) Quint (TLA-based, Apalache model checker) targets blockchain protocols. [Quint repo](https://github.com/informalsystems/quint) Amazon's experience (Newcombe et al., CACM 2015) was named by the search but not read: not verified here. Tamarin and ProVerif: no source gathered in this pass.
- Audit contests: Code4rena states it runs contests at zero platform fee so cost is the prize pool, and recommends a contest paired with a traditional audit (vendor statement, secondary); contests give breadth, private audits depth ([Sherlock](https://sherlock.xyz/solutions/audit-contests)); limits of contests discussed by [Runtime Verification](https://runtimeverification.com/blog/on-the-limitations-of-audit-competitions) and [Consensys Diligence](https://diligence.consensys.io/blog/2024/04/diving-deep-into-audit-contests-analytics-and-economics) (both seen only as titles in search; not read). Public price lists for private audits: not found.
- Bug bounty sizing in the survey: critical = 10 % of directly affected funds with a cap of about $2 million (Arbitrum, Optimism, above); the low tier was $10,000 to $50,000.
- Value caps and fallbacks: Arbitrum's Security Council challenge grace period (1 to 2 days); Optimism's permissioned fallback and "re-prove all withdrawals" cost; Aptos's `force_end_epoch`; Tezos's RANDAO fallback.
- L2BEAT stages (page updated 2025-07-23): Stage 0 needs a proper proof system and at least 5 external actors able to submit a fraud proof; Stage 1 needs that the only way to block or push an invalid message, bugs aside, is compromising at least 75 % of the Security Council, and upgrades outside the council need at least a 7-day exit window; Stage 2 needs permissionless proofs, at least 30 days to exit before unwanted upgrades, and council action only on adjudicable on-chain bugs. [L2BEAT stages](https://l2beat.com/stages)
- Kill switch versus immutability: Optimism's rubric (disclose and blacklist if the exploit is costly, fall back first if cheap) and Sui's need for a force-close show what a switch buys; the cost is a trusted party and, on Optimism, re-proving all pending withdrawals. Stage 2 explicitly limits the switch to adjudicable on-chain bugs.

### Inferences (not findings)
- Agent-based or economic simulation: Cardano's tier table (ant to owl) and this project's attacker-gain tables are the same instrument. A simulation of the RSD game (claimant, watcher, refuter with fee and bond variables, a delayed watcher, an attacker with hardware factor A) would test the "one honest watcher" assumption cheaply; no external source for tooling was gathered.
- This repository's method (attack tests first, executable-spec model as oracle in M5 PREFLIGHT, adversarial reviewer after each milestone, testnet-10 probes) already matches the best small-team substitutes. What it lacks is review from people who did not write the spec.
- A lottery covenant has no governance to patch it, so a value cap (maximum jackpot or round liability) plus an early bounty is the practical substitute for a guardian, and "immutable but capped" is the honest label.

### Gaps
- Typical private audit prices and durations; bounty payout history; tool comparison for Tamarin/ProVerif; the L2BEAT numbers beyond the page (secondary coverage disagrees on council sizes).

---

## Implications for this project

Ordered checklist from research to mainnet for the RSD seed. "Local" = the team and agents can do it; "Outside" = needs people outside the team. Tags: [covered] = the current method already does it, [partial], [gap].

1. **Write the hardware and cryptanalysis question as a public, bounded problem.** One page: the assumption (A_max about 50, no shortcut to H^T), the attacker model, the cost model. Copy Cardano's CPS/CIP split and its "best-effort" wording (KQ2). Local. [partial: the findings file has the content]
2. **Ask outside cryptographers to attack the sequential assumption and the refutation game before code.** A short, focused review (the MinRoot break took three days, KQ3). Outside. [gap]
3. **Measure, do not assume.** Spike S1 (steps per transaction, per-iteration script overhead, fees); verify limits in refs/. Kaspa facts: to verify in refs/. Local. [covered by the spike and the "never from memory" rule]
4. **Build a small executable model of the claim/refute/finalize game and the void paths, plus an attacker-economics run** (bond, burn split, watcher delay, free-riding, spam refutations), tied to BoLD/Optimism timer bugs (KQ5) and BitVM2 griefing critiques (KQ6). Local. [partial: M5-style model/oracle exists as a pattern]
5. **Spec with every unhappy path first** (lost round, no claim, all claims refuted, stuck refutation chain, deadlines), each with a permissionless timeout (KQ4, Sui). Local. [covered by R1-R4 and the spec-first method]
6. **Attack tests first, then covenants, with differential tests against the model and mutants** (KQ5: timer, clock and bond bugs are where audits found issues). Local. [covered: attack tests, mutants, ATTRIB, PREFLIGHT]
7. **Adversarial reviewer on every milestone and on every fix commit** (Optimism's unreviewed fork, KQ5). Local. [covered for milestones; add fix commits]
8. **Testnet-10 soak with a third-party watcher and a hostile claimant**: invite outside people to run a watcher and try to break it, for weeks, not hours (Arbitrum 4 to 8 weeks, KQ5). Local plus a few volunteers. [partial: probes exist, no external adversary]
9. **External review of the final covenants and spec**: one paid private reviewer with covenant or bitcoin-script experience; then an open challenge or contest at a modest prize (KQ5, KQ7). Outside. [gap]
10. **Public bug bounty before mainnet, sized to the value cap** (critical = 10 % of affected funds, capped; KQ7). Outside. [gap]
11. **Fix-review round**: re-review the code that changes after step 9 (Optimism, KQ5). Outside. [gap]
12. **Guarded mainnet launch**: low per-round liability cap, short initial run, publish the L2BEAT-style claims (what is trust-minimised, what is not), and keep the genesis mainnet refusal until the release gate is closed (CLAUDE.md, D100). Local decision, owner approval. [partial: the gate exists]
13. **Raise the cap in steps** only after clean rounds and no open findings; document the ladder. Local. [gap: not yet written]

Which steps the current method covers: 3, 5, 6, 7 fully; 1, 4, 8, 12 partly. What it cannot supply is independence: steps 2, 9, 10, 11 need outside parties. Rank by value: 2, 9 and 8 find the bugs the team cannot see (all five professional programs found their serious defects this way); 10 sets the price of finding them.
