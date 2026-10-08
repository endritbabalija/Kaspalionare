# Milestones

Status: `todo` · `in progress` · `done`. Update the status line when a milestone finishes. Each milestone gets a spec in `docs/specs/` before any code. Section numbers refer to `docs/design-v0.4.md`.

Every covenant milestone also checks value conservation for its transitions (5.5): inputs equal outputs plus caller rewards and fees, with transaction fees paid from external inputs.

Out of scope until later: a real seed source (randomness is parked), economic tuning, any front end, mainnet.

---

## M0 — Feasibility (no protocol code)
Status: done (2026-10-09, see `docs/feasibility.md`)

Clone SilverScript, rusty-kaspa and the KIPs into `refs/`. Read SilverScript's TUTORIAL.md and DECL.md. Answer, with evidence (file paths and line numbers, measurements, links):

1. Can SilverScript express stateful covenants with multiple inputs and outputs and KIP-20 covenant IDs?
2. Can a script verify a sparse Merkle path and compute the updated root from the same path (two passes of about 16–23 hash steps each, 8.2) within script and mass limits? Which BLAKE-family hash opcode is available? Measure it.
3. Can covenant scripts be tested locally with the rusty-kaspa script engine in Rust tests, without testnet?
4. What storage mass does a ticket output have, and what minimum deposit does that imply?
5. Roughly how many shard inputs can one `close()` transaction consume?
6. Which rusty-kaspa release should we pin? Section 2 records v2.0.1 (`cfafeb4c…`) as the baseline and says to pin the production release before implementation. Compare them and recommend one.

Done when: `docs/feasibility.md` answers all six; the chosen release and hash are recorded in `docs/DECISIONS.md`; a Rust workspace exists; `cargo test` runs at least one covenant test in the local script engine; the exact build and test commands are written into `CLAUDE.md`.

## M1 — One shard and `buy()`
Status: todo

Shard state (with game size and ticket price, section 6), ticket output, sparse Merkle counter, `buy()` transition (7.1), including several tickets for the same shard in one transaction.

Done when: valid buys increment the counter and the pot and create tickets under the round's covenant family; tests reject a forged count, the wrong shard, a non-canonical combination, the wrong price, extra outputs and a non-standard payout script.

## M2 — `close()` with one shard
Status: todo

Frozen marker, next-round controller and shard, game-size ladder rule (4.6, 7.2).

Done when:
- the marker records all roots, gross sales S, rolling pot O, ticket count, game size, floor and the seed reference (maximum creation DAA score of the inputs plus the committed offset, stored as an absolute value), and holds S minus the `close()` reward plus O as its value;
- the next round opens in the same transaction with the size chosen by the ladder rule, an empty rolling pot and its "previous round drawn" flag cleared, and new shards that record the new game size and price;
- the `close()` caller reward is paid and value is conserved;
- tests reject a close before the deadline, a close before the previous round is drawn, and a late buy against the old shard.

## M3 — `draw()` with a test seed
Status: todo

Winning numbers from the test seed, tier budgets and slices, jackpot proof, vault settlement for the floor (5.2–5.3), rolling pot, prize pools (7.3).

Done when:
- all outputs match a reference calculation in the tests, for jackpot won and jackpot not won;
- `draw()` adds the new rolling pot to the round r+1 controller, records its floor and sets its flag;
- the protocol fee and `draw()` caller reward are paid and value is conserved;
- tests reject a wrong jackpot proof and the absorption of a pool before its deadline.

## M4 — `claim()`, `reclaim()` and `refresh()`
Status: todo

Claims (7.4), reclaim and refresh (7.5).

Done when:
- duplicate holders split a slice exactly, and a claim pays floor(slice / h) plus the ticket's deposit to the ticket's own payout script;
- the pool decreases by exactly the prize;
- tests reject a double claim, a forged count, the wrong tier, a ticket from another round or covenant family, a redirected or inflated payout, and a claim against an absorbed pool;
- `reclaim()` is rejected before its delay and without satisfying the payout script;
- `refresh()` produces a byte-identical successor and moves no deadline or seed reference.

## M5 — Backer vault
Status: todo

Vault mechanics are **open** in the design (5.4, 14). The spec for this milestone must first propose them; record the choice in `docs/DECISIONS.md` and bring it back for a design update before building.

Done when: deposit and withdrawal requests convert at the next draw's share price, so a new backer cannot capture returns from earlier rounds; tests reject withdrawing capital that secures the current floor before the round is drawn.

## M6 — K shards and limits
Status: todo

Done when: the largest workable K for `close()` is measured; purchase conflict rate is measured; `docs/costs.md` lists size and mass per transition and the minimum ticket deposit and minimum prize.

## M7 — Testnet-10 end to end
Status: todo

Done when several consecutive rounds run on testnet-10, including:
- a game-size change and a duplicated jackpot combination;
- backer deposit and withdrawal;
- a claim built from a shard tree the wallet kept during the round, more than 30 hours later;
- a claim built from an independent indexer copy;
- a round that overruns 30 hours because a `draw()` is late, and still sells using an indexer copy (section 9).
