# M4 handoff (staged execution, plan `fizzy-skipping-pebble.md`)

## Stage 1: layout and wiring (done, 2026-10-10, branch `m4-claim`, uncommitted)

Status: **green.** `cargo build --tests` is clean, and the full `cargo test` passes (M0–M3 plus the new tests). No new transitions were added. Stage 2 has not been started.

### What was done (files and why)

| File | Change | Spec |
|---|---|---|
| `covenants/drawer.sil` | State gains `byte[32] ticket_tpl` after `vault_tpl` (constructor arg `init_ticket_tpl`). Check 22 copies `ticket_tpl` into drawer'. Check 25 writes `ticket_tpl: ticket_tpl` and `reserve: ctrl.reserve` into all three pool `validateOutputStateWithTemplate` calls (jackpot, second prize, lower loop). The `PoolState` struct (used by check 18 and check 25) has the new layout. Comments updated (route of trust, check 22/25 headers). Check 1 adds no range (A7). | D54, D55, A3.2, A3.3, A7 |
| `covenants/pool.sil` | Pool state gains `byte[32] ticket_tpl` and `int reserve` after `deadline`, before `funder_class`, per A3.3. The `DrawerState` struct gains `ticket_tpl`. `absorb_leg` and the placeholder `claim()` are unchanged. | A3.3, A7 |
| `covenants/marker.sil`, `covenants/vault.sil` | `DrawerState` struct gains `ticket_tpl` (layout only). | A3.2 |
| `covenants/ticket.sil` | New first constructor parameter `byte[32] poolTemplateHash` (POOL_TPL). The body is still the M1 `locked()` placeholder. | A2, D54 |
| `crates/lottery/src/state.rs` | `DrawerState.ticket_tpl`; `PoolState.ticket_tpl` and `PoolState.reserve: i64` (field order = A3.3); `TicketState::ctor(pool_hash)`. | A8 |
| `crates/lottery/src/buy.rs` | `Templates::compile_sources` order is now controller → drawer(controller) → marker/vault/pool(drawer) → ticket(pool) → shard(ticket, controller), params(controller). The doc comment was updated. `dummy_pool` has the new fields. `Sources` is unchanged. | D54 |
| `crates/lottery/src/genesis.rs` | `pub const D_CLAIM_FLOOR: i64 = 11_000_000`; `validate()` refuses `d < D_CLAIM_FLOOR`. The genesis drawer gets `ticket_tpl = t.ticket.hash`. New `validate_with(&Templates)` holds all checks plus the drawer route (pool_tpl, vault_tpl, **ticket_tpl**). `validate()` = `validate_with(&Templates::get(b_max))`. `build_with(t)` calls `validate_with(&t)` (the old separate pool/vault panic folded in). New unit test `kp19_genesis_ticket_tpl_and_claim_floor`. | A7, D57, KP19 |
| `crates/lottery/src/draw.rs` | The fixture drawer gets `ticket_tpl = t.ticket.hash`. Expected new pools get `ticket_tpl = drawer.ticket_tpl` and `reserve = ctrl.reserve`. Fabricated absorbed pools carry the same values. `RoundResult` gains `ticket_tpl: [u8; 32]` and `reserve: u64`: `new()` takes them from the case drawer and from `x.values[0]`, `from_tx()` from the drawer redeem state (input 0) and output 0's value. `pool_state()` rebuilds both. | A8 (R4) |
| `crates/lottery/tests/common/m3.rs`, `tests/m3_attacks.rs` (`retarget`) | `retarget` also sets `drawer.ticket_tpl = t.ticket.hash`, so mutant and look-alike template sets stay consistent. | — |
| `crates/lottery/tests/m3_attacks.rs` | New `ka17_pool_ticket_tpl_reserve_and_drawer_ticket_tpl` (see KA17 below). | KA17 |
| `crates/lottery/tests/m3_draw.rs` | DP1 now asserts every new pool's `ticket_tpl = t.ticket.hash` and `reserve = R`, and that `RoundResult` (`new` and `from_tx`) carries `ticket_tpl` and `reserve`. | A8 |
| `crates/tn10` | No change needed. It builds through the library constructors and `RoundResult::from_tx`, and the workspace builds. | — |

**KA17** (`m3_attacks.rs`): base draw, np = 5. For each of the 5 new pools, six one-output edits each fail at the drawer through `pool_out`/`strike` (good passes, bad rejects at input 0):
- `ticket_tpl` bit-flip, zero, or = pool hash;
- `reserve` +1, −1 or 0.

Then drawer' gets `ticket_tpl` bit-flip, zero, or = pool hash. The base is accepted at the end.

Attribution (`M3_ATTRIB=1 cargo test -p lottery --release --test m3_attacks ka17 -- --nocapture --test-threads=1`): **30 × "check 25"** and **3 × "check 22 (output 0 script edited)"**, which is exactly as the spec names them.

**Mutants:** checks 22 and 25 contain no `// mut:` tags on the new fields. They are template-built `validateOutputState*` bindings; the only check 25 tags are `D25_class` and `D25_pad`, which are untouched. `m3_mutants` passes 93/93 unchanged.

**Fixtures and D_CLAIM_FLOOR:** no fixture change was needed. `TEST_DEPOSIT` (the genesis, buy and close fixture d) is already 11,000,000 = D_CLAIM_FLOOR. The only 0.1 KAS deposits are the compile-time dummy states in `buy.rs` (`compile_sources`, `dummy_shard`), which never go through genesis. `m3_game.rs` already uses 11,000,000. `m1_mutants`/`m1_measure` set `shard.d` on BuyCases, not on genesis.

### Decisions taken in Stage 1

| ID | Decision | Why |
|---|---|---|
| S1-1 | **`validate()` checks the drawer's route, `ticket_tpl` included, against the committed templates (`Templates::get(b_max)`). `validate_with(&Templates)` checks against a given set, and `build_with(t)` uses `validate_with(&t)`.** The pool_tpl/vault_tpl check moved from a separate panic in `build_with` into `validate_with` (it is now also an `Err` from `validate()`). | Spec A7 says ticket_tpl is "checked in `validate()`", but `validate()` had no templates. Tests that build genesis under mutant or look-alike templates keep working through `build_with(t)`. This is stricter than before: `validate()` now also catches a wrong pool_tpl/vault_tpl. |
| S1-2 | **The D_CLAIM_FLOOR check runs before the generic `d` range**, with an error naming `D_CLAIM_FLOOR (D57)`. | The KP19 test asserts the reason, not just a refusal. |
| S1-3 | **`RoundResult.reserve` is `u64`** (an output value), and `pool_state()` casts it to `PoolState.reserve: i64`. | R comes from output 0's value (A8). R ≤ RESERVE_MAX = 10¹⁰, so the cast is exact. |
| S1-4 | **Fabricated absorbed pools in `DrawCase::absorb` carry `ticket_tpl = drawer.ticket_tpl` and `reserve = controller.reserve`.** | These are the values a real round r − 1 pool would carry. The drawer does not read either field from absorbed pools (check 18 reads role, round, deadline and funder only). |
| S1-5 | **KA17 lives in `m3_attacks.rs`** (it attacks the drawer's `draw()`) and reuses `pool_out`/`drawer_out`. It also checks a "real other hash" (the pool hash as ticket_tpl), not just bit flips. | As the stage brief asked; this follows the M3 strike pattern. |

### Findings and open questions (no spec deviation decided)

1. **FYI: SilverScript drops an unused constructor constant.** I probed the compiled ticket: its suffix is 26 B and contains no pool hash. While the ticket body is the placeholder, **the ticket hash does not yet depend on POOL_TPL**. This is harmless now: drawer ticket_tpl = whatever the compiled ticket hash is, and KP19 checks that. In Stage 3 the hash will depend on POOL_TPL once the code reads it, which is what D54 needs. **Stage 3 should add a test** that recompiling the ticket with a different pool hash changes `t.ticket.hash` (that is, POOL_TPL is really bound). This is not a spec deviation.
2. No spec issue blocked Stage 1. A3.2/A3.3 field order is implemented exactly. I did not edit `docs/DECISIONS.md`: per the plan, S1-* rows go there in Stage 5. CLAUDE.md asks that every decision be recorded, so the orchestrator should carry S1-1…S1-5 into DECISIONS at Stage 5.
3. Cosmetic, pre-existing: `m2_measure` still labels the marker "marker (placeholder)" (it is the M3 marker, 1,399 B). Not changed.

### Open items for Stage 2

- `TicketState::ctor(pool_hash)`: every ticket compile now needs the pool hash. `Templates` handles it, and any new ticket look-alike source must be compiled through `compile_sources` (or with `t.pool.hash`).
- `RoundResult` now has everything needed to rebuild pool states (`pool_state(p)` includes ticket_tpl and reserve). Use it for fabricated pools (D63) and for `ClaimCase`.
- `Chain` in `tests/common/m2.rs` stores pools as `(outpoint, PoolState, …)` from `DrawExpected.new_pools`, which already carry the new fields.
- Pool-side redeem grew (see drift). The `ticket_leg` that replaces `claim()` in Stage 3 changes the pool hash and therefore the ticket hash (via POOL_TPL), the drawer state (ticket_tpl) and the shard constant (TICKET_TEMPLATE_HASH). All of these are computed, not hard-coded.
- Genesis: `D_CLAIM_FLOOR` is `lottery::genesis::D_CLAIM_FLOOR` (i64). The M4-c worst case should use d = D_CLAIM_FLOOR.

### Test output (full `cargo test` from `crates/`, debug, exit 0)

| Binary | Result |
|---|---|
| harness (unit) | 0 passed |
| covenant_smoke | 3 passed |
| m0_limits | 3 passed |
| m0_mass | 5 passed (139.6 s) |
| m0_merkle | 2 passed |
| m0_shapes | 6 passed |
| lottery (unit; includes new `kp19_genesis_ticket_tpl_and_claim_floor`) | 22 passed |
| m1_attacks | 25 passed |
| m1_buy | 14 passed |
| m1_measure | 4 passed |
| m1_mutants | 11 passed |
| m2_attacks | 40 passed |
| m2_close | 8 passed |
| m2_game | 2 passed |
| m2_measure | 5 passed |
| m2_mutants | 25 passed |
| m2_params | 17 passed |
| m3_attacks (includes new KA17) | 22 passed |
| m3_draw | 24 passed |
| m3_game | 13 passed |
| m3_measure | 7 passed |
| m3_mutants | 93 passed |
| doc-tests harness, lottery | 0, 0 |

All are `test result: ok`, with 0 failed. The only warnings are 2 pre-existing unused variables in `harness/tests/m0_mass.rs:459`.

Feature gates:
- `cargo check -p lottery --no-default-features --features mainnet,test-seed` fails as required: `error: TEST-ONLY seed cannot be built for mainnet`.
- `cargo check -p lottery --no-default-features --features mainnet` compiles.

### Drift (release `m3_measure` and `m2_measure`, against `docs/costs.md`)

Redeem sizes (M3-a):

| Object | costs.md | Now | Δ |
|---|---|---|---|
| drawer | 42,049 (state 104) | 42,914 (state 137, suffix 42,776) | +865 |
| marker | 1,326 | 1,399 | +73 (DrawerState struct) |
| vault | 594 | 667 | +73 |
| pool | 1,228 (state 607) | 1,358 (state 649, suffix 708) | +130 |
| controller | 27,694 | 27,694 | 0 |

Each delegate leg reads the larger drawer, so delegate units rose by about 6,200: marker 297,464 → 303,626, vault 296,001 → 302,163, pool (max) 297,287 → 303,563, controller 63,734 → 64,245.

M3-b / DP21 (K = 16, m = 50, a = 5):

| Shape | costs.md c / t / s | Now c / t / s | % now |
|---|---|---|---|
| np 5, 1 funding | 132,924 / 365,136 / 59,173 | 135,573 / 372,132 / 59,173 | 27.1 / 37.2 / 11.8 |
| np 5, F_MAX | 140,764 / 368,496 / 59,173 | 143,413 / 375,492 / 59,173 | 28.7 / 37.5 / 11.8 |
| np 7, 1 funding | 134,138 / 365,832 / 59,175 | 136,787 / 372,828 / 59,175 | 27.4 / 37.3 / 11.8 |
| np 7, F_MAX | 141,978 / 369,192 / 59,175 | 144,627 / 376,188 / 59,175 | 28.9 / 37.6 / 11.8 |

M3-c: committed drawer K_MAX 16 / M_DRAW 50 = 42,914 (was 42,049); the whole table shifted by about +865. The m_max = 50 worst shape is 28.9 / 37.6 / 11.8 %, and m_max stays 50.

M3-d storage is unchanged:
- supremum 292,182 (58.4 %) at f_min = 1 KAS;
- 382,182 (76.4 %) at F_MIN_FLOOR.

M3-f / M2-a / CP11, K = 16 `close()`:
- compute 293,841 → 293,914 and transient 864,604 → **864,896 (86.5 %)**, +292 from the 73 B larger marker; storage 402,301 is unchanged;
- K = 4: 102,533 / 311,932 → 102,706 / 312,224;
- the M2-c storage worst case is unchanged at 449,692 (89.9 %);
- the largest K at 90 % stays 16 on all three masses.

There is no stop case. These are layout-only increases. The real Stage 3 changes (the pool's `ticket_leg`, the marker's `refresh`) will need M4-e/M4-f re-measurement. The `close()` transient headroom is now 35,104 grams below 90 %.

## Stage 2a: tests first, builders and positive tests (done, 2026-10-10, branch `m4-claim`, uncommitted)

Status: **at the gate.**
- `cargo build --tests` is clean. The only warnings are the 2 pre-existing unused variables in `harness/tests/m0_mass.rs:459`.
- `m4_game` passes (4 tests, pure Rust).
- The full M0–M3 suite stays green (table below).
- `m4_claim`: 22 tests. 21 script positives FAIL, each at **input 0 with `EarlyReturn`** inside an M4 step (claim, return or refresh). `kp19_genesis_ticket_tpl` passes (no M4 script).
- No covenant source was touched. Attack tests (stage 2b) and covenant code (stage 3) were not started.

### Files

| File | What |
|---|---|
| `crates/lottery/src/claim.rs` (new) | `ClaimCase`/`ClaimExpected`/`ClaimLie`, `ReturnCase`/`ReturnPath`/`ReturnExpected`, `RefreshCase`/`RefreshObj`, entry signatures `SIG_*`, fixture helpers (`combo_for_tier`, `combo_matching`, `tier_shape`, `fabricated_ticket`, `payout_spk`, `ticket_leg_args`), constants `TEST_WIN5`, `TEST_POOL_DEADLINE`, `ST_ALL`, `TEST_PAYOUT`. Unit tests: `planned_encoding_matches_the_abi`, `drifted_signature_panics`, `combo_helpers_hit_every_tier`, `standalone_cases_are_valid_on_the_rust_side`. |
| `crates/lottery/src/lib.rs` | `pub mod claim;` (not gated on `test-seed`: it does not use the drawer). |
| `crates/lottery/src/game.rs` | M4 constants `P_MIN`, `D_RET`, `D_IDLE`, `M_CLAIM`, `TICKET_B_MAX` (4), `CLAIM_COUNT_MAX`, `CLAIM_RESERVE_MAX`; `enum ClaimMode { Final, Pay, Accrue }` (`n_cov`, `payout_output`, `n_fixed_outputs`); `tier_of`, `claim_amount`, `claim_mode` (A4 checks 6, 8, 1+4+9, step for step). |
| `crates/lottery/src/buy.rs` | `entry_sigscript` also accepts a full dispatch signature (S2-1); new `dispatch_tag(sig)` and `planned_sig_script(sig, args)`. Existing callers (plain entry names) are unchanged. |
| `crates/lottery/tests/common/m2.rs` | `accept_m4(step, built)`. `Chain` gains `tickets`, `drawn_trees`, `marker_daa`. `buy` records the ticket (output 1). `close` sets `marker_daa`. `draw_case` reads `marker_daa` (equal to the controller's DAA unless a refresh moved it). `draw` stores the round's trees in `drawn_trees`. New M4 steps (below). |
| `crates/lottery/tests/m4_game.rs` (new) | KP18 Rust half (4 tests). |
| `crates/lottery/tests/m4_claim.rs` (new) | KP1–KP17, KP18 script grid, KP19 integration (22 tests). |

Cargo needs no registration: tests are auto-discovered, and `blake3`/`silverscript-abi` were already dependencies.

### Entry contract for stage 3 (S2-1): signatures, not indices

SilverScript v1.0.0 does **not** select entries by index. Each entry's dispatch tag is the first 4 bytes of `blake3("name(type,type,...)")`. Source: `refs`-pinned `silverscript-lang/src/compiler/compile/helpers.rs:143-159` and the type names from `write_dispatch_type_name` (`helpers.rs:85-125`). The redeem script compares the pushed tag with each entry's tag in turn, and the last `OpElse` is `OpReturn` (`compile.rs:352-391`). So **entry order does not matter**. Only the exact name and the parameter types in order matter; parameter names are free. Stage 3 must declare exactly:

| Contract | Entry (as the builder calls it) | Called at | Args (builder order) |
|---|---|---|---|
| Ticket | `claim(int,byte[],byte[],int,int)` | input 0 | idx, leaves, siblings, pool_pl = `t.pool.prefix.len()`, pool_sl = `t.pool.suffix.len()` (`CLAIM_ARG_*` 0..4) |
| Ticket | `return_deposit_after()` | input 0 | none |
| Ticket | `return_deposit_with_pool(int,int)` | input 0 | pool_pl, pool_sl |
| Ticket, Pool, Marker | `refresh()` | input 0 | none |
| Pool | `ticket_leg(int,int)` | input 1 | ticket_pl = `t.ticket.prefix.len()`, ticket_sl = `t.ticket.suffix.len()` |

How it switches over:
- While an entry does not exist, `entry_sigscript` encodes it by hand (`planned_sig_script`: args pushed as the ABI does, then the tag).
- Once the compiled template has an entry of that **name**, the builder automatically uses the ABI encoder. It first asserts that the compiled tag equals `dispatch_tag(signature)`, and panics with "the compiled entry's signature differs from the builder's" otherwise. A drifted signature therefore fails loudly and does not silently fall through to `OpReturn`.
- The hand encoding is proven equal to the ABI's for ints (`Marker.draw_leg`) and for bytes, bool, byte and fixed bytes (`Shard.buy`): see the unit test `planned_encoding_matches_the_abi`.

Stage 3 should also:
- remove the ticket's `locked()` and the pool's placeholder `claim()` (spec A7);
- keep `byte[]` (dynamic) for leaves and siblings, as the drawer does;
- remember that a `bool`/`byte` parameter would change the signature string.

### API summary of `claim.rs` (enough for stage 2b and stage 3)

**`ClaimCase`** (`Clone`, all fields public):
- `b_max`;
- `ticket: TicketState`, `ticket_value` (d·b + accrued), `ticket_daa`;
- `pool: PoolState`, `pool_value` (= remaining + `pool.reserve`), `pool_daa`;
- `trees: Vec<Tree>` (the round's K shard trees);
- `idx: i64` (range attacks use −1 and b);
- `funding: Vec<FundingIn>`, `change: bool`, `fee`;
- `lock_time` (0: `claim()` has no time check);
- `lie: ClaimLie`, `cov`, `funding_sk`;
- `budget_delta: i32` (added to input 0's covering budget; KA16 uses −1).

Constructors:
- `ClaimCase::new(k)`: the standalone base. m = 10; a b = 2 ticket with idx 0 = a tier 2 (4 + bonus) combination held once in shard 0 and idx 1 non-winning; a tier 2 pool with slice 0.2 KAS. This is a **PAY of 0.2 KAS**.
- `ClaimCase::fabricated(k, m, tier, slice, holders)` / `fabricated_with(…, &Templates)` (D63):
  - the win is `TEST_WIN5` with bonus 4 (1 if m < 4); the combination comes from `combo_for_tier(win, tier, m, 0)`;
  - shard i holds it `holders[i]` times (`holders[0] ≥ 1` is the ticket itself) plus one non-winning filler, so every shard is non-empty;
  - pool: round 1, the trees' roots, deadline `TEST_POOL_DEADLINE` = 7,500,000, `ticket_tpl` = `t.ticket.hash`, reserve `TEST_RESERVE`, funder = the funding key; remaining = slice + 1 KAS;
  - ticket value 2·TEST_DEPOSIT, ticket DAA 300,000, pool DAA 2,000,000;
  - funding: 10 KAS at DAA 2,100,000; fee 0.1 KAS.

Helpers: `make_final()` (marks every other index claimed), `sync_roots()`, `remaining()`, `bit()`, `comb()`, `funding_total()`, `k()`. Input constants: `TICKET` = 0, `POOL` = 1, `funding_input(n)` = 2 + n.

**`ClaimCase::expected() -> ClaimExpected`**. It **never panics on reference failures** (only on a no-change case whose funding ≠ fee). It lists them in `rejects: Vec<String>` ("check 1: idx outside [0, b)", "check 3: pool ranges", "check 3: pool round != ticket round", "check 4: index already claimed", "check 5: combination not canonical", "check 6: tier", "check 8: h < 1", "check 8: prize > remaining", "check 12: prize + ticket value > TOTAL_MAX", "check 1: ticket b or payout_class", "check 1: ticket round or deadline"). Positive tests assert `rejects` is empty; attacks can assert which check the reference names.

Fields: `idx`, `comb: [u8; 6]`, `rank`, `tier: Option<u8>`, `leaves`, `siblings`, `counts` (the supplied per-proof count strings), `h`, `prize`, `remaining`, `bit`, `mode`, `pool` (pool' = the pool state unchanged), `ticket: Option<TicketState>` (ticket' with the bit set; `None` on FINAL), `values` (every output in order, change last), `change_output`, `rejects`. Methods: `n_cov()`, `payout_output()`, `n_outputs()`.

When the reference rejects the mode (bit already set, idx out of range), `mode` falls back to "the shape a cheater would build": FINAL if `c | bit = 2^b − 1`, else PAY if prize ≥ P_MIN, else ACCRUE. `lie.mode` overrides everything.

**`ClaimLie`**, the consistent false claims for stage 2b (the builder follows each one everywhere):
- `counts: Option<Vec<Vec<u16>>>`: leaves come from these, siblings stay real, and h follows;
- `k_proofs: Option<usize>`: only the first n proofs;
- `swap: Option<(i, j)>`: swaps two proofs; h is unchanged;
- `rank: Option<u32>`: proves another rank's leaf and siblings;
- `h: Option<i64>`: the prize follows;
- `prize: Option<i64>`;
- `mode: Option<ClaimMode>`: the shape follows.

**`ClaimCase::build()` / `build_with(Arc<Templates>)` → `Built`**:
- inputs: [ticket `Entry{ticket, SIG_CLAIM, args}`, pool `Entry{pool, SIG_TICKET_LEG, ticket_leg_args}`, funding…];
- outputs: pool' (binding (0, cov)); then FINAL: payout; PAY: ticket' (binding) and payout; ACCRUE: ticket' (binding); then change.

The payout script is `payout_spk(class, key)`: a class above 2 falls back to Schnorr so that range attacks build. `args(&t, &x)` gives the claim argument list. Output-side attacks edit the `Built` (`entry_args_mut(0)`, `outputs`, `set_entry_redeem`), as in M3.

**`ReturnCase`**:
- fields: `b_max`, `ticket`, `ticket_value`, `ticket_daa`, `path: ReturnPath::{After, WithPool{pool, pool_value, pool_daa}}`, `funding`, `change`, `fee`, `lock_time`, `cov`, `funding_sk`, `budget_delta`;
- constructors:
  - `ReturnCase::after()` / `after_of(ticket, value, daa)`: lock_time = T + D_RET;
  - `ReturnCase::with_pool()` / `with_pool_of(ticket, tv, tdaa, pool, pv, pdaa)`: lock_time = pool.deadline;
  - the standalone versions use `ClaimCase::new(1)`'s ticket and pool;
- `expected() -> ReturnExpected { pool, values, payout_output, change_output }`;
- inputs: path b [ticket `SIG_RETURN_AFTER`, funding…]; path a [ticket `SIG_RETURN_WITH_POOL(pool_pl, pool_sl)`, pool `SIG_TICKET_LEG`, funding…];
- outputs: path a pool' (same state and value, binding); payout = the whole ticket value; change;
- `funding_input(n)`.

**`RefreshCase`**:
- fields: `b_max`, `obj: RefreshObj::{Marker, Pool, Ticket}`, `value`, `daa` (the creation DAA = OpTxInputDaaScore(0)), `funding`, `change`, `fee`, `lock_time`, `cov`, `funding_sk`, `budget_delta`;
- `RefreshCase::new(obj, value, daa)`: lock_time = daa + D_IDLE; funding 10 KAS at daa + 1,000; fee 0.1 KAS;
- `values()`;
- output 0 has the same script and value as input 0, with binding (0, cov); then change.
- `RefreshObj::{template, redeem, spk}(&t)`.

Common to all three:
- `Built::finish()` sizes budgets with `ComputeBudget::checked_covering_script_units` (`harness::budget_for`), commits the storage mass and signs funding;
- sequence 0 (`Built::assemble`), and lock_time = the case's `lock_time` (check-7 fact row);
- change = funding − fee in every shape (no covenant value pays fees, D60); without change, funding must equal the fee.

**`Chain` M4 steps** (`tests/common/m2.rs`). Each `case` fn funds 10 KAS from the ledger at `funding_daa` and returns (case, outpoints in input order). Each run fn uses `accept_m4` and then applies to the ledger.
- `buy_case_funded(i, funding)`: buys at prices above 1 KAS.
- `fabricate_ticket(state, value, daa)` and `fabricate_pool(state, remaining, daa)`: D63 UTXOs under `ch.cov`, tracked; they return the index.
- `pool_of(round, tier)` gives the index in `pools`.
- `claim_case(ti, pi, idx, funding_daa)` / `claim(c, ops, daa)`:
  - the pool moves to pool' with remaining − prize;
  - the ticket moves to ticket' or is removed on FINAL;
  - `claim` asserts `rejects` is empty, so attacks use `claim_built` plus their own runner.
- `return_after_case(ti, …)`, `return_with_pool_case(ti, pi, …)` / `return_deposit(c, ops, daa)`.
- `refresh_marker_case(…)`, `refresh_pool_case(pi, …)`, `refresh_ticket_case(ti, …)` / `refresh(c, ops, daa)`: the object's outpoint and creation DAA move.
- `tickets[i]` is (outpoint, state, value, DAA). `drawn_trees[round]` holds the K trees.

### Positive tests (`m4_claim.rs`) and fixtures

The chain fixtures follow D63:
- `round1(price, st, bundles)` runs genesis (K = 4) at `price` per combination and lower shares `st`, then buys (`sell_close_draw`), close, and draw with seed [1; 32].
- `P100` = 100 KAS per combination (S2-6). At the default 1 KAS price every lower slice is below P_MIN (e.g. tier 4 + b: 0.04·S / 125), so PAY-size lower prizes need a larger S.
- Claims are funded at `NOW − 1,000` and applied at `NOW` = 2,000,000.

| ID | Test | Fixture facts asserted before any script runs |
|---|---|---|
| KP1 | `kp1_pay_claim_lower_tier` | tier 2, h = 1, PAY, prize = slice ≥ P_MIN (P100, S = 400 KAS) |
| KP2 | `kp2_duplicates_split_exactly` | one combination twice in shard 0's bundle and once in shard 1: h = 3 for all three claims, prize = slice / 3; pool drop = 3 × prize |
| KP3 | `kp3_jackpot_claims` | h_J = 1 (b = 2 ticket: PAY, prize = J), then h_J = 2 (two b = 1 tickets: FINAL, J / 2 each) |
| KP4 | `kp4_second_prize_claims` | n2 = 1, so the pool holds G2; h = 2 at one other bonus; prize = G2 / 2 |
| KP5 | `kp5_final_single_combination` | b = 1: FINAL, payout = prize + d, no ticket', ticket removed |
| KP6 | `kp6_final_after_accrue` | idx 0 tier 3 ACCRUE (0 < prize < P_MIN), then idx 1 tier 2 FINAL = prize + 2d + accrued |
| KP7 | `kp7_accrue_then_return` | ACCRUE (ticket' = 2d + prize), then `return_deposit_after` pays 2d + prize |
| KP8 | `kp8_p_min_boundary` | fabricated: slice P_MIN → PAY; P_MIN − 1 → ACCRUE |
| KP9 | `kp9_zero_prize` | fabricated: slice 1, h = 2 → prize 0, ACCRUE, pool value unchanged, bit set |
| KP10 | `kp10_one_claim_per_tier` | `ST_ALL`, P100: all 7 pools exist; each tier is claimed on a clone (tier and h = 1 asserted) |
| KP11 | `kp11_empty_leaves` | all 4 shards non-empty (root ≠ Z_18); shards 1..3 have Z_0 leaves at the rank; h = 1 |
| KP12 | `kp12_drained_pool_absorbed` | h = 2, two FINAL claims drain the pool; round 2 buy and close; the draw absorbs it at its deadline. Q = remainder, O' formula as DP14, R to the funder, the pool gone |
| KP13 | `kp13_return_after_at_t_plus_d_ret` | lock_time = T + D_RET exactly; payout 2d |
| KP14 | `kp14_return_with_pool_at_deadline` | lock_time = pool.deadline; pool' byte-identical, same value |
| KP15 | `kp15_refresh_then_use` (chain) | marker refresh → draw on the refreshed marker → pool A, ticket and pool B refreshed → claim on the refreshed ticket and pool A → round 2 absorbs refreshed pool B |
| KP15 | `kp15_refresh_{ticket,pool,marker}_standalone` | each object at creation + D_IDLE exactly, with and without change |
| KP16 | `kp16_change_and_f_max_funding` | PAY without change (3 outputs); 8 funding inputs; a return without change |
| KP17 | `kp17_k16_m50_each_mode` | K = 16, m = 50, h = 16 (every leaf non-empty), PAY / ACCRUE / FINAL; leaves 1,600 B, siblings 9,216 B. Acceptance only: the M4-b numbers are stage 4 |
| KP18 | `kp18_script_accepts_reference_grid` | tiers 0..6 × {FINAL, PAY, ACCRUE}, standalone |
| KP19 | `kp19_genesis_ticket_tpl` (+ the stage 1 unit test) | drawer ticket_tpl = the compiled ticket hash; every round-1 pool has it and reserve = R; d < D_CLAIM_FLOOR and a wrong ticket_tpl are refused |

`check_claim` verifies every output:
- count, values, scripts and covenant bindings;
- pool' byte-identical, with pool' = pool − prize;
- per mode, the payout and ticket' values; ticket' differs only in `claimed |= bit`;
- change last; inputs − outputs = fee;
- conservation: pool + ticket = the covenant and payout outputs.

`check_return` and `check_refresh` do the same for their shapes.

### Decisions taken in Stage 2a

| ID | Decision | Why |
|---|---|---|
| S2-1 | **Entries are called by full dispatch signature (`SIG_*`).** Missing entries are encoded by hand; existing ones go through the ABI with a tag-equality assert. | SilverScript selects entries by `blake3(signature)[..4]`, not by index (see "Entry contract"). The ABI encoder cannot encode an entry the artifact lacks, so the positives would otherwise panic in the builder instead of failing in the script. |
| S2-2 | **FINAL has 2 fixed outputs (pool', payout).** `ClaimMode::n_fixed_outputs` = nCov + [FINAL or PAY], per the A4 shape table and D56. | A4 check 11's formula `nCov + [pay] + ch` gives 1 + ch for FINAL, which contradicts the table. **Open question Q1.** |
| S2-3 | `ClaimExpected.rejects` lists reference failures instead of panicking, with a "cheater's shape" mode fallback. | Stage 2b can build every KA case from the same builder and assert the reference's named check. |
| S2-4 | `claim()` and the pool leg have no time check (A4), so `ClaimCase.lock_time` = 0. Inputs use sequence 0 (unchanged `Built::assemble`). Returns and refreshes use lock_time = their bound exactly (KP13, KP14, KP15). | Check-7 fact row: CLTV needs sequence ≠ MAX and lock_time in the DAA domain. |
| S2-5 | change = funding − fee in every M4 shape; the no-change cases use funding = fee exactly. | D60: the caller pays the fee, and no covenant value moves to fees. |
| S2-6 | Chain positives that need PAY-size lower prizes use price 100 KAS per combination (`P100`). KP10 uses `ST_ALL` = [400, 300, 300, 200, 200] (share room 4,000 ≤ 10⁴). | At 1 KAS the lower slices are below P_MIN. Genesis `validate()` accepts both. |
| S2-7 | `Chain` tracks tickets, the drawn rounds' trees and the marker's creation DAA. `draw_case` reads `marker_daa`. | Claims need the round's trees after the draw (the chain used to drop them), and KP15 needs a refreshed marker's DAA in the draw (check 19 own_daa). M2/M3 behaviour is unchanged: marker_daa = the close DAA = controller DAA. |
| S2-8 | `accept_m4(step, b)` (input 0 first) panics with the step name and every input's result. | Gate evidence: it shows the rejection is the M4 transaction's, not that of a genesis, buy, close or draw before it (those also use `accept`). |
| S2-9 | KP15 is one chain test plus three standalone refresh tests, one per object. | A chain test stops at its first failure (the marker refresh). Separate tests show each object's own rejection now and isolate failures in stage 3. |
| S2-10 | `TICKET_B_MAX` = 4 in `game.rs` (the ticket's B_MAX constant per A2). `claim_mode` uses it. | It mirrors the script constant. M1 tests that resize B_MAX via `ticket_source(b_max)` do not claim. |

### Open questions (not decided; for the orchestrator and the user)

1. **Q1 (spec inconsistency, A4 check 11).** `nOut = nCov + [pay] + ch` gives 1 + ch for FINAL, but the A4 table and D56 put a payout at output 1 for FINAL. The builder and tests follow the table (S2-2), i.e. nOut = 2 + [pay] + ch. Stage 3 must implement the same, and the spec should be corrected (proposed: "nOut = nCov + [final ∨ pay] + ch"). Check 11's "every output ≥ nCov has the zero ID" is consistent with the table.
2. **Note for the reviewer (no deviation).** A `refresh()` moves an object's creation DAA.
   - A refreshed marker raises `draw()`'s own_daa (check 19), and so that round's D_claim, by at most the refresh time. KP15 exercises it.
   - A refreshed pool that a later draw absorbs also raises that draw's own_daa. That draw's funding is already fresh (after seed_ref + DRAW_WINDOW), so the effect is bounded by "now".
   - Not a deadline move (D59), but worth one line in the stage-5 brief ("refresh idle check with DAA −1" is already there).
3. **Note.** `claim()` has no lower or upper time bound. A claim stays possible after pool.deadline until a `draw()` absorbs the pool, which matches the spec (absorption, not expiry, ends claims; R3: no expiry). Stated here only so that stage 3 does not add a deadline check.
4. **Note.** KP17's "within limits (M4-b)" is acceptance only here. The mass/limit numbers are stage 4 (`m4_measure`).

### For stage 2b (attacks)

- Use `ClaimCase`/`ReturnCase`/`RefreshCase` + `ClaimLie` + `Built` edits. `strike`/`reject_at` from `common/m2.rs` already work for any `Built` (input-keyed), and `accept_m4` is the acceptance half.
- Until stage 3, every strike fails in its "good passes" half (the good transaction is rejected at input 0 or 1 with EarlyReturn), as the plan expects.
- D63 range attacks: `Chain::fabricate_ticket` / `fabricate_pool`, or a `ClaimCase` with edited `ticket` / `pool` fields. Both build at the instance ID.
- A look-alike ticket from a fake shard under a fresh ID (KA4) means changing `ClaimCase.cov` on the ticket input only. Stage 2b needs a small `Built` edit: rebind the input-0 entry's covenant ID to `OTHER_COV`.
- Budget − 1 (KA16): set `budget_delta = −1` on the case.

### For stage 3

- Entry signatures above (exact). Tag drift panics in the builder.
- The PT checks read `ticket_tpl` from the pool's own state.
- The ticket reads the pool with `readInputStateWithTemplate(1, pool_pl, pool_sl, POOL_TPL)`; `pool_pl`/`pool_sl` are `t.pool.prefix/suffix.len()`.
- The stage 1 finding still stands: once the ticket reads POOL_TPL, add a test that a different pool hash changes `t.ticket.hash`.
- The FINAL output count (Q1).
- **Two existing tests encode the ticket's `locked()` through the ABI with `.unwrap()`:** `crates/lottery/tests/m1_attacks.rs:101` and `crates/lottery/tests/m2_attacks.rs:404`. Both spend a ticket inside a buy or close attack, where the ticket input is the attack's point.
  - Once stage 3 removes `locked()` (A7), both will panic in the encoder.
  - Stage 3 must switch them to a real entry (e.g. `refresh()`, which those shapes also reject) or to `lottery::buy::planned_sig_script("locked()", &[])`, which keeps a non-existent tag.
  - Then check that each still fails at its named input for the same reason.

### Test output (stage 2a, debug, from `crates/`)

**Why `EarlyReturn` is the right reason.** The compiled dispatcher compares the pushed tag with each entry's tag; the last `OpElse` is `OpReturn` (`silverscript-lang/src/compiler/compile.rs:352-391`, SilverScript v1.0.0). So `EarlyReturn` means "no entry with this signature". The ticket's `locked()` placeholder would fail with `VerifyError`, not `EarlyReturn`.

Other facts from the output:
- Funding inputs pass (`Ok(100001)`), so the signing and the sighash are right.
- The pool's `ticket_leg` (input 1) also fails at dispatch.
- No failure occurs in the genesis, buy, close or draw that precede the M4 steps: every panic is an `accept_m4` panic naming an M4 step.

Full `cargo test --no-fail-fast` from `crates/` (debug):

| Binary | Result |
|---|---|
| harness (unit) | 0 passed |
| covenant_smoke | 3 passed |
| m0_limits | 3 passed |
| m0_mass | 5 passed (100.2 s) |
| m0_merkle | 2 passed |
| m0_shapes | 6 passed |
| lottery (unit) | **26 passed** (22 + the 4 new `claim::tests`) |
| m1_attacks | 25 passed |
| m1_buy | 14 passed |
| m1_measure | 4 passed |
| m1_mutants | 11 passed |
| m2_attacks | 40 passed |
| m2_close | 8 passed |
| m2_game | 2 passed |
| m2_measure | 5 passed |
| m2_mutants | 25 passed |
| m2_params | 17 passed |
| m3_attacks | 22 passed |
| m3_draw | 24 passed |
| m3_game | 13 passed |
| m3_measure | 7 passed |
| m3_mutants | 93 passed |
| **m4_claim** | **FAILED: 1 passed (kp19), 21 failed** (expected at this stage) |
| **m4_game** | **4 passed** |
| doc-tests harness, lottery | 0, 0 |

`m4_claim` failures (exact panic text, from the same run; sorted):

```
kp1_pay_claim_lower_tier: M4 step `claim`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp10_one_claim_per_tier: M4 step `claim`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp11_empty_leaves: M4 step `claim`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp12_drained_pool_absorbed: M4 step `claim`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp13_return_after_at_t_plus_d_ret: M4 step `return_deposit`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
kp14_return_with_pool_at_deadline: M4 step `return_deposit`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp15_refresh_marker_standalone: M4 step `refresh (standalone marker)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
kp15_refresh_pool_standalone: M4 step `refresh (standalone pool)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
kp15_refresh_then_use: M4 step `refresh (marker)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
kp15_refresh_ticket_standalone: M4 step `refresh (standalone ticket)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
kp16_change_and_f_max_funding: M4 step `claim (standalone)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp17_k16_m50_each_mode: M4 step `claim (standalone)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp18_script_accepts_reference_grid: M4 step `claim (standalone)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp2_duplicates_split_exactly: M4 step `claim`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp3_jackpot_claims: M4 step `claim`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp4_second_prize_claims: M4 step `claim`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp5_final_single_combination: M4 step `claim`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp6_final_after_accrue: M4 step `claim`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp7_accrue_then_return: M4 step `claim`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp8_p_min_boundary: M4 step `claim (standalone)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
kp9_zero_prize: M4 step `claim (standalone)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
test result: FAILED. 1 passed; 21 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.83s
```

The chain positives assert their Rust-side fixture facts (mode, tier, h, prize) before the first M4 step. Reaching `accept_m4` therefore shows that the genesis, buys, close and draw of each fixture passed the real scripts, and that the fixture is what the test names.

Commands:
- `cargo test -p lottery --test m4_game --test m4_claim`;
- `cargo test -p lottery --lib claim` (the 4 claim units, including the encoding proof).

Feature gates (re-run): `cargo check -p lottery --no-default-features --features mainnet` compiles (`claim.rs` uses no `test-seed` item); `--features mainnet,test-seed` fails as required with `error: TEST-ONLY seed cannot be built for mainnet`.


## Stage 2b: attack tests KA1–KA16 (done, 2026-10-10, branch `m4-claim`, uncommitted)

Status: **at the gate.**
- `cargo build --tests` is clean (only the 2 pre-existing `m0_mass.rs:459` warnings).
- M0–M3, `m4_game` and the lottery unit tests stay green (table at the end).
- `m4_attacks`: 19 tests. 16 FAIL, each on the **"good passes" half** of its first strike (`BASE REJECTED at target input N: EarlyReturn`: dispatch finds no M4 entry) or, for KA16, on the base acceptance. 3 pass: the two structural ledger tests and the instrument self-test.
- Under `M4_PREFLIGHT=1` all 19 pass. That run executes every one of the 146 attacks plus the 8 KA16 cases. Each bad transaction is rejected by a *script* error (`EarlyReturn`), never by a covenant-context error. Each attack really changes the transaction, and each good transaction fails only at dispatch. So no attack fails early because of a builder bug.
- No covenant source was touched.

### Files

| File | What |
|---|---|
| `crates/lottery/tests/common/m4.rs` (new) | The attack library: `Case` (`Claim`/`Return`/`Refresh`, `retarget`), `Atk` (id, input, expected check, good case, case edit, `Built` edit; `pair`, `pair_case`, `strike`), `strike4`, `done4`, `M4_PREFLIGHT`, the `M4_ATTRIB` instrument (`first_failing_check4`, `first_failing_check_in`, `passes_under_source`), bases (`pay`, `fin`, `accrue`, `ka2_base`, `claim_with_second`, `refresh_{ticket,pool,marker}`), `Built` edit helpers, one `pub fn kaN() -> Vec<Atk>` per KA (KA1–KA15), `ka16_cases()`, `chain_round1()`. |
| `crates/lottery/tests/m4_attacks.rs` (new) | 19 tests: `ka1_double_claim_same_index`, `ka1_double_claim_ledger`, `ka2_forged_h`, `ka3_wrong_tier`, `ka4_other_round_and_family`, `ka5_payout`, `ka6_pool_successor`, `ka7_next_round_pool`, `ka7_absorbed_pool_ledger`, `ka8_mode_cheating`, `ka9_ticket_successor`, `ka10_shape`, `ka11_pool_leg_input0`, `ka12_ranges`, `ka13_return_after`, `ka14_return_with_pool`, `ka15_refresh`, `ka16_leader_budget_minus_one`, `attribution_instrument_self_test`. |
| `crates/lottery/tests/common/mod.rs` | `pub mod m4;` |
| `crates/lottery/tests/common/m2.rs` | `Sil::Ticket` (in `source_mut` and `hash_of`), so `lookalike(Sil::Ticket)` / `mutant(Sil::Ticket, …)` work (KA11; stage 4). |
| `crates/lottery/tests/m2_mutants.rs` | `route_control`'s `unreachable!()` arm also lists `Sil::Ticket` (exhaustive match). |

`claim.rs`, `game.rs`, `buy.rs` and the `Chain` were not changed: the stage 2a API was enough.

### How an attack is checked

`Atk::strike()` (committed templates) does the following:
1. builds good and bad (`good` → case edit → build → `Built` edit);
2. asserts the attack changed something: tx ID, input entries **or signature scripts** (S2b-2);
3. requires the bad transaction to be rejected by `input` with a script error (`is_script_rejection`: not a budget error, not a covenant-context error);
4. requires the good transaction to pass `input`;
5. accepts the good transaction with every input (`accept_m4`).

Each test runs its attacks in order and stops at the first failure.

Commands (from `crates/`):
- `cargo test -p lottery --test m4_attacks`
- gate evidence before stage 3: `M4_PREFLIGHT=1 cargo test -p lottery --test m4_attacks -- --nocapture --test-threads=1` (lines `PREFLIGHT <test> | <attack id> | expect input <i> <check> | bad: … | good: … | reference: <ClaimExpected.rejects>`)
- attribution after stage 3: `M4_ATTRIB=1 cargo test -p lottery --release --test m4_attacks -- --nocapture --test-threads=1` (lines `ATTRIB <test> | <attack id> | expected <check> | got <check> | OK|MISMATCH|NO MARKERS`)

### Attack list (146 strikes + 8 budget cases + 2 ledger tests)

Check names:
- `ticket:N` = A4 claim check N;
- `ticket:A5b.N` / `ticket:A5a.N` = A5 path b / path a check N;
- `<obj>:A6.N` = A6 refresh check N;
- `pool:PTn` = the pool's `ticket_leg`.

Input 0 is the ticket (claim, returns) or the refreshed object; input 1 is the pool. "Reference" is `ClaimExpected.rejects` for the bad claim case. "—" means the reference accepts the case: the attack is in the `Built`, or in proof bytes the reference does not check.

| Attack (id as printed) | Input | Expected check | Reference |
|---|---|---|---|
| KA1 index already claimed (PAY) | 0 | `ticket:4` | [check 4: index already claimed] |
| KA1 index already claimed (ACCRUE) | 0 | `ticket:4` | [check 4: index already claimed] |
| KA2 stale leaf: shard 1 count - 1 at the bonus | 0 | `ticket:7` | — |
| KA2 stale leaf: shard 1 count - 1 at another bonus | 0 | `ticket:7` | — |
| KA2 Z_0 for shard 3's non-empty leaf | 0 | `ticket:7` | — |
| KA2 k - 1 proofs | 0 | `ticket:7` | — |
| KA2 proofs of shards 0 and 3 swapped (equal leaves) | 0 | `ticket:7` | — |
| KA2 proofs of shards 0 and 1 swapped | 0 | `ticket:7` | — |
| KA2 leaves + 2 bytes | 0 | `ticket:7` | — |
| KA2 leaves - 2 bytes | 0 | `ticket:7` | — |
| KA2 siblings + 1 byte | 0 | `ticket:7` | — |
| KA2 siblings - 1 byte | 0 | `ticket:7` | — |
| KA2 another rank's leaf and path (a decoy held in shards 0, 2) | 0 | `ticket:7` | — |
| KA2 under-reported h with true leaves (outputs follow h = 2) | 0 | `ticket:12` | — |
| KA3 tier 0 combination against the tier 1 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 tier 1 combination against the tier 0 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 tier 1 combination against the tier 2 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 tier 2 combination against the tier 1 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 tier 2 combination against the tier 3 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 tier 3 combination against the tier 2 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 tier 3 combination against the tier 4 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 tier 4 combination against the tier 3 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 tier 4 combination against the tier 5 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 tier 5 combination against the tier 4 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 tier 5 combination against the tier 6 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 tier 6 combination against the tier 5 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 non-winning combination (1 match with the bonus) against the tier 6 pool | 0 | `ticket:6` | [check 6: tier] |
| KA3 non-winning combination (2 matches without the bonus) against the tier 6 pool | 0 | `ticket:6` | [check 6: tier] |
| KA4 round 2 ticket against a round 1 pool | 0 | `ticket:3` | [check 3: pool round != ticket round] |
| KA4 look-alike ticket under a fresh ID, at the pool | 1 | `pool:PT2` | — |
| KA4 look-alike ticket under a fresh ID, at the ticket | 0 | `ticket:2` | — |
| KA5 PAY payout to another script | 0 | `ticket:13` | — |
| KA5 PAY payout to another class (P2SH of the key) | 0 | `ticket:13` | — |
| KA5 PAY payout + 1 | 0 | `ticket:12` | — |
| KA5 PAY payout - 1 | 0 | `ticket:12` | — |
| KA5 PAY extra plain output | 0 | `ticket:11` | — |
| KA5 FINAL payout to another script | 0 | `ticket:13` | — |
| KA5 FINAL payout to another class (P2SH of the key) | 0 | `ticket:13` | — |
| KA5 FINAL payout + 1 | 0 | `ticket:12` | — |
| KA5 FINAL payout - 1 | 0 | `ticket:12` | — |
| KA5 FINAL extra plain output | 0 | `ticket:11` | — |
| KA5 PAY payout at output 1 (ticket' at 2) | 0 | `ticket:10` | — |
| KA5 FINAL payout at output 2 (change at 1) | 0 | `ticket:12` | — |
| KA6 pool' value + 1 | 0 | `ticket:12` | — |
| KA6 pool' value - 1 | 0 | `ticket:12` | — |
| KA6 pool' slice + 1 | 0 | `ticket:14` | — |
| KA6 pool' deadline + 1 | 0 | `ticket:14` | — |
| KA6 pool' roots byte flipped | 0 | `ticket:14` | — |
| KA6 pool' ticket_tpl byte flipped | 0 | `ticket:14` | — |
| KA6 pool' reserve + 1 | 0 | `ticket:14` | — |
| KA6 pool' funder key byte flipped | 0 | `ticket:14` | — |
| KA7 the next round's pool of the same tier | 0 | `ticket:3` | [check 3: pool round != ticket round] |
| KA8 PAY with prize < P_MIN | 0 | `ticket:11` | — |
| KA8 ACCRUE with prize >= P_MIN | 0 | `ticket:12` | — |
| KA8 PAY shape (ticket' kept) when final | 0 | `ticket:10` | — |
| KA8 ACCRUE shape (ticket' kept) when final | 0 | `ticket:10` | — |
| KA8 FINAL (ticket consumed) with another index unclaimed | 0 | `ticket:10` | — |
| KA9 PAY ticket' bit not set | 0 | `ticket:15` | — |
| KA9 PAY ticket' the wrong bit set | 0 | `ticket:15` | — |
| KA9 PAY ticket' round + 1 | 0 | `ticket:15` | — |
| KA9 PAY ticket' T + 1 | 0 | `ticket:15` | — |
| KA9 PAY ticket' b - 1 | 0 | `ticket:15` | — |
| KA9 PAY ticket' bundle byte changed | 0 | `ticket:15` | — |
| KA9 PAY ticket' payout key changed | 0 | `ticket:15` | — |
| KA9 PAY ticket' payout class changed | 0 | `ticket:15` | — |
| KA9 PAY ticket' value - 1 | 0 | `ticket:12` | — |
| KA9 ACCRUE ticket' bit not set | 0 | `ticket:15` | — |
| KA9 ACCRUE ticket' the wrong bit set | 0 | `ticket:15` | — |
| KA9 ACCRUE ticket' round + 1 | 0 | `ticket:15` | — |
| KA9 ACCRUE ticket' T + 1 | 0 | `ticket:15` | — |
| KA9 ACCRUE ticket' b - 1 | 0 | `ticket:15` | — |
| KA9 ACCRUE ticket' bundle byte changed | 0 | `ticket:15` | — |
| KA9 ACCRUE ticket' payout key changed | 0 | `ticket:15` | — |
| KA9 ACCRUE ticket' payout class changed | 0 | `ticket:15` | — |
| KA9 ACCRUE ticket' value - 1 | 0 | `ticket:12` | — |
| KA9 PAY ticket' value + prize (prize paid twice) | 0 | `ticket:12` | — |
| KA10 parallel group (a second ticket and pool pair) | 0 | `ticket:2` | — |
| KA10 two tickets, one pool, one payout: at ticket A | 0 | `ticket:2` | — |
| KA10 two tickets, one pool, one payout: at ticket B (input 2) | 2 | `ticket:1` | — (passes already, see S2b-6) |
| KA10 extra covenant output (a ticket' copy) | 0 | `ticket:10` | — |
| KA10 extra covenant output on FINAL (a pool' copy) | 0 | `ticket:10` | — |
| KA10 funding input with a covenant ID | 0 | `ticket:2` | — |
| KA10 ticket and pool swapped: the pool at 0 | 0 | `pool:PT1` | — |
| KA10 ticket and pool swapped: the ticket at 1 | 1 | `ticket:1` | — |
| KA10 pool leg authorizing pool' (at the pool) | 1 | `pool:PT3` | — |
| KA10 pool leg authorizing pool' (at the ticket) | 0 | `ticket:10` | — |
| KA11 look-alike ticket template at input 0 | 1 | `pool:PT4` | — |
| KA11 another pool at input 0 | 1 | `pool:PT4` | — |
| KA11 real ticket template with role 0x07 at input 0 | 1 | `pool:PT4` | — |
| KA11 round 2 ticket at the round 1 pool | 1 | `pool:PT4` | [check 3: pool round != ticket round] |
| KA12 idx = b | 0 | `ticket:1` | [check 1: idx outside [0, b), check 5: combination not canonical, check 6: tier, check 8: h < 1] |
| KA12 idx = -1 | 0 | `ticket:1` | [check 1: idx outside [0, b), check 5: combination not canonical, check 6: tier, check 8: h < 1] |
| KA12 ticket b = 0 | 0 | `ticket:1` | [check 1: ticket b or payout_class, check 1: idx outside [0, b)] |
| KA12 ticket b = B_MAX + 1 | 0 | `ticket:1` | [check 1: ticket b or payout_class, check 4: index already claimed] |
| KA12 ticket payout_class = 3 | 0 | `ticket:1` | [check 1: ticket b or payout_class] |
| KA12 ticket round = 0 | 0 | `ticket:1` | [check 1: ticket round or deadline, check 3: pool round != ticket round] |
| KA12 pool_pl = -1 | 0 | `ticket:1` | — |
| KA12 pool_sl = SCRIPT_MAX + 1 | 0 | `ticket:1` | — |
| KA12 bonus = pool m + 1 | 0 | `ticket:5` | [check 5: combination not canonical, check 6: tier, check 8: h < 1] |
| KA12 non-canonical main numbers (n1 = n2) | 0 | `ticket:5` | [check 5: combination not canonical, check 6: tier, check 8: h < 1] |
| KA12 pool m = M_CLAIM + 1 | 0 | `ticket:3` | [check 3: pool ranges] |
| KA12 pool k = 0 | 0 | `ticket:3` | [check 3: pool ranges] |
| KA12 pool k = K_MAX + 1 | 0 | `ticket:3` | [check 3: pool ranges] |
| KA12 pool tier 7 | 0 | `ticket:3` | [check 3: pool ranges, check 6: tier] |
| KA12 pool reserve > value | 0 | `ticket:3` | [check 3: pool ranges, check 8: prize > remaining] |
| KA12 prize > remaining (value = reserve + prize - 1) | 0 | `ticket:8` | [check 8: prize > remaining] |
| KA13 lock_time = T + D_RET - 1 | 0 | `ticket:A5b.3` | — |
| KA13 a co-spent covenant input (a second ticket) | 0 | `ticket:A5b.2` | — |
| KA13 payout to another script | 0 | `ticket:A5b.6` | — |
| KA13 payout + 1 | 0 | `ticket:A5b.6` | — |
| KA13 payout - 1 | 0 | `ticket:A5b.6` | — |
| KA13 extra plain output | 0 | `ticket:A5b.5` | — |
| KA13 payout under the covenant ID | 0 | `ticket:A5b.4` | — |
| KA14 lock_time = pool deadline - 1 | 0 | `ticket:A5a.4` | — |
| KA14 a pool of round 2 | 0 | `ticket:A5a.3` | — |
| KA14 pool' value + 1 | 0 | `ticket:A5a.7` | — |
| KA14 pool' value - 1 | 0 | `ticket:A5a.7` | — |
| KA14 pool' slice changed | 0 | `ticket:A5a.7` | — |
| KA14 payout to another script | 0 | `ticket:A5a.8` | — |
| KA14 payout - 1 | 0 | `ticket:A5a.8` | — |
| KA15 ticket: lock_time = creation + D_IDLE - 1 | 0 | `ticket:A6.3` | — |
| KA15 ticket: creation = -1 (unaccepted parent) | 0 | `ticket:A6.3` | — |
| KA15 ticket: value + 1 | 0 | `ticket:A6.6` | — |
| KA15 ticket: value - 1 | 0 | `ticket:A6.6` | — |
| KA15 ticket: a second covenant input | 0 | `ticket:A6.2` | — |
| KA15 ticket: extra covenant output | 0 | `ticket:A6.4` | — |
| KA15 ticket: extra plain output | 0 | `ticket:A6.5` | — |
| KA15 ticket: successor claimed changed | 0 | `ticket:A6.7` | — |
| KA15 pool: lock_time = creation + D_IDLE - 1 | 0 | `pool:A6.3` | — |
| KA15 pool: creation = -1 (unaccepted parent) | 0 | `pool:A6.3` | — |
| KA15 pool: value + 1 | 0 | `pool:A6.6` | — |
| KA15 pool: value - 1 | 0 | `pool:A6.6` | — |
| KA15 pool: a second covenant input | 0 | `pool:A6.2` | — |
| KA15 pool: extra covenant output | 0 | `pool:A6.4` | — |
| KA15 pool: extra plain output | 0 | `pool:A6.5` | — |
| KA15 pool: successor deadline changed | 0 | `pool:A6.7` | — |
| KA15 pool: successor roots changed | 0 | `pool:A6.7` | — |
| KA15 marker: lock_time = creation + D_IDLE - 1 | 0 | `marker:A6.3` | — |
| KA15 marker: creation = -1 (unaccepted parent) | 0 | `marker:A6.3` | — |
| KA15 marker: value + 1 | 0 | `marker:A6.6` | — |
| KA15 marker: value - 1 | 0 | `marker:A6.6` | — |
| KA15 marker: a second covenant input | 0 | `marker:A6.2` | — |
| KA15 marker: extra covenant output | 0 | `marker:A6.4` | — |
| KA15 marker: extra plain output | 0 | `marker:A6.5` | — |
| KA15 marker: successor seed_ref changed | 0 | `marker:A6.7` | — |
| KA15 marker: successor roots changed | 0 | `marker:A6.7` | — |

Not `Atk`s:

| Test | Kind | Expected |
|---|---|---|
| `ka1_double_claim_ledger` | ledger | Real chain (genesis K = 4 → buy → close → draw). A claim of ticket 0 on the round 1 tier 2 pool is applied to the `Ledger` **without running scripts**. The same claim again from the original ticket outpoint (fresh funding) → `LedgerError::Missing { index: 0 }`. **Passes now.** |
| `ka7_absorbed_pool_ledger` | ledger | Real chain. A claim is prepared on a round 1 pool. Then round 2 (buy, close, `draw()` absorbing that pool at its deadline) runs the real M2/M3 scripts. The claim's pool outpoint → `LedgerError::Missing { index: 1 }`. **Passes now.** |
| `ka16_leader_budget_minus_one` | budget | For claim PAY / FINAL / ACCRUE, `return_deposit_after`, `return_deposit_with_pool`, and refresh of the ticket / pool / marker: the base is accepted, then `budget_delta = −1` → input 0 `ExceededCommittedScriptUnits`. |
| `attribution_instrument_self_test` | instrument | See "Attribution contract". **Passes now.** |

### Fixtures (bases)

- `pay()` = `ClaimCase::new(4)`: K = 4, m = 10, b = 2, idx 0 of tier 2 held once, slice 0.2 KAS → PAY. Outputs: pool', ticket', payout, change.
- `fin()` = `pay()` + `make_final()` → FINAL. Outputs: pool', payout, change.
- `accrue()` = fabricated tier 2, slice P_MIN − 1 → ACCRUE. Outputs: pool', ticket', change.
- `ka2_base()`: K = 4, tier 2, slice 0.6 KAS, h = 3.
  - The combination is held in shards 0, 1 and 3; shard 2 is non-empty with a Z_0 leaf at the rank.
  - Shard 1 also holds the same main numbers with another bonus, so its leaf has a nonzero count elsewhere.
  - A decoy rank (another tier 2 combination, same bonus) is held in shards 0 and 2.
  - Result: PAY of 0.2 KAS.
- `claim_with_second(6, other)`: the ticket is [tier 6 combination, `other`], both held in shard 0; KA3 claims idx 1.
- Refresh bases (as KP15): ticket and pool of `ClaimCase::new(4)` and the `CloseCase::new(4)` marker, created at DAA 1,500,000, with lock_time = creation + D_IDLE.
- Returns: `ReturnCase::after()` and `ReturnCase::with_pool()` (stage 2a).
- All of these are fabricated UTXOs under the instance ID (D63). `Case::retarget(t)` sets every pool state's `ticket_tpl = t.ticket.hash`, so stage 4 can re-run each attack under a ticket or pool mutant (both change the ticket hash, D54).

### Attribution contract for stage 3 (`M4_ATTRIB`)

Stage 3 must put a marker line at the start of every check of every M4 entry, inside the entry body and in check order:

```
// ==== check <label>: <text>
```

| File | Entry | Labels |
|---|---|---|
| `ticket.sil` | `claim` | `1` … `15` (A4 checks 1–15) |
| `ticket.sil` | `return_deposit_after` | `A5b.1` … `A5b.6` |
| `ticket.sil` | `return_deposit_with_pool` | `A5a.1` … `A5a.8` |
| `ticket.sil` | `refresh` | `A6.1` … `A6.7` |
| `pool.sil` | `ticket_leg` | `PT1` … `PT4` |
| `pool.sil` | `refresh` | `A6.1` … `A6.7` |
| `marker.sil` | `refresh` | `A6.1` … `A6.7` |

Rules:
- Labels are unique per file. The group of a label is its prefix: "" for bare numbers, `A5b.`, `A5a.`, `A6.`, `PT`. The existing drawer markers and the unmarked `absorb_leg` / `draw_leg` are unaffected.
- A marker may sit inside an open block; the tool closes open braces, as with the drawer.
- Code before a group's first marker is in every truncation. Put check 1's marker as the first line of the entry body.

How it works (`common::m4::first_failing_check_in`):
1. The attack's expected check names the file and the group.
2. For each marker of the group, the source is cut before the *next* marker of that group, with open braces closed; the last marker uses the whole file.
3. Each variant compiles through `Templates::compile_sources` with the committed other sources (cached per file, group and source).
4. The variant is swapped in at the target input: same state bytes and arguments, P2SH rebuilt.
5. Genuine successors are rebuilt under the variant: a refresh's output 0 when it equals the spent script, and a claim's ticket' at output 1 when it is the spent ticket with some `claimed` byte.
6. Only the target input runs. A binary search finds the first rejecting truncation.
7. If an attack edited the successor itself, the label gets " (successor script edited)". Only the whole source can judge such an output, so a label of A6.7 or 15 is still exact. The OK/MISMATCH verdict compares only the label part.
8. Sanity check: the whole source must reject the attack, otherwise the tool panics.

`attribution_instrument_self_test` runs the tool on a synthetic ticket: the committed state layout plus a `refresh()` with markers A6.1, A6.2, A6.3 (inside a braced block) and A6.5. It checks that the tool names A6.2 (second covenant input), A6.3 (creation = −1) and A6.5 (extra plain output), and that the good refresh passes the swapped template with output 0 rebuilt. **It passes now**, so the instrument is proven before stage 3 relies on it. It reads only the ticket header up to `byte[33] payout_key = init_payout_key;`, so it survives stage 3.

How stage 3 confirms it: `M4_ATTRIB=1 … --test m4_attacks | grep "MISMATCH\|NO MARKERS"` should print nothing.

### Decisions taken in Stage 2b

| ID | Decision | Why |
|---|---|---|
| S2b-1 | **A separate M4 strike (`strike4`) and env switches `M4_PREFLIGHT` / `M4_ATTRIB`**, not `common::m2::strike` / `M3_ATTRIB`. | `m2::strike` runs the drawer attribution for every input-0 strike and asserts the drawer state layout, so it would panic on a ticket. The M4 tool needs the case (to rebuild ticket') and per-entry label groups. |
| S2b-2 | `strike4` asserts the attack changed the tx ID, an input entry **or a signature script**. | Kaspa's tx ID excludes signature scripts. The first preflight flagged `KA12 pool_pl = −1` and `KA2 stale leaf at another bonus` as "changed nothing" for that reason; both are real argument-only attacks. |
| S2b-3 | **Extra attacks beyond the A9 rows** (one change each): KA2 "under-reported h with the true leaves" (`lie.h`, expected **12**: h is not a script argument, so only the outputs change); KA12 ticket round 0, payout_class 3, pool_pl −1, pool_sl SCRIPT_MAX + 1, non-canonical main numbers, prize > remaining (check 8); KA13 payout under the covenant ID (A5b.4); KA11 a round 2 ticket at the round 1 pool (PT4); KA8 ACCRUE shape when final; KA5/KA9 on both PAY and FINAL / ACCRUE bases; KA16 for each claim mode and each refresh object. | Fuller coverage of the checks named in A4–A6; each gives stage 4 a mutant target. |
| S2b-4 | **The ledger cases are structural only and pass now.** `ka1_double_claim_ledger` applies its first claim to the `Ledger` without a script run. `ka7_absorbed_pool_ledger` runs real scripts only for genesis, buy, close and draw. | Per the brief ("checked structurally against the UTXO set"). The scripts' acceptance of the same claim shape is KP1–KP17. The reviewer may want `accept_m4` on KA1's first claim added after stage 3. |
| S2b-5 | `Sil::Ticket` added to `common::m2::Sil`. | KA11's look-alike ticket (`lookalike(Sil::Ticket)`, cached in `fake_ticket()`); stage 4's ticket mutants. |
| S2b-6 | KA10 "two tickets, one pool" is struck at ticket A (input 0, check 2) and at ticket B (input 2, check 1). | Input 2 of the good transaction is a funding input, so the "good passes input 2" half is trivially true. **This strike passes already** (bad input 2 = ticket B's `claim`, rejected at dispatch). After stage 3 it must be rejected at `ticket:1`, which the attribution shows. |
| S2b-7 | The KA4 "look-alike ticket from a fake shard under a fresh ID" uses the **genuine** ticket template and state under `OTHER_COV`, with every covenant output bound to `(0, OTHER_COV)`. A look-alike *template* is KA11. | It isolates D61 (the covenant ID check): only PT2 / check 2 can tell it apart. The outputs carry the same ID so that consensus' covenant context is valid and the rejection is the script's (as M3's `other_leader`). |

### Expected checks that differ from, or refine, the A9 "Check" column (open; not decided)

1. **KA5 "payout at the wrong index"**: the row lists 13, 12, 11. A misplaced payout fails elsewhere by construction:
   - PAY with payout and ticket' swapped: output 1 is not authorized, so `OpAuthOutputIdx(0, 1) = 2` → **check 10**;
   - FINAL with payout and change swapped: the count and IDs are fine, out1 is the change value → **check 12**.

   Recorded as 10 and 12; the spec row could list 10.
2. **KA8**: the row lists 10, 11, 12. Mapping used:
   - PAY with prize < P_MIN → **11** (nOut 4 ≠ 2 + ch);
   - ACCRUE with prize ≥ P_MIN → **12** (3 outputs pass check 11 as "PAY without change"; out1 = value + prize fails PAY's out1 = in0.value);
   - non-final shapes when final and FINAL when not final → **10**.
3. **KA12 "reserve > value"** is caught by the **check 3** pool range (reserve ≤ input value), not check 8. Check 8 is covered by the extra "prize > remaining" case.
4. **KA2 under-reported h** (extra) is **check 12**, not 7 (S2b-3). The spec's "under-reported h" under 7 is read as h under-reported through stale or forged leaves, which is covered at 7.
5. **Q1 (stage 2a) is assumed resolved as briefed**: check 11 nOut = nCov + [final ∨ pay] + ch. The FINAL attacks (KA5 extra output → 11; FINAL payout at output 2 → 12) depend on it.
6. **Reference quirk (no script impact)**: for b = B_MAX + 1, `ClaimExpected.rejects` also lists "check 4: index already claimed". `claim_mode` returns `None` for an out-of-range b, and the reference then reads that as "claimed". Cosmetic; check 1 is listed first. Not changed (stage 2a API).

### Open items for stage 3

- Place the markers per the attribution contract. Run `M4_ATTRIB=1` on `m4_attacks` and resolve every `MISMATCH`: either the script's check order differs from A4–A6, or an expectation above is wrong. Record which; do not silently change an expected check.
- KA10 at input 2 (S2b-6) must then be rejected at `ticket:1`.
- The two ledger tests and the instrument self-test do not depend on stage 3.
- Stage 4 (`m4_mutants.rs`): `use common::m4::*`, pick attacks by id from `kaN()`, then `atk.pair(&mutant(Sil::Ticket, &[…]))` etc. `Case::retarget` already handles the ticket_tpl that a ticket or pool mutant changes.

### Test output (stage 2b, debug, from `crates/`)

Full `cargo test --no-fail-fast`:

| Binary | Result |
|---|---|
| harness (unit) | 0 passed |
| covenant_smoke | 3 passed |
| m0_limits | 3 passed |
| m0_mass | 5 passed (129.2 s) |
| m0_merkle | 2 passed |
| m0_shapes | 6 passed |
| lottery (unit) | 26 passed |
| m1_attacks | 25 passed |
| m1_buy | 14 passed |
| m1_measure | 4 passed |
| m1_mutants | 11 passed |
| m2_attacks | 40 passed |
| m2_close | 8 passed |
| m2_game | 2 passed |
| m2_measure | 5 passed |
| m2_mutants | 25 passed |
| m2_params | 17 passed |
| m3_attacks | 22 passed |
| m3_draw | 24 passed |
| m3_game | 13 passed |
| m3_measure | 7 passed |
| m3_mutants | 93 passed |
| **m4_attacks** | **FAILED: 3 passed, 16 failed** (expected at this stage) |
| **m4_claim** | **FAILED: 1 passed, 21 failed** (unchanged from stage 2a, same `EarlyReturn` panics) |
| m4_game | 4 passed |
| doc-tests harness, lottery | 0, 0 |

`m4_attacks` failures (exact panic text, sorted):

```
ka1_double_claim_same_index: BASE REJECTED at target input 0 (KA1 index already claimed (PAY)): EarlyReturn
ka10_shape: BASE REJECTED at target input 0 (KA10 parallel group (a second ticket and pool pair)): EarlyReturn
ka11_pool_leg_input0: BASE REJECTED at target input 1 (KA11 look-alike ticket template at input 0): EarlyReturn
ka12_ranges: BASE REJECTED at target input 0 (KA12 idx = b): EarlyReturn
ka13_return_after: BASE REJECTED at target input 0 (KA13 lock_time = T + D_RET - 1): EarlyReturn
ka14_return_with_pool: BASE REJECTED at target input 0 (KA14 lock_time = pool deadline - 1): EarlyReturn
ka15_refresh: BASE REJECTED at target input 0 (KA15 ticket: lock_time = creation + D_IDLE - 1): EarlyReturn
ka16_leader_budget_minus_one: M4 step `claim (attack base) [claim PAY]`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Err(EarlyReturn), Ok(100001)]
ka2_forged_h: BASE REJECTED at target input 0 (KA2 stale leaf: shard 1 count - 1 at the bonus): EarlyReturn
ka3_wrong_tier: BASE REJECTED at target input 0 (KA3 tier 0 combination against the tier 1 pool): EarlyReturn
ka4_other_round_and_family: BASE REJECTED at target input 0 (KA4 round 2 ticket against a round 1 pool): EarlyReturn
ka5_payout: BASE REJECTED at target input 0 (KA5 PAY payout to another script): EarlyReturn
ka6_pool_successor: BASE REJECTED at target input 0 (KA6 pool' value + 1): EarlyReturn
ka7_next_round_pool: BASE REJECTED at target input 0 (KA7 the next round's pool of the same tier): EarlyReturn
ka8_mode_cheating: BASE REJECTED at target input 0 (KA8 PAY with prize < P_MIN): EarlyReturn
ka9_ticket_successor: BASE REJECTED at target input 0 (KA9 PAY ticket' bit not set): EarlyReturn
test result: FAILED. 3 passed; 16 failed; 0 ignored; 0 measured; 0 filtered out
```

Passing now: `ka1_double_claim_ledger`, `ka7_absorbed_pool_ledger`, `attribution_instrument_self_test`.

`M4_PREFLIGHT=1 cargo test -p lottery --test m4_attacks -- --nocapture --test-threads=1`:

```
test result: ok. 18 passed; 0 failed   (run before the self-test was added; it passes on its own)
PREFLIGHT lines per test: ka1 2, ka2 12, ka3 14, ka4 3, ka5 12, ka6 8, ka7 1, ka8 5, ka9 19,
  ka10 9 (+1 strike that passes outright, S2b-6), ka11 4, ka12 16, ka13 7, ka14 7, ka15 26, ka16 8
153 lines: "bad: EarlyReturn" 153 times; "good: EarlyReturn" 145 times (the 8 KA16 lines have no good half)
```

So every bad transaction was rejected at its target input by a script error (dispatch), none by a covenant-context error. Every good transaction failed only at dispatch (`EarlyReturn` = "no entry with this signature"; `compile.rs:352-391`, as in stage 2a). The per-attack `reference` column above comes from the same run.

`cargo test -p lottery --test m4_attacks attribution`: `test result: ok. 1 passed` (24 s, 3 template compiles).

## Stage 3a: `claim()` and the pool's `ticket_leg` (done, 2026-10-10, branch `m4-claim`, uncommitted)

Status: **green at the claim boundary; Stage 3b not started.**
- Every claim positive passes: KP1–KP6, KP8–KP12, KP17, KP18 (grid) and KP19. KP16's two claim parts pass too.
- `m4_attacks` KA1–KA12 pass, and the three claim cases of KA16 pass.
- `M4_ATTRIB=1` gives **106 / 106 OK**, with no MISMATCH and no NO MARKERS. No remap was needed.
- M0–M3 and `m4_game` pass, after two test fixes (S3-4, S3-5) that the removed placeholders required.
- The remaining failures are all Stage 3b entries, which do not exist yet and are rejected at dispatch (`EarlyReturn`); see the output below.
- **Flag for the orchestrator (D64): `buy()` storage is 95.7 %, which is over 90 %.** This storage figure is unchanged by this stage and was already on record (D15 row). See "Sizes and `buy()` masses" below. Because of this flag I stopped before 3b, as the stop rule asks.

### What was implemented

| File | Change |
|---|---|
| `covenants/ticket.sil` | The M1 `locked()` placeholder is removed. **`claim(int idx, byte[] leaves, byte[] siblings, int pool_pl, int pool_sl)`** implements A4 checks 1–15 in order. Each check starts with a `// ==== check N:` line at entry level, never inside an if or for body. Constants are per A2: K_MAX 16, M_CLAIM 50, B_MAX 4, F_MAX 8, DEPTH 18, PATH 576, COUNT_MAX 32,767, P_MIN 5,000,000, DAA_MAX, TOTAL_MAX, RESERVE_MAX, SCRIPT_MAX. D_RET and D_IDLE come in 3b with their entries. There is a `PoolState` struct (A3.3) and `payoutSpk` (a copy of the drawer's). POOL_TPL is the constructor constant `poolTemplateHash` (the Stage 1 name). |
| `covenants/pool.sil` | The placeholder `claim()` is replaced by **`ticket_leg(int ticket_pl, int ticket_sl)`**, which implements PT1–PT4 with markers and tags. A `TicketState` struct (M1 4.2) is added. The pool reads input 0 under `ticket_tpl` from its own state. |
| `crates/lottery/src/buy.rs` | `Sources::committed` also resizes `pool.sil` with `resize_ticket_arrays` (S3-3); the doc comment is updated. |
| `crates/lottery/src/claim.rs` | New unit test `ticket_hash_depends_on_pool_template` (the Stage 1 open item). Recompiling the committed ticket with `t.pool.hash` reproduces `t.ticket.hash`. Flipping byte 0 or byte 31 of the pool hash changes it. **Passes.** |
| `tests/m1_attacks.rs` (a6), `tests/m2_attacks.rs` (ca10) | The ticket input now uses `lottery::buy::planned_sig_script("locked()", &[])`, a tag that no entry has. This keeps the inserted ticket inert, as the handoff note asked. Both tests still pass, judged at the same input as before (the shard and the controller). |
| `tests/m1_attacks.rs` `a8_wrong_ticket_template` | S3-4. |
| `tests/m3_mutants.rs` `every_m3_tag_is_tested` | S3-5. |
| `tests/m4_claim.rs` `kp12` | S3-6: a fixture bug that was hidden until claims passed. |

The claim code follows the drawer's check-9 approach:
- the leaf is Z_0 if the counts are all zero, else H(0x00 ‖ rank(3) ‖ counts);
- each proof slices its own 576-byte path once;
- a 128-byte zero buffer is built by doubling;
- counts are read unsigned (2 B ‖ 0x00) and must be ≤ COUNT_MAX;
- h is the exact sum at the bonus slot.

Temporaries live in braced blocks (rank, tier, the proof loop, check 11). The compile hit no stack-limit error.

### Mutant tags (for stage 4)

Ticket `claim`:
- K1: `K1_active`, `K1_role`, `K1_round`, `K1_deadline`, `K1_b`, `K1_class`, `K1_value`, `K1_tpl`, `K1_idx`
- K2: `K2_count`, `K2_pool`, `K2_fund`, `K2_zero`
- K3: `K3_role`, `K3_tier`, `K3_slice`, `K3_m`, `K3_k`, `K3_deadline`, `K3_reserve`, `K3_value`, `K3_round`
- K4–K8: `K4_claimed`, `K5_canon`, `K5_bonus`, `K6_tier`, `K7_len`, `K7_root`, `K7_count`, `K8_h`, `K8_remaining`
- K10–K13: `K10_auth`, `K10_idx`, `K10_cov`, `K11_count`, `K11_zero`, `K12_pool`, `K12_pay_ticket`, `K12_pay_payout`, `K12_guard`, `K12_sum`, `K13_spk`

Pool: `PT1`, `PT2`, `PT3`, `PT4_tpl`, `PT4_role`, `PT4_round`.

Some checks have no tag:
- **Check 9** (mode) contains no `require`; it only computes `isFinal`, `pay` and `nCov`. Checks 10–12 enforce the mode.
- **Checks 14 and 15** are template-built `validateOutputState*` calls, as drawer checks 22–25.

**Likely equivalent mutants (stage 4 should confirm, with the source cited):**
- `K1_value` (v0 ≥ 0 and v0 ≤ TOTAL_MAX) and the upper half of `K3_value` (v1 ≤ TOTAL_MAX) are probably unbreakable. Values are u64, and the maximum supply (about 2.87·10¹⁸) is below 2⁶²; cite the rusty-kaspa value bound in `refs/` before claiming this.
- `K3_slice` ≥ 0 is reachable only through fabricated pools (D63).

### Decisions taken in Stage 3a

| ID | Decision | Why |
|---|---|---|
| S3-1 | **Check 11 is nOut = nCov + [final ∨ pay] + ch, ch ∈ {0, 1}**, written as `ch = nOut − nCov − (isFinal ∨ pay)` with 0 ≤ ch ≤ 1. Plain outputs ≥ nCov must have the zero ID. | User decision (Q1); this follows the A4 shape table. |
| S3-2 | **bit = 2^idx and 2^b come from small bounded loops** (`for(bq, 0, idx, B_MAX)`). claimed is read unsigned as `OpBin2Num(claimed.slice(0,1) ‖ 0x00)`, and ticket' gets `claimed \| (bit as byte[1])`. This equals c + bit once check 4 proves the bit clear (A4 check 15). | This avoids runtime array indexing and the sign-bit trap of `(c + bit) as byte[1]` when c ≥ 128. |
| S3-3 | **`pool.sil` goes through `resize_ticket_arrays`** in `Sources::committed`, so its `TicketState` (`byte[24] bundle`, `byte[1] claimed`) follows B_MAX. The ticket's own `B_MAX = 4` constant (A2) is not rewritten, so b > 4 tickets compiled for `Templates::get(5..8)` (m1_measure) compile but cannot claim. Nothing claims at B_MAX ≠ 4. | The pool reads the ticket state at PT4. Without the resize, the pool and the ticket layouts would disagree for B_MAX ≠ 4. |
| S3-4 | **`m1_attacks::a8_wrong_ticket_template`**: the look-alike is now "the real ticket plus a permissive `backdoor()` entry" (the `lookalike_source` pattern of common/m2). The old one inverted `locked()`'s `require(this.activeInputIndex < 0)`, which no longer exists, so the "look-alike" had compiled to the real hash and the test's own `assert_ne!` failed. | The test keeps its intent: same state layout, other code. This does not weaken it, and both strikes still fail at the shard. |
| S3-5 | **`m3_mutants::every_m3_tag_is_tested` skips the `PT*` tags of `pool.sil`** (as it already skips the controller's non-`CL` tags). The count stays 88. | The PT tags are M4's. **Stage 4 must add the same coverage test for the ticket K* tags and the pool PT* tags in `m4_mutants.rs`.** |
| S3-6 | **KP12 fixture fix:** the round-2 buy used `BuyCase`'s default bundle (bonus 4), but the round-2 m from the game-size rule is 2 here. The panic was `smt.rs:159` index 3 of len 2, inside `Chain::buy`, before any script ran. The bundle is now `[3,7,12,19,28]` with bonus 1. | Test-fixture bug, first reached once claims passed. No assertion was changed. KP15 has a similar round-2 `buy_case(3)` with the default bundle at price 1 KAS; check it when 3b makes KP15 reach that line. |
| S3-7 | `isFinal` and `nextClaimed` are used as identifiers, not `final` and `next`. | This avoids possible reserved words; there is no semantic change. |

### Attribution (`M4_ATTRIB=1 cargo test -p lottery --release --test m4_attacks -- --nocapture --test-threads=1`)

106 ATTRIB lines, **106 OK, 0 MISMATCH, 0 NO MARKERS**. Per test: ka1 2, ka2 12, ka3 14, ka4 3, ka5 12, ka6 8, ka7 1, ka8 5, ka9 19, ka10 10, ka11 4, ka12 16.

- Every expectation of the Stage 2b table holds exactly, including the refinements recorded there:
  - KA5 wrong index → 10 / 12;
  - KA8 → 11 / 12 / 10;
  - KA12 reserve > value → 3;
  - under-reported h → 12;
  - KA10 ticket B at input 2 → `ticket:1`.
- 12 KA9 lines report `ticket:15 (successor script edited)`, as the contract expects.
- **Grep tip:** with `--nocapture`, the first ATTRIB line of each test shares a line with libtest's `test <name> ... ` prefix. Use `grep -oE "ATTRIB .*"`, not `grep "^ATTRIB"`; the latter misses 12 lines.

### Sizes and `buy()` masses (release)

**Ticket redeem: 22,674 B** (prefix 1, state 92, suffix 22,581). D53 estimated about 20 kB.

**Pool redeem: 1,934 B** (suffix 1,284), up from 1,358 (+576: `TicketState` + `ticket_leg`). This grows `draw()`'s `pool_prefix`/`pool_suffix` arguments and the absorbed-pool legs, so **M4-e (DP21) must be re-measured in stage 4.**

`m1_measure mc_buy_masses_and_referral_minimum`, K = 16. "Before" is the Stage 1/2 placeholder ticket, measured by temporarily swapping in `git show HEAD:covenants/ticket.sil` plus the `poolTemplateHash` parameter. The file was restored afterwards and checked with `cmp`.

| Shape | Compute before → now (% of 500k) | Transient before → now (% of 1M) | Storage (% of 500k) | Shard units before → now |
|---|---|---|---|---|
| b = 1, no referral, large change | 15,637 → 40,394 (8.1 %) | 50,148 → 140,376 (14.0 %) | 377,721 (75.5 %) | 100,755 → 326,311 |
| b = 1, referral 0.1 KAS, large change | 16,049 → 40,806 (8.2 %) | 50,356 → 140,584 (14.1 %) | **478,296 (95.7 %)** | 101,039 → 326,595 |
| b = 1, referral, change 1 KAS | 16,049 → 40,806 | 50,356 → 140,584 | **468,152 (93.6 %)** | 101,039 → 326,595 |
| b = 4, referral, large change | 18,656 → 43,513 (8.7 %) | 57,584 → 147,812 (14.8 %) | 122,690 (24.5 %) | 186,177 → 411,733 |
| b = 4, m = 100, referral | 19,477 → 44,334 | 60,468 → 150,696 | 122,690 | 199,349 → 424,905 |

The percentages use the limits the earlier handoffs used (compute and storage 500,000, transient 1,000,000).

**D64 assessment (needs the orchestrator or the user):**
- The ticket growth raised `buy()` compute and transient by about 2.5× and 2.8×. Both remain far below 90 %.
- **Storage does not depend on the script:** it is identical before and after this stage. The b = 1 with 0.1 KAS referral buy at d = 0.11 KAS is at 95.7 %. That is already recorded in the DECISIONS D15 deviation row ("at 0.11 such a buy is at 96 % of the storage limit") and in costs.md M-c.
- Read literally, the brief's stop rule ("any mass > 90 % → STOP") is met, so I stopped before 3b. My reading is that the rule targets the template growth and that this is a pre-existing M6 item (final d). The decision is the user's.

Also from `m1_measure` (`ma_mb`): the largest K at 90 % for `close()` at B_MAX 4 stays 18 (the shard script is unchanged; only its TICKET_TEMPLATE_HASH value changed).

### Open questions

1. **D64 / storage:** see above. Is the pre-existing 95.7 % `buy()` storage at d = 0.11 with a 0.1 KAS referral acceptable until M6, or must d rise now?
2. **Equivalent mutants:** see the tags section (`K1_value`, the upper half of `K3_value`).
3. **Spec text:** A4 check 11 should be corrected to "nCov + [final ∨ pay] + ch" (S3-1, user decision). The A9 KA5/KA8/KA12 "Check" columns could list the Stage 2b refinements, all of which held under attribution.

### Remaining work for 3b

- `ticket.sil`:
  - `return_deposit_after()` (A5b.1–6);
  - `return_deposit_with_pool(int,int)` (A5a.1–8);
  - `refresh()` (A6.1–7);
  - constants D_RET 12,096,000 and D_IDLE 864,000;
  - markers per the attribution contract.
- `pool.sil` `refresh()` and `marker.sil` `refresh()` (A6.1–7, with markers). Adding refresh to the marker changes its hash, so re-check `close()` K = 16 (M4-f, 86.5 % transient after Stage 1, about 35k headroom).
- Gate for 3b:
  - KP7, KP13, KP14, KP15 ×4 and KP16's return part;
  - KA13, KA14, KA15 and KA16's 5 return/refresh cases;
  - attribution for the A5a/A5b/A6 groups.
- `ticket_leg` already serves path a: in KP14, input 1 currently passes (`Ok(163036)`), and only the ticket's dispatch fails.

### Test output (Stage 3a)

Full `cargo test --no-fail-fast` (debug, from `crates/`), run before S3-4/S3-5. The two M1/M3 failures it showed were fixed and re-run:

| Binary | Result |
|---|---|
| harness unit / covenant_smoke / m0_limits / m0_mass / m0_merkle / m0_shapes | 0 / 3 / 3 / 5 / 2 / 6 passed |
| lottery (unit) | **27 passed** (+ `ticket_hash_depends_on_pool_template`) |
| m1_attacks | 24 passed, 1 failed (`a8_wrong_ticket_template`, S3-4). **After the fix: `--test m1_attacks a8` 1 passed** |
| m1_buy / m1_measure / m1_mutants | 14 / 4 / 11 passed |
| m2_attacks / m2_close / m2_game / m2_measure / m2_mutants / m2_params | 40 / 8 / 2 / 5 / 25 / 17 passed |
| m3_attacks / m3_draw / m3_game / m3_measure | 22 / 24 / 13 / 7 passed |
| m3_mutants | 92 passed, 1 failed (`every_m3_tag_is_tested`: "pool.sil: tag PT1 has no mutant test", S3-5). **After the fix: 1 passed** |
| m4_game | 4 passed |
| m4_claim | 14 passed, 8 failed (3b only, below) |
| m4_attacks | 15 passed, 4 failed (3b only, below) |
| doc-tests | 0, 0 |

Remaining failures (release re-run after the fixes, exact text, sorted). Every one is dispatch (`EarlyReturn`) of a 3b entry:

```
m4_claim (14 passed; 8 failed: kp7, kp13, kp14, kp15_refresh_then_use, kp15_refresh_{marker,pool,ticket}_standalone, kp16 return part):
M4 step `refresh (marker)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
M4 step `refresh (standalone marker)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
M4 step `refresh (standalone pool)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
M4 step `refresh (standalone ticket)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
M4 step `return_deposit (standalone)`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
M4 step `return_deposit`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
M4 step `return_deposit`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
M4 step `return_deposit`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(163036), Ok(100001)]
m4_attacks (15 passed; 4 failed):
BASE REJECTED at target input 0 (KA13 lock_time = T + D_RET - 1): EarlyReturn
BASE REJECTED at target input 0 (KA14 lock_time = pool deadline - 1): EarlyReturn
BASE REJECTED at target input 0 (KA15 ticket: lock_time = creation + D_IDLE - 1): EarlyReturn
M4 step `return_deposit_after (attack base) [return_deposit_after]`: valid transaction rejected at input 0: EarlyReturn; all inputs: [Err(EarlyReturn), Ok(100001)]
```

Passing in `m4_claim`: kp1, kp2, kp3, kp4, kp5, kp6, kp8, kp9, kp10, kp11, kp12, kp17, kp18, kp19.

Passing in `m4_attacks`: ka1 (both), ka2–ka12, the ka7 ledger test, and the instrument self-test. In ka16, the three claim cases run before the first return case.

Feature gates:
- `--no-default-features --features mainnet` compiles.
- `mainnet,test-seed` fails as required with `error: TEST-ONLY seed cannot be built for mainnet`.
- `cargo build -p tn10` is clean (its `claim` probe is the M0 mass probe and does not use the ticket script).

## Orchestrator: user decisions (2026-10-10)
- U1: A4 check 11 is nOut = nCov + [final ∨ pay] + ch (follow the shape table). Goes to the spec Corrections section and DECISIONS.
- U2: D64 `buy()` storage 95.7 % (b = 1, d = 0.11 KAS, 0.1 KAS referral) is pre-existing (D15 row) and unchanged by M4; accepted until M6. The D64 `buy()` stop rule applies to the masses M4 changes (compute, transient). DECISIONS row needed.

## Stage 3b: `return_deposit_*()` and `refresh()` (done, 2026-10-10, branch `m4-claim`, uncommitted)

Status: **green.**
- `m4_claim` 22/22: KP1–KP19, including KP7, KP13, KP14, the KP15 chain test and the 3 standalone refreshes, and KP16's return part.
- `m4_attacks` 19/19: KA1–KA16.
- `M4_ATTRIB=1`: **146 / 146 OK**, with 0 MISMATCH and 0 NO MARKERS. No remap was needed.
- Full `cargo test --no-fail-fast` (debug) passes for M0–M4.
- Feature gates and build: the seed gate fails as designed, the mainnet-only check compiles, and `cargo build -p tn10` is clean.
- D64 masses are below 90 %, so there is no stop.

### What was implemented

| File | Change |
|---|---|
| `covenants/ticket.sil` | Constants `D_RET = 12,096,000` and `D_IDLE = 864,000` (A2). Three new entries:<br>- **`return_deposit_after()`** = A5b.1–A5b.6.<br>- **`return_deposit_with_pool(int pool_pl, int pool_sl)`** = A5a.1–A5a.8. The pool ranges in A5a.3 are the same code as claim check 3. pool' in A5a.7 uses the same `validateOutputStateWithInputTemplate` as claim check 14, preceded by `out0.value = v1`.<br>- **`refresh()`** = A6.1–A6.7.<br>Each check starts with a `// ==== check <label>:` line at entry level (attribution contract). The header comment lists the four entries. |
| `covenants/pool.sil` | Constants `F_MAX 8`, `D_IDLE` and `DAA_MAX`. **`refresh()`** = A6.1–A6.7; the successor keeps all 13 fields unchanged. `ticket_leg` is unchanged. |
| `covenants/marker.sil` | The same constants and **`refresh()`** = A6.1–A6.7. The successor keeps all 22 fields unchanged, so seed_ref and the funder are unchanged. `draw_leg` is unchanged. |
| `crates/lottery/tests/m3_mutants.rs` | `every_m3_tag_is_tested` also skips the `RF*` tags of `pool.sil` and `marker.sil` (S3b-3). The count stays 88. |

No Rust library code and no M4 test changed. Every 3b test passed on its first run against the Stage 2 expectations.

**`ticket_leg` also serves `return_deposit_with_pool` (A4 note): verified.**
- PT1–PT4 do not depend on which ticket entry leads.
- In KP14 and all seven KA14 good transactions, input 1 runs `ticket_leg` and passes.
- In the bad KA14 transactions, the ticket rejects at the named A5a check.

**Time semantics (verified in `refs/` and cited in the source):**
- `require(tx.daa >= x)` compiles to x, OpDup, 0 ≤ x < LOCK_TIME_THRESHOLD (OpWithin, OpVerify), then OpCheckLockTimeVerify (`refs/silverscript/silverscript-lang/src/compiler/compile/statement.rs:375-388`).
- CLTV requires that tx lock_time ≥ x, that both are in the same domain, and that the input's sequence ≠ MAX (rusty-kaspa v2.1.0 `crypto/txscript/src/opcodes/mod.rs:892-918`).
- LOCK_TIME_THRESHOLD = 500,000,000,000 = DAA_MAX (`crypto/txscript/src/lib.rs:84`).
- `OpTxInputDaaScore` pushes `utxo.block_daa_score as i64` (`opcodes/mod.rs:1124-1130`).
- An unaccepted parent has `UNACCEPTED_DAA_SCORE = u64::MAX` (`consensus/core/src/constants.rs:51`), which is pushed as −1. `creation >= 0` therefore rejects it (KA15 "creation = −1", for all three objects).
- There is no overflow: T < DAA_MAX and creation < DAA_MAX are checked first, so each sum is below 2⁶³. A sum ≥ LOCK_TIME_THRESHOLD fails the OpWithin, which rejects, so that case is safe.

### Mutant tags (for stage 4)

| Entry | Tags |
|---|---|
| ticket `return_deposit_after` | `RB1_active`, `RB1_role`, `RB1_round`, `RB1_deadline`, `RB1_b`, `RB1_class`, `RB1_value`, `RB2_count`, `RB2_fund`, `RB2_zero`, `RB3`, `RB4_auth`, `RB4_cov`, `RB5_count`, `RB5_zero`, `RB6_value`, `RB6_spk` |
| ticket `return_deposit_with_pool` | `RA1_active`, `RA1_role`, `RA1_round`, `RA1_deadline`, `RA1_b`, `RA1_class`, `RA1_value`, `RA1_tpl`, `RA2_count`, `RA2_pool`, `RA2_fund`, `RA2_zero`, `RA3_role`, `RA3_tier`, `RA3_slice`, `RA3_m`, `RA3_k`, `RA3_deadline`, `RA3_reserve`, `RA3_value`, `RA3_round`, `RA4`, `RA5_auth`, `RA5_idx`, `RA5_cov`, `RA6_count`, `RA6_zero`, `RA7_value`, `RA8_value`, `RA8_spk` |
| `refresh` in ticket, pool and marker (the same tags in each file) | `RF1`, `RF2_count`, `RF2_fund`, `RF2_zero`, `RF3_creation`, `RF3_idle`, `RF4_auth`, `RF4_idx`, `RF4_cov`, `RF5_count`, `RF5_zero`, `RF6` |

The state half of A5a.7 and all of A6.7 are template-built `validateOutputState*` calls with no tag, as in claim checks 14 and 15. They need look-alike controls in stage 4.

**Candidates for equivalent or weak mutants (stage 4 should confirm):**
- `RB1_value`, `RA1_value` and the upper half of `RA3_value` test the same u64 argument as `K1_value`.
- `RF4_idx`: with OpAuthOutputCount(0) = 1 and OpCovOutputCount(cov) = 1, can the one authorized output sit at an index ≠ 0 while output 0 is valid? That needs output 0 to be a covenant output that input 0 does not authorize, and A6.2 (no other covenant input) seems to rule that out. It is probably equivalent; prove it in the engine.
- The lower bound of `RB2_fund` and `RF2_fund` (≥ 2 inputs): a 1-input transaction pays no fee. Whether that is reachable in the engine is a stage 4 question.
- The `RF5_zero` loop runs at most once (output 1).

### Sizes (release, B_MAX = 4)

| Object | Before | Now | Δ |
|---|---|---|---|
| ticket redeem | 22,674 (3a: prefix 1, state 92, suffix 22,581) | **25,452** (prefix 1, state 92, suffix 25,359) | +2,778 |
| pool redeem | 1,934 (3a: suffix 1,284) | **2,481** (state 649, suffix 1,831) | +547 |
| marker redeem | 1,399 (Stage 1) | **2,104** (state 715, suffix 1,388) | +705 |

The drawer (42,914), vault (667) and controller (27,694) are unchanged.

### D64 masses (release `m3_measure`, `m2_measure`, `m1_measure`)

| Check | Stage 1 / 3a (c / t / s) | Now (c / t / s) | % c / t / s | ≤ 90 % |
|---|---|---|---|---|
| `close()` K = 16 (CP11 / M3-f) | 293,914 / 864,896 / 402,301 | **294,619 / 867,716 / 402,301** | 58.9 / **86.8** / 80.5 | yes (transient headroom 32,284) |
| DP21 np 5, 1 funding | 135,573 / 372,132 / 59,173 | 144,016 / 401,904 / 59,173 | 28.8 / 40.2 / 11.8 | yes |
| DP21 np 7, F_MAX (worst) | 144,627 / 376,188 / 59,175 | **153,270 / 405,960 / 59,175** | 30.7 / 40.6 / 11.8 | yes |
| `buy()` b = 1, referral, large change | 40,806 / 140,584 / 478,296 | 43,884 / 151,696 / 478,296 | 8.8 / 15.2 / 95.7 | compute and transient yes; storage is unchanged (U2) |
| `buy()` b = 4, referral | 43,513 / 147,812 / 122,690 | 46,491 / 158,924 / 122,690 | 9.3 / 15.9 / 24.5 | yes |

Other measurements:
- The M2-c storage worst case is unchanged at 449,692 (89.9 %).
- The largest K at 90 % for `close()` stays 16 on all three masses (m2b), and 18 at B_MAX 4 (m1 `ma_mb`).
- Delegate units: the marker's `draw_leg` is 305,036 and the pool's (max) is 305,809.
- **Watch item:** `close()` K = 16 transient is now 86.8 %. Every further marker byte costs about 4 grams at K = 16.

### Decisions taken in Stage 3b

| ID | Decision | Why |
|---|---|---|
| S3b-1 | **Refresh tags use the same `RF*` IDs in all three files.** Returns use `RB*` (path b) and `RA*` (path a). | Tags are per file, so the same IDs mean the same A6 check in each. The prefixes collide with no existing tag (K, PT, PL, ML, D, C, CL). |
| S3b-2 | **A5a.7 checks `out0.value = v1` before `validateOutputStateWithInputTemplate`.** Both sit under the A5a.7 marker. **A6.5 checks the zero ID of output 1 in a loop of at most 1 iteration.** | This follows the spec text order. Value attacks and state attacks both attribute to A5a.7, as Stage 2b expected. |
| S3b-3 | **`m3_mutants::every_m3_tag_is_tested` also skips `RF*` in `pool.sil` and `marker.sil`**, as S3-5 did for PT*. | These are M4 tags, and their coverage belongs in `m4_mutants.rs` (stage 4). The M3 count (88) is unchanged, and no M3 tag lost coverage. |
| S3b-4 | **Both return entries re-check every A3 ticket range for the ticket's own state; path a also re-checks the full A3 pool ranges.** The code is inline, duplicated from claim, not a shared function. | The spec says "own ranges" and "plus the A3 pool ranges". Inline code keeps one `require` per tagged line for the mutant tool. Its cost is part of the +2,778 B ticket growth, which `buy()` pays in transient mass (about +11k). |

### Attribution (`M4_ATTRIB=1 cargo test -p lottery --release --test m4_attacks -- --nocapture --test-threads=1`)

146 ATTRIB lines (`grep -oE "ATTRIB .*"`): **146 OK, 0 MISMATCH, 0 NO MARKERS** (73 s).

| Test | Lines |
|---|---|
| ka1 | 2 |
| ka2 | 12 |
| ka3 | 14 |
| ka4 | 3 |
| ka5 | 12 |
| ka6 | 8 |
| ka7 | 1 |
| ka8 | 5 |
| ka9 | 19 |
| ka10 | 10 |
| ka11 | 4 |
| ka12 | 16 |
| **ka13** | **7** |
| **ka14** | **7** |
| **ka15** | **26** |

The 3b lines match the Stage 2b table exactly:
- **KA13:** A5b.3, A5b.2, A5b.6 ×3, A5b.5, A5b.4.
- **KA14:** A5a.4, A5a.3, A5a.7 ×3, A5a.8 ×2.
- **KA15**, for each of the ticket, pool and marker: A6.3 (lock_time − 1), A6.3 (creation = −1), A6.6 ×2, A6.2, A6.4, A6.5, and A6.7 "(successor script edited)". The A6.7 edits are the ticket's claimed, the pool's deadline and roots, and the marker's seed_ref and roots.

### Open questions and notes for the reviewer

1. **Third-party returns forfeit unclaimed prizes.** This is by design (D58); it is restated here, not decided. Both return paths accept any submitter, because the payout always goes to the ticket's script. Both consume the ticket and forfeit unclaimed indices.
   - Path a (after the pool deadline): claims are possible only until absorption anyway.
   - Path b (after T + D_RET): a griefer can return a winning ticket before its owner claims, but only in the parked missed-draw case that D58 names.

   This is worth a line in the stage 5 brief.
2. **A refresh moves the creation DAA** (Stage 2a note 2 still stands). A third party can refresh a marker before `draw()`. That raises the round's own_daa and so D_claim, by at most "now". KP15 exercises it, and no deadline stored in state moves.
3. **`close()` transient is at 86.8 %**, the tightest D64 margin. There is no stop.
4. **Spec text corrections are still pending from 3a:** A4 check 11 (U1) and the refinements to the A9 Check column.

### Work list for Stage 4

- **`m4_mutants.rs`:**
  - Test every tag above plus 3a's K*/PT* tags. Pick attacks from `common::m4::kaN()` by id, then call `atk.pair(&mutant(Sil::…, &[tag]))`.
  - Add a coverage test like `every_m3_tag_is_tested` for all of `ticket.sil`, the PT* and RF* tags of `pool.sil`, and the RF* tags of `marker.sil`.
  - Add look-alike negative controls for claim checks 14/15, A5a.7 and A6.7 (template bindings).
  - Confirm or refute each equivalent-mutant candidate (K1_value, RB1/RA1_value, RF4_idx, the RB2/RF2_fund lower bound), with the source cited.
  - Check that `common::m2::Sil` has a marker variant usable by `mutant()`.
- **Fabricated-range attacks for the return entries:** most single-tag removals in A5b.1/A5a.1 (role, round, b, class) and A5a.3 (pool ranges) have no dedicated attack in KA13/KA14. Add one per tag, as KA12 does for claim, using `ReturnCase` edits (D63).
- **`m4_measure.rs`:** M4-a…M4-h. The numbers above are a quick start only. M4-g needs the return and refresh masses, and M4-c needs P_MIN. Record the results in `docs/costs.md`, including the ticket 25,452 / pool 2,481 / marker 2,104 sizes and `close()` K = 16 at 86.8 % transient.
- **`crates/tn10`:** `probe claim-m4` (genesis → buy → close → draw → claim, chain-merged inputs per S5-5). Run it only with the user's go-ahead.
- **CLAUDE.md commands:**
  - the M4 test line: `cargo test -p lottery --test m4_game --test m4_claim --test m4_attacks --test m4_mutants`;
  - the measure line: `cargo test -p lottery --release --test m4_measure -- --nocapture --test-threads=1`;
  - the attribution line: `M4_ATTRIB=1 cargo test -p lottery --release --test m4_attacks -- --nocapture --test-threads=1` (lines `ATTRIB`; use `grep -oE "ATTRIB .*"`);
  - the probe name.
- **Stage 5 DECISIONS:** S3b-1…S3b-4, together with the S3-*, S2*, S1-* rows, U1 and U2.

### Test output (Stage 3b)

Release, targeted:

```
cargo test -p lottery --release --test m4_claim
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.06s
cargo test -p lottery --release --test m4_attacks
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.31s
M4_ATTRIB=1 cargo test -p lottery --release --test m4_attacks -- --nocapture --test-threads=1
146 ATTRIB lines, 146 "| OK"; grep "MISMATCH\|NO MARKERS" prints nothing
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 73.05s
```

Full `cargo test --no-fail-fast` (debug, from `crates/`, exit 0):

| Binary | Result |
|---|---|
| harness unit / covenant_smoke / m0_limits / m0_mass / m0_merkle / m0_shapes | 0 / 3 / 3 / 5 (128.6 s) / 2 / 6 passed |
| lottery (unit) | 27 passed |
| m1_attacks / m1_buy / m1_measure / m1_mutants | 25 / 14 / 4 / 11 passed |
| m2_attacks / m2_close / m2_game / m2_measure / m2_mutants / m2_params | 40 / 8 / 2 / 5 / 25 / 17 passed |
| m3_attacks / m3_draw / m3_game / m3_measure / m3_mutants | 22 / 24 / 13 / 7 / 93 passed |
| **m4_attacks / m4_claim / m4_game** | **19 / 22 / 4 passed** |
| doc-tests harness, lottery | 0, 0 |

The only warnings are the 2 pre-existing unused variables at `harness/tests/m0_mass.rs:459`.

Feature gates:
- `cargo check -p lottery --no-default-features --features mainnet,test-seed` fails as required with `error: TEST-ONLY seed cannot be built for mainnet`.
- `cargo check -p lottery --no-default-features --features mainnet` compiles.
- `cargo build -p tn10` is clean.

## Stage 4: mutants, measurements, tn10 probe (2026-10-11, branch `m4-claim`, uncommitted)

Status: written in two passes (the session was interrupted once). This first pass records the work and the results already run; the "Gate" subsection at the end holds the full re-run.

### Files

| File | Change |
|---|---|
| `crates/lottery/tests/common/m4.rs` | Stage 4 edit helpers (`rogue_copy`, `genesis_output`, `input_role`, `pool_role`, `class3_payout`, `no_funding`, `nine_funding`, `script_rank`, `noncanonical`, `m_base`/`set_m`, `with_pool_edit`); about 125 new attacks appended to KA10-KA15 (one change each, fabricated per D63); `engine_only()` (value bounds above 2^62, S4-3); `all_atks()` and `atk(id)` (lookup by exact id); `with_template` tolerates a refresh with no outputs. |
| `crates/lottery/tests/m4_attacks.rs` | `engine_only_value_bounds`, `attack_ids_are_unique`. |
| `crates/lottery/tests/m4_mutants.rs` (new) | Line-level mutants for every M4 tag, look-alike template controls, coverage test, ignored diagnostic `redundancy_attribution`. |
| `crates/lottery/tests/m4_measure.rs` (new) | M4-a ... M4-h. |
| `crates/tn10/src/cases.rs`, `main.rs` | `draw_m3` body moved into `round1_drawn(ctx, tag) -> Drawn` (draw-m3 behaviour unchanged; a rejected draw is still a report row); new `claim_m4`; `probe claim-m4` wired (also part of `all`). |
| `CLAUDE.md` | M4 test / measure / `M4_ATTRIB` line; `claim-m4` in the probe list. Nothing else changed. |

No covenant source and no library code (`crates/lottery/src`) changed in Stage 4.

### Scope item 1: missing attacks (A5b.1, A5a.1, A5a.3, and mutant targets)

New attacks, all struck in `m4_attacks` with attribution checked:
- KA13, A5b.1: role 0x07, round 0, round TOTAL_MAX, deadline -1, deadline DAA_MAX, b = 0, b = B_MAX + 1, payout_class 3 paid to its P2SH form, a covenant-less copy of the ticket at input 1. A5b.2: a covenant-less ticket, no funding, nine funding inputs, a funding input with a covenant ID. A5b.5: the change under a fresh genesis covenant ID; no output.
- KA14, A5a.1: the same own-state set (round edits also move the pool's round), a covenant-less copy at input 2, pool_pl/pool_sl -1 and SCRIPT_MAX + 1. A5a.2: the pool under another covenant ID, no / nine funding, funding with a covenant ID. A5a.3: pool role 0x07 (pool' follows), tier 7, slice -1 / TOTAL_MAX + 1, m 1 / 51, k 0 / 17, deadline -1 / DAA_MAX, reserve -1 / RESERVE_MAX + 1, reserve > value. A5a.5: pool' without covenant binding. A5a.6: only pool', an extra plain output, the change under a genesis ID.
- KA15, per object: a covenant-less copy at input 1 (A6.1); a covenant-less object, no / nine funding, funding with a covenant ID (A6.2); creation = DAA_MAX (A6.3); successor without covenant binding, no output (A6.4); the change under a genesis ID (A6.5).
- Claim and pool leg: covenant-less ticket copy at input 2 (`ticket:1`); pool under another covenant ID, covenant-less ticket and pool, no / nine funding (`ticket:2`); pool' and ticket' unbound (`ticket:10`); PAY without payout and change, change under a genesis ID (`ticket:11`); covenant-less pool copy at input 2 (`pool:PT1`); ticket_pl/sl -1 and SCRIPT_MAX + 1 (`pool:PT4`). KA12: ticket role 0x07 (FINAL), ticket and pool round 0 / TOTAL_MAX, ticket deadline -1 / DAA_MAX, class 3, idx = b with a winning combination in the bundle padding, pool_pl SCRIPT_MAX + 1, pool_sl -1 (`ticket:1`); pool role 0x07, a non-winning combination against a tier 7 pool, slice -1, slice TOTAL_MAX + 2 with h = 2, m 1 and 51 (matching trees), deadline -1 / DAA_MAX, reserve -1 / RESERVE_MAX + 1, k = 0 with no proofs (`ticket:3`); n1 = 0, n1 = n2, n2 = n3, n3 = n4, n4 = n5 (a repeated number aliases another rank's leaf and scores as a jackpot), n5 = 31, bonus 0 and m + 1 against the tier 3 pool (`ticket:5`); a count of 32,768 at the bonus (`ticket:7`); a combination no shard holds, h = 0 (`ticket:8`).
- `engine_only()`: ticket / pool value TOTAL_MAX + 1 (claim, both returns), ticket value TOTAL_MAX - 10 with an ACCRUE prize, ticket value 2^63.

### Scope item 2: `m4_mutants.rs`

Mechanism: a tag is `"file:TAG"` (all its lines) or `"file:TAG#n"` (the n-th line carrying TAG, 0-based in file order), so each half of a range pair is mutated alone. `killed`: strike with the committed templates, base accepted under the mutant, the mutant's named input accepts the attack. `redundant`: base accepted, the mutant still rejects (not on the budget). Groups whose members are each redundant are also removed together and killed (as M3 D17).

Coverage: 131 M4 tags on 187 tagged lines (ticket.sil all tags; pool.sil PT*, RF*; marker.sil RF*). `every_m4_tag_is_tested` checks that every line is named and pins (131, 187). 211 tests: 129 killed (102 direct + 9 x 3 refresh), 75 redundant (48 + 9 x 3), 6 template controls, 1 coverage. The group kills are `K2_count+K2_pool`, `K10_auth+idx+cov`, `RB4_auth+RB4_cov+RB5_zero`, `RA2_count+RA2_pool`, `RA5_auth+idx+cov`, and per object `RF2_count+RF2_zero` and `RF4_auth+idx+cov`.

Redundant (equivalent) lines with proof. The last column (first rejecting check under the mutant) comes from the ignored diagnostic `redundancy_attribution` (679 s), which runs the M4 attribution instrument on the mutant source.

| Line(s) | Why the mutant still rejects | First rejecting check |
|---|---|---|
| `K1_b#0` (b >= 1) | 0 <= idx < b implies b >= 1 (K1_idx) | ticket:1 |
| `K1_b#1` (b <= B_MAX) | check 9 `for(fq, 0, b, B_MAX)` carries the compiler's require(end - start <= max) (silverscript-lang/src/compiler/for.rs:135-155) | ticket:9 |
| `K1_value#0`, `RB1_value#0`, `RA1_value#0` (v0 >= 0) | unbreakable: OpTxInputAmount converts u64 to i64 and fails NumberTooBig above i64::MAX (rusty-kaspa v2.1.0 crypto/txscript/src/opcodes/mod.rs:1100-1107); run with value 2^63 | ticket:1 / A5b.1 / A5a.1 |
| `K1_tpl#0-3`, `RA1_tpl#0-3`, `PT4_tpl#0-3` (template lengths) | the template hash is blake3(len, prefix, len, suffix) (silverscript compiler/compile/state.rs:140-167): only the true lengths read the input; out-of-range ones hit a script-sig substring error | ticket:3 / A5a.3 / PT4 |
| `K1_idx#0` (idx >= 0) | co = -6: OpSubstr rejects a negative index (opcodes/mod.rs:502-507, 205-215) | ticket:5 |
| `K2_count`, `K2_pool`, `RA2_count`, `RA2_pool` | pigeonhole with K2_zero / RA2_zero (input 0 carries cov, inputs >= 2 the zero ID); a covenant-less input 0 authorizes no output (crypto/txscript/src/covenants.rs:124-131), so check 10 / A5a.5 rejects | ticket:2 or 10 / A5a.2 |
| `K2_fund#1`, `RA2_fund#1`, `RB2_fund#1`, `RF2_fund#1` (<= F_MAX funding) | the zero-ID loop's F_MAX bound | check 2 / A5a.2 / A5b.2 / A6.2 |
| `K3_k#0` (k >= 1) | k = 0 with no proofs gives h = 0, K8_h | ticket:8 |
| `K3_k#1` (k <= K_MAX) | the check 7 loop bound `for(i, 0, pl.k, K_MAX)` | ticket:7 |
| `K3_value#0` (reserve <= v1) | K8: 0 <= prize <= v1 - reserve | ticket:8 |
| `K5_bonus#0`, `#1` | `counts.slice(2(bonus - 1), 2 bonus)`: InvalidIndex(-2) / OutOfBoundsSubstring(20, 22, 20) | ticket:7 |
| `K8_h` (h >= 1) | slice / 0 fails in OpDiv (opcodes/mod.rs:656-661) | ticket:8 |
| `K10_auth`, `K10_idx`, `K10_cov` | each implied by the other two plus K11_zero (input 0's authorized outputs are continuation outputs under cov; OpAuthOutputIdx fails past the count) | ticket:10 |
| `K11_count#0` (ch >= 0) | check 12 reads a missing output (InvalidOutputIndex) | ticket:12 |
| `RB1_deadline#1`, `RA3_deadline#0`, `#1`, `RF3_creation#1` | `require(tx.daa >= x)` compiles to 0 <= x < LOCK_TIME_THRESHOLD = DAA_MAX before CLTV (silverscript compile/statement.rs:375-388; rusty-kaspa crypto/txscript/src/lib.rs:84); a creation score >= 2^63 is pushed negative (opcodes/mod.rs:1124-1130) and fails RF3_creation#0 | A5b.3 / A5a.4 / A6.3 |
| `RB4_auth`, `RB4_cov` | imply each other (input 0 is the only cov input), and RB5_zero implies both | A5b.4 |
| `RB5_count#0`, `RA6_count#0` | A5b.6 / A5a.8 read the missing output | A5b.6 / A5a.8 |
| `RB5_count#1`, `RA6_count#1`, `RF5_count#1` | the zero-ID loop bound (2, 2 and 1 iterations) | A5b.5 / A5a.6 / A6.5 |
| `RA5_auth`, `RA5_idx`, `RA5_cov` | each implied by the other two plus RA6_zero | A5a.5 |
| `RF2_count` (x 3) | pigeonhole with RF2_zero; a covenant-less object fails A6.4 | A6.2 / A6.4 |
| `RF4_auth`, `RF4_idx`, `RF4_cov` (x 3) | each implied by the other two plus RF5_zero (the 3b suspicion on RF4_idx is confirmed) | A6.4 |
| `RF5_count#0` (x 3) | A6.4 (one authorized output) | A6.4 |

The funding lower bounds (3b question) are NOT equivalent: `K2_fund#0`, `RB2_fund#0`, `RA2_fund#0` and `RF2_fund#0` are killed by a transaction with no funding input (fee 0). Consensus only requires outputs <= inputs, so a zero-fee transaction is consensus-valid; a miner can include one.

Consensus-equivalent, engine-killed (S4-3): `K1_value#1`, `K3_value#1`, `K12_guard`, `RB1_value#1`, `RA1_value#1` and `RA3_value#1` need a value or a sum above 2^62. Consensus caps every output and every input total at MAX_SOMPI = 2.9 x 10^18 < 2^62 (consensus/core/src/constants.rs:34; tx_validation_in_isolation.rs:154-166; tx_validation_in_utxo_context.rs:101-110), so no on-chain UTXO reaches them. The engine has no such rule, so the mutants are killed there; this shows the requires execute.

Template-binding controls (M2 test-method row), all passing:
- `template_claim_reads_pool_under_pool_tpl` (checks 3 and 14): a ticket compiled with a look-alike pool's hash as POOL_TPL accepts that pool; the committed ticket rejects the same pool.
- `template_claim_pool_successor_under_input_template` (check 14, output half).
- `template_claim_ticket_successor_is_own_template` (check 15).
- `template_return_reads_pool_under_pool_tpl` (A5a.3 and A5a.7).
- `template_refresh_successor_is_own_template` (A6.7 for the ticket, pool and marker).
- `template_pool_leg_reads_ticket_under_ticket_tpl` (PT4).

### Scope item 3: `m4_measure.rs` (release; tables ready for docs/costs.md)

M4-a, redeem sizes (B_MAX = 4):

| object | prefix B | suffix B | state B | redeem B |
|---|---|---|---|---|
| ticket (claim, return x2, refresh) | 1 | 25359 | 92 | 25452 |
| pool (absorb_leg, ticket_leg, refresh) | 1 | 1831 | 649 | 2481 |
| marker (draw_leg, refresh) | 1 | 1388 | 715 | 2104 |
| drawer (draw_test_seed) | 1 | 42776 | 137 | 42914 |
| vault (draw_leg) | 1 | 610 | 56 | 667 |
| controller (close, draw_leg) | 1 | 27266 | 427 | 27694 |
| shard (buy, close_leg) | 1 | 11244 | 116 | 11361 |

Ticket redeem B by K_MAX (rows) and M_CLAIM (columns):

| K_MAX | M_CLAIM 10 | 25 | 50 |
|---|---|---|---|
| 4 | 10834 | 10836 | 10836 |
| 8 | 15706 | 15708 | 15708 |
| 16 | 25450 | 25452 | 25452 |

The ticket grows about 1.22 kB per unit of K_MAX (the proof loop is unrolled K_MAX times); M_CLAIM barely matters.

M4-b, claim() per-input units and masses (worst proofs: h = K, every leaf non-empty; one funding input):

| mode | K | m | proof B k(576+2m) | ticket units / sigscript B | pool leg units | compute | transient | storage | priority | % c / t / s |
|---|---|---|---|---|---|---|---|---|---|---|
| Pay | 4 | 10 | 2384 | 288826 / 27854 | 183627 | 38007 | 123788 | 277425 | 277425 | 7.6 / 12.4 / 55.5 |
| Pay | 4 | 50 | 2704 | 293399 / 28175 | 183627 | 38428 | 125072 | 277425 | 277425 | 7.7 / 12.5 / 55.5 |
| Pay | 16 | 10 | 9536 | 667302 / 35007 | 183651 | 48960 | 152400 | 275149 | 275149 | 9.8 / 15.2 / 55.0 |
| Pay | 16 | 50 | 10816 | 700682 / 36287 | 183651 | 50640 | 157520 | 275149 | 275149 | 10.1 / 15.8 / 55.0 |
| Accrue | 4 | 10 | 2384 | 288322 / 27854 | 183627 | 37595 | 123580 | 178337 | 178337 | 7.5 / 12.4 / 35.7 |
| Accrue | 4 | 50 | 2704 | 292895 / 28175 | 183627 | 38016 | 124864 | 178337 | 178337 | 7.6 / 12.5 / 35.7 |
| Accrue | 16 | 10 | 9536 | 666798 / 35007 | 183651 | 48548 | 152192 | 178337 | 178337 | 9.7 / 15.2 / 35.7 |
| Accrue | 16 | 50 | 10816 | 700178 / 36287 | 183651 | 50228 | 157312 | 178337 | 178337 | 10.0 / 15.7 / 35.7 |
| Final | 4 | 10 | 2384 | 186107 / 27854 | 183627 | 36550 | 123440 | 26857 | 61720 | 7.3 / 12.3 / 5.4 |
| Final | 4 | 50 | 2704 | 190680 / 28175 | 183627 | 36971 | 124724 | 26857 | 62362 | 7.4 / 12.5 / 5.4 |
| Final | 16 | 10 | 9536 | 564580 / 35007 | 183651 | 47503 | 152052 | 24581 | 76026 | 9.5 / 15.2 / 4.9 |
| Final | 16 | 50 | 10816 | 597960 / 36287 | 183651 | 49083 | 157172 | 24581 | 78586 | 9.8 / 15.7 / 4.9 |

K = 16, m = 50, PAY with F_MAX = 8 funding inputs: ticket 701,172 units, pool 183,651; compute 58,480, transient 160,880, storage 177,525 (11.7 / 16.1 / 35.5 %).

M4-c, P_MIN (worst PAY: b = 2, d = 0.11 KAS, ticket' 0.22 KAS, pool' at R = 1.69 KAS, one funding input), storage by payout P:

| change | P = 0.03 KAS | 0.04 | 0.0426 | 0.05 (P_MIN) | 0.1 | smallest P <= 90 % (sompi) | rounded up to 0.01 KAS |
|---|---|---|---|---|---|---|---|
| 1 KAS | 466584 | 383521 | 368332 | 333791 | 235101 | 3157513 (0.031575 KAS) | 0.04 KAS |
| 1,000 KAS | 538584 | 455251 | 439992 | 405251 | 305251 | 4085802 (0.040858 KAS) | 0.05 KAS |
| 1,000,000 KAS | 538819 | 455486 | 440227 | 405486 | 305486 | 4089729 (0.040897 KAS) | 0.05 KAS |

The worst case is a large change (the KIP-9 input relief shrinks as the input total grows; the 1,000,000 KAS row is the supremum within about 0.1 %), not the 1 KAS change. **Measured P_MIN = 0.05 KAS = 5,000,000 sompi: D57 is confirmed; the constant is unchanged.** At P_MIN the worst PAY storage is 405,486 (81.1 %). The spec's 0.0426 KAS estimate is 0.0409 KAS measured; both round up to 0.05.

M4-d, buy() with the M4 ticket template (ticket redeem 25,452 B; d = 0.11 KAS, price 1 KAS, m = 10):

| shape | shard units | shard sigscript B | compute | transient | storage | % c / t / s |
|---|---|---|---|---|---|---|
| b = 1, referral none, change large | 354091 | 37378 | 43472 | 151488 | 377721 | 8.7 / 15.1 / 75.5 |
| b = 1, referral 0.1 x b KAS, change large | 354375 | 37378 | 43884 | 151696 | 478296 | 8.8 / 15.2 / 95.7 |
| b = 1, referral 0.1 x b KAS, change 1 KAS | 354375 | 37378 | 43884 | 151696 | 468152 | 8.8 / 15.2 / 93.6 |
| b = 4, referral 0.1 x b KAS, change large | 439513 | 39185 | 46491 | 158924 | 122690 | 9.3 / 15.9 / 24.5 |
| b = 4, referral 0.1 x b KAS, change 1 KAS | 439513 | 39185 | 46491 | 158924 | 121677 | 9.3 / 15.9 / 24.3 |

D64: compute and transient <= 90 % (asserted). Storage of 95.7 % / 93.6 % for b = 1 with a referral is the pre-existing D15 figure, accepted until M6 (U2; asserted <= 100 % only).

M4-e, DP21 with the M4 pool (2,481 B) and marker; K = 16, m = 50, a = A_MAX = 5:

| np | funding inputs | drawer units | pool leg units (max) | compute | transient | storage | % c / t / s | <= 90 % |
|---|---|---|---|---|---|---|---|---|
| 5 | 1 | 1512040 | 305809 | 144016 | 401904 | 59173 | 28.8 / 40.2 / 11.8 | true |
| 5 | 8 | 1512628 | 305809 | 151856 | 405264 | 59173 | 30.4 / 40.5 / 11.8 | true |
| 7 | 1 | 1565224 | 305809 | 145430 | 402600 | 59175 | 29.1 / 40.3 / 11.8 | true |
| 7 | 8 | 1565812 | 305809 | 153270 | 405960 | 59175 | 30.7 / 40.6 / 11.8 | true |

A_MAX = 5 stands (D44).

M4-f, close() with the M4 marker (2,104 B):

| K | controller units | compute | transient | storage | % c / t / s | <= 90 % |
|---|---|---|---|---|---|---|
| 4 | 1037088 | 103411 | 315044 | 118411 | 20.7 / 31.5 / 23.7 | true |
| 16 | 3379799 | 294619 | 867716 | 402301 | 58.9 / 86.8 / 80.5 | true |

Transient headroom below 90 % at K = 16: 32,284 grams. This is the tightest D64 margin (about 4 grams per marker byte).

M4-g, return_deposit() and refresh() (one funding input, change):

| transaction | leader units / sigscript B | pool leg units | compute | transient | storage | % c / t / s |
|---|---|---|---|---|---|---|
| return_deposit_after (b = 2 ticket) | 51819 / 25460 | - | 28052 | 103328 | 37659 | 5.6 / 10.3 / 7.5 |
| return_deposit_with_pool (b = 2 ticket, K = 1 pool) | 96636 / 25464 | 183576 | 33260 | 113880 | 41239 | 6.7 / 11.4 / 8.2 |
| refresh ticket | 153991 / 25460 | - | 29097 | 103468 | 174023 | 5.8 / 10.3 / 34.8 |
| refresh pool (K = 4) | 22647 / 2489 | - | 4826 | 11584 | 7869 | 1.0 / 1.2 / 1.6 |
| refresh marker (K = 4) | 20046 / 2112 | - | 4449 | 10076 | 310 | 0.9 / 1.0 / 0.1 |
| refresh marker (K = 16) | 20046 / 2112 | - | 4449 | 10076 | 310 | 0.9 / 1.0 / 0.1 |

M4-h, claim() priority fee (minimum feerate 100 sompi/gram x priority mass, `harness::priority_fee`; informational):

| shape | priority mass | priority fee (KAS) | % of P_MIN | % of 0.1 KAS | % of 1 KAS |
|---|---|---|---|---|---|
| PAY, K = 4, m = 10 | 277425 | 0.27742 | 554.9 | 277.4 | 27.74 |
| PAY, K = 16, m = 50 | 275149 | 0.27515 | 550.3 | 275.1 | 27.51 |
| ACCRUE, K = 16, m = 50 | 178337 | 0.17834 | 356.7 | 178.3 | 17.83 |
| FINAL, K = 16, m = 50 | 78586 | 0.07859 | 157.2 | 78.6 | 7.86 |

Worst PAY at P = P_MIN: storage 405,486; if storage is the priority mass, the priority fee is about 0.405 KAS (8 x P_MIN). Finding: at the priority floor (full blocks), a PAY claim of a small prize costs several times the prize, because storage mass (C / payout, C / ticket') dominates. The relay floor ignores storage mass, so the fee is far lower when blocks are not full. Informational; see open question 2.

### Scope item 4: tn10 `probe claim-m4` (built, NOT run)

Command (from `crates/`): `cargo run -p tn10 -- probe claim-m4`. Expected behaviour:
1. The draw-m3 sequence, with rows labelled `setup: claim-m4 ...`: genesis with K = 16, J(1) = 10 KAS, V0 = 60 KAS, T = virtual DAA + 600; a buy of the round 1 jackpot combination of the TEST-ONLY seed [0x5e; 32] on a b = 1 ticket in shard 0; wait for T; close(); wait for seed_ref; draw_test_seed(). Every step is checked in the local engine first.
2. The claim. Inputs: the ticket (buy output 1), the jackpot pool (its draw output; the state is rebuilt from the draw bytes by `RoundResult::pool_state` and asserted equal to the output script), and the draw's caller output as funding. Each entry is the node's chain-merged entry (`merged`, S5-5), re-read just before submission and rebuilt up to 3 times if one moved. The reference must give no rejects and mode FINAL (b = 1, h_J = 1). Outputs: pool' = R, payout = J + d to our key, change. lock_time 0.
3. One report row `claim() M4 real scripts, FINAL on the jackpot pool: K = 16, m = ..., h = 1, prize ..., payout ..., pool' ... (R)`; results go to `crates/tn10/results-claim-m4.md` (copy into docs/tn10-mass-check.md as M4-i).

Expected masses (M4-b, FINAL): compute about 49k, transient about 157k, storage about 25k. Duration about that of draw-m3 (about 2 minutes of DAA waits plus merge waits). It spends what draw-m3 spends plus one claim fee; the payout (J + d) and the change come back to our key. The probe is also part of `probe all`.

### Decisions taken in Stage 4

| ID | Decision | Why |
|---|---|---|
| S4-1 | Mutants are per line: `"file:TAG#n"` removes the n-th line of a multi-line tag. Each of the 187 tagged lines is either killed or shown redundant. | A tag-level mutant removes both halves of a range together, so a dead half could go unnoticed. |
| S4-2 | Redundant lines are tested (`redundant`), and groups of mutually redundant lines are also removed together and killed (K2, K10, RB4 + RB5_zero, RA2, RA5, RF2, RF4). | As M3: a later change that removes the implying check fails a test. The group kills show that each group still matters. |
| S4-3 | Value bounds above 2^62 are killed in the local engine only (`engine_only()`) and recorded as consensus-equivalent (MAX_SOMPI < 2^62). The `v >= 0` lines are unbreakable (OpTxInputAmount). | This proves the requires execute; no on-chain transaction can tell them apart. |
| S4-4 | Covenant-less copies of the ticket, pool or marker, spent as funding, are the attacks for the `*_active`, RF1 and PT1 lines. | Anyone can create a P2SH output with any state and no covenant ID; with the index check removed, its script would pass at a funding index. It is harmless value-wise, but it is the only distinguishing transaction. |
| S4-5 | M4-c reports the worst change over {1, 1,000, 1,000,000 KAS}; the supremum is the large change. | The KIP-9 input relief falls as the input total grows. |
| S4-6 | tn10 `draw_m3` is refactored into `round1_drawn(ctx, tag)`; draw-m3's behaviour (including a rejected draw being a report row) is kept. | claim-m4 shares the 230-line sequence instead of copying it. |
| S4-7 | M4-b and M4-h "worst proofs": every shard holds the combination once (h = K); the PAY slice is 2 P_MIN K, the ACCRUE slice is K (prize 1 sompi), and FINAL is PAY with index 1 claimed. | Every leaf is then a full hash plus 18 levels, as DP21's method. |

### Open questions

1. Spec wording (M4-c): it names "change 1 KAS and large"; the worst is the large change (S4-5). The measured P_MIN still rounds to 0.05 KAS. No constant changed.
2. M4-h: at the minimum feerate over the priority mass, a PAY claim near P_MIN pays about 5-8 x P_MIN when blocks are full (driven by storage mass). Not a correctness issue; a fee-economics item for M6 (P_MIN, d, possibly batching claims).
3. Reviewer note (S4-4): covenant-less copies of the covenant scripts are inert under the committed code (every entry checks its index and/or input 0's covenant ID); the mutants show which line carries that.
4. Zero-fee note for the stage 5 brief: a claim, return or refresh with no funding input is consensus-valid and is rejected only by the K2_fund / RB2_fund / RA2_fund / RF2_fund lower bounds, which are not equivalent.

### Gate (re-run after the interruption, 2026-10-11)

All green.

Full `cargo test --no-fail-fast` (debug, from `crates/`, `EXIT 0`):

| Binary | Result |
|---|---|
| harness unit / covenant_smoke / m0_limits / m0_mass / m0_merkle / m0_shapes | 0 / 3 / 3 / 5 (119.7 s) / 2 / 6 passed |
| lottery (unit) | 27 passed |
| m1_attacks / m1_buy / m1_measure / m1_mutants | 25 / 14 / 4 / 11 passed |
| m2_attacks / m2_close / m2_game / m2_measure / m2_mutants / m2_params | 40 / 8 / 2 / 5 / 25 / 17 passed |
| m3_attacks / m3_draw / m3_game / m3_measure / m3_mutants | 22 / 24 / 13 / 7 / 93 passed |
| **m4_attacks / m4_claim / m4_game / m4_measure / m4_mutants** | **21 / 22 / 4 / 8 / 211 passed** (m4_mutants: 1 ignored, the `redundancy_attribution` diagnostic; 172.9 s) |
| doc-tests harness, lottery | 0, 0 |

The only warnings are the 2 pre-existing unused variables in `harness/tests/m0_mass.rs`.

Release `cargo test -p lottery --release --test m4_measure -- --nocapture --test-threads=1`: `test result: ok. 8 passed; 0 failed` (11.9 s). The tables in "Scope item 3" are this run's output (identical figures). Tail:

```
P_MIN (committed) = 5000000 sompi: worst PAY storage 405486 (81.1 %). Measured P_MIN (smallest P within 90 %, rounded up to 0.01 KAS) = 5000000 sompi (0.05 KAS): D57 confirmed
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 11.91s
```

`M4_ATTRIB=1 cargo test -p lottery --release --test m4_attacks -- --nocapture --test-threads=1`: **274 ATTRIB lines, 274 OK, 0 MISMATCH, 0 NO MARKERS**; `test result: ok. 21 passed` (116 s). Per test: engine_only 9, ka1 2, ka2 12, ka3 14, ka4 3, ka5 12, ka6 8, ka7 1, ka8 5, ka9 19, ka10 18, ka11 9, ka12 46, ka13 22, ka14 41, ka15 53 (Stage 3b: 146).

Release `cargo test -p lottery --release --test m4_mutants`: `211 passed` (66 s). The ignored `redundancy_attribution` (release, 679 s) produced the "first rejecting check" column above.

D64 results: `close()` K = 16 is 58.9 / 86.8 / 80.5 %; DP21 worst (np 7, F_MAX) is 30.7 / 40.6 / 11.8 %; `buy()` compute and transient are at most 9.3 / 15.9 %. All are <= 90 %, so there is **no stop**. `buy()` storage 95.7 % is accepted per U2.

Feature gates:
- `cargo check -p lottery --no-default-features --features mainnet,test-seed` fails as required: `error: TEST-ONLY seed cannot be built for mainnet`.
- `cargo check -p lottery --no-default-features --features mainnet` compiles.
- `cargo build -p tn10` is clean (no warnings).

The tn10 probe was **not run**.

## Review (covenant-reviewer @ 2dd53ca) and user decisions (2026-10-11)
No must-fix. Should-fix S1–S4, notes N1–N3 (full report summarised by the orchestrator).
- U3 (S1): add genesis checks in `validate_with`: claim_window ≥ 6,048,000 (7 days) and seed_offset + draw_window + claim_window ≤ D_RET. Failing test first (reviewer repro: claim_window = 14 days → return_deposit_after forfeits a winning ticket before its pool deadline; claim_window = 0; draw_window = 7 days).
- U4 (S2): late draw() can create pools whose claim deadline already passed (caller picks old funding inputs; drawer check 19). Park with the missed-draw item and release gate; DECISIONS row stating design 4.5 "at least 7 days" holds only for a draw within draw_window. No M4 code change.
- U5 (S3): one draw() creates up to 7 pools but absorbs A_MAX = 5. Measure DP21 with A_MAX = 7; if every mass ≤ 90 %, raise A_MAX to 7 in the drawer; otherwise stop and ask.
- S4: DECISIONS rows, costs.md M4, spec U1 text, tn10 M4-i (probe ran OK 2026-10-11, results in crates/tn10/results-claim-m4.md) — Stage 5 recording.
- Notes: N1 third-party forfeits after deadline and zero-prize ACCRUE/path-a re-spends without D_IDLE are by design (D56/D58); N2 refresh only raises next draw's own_daa (extends D_claim); N3 claim fee economics near P_MIN → M6.

## Review fixes (U3, U4, U5) (2026-10-11, branch `m4-claim`, uncommitted)

### What changed

| File | Change |
|---|---|
| `crates/lottery/src/genesis.rs` | **U3:** `validate_with` refuses `claim_window < CLAIM_WINDOW` (6,048,000) and `seed_offset + draw_window + claim_window > D_RET` (sum in i128; D_RET is `game::D_RET`). New unit tests `u3_genesis_refuses_windows_beyond_d_ret`, `d_ret_mirrors_ticket_sil`, `a_max_mirrors_drawer_sil`. The accepted instance in `genesis_refuses_rereview_instances` (windows summing to WINDOWS_MAX) is now refused, so the test asserts that refusal ("D_RET") and that D_RET <= WINDOWS_MAX (see R-2) |
| `crates/lottery/tests/m4_claim.rs` | **U3:** `u3_r1_windows_refused_at_genesis` (the reviewer's three repros, through `validate()` and the checked `Chain::genesis_with`), `u3_default_windows_claim_deadline_before_return_after` (the r1 sequence on the default genesis: pool deadline < T + D_RET, and >= 7 days after the draw). **U4:** `parked_u4_late_draw_creates_already_expired_pools` (the reviewer's r2, documents the parked behaviour, see R-3) |
| `covenants/drawer.sil`, `crates/lottery/src/game.rs` | **U5:** `A_MAX = 7` (was 5, D44), with a comment |
| `crates/lottery/tests/common/m3.rs`, `m3_attacks.rs` | A_MAX + 1 attacks now use `A_MAX` (valid base absorbs 7, attack 8). With the old literal 5 + 1 = 6 the attack would have been a valid transaction |
| `crates/lottery/tests/m3_draw.rs` | DP14 also absorbs A_MAX = 7 round-1 pools on a real two-round chain whose genesis funds every lower tier (`ST_ALL`, round 1 creates 7 pools); `two_round_chain_with(seed, st, np)` added |
| `crates/lottery/tests/m3_mutants.rs`, `m3_measure.rs` | Comment / printed label only (a = A_MAX + 1 = 8; M3-c label prints A_MAX) |

No other fixture or genesis uses non-default windows: every genesis (tests and tn10 `close-m2`, `draw-m3`, `claim-m4`) uses CLAIM_WINDOW, DRAW_WINDOW and seed_offset 600 (sum 6,480,600 <= 12,096,000). `m3_game.rs` uses seed_offset 0 in a controller state that is not a genesis. No test needed short windows, so no test escape hatch was needed.

### Failing-first output (before the `validate_with` change)

```
test genesis::tests::d_ret_mirrors_ticket_sil ... ok
test genesis::tests::u3_genesis_refuses_windows_beyond_d_ret ... FAILED
thread 'genesis::tests::u3_genesis_refuses_windows_beyond_d_ret' panicked at lottery\src\genesis.rs:619:27:
claim_window = 14 days: bad instance accepted
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 27 filtered out

test u3_r1_windows_refused_at_genesis ... FAILED
test u3_default_windows_claim_deadline_before_return_after ... ok
---- u3_r1_windows_refused_at_genesis stdout ----
genesis accepted: ["claim_window = 14 days", "claim_window = 0", "draw_window = 7 days"]
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 22 filtered out
```

After the change: lib 30 passed (27 + 3 new); `m4_claim` u3 tests 2 passed.

### U5: DP21 with A_MAX = 7 (release, measured before raising A_MAX; all <= 90 %, so A_MAX = 7 is kept)

| Shape (K = 16, m = 50, a = 7) | compute | transient | storage | % c / t / s |
|---|---|---|---|---|
| np 5, 1 funding input | 160,788 | 432,912 | 71,007 | 32.2 / 43.3 / 14.2 |
| np 5, 8 funding inputs | 168,628 | 436,272 | 71,007 | 33.7 / 43.6 / 14.2 |
| np 7, 1 funding input | 162,202 | 433,608 | 71,009 | 32.4 / 43.4 / 14.2 |
| **np 7, 8 funding inputs (worst: 7 created + 7 absorbed, 19 inputs, 20 outputs)** | 170,042 | 436,968 | 71,009 | **34.0 / 43.7 / 14.2** |

At A_MAX = 5 (Stage 4) the worst was 30.7 / 40.6 / 11.8 %. M4-e (M4 pool, 2,481 B redeem) gives the same figures; pool leg units at most 323,673.

Storage, M3-d (values at the worst, R = 1.69 KAS): a = 7 reaches 58.8 % (f_min = F_MIN) and 76.8 % (f_min = F_MIN_FLOOR). The supremum with input relief dropped is 304,016 (60.8 %) and 394,016 (**78.8 %**, was 76.4 % at a = 5). With the drawer alone at a = 7, R = 1.116 KAS would be enough. M3-c worst by m (a = 7, np 7, F_MAX): 34.0 / 43.7 / 14.2 % at m = 50. Drawer redeem 45,466 B (K 16, M_DRAW 50).

`close()` K = 16 does not change: 58.9 / 86.8 / 80.5 % (M3-f and M4-f).

### Decisions

| ID | Decision | Why |
|---|---|---|
| R-1 | U3 as decided: `claim_window >= CLAIM_WINDOW` and `seed_offset + draw_window + claim_window <= D_RET`, both in genesis `validate_with`. These are genesis-only rules: all three values are fixed for the instance's life (seed_offset is copied unchanged by close/draw, and the windows by draw). Tests `d_ret_mirrors_ticket_sil` and `a_max_mirrors_drawer_sil` tie the Rust constants to the .sil sources | User decision U3 |
| R-2 | The WINDOWS_MAX check (M3 re-review S1) is kept, though the new rule implies it while D_RET <= WINDOWS_MAX (asserted). Its accept-boundary instance (windows sum = WINDOWS_MAX) is now asserted refused, by the D_RET rule | A stricter rule supersedes the old boundary. The test still pins each refusal |
| R-3 | U4: no code change. `parked_u4_late_draw_creates_already_expired_pools` asserts today's behaviour as a tripwire. When the missed-draw item is fixed, it fails and must be replaced by the attack test | The decision is parked. The test keeps the gap visible |
| R-4 | U5: A_MAX = 7 in drawer.sil and `game::A_MAX`. DP14 now absorbs 7 real pools; A_MAX + 1 = 8 is rejected (`m3_attacks` pair, `m3_mutants::redundant_d2_a_upper`) | Measured within 90 % (table above) |

DECISIONS rows for R-1..R-4 (and the D44 update to A_MAX = 7) are left for Stage 5 recording with S4.

### Open questions

1. **The U3 rule is exact only for a `close()` at exactly T.** The pool deadline after an on-time draw is `close_own + seed_offset + draw_window + claim_window`, with `close_own >= T`. So `return_deposit_after` (T + D_RET) cannot precede the claim deadline only while `close_own - T <= D_RET - (seed_offset + draw_window + claim_window)`. At the defaults that slack is 5,615,400 DAA (about 6.5 days). At an instance on the boundary (sum = D_RET), any late `close()` reopens the r1 forfeit for the delay. This is the same family as U4 (late close or draw), so it was not changed. Options: park it with the missed-draw item, require a minimum slack at genesis, or anchor return_deposit_after to the pool deadline instead of T.
2. The M3 ATTRIB instrument has no built-in mismatch flag (it prints the first failing check). It was compared line by line with a HEAD run in a temporary worktree: 262 lines, identical.

### Gate (from `crates/`)

- Full `cargo test --no-fail-fast` (debug): **EXIT 0**. harness 0 / 3 / 3 / 5 / 2 / 6; lottery unit **30**; m1 25 / 14 / 4 / 11; m2 40 / 8 / 2 / 5 / 25 / 17; m3_attacks 22, m3_draw 24, m3_game 13, m3_measure 7, m3_mutants 93; m4_attacks 21, **m4_claim 25** (22 + 3), m4_game 4, m4_measure 8, m4_mutants 211 (1 ignored, as before). The only warnings are the 2 pre-existing ones in `harness/tests/m0_mass.rs`.
- Release `m3_measure`: `test result: ok. 7 passed`. Release `m4_measure`: `test result: ok. 8 passed` (tail: `P_MIN ... D57 confirmed` unchanged; `A_MAX = 7 stands (D44).`).
- `M3_ATTRIB=1 ... m3_attacks`: 262 ATTRIB lines, identical to HEAD; `22 passed`. `M4_ATTRIB=1 ... m4_attacks`: **274 lines, 274 OK, 0 MISMATCH, 0 NO MARKERS**; `21 passed`.
- `cargo check -p lottery --no-default-features --features mainnet,test-seed` fails: `error: TEST-ONLY seed cannot be built for mainnet`. `--features mainnet` compiles. `cargo build -p tn10` is clean.
- The tn10 probe was not re-run. The drawer script changed (A_MAX), so the `claim-m4` and `draw-m3` probe results describe the A_MAX = 5 drawer.

## Orchestrator: user decisions after review fixes (2026-10-11)
- U6: the late-close slack gap (U3 only fully protects a close at T; slack D_RET − (seed_offset + draw_window + claim_window) ≈ 6.5 days at defaults) is parked with U4 / the missed-draw item and the release gate. DECISIONS row; no M4 code change.
- U7: re-run `probe claim-m4` on tn10 after the fix diff is reviewed; record final-script results as M4-i.
