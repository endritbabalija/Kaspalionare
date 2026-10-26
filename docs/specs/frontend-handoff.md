# Front-end track: build handoff

Spec: `docs/specs/frontend.md` (approved 2026-10-25, D129–D139). Delete this file when the track closes and its content is in `docs/DECISIONS.md`.

Trimmed after F4's review (owner process decision, DECISIONS 2026-10-26 "full rigour for money paths"): finished stages keep a few lines and their commit; their full notes, per-item tables and command outputs are in git history (`git show <commit>:docs/specs/frontend-handoff.md`; the last untrimmed version is `0385ed9`). Kept in full: the stage log, what F5 and F6 need, the open items, the proposed DECISIONS rows not yet recorded, and this stage's section.

## How the build session works
- The build runs in a fresh chat. That chat orchestrates: each stage F0–F6 runs in a fresh agent, and the chat reviews each report. Reviewers run as separate agents (`covenant-reviewer` after F1; a security review in F6). The chat brings owner decisions back, records them and commits.
- Each stage agent reads, in this order:
  1. `CLAUDE.md`
  2. the spec
  3. this file
  4. the files the stage touches
- Each stage agent stops at a suite-green boundary and updates this file before it ends: what was done, the commands and their output, what is open. A complete handoff beats more rows.
- Owner rules for every stage (D137):
  - Improve UX, DX and AX together without breaking anything: no covenant change, `cargo test` green, native transactions byte-identical (W1).
  - No taglines, helper text or secondary labels in the UI; if a screen needs them, fix the layout.
- The owner is not a developer. Bring every decision with what it means for trust, money and work, in plain words.
- Safety: testnet-10 and the private simnet only; no `kaspa:` addresses, no keys in the repo (CLAUDE.md).


## Stage log

| Stage | Status | Notes |
|---|---|---|
| F0 wallet experiment | done (2026-10-26; `d21e284`, `eedf3c5`) | KasWare `signPskt` with the explicit P2PK index list signed only that input for a `buy()` and an 18-input `close()`; both passed the post-sign check and were accepted on testnet-10. Without a list KasWare overwrote covenant sigscripts (refused by the check). `pushPskt` is not exposed. So the page always passes `signInputs` and submits itself. Kaspire untested. Records: `docs/tn10-mass-check.md` "Front end F0", DECISIONS 2026-10-26. |
| F1 builder refactor for wasm | done (parts A–C, review, re-review; `cc7d68e`, `fe7d2fb`, `b3b3520`, `f7f8102`, `6084821`) | Templates embedded (W2), compiler behind a feature, one rusty-kaspa version; `lottery::web` (decoders, verify, 8 unsigned builders, the signing split, the post-sign check, typed errors, W3 property tests); `crates/web` wasm-bindgen API with a generated `.d.ts`, one Safe JSON serialiser; W1 (wasm = native digests), W4 (mainnet build has no quick wallet and refuses to load). Review: every page figure out of Rust and bound to its transaction (`built_mismatch`); re-review: the anchor block's header from a public node binds X before the draw (`anchor_invalid`), `winningNumbers` final/provisional; one signer per build. |
| F2 indexer API | done; reviewed with F3 (`fda74be`, fixes `8647382`) | `crates/indexer-api`: read-only HTTP JSON server over the M8 store, 11 GET endpoints under `/v1/`, typed errors, caps, deadlines; the page's checks `web::indexer` (`parseIndexer*`, `checkIndexer*`; any disagreement `chain_mismatch`); TEST-ONLY doctored modes in a separate binary. Serves `simnet dev` and, read-only, the M8 testnet-10 store. |
| F3 web shell and Play | done (parts A, B; review fixed; `add7983`, `2319d11`, `8647382`) | `web/` (React 19, Vite 7, Tailwind 4, Vitest, Playwright + axe), tokens and themes, the wasm in a worker, the node client in wasm, `npm run dev:simnet`; wallet adapters (KasWare with the index list, Kaspire untested, the quick wallet), post-sign attack tests through the real path, S1 Play, S7, `webstore/1`. Review fixes: open-round lists all-or-nothing, rebuild signed only as shown, quick key by compare-and-swap, lost submit answers settled from the chain, status from chain objects only, CSP as an HTTP header (`vercel.json`). Codes 35–49. |
| F4 My tickets, claims, backup | done (2026-10-26, `d3b24b0`; reviewed by `covenant-reviewer`, every must-fix and should-fix fixed, see "F4 review and fixes") | S2 My tickets, S8 Backup, claims and deposit returns from the local copy without the API, `kaspalionare-backup/1`, `webstore/2`, codes 50–53; done criterion met on simnet. **Review fixes (this stage):** Return deposit offered only when nothing unclaimed can still be claimed (M1); a drawn copy checked for its pools, "expired" only on chain evidence, the marker kept so a copy lists every pool of the draw, never fewer pools (M2); void only where chain and copy do not contradict it; J3 by payout only for this wallet; "With last claim" only when the last claim will be FINAL; a closed copy with its marker spent is incomplete; claims accepted on their own outputs; pending claims kept and settled; limits consistent; stale purchases not resent by themselves; F3 copies migrated; `onversionchange`. |
| F5 Rounds, verify, run the round | done (2026-10-26, part A; not committed; review pending) | S3 list and round detail (facts from Rust with their sources, "Unverified" for the indexer's), Verify in this browser item by item (`verifyRound`), X.hash after the draw bound to a public node's block, S4 close/draw/void from the panel (`callerFigures`, code 54), the SN5 dev self-check fixed, `POST /mine` (TEST-ONLY); e2e: S3, draw, close, doctored API, void. See "F5" below. |
| F6 Vault, How it works, beta instance, deploy, security review | not started | Read "What F6 needs" below. |

## What F6 needs (Vault, How it works, beta, deploy, security review)

- **From F5: the draw's anchor proof on testnet-10.** The page draws with the indexer's proof (checked in the browser first). The standalone API serves a proof for a closing round only when its process follows a node (`AnchorProvider`, as `simnet dev`'s `NodeAnchor` in `crates/simnet/src/dev.rs`). The beta's API must run that way (the M8 indexer in `tn10` already follows a node; embed `indexer_api::run` with a node-backed provider), else S4 shows "Anchor proof: Unavailable" and only the void is possible after D + draw_window.
- **From F5: expired pools are never absorbed by the page's draw()** (`absorbed: []`). Their value and R stay locked until a draw absorbs them (design 7.3 "any prize pools past their deadline"). Add the live pools of earlier rounds past their claim deadline (from the indexer, each checked on the node; at most `A_MAX`) to `prepareCaller`, with a test that the reserve refunds go to their funders.

- **Beta instance:** never launched through `tn10/src/cases.rs` or `tn10/src/wallet_probe.rs` (their admin hash is `admin_hash(ADMIN_SK)`, a key in the repo: anyone could call `set_params()`); use the campaign path or another that sets a fresh admin key, and check the bundle's admin hash before announcing it (F1 re-review N5).
- **W4 on the deployed artifact** (the file Vercel serves, after wasm-bindgen and any optimiser). W1's signing half covers the 14 shapes the quick wallet can sign; the 4 ECDSA and P2SH owner withdrawals are signed in wasm by no test (the page never creates such positions).
- **Hosting the indexer API:** one binary plus the indexer's store directory (on the same machine or a synced copy); plain HTTP/1.1 behind a TLS proxy (`--bind 0.0.0.0:<port>`, `--allow-origin <site>`); only global limits exist (64 requests at once, 256 connections, 10 s deadline), so the proxy adds per-client rate limits; in embedded mode cache the anchor proof per round and rate-limit that endpoint; the follower re-reads `index.json` (18 MB for M8) every 10 s, fine now. The security review looks at the follower (untrusted log lines) and the caps.
- **Page security settings:** narrow the CSP `connect-src` (`https: wss:`) to the indexer and the resolver's node hosts; `vercel.json` has only `headers` (root `web/`, build command, output `dist` are F6's); re-run `npm run check:csp` and check the live headers after the first deploy; `wss://` through the resolver from an `https` page is untried in a real tab; give the node client a hard connect deadline so a refused connection ends in `node_unavailable` (a built page already refuses `ws://` node URLs).
- **Testnet-10 journeys (spec 8):** KasWare claims and buys in a real browser (the same adapters and `assemble`); a `return_deposit()` built by the page for an old M8-instance ticket already past its window (owner, DECISIONS 2026-10-26); Kaspire if installed; the faucet link `https://faucet-tn10.kaspanet.io` checked by hand.
- **Not measured yet (spec 8 quality):** the time to build a buy and a claim on a mid-range phone (desktop Chromium: under 0.31 s ticket to cost panel; the whole F4 e2e test with 4 claims about 55 s); bundle 2.27 MB brotli with the node client (opt-level "s" would save about 10%, and changes the artifact W4 checks).
- **The copy pass (spec 8):** every visible string a figure, name, status, error or control label. F4 review changed "Not on chain" → "Spent", "Open: not kept" → "Not kept", "Nothing on chain" → "Gone", "Imported: N tickets" → row "Tickets kept" N, "No tickets" → row "Tickets" 0, "No rounds" → row "Rounds" 0; new: "Expired", "No pool", "Unknown", "Incomplete", "Payout", "Send". Earlier candidates fixed in F2/F3 ("Cost", "Back", "Round", "Details").
- **A claim refused after a conflict, pressed again at once, can be refused again** (F4 review e2e): the re-press was built from the node's UTXO index before it caught up with the other claim (refused as an orphan: an input already spent on chain); a later press went through. A claim does not rebuild by itself after a conflict (a purchase does, F2/F3 M2); consider the same rebuild-and-confirm for claims, with a short wait for the index.
- **The raw node answer per address** is bounded only by the wRPC message limit (v2.1.0 has no UTXO query by outpoint); if dust at a covenant address ever hurts a phone, the fix is node- or indexer-side.
- **Spec 6 S6 How it works:** the draw cabinet's three conditions (real numbers from Rust, rebuildable from a clean clone, embedded in isolation with its own CSP); it must state the open D100 release-gate item.

## Open items (all stages)

- Kaspire: untested with a real extension; WalletConnect on Android not built. KasWare does not reconnect by itself after a reload.
- J3 without the API (a wallet's tickets with no local copy and no indexer) is impossible by design (design 9 "Residual dependency"; spec 12 "Pruning").
- `return_deposit()` end to end is not reachable on the simnet (its earliest lock time is a pool's claim deadline, at least 6.9 million DAA after the anchor, or T + D_RET; genesis floors): covered by Rust and Vitest tests; the real-chain check is F6's (owner decision above).
- F1 notes still true: the drawer input's units are bound by its budget only; `toWallet` uses the inputs' scripts (a change address not a signed coin's shows as not coming back; every page flow uses the wallet's own address); `close()`'s caller output below `MIN_CALLER_OUTPUT` stays `policy`; fixture-constructor key bytes in the release wasm are checked absent by W4 and `check-bundle`.
- On Windows the dev run's children outlive a killed parent: stop `simnet.exe` and Vite's `node.exe` by their ports (8790, 8791, 18610, 5173) after a background run.
- A round of a dev run closes about 10 minutes (DAA at 10/s) after the run starts; the e2e suite needs a fresh dev run if started later.
- **Owner questions answered so far** (all in DECISIONS 2026-10-26): simnet addresses (a `simnet` feature, release refuses it); change dust (refused unless exactly zero); provisional numbers before the draw (shown, labelled); dark theme first; the faucet link; the deposit row's date; the full/light process split; `return_deposit()` checked by hand on testnet-10; plain backups. The npm accident in the home folder (F1 part C, `npm install` run outside `web/` removed 491 packages from `C:\Users\endri\node_modules`; records in that session's scratchpad) was raised for the owner in F1 part C; the npm safety rule in `web/CLAUDE.md` prevents a repeat.
- **Owner questions open:** see "F4 review and fixes", "Owner decisions".

## Reference for later stages

### Wallet facts (design session and F0)

- **KasWare:** `window.kasware.signPskt({txJsonString, options: {signInputs: [{index, sighashType}]}})`, `switchNetwork("kaspa_testnet_10")` (disabled in its mobile app). Only the explicit index list is safe (F0). Sources: docs.kasware.xyz.
- **Kaspire:** `window.kaspire.signPskt` with `signInputs`; WalletConnect `kaspa_signPskt` on Android; supports `testnet-10`; may refuse "unknown templates and generic covenant requests" (unverified for `signPskt`). The page checks its network before each signature, as for KasWare.
- **Not usable:** Kastle (`signTx` leaves covenant inputs empty, forbole/kastle#353), Kaspium, Kaspa NG, Tangem, OKX (no documented dApp signing). No cross-wallet standard yet (KCC-12 unmerged).
- **KasWare setup for the F6 journeys** (F0 part B, owner steps): install KasWare Wallet from the Chrome Web Store (publisher kasware.xyz), create a wallet (keep the phrase private even for testnet), choose **Kaspa Testnet 10**, copy the `kaspatest:` address; approve only the signing windows the page opens. For Kaspire: the extension from kaspire.kaslab.space, network **TN10**.

### Error codes

One table: `lottery::web::Code` (`crates/lottery/src/web/mod.rs`), loaded by the page at boot (`errorTable`); `f1_web::error_codes_are_stable` pins all 54 (numbers never change; new codes are appended). F1: 1–34; F3: 35–44; F2/F3 review: 45–49; F4: 50–53 (`backup_invalid`, `backup_mismatch`, `copy_mismatch`, `fee_above_prize`); F5: 54 (`fee_above_reward`, a warning).

### Endpoints (v1), with real responses from the simnet instance
The run that was self-checked: covenant ID `5719df78…0db5`, store `crates/target/simnet-dev/5719df78…0db5/`.

| Endpoint | Returns (type in `web::indexer`) | Simnet example |
|---|---|---|
| `GET /v1/status` | `ApiStatus` | `{"api":"v1","network":"simnet","covenantId":"84e27426…","logLines":"14","lastChainBlock":"0faca088…","lastChainDaaScore":"8369","currentRound":"3","gaps":[],"error":null,…}`, from the first run of the same scenario; its DAA went 8,369 → 8,718 in 80 s as the indexer followed |
| `GET /v1/instance` | the bundle file, byte for byte (`bundle/1`) | 332,069 B; `checkIndexerInstance`: ok, k 16, B_MAX 4, bundle hash `4146e95b…ff7f` |
| `GET /v1/rounds[?before=r]` | `ApiRounds` (newest first, 200 per page) | `{"round":"3","status":"open","m":2,"k":16,"buys":3,"combinations":4,"sales":"400000000","jackpot":"0",…}`, then round 2 `closing` and round 1 `drawn` (m 3, 4 buys, 7 combinations) |
| `GET /v1/rounds/{r}` | `ApiRound` | round 1 (7,254 B): `{"round":"1","status":"drawn","m":3,"k":16,"deadline":"1662","anchorDaaScore":"2269","draw":{"void":false,"winning":{"main":[1,3,4,5,6],"bonus":2},"anchorHash":"644a2cd6…","claimDeadline":"6914269","pools":[{"tier":2,"slice":"224000","value":"197000000",…},…]},"objects":[drawer, pool, pool, pool],…}`; round 2: `closing`, D 8,269, objects drawer, vault, controller, marker; round 3: `open`, T 13,662, 18 objects (controller, params, 16 shards) |
| `GET /v1/rounds/{r}/purchases` | `ApiPurchases` | round 3: `{"round":"3","m":2,"k":16,"shards":["","","","","0e1012191d01070a0c0e1e02","",…,"050b0d191e0102090f1d1e01",…]}` |
| `GET /v1/rounds/{r}/shards/{i}` | `ApiShard` | `{"round":"3","shardIndex":4,"m":2,"count":2,"purchases":"0e1012191d01070a0c0e1e02"}` |
| `GET /v1/rounds/{r}/anchor-proof` | `ApiAnchorProof` (the fallback, spec 6.1) | round 1: 272 bytes, `"source":"draw"`; round 2 (closing): 272 bytes, `"source":"node"`, built by the dev run's node; round 3: `404 anchor_unavailable` ("round 3 is not closed") |
| `GET /v1/tickets?payout=` | `ApiTickets` (live tickets and purchase history) | 10 live tickets of the dev wallet, 10 purchases |
| `GET /v1/vault` | `ApiObjects` (the vault, the newest controller, the drawer, the live records) | 3 objects |
| `GET /v1/positions?owner=`, `GET /v1/requests?owner=` | `ApiObjects` (requests come with the live records of their rounds) | 1 position (the genesis position), 0 requests |
| errors | `ApiErrorBody` | `/v1/rounds/9`: `404 {"error":{"code":"round_unknown","message":"The index has no such round.","detail":"round 9"}}`; `POST /v1/status`: `405 method_not_allowed`; `/v1/nope`: `404 not_found`; a `kaspatest:` address on the simnet server: `400 address_network` |

The self-check of that run (the page's checks against the node):
```
SELF-CHECK instance: ok (k 16, B_MAX 4, bundle hash 4146e95b292f22d43bb02f5c7e7f7205ea7a5c230d61fed50780c13a87a8ff7f)
SELF-CHECK round 3: ok, Open (from the chain objects: true), 18 objects against the node, roots 16, numbers -
SELF-CHECK round 2: ok, Closing (from the chain objects: true), 4 objects against the node, roots 16, numbers [5, 6, 12, 20, 30]+2 Provisional (D 8269)
SELF-CHECK round 1: ok, Drawn (from the chain objects: true), 4 objects against the node, roots 16, numbers [1, 3, 4, 5, 6]+2 Final (D 2269)
SELF-CHECK tickets: ok, 10 live tickets of kaspasim:qpsneutp…phyfvu against the node (10 purchases)
SELF-CHECK /v1/vault: ok, 3 objects against the node
SELF-CHECK /v1/positions?owner=kaspasim:qpsneutp…phyfvu: ok, 1 objects against the node
SELF-CHECK /v1/requests?owner=kaspasim:qpsneutp…phyfvu: ok, 0 objects against the node
dev: ready in 9 s: {"api":"http://127.0.0.1:8790/v1/","covenantId":"5719df78…0db5","network":"simnet","rounds":{"1":"drawn","2":"closing","3":"open"},…,"wrpcBorsh":"ws://127.0.0.1:56074"}
```
The doctored binary on the same store (`--doctor missing_purchase --bind 127.0.0.1:8791`) answered with `x-indexer-doctored: missing_purchase` and served round 3 shard 4 as `0e1012191d01`, one of its two purchases dropped. The honest server serves `0e1012191d01070a0c0e1e02`.

**Testnet-10 data (read-only).** The release production binary ran on the M8 store `tn10-data/e9f4f810…88d6/` (a 36.8 MB log). The files' sizes and times were unchanged afterwards.
- It was up in 2 s: `412 log lines, 9 rounds, 383 live objects`; status `lastChainDaaScore 605601500`, no gaps, no error.
- Rounds 1–8 are drawn and round 9 is open. Their buy counts (76, 77, 0, 36, 40, 40, 0, 75, 0) are the same as in the indexer's own `index.json`.
- `/v1/rounds/8`: drawn with `[2, 6, 8, 13, 17]+2` and three live pools. Round 6's purchases: 160 combinations. Round 8's anchor proof is served from its draw.


### Anchor proof from a browser (spec 6.1): the wRPC finding
- **It compiles.** `kaspa-wrpc-client` v2.1.0 compiles into this wasm (`--features wrpc-probe`) once ring gets its browser random source: `ring`'s feature `wasm32_unknown_unknown_js`, as rusty-kaspa's own `rpc/wrpc/wasm/Cargo.toml:43` sets it. Without it, rustls and ring fail to compile for wasm32.
- **It runs.** Under Node, which offers the same WebSocket and fetch APIs as a browser, `wrpcProbe()`:
  1. connected through the public resolver to `wss://muon-10.kaspa.blue/kaspa/testnet-10/wrpc/borsh`;
  2. read the sink;
  3. called `GetSeqCommitLaneProof(sink, COINBASE_LANE_KEY)`;
  4. recomputed the block's KIP-21 commitment from the proof (lanes root from the SMT proof, then the activity root, the state root and the commitment).

  The result equals the header's `accepted_id_merkle_root`: `{"block":"b78912f0…4c39","commitmentMatches":true,"daaScore":"605799412","hasLane":true,"network":"testnet-10","smtProofBytes":130}`. It took 21.4 s, most of it the resolver and the connection.
- **Not tried:** a real browser tab. Open there: the CORS rules of the resolver's HTTP endpoint, and `wss` from an `https` page. kaspa-wasm itself runs in browsers through the same resolver, so this is expected to work; F3's first browser run settles it.
- **Cost:** +1.86 MB raw, +578 KB gzip, +372 KB brotli. After wasm-bindgen the build is 7,284,023 B, against 5,423,397 B without the client.
- **Still missing** before the browser can build the 272-byte proof itself: the miner payload root. It needs the coinbase payload of every mergeset block (one `get_block` each), which is what `tn10/src/anchor.rs` assembles. That code is generic over `RpcApi`; moving it to a shared crate (or to `lottery`) is F2/F5 work.
- So the spec 6.1 fallback is not forced: both paths stay open (proposed row (i)).


### The local store (`webstore/2`), claims and backups, as they stand after the F4 review

- Database `kaspalionare`, version 2, stores `meta`, `tickets`, `rounds`, `pending`; records declared in `crates/web/src/webstore.rs` (tsify; fields only ever added with a default). `webstore/1` (F3) migrates in place; F3 copies at the closed or drawn stage (no marker or result) are marked open by the migration and copied again.
- `StoredTicket`: the live ticket (after PAY/ACCRUE its successor), `ended` after FINAL or a return, and (F4 review) `pending`: a signed claim or return whose outcome is unknown, with its successor (`successor`, `successorValue`) and payout output (`paid`); settled on the next visit; never in a backup.
- `StoredRound`: the k lists, roots, stage; `against` (closed: the marker; drawn: a pool); `result` (numbers, claim deadline, pools: only ever added to, an absorbed pool stays listed); and (F4 review) `marker`, the closed copy's marker kept into the drawn copy: with it the result lists every pool `draw()` created (`backup::round_pools`, `game::pool_set`).
- A copy is kept only through `mergeCopy` (Rust): never a stage back, never fewer pools; frozen lists, roots, numbers or deadline that differ are `copy_mismatch`.
- `checkCopy` (no API): closed with a live marker → verified, marker spent → incomplete; drawn: each listed pool live, absorbed (not live after its claim deadline) or unknown; `complete` when the marker is kept and no listed pool is unknown. `ticketCard` takes `absorbed` and `complete`: a win is `claimable`, `expired` (listed and absorbed), `noPool` (the complete set has none) or `unknown` (re-copied from the API; never shown expired). Return deposit only when the result is known and nothing unclaimed is claimable or unknown; "With last claim" only when every unclaimed index is claimable.
- Claims and returns: built from the ticket's node entry, the live pools found by script (`nodeScriptEntries`) and the copy's lists; figures from `claimFigures`/`returnFigures` bound to the transaction; sent through `signAndSubmit`; accepted when any of the transaction's own outputs is live (the successor, the payout, the change). The signed transaction is kept before sending (`pending`); sent again by itself only within `RESEND_MS` (10 minutes) of its signature, purchases included; an older purchase waits for a press (My tickets, "Pending purchase").
- Backup `kaspalionare-backup/1`: compact JSON with a checksum, at most `BACKUP_MAX` (8 MiB) and within the worker's 16 MiB argument cap once escaped (the writer checks both); a record that fails its own check is left out and named (`skipped`), never fatal; read in Rust (`backup_invalid`), then every record checked against the chain (`backup_mismatch` refuses the whole file); ended tickets are not looked up; node reads in batches of `indexerLimits().nodeObjects` (2,000).
- J3 (`/v1/tickets?payout=`): only tickets whose payout (from the state, read by the wasm) is the connected wallet's; a stored ticket paying elsewhere shows a Payout row on its card and its claim row.

## Proposed DECISIONS rows not yet recorded (for the orchestrator)

The stage agents' build decisions, kept verbatim until recorded. The owner decisions are already rows (DECISIONS 2026-10-26).

### F1 part A
- (a) **The templates are embedded** as `silverc` JSON in `crates/lottery/templates/` and used by `Templates::get` in every build, native included. `regen-templates` writes them, and W2 (`f1_templates`) fails on any difference from a fresh compile of `covenants/`. A `.sil` change now needs one regenerate command; CLAUDE.md says so.
- (b) **`compiler` feature** (default on) in `harness` and `lottery`; the web build leaves it off. The TEST-ONLY short-void variant now also needs `compiler`. The SP12 probe's positive control enables `compiler`; the mainnet absence check is unchanged.
- (c) **The ABI on v2.1.0 by a vendored copy plus `[patch]`** (3 marked edits), not a fork or a re-implementation. It applies to native builds too, so there is one ABI everywhere.
  - Proof of no encoding change: the golden digests (written before the change, equal after), W2 after the patch, the upstream ABI tests 21/21, and the source check.
  - The vendored copy must be updated by hand on any SilverScript bump. This adds to the existing standing task in DECISIONS row 63.
- (d) **Finding for DECISIONS row 63 / research N2:** SilverScript's rusty-kaspa rev `a41a333` is an ancestor of v2.1.0, 13 commits before it. `crypto/txscript` still differs (19 files, including engine changes such as the removed minimal-push checks). The compiler and sil-debug still run on `a41a333`; only bytecode crosses, as D-row 18 says.
- (e) **The wasm toolchain:**
  - getrandom 0.3 gets `wasm_js` from a lottery target dependency and its cfg from `crates/.cargo/config.toml`;
  - clang and ar for `secp256k1-sys` come from the environment variables `CC_wasm32_unknown_unknown` and `AR_wasm32_unknown_unknown` (LLVM at `C:\Program Files\LLVM\bin`), documented in CLAUDE.md.
- (f) **`crates/web` exists as a cdylib** with only a link-probe export. Part B replaces it with the wasm-bindgen API.
- (g) **`lottery::digest` is a public library module** (the fixture digests), so part B's W1 can run the same function in wasm and compare it with `tests/fixtures/f1_digests.txt`.


### F1 part B
- (a) **The signing split covers every case.** `Built::finish_unsigned` lists the funding and owner inputs (D67) to sign; F0's owner refusal is removed. An owner input signed outside the library is `InKind::OwnerPub` (its public key only). `finish()` refuses it.
- (b) **The owner-input budget without the key** is `owner_signed_units` of the class, measured by signing a throwaway transaction with a stand-in key of that class. It is proven equal to `finish()`'s signed re-run for 72 random keys. All three classes commit budget 10, one signature check: 100,001 / 100,001 / 100,102 units.
- (c) **The post-sign check moves to `lottery::web::sign`**; tn10 delegates.
  - It is generalized to owner inputs: the exact P2SH redeem push is required.
  - `assemble` also requires the signed masses to equal the built ones and runs the engine on every listed input against our entries.
  - F0's row (d) stands: storage-mass and entry differences are notes.
- (d) **`lottery::web` is the page-facing layer**, with typed errors: 26 stable codes, each with a number, a name and one message, plus a detail string for logs. The detail is never page copy.
- (e) **Web builders: guard, native build, bind.**
  - The guard mirrors each covenant's own state ranges and the reference's preconditions; a refusal is a typed error.
  - The build is the native builder on a placeholder key.
  - The bind puts in the wallet's coins and change; a transaction with a placeholder script left is refused.
  - Every covenant input is run in the engine before the transaction goes to a wallet.
  - Proven byte-identical to the native builder for every entry point (`f1_web`).
- (f) **The draw's dry run in the web build** uses a one-block `ChainSim` built from the proof's X (`sim_from_proof`; P's hash is a placeholder because the proof omits it); a void uses a trivial sim. The bytes equal the native build's.
- (g) **The pinned storage-mass calculator's panics are refused up front:**
  - `harness::try_masses` refuses a zero-value output and an input total above u64;
  - `web::sign::unsigned` refuses a zero-value input.
- (h) **Draw guard: G2 × (m − 1) must fit an i64.** This is conservative: it refuses only G2 above about 1.9 × 10^17 sompi (about 1.9 billion KAS). Otherwise `game::pool_set`'s checked product would panic where the drawer's checked multiplication rejects.
- (i) **`.gitattributes`: `-text`** for the embedded templates and the golden digests.
- (j) **`Template::try_from_artifact` requires exactly one contract.** All templates have one; `harness` already asserted it elsewhere.
- (k) **The indexer (`index::Rebuild`, `RoundResult::from_tx`) stays native-only.** Its `index::model` catch_unwind guards are not wasm-safe. If it ever runs in a browser, they must become `Result` paths first.


### F1 part C
- (a) **The page builds from chain objects** (`lottery::web::cases`).
  - The page passes the covenant UTXOs with their state bytes, the purchases and the user's choices.
  - Rust derives every case field, and requires the built inputs to be exactly the given UTXOs.
  - No `*Case` crosses the boundary.
- (b) **The boundary:**
  - every call returns `{ok} | {error}`, and never throws;
  - every 64-bit integer is a decimal string and bytes are hex (`Dec`, `Hex`, `Address` in the `.d.ts`);
  - unknown input fields are refused;
  - types come from tsify 0.5.5 (declarations only), and values cross through serde-wasm-bindgen;
  - `BuiltTx` is plain data, and `assemble` checks it again.
- (c) **One error table** (a macro), with the appended codes 27 `address_network`, 28 `input_format`, 29 `not_started` and 30 `mainnet_refused`. The TypeScript `ErrorName` union is generated from it.
- (d) **Networks:** the beta build starts on testnet-10 only and refuses every other address prefix. The `simnet` feature (dev only) adds the private simnet, with `kaspasim:` addresses (open question 1).
- (e) **The mainnet gate (W4):** a `--features mainnet` build refuses at instantiation and at `start`, and compiles out the quick wallet and its types. W4 checks this with a byte scan, a positive control and a Node load.
- (f) **The quick wallet's key hook:** a secp256k1 key from getrandom (`wasm_js`), signing with rusty-kaspa's `sign_input`.
  - It signs only the listed inputs it owns, and its output passes the same `assemble` as any wallet's.
  - Spec 6.2 names the kaspa-wasm `PrivateKey`. This uses the same secp256k1 key type and rusty-kaspa's own signer from Rust, so the page loads no second wasm package.
- (g) **One Safe JSON serialiser**, in `web::safe_json`; tn10 calls it.
- (h) **The referral and the reserve funders:** the referral is paid only if a referrer is named and r ≥ M_REF, decided in Rust. The caller's change address funds the reserves in `close()` and `draw()`.
- (i) **The anchor proof:** the rusty-kaspa wRPC client runs in wasm (with `ring/wasm32_unknown_unknown_js`) and fetches `GetSeqCommitLaneProof` from testnet-10.
  - The browser can build the proof itself once `tn10/src/anchor.rs` is shared (F2/F5), at +372 KB brotli.
  - The indexer fallback stays available.
- (j) **The templates stay pretty JSON:** compact JSON would save only 9.6 KB brotli.
- (k) **The shipped wasm** is opt-level "s", then wasm-bindgen, then `wasm-opt -Oz`: 1.91 MB gzip, 1.69 MB brotli.


### F1 review
- (a) **The page passes no derived game value.** `checkRoots` takes `{covenantId, against, purchases}` and `winningNumbers` `{covenantId, from: {closed: {marker, controller}} | {drawn: {pool, drawer}}, anchorProof}`. Rust derives the round, m and D: D = marker creation DAA + seed_offset (closed), or the pool's claim deadline − draw_window − claim_window (drawn, drawer check 19); on the drawn path the numbers must equal the pool's `win`.
- (b) **A built transaction's figures are bound to it.** `assemble` and `quickWalletSign` recompute every `BuiltTx` field from its transaction and refuse a difference (31 `built_mismatch`). The summary is a function of the transaction alone: `walletIn` = the inputs the wallet signs; `toWallet` = a plain output paying the script of one of them. The drawer input's units are bound by its committed budget only (its seq-commit opcode needs the node's chain).
- (c) **Vault deposits from the page are Schnorr P2PK owners only** (32 `owner_type`). D67 is unchanged (any P2PK or P2SH owner at the covenant level); the page creates only positions its signers (KasWare, the quick wallet: Schnorr P2PK) can withdraw. Withdrawals still accept all three owner classes, for positions made by other tools.
- (d) **33 `change_dust`:** the fee loop never drops change; when the coins cover the transaction without change but the change cannot pay for its own output, the refusal says so. Money behaviour unchanged (see the owner decision).
- (e) **The `fixtures` feature** (lottery, default on; web, dev only): the fixture calls, their wallet key and the golden-digest lines are out of the release wasm; W4 checks the release artifact, built by the documented wasm build command, for them and for the simnet.
- (f) **Golden digests:** 97 lines; lines 78-97 are the F1 review's 20 shapes, appended with the reviewer's digest function and its pre-F1 evidence, never regenerated. `w1_api.txt`: the 6 withdrawal lines rewritten for the `toWallet` definition (transactions unchanged). New `w1_verify.txt` (290 decode and verify results, native = wasm).
- (g) **W1 is one ordered wasm test** (`w1_wasm_equals_native`: API, library, verify, sign).


### F1 re-review
- (h) **Winning numbers say whether they are final.** `winningNumbers` returns `status`: `final` on the drawn path (the pool's `win`, written by `draw()` after its on-chain anchor check), `provisional` on the closed path. The page never shows provisional numbers as verified.
- (i) **Before the draw, X is bound to the node's anchor block.** The closed path takes `anchorBlock: {header, isChainBlock}` from a public node's `getBlock(X.hash)`: the header must hash to X.hash, commit to X's fields (`acceptedIdMerkleRoot` = SeqCommit) with the same DAA and blue score, and the node must report X on its selected chain (`anchor_invalid` otherwise). The page fetches it from a node by X.hash, never from the indexer. No proof-of-work check (the header comes from the node).
- (j) **One payer per page build:** code 34 `mixed_signers`; every input to sign is of one key.
- (k) **Liveness is the page's:** no call checks that a UTXO entry is unspent; the page reads every entry it passes to the wasm from a public node by outpoint, never from the indexer.
- (l) **W1 signs 14 of 18 shapes in wasm** (all the quick wallet can sign); `w1_verify.txt` is 291 lines and its drawn fixture's X hash is a header's.
- (m) **tn10 `anchor_from_rpc` self-check (5):** the node's header passes `BlockFields::check_header` before anything is built from a proof.


### F2
- (a) **The indexer API follows the store, not the chain.**
  - `crates/indexer-api` rebuilds `index/1` from `index-log.jsonl` and the verified `bundle/1`, and follows the log.
  - It has no RPC client, no keys and no writes.
  - A process that runs an indexer embeds it (`indexer_api::run`) and may give it an anchor-proof source.
- (b) **Stack:** axum 0.8.9 (no default features) on hyper 1, hyper-util and tokio, with tower-http for CORS; all were already in `Cargo.lock`.
- (c) **API v1:**
  - 11 GET endpoints: spec 6.3's table, plus `/v1/status` and `/v1/rounds/{r}/purchases` (all shards in one response, the form the builders take);
  - `/v1/instance` serves the bundle file byte for byte;
  - typed errors (`ApiErrorBody`, 11 codes);
  - GET and HEAD only; CORS for any origin by default.
- (d) **One type source for the API:** the response types live in `web::indexer` (tsify). The server serializes them and the page parses them. Per-kind size caps (`LIMITS`) are checked on both sides, by the page before parsing.
- (e) **One refusal for the API's copy:** `checkIndexerInstance`, `checkIndexerObjects` and `checkIndexerRound` report every disagreement with the chain as `chain_mismatch`, with the underlying code in the detail. They reuse `cases::live`, `check_roots` and `winning_numbers`.
- (f) **The launch bundle hash** (spec 6.5) is the blake2b-256 of the bundle file's bytes.
- (g) **Doctored modes are TEST-ONLY:** the feature `doctored`, a separate loopback-only binary that marks its responses, no flag in the production binary, and a byte scan of the production build.
- (h) **`simnet dev` is the dev instance:**
  - the production templates, with round_len = d_min = 6,000 (600 is too short: the anchor wait after a close passes the next deadline);
  - a ground win in round 1, and the three statuses at once;
  - the indexer and the API in one process;
  - a self-check of every endpoint against the node.
- (i) **The anchor-proof fallback:**
  - a drawn round's proof is served from its draw transaction;
  - a closed round that is not drawn yet gets a proof from a node, only when the serving process follows one;
  - otherwise the answer is `anchor_unavailable`.


### F3 part A
- (a) **The page's network layer is the rusty-kaspa v2.1.0 wRPC client compiled into `crates/web`** (feature `node`, Borsh), not the kaspa-wasm npm package: one rusty-kaspa version, generated types, the anchor-proof RPC reachable. Every entry the page passes to the lottery code comes from `nodeEntries`/`nodeCoins` (node, by outpoint), every X header from `nodeAnchorBlock`.
- (b) **workflow-core's window timer overrides are switched off in the worker** (`disable_persistent_timer_overrides`): a worker has no `window`; its timers are not throttled like a background tab's.
- (c) **Five error codes appended** (35–39): node unavailable, node on another network, node refused, indexer unavailable, worker stopped; the page loads the one table at boot (`errorTable`).
- (d) **The worker is replaced after any crash**, the session boot replayed, pending calls rejected with `worker_stopped`; at most 3 deaths a minute. Arguments to the worker are capped at 16 MiB; indexer bodies at the kinds' caps before parsing.
- (e) **`simnet dev` for the page:** node Borsh wRPC on 127.0.0.1:18610, the dev faucet (simnet binary only, 100 KAS a request, kaspasim: only, loopback), `current.json` written after the self-check with `startedMs`; `npm run dev:simnet` is the one command.
- (f) **Stack versions** as listed (exact pins, newest per known-compatible major at least two weeks old); shadcn/ui components written by hand over Radix (no CLI).
- (g) **Tokens:** spec 5's table plus `on-teal`, `on-mint`, `status-void-fg`, `focus`; contrast pinned by a unit test.
- (h) **W4 checks the shipped builds** (`node` in release, dev and mainnet); the node client is compiled out of a `mainnet` build.
- (i) **The dev catalogue and faucet stay out of production**, checked by `check-bundle.mjs` (with a negative control).


### F3 part B
- (a) **Wallet adapters** (spec 6.2): one interface; coins always from the node; KasWare with the explicit `signInputs` list only, never `pushPskt`, `kaspa_testnet_10` checked at connect and before each signature; Kaspire built to its docs and marked untested; one wallet request at a time; every wallet address checked by `scriptOf`; extension wallets refused on the simnet.
- (b) **The post-sign check on the page path:** `signAndSubmit` is the only way to `nodeSubmit` a purchase; it runs `assemble` in the worker first and requires the node's id to be the built one. Spec 8's list is tested through it on the real wasm (Vitest, in-process worker sharing the worker's handler), with fixture inputs generated from crates/web and checked current by a Rust test.
- (c) **Cost panel from Rust** (`buyFigures`): every row of the buy's cost panel, and the ticket's state bytes for the local copy, are computed in Rust and checked against the built transaction (`built_mismatch`). The deposit's return date is T + D_RET (the date from which `return_deposit()` is valid on the ticket alone); with a win, the deposit comes back earlier with the last claim.
- (d) **Change dust (owner, 2026-10-26 (2)) implemented:** exactly-zero leftover builds without change and without extra fee; any other sub-dust leftover is `change_dust`.
- (e) **Shard contention:** a submit refused for a spent input (mempool double spend, disallowed orphan, missing outpoint; texts from rusty-kaspa v2.1.0 `mining/errors/src/mempool.rs`) is code 40 `node_conflict`; the page rebuilds on another live shard up to 3 times and shows it; a stale indexer copy after a conflict is waited out (8 tries, 1.5 s apart).
- (f) **Codes 41-44** (`wallet_missing`, `wallet_rejected`, `wallet_network`, `storage_unavailable`), raised by the page from the one table.
- (g) **`webstore/1`** (spec 6.3): record types in Rust (tsify), per-instance keys, stages open → closed → drawn, the quick wallet's key in its own database and never in the store or its backup; "accepted" means the ticket is in the node's UTXO set and its entry passed `verifyState`.
- (h) **Referral links:** validated by `scriptOf`, kept in `localStorage` (`kl.ref`), replaced only by another valid link, a refused link silently ignored (logged); the dev run's site owner is a fresh simnet address in `current.json`.
- (i) **Theme (owner, 2026-10-26, amends D139):** dark for every first-time visitor; "device" is an opt-in menu choice.
- (j) **A mainnet page check** (`npm run check:mainnet`, `VITE_MAINNET_CHECK`): the page's quick-wallet code, and its call names, are compiled out; the wasm side stays W4's.
- (k) **simnet dev for F3:** s_R = 500 so both referral cases run on one instance; the dev faucet combines coins; DEV-only e2e hooks (`window.__klDev`) for a deterministic shard conflict, refused in `dist/` by `check-bundle.mjs`.


### F2/F3 review
- (a) **A purchase is signed only as shown** (F2/F3 review M2): after a shard conflict the page rebuilds in the same round only; it signs the rebuild by itself only when every figure the player saw is unchanged (round, combinations, payout, price, deposit and date, referral, fee, total); otherwise the new figures wait for a fresh press. A round that stopped selling ends the purchase (`sales_closed`); a purchase never moves to the next round.
- (b) **A lost submit answer is settled from the chain** (S3): the same signed transaction is sent again (it cannot buy twice) and the ticket output is looked for; only a spent input with no ticket of ours permits a rebuild; no answer is `submit_unknown`, and Buy stays disabled for that purchase.
- (c) **The quick wallet's key is written only by compare-and-swap** (S1): a stored key is replaced only through Import → (the stored key is shown) → Replace; the adapter is pinned to the key it connected with and refuses to sign if this browser's key changed (`wallet_changed`).
- (d) **Open-round lists are all-or-nothing** (M1): `checkIndexerRound` returns purchases only when every one of the k lists was compared with a live shard read from the node (k from the live controller), roots keyed by shard index.
- (e) **Status from the chain only** (S4): round r's live controller shows "open"; the page never shows a status no live object shows, and "selling" is live shards of the round before T.
- (f) **A self-referral is the referral row** (S2): paid to the buyer's own address, total = price + deposit + fee; a stored link the builder cannot use falls back to the site owner.
- (g) **Node answers are filtered before any count** (S5): one address per RPC call, exactly the wanted outpoints kept; `nodeCoins` the 256 largest plain coins.
- (h) **The CSP is an HTTP header** (S6) from `web/vercel.json`, with `frame-ancestors 'none'`; `vite preview` serves the same; no `'unsafe-eval'` (the glue's `new Function` is never reached in a worker).
- (i) **Codes 45-49** (`sales_closed`, `purchase_changed`, `submit_unknown`, `wallet_timeout`, `wallet_changed`).
- (j) **The game's shape comes from the wasm** (`gameShape`); **the badge reads `<network> · no value`**; the testnet-10 faucet link defaults to `https://faucet-tn10.kaspanet.io`; the dev faucet answers the dev page's origin only.


### F4
- (a) **Local store `webstore/2`** (spec 6.3): a `pending` store for purchases whose outcome is unknown (settled on the next visit: the ticket output live makes it a ticket; otherwise the same signed transaction is sent again; an input spent without our ticket drops it); a closed copy keeps its marker, a drawn copy the round result (numbers, claim deadline, every live pool's state), so a claim needs only the copy and a node; a copy never moves back a stage; `webstore/1` stores migrate in place.
- (b) **Claim and return figures from Rust** (`ticketCard`, `claimFigures`, `returnFigures`): every figure of S2 is computed in Rust; a built claim's or return's figures are checked against the transaction (`built_mismatch`), so a row shows what is signed. A claim's fee is shown before signing (each claimable row is built as soon as a wallet is connected); "fee above prize" is a warning (code 53), not a refusal.
- (c) **The local copy is re-checked without the API** (`checkCopy` with `nodeScriptEntries`): a pool is found on a node by its script, which its claims never change; a copy whose lists no longer rebuild a live marker's or pool's roots builds nothing (`copy_mismatch`).
- (d) **Backup `kaspalionare-backup/1`**: one JSON file per instance with a checksum (finds damage; it is no seal); read in Rust with strict checks (`backup_invalid`); every record checked against the chain before anything is kept (`backup_mismatch` refuses the whole file); never the quick wallet's key; no pending purchases.
- (e) **Codes 50-53** (`backup_invalid`, `backup_mismatch`, `copy_mismatch`, `fee_above_prize`).
- (f) **The void card state uses the indexer's round status** (display only; no live chain object shows a void; no money depends on it).
- (g) **`simnet dev` `POST /round`** (TEST-ONLY dev tool, simnet binary only): draws the closing round, closes the open one and draws it with its anchor ground for a given jackpot combination (D114's grinder); the e2e suite's deterministic wins. The e2e suite simulates "API stopped" by refusing the API's origin in the browser context.


### F4 review
- (a) **Return deposit only when nothing unclaimed can still be claimed** (F4 review M1): the page offers `return_deposit()` when the chain allows it, the round's result is known (drawn or void) and no unclaimed index is claimable or of unknown result; a claimable index is claimed first (design 7.5: a return forfeits the unclaimed winning indices; D102: path b can precede a late close's claim deadline).
- (b) **A drawn copy shows every pool a win needs** (M2): with the closed copy's marker kept, the copy lists `draw()`'s whole pool set (`game::pool_set` from the marker and the lists, built as `lottery::draw` builds pools); without it (a copy first taken drawn: the marker is spent and rusty-kaspa v2.1.0 returns no spent output or transaction by ID), a pool for every winning tier of the player's unclaimed combinations; otherwise the win is `unknown` and the round is copied again. "Expired" only for a listed pool not live after its claim deadline. A kept copy never moves back a stage and never lists fewer pools (`mergeCopy`). Deviation from the review's "require the full set" for copies first taken drawn, for the reason given.
- (c) **Void as far as the chain supports it** (S1; outcome of F4's owner question 2, needs the owner's yes): "No draw: void" only when the indexer says void, the live drawer is past the round, no marker, pool, open controller or shard of the round is live, and this browser's copy holds neither the result nor a live marker; otherwise the card shows the copy's own stage. Residual: an indexer hiding a drawn round's pools from a browser that never saw the result.
- (d) **J3 keeps only the wallet's own tickets** (S2): a ticket found by payout is kept only when its state's payout is the connected wallet's address; a stored ticket paying elsewhere shows a Payout row on its card and claim row.
- (e) **Signed claims and returns are kept until settled** (S6), like purchases: `StoredTicket.pending` (the signed transaction, the successor, the payout output) is written before sending and settled on the next visit; any signed transaction of this browser is sent again by itself only within 10 minutes of its signature, later only on a press (a stale purchase is listed as "Pending purchase" with Send).
- (f) **A claim or return is accepted on any of its own outputs** (S5): the successor, the payout or the change, never the pool's successor alone (another claimer may spend it at once).
- (g) **Backup and node-read limits** (S7): the backup is compact JSON within 8 MiB and within the worker's 16 MiB argument cap once escaped, checked by the writer; a record failing its own check is left out and named, never fatal; node reads go in batches of 2,000 objects; `indexerLimits()` publishes `nodeObjects`, `backup`, `workerArgs`.
- (h) **`simnet dev` `POST /close`** (TEST-ONLY dev tool, simnet binary only): draws the closing round and closes the open round, so the e2e suite sees a round closing with its marker live.

### F5
- (a) **After the draw, X.hash is shown as checked only when bound to a public node's block.** `winningNumbers`' drawn path takes an optional `anchorBlock` (as the closed path); `checkIndexerRound` reports `anchorBound`. Without it the page shows the indexer's anchor hash marked Unverified. Reason: the pool's numbers alone do not fix X.hash; a 32-byte value giving them was found in 658,782 tries (`f5_rounds`). `WinningOut` is unchanged (W1's verify golden unchanged).
- (b) **The draw's anchor proof is the indexer's (spec 6.1 fallback), checked in the browser before anything is built**: `anchorProofX` reads X, the node's `getBlock(X.hash)` binds it (header hash, commitment, chain block), `checkIndexerRound` checks it anchors D and the purchases rebuild the marker's roots; `draw()` checks the anchor on chain. Size cost 0. Not chosen: assembling the 272-byte proof in the browser (`tn10/src/anchor.rs` shared): the page cannot find X without a chain block below D to start from (v2.1.0 has no chain block by DAA score, and `get_headers` is unimplemented, `rpc/service/src/service.rs:1040`), which only the indexer would supply; it would add dozens of node calls (one `getBlock` per mergeset block) per draw on public nodes. F6: the beta's API must follow a node to serve closing rounds' proofs.
- (c) **Verify in this browser is one Rust call, item by item** (`verifyRound`): each object's state, the status, the indexer's frozen roots, each shard's tree, the winning numbers, the anchor block; each match, mismatch or not checkable, so one lie shows as one mismatch (every doctored mode attributes to its own item, `verify_names_each_lie_as_its_own_item`).
- (d) **The round's figures come from Rust with their sources** (`CheckedRound.facts`: T, S, J, D, the void lock, the claim deadline, the k roots, the live pools and what is left in them, the caller reward, R; each with the outpoints it is read from, the explorer link). A figure no live object fixes (S and J of a drawn round, T after the close) is the indexer's, marked Unverified. A status no live object shows is "Unverified", except a void the chain supports (F4 rule).
- (e) **The caller panel's rows are bound to the built transaction** (`callerFigures`): the reward from the chain objects (C_close from the controller and all k shards; C_draw from the marker, a void too), the fee, the reserves posted (funding + reward - fee - caller output, a whole number of R: 1 for close, the pools for draw, 0 for a void) and when they come back, refunds to the wallet, the total. The kind (draw or void) from the lock time. Code 54 `fee_above_reward` is a warning, not a refusal (design 12).
- (f) **A close, draw or void whose outcome is unknown is not kept as a pending record** (unlike purchases and claims): the press stays disabled while the same transition is due; a later press is safe (a second transaction spending the same objects is refused; a refused transaction costs nothing).
- (g) **`simnet dev` `POST /mine?to=deadline|anchor|void`** (TEST-ONLY dev tool, simnet binary only): mines only, so the page's own panel finds a transition due; the void's blocks go to a throwaway address.
- (h) **SN5's check reads the grind's record consistently**: a first candidate that already meets the target is "no grind needed" (the dev run's any-tier target hits often: 11 tries against an estimate of 1,425 in this stage's run); the SN scenarios still require a real grind.

## F4 review and fixes (summary; full notes in `git show 66a2db7:docs/specs/frontend-handoff.md`)

`covenant-reviewer` on F4 found 2 must-fix and 7 should-fix items; all fixed in `8a264ac` with regression tests (`crates/web/tests/f4_review.rs`, `crates/indexer-api/tests/review_f4.rs`, `web/src/tickets/review.test.ts`) and negative controls. The three owner questions it raised are answered (DECISIONS 2026-10-26, "F4 leftovers kept for the testnet beta").

## F5 (stage agent, 2026-10-26): S3 Rounds, Verify in this browser, S4 Run the round

Done in one run (part A); "What is left" lists the rest. Not committed; no review yet (money paths: the full process applies, see "What the reviewer should attack").

### What was built
- **Rust (`crates/web`).** `rounds.rs` (new): `RoundFacts` inside `checkIndexerRound` (T, previous round drawn, S, J, D, the void lock, the claim deadline, the k frozen roots, the live pools with what is left in them, the caller reward C_close/C_draw, R; each with the outpoints it is read from); `verifyRound` (J6 item by item); `callerFigures` (S4 rows bound to a built close, draw or void); `anchorProofX` (X of a proof, to fetch its block from a node). `indexer.rs`: `classify` shared by `check_round` and `verify_round` (behaviour unchanged), `anchorBound`, the drawn path's optional `anchorBlock`. `api.rs`: `winning_numbers_bound`. Code 54 `fee_above_reward` (`lottery::web`).
- **Page** (`useRoundDetail` clears the panel's last progress when another transition is due: the void e2e's bug). `src/rounds/` (`load.ts`: list rows and round detail; objects read from the node by outpoint, spent ones left out, checked by `checkIndexerRound`; status from the chain, void as F4; `verify.ts`; `caller.ts`: `dueOf`, `prepareCaller`, `sendCaller`), `screens/Rounds.tsx` (`#/rounds`, `#/rounds?r=<n>`), `screens/rounds/` (detail view, hooks), components Round row, Verify panel, Caller panel, Source link, Unverified (catalogue entries for every state). `HashOrAddress`'s buttons no longer shrink below 44 px.
- **Anchor proof for `draw()` (brief item 4):** the indexer's proof (spec 6.1 fallback), used only after the browser bound it to the block a public node reports for X and to D, and after the purchases rebuilt the marker's roots; `draw()` checks it on chain. Size cost 0. Why not the browser-built proof: proposed row F5 (b). F6 must run the beta's API with a node-backed `AnchorProvider` (What F6 needs).
- **`anchorHash` after the draw (the recorded F5 item):** bound by the node's block for X.hash (drawn path `anchorBlock`, checked by Verify); otherwise shown "Unverified". Row F5 (a).
- **Voids end to end:** `POST /mine?to=void` (TEST-ONLY) mines past D + draw_window (VOID_MIN = 864,000 DAA: about 34 minutes on this PC, the node's store grows to about 2.4 GB; blocks to a throwaway address); the page's panel then shows "Void is due"; S3 shows "No draw: void"; S2's card shows its F4 void state (`void.spec.ts`, which runs last: the node is slow and about 6.6 GB in memory afterwards; a full suite with it earlier starved the F4 claim test).
- **The `simnet dev` self-check failure (brief item 6):** `sn5` required the template's first nonce to give a result other than the ground one. The dev run grinds for "a held combination wins any tier", which the first nonce sometimes already meets (39 and 11 tries in this stage's runs, against the code's estimate of 1,425): the grinder then stops at once (tries 1, the ground result is the honest one), and the check failed while printing two equal values with "!=". Now `grind_outcome` reads the record: ground (tries > 1, other numbers), "no grind needed" (tries 1, same numbers and nonce), anything else inconsistent; the dev run reports the second as such, never as a failure; the SN scenarios (jackpot targets, about 1 in 400,000 per try) still require a real grind. Unit tests in `sn.rs` (3) and `faucet.rs` (`mine_targets_parse`).

### Tests added, and negative controls
- `crates/web/tests/f5_rounds.rs` (3): a forged X.hash with the pool's numbers (found in 658,782 tries) passes the numbers check alone but is never `bound`, and the node's block refuses it; the facts against the fixtures' own cases (D, void lock, claim deadline, S, J, C_draw, R, roots; drawn: D from the pool); the verify items all match on an honest drawn round and only the anchor item mismatches with another header; `callerFigures` for close (1 R, C_close), draw without winners and void (0 R), draw with winners (one R per pool, back from the claim deadline), refusals (a missing shard, a wrong controller, an edited summary, a marker without the drawer).
- `crates/indexer-api/tests/doctored.rs` `verify_names_each_lie_as_its_own_item`: honest rounds 1–3 show no mismatch; each of the 6 doctored modes shows exactly its own item(s) as mismatch (states; the one changed shard's tree; the numbers).
- Vitest: `src/rounds/caller.test.ts` (6: close due only after T with the previous round drawn; rows bound and adding up; the press signs that build, accepted on the caller output; a wallet changing the caller output refused by `assemble` before anything is sent; lost answers settled, `submit_unknown`, then the same id; `dueOf` rules), `src/components/rounds.test.tsx` (6).
- Negative controls (fix disabled by a temporary edit, the test run, the file restored and compared): drawn-path block check off → `after_the_draw…` and `the_round_facts…` FAILED; drawn path always "bound" → both FAILED; void detection off → `caller_figures…` FAILED; tree comparison off → `verify_names…` FAILED. Not caught by any test: close's "exactly one R" (`n == 1` relaxed to `n <= 7` passes): the covenant already fixes the caller output, so no honest build can reach it; kept as defence in depth.

### e2e (`web/e2e/rounds.spec.ts`, `void.spec.ts`, in a fresh dev run)
Full `npm run e2e` in a fresh dev run (`-- --skip-build`), 45.2 min: **15 passed, 2 failed**.
- play.spec 6/6, smoke.spec 4/4, the F4 closed-copy test: passed.
- `rounds.spec.ts` 4/4 passed: S3 (rows 3 open, 2 closing, 1 drawn; round 3's S checked from all 16 shards; round 1 Verify: every state, status, roots, 16 trees, numbers and anchor a match; X.hash shown checked after Verify; round 2 numbers "Provisional"; axe clean in both themes, 360/430 px, every control named and tagged); draw() of round 2 from its panel (reward 0.02 KAS, fee 0.2214 KAS, total 5.27 KAS: three pools' reserves), then Verify: numbers and anchor a match; close() of round 3 from its panel after `POST /mine?to=deadline` (reward 0.085 KAS, fee 0.4323 KAS, total 2.04 KAS: the marker's reserve); doctored API: each mode's mismatch is its own item (`wrong_state` → the 4 states and the detail refuses with chain_mismatch; `wrong_tree`, `missing_purchase`, `extra_purchase` → shard 0's tree; `wrong_winning`, `wrong_anchor_proof` → the numbers), and with doctored purchases round 3's panel shows "The indexer's copy does not match the chain." with Draw disabled.
- **Failed: the F4 claim test** (`tickets.spec.ts:185`, 40-min timeout; not F5 code): in its two-browsers-one-wallet race the page reported T2's claim refused (`node_conflict`, "Rejected transaction cde3fc…") while T3's was accepted; at the timeout T2's card showed "Claimed", so the test's re-press loop waited for a Claim button that never came. Either the refused claim went through after all (its own outputs spent at once by the other browser of the same wallet, so the page saw none of them live) or another path claimed it; the trace was overwritten by the next run. This is the race "What F6 needs" already lists (a claim refused after a conflict); in F4's run the order was the other way and it passed. To look at in part B or F6 with a fresh trace.
- **Failed: `void.spec.ts`**, only because the hung F4 test had left the open round past its deadline (no ticket could be bought). The spec now starts from the TEST-ONLY `POST /round` (a selling round whose predecessor is drawn), whatever ran before.
- An earlier full run (the void inside `rounds.spec.ts`) passed the void (31.5 min, "No draw: void" on S3, the ticket's card void on S2) but left the node at about 6.6 GB of memory and about one block a second, which starved the F4 test after it: hence `void.spec.ts`, last.
- **Bug found and fixed by the void e2e:** after one transition was accepted on a page (the close), the panel kept its "Accepted" progress, which hid the press of the next transition due on the same page (the void); `useRoundDetail` now clears the progress when another transition is due.
- `npx playwright test e2e/void.spec.ts` after the fix, in a fresh dev run: **1 passed (33.0 min)**: `POST /round` → round 5 selling; a ticket; `POST /mine?to=deadline`; close() of round 5 from its panel; `POST /mine?to=void` (1,926 s); "Void is due"; void accepted (reward 0.005 KAS, fee 0.2017 KAS, total -1.49 KAS: the marker's 1.69 KAS reserve refunded to this wallet, which had closed the round); S3 "Void" and "No draw: void"; the ticket's card on S2 `void`.
- Not re-run after the fix: the whole suite in one go (about 80 minutes with the void). `npm test` after the fix: 22 files, 180 tests passed.

### Commands run (the full set once at the end, one cargo process at a time)
- `cargo test -j 4 --no-fail-fast` (from `crates/`, debug; default members): **1,109 passed, 0 failed, 2 ignored**; the one red target is `spike_vdf`, which overflows its stack in debug as before (D113; not touched).
- `cargo test -j 4 -p web --release --lib --test api --test f4_tickets --test f4_review --test f5_rounds`: **48 passed, 3 ignored** (f5_rounds 3).
- W1: `cargo test -j 4 -p web --target wasm32-unknown-unknown --release --test w1_wasm -- --nocapture` (CC/AR set): 1 passed ("18 builds … 97 digest lines … 291 decode and verify calls … equal the native golden file": the goldens are unchanged).
- W4: `cargo test -j 4 -p web --release --test w4_mainnet -- --nocapture`: 1 passed.
- `cargo test -j 4 -p indexer-api --features doctored --test api --test doctored --test review_f3 --test review_f4`: **13 passed** (doctored 4, with `verify_names_each_lie_as_its_own_item`); `--test production`: 1 passed.
- `cargo test -j 4 -p simnet --release`: **15 passed** (11 + 4 new). `cargo test -j 1 --release -p tn10`: **25 passed**.
- `npm test` (in `web/`): `tsc -b` clean, ESLint clean, Prettier clean, Vitest **Test Files 22 passed, Tests 180 passed** (12 new).
- `npm run build` (placeholder indexer URL and covenant ID): release wasm **6,710,306 B** (F4: 6,650,316; +59,990 B), brotli 2,366,686 B; page JS index 473,476 B (F4: 443,182), worker 48,585 B; `check-bundle: served wasm … = the release package, no fixture code or key`, `no catalogue, faucet, hooks or simnet configuration in the production bundle`; the control finds every dev marker. `npm run check:mainnet`: "ships no quick wallet code, page, glue or wasm". `npm run check:csp`: 10 ok lines.
- Dev runs stopped after each use (ports 18610, 8790, 8791, 5173, 8792–8797 free); the simnet data directories this stage made (up to 2.4 GB each) deleted.

### Files changed
- Rust: `crates/web/src/{rounds.rs (new), indexer.rs, api.rs, dto.rs, bindings.rs, lib.rs}`, `crates/web/tests/{f5_rounds.rs (new), common/mod.rs (Drawn: vault, coins), api.rs (the new field)}`, `crates/lottery/src/web/mod.rs` (code 54), `crates/lottery/tests/f1_web.rs`, `crates/indexer-api/tests/doctored.rs`, `crates/simnet/src/{sn.rs, main.rs, dev.rs, faucet.rs}`.
- Page: `web/src/rounds/` (new), `web/src/screens/Rounds.tsx`, `web/src/screens/rounds/` (new), `web/src/components/{CallerPanel, callerPanelState, RoundRow, SourceLink, Unverified, VerifyPanel, rounds.test}` (new), `HashOrAddress.tsx`, `web/src/dev/{Catalogue.tsx, roundSamples.ts (new)}`, `web/src/app/App.tsx`, `web/src/worker/protocol.ts`, `web/e2e/{rounds.spec.ts, roundsHelpers.ts, void.spec.ts}` (new); `web/CLAUDE.md`, root `CLAUDE.md`.
- Not touched: `covenants/`, `crates/lottery/templates/`, every golden file (`w1_api.txt`, `w1_verify.txt`, `f1_digests.txt`, the page fixtures), `Cargo.lock`, `package-lock.json`, the unrelated uncommitted files.

### What is left (part B or F6)
- Expired pools are not absorbed by the page's `draw()` (What F6 needs).
- The beta API's node-backed anchor proofs (What F6 needs).
- A close, draw or void whose outcome is unknown is not kept across a reload (row F5 (f)); no money depends on it; a reviewer may want it kept like claims.
- The F4 claim test's two-browsers-one-wallet race failed in the final full run (see e2e above): look at it with a fresh trace; then one full `npm run e2e` in a fresh dev run (about 80 minutes with `void.spec.ts`).

### What the reviewer should attack
1. `verifyRound`: can an indexer answer make an item read "match" without the chain fixing it (objects of another round, a stale but live object, the anchor path with the drawer withheld, a drawn round served with a marker)? Can a lie hide behind "not checkable" while the page shows the figure as checked?
2. The drawn path's `anchorBound`: any path where the page shows X.hash as checked without a node block (`RoundDetail` uses Verify's `anchorBound` and the closed path's `provisional`).
3. `callerFigures`: the reserves count inferred from the caller output, void versus draw from the lock time (absorbed pools raise a draw's lock time), refunds counted when the wallet funded the marker, the total's sign.
4. `prepareCaller`: are the objects given to `buildDraw` exactly the ones `checkIndexerRound` checked? Is a draw ever built from unchecked purchases or an unbound proof?
5. Status and figures: `shownStatus` (void and the local copy), the "Unverified" marks; does any indexer-only figure appear without one?
6. `POST /mine` exists in the simnet binary only (TEST-ONLY), under the faucet's loopback and origin rules.
7. The S2/S3 void wording against D96 (no winners; the deposit back by `return_deposit()` path b).

### Owner decisions (needed; nothing was decided)
1. **The void test makes `npm run e2e` about 35 minutes longer.** The protocol's shortest draw window is 24 hours of chain time (864,000 blocks); the private simnet has to mine all of them before a void is possible, which takes about 30 minutes on this PC, about 2.4 GB of disk per dev run (deleted afterwards), and leaves the dev node at about 6.6 GB of memory and slow, so the void runs last of all (`void.spec.ts`). Trust: none. Money: none. Work: every full end-to-end run takes about 45 minutes instead of 10. Options: keep it in every run (as now; spec 8 lists the void), or run it only when asked (`E2E_VOID=1`) and before each commit that touches the draw, voids or S2/S3. Recommendation: only when asked, plus once per stage that touches those parts.
2. **To know, no decision for the beta: running the round costs the caller more than the reward.** On the dev instance a `close()` paid a 0.085 KAS reward against a 0.43 KAS network fee, and it locks 1.69 KAS (the marker's reserve) until the draw; a `draw()` paid 0.02 KAS against 0.22 KAS and locks 1.69 KAS per prize pool for at least the 7-day claim window. The page says so ("The network fee is larger than the caller reward.") but nobody gains by pressing, so on the beta the owner will run every close and draw (testnet coins). If nobody draws within 24 hours of chain time, the round ends as a void: no winners. For any mainnet front end the caller shares (s_C, s_D) need sizing against real fees (design 12).
3. **To know: on testnet-10, `draw()` from the page needs the hosted indexer to follow a node.** The browser checks the indexer's anchor proof against a public node before building, and the chain checks it again, so the indexer cannot make the page sign a wrong draw (trust unchanged). If the hosted indexer has no node connection, the page shows "Anchor proof: Unavailable" and the round can only be drawn by the repo's tools, or voided after 24 hours. Work: about half a day in F6 to run the API inside the indexer process that already follows a node.
