# How lotteries (regulated and on-chain) handle draw timing, jackpot announcement and randomness (status as of 17 October 2026)

Scope note: research only. Primary sources preferred; secondary sources are flagged "(secondary)". "Not found" means my searches did not surface it; it does not mean it does not exist. Several fetches returned only search-snippet level detail; those are flagged. Searches were broad but shallow (one pass per topic), so treat Gaps as real work items.

Headline summary:
- Regulated lotteries do not wait hours between sales close and draw, but they do accept gaps of minutes to about 1-2 h (Powerball retail cutoff "typically 1-2 hours" before the 10:59 pm ET draw; Florida's own rule closes terminal sales at 10:00 pm ET, QuickTickets at 9:59 pm ET). They publish the next jackpot as an ESTIMATE, explicitly not guaranteed, set by the game group from projected sales and interest rates, and revised on draw day. Players buy against estimates routinely.
- Regulators do not ask for a specific delay. They ask that the outcome be unpredictable, not influenceable by the operator, independently tested and auditable. The UK RTS 7 text on lotteries that use external events is the closest match to an on-chain beacon draw.
- On-chain production lotteries accept a trusted oracle (Pyth Entropy, Chainlink VRF) and a seconds-to-minutes gap. None of those I checked uses a time-delay function. The documented attack classes are: public block data (Fomo3D, SmartBillions, EOS dice), provider withholding/re-roll (Pyth docs, Chainlink VRF guidance and its 2022-23 critical bug), and re-requesting.
- A 2-6 h delay is long relative to on-chain norms (seconds to minutes) but short relative to what regulated draw games already do between ticket purchase and result (days). The new element is not the delay but selling the next round during it.
- Nothing about randomness or lotteries in the Kaspa ecosystem was found in primary form (KIPs, forum). One unreviewed Substack paper only.

---

## Q1. Regulated lotteries: cutoff, results, jackpot announcement, advertising rules

### Takeaway
All large draw games sell next-draw tickets only after the current draw (sales window is closed for the draw, reopens for the next). Jackpots are shown as estimates, with a stated disclaimer; only a few games carry guaranteed minimums. Estimates move up on draw day if sales run strong. Nobody is liable for the difference.

### Cited Findings
- Powerball advertised jackpot = "estimated annuitized Jackpot Prize amount as determined by MUSL prior to the Jackpot Prize drawing"; "not a guaranteed prize amount"; no guaranteed minimum for ordinary drawings; neither MUSL nor the state lottery "shall be responsible or liable for the difference"; unawarded jackpot money rolls into the next drawing. — Florida Admin. Code 53ER25-13, paras 2(a), 9(a), 9(e) (retrieved 2026-10-17) https://www.law.cornell.edu/regulations/florida/Fla-Admin-Code-Ann-R-53ER25-13 . Vermont's Powerball rule uses near-identical "Advertised Grand Prize" wording: https://law.cornell.edu/regulations/vermont/01-002-Code-Vt-R-01-130-002-X
- Draw schedule and cutoff in Florida: draws Mon/Wed/Sat; terminal tickets sold after 10:00 pm ET count for the next drawing; QuickTickets close 9:59 pm ET (same Florida rule, paras 4(a), 15(c)). The draw itself is 10:59 pm ET in Tallahassee. — https://www.powerball.com/powerball-jackpot-increased-to-850-million (dated 2023-09-27)
- Official Powerball statement: "Ticket sales cut-off times vary by jurisdiction", generally one to two hours before the drawing (same page).
- Estimate revised on draw day: on 2023-09-27 the annuity estimate was raised from $835M to $850M "citing strong ticket sales" (cash value "estimated at $397.4 million"). — same page.
- Annuity and cash options both quoted "before taxes", annuity "30 graduated payments over 29 years". — https://www.powerball.com/powerball-prize-chart
- 2020 change: MUSL suspended minimum starting jackpots and minimum increases between rolls; starting jackpots and increases are "based on estimated sales and interest rates". — Louisiana news report (secondary; redirect not fetched) https://www.katc.com/news/covering-louisiana/powerball-suspends-minimum-starting-jackpots . Confirm against MUSL rules.
- Mega Millions: group members meet Tuesday and Friday mornings to set estimated jackpots for the next two drawings from projected sales; annuity rate tracks 30-year Treasury. Winning numbers posted shortly after the drawing; winner counts next morning after lottery security verifies sales. Retail cutoffs vary by state, e.g. 15 min before in about half of jurisdictions, Colorado 7:30 pm ET, Indiana 10:44 pm ET. — (secondary) https://www.clickondetroit.com/news/2023/08/08/mega-millions-drawing-on-tuesday-aug-8-2023-what-time-how-to-find-winning-number/ ; https://wgntv.com/news/how-late-can-you-buy-mega-millions-tickets-for-800m-jackpot-it-depends-where-you-live/amp/
- Daily games: cutoff is minutes before the draw. South Carolina Pick 3 evening: sales suspended 6:45 pm for a draw between 6:59 and 7:00 pm; Oklahoma 8:59 pm for 9:00 pm CT; Nebraska 9:00 pm for 10:00 pm CT. A ticket bought after cutoff is valid for the next draw. — (secondary roundup) https://www.lotteryvalley.com/oklahoma/games/pick-3 ; SC rules PDF https://dc.statelibrary.sc.gov/server/api/core/bitstreams/145ca5c8-9842-46b9-87b0-87cd49233039/content
- EuroMillions: draws Tuesday and Friday 20:45 BST; UK sales close 19:30 BST (75 min gap); jackpot cap EUR 250M. — (secondary aggregators, not official rulebook) https://www2.featurespace.com/euro-lottery-results-friday-p9ma.html
- Draw certification in the Powerball rule: results official "after certification by the Draw Manager and the Accountant" (Double Play para 12(h)3, Florida rule above).

### Inferences
- The structure "sales closed for draw N, draw later, sales for N+1 open at once, N+1 jackpot shown as estimate" is the industry norm. Our option "show a minimum, 'at least X'" is stricter than Powerball (which shows a point estimate with no floor). Our option "fix at worst case at close" is a stronger form of a guaranteed jackpot, which the industry uses sparingly.
- Powerball's practice of revising the estimate on the draw day shows players tolerate a moving number; but the revision is upward by sales, never a function of the previous draw's winner (the prior draw has already happened by the time the next is on sale).

### Gaps
- MUSL Powerball Group Rules (the actual PDF) and the Mega Millions rulebook not retrieved; only state rules incorporating them. The "ban/limits on advertising estimates" question is not answered by any primary document I found. The UK National Lottery (Allwyn) Lotto rules, official cutoff and guaranteed-jackpot terms: not found (results were business news). Official EuroMillions cutoff: not found in primary form.

---

## Q2. Regulated RNG standards

### Takeaway
GLI-19 and the UK Gambling Commission RTS 7 require unpredictability, independence between draws, a cryptographically strong source, source-code review and statistical testing by an independent laboratory. None sets a minimum or maximum delay between sales close and draw. RTS 7 has an explicit clause for lotteries whose result comes from an external event: it must be unpredictable, publicly available, externally verifiable and not influenceable by the operator.

### Cited Findings
- GLI-19 v3.0 (rev. 17 July 2020), section 3.2.1: the independent test laboratory reviews source code of "any and all core randomness algorithms" for "sources of bias... malicious code... undisclosed switches". 3.2.2: statistical tests evaluated "at a 99% confidence level". 3.2.3: each selection equally likely; scaling unbiased. 3.2.4: "Knowledge of the numbers chosen in one draw shall not provide information on the numbers that may be chosen in a future draw." — https://gaminglabs.com/wp-content/uploads/2024/06/GLI-19-Interactive-Gaming-Systems-v3.0.pdf (text extracted locally)
- GLI-19 3.3.1-3.3.2: RNG must be "cryptographically strong", resistant to an attacker "who may have knowledge of the source code"; direct cryptanalytic attack, known-input attack ("shall not be seeded from a time value alone") and state-compromise extension attack (periodic reseeding from external entropy). Section 3.3.3: hardware RNGs need dynamic output monitoring that disables play on degradation. (same PDF)
- GLI-19 4.4.11 (Keno/Bingo/Lottery): drawn numbers shall be clearly identified and displayed to the player. (same PDF)
- A GLI-19 v4.0 DRAFT dated 2026-07-07 lists sections 5.2 RNG general requirements, 5.3 strength and monitoring, 5.4 mechanical RNGs and 5.5 "provably fair" RNGs. Draft only; text not read. — https://gaminglabs.com/wp-content/uploads/2026/07/EN-GLI-19-v4.0-DRAFT-2026-07-07.pdf (contents seen in search result only)
- UK Gambling Commission RTS 7A: outcomes "acceptably random", no adaptive (compensated) behaviour. For lotteries using external events: outcome "must be unpredictable and externally verifiable"; events "may reasonably be assumed to be random", "unable to be influenced by the lottery operator (or external lottery manager)", "publicly available and externally verifiable". Guidance 7A.4: seeding "do[es] not introduce predictability". — https://www.gamblingcommission.gov.uk/manual/remote-gambling-and-software-technical-standards/rts-7-generation-of-random-outcomes
- RTS applies mainly to remote lotteries above GBP 250,000 per year of remote entries; ancillary licences are exempt. (secondary summary from search, same domain)
- drand states its randomness is used in "an RNG certified by Gaming Labs International for several lottery games" — drand claim, not verified at GLI. https://www.drand.love/about

### Inferences
- A regulator looking at an on-chain draw would map our seed onto the RTS 7 external-event clause: is the anchor block hash unpredictable (yes ex ante), externally verifiable (yes), not influenceable by the operator (we have no operator, but a miner can influence it: this is exactly the OPEN bias issue). The proposed sequential-hash delay is the cure to the "influence" limb; the regulator would then ask about the delay function's own assumptions.
- GLI 3.2.1 (source review) and 3.3.2 (attacker knows the source) map to compiled attack tests and an external audit of covenants. "Periodic reseeding from external entropy" maps to the fresh block hash per round.
- Neither document requires a certain minimum delay; the 2-6 h gap is a product choice only.

### Gaps
- GLI-11 (gaming devices; draw equipment) and GLI-33 (event wagering) text not retrieved; no result on GLI-33. GLI-19 v4.0 sections 5.2-5.5 not read. No jurisdiction's rule on blockchain-based draws found. Draw-procedure rules for draw-based games (witness, independent auditor present, video) not retrieved: the Florida Powerball rule names only the Draw Manager and Accountant.

---

## Q3. On-chain lotteries in production

### Takeaway
All checked production systems take a trusted or semi-trusted oracle and accept a draw gap of seconds to minutes. None uses a delay function. Jackpot display details are mostly not documented in primary sources.

### Cited Findings
**MegaPot (Base)**
- $1 USDC tickets as ERC-721, draw every 24 h; `drawingTime` per drawing; "Tickets can't be bought once drawingTime has passed". Settlement is triggered externally via `Jackpot.runJackpot()`, which requests randomness from the Pyth entropy provider and pays an ETH fee; a "usually short delay (seconds to minutes)" until the callback sets the winning numbers. On settlement a new drawing starts automatically under the next drawing id; winners call `claimWinnings`. The docs state "a Pyth-seeded randomness oracle selects winners". — https://llms.megapot.io (retrieved 2026-10-17)
- Docs site: "1 drawing every day", "Results are posted immediately after each drawing", prize pool backed by depositors (LPs). — https://docs.megapot.io and https://docs.megapot.io/getting-started/how-it-works.md
- Economics (Bankless, opinion piece, investor in Megapot, 2026-04-04, secondary): launch pool $1.1M, jackpot about $220k (about 20% of pool, "the pool must be capitalized to cover every possible winning combination"); per $1 ticket about 70c to players, 20c to LPs, 10c to referrer; LPs lose principal when a jackpot hits. — https://www.bankless.com/first-global-lottery-2
- Jackpot formula, how the next jackpot is shown, and guaranteed-minimum-payout calculator: the llms page names a `GuaranteedMinimumPayoutCalculator` contract but gives no formula; not found.

**Pyth Entropy (Megapot's source)**
- Two-party commit/reveal with provider hash chain. Trust assumptions: providers "are trusted to reveal their random number" regardless of outcome; observing provider can manipulate; leaked chain makes results predictable. Known limitation: provider can compute the result off-chain before revealing, enabling withholding of unfavorable outcomes. Reorgs not discussed. — https://docs.pyth.network/entropy/protocol-design

**PoolTogether V5**
- Daily draws. Two timed auctions (Start RNG, Finish RNG) pay third parties to request randomness and award the draw; a variable waiting time while the RNG service fulfils; prize pool is "fully autonomous and immutable". — https://dev.pooltogether.com/protocol/design/draw-auction ; https://dev.pooltogether.com/protocol/design/prize-pool
- Production RNG at audit time (2023): Chainlink VRF v2 Direct Funding; blockhash only in test environments. — Code4rena audit (secondary, 2023) https://code4rena.com/reports/2023-08-pooltogether . Current RNG provider not confirmed (docs name Witnet and Chainlink as examples).

**Chainlink VRF**
- "Do not allow re-requesting or cancellation of randomness"; pick confirmations so rewrite attacks are unprofitable; a reverting `fulfillRandomWords` is not retried. — https://docs.chain.link/vrf/v2-5/security
- A critical bug (malicious or compromised subscription owner could block and reroll randomness until the desired value) earned a $300,000 bounty; fixed afterwards; reported 2023-11-15. — (secondary) https://cryptoslate.com/chainlink-vrf-vulnerability-thwarted-by-white-hat-hackers-with-300k-reward/

**Solana**
- ORAO: nodes sign values and combine on Byzantine quorum; fulfilment typically 4-20 s. — (secondary profile) https://solanacompass.com/projects/orao-network
- Switchboard: TEE (SGX) oracle, commit (previous slot hash as seed) then reveal then consume. — (secondary, QuickNode guide updated 2026-05-22) https://www.quicknode.com/guides/solana-development/3rd-party-integrations/generate-onchain-random-numbers-with-switchboard-vrf
- dRaffle fixes the slot blockhash at the end time as the seed (example of the weak pattern). — https://docs.rs/crate/draffle/0.1.0 (secondary)

**drand**
- Threshold BLS beacon, League of Entropy since 2019; 3 s mainnet chain supports timelock encryption (tlock); the older 30 s chain does not. No live on-chain lottery built on it found. — https://www.drand.love/about ; https://beta.pkg.go.dev/github.com/513th/tlock

### Inferences
- Production on-chain lotteries have effectively zero sales-close-to-result gap (minutes), and sell the next drawing immediately. MegaPot is the direct comparison to our model: new drawing starts on settlement, so its next jackpot is determined only after settlement (primary docs do not say otherwise, but do not say how). I could not establish whether MegaPot sells next-drawing tickets before settlement.
- Withholding by the randomness provider is the shared weakness (Pyth, Chainlink owner bug). Our design's analogue is the anchor miner; the delay function removes miner bias at the cost of a liveness assumption on someone computing the delay.

### Gaps
- MegaPot jackpot size rule and "how-to-verify" page; whether tickets for drawing N+1 can be bought between drawing time and settlement. PoolTogether's current RNG. ORAO and Switchboard primary docs. Wider survey (Solana raffle programs, other Base/EVM lotteries) not done.

---

## Q4. Randomness exploits and industry lessons

### Takeaway
Every successful exploit used a value either public at transaction time, predictable from block fields, or controllable by a party who could choose whether to proceed. Fixes converged on: separate commit and reveal, a future unknown value, no retraction or re-request, and independent randomness sources.

### Cited Findings
- Fomo3D airdrop: "random" number derived from block data and caller address was predictable; a wrapper contract could check the same value first and only enter on a win; banning contract callers was bypassed. — Quantstamp (2022-02-16, secondary) https://quantstamp.com/blog/proper-treatment-of-randomness-on-evm-compatible-networks ; Apriorit (secondary) https://apriorit.com/dev-blog/556-fomo3d-vulnerability
- Quantstamp: commit-and-reveal needs locked outcomes, no retractions, no repeat commitments, and a block-based lock so commit and reveal cannot resolve in one block; miners and Flashbots can also exploit weak randomness. (same article)
- SmartBillions: historical blockhash as the winning number; `blockhash` is zero for blocks older than 256, so a player could claim after the window. I found only the general blockhash limitation in a static-analysis note, not a SmartBillions post-mortem; SmartBillions details are from my background knowledge and unverified here.
- EOS dice: EOSDice and others derived randomness from TAPOS block prefix/number and time; attackers recomputed outcomes. A 2019 SlowMist report describes a further attack based on the EOS CPU resource model. — (secondary) https://steemit.com/eos/@kirill-yurkov/how-to-hack-an-eos-casino ; https://slowmist.medium.com/details-of-a-new-type-random-number-attack-on-eos-ede0211d9cc2 ; https://www.apriorit.com/dev-blog/588-eos-smart-contract-vulnerabilities
- Chainlink VRF subscription-owner reroll: see Q3.
- Academic survey: "From Fomo3D to Lottery DAPP", arXiv:2508.12303 (text not readable by my fetcher). https://arxiv.org/pdf/2508.12303

### Inferences
- Lessons applying to our design: (1) the seed must be unknowable when tickets are bought and after counts are frozen (already R-rules and `close()`); (2) no actor may choose whether to reveal (our "missed window ends in a void" rule is the fix for withholding, but a void still lets a withholder cancel an unfavorable round: the cost of that option is the economic question); (3) no re-request path.
- The delay function's main purpose is eliminating the "anchor miner may withhold or grind" class, the same class as the Pyth withholding limitation.

### Gaps
- SmartBillions and Fomo3D primary post-mortems (contracts, amounts) not retrieved. Post-2023 exploits not surveyed.

---

## Q5. Player behaviour evidence

### Takeaway
Weak direct evidence. Jackpot size strongly drives sales, and there is evidence that some players prefer delayed resolution. Nothing found on estimated vs guaranteed jackpots or on a hours-long delay.

### Cited Findings
- Jackpot size predicts sales: Toronto postcode data 2012-2015, large effect, weaker in low-SES areas. — https://tandf.figshare.com/articles/journal_contribution/Socioeconomic_correlates_of_the_lottery_rollover_effect_in_Toronto_Canada/25061274 (abstract only)
- Powerball/Mega Millions redesigns lowered the elasticity of sales revenue to the advertised jackpot while raising the average jackpot (2010-2019 data). — https://www.informahealthcare.com/doi/full/10.1080/00036846.2024.2348180 (abstract only)
- Kocher, Krawczyk, van Winden ("Let me dream on!", 2009 WP, published 2014): a substantial minority of subjects chose delayed lottery resolution for anticipatory thrill; most preferred spreading chances across two draws. — https://econpapers.repec.org/RePEc:dgr:uvatin:20090098 ; https://cepr.org/publications/DP7715
- Guryan and Kearney: retailers that sold a big winner see 12-38% sales gains next week, growing with jackpot size. — https://www.nber.org/papers/w11287

### Inferences
- Advertised (estimated) jackpots clearly drive purchases in regulated games, so players buy on estimates. The estimates in those games are point values; a stated floor is a weaker, more honest signal and would likely be read as "at least".
- A delay costs little among players who like anticipation, but results arrive at a known time in all regulated games; a variable 2-6 h delay is less standard than a fixed time.

### Gaps
- No study of sales before result vs jackpot uncertainty; no on-chain data on how MegaPot sales respond to jackpot level or to settlement latency.

---

## Q6. Kaspa ecosystem

### Cited Findings
- No KIP, Kaspa forum thread or KRC-20 project on lottery randomness found. Toccata activated on mainnet 2026-06-30 at DAA score 474,165,565; specs KIP-16, KIP-17, KIP-20, KIP-21 (secondary summary). — https://docs.kaspa.org/toccata ; https://medium.com/@michaelsuttonil/kaspa-covenants-toccata-hard-fork-outlook-a4d81a40900c
- A self-published Substack paper proposes a hybrid beacon mixing Bitcoin and Kaspa block hashes with optional quantum RNG; not peer reviewed. — https://kodinglsfun.substack.com/p/a-verifiable-cheatresistant-hybrid
- A daily-activity snapshot (July 2026, secondary) lists Igra L2, Kasplay, Kasplex L2 and KRC-20 as active, nothing lottery-specific. — https://www.kucoin.com/news/insight/KAS/6a5e7a0cdd913100071d0c83

### Gaps
- Search of the rusty-kaspa KIP repo and Kaspa Research forum directly not done (the tool only gave web-search hits). Treat "no prior art" as unconfirmed.

---

## Implications for our protocol

1. **Draw schedule.** Industry practice: gap between sales close and draw ranges from a minute (daily games) to about 1-2 h (Powerball retail); on-chain production lotteries: seconds to minutes. No regulator or standard sets a number. A 2-6 h delay is longer than anything I found but not unreasonable if announced up front. Recommendation (judgement, not evidence): fix the delay to a published constant, with a stated draw-by time, rather than a variable range; prefer the low end (about 2 h) unless the delay-function security argument needs more; announce the draw time with the round. Player-facing framing: "result at about close + 2 h", and show a countdown.
2. **Jackpot option.** Regulated games sell the next draw as an estimate with a disclaimer and no obligation; they do not guarantee. But in those games the previous draw is complete before the next goes on sale. Our case is different because round r+1's jackpot depends on an unresolved draw. The closest industry analogue is a guaranteed minimum (Mega Millions/Powerball starting jackpots, EuroMillions guaranteed floor), which is advertised as "at least". Fixing at the worst case at close (option 2) gives a number that is final the moment sales start, avoiding any misleading-advertising question, at the cost of a lower headline. Showing "at least X, Y if round r unwon" (option 1) is legal-comparable to estimate-with-disclaimer practice but creates two numbers during the delay. Recommendation: option 2 (fix at worst case) matches the regulatory instinct that the advertised prize must be one the operator is bound to pay, and Powerball's own rules protect the operator only because the figure is labelled an estimate; an on-chain covenant cannot disclaim. If option 1 is chosen, label the lower figure as guaranteed and the higher as "if round r is unwon", and never display the higher figure in the headline.
3. **What a regulator or auditor would ask of the randomness.** From GLI-19 and RTS 7:
   - Is the source unpredictable at purchase time and unaffected by the operator or any single party who can choose whether to proceed (RTS 7A external-event clause)? Our answer needs the miner-withholding bias closed or bounded; the delay function targets that.
   - Is the source externally verifiable and publicly available (yes: chain data and a 272-byte proof)?
   - Does a missed or withheld draw have a defined, fair outcome (void and refund rules), and can any party re-request or retract (Chainlink guidance: no)?
   - Source-code review and independent testing: independent audit of covenants, and statistical testing of the mapped combination outcome, including the scaling from hash to combination (GLI 3.2.2-3.2.4).
   - Seeding: no time-only seed; fresh entropy per round (GLI 3.3.2 b, c).
   - Audit trail: all inputs (seed block, proof, counts) on chain; results posted; the delay-function assumptions and any trusted parameters disclosed.
   - Draw scheduling and certification: fixed published draw time; clear rule for what happens if the draw is late.
4. **Open items for follow-up research:** MUSL Powerball Group Rules (advertising and estimate clauses), Allwyn National Lottery rules, MegaPot jackpot and next-drawing sale behaviour, GLI-19 v4.0 5.5 "provably fair", and a direct search of Kaspa KIP/forum material.
