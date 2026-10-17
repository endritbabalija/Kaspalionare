# Spec outline: refereed sequential delay (RSD) for the seed

Status: OUTLINE, not approved. Owner decision D111 first. Findings: `docs/research/seed-fairness-findings.md`. This outline lists what a full spec must fix; it is not itself a spec. No code before the spec is approved. **Revised 2026-10-17 (D127): section 5 overrides the parameters and adds checks; read it first.**

## 0. Order of work

1. **Spike (local engine only, measure before any design freeze):** steps per transaction for an iterated `OpBlake3` verifier written in SilverScript (unrolled loop) and as a raw `OpBlake3` run; the P2SH redeem re-hash cost; compile time at 200,000 steps; masses at the 90 % rule; whether a near-full-block transaction is accepted on testnet-10 at what fee; Blake3 sequential speed on two more CPUs. Output: s, N, fees per round.
2. **Spec** (this outline expanded, with the attack table first).
3. **Milestone:** attack tests first, then covenants, then `covenant-reviewer`.

## 1. New and changed covenants

| Covenant | Role |
|---|---|
| drawer (changed) | gains `commit_anchor()` (the M7 anchor checks move here) and `finalize()`; `draw()` takes y from the drawer state instead of the anchor proof |
| claim (new) | KIP-20 lineage from a checked genesis: state (cov, round, x, R, batches posted, complete flag, completion DAA, bond, claimant script) |
| verify (new) | KIP-20 lineage: state (cov, round, claim genesis id, R, index j, current value, steps done, creation DAA, refuter script) |

T, s, N, W_c, T_honest_bound and B are genesis constants outside `set_params()` (review S7).

## 2. Checks each transition must make (attack tests derive from these)

- **commit_anchor():** M7 checks D93–D95 unchanged; writes x and commit DAA (R1: x is in the transaction); one commit per round; reward to the caller.
- **claim genesis:** template, cov, round, c₀ = x (position 0 under R), bond ≥ B; later batch transactions extend R by Merkle roots over (index, 32-byte value) with batch overlap or per-batch roots so that any two adjacent checkpoints are provable in one transaction (review M7); complete only when c_N = y is at position N.
- **verify genesis and steps:** genesis state proven against R at position j; exactly s steps (the last segment if s does not divide T: fixed by choosing T = N·s); each step transaction continues the lineage (review F1).
- **refute (last verify step):** consumes the claim; checks claim lineage (genesis id recomputed), same R, c_{j+1} at position j+1 under R (review M4), result ≠ c_{j+1}; burns half the bond, pays half to the refuter (review M1).
- **finalize():** consumes drawer + claim; claim lineage, cov, round, c₀ = x, complete; DAA ≥ max(commit DAA + T_honest_bound + W_c, completion DAA + W_c) (lower bounds only, R3, review M5); writes y; claimant bond returned.
- **orphaned verify:** refund after a lower-bound timeout (review S6).
- **void:** D96 before commit; after commit only a backstop of days (review M3).
- **windows:** D_claim and D_RET re-derived from the finalize DAA and stored in the round record (R1, R4; review M6).

## 3. Attack table (minimum)

Forged claim or verify UTXO (F1); self-refutation front-running (M1); claim flood vs refutation block space (M2); slot occupation and stall-to-void (M3); refutation with wrong positions or another claim's R or another round (M4); finalize/refute race at the window edge (M5); window re-derivation (M6); adjacent checkpoints across batches (M7); near-full transaction inclusion under load (S2); commit liveness and a commit near the opcode-window edge (S4); late draw against shard-data retention (S5); T changed by the admin (S7); reorg of X after commit (`OpChainblockSeqCommit` re-evaluation, `refs/kips/kip-0021/seqcommit-accessor.md`).

## 4. Measurements

Masses of commit, each claim batch, a verify step, the refute step and finalize, all within 90 %; fees per round at the relay floor and at priority; end-to-end latency on testnet-10.

## 5. Revisions after the practice research (2026-10-17, D127)

Sources: `docs/research/optimistic_verification_practice.md` (OV), `docs/research/sequential_hash_hardware.md` (SH), `docs/research/lottery_draw_practice.md` (LD), `docs/research/shipping_randomness_process.md` (SP). Each item is a research recommendation for the spec, not a decision.

### 5.1 Parameters (replace the findings 4.3 values)

| Quantity | Was | Now | Basis |
|---|---|---|---|
| A_max (attacker speed over the 53 ns reference CPU) | 50 | **25** to plan with; 50 stays the safe ceiling; go below 25 only after a Blake3 synthesis result | SH implication 1: no published ASIC latency for one Blake3; logic-depth analogy to the AES bound (4–5× over CPU), Chia ASICs 3–4×; evidence range 5–18 |
| T at δ_min = 120 s | 1.2·10¹¹ | **5.7·10¹⁰** (honest about 50 min) | SH implication 3; honest time = δ_min × A_max |
| N at s = 10⁶ | 1.2·10⁵ (3.8 MB) | about 5.7·10⁴ (about 1.8 MB) | follows from T |
| Hash | Blake3 | Blake3 (unchanged) | SH implication 2; `OpBlake3` and `OpSha256` 1 unit per byte, `OpBlake2b` 2 (`refs/rusty-kaspa-v2.1.0/crypto/txscript/src/opcodes/mod.rs:61-71`, re-checked 2026-10-17) |
| W_c | 2 h | **measured, not assumed:** W_c ≥ (sequential refutation hops + 1) × measured p99.99 inclusion latency under an attacker miner + honest recompute margin | OV implication 1: every production fraud-proof system uses days (Arbitrum about 6.4 d); 2 h has no precedent and no censorship model |
| B | covers a refutation at a high feerate | B ≥ 2 × (one full refutation chain at the maximum assumed feerate + a failed-attempt margin) × the number of simultaneous false claims the honest side must kill; half burned, half to the refuter | OV implication 4 (Optimism's 2× rule) |

Resulting draw time: about 1.7 h (honest time with the 2× watcher bound) + W_c after the commit. With 24 h rounds, W_c can grow to several hours without pausing sales (round r+1 is on sale meanwhile; `close()` of r+1 needs only r's draw before its own deadline).

### 5.2 Design changes for the spec

1. **Fan-out refutation (OV 2).** The refuter commits the true intermediate values in one transaction, the segment's chunks (about 200,000 steps each) are verified in parallel transactions, one final transaction consumes them and the claim: about 3 sequential hops instead of about 7 (BitVM2's pattern, 70 → 3). Needs a soundness check in the spec.
2. **Window-edge extension (OV 3).** No refutation chain may be cut off by the deadline: finalize ≥ max(completion + W_c, last activity of any verify chain against that claim + W_ext). Lesson from Optimism's dispute-game timer bug.
3. **False-claim flood (OV 5; biggest risk found).** One unrefuted false claim finalizes, so the honest side must refute every false claim before its own deadline. The spec must choose: a bond that rises with the number of outstanding claims, a cap on open claims, or both; and answer whether one verified segment can kill several claims that share the same first wrong checkpoint (a UTXO is spent once). Testnet test with 50 and 500 false claims.
4. **Paid watchers (OV 6, 7).** The finalize reward goes to the claimant whose claim finalizes, so every rational honest party computes y, posts a correct claim and refutes rivals. Keep the refuter's half bond. No cap on the number of claims. The front-end, the indexer and large ticket holders run a watcher by default; the spec states the watcher uptime required per round.
5. **No guardian (OV 8, SP).** Covenants are immutable: a refutation bug is a stuck round or a stolen draw. Decide the fallback up front (void after a long backstop, M3) and test the failure paths (stuck round, void, late claim, window edge) as heavily as the happy path (Sui's May 2026 outage was in the DKG-failure path).
6. **Attacker finishes first (OV risks).** A fast attacker has y before any watcher; check that a refutation never requires the honest party to have finished its own computation before a claim completes (the T_honest_bound term in finalize is the answer to confirm).
7. **Refuter griefing (OV 10).** Show that a false refutation attempt against a correct claim costs the claimant nothing; if so, no challenger collateral is needed.
8. **Jackpot timing (LD 2; owner decision open).** With a delayed draw, round r+1 is on sale before `draw()` of round r fixes its jackpot. Checked with `game::fix_jackpot` (D127): J(r+1) does not depend on round r's result while the vault is large, because the top-up fills J to the edge or whale limit, which depends only on m (fixed at `close()`) and the price; a boost pot shifts the house/boost split but not the total. It depends on round r only when the 20 % cap binds (small vault, e.g. 6M KAS: 1,040,421 KAS if unwon vs 885,639 if won at margin 1.17). Options: (a) the front-end shows the exact figure when both outcomes agree and "at least X" (the won case) otherwise; no contract change; recommended first. (b) Fix J(r+1) at `close()` at the worst case so it is always final when sales start (research favours a bound figure: a covenant cannot carry an "estimate" disclaimer); this changes the solvency non-negotiable (L(r) + L(r+1) held at once) and design 4.4 and 4.6 must be re-checked.
9. **Draw schedule (LD 1).** A published, fixed draw delay with a draw-by time and a countdown; regulated games close sales 1–2 h before the draw, so a fixed delay of a few hours is acceptable when announced.

### 5.3 Spike additions (section 0 step 1)

- Inclusion latency per hop on testnet-10 at the relay floor and under load, p50 / p99 / p99.99, for near-full transactions (also whether they relay: OV 9).
- The fan-out refutation's masses and hop count.
- k = 50 and 500 false claims against one honest refuter set: block space, fees, time.
- Optional (SH 5): open-PDK synthesis of a Blake3 compress core (critical path in ps) to replace the 5–18 estimate; an unrolled Blake3 CPU reference (the agent measured 61 ns on 2026-10-17 against the earlier 53 ns).

### 5.4 Release-gate steps that need outside parties (SP checklist)

No team found shipped a mechanism of this kind on internal review alone (Optimism's permissionless fault proofs went live 2024-06-10 and were reverted about two months later after audits found high-severity bugs). The current method covers attack tests, the executable model, the reviewer agent and testnet-10; it does not supply independent eyes. Before mainnet:
1. a cryptographer reviews the sequential-Blake3 assumption and A_max;
2. an external audit of the RSD covenants, then a review of the fixes;
3. a public bug bounty;
4. a guarded launch: the jackpot cap set low at first, raised in stages.
