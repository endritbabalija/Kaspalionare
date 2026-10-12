# M5 handoff (staged executors)

Plan: `C:\Users\endri\.claude\plans\steady-imagining-grove.md`. Spec: `docs/specs/5.md`. Each stage appends its own section. Delete this file in Stage 8.

## Stage 1: layouts, templates, genesis and the Rust reference

Branch `m4-claim` (the session started on `master`, which pointed at the same commit `1abab20`; switched before any edit). Nothing committed.

### What was done

- **New placeholder covenants** (as D13, D28: final state layout, one always-rejecting entry `locked()`):
  - `covenants/request.sil` (role 0x07, A3.2 layout; constructor constants `recordTemplateHash` = RECORD_TPL and `positionTemplateHash` = POSITION_TPL);
  - `covenants/record.sil` (role 0x08, A3.3 layout; no constructor constant);
  - `covenants/position.sil` (role 0x0B, A3.4 layout; constructor constant `vaultTemplateHash` = VAULT_TPL).
- **`covenants/vault.sil`**: the A3.1 layout (`record_tpl`, `request_tpl`, `position_tpl` appended after `liability`, as constructor params `init_*` and state fields). `draw_leg` is unchanged. No request entries yet (Stage 3).
- **`covenants/drawer.sil`**: the `VaultState` struct gains the three template fields; check 23 copies them from `vt` into vault'. Nothing else changed: check 5 ranges, the shape, the arguments and every other check are as in M4 (the A5 changes are Stage 4).
- **`crates/lottery/src/state.rs`**: `ROLE_REQUEST`, `ROLE_RECORD`, `ROLE_POSITION` (0x0B), `KIND_DEPOSIT`/`KIND_WITHDRAW`; `VaultState` gains the three fields; new `RequestState`, `RecordState`, `PositionState` with `values()` and `ctor()` (request: RECORD_TPL, POSITION_TPL then state; position: VAULT_TPL then state; record: state only).
- **`crates/lottery/src/buy.rs`**: `Sources` and `Templates` gain `record`, `position`, `request`; `compile_sources` follows D74; `dummy_record/position/request`; `record_/position_/request_redeem` and `_spk` helpers.
- **`crates/lottery/src/game.rs`** (M5 section): constants `SHARE_UNIT`, `VS`, `VA`, `MIN_DEPOSIT`, `SHARES_MAX`, `ASSETS_MAX` (P_MIN and D_IDLE reused from M4, same values as spec A2); `mulfrac` (u128), `mulfrac_steps` (the script's 63-step algorithm in checked i64), `withdraw_out`, `Conversion` + `convert` (C1–C3, with `vault_after`, `record`, `record_value`), `wc_witness_ok`, `SettleMode`, `Settlement` + `settle` (A6.1 checks 3–6), `record_in_range`, `request_in_range`.
- **`crates/lottery/src/genesis.rs`**: S₀ = V₀ / SHARE_UNIT in vault and position; deployer `position` field (default owner = Schnorr P2PK of `FUNDING_SK`), output k + 4 with value R; covenant group 0..k+5; `position_output()`, `set_owner()`; `set_vault()` also sets S₀; `validate_with` checks V₀ ∈ [SHARE_UNIT, ASSETS_MAX], total_shares = S₀, the three vault template fields = the compiled hashes, the position {0, S₀, D11 owner}. The old "vault shares" case (total_shares = 1 refused) is inverted: total_shares = 0 and S₀ + 1 are refused.
- **`crates/lottery/src/close.rs`** `genesis_and_ledger`: 9 bound outputs, change at 9, 10 outputs; position output value and script checked.
- **`crates/lottery/src/draw.rs`**: the `DrawCase` fixture vault carries the compiled template hashes (no behaviour change).
- **`crates/tn10/src/cases.rs`**: both genesis probes set the deployer position owner to the probe key.
- **New test `crates/lottery/tests/m5_game.rs`** (11 tests): VP12 Rust half and VP1, plus Stage 1 scaffolding checks (see below).

### Decisions

- **S1-1 Placeholder constants must be read at run time.** The first placeholder version used `require(vaultTemplateHash.length == 32)`; the compiler folds that away and the template hash did not depend on VAULT_TPL (caught by `d74_template_route`). The placeholders now use `require(OpInputCovenantId(0) != <constant>)` before the rejecting `require(this.activeInputIndex < 0)`, so the D74 route is real in Stage 1 already. Stages 3 and 5 replace these entries; the real entries use the constants through `readInputStateWithTemplate` / `validateOutputStateWithTemplate`.
- **S1-2 Compile order.** controller → drawer(controller) → marker, vault, pool (drawer) and record → position(vault) → request(record, position) → ticket(pool) → shard(ticket, controller), params(controller). D74 lists the record under "drawer ← {marker, vault, pool, record}" but also says "The record has none" (no constructor constant). I read the arrow for the record as compile order only: the record embeds no hash. Without the `test-seed` feature the record, position and request still compile (position gets the vault hash compiled with the zero drawer hash, as marker/vault/pool do today).
- **S1-3 Drawer check 5 unchanged in Stage 1.** The extended A3.1 ranges (total_assets ≤ ASSETS_MAX, total_shares ≤ SHARES_MAX, pend_wsh ≤ total_shares) belong to the plan's Stage 4 list ("check 5 ranges"), so Stage 1 has no drawer behaviour change beyond carrying the struct fields (plan: "nothing else changed in behaviour").
- **S1-4 Genesis V₀ range is [SHARE_UNIT, ASSETS_MAX]** (was [0, TOTAL_MAX]). This forced one fixture change: `genesis_refuses_stage5_review_instances` used `set_vault(TOTAL_MAX)` for its G2_MAX case; it now uses `set_vault(ASSETS_MAX)` (same intent: a vault large enough that only the G2 check can refuse).
- **S1-5 Deployer position defaults.** Owner = `Payout::Schnorr(xonly(FUNDING_SK))` (the deployer's funding key, D11 class 0); value = controller.reserve (R); round 0; `validate` requires round = 0 and shares = total_shares (VP1 "{0, S₀, owner}"). `set_owner` changes the owner; the tn10 probes use it.
- **S1-6 `convert` domain (`None` = the script rejects).** Outside the A3.1 ranges (V' > ASSETS_MAX, S > SHARES_MAX, W ∉ [0, S], D < 0), V' + D > TOTAL_MAX (check 20's existing guard), and L' > V' + D_c (A8: the subtraction slack = V' + D_c − L' is formed only after its comparison). wc is computed in closed form, floor(((slack + 1)(S + VS) − 1)/(V' + VA)) capped at W, in u128; the script takes it as a witness (`wc_witness_ok` mirrors C2). The tests compare it with a binary search and, on a small grid, a linear scan.
- **S1-7 `mulfrac_steps`** mirrors the A8 algorithm exactly (bits 62 down to 0, double-and-subtract, add-and-subtract), with checked i64 ops and the invariant 0 ≤ X < c asserted each step. It is the reference for the Stage 4 script implementation; VP12 shows it equals the u128 `mulfrac` on the boundaries and on 100,000 random triples.
- **S1-8 `settle` reference details.** The order of the pay rule follows A6.1-4 literally: pay = 0 if burn_rem = 0, else out_rem if conv = burn_rem, else mulfrac(out_rem, conv, burn_rem). `request_in_range` includes the D11 key padding; the value range (R ≤ value ≤ TOTAL_MAX) is left to the builders (it needs the UTXO value). `Settlement::payout(s, value)` gives the plain payout of A6.1 check 9.
- **S1-9 Derived properties tested beyond the A6.2 list** (all follow from the floors; stated in the test docs): (a) a conversion never lowers the price per share for the holders who stay, (A' + VA)(S + VS) ≥ (V' + VA)(S' + VS); (b) every settler gets at least floor(converted total × s / side total) of each side (from the ratio invariant); (c) a refund-mode record repays exactly D; (d) conservation V' + D = total_assets' + out_c + (D − D_c).
- **S1-10 Fixtures left for later stages.** The standalone `DrawCase` vault keeps total_shares = 0 (M3 fixture); `Chain` (tests/common/m2.rs) does not yet track the genesis position. The plan puts positions/requests/records in `Chain` and the conversion in `DrawCase` in Stage 2.

### Deviations from the spec

None in substance. S1-1 (how the placeholders reference their constants) and S1-2 (reading the record in D74's arrow list as compile order) are interpretations, listed for the reviewer.

### Open items and questions

- **O1 (spec wording, D72):** "settlement order cannot change the totals by more than one unit per settler" is not precise enough to test as written (the totals are order-independent by exact drain; per-request amounts can differ by order). Stage 1 tests the provable form instead (S1-9 b). Suggest a spec Correction in Stage 8.
- **O2 (D74 wording):** "drawer ← {marker, vault, pool, record}" vs "The record has none" (S1-2). Suggest a Correction: the record has no dependency and may be compiled anywhere before the request.
- **O3 (stack headroom, D78c):** the drawer's `VaultState` grew by three fields (three more bindings when `vt` is read). The drawer still compiles. The S3-6 headroom figure (18 spare live ints) was not re-measured in Stage 1; Stage 4 adds the conversion block and must re-check it before anything else.
- **O4 (drawer bytes changed):** the drawer, and through DRAWER_TPL the marker, vault and pool, and through POOL_TPL the ticket and shard, have new template hashes. The M3/M4 measurement suites still pass (below), but `docs/costs.md` numbers are not updated in Stage 1 (Stage 6 re-measures, incl. M5-d `close()` K = 16).
- **O5:** `game.rs`'s module note "every function mirrors the script in signed 64-bit integers" does not hold for `mulfrac` (u128 by D73); `mulfrac_steps` is the i64 mirror.
- **O6:** the stray untracked `DECISIONS.md` at the repo root (not `docs/DECISIONS.md`) predates this stage; it was left untouched. It looks like notes from another project (SilverScript v0.1.3, burial delay); the user should decide whether to delete it.

### Files changed

- New: `covenants/request.sil`, `covenants/record.sil`, `covenants/position.sil`, `crates/lottery/tests/m5_game.rs`, `docs/specs/5-handoff.md`.
- Changed: `covenants/vault.sil`, `covenants/drawer.sil`, `crates/lottery/src/{state,buy,game,genesis,close,draw}.rs`, `crates/tn10/src/cases.rs`.

### Tests in m5_game.rs

| Test | Covers |
|---|---|
| `vp12_mulfrac_boundaries` | a ∈ {0, 1, 2, 2⁶² − 1, 2⁶²} × 7 values of c (incl. 1, 2⁶²) × r ∈ {0, c/2, c − 1}; the extreme product; 9 out-of-domain triples |
| `vp12_mulfrac_random` | 100,000 random triples, `mulfrac` and `mulfrac_steps` vs u128 |
| `vp12_convert_matches_reference` | 20,000 random states + an exhaustive small grid vs the brute reference (u128 mint, binary-search and linear-scan wc); C2 witness uniqueness; D69 lock; no dilution; record fields and ranges; conservation; every branch reached |
| `vp12_convert_boundaries` | refund at mint < ceil(D / MIN_DEPOSIT) (price 10⁹/share; price exactly MIN_DEPOSIT and one sompi above); the SHARES_MAX pre-guard at D = (SHARES_MAX − S)/q and + 1, and with q > room; cap binding and D69 (same-draw deposits raise wc); 9 out-of-range states; the largest state; nothing pending |
| `vp12_settle_matches_reference` | 20,000 random records/requests vs u128 floors, all five modes; 16 rejections (wrong draw's record t ± 1, s > rem, sh = 0, ranges); PAY at pay = P_MIN and ACCRUE at P_MIN − 1; refund and FINAL payouts |
| `a62_drain_invariants_random` | 3,000 random draws × 2 random orders: every A6.2 invariant and S1-9 (b), (c); every mode reached |
| `a62_min_mint_every_share_positive` | mint = ceil(D / MIN_DEPOSIT) exactly, s ∈ [MIN_DEPOSIT, 1.5 MIN_DEPOSIT], 1–6 requests: every sh ≥ 1 |
| `vp1_genesis_shares_position_templates` | built genesis for Schnorr, ECDSA and P2SH owners: S₀, the position at k + 4 (value R, script), the template fields, bindings 0..=k+4, change unbound, fee, ledger; S₀ rounds down |
| `vp1_genesis_refusals` | V₀ = SHARE_UNIT − 1, 0, ASSETS_MAX + 1; total_shares S₀ − 1, 0; each template field wrong, swapped, = vault hash; position shares ± 1, round 1, owner class 3, padding; builder panics on each; boundaries V₀ = SHARE_UNIT, ASSETS_MAX accepted |
| `d74_template_route` | VAULT_TPL in the position template, RECORD_TPL and POSITION_TPL in the request template, the committed hashes equal those compiled from the committed hashes; vault template independent of its state; all 11 templates distinct |
| `stage1_placeholders_reject` | each placeholder has exactly one entry `locked()`, which fails with VerifyError (funding input passes) |

### Final `cargo test` (from `crates/`)

Run as `cargo test -j 2` (debug). `-j 2` because a parallel build ran out of Windows paging file (os error 1455) on this machine; the tests are the same. Exit code 0. Also checked: `cargo check -p lottery --no-default-features --features mainnet` builds, and the seed gate `--features mainnet,test-seed` still fails with `TEST-ONLY seed cannot be built for mainnet`.

```
harness unit          test result: ok. 0 passed; 0 failed
covenant_smoke        test result: ok. 3 passed; 0 failed
m0_limits             test result: ok. 3 passed; 0 failed
m0_mass               test result: ok. 5 passed; 0 failed
m0_merkle             test result: ok. 2 passed; 0 failed
m0_shapes             test result: ok. 6 passed; 0 failed
lottery unit          test result: ok. 30 passed; 0 failed
m1_attacks            test result: ok. 25 passed; 0 failed
m1_buy                test result: ok. 14 passed; 0 failed
m1_measure            test result: ok. 4 passed; 0 failed
m1_mutants            test result: ok. 11 passed; 0 failed
m2_attacks            test result: ok. 40 passed; 0 failed
m2_close              test result: ok. 8 passed; 0 failed
m2_game               test result: ok. 2 passed; 0 failed
m2_measure            test result: ok. 5 passed; 0 failed
m2_mutants            test result: ok. 25 passed; 0 failed
m2_params             test result: ok. 17 passed; 0 failed
m3_attacks            test result: ok. 22 passed; 0 failed
m3_draw               test result: ok. 24 passed; 0 failed
m3_game               test result: ok. 13 passed; 0 failed
m3_measure            test result: ok. 7 passed; 0 failed
m3_mutants            test result: ok. 93 passed; 0 failed
m4_attacks            test result: ok. 21 passed; 0 failed
m4_claim              test result: ok. 25 passed; 0 failed
m4_game               test result: ok. 4 passed; 0 failed
m4_measure            test result: ok. 8 passed; 0 failed
m4_mutants            test result: ok. 211 passed; 0 failed; 1 ignored
m5_game               test result: ok. 11 passed; 0 failed
(doc-tests)           test result: ok. 0 passed; 0 failed (x2)
```

### Notes for Stage 2

- Builders to write next: `vault.rs` (DepositCase, WithdrawCase, SettleCase, RefreshCase), the DrawCase conversion and record (index 3 + np, new ARG_* positions), `Chain` positions/requests/records, the owner `InKind` (ECDSA / P2SH).
- Use `game::convert` / `game::settle` as the reference; `Conversion::vault_after` and `Conversion::record` give vault' and the record.
- The genesis position is at `GenesisCase::position_output()` = k + 4; `Chain::genesis_with` does not yet record it.

## Stage 2: tests first (builders, VP/VA tests, attribution and preflight)

Branch `m4-claim`, nothing committed. No covenant source was changed in this stage.

### What was done

- **Builders (`crates/lottery/src`)**
  - `buy.rs`: `InKind::Owner(OwnerKey)` and `OwnerKey` {Schnorr, Ecdsa, P2sh} for the D67 owner input. P2SH is a P2SH-wrapped Schnorr P2PK of the key. ECDSA signs the ECDSA sighash (rusty-kaspa v2.1.0 `consensus/core/src/hashing/sighash.rs:282-293`) with a compact signature, as `OpCheckSigECDSA` verifies it (`crypto/txscript/src/opcodes/mod.rs:804`, `crypto/txscript/src/lib.rs:885-892`). `Built::finish` budgets owner inputs at one signature check, re-budgets from the signed run when it needs more (P2SH), and signs them.
  - `claim.rs`: `RefreshObj` gains `Position`, `Request`, `Record` (+ `name()`).
  - `draw.rs`: `DrawCase` converts the pending totals with `game::convert` at V' and L' (C1-C3). Vault' = C3; the record sits at output 3 + np iff anything is pending, and the caller pays its R; controller' v_rec = total_assets'. New `Lie` fields `wc`, `conv_v`, `j_v`, `refund`, `record`, `out_delta`. `DrawExpected` gains `conv`, `rec`, `record`, `record_output()`, `n_cov()`, and every plain-output index shifts by rec. `ARG_WC` = 19, `ARG_RECORD_PREFIX` = 20, `ARG_RECORD_SUFFIX` = 21, `ARG_OWN` = 22 (`ARG_OWN_M4` = 19). `SIG_DRAW_M4` / `SIG_DRAW_M5`, `drawer_is_m5`. `RoundResult` gains `record: Option<ResultRecord>`. `from_tx` reads 25 pushes (M5) or 22 (M4) and rebuilds the record from the transaction bytes: the vault's and the controller's redeems, V' as check 14, L' from x2, y, g and m, the `wc` argument (checked to be the closed-form wc) and the funder. It then checks the record output's script and value. The fixture vault has total_shares = TEST_VAULT / SHARE_UNIT.
  - New `vault.rs`: `DepositCase`, `WithdrawCase`, `SettleCase` (+ `SettleLie`, `SettleExpected`), the `SIG_*` entry constants and argument positions, the fixtures (`fixture_vault`, `fixture_record`, `owner()`, `OWNER_SK`), `position_worth`. Refreshes use `claim::RefreshCase` with the M5 objects.
- **Tests (`crates/lottery/tests`)**
  - `common/m2.rs`: `Sil::{Record, Position, Request}`. `Chain` gains `positions` (the genesis position recorded, closing S1-10), `requests` and `records`. `Chain::draw` records the record and sends a converting draw through `accept_draw5`.
  - New `common/m5_spec.rs`: an executable model of every M5 check (labels below), evaluated on the finished transaction.
  - New `common/m5.rs`:
    - `Case5`, `Atk5`, `accept5`, `strike5`, `run5`;
    - `M5_PREFLIGHT`, and `M5_ATTRIB` (truncation instrument for vault/request/record/position; the drawer uses the M3 instrument);
    - the `Chain` M5 steps (`deposit*`, `withdraw*`, `settle*`, `refresh5*`, `close_now`, `draw_now`, `assert_share_invariants`);
    - `check_deposit` / `check_withdraw` / `check_settle` / `check_draw5` (VP13 conservation);
    - the D79 bases;
    - VA1-VA14 as data, and VA15 cases.
  - New `m5_vault.rs` (13 tests) and `m5_attacks.rs` (19 tests: 212 strikes, the VA4 ledger case, VA15 with 13 budget cases, three structural tests).
  - `m3_mutants.rs` `every_m3_tag_is_tested`: drawer.sil and vault.sil tags starting with `M5` are filtered out (S2-8).

### Decisions

- **S2-1 (PROVISIONAL, user decision O-S2-1): template bytes as arguments.** SilverScript v1.0.0's `validateOutputStateWithTemplate` takes exactly five arguments, including the template prefix and suffix bytes (`silverscript-lang/src/compiler/static_check.rs:811-825`, `docs/TUTORIAL.md:1218-1229`). Spec A4.1 check 9 and A4.2 check 10 bind the request with it, and A6.1 check 12 binds the position with it. None of those transactions has an input carrying that template, and the spec's argument lists carry no template bytes. Following the spec's own drawer precedent (`record_prefix`, `record_suffix` for check 26; `pool_prefix`, `pool_suffix` for check 25), the builders append:
  - `request_deposit(byte owner_class, byte[33] owner_key, byte[] req_prefix, byte[] req_suffix)`;
  - `request_withdraw(int w, int owner_idx, int pos_pl, int pos_sl, byte[] req_prefix, byte[] req_suffix)`;
  - `settle(int rec_pl, int rec_sl, byte[] pos_prefix, byte[] pos_suffix)`. pos_prefix and pos_suffix are ignored unless deposit-convert.

  The binding stays by hash, so the arguments are safe (as the pools). Changing this is one `SIG_*` constant and one `args()` per entry in `vault.rs`, plus the `*_ARG_*` positions.
- **S2-2 M4/M5 drawer signatures.** The drawer exists with the M4 entry, so `entry_sigscript` with the M5 signature would panic (tag mismatch). `DrawCase` therefore encodes the M5 signature only when the compiled drawer has it (`drawer_is_m5`), and the 20 M4 arguments otherwise. Draws with nothing pending are byte-identical to M4 draws, so M3/M4 stay green. Stage 4 can delete the M4 branch (`args_m4`, `SIG_DRAW_M4`, the 22-push path of `from_tx`).
- **S2-3** The standalone `DrawCase` vault has total_shares = TEST_VAULT / SHARE_UNIT (D66 as genesis). This changes no M3/M4 result: the M4 drawer copies it.
- **S2-4 Owner input.** P2SH owners are P2SH-wrapped Schnorr P2PK, so the owner input is really signed in every class. The owner input sits at 2 + `owner_at` among the plain inputs (default the first, owner_idx = 2).
- **S2-5 Preflight = executable spec model.** M4_PREFLIGHT only showed that each attack builds and gets *a* script error. `M5_PREFLIGHT=1` is stronger:
  - each strike requires `m5_spec` to reject the bad transaction at the target input with exactly the named check, and to accept the good one (a good plain input is not modelled; its own P2PK script must pass);
  - `accept5` tolerates a not-yet-written script (placeholder `EarlyReturn`, or any rejection by an M4-signature drawer) only where the model accepts that input, and then applies the transaction, so every positive chain runs end to end. The unchanged legs (marker, vault `draw_leg`, controller, pools) run their real scripts.

  Once a script exists, preflight also requires the script and the model to agree (accept/reject) at every modelled input. Stages 3-5 should run it, and a disagreement is a finding. Limits: the drawer model covers only the checks M5 adds or changes (5, 13, 17, C1, C2, 20-26) and assumes the unchanged M3 checks pass. Its truth for 20-26 is the builder's reference (`DrawCase` without its `Lie`). The settle model uses `game::settle`. So for amounts the model is as good as the Rust reference (VP12), not independent of it.
- **S2-6 Check labels** (the contract for Stages 3-5's `// ==== check <label>:` markers, in this order inside each entry):
  - vault.sil `request_deposit` D1..D9 (A4.1 1-9) and `request_withdraw` W1..W11 (A4.2 1-11);
  - position.sil `vault_leg` PO1..PO4; record.sil `request_leg` RL1..RL4;
  - request.sil `settle` S1..S13 (A6.1 1-13);
  - position/request/record.sil `refresh` A7.1..A7.7 (M4 A6 1-7);
  - drawer.sil: new markers `C1`, `C2`, `C3` after check 17 and `26` after 25 (the M3 instrument reads every marker of drawer.sil in order).

  Groups for truncation: through the first '.', else the leading letters (`common::m5::group5`).
- **S2-7 Labels where the spec's "Check" column names a compute-only step.** C1 and C3 contain no `require` (C1 only decides ok/refund). So a draw that claims conversion against C1 (VA1, and VA6 with the mint guard decision flipped) is rejected first at check 20 (vault' value), and a wrong mint is rejected first at check 23 (vault' state). Record-field lies are rejected at check 26. These labels are what the model shows.
- **S2-8 M5 mutant tags start with `M5`** in drawer.sil and vault.sil, so `every_m3_tag_is_tested` filters them. New files (request, record, position) need no filter.
- **S2-9 VP14 share invariant, general form.** Between a draw and its settlements, minted shares are in the record (mint_rem), not yet in positions. Converted withdrawal shares are burnt but still held by the requests (burn_rem). The tested invariants are:
  - total_shares = Σ positions + pend_wsh + Σ records' mint_rem;
  - Σ withdrawal requests = pend_wsh + Σ records' burn_rem.

  With every record drained these are the spec's "total_shares = Σ positions + pend_wsh". `Chain::assert_share_invariants` checks them after every chain step of VP2-VP14. The `round()` helper also checks total_assets' >= L' after each draw.
- **S2-10** VA1's "D = MIN_DEPOSIT - 1 / 0" and VA6's engine-only attacks are built as fabricated or over-funded cases. Values above MAX_SOMPI are engine-only, as M4 S4-3: VA6 "V' = ASSETS_MAX + 1" and "D = TOTAL_MAX - total_assets - pend_dep + 1".

### Deviations from the spec

- S2-1: three entry signatures gain template-byte arguments (provisional, needs a user decision).
- S2-7: the expected check of the draw-side VA1, of the VA6 mint-guard cases, and of the price/mint lies is 20, 23 or 26 rather than "C1"/"C1-C3" (C1 and C3 have no require). The settlement-side labels follow the spec.
- S2-9: VP14's invariant tested in its general form.

### Open items

- **O-S2-1 (BLOCKING before Stage 3, user decision): S2-1.** The spec's A4.1, A4.2 and A6.1 argument lists cannot compile with SilverScript v1.0.0. Proposed Correction: the S2-1 signatures. Alternatives (none found workable): a request/position input carrying the template does not exist in these shapes; storing template bytes in vault state contradicts D74.
- **O-S2-2 (Stage 4): where V' <= ASSETS_MAX is checked.** Spec A5 says "V' <= ASSETS_MAX and S <= SHARES_MAX (check 5 extended)", but check 5 reads the input vault (V), not V'. V <= ASSETS_MAX and check 14's guard (H <= TOTAL_MAX - V) only give V' <= TOTAL_MAX = ASSETS_MAX + 1, which would put M = V' + VA = 2^62 + 1 outside mulfrac's domain (c <= 2^62). The model requires it as the first statement of C1 (label `drawer:C1`, attack "VA6 V' = ASSETS_MAX + 1", engine only). Alternatively check 14's guard could become `hAmt <= ASSETS_MAX - vt.total_assets`; then relabel that attack `drawer:14`.
- **O-S2-3 (Stage 5): the request's value range.** A3.2 says "R <= value <= TOTAL_MAX", but the request has no R field and does not know the vault's R. The model checks 0 <= value <= TOTAL_MAX in S1. Creation fixes value = R (D7/W8), so nothing depends on a lower bound. Suggested Correction: "0 <= value <= TOTAL_MAX".
- **O-S2-4:** `RoundResult::from_tx` refuses an M4-signature draw with something pending (no wc argument). VP4 checks the refusal until Stage 4, then the full rebuild.
- **O-S2-5:** VP14's invariant (S2-9) and O1 (D72 wording) for the spec Corrections in Stage 8.
- **O-S2-6:** The VA4 ledger test and VP7/VP14 depend on real settlements, so they go green only after Stage 5.
- Stage 1's O3 (stack headroom) is unchanged and is Stage 4's first task.

### Test table

Status now (normal mode, committed templates). "Base rejected" means the strike's first good transaction was rejected at the target input, by the placeholder's dispatch (`EarlyReturn`) or by the M4 drawer (`VerifyError`). Each test runs its attacks in order and stops at the first one, so the reason given is that of the first attack. Every strike, positive and VA15 case passes under `M5_PREFLIGHT=1` (model as the oracle).

**Positives (`m5_vault.rs`)**

| ID | Test fn | Now | Failure reason now |
|---|---|---|---|
| VP1 | m5_game `vp1_*` (Stage 1) | pass | - |
| VP2 | `vp2_request_deposit` | FAIL | request_deposit: vault placeholder, input 0 EarlyReturn |
| VP3 | `vp3_request_withdraw` | FAIL | request_withdraw: position placeholder, input 1 EarlyReturn (vault input 0 too) |
| VP4 | `vp4_draw_with_deposits` | FAIL | first deposit request: input 0 EarlyReturn |
| VP5 | `vp5_withdrawals_within_slack` | FAIL | withdrawal request: input 1 EarlyReturn |
| VP6 | `vp6_cap_binds` | FAIL | converting draw: M4 drawer, input 0 VerifyError |
| VP7 | `vp7_deposit_drain_two_orders`, `vp7_withdrawal_modes` | FAIL | input 0 EarlyReturn / input 1 EarlyReturn (first request) |
| VP8 | `vp8_fairness` | FAIL | deposit request: input 0 EarlyReturn |
| VP9 | `vp9_refund_mode` | FAIL | refund-mode draw: M4 drawer, input 0 VerifyError |
| VP10 | `vp10_nothing_pending` | **pass** | - (M4 shape, real drawer) |
| VP11 | `vp11_refresh_then_use` | FAIL | refresh position: placeholder, input 0 EarlyReturn |
| VP12 | m5_game (Rust half, pass); `vp12_script_accepts_reference_grid` | FAIL | converting draw: M4 drawer, input 0 VerifyError |
| VP13 | `check_*` in every positive | - | (assertions run after acceptance) |
| VP14 | `vp14_three_draws` | FAIL | deposit request: input 0 EarlyReturn |

**Attacks (`m5_attacks.rs`)**

| ID | Test fn | Strikes | Now | Failure reason now (first strike) |
|---|---|---|---|---|
| VA1 | `va1_zero_share_deposit` | 4 | FAIL | base rejected, vault input 0 EarlyReturn |
| VA2 | `va2_cancelling` | 4 | FAIL | base rejected, request input 0 EarlyReturn |
| VA3 | `va3_wrong_draws_record` | 6 | FAIL | base rejected, request input 0 EarlyReturn |
| VA4 | `va4_second_settlement` | 1 | FAIL | base rejected, request input 0 EarlyReturn |
| VA4 | `va4_second_settlement_ledger` | ledger | FAIL | the chain's first deposit request: EarlyReturn |
| VA5 | `va5_withdrawal_below_liability` | 4 | FAIL | base rejected, drawer input 0 VerifyError (M4 drawer) |
| VA6 | `va6_overflow_boundaries` | 10 | FAIL | base rejected, drawer input 0 VerifyError |
| VA7 | `va7_stale_or_future_price` | 18 | FAIL | base rejected, drawer input 0 VerifyError |
| VA8 | `va8_double_conversion` | 7 | FAIL | base rejected, drawer input 0 VerifyError |
| VA9 | `va9_request_creation` | 38 | FAIL | base rejected, vault input 0 EarlyReturn |
| VA10 | `va10_withdrawal_authorization` | 30 | FAIL | base rejected, vault input 0 EarlyReturn |
| VA11 | `va11_settlement_outputs` | 37 | FAIL | base rejected, request input 0 EarlyReturn |
| VA12 | `va12_record_leg` | 9 | FAIL | base rejected, record input 1 EarlyReturn |
| VA13 | `va13_refresh` | 42 | FAIL | base rejected, position input 0 EarlyReturn |
| VA14 | `va14_j_independence` | 2 | FAIL | base rejected, drawer input 0 VerifyError |
| VA15 | `va15_leader_budget_minus_one` | 13 cases | FAIL | the deposit base: input 0 EarlyReturn |
| - | `attack_ids_are_unique`, `attacks_are_well_formed`, `attribution_instrument_self_test` | - | pass | - |

**Vacuous attacks.** None passes vacuously: strikes require the good transaction to pass the target input, so with a placeholder target every M5 attack test fails at "BASE REJECTED" instead of passing. Two attacks are rejected for the right reason by the current M4 drawer, but sit behind earlier failing strikes in their test, so they do not show as passing yet:
- "VA8 a record when nothing is pending" (M4 check 13, nCov);
- "VA8 the record without covenant binding": its base is pending, so it fails now.

The real evidence for the attacks now is the preflight below.

### Preflight result

`M5_PREFLIGHT=1 cargo test -p lottery --release --test m5_attacks --test m5_vault -- --nocapture --test-threads=1`:

- m5_attacks: `test result: ok. 19 passed; 0 failed`. All 212 strikes print `| OK`: the model rejects the bad transaction at exactly the named check and accepts the good one. VA15: 13 bases accepted by the model.
- m5_vault: `test result: ok. 13 passed; 0 failed`. 111 `PREFLIGHT accept` lines (the not-yet-written input tolerated where the model accepts). Every chain runs end to end (genesis, requests, buys, close, converting draws, settlements and refreshes over up to three rounds) with all VP13 checks and S2-9 invariants. The real marker, vault `draw_leg`, controller and pool leg scripts accept every converting draw.

The first preflight run caught four tooling errors, fixed before the result above:
- three attacks whose target is a plain input in the good transaction (the verdict now accepts a good "not modelled" input, whose own script must pass);
- one rogue copy inserted past the good transaction's last input.

### Commands

- Attribution, once markers exist: `M5_ATTRIB=1 cargo test -p lottery --release --test m5_attacks -- --nocapture --test-threads=1 | grep -oE "ATTRIB .*"` (lines `| expected X | got Y | model Z | OK|MISMATCH`).
- Gate: `cargo test -j 2 --no-fail-fast` (without `--no-fail-fast` cargo stops at the first failing test binary, now m5_attacks). The expected failures are exactly the 28 M5 tests listed above (12 in m5_vault, 16 in m5_attacks).

### Final `cargo test` (from `crates/`, `-j 2 --no-fail-fast`, debug)

Exit code 101 (from the two M5 binaries only). The first `unit` line is harness; the second is lottery (31 = 30 + the new `vault::tests`). Also: `cargo check -p lottery --no-default-features --features mainnet` builds, and the seed gate `--features mainnet,test-seed` still fails with `TEST-ONLY seed cannot be built for mainnet`.

```
unit (src/lib.rs)     test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
covenant_smoke        test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
m0_limits             test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
m0_mass               test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 64.39s
m0_merkle             test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.13s
m0_shapes             test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.64s
unit (src/lib.rs)     test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.35s
m1_attacks            test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.42s
m1_buy                test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.82s
m1_measure            test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 25.90s
m1_mutants            test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 36.77s
m2_attacks            test result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 15.69s
m2_close              test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.69s
m2_game               test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 30.90s
m2_measure            test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 129.92s
m2_mutants            test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 21.36s
m2_params             test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.09s
m3_attacks            test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 24.57s
m3_draw               test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.55s
m3_game               test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.95s
m3_measure            test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 20.80s
m3_mutants            test result: ok. 93 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 60.57s
m4_attacks            test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 23.10s
m4_claim              test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.72s
m4_game               test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.40s
m4_measure            test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 35.99s
m4_mutants            test result: ok. 211 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 129.57s
m5_attacks            test result: FAILED. 3 passed; 16 failed; 0 ignored; 0 measured; 0 filtered out; finished in 26.48s
m5_game               test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.46s
m5_vault              test result: FAILED. 1 passed; 12 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.11s
doc-tests harness     test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
doc-tests lottery     test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Notes for Stage 3

- Resolve O-S2-1 first. The vault entries and `vault_leg` must use exactly the `SIG_*` signatures in `vault.rs` (`entry_sigscript` asserts the compiled dispatch tag).
- Put the S2-6 markers in check order. Then run, in this order:
  - `M5_PREFLIGHT=1` (script and model must agree at every input; Stage 3 makes vault and position real, the rest stays tolerated);
  - normal mode for VP2, VP3, VA9, VA10 (and the vault/position strikes of VA1, VA2, VA6);
  - `M5_ATTRIB=1`.
- Tag every new `require` `// mut:M5...` (S2-8).

## Stage 3: vault request entries and the position leg

Branch `m4-claim`, nothing committed.

Changed:
- `covenants/vault.sil`;
- `covenants/position.sil`;
- `crates/lottery/tests/m5_game.rs` (a scaffolding test);
- `crates/lottery/tests/common/m5_spec.rs` and `common/m5.rs` (the U-2 model and label).

The drawer source is untouched.

### User decisions taken after Stage 2

- **U-1 (O-S2-1, closed): template bytes are entry arguments.** S2-1 is final:
  - `request_deposit(byte,byte[33],byte[],byte[])`;
  - `request_withdraw(int,int,int,int,byte[],byte[])`;
  - `settle(int,int,byte[],byte[])`.

  The binding stays by template hash: from state (vault: `request_tpl`) or from a constant (request: POSITION_TPL). Stage 8: spec Correction to A4.1, A4.2 and A6.1.
- **U-2 (O-S2-2, closed): V' <= ASSETS_MAX via check 14.** The drawer's check 14 guard becomes `hAmt <= ASSETS_MAX - vt.total_assets`; C1 gets no new require. The drawer edit itself is Stage 4 (O-S3-1). Already done in this stage:
  - the model (`m5_spec.rs`) checks V + H <= ASSETS_MAX as label `drawer:14`, between 13 and 17;
  - the model's C1 statement is removed;
  - the attack "VA6 V' = ASSETS_MAX + 1" is relabelled `drawer:14`.

  The preflight stays green.
- **U-3 (O-S2-3, closed): the request's value range is 0 <= value <= TOTAL_MAX.** The model already checks this (S1), so nothing changed. Stage 5 implements it in `settle`; the spec Correction is Stage 8.

### What was done

- **`vault.sil`**
  - Constants TOTAL_MAX, ASSETS_MAX (2^62 - 1), SHARES_MAX (2^62 - 1000), F_MAX, RESERVE_MAX and MIN_DEPOSIT, with the values of spec A2 and `game.rs`.
  - Structs `RequestState` (A3.2) and `PositionState` (A3.4).
  - `payoutSpk`, the same code as in the drawer and the ticket.
  - Entries `request_deposit` (markers D1..D9) and `request_withdraw` (W1..W11), exactly as A4.1 and A4.2, in check order.
  - `draw_leg` is unchanged.
- **`position.sil`**: the placeholder `locked()` is replaced by `vault_leg(int vault_pl, int vault_sl)` (PO1..PO4), with the A3.1 `VaultState` struct. No `refresh` yet (Stage 5).
- **`m5_game.rs` `stage1_placeholders_reject`**: the position is removed from the placeholder list. Request and record stay in it; Stage 5 deletes the test.

### Decisions

- **S3-1 Own-state ranges (D1/W1).** These are the A3.1 ranges, nothing extra, read from the vault's own state fields:
  - role = 0x06 (a state field, so a fabricated UTXO can carry another value, D79);
  - 0 <= round < TOTAL_MAX;
  - total_assets in [0, ASSETS_MAX];
  - total_shares in [0, SHARES_MAX];
  - pend_dep in [0, TOTAL_MAX - total_assets];
  - pend_wsh in [0, total_shares];
  - liability in [0, TOTAL_MAX];
  - then R = in0.value - total_assets - pend_dep in [0, RESERVE_MAX].

  The subtraction for R cannot overflow. OpTxInputAmount pushes a value in [0, 2^63) or fails (rusty-kaspa v2.1.0 `crypto/txscript/src/opcodes/mod.rs:1100-1107`), and both subtracted terms are >= 0.
- **S3-2 D5 arithmetic.** D = out0.value - in0.value.
  - Both values are in [0, 2^63) (OpTxOutputAmount, `mod.rs:1151-1161`), so the difference fits in i64.
  - The bound TOTAL_MAX - total_assets - pend_dep is >= 0 by D1, so pend_dep + D <= TOTAL_MAX - total_assets in D8.
  - W9's pend_wsh + w <= total_shares holds by W4.
- **S3-3 Owner input (W5, D67).** After 2 <= owner_idx < tx.inputs.length, the vault requires `tx.inputs[owner_idx].scriptPubKey == payoutSpk(pc, ps.owner_key)`.
  - OpTxInputSpk and OpTxOutputSpk push the same `script_public_key.to_bytes()` encoding (`mod.rs:1112-1121`, `1163-1173`). So the D11 `payoutSpk` bytes (2-byte version, then the script) also match an input.
  - Negative control: with this require removed, VA10's first strike is accepted ("ATTACK ACCEPTED ... by input 0"). The require was then restored.
- **S3-4 Position checks are in the vault (W3).** The vault reads the position under its own `position_tpl` and checks:
  - role 0x0B;
  - shares in [1, SHARES_MAX];
  - owner_class <= 2 with the D11 padding;
  - in1.value = R.

  The leg checks only PO1-PO4, as the pool's `ticket_leg` does. The position's round is not checked; it is copied into position' (A3.4: "informational").
- **S3-5 W11 role.** position' takes role = ps.role, which W3 has already fixed to 0x0B, so this is the same as writing the literal.
- **S3-6 Entry arguments.**
  - The deposit's owner arguments are named `owner_class_arg` and `owner_key_arg`. Names are not part of the dispatch signature, so the builders are unaffected.
  - The entry prologue enforces the byte and byte[33] lengths (DECISIONS row; `silverscript-lang/src/compiler/compile/statement.rs:100-160`).
  - req_prefix and req_suffix are not length-checked. A wrong one fails the template hash in D9/W10, as with `pool_prefix` (M3 S3-7).
- **S3-7 Mutation tags.** One statement per tagged line.
  - Vault: `M5D<n>_*` and `M5W<n>_*` (the S2-8 filter).
  - Position: `PO1`, `PO2`, `PO3`, `PO4_tpl`, `PO4_role`. It is a new file, so no filter is needed.

### Deviations from the spec

None in this stage. U-1 changes the argument lists, as approved by the user; its Correction is pending (Stage 8).

### Status delta (normal mode)

| ID | Test fn | Stage 2 | Stage 3 | Reason now |
|---|---|---|---|---|
| VP2 | `vp2_request_deposit` | FAIL | **pass** | - |
| VP3 | `vp3_request_withdraw` | FAIL | **pass** | - (Schnorr, ECDSA and P2SH owners; whole and partial) |
| VP4, VP5, VP6, VP7 (both), VP8, VP9, VP12 grid, VP14 | | FAIL | FAIL | the chain now gets through the real request entries and stops at the converting draw: M4 drawer, input 0 VerifyError (Stage 4) |
| VP11 | `vp11_refresh_then_use` | FAIL | FAIL | refresh position: no `refresh` entry yet, input 0 EarlyReturn (Stage 5) |
| VA9 | `va9_request_creation` | FAIL | **pass** | 38 strikes |
| VA10 | `va10_withdrawal_authorization` | FAIL | **pass** | 30 strikes |
| VA1 | `va1_zero_share_deposit` | FAIL | FAIL | both D5 strikes pass; it stops at "settle: sh = 0" (request placeholder, Stage 5) |
| VA2 | `va2_cancelling` | FAIL | FAIL | the first strike is at the request (Stage 5); the vault:D2 strike passes in preflight |
| VA6 | `va6_overflow_boundaries` | FAIL | FAIL | the first strike is at the drawer (Stage 4); the six vault strikes (D1, W1, D5, W4 x3) pass in preflight and attribution |
| VA15 | `va15_leader_budget_minus_one` | FAIL | FAIL | the deposit, partial and whole withdrawal budget cases pass; it stops at "draw with conversion" (Stage 4) |
| VA3, VA4 (both), VA11, VA12, VA13 | | FAIL | FAIL | request, record or position-refresh placeholders: EarlyReturn (Stage 5) |
| VA5, VA7, VA8, VA14 | | FAIL | FAIL | M4 drawer: VerifyError (Stage 4) |

Exactly 10 tests still fail in m5_vault and 14 in m5_attacks. Each fails at the M4 drawer (Stage 4) or at a request, record or refresh placeholder (Stage 5).

### Preflight and attribution

**Preflight:** `M5_PREFLIGHT=1 cargo test -p lottery --release --test m5_attacks --test m5_vault -- --nocapture --test-threads=1`
- m5_attacks: `test result: ok. 19 passed; 0 failed`.
- m5_vault: `test result: ok. 13 passed; 0 failed`.
- All 212 strikes print `| OK`.
- The tolerated inputs ("PREFLIGHT accept") are only:
  - the M4 drawer (VerifyError);
  - the settle request and the record (EarlyReturn);
  - refresh.

  No vault request entry and no position leg is tolerated. So at every vault and position input, the script and the model agree in every positive and every strike.

**Attribution:** `M5_PREFLIGHT=1 M5_ATTRIB=1 ... --test m5_attacks` gives 78 `OK`, 0 `MISMATCH` and 5 `NO MARKERS`.
- All 77 vault and position strikes name their expected check:
  - vault: D1 1, D2 5, D3 2, D4 2, D5 3, D6 2, D7 2, D8 6, D9 6, W1 2, W2 2, W3 3, W4 3, W5 17, W6 3, W7 1, W8 3, W9 1, W10 4, W11 3;
  - position: PO1 2, PO2 1, PO3 1, PO4 2.
- The 78th `OK` is one drawer:13 strike.
- The 5 `NO MARKERS` lines are request, record or refresh strikes whose good input is a plain input. The Stage 5 files have no markers yet.

### Open items

- **O-S3-1 (Stage 4): the drawer edit for U-2.** Add `require(hAmt <= ASSETS_MAX - vt.total_assets)` to check 14; drawer.sil needs an ASSETS_MAX constant for it. Then the VA6 "V' = ASSETS_MAX + 1" strike must attribute to `drawer:14`.
- **O-S3-2 (Stage 6): the vault redeem script grew.** It gained two entries and `payoutSpk`. The vault's redeem is part of every `draw()` (input 2), so the draw mass rises. The M3/M4 measurement suites still pass, but M5-a and M5-b must re-measure (D78a). The position redeem changed too (code, and VAULT_TPL), and so did the request's (POSITION_TPL).
- **O-S3-3 (Stage 5): placeholders left.** The position has no `refresh` yet (A7, markers A7.1..A7.7). Request and record are still placeholders. Delete `stage1_placeholders_reject` once they are replaced.
- **O-S3-4 (Stage 6): mutants.** Mutants for the `M5D*`, `M5W*` and `PO*` tags, and a coverage pin.
- Unchanged: O-S2-4, O-S2-5 and O-S2-6. Stage 1's O3 (stack headroom) is still Stage 4's first task.

### Final `cargo test` (from `crates/`, `-j 2 --no-fail-fast`, debug)

Exit code 101, from the two M5 binaries only. The seed gate `--features mainnet,test-seed` still fails with `TEST-ONLY seed cannot be built for mainnet`; `--features mainnet` builds.

```
unit (harness)        test result: ok. 0 passed; 0 failed
covenant_smoke        test result: ok. 3 passed; 0 failed
m0_limits             test result: ok. 3 passed; 0 failed
m0_mass               test result: ok. 5 passed; 0 failed
m0_merkle             test result: ok. 2 passed; 0 failed
m0_shapes             test result: ok. 6 passed; 0 failed
unit (lottery)        test result: ok. 31 passed; 0 failed
m1_attacks            test result: ok. 25 passed; 0 failed
m1_buy                test result: ok. 14 passed; 0 failed
m1_measure            test result: ok. 4 passed; 0 failed
m1_mutants            test result: ok. 11 passed; 0 failed
m2_attacks            test result: ok. 40 passed; 0 failed
m2_close              test result: ok. 8 passed; 0 failed
m2_game               test result: ok. 2 passed; 0 failed
m2_measure            test result: ok. 5 passed; 0 failed
m2_mutants            test result: ok. 25 passed; 0 failed
m2_params             test result: ok. 17 passed; 0 failed
m3_attacks            test result: ok. 22 passed; 0 failed
m3_draw               test result: ok. 24 passed; 0 failed
m3_game               test result: ok. 13 passed; 0 failed
m3_measure            test result: ok. 7 passed; 0 failed
m3_mutants            test result: ok. 93 passed; 0 failed
m4_attacks            test result: ok. 21 passed; 0 failed
m4_claim              test result: ok. 25 passed; 0 failed
m4_game               test result: ok. 4 passed; 0 failed
m4_measure            test result: ok. 8 passed; 0 failed
m4_mutants            test result: ok. 211 passed; 0 failed; 1 ignored
m5_attacks            test result: FAILED. 5 passed; 14 failed
m5_game               test result: ok. 11 passed; 0 failed
m5_vault              test result: FAILED. 3 passed; 10 failed
doc-tests (x2)        test result: ok. 0 passed; 0 failed
```

### Notes for Stage 4

- Order of work:
  1. the stack headroom (Stage 1 O3);
  2. O-S3-1;
  3. C1-C3 and check 26.
- The drawer must expose `SIG_DRAW_M5`. Once it does, `DrawCase` switches to it on its own (`drawer_is_m5`).
- Every chain positive now reaches its first converting draw through the real vault entries. After Stage 4 they will stop at the first settlement (Stage 5).

## Stage 4: drawer conversion (C1-C3, check 26, mulfrac)

Branch `m4-claim`, nothing committed. Changed: `covenants/drawer.sil`; `crates/lottery/tests/m3_mutants.rs` (one mutant, below). No Rust builder or model change was needed: `DrawCase` switched to `SIG_DRAW_M5` on its own (`drawer_is_m5`).

### Stack headroom (Stage 1 O3, D78c)

**Limit (verified, not from memory):** `MAX_STACK_SIZE = 244` combined main + alt stack items, rusty-kaspa v2.1.0 `crypto/txscript/src/lib.rs:77`, enforced at run time at `lib.rs:636-637` (`StackSizeExceeded`). SilverScript v1.0.0 (`refs/silverscript`, tag v1.0.0, `3ed97333`) enforces the same constant at compile time in two places: live bindings (`silverscript-lang/src/compiler/compile/statement.rs:337-345`, `VariableStackTooLarge`) and a bytecode stack simulation over both branches of every IF (`compile/stack_analysis.rs:138-170`, `BytecodeStackTooLarge`, "reaches N combined stack items").

**Method:** a temporary test (deleted after use) inserted P extra live `int` bindings at a marker line, consumed them in one `require` at the end of the entry, and recorded the largest P that compiles. Pads at a line count against every point after it, so "max P" is the free slots at the tightest point after that line (the `require(sum)` costs about one slot, so these are 1 below S3-6's way of counting: HEAD's 17 here = S3-6's "pad 18 compiles").

| Source | pads at check 2 / C1 / 18 | at 23 / 24 | at 25 | at 26 | tightest point |
|---|---|---|---|---|---|
| HEAD (`1abab20`, M4 drawer) | 17 / - / 17 | - | - | - | check 24 |
| Stage 3 (Stage 1 struct, before this stage) | 11 / - / 11 | 11 / 11 | 20 | - | check 24 (39-field controller' struct) |
| **Stage 4 (committed now)** | **4 / 4 / 4** | - | 13 | 18 | check 24 |

- The Stage 1 struct growth cost 6 slots at check 24 (3 of them are `vt`'s new template-field bindings, live at check 24; the other 3 were not traced further). Estimate before writing: 11 − 3 new arguments − 4 values that must live past check 24 (mintC, refund, outC, tA2) = 4. Measured after: 4. **Not the stop case** (the limit is not hit; no restructuring was done).
- What was done to stay inside: every C1/C2/C3 temporary lives in a braced block; `rec` is not stored but recomputed (`pend_dep > 0 || pend_wsh > 0`) at checks 13, 20 and 26; the record index in check 26 is check 25's `po` (= 3 + np).
- Headroom left for Stage 5+: 4 slots at check 24, so any later drawer change that adds a value live across check 24 must be re-measured (Stage 6 reviewer note).

### What was done (drawer.sil)

- Constants VS, VA, MIN_DEPOSIT, SHARES_MAX, ASSETS_MAX (spec A2, = `game.rs`); `RecordState` struct (A3.3); `function mulfrac(a, r, c)` (A8, the `game::mulfrac_steps` algorithm, bit value p from 2^62 halving, 63 unrolled steps).
- Arguments `int wc, byte[] record_prefix, byte[] record_suffix` before `own_daa` (signature = `SIG_DRAW_M5`).
- Check 5: total_assets ≤ ASSETS_MAX, total_shares ≤ SHARES_MAX, pend_wsh ≤ total_shares (A3.1). The other bounds and the value check are unchanged.
- Check 13: nCov += 1 iff rec; the `OpAuthOutputIdx` loop bound 10 → 11 (SilverScript's runtime loop-bound assert would otherwise reject nCov = 11, `docs/TUTORIAL.md` "For Loops").
- Check 14 (U-2 / O-S3-1): `require(hAmt <= ASSETS_MAX - vt.total_assets)`.
- C1, C2, C3 after check 17, as spec A5 (markers `// ==== check C1:` / `C2:` / `C3:`, top level).
- Check 20: out1 = total_assets' + R; the record at nCov − 1 = 3 + np has out_c + (D − D_c) + R; fee/refund indices follow the new nCov.
- Check 21: unchanged text; the zero-ID loop starts at the new nCov.
- Check 23: vault' {total_assets', S + mint_c − wc, pend_dep 0, W − wc, L', templates}. Check 24: v_rec = total_assets'.
- Check 26 (after 25): iff rec, `validateOutputStateWithTemplate(po, RecordState{0x08, round, vt.request_tpl, refund, D, mint_c, W, wc, out_c, funder_class, funder_key}, record_prefix, record_suffix, vt.record_tpl)`.

### Decisions

- **S4-1 Stack measurement** as above (method and numbers; temporary probe deleted).
- **S4-2 The V' + D ≤ TOTAL_MAX guard moved from check 20 to C2** (`require(vt.pend_dep <= TOTAL_MAX - vP)`, `M5C2_sum`). Check 20 no longer forms V' + D (vault' is total_assets' + R), and C2 needs the guard first to form V' + D_c. It bounds every later sum: tot, tA2 ≤ V' + D_c, and out_c + D ≤ V' + D in refund mode. Only fabricated states reach it (lineage keeps V' + D ≤ MAX_SOMPI); no attack targets it, and the model treats it as `convert`'s domain (S1-6).
- **S4-3 C1 decides before summing:** `frac <= room - whole` (room = SHARES_MAX − S, whole = D q ≤ room after the pre-guard), then mint = whole + frac. This is the spec's "mint ≤ SHARES_MAX − S" without forming a sum that could reach 2^63 − 1000. `q = 0 ∨ D ≤ room / q` uses nested ifs (no short-circuit is assumed; `room / q` is only formed when q > 0).
- **S4-4 mulfrac has no require of its own.** Its domain holds at all three call sites from earlier requires: a = D ≤ 2^62 (5), r = N % M < M, c = M = V' + 1 ≤ 2^62 (14) for C1; a = M ≤ 2^62, r = wc or wc + 1 ≤ W ≤ S < N = S + VS ≤ 2^62 (5, C2 range) for C2. Out of domain, an OpAdd would fail on overflow (checked 64-bit), not wrap.
- **S4-5 Values after C3 kept live:** mintC, refund, outC, tA2 (4 slots). rec is recomputed (S4-1). tA2 (vault' total_assets = v_rec) is computed once in C3.
- **S4-6 C2 is layered on V' ≥ 0:** with L' ≥ 0, `lNext <= V' + D_c` (`M5C2_slack`) and `out_c <= slack` (`M5C2_fits`) both reject V' < 0, so `D14_nonneg` is no longer the only check for it (mutant adjustment below).
- **S4-7 record_prefix / record_suffix are not length-checked**, as pool_prefix (M3 S3-7, Stage 3 S3-6): a wrong one fails the record template hash in check 26; they are unused when nothing is pending.
- **S4-8 Mutation tags (S2-8, prefix M5):** `M5D5_assets`, `M5D5_shares`, `M5D5_wsh`, `M5D13_rec` (an assignment), `M5D14_assets`, `M5C2_sum`, `M5C2_slack`, `M5C2_range` (×2 lines), `M5C2_fits`, `M5C2_max`, `M5D20_record`. Check 26 and the changed checks 23/24 are template bindings (look-alike controls in Stage 6). The changed check 20 vault line keeps its M3 tag `D20_vault`. The new guard `require(dRef <= TOTAL_MAX - outC)` in check 20 is untagged (provably redundant after S4-2, as M3's untagged guards).
- **S4-9 The S2-2 M4 branch in `draw.rs` is left in place** (`args_m4`, `SIG_DRAW_M4`, the 22-push path of `from_tx`). It is dead now (the compiled drawer has the M5 signature) and harmless; deleting it is a cleanup for Stage 6/8.

### Deviations from the spec

None in substance. S4-2 moves an existing guard between checks (the spec lists none for V' + D in A5; A5 says "the sum is guarded"). S4-3 is an equivalent rewrite of C1's mint bound.

### Adjusted M3/M4 tests

- `m3_mutants::d14_nonneg`: `killed(D, &["D14_nonneg"], vault_negative())` → `killed(D, &["D14_nonneg", "M5C2_slack", "M5C2_fits"], vault_negative())`. Reason S4-6: the attack (V' = −1, L' = 0) is now also rejected by the two new C2 requires; the old single-tag mutant was "still rejects: VerifyError". With the three removed it is accepted again (93/93). No other M3/M4 test changed; m4_mutants 211 pass.

### Status delta (normal mode)

| ID | Test fn | Stage 3 | Stage 4 | Reason now |
|---|---|---|---|---|
| VP4 | `vp4_draw_with_deposits` | FAIL | **pass** | |
| VP5 | `vp5_withdrawals_within_slack` | FAIL | **pass** | |
| VP6 | `vp6_cap_binds` | FAIL | **pass** | |
| VP10 | `vp10_nothing_pending` | pass | pass | |
| VP12 | `vp12_script_accepts_reference_grid` | FAIL | **pass** | |
| VP8, VP9 | `vp8_fairness`, `vp9_refund_mode` | FAIL | FAIL | their converting/refund draws now pass the real drawer; they stop at the first settlement (request placeholder, input 1 EarlyReturn, Stage 5) |
| VP7 (both), VP14 | | FAIL | FAIL | as VP8: draws pass, first settle EarlyReturn (Stage 5) |
| VP11 | `vp11_refresh_then_use` | FAIL | FAIL | refresh position (Stage 5) |
| VA5 | `va5_withdrawal_below_liability` | FAIL | **pass** | 4 strikes |
| VA6 | `va6_overflow_boundaries` | FAIL | **pass** | 10 strikes (drawer:5, drawer:14, two drawer:20 mint-guard cases) |
| VA8 | `va8_double_conversion` | FAIL | **pass** | 7 strikes |
| VA14 | `va14_j_independence` | FAIL | **pass** | 2 strikes |
| VA1 | `va1_zero_share_deposit` | FAIL | FAIL | draw strike passes (attribution OK, drawer:20); stops at "settle: sh = 0" (Stage 5) |
| VA7 | `va7_stale_or_future_price` | FAIL | FAIL | all 11 drawer strikes pass (attribution below); stops at "settlement sh + 1" (Stage 5) |
| VA15 | `va15_leader_budget_minus_one` | FAIL | FAIL | "draw with conversion" budget case now passes; stops at "settle deposit-convert" (Stage 5) |
| VA2, VA3, VA4 (both), VA11, VA12, VA13 | | FAIL | FAIL | request, record or refresh placeholders (Stage 5) |

m5_vault 7 pass / 6 fail; m5_attacks 9 pass / 10 fail. Every remaining failure is at a Stage 5 placeholder (settle, record leg, refresh).

### Preflight and attribution

- `M5_PREFLIGHT=1 ... --test m5_attacks`: `test result: ok. 19 passed; 0 failed`; 115 tolerated inputs, all settle/record/refresh, **0 at the drawer** (no VerifyError tolerated). `--test m5_vault`: `ok. 13 passed; 0 failed`; 35 tolerated, all settle/refresh. So the real drawer and the model agree at input 0 of every converting, refund-mode and capped draw in every chain (VP4-VP9, VP12, VP14) and every drawer strike.
- `M5_PREFLIGHT=1 M5_ATTRIB=1 ... --test m5_attacks`: **106 OK, 0 MISMATCH, 5 NO MARKERS** (the 5 are Stage 5 files, unchanged). Drawer strikes (29, all OK, got = expected = model): drawer:5 ×1, :13 ×4, :14 ×1, :17 ×2, :C2 ×3, :20 ×7, :23 ×5, :26 ×6.

### Negative controls

1. `require(outC <= slack); // mut:M5C2_fits` removed: `ATTACK ACCEPTED (VA5 wc + 1 (vault' total_assets < L')) by input 0`. Restored.
2. Check 26 disabled (its condition changed to `vt.pend_dep < 0`): `ATTACK ACCEPTED (VA7 record mint_rem + 1) by input 0`. Restored (drawer.sil byte-identical to the committed-stage copy afterwards).

### Draw mass sanity (for Stage 6; not the M5 measurements)

m3_measure 7/7 and m4_measure 8/8 pass (release). Nothing pending (no conversion runs), so these show the script-size cost only:
- drawer redeem 45,466 B (M4) → **65,234 B** (+19,768 B; mostly the three inlined mulfrac calls, about 6 kB each);
- DP21 worst (K 16, m 50, a 7, np 7, F_MAX): 34.0 / 43.7 / 14.2 % (M4) → **41.4 / 52.7 / 14.2 %** (compute 206,792, transient 526,768, storage 71,009);
- `close()` K = 16 with the new marker: 58.9 / 86.8 / 80.5 % (unchanged: the marker redeem keeps its 2,104 B).
- The conversion's run-time cost (3 × 63 steps) and the record output come on top in M5-b; if M5-b is tight, one option for the user is a single mulfrac pass that returns out(wc) and its remainder, so out(wc + 1) = out(wc) + floor((X + M) / N) without a third call.

### Open items

- **O-S4-1 (Stage 6):** M5-b/M5-c with conversion and the record on the 65 kB drawer (D78a); M5-d `close()` K = 16 (same 86.8 % now).
- **O-S4-2 (Stage 6):** mutants for the S4-8 tags and look-alike controls for check 26 (record_tpl from vault state) and the changed 23/24; coverage pin.
- **O-S4-3:** drawer headroom is 4 stack slots at check 24 (S4-1); re-measure after any drawer change.
- **O-S4-4:** delete the dead M4 draw branch (S4-9) in a cleanup stage.
- Unchanged: O-S2-5, O-S2-6, O-S3-2 (vault redeem growth), O-S3-3, O-S3-4.

### Final `cargo test` (from `crates/`, `-j 2 --no-fail-fast`, debug)

The run below was before the `d14_nonneg` adjustment; `cargo test -j 2 -p lottery --test m3_mutants` afterwards: `test result: ok. 93 passed; 0 failed`. Failing binaries are only m5_attacks and m5_vault (Stage 5).

```
unit (harness)        ok. 0 passed; 0 failed
covenant_smoke        ok. 3 passed; 0 failed
m0_limits             ok. 3 passed; 0 failed
m0_mass               ok. 5 passed; 0 failed
m0_merkle             ok. 2 passed; 0 failed
m0_shapes             ok. 6 passed; 0 failed
unit (lottery)        ok. 31 passed; 0 failed
m1_attacks            ok. 25 passed; 0 failed
m1_buy                ok. 14 passed; 0 failed
m1_measure            ok. 4 passed; 0 failed
m1_mutants            ok. 11 passed; 0 failed
m2_attacks            ok. 40 passed; 0 failed
m2_close              ok. 8 passed; 0 failed
m2_game               ok. 2 passed; 0 failed
m2_measure            ok. 5 passed; 0 failed
m2_mutants            ok. 25 passed; 0 failed
m2_params             ok. 17 passed; 0 failed
m3_attacks            ok. 22 passed; 0 failed
m3_draw               ok. 24 passed; 0 failed
m3_game               ok. 13 passed; 0 failed
m3_measure            ok. 7 passed; 0 failed
m3_mutants            FAILED. 92 passed; 1 failed (d14_nonneg; fixed, rerun 93 passed)
m4_attacks            ok. 21 passed; 0 failed
m4_claim              ok. 25 passed; 0 failed
m4_game               ok. 4 passed; 0 failed
m4_measure            ok. 8 passed; 0 failed
m4_mutants            ok. 211 passed; 0 failed; 1 ignored
m5_attacks            FAILED. 9 passed; 10 failed
m5_game               ok. 11 passed; 0 failed
m5_vault              FAILED. 7 passed; 6 failed
doc-tests (x2)        ok. 0 passed; 0 failed
```

### Notes for Stage 5

- The drawer writes the record at 3 + np with `{0x08, round r, vt.request_tpl, refund, D, mint_c, W, wc, out_c, funder}` and value out_c + (D − D_c) + R; the settle model already matches (preflight).
- Every chain positive now stops at its first `settle` (input 1 record / input 0 request EarlyReturn).

## Stage 5: settlement and refresh (settle, request_leg, refresh x3)

Branch `m4-claim`, nothing committed.

Changed:
- `covenants/request.sil`, `covenants/record.sil`, `covenants/position.sil`;
- `crates/lottery/tests/m5_game.rs` (the placeholder test and the imports only it used, deleted);
- `crates/lottery/tests/m5_attacks.rs` (VA15, S5-3).

No Rust builder or model change was needed: the scripts follow the S2-5 model and `game::settle`, and the preflight shows that they agree at every input.

### What was done

- **`request.sil`**: the placeholder `locked()` is replaced by two entries.
  - `settle(int rec_pl, int rec_sl, byte[] pos_prefix, byte[] pos_suffix)` (the U-1 signature), with markers S1..S13 in spec order (A6.1 checks 1-13).
  - `refresh()`, with markers A7.1..A7.7. It is the M4 A6 code (D59, D76): every field and the value stay unchanged.
  - Also added:
    - the constants TOTAL_MAX, SHARES_MAX, F_MAX, RESERVE_MAX, MIN_DEPOSIT, P_MIN, SCRIPT_MAX, D_IDLE and DAA_MAX (the spec A2 / M4 values, equal to `game.rs`);
    - the structs `RecordState` (A3.3) and `PositionState` (A3.4);
    - `payoutSpk` (the drawer/ticket/vault code);
    - `mulfrac` (the `Drawer.mulfrac` code, A8).
- **`record.sil`**:
  - `request_leg(int req_pl, int req_sl)`, checks RL1..RL4:
    - activeInputIndex = 1;
    - OpInputCovenantId(0) = the record's own ID;
    - OpAuthOutputCount(1) = 0;
    - input 0, read under the record's own `request_tpl`, has role 0x07 and round = the record's round.
  - `refresh()` (A7.1..A7.7).
- **`position.sil`**: `refresh()` (A7.1..A7.7) after `vault_leg`, and the constants F_MAX, D_IDLE and DAA_MAX.
- **`m5_game.rs`**: `stage1_placeholders_reject` is deleted, which closes O-S3-3.

### `settle` as written (A6.1)

- **S1:**
  - leader at input 0; role 0x07; 1 <= round < TOTAL_MAX; kind <= 1;
  - amount in [MIN_DEPOSIT, TOTAL_MAX] for a deposit, or [1, SHARES_MAX] for a withdrawal;
  - owner class <= 2 with the D11 padding;
  - in0 value in [0, TOTAL_MAX] (U-3);
  - rec_pl and rec_sl in [0, SCRIPT_MAX].
- **S2:**
  - OpCovInputCount(cov) = 2 and OpInputCovenantId(1) = cov;
  - 3 <= inputs <= 2 + F_MAX;
  - inputs >= 2 have the zero ID.
- **S3:** the record, read under RECORD_TPL.
  - Role 0x08.
  - Every A3.3 range:
    - round;
    - refund <= 1;
    - each _rem in [0, TOTAL_MAX];
    - mint_rem <= SHARES_MAX;
    - burn_rem <= wsh_rem;
    - refund = 1 ⇒ mint_rem = 0;
    - funder_class <= 2.
  - in1 = out_rem + refund × dep_rem + R_rc, with R_rc in [0, RESERVE_MAX].
  - **rc.round = round.**
- **S4:**
  - The side is the deposits (mint_rem over dep_rem) or the withdrawals (burn_rem over wsh_rem).
  - `require(s <= side)`.
  - part = the side's remainder if s = side, else mulfrac(remainder, s, side).
  - Deposit-convert: sh = part and `require(sh >= 1)`. Refund: sh = 0.
  - Withdrawal: conv = part. pay = 0 if burn_rem = 0, out_rem if conv = burn_rem, else mulfrac(out_rem, conv, burn_rem) (the S1-8 order). left = s − conv.
- **S5, S6:** compute record's remainders, drained and the mode. They contain no require, as C1 and C3 in the drawer.
- **S7:**
  - nCov = [not drained] + [convert, PAY or ACCRUE];
  - OpAuthOutputCount(0) = nCov;
  - OpAuthOutputIdx(0, j) = j;
  - OpCovOutputCount(cov) = nCov.
- **S8:**
  - outputs in [base, base + 1], with base = nCov + [payout] + [drained];
  - outputs >= nCov have the zero ID.
- **S9:** values.
  - record' = out' + refund × dep' + R_rc.
  - The position, or request' in PAY, = in0.
  - request' in ACCRUE = in0 + pay (guarded).
  - The payout:
    - refund: s + in0 (guarded);
    - FINAL: pay + in0 (guarded);
    - PAY: pay.
  - The funder refund = R_rc.
- **S10:** the payout goes to payoutSpk(owner); the funder refund goes to payoutSpk(rc.funder).
- **S11:**
  - record' is validated under input 1's template (validateOutputStateWithInputTemplate, RECORD_TPL);
  - it has the new remainders, and every other field from rc.
- **S12** (deposit-convert): the position {0x0B, rc.round, sh, owner} under POSITION_TPL, with pos_prefix and pos_suffix.
- **S13** (PAY or ACCRUE): request' {round + 1, kind, left, owner}, by validateOutputState (the request's own template).

### Decisions

- **S5-1 One mulfrac call for sh and conv.**
  - Deposits and withdrawals select their side (a, c) first and share one call `mulfrac(a, s, c)`; the pay call is the second.
  - So the request has two inlined copies of mulfrac, not three (each is about 6 kB in the drawer: Stage 4 mass note).
  - Domains:
    - first call: a <= TOTAL_MAX (S3), and r = s < c <= TOTAL_MAX (the call runs only when s < side);
    - pay call: r = conv < c = burn_rem (the call runs only when burn_rem > 0 and conv < burn_rem).
  - In refund mode mint_rem = 0 (S3), so part = 0 and sh = 0, as the spec requires. sh is still set explicitly only in convert mode.
- **S5-2 No subtraction before its comparison (A8).**
  - R_rc is formed in three steps: `v1 >= out_rem`, then `v1 - out_rem >= held`, then `v1 - out_rem - held`. This is the spec's "in1 = out_rem + refund × dep_rem + R_rc", but no value near −2^63 is ever formed.
  - Every record' remainder is a difference taken after S4's bounds: s <= side, sh <= mint_rem, conv <= burn_rem and pay <= out_rem. Each part is at most its remainder, because mulfrac(a, r, c) <= a.
  - The record' value out' + held' + R_rc is <= in1 < 2^63.
- **S5-3 VA15 (a test change): the position and record refreshes commit budget 0.**
  - Their refresh runs use 8,044 and 8,651 script units.
  - That is under the free per-input allowance of SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT − 1 = 9,999 units (rusty-kaspa v2.1.0 `consensus/core/src/mass/units.rs:4-24`, applied at `consensus/core/src/tx.rs:102`).
  - The committed budget is therefore 0, and "budget − 1" does not exist (`budget_delta` clamps at 0). The old test panicked with "under-budget leader accepted".
  - `va15_leader_budget_minus_one` now prints and skips exactly the cases whose committed budget is 0, naming them. Every other case still requires `ExceededCommittedScriptUnits`, and the test's panic message names the case.
  - The request refresh and every settle mode commit a budget of at least 1, and are still tested.
- **S5-4 Mutation tags (S2-8, prefix M5).** One statement per tagged line.
  - request: `M5S1_*` .. `M5S10_*`. S11-S13 are template bindings, left for the Stage 6 look-alike controls.
  - request refresh: `M5RQ_RF*`.
  - record: `M5RL1` .. `M5RL4_*`, and refresh `M5RC_RF*`.
  - position refresh: `M5PO_RF*`.
  - The refresh tags are the M4 RF names with an object prefix, so no tag collides across files. The position's `PO*` tags are unchanged.
- **S5-5 Loop bounds.**
  - S7's `OpAuthOutputIdx` loop runs nCov <= 2 times (maximum 2).
  - S8's zero-ID loop runs outputs − nCov <= base + 1 − nCov <= 3 times (maximum 3).
  - SilverScript asserts the bound at run time (`docs/TUTORIAL.md`, "For Loops"), so a larger count would reject. It cannot occur: S8's count check runs first, in the same check.
- **S5-6 The funder key padding is not checked.**
  - A3.3 lists only funder_class <= 2; the drawer writes the funder from its arguments.
  - payoutSpk uses key bytes 0..31 for classes 0 and 2, and all 33 bytes for class 1. So the refund script is fixed either way.
- **S5-7 The record leg authenticates the leader by the record's own `request_tpl`** (state, D74), as the model does.
  - The request's own entries are `settle`, which needs the record at input 1 under the same ID, and `refresh`, which needs OpCovInputCount = 1.
  - So a record can only be spent by its own refresh, or by the settle of a request of the same round.

### Soundness notes for the reviewer (the Stage 5 hunt list)

- **Settlers cannot take more than the record holds.**
  - Each part is at most its remainder: mulfrac(a, r, c) <= a for r < c, and the exact-remainder branch takes exactly the remainder.
  - record' carries remainder − part. So Σ sh <= mint_c and Σ pay <= out_c, and the last settler of a side takes exactly what is left (VP12 / A6.2; VP7 in two orders).
- **The record is never left undrainable.**
  - With mint >= ceil(D / MIN_DEPOSIT) and every s >= MIN_DEPOSIT, the invariant mint_rem >= ceil(dep_rem / MIN_DEPOSIT) is preserved:
    - floor(mint_rem × s / dep_rem) >= floor(s / MIN_DEPOSIT) >= 1;
    - mint' >= mint_rem × (dep_rem − s) / dep_rem >= dep' / MIN_DEPOSIT.
  - So `require(sh >= 1)` never rejects a deposit that lineage can produce.
  - The withdrawal side has no rejecting amount check, and mulfrac is always called inside its domain.
  - The guarded sums cannot bind for lineage values, which are at most MAX_SOMPI.
  - Every payout and refund output is at least R >= R_MIN > 0, or at least P_MIN in PAY.
- **A request of another round or kind.**
  - S3 (rc.round = round) and RL4 (the leader's round = the record's round) both reject another round.
  - A deposit against a record with only withdrawals fails s <= dep_rem = 0, and the reverse fails the same way.
- **Double settlement:** the request is consumed. request' has round + 1, so it fails S3 and RL4 against the same record (both VA4 tests).
- **Refund mode pays exactly D:** the payout is s + in0, and record' keeps dep_rem − s in its value (VP9; VA11 payout ± 1).
- **Deposit-convert cannot mint more than the record allots:**
  - S12 binds the position's shares to sh (POSITION_TPL);
  - S11 binds record's mint_rem to mint_rem − sh (VA7 sh ± 1, VA11 shares ± 1).

### Deviations from the spec

None in substance.
- S5-1 and S5-2 are equivalent rewrites of A6.1 checks 3-4.
- S5-3 changes a test, not the spec: VA15's "leader budget − 1" has no meaning for a leader that commits budget 0.

### Status (normal mode): every VP and VA is green

| ID | Test fn | Stage 4 | Stage 5 |
|---|---|---|---|
| VP1 | m5_game `vp1_*` | pass | pass |
| VP2, VP3 | `vp2_request_deposit`, `vp3_request_withdraw` | pass | pass |
| VP4, VP5, VP6, VP10 | | pass | pass |
| VP7 | `vp7_deposit_drain_two_orders`, `vp7_withdrawal_modes` | FAIL | **pass** |
| VP8 | `vp8_fairness` | FAIL | **pass** |
| VP9 | `vp9_refund_mode` | FAIL | **pass** |
| VP11 | `vp11_refresh_then_use` | FAIL | **pass** |
| VP12 | m5_game (Rust) and `vp12_script_accepts_reference_grid` | pass | pass |
| VP13 | `check_*` in every positive | - | asserted in every positive above |
| VP14 | `vp14_three_draws` | FAIL | **pass** |
| VA1 | `va1_zero_share_deposit` | FAIL | **pass** |
| VA2 | `va2_cancelling` | FAIL | **pass** |
| VA3 | `va3_wrong_draws_record` | FAIL | **pass** |
| VA4 | `va4_second_settlement`, `va4_second_settlement_ledger` | FAIL | **pass** |
| VA5, VA6, VA8, VA9, VA10, VA14 | | pass | pass |
| VA7 | `va7_stale_or_future_price` | FAIL | **pass** |
| VA11 | `va11_settlement_outputs` | FAIL | **pass** |
| VA12 | `va12_record_leg` | FAIL | **pass** |
| VA13 | `va13_refresh` | FAIL | **pass** |
| VA15 | `va15_leader_budget_minus_one` | FAIL | **pass** (S5-3) |

Results:
- m5_vault: 13/13;
- m5_attacks: 19/19;
- m5_game: 10/10 (11 minus the deleted placeholder test).

### Preflight and attribution

Command: `M5_PREFLIGHT=1 M5_ATTRIB=1 cargo test -j 2 -p lottery --release --no-fail-fast --test m5_attacks --test m5_vault -- --nocapture --test-threads=1`.

- m5_attacks: `test result: ok. 19 passed; 0 failed`. m5_vault: `test result: ok. 13 passed; 0 failed`.
- **0 `PREFLIGHT accept` lines.** No input anywhere is tolerated as "not yet written". The script and the model agree at every modelled input of every positive and every strike.
- 212 `PREFLIGHT ... | OK` strike lines; 0 MISMATCH, 0 DISAGREES.
- **ATTRIB: 212 OK, 0 MISMATCH, 0 NO MARKERS.**
  - By object: vault 71, request 70, drawer 29, record 22, position 20.
  - Stage 5 labels:

| Object | Check counts |
|---|---|
| request | S1 5, S2 4, S3 7, S4 3, S7 4, S8 3, S9 11, S10 3, S11 7, S12 6, S13 3 |
| request refresh | A7.1 1, A7.2 3, A7.3 3, A7.4 2, A7.5 2, A7.6 2, A7.7 1 |
| record | RL1 2, RL2 1, RL3 1, RL4 4; refresh A7.1-A7.7 as the request's (1/3/3/2/2/2/1) |
| position | PO1 2, PO2 1, PO3 1, PO4 2; refresh A7.1-A7.7 (1/3/3/2/2/2/1) |

### Negative controls

Each was restored afterwards and checked with `cmp` against a saved copy.

1. request.sil, `require(rc.round == round); // mut:M5S3_draw` removed: `ATTACK ACCEPTED (VA3 request round t against the record of t - 1) by input 0`.
2. request.sil, `require(sh >= 1); // mut:M5S4_sh` removed: `ATTACK ACCEPTED (VA1 settle: a fabricated record where sh = 0) by input 0`.
3. record.sil, `require(rq.round == round); // mut:M5RL4_round` removed: `ATTACK ACCEPTED (VA3 the record of t + 1 at the record leg (RL4: round = own round)) by input 1`.
4. position.sil, `require(tx.daa >= creation + D_IDLE); // mut:M5PO_RF3_idle` removed: `ATTACK ACCEPTED (VA13 position: lock_time = creation + D_IDLE - 1) by input 0`.

### Open items for Stages 6-8

- **O-S5-1 (Stage 6): mutants.**
  - Mutants for the S5-4 tags: request S1-S10, RL1-RL4 and the three refreshes, with a coverage pin.
  - Look-alike template controls for:
    - S11 (record', via input 1's template);
    - S12 (POSITION_TPL);
    - S13 (the own template);
    - RL4 (request_tpl from the record's state).
- **O-S5-2 (Stage 6): M5-a and M5-e.**
  - The request redeem now carries two inlined copies of mulfrac (S5-1), and it is part of every settle and every request refresh.
  - Measure its size, the settle masses per mode (drained or not), and the three refreshes.
- **O-S5-3 (Stage 8, spec Corrections):**
  - A10 VA15, "leader budget − 1 for each new entry", does not apply to the position and record refreshes, which commit budget 0 (S5-3);
  - A6.1 checks 3-4 as written in S5-1 and S5-2 (equivalent forms);
  - U-3 (the request's value range) is implemented in S1.
- **O-S5-4 (Stage 8):** the A6.1 settlement table shows "position (sh, R)" and "payout s + R". The script uses in0.value (the request's value) for R, which equals R by lineage (D68). This is worth one sentence in the Corrections.
- Unchanged:
  - O-S2-5, O-S3-2, O-S3-4, O-S4-1, O-S4-2, O-S4-4;
  - O-S4-3: the drawer is untouched in this stage, so its 4-slot headroom is unchanged.

### Final `cargo test` (from `crates/`, `cargo test -j 2`, debug, without `--no-fail-fast`)

Exit code 0. Every binary is green, M0-M5. The seed gate `--features mainnet,test-seed` still fails with `TEST-ONLY seed cannot be built for mainnet`; `--features mainnet` builds.

```
src\lib test result: ok. 0 passed; 0 failed; 0 ignored; finished in 0.00s
covenant_smoke test result: ok. 3 passed; 0 failed; 0 ignored; finished in 0.01s
m0_limits test result: ok. 3 passed; 0 failed; 0 ignored; finished in 0.13s
m0_mass test result: ok. 5 passed; 0 failed; 0 ignored; finished in 65.41s
m0_merkle test result: ok. 2 passed; 0 failed; 0 ignored; finished in 2.21s
m0_shapes test result: ok. 6 passed; 0 failed; 0 ignored; finished in 1.62s
src\lib test result: ok. 31 passed; 0 failed; 0 ignored; finished in 11.09s
m1_attacks test result: ok. 25 passed; 0 failed; 0 ignored; finished in 17.11s
m1_buy test result: ok. 14 passed; 0 failed; 0 ignored; finished in 9.29s
m1_measure test result: ok. 4 passed; 0 failed; 0 ignored; finished in 50.89s
m1_mutants test result: ok. 11 passed; 0 failed; 0 ignored; finished in 58.10s
m2_attacks test result: ok. 40 passed; 0 failed; 0 ignored; finished in 20.08s
m2_close test result: ok. 8 passed; 0 failed; 0 ignored; finished in 7.78s
m2_game test result: ok. 2 passed; 0 failed; 0 ignored; finished in 32.13s
m2_measure test result: ok. 5 passed; 0 failed; 0 ignored; finished in 134.19s
m2_mutants test result: ok. 25 passed; 0 failed; 0 ignored; finished in 31.12s
m2_params test result: ok. 17 passed; 0 failed; 0 ignored; finished in 11.76s
m3_attacks test result: ok. 22 passed; 0 failed; 0 ignored; finished in 34.19s
m3_draw test result: ok. 24 passed; 0 failed; 0 ignored; finished in 20.13s
m3_game test result: ok. 13 passed; 0 failed; 0 ignored; finished in 0.98s
m3_measure test result: ok. 7 passed; 0 failed; 0 ignored; finished in 33.18s
m3_mutants test result: ok. 93 passed; 0 failed; 0 ignored; finished in 83.99s
m4_attacks test result: ok. 21 passed; 0 failed; 0 ignored; finished in 33.33s
m4_claim test result: ok. 25 passed; 0 failed; 0 ignored; finished in 17.44s
m4_game test result: ok. 4 passed; 0 failed; 0 ignored; finished in 5.77s
m4_measure test result: ok. 8 passed; 0 failed; 0 ignored; finished in 59.03s
m4_mutants test result: ok. 211 passed; 0 failed; 1 ignored; finished in 172.98s
m5_attacks test result: ok. 19 passed; 0 failed; 0 ignored; finished in 37.65s
m5_game test result: ok. 10 passed; 0 failed; 0 ignored; finished in 7.23s
m5_vault test result: ok. 13 passed; 0 failed; 0 ignored; finished in 14.86s
doc-tests harness test result: ok. 0 passed; 0 failed; 0 ignored; finished in 0.00s
doc-tests lottery test result: ok. 0 passed; 0 failed; 0 ignored; finished in 0.00s
```

The first `src\lib.rs` line is harness and the second is lottery.

## Stage 6: mutants, measurements, attribution, tn10 probe

Branch `m4-claim`, nothing committed. No covenant source was changed in this stage (no surviving mutant needed a fix).

Changed or new:
- new `crates/lottery/tests/m5_mutants.rs` (251 tests), `crates/lottery/tests/m5_measure.rs` (5 tests);
- `crates/lottery/tests/common/m5.rs`: `stage6()` (98 boundary and shape attacks, in `all_atks5`), three more VA13 attacks per object (covenant-less object, nine funding inputs, no output), `fund_dep` (replaces `funded_dep`, S6-3), helpers `role_at`, `vault_role`, `lower_in_out`, `st_case`;
- `crates/lottery/tests/m5_attacks.rs`: test `stage6_boundaries_and_shapes` (runs `stage6()`);
- `crates/tn10/src/cases.rs` (`round1_drawn_with`, `VaultRequests`, `vault_m5`), `crates/tn10/src/main.rs` (`probe vault-m5`).

A temporary discovery test (for every M5 tag line, which attacks the single-line mutant accepts) chose the attacks; it was deleted afterwards.

### Mutants (O-S3-4, O-S4-2, O-S5-1)

Tag set: **153 tags, 212 tagged lines** (vault.sil and drawer.sil: tags `M5*`; request.sil, record.sil, position.sil: all). `every_m5_tag_is_tested` pins both numbers and checks both directions: every tag line is named by a test, and every `"file:TAG[#n]"` named in m5_mutants.rs is a tag line of that file.

`cargo test -p lottery --release --test m5_mutants -- --test-threads=3`: **`test result: ok. 251 passed; 0 failed`** (159 s).

| Kind | Tests |
|---|---|
| killed (single line or tag) | 125 top-level + 30 refresh (10 x 3 objects) |
| redundant (the implying check in the comment) | 59 top-level + 27 refresh (9 x 3) |
| killed_positive (assignment `M5D13_rec`: the mutant rejects a valid converting draw) | 1 |
| look-alike template controls | 8 tests covering D8, D9, W3, W9, W10, W11, PO4, S3, S11, S12, S13, RL4, drawer 26 |
| coverage pin | 1 |

Redundancy classes (each run with the attack the removed line would otherwise catch; the mutant still rejects it at the named input):
- template lengths (`*_tpl` of W1, S1, RL4, PO4; 16 lines): the template hash commits the lengths (as M4 K1_tpl);
- loop bounds (`*_fund#1`, `D4_count#1`, `W7_count#1`, refresh `RF2_fund#1`, `RF5_count#1`): the compiler's `for` bound;
- pigeonhole groups (D2_count; W2_count / W2_pos; S2_count / S2_rec; refresh RF2_count) and output-binding triples (D3, W6, S7, refresh RF4): each implied by the others plus the zero-ID loop; every group is also **killed** when removed together;
- implied ranges: `total_shares >= 0` by `0 <= pend_wsh <= total_shares` (D1, W1); `wsh_rem >= 0` by `0 <= burn_rem <= wsh_rem`; `W1_wsh#1` and `W3_shares#0` by W4; deposit `D1_assets#1` and `D1_dep#1` by D5; `R >= 0` (D1_rsv#0, W1_rsv#0) by an output or input value equal to R; `S3_value#0` by `S3_value#1`; drawer `M5D5_assets` by check 14 (removed together with `M5D14_assets`: killed); `M5C2_range#0` by `M5C2_max`; `W5_idx#1`, `D4_count#0`, `W7_count#0`, `S8_count#0`, refresh `RF5_count#0`: index errors;
- round pairs: `S1_round#k` and `S3_round#k` imply each other through `rc.round = round`; each pair removed together is killed;
- equivalent (no transaction tells them apart; in `EQUIVALENT` with the proof, also run as `redundant`): `request:M5S1_value#0` (OpTxInputAmount cannot push a negative value, rusty-kaspa v2.1.0 `crypto/txscript/src/opcodes/mod.rs:1100-1107`); `drawer:M5C2_slack` (check 17 gives L' <= muldiv(V', cap) <= V' for V' >= 0; V' < 0 is killed with D14_nonneg and M5C2_fits removed together in m3_mutants.rs `d14_nonneg`, S4-6).

**Survivors: none.** Every mutant without a killing attack in VA1-VA14 lacked a boundary attack; there was no script gap. The 98 `stage6()` attacks were added for them. Each strikes with the committed scripts and attributes to its expected check (below). Notable kill shapes:
- `M5W5_idx#0` (owner_idx >= 2): a fabricated position owned by the P2SH script whose hash is the vault's own redeem hash. With owner_idx = 0 the vault input's script equals the owner's script, so only `owner_idx >= 2` rejects it. The line is load-bearing.
- `M5C2_sum`: V + H = ASSETS_MAX, D = TOTAL_MAX - V (engine only): only C2's sum guard rejects.
- Class-3 owners and funders are paid to the P2SH form (payoutSpk's default branch), as M4 "paid to its P2SH form".

### Measurements (`m5_measure.rs`, release; limits verified in `refs/rusty-kaspa-v2.1.0` at 01b532e8)

Limits: compute 500,000, storage 500,000, transient 1,000,000 (`consensus/core/src/config/params.rs:626`, `:683`), checked per dimension at `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:88-96`; KIP-9 C = 10^12 (`consensus/core/src/constants.rs:26`).

**M5-a** redeem sizes (B): vault 3,316; request 15,473 (2 mulfrac copies); record 1,154; position 1,194; drawer 65,234 (3 copies); marker 2,104. **One inlined mulfrac call = 6,117 B (drawer) / 6,118 B (request)**, measured as the code-size difference to a variant with one call replaced by a plain expression.

**M5-b** DP21 (K = 16, m = 50, a = A_MAX = 7) with conversion and the record (D78a), compute / transient / storage %:

| pending | np 5, 1 funding | np 5, F_MAX | np 7, 1 | np 7, F_MAX |
|---|---|---|---|---|
| nothing | 39.7 / 52.7 / 14.2 | 41.3 / 53.0 / 14.2 | 40.0 / 52.7 / 14.2 | 41.6 / 53.1 / 14.2 |
| both sides converted (2 mulfrac runs) | 39.8 / 52.7 / 14.5 | 41.4 / 53.0 / 14.5 | 40.1 / 52.8 / 14.5 | 41.7 / 53.1 / 14.5 |
| cap binding, wc < W (3 runs) | 39.8 / 52.7 / 14.2 | 41.4 / 53.0 / 14.2 | 40.1 / 52.8 / 14.2 | **41.7 / 53.1** / 14.2 |
| refund branch + cap binding (3 runs) | 39.8 / 52.7 / 14.2 | 41.4 / 53.0 / 14.2 | 40.1 / 52.8 / 14.2 | 41.7 / 53.1 / 14.2 |

Worst: compute 208,454 (41.7 %), transient 531,136 (53.1 %), storage 72,366 (14.5 %). The conversion's run time adds at most about 24,000 drawer script units (1,783,701 -> 1,807,553), about 700 compute grams. The record arguments add 3-6 sigscript bytes. **Below 90 %: no D78a stop.** The "drop the third mulfrac" option is not needed and was not done.

**M5-c** draw() storage with the record at exactly R (W = 1 share at the smallest V': wc = 1, out_c = 0). Supremum = 11 x 4C/R + (1 + a) C/R + C/f_min + C/caller_min:
- f_min = F_MIN = 1 KAS: measured max 317,481 (63.5 %), supremum 327,684 (**65.5 %**);
- f_min = F_MIN_FLOOR = 0.1 KAS: measured max 407,481 (81.5 %), supremum 417,684 (**83.5 %**; M3 without the record: 78.9 %).

Below 90 %: no stop. One such draw is also run through every script and accepted.

**M5-d** close() K = 16 with the M5 marker (redeem 2,104 B, size unchanged): 58.9 / **86.8** / 80.5 %, the same as M4 (only the marker's DRAWER_TPL bytes changed). Below 90 %: no D78b stop.

**M5-e** (one funding input, change; covenant outputs at R), compute / transient / storage %, all <= 9.7 %:

| transaction | leader units | % c / t / s |
|---|---|---|
| request_deposit | 177,393 | 4.6 / 7.7 / 5.0 |
| request_withdraw whole / partial (Schnorr; ECDSA and P2SH within 0.1) | 186,601 / 198,924 | 4.9 / 8.2 / 4.9; 5.0 / 8.2 / 9.7 |
| settle deposit-convert / drained | 67,246 / 52,963 | 4.4 / 7.3 / 4.0; 4.4 / 7.3 / 2.4 |
| settle deposit-refund / drained | 56,126 / 42,397 | 4.4 / 7.3 / 0.0; 4.4 / 7.3 / 0.0 |
| settle FINAL / drained | 59,805 / 42,430 | 4.4 / 7.3 / 0.0; 4.4 / 7.3 / 0.0 |
| settle PAY / drained | 121,786 / 104,920 | 4.6 / 7.4 / 4.3; 4.6 / 7.3 / 4.5 |
| settle ACCRUE / drained | 121,278 / 104,433 | 4.5 / 7.3 / 5.6; 4.5 / 7.3 / 2.3 |
| refresh position / request / record | 8,044 / 93,773 / 8,651 | 0.7 / 0.6 / 3.4; 3.7 / 6.4 / 3.4; 0.7 / 0.6 / 0.0 |

The record leg runs 111,160 units in every settle (it reads the request). The position leg runs 25,944 in a withdrawal.

### Attribution

`M5_ATTRIB=1 cargo test -p lottery --release --test m5_attacks -- --nocapture --test-threads=1`: `test result: ok. 20 passed; 0 failed`; **319 ATTRIB lines: 319 OK, 0 MISMATCH, 0 NO MARKERS** (212 from Stage 5, 9 new VA13, 98 `stage6()`).

### tn10 `probe vault-m5` (M5-f): PENDING

The probe is written and builds. Its sequence: genesis -> `request_deposit()` (D = 2 KAS) -> `request_withdraw()` (partial, 1 % of the deployer position, P2PK Schnorr owner input) -> buy -> close -> `draw()` with the conversion and the record -> deposit `settle()` (deposit-convert, record' kept) -> withdrawal `settle()` (FINAL, drained, R refunded). Every step uses the real scripts, is checked in the local engine first, and spends chain-merged node entries (`merged`). `round1_drawn` is now `round1_drawn_with(.., None)`; `draw-m3` and `claim-m4` behave as before.

`.env` has `TN10_PRIVATE_KEY`, but `cargo run -p tn10 -- info` failed three times with `wRPC -> WebSocket -> Connection timeout` (public resolver, no `TN10_WRPC_URL`). No node was reachable from this session, so the balance could not be checked and the probe was **not run**. `docs/tn10-mass-check.md` is not updated. To run it: `cargo run -p tn10 -- probe vault-m5` (about 3 minutes of DAA waits).

### Decisions

- **S6-1 Boundary attacks as data.** The 98 mutant-isolating attacks are in `common::m5::stage6()` and part of `all_atks5()` (unique ids, well-formedness, attribution). They run as `stage6_boundaries_and_shapes` in m5_attacks.rs, so each is a real strike, not only a mutant input. Ids carry their spec group (VA5, VA6, VA8-VA13).
- **S6-2 Engine-only attacks** (values above MAX_SOMPI, as M4 S4-3) cover the value guards: D5_max, W1_assets#1, W1_dep#1, S1_value#1, S3_out#1, S9_*#0, M5D14_assets, M5C2_sum, M5D5_assets. These lines are consensus-equivalent and killed in the engine.
- **S6-3 `funded_dep` -> `fund_dep`** (test helper). The old helper replaced the whole case with a committed-template deposit, so under a mutant the vault's request_tpl was stale and the attack was rejected at D9 for the wrong reason. It now edits D and the funding in place. Under the committed templates the VA6 attack is unchanged.
- **S6-4 Builder fallbacks for out-of-domain draws.** Where `game::convert` refuses a fabricated state but the script computes something, the attack sets the `Lie` so the builder writes what the script computes (`VA6 V' + D > TOTAL_MAX` converts with `refund = Some(false)`; `VA6 pend_wsh = total_shares + 1` uses `wc = Some(W)`). Otherwise a mutant would "survive" only because the builder and the script disagree.
- **S6-5 M5-c case.** `storage_case_rec` cannot make V' = 0 (as m3_measure: V = max(V - V', 0)). The record still sits at exactly R (out_c = floor((V' + 1) / (S + VS)) = 0), and vault' at R + V'. The supremum counts the record as one more covenant term 4C/R.
- **S6-6 M5-b shapes.** The "refund branch" is measured as a refund by price (C1 computes the mint, then refunds), which runs mulfrac. A pre-guard refund skips it and is cheaper.
- **S6-7 vault-m5 settles both sides.** The spec's M5-f names only the deposit settlement; the withdrawal settlement also drains the record and refunds its R.

### Open items

- **O-S6-1 (user / Stage 8): M5-f pending** (no reachable testnet node from this session). Run `cargo run -p tn10 -- probe vault-m5` and record the rows in `docs/tn10-mass-check.md`.
- **O-S6-2 (Stage 7 reviewer):** `M5W5_idx#0` is load-bearing (a P2SH owner equal to the vault's script); check it in the owner-auth bypass hunt. The record leg's 111,160 units are the largest share of a settle's compute.
- **O-S6-3 (Stage 8):** `docs/costs.md` M5 from the tables above; the CLAUDE.md M5 commands (`--test m5_mutants`, `m5_measure`, `M5_ATTRIB`, `probe vault-m5`).
- Unchanged: O-S2-5, O-S4-3 (drawer stack headroom 4 slots; the drawer is untouched), O-S4-4 (dead M4 draw branch), O-S5-3, O-S5-4.

### Final `cargo test` (from `crates/`, `cargo test -j 2`, debug)

Exit code 0. Every binary is green, M0-M5. The seed gate `cargo check -p lottery --no-default-features --features mainnet,test-seed` still fails with `error: TEST-ONLY seed cannot be built for mainnet`. One fix after the first full run: the debug build caught an underflow in the test helper of the stage6 attack "VA11 a record value below out_rem" (release wraps). The attack now lowers only the record input by R_rc + 1; m5_attacks (with `M5_ATTRIB=1`: OK) and the three `s3_value` mutants were re-run before the run below.

```
harness  test result: ok. 0 passed; 0 failed; finished in 0.00s
covenant_smoke  test result: ok. 3 passed; 0 failed; finished in 0.01s
m0_limits  test result: ok. 3 passed; 0 failed; finished in 0.13s
m0_mass  test result: ok. 5 passed; 0 failed; finished in 63.84s
m0_merkle  test result: ok. 2 passed; 0 failed; finished in 2.13s
m0_shapes  test result: ok. 6 passed; 0 failed; finished in 1.63s
lottery  test result: ok. 31 passed; 0 failed; finished in 10.87s
m1_attacks  test result: ok. 25 passed; 0 failed; finished in 17.23s
m1_buy  test result: ok. 14 passed; 0 failed; finished in 9.57s
m1_measure  test result: ok. 4 passed; 0 failed; finished in 35.59s
m1_mutants  test result: ok. 11 passed; 0 failed; finished in 50.75s
m2_attacks  test result: ok. 40 passed; 0 failed; finished in 19.16s
m2_close  test result: ok. 8 passed; 0 failed; finished in 7.76s
m2_game  test result: ok. 2 passed; 0 failed; finished in 31.68s
m2_measure  test result: ok. 5 passed; 0 failed; finished in 132.26s
m2_mutants  test result: ok. 25 passed; 0 failed; finished in 29.60s
m2_params  test result: ok. 17 passed; 0 failed; finished in 11.01s
m3_attacks  test result: ok. 22 passed; 0 failed; finished in 32.40s
m3_draw  test result: ok. 24 passed; 0 failed; finished in 18.48s
m3_game  test result: ok. 13 passed; 0 failed; finished in 1.03s
m3_measure  test result: ok. 7 passed; 0 failed; finished in 31.94s
m3_mutants  test result: ok. 93 passed; 0 failed; finished in 81.57s
m4_attacks  test result: ok. 21 passed; 0 failed; finished in 32.45s
m4_claim  test result: ok. 25 passed; 0 failed; finished in 14.15s
m4_game  test result: ok. 4 passed; 0 failed; finished in 5.79s
m4_measure  test result: ok. 8 passed; 0 failed; finished in 49.29s
m4_mutants  test result: ok. 211 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 169.93s
m5_attacks  test result: ok. 20 passed; 0 failed; finished in 38.65s
m5_game  test result: ok. 10 passed; 0 failed; finished in 7.02s
m5_measure  test result: ok. 5 passed; 0 failed; finished in 12.93s
m5_mutants  test result: ok. 251 passed; 0 failed; finished in 191.27s
m5_vault  test result: ok. 13 passed; 0 failed; finished in 14.58s
doc-tests harness  test result: ok. 0 passed; 0 failed; finished in 0.00s
doc-tests lottery  test result: ok. 0 passed; 0 failed; finished in 0.00s
```
