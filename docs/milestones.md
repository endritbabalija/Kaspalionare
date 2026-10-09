# Milestones

Status: `todo` · `in progress` · `done`. Update the status line when a milestone finishes. Each milestone gets a spec in `docs/specs/` before any code. Section numbers refer to `docs/design.md`.

Every covenant milestone checks value conservation for its transitions (5.5). It is stated over economic value, with reserves R moving per the reserve rule. Inputs must equal outputs plus referral, fee, house share, boost pot and caller rewards. Transaction fees are paid from external inputs.

Proposals in `docs/DECISIONS.md` marked "confirm in spec Mx" are settled in that milestone's spec before any code.

Out of scope until later: a real seed source (randomness is parked), economic tuning beyond the starting values, ticket transfer, any front end, mainnet.

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

## M1 — Shard, bundles and `buy()`
Status: todo

**First task, before any protocol code: mass-limit check (design section 12; research G1–G3).**
- Determine which per-transaction mass limits (storage, transient, compute) the v2.1.0 mempool enforces.
- Measure M0's probe shapes for `buy()` and for `close()` at K = 16/32/64, plus a `draw()`-sized probe, with the node's mass calculator. Use worst-case caller funding: KIP-9 input relief depends on the caller's inputs and cannot be relied on.
- Submit each through a testnet-10 node's mempool, with real signatures on the funding inputs (M0's OpTrue stand-in is non-standard).
- Record that the wallet SDK generator caps transactions at 100,000 mass, so our builder must build protocol transactions itself.
- Repeat the check with the real `close()` and `draw()` in M2, M3 and M6.
- Record in `docs/DECISIONS.md` whether d ≈ 0.08 KAS, R = 0.08 × (K + 2) and the largest K (62/37/23) hold under relay. If they don't, recompute them before writing the spec.

The spec also settles five open decisions:
- the covenant family structure;
- the shape policy and who pays fees;
- output binding through the authorized-output index (R2);
- the late-buy bound (7.1);
- D_min, with M2.

It adopts R5–R8 and the Merkle rules (domain tags, flat sibling string, BLAKE3 vs BLAKE2b benchmark).

Fixed 5/30 + bonus game (4.1). Shard state with m, price, d, s_R, S and P_R (section 6). Counter leaf keyed by the 5-number rank with per-bonus counts, and the empty-leaf rule (8.2). Bundle ticket. `buy()` (7.1) in any shard, with the ticket split and the optional referral output (5.1). Fee accumulation starts here: the shard records S and P_R, from which `close()` and `draw()` derive the fee, house share, caller rewards and boost pot. The spec confirms the leaf layout, b_max and M_ref.

Done when:
- valid bundles increment each combination's bonus count and the shard's S and P_R;
- the shard value grows by exactly b × p − r;
- the ticket carries the bundle, a zero claimed bitmap, d × b and its payout script, under the round's covenant family;
- a bundle that repeats a combination, or combinations sharing main numbers, updates correctly;
- a referral output pays exactly floor(b × p × s_R / 10⁴) and only when ≥ M_ref; omitting it leaves the share in the shard;
- value is conserved: the shard value + referral output + ticket deposit equals the buyer's spend, minus change and fee; a test derives F, H, ΣB_t and b_r from S and P_R and checks b_r ≥ 0;
- the ticket records the round deadline T;
- tests also reject a parallel authorization group with the same covenant ID, an extra covenant output, a wrong-role or wrong-round input, a wrong-template read, a malformed sibling string, and a second buy on a shard created after T;
- tests run with production engine flags and real compute budgets, and an under-funded compute budget is rejected;
- tests reject a forged count, a wrong leaf or root, a non-canonical combination, a bonus outside 1..m, b = 0 or b > b_max, the wrong price or deposit, an inflated, deflated or below-threshold referral, a non-standard payout or referral script, and extra outputs;
- measured and recorded in `docs/costs.md`: shard script size per bundle slot (and the K it allows at `close()`, feasibility Q5), and the minimum referral output M_ref.

## M2 — `close()`, game-size rule and admin parameters
Status: todo

Frozen marker, next-round controller and shards (7.2). Game-size rule: edge rule, whale check, m_min/m_max, estimate Ĵ (4.6). Parameter snapshot and `set_params()` (5.6). The spec sets the open parameters (s_t split and paying tiers, margin within the 4.6 bound, m_min, m_max, cap, s_C/s_D) and confirms the whale-check and parameter-storage proposals. It also lists the arithmetic order and maximum values, with range checks (R8). The admin signature covers domain tag, covenant ID, target round, parameters and nonce, and the controller's nonce is monotonic. The spec decides research item K3: whether the admin spends the controller or a separate parameter UTXO. `close()` is built as leader with the shards as delegates (R6). `close()` sets T(r+1) = max(schedule, its DAA + D_min); a test shows that a late `close()` still opens a round that sells.

Done when:
- the marker records all K roots, S, P_R, m, J, J_boost, G2, the shares and the seed reference (the maximum creation DAA score of the inputs plus the committed offset, stored as an absolute value). It holds S − P_R − C_close + J_boost;
- the next round opens in the same transaction, with:
  - m(r+1) = the smallest m in range that passes both checks at Ĵ, or m_max if none does, matching a reference calculation over a grid of vault values, boost pots and prices;
  - the boost pot and fee accumulator carried over;
  - the flags cleared;
  - K shards recording m, the price, d and s_R;
- pending admin parameters are applied to the round being opened and never to an open round;
- the caller receives C_close in its own output; reserves follow the reserve rule; value is conserved;
- tests reject:
  - a close before the deadline, or before the previous round is drawn;
  - a late buy against an old shard;
  - a wrong m (too small, too large or out of range);
  - a marker or controller with altered sums;
- `set_params()` tests reject s_F > 10%, s_R > 15%, a second change in one round, a wrong key, a replayed or stale-nonce signature, a signature for another round or covenant ID, and any change to other fields or to the controller's value;
- `close()` at the largest K runs with worst-case scripts within the mass limits found in M1; a shard dragged into a close under a different leader route is rejected.

## M3 — `draw()` with a test seed
Status: todo

Winning numbers from the test-only seed. Lower-tier budgets and slices. Jackpot and second-prize counts from K leaves (8.4). Vault settlement (5.4). Fixing J(r+1) with base, hold-back and top-up (4.6). Boost pot. Fee payout. Prize pools and absorption (7.3). The spec confirms the J-fixing, top-up, fee-threshold and cap proposals, and the tier split.

Done when:
- all outputs match a reference calculation for these cases:
  - jackpot held once, held several times and not held;
  - n2 = 0, 1 and m − 1;
  - a boost pot below and above the safe level;
  - a fee below and above the minimum output;
- J(r+1) is recorded in the round r+1 controller and both checks hold for it. V_rec is recorded and the flag is set;
- unclaimed or absorbed pool money and unheld J_boost go to the boost pot, never to the vault;
- value is conserved;
- tests reject a wrong or missing leaf among the K, a wrong h_J or n2, a vault paid more than J_house × [h_J ≥ 1] + G2 × n2, a J(r+1) that fails either check or the cap, a redirected fee, and the absorption of a pool before its deadline;
- the test-only seed is behind a build feature that cannot be enabled for a mainnet build;
- the marker leads `draw()` (R6); fee, caller and reserve-refund outputs sit at fixed indices with exact values (R7);
- measured: `draw()` script size as a function of K and m_max, and `draw()` at maximum K with worst-case proofs within the mass limits.

## M4 — `claim()`, `return_deposit()` and `refresh()`
Status: todo

Bundle claims with K-leaf proofs (7.4), deposit return after the window, and refresh (7.5). The spec confirms the `return_deposit()` condition (co-spent pool past its deadline, or T + D_ret).

Done when:
- duplicate holdings split a slice exactly; h is the sum over K leaves;
- a claim pays floor(slice / h) to the ticket's payout script, marks the index claimed, and returns d × b on the ticket's last claim;
- the pool decreases by exactly the prize, for lower tiers, the jackpot and the second prize;
- tests reject:
  - a double claim of an index;
  - a forged or under-reported h (missing or stale leaf);
  - the wrong tier;
  - a ticket from another round or covenant family;
  - a redirected or inflated payout;
  - a claim against an absorbed pool;
- `return_deposit()` is rejected before its condition holds (pool deadline not passed, or before T + D_ret) and pays only to the ticket's payout script;
- value is conserved for every claim and deposit return;
- measured: `claim()` script and proof size as a function of K and m;
- `refresh()` produces a byte-identical successor and moves no deadline or seed reference.

## M5 — House vault shares
Status: todo

Vault shares, deposit and withdrawal requests, conversion at each draw's share price, and the liability lock (5.4). The spec proposes the request and conversion transactions and keeps `draw()`'s shape fixed. Record the choice in `docs/DECISIONS.md`. Requirements (5.4):
- internal accounting;
- dead shares or a virtual offset;
- round down, and reject zero-share mints;
- irrevocable requests added to the vault's pending totals;
- aggregate conversion at the next draw into a per-draw settlement record, with withdrawals capped to keep L(r+1);
- owners settle against the record at any time.

Done when:
- requests convert at the share price set by the next draw, so a new backer cannot capture returns from earlier rounds;
- tests reject a zero-share deposit, cancelling a request, settling against the wrong draw's record, a second settlement, a withdrawal that takes the vault below L(r+1), and share × price overflow boundaries;
- J(r+1) is fixed before that draw's conversions;
- tests reject withdrawing below the liability L = J_house + G2 × (m − 1) of the round whose jackpot is fixed, before that round is drawn, a conversion at a stale or future price, and a double conversion;
- value is conserved.

## M6 — K shards and limits
Status: todo

Done when:
- the largest workable K for `close()`, b_max and m_max are measured with the real scripts;
- R is checked against the larger of `close()` and `draw()` fresh outputs;
- purchase conflict rate is measured;
- `docs/costs.md` lists size and mass per transition, the minimum ticket deposit, M_ref and the minimum fee payout;
- final R, d, K, b_max and m_max are recorded in `docs/DECISIONS.md`.

## M7 — Testnet-10 end to end
Status: todo

Done when several consecutive rounds run on testnet-10, including:
- an m change, a duplicated jackpot combination and a held second prize;
- a referral payout and a below-threshold referral;
- an admin parameter change that takes effect only in the next opened round;
- backer deposit and withdrawal;
- a claim built from shard trees the wallet kept during the round, more than 30 hours later;
- a claim built from an independent indexer copy;
- a round that overruns 30 hours because a `draw()` is late: its sales pause after the deadline, and its claims and `draw()` proof are built from an indexer copy (section 9);
- every shape submitted through the public mempool, not directly to a miner;
- the project indexer and a published launch bundle (genesis outpoint, authorized genesis outputs, covenant ID, template preimages, live UTXOs), with `sil-debug` and kascov preflight used in review.

Release gate (not a milestone):
- no mainnet value until the mass-limit question and the SilverScript/rusty-kaspa revision check are resolved;
- the reviewer checklist passes on every shape;
- an independent audit is scheduled.
