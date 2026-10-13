# M7 — Real seed: implementation plan (for the implementation session)

## Context

`docs/specs/7.md` (approved 2026-10-13) replaces the TEST-ONLY seed of `draw()` with a real one: the anchor is the first selected-chain block X with DAA score ≥ D = marker creation DAA + 600, authenticated in script with two `OpChainblockSeqCommit` calls and the KIP-21 preimages; the seed is X.hash. It adds a void mode (empty anchor, lock ≥ D + 2F, no winners, everything but fee and C_draw to the boost pot), removes the marker's `seed_ref` and `refresh()`, removes `own_daa`, deletes the `test-seed` feature, and re-confirms K, R and m_max. The milestone entry splits the work into this design session (spec, plan, decisions) and one implementation session that follows this plan. Deliverable of this session after approval: this plan saved as `docs/specs/7-handoff.md`, the D93–D105 rows in `docs/DECISIONS.md`, M7 set to in progress, one commit.

No Plan subagent was used: the plan below is drawn from three exploration reports plus the spec; the file:line references come from those reports.

## Order of work (stages for the implementation session; each stage ends with `cargo test` green except where stated)

### Stage 0 — Harness: fabricated chain (spec A7)
Files: `crates/harness/src/lib.rs`, `crates/harness/Cargo.toml`, `crates/Cargo.toml` (workspace dep), new `crates/harness/src/chain.rs`.
- Add workspace git dep `kaspa-seq-commit` (package name confirmed, `refs/rusty-kaspa-v2.1.0/consensus/seq-commit/Cargo.toml:2`) at the pinned rev; harness dev-dep only (pins).
- `chain.rs`: `SimBlock`, `ChainSim { blocks, chain, pov_blue_score, f }`, `seq_commit(&SimBlock)` with `kaspa_hashes::{SeqCommitMergesetContext, SeqCommitMerkleBranch}` (the KIP-21 formula; genesis parent commitment = ZERO_HASH), `impl kaspa_txscript::SeqCommitAccessor`, builders `line`, `with_jump`, `with_side_block`, `reorg_to`, `set_pov`, `anchor_for(d)`, `proof_bytes(x, p)` (spec A3 layout, 304 B), and `anchored(d, x_hash)` (a minimal chain: P at daa d−1, X = x_hash at daa d, POV in window) for fixtures that only need "some valid anchor with this hash".
- `execute_on_chain(tx, entries, idx, enforce, &ChainSim)` = `execute` with `.with_seq_commit_accessor_opt(Some(sim))`; `execute` unchanged (SA3 "opcode disabled" control). Default `f = MAINNET_PARAMS.finality_depth()`.
- Unit tests in the harness: K_SEQ/K_MC byte pins against `kaspa_hashes` hashers; `seq_commit` against `kaspa_seq_commit::hashing::seq_commit` (SP11); a smoke test compiling a 10-line SilverScript contract that calls `OpChainblockSeqCommit` and `blake3WithKey` and verifies a sim block's preimage (proves the compiler path before touching the drawer).

### Stage 1 — Rust scaffolding and the tests, written before the scripts change
Files: `crates/lottery/src/{anchor.rs (new), draw.rs, close.rs, buy.rs, genesis.rs, game.rs, state.rs, claim.rs, lib.rs}`, `crates/lottery/Cargo.toml`, `crates/lottery/tests/common/{m2.rs, m3.rs, m4.rs, m5.rs, m5_spec.rs, mutants.rs, measure.rs}`, new `tests/{m7_seed.rs, m7_attacks.rs, m7_mutants.rs, m7_measure.rs}`.
- `game.rs`: `SEED_OFFSET = 600`, `DRAW_WINDOW = 864_000`, `VOID_MIN = DRAW_WINDOW`, `SEED_TAG = b"KLOT/seed"`; `seed_numbers(cov, round, x_hash, m)` (same body); `anchor_daa(mk_daa, seed_offset)` replaces `seed_ref(own, offset)` (game.rs:246); `claim_deadline(d, draw_window, claim_window)` = d + dw + cw (game.rs:612); `CloseMath` loses `seed_ref` (game.rs:238, 280); `void_lock(d, draw_window)`.
- `anchor.rs`: `BlockFields {hash, parent_commit, activity_root, miner_payload_root, sp_timestamp, daa_score, blue_score}`, `AnchorProof {x, p}`, `to_bytes/from_bytes`, `seq_commit(&BlockFields)` (shared with `ChainSim` via a harness re-export or duplicated 4-hash function pinned equal in SP11).
- `state.rs`: remove `MarkerState.seed_ref` and its `fields()` entry (state.rs:363, 410); the Rust decoder `draw.rs::state_values` reads by name, so only the literal sites change (close.rs:308, buy.rs:437 `dummy_marker`, draw.rs:303, tests).
- `draw.rs`: `DrawCase.test_only_seed` → `x_hash: [u8;32]` + `sim: Arc<ChainSim>` (default `ChainSim::anchored(D, x_hash)`), `void: bool`; `seed()` from `x_hash`; `d()` = `marker_daa + controller.seed_offset`; `fix_lock_time` uses `d()` (void: `d() + draw_window`); `absorb` deadline `d() − 100_000`; `own()` removed, `Lie.own` → `Lie.mk_daa: Option<i64>`; `expected()`: `d_claim = claim_deadline(d, …)`, void branch (no pools, V' = V, O' += H, win5 = 0, no leaves); `args()`: `anchor` bytes (or empty) at ARG_SEED → rename `ARG_ANCHOR`, `Int(mk_daa)` at the last position (`ARG_MK_DAA`, replaces `ARG_OWN`); `SIG_DRAW_M5` → `SIG_DRAW = "draw(byte[],byte[5],…,int)"`; `TEST_SEED_REF` → fixture `marker_daa = 1_000_000 + (r−1)·10⁶` (so D = …600 as before); `RoundResult` gains `x_hash, mk_daa, void`, drops `own`; `from_tx` parses the anchor (empty = void) and `mk_daa`, derives D_claim.
- `buy.rs`: drop every `#[cfg(feature = "test-seed")]` (Sources/Templates/drawer_hash/drawer_redeem/drawer_spk, buy.rs:25,123-125,147-148,178-179,210-215,248-252,341-349); `Final` gains `sim: Option<Arc<ChainSim>>` and `run/run_input/run_all` call `execute_on_chain` when set (buy.rs:1047-1061); `Built.finish()` dry runs too; `DrawCase::build_with` sets it.
- `close.rs`: `TEST_SEED_OFFSET` → `game::SEED_OFFSET`; `expected()` marker literal loses `seed_ref` (close.rs:308).
- `claim.rs`: remove `RefreshObj::Marker` (claim.rs:741-781).
- `genesis.rs`: `validate_with`: `draw_window >= VOID_MIN` (new; replaces nothing), keep `seed_offset + draw_window + claim_window <= D_RET` and `WINDOWS_MAX`; drop the `cfg!(not(feature = "test-seed"))` branch (genesis.rs:377-379) and the duplicate `drawer_spk` (459-468); the network refusal stays with a new message ("the drawer is deployed on testnet or devnet only until the release gate"); add `allow_short_void_window: bool` (TEST-ONLY, refused unless network ∈ {Testnet, Devnet}) for `probe void-m7`.
- `lib.rs`, `Cargo.toml`: remove the `compile_error!` gate and `#[cfg(feature = "test-seed")] mod draw`; features `default = []`, `mainnet = []`.
- Test fixtures: `Chain` (m2.rs:389-830): keep `marker_daa`; `draw_case(seed, absorb, funding_daa)` sets `x_hash = seed` and `sim = ChainSim::anchored(d, seed)`; `winning(seed)` unchanged in meaning; `draw_round`/`draw_now` fund at `d + DRAW_WINDOW + 1_000`; remove `refresh_marker_case` and the `RefreshObj::Marker` arm of `refresh` (m2.rs:768-815). `Chain` gains `void_case()`/`void()`.
- Tests that change: m3 `early_lock` (lock = D − 1), `late_deadline`, `deadline_without_draw_window` and `own_lie` (m3.rs:305, 687, 714-734) → rewritten as SA5 cases; m3 DA12 → SA6; DP19 → SP6; DP18 → SP3 (search X.hash values); DP22 → SP12; m4 `kp15_refresh_then_use`, `kp15_refresh_marker_standalone`, KA15/KA16 marker arms, `refresh_bases`, m4_measure and m6_measure `refresh_marker` rows → removed (SP10 replaces them); `parked_u4_late_draw_creates_already_expired_pools` (m4_claim.rs:683) → inverted into SP6; m2 `ca12_seed_ref` and m2_close.rs:143 → marker without `seed_ref`; m4_mutants `refresh_mutants!(marker)` and the (131, 187) pin → marker contributes no RF tags; m3_mutants pin 88 → new count with D7/D19_own/D19_max retired and `M7*` tags excluded; `m5_spec::drawer` (m5_spec.rs:549) gains the new check 7/8 labels and the void branch (PREFLIGHT).
- New test files per spec A8: `m7_seed.rs` (SP1–SP13; SP9 is a pure-Rust inequality test; SP12 shells out as DP22 did, m3_draw.rs:637-653), `m7_attacks.rs` (SA1–SA10 as `Atk5`-style data with `at(input, "drawer:M7A_xc")` labels so ATTRIB/PREFLIGHT cover them), `m7_mutants.rs` (tags of spec A8, `every_m7_tag_is_tested`), `m7_measure.rs` (M7-a…M7-f; `m6a_rows` drawer label updated; a stack-headroom measurement by compiling drawer variants with `n` extra `int` locals kept live across check 24 until the compiler refuses — replaces the fixed sentence at m6_measure.rs:838).
- End of stage: `cargo test -p lottery` is expected to FAIL only in the M7 suites and the adapted M3 draw tests, because the scripts still expose `draw_test_seed` (dispatch mismatch) and the marker still carries `seed_ref`; record the failure reasons in the handoff.

### Stage 2 — Scripts
Files: `covenants/drawer.sil`, `covenants/marker.sil`, `covenants/controller.sil` (+ the `MarkerState` struct copies in `drawer.sil:132-155`; `shard.sil`/`params.sil` keep `seed_offset` in their `ControllerState` copies unchanged).
- `marker.sil`: drop `init_seed_ref`/`seed_ref` (31, 56) and the whole `refresh()` entry (103-172); keep `draw_leg`.
- `controller.sil`: check 20 keeps `own` for T', drops `seedRef` (480); check 24 marker literal drops `seed_ref` (586); the `MarkerState` struct (160) drops the field; `draw_leg` unchanged.
- `drawer.sil`: entry `draw(byte[] anchor, …, int mk_daa)`; check 1 adds `anchor.length == 0 || == 304` and `isVoid`; the `MarkerState` struct and check 4 ranges drop `seed_ref` (152, 430-431); new check 7 per spec A4 (mkDaa, mk_daa equality, D, draw lock / void lock); check 8 rewritten (preimage recomputation with `blake3WithKey` and the two 32-byte key constants, two opcode calls, `pc == x_pc`, `dX >= D`, `dP < D`, seed from `x_hash` with tag 0x4b4c4f542f73656564; void branch: win5 zero, leaves/siblings empty, skip 8–10); check 12 forces lp_t = 0 in void (replace the repeated `bud[t]/wt[t] >= 1` by `!isVoid && …` at 590, 629, 834, 1000); check 14 `hAmt` only when not void; check 15 adds H in void; check 19 = `dClaim = D + draw_window + claim_window`; check 24 unchanged. Mutant tags per spec A8; keep every `// ==== check N` marker line so the ATTRIB truncation tool (m2.rs:149) still works.
- Compile check for the 244-item stack limit (the `variant()` helper turns it into an `Err`); if the drawer fails to compile, move the preimage work into nested braces (the check-8 block already drops its locals).
- End of stage: full `cargo test --release` green; `M5_ATTRIB=1`/`M5_PREFLIGHT=1` over m5/m6/m7 attacks 0 not OK; `M3_ATTRIB=1` baseline compared.

### Stage 3 — Measurements and the D83 re-confirmation
`m7_measure.rs` rows M7-a…M7-f; `docs/costs.md` M7 section (sizes, worst/typical draw, void, close K = 16, K/B_MAX/M_DRAW frontiers, R check, drawer sensitivity and the measured stack headroom); stop rule D104 if any row > 90 %.

### Stage 4 — Testnet-10
Files: `crates/tn10/src/{cases.rs, probe.rs, main.rs, net.rs}`, `crates/tn10/Cargo.toml` (add `kaspa-seq-commit`, `kaspa-smt` at `refs/rusty-kaspa-v2.1.0/crypto/smt`, `kaspa-hashes`).
- `anchor_from_rpc(ctx, hash)`: `get_block(hash, true)` → header (daa, blue score, `accepted_id_merkle_root`), verbose `selected_parent_hash`, `merge_set_blues_hashes`, `merge_set_reds_hashes`, `is_chain_block` (`rpc/core/src/model/block.rs:67-77`); the selected parent's timestamp from its header; coinbase payloads of the mergeset blocks; `get_seq_commit_lane_proof(hash, COINBASE_LANE_KEY)` → `parent_seq_commit`, `payload_and_ctx_digest`, `inactivity_shortcut`, SMT proof → `lanes_root` via `OwnedSmtProof::compute_root::<SeqCommitActiveNode>` → `activity_root`; `mpr` from the mergeset in consensus order (selected parent first, then by blue work then hash — verify against `payload_and_ctx_digest`, try the alternative order if it mismatches and record which one the node uses); final check against the header.
- `find_anchor(ctx, start_hash, d)`: walk `get_virtual_chain_from_block` (already used, cases.rs:381-430) until the first chain block with daa ≥ d.
- `draw-m7` = `round1_drawn_with` with the anchor replacing `TEST_ONLY_SEED` (cases.rs:507, 651, 742, 756): buy the combination of `seed_numbers(cov, 1, x_hash, m)` only after X exists? No — the buy happens before close; on testnet the probe buys a fixed combination and the draw is a normal "jackpot not held" draw (DP4 shape); `claim-m4` and `vault-m5` keep working on top of it (claim of a lower tier or FINAL deposit return: pick what the result allows, record it).
- `void-m7`: genesis with `allow_short_void_window` (draw_window = 1,200), close, wait D + 1,200, void.
- `draw-m7-late` optional (13 h).
- Results in `crates/tn10/results-*.md` and `docs/tn10-mass-check.md` M7.

### Stage 5 — Review
Update `.claude/agents/covenant-reviewer.md:43` ("Randomness must stay a test-only seed" → the M7 status: the seed is the anchor block's hash; the miner-withholding bias is OPEN on the release gate, D100). Run `covenant-reviewer` with `docs/specs/7.md` and the changed files; fix must-fix/should-fix; re-run everything.

### Stage 6 — Record
`docs/DECISIONS.md` (implementation rows, review rows, measured rows, design-edit proposal row), `docs/costs.md`, `docs/milestones.md` (M7 done; preamble "randomness is parked" lifted), `CLAUDE.md` (goal line and Non-negotiables: replace the parked rule with the M7 status; commands: M7 test lines, the DP22 line → SP12 wording, probes `draw-m7`/`void-m7`), `docs/design.md` untouched (proposal only). Commit.

## Verification
- Stage 0: `cargo test -p harness` (new chain tests + SP11 pins).
- Stage 1 end: `cargo test -p lottery --test m7_seed --test m7_attacks` fails only at the script boundary (documented); every other suite green.
- Stage 2 end: `cargo test --release` (workspace); `M5_ATTRIB=1 cargo test -p lottery --release --test m5_attacks --test m6_attacks --test m7_attacks -- --nocapture --test-threads=1`, same with `M5_PREFLIGHT=1`; `M3_ATTRIB=1` on `m3_attacks`; `cargo check -p lottery --no-default-features --features mainnet` builds, `--features mainnet,test-seed` fails as unknown feature (SP12).
- Stage 3: `cargo test -p lottery --release --test m7_measure -- --nocapture --test-threads=1`; every row ≤ 90 % except genesis.
- Stage 4: `cargo run -p tn10 --release -- probe draw-m7` and `probe void-m7` accepted on testnet-10.

## Open points for the implementation session (decide and record, do not stop for them)
- Whether `kaspa_hashes`' hashers are usable from the harness without pulling std-only deps (they are; `kaspa-hashes` is already a dep).
- The exact consensus mergeset order for `mpr` (verified at runtime by the `payload_and_ctx_digest` self-check).
- The SilverScript name/arity of `blake3WithKey(data, byte[32] key)` (builtin_types.rs:143) — confirmed by the Stage 0 smoke contract.

## Progress log (implementation session, 2026-10-13)

Each stage agent appends one section here before it stops: what was done, files changed, the exact test commands run and their results (pass/fail counts, the failing tests and why), deviations from the spec or plan with their reason (as draft DECISIONS rows, marked PENDING until the orchestrator records them), and what the next stage must know. Handoff completeness beats brevity.

### Stage 0 — Harness: fabricated chain (done)

**What was done.** `harness::ChainSim` (spec A7), `harness::execute_on_chain`, the SP11 pins, and a SilverScript smoke contract that runs `OpChainblockSeqCommit` + `blake3WithKey` through the pinned v2.1.0 engine. No change to `crates/lottery`, `covenants/` or other docs. Nothing committed.

**Files changed.**
- `crates/Cargo.toml`: workspace dep `kaspa-seq-commit` (git rusty-kaspa, rev `01b532e8b553523216471682649693af92f0fd16`, the same rev as the other kaspa crates). `crates/Cargo.lock` updated (adds kaspa-seq-commit and its deps kaspa-merkle, kaspa-smt from the same rev).
- `crates/harness/Cargo.toml`: `[dev-dependencies] kaspa-seq-commit.workspace = true` (dev only; `ChainSim` computes commitments with the `kaspa_hashes` hashers at runtime).
- `crates/harness/src/chain.rs` (new): `SimBlock`, `ChainSim`, free fns `seq_commit_with_parent`, `mergeset_context`, `block_fields_bytes`, `default_finality_depth`; constants `K_SEQ`, `K_MC`, `BLOCK_PROOF_LEN = 152`, `ANCHOR_LEN = 304`; 8 unit tests.
- `crates/harness/src/lib.rs`: `pub mod chain;` and re-exports `ChainSim, SimBlock, K_SEQ, K_MC, ANCHOR_LEN, BLOCK_PROOF_LEN`; `SeqCommitAccessor` re-exported from `kaspa_txscript`; `execute` now delegates to a private `execute_with(.., None)` (behaviour unchanged: no accessor, so the opcode fails with `InvalidOpcode`); new `execute_on_chain(tx, entries, idx, enforce_budget, &ChainSim)` = the same with `.with_seq_commit_accessor_opt(Some(sim))`.
- `crates/harness/tests/m7_chain.rs` (new): 8 tests (SP11 pins, proof layout, smoke contract).

**Verified against refs (not from memory).**
- Opcode `OpChainblockSeqCommit` 0xd4 (`crypto/txscript/src/opcodes/mod.rs:1389-1404`): no accessor → `InvalidOpcode`; `is_chain_ancestor_from_pov` None → `BlockAlreadyPruned`, Some(false) → `BlockNotSelected`; `seq_commitment_within_depth` None → `BlockIsTooDeep`; else pushes the 32-byte commitment. Trait `kaspa_txscript::SeqCommitAccessor: Sync` (`seq_commit_accessor.rs:3-6`, re-exported at `crypto/txscript/src/lib.rs:66`). Matches spec A0.
- Real accessor window: `low_blue_score + threshold > high_blue_score` (`consensus/src/model/services/seq_commit_accessor.rs:8-10, 36-44`), threshold = `self.finality_depth` (`consensus/src/pipeline/virtual_processor/utxo_validation.rs:373, 445`). `ChainSim` uses the same strict inequality.
- F: `MAINNET_PARAMS.finality_depth()` returns `blockrate.finality_depth` (`consensus/core/src/config/params.rs:424-426`) = BPS × FINALITY_DURATION (`bps.rs:92-93`; `constants.rs:70` = 43,200) = **432,000** (pinned by a unit test).
- Hasher keys: `blake3_hasher!` zero-pads the tag into a 32-byte BLAKE3 key (`crypto/hashes/src/hashers.rs:120-138`); `SeqCommitMerkleBranch => b"SeqCommitmentMerkleBranchHash"` (:40), `SeqCommitMergesetContext => b"SeqCommitMergesetContext"` (:47). **Equal to spec A2 K_SEQ / K_MC.**
- Formula: `consensus/seq-commit/src/hashing.rs:80-84` (`H_mc(le64 ts ‖ le64 daa ‖ le64 bs)`), `:152-157` (`payload_and_context_digest = H_seq(ctx ‖ payload_root)`), `:160-165` (`seq_state_root = H_seq(activity_root ‖ pcd)`), `:168-173` (`seq_commit = H_seq(parent ‖ state_root)`). **Equal to spec D95.** KIP-21: `SeqCommit(genesis)` = ZERO_HASH (`refs/kips/kip-0021.md:365`).
- `OpBlake3WithKey` 0xda (`opcodes/mod.rs:1465-1475`) pops `[data, key]`; the key must be exactly 32 bytes (`MalformedPush` otherwise); result `blake3::keyed_hash(key, data)`.

**Confirmed SilverScript names and arity** (`refs/silverscript` rev 3ed97333, `silverscript-lang/src/compiler/builtin_types.rs`):
- `OpChainblockSeqCommit(byte[32] block) -> byte[32]` (:97, :180; emitted as the opcode in `compile/expression/builtin.rs:77`).
- `blake3WithKey(byte[] data, byte[32] key) -> byte[32]` (:93, :143; two arguments, data first; `builtin.rs:226-231`). The plan's `builtin_types.rs:143` reference is correct.
- Working pattern from the probe (copy into `drawer.sil` check 8; `blk` = the 152-byte X or P slice of `anchor`):
  ```
  byte[32] kSeq = byte[32](0x536571436f6d6d69746d656e744d65726b6c654272616e636848617368000000);
  byte[32] kMc  = byte[32](0x536571436f6d6d69744d65726765736574436f6e746578740000000000000000);
  byte[32] ctx = blake3WithKey(blk.slice(128, 152), kMc);
  byte[32] pcd = blake3WithKey(byte[](ctx) + blk.slice(96, 128), kSeq);
  byte[32] sr  = blake3WithKey(blk.slice(64, 96) + byte[](pcd), kSeq);
  byte[32] c   = blake3WithKey(blk.slice(32, 64) + byte[](sr), kSeq);
  require(OpChainblockSeqCommit(byte[32](blk.slice(0, 32))) == c);
  ```
  The two hex constants are K_SEQ and K_MC (the probe formats them from the Rust constants; `key_constants_are_the_spec_bytes` pins the bytes). The probe compiles to a 162-byte redeem and uses 2,410 script units per block (inside the free per-input allowance, budget 0).

**Commands and results** (run from `crates/`):
- `cargo test -p harness --lib --test m7_chain -j 2`: lib 8 passed, m7_chain 8 passed.
- `cargo test -p harness -j 2`: **35 passed, 0 failed** (lib 8, covenant_smoke 3, m0_limits 3, m0_mass 5, m0_merkle 2, m0_shapes 6, m7_chain 8; doctests 0).
- `cargo build --tests -j 2` and `cargo build --workspace --tests -j 2` (includes `tn10`): both finish; only the pre-existing `m0_mass` unused-variable warnings.

**Tests added.**
- `chain::tests` (lib): `chain_sim_is_send_sync` (also `Arc<ChainSim>`), `default_f_is_mainnet_finality_depth` (432,000), `key_constants_are_the_spec_bytes`, `anchored_has_x_and_p`, `anchor_for_picks_first_chain_block_at_or_after_d` (jump of 248; equal-DAA chain blocks pick the lowest blue score; no anchor → `None`), `accessor_unknown_and_side_block` (unknown → None; side block → Some(false)), `accessor_depth_boundary` (admitted at pov = bs + f − 1, None at bs + f), `reorg_moves_the_chain`.
- `tests/m7_chain.rs`: `sp11_k_seq_pins_seq_commit_merkle_branch` (hasher = `blake3::Hasher::new_keyed(&K_SEQ)` on 4 inputs, plus the golden vector of `hashing.rs` `test_seq_commit_golden` recomputed with the raw key), `sp11_k_mc_pins_seq_commit_mergeset_context` (also `mergeset_context` = `kaspa_seq_commit::hashing::mergeset_context_hash`), `sp11_seq_commit_equals_kaspa_seq_commit` (fixed blocks incl. u64::MAX fields × 3 parent commitments, and every block of a sim with a jump, an equal-DAA block and a side block, against the `kaspa_seq_commit` composition), `proof_bytes_layout` (A3 offsets), `smoke_probe_verifies_x_and_p_on_chain` (X and P of `anchored`, also with `enforce_budget = true` under the committed budget; every block of a line with a jump), `smoke_probe_fails_without_accessor` (`execute` → `InvalidOpcode`), `smoke_probe_rejects_each_altered_field` (one bit in each of parent_commit, activity_root, mpr, ts, daa, bs → `EvalFalse`/`VerifyError`; altered hash → `BlockAlreadyPruned`), `smoke_probe_accessor_failure_modes` (side block → `BlockNotSelected`; pov = bs + f − 1 passes, bs + f → `BlockIsTooDeep`; after `reorg_to` the old tip → `BlockNotSelected`).

**API for Stage 1** (`use harness::*;` brings `ChainSim`, `SimBlock`, `ANCHOR_LEN`, `BLOCK_PROOF_LEN`, `K_SEQ`, `K_MC`, `execute_on_chain`; the free functions are under `harness::chain::`).
- `SimBlock { hash, parent, blue_score, daa_score, sp_timestamp, activity_root, miner_payload_root }` (`Copy`). `parent == ZERO_HASH` marks a root of the sim, whose parent commitment is ZERO_HASH.
- Constructors: `ChainSim::anchored(d, x_hash)` — P = root at daa d − 1 / blue d − 1, X = `x_hash` at daa d / blue d, both on the chain, pov = X's blue score (requires d ≥ 1); `ChainSim::line(daa_start, blue_start, n)` — n ≥ 1 chain blocks, +1 daa and +1 blue each, pov = tip; `ChainSim::default()` — empty (every hash unknown), mainnet F.
- Chain growth (each moves the pov to the new tip): `push_chain(daa_delta, blue_delta) -> Hash`, `push_chain_with_hash(Some(hash), daa_delta, blue_delta) -> Hash` (to place a chosen X.hash, e.g. for SP3 seed searches), `extend(n) -> &mut Self`; fluent `with_jump(delta) -> Self` (one chain block with daa + delta, blue + 1; delta 0 = an equal-DAA chain block), `with_extend(n) -> Self`.
- Off-chain blocks: `add_side_block(parent, daa) -> Hash` (blue = parent's + 1; fluent `with_side_block(parent, daa)` drops the hash), `fork(from, n) -> Hash` (n off-chain descendants, returns the last); `reorg_to(tip) -> &mut Self` (chain := ancestors of `tip` inclusive, pov := tip's blue score; the old chain's blocks stay known, so they now give `BlockNotSelected`).
- Point of view and depth: `set_pov(bs) -> &mut Self`, `with_pov(bs) -> Self`, `set_finality_depth(f)`, `finality_depth()`, `pov_blue_score()`. `set_pov` moves only the depth threshold; it does not shrink the chain set (spec A7 literal: `is_chain_ancestor_from_pov` = `chain.contains`). To model a pov whose chain ends earlier, `reorg_to` the earlier block.
- Queries: `tip()`, `block(h) -> Option<&SimBlock>`, `on_chain(h)`, `chain_blocks()` (sorted by blue score), `commit_of(h) -> Option<Hash>`, `seq_commit(&SimBlock)` (parent commitment looked up in the sim).
- Anchor: `anchor_for(d) -> (X, P)` (first chain block with daa ≥ d — lowest blue score among equals — and its selected parent; panics if none), `try_anchor_for(d) -> Option<(X, P)>`, `block_bytes(&b) -> [u8; 152]`, `proof_bytes(&x, &p) -> Vec<u8>` (304 B, A3), `anchor_proof(d)` = `proof_bytes(anchor_for(d))`. For forged proofs (SA1, SA2, SA4) edit the bytes, or use `harness::chain::block_fields_bytes(&b, &parent_commit)` with any `SimBlock`/commitment and `seq_commit_with_parent(&parent_commit, &b)` to compute a commitment off the sim.
- Blocks are immutable once inserted (commitments are computed at insertion, parents first), so a built sim is read-only; share it as `Arc<ChainSim>` (`Send + Sync` is pinned by a test). For SP7 reorgs: `let mut b = (*arc).clone(); b.reorg_to(..); Arc::new(b)`.
- Execution: `harness::execute_on_chain(&tx, &entries, idx, enforce_budget, &*sim)` — identical to `execute` plus the accessor; `execute` keeps no accessor (the SA3 "opcode disabled" control returns `TxScriptError::InvalidOpcode`). Fabricated hashes and roots are deterministic (blake3 of a per-sim counter), so rebuilding the same fixture gives the same bytes.

**Deviations (draft DECISIONS rows, PENDING).**
- PENDING S0-1 (mechanical, API naming): spec A7 lists `.with_jump(at, delta)` and `.reorg_to(other_chain)`. Implemented as `with_jump(delta)` (appends one chain block at tip.daa + delta; position it with `line`/`extend` first, so "at" is the tip) and `reorg_to(tip: Hash)` (the other chain is built in the same sim with `fork`/`add_side_block` and selected by its tip). Added `push_chain`, `push_chain_with_hash`, `extend`, `with_extend`, `fork`, `add_side_block`, `try_anchor_for`, `anchor_proof`, `with_pov`, `set_finality_depth`. Reason: builders must return hashes for the attack tests, and keeping every block known makes the old chain's blocks side blocks (BlockNotSelected), as in consensus. No security effect.
- PENDING S0-2 (mechanical): `seq_commit` exists both as `ChainSim::seq_commit(&SimBlock)` (spec form) and as the free `harness::chain::seq_commit_with_parent(&parent_commit, &SimBlock)`. Stage 1's `lottery::anchor::seq_commit(&BlockFields)` can either duplicate the 4-hash function (then pin it equal to `kaspa_seq_commit` like SP11) or convert to a `SimBlock` and call `seq_commit_with_parent` (lottery and tn10 already depend on harness).
- No deviation in the formula, the key bytes or the opcode semantics: all three match spec A2/D95/A0 as cited above.

**What Stage 1 must know.**
- With the fixture `marker_daa = 1_000_000 + (r−1)·10⁶`, `ChainSim::anchored(D, x_hash)` puts X's blue score at D, so the pov is far from any window edge. Use `sim.anchor_proof(D)` for the `anchor` argument (304 B).
- In the probe a wrong preimage fails as `EvalFalse`/`VerifyError` from `require(... == c)`; in the drawer, attack tests should match on the input index and the `M7A_*` label as for other checks.
- Cost data point for the drawer budget/stack work: one block's recomputation (4 × `blake3WithKey` + the opcode, each hashed message ≤ 64 B) plus the length check and slices is ~2,410 script units; the drawer does it twice.
