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
2. **Replacement (D87):** `submit_transaction_replacement` with a strictly higher feerate was accepted by the node only when A was still in its mempool: 1/10 in run 3 (rep 5) and 1/1 in run 2. **In both cases A, not B, was accepted on chain**; B was dropped (C on B was an orphan right after the replacement returned, and B was never merged). In the other 9/10 the node answered "no double spending transaction in the mempool", because A was already block-included. The node broadcasts an accepted replacement and P2P peers insert with RbfPolicy::Allowed (`protocol/flows/src/flow_context.rs:676-690`, `protocol/flows/src/v10/txrelay/flow.rs:233`), but a block that already holds A removes its double spends from the mempool (`mining/src/mempool/handle_new_block_transactions.rs`, `remove_double_spends`). **So, at 10 BPS with a ~0.46 s round trip to a public node, the covenant spend is RBF-eligible at the node, but it did not displace a pending transaction in any of 11 attempts.** This is an input to D87 (the "honest request outbids the attacker's pending request" argument); nothing was redesigned.
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

Balance after the RV-1 re-runs: 7,333.81 testnet KAS (7,542.77 before them).

Locked on testnet after the re-runs: for `claim-m4` and `vault-m5`, the round 2 objects, drawer', vault', the pools (and in `vault-m5` the new position and position'); for `buy-m1`, both shard successors and both tickets. Payouts, referrals, refunds and changes are ours. Balance after all M6 runs: 7,542.77 testnet KAS (480.11 KAS locked or paid in fees by `conflict-m6` runs 2 and 3, the two `buy-m1` runs, `claim-m4` and `vault-m5`).
