# Research brief: on-chain-only seed fairness (handoff for a fresh session)

Written 2026-10-13 at the end of the design session that produced D108 (`docs/DECISIONS.md`). Milestone entry: `docs/milestones.md` "Research — on-chain-only seed fairness (before M8)". Read `CLAUDE.md` first; then design §11, §4.6, §10; spec `docs/specs/7.md` A6; DECISIONS rows D100, D100 amended, D106, D107, the margin-floor row and D108. Numbers in `docs/costs.md` "Spike VDF+ZK" and "Margin floor".

## The question

The winning numbers come from the hash of the anchor block X (the first selected-chain block with DAA ≥ D, D fixed by the marker). The miner, in practice the pool operator, who finds X sees the numbers first and can withhold X; the next block becomes the anchor instead. Find a mechanism that the covenants alone enforce, or a tighter bound, that makes this re-roll unprofitable for a pool operator with 30–35 % of hashrate, without a VDF, a house secret, an oracle or any per-round off-chain actor, and without pricing the game the way a 1.5 margin floor does. Or show that none exists under the non-negotiables and quantify the residual.

## Facts (verified in `refs/`, cite them again, never from memory)

- A withheld block stays blue, and paid, only while its blue anticone is at most k = 124 (`refs/rusty-kaspa-v2.1.0/consensus/core/src/config/bps.rs:40`; rule `consensus/src/processes/ghostdag/protocol.rs:114`). At 10 BPS that is about 12 s. A red block's subsidy goes to the merging block's miner (`consensus/src/processes/coinbase.rs:117-132`). So withholding is free for about 12 s and costs one subsidy after.
- Finality 43,200 s (`consensus/core/src/config/constants.rs:70`); `OpChainblockSeqCommit` admits only a selected-chain block within that depth and re-evaluates after a reorg (KIP-21 in `refs/kips`).
- Script integers are 64-bit; no big-integer or pairing ops; Groth16 and RISC Zero only through `OpZkPrecompile` (140,000 / 250,000 grams plus 2,500 per Groth16 public input; `crypto/txscript/src/zk_precompiles/`). Compute limit 500,000 grams per transaction; the worst `draw()` is at 41.3 % today.
- The anchor time is predictable to within seconds once `close()` is accepted (D = creation DAA + 600).

## The model (spec 7 A6, D100 amended)

Attacker with hashrate share α holding c combinations at price p: about α/(1−α) extra samples per round, each worth c·EV_t. Profit per round ≈ c·p·[α/(1−α)·(1−ε) − ε], ε = the attacker's expected loss fraction per ticket. Profitable iff α > ε. ε is set by the game-size rule: where the whale check binds, ε = 1 − 0.79/margin − (returned shares), with the worst attacker (self-referring, calling `close()` and `draw()`) getting back s_t + s_R + s_C + s_D = 21 % of the price. Table of ε by floor in `docs/costs.md` "Margin floor": at 1.00 the worst ε is 0; at 1.256 it is 0.158; at 1.5 it is 0.27 (and an ordinary player then loses 37 % instead of 27 %); beating a 34 % pool needs about 1.76. Levers: withholding, and leaving an honest candidate out of one's parents (cheaper, same bound). A bound on J does not help: the gain scales with c.

## Measured on 13 Oct 2026 (web, undated pages; D108 lists the sources)

Network 332–366 PH/s. HumPool 27.6–27.8 % (33.9 % on one aggregator), F2Pool 26.6–27.2 %, ViaBTC about 13 %; top two 55–60 %. No dated 12-month series; rentable kHeavyHash not measurable; attribution method unstated. Re-measure, with method, as part of the session if a candidate depends on the figure.

## Ruled out, with the reason (confirm or overturn, with evidence)

- VDF (any variant): the owner will not run a prover, publish a key or hold a ceremony; every variant needs at least the ceremony and a prover every round (D106, D107, D108).
- House secret (commit at genesis, reveal per round): keeper bot, trusted house, stalling lever on a jackpot hit, leak to a miner (D108).
- Commit-reveal by the closer or by players: the revealer can be the miner.
- Several anchors or a hash over many blocks: the last miner keeps the last word.
- A bound on J: gain scales with c, not J (D100).
- Outside beacon (drand): no native BLS12-381; only via the ZK route (`docs/research/randomness-sources-on-kaspa.md`).
- Void penalty on the vault: every void becomes a loss of L for the backers and a griefing lever (D108).

## Angles not yet examined (starting points, not conclusions)

- Make the anchor time unpredictable so burst rental and pre-positioned hashrate do not count (for example an anchor chosen by a condition on block hashes over a long window), and model whether a sustained share of 30 % still pays.
- Change what a re-roll is worth: anything that bounds c per round in a way the covenant can enforce (per-shard or per-round caps are already partly there: K shards, B_MAX = 4 per buy), or that makes the attacker's tickets lose more than everyone else's (the returned shares s_R, s_C, s_D are 11 % of the lever).
- Use more of what `OpChainblockSeqCommit` commits (read KIP-21 in `refs/kips` for the exact fields) so that the seed depends on data the miner of X does not control alone; check whether any field is fixed before X's PoW.
- Literature on randomness from PoW blockchains with fast blocks and DAG ordering (GHOSTDAG), and on withholding economics when the withheld block stays rewarded.
- Quantify the residual honestly: at a given floor, the per-round gain of a 28 %, 34 % and 55 % operator as a fraction of its ticket stake, and what ticket volume would make it worth a pool operator's reputation.

## Output

A DECISIONS row per conclusion; a proposed floor and gate threshold for the owner with the player-loss and jackpot effect of each candidate; if a mechanism is found, a milestone spec outline before any code. No edits to `docs/design.md`; no covenant changes.
