# Milestones

Status: `todo` · `in progress` · `done`. Update the status line when a milestone finishes. Each milestone gets a spec in `docs/specs/` before any code. Section numbers refer to `docs/design.md`.

Every covenant milestone checks value conservation for its transitions (5.5). It is stated over economic value, with reserves R moving per the reserve rule. Inputs must equal outputs plus referral, fee, house share, boost pot and caller rewards. Transaction fees are paid from external inputs.

Proposals in `docs/DECISIONS.md` marked "confirm in spec Mx" are settled in that milestone's spec before any code.

Out of scope until later: economic tuning beyond the starting values, ticket transfer, any front end, mainnet.

Seed: the seed source is the anchor block's hash (M7); the miner-withholding bias is OPEN on the release gate (D100, amended 2026-10-13).

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
Status: done (2026-10-10, spec `docs/specs/1.md`; B_MAX = 5, see `docs/DECISIONS.md` and `docs/costs.md`)

**Mass-limit check (research G1–G3): done before M1, 2026-10-10** (feasibility Q7, `docs/tn10-mass-check.md`).
- v2.1.0 nodes enforce the block limits per transaction. M0's d, R and largest K hold under relay on testnet-10.
- Still to do: repeat the check with the real `buy()` and `close()` here and in M2, and with `draw()` in M3 and M6. Use `cargo run -p tn10 -- probe ...`, adding cases for the real scripts.
- The wallet SDK generator's 100,000 cap means our builder builds protocol transactions itself.
- The M0 probe hygiene gaps listed in DECISIONS (2026-10-10) are M1 requirements.

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
Status: done (2026-10-10, spec `docs/specs/2.md`; K = 16, B_MAX = 4, R = 1.69 KAS; open: seed_ref chosen by the caller, review R1; see `docs/DECISIONS.md` and `docs/costs.md`)

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
- `close()` at the largest K runs with worst-case scripts within the per-transaction mass limits confirmed before M1 (feasibility Q7); a shard dragged into a close under a different leader route is rejected.

## M3 — `draw()` with a test seed
Status: done (2026-10-10, spec `docs/specs/3.md`; the drawer singleton leads `draw()`, not the marker (D37); m_max = M_DRAW = 50, R = 1.69 KAS unchanged, f_min ≥ 0.1 KAS; open: next G2 not capped in `draw()` (O-S5-1), seed_ref chosen by the caller (R1); see `docs/DECISIONS.md` and `docs/costs.md`)

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
Status: done (2026-10-11, spec `docs/specs/4.md`; the ticket leads `claim()` (D53); P_MIN = 0.05 KAS confirmed, D_RET 14 days, D_IDLE 24 h, A_MAX raised to 7 (U5); carried items closed: same covenant ID (D61), minimum non-final prize (D56, D57), A_MAX re-check (U5); `close()` K = 16 at 86.8 % transient; open: late `draw()`/`close()` window gaps (U4, U6) on the release gate, claim fees near P_MIN and `buy()` storage at d = 0.11 KAS for M6; see `docs/DECISIONS.md` and `docs/costs.md`)

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
- carried from earlier milestones: `claim()` and `return_deposit()` require the ticket's covenant ID to equal the pool's (look-alike tickets, DECISIONS row of M1 review); the minimum non-final prize rule; A_MAX re-checked with the real pool claim code (D44);
- if the ticket, controller or marker script changed, `close()` at K = 16 is re-measured; stop and ask if any mass exceeds 90 % (it was 86.5 % transient after M3).

## M5 — House vault shares
Status: done (2026-10-12, spec `docs/specs/5.md`; conversion inside `draw()` with one settlement record (D65), virtual offset VS = 1,000 / VA = 1 and a deployer position at genesis (D66), share positions with owner-input withdrawal (D67), withdrawal cap w_c by witness (D69), exact-drain settlement (D72), 63-step `mulfrac` (D73), vault request age window δ = 600 DAA against `draw()` front-running (U-4); `close()` K = 16 unchanged at 86.8 % transient, worst `draw()` 41.7 / 53.1 / 14.5 %; open for M6: S₀ floor (U-5), deposit price lock-out, request griefing (U-6) and request throughput; see `docs/DECISIONS.md` and `docs/costs.md`)

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
- value is conserved;
- if the controller or marker script changed, `close()` at K = 16 is re-measured; stop and ask if any mass exceeds 90 %.

## M6 — K shards and limits
Status: done (2026-10-13, spec `docs/specs/6.md`; d = 0.13 KAS final, closes U2 (D80, D81); B_MAX = 4 final; K = 16, m_max = 50 and R = 1.69 KAS provisional pending the real-seed milestone (D83: largest K for `close()` 16 within 90 % and 18 within 100 % of transient, R bound by the `close()` storage worst case at 89.9 %, drawer byte budget about 91,950 B and 4 free stack slots); P_MIN = 0.05 KAS kept, claim fees under load documented (D82: PAY at P_MIN ≈ 7.55 × P_MIN at the priority floor); SHARE_UNIT = 1 and V₀ ≥ 1 KAS, closes U-5 (D84); MIN_DEPOSIT = W_MIN_VALUE = 10 KAS with vault check W4b and its remainder rule (D85, D86, review RV-1, O-M6-2); request griefing mitigated by cost only, the RBF fee auction refuted on testnet-10 (D87, O-M6-1); purchase conflict rate measured (D88: τ_chain 0.919 s, τ_merge 1.655 s); genesis exempt from the 90 % margin and refused above 100 % (U-M6-1); dead M4 draw branch removed (D89); `close()` K = 16 unchanged at 86.8 % transient; O-M6-1…O-M6-3 were picked by the orchestrator under the user's delegation and can be overturned; open on the release gate: residual deposit lock-out (D84), request griefing residual (D87), q = 0 small-vault withdrawals (O-M6-3); see `docs/DECISIONS.md` and `docs/costs.md`)

Done when:
- the largest workable K for `close()`, b_max and m_max are measured with the real scripts;
- R is checked against the larger of `close()` and `draw()` fresh outputs;
- purchase conflict rate is measured;
- `docs/costs.md` lists size and mass per transition, the minimum ticket deposit, M_ref and the minimum fee payout;
- final R, d, K, b_max and m_max are recorded in `docs/DECISIONS.md`.
- carried from M4: P_MIN and d against claim fees under load (a PAY claim near P_MIN pays about 5–8 × P_MIN at the priority floor, M4-h); the b = 1 referral `buy()` at 95.7 % of storage with d = 0.11 KAS (U2); `close()` K = 16 transient headroom is 32,284 grams (86.8 %, M4-f).
- carried from M5 (vault shares, `docs/DECISIONS.md` 2026-10-12):
  - S₀ floor (U-5): genesis accepts V₀ = SHARE_UNIT, so S₀ = 1 against VS = 1,000 virtual shares, which then own about 99.9 % of the vault; set a minimum S₀ (or V₀) relative to VS;
  - deposit price lock-out: once the price passes MIN_DEPOSIT sompi per share (about 10⁵ × the genesis price, or more than about 1,000 KAS accrued with no real shares), D70's mint ≥ ceil(D / MIN_DEPOSIT) refunds every deposit; tune SHARE_UNIT, VS and MIN_DEPOSIT;
  - request griefing (U-6): the age window gives one request per 600 DAA, won by the highest fee; a position holder sending `request_withdraw(w = 1)` every slot starves honest requests for about 70 KAS/day in fees plus about 2,400 KAS/day of reserves locked until the next draw and settlement (solvency and `draw()` unaffected); candidates: several requests per vault spend (batching), a minimum withdrawal size, a larger request reserve;
  - request throughput: at most one request per VAULT_REQ_DELAY = 600 DAA (about 1,440 per day) by construction; revisit δ with the batching choice;
  - drawer stack headroom: 4 free slots at check 24 (M5 S4-1); any drawer change that keeps a value live across check 24 must re-measure it;
  - cleanup: the dead M4 draw branch in `crates/lottery/src/draw.rs` (`SIG_DRAW_M4`, `args_m4`, the 22-push path of `RoundResult::from_tx`; M5 S4-9).

## M7 — Real seed
Status: done (2026-10-13, spec `docs/specs/7.md` with its implementation amendments; the seed is the hash of the anchor block X, the first selected-chain block with DAA ≥ D = the marker's creation DAA + seed_offset, authenticated in `draw()` by one `OpChainblockSeqCommit` call and a 272-byte KIP-21 preimage proof (D93–D95, amended: the P opcode call and P.hash dropped); closes R1 (D93, SP5); void mode as the missed-draw fallback, valid from D + 2F (D96), with the floor `draw_window >= VOID_MIN` enforced in drawer check 1 and `seed_offset >= 1` in `close()`; U4 closed (D98), U6 bounded (D102); marker without `seed_ref` and `refresh()` (D99); the TEST-ONLY seed and the `test-seed` feature deleted (D101, SP12); K = 16, m_max = 50, B_MAX = 4 and R = 1.69 KAS final (D83 re-confirmed: `close()` K = 16 at 86.5 % transient, `close()` storage worst case 89.9 %, worst `draw()` 41.3 / 52.7 / 14.2 %, drawer stack headroom 4 = the D104 floor); `draw-m7` and `void-m7` accepted on testnet-10 (KIP-21 live; the void ran under a TEST-ONLY short-void drawer variant that a `mainnet` build does not contain); two covenant-reviewer passes, no must-fix; open on the release gate: the miner-withholding bias (D100 amended: ε far below 0.27 where the whale check binds, rentable burst hashrate, the parent-omission lever; candidate fix VDF+ZK); see `docs/DECISIONS.md` and `docs/costs.md`)

Replaces the TEST-ONLY seed of `draw()` with a real seed source, following the direction in `docs/research/randomness-sources-on-kaspa.md` (§4, M. Sutton: a PoW chain block fixed in advance at a predetermined blue-score offset, read through `OpChainblockSeqCommit`, no menu for the miner or the caller) and its three open items (blue-score anchoring, miner-withholding cost, missed-draw fallback; DECISIONS 2026-10-13), and fixes review R1 (the `close()` caller must no longer choose seed_ref). Two sessions: a design session that writes `docs/specs/7.md` with the owner's decisions and no covenant code, then an implementation session (attack tests first, then the scripts, then `covenant-reviewer`, then the re-measurements below).

First task of the design session, before anything else: determine whether the harness engine (`crates/harness`) can exercise `OpChainblockSeqCommit` locally with a fabricated selected chain (blue scores, finality window, reorg). If it cannot, the spec defines a testnet-10 test path for the seed checks and says which attack tests can only run there.

Done when (the spec sets the details):
- the design §11 requirements are enforced by the scripts and proven by compiled attack tests: the anchor is fixed at `close()` from committed data before any block at it exists; `draw()` accepts only the uniquely eligible block (rejected: a neighbouring chain block, a non-chain block, a block outside the finality window, a wrong blue score, a caller-supplied anchor or seed_ref); a missed window cannot re-roll the result;
- the manipulation-cost argument is written in the spec: what the miner of the anchor block can do (withhold, grind block contents) and at what cost, what a caller can do by delaying `draw()`, and the resulting bound on J; `draw()` or the jackpot rule enforces that bound, or the spec records why none is needed;
- the missed-draw fallback is specified and implemented (it is not optional: the release gate, M8's late-draw round and the vault's "target draw never happens" refund all depend on it), including the U4 and U6 claim-window gaps listed in the release gate;
- every consumer of `seed_ref` is re-derived if its unit or meaning changes (blue score instead of DAA score): the `draw()` lock (drawer check 7), the claim deadline (check 19, D39, `DRAW_WINDOW`), `return_deposit` (D_RET, U6), and the placeholders `seed_offset = 600` and `DRAW_WINDOW = 432,000` in `crates/lottery/src/game.rs` get real values with their derivation;
- the TEST-ONLY path is removed or stays test-only and mainnet-blocked: `draw_test_seed()` and the `test-seed` feature are either deleted or kept behind the DP22 gate, and `cargo check -p lottery --no-default-features --features mainnet,test-seed` still fails with the `compile_error!`;
- the design edits are proposed in `docs/DECISIONS.md` (not applied to `docs/design.md`): §11 rewritten for the chosen source, the two "Parked" rows of §10 ("Nobody calls `draw()` in time", "Miner manipulation of the seed") resolved, and "randomness is parked" lifted in `CLAUDE.md` and in the preamble above;
- the release-gate items it owns are closed: R1 (seed_ref choice) and the missed-draw item;
- K, R and m_max are re-confirmed (D83, spec 6 A5a; approved 2026-10-13): they count as final only after these M6 measurements are re-run with this milestone's final `draw()` (and `close()`, if it changes) and stay within 90 %:
  - the M6-a `draw()` rows (worst and typical) and the dependent M6-h fee rows;
  - the M6-a `close()` rows, M6-b (K frontier) and M6-c (B_MAX frontier), if `close()` changes (for example the R1 fix);
  - the M6-b `draw()` estimates at the K_MAX variants;
  - the M6-d `draw()` rows with the M_DRAW variants (they set m_max);
  - M6-g: the `close()` storage worst case if `close()` gains or loses an output, and the `draw()` storage supremum if `draw()` gains an output (for example a seed-proof object);
  - M6-j: the drawer sensitivity and byte budget (about 91,950 B before the worst `draw()` reaches 90 % of transient), and the drawer's stack headroom (4 free slots at check 24) whenever a value stays live across check 24.

## Spike — VDF+ZK seed feasibility (before M8)
Status: done (2026-10-13, spec `docs/specs/spike-vdf.md`; D106: Groth16 fits inside the worst `draw()` as is, V1 plain Groth16 72.1 % and V2 RISC Zero-over-Groth16 72.7 % compute, stack headroom 4 kept; RISC Zero succinct cannot be encoded (signature script over the 250,000-byte consensus limit) and does not fit alone either; D107: binding of V2 not exercised, no production setup, VDF not chosen or sized, proving fits the opcode window only for delays of minutes and the proving key grows 608 B per iteration, the draw fee and proving share C_draw, and a sole prover could choose between the result and a void (new release-gate item); covenant-reviewer: no must-fix, eight should-fix applied; margin floor handed to the owner with the ε table; `docs/costs.md` "Spike VDF+ZK")

Local only, no testnet. Answers whether the candidate fix for the anchor miner's withholding bias (D100, amended) fits in `draw()`: seed = H(VDF(X.hash)) with the VDF evaluation proven by `OpZkPrecompile`. Starts with a short spec approved by the owner.

Done when:
- a drawer variant verifies a real Groth16 proof (and, for comparison, a RISC Zero succinct proof) inside the worst M7 `draw()` in the local engine;
- compute, transient and storage mass and the stack use at check 24 are measured against D104 (lower bound from the precompile alone: about 69 % compute with Groth16, about 91 % with RISC Zero);
- the verdict is recorded: fits as is, fits only with a split `draw()` (and what that costs), or does not fit;
- the margin-floor question of the amended D100 row (ε = 0 at margin 1.0 for a self-referring caller) is answered or handed to the owner as a decision.

## M8 — Testnet-10 end to end
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
- no mainnet value until the SilverScript/rusty-kaspa revision check is resolved and the mass-limit result (feasibility Q7) has been re-run against the release in use;
- the reviewer checklist passes on every shape;
- an independent audit is scheduled.
- ~~the `close()` caller can no longer choose seed_ref (review R1, M2), fixed as part of the real seed design~~ **closed in M7** (D93: no seed reference is stored or chosen; the anchor follows from the marker's creation DAA; SP5);
- `draw()` caps the next round's G2 at G2_MAX (O-S5-1, M3);
- the toolchain check is done: SilverScript's rusty-kaspa rev `a41a333` is an ancestor of the pinned release and `crypto/txscript` is diffed (DECISIONS pins row, 2026-10-09).
- the vault items left open by M6 (DECISIONS 2026-10-13 OPEN rows) are resolved or accepted: the residual deposit lock-out at 10⁹× the genesis price (D84), the request griefing residual, mitigated by cost only (D87, O-M6-1; candidates: request batching, a per-round request queue), and partial withdrawals while the vault holds less than 10 KAS (q = 0, O-M6-3; reviewer's option: whole withdrawals only);
- ~~the missed-draw item is resolved, including the claim-window gaps it causes (M4): a late `draw()` whose caller uses old inputs can create pools whose claim deadline has already passed (U4; tripwire test `parked_u4_late_draw_creates_already_expired_pools`), and a `close()` later than T + D_RET − (seed_offset + draw_window + claim_window) lets `return_deposit_after` forfeit winning tickets before their pool deadline (U6)~~ **closed in M7** (void mode, D96, with the on-chain floor `draw_window >= VOID_MIN`; U4 closed by D98, the tripwire inverted into SP6; U6 bounded by D102);
- **OPEN (M7, D100 amended 2026-10-13):** the anchor miner's withholding bias. The profit condition α > ε must use the minimum ε the game-size rule allows (where the whale check binds ε is far below 0.27, and about 0 at margin = 1.0 for a player who also takes the referral and both caller rewards), α is rentable burst hashrate (the anchor time is predictable to within seconds), and leaving an honest candidate out of a block's parents is a cheaper lever than withholding. Items: a feasibility spike for the candidate fix (seed = H(VDF(X.hash)) proven with `OpZkPrecompile`) and a measurement of rentable burst hashrate against that ε. The genesis builder refuses mainnet until this is resolved.
