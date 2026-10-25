# Testnet-10 mass-limit check (research G1–G3)

Date: 2026-10-10. This checks the open mass-limit questions G1–G3 of `docs/research/kaspa-best-practices.md` on the live testnet-10 network.

## Setup

**Network.** I reached public testnet-10 nodes through the wRPC resolver: `*-10.kaspa.blue`, `.green`, `.red` and `.stream`, at DAA ≈ 592,537,600–592,541,100. Every node reported version 2.1.0 and network `testnet-10`, was synced, and had its UTXO index enabled.

**Tool.** `crates/tn10`. Run it from `crates/` with `cargo run -p tn10 -- probe <case>`. How it builds transactions:
- Funding inputs are real Schnorr P2PK spends.
- Every transaction commits its storage mass.
- KIP-20 genesis transactions create the covenant objects (`populate_genesis_covenants`), and the probes spend them as continuations.

**Probe scripts.** The covenant objects use a padded P2SH redeem script, `OP_DROP OP_TRUE`. Its signature script is sized to M0's measured sigscripts: 3,418 B for a shard and 190 + 420·K B for the controller. The committed compute budgets are M0's too. So the compute, transient and storage masses match M0's shapes; the protocol logic itself is not exercised.

Each case spends its setup outputs while they are still unconfirmed (chained in the mempool), then sweeps everything back to the funding address.

**Fees.** Every transaction pays 100 sompi per gram of its priority mass, i.e. at least the relay floor. Priority mass is the normalized maximum of compute, transient and storage mass (`mining/src/mempool/model/frontier/feerate_key.rs:86-98`); the relay floor ignores storage mass.

**"Visible after".** The time from submission until the transaction's change output appeared in the node's UTXO index, i.e. until it was mined.

## Results

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| baseline P2PK → P2PK | 2,038 | 1,272 | 10,000 | 10,000 | 1 | 0.0101 | accepted; visible after 1.3 s | `d3207bf494d2ca16de0363dd98d6247e95391601ba5836eb9b893ced2000376c` |
| storage ≈ 99,000: one plain output of 0.101010 KAS | 2,038 | 1,272 | 99,000 | 99,000 | 1 | 0.0991 | accepted; 1.4 s | `68725d43d23502fffbb8d6d596f5bdc7bf24d32f49372004dafbf45e6c30d172` |
| storage ≈ 101,000: one plain output of 0.099010 KAS | 2,038 | 1,272 | 101,000 | 101,000 | 1 | 0.1011 | accepted; 0.5 s | `8215fabc6953c7784309d37880b45fbc8b343627d21ac52b83419ae6bcb4696f` |
| storage ≈ 499,000: one plain output of 0.020040 KAS | 2,038 | 1,272 | 499,000 | 499,000 | 1 | 0.4991 | accepted; 0.5 s | `e0629721f8e0ebdadd987dc4c26c5d0cca23b78fca0d938c550cfc023589b40f` |
| storage ≈ 501,000: one plain output of 0.019960 KAS | 2,038 | 1,272 | 501,000 | 501,000 | 1 | 0.5011 | **rejected**: "transaction storage mass of 501000 is larger than max allowed size of 500000" | `f242b7ecfd2b36f3bb7488861feebc12a5e4ed81453326cba19d4843407aea28` |
| transient ≈ 110,000: two padded inputs of 13,550 B | 29,034 | 109,896 | 0 | 54,948 | 3 | 0.0550 | accepted; 1.4 s | `82c71711ee5bc4ae75d6c1ab4b2699070dfb605995254e126c1b58c8d75331a7` |
| transient ≈ 990,000: two padded inputs of 123,550 B | 251,234 | 989,896 | 0 | 494,948 | 3 | 0.4950 | accepted; 3.4 s | `fb4027639a15563d9630e7c2b43a40110995df83a7c036095d1792cba2682d6c` |
| transient ≈ 1,010,000: two padded inputs of 126,050 B | 256,234 | 1,009,896 | 0 | 504,948 | 3 | 0.5050 | **rejected**: "transaction transient (storage) mass of 1009896 is larger than max allowed size of 1000000" | `3126a3137b961bb8d609cedfab9bdb5be939900b6d0dec5df57478613ceb1704` |
| dust: plain output 0.019 KAS | 2,038 | 1,272 | 526,315 | 526,315 | 1 | 0.5264 | **rejected**: storage 526,315 > 500,000 | `9828cb3c4a513303892c4b7f3ca6512019881e9a6e3ba4d6e990fb989ad7e128` |
| dust: plain output 0.020 KAS | 2,038 | 1,272 | 500,000 | 500,000 | 1 | 0.5001 | accepted; 1.5 s | `e073e4c8aebe0a2ce4c83a559adbdfbf36a94f8fae20b096688fc7d44c8a3690` |
| dust: plain output 0.100 KAS | 2,038 | 1,272 | 100,000 | 100,000 | 1 | 0.1001 | accepted; 0.5 s | `5c5201d7b8cbb1f5763103df5fd5db51b896444dabded78b8cd55eebfe27db21` |
| dust: covenant output 0.080 KAS | 2,083 | 1,412 | 500,000 | 500,000 | 1 | 0.5001 | accepted; 0.5 s | `ca38aeb82b8625fb6c75d48e8cf66b5c5bc9aa2164d9236ee97671d3d2b71b71` |
| dust: covenant output 0.100 KAS | 2,083 | 1,412 | 400,000 | 400,000 | 1 | 0.4001 | accepted; 0.4 s | `3e1b18e25a9ac1a3e37f54776b444805d580d380800754c02e9161a9f8368bc1` |
| dust: covenant output 0.400 KAS | 2,083 | 1,412 | 100,000 | 100,000 | 1 | 0.1001 | accepted; 1.5 s | `217fe5352d0ff033cf727d61db00fb1c253a9155b26f4d9469cb669f511c961d` |
| `buy()`, R = 1.44, d = 0.0844 (M0 minimum), no referral | 6,612 | 15,648 | 499,992 | 499,992 | 2 | 0.5001 | accepted; 0.5 s | `4c5ef10371278d2127acf809b434815903c1e3870a4c33c2d12b9b35039f6e47` |
| `buy()`, R = 1.44, ticket 0.1075, referral output 0.1 KAS | 7,024 | 15,856 | 499,992 | 499,992 | 2 | 0.5001 | accepted; 1.4 s | `602f1b74cf9b2c64c3d60056f1d82ceede248439f84389b31b76ae84340d187d` |
| `claim()`, last claim, prize 0, payout = d | 6,921 | 16,524 | 146,259 | 146,259 | 3 | 0.1464 | accepted; 0.4 s | `7320f0eba20e7f02d91717f519ce6e8b6110fe473591bd0b3626738feffad000` |
| `claim()`, non-final, b = 2, prize 0.0426 (minimum) | 7,378 | 16,872 | 499,976 | 499,976 | 3 | 0.5001 | accepted; 1.5 s | `44b115e9014b3ad4ed9f3bd7275435b1d69bcbe72093ebaca44a283070d36714` |
| `close()`, K = 16, R = 1.44, controller sig 6,914 B | 74,572 | 257,408 | 498,657 | 498,657 | 18 | 0.4988 | accepted; 1.4 s | `b28fc2483552682ba61ebc996b80a7aaa631951e2960d49d259fc8ac94959a0c` |
| `close()`, K = 32, R = 2.72, controller sig 13,634 B | 146,356 | 512,064 | 495,013 | 495,013 | 34 | 0.4951 | accepted; 1.5 s | `8962430d2e5273a6784f401e7ab09d47ab3b56155cc4e9b4e3e8d3d7457303e8` |
| `close()`, K = 62, R = 5.1439, controller sig 26,234 B | 280,926 | 989,544 | 479,758 | 494,772 | 64 | 0.4949 | accepted; 1.4 s | `5a73e66a08515ddeddb351c1377c9ac0332b523403462047ce58be748431312c` |

Setup transactions (genesis and funding) were all accepted. Their IDs are in the tool output; they are left out here.

## Conclusions

**G1: which per-transaction limit applies.** The block limits do: storage 500,000 and transient 1,000,000.
- Storage mass of 101,000 and 499,000 was accepted and mined; 501,000 was rejected.
- Transient mass of 989,896 was accepted and mined; 1,009,896 was rejected.
- Compute mass was not pushed to its 500,000 limit.
- v2.1.0 nodes do not apply KIP-9's 100,000-gram standard limit. This matches the source: `mining/src/mempool/check_transaction_limits.rs:15-44` checks the block limits, and the node passes in `config.block_mass_limits` (daemon source, `daemon.rs:648-652`).

**G2: covenant outputs cost 4×, and relief is small.**
- A covenant-bound P2SH output costs 4·10¹²/value in storage mass; a plain output costs 10¹²/value (plurality 2 vs 1).
  - Local test (`m0_limits::g2_covenant_output_plurality`): 40,000 vs 10,000 for a 1 KAS output.
  - On chain: a 0.08 KAS covenant output costs 500,000, and a 0.1 KAS one costs 400,000.
- KIP-9 input relief offsets little in `close()`. At M0's minimum R, typical callers still use 74–87% of the storage limit (`m0_limits::g2_close_relief_by_funding`).
- `close()` with M0's minimum R and script sizes was accepted and mined within 1.5 s at K = 16, 32 and 62. At K = 62 transient mass was 989,544 and storage mass 479,758.

**G3: dust floor.** The v2.1.0 mempool has no dust rule; the floor comes from storage mass alone, against the 500,000 per-transaction limit:
- 0.02 KAS for a plain output (0.019 KAS was rejected at 526,315);
- 0.08 KAS for a covenant-bound output.

Aspectron's 0.019 KAS figure is just over the limit. The research doc's 0.1 / 0.4 KAS derivation assumed a 100,000 limit that nodes do not apply.

**M0's figures hold under relay.** These were all accepted and mined:
- `buy()` at the minimum deposit d = 0.0844 KAS without a referral output, and with a 0.1 KAS referral output and a 0.1075 KAS ticket. The second is a mass test of the referral shape: at price 0.1 a real 10 % referral is 0.01 KAS, below the 0.02 KAS floor;
- the last claim, and a non-final bundle claim at the minimum prize;
- `close()` up to K = 62.

**Not covered.**
- Compute mass was not pushed to its limit.
- K = 37 / 23 for larger shard scripts follow from the verified transient limit but were not submitted.
- The K = 62 `close()` used O = 0 with large change, not the large-O profile that sets R at that K (local 499,862 storage).
- No `draw()` shape was submitted; that is for M3 and M6, with the real script.

**Fee priority.** A transaction near the storage limit has a priority mass near 500,000, so paying the floor feerate costs about 0.5 KAS; the relay floor alone for `buy()` is about 0.008 KAS. On an idle testnet this made no difference to inclusion. Under load it decides inclusion (feasibility Q7).

**Wallet SDK.** The rusty-kaspa wallet SDK's transaction generator caps max(compute, storage) at 100,000 (`wallet/core/src/tx/mass.rs:25, 299-319`; `generator/generator.rs:739-745, 839`). This is a client-side limit on transactions the generator builds, not a network rule. Our builder constructs protocol transactions itself.

## Notes

- **Two runs failed because of probe mistakes, not network behaviour.** The rows above are the corrected runs.
  - A first `buy()` run added a 1 KAS output that the real shape does not have, and was rejected at storage 509,992.
  - A first claim run put the pool and a minimum-deposit ticket in one setup transaction, which was rejected at 501,803.
- One 0.0844 tKAS covenant output from the failed claim setup was left unswept (`fefe481e8c365a0c4ba61a2a53a7325517cc44c207fb738de2373e72cc0fa8d3`, output 0).

## M1: real `buy()` (spec M-e, 2026-10-10)

`cargo run -p tn10 -- probe buy-m1`. Unlike the probes above, this runs the real shard script (`covenants/shard.sil`, B_MAX = 5), built by the `lottery` builder and checked in the local v2.1.0 engine before submission.
- A KIP-20 genesis transaction creates one fresh shard (empty tree, deadline = virtual DAA + 10,000,000, R = 1.44 KAS).
- The probe is a `buy()` of 5 distinct combinations with a 0.5 KAS referral output (s_R = 10%), d = 0.11 KAS, submitted through the public mempool with the builder's fixed 0.5 KAS fee.
- Node: `neutrino-10.kaspa.stream`, version 2.1.0, DAA ≈ 592,574,552.

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis shard (real template) | 2,083 | 1,412 | 27,777 | 27,777 | 1 | 0.0279 | accepted; 0.5 s | `a7f765f24ab5e264aa5b002f07980ce2946ae2ebf22144c54807dee80e037ba6` |
| `buy()`, real shard script, b = 5, referral, d = 0.11 KAS, R = 1.44 KAS | 19,347 | 59,148 | 99,453 | 99,453 | 2 | 0.5000 | accepted; 1.4 s | `2ff25ef4744517e7c1eba4523859a3f876f89d7d79931a4d3d76757959b27b7f` |

The shard successor and the ticket have no spend path in M1 (the ticket template is a placeholder until M4; `close()` comes in M2), so about 7.0 testnet KAS stay locked in them. The referral and change outputs were swept back.

## M2: real genesis and K = 16 `close()` (spec M2-e, 2026-10-10)

`cargo run -p tn10 -- probe close-m2`. This runs the real controller, params and shard scripts (B_MAX = 4, R = 1.69 KAS). The `lottery` builder builds both transactions, and the local v2.1.0 engine checks the `close()` before submission.
- **Genesis (D31)** creates the round 1 controller, the params and 16 fresh shards under a new KIP-20 covenant ID.
  - The controller is drawn = 1 with the fixture's J, G2, L and V_rec, boost pot 0 and acc 2 KAS. No fabricated controller was needed, because genesis writes drawn = 1.
  - Deadline T is 1,000 DAA below the virtual DAA at build time, so a `close()` with lock_time = T is final at once.
- **`close()`** runs after genesis is mined (creation DAA score of the genesis outputs: 593,039,315).
  - It spends the controller, the params, the 16 shards and the genesis change as caller funding.
  - Zero sales, so C_close = 0. The caller output is the change minus R and the fee.
  - Its fee is the minimum feerate over the priority mass, plus 0.0001 KAS.
- Node: `muon-10.kaspa.blue`, version 2.1.0, DAA ≈ 593,039,254.
- Covenant ID: `f3c1b539f195d8d2300b32a3012989cec373215ed4cc3ed9a4c0e6880b492cb3`.

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, real templates, R = 1.69 KAS | 9,852 | 7,328 | 413,196 | 413,196 | 1 | 0.4133 | accepted; visible after 1.3 s | `30716f96f2eff9e60ed1ed9ba10d685030a5d03742665acba8b06715308be027` |
| `close()`, real scripts, K = 16, B_MAX = 4, zero sales, R = 1.69 KAS | 289,977 | 856,748 | 435,348 | 435,348 | 19 | 0.4354 | accepted; visible after 1.3 s | `ae28c020a8ce0f12e982ae2fb5e24cc2b533a404ad029abaf917a90884daa7d9` |

Compute and transient mass match the local M2-a figures exactly (289,977 and 856,748, 85.7 % of the transient limit). Storage (87.1 %) is this shape's: pot 0, acc 2 KAS and a large caller output. The worst case at R = 1.69 is 449,692 (`docs/costs.md` M2-c).

This probe ran before the review fix R2 added one `require` to `close()` check 2. The controller redeem is now 19 B larger (26,321 B), and a local K = 16 `close()` measures compute 289,996 and transient 856,824 (+19 and +76; storage unchanged; `docs/costs.md` M2-a). That is 85.7 % of the transient limit, as before, so the probe was not repeated.

About 34.1 testnet KAS stay locked: 2 + 19 × 1.69, i.e. the round 2 controller (acc + R), params and 16 shards, plus the round 1 marker. They have no spend path yet: round 2 is not drawn, and the marker is the D28 placeholder. The caller output is ours.

## M3: genesis, buy, K = 16 `close()` and `draw()` with a TEST-ONLY seed (spec M3-g, 2026-10-10)

`cargo run -p tn10 -- probe draw-m3`. It runs the real drawer, marker, vault, controller, params and shard scripts (B_MAX = 4, R = 1.69 KAS). The `lottery` builder builds every transaction, and the local v2.1.0 engine checks it before submission.
- **Genesis** is as in M2, plus the vault (V0 = 60 KAS) and the drawer, with J(1) = 10 KAS. Deadline T is the virtual DAA at build time + 600.
  - `buy()` needs the shard created before T (shard.sil check 6).
  - `close()` needs lock_time = T to be final, so the probe waits for T.
- **`buy()`** on shard 0 sells the round 1 jackpot combination of the TEST-ONLY seed `[0x5e; 32]`, so the draw creates a jackpot pool.
- **`close()`** (K = 16) is funded by the buy change. It sets seed_ref = own + 600.
- **`draw_test_seed()`**:
  - Inputs: drawer, marker, vault, controller, and one funding input (the close caller output). lock_time = seed_ref. In run 3 (table below) each input's creation DAA score was read from our UTXO index; run 4 reads chain-merged node entries instead (see the O-S4-4 explanation and run 4 further down).
  - Before submission the probe fetched the node's UTXO entry of every input. Each matched the builder's (amount, script, creation DAA score, covenant ID), and every input passed the local engine on the node's entries.
- Node: `muon-10.kaspa.blue`, version 2.1.0, DAA ≈ 593,170,947. Covenant ID `278095041b9755b3bf5b8073d97887ce983985e0bca2fe1d2192505aa74d3db0`.

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, real templates, R = 1.69 KAS | 10,766 | 8,024 | 437,512 | 437,512 | 1 | 0.4376 | accepted; visible after 1.3 s (DAA 593,170,984) | `1737e827875e2a277f5e1001ef23bf3776d58ae8d1590134253920b2cca61c86` |
| setup: `buy()` of the jackpot combination [3, 19, 22, 25, 27] + 7 on shard 0 | 15,637 | 50,148 | 378,497 | 378,497 | 2 | 0.3786 | accepted; visible after 1.3 s | `ad8dc58558fe0120cdd677b028f53cd21f637345dac95d87742926957af82d60` |
| setup: `close()`, K = 16, one sale of 1 KAS | 293,841 | 864,604 | 426,540 | 432,302 | 19 | 0.4324 | accepted; visible after 1.3 s (DAA 593,171,615; seed_ref 593,171,609) | `90da213e31db83b0be39c9b58f5df03a9641c1fcd3df67ba85e077acac9a8b3e` |
| **`draw_test_seed()`, real scripts, K = 16, a = 0, np = 4, h_J = 1, n2 = 0, fee paid** | **107,048** | **332,712** | **130,534** | 166,356 | 5 | 0.1665 | **accepted; visible after 1.3 s** | `459791070e5462e3256d7cab594f2b581105248b100de4636d977c941f901206` |

The round result was derived from the draw transaction's bytes alone (`RoundResult::from_tx`, A9):
- round 1, win (3, 19, 22, 25, 27, 7), D_claim 599,651,609 (= seed_ref + DRAW_WINDOW + CLAIM_WINDOW);
- pools as (tier, slice, value in sompi):
  - (0 jackpot, 10 KAS, 11.69 KAS);
  - (2, 32,000, 1.73 KAS);
  - (3, 2,666, 1.72 KAS);
  - (4, 1,000, 1.72 KAS).

Draw script units per input on the node's entries: drawer 1,288,729, marker 297,436, vault 295,973, controller 63,734, funding 100,001.

Compute and transient mass match the local M3-b figures for this shape. The `close()` matches M3-f exactly (293,841 / 864,604, 86.5 % of transient). The draw is 21.4 % of compute and 33.3 % of transient. Its storage (26.1 %) is high because the four pools sit near R.

**Earlier attempts in the same session:**
1. **Run 1**: T was set below the current DAA, as in close-m2. The local engine rejected the buy (shard created at or after T) before submission. Only its genesis (`b800e4cf…7d45`) went to testnet.
2. **Run 2**: genesis `7fb8fa44…49d1`, buy `c22e23c5…5081` and close `9c26731b…2602` were accepted. The draw `6a17386a…cf7` was **REJECTED** by the node ("script ran, but verification failed", a failed `require`), although the local engine had accepted the same transaction.
   - That run had no entry check. Its close was read as mined at DAA 593,168,541, below its seed_ref of 593,168,551, and the read happened 0.4 s after the output first became visible.
   - The cause was not identified. A possible cause is that a young input's creation DAA score changed between the read and the node's validation, which would break the exact `own_daa` equality of check 19. Its caller output was spent before it could be re-read.
   - This was open item O-S4-4; explained below (Stage 5).
3. **Run 3**: the probe above, with the entry check, was accepted.

**O-S4-4 explanation (M3 Stage 5 review):** run 2's rejection fits how the node assigns creation DAA scores; the exact `own_daa` equality of draw() check 19 exposes it.
- An output accepted only by the virtual (its transaction is in the virtual's mergeset, not yet merged by a chain block) is in the virtual UTXO set with the **virtual's** DAA score: `calculate_utxo_state(.., virtual_daa_window.daa_score)` (rusty-kaspa v2.1.0 `consensus/src/pipeline/virtual_processor/processor.rs:571-582`).
- When a chain block merges the transaction, the output is re-added with that **chain block's header DAA score** (`pov_daa_score = header.daa_score`, same file `:446-455`). The UTXO index can show the virtual-time score first and the chain-block score later.
- An input whose parent is still in the mempool has DAA `UNACCEPTED_DAA_SCORE` = u64::MAX (`consensus/core/src/constants.rs:51`), which `OpTxInputDaaScore` pushes as `block_daa_score as i64` = −1 (`crypto/txscript/src/opcodes/mod.rs:1130`).
- Run 2 read the close output 0.4 s after it appeared, below its own seed_ref: consistent with a virtual-time score that the merging chain block later replaced. The draw's `own_daa` (and D_claim) then no longer matched the node's maximum input DAA score, so check 19 failed. Funds were never at risk (the transaction is simply invalid; the caller retries).
- **Probe fix (Stage 5):** `draw-m3` now reads each input only after a chain block has merged its creating transaction (`GetVirtualChainFromBlock` with accepted transaction IDs, then `GetBlock` for that chain block's DAA score) and only once the node's UTXO entry carries that DAA score (`cases.rs::merged`). It builds the draw from the node's own entries, re-reads them right before submission, and rebuilds if any entry moved. The same rule applies to the genesis, buy and close outputs that later transactions spend. Builder policy for M4+: never build a DAA-dependent transaction (close, draw) on an input that a chain block has not merged.

**Run 4 (M3 Stage 5, 2026-10-10)**, same probe with the chain-merge rule; node `muon-10.kaspa.blue`, version 2.1.0, DAA ≈ 593,193,888; covenant ID `3aeec3ebd3063973efa4bb197ccc0eef49483dec7bb11a4dc52817cd09912214`. Each input was read after its chain block merged it; the node's entry DAA equalled the merging chain block's DAA score every time (the genesis output was seen "not yet merged by a chain block" for a few seconds before that). No entry moved between build and submission (attempt 1).

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, real templates, R = 1.69 KAS | 10,766 | 8,024 | 437,512 | 437,512 | 1 | 0.4376 | accepted (merged at DAA 593,194,441) | `a315cdcd470cff1ffecb98a2e3472688f3029b09d2a4d5352e939e393f5736ac` |
| setup: `buy()` of the jackpot combination [6, 7, 8, 12, 14] + 7 on shard 0 | 15,637 | 50,148 | 378,497 | 378,497 | 2 | 0.3786 | accepted (merged at DAA 593,194,940) | `893559d2eca8449f91180f89e06f2db2128e51871bbeabf5208f045b4ea281cc` |
| setup: `close()`, K = 16, one sale of 1 KAS | 293,841 | 864,604 | 426,503 | 432,302 | 19 | 0.4324 | accepted (merged at DAA 593,195,727; seed_ref 593,195,540) | `3ca43680791e15a8de7bba14ca62e7b3de56f63cdba83972a9333605be9f95fd` |
| **`draw_test_seed()`, real scripts (Stage 5 drawer, f_min floor), K = 16, a = 0, np = 4, h_J = 1, n2 = 0, fee paid** | **107,052** | **332,728** | **130,534** | 166,364 | 5 | 0.1665 | **accepted; visible after 4.9 s** | `8b9d0a0c913e371db9bebe12ab9fc6e0c07e181504a1a910c2d2dd4d26db8305` |

Round result from the draw transaction bytes (`RoundResult::from_tx`): round 1, win (6, 7, 8, 12, 14, 7), D_claim 599,675,540; pools (tier, slice, value in sompi) (0, 10 KAS, 1,169,000,000), (2, 32,000, 173,000,000), (3, 2,666, 172,000,000), (4, 1,000, 172,000,000). The draw's compute and transient are 4 and 16 above run 3 (the drawer grew by 4 B).

Locked on testnet (no spend path in M3), for each of runs 3 and 4: the round 2 objects, drawer', vault', controller', the 4 pools and the ticket. Runs 1 and 2 also left their genesis (and run 2's round 1 to 2) objects. The caller and marker refund outputs are ours. Balance after run 3: 8,648.65 testnet KAS (the balance after run 4 was not recorded).

## M4: `claim()` with the real ticket and pool scripts (spec M4-i, 2026-10-11)

`cargo run -p tn10 -- probe claim-m4`. It runs the `draw-m3` sequence (shared code, `round1_drawn`) with the final M4 scripts and then one `claim()`. Every transaction is built by the `lottery` builder and checked in the local v2.1.0 engine before submission.
- **Genesis** as in M3 (K = 16, J(1) = 10 KAS, V0 = 60 KAS, T = virtual DAA + 600), with the M4 templates: the drawer carries `ticket_tpl` and A_MAX = 7, the pool has `ticket_leg` and `refresh`, the marker has `refresh`.
- **`buy()`** of the round 1 jackpot combination of the TEST-ONLY seed `[0x5e; 32]` on a b = 1 ticket in shard 0; then `close()` and `draw_test_seed()` as in M3.
- **`claim()`** spends the ticket (buy output 1), the jackpot pool (its draw output; the pool state is rebuilt from the draw bytes by `RoundResult::pool_state` and checked equal to the output script) and the draw's caller output as funding. Each input is the node's chain-merged entry (builder policy S5-5), re-read before submission. The reference gives mode FINAL (b = 1, h_J = 1): pool' = R, payout = J + d to our key, change. lock_time 0.
- Node: public testnet-10 node, version 2.1.0, network `testnet-10`.

Final-script run (2026-10-11):

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: claim-m4 genesis, K = 16, real templates, R = 1.69 KAS | 10766 | 8024 | 437512 | 437512 | 1 | 0.4376 | accepted | `89849e12d4ec45b875feda18a1d25fd9d97d213cc693971bfef2baf1b5850015` |
| setup: buy() of the jackpot combination [8, 10, 11, 15, 20] + 9 on shard 0 | 43472 | 151488 | 378497 | 378497 | 2 | 0.3786 | accepted | `68b0770124fdf8b87f9894b0ed2107c54efacd1564be584c778bf91902fade81` |
| setup: close(), K = 16, one sale of 1 KAS | 294619 | 867716 | 426466 | 433858 | 19 | 0.4340 | accepted | `544c55868d92716c635b8054ecfa8f3e603ff992ca27ba8d46e5ff1b015b344b` |
| setup: draw_test_seed(), K = 16, a = 0, np = 4, h_J = 1, n2 = 0, R = 1.69 KAS | 113931 | 354644 | 130534 | 177322 | 5 | 0.1774 | accepted | `a80be9729d305823b0e186cdde37ff8e4a8c4ecd6c126938d6ddaf3c3e627c4b` |
| **claim() FINAL on the jackpot pool: K = 16, m = 10, h = 1, prize 10.0000 KAS, payout 10.1100 KAS, pool' 1.6900 KAS (R)** | **47503** | **152052** | **24633** | 76026 | 3 | 0.0761 | **accepted** | `e7dfa473296baae3ce89bb498a891ee9cc73671d1758e232109ca28a118334b8` |

Round result from the draw transaction bytes (`RoundResult::from_tx`): round 1, win [8, 10, 11, 15, 20, 9], D_claim 599,918,756, pools (tier, slice, value) [(0, 1000000000, 1169000000), (2, 32000, 173000000), (3, 2666, 172000000), (4, 1000, 172000000)].

The masses match the local measurements:
- `buy()` = M4-d (b = 1, no referral: 43,472 / 151,488; storage 378,497 is this shape's);
- `close()` = M4-f (294,619 / 867,716, 86.8 % of transient);
- the claim = M4-b FINAL, K = 16, m = 10 (47,503 / 152,052; storage 24,633 here vs 24,581 in the measured shape, from the different output values). The claim is 9.5 % of compute, 15.2 % of transient and 4.9 % of storage; its priority mass 76,026 is transient / 2.

**Earlier run, superseded.** A first `claim-m4` run on 2026-10-11 used the A_MAX = 5 drawer (before review fix U5). It was also accepted end to end; claim `e0787782f4d2f9cc3ebd4bd16f3009b84f7a6e3b22291516206428d3eaee4fe0` had the same claim masses (47,503 / 152,052 / 24,633), and its draw measured 110,779 / 344,436 / 130,534. The run above replaces it (DECISIONS U7).

**Note on the M3 rows above.** The `draw-m3` runs (M3 section) predate the M4 layout (drawer `ticket_tpl`, pool `reserve`, the larger pool and marker) and A_MAX = 7. Their figures describe the M3 scripts; the `draw()` row in this section is the current one.

## M5: vault requests, a converting `draw()` and settlements (spec M5-f, 2026-10-12)

`cargo run --release -p tn10 -- probe vault-m5`. It runs the `draw-m3` sequence (shared code, `round1_drawn_with`) with the final M5 scripts, adds two vault requests before the buy, and settles both after the draw. Every transaction is built by the `lottery` builders and checked in the local v2.1.0 engine before submission; every input is the node's chain-merged entry (M3 S5-5).
- **Genesis** as in M4 (K = 16, J(1) = 10 KAS, V0 = 60 KAS), with the M5 templates: vault template fields, S0 = V0 / SHARE_UNIT = 6,000,000 shares in the deployer position (owner: the probe key, Schnorr P2PK). T is set 3δ later than in M4 to leave room for the two requests.
- **Negative case (U-4 age window):** the `request_deposit()` below, with lock_time = vault creation + VAULT_REQ_DELAY − 1, submitted once the virtual DAA had passed that lock time (so the transaction is final and only the script decides). The local engine and the node both rejected it at input 0 (vault check D10).
- **`request_deposit()`** of D = 2 KAS at exactly lock_time = vault creation + δ; **`request_withdraw()`** (partial, w = 1 % of S0) of the deployer position at vault' creation + δ, with the deposit's change as the P2PK Schnorr owner input.
- **`buy()`**, **`close()`** and **`draw_test_seed()`** as in M4. The draw converts both sides (mint 236,539 shares for D; wc = W = 60,000, out_c = 0.5073 KAS) and writes the settlement record at output 7 (3 + np); the record state was rebuilt from the draw bytes alone (`RoundResult::record`) and matched the output script.
- **`settle()`** of the deposit (deposit-convert, a new position, record' kept), then of the withdrawal (FINAL, the record drained, its R refunded to the draw caller).
- Node: `vector-10.kaspa.green`, version 2.1.0, network `testnet-10`, DAA ≈ 595,068,833. Covenant ID `403d8bc8ef49d2d9fd6f1cff6fa55e9b0ab21d32d2e6239f547b540da27e3abd`.

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: vault-m5 genesis, K = 16, real templates, R = 1.69 KAS | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted; visible after 1.4 s | `71f7ad9abd324c1899ec5d8e6535306a1dc372f470e173053c819f1e4d8f1ae6` |
| **negative: `request_deposit()` at vault age δ − 1 (lock_time = creation 595,068,865 + 599 = 595,069,464)** | 41,443 | 77,372 | 24,288 | 41,443 | 2 | 0.0388 | **REJECTED: "failed to verify the signature script: Unsatisfied lock time: locktime requirement not satisfied -- locktime is greater than the transaction locktime: 595069465 > 595069464"** | `79be31e709d89c71e504c120defa46b46d1fdeba4e1a3832fa83690e95057ed4` |
| **`request_deposit()`, real vault script: D = 2.00 KAS, request value R** | **23,143** | **77,372** | 24,288 | 38,686 | 2 | 0.0388 | **accepted; visible after 0.5 s** | `85db3c8270451a99b7f59d78c3f2202977f4d8f01722517bab803ef3e25ec24a` |
| **`request_withdraw()`, real vault and position scripts: partial, w = 60,000 of 6,000,000 shares, P2PK Schnorr owner input** | **25,233** | **82,652** | 47,935 | 47,935 | 3 | 0.0480 | **accepted; visible after 0.5 s** | `bd8321bb70cb61e03a8ad4c5073c89d430204685b5445b6f32d60968de4324f5` |
| setup: `buy()` of the jackpot combination [1, 9, 13, 15, 29] + 8 on shard 0 | 43,472 | 151,488 | 378,497 | 378,497 | 2 | 0.3786 | accepted; visible after 0.5 s | `621e5b71c4cb4b860ce47144221102835cd37abfdd4fd243106cf1809e89d666` |
| setup: `close()`, K = 16, one sale of 1 KAS | 294,619 | 867,716 | 426,392 | 433,858 | 19 | 0.4340 | accepted; visible after 1.4 s | `2891e0572e763e698ce19730c3af78134b60b4c7dc2f4fd395167a100be51281` |
| **`draw_test_seed()`, real scripts, K = 16, a = 0, np = 4, h_J = 1, n2 = 0, fee paid; conversion D = 2.00 KAS → mint 236,539 shares, W = 60,000 → wc = 60,000, out_c = 0.5073 KAS; record at output 7** | **142,504** | **449,056** | **148,708** | 224,528 | 5 | 0.2246 | **accepted; visible after 1.4 s** | `4d473f90983d98f3ffbac769e14496d566a16d613fcbc0fa360682b13c510a85` |
| **`settle()`, real request and record scripts: deposit-convert, s = 2.00 KAS → position of 236,539 shares; record' kept** | **22,147** | **73,388** | 41,843 | 41,843 | 3 | 0.0419 | **accepted; visible after 0.5 s** | `1ffa5328e57a0fff3107beab8bd2d54cc559079b7d15bc8cfec7170d5f26c0f4` |
| **`settle()`: withdrawal FINAL, w = 60,000 shares, payout 2.1973 KAS (pay + R), record drained, R refunded to the funder** | **21,857** | **73,108** | 10,439 | 36,554 | 3 | 0.0367 | **accepted; visible after 0.5 s** | `2dec3958a0efb894d210379b133a290e097e178c74e2e3eadcb26ae8f497ab3d` |

Round result from the draw transaction bytes (`RoundResult::from_tx`): round 1, win [1, 9, 13, 15, 29, 8], D_claim 601,550,813, pools (tier, slice, value) [(0, 1000000000, 1169000000), (2, 32000, 173000000), (3, 2666, 172000000), (4, 1000, 172000000)], plus the record.

The masses match the local measurements:
- requests and settlements = M5-e (request_deposit 23,143 / 77,372; request_withdraw partial 25,233 / 82,652; settle deposit-convert 22,147 / 73,388; settle FINAL drained 21,857 / 73,108; storage differs with the output values);
- `close()` = M5-d (294,619 / 867,716, 86.8 % of transient), unchanged from M4;
- the converting draw is 28.5 % of compute and 44.9 % of transient (M4-i's draw without conversion: 113,931 / 354,644); the larger drawer (65 kB) and vault (3.4 kB) redeems account for most of the difference.

The rejection message is the node's CLTV error: the script's required lock time, creation + 600 = 595,069,465, is greater than the transaction's 595,069,464 (rusty-kaspa v2.1.0 `crypto/txscript/src/opcodes/mod.rs:874-923`). That is exactly check D10. The masses in that row belong to a transaction the builder sized for a script that fails (its compute, 41,443, differs from the valid request's 23,143); they are not a measurement of any accepted shape. The valid request one DAA later was accepted.

**Earlier runs, superseded.** Two earlier `vault-m5` runs on 2026-10-12 were also accepted end to end. The first, before the age window (U-4), measured request_deposit transient 77,092, request_withdraw 82,372 and draw 448,800 (the vault redeem was 64 B smaller); its first attempt in a debug build had overflowed the 1 MB Windows main-thread stack before submitting anything (fixed: the probe runs on a 64 MB thread). The second, on the final scripts without the negative case (node `vector-10.kaspa.green`, DAA ≈ 595,059,437), had the same masses as the table above (draw `2691a0f00f8873859800b57d2894283fab288386bf58bbd345fbb2ffa13f4448`). The run above replaces both.

Locked on testnet (no further entry exercised): the round 2 objects, drawer', vault', the four pools, the ticket, the new position (236,539 shares) and position' (5,940,000 shares). Payouts, the record's refunded R and changes are ours.

## M6: purchase conflicts, replacement and chained buys, and the re-runs at the final templates (spec A6, 2026-10-13)

All runs: `cargo run -p tn10 --release -- probe <name>` from `crates/`, public testnet-10 nodes found by the resolver (all version 2.1.0, network `testnet-10`), the M6 templates (vault with W4b, MIN_DEPOSIT = 10 KAS, d = 0.13 KAS). Every transaction is built by the `lottery` builders and checked in the local v2.1.0 engine before submission. Balance before the M6 runs: 8,022.88 testnet KAS.

### `conflict-m6` (D87, D88, research F3)

`probe conflict-m6` (new; not part of `probe all`, since it locks about 160 KAS). Genesis as `draw-m3` (K = 16, J(1) = 10 KAS, V0 = 60 KAS) with T = virtual DAA + 10⁷ (no `close()` follows), then a fan-out of 50 funding outputs of 3 KAS. Ten repetitions; repetition i works on genesis shard i:
- **Part R (spec A6 steps 1–3).** Buys A and B (b = 1, no referral, different combinations and funding inputs) on the same shard state; B pays twice A's fee (feerate 199.61 vs 100.32 sompi/gram; the mempool compares fee / max(compute, normalized transient, storage), `consensus/core/src/tx.rs:662-668`, `mining/src/mempool/replace_by_fee.rs:150-164`). A is submitted with `submit_transaction`, then B at once (RbfPolicy::Forbidden), then the same B with `submit_transaction_replacement` (RbfPolicy::Mandatory, `rpc/core/src/api/rpc.rs:217-227`). Then a buy C chained on the unaccepted successor shard (creation DAA UNACCEPTED_DAA_SCORE, which the script reads as −1) of the node's winner, and if that is rejected, the C built on the other candidate (both Cs are built before the race). Which of A and B a chain block accepted is read from the virtual chain.
- **Part F (step 4, clean timing).** On the shard's chain-merged tip after part R, a buy P and, at once, a buy Q chained on P's unaccepted successor (built before P is submitted). τ_chain = start of P's submission → Q accepted by the mempool. τ_merge = start of a buy's submission → a chain block accepted it (virtual chain polled every 200 ms after each response, about every 0.6 s in practice, so an upper bound by one poll). Each merged buy's change output was then read from the UTXO index with the merging chain block's DAA score (`merged()`).

**Run 3 (recorded; node `quark-10.kaspa.red`, DAA ≈ 595,208,236; covenant instance from genesis `424df434…`).** All 60 buys have the same compute and transient, which equal M6-a's `buy()` b = 1 without referral (43,472 / 151,488). Their storage is 309,126–311,382, between M6-a's 1 KAS change row (311,729) and its large-change row (321,777): here the funding input is 3 KAS and the change about 1.56 KAS.

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, real templates, R = 1.69 KAS, T = DAA + 10⁷ | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted | `424df434081eaad0b08ebe52621cec8a06da87a38907a6bac9f35e968e1e2455` |
| setup: fan-out, 50 funding outputs of 3 KAS | 22,226 | 11,464 | 166,650 | 166,650 | 1 | 0.1668 | accepted | `c87cdbcfdeee65497e183de38a3eac50cfb165177f74bdb976b794ce0b7ebec6` |
| buy A (each rep) | 43,472 | 151,488 | 309,786 | 309,786 | 2 | 0.3108 | accepted 10/10 | per rep below |
| buy B (each rep), race and replacement | 43,472 | 151,488 | 311,382 | 311,382 | 2 | 0.6215 | see below | per rep below |
| buy C chained on A (each rep) | 43,472 | 151,488 | 309,126 | 309,126 | 2 | 0.3101 | accepted 10/10 | per rep below |
| buy P on the merged tip (each rep) | 43,472 | 151,488 | 309,178 | 309,178 | 2 | 0.3102 | accepted 10/10 | per rep below |
| buy Q chained on P (each rep) | 43,472 | 151,488 | 309,429 | 309,429 | 2 | 0.3104 | accepted 10/10, first attempt | per rep below |

Per repetition (B's race rejection: "spent" = `output (<shard outpoint>) already spent by transaction <A> in the mempool`; "orphan" = `transaction <B> is an orphan where orphan is disallowed`. Replacement: "no DS" = `replace by fee found no double spending transaction in the mempool`. The node's error texts are verbatim in the run output):

| rep | A (chain winner, every rep) | B race | B replacement | C (on A) | τ_merge A (s) | P | Q | τ_chain (s) | τ_merge P (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | `5d222980…` | orphan | no DS | `a54772e1…`, attempt 1 | 2.49 | `0a750315…` | `70a4ee54…` | 0.920 | 1.34 |
| 1 | `96e4363c…` | orphan | no DS | `34761462…`, attempt 1 | 2.25 | `0dda1fd3…` | `68548649…` | 0.924 | 1.35 |
| 2 | `b13cbc79…` | spent | no DS | `f01a6aee…`, attempt 1 | 2.27 | `4b637341…` | `7aeac789…` | 0.918 | 1.95 |
| 3 | `a0f35ea6…` | orphan | no DS | `60fb74d5…`, attempt 1 | 2.31 | `f950b8a7…` | `dcd3ff5d…` | 0.922 | 1.96 |
| 4 | `3945faba…` | orphan | no DS | `3c9042c3…`, attempt 1 | 2.25 | `166465f7…` | `14c075b2…` | 0.915 | 1.96 |
| 5 | `94c1af34…` | spent | **accepted by the node, replaced A** (0.660 s); A still won on chain | `6b8f3c3d…`, attempt 2 (C on B: orphan) | 2.91 | `cb7347c8…` | `72b8a87c…` | 0.922 | 1.34 |
| 6 | `96759540…` | spent | no DS | `c86e2a21…`, attempt 1 | 2.24 | `9e4518ee…` | `4252555e…` | 0.925 | 1.34 |
| 7 | `cc0f3ac7…` | orphan | no DS | `8cc81bff…`, attempt 1 | 2.28 | `01d1fed4…` | `0e8de099…` | 0.914 | 1.96 |
| 8 | `546bb7e8…` | orphan | no DS | `28b1fdbe…`, attempt 1 | 2.26 | `5ca330ff…` | `a2b7310c…` | 0.919 | 1.34 |
| 9 | `890435db…` | spent | no DS | `c3aef386…`, attempt 1 | 2.26 | `7728a8e7…` | `3fd94ee8…` | 0.919 | 1.96 |

Full transaction ids are in the run output (`crates/tn10/results-conflict-m6.md`, git-ignored, and the session log). Every C and every Q was later accepted by a chain block (consensus re-ran shard check 6 with the merging block's DAA score), and no losing B was ever accepted.

**Facts (run 3, plus run 2 below):**
1. **Race:** a second buy on the same shard state was rejected 10/10. 4/10 as a mempool double spend; 6/10 as an orphan, because 0.46–0.69 s after A's submission started, A had already left the mempool: it was in a block and the shard outpoint was spent in the virtual UTXO set.
2. **Replacement (D87):** `submit_transaction_replacement` with a strictly higher feerate was accepted by the node only when A was still in its mempool: 1/10 in run 3 (rep 5) and 1/1 in run 2. **In both cases A, not B, was accepted on chain**; B was dropped (C on B was an orphan right after the replacement returned, and B was never merged). In the other 9/10 the node answered "no double spending transaction in the mempool", because A was already block-included. The node broadcasts an accepted replacement and P2P peers insert with RbfPolicy::Allowed (`protocol/flows/src/flow_context.rs:676-690`, `protocol/flows/src/v10/txrelay/flow.rs:233`), but a block that already holds A removes its double spends from the mempool (`mining/src/mempool/handle_new_block_transactions.rs`, `remove_double_spends`). **So, at 10 BPS with a ~0.46 s round trip to a public node, the covenant spend is RBF-eligible at the node, but it did not displace a pending transaction in any of 11 attempts.** This is an input to D87 (the "honest request outbids the attacker's pending request" argument); nothing was redesigned. D87 was amended on this basis (O-M6-1, `docs/DECISIONS.md` 2026-10-13): the fee auction is not a mitigation; request griefing is mitigated by cost only.
3. **Chain (F3):** a buy chained on the unaccepted successor shard was accepted by the mempool 20/20: C in part R (9 at the first attempt, 1 at the second after the orphaned C on B) and Q in part F (10/10 at the first attempt, each before its parent was seen chain-accepted). All 20 were then merged on chain.
4. **Timing (part F, n = 10):** **τ_chain median 0.919 s** (mean 0.920, max 0.925): two RPC round trips (P's, then Q's); local build time 0.005 s per buy, excluded. **τ_merge median 1.655 s** (mean 1.650, max 1.962; values cluster at 1.34 and 1.96 because of the ~0.6 s poll). Part R's τ_merge for A (median 2.262 s, mean 2.351, max 2.914) is biased upward, because its polling starts only after the chain step (about 1.8 s); pooled R + F: median 2.103 s. Q accepted → chain-accepted: median 1.047 s, max 1.684. The medians of part F feed `m6_conflict` (`TAU_CHAIN = 0.919`, `TAU_MERGE = 1.655`; `docs/costs.md` "Purchase conflict rate").

**Earlier runs.**
- **Run 1** was killed by the operator after about a minute while the resolver connection was still being set up. Nothing was submitted, and the balance was unchanged.
- **Run 2** (node `muon-10.kaspa.blue`; first probe version) stopped in rep 0 after 300 s. That version assumed the node's replacement decides the winner, and waited for B. What happened:
  - genesis `049d5aa3e8af2d9ca21fbe746763c7590feb25db5a5d6332fdd8417da55ec1ec`;
  - A `e5ec7c275506f40b1335ae45382cf8d705e2f39ad7e92dad0cf6b3970871740c` accepted (submit 0.612 s);
  - B `e0d26f2bd32f7ea77adbb0dd1978fc2902743d69a4f7d75ef6fbc5561a9f889e` rejected "already spent by transaction e5ec7c27… in the mempool", then **accepted by `submit_transaction_replacement`, which returned A as the replaced transaction** (0.545 s);
  - C `97305742…` on B was an orphan for 60 s (100 attempts);
  - A's change was later found in the UTXO set at DAA 595,200,941, and B's funding was unspent.

  The probe was then changed: part R detects the chain winner from the virtual chain and tries C on both candidates, and part F was added. Run 2's genesis and fan-out (about 96 KAS) and A's shard successor and ticket stay locked.

Locked on testnet by run 3: every object of the instance (controller, params, 16 shards, vault, drawer, position; the 40 tickets of the accepted buys and the shard successors). Unused funding outputs, the changes and the fan-out change are ours.

### Re-runs at the final templates

**`buy-m1`** (run 2, node `boson-10.kaspa.red`, DAA ≈ 595,214,626). The probe now runs b = B_MAX = 4 and b = 1, each with a referral (s_R = 10 %, so 0.4 KAS and 0.1 KAS = M_REF), each on its own genesis shard, with the builder's fixed 0.5 KAS fee and a large change (≈ 7,700 KAS):

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis shard (b = 4) | 2,083 | 1,412 | 23,668 | 23,668 | 1 | 0.0238 | accepted | `f1b22913a3b3fb2404978e65a9bfdf0c64bc80422a9921fd155d02ce8d7d0a14` |
| **`buy()`, real shard script, b = 4, referral 0.40 KAS, d = 0.13 KAS, R = 1.69 KAS** | **46,491** | **158,924** | 109,473 | 109,473 | 2 | 0.5000 | **accepted** | `978f08b67bc14b1a24e55b2fa47ccd21c28d1db4bf9700c6c0682d04f694de00` |
| setup: genesis shard (b = 1) | 2,083 | 1,412 | 23,668 | 23,668 | 1 | 0.0238 | accepted | `d8a820d9efb2bca145f87ea4308a2795433f9317af9ec174daff2d84e7239426` |
| **`buy()`, real shard script, b = 1, referral 0.10 KAS, d = 0.13 KAS, R = 1.69 KAS (D80 worst shape)** | **43,884** | **151,696** | **423,125** | 423,125 | 2 | 0.5000 | **accepted** | `2263d66cc5b96bdd532b37798be5abb52b4a600aed06b920bb11ba023f731070` |

Compute and transient equal M6-a exactly (b = 4 referral 46,491 / 158,924; b = 1 referral 43,884 / 151,696). Storage differs with the output values. The b = 1 referral buy is 423,125 (84.6 %) against M6-a's 422,352 (84.5 %, change 1,000,000 KAS); the b = 4 buy is 109,473 against 108,704 (large change). The b = 1 shape at d = 0.13 is the one D80 brought under 90 %; at d = 0.11 it was 478,296 (M4-d). An earlier run the same day (b = 4 only, before the probe gained the b = 1 case) gave the same b = 4 masses (storage 109,476): `40bfedaeeec6868365cbc68b83c8a0600c104a5f722cea0c7800f736616a7baa`.

**`claim-m4`, first M6 run (its draw is SUPERSEDED by the Stage 5 RV-1 re-run below: the draw spends the vault, so it carries the vault redeem)** (node `vector-10.kaspa.green`, DAA ≈ 595,209,816):

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, real templates, R = 1.69 KAS | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted | `945762f9cd24b9855e5bfa0779c59758192e9455504fc7fba3eea5e4ecc645fa` |
| setup: `buy()` of the jackpot combination [5, 11, 19, 23, 24] + 7 on shard 0 | 43,472 | 151,488 | 322,550 | 322,550 | 2 | 0.3226 | accepted | `9a380a2d75b05f668949e0edbd5a30c1fe4cf26a180138a498f91290557b3511` |
| setup: `close()`, K = 16, one sale of 1 KAS | 294,619 | 867,716 | 426,281 | 433,858 | 19 | 0.4340 | accepted | `1906f7b9b3a4cb3a02dd1f8f8a354834856323bfece284b94e46c75924126789` |
| setup: `draw_test_seed()`, K = 16, a = 0, np = 4, h_J = 1, n2 = 0, R = 1.69 KAS | 141,986 | 448,864 | 130,525 | 224,432 | 5 | 0.2245 | accepted | `75eff605977608fb617aeea00cb1842780c5f1cd8e00ab78fd749502b827348c` |
| **`claim()` FINAL on the jackpot pool: K = 16, m = 10, h = 1, prize 10.0000 KAS, payout 10.1300 KAS, pool' 1.6900 KAS (R)** | **47,503** | **152,052** | **24,626** | 76,026 | 3 | 0.0761 | **accepted** | `a4b593bd0da1d7e41e32b7c722c0c988793731d9f14d70ed5fea60934dcd2a78` |

Round result from the draw bytes: round 1, win [5, 11, 19, 23, 24, 7], D_claim 601,690,509, pools (tier, slice, value) [(0, 1000000000, 1169000000), (2, 32000, 173000000), (3, 2666, 172000000), (4, 1000, 172000000)].

Comparison with the measured shapes:
- **The claim** has the same compute and transient as M4-i's run (47,503 / 152,052); the ticket script did not change in M6. M6-a's FINAL row is the m = 50, h = K shape (49,083 / 157,172), not this m = 10, h = 1 one. The payout is J + d = 10.13 KAS at d = 0.13.
- **The buy** equals M6-a b = 1 without referral (43,472 / 151,488).
- **`close()`** equals M6-a (294,619 / 867,716).
- **The draw** (141,986 / 448,864) is the m = 10, a = 0 shape with np = 4 pools. M6-a's typical draw has np = 3 (141,229 / 448,516), so this one carries one more pool output.
- **The genesis** storage (461,180, 92.2 %) is U-M6-1's exempt shape.

**`vault-m5`, first M6 run (SUPERSEDED by the Stage 5 RV-1 re-run below: the vault script then had W4b without the remainder rule; kept for the record)** (node `vector-10.kaspa.green`, DAA ≈ 595,210,933). Rescaled for M6:
- **Deposit:** D = 12 KAS (≥ MIN_DEPOSIT = 10 KAS).
- **Withdrawal:** partial, w = wmin = `game::w_min(6,000,000,000, 6,000,000,000)` = **1,000,000,166 shares**. That is exactly the W4b boundary at the vault' it spends (total_assets = V0 = 60 KAS, total_shares = S0 = V0 at SHARE_UNIT = 1), and it passed the builder policy `WithdrawCase::checked()`.
- **The δ − 1 negative case** is kept.

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, real templates, R = 1.69 KAS | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted | `7d6651120fe6b1023d7dd9af3dd6d98892fbbd97305d7a2889cc7068b4fc6eff` |
| **negative: `request_deposit()` at vault age δ − 1 (lock_time = creation 595,210,970 + 599 = 595,211,569)** | 41,485 | 77,540 | 24,199 | 41,485 | 2 | 0.0389 | **REJECTED: "failed to verify the signature script: Unsatisfied lock time: locktime requirement not satisfied -- locktime is greater than the transaction locktime: 595211570 > 595211569"** | `f9d84a860d78d17a3839060131091130901c86b6c83347e3e615bdfe17e0ebd3` |
| **`request_deposit()`, real vault script: D = 12.00 KAS** | **23,185** | **77,540** | 24,199 | 38,770 | 2 | 0.0389 | **accepted** | `eb865a3b0c93df2c2b84b6b8cb7f799c06e9e21912311dac6329777152efbc86` |
| **`request_withdraw()`, partial, w = 1,000,000,166 = wmin of 6,000,000,000 shares, P2PK Schnorr owner** | **25,276** | **82,824** | 47,849 | 47,849 | 3 | 0.0479 | **accepted** | `74d3a57d7a239604d8b6339f8bcf339e67dbffb0045589183270a54d7de313a4` |
| setup: `buy()` of the jackpot combination [2, 6, 9, 10, 21] + 7 on shard 0 | 43,472 | 151,488 | 322,550 | 322,550 | 2 | 0.3226 | accepted | `e171b72bbd507d0c9ea1b45b7a62c387cc56f9330be02ca67ff2589db642909e` |
| setup: `close()`, K = 16, one sale of 1 KAS | 294,619 | 867,716 | 426,244 | 433,858 | 19 | 0.4340 | accepted | `4fd32210179034bbdb76a2ba2850f7766b926b351a6a2d1b6d24ef5487828e0c` |
| **`draw_test_seed()`, K = 16, a = 0, np = 4, h_J = 1, n2 = 0; conversion D = 12.00 KAS → mint 1,418,999,053 shares, W = 1,000,000,166 → wc = W, out_c = 8.4567 KAS; record at output 7** | **142,547** | **449,228** | 134,410 | 224,614 | 5 | 0.2247 | **accepted** | `1a004c020444f457f3d86ddae9ed73e5a44ab72e23bae7562e927423b101a0e1` |
| **`settle()`: deposit-convert, s = 12.00 KAS → position of 1,418,999,053 shares; record' kept** | **22,147** | **73,388** | 27,581 | 36,694 | 3 | 0.0368 | **accepted** | `3e56e413793f510ee68a8538e5aa762bbcc3ebfc204bb0133db201c61bfe6db2` |
| **`settle()`: withdrawal FINAL, w = 1,000,000,166 shares, payout 10.1467 KAS (pay + R), record drained** | **21,857** | **73,108** | 6,873 | 36,554 | 3 | 0.0367 | **accepted** | `7b899c3cc8f31536208945a90bf6491ed005026be694a1dd21efc6fd354a4a36` |

Round result: round 1, win [2, 6, 9, 10, 21, 7], D_claim 601,692,908, the same four pools as `claim-m4`, plus the record.

The masses equal M6-a / M6-i:
- `request_deposit` 23,185 / 77,540 (M6-a, D = 10 KAS);
- `request_withdraw` partial at the W4b boundary 25,276 / 82,824 (M6-i, w = wmin; 1 compute and 4 transient below the generic partial row 25,277 / 82,828, because the sigscript is 1 B shorter);
- settle deposit-convert 22,147 / 73,388;
- settle withdrawal FINAL drained 21,857 / 73,108;
- `close()` 294,619 / 867,716.

The converting draw is 142,547 / 449,228. M5's run was 142,504 / 449,056; the difference, +43 compute and +172 transient, is the vault redeem's +42 B (check W4b; 168 transient at 4 grams per byte) plus 1 B for the larger push of W (1,000,000,166 is a 4-byte script number, M5's 60,000 a 3-byte one). The withdrawal is worth about 10 KAS (out_c 8.4567 KAS + the request's R = 10.1467 KAS payout; the gap to W_MIN_VALUE comes from the share price after the draw's jackpot payout and the deposit's mint, not the request-time price π of the W4b bound). The node rejected the early deposit with check D10's CLTV error, as in M5.

**`vault-m5`, re-run after the Stage 5 RV-1 fix (current; 2026-10-13)** (node `vector-10.kaspa.green`, DAA ≈ 595,238,862; log: orchestrator scratchpad `vault-m5_rv1.txt`, 0 hits for "PRIVATE"). The vault script now has W4b with the remainder rule (orchestrator decision O-M6-2: a partial withdrawal needs w ≥ wmin and shares − w ≥ wmin; vault redeem 3,432 B, +10 B). Same values as the first run: D = 12 KAS; w = wmin = `game::w_min(6,000,000,000, 6,000,000,000)` = 1,000,000,166 shares from the deployer's 6,000,000,000-share position, so the remainder is 4,999,999,834 ≥ wmin and the builder policy `WithdrawCase::checked()` (which now includes the remainder rule) accepts it.

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, real templates, R = 1.69 KAS | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted | `7722bd3818521fde39d718b9ed49241187902632cadf0f953aa530802447fa2d` |
| **negative: `request_deposit()` at vault age δ − 1 (lock_time = creation 595,238,911 + 599 = 595,239,510)** | 41,495 | 77,580 | 24,199 | 41,495 | 2 | 0.0389 | **REJECTED: "… Unsatisfied lock time: locktime requirement not satisfied -- locktime is greater than the transaction locktime: 595239511 > 595239510"** | `0b3cee648606c2e9622f5cf8d6c6b577cf34cc37755d122cecaa7ff194ecbda4` |
| **`request_deposit()`, real vault script: D = 12.00 KAS** | **23,195** | **77,580** | 24,199 | 38,790 | 2 | 0.0389 | **accepted** | `952e46398dc7cc4129f846c62e4b19c6754f481b5fcfa16af906e20df05bce88` |
| **`request_withdraw()`, partial, w = 1,000,000,166 = wmin of 6,000,000,000 shares (remainder 4,999,999,834 ≥ wmin), P2PK Schnorr owner** | **25,286** | **82,864** | 47,849 | 47,849 | 3 | 0.0479 | **accepted** | `d5cea0c1de398fcb49eab623741d9a303f58f0a2385260b019f55ebf22d85ec6` |
| setup: `buy()` of the jackpot combination [3, 13, 14, 20, 21] + 5 on shard 0 | 43,472 | 151,488 | 322,550 | 322,550 | 2 | 0.3226 | accepted | `ffce1240f4600118d3215155c75945690e89276308487bd5260ec5cc43fb6200` |
| setup: `close()`, K = 16, one sale of 1 KAS | 294,619 | 867,716 | 426,207 | 433,858 | 19 | 0.4340 | accepted | `2a3056e0ad013858bee6c7c8e3ad020980fde259419c2553ceba3a36902a05e2` |
| **`draw_test_seed()`, K = 16, a = 0, np = 4, h_J = 1, n2 = 0; conversion D = 12.00 KAS → mint 1,418,999,053 shares, W = 1,000,000,166 → wc = W, out_c = 8.4567 KAS; record at output 7** | **142,557** | **449,268** | 134,410 | 224,634 | 5 | 0.2247 | **accepted** | `fa2980bea369105b22fa3f2418b7c9067e9e36376688aca79952763ade7c0839` |
| **`settle()`: deposit-convert, s = 12.00 KAS → position of 1,418,999,053 shares; record' kept** | **22,147** | **73,388** | 27,581 | 36,694 | 3 | 0.0368 | **accepted** | `e476a247b41b39a0044a77af1378625f73161c35b087d3696d5d58643a1b27c5` |
| **`settle()`: withdrawal FINAL, w = 1,000,000,166 shares, payout 10.1467 KAS (pay + R), record drained** | **21,857** | **73,108** | 6,873 | 36,554 | 3 | 0.0367 | **accepted** | `ff94bba0b31da4eef984f9e437ecfd285598afc21db21543406e7c5568763a22` |

Round result: round 1, win [3, 13, 14, 20, 21, 5], D_claim 601,720,854, the same four pools (tier, slice, value) as the first run, plus the record.

Against the first run, every transaction that carries the vault redeem grew by exactly the +10 B: `request_deposit` +10 / +40 (23,195 / 77,580), `request_withdraw` +10 / +40 (25,286 / 82,864), the draw (vault leg) +10 / +40 (142,557 / 449,268). The requests equal the re-measured M6-a / M6-i rows (`request_deposit` 23,195 / 77,580; M6-i 25,286 / 82,864). Settlements, `close()`, the buy and genesis are unchanged (they do not carry the vault redeem). The negative D10 case was rejected by the node as before.

**`claim-m4`, re-run after the Stage 5 RV-1 fix (current; 2026-10-13)** (node `electron-10.kaspa.blue`, DAA ≈ 595,241,800; log: orchestrator scratchpad `claim-m4_rv1.txt`, 0 hits for "PRIVATE"). Re-run because its `draw()` spends the vault (`draw_leg`) and so carries the vault redeem (+10 B). `buy-m1` and `conflict-m6` were not re-run: they submit only genesis and `buy()`, which carry no vault script, and the vault template hash feeds only the position and request templates (`lottery::buy::Templates::compile_sources`), so their shard, ticket, controller and params scripts are byte-identical.

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, real templates, R = 1.69 KAS | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted | `db4851b9f461b5199e8945ada67fe0db4f280ae377ba2531d31eda15f015b210` |
| setup: `buy()` of the jackpot combination [10, 13, 15, 17, 28] + 1 on shard 0 | 43,472 | 151,488 | 322,550 | 322,550 | 2 | 0.3226 | accepted | `7c542d4efca73f6e0c5f8e9557c4c44cc2d1f77ac5eddf97ce0fe543ab85754d` |
| setup: `close()`, K = 16, one sale of 1 KAS | 294,619 | 867,716 | 426,170 | 433,858 | 19 | 0.4340 | accepted | `36d369250fb01ee68edea4e7dd13ec04d48bbd3a31d41a9ca373e85f50d96e92` |
| setup: `draw_test_seed()`, K = 16, a = 0, np = 4, h_J = 1, n2 = 0, R = 1.69 KAS | **141,996** | **448,904** | 130,516 | 224,452 | 5 | 0.2246 | accepted | `005e5f10336dc832211a1ddc9049e7c9829363ca6eb9fd9d1faf53d3ef4c60a8` |
| **`claim()` FINAL on the jackpot pool: K = 16, m = 10, h = 1, prize 10.0000 KAS, payout 10.1300 KAS, pool' 1.6900 KAS (R)** | **47,503** | **152,052** | **24,621** | 76,026 | 3 | 0.0761 | **accepted** | `4a01521f648dbdb14dce63945716f7f3e5b9f9664fe638a9bc2d8771825f251b` |

Round result: round 1, win [10, 13, 15, 17, 28, 1], D_claim 601,722,479, the same four pools as the first run. The draw is +10 / +40 against the first run (141,986 / 448,864), exactly the vault redeem's +10 B; genesis, buy, `close()` and the claim have the same compute and transient as before.

Locked on testnet by the re-runs (no further entry exercised): for each of the four `claim-m4` and `vault-m5` runs (the first M6 runs and the RV-1 re-runs, genesis `945762f9…`, `7d665112…`, `db4851b9…`, `7722bd38…`), the round 2 objects, drawer', vault' and the four pools, and in both `vault-m5` runs also the new position and position'; for both `buy-m1` runs, the shard successors and the tickets. Payouts, referrals, refunds and changes are ours.

**Balance (all M6 runs):** 8,022.88 testnet KAS before; 7,542.77 after `conflict-m6` runs 2 and 3, both `buy-m1` runs and the first `claim-m4` and `vault-m5` runs (480.11 KAS locked or paid in fees); **7,333.81 after the RV-1 re-runs of `vault-m5` and `claim-m4`** (208.96 KAS), 689.07 KAS in total.

## M7: real anchor `draw()`, void, and the M4/M5 probes on top of it (spec D103, A9; 2026-10-13)

All runs: `cargo run -p tn10 --release -- <command>` from `crates/`, public testnet-10 nodes found by the resolver (all version **2.1.0**, network `testnet-10`, synced, UTXO index on). Every transaction is built by the `lottery` builders and checked in the local v2.1.0 engine before submission; every input is the node's chain-merged entry (M3 S5-5).

### Stage 5 fix re-run (272-byte anchor proof, void floor in the drawer; 2026-10-13)

The scripts after the M7 Stage 5 fix: the anchor proof is **272 B** (X 152 B ‖ P's preimage fields without P.hash, 120 B), the drawer makes one `OpChainblockSeqCommit` call (on X.hash) and authenticates P by `SeqCommit(P) == X.parent_commit`; check 1 requires `draw_window >= VOID_MIN = 864,000`. `void-m7` therefore runs under the **TEST-ONLY short-void drawer variant** (the same `drawer.sil` with `int constant VOID_MIN = 864000;` replaced by `= 0`), which only the genesis flag `allow_short_void_window` selects (`GenesisCase::templates()`; refused off testnet/devnet); its template set (drawer and every template committing to it) differs from production. Balance before: 6,914.69 testnet KAS; after both runs: **6,709.55** (205.15 KAS locked or paid in fees). Both probes exited 0 on the first attempt.

`draw-m7`, node `vector-10.kaspa.green`, DAA ≈ 595,681,446, covenant ID `dcc269b83c894d167f5d9ce4d2ee2a8518e4b1eddb67cf9a2f014180a256c468`; `close()` merged at DAA 595,682,123, so D = 595,682,723:

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, real templates, R = 1.69 KAS, seed_offset 600, draw_window 864,000 | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted | `8b7c7aad8ee31d0f64027a55cc281eace5a30407211dbe5b8cb11998da476df4` |
| setup: `buy()` of [1, 2, 3, 4, 5] + 1 on shard 0 | 43,472 | 151,488 | 322,550 | 322,550 | 2 | 0.3226 | accepted | `70fa1c9dee66602a19a8cad56c9dba9517ddd38749a316f605406ae27cccf82a` |
| setup: `close()`, K = 16, one sale of 1 KAS | 293,743 | 864,612 | 426,022 | 432,306 | 19 | 0.4324 | accepted (merged at DAA 595,682,123) | `e5d8914e3dc65582d22d5c20d5a7a8d6c61075c2d4c43c87df44838a4372b6e6` |
| **`draw()` with the real 272-byte anchor, K = 16, a = 0, np = 3, h_J = 0, n2 = 0, fee paid, lock_time = D = 595,682,723** | **139,891** | **443,964** | **126,964** | 221,982 | 5 | 0.2221 | **accepted; visible after 1.8 s** | `9de5af6e4c2422988fa7b7a66913f9aa1d8bbda477ec920f4b04e7ab18840553` |

Anchor: X `88fab88b324f05241d2727e7074e743d1464c68408d796b196ecce3b399edd80` (DAA 595,682,723 = D, blue score 584,105,255, mergeset 2 blues; SeqCommit `a1c2acbd…52f2` = header), P `de5041b483cd0b15120a64ed1a1a69e52d411a1938dc459ed15084fd827a0730` (DAA 595,682,721, blue score 584,105,253, mergeset 1 blue; SeqCommit `0416ce2e…20fc` = header; P.hash is no longer in the proof). Consensus mergeset order matched for both. The pre-close sink had left the chain (2 removed chain blocks); `find_anchor` walked from the fork point. Round result: round 1, not void, win [1, 8, 11, 23, 27, 8], D_claim 602,594,723 (= D + 864,000 + 6,048,000), pools (tier, slice, value) [(2, 32000, 173000000), (3, 2666, 172000000), (4, 1000, 172000000)].

`void-m7`, node `quark-10.kaspa.red`, DAA ≈ 595,683,456, covenant ID `1d3173e8dee511630e2fedb9d5eae9c1d4751b59ce9ce89af07c4dcafb13957b`; `close()` merged at DAA 595,684,109, so D = 595,684,709 and the void lock D + 1,200 = 595,685,909 (reached 177 s after the close):

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, R = 1.69 KAS, seed_offset 600, **draw_window 1,200 (TEST-ONLY flag, short-void drawer variant)** | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted | `835004fdc3387192caa6c73d4d1f921b15e407b0a4546bc87f993eb1ada446c2` |
| setup: `buy()` of [1, 2, 3, 4, 5] + 1 on shard 0 | 43,472 | 151,488 | 322,550 | 322,550 | 2 | 0.3226 | accepted | `1fe01250d1a83c77b76ce0264d5497afc4ac3ccf9e60ff7ba42d6925ecaf7c23` |
| setup: `close()`, K = 16, one sale of 1 KAS | 293,743 | 864,612 | 425,985 | 432,306 | 19 | 0.4324 | accepted (merged at DAA 595,684,109) | `17c49523a164300cd01613ae984166e7cd2031d8900037ec81daa5852dcccfc9` |
| **void: `draw()` with an empty anchor, K = 16, a = 0, np = 0, fee paid, lock_time = D + 1,200 = 595,685,909** | **123,103** | **403,652** | **50,204** | 201,826 | 5 | 0.2019 | **accepted; visible after 1.7 s** | `736d5a15172fc56208ce9ec92974af3186010b4e42b7d671a0681b6cfe0ec9ce` |

Round result: round 1, **void**, X.hash zero, win [0, 0, 0, 0, 0, 0], D_claim 601,733,909, no pools. With the production drawer this instance could neither void nor draw (check 1); locally SA12 shows both rejections and the negative control.

Comparison with `docs/costs.md` M7 (after the fix): the typical draw 139,891 / 443,964 equals the local typical draw exactly (28.0 / 44.4 %; −46 compute / −184 transient against the Stage 4 run below: the anchor −32 B and the drawer redeem −14 B). The void 123,103 / 403,652 is 3 compute / 12 transient grams below the local production void (123,106 / 403,664): the variant's `VOID_MIN = 0` is a 1-byte push where 864,000 takes 4, so its redeem is 3 B shorter. `close()` is unchanged (58.7 / 86.5 %).

### Stage 4 runs (history: 304-byte anchor proof with P.hash and two opcode calls; no void floor in the drawer)

The M7 Stage 2 scripts (drawer `draw()` with the 304-byte anchor proof and the void mode, marker without `seed_ref`/`refresh()`). Balance before: 7,333.81 testnet KAS; after the four runs below: **6,914.69** (419.12 KAS locked or paid in fees). These runs used the production template set for `void-m7` too (the drawer had no void floor then).

**How the anchor is built (`crates/tn10/src/anchor.rs`).**
- `find_anchor(start, D)`: `GetVirtualChainFromBlock(start)` from the sink taken before `close()` was submitted, then a binary search over the added chain blocks by header DAA (non-decreasing along the chain) for the first block X with DAA ≥ D. P = X's verbose `selected_parent_hash`; it must equal the previous added chain block and have DAA < D. A start that a reorg removed is handled (the walk continues from the fork point; seen once, in `claim-m4`: "5 removed chain blocks").
- `anchor_from_rpc(hash)`, for X and for P: `GetBlock(hash)` (header: `accepted_id_merkle_root` = SeqCommit, DAA, blue score; verbose: selected parent, mergeset blues and reds, `is_chain_block`), the selected parent's header (timestamp, `accepted_id_merkle_root`), `GetBlock(h, true)` of every mergeset block (blue work; coinbase payload = transaction 0's payload), and `GetSeqCommitLaneProof(hash, COINBASE_LANE_KEY)` (`parent_seq_commit`, `payload_and_ctx_digest`, `inactivity_shortcut`, the SMT proof and the lane entry). Then `lanes_root = OwnedSmtProof::compute_root::<SeqCommitActiveNode>(COINBASE_LANE_KEY, smt_leaf_hash(tip, bs))`, `activity_root = activity_root_hash(inactivity_shortcut, lanes_root)` and `mpr = miner_payload_root(miner_payload_leaf(hash, blue_work BE, payload) per mergeset block)`.
- Self-checks before anything is built (D103): `H_seq(H_mc(sp.ts ‖ daa ‖ bs) ‖ mpr) == payload_and_ctx_digest`; `parent_seq_commit` = the selected parent's `accepted_id_merkle_root`; the full KIP-21 recomputation (`kaspa_seq_commit`) = the header's `accepted_id_merkle_root`; `lottery::anchor::seq_commit` over the A3 bytes (the drawer's formula) = the same; `pc == x_pc`, dX ≥ D, 0 ≤ dP < D. The local engine then runs the draw against `harness::ChainSim::from_real_chain(P's parent commitment, [P, X])`, so `OpChainblockSeqCommit` returns the real commitments locally too.
- The probe waits for 50 blue score of confirmations on X, and checks again that X is still a chain block right before submission.

**Mergeset order (open point of the handoff).** For the miner payload root, consensus orders the mergeset as **the selected parent first, then the rest of the mergeset (blues and reds together) ascending by (blue_work, hash)** (`consensus_ordered_mergeset`, rusty-kaspa v2.1.0 `consensus/src/model/stores/ghostdag.rs:116-180`; `consensus/src/pipeline/virtual_processor/utxo_validation.rs:116-125, 465-490`; `consensus/src/processes/ghostdag/ordering.rs:38-42`). The probe tries this order first and three alternatives after it. It matched every time:
- `cargo run -p tn10 --release -- anchor-check 300` (node `quark-10.kaspa.red`, DAA ≈ 595,634,600; nothing submitted): **97 consecutive chain blocks** rebuilt from RPC, every recomputation = the header, consensus order matched in 97 of 97. Mergesets of 1 (44 blocks), 2 (30), 3 (16), 4 (5) and 5 (2) blues. **No red block occurred**, so the place of reds in the order rests on the source citation above, not on observation.
- The four probe anchors below (X and P each): consensus order matched in 8 of 8, with mergesets of 1–3 blues.

**KIP-21 activation state.** `OpChainblockSeqCommit` is active on testnet-10 with KIP-21 semantics (accessor condition 4, `refs/kips/kip-0021/seqcommit-accessor.md` "Opcode Mechanics"). Three draws were accepted and mined, and in each the drawer compares the opcode's result with the KIP-21 recomputation of X and of P. Every `accepted_id_merkle_root` read (105 chain blocks) equals the KIP-21 recomputation. rusty-kaspa v2.1.0 has no Toccata activation parameter (`consensus/core/src/config/params.rs:796-798` rejects `toccata_activation` as unknown), which is consistent with that.

#### `draw-m7` (genesis → buy → close → `draw()` with the real anchor)

Node `electron-10.kaspa.blue`, DAA ≈ 595,636,803. Covenant ID `2e5562983d2c5d2399404b367b75d2101c531dbecbad6fb4d2507ff66bcf4888`. The buy holds one fixed combination ([1, 2, 3, 4, 5] + 1). The outcome is unknown until X exists, so this is a "jackpot not held" draw (DP4 shape); the three lower-tier pools come from the 1 KAS sale. `close()` was merged at DAA 595,637,459 (the marker's creation DAA, mk_daa), so **D = 595,638,059**.

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, real templates, R = 1.69 KAS, seed_offset 600, draw_window 864,000 | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted | `45efc4ca7af0ce2e1d16556abab341aba768cf6e07ada669c74151a13fcc21bd` |
| setup: `buy()` of the fixed combination [1, 2, 3, 4, 5] + 1 on shard 0 | 43,472 | 151,488 | 322,550 | 322,550 | 2 | 0.3226 | accepted | `4c8738bfcf80a15e3d30a25bb598ed69e3cbae8579ccb9603dc8268bc69e7c2a` |
| setup: `close()`, K = 16, one sale of 1 KAS | 293,743 | 864,612 | 426,170 | 432,306 | 19 | 0.4324 | accepted (merged at DAA 595,637,459) | `6d47cafadd5fd7da55e8b5961b408ec65499fe9c58dbf95468036efd7402117e` |
| **`draw()` with the real anchor, K = 16, a = 0, np = 3, h_J = 0, n2 = 0, fee paid, lock_time = D = 595,638,059** | **139,937** | **444,148** | **126,973** | 222,074 | 5 | 0.2222 | **accepted; visible after 1.7 s** | `52fe7e03ccee64e39d33565a90f9e529d9c375e158fd4f1cf5c73f764baaf63f` |

Anchor:
- **X `9977b8a0033615ea67e76e0ea93da05271080ff3953c1ffe47b41201ac735551`**: DAA **595,638,059** (= D), blue score 584,060,591, mergeset 1 blue; SeqCommit(X) `10eaced9543aaa300588cf5d5a78f7468f7f623022f0b3fe1ea2e8d996c6076e`.
- **P `06972b599ac321a4f8fa8c40c43fa2f1d2634a8c7fa5a37fc162532fede7b557`**: DAA 595,638,058, blue score 584,060,590, mergeset 2 blues; SeqCommit(P) `26a529a7fad62fd1534df95c3a35545402752c7a6d1fb4623842c71e8c11a700`.
- Coinbase lane active in both. X was 339 chain blocks after the pre-close sink.

Round result from the draw bytes (`RoundResult::from_tx`): round 1, not void, X.hash = X above, mk_daa 595,637,459, win [6, 12, 16, 18, 25, 10], D_claim **602,550,059** (= D + 864,000 + 6,048,000), pools (tier, slice, value) [(2, 32000, 173000000), (3, 2666, 172000000), (4, 1000, 172000000)].

#### `void-m7` (TEST-ONLY genesis with draw_window = 1,200 → buy → close → void)

Node `vector-10.kaspa.green`, DAA ≈ 595,639,243. Covenant ID `61934f9bc046953b5eb4e29447d1681c7ab4d0dad076b7711098b2bccc012ae4`. Genesis sets the TEST-ONLY `allow_short_void_window` (`GenesisCase::validate` accepts it only because the network is testnet) and draw_window = 1,200. `close()` was merged at DAA 595,639,899, so D = 595,640,499 and the void lock D + 1,200 = 595,641,699 (reached 170 s after the close).

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, R = 1.69 KAS, seed_offset 600, **draw_window 1,200 (TEST-ONLY)** | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted | `b5ce622ecdc4f6ac5970069d30b409170ce021ac9c8de81d626b8b782e93a04a` |
| setup: `buy()` of the fixed combination [1, 2, 3, 4, 5] + 1 on shard 0 | 43,472 | 151,488 | 322,550 | 322,550 | 2 | 0.3226 | accepted | `b7a0ba66f8d97a91db2f485b89ac96dfc1fd907e2ff53119e12dab8bc38489e9` |
| setup: `close()`, K = 16, one sale of 1 KAS | 293,743 | 864,612 | 426,133 | 432,306 | 19 | 0.4324 | accepted (merged at DAA 595,639,899) | `fff997e5e70cb7a870610b26e5ce05d2555ab43bab5def67217b25c204797e48` |
| **void: `draw()` with an empty anchor, K = 16, a = 0, np = 0, fee paid, lock_time = D + 1,200 = 595,641,699** | **123,120** | **403,720** | **50,204** | 201,860 | 5 | 0.2020 | **accepted; visible after 0.6 s** | `a8ce26db4455343dedc58b59326a938cfbd29deb49ad41e868028913cbafdd6e` |

Round result from the void's bytes: round 1, **void**, X.hash zero, mk_daa 595,639,899, win [0, 0, 0, 0, 0, 0], D_claim 601,689,699 (= D + 1,200 + 6,048,000), no pools. With the TEST-ONLY window the anchor was still inside the opcode window when the void was accepted, so a draw would also have been valid. The A5 argument that a void and a draw cannot both be valid needs draw_window ≥ 2F; it is shown locally (SP9), not here.

#### `vault-m5` on the real anchor

Node `vector-10.kaspa.green`, DAA ≈ 595,642,295. Covenant ID `7cffbbd4f9867914af21ef21263b88b455bdc0248636c534b8c7061831035793`. Same values as the M6 RV-1 run (D = 12 KAS, w = wmin = 1,000,000,166 shares, the δ − 1 negative case). The draw uses the real anchor:
- **X `dcd34e71211ad3ac6401a04561df8b3c3c34b1179b2787d1752d70f212cf2a49`**: DAA 595,645,338 = D, blue score 584,067,870, mergeset 3 blues.
- **P `d949810036ddd464300f3d86c39b4894a35318ee93511c9a8ccaa0fe9dfb4daf`**: DAA 595,645,335, blue score 584,067,867, mergeset 3 blues.

P and X are 3 DAA apart.

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, R = 1.69 KAS | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted | `8802582c745d814e7929bd67c424506b0483bb8e7ef8117aac51cc83601ff23f` |
| **negative: `request_deposit()` at vault age δ − 1 (lock_time = 595,642,330 + 599)** | 41,495 | 77,580 | 24,199 | 41,495 | 2 | 0.0389 | **REJECTED: "… Unsatisfied lock time: … 595642930 > 595642929"** (check D10, as before) | `b6a2a527831f56a22cf1c086fba4b42ee9b5ee986e16b8de701aeaa2e07d0338` |
| `request_deposit()`: D = 12.00 KAS | 23,195 | 77,580 | 24,199 | 38,790 | 2 | 0.0389 | accepted | `af25b8b11450f91a97f48b9be26f06f61f3ef393c69c3e00f04a92badc3e9ab9` |
| `request_withdraw()`: partial, w = 1,000,000,166 = wmin | 25,286 | 82,864 | 47,844 | 47,844 | 3 | 0.0479 | accepted | `c4cca7b16ccf7ef45ee1a7ca2a6867274e58194c122b9dcf0e081afb223bebad` |
| setup: `buy()` of [1, 2, 3, 4, 5] + 1 on shard 0 | 43,472 | 151,488 | 322,550 | 322,550 | 2 | 0.3226 | accepted | `3be4d95bce11ac685e6d1a706c9f98691484fbf6c4fac2bcbf87c0f89c5d3544` |
| setup: `close()`, K = 16, one sale of 1 KAS | 293,743 | 864,612 | 426,096 | 432,306 | 19 | 0.4324 | accepted (merged at DAA 595,644,738) | `5458cf19306a12151980320b8192a982302525db51f9294d8a6f41d8cd109c5d` |
| **`draw()` with the real anchor, np = 3, h_J = 0; conversion D = 12.00 KAS → mint 1,185,380,506 shares, W = wc = 1,000,000,166, out_c = 10.1233 KAS; record at output 6** | **140,498** | **444,512** | 130,332 | 222,256 | 5 | 0.2224 | **accepted** | `e9c097ec2d80632cab791289f5d56c7cb7bb341532e95164ef93c3455f7e0aaa` |
| `settle()`: deposit-convert → position of 1,185,380,506 shares; record' kept | 22,147 | 73,388 | 27,020 | 36,694 | 3 | 0.0368 | accepted | `2cada5d06e4e0f6633caca41a0c1dc42196f55d7d60eb121c794f01defc7589d` |
| `settle()`: withdrawal FINAL, payout 11.8133 KAS (pay + R), record drained | 21,857 | 73,108 | 6,729 | 36,554 | 3 | 0.0367 | accepted | `acb18089711fa37cd1aa84d5f4174f244e7e0c3877f75d807331f001e18a0555` |

Round result: round 1, X.hash = X above, win [5, 6, 17, 21, 28, 8], D_claim 602,557,338, the three lower-tier pools, plus the record. Requests, settlements, buy and genesis have the same compute and transient as in the M6 RV-1 run: the vault, request, record and position scripts did not change in M7.

#### `claim-m4` on the real anchor (claim not exercised)

Node `quark-10.kaspa.red`, DAA ≈ 595,646,047. Covenant ID `aa61cac2e6cf840dd11275545253ecf7a6d74f9cf09d0601fe1ef6ce0ca1f070`.

**What had to change.** D101 deleted the TEST-ONLY seed, so the winning numbers now come from X, which exists only after `close()`; a winning ticket cannot be prepared in advance. The probe now buys the fixed combination, draws with the real anchor, and claims only if the ticket falls in a tier whose pool exists (≈ 0.3 % at m = 10). Otherwise it records that the claim was not exercised and ends without error. Here the ticket did not win. The M6 RV-1 `claim-m4` run above remains the claim evidence; `ticket.sil` and `pool.sil` changed only in comments in M7.

| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |
|---|---|---|---|---|---|---|---|---|
| setup: genesis, K = 16, R = 1.69 KAS | 11,223 | 8,372 | 461,180 | 461,180 | 1 | 0.4613 | accepted | `30a8c9b2e427196b99b991aa6b38a0bc39ac890797e19ff6023d481263f916d4` |
| setup: `buy()` of [1, 2, 3, 4, 5] + 1 on shard 0 | 43,472 | 151,488 | 322,550 | 322,550 | 2 | 0.3226 | accepted | `4d4a6532992f61126e1310e721328562428e23e987f7ec35c69b14d12c6f48a9` |
| setup: `close()`, K = 16, one sale of 1 KAS | 293,743 | 864,612 | 426,059 | 432,306 | 19 | 0.4324 | accepted (merged at DAA 595,646,693) | `20e16e142f376eff0d7e618cabe9734a22150ab062090597a4f49379ae8cc586` |
| **`draw()` with the real anchor, np = 3, h_J = 0, n2 = 0, lock_time = D = 595,647,293** | **139,937** | **444,148** | 126,964 | 222,074 | 5 | 0.2222 | **accepted** | `fb96ea4a0037221e1eed908c7f448d32e0315c9607351633c0124acc19fc1d6d` |

Anchor:
- X `9783a6d8f125f39e18535181f9c4d7a16e68c62767681f9fbff16e2b80203ee9`: DAA 595,647,293 = D, blue score 584,069,825, mergeset 2 blues.
- P `65d803574bad10aa178485df87b5269de450d38d622f1a161120a98ff28b16ba`: DAA 595,647,291, mergeset 1 blue.

By the time the anchor was looked up, the pre-close sink had left the chain (5 removed chain blocks); `find_anchor` walked from the fork point. Round result: win [1, 3, 8, 11, 18, 8], D_claim 602,559,293. The ticket [1, 2, 3, 4, 5] + 1 matches one number, so it is in no tier.

#### Comparison with the local measurements (`docs/costs.md` M7, Stage 3 values)

- **`draw()`** with the real anchor, typical (m = 10, a = 0, np = 3): 139,937 / 444,148 = **28.0 % / 44.4 %**, the same as the local typical draw of Stages 2–3 (28.0 / 44.4 %). The real anchor costs the same as the fabricated one: the proof had a fixed length (304 bytes then, 272 since the Stage 5 fix) and the script's work does not depend on the values.
- **Void**, typical: 123,120 / 403,720 = **24.6 % / 40.4 %**, the same as the local typical void (24.6 / 40.4 %). Storage is 50,204 (10.0 %) here vs 6.7 % locally, because the output values differ.
- **`close()`** K = 16: 293,743 / 864,612 = 58.7 / **86.5 %**, exactly M7-c. Against M6's 294,619 / 867,716 it is −876 compute and −3,104 transient, because the marker lost `seed_ref`.
- **Converting draw** (`vault-m5`): 140,498 / 444,512, which is +561 / +364 over the plain draw: the record output and the conversion work.

Locked on testnet (no further entry exercised): for each of the four runs, the round 2 objects, drawer', vault' and the new pools (none in the void); in `vault-m5` also the new position and position'. Payouts, the record's refunded R, refunds and changes are ours. `draw-m7-late` (the 13 h out-of-window rejection) was not run (optional, D103).

## M8: fast campaign C0-C7 on testnet-10 (spec 8 A2, A5; S3b, 2026-10-17)

`tn10 campaign start --until C7 --c6-deposit 50` (then `campaign resume`) from `crates/`, release build, public testnet-10 nodes from the resolver (all version **2.1.0**, network `testnet-10`, synced, UTXO index on): `vector-10.kaspa.green` (164 transactions), `proton-10.kaspa.stream` (96), `quark-10.kaspa.red` (40); `muon-10.kaspa.blue` served one resume without submissions. Every transaction went through `submit_transaction` with orphans disallowed, after the local v2.1.0 engine accepted it (expected refusals: rejected locally first, then by the node). Fee rate: the node's normal bucket, 100 sompi/gram throughout (D120). The indexer ran inside the runner for the whole campaign (v2 virtual chain, Full verbosity); the wallet and indexer stores fed every offline build.

**Instance (A2, production templates, no short-void variant):** covenant ID `e9f4f810c86467e7768aa1b42e6c62182b8a0d934bda922bdbb2804f223988d6`, genesis `e64f08175f833100814f1fd24d1a0d8e05e2dcefbf4828892515e400cdaabd57` (DAA about 598,543,920). K 16, B_MAX 4, margin 15,000, m 3 -> 2, round_len = d_min = 6,000, seed_offset 600, draw_window 864,000, V0 100 KAS, J1 10 KAS. Launch bundle `docs/launch/tn10-m8.json` (305 live UTXOs at DAA 598,620,952): `tn10 bundle-verify docs/launch/tn10-m8.json` **PASS** (covenant ID recomputed, 11 templates = the local compilation, genesis checks pass, every live UTXO present on `vector-10.kaspa.green` with its value, creation DAA and covenant ID).

**Owner amendments applied (PENDING DECISIONS rows in `docs/specs/8-handoff.md`):** the C6 deposit of 50 tKAS (so the carried withdrawals settle FINAL); the budget cap raised from 900 to 950 tKAS spent or locked (the approved 556 / 750 were end figures; the peak is higher); and **C5b/C5c shifted by one round**. The runner was paused for the budget check after round 2's buys, and round 2 was closed about 14 min after its deadline T with inputs no newer than T. `close()` sets T' = max(T + round_len, own + d_min) with own = the newest input's creation DAA (`controller.sil:473-484`), so round 3's deadline (598,562,019) fell before its own creation (598,564,744): round 3 could not sell (shard check 6). Round 3 was closed with no sales and the sales-pause test ran on round 4. The runner now funds every `close()` with a freshly accepted input and refuses a close that would leave under d_min / 2 of sales (finding for the S5 reviewer: a late closer with old inputs can create a no-sales round; no funds at risk).

### Result per step (62 checks, all ok)

| step | result | what was shown |
|---|---|---|
| C0 | pass | balance 6,709.55 >= budget 2,000; fee rate 100 sompi/gram |
| C1 | pass | genesis; covenant ID recomputed; bundle-verify against the node (21 live UTXOs, no mismatch) |
| C2 | pass | round 1 (m 3): 75 buys b = 4 over 16 shards (300 tickets), one with a 0.4 KAS referral; 9.99 KAS deposit refused; 20 KAS deposit; `set_params` s_R -> 500 (nonce 1); second `set_params` refused; the buy after `set_params` still pays the referral at s_R 1,000 (0.4 KAS) |
| C3 | pass | close r1: round 2 m' = 2 and s_R 500 in its controller and all 16 shards; wallet and indexer roots = frozen roots; draw 1 (real anchor): conversion and record = `game::convert` (deposit 20 KAS minted 615,460,425 shares), fee to round 2's fee address; deposit settled DepositConvert = `game::settle`; second settle refused |
| C4 | pass | wallet-store claims offline: a claim with one leaf count changed by 1 refused; 2 claims of round 1 (tier 4, ACCRUE; the partial claim) |
| C5 | pass | round 2 (m 2): 75 buys b = 4; b = 1 with a 0.05 KAS referral refused; b = 1 without accepted; b = 2 with a 0.1 KAS referral paid at s_R 500; depositor's partial withdrawal below wmin refused (w 303,240,973 of 615,460,425, wmin 312,219,453); the depositor's whole and the genesis backer's full (10^10 shares) withdrawals requested; close r2 (m' 2, roots equal) |
| C5b (shifted) | pass | draw 2 (late) = model; **liability lock**: W 10,615,460,425, wc 8,492,369,140 < W, vault after = L' = 116.866 KAS exactly (slack 0); both withdrawals settle **PAY** = model with carried requests for round 3 (pay 27.10 and 440.36 KAS); round 3 recorded as opened past its deadline and closed with no sales; round 4: 20 buys b = 1 before its deadline, **16 buys after it (one per shard whose UTXO predated T)**, a further post-deadline buy refused (sales pause), `close()` of round 4 before draw 3 refused (strict order) |
| C5c (shifted) | pass, one item not exercised | draw 3 held to D + 36,000 (**60 min late**, inside 12 h) = model (no pools, no fee output: no sales); carried withdrawals PAY = model; close r4 (fresh input, roots equal); **draw 4 built from the indexer store offline** = model; the round-4 claim from the indexer store **not exercised** (no winning round-4 holding) |
| C6 | pass | carried settles PAY after draw 4; round 5: 40 buys, **50 KAS deposit**, close, draw 5 = model (deposit converted to 31,047,840 shares); carried withdrawals settle **FINAL** = model (pay 7.93 and 128.83 KAS, nothing left); deposit DepositConvert = model; 1 wallet claim (tier 4, ACCRUE); round 6: 40 buys, close, draw 6 = model; m stays 2; no live withdrawal request left |
| C7 | pass | report written; every check of C0-C6 ok |

### Expected refusals (each rejected by the local engine at input 0, then by the node with the matching reason)

| check | txid (not accepted) | node reason |
|---|---|---|
| C2 request_deposit 9.99 KAS (MIN_DEPOSIT) | `cbd4b7e0f9c178a2704767afac5bcef6c7090be7643d061517b9d1afbcd09e73` | failed to verify the signature script: script ran, but verification failed |
| C2 second `set_params` in round 1 (used = 1) | `9b051a01278c310912a4e11726ab69e4f5df351092a242f2d205ee856f4b899d` | failed to verify the signature script |
| C3 second `settle()` of the spent request | `39c558ef2e7ef6b85a591c846ef4d0ef2c44872252e19d556caf61042213a044` | transaction ... is an orphan where orphan is disallowed |
| C4 claim with one leaf count changed by 1 (untrusted copy) | `0822ca9f5aab66371af07a49e91df0a911a88b7589ed285ceb2f5f770ed8aa2d` | failed to verify the signature script |
| C5 b = 1 with a 0.05 KAS referral (below M_REF) | `190ff38d46eeccef04546fe3b410d8cd8f7b86064deca2741244fb7a7c0fb343` | failed to verify the signature script |
| C5 partial withdrawal leaving less than wmin | `5ff5a34fc35db85a73aab8873942562983f9032bc175bb3e0f9096e905c77869` | failed to verify the signature script |
| C5b further buy on a post-deadline shard (sales pause) | `9ba9c10cf1d885eec7ee26241d1ce1248213bf446392fd1a89768264ba3802a5` | failed to verify the signature script |
| C5b `close()` of round 4 before round 3's draw (strict order) | `d9219af71720d8933322c5ade722d04914e1bed5719a6bdbafa19e63bd4d0c74` | failed to verify the signature script |

### Rounds, anchors and luck

| round | m | sales | anchor X (DAA) | winning combination | our winning holdings |
|---|---|---|---|---|---|
| 1 | 3 | 76 buys | `5d7a30f960ff9fb6da36cc8b6303d46273ba38c6fb6b690d960e46119582d121` (598,551,178) | [1, 12, 17, 20, 30] + 2 | 2 (tier 4), claimed from the wallet |
| 2 | 2 | 77 buys | `40828cfa5a82b897f37b0c8efc42a009b44b87673bf0240e88ed2df08ccffcdf` (598,565,344) | [5, 7, 12, 18, 30] + 2 | none |
| 3 | 2 | none (opened past its deadline) | `de5c4619209c8ae254a43548da8fd8f006ef4b72f6267df4781913f02b08a95f` (598,570,834) | [1, 11, 13, 17, 19] + 1 | none (no tickets, no pools) |
| 4 | 2 | 36 buys b = 1 | `e25139543e726b32ae09e2a1c38f091c599f8a6fee29d80d969af8d842ac133b` (598,608,025) | [9, 13, 18, 22, 30] + 2 | none |
| 5 | 2 | 40 buys | `07f5f68949a6fd31fd23b9fb82f41ad1f355708d274ba9c8b7258d9570ebe0ca` (598,614,375) | [4, 9, 18, 22, 26] + 2 | 1 (tier 4), claimed from the wallet |
| 6 | 2 | 40 buys | `d628d10b6461cd5a1a11a552aad78166ad41dee5b10f3e34f2e9b91ec04aac4e` (598,620,563) | [2, 5, 12, 21, 29] + 1 | none |

No jackpot, duplicated jackpot combination or second-prize hit occurred (A5 records them only if they occur; they are forced on the private simnet in S4, D114). The C4 luck retry was not needed.

**Not exercised on testnet-10 in S3b:** the round-4 claim from the indexer store (C5c; no winning holding, A5's retry covers only C4); a last claim (FINAL): every claim was ACCRUE (h = 1, tier 4); buys in round 3 (moved to round 4 by the amendment). The soak (C8) is S4.

### Masses and fees per shape (all 300 accepted instance transactions; `tn10 masses`, mainnet mass parameters; the computed storage mass equals the committed one for every transaction)

| shape | count | inputs | compute | transient | storage | priority mass | fee (KAS) | example txid |
|---|---|---|---|---|---|---|---|---|
| genesis (K = 16, V0 100 KAS, J1 10 KAS) | 1 | 1 | 11,223 | 8,372 | 460,925 | 460,925 | 0.4609 | `e64f08175f833100814f1fd24d1a0d8e05e2dcefbf4828892515e400cdaabd57` |
| buy b = 4 | 229 | 2 | 46,014-46,022 | 158,456-158,488 | 76,973-83,938 | 79,228-83,938 | 0.0792-0.0839 | `f874f270b7f0d3ae98ef50001ad6029476bd34814a65c26200a4255199c52dfc` |
| buy with referral (b = 4 x 2, b = 2 x 1) | 3 | 2 | 44,754-46,434 | 153,976-158,696 | 103,787-253,938 | 103,787-253,938 | 0.1038-0.2539 | `10241a9314f9d22876332eaa8fefe48b9a95b5148da9dc4a9f4f27cb66841503` |
| buy b = 1 (C5) | 1 | 2 | 43,456 | 151,424 | 309,440 | 309,440 | 0.3094 | `0e498c9ac863b3b9a716a8982e8b8410f9e2e322f926f1e079a2717d384ac984` |
| buy b = 1 before the deadline (C5b) | 20 | 2 | 43,456 | 151,424 | 313,857-322,547 | 313,857-322,547 | 0.3139-0.3225 | `367a62a48c3fd93112d3b7d1753bcaf1b08ee59366000b4b1c065c9f3baf45ea` |
| buy b = 1 after the deadline (C5b) | 16 | 2 | 43,456 | 151,424 | 311,501-316,206 | 311,501-316,206 | 0.3115-0.3162 | `cdf14d7694bf487da74478747b1a53734fc5e194e9b4df518c6ee710535621b9` |
| set_params | 1 | 2 | 6,404 | 13,896 | 23,654 | 23,654 | 0.0237 | `ef3f00a8416d077b8cc797409a26ed1b0d2a3cafc76173f9393274d7e1533d36` |
| request_deposit | 2 | 2 | 23,195 | 77,580 | 23,982-24,298 | 38,790 | 0.0388 | `ae05e4a1f7a4c4544593bca93ed72082a843823a6f961e784aaf8136406a4115` |
| request_withdraw | 2 | 3 | 24,729-24,730 | 82,516-82,520 | 23,744 | 41,258-41,260 | 0.0413-0.0413 | `ee68b8f6c73755278fcf35ef96de71aeca488adfc5e7f094ae15761d4f39bc32` |
| close (K = 16) | 6 | 19 | 293,743 | 864,612 | 267,243-423,740 | 432,306 | 0.4323 | `5684a86b5a37acdc21805fa3329450b551552329ffb0107b34dfee9a71b7660a` |
| draw (real anchor, K = 16) | 5 | 5 | 139,535-140,226 | 442,940-443,424 | 41,721-81,731 | 221,470-221,712 | 0.2215-0.2217 | `402d1ce519bee939e8c54c798e0260883dbdc2e9633c3708a9e36d2e329b9204` |
| draw of the no-sales round, 60 min late (C5c) | 1 | 5 | 137,613 | 442,052 | 32,320 | 221,026 | 0.2210 | `596ef4386a228b0601ffce5e2e775828c73ee79162ba973f604908b17ff1fbae` |
| settle (deposit convert, PAY, FINAL) | 10 | 3 | 22,002-23,159 | 73,248-73,596 | 1,302-29,797 | 36,624-36,798 | 0.0366-0.0368 | `3e3bf0d87268595d291922b3244c9381d1fc268fa795b1852b4d2fc3d4de6b38` |
| claim (tier 4, ACCRUE) | 3 | 3 | 48,190-48,323 | 151,160-151,292 | 80,137-82,803 | 80,137-82,803 | 0.0801-0.0828 | `6d535230d8f5dadebf0dbc95421069759e972ca91830d875931dc93daed7257c` |

300 distinct instance transactions; their fees total 35.5608 KAS.

The `close()` masses equal M7's (293,743 / 864,612). The real-anchor draws (139,535-140,226 / 442,940-443,424) sit within a few hundred grams of M7's 139,891 / 443,964; the M8 draws also carry the conversion and the record. The b = 1 buys have a higher storage mass than the b = 4 buys because their funding change outputs are smaller (KIP-9 storage mass), not because of the script.

### Spend, incidents and runner fixes

- **Spend:** balance 6,709.548 -> 6,152.772 tKAS = **556.78 tKAS spent or locked** at the end (vault, pools, reserves and fees); **peak 843.73** (balance 5,865.82 after round 2's buys, before draw 2 and the withdrawal settles paid about 467 KAS back). Fees of the 300 instance transactions: 35.56 tKAS. The hard cap (900, raised to 950) was never crossed; a balance watchdog would have stopped the runner at 920.
- **Chain reorgs:** 8 instance transactions were reverted by chain reorgs on testnet-10 and accepted again (index log `remove` lines at chain DAA 598,576,376 to 598,613,775). The index reverted and re-applied them; the runner's wallet feed did not handle the first revert (runner fix 2).
- **Node hiccups:** "WebSocket disconnected" (C5, deadline wait) and "RPC request timeout" (C5c, draw 3 hold); both resumed with `campaign resume`, nothing in flight.
- **Runner fixes (tn10 only, no covenant change):** (1) every `close()` spends a freshly accepted funding input and refuses a close leaving under d_min / 2 of sales (`close_deadline`, unit test with this run's numbers); (2) after a sync that reverted transactions, the wallets are rebuilt from the index log (reproduced in the dry run with `TN10_DRY_REORG=1`); (3) the C5b post-deadline check counts the accepted actions instead of the shards still predating the deadline (a resume had recorded "0 shards" though all 16 were accepted). Also a read-only `tn10 masses` subcommand for the table above.

### Transactions per step (accepted; node; accepting chain block DAA)

The refused transactions are in the table above. Fan-outs and fresh funding transfers to our own address are not instance transactions and are not listed.

**C1** (1 transactions)

- C1/genesis `e64f08175f833100814f1fd24d1a0d8e05e2dcefbf4828892515e400cdaabd57` (vector-10.kaspa.green, chain DAA 598,543,922)

**C2** (78 transactions)

- C2/buy/0 `10241a9314f9d22876332eaa8fefe48b9a95b5148da9dc4a9f4f27cb66841503` (vector-10.kaspa.green, chain DAA 598,543,966)
- C2/buy/1 `f874f270b7f0d3ae98ef50001ad6029476bd34814a65c26200a4255199c52dfc` (vector-10.kaspa.green, chain DAA 598,543,966)
- C2/buy/2 `6e888ec31400dbdbff46fcf7b5088bf3c0c79287c3c0d443ac121b3823b1b580` (vector-10.kaspa.green, chain DAA 598,543,966)
- C2/buy/3 `721e9753fc55497b4871409ccb6a749e75d424be15a9df7982ff9ea4f6e27bde` (vector-10.kaspa.green, chain DAA 598,543,966)
- C2/buy/4 `c9aa6102fb154c4b80e3162b7fc82bdcc122832b89f81a1a9405a6b5790bd090` (vector-10.kaspa.green, chain DAA 598,543,966)
- C2/buy/5 `f68bfaa04f9714b08af0d595d570ff470f684e25d8abc291a9546ec9fbbf3cac` (vector-10.kaspa.green, chain DAA 598,543,972)
- C2/buy/6 `13a16ff242c0bc5c6135974e383996abda2b0ba392775adc933882c96d3529f9` (vector-10.kaspa.green, chain DAA 598,543,972)
- C2/buy/7 `676c3a3709454b9b89c13ee6c04a6dc09c0f77ae73f31c0078b3c180bc63ffd3` (vector-10.kaspa.green, chain DAA 598,543,972)
- C2/buy/8 `03944439407bbc32b8f6779d77e3f733160c76a6402d44df86f7c4b054a573d7` (vector-10.kaspa.green, chain DAA 598,543,972)
- C2/buy/9 `38d9bf443988b7d306d37c5895d76a762fa9e0db828bef57c43ddc728365c864` (vector-10.kaspa.green, chain DAA 598,543,972)
- C2/buy/10 `3cc909ac097f18138a7b134ab5ed1b5fcd8f0dc7546bccea7d8a3b0e81cd1c32` (vector-10.kaspa.green, chain DAA 598,543,977)
- C2/buy/11 `056ff5c8957d7088fb18022e86502421dcf74475df47eda62dd9f4651454b7e0` (vector-10.kaspa.green, chain DAA 598,543,977)
- C2/buy/12 `00e5ebb6256e166f82428806977caa5fcd28ae1f078b743ab3beb910050bf7d1` (vector-10.kaspa.green, chain DAA 598,543,977)
- C2/buy/13 `a1e4f7c8d5388990494de79a3730b3b68c9479708b0363c7b91df7b6a276530c` (vector-10.kaspa.green, chain DAA 598,543,979)
- C2/buy/14 `7b8b53004ed16d196b59358795e981b9811697e7d670107d8e6a13a2bf02f53e` (vector-10.kaspa.green, chain DAA 598,543,977)
- C2/buy/15 `f433d1f85d20650c3c80de0e3c252b74790d50a5d5d74e1eb14149680100f942` (vector-10.kaspa.green, chain DAA 598,543,977)
- C2/buy/16 `dad281b57e7c9a421c4cc0cce690839eea066075f956e9dcaf88510a119c3b15` (vector-10.kaspa.green, chain DAA 598,544,018)
- C2/buy/17 `b6135abef0e2fa179408706fb839df9e96193d8bc31a2abb823daa78385764f6` (vector-10.kaspa.green, chain DAA 598,544,018)
- C2/buy/18 `d69428fd409e08d8a50edd15a1b0d765f9ddd287c43d503398d8c4d3495e596b` (vector-10.kaspa.green, chain DAA 598,544,018)
- C2/buy/19 `01a33f8f5e2db9111b3c55b2ffb678fbb0844dd96b4315012082e397d8d37205` (vector-10.kaspa.green, chain DAA 598,544,020)
- C2/buy/20 `0453e0880554f0ea3ad439a3456f79b1ae35d0602e906ae533f9e95baf11f14c` (vector-10.kaspa.green, chain DAA 598,544,020)
- C2/buy/21 `01da9f00bc7723b532e092ac675aafee4907b9563a308e80c91380bacd5d9be1` (vector-10.kaspa.green, chain DAA 598,544,020)
- C2/buy/22 `b5fb53103d60f97cd0c49c80281845f407e0377571cbc39325105d2fb2bb51b5` (vector-10.kaspa.green, chain DAA 598,544,020)
- C2/buy/23 `485a84929692206650336dc6913901ebcfc90b22615180a8a9a1e0c00736b2c9` (vector-10.kaspa.green, chain DAA 598,544,020)
- C2/buy/24 `bf73d8946386dbc90f7d96b7123ddd92b67560c4db530ee97d2c2a4c07b8f117` (vector-10.kaspa.green, chain DAA 598,544,020)
- C2/buy/25 `29f2d1ef367a3caac3fb49b1c87a702943d1d90b77c735ee8a714a1d34f4fdfb` (vector-10.kaspa.green, chain DAA 598,544,028)
- C2/buy/26 `832d2ce232d11643760512b4a014c169805cc24309bb10b0329cf8badccd512b` (vector-10.kaspa.green, chain DAA 598,544,028)
- C2/buy/27 `ec45175e3b09174612707b25f3ae3185237293fa73adef74e2bfde2e3722fb3e` (vector-10.kaspa.green, chain DAA 598,544,028)
- C2/buy/28 `e347d1712fb1eaf5c1884a222689e796d4d34cef77446105e6eaadfad7fee40e` (vector-10.kaspa.green, chain DAA 598,544,028)
- C2/buy/29 `17dd091edc1a4a48f416df908f79740c55c50459b3f492e0f2052a5e009ada68` (vector-10.kaspa.green, chain DAA 598,544,028)
- C2/buy/30 `642e187bec3df443542d85b5f8e2d08ae2e0e3ba76ed71f037513662470e959e` (vector-10.kaspa.green, chain DAA 598,544,028)
- C2/buy/31 `122b8ccd60637ab90b4e5aeb0b46cb98742200ee407d4f04914f41a64a6a1315` (vector-10.kaspa.green, chain DAA 598,544,033)
- C2/buy/32 `0e80be7f33af0bfb1f2dfd31a5303a58b73c9f17905a2a96a8a14b06130ba585` (vector-10.kaspa.green, chain DAA 598,544,061)
- C2/buy/33 `98be3168c820bde81dda12ffa5305578f5a2eda9ae7542e634ff6643771055c2` (vector-10.kaspa.green, chain DAA 598,544,061)
- C2/buy/34 `554ba7101458e2693a83027849f3bd4a467eda544216a6926e0675ed1e7874da` (vector-10.kaspa.green, chain DAA 598,544,061)
- C2/buy/35 `e40d35cdddf7de6c3d558b06a5c35dbda22216fa06aaa4b1a07cb41628b62081` (vector-10.kaspa.green, chain DAA 598,544,069)
- C2/buy/36 `037b29b0a00d406a0f3b3e28c9452e72024d6645796cb329a357857f91f335a4` (vector-10.kaspa.green, chain DAA 598,544,069)
- C2/buy/37 `3794a35b159c3eb6c199e5c9f8be9b8c433673d3bf09ad50c05803209bc282c5` (vector-10.kaspa.green, chain DAA 598,544,069)
- C2/buy/38 `a4c47546adf84dc444bc829e8fe9947233ed7a1afd21bafffc0d2aad8c54fe5f` (vector-10.kaspa.green, chain DAA 598,544,069)
- C2/buy/39 `1c7a7cd0417feddacc1a6c19b0dd3b912cd5234b8e496783fb1189e06f9a11db` (vector-10.kaspa.green, chain DAA 598,544,069)
- C2/buy/40 `37aa0625bd0b829242b8659e8844b75b09799201c94864e9260857d57c4c1604` (vector-10.kaspa.green, chain DAA 598,544,069)
- C2/buy/41 `1c778ec475d00cea0f19a755bb51a0dc810ac4018337c5d784fb477d8ed510ad` (vector-10.kaspa.green, chain DAA 598,544,072)
- C2/buy/42 `d5a39cbc45bfa5a673cf97f1e15a1b6dae4a51b837e8f93d610772b004d6141d` (vector-10.kaspa.green, chain DAA 598,544,072)
- C2/buy/43 `41299d8d5c6551cbf12e1aed7e9bda9ddc07e8f373e7dbb5656447aec988ad39` (vector-10.kaspa.green, chain DAA 598,544,072)
- C2/buy/44 `1f91ce8a1b9bafa6cabb4c4c1450550f53c5c17b662e11dfa8cd970cce1f0ccc` (vector-10.kaspa.green, chain DAA 598,544,073)
- C2/buy/45 `c92fb48b4d1da05cc86c0e536b82bac1fba057136faf01ecdb8f39c4a126bf0e` (vector-10.kaspa.green, chain DAA 598,544,073)
- C2/buy/46 `dbfab53ab8179b9aeb7dd276ed8f6a5e13438f5ca2ac0968cfe6592038707693` (vector-10.kaspa.green, chain DAA 598,544,073)
- C2/buy/47 `df4864868f44cce7e2a24e234518f375afcf1e9d2e202b6b331b0a73228c01bd` (vector-10.kaspa.green, chain DAA 598,544,073)
- C2/buy/48 `5c3f6612584ab3f0f1c5abf465e18de44860a753c6e5ae36b6e6aea643745f57` (vector-10.kaspa.green, chain DAA 598,544,119)
- C2/buy/49 `d6e9afaedc9d562eed793e8effc71a7f3368f9726fdaa5272e50e3f94bd78e2d` (vector-10.kaspa.green, chain DAA 598,544,121)
- C2/buy/50 `9fb481ecf421e2a0231bb479d8281accb3ff938311ae1c377b9da675b0f2a02b` (vector-10.kaspa.green, chain DAA 598,544,119)
- C2/buy/51 `e623a12125d58eb2cba5d71ae60da15a6ef2fc47569f781f73840a0d125019e9` (vector-10.kaspa.green, chain DAA 598,544,119)
- C2/buy/52 `e5de70848975beecdb35d2b35c7340cac128c88cb65783d029f7fdc33d545e70` (vector-10.kaspa.green, chain DAA 598,544,119)
- C2/buy/53 `eecb222a183865a196888369f954cd7c1f08ea0b7bad5a830928b913acf7497f` (vector-10.kaspa.green, chain DAA 598,544,124)
- C2/buy/54 `5097ebbd71be38fb63c8301e0d8e0369d8a595617df7da3e041ff388ecde3bbf` (vector-10.kaspa.green, chain DAA 598,544,124)
- C2/buy/55 `bb631f74e67ddf30db6c7d36276694b12d4dc6514686b59ce720e41543b2c900` (vector-10.kaspa.green, chain DAA 598,544,124)
- C2/buy/56 `5e308941cf6965299a42f790a5df998a5028bb369c6fa6bbacadfa9559c62d9f` (vector-10.kaspa.green, chain DAA 598,544,124)
- C2/buy/57 `40f18cf2031dfcb04a1225c4ea672ac071f432ee2148bf063e356e668ea77d4d` (vector-10.kaspa.green, chain DAA 598,544,124)
- C2/buy/58 `1025aa2043ef6c86b0e6c85cd21b85ceae3372ffecbe9ceb745c0369fa1d0453` (vector-10.kaspa.green, chain DAA 598,544,124)
- C2/buy/59 `6d2a5cb9c6d4a3c6db05f22dcf68fb93418219746ea1d887fc2b6e3b1c6d4c8c` (vector-10.kaspa.green, chain DAA 598,544,128)
- C2/buy/60 `4dc43957221cdd72d3a0c8125af61ed483a1d286dba69d259f3a2efeadfc4731` (vector-10.kaspa.green, chain DAA 598,544,128)
- C2/buy/61 `47dd2424c41360977446468cb3f9d45c20f3538cd88c9710c2fb0cc45e10c4d2` (vector-10.kaspa.green, chain DAA 598,544,128)
- C2/buy/62 `f26790e68d8cf5816a1e10db3504729b9940db23b3fd32991b5eae365cb8c9be` (vector-10.kaspa.green, chain DAA 598,544,128)
- C2/buy/63 `309c7880c028615097cec56bf10332b40cc653c5bf3d5f9262b7fa2209f0559c` (vector-10.kaspa.green, chain DAA 598,544,128)
- C2/buy/64 `523d51d519a23c0ca59405ff3a99f10fae0c573fc089ce0704bf0a8730a5aebd` (vector-10.kaspa.green, chain DAA 598,544,145)
- C2/buy/65 `5da90552e7ea335ef7aa6898a13b50458f82bd8dd407fea370c8fd53a5b7b1f3` (vector-10.kaspa.green, chain DAA 598,544,145)
- C2/buy/66 `bc027824ada4a8d4cfab1b5b844347f08f0311332c566ccc83ecf6e6f6a55bec` (vector-10.kaspa.green, chain DAA 598,544,145)
- C2/buy/67 `7623fdc540b4f42f0f32b5ea5cea30c469a341ea2b1db81df47926975770b946` (vector-10.kaspa.green, chain DAA 598,544,145)
- C2/buy/68 `4cf6950f95b7da25940a5ab92f7c0c12232b992013546cf69abeca81780d1d8c` (vector-10.kaspa.green, chain DAA 598,544,153)
- C2/buy/69 `147a43c4f7ecb4d95f75aa7f0c41146ac045dabbed229c5d112689e6123217f2` (vector-10.kaspa.green, chain DAA 598,544,153)
- C2/buy/70 `96bdd310d34b389a6f32400a987dbc2b7a94e466eb611dfb19e0d260fed37017` (vector-10.kaspa.green, chain DAA 598,544,153)
- C2/buy/71 `0fd247a4aa1d7d98e4130b73dcb2e528d8516e56482196b8a092001e8a6c2835` (vector-10.kaspa.green, chain DAA 598,544,153)
- C2/buy/72 `c0c6a88cfa4dcc46e9fd530f0eccdf2708eccd55bc9602787af3ee43308b6865` (vector-10.kaspa.green, chain DAA 598,544,153)
- C2/buy/73 `386610789491c30e6a4b84f4ef399a0f9b3da8d26f1b5c9b41336e98c73589db` (vector-10.kaspa.green, chain DAA 598,544,153)
- C2/buy/74 `2eea683e48747bfb472826767b08004de87516980ce23b77e1a37cb9fc91a587` (vector-10.kaspa.green, chain DAA 598,544,157)
- C2/deposit 20 KAS `ae05e4a1f7a4c4544593bca93ed72082a843823a6f961e784aaf8136406a4115` (vector-10.kaspa.green, chain DAA 598,544,559)
- C2/set_params s_R 500 `ef3f00a8416d077b8cc797409a26ed1b0d2a3cafc76173f9393274d7e1533d36` (vector-10.kaspa.green, chain DAA 598,544,586)
- C2/buy after set_params `2a12c4509e699fe02730615030ea8ac6ef3b3034745f49bdd475c4c2f1a85417` (vector-10.kaspa.green, chain DAA 598,544,612)

**C3** (3 transactions)

- C3/close r1 `5684a86b5a37acdc21805fa3329450b551552329ffb0107b34dfee9a71b7660a` (vector-10.kaspa.green, chain DAA 598,550,578)
- C3/draw r1 `402d1ce519bee939e8c54c798e0260883dbdc2e9633c3708a9e36d2e329b9204` (vector-10.kaspa.green, chain DAA 598,551,321)
- C3/deposit/settle/ae05e4a1f7a4c4544593bca93ed72082a843823a6f961e784aaf8136406a4115:1 `3e3bf0d87268595d291922b3244c9381d1fc268fa795b1852b4d2fc3d4de6b38` (vector-10.kaspa.green, chain DAA 598,551,351)

**C4** (2 transactions)

- C4/r1/claim/d69428fd409e08d8a50edd15a1b0d765f9ddd287c43d503398d8c4d3495e596b:1/0 `6d535230d8f5dadebf0dbc95421069759e972ca91830d875931dc93daed7257c` (vector-10.kaspa.green, chain DAA 598,551,458)
- C4/r1/claim/bc027824ada4a8d4cfab1b5b844347f08f0311332c566ccc83ecf6e6f6a55bec:1/0 `c80a3b4ccf8c1fcdc80a1592895b21d8d5c662c7ce00eb3df627a9c7c343193e` (vector-10.kaspa.green, chain DAA 598,551,705)

**C5** (80 transactions)

- C5/buy/0 `3bcafada48421343fc8f2290889462007e070a46e57e226c1eef6c2782ba754e` (vector-10.kaspa.green, chain DAA 598,551,806)
- C5/buy/1 `eb20d715333f28e590c8083b9ab17d5a204447d231824be9bd98e06bc878f120` (vector-10.kaspa.green, chain DAA 598,551,806)
- C5/buy/2 `f197d92b9c48aaf1af398b7958ff13c03ba3e092f3b57a5b83a6a3959fe302f2` (vector-10.kaspa.green, chain DAA 598,551,806)
- C5/buy/3 `2f6b8616b57db653e2759daba30bf7d28d80c6dedf763052031cf071ef2ba990` (vector-10.kaspa.green, chain DAA 598,551,810)
- C5/buy/4 `7f46f5edd674c5f87cfc0ec6abcd70ed7f651dc4cbd5a5cb88a4081ff90ce156` (vector-10.kaspa.green, chain DAA 598,551,810)
- C5/buy/5 `2416ca93943e93132f89f15ca7720ad61e5f6bdd6afdfc71f2136edbae12a2f1` (vector-10.kaspa.green, chain DAA 598,551,810)
- C5/buy/6 `ced72e7073f1fa178bcdd1e72ca0306c4251019dc8793a48a5bb5b7940bc19e9` (vector-10.kaspa.green, chain DAA 598,551,810)
- C5/buy/7 `227bd9b4644e7924a95d37f387103e49d316f4396e01d87209fdb4692bff3731` (vector-10.kaspa.green, chain DAA 598,551,810)
- C5/buy/8 `dc1382349cc1d2e2421c6ba002256ec70df4e32e14716ca024552ce60cdd9819` (vector-10.kaspa.green, chain DAA 598,551,810)
- C5/buy/9 `6d78f9cf5a8acb4da313d66554fab80729cbfcc7807d003f70b0e1c0a105ff3e` (vector-10.kaspa.green, chain DAA 598,551,811)
- C5/buy/10 `b0e182b7ce51c77ce355acf64f218b34c57a94a90bcb3a68b8a78f1951710a16` (vector-10.kaspa.green, chain DAA 598,551,811)
- C5/buy/11 `edc753421e6102b8e1a99ee8162a4d4a784115069fea45dd2e2c5bdbcce2a93a` (vector-10.kaspa.green, chain DAA 598,551,811)
- C5/buy/12 `7e964d09e7568f173a9f329de0d9bce3c209198ede0401cc2bda789d45759940` (vector-10.kaspa.green, chain DAA 598,551,811)
- C5/buy/13 `33f2489192a88041d1da3d3202736566e8cec97b9f1fd1e79476347fe0f3174a` (vector-10.kaspa.green, chain DAA 598,551,811)
- C5/buy/14 `962227b28a1129b246f0e5e05b2522b703e6ab66fd3dcc82a03a0afb6cd4a6e7` (vector-10.kaspa.green, chain DAA 598,551,811)
- C5/buy/15 `3c9669f07edf18c9057d1b1a22b81b5f3839675583fe177fe8d21615c9eeb61d` (vector-10.kaspa.green, chain DAA 598,551,817)
- C5/buy/16 `9294fd58ec3eae816772ef1efcb1e6143d8fc52d4e4847ad891e62d89c406188` (vector-10.kaspa.green, chain DAA 598,551,853)
- C5/buy/17 `1d87bc0a6101a8e550c2f98458d6aeedf0ef6e4b235637b5417b362a0c822004` (vector-10.kaspa.green, chain DAA 598,551,853)
- C5/buy/18 `77469306f6a01fd7ec0515e16bf5d44b86fe90f3842c423db802a48bc83b2201` (vector-10.kaspa.green, chain DAA 598,551,853)
- C5/buy/19 `92534a01b16bb47dd2e3a13d4f1c33dfdb73577a397b4ba59e7f11b8ffd28e6c` (vector-10.kaspa.green, chain DAA 598,551,853)
- C5/buy/20 `f4e3f815a8c89d36681636cab7948502e93034aa604cbe5ac990949cb82a0f92` (vector-10.kaspa.green, chain DAA 598,551,853)
- C5/buy/21 `967a38200d68703b66596749d750164fc47ab4ee79dd8a90da2c281607c364fc` (vector-10.kaspa.green, chain DAA 598,551,853)
- C5/buy/22 `23a8de41f5e4a1afa73ecbe37921d1222159cf8214b37b0825da5a031993967e` (vector-10.kaspa.green, chain DAA 598,551,860)
- C5/buy/23 `bd82a8904e71df658a3e3478c6230488eb9d9932f3216e7ac313e2e184e8e944` (vector-10.kaspa.green, chain DAA 598,551,860)
- C5/buy/24 `d98d5606d969ee73ca96f6c0ce3c05dca2c1d84c00ca12fff441c6f4f76f0bb1` (vector-10.kaspa.green, chain DAA 598,551,860)
- C5/buy/25 `c3b7cb7702faab19777e4d5c3f07319d5b9f4de8a0e2a8dc9a702b9d97e8543a` (vector-10.kaspa.green, chain DAA 598,551,860)
- C5/buy/26 `dd9a819b23bc80a3fef13cc4ca51247ba40bae23959ec56675dd26a9ac1cb867` (vector-10.kaspa.green, chain DAA 598,551,860)
- C5/buy/27 `01822d3cdf68f215cc936e8cc8f343644501e392b6cb364750be47e05a8fe789` (vector-10.kaspa.green, chain DAA 598,551,860)
- C5/buy/28 `76f751e08d1d0ed2c0f6ad1c06a570dec16fd69242c52f5a06cb86dfbca4640c` (vector-10.kaspa.green, chain DAA 598,551,867)
- C5/buy/29 `3c813cd5410eff3011a38b5baaae9b0fb620bf57f8bfe0ef72f5c5aa5f6ccede` (vector-10.kaspa.green, chain DAA 598,551,867)
- C5/buy/30 `f7822d8d1b6bdcc0c433021ee4522887f63f7808636c44b5de27796bcfeae072` (vector-10.kaspa.green, chain DAA 598,551,867)
- C5/buy/31 `c9c7dcdfd0589abe9cf44ed9c6ae3defda5971bcdb88e542c88def58f1524525` (vector-10.kaspa.green, chain DAA 598,551,867)
- C5/buy/32 `399e93674650664e077db93388f0878242e52e9cf8e52683f2301024797db8a4` (vector-10.kaspa.green, chain DAA 598,551,879)
- C5/buy/33 `c2d3be9881cd31eea9e92793b50ef3d75a7fd1f7926a19bfb0f9030d16732a1b` (vector-10.kaspa.green, chain DAA 598,551,886)
- C5/buy/34 `169dd1f2674b024f23232c384dc859bcae2ef8c38e7398cdeef2639bc66c89a2` (vector-10.kaspa.green, chain DAA 598,551,886)
- C5/buy/35 `4a1a46e1533d7c511bae8661c31ce489b17ea8d2bd1c2672af751a5f2fad9c63` (vector-10.kaspa.green, chain DAA 598,551,886)
- C5/buy/36 `7cb98c62a37c9a3300c8f017e9b995ac04ca02f5be2e80cdfb8fc2487c4e5ea3` (vector-10.kaspa.green, chain DAA 598,551,886)
- C5/buy/37 `7540f7dc0de9cccc26bb62c9b8a97a23efe9b8ead137962cfc80cee86a238388` (vector-10.kaspa.green, chain DAA 598,551,886)
- C5/buy/38 `0e4534c3c63b999fd6025718566235b5edd704af78a34a9ae33578cbd5bef431` (vector-10.kaspa.green, chain DAA 598,551,891)
- C5/buy/39 `5c54107db1396daafcced108449fc595dbcef19ec9c60369a622cc856e2b9a96` (vector-10.kaspa.green, chain DAA 598,551,891)
- C5/buy/40 `b9e79feca6bfe9a8cc63c06fb8aab266f553d62cc6bbdfdf4830c45ffde2ee2b` (vector-10.kaspa.green, chain DAA 598,551,891)
- C5/buy/41 `ff1f63d24748a950528ef715b1cc45ae7589e463f3759f3161931ebbfdb7ce15` (vector-10.kaspa.green, chain DAA 598,551,891)
- C5/buy/42 `021994e27cc569b1f4c1d16eaf6284513ba5a0b5291bdd806a688a1ef91de433` (vector-10.kaspa.green, chain DAA 598,551,891)
- C5/buy/43 `989bcfdd0ec0f31ff6032d6a88768c9b489383303fbbb9a8939f0abbadd6de5a` (vector-10.kaspa.green, chain DAA 598,551,892)
- C5/buy/44 `eca7dddd2f73bc238c23bc8c97b7505335ef7a65c1aca6df2e237883fcdbb099` (vector-10.kaspa.green, chain DAA 598,551,892)
- C5/buy/45 `0f39e9ce1ed95fd6aa66a5e97d8d8f78cb0e308c5fd03e2e948dd9593f5c62ec` (vector-10.kaspa.green, chain DAA 598,551,892)
- C5/buy/46 `5335fa1bfc93fcd3eb8eb93ecd7d3f00923bfc1f8ec28a8cc9d98f429b841622` (vector-10.kaspa.green, chain DAA 598,551,892)
- C5/buy/47 `8a2075f4ec78de1346acfed7141e85d315bb578310a7671d6591aad8483444ba` (vector-10.kaspa.green, chain DAA 598,551,892)
- C5/buy/48 `1fd414c011f383e5e200e6de29620febb99b23376b31d4d93d16d7fc4674f37f` (vector-10.kaspa.green, chain DAA 598,551,919)
- C5/buy/49 `a1b5eab320e1ffb6b3a3fb693a68aeb5fa164875dfd1522db48bb7acbb1c105f` (vector-10.kaspa.green, chain DAA 598,551,919)
- C5/buy/50 `9c730b36d0a50ca3f84919c73fd3aac68ce47ef778ce1d5524991f29e0363acb` (vector-10.kaspa.green, chain DAA 598,551,929)
- C5/buy/51 `eafed34f8ad499560970d8ea425226296ef3bba1f76a1ab77090d0f839d6b21c` (vector-10.kaspa.green, chain DAA 598,551,929)
- C5/buy/52 `06e5ffbd2488b4128fa8653807f8bd94795765e0e43a0ba7d2c60f2dd720d1e7` (vector-10.kaspa.green, chain DAA 598,551,929)
- C5/buy/53 `170b61a98f9ba1cdf2f35a8da6895e1b45baa0e45b1c1288f4dc93d29c18d7a5` (vector-10.kaspa.green, chain DAA 598,551,929)
- C5/buy/54 `e8c70a6ccde8c134ddeac0d852abf3fe7ada93f6c0dc0a3a7efc6583e083267c` (vector-10.kaspa.green, chain DAA 598,551,929)
- C5/buy/55 `eb268985056b448b3413e25d6e3e92056eb9454f4df1fc5f64fa37d58c2bbe3e` (vector-10.kaspa.green, chain DAA 598,551,929)
- C5/buy/56 `15b2fd14a4e922f00da7b458e2c4a383b6cf37bf596c239a573c9e958dea0969` (vector-10.kaspa.green, chain DAA 598,551,933)
- C5/buy/57 `b6ca178a857b2066ee0d6579778cebb422b8d352752967c3a7ebf60b57172c29` (vector-10.kaspa.green, chain DAA 598,551,933)
- C5/buy/58 `1111340a5ade272b35b9da285ac30a25b640dab7b987e8d8f20a75af929b81c9` (vector-10.kaspa.green, chain DAA 598,551,933)
- C5/buy/59 `b8b12b2318db1936f91aa3712e8cc427fbf5cb4a7f646684cafabc25bf8b2f87` (vector-10.kaspa.green, chain DAA 598,551,933)
- C5/buy/60 `1e07e0d66864bd67c0f6f6c6634bbc0bf327a1735eb12ca5daaa220f2b89d0c8` (vector-10.kaspa.green, chain DAA 598,551,933)
- C5/buy/61 `1570dee3fcc1deaab4fb339ae4e4998e3350c0af071ae98bb0141949c067e5fe` (vector-10.kaspa.green, chain DAA 598,551,940)
- C5/buy/62 `36b9b0363e1507421908ffa3950183227cb10afefa5de65201dee0baccb0d674` (vector-10.kaspa.green, chain DAA 598,551,940)
- C5/buy/63 `1802cdddc923f748786cce834e694396b0e1419662fe684329b1dcb215349b2f` (vector-10.kaspa.green, chain DAA 598,551,940)
- C5/buy/64 `1b58905e223aa3439d41b46513bc34fd2950080ee5f69c55aad129fa8bda6e30` (vector-10.kaspa.green, chain DAA 598,551,978)
- C5/buy/65 `347358120d2a63552221d6c926a03804ac0d0de1a059e1457e8e72f53d8d2c83` (vector-10.kaspa.green, chain DAA 598,551,986)
- C5/buy/66 `62826fcb5eded691e01f7be9341fed3986a546ef421c501130d537d99d8d01d8` (vector-10.kaspa.green, chain DAA 598,551,986)
- C5/buy/67 `fabe299e7987e775f32e6c6e10099382f3f920c7513d1c8efd350817d5968444` (vector-10.kaspa.green, chain DAA 598,551,986)
- C5/buy/68 `5483ee22d5fafb7d1f1edc94d44ecdce0ad0e0b8d468ab42f3f2ff0b17c8901f` (vector-10.kaspa.green, chain DAA 598,551,986)
- C5/buy/69 `feaeacd35bd319aa38444f7aa7096814fc435988d4b0849a0c86d3fe08b1013b` (vector-10.kaspa.green, chain DAA 598,551,986)
- C5/buy/70 `21efcea3afbdc02daec9b19078de43099e0a6a2aa404059645c1922e5a771adc` (vector-10.kaspa.green, chain DAA 598,551,994)
- C5/buy/71 `388c3e5a680a05dfa5319e2cae14ecfbaf59dca25a977cdacf6fd9714cf89333` (vector-10.kaspa.green, chain DAA 598,551,994)
- C5/buy/72 `7a60eebb8836483ca38ba28b1f1dc138254cf01d2d818629610bef1b48f0f70d` (vector-10.kaspa.green, chain DAA 598,551,994)
- C5/buy/73 `405606b32fa998a72b5afd8981923c04e8384e165aa0e94b89e29bda7c8cc1e9` (vector-10.kaspa.green, chain DAA 598,551,994)
- C5/buy/74 `76f1d482a499599076c091f81a1b7b6151b2866bb3a66f5a7c5764e9a6c54aca` (vector-10.kaspa.green, chain DAA 598,551,994)
- C5/b=1 without referral `0e498c9ac863b3b9a716a8982e8b8410f9e2e322f926f1e079a2717d384ac984` (vector-10.kaspa.green, chain DAA 598,552,043)
- C5/b=2 with 0.1 KAS referral `dcc813d9962c5c309a79cf8059e4c3316c9a15e5f11c672999700677728d6339` (vector-10.kaspa.green, chain DAA 598,552,043)
- C5/depositor whole withdrawal `ee68b8f6c73755278fcf35ef96de71aeca488adfc5e7f094ae15761d4f39bc32` (vector-10.kaspa.green, chain DAA 598,552,080)
- C5/genesis backer full withdrawal `6104ca564e7cf4bd04e3dbd196e30c3bcb90dc6817af7f48bff8388d41292d7b` (vector-10.kaspa.green, chain DAA 598,552,725)
- C5/close r2 `11c54df612e05dc831848dedc086af6db30a2efc9d15bd960ca55fa80e78a4a0` (vector-10.kaspa.green, chain DAA 598,564,744)

**C5b** (40 transactions)

- C5b/draw r2 (late) `98635a7d20299bf82dc8c72a2155818cdd45bf433b99308ebedf77798514c15b` (quark-10.kaspa.red, chain DAA 598,570,087)
- C5b/withdrawals/settle/ee68b8f6c73755278fcf35ef96de71aeca488adfc5e7f094ae15761d4f39bc32:1 `b583d8cd9149c36570546777f20ab984c52deab0ab5fa28b1745718003a12d5a` (quark-10.kaspa.red, chain DAA 598,570,107)
- C5b/withdrawals/settle/6104ca564e7cf4bd04e3dbd196e30c3bcb90dc6817af7f48bff8388d41292d7b:1 `4587d7da9863a7312030467abab709ea1c4af5a06e935df68016ee06a3619190` (quark-10.kaspa.red, chain DAA 598,570,126)
- C5b/close r3 (no sales) `e704c4fabb937e25eeb7e3935f80033d8c3944f8bb240e293c0969f31a3dd46e` (quark-10.kaspa.red, chain DAA 598,570,234)
- C5b/buy/0 `ce9f94c156a9d332db336d5a6db0c7308caac225ffa45b75e6e0f78e2fd58e9e` (quark-10.kaspa.red, chain DAA 598,570,261)
- C5b/buy/1 `3cc50dcbcb00d42843798cf4db824c367aa233e53a52a240ed37c6e859885c32` (quark-10.kaspa.red, chain DAA 598,570,264)
- C5b/buy/2 `616176c43c50fc9e4a33f82bcdfe148e3d4e15dc56e26f34c6786f5bf94e710a` (quark-10.kaspa.red, chain DAA 598,570,259)
- C5b/buy/3 `367a62a48c3fd93112d3b7d1753bcaf1b08ee59366000b4b1c065c9f3baf45ea` (quark-10.kaspa.red, chain DAA 598,570,257)
- C5b/buy/4 `7dd55fa5c46c221e993fbfaa225b218c6ee1bd63692a17dde1e1a1ae08efc0d0` (quark-10.kaspa.red, chain DAA 598,570,263)
- C5b/buy/5 `87111bb7ca8c678f70d818a3970974a6460e71782e06060a0596c60fe2bbc968` (quark-10.kaspa.red, chain DAA 598,570,275)
- C5b/buy/6 `fd8ad737c1787f75ba7bfb1881fd8153e1e5129077e2940599a113f753ed0cd7` (quark-10.kaspa.red, chain DAA 598,570,265)
- C5b/buy/7 `b76ae9ff7d87f78ea4328e4973fac8044d3acab77663ac29ebacd30161d449cb` (quark-10.kaspa.red, chain DAA 598,570,271)
- C5b/buy/8 `3aa52f1964b2673fa1bc6dcd0d02a76618eaff01d505117883df396f862f3065` (quark-10.kaspa.red, chain DAA 598,570,266)
- C5b/buy/9 `d2b439faa8984ace6d88b1db71b1a3258efdf3571fc33d5a769962a14b7013f4` (quark-10.kaspa.red, chain DAA 598,570,268)
- C5b/buy/10 `98560574657c7bc589ad255182dff77e906cd484058fee3bd730ec3856669e45` (quark-10.kaspa.red, chain DAA 598,570,272)
- C5b/buy/11 `8b8b424374f79dcb4c8f0b227bbf1abfa8da8823da1943567bdac02507059f46` (quark-10.kaspa.red, chain DAA 598,570,277)
- C5b/buy/12 `449457109f85564a710c23dcac3e9f387017a8d536459f91fa1c094c606b82ca` (quark-10.kaspa.red, chain DAA 598,570,275)
- C5b/buy/13 `f3ae41b90fb9814d2aac0c7890ab53db0ed975fe2afc48f7690f15b3bc575c25` (quark-10.kaspa.red, chain DAA 598,570,277)
- C5b/buy/14 `d4b807a04c21a6ee32e9cae496053a901b2894e7c706d18e14790d5e58115ed9` (quark-10.kaspa.red, chain DAA 598,570,281)
- C5b/buy/15 `3453b300d1357a756dfc2990f3c860b679d1740ecb6dbee82f15f9411540d0ba` (quark-10.kaspa.red, chain DAA 598,570,280)
- C5b/buy/16 `8c0bb65e89279eb73081c00afe9eb302b16f0d4cb6b3870f0df7129e567cb6e3` (quark-10.kaspa.red, chain DAA 598,570,304)
- C5b/buy/17 `506aa8322dc944f074c5d4a9a5b6700f8f6b94896f702c54e5e45fadf65b9191` (quark-10.kaspa.red, chain DAA 598,570,302)
- C5b/buy/18 `781dafae7c6ca2b25a80bc6a6c75a265ac670010d98c4c354aba43ecee1e405d` (quark-10.kaspa.red, chain DAA 598,570,302)
- C5b/buy/19 `082feb8ef3b4224ff68a9bb351a543e610e9f11da7d8127607126f861f06cf6d` (quark-10.kaspa.red, chain DAA 598,570,309)
- C5b/post-deadline buy shard 0 `a0bc0bb59ce7ee45c18b1b107f381c2c2e78a59ed26d19970bd9d95bca6c8941` (quark-10.kaspa.red, chain DAA 598,576,368)
- C5b/post-deadline buy shard 1 `cdf14d7694bf487da74478747b1a53734fc5e194e9b4df518c6ee710535621b9` (quark-10.kaspa.red, chain DAA 598,576,364)
- C5b/post-deadline buy shard 2 `1f7fbe2d973483fa55919b9075788653f9cef108388fb3b3dcb482629c807891` (quark-10.kaspa.red, chain DAA 598,576,365)
- C5b/post-deadline buy shard 3 `01eee28c6ce2556fe9818a6373a14042d88960fa8e9d8268e4554e5f2ed31279` (quark-10.kaspa.red, chain DAA 598,576,370)
- C5b/post-deadline buy shard 4 `33e39a52a25c0914b3a0d212f45c972de0854dc606c934bf9ba9821b95acbd8d` (quark-10.kaspa.red, chain DAA 598,576,373)
- C5b/post-deadline buy shard 5 `8af7b382676e27b336b6a342cfe355ed8953e3405cdf988b311aaee4a2d61ee6` (quark-10.kaspa.red, chain DAA 598,576,376)
- C5b/post-deadline buy shard 6 `bb04bb0511cfc06f596123ea155b3f86abbf85222c8b42f2db7f64bf2f73a912` (quark-10.kaspa.red, chain DAA 598,576,379)
- C5b/post-deadline buy shard 7 `9c18637d4e7569d00165b01b92faf2d696b3564e4d951922fbe91c5e3eb9ecd7` (quark-10.kaspa.red, chain DAA 598,576,372)
- C5b/post-deadline buy shard 8 `02c31507764fb685e234379d6c579c93d5fd96166dbdce7186b5816f3abfcf97` (quark-10.kaspa.red, chain DAA 598,576,386)
- C5b/post-deadline buy shard 9 `b6da9a7327c6270496f45d96b7970104466720e56d75f4b387137bdca13e60c5` (quark-10.kaspa.red, chain DAA 598,576,381)
- C5b/post-deadline buy shard 10 `18bee1c23ea9d5ea64a5cd799531bdf79f21a9c989d39ac2be2d56cba1cee983` (quark-10.kaspa.red, chain DAA 598,576,379)
- C5b/post-deadline buy shard 11 `d2e468b758ae7a796cf36b9b742de18606ae45213fa898df646ccc7bbf557367` (quark-10.kaspa.red, chain DAA 598,576,388)
- C5b/post-deadline buy shard 12 `3f159095bdb0b1c4649bfad7b8485a36011debeb24b70d6c06939260a69e00a7` (quark-10.kaspa.red, chain DAA 598,576,384)
- C5b/post-deadline buy shard 13 `24ad902f4f6c82d832ad1c90f12821aa5ea7a3e4bfee93aa58ef895744218beb` (quark-10.kaspa.red, chain DAA 598,576,384)
- C5b/post-deadline buy shard 14 `04a6ee380919bbbaa2bb311acb399232ea8fe7aff7ae3f33e3c6cddb3cb6ca43` (quark-10.kaspa.red, chain DAA 598,576,380)
- C5b/post-deadline buy shard 15 `efcf2b8066e5a894935c2211f6e2a6c945c43a29e9cfe98f5848a06ccacd1ef7` (quark-10.kaspa.red, chain DAA 598,576,390)

**C5c** (5 transactions)

- C5c/draw r3 (60 min late) `596ef4386a228b0601ffce5e2e775828c73ee79162ba973f604908b17ff1fbae` (proton-10.kaspa.stream, chain DAA 598,607,314)
- C5c/carried r3/settle/b583d8cd9149c36570546777f20ab984c52deab0ab5fa28b1745718003a12d5a:1 `717ff462a5fe8e13b34fa2d83a9d09643f5c55f08cf8a4fff878ac64ebd5db69` (proton-10.kaspa.stream, chain DAA 598,607,330)
- C5c/carried r3/settle/4587d7da9863a7312030467abab709ea1c4af5a06e935df68016ee06a3619190:0 `5c6a29814e455e2bb7fbbb3c39b4999dcbe0f02e72cf8754397e5b49b0d2c1f1` (proton-10.kaspa.stream, chain DAA 598,607,345)
- C5c/close r4 `0c32113e6d3764ed999574453d90642ea49f451e5f785fd19b6a786f2e699733` (proton-10.kaspa.stream, chain DAA 598,607,424)
- C5c/draw r4 (indexer store) `c8c4322239af15165ec94fb8803dc93f44548bdbcd4059a3ae5e8dd006f46977` (proton-10.kaspa.stream, chain DAA 598,608,150)

**C6** (91 transactions)

- C6/carried r4/settle/717ff462a5fe8e13b34fa2d83a9d09643f5c55f08cf8a4fff878ac64ebd5db69:1 `9740e64736ff5ff4da79bceab481e832e6cc4f7b877fbe226633446cfba73757` (proton-10.kaspa.stream, chain DAA 598,608,284)
- C6/carried r4/settle/5c6a29814e455e2bb7fbbb3c39b4999dcbe0f02e72cf8754397e5b49b0d2c1f1:0 `8cdf454e76a063f44b6df2f775049bc04d63226f93108e926d16ec9df8d91e33` (proton-10.kaspa.stream, chain DAA 598,608,296)
- C6/r5/buy/0 `7d608d09adfa6b8d46eb9ba7cf6cf32c97510f10523d052cdb54c16d736873eb` (proton-10.kaspa.stream, chain DAA 598,608,321)
- C6/r5/buy/1 `5b4ab1dc424eec599eb35d4522fd98e059269d901ca0912b2c5e233216ae503e` (proton-10.kaspa.stream, chain DAA 598,608,321)
- C6/r5/buy/2 `e3a050bee936f80c484d733aca83acaaaa119899027da32f36965c028e4e5561` (proton-10.kaspa.stream, chain DAA 598,608,321)
- C6/r5/buy/3 `dd90c717a5201d782f78fede856d92153051dbe7aa54673bedcce8a3854b710e` (proton-10.kaspa.stream, chain DAA 598,608,324)
- C6/r5/buy/4 `f97aeebd0747467a9cb19156fb188e6f618a02d59fddad8f852d6202b66d88ee` (proton-10.kaspa.stream, chain DAA 598,608,324)
- C6/r5/buy/5 `d9f02bf4541e00d47466b4e61696b4f82eaf64aa806dba4d19522b10cf82d440` (proton-10.kaspa.stream, chain DAA 598,608,327)
- C6/r5/buy/6 `32d35ca1129b49f3057290bf7ac213f4067ec3dcb1174488d44b057d4cc5d938` (proton-10.kaspa.stream, chain DAA 598,608,324)
- C6/r5/buy/7 `fcc7e8ab266722c218f79f0f0031a257c6afd980b6b4b9ee67830f6bb348a54a` (proton-10.kaspa.stream, chain DAA 598,608,327)
- C6/r5/buy/8 `41297b679835969a0509268ce536f4f49aaa634f225145ddb589774baee37b57` (proton-10.kaspa.stream, chain DAA 598,608,328)
- C6/r5/buy/9 `6dbab9fdb85cad3a6b0924adeeac9578932c889fd83c9cfc26a80ebbeb88c165` (proton-10.kaspa.stream, chain DAA 598,608,328)
- C6/r5/buy/10 `2a27b0aa0051d7f8a6f954c9933941f30a0492b6ddb1061a477075bf775c9dc1` (proton-10.kaspa.stream, chain DAA 598,608,327)
- C6/r5/buy/11 `74c8ef9c27e27a54cac32b2c7b83112b39e7d2d571d67774a2f2ebba23b25352` (proton-10.kaspa.stream, chain DAA 598,608,327)
- C6/r5/buy/12 `055132b183a57488311644558deebec6b151f43cb2320fbc6a295171860bb307` (proton-10.kaspa.stream, chain DAA 598,608,328)
- C6/r5/buy/13 `94fbb0e69209592239ab3f1d350920e4be1ffebf40dab67df16a09a93558d4f3` (proton-10.kaspa.stream, chain DAA 598,608,324)
- C6/r5/buy/14 `c08e1339e55e8ff98923b9446bd6f5d3e2255e82e40f0fd2de2eaf3eb294beb3` (proton-10.kaspa.stream, chain DAA 598,608,324)
- C6/r5/buy/15 `5b8bed1d32b7dc3db0d768b63eb3be4976d1e5928478d8f6bc6c40dcecc2080b` (proton-10.kaspa.stream, chain DAA 598,608,331)
- C6/r5/buy/16 `3bfabc79ad43d21c4de3d5894f685f83496513ebfddefd03649d15838aa25616` (proton-10.kaspa.stream, chain DAA 598,608,373)
- C6/r5/buy/17 `4b286d56d05cd9741f591ceb16f839459517d83ae7ed35f6409082b882327c2d` (proton-10.kaspa.stream, chain DAA 598,608,373)
- C6/r5/buy/18 `1a64df585abd589572973a068e560658504fb13bde4e5ec34e4f58d933f691ed` (proton-10.kaspa.stream, chain DAA 598,608,373)
- C6/r5/buy/19 `209e4815c0507cc7170e04c5df1ba53ef2cdc1529daf24ce905439faf74e2323` (proton-10.kaspa.stream, chain DAA 598,608,382)
- C6/r5/buy/20 `4c58d0621725e48309f0a6e1655630bf2efea078d17d9aeed88ae3f7b42cbb09` (proton-10.kaspa.stream, chain DAA 598,608,382)
- C6/r5/buy/21 `e584454409c099af65340eba0f3dc55cd824d881e8bc668c238f3938c84d1a3a` (proton-10.kaspa.stream, chain DAA 598,608,382)
- C6/r5/buy/22 `770706d1955d85eb4cbdd94d8bcc086406f318025b4ace20b14c56af51801241` (proton-10.kaspa.stream, chain DAA 598,608,382)
- C6/r5/buy/23 `ae4d203c2c8ad299f71dbcec32658fd71c780c48266d6bdc4ebd8528562de370` (proton-10.kaspa.stream, chain DAA 598,608,382)
- C6/r5/buy/24 `6fa51e34de7f72220745e317dff1cdeb4296fd4a14fc0944cba39e604ef78261` (proton-10.kaspa.stream, chain DAA 598,608,387)
- C6/r5/buy/25 `b63ad49bf814346a0916dc505226c4dd99d40e2cbc672c5574550ca5fec66458` (proton-10.kaspa.stream, chain DAA 598,608,387)
- C6/r5/buy/26 `a60516b9dab9e9bab0d74d098fb2c11ad847a9f927da89374597aece23429919` (proton-10.kaspa.stream, chain DAA 598,608,390)
- C6/r5/buy/27 `dcafa561376bd030a9924cb6c439dce5b8372131fdeec7c0678354c31262b4aa` (proton-10.kaspa.stream, chain DAA 598,608,387)
- C6/r5/buy/28 `4b8ba546d204350afdb1541265d51b599273d23aed4c325f496a1595228f61d2` (proton-10.kaspa.stream, chain DAA 598,608,390)
- C6/r5/buy/29 `b779f8a3782b39bbdca014c4746719d4cc8d54030b49bc6aed901c93986b9087` (proton-10.kaspa.stream, chain DAA 598,608,387)
- C6/r5/buy/30 `02486c323fc4bae9aa9ac0dd1f82fdd66c219e9a8fb4368a00ee3d980df22cb6` (proton-10.kaspa.stream, chain DAA 598,608,392)
- C6/r5/buy/31 `8a41915738f2fc87bc6035fb2c749027fa977ce3c8a7300622c658b091ca8790` (proton-10.kaspa.stream, chain DAA 598,608,392)
- C6/r5/buy/32 `b173e68ad0a4f01b946f4e7ec8d7d3eb041f1d7bb1e835627f11e0586941c84b` (proton-10.kaspa.stream, chain DAA 598,608,399)
- C6/r5/buy/33 `bbbcd35718e90530304a8cfca510ca469cabd7eb395327c935527e17bc65360e` (proton-10.kaspa.stream, chain DAA 598,608,403)
- C6/r5/buy/34 `a437934fdd45deaebb74659bb94a44a3f0d1ea44a42abef2a21b8e1a51a3489b` (proton-10.kaspa.stream, chain DAA 598,608,403)
- C6/r5/buy/35 `faf005f212640ffeb93609d29ce04743fd8fd38bef76660e01ad303ce1472ca6` (proton-10.kaspa.stream, chain DAA 598,608,403)
- C6/r5/buy/36 `f2d925eaa8bcb1faeb42c8f689a59ec1b55c643a0afb0958a66ab2f05ebd47ed` (proton-10.kaspa.stream, chain DAA 598,608,403)
- C6/r5/buy/37 `8c85987667867e5e68a363c8ccfc2a52a8585920ad5bce8595e1d4f94191cd24` (proton-10.kaspa.stream, chain DAA 598,608,403)
- C6/r5/buy/38 `8a8b9f35a0038997e522b34e47db9b587c6d9a8afa449183433bcbfed0a95807` (proton-10.kaspa.stream, chain DAA 598,608,410)
- C6/r5/buy/39 `12fa3e66f231ff537d89ffee9cc810fd5cc2e3253c3bf7bded3365cb54675fdd` (proton-10.kaspa.stream, chain DAA 598,608,410)
- C6/deposit 50.00000000 KAS (backs the carried withdrawals) `b2a052fcfc4a5c40dd7d053a07b9735be89f31b47c2109d1659d3e5b10f7ffbe` (proton-10.kaspa.stream, chain DAA 598,608,785)
- C6/close r5 `9156a244f15c7d860a1074c5cddf5a0f3f9f85e00e0c9ae519f64ee22b82b1a0` (proton-10.kaspa.stream, chain DAA 598,613,775)
- C6/draw r5 `afa2bff2524584baab122fc2cd20b7cc6f916aa33f4fbf275a1f1cf670c0c8c3` (proton-10.kaspa.stream, chain DAA 598,614,522)
- C6/carried r5/settle/9740e64736ff5ff4da79bceab481e832e6cc4f7b877fbe226633446cfba73757:1 `28b0223cd3b54189ec3b2bec45e81c6fdea0e95ade7482048c8086483365c778` (proton-10.kaspa.stream, chain DAA 598,614,543)
- C6/carried r5/settle/8cdf454e76a063f44b6df2f775049bc04d63226f93108e926d16ec9df8d91e33:0 `db3682ad4838aaeef20c5b6543be6999c250a49f37a66c5d562eec97e78e5446` (proton-10.kaspa.stream, chain DAA 598,614,559)
- C6/deposit r5/settle/b2a052fcfc4a5c40dd7d053a07b9735be89f31b47c2109d1659d3e5b10f7ffbe:1 `4860057e47fc370fb96a510e89ea10c8a35aebb0b0d6fd52dea77c88b936ba2a` (proton-10.kaspa.stream, chain DAA 598,614,581)
- C6/r5/claim/c08e1339e55e8ff98923b9446bd6f5d3e2255e82e40f0fd2de2eaf3eb294beb3:1/0 `adc7d97cf90bfcd75c76a6a06934d41492616d05206deca46327cf4659e1135c` (proton-10.kaspa.stream, chain DAA 598,614,609)
- C6/r6/buy/0 `2ad38de186f397a0c4c331881603596ba4b9c1255c98794e8f611bbbc93e1089` (proton-10.kaspa.stream, chain DAA 598,614,740)
- C6/r6/buy/1 `b8eed6505a4fa4b26490f7f4698aa679517579a1b2d1ef6c38613a01975c22bf` (proton-10.kaspa.stream, chain DAA 598,614,745)
- C6/r6/buy/2 `51a7183836304305bec31ad373de463d9f5f474edc0f34e440430d3648741e66` (proton-10.kaspa.stream, chain DAA 598,614,745)
- C6/r6/buy/3 `99505db4e8ce095eef5d6a29bbd6828983517c2341ca22130d5bfb2d92cf939e` (proton-10.kaspa.stream, chain DAA 598,614,745)
- C6/r6/buy/4 `ab7487a5d5b52075ebdf6d70557e1914a887e2c185fb05c8f4baa3a8d78fbf62` (proton-10.kaspa.stream, chain DAA 598,614,745)
- C6/r6/buy/5 `5dca32af17d601dcc5212efe16e7e06f015760126cbf93badc66be9de66be0c4` (proton-10.kaspa.stream, chain DAA 598,614,745)
- C6/r6/buy/6 `c4c84fd63f6f6195083338d69b256c553b3c7032827aa31989f75da645555fe7` (proton-10.kaspa.stream, chain DAA 598,614,745)
- C6/r6/buy/7 `6eda3bf2b9a7592448205a98554dcb483be6cc443b5e1c92b114d56d5b02d2b9` (proton-10.kaspa.stream, chain DAA 598,614,746)
- C6/r6/buy/8 `1129de89ea6465ea9b0777db56cddbb709eff9b25e3cef80044c85c2693953cd` (proton-10.kaspa.stream, chain DAA 598,614,746)
- C6/r6/buy/9 `37c4d9cfa844b92fb55f7dd3cb59b616ee014217ba7f624eaa39b72a06453d23` (proton-10.kaspa.stream, chain DAA 598,614,746)
- C6/r6/buy/10 `8928c26cbab2da292dad461280b0b7644ba3d3ef685711392b6c9f16e64fdf96` (proton-10.kaspa.stream, chain DAA 598,614,748)
- C6/r6/buy/11 `ba5f1f70326ff50674b1338a355ace7b601f105a3c45867a85adbe59170ccb52` (proton-10.kaspa.stream, chain DAA 598,614,746)
- C6/r6/buy/12 `5ac0c1da2938c660bcfecb92d7a07024e85b375323a488944039809cda942993` (proton-10.kaspa.stream, chain DAA 598,614,746)
- C6/r6/buy/13 `e6bd32e25627e22ccfcfac63842372d45db0ea6c22ec2138e07ba460bfb503f0` (proton-10.kaspa.stream, chain DAA 598,614,750)
- C6/r6/buy/14 `4c4dabcdedb7168bfa361f7640df9b196ed9d43423b8712ec0f58f86ff77a07d` (proton-10.kaspa.stream, chain DAA 598,614,750)
- C6/r6/buy/15 `c09373d10c795ec0d588ae71bda6e12dd247f0e96a08197636be015c450a7802` (proton-10.kaspa.stream, chain DAA 598,614,750)
- C6/r6/buy/16 `4588b8f93fd316555614b628c2f37a3ee46fd7e93a2de627a299fe51f831278c` (proton-10.kaspa.stream, chain DAA 598,614,814)
- C6/r6/buy/17 `2b471e1530006e4b1908945ac1e98f5b162692a7e65cf44f1d4bac3bcd56e69c` (proton-10.kaspa.stream, chain DAA 598,614,814)
- C6/r6/buy/18 `13828e37eb38e96efb29e298218f43b15994bca0b8639cb1b7ed312ef13bafdc` (proton-10.kaspa.stream, chain DAA 598,614,814)
- C6/r6/buy/19 `99f06c1ed6494e6775ff01697c3ea453c57633ab6a642566d8462813229e99bc` (proton-10.kaspa.stream, chain DAA 598,614,814)
- C6/r6/buy/20 `cbe2bb41a1bdf4e5367fd572eaa2c720dde588e8b1e80e71c7f2c62f19fd6417` (proton-10.kaspa.stream, chain DAA 598,614,814)
- C6/r6/buy/21 `0ddca392babe414e661645d3af68e3f817b6a8a3a1da8ae757667068dcae66c3` (proton-10.kaspa.stream, chain DAA 598,614,815)
- C6/r6/buy/22 `d3555657e818539a74ae8e5a12960e19f0db0030081f718e49d63669d64fae4d` (proton-10.kaspa.stream, chain DAA 598,614,815)
- C6/r6/buy/23 `f181e35905a341423ddd7b58bcdedee446844fa9cd40be9dd92a469547f60ddd` (proton-10.kaspa.stream, chain DAA 598,614,815)
- C6/r6/buy/24 `269338ab59e2b3a7a27cf803c2b9245446952e46540e514565f285359ed97d39` (proton-10.kaspa.stream, chain DAA 598,614,817)
- C6/r6/buy/25 `7833b2da7d89384195b925739fe1f136015a8e69f79655d60becb898cef6d0ee` (proton-10.kaspa.stream, chain DAA 598,614,815)
- C6/r6/buy/26 `43bd0437b9917eaf6641dc521de5039b522a08e760ce35792c6b97ed7d4aeab3` (proton-10.kaspa.stream, chain DAA 598,614,815)
- C6/r6/buy/27 `99d834fc84f4302117c61518703da6f4102a3bcd39f10b63a9d47e0f818bc494` (proton-10.kaspa.stream, chain DAA 598,614,815)
- C6/r6/buy/28 `f5c7c334c4c99d195984a28d9e0595ad6b12da85fa2b13dc8588f9fcb176b6e8` (proton-10.kaspa.stream, chain DAA 598,614,826)
- C6/r6/buy/29 `405c41cc257e5b7cdc1699408c54aba99e196166136b95f97f267bcb9bfd8058` (proton-10.kaspa.stream, chain DAA 598,614,826)
- C6/r6/buy/30 `51d5a12fa77ab39e5062b675a9a29dabf24629a4b0d4375f69bbfa4e87871d4a` (proton-10.kaspa.stream, chain DAA 598,614,826)
- C6/r6/buy/31 `0ccdbf68f5f776212a63e85d1fdc2f4e768fc7710d8c881623e63ac5e2af19cf` (proton-10.kaspa.stream, chain DAA 598,614,826)
- C6/r6/buy/32 `5e0227b8c74905f318e03345296394f728a38803a7bdedfbe15f0ccebd96bc48` (proton-10.kaspa.stream, chain DAA 598,614,851)
- C6/r6/buy/33 `1ec3878d466d8ac724500feb66eb51d96d9b5925a8ba2251c4515289e004b423` (proton-10.kaspa.stream, chain DAA 598,614,858)
- C6/r6/buy/34 `b751f126750488f6492067e63615edbbdf1ed3e660f8de8a164f99e0de803dff` (proton-10.kaspa.stream, chain DAA 598,614,858)
- C6/r6/buy/35 `e28938023bf5241e909508cb813b3340702a739b65c4e09a7f4de7db307dc70e` (proton-10.kaspa.stream, chain DAA 598,614,858)
- C6/r6/buy/36 `fcbb11dc9c67f601223be94b8832ebb5ddf7f3bf4afd74e760ec654cbf1de2b0` (proton-10.kaspa.stream, chain DAA 598,614,858)
- C6/r6/buy/37 `949aa8e966e88d7065b6332d5ea6b1438b7edc45b4df4b92492db5020b2fcc24` (proton-10.kaspa.stream, chain DAA 598,614,858)
- C6/r6/buy/38 `9b5300d7842f5fb9329ca025373f9a65b7befb9e32a8f651dc8f4d6f7faadbb0` (proton-10.kaspa.stream, chain DAA 598,614,866)
- C6/r6/buy/39 `caed2f7e3070c388a1ac4c30ef949047d275faf51934df9a629978824958b28f` (proton-10.kaspa.stream, chain DAA 598,614,866)
- C6/close r6 `80e35d62fb42851c2982f8ef0e1e74402099add10b292f150e8a81c2d84997d0` (proton-10.kaspa.stream, chain DAA 598,619,963)
- C6/draw r6 `98ce1ffe76f1ca4688227c20a6d3c76b95ab8e1b63ab6771eec3c1ea4a584a1d` (proton-10.kaspa.stream, chain DAA 598,620,662)

## M8 S4: 30-hour soak C8a-C8c, mode B (spec 8 A5 C8, D117; 2026-10-17 to 2026-10-25)

Same instance as the fast campaign: covenant ID `e9f4f810c86467e7768aa1b42e6c62182b8a0d934bda922bdbb2804f223988d6`. Soak round **8**, deadline T = DAA 598,647,336.

### Sittings

| Sitting | When (local) | Result |
|---|---|---|
| 1, C8a | 2026-10-17 02:50-03:00 | round 7 closed `db81b784c9a7e94f68512819a23b29286ec5e901477b2c312143e11fd43813f7` and drawn empty `6ea01d94c4681c50b9c7e3f67f2c7b7970dd94b41ff8ecb20374f7d413d775c8`; 75 buys (b = 4) in round 8; t0 = 02:58:30 (first buy `c508432c397c56a0060b3f52735aac71ef918d5e360d7c62d0faad0871daef2d`, chain DAA 598,642,246, chain block `e0a62803c331902af3c429c0936bfc84244dc7c1fe4efbfcfaf7c435248257d5`) |
| 2, C8b | 2026-10-17 14:31 | indexer caught up inside the pruning window (chain block `e0953ebf…e4b5`, DAA 599,058,255), 0 transactions; about 25 min for about 417,000 DAA |
| 3, first try | 2026-10-18 ~20:00-21:01 | never reached the close: public nodes timed out on long `get_virtual_chain_from_block_v2` catch-up calls, then the run was killed for low memory. Nothing submitted |
| 3, C8c | 2026-10-25 02:30-02:33 CEST | re-anchor, close, deletion evidence, draw from the indexer store, four claims (dual build), refresh; 12 checks, 0 failed |

### Re-anchor (PENDING A4 amendment)

By 25 Oct the saved resume block `e2f2ddcfbea137fb31e7e5aeece693944354555abc00f8731472462313d1cf70` was below every node's pruning point (`boson-10.kaspa.red`: "cannot find header"; pruning point DAA 604,298,078, 34.3 h back, virtual DAA 605,531,267). `tn10 campaign reanchor` (wss://vector-10.kaspa.green/kaspa/testnet-10/wrpc/borsh, wall 1,792,888,212) picked the sink first, then read the utxoindex (`get_utxos_by_addresses`): **all 380 live instance UTXOs unchanged** (script, value, creation DAA, covenant ID); 578 entries at their addresses and ours, 198 not instance-bound, **no unknown instance-bound UTXO**. The `index/1` record (log line 406, `at` 405):

- old block `e2f2ddcf…cf70`, old_daa 598,642,669 (the newest logged transaction; the snapshot of that time did not store the block's own DAA, about 600.02 M);
- new block `1c6f6873fb149b31523f3355e049ee1e3e89d68650b2541a8ffed76696af6f72`, DAA 605,547,454;
- argument: KIP-20 (`refs/kips/kip-0020.md:25-27, 33`) lets an output carry the instance covenant ID only by spending a UTXO with that ID (or by a genesis over a unique outpoint), so every instance transaction spends an instance UTXO; an unchanged live set means no instance transaction happened between the two blocks.

The catch-up from the new block took seconds (chain block `428c66ff…d25e`, DAA 605,547,467). Round 8's buys were indexed on 17 Oct and their trees were in both stores, so the soak evidence does not depend on the gap.

### C8c transactions (all through wss://electron-10.kaspa.blue/kaspa/testnet-10/wrpc/borsh; accepting chain DAA)

- fresh funding `dfe3ea7397a80196688d0c7185b8a5402bbdc7721223e8f236e295c310065812` (10 KAS, plain)
- close r8 `6e9ddf5bb426a45f235e5b5f181e6d50388fe40089c9dcdb4cae626f0a4d2b9b` (605,547,994; fee 0.4323 KAS): round 9 m' = 2, s_R = 500; wallet and indexer roots = the frozen roots
- draw r8 from the **indexer** store `a08b156fb90063e62da8a760b3afbcf0a58bf3c16dee9d2268075dba1a8eed6c` (605,548,738; fee 0.2215 KAS): D = 605,548,594, anchor X `204c2a2a3d6bcda377600b272d2990b0480520253e11775de22e7eb9bb0b2a7e` (DAA 605,548,594), P `9277a7f7…86d9`; conversion and record = model (D124); fee output paid to round 9's fee address; win [2, 6, 8, 13, 17] + 2; pools tier 2 13.69, tier 3 10.69, tier 4 10.69 KAS
- claims, each built from the wallet store and from the indexer store, identical except the funding signature (same txid; 2 covenant inputs and all 3 outputs equal), the indexer-built one submitted; all tier 4, h 1, prize 0.003 KAS, ACCRUE:
  - `43fb4cf3c4405e61ddb6e50c4af5e250f7c91348f5b5aa412f8b71f833f8b37f` (605,548,761)
  - `2e9508db40615cfd8c955cd42112a9bf574b6edb85b001faf4254538909f97be` (605,548,883)
  - `69744d6a4017bcdf0bc7fb2b1e8dbfe51f9045d2f4ee56f2ba4abac1cdd55f90` (605,548,940)
  - `31deb51166dfe2919160f8971f3fcc5ec19fd3c2f976e8b0f30f709222e438af` (605,549,055)
- refresh of the oldest round-8 ticket (idle about 8 days) `d9886d26a583c208586ff8b19ad2c271724bcd0550867dc558af648aac5d6d60` (605,549,110; fee 0.0769 KAS)

### Deletion evidence (D117)

Before the draw, the runner's check: `get_block(e0a62803c331902af3c429c0936bfc84244dc7c1fe4efbfcfaf7c435248257d5)` failed with "RPC Server (remote error) -> cannot find header" (the chain block that accepted round 8's first buy). The same block was already refused by `boson-10.kaspa.red` at 02:04 CEST. The draw and the claims were built about 192 h after the buys (D117 needs more than 30 h).

### A7 items

Both are exercised: a claim from wallet-kept trees more than 30 h later, and a claim from the indexer copy (four dual-built claims, the indexer-built ones submitted). The C5c "claim from the indexer copy" that luck skipped in S3b is covered here.

### Late-close finding (U6, D102; owner accepted 2026-10-25)

The latest close of round 8 that keeps its pools' claim deadline at or before the tickets' `return_deposit_after` (T + D_RET = 610,743,336; `ticket.sil:379`, D_RET 12,096,000) was DAA 610,743,336 − (600 + 864,000 + 6,048,000) = **603,830,736** (about 23 Oct). The close came at 605,547,994, so round 8's claim deadline is D + 6,912,000 = 612,460,594. From about 31 Oct until that deadline, anyone could return an unclaimed winning round-8 ticket and the prize would be lost. Every round-8 ticket is ours, and every winning index was claimed in this sitting, so nobody lost anything. D102's bound is reachable through a late close alone, with no missed draw (PENDING row in the M8 handoff).

### Spend and timing

Sitting 3 cost about 1.05 tKAS in fees. The balance after it is 5,819.01 tKAS, so **890.54 tKAS spent or locked** since the start (cap 950). That is 11.87 tKAS more than the 5,807.14 recorded after sitting 1, probably the draw's fee output (s_F of round 8's sales) paid to our fee address (not verified).

Timing: a catch-up of about 29 h through public nodes needs many retries. Single long v2 calls hit the wRPC client's fixed 60 s request timeout, and WebSockets drop. The runner now retries up to 12 times on a fresh resolver connection and resumes from the last saved batch. Sitting 2 should sit late in its window, and sitting 3 must keep the close inside the U6 bound (here at most T + 6 days).

## M8 S4: private simnet, forced jackpot, duplicate and second prize (SN1-SN5; spec 8 A5b, D114, D125; 2026-10-17)

**Simnet only. Not a testnet-10 or mainnet result, and not a mainnet cost.** Each scenario ran on its own fresh in-process rusty-kaspa v2.1.0 node (`--simnet`, no peers, inbound 0, no DNS seeding or UPnP, Borsh wRPC on 127.0.0.1 only, fresh data directory under `crates/target/simnet-data/`). The runner was the only miner. It used the production templates (`Templates::get(b_max())`, `short_void_window = false`) and no test-only game or seed variant. Every lottery transaction was built by the `lottery` builders, run first in the local v2.1.0 engine, with each covenant input's entry compared to the node's UTXO-index entry, then sent with `submit_transaction` and mined from the node's own templates. The one block the runner changed is the anchor X: the template at DAA D (its selected parent, the sink, at D − 1), with only the nonce changed. That block was hashed locally (`hashing::header::hash_override_nonce_time`, `refs/rusty-kaspa-v2.1.0/consensus/core/src/hashing/header.rs:5-35`), and simnet skips proof of work (`consensus/core/src/config/params.rs:737`). After 50 more blocks, `tn10/src/anchor.rs` rebuilt and self-checked the 272-byte proof from RPC (`get_seq_commit_lane_proof`), unchanged, and its X equalled the ground block. Command (from `crates/`, with `LIBCLANG_PATH` and `PROTOC` set): `cargo run -j 2 -p simnet --release -- all` (or `sn1` … `sn5`). The run below is `all`, 14.9 s in total, with node data in `simnet-data/1792200531729`, `…535455`, `…539112` and `…542495`. Every draw, claim and settle output value equalled the model (`DrawCase::expected`, `ClaimCase::expected`, `SettleCase::expected`). Each round result's X.hash and winning numbers equalled `game::seed_numbers` of the ground header. All four held main numbers are [6, 10, 22, 25, 27] (colex rank 77,777). K = 16, price 1 KAS, cap 2,000 bp.

| SN | Setup | Result vs model | Transactions |
|---|---|---|---|
| SN1 | m = 3, V0 1,000 KAS, J(1) 100 KAS with J_boost 20 KAS (pot 30 KAS), G2 1 KAS, L(1) 82 KAS; one ticket of c = [6, 10, 22, 25, 27]+2 | h_J = 1, n2 = 0. Jackpot pool = J = 100 KAS (+ R). The vault pays J_house = 80 KAS: V' = 1,000 + H 0.74 − 80 = 920.74 KAS (`game::vault_settle`). The boost pot pays J_boost = 20 KAS: it moved into the marker at close (O 30 → 10 KAS), and O' = 10.10 KAS does not get it back (`game::boost_after` with jp). The claim pays all of J: FINAL, h = 1, prize 100 KAS, payout 100.13 KAS (J + ticket value), pool' = R. Afterwards L' = 184.148 KAS ≤ cap × V' = 184.148 KAS (the cap binds). | genesis `fe30d163…0271`, buy `75fc7c0f…79a2`, close `688b2be0…18f7`, draw `a44cdfb6…20f0`, claim `fd68c2a2…3660` |
| SN2 | m = 2, J(1) = 100 KAS + 1 sompi (odd), G2 1 KAS; c = [6, 10, 22, 25, 27]+2 in a b = 1 ticket on shard 0 and a b = 2 ticket [c, [7, 10, 22, 25, 27]+1] on shard 1 | h_J = 2. Jackpot pool = J (+ R). Claim 1 (shard 1, idx 0, PAY) pays floor(J / 2) = 50 KAS. Claim 3, the same index of ticket' again, was sent while the pool still held 50.00000001 KAS, so only the claimed bit can refuse it. It was refused by the model ("check 4: index already claimed"), the local engine (input 0, VerifyError) and the node ("failed to verify the signature script: script ran, but verification failed"). Claim 2 (shard 0, FINAL) pays floor(J / 2) = 50 KAS. The remainder of 1 sompi stays in the pool (pool' = 1 sompi + R). | genesis `cabdb72d…83e1`, buys `e71c0626…abf6`, `cf9ae0a4…a0df`, close `64dd6734…900f`, draw `0c0575bd…7c6e`, claim 1 `d7babe06…3a2e`, claim 3 refused `7c753407…14a6` (not mined), claim 2 `753833c3…03d8` |
| SN3 | m = 3, J(1) 100 KAS, g = 1,000 bp so G2 = 10 KAS, L(1) 120 KAS; [6, 10, 22, 25, 27]+1 on shard 0 and +2 on shard 1; ground for bonus 3 | h_J = 0, n2 = 2. No jackpot pool. Second-prize pool = 2 × G2 = 20 KAS (+ R). J_house = 100 KAS stays in the vault: V' = 1,000 + H 1.48 − 2 G2 = 981.48 KAS. Both claims pay G2 = 10 KAS (h = 1 each), and the pool is drained to R. | genesis `36f5f442…b404`, buys `3ca50ab7…f2aa`, `956cbdf1…1758`, close `af961387…72f8`, draw `0a07a601…77db`, claims `958053bd…0e64`, `1de9e6d7…083a` |
| SN4 | m = 2, J(1) 100 KAS, G2 10 KAS, L(1) 110 KAS; [6, 10, 22, 25, 27]+1 on shard 0 and +2 on shard 1; `request_withdraw()` of the whole deployer position (all 10^11 shares, the only backer) in round 1 | Drawn bonus 2: h_J = 1, n2 = 1. The vault pays J_house + G2 × n2 = 110 KAS ≤ L(1) = 110 KAS (equal at m = 2). The conversion after the hit equals `game::convert(V' 891.48 KAS, S 10^11, D 0, W 10^11, L' 178.296 KAS)`: wc = 80,000,000,800 of 10^11 shares, out_c = slack = 713.184 KAS. It keeps L': vault' total_assets = 178.296 KAS = L'. The withdrawal's `settle()` equals `game::settle`: PAY, 80,000,000,800 shares converted, pay 713.184 KAS, 19,999,999,200 shares carried. The jackpot claim pays J = 100 KAS and the second-prize claim pays G2 = 10 KAS. | genesis `674d5b92…6804`, request_withdraw `5f4fe4c3…8daf`, buys `e37f15d4…41cb`, `40d4049c…a139`, close `91978514…f761`, draw `2984dcbf…6b42`, settle `8139cc28…6437`, claims `91673147…b053`, `25dbccb8…6671` |

**SN5, grinding** (one thread on the owner's Windows PC, release build; each try is one header hash plus the seed map; the honest first candidate is the template's own nonce 0):

| SN | Ground for | Tries | Expected 1/p | Seconds | Ground X → numbers | Honest first candidate → numbers |
|---|---|---|---|---|---|---|
| SN1 (m = 3, D = 2,269) | c = jackpot (main and bonus), p = 1/(N·3) | 446,146 | 427,518 | 0.521 | `984db5a0…87bd` → [6, 10, 22, 25, 27]+2 | `ea1b5f55…2edd` → [7, 8, 10, 14, 23]+1 (c: no tier) |
| SN2 (m = 2, D = 2,269) | c = jackpot, p = 1/(N·2) | 71,106 | 285,012 | 0.076 | `d262f3fe…a00a` → [6, 10, 22, 25, 27]+2 | `a6a87863…523b` → [8, 16, 23, 27, 30]+1 (no tier) |
| SN3 (m = 3, D = 2,269) | held main, bonus 3, p = 1/(N·3) | 88,889 | 427,518 | 0.104 | `fb6400d7…5117` → [6, 10, 22, 25, 27]+3 | `50d71525…4fe0` → [1, 6, 18, 20, 29]+3 (main differs) |
| SN4 (m = 2, D = 3,069) | held main, either bonus, p = 1/N | 129,055 | 142,506 | 0.136 | `f9388fc8…dd95` → [6, 10, 22, 25, 27]+2 | `788c2c98…2a03` → [1, 6, 11, 12, 29]+1 (main differs) |

In total: 735,196 header hashes in 0.84 s, about 0.86–0.95 million tries per second, against 1,282,554 expected. In every scenario the ungrounded first candidate gave a different result from the ground one. **What this shows (D100):** a sole miner chooses X.hash for one header hash per try and can make a chosen combination win (expected C = 142,506·m tries for one target combination) in under a second on a PC. On simnet nothing competes for the block and proof of work is skipped. On a real network every try is a full proof-of-work solution, and the block must still win the race. So this is evidence of the mechanism, not a mainnet cost. Earlier runs on the same code: SN1 850,198 tries in 0.89 s; SN2 140,311 in 0.16 s; SN3 80,482 in 0.10 s; SN4 61,874 in 0.07 s.

## M8 S5: review tools (sil-debug, kascov preflight, bundle refresh; spec 8 A7, D116; 2026-10-25)

Nothing was submitted to any network, and no `.sil` or repo code changed. The helper programs below are scratch tools in the session scratchpad, not in the repository.

### sil-debug on the campaign's `draw()` and `claim()` spends

**Build.** `cli-debugger` from the pinned `refs/silverscript` v1.0.0 (`3ed97333`, `debugger/cli`), `cargo +1.94.0 build -j 1 --release --locked -p cli-debugger` with `CARGO_TARGET_DIR=crates/target/sil-debug` (8 min 27 s). `refs/` is unchanged (`git status` clean). The debugger links rusty-kaspa **`a41a333b`**, SilverScript's own revision (`refs/silverscript/Cargo.toml:24-31`), not our pinned v2.1.0. `crypto/txscript/src` differs between the two revisions (`lib.rs`, `opcodes/mod.rs`, `data_stack.rs`, `runtime_resource_meter.rs`, zk precompiles: `diff -r` of the two cargo checkouts). This is the open toolchain task in the DECISIONS pins row (2026-10-09), so a debugger result is evidence from that engine revision only.

**Inputs.** Round 8's draw `a08b156fb90063e62da8a760b3afbcf0a58bf3c16dee9d2268075dba1a8eed6c` (input 0, the drawer) and claim `43fb4cf3c4405e61ddb6e50c4af5e250f7c91348f5b5aa412f8b71f833f8b37f` (input 0, the ticket). Both were rebuilt from the indexer store (`tn10-data/e9f4…/index-log.jsonl` and `bundle.json`): the real transaction, each spent UTXO's value, creation DAA, script and covenant ID, and the input-0 state decoded by `lottery::index::State::decode` (constructor arguments `TicketState::ctor(pool hash)` and `DrawerState::ctor(controller hash)`). The entry arguments were decoded from the real signature script. The debugger test file gives input 0 by constructor arguments, every other input with its real UTXO script and signature script, and every output with its real script and covenant binding. The committed sources `Sources::committed(4)` for the ticket and the drawer are byte-identical to `covenants/ticket.sil` and `covenants/drawer.sil`, so the debugger ran on the repo files.

**Stock `cli-debugger --run`:**

| spend | result |
|---|---|
| claim (`ticket.sil`, entry `claim`, 5 args) | **PASS** |
| draw (`drawer.sil`, entry `draw`, 23 args) | stops at **check 7**, `drawer.sil:498` `require(mk_daa == mkDaa)`. The stock CLI creates every UTXO with `block_daa_score` 0 (`debugger/cli/src/main.rs:944`), so `OpTxInputDaaScore(1)` reads 0, not the marker's 605,547,994 |

**Patched copy (scratch).** The patched copy is `debugger/cli/src/main.rs` copied, built against the same `debugger-session` crate in `refs/`. The patch changes only the transaction context: (1) each UTXO gets its real creation DAA; (2) a `SeqCommitAccessor` answers for the anchor X only, with the commitment recomputed from the proof's X preimage exactly as check 8 does. On chain the node supplied that commitment and accepted the draw. (3) It checks that the redeem script compiled from the source and constructor arguments equals the on-chain one. (4) It counts the stack peak opcode by opcode.

The patch is needed for two reasons. With (1) but without (2), the draw stops at `drawer.sil:544` with "attempt to execute invalid opcode Opcode<0xd4>": the stock CLI gives the engine no accessor (`main.rs:948`), and `OpChainblockSeqCommit` is then invalid (`a41a333` `crypto/txscript/src/opcodes/mod.rs:1581-1585`). Second, the stock `--run` reports PASS once every opcode has executed (`session.rs:366-405`). It does not apply the engine's final clean-stack check, so the patched copy prints the final stack.

| spend | redeem compiled = on chain | result | opcodes run (redeem script) | stack peak (dstack + astack; limit 244, rusty-kaspa v2.1.0 `crypto/txscript/src/lib.rs:77`) | final stack |
|---|---|---|---|---|---|
| claim | yes (25,452 B) | **PASS**: every `require` passes | 22,453 | **88**, at `ticket.sil:331` (check 15, `validateOutputState` of ticket') | `[01]` |
| draw | yes (64,629 B) | **PASS**: every `require` passes, the anchor checks 7-8 included | 57,170 | **240**, at `drawer.sil:994` (check 24, `validateOutputStateWithInputTemplate` of controller') | `[01]` |

**What it shows.** The entry points are `claim(idx = 2, leaves, siblings, pool_pl = 1, pool_sl = 1,831)` on a round-8 ticket (b 4, deadline 598,647,336, claimed 0x00) and `draw(anchor 272 B, …)`. Every `require` passes on the real bytes. The draw's runtime stack peak of 240 at check 24 leaves **4 free slots**, the same headroom that `docs/costs.md` M7-f measured at compile time (n = 5 extra locals fails with 245 > 244). This is a runtime confirmation of the D104 floor, now on a real testnet spend. The claim peaks at 88. Surprises: the three debugger limits above (UTXO DAA 0, no seq-commit accessor, PASS without the final clean-stack check), plus the debugger's own engine revision. In the stock CLI none of them changes the claim's result. All of them matter for `draw()`.

### kascov.io preflight

**How to submit (from the site, 2026-10-25).** `POST https://kascov.io/data/testnet-10/preflight` takes the transaction in the SDK/RPC JSON shape. Each input carries `utxo {amount, scriptPublicKey}`. The engine runs on every input that has both its `signatureScript` and its UTXO script. The site states that it is public and keyless, the response is `no-store`, and nothing is broadcast or kept (`https://kascov.io/openapi.json`; the playground's `#/preflight` page). No account, login or registration was needed. The `kascov-decode` crate offers no preflight. `GET /data/testnet-10/debug/{txid}` is the spend-trace replay. kascov had already indexed the instance from public chain data, on its own: covenant `e9f4…88d6`, 384 events, 383 live UTXOs, `lineage_complete` true. Its server runs SilverScript v1.0.0 `3ed97333` (`/data/testnet-10/compilers.json`). Its rusty-kaspa revision is not published.

**What was sent.** For each shape, one accepted campaign transaction, rebuilt from the index log with every input's spent UTXO (amount, script, `blockDaaScore`, `covenantId`), the outputs with their covenant bindings, and the lock time. The funding inputs' script is the transaction's own zero-ID change script; the funding signature check passing in every run confirms it. The bodies hold public testnet-10 bytes only: no keys, no `.env`, no `kaspa:` string (checked). The redeem scripts sent are the ones already revealed on chain. No `.sil` source was uploaded: the compile endpoint was not used. All fourteen calls answered HTTP 200.

| shape | txid | kascov verdict | inputs run: pass / total | kascov compute / transient / storage | ours (`tn10 masses`; storage = the committed mass) |
|---|---|---|---|---|---|
| buy (b = 4) | `f874f270…2dfc` | **ready** | 2 / 2 | 46,022 / 158,488 / 83,083 | 46,022 / 158,488 / 81,507 |
| buy with referral | `10241a93…1503` | **ready** | 2 / 2 | 46,434 / 158,696 / 109,479 | 46,434 / 158,696 / 109,470 |
| set_params | `ef3f00a8…3d36` | **ready** | 2 / 2 | 6,404 / 13,896 / 23,663 | 6,404 / 13,896 / 23,654 |
| close r8 (K = 16) | `6e9ddf5b…2b9b` | **will_fail**: input 0 (controller) `input_script_failed` after 1,493,292 of 3,379,999 units | 15 / 16 reported (inputs 1-15 pass; inputs 16-18 not reported) | 293,743 / 864,612 / 418,251 | 293,743 / 864,612 / 393,234 |
| draw r8 (real anchor) | `a08b156f…ed6c` | **will_fail**: input 0 (drawer) after 370,579 of 1,499,999 units | 4 / 5 (marker, vault, controller legs and funding pass) | 139,536 / 442,944 / 41,212 | 139,536 / 442,944 / 41,104 |
| draw of the no-sales round | `596ef438…fbae` | **will_fail**: input 0 after 370,539 units | 4 / 5 | 137,613 / 442,052 / 32,419 | 137,613 / 442,052 / 32,320 |
| claim (tier 4, ACCRUE) | `43fb4cf3…b37f` | **ready** | 3 / 3 | 48,190 / 151,160 / 80,207 | 48,190 / 151,160 / 80,170 |
| request_deposit | `ae05e4a1…4115` | **ready** | 2 / 2 | 23,195 / 77,580 / 23,991 | 23,195 / 77,580 / 23,982 |
| request_withdraw | `ee68b8f6…bc32` | **ready** | 3 / 3 | 24,729 / 82,516 / 23,769 | 24,729 / 82,516 / 23,744 |
| settle, deposit convert | `3e3bf0d8…6b38` | **ready** | 3 / 3 | 22,002 / 73,248 / 29,571 | 22,002 / 73,248 / 29,546 |
| settle, PAY | `b583d8cd…2d5a` | **ready** | 3 / 3 | 23,159 / 73,596 / 24,112 | 23,159 / 73,596 / 24,087 |
| settle, carried (PAY) | `717ff462…db69` | **ready** | 3 / 3 | 23,159 / 73,596 / 25,943 | 23,159 / 73,596 / 25,913 |
| settle, FINAL | `28b0223c…c778` | **ready** | 3 / 3 | 22,102 / 73,248 / 1,332 | 22,102 / 73,248 / 1,302 |
| refresh | `d9886d26…6d60` | **ready** | 2 / 2 | 29,097 / 103,468 / 76,917 | 29,097 / 103,468 / 76,907 |

Not exercised: a void (missed-window) draw, because the campaign had none (the TEST-ONLY `void-m7` probe is a different instance), and `return_deposit_*`.

**Where kascov differs from our engine and the node** (each of these transactions was accepted by testnet-10 nodes and by the local v2.1.0 engine):

1. **UTXO DAA score not modelled.** Removing `blockDaaScore`, setting it to 0, or sending it as a string gave the same failure at the same script-unit count for the draw (370,579) and the close (1,493,292), so kascov ignores the field. The drawer's check 7 compares the `mk_daa` argument with `OpTxInputDaaScore(1)`. The stock debugger with UTXO DAA 0 fails exactly there (above), which is very likely kascov's draw failure. The close computes T' from the newest input's creation DAA (`controller.sil:473-484`, `OpTxInputDaaScore` at `:475`), so the controller' it expects differs from the output. This is inferred from the field test and the code; kascov's trace was not read. The buy, claim and refresh pass because their DAA checks are lower or upper bounds that a constant DAA satisfies. Because the draw stops at check 7, **whether kascov supports `OpChainblockSeqCommit` (KIP-21) could not be tested**. Result for `close()` and `draw()`: **unsupported** (preflight cannot model creation DAA), not a defect of the spends.
2. **Storage mass.** kascov's storage mass is 9 to 25,017 grams above the storage mass committed in each accepted transaction, which equals ours: buy +1,576, close +25,017, the others +9 to +108. Compute and transient mass agree exactly for all fourteen. kascov's fee estimate is its own formula (100 sompi × compute grams + 200,000), not the node's fee rate on the priority mass (D120).
3. **Close: only 16 of 19 inputs are reported** in `executed` (inputs 16-18, two shards and the funding input, are missing). No note in the response explains it.

**Spend-trace replay** (`GET /data/testnet-10/debug/{txid}`, the same fourteen txids): every one answered `ok: true`, `pass: false`, "script ran, but verification failed". kascov documents this: "replayed against a fabricated 1-in/1-out transaction … introspection checks … can fail here even though the spend passed on-chain". Each replay runs one tracked input, for example the pool leg for the claim and the drawer for the draw. Our scripts introspect other inputs and outputs from their first checks, so the replay has no review value for this protocol. The traces are capped at 2,000 opcodes, and close, draw and refresh were truncated.

### Bundle refresh

`cargo build -j 1 --release -p tn10` (clean), then from `crates/`:

- `./target/release/tn10 bundle --out ../docs/launch/tn10-m8.json` (vector-10.kaspa.green, v2.1.0, synced, utxoindex): covenant ID recomputed, 11 templates = the local compilation, genesis checks pass, **383 live UTXOs** (redeem = template + state bytes, P2SH = script), every one present on the node; wrote the file, **598,201 bytes, snapshot DAA 605,601,500**. The previous S3b bundle was 545,095 bytes with 305 UTXOs at DAA 598,620,952.
- `./target/release/tn10 bundle-verify ../docs/launch/tn10-m8.json` (muon-10.kaspa.blue, DAA 605,602,325): `bundle-verify ../docs/launch/tn10-m8.json: PASS`, **exit 0**. kascov's own index also counts 383 live UTXOs for the instance.
