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
  - Inputs: drawer, marker, vault, controller, and one funding input (the close caller output). lock_time = seed_ref. Each input's creation DAA score is read from our UTXO index.
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
   - This is an open item in the M3 handoff (O-S4-4).
3. **Run 3**: the probe above, with the entry check, was accepted.

Locked on testnet (no spend path in M3): the round 2 objects, drawer', vault', controller', the 4 pools and the ticket of run 3. Runs 1 and 2 also left their genesis (and run 2's round 1 to 2) objects. The caller and marker refund outputs are ours. Balance after: 8,648.65 testnet KAS.
