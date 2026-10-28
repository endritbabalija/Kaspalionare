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
| F5 Rounds, verify, run the round | done (2026-10-26, `d23ed0d`; reviewed by `covenant-reviewer`, every should-fix and note fixed 2026-10-27, see "F5 review and fixes"; not committed) | S3 list and round detail (facts from Rust with their sources, "Unverified" for the indexer's), Verify in this browser item by item (`verifyRound`), X.hash after the draw bound to a public node's block, S4 close/draw/void from the panel (`callerFigures`, code 54), the SN5 dev self-check fixed, `POST /mine` (TEST-ONLY); e2e: S3, draw, close, doctored API, void. See "F5" below. |
| F6 Vault, How it works, beta instance, deploy, security review | part A done (2026-10-27, S5 Vault, `0dcb01a`; reviewed by `covenant-reviewer`, fixes `e5309c5`); part B done (2026-10-27, S6 How it works; light process: web tests, no adversarial review, the owner reads the page; not committed); part C done (2026-10-27, deploy preparation: hosted indexer API `--node`, site configuration and narrowed CSP, prebuilt deploy with `check:deploy`, beta genesis dry-run with a fresh admin key, journeys plan; nothing deployed or launched; not committed); security review done (2026-10-28: no must-fix; four should-fix and the notes fixed by the fix agent, see "F6 security review and fixes"; committed `3129728`); part E done (2026-10-28, launch preparation with the owner's choices: `beta8h` profile and dry run, several indexer URLs with fallback, `/v1/status` error masked, M8 store re-anchored, no M8 transfer (same key), Vercel proxy feasible on paper; not committed, see "F6 part E"); part F (2026-10-28, launch: e2e + e2e:void green, **beta genesis on testnet-10** `2b86171b…01ec00`, launch bundle verified, hosted indexer on the Linode at `https://172-104-238-197.sslip.io` with a witness node, `web/dist` built and checked, a page buy accepted on testnet-10; not committed, see "F6 part F"); Vercel deploy and journeys not started | Part A: "F6 part A"; part B: "F6 part B"; part C: "F6 part C" (owner choices listed there). Then and the security review: read "What F6 needs". |

## What F6 needs (Vault, How it works, beta, deploy, security review)

- **From F5: the draw's anchor proof on testnet-10.** The page draws with the indexer's proof (checked in the browser first). The standalone API serves a proof for a closing round only when its process follows a node (`AnchorProvider`, as `simnet dev`'s `NodeAnchor` in `crates/simnet/src/dev.rs`). The beta's API must run that way (the M8 indexer in `tn10` already follows a node; embed `indexer_api::run` with a node-backed provider), else S4 shows "Anchor proof: Unavailable" and only the void is possible after D + draw_window.
- **From the F5 review: an absorbing `draw()` from the page on a real chain.** The page's draw and void now absorb expired pools (S1, see "F5 review and fixes"), proven byte-identical to the native builder in Rust and through the page's own code in Vitest, but no simnet e2e can reach an expired pool (the claim window is at least 6.9 million DAA after the anchor; the owner ruled out a test-only short window, DECISIONS 2026-10-26). The M8 testnet-10 instance's pools (rounds 1–8) are long past their deadlines: the first page draw of its round 9 (or of the beta) should absorb them; check that draw's "Expired pools" row and the funders' refunds on the explorer. The beta's API must serve `GET /v1/pools` (it does; same binary).

- **Beta instance:** never launched through `tn10/src/cases.rs` or `tn10/src/wallet_probe.rs` (their admin hash is `admin_hash(ADMIN_SK)`, a key in the repo: anyone could call `set_params()`); use the campaign path or another that sets a fresh admin key, and check the bundle's admin hash before announcing it (F1 re-review N5).
- **W4 on the deployed artifact** (the file Vercel serves, after wasm-bindgen and any optimiser). W1's signing half covers the 14 shapes the quick wallet can sign; the 4 ECDSA and P2SH owner withdrawals are signed in wasm by no test (the page never creates such positions).
- **Hosting the indexer API:** one binary plus the indexer's store directory (on the same machine or a synced copy); plain HTTP/1.1 behind a TLS proxy (`--bind 0.0.0.0:<port>`, `--allow-origin <site>`); only global limits exist (64 requests at once, 256 connections, 10 s deadline), so the proxy adds per-client rate limits; in embedded mode cache the anchor proof per round and rate-limit that endpoint; the follower re-reads `index.json` (18 MB for M8) every 10 s, fine now. The security review looks at the follower (untrusted log lines) and the caps.
- **Page security settings:** narrow the CSP `connect-src` (`https: wss:`) to the indexer and the resolver's node hosts; `vercel.json` has only `headers` (root `web/`, build command, output `dist` are F6's); re-run `npm run check:csp` and check the live headers after the first deploy; `wss://` through the resolver from an `https` page is untried in a real tab; give the node client a hard connect deadline so a refused connection ends in `node_unavailable` (a built page already refuses `ws://` node URLs).
- **Testnet-10 journeys (spec 8):** KasWare claims and buys in a real browser (the same adapters and `assemble`); a `return_deposit()` built by the page for an old M8-instance ticket already past its window (owner, DECISIONS 2026-10-26); Kaspire if installed; the faucet link `https://faucet-tn10.kaspanet.io` checked by hand.
- **Not measured yet (spec 8 quality):** the time to build a buy and a claim on a mid-range phone (desktop Chromium: under 0.31 s ticket to cost panel; the whole F4 e2e test with 4 claims about 55 s); bundle 2.27 MB brotli with the node client (opt-level "s" would save about 10%, and changes the artifact W4 checks).
- **The copy pass (spec 8):** every visible string a figure, name, status, error or control label. F4 review changed "Not on chain" → "Spent", "Open: not kept" → "Not kept", "Nothing on chain" → "Gone", "Imported: N tickets" → row "Tickets kept" N, "No tickets" → row "Tickets" 0, "No rounds" → row "Rounds" 0; new: "Expired", "No pool", "Unknown", "Incomplete", "Payout", "Send". Earlier candidates fixed in F2/F3 ("Cost", "Back", "Round", "Details").
- **A claim refused after a conflict, pressed again at once, can be refused again** (F4 review e2e): the re-press was built from the node's UTXO index before it caught up with the other claim (refused as an orphan: an input already spent on chain); a later press went through. F6 part A review N1: the refused ID itself is never signed or sent again and a build that comes out as it is built again after a pause (`unrefused`); a re-press can still be a new transaction the node refuses as an orphan before it has caught up (seen in every two-claimer run; the next press goes through). A short wait for the node before offering the re-press would remove that refusal.
- **The raw node answer per address** is bounded only by the wRPC message limit (v2.1.0 has no UTXO query by outpoint); if dust at a covenant address ever hurts a phone, the fix is node- or indexer-side.
- **Spec 6 S6 How it works:** done in part B ("F6 part B"). For the deploy: the site's CSP now has `frame-src 'self'` and the figures (`/figures/(.*)`) their own CSP rule; check both live headers after the first deploy.

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

### F5 review
- (a) **The page's `draw()` and void absorb expired pools** (design 7.3): `GET /v1/pools` (new; the live pools, oldest claim deadline first, at most 256) only says where to look; each candidate is read from a public node by outpoint and kept by `absorbablePools` (Rust) when a pool of this instance, of an earlier round, past its claim deadline (below the node's virtual DAA; for a draw also below the void lock, so its lock time stays a draw's), with the controller's R; oldest first, at most A_MAX. Without the list nothing is absorbed (liveness only: a hidden pool waits for a later draw).
- (b) **A void is built without purchases**: `web::cases::draw` uses k empty trees for a void given none; `web::build::draw` checks the trees' roots for a draw only (a void uses no leaves, siblings or counts); purchases given for a void are still checked. Native transactions unchanged (byte-identical, `review_f5`).
- (c) **A settlement record alone fixes no status** (a void with pending requests leaves one too): no checked status, m checked only from a marker, pool, open controller or shard (`mChecked`); a record contradicts "open" and "closing".
- (d) **The close and draw transaction IDs are shown checked only when a live object only that transaction creates fixes them** (close: the marker, or round r + 1's controller with drawn = 0; draw or void: the drawer of round r + 1, or r + 1's controller with drawn = 1); never from a pool or record (claims and settlements re-create those); an indexer ID differing from a bound one is `chain_mismatch`; otherwise "Unverified". Transaction IDs and hashes in round, object and ticket answers are 64 hex digits.
- (e) **X before the draw is "Provisional"** (bound only to the node's current view, like the numbers); after the draw the round detail binds X to the node's block itself and shows its DAA score.
- (f) **`callerFigures` reads the kind from the exact lock time** (draw: max(D, absorbed deadlines); void: max(D + draw_window, absorbed deadlines); both matching is refused), takes the vault (whether a settlement record is posted) and the absorbed pools (every covenant input named); "Back" is the new pools' claim deadline only; a record's R comes back "With settlement".
- (g) **The void e2e runs on request**: `npm run e2e:void` (E2E_VOID=1); `npm run e2e` skips it (owner, DECISIONS 2026-10-27).
- (h) **No simnet e2e of an absorbing draw** (pools expire about 6.9 million DAA after their draw; no test-only short window, DECISIONS 2026-10-26): Rust byte-identity and page-level Vitest tests; the real-chain check is F6's on testnet-10.

## F6 part F (stage agent, 2026-10-28): launch of the testnet-10 beta (no Vercel command; not committed)

No covenant change; goldens untouched. Testnet-10 only. Every txid is in `docs/tn10-mass-check.md` "Front end beta:
genesis".

- **Owner rule, void e2e:** in one fresh dev run, `npm run e2e` **23 passed, 1 skipped**, then `npm run e2e:void`
  **1 passed** (1.1 h). Dev run stopped; ports 18610, 8790, 8791, 5173, 4173, 4174 free; that run's 2.5 GB of simnet
  data deleted.
- **Genesis:** dry run all ok (547.49 test-KAS, fee 0.46065700). Live: **covenant ID
  `2b86171b385bb93ffb7726994ba83901a7732af432c2c583aaa45cce6401ec00`**, genesis
  `1b6cc6eb87f2aeeb3d9e824621482e2e9eb102c1f34421ca6aaf2b1e05408caf` (fee 0.46061100 KAS), T(1) = DAA 608,535,700
  (about 11:34 UTC on 2026-10-28, shown as 12:34 on this PC). Store `tn10-data/beta/2b86171b…/` (`tn10-data/beta/current`; M8 keeps
  `tn10-data/current`). **Launch bundle `docs/launch/tn10-beta.json`: `bundle-verify` PASS** on a second node.
- **Hosted indexer** (Linode `172.104.238.197`, Ubuntu 24.04, 1 GB): option B of `docs/launch/indexer-server.md`.
  - The binary built on this PC by the Dockerfile's `export` target (Linux x86-64, sha256 `d10a622f…378e924c`, no
    doctored code), installed as `/usr/local/bin/indexer-api`; the repo's systemd unit unchanged (`DynamicUser`,
    sandboxed, `MemoryMax=1G`), enabled; store in `/var/lib/private/kaspalionare` (seeded from the PC's genesis
    store). No key, no `.env` on the server (store and bundle checked: neither key's hex is in them).
  - Settings `/etc/kaspalionare/indexer-api.env` (the example with `INDEXER_API_COV` set,
    `INDEXER_API_STORE=/var/lib/kaspalionare`): `NODE=resolver`, **`WITNESS=resolver`** (seen: pinned to
    `vector-10.kaspa.green`, witness `muon-10.kaspa.blue`; after a restart the pair may swap), loopback bind,
    `CLIENT_IP=forwarded`, per-client limits as the example.
  - Caddy 2.11.7 (official apt repo), `/etc/caddy/Caddyfile` = the repo's with the site `172-104-238-197.sslip.io`:
    **Let's Encrypt certificate issued at once** (nip.io not needed). The hostname is in public certificate logs:
    scanners hit it within seconds (502s before the API started; harmless, GET only).
  - Checked from this PC: `https://172-104-238-197.sslip.io/health` `{"ok":true}`; `/v1/status` testnet-10, this
    covenant ID, DAA growing; `/v1/instance` byte-identical to the store's `bundle.json`, blake2b
    `d10bb199…7095d60` = `VITE_BUNDLE_HASH`, and equal to `tn10-beta.json` in every field but `snapshot`; CORS answers
    only the listed origins; POST 405. **Reboot test:** back in about 70 s, resumed from its log; 65 MB RSS.
  - **CORS, to update once the Vercel URL is known:** on the server, edit
    `INDEXER_API_ALLOW_ORIGIN=https://kaspalionare.vercel.app,https://site-placeholder.invalid` in
    `/etc/kaspalionare/indexer-api.env` (comma list, exact origins, no trailing slash), then
    `systemctl restart indexer-api` (the API listens again about 20 s later, after it picked its witness). The
    smoke test's temporary `http://127.0.0.1:4173` was removed afterwards.
- **Explorer:** `explorer-tn10.kaspa.org` answers 402 `DEPLOYMENT_DISABLED`. `https://tn10.kaspa.stream` shows
  testnet-10 transactions, but under `/transactions/<id>` (addresses and blocks as the page's layout). Page change
  (UI only): `src/lib/explorer.ts` uses `transactions` on `kaspa.stream` hosts, `txs` elsewhere; new
  `src/lib/explorer.test.ts` (4 tests; mutant "fixed path" fails one). `npm test`: 32 files, **237 passed**.
- **The production build** (`web/.env.production`, new file, every value public):
  `VITE_NETWORK=testnet-10`, `VITE_INDEXER_URL=https://172-104-238-197.sslip.io/v1/`,
  `VITE_COVENANT_ID=2b86171b385bb93ffb7726994ba83901a7732af432c2c583aaa45cce6401ec00`,
  `VITE_BUNDLE_HASH=d10bb199b12967b46edd94d0a2c3d9f29bd998bbf5f01f344c55747ff7095d60`,
  `VITE_REFERRAL_ADDRESS=kaspatest:qz2cr4g5262katpapg4djvpvgakltuv5h99dedl4hh5ktluvke6pza8pa3h2f`,
  `VITE_EXPLORER_URL=https://tn10.kaspa.stream`, `VITE_FAUCET_LINK=https://faucet-tn10.kaspanet.io`, no
  `VITE_NODE_URL` (resolver), no proxy. `npm run csp:write` (vercel.json connect-src: the sslip.io origin and the
  resolver's four domains); `npm run build` ok (check-bundle, control); `check:mainnet` ok; `check:csp` **34 ok**;
  `check:deploy` **every check ok**. `web/dist` unchanged since (sha256 of all 13 files compared after the smoke
  test; `index.html` `40df8a04…`, `assets/web_bg-CQ1zupv_.wasm` `8059a918…`).
- **Smoke test on testnet-10** (that `dist` under `vite preview` with vercel.json's headers, headless Chromium,
  a throwaway profile): round card Round 1, Open, jackpot 100 KAS, deadline DAA 608,535,700; Rounds row 1 open;
  round 1 detail ready and **Verify: 35 items, all match** (18 state, status, 16 trees); How it works with the
  figure; no CSP report. Quick wallet `kaspatest:qzlz2nfl…ykufykfcu9u` funded 20 tKAS
  (`d38178b8…1939`); **buy b = 1 ACCEPTED `b950e55699b4c71bc6dc34794af8fc74fc2c8c8975edcb104f9c6a8a7365fdb3`**
  (total 1.55 KAS; referral to the owner's default); shown on My tickets and counted by the hosted indexer. The page's
  one 404 is `/v1/rounds/1/anchor-proof` for the open round (no proof before the close; expected).
- **Seen, not fixed (UX, no money risk):** on a cold load the quick-wallet button reads "Create quick wallet" for
  several seconds while the worker boots and connects to a node, even when a key is stored; a press then is refused
  by the key store's compare-and-swap (`wallet_changed`), the stored key is untouched, and the button turns into
  "Use quick wallet" (and a remembered quick wallet reconnects by itself). A loading state on that button would
  remove the refusal.
- **Left:** the Vercel deploy (orchestrator, with `site-deploy.md` steps 4–7 and the "After deploying" checks, then
  the CORS line above); `web/.env.production` and the new `vercel.json` connect-src to commit; the journeys
  (`docs/launch/beta-journeys.md`); round 1's Close after DAA 608,535,700 and Draw within 24 h; the smoke wallet's key
  and its ticket live only in this session's scratchpad profile (`smoke-profile`, about 18.4 tKAS), a throwaway.

### Proposed DECISIONS rows (F6 part F)

- **Beta instance launched:** covenant ID `2b86171b…01ec00`, genesis `1b6cc6eb…08caf` (profile `beta8h`, V0 510 KAS),
  launch bundle `docs/launch/tn10-beta.json` (bundle-verify PASS). Reason: owner's go-ahead (delegated for the night).
- **Indexer host:** Linode `172.104.238.197`, systemd binary built on the PC, Caddy with the sslip.io hostname and a
  Let's Encrypt certificate (no domain); two public nodes must agree (witness). Reason: no domain yet; the 1 GB server
  must not compile.
- **Explorer:** `https://tn10.kaspa.stream`; the page links kaspa.stream transactions under `/transactions/`. Reason:
  `explorer-tn10.kaspa.org` is disabled.

## F6 part E (stage agent, 2026-10-28): launch preparation with the owner's choices (nothing launched, deployed or submitted; not committed)

No covenant change; goldens untouched. Network contact: read-only RPC (`tn10 info`, fee estimate, the two re-anchor
proofs and the catch-up) and the Vercel/Caddy documentation. **No testnet-10 transaction was submitted** (item 4b
below), no Vercel command, no genesis.

### What changed

1. **`tn10 beta genesis --profile beta8h`** (`crates/tn10/src/campaign.rs` `GenesisKnobs::beta`, `main.rs`):
   round_len 288,000 (8 h), d_min 144,000, T(1) = genesis + 8 h, windows at their floors, J(1) 100 KAS,
   **V0 510 KAS** (the cap's floor for L(1) = 102 KAS; draw() tops the jackpot up to the cap, so 510 keeps it at
   100 KAS from round 2, 600 would give 117.65), the owner's fee address built in (`BETA_FEE_ADDRESS`,
   `schnorr_payout`; `--fee-address` overrides), the referral address printed with the page values
   (`BETA_REFERRAL_ADDRESS`). Test `beta_profiles_pass_genesis_validation` extended (the knobs, the fee key in the
   controller and params, V0 509.99999999 refused, J(2) at V' 510/600 and m' 2/3/10, a mainnet address refused).
2. **Several indexer URLs with fallback** (`web/src/net/indexers.ts`, new): `VITE_INDEXER_URL` one URL or a
   comma-separated list (at most 8, no duplicates, http(s) or the same-origin `/api/v1/`); `IndexerSet` asks them in
   order, the last good one first (`localStorage` `kl.indexer`); `withIndexer(ctx, op)` runs one read-and-check
   step on one indexer and moves on for `FALLBACK` codes (`indexer_unavailable` incl. timeout and busy,
   `chain_mismatch`, `bytes_length`, `input_format`, `store_format`, `anchor_invalid`); any other code is thrown at
   once; a refusal is reported over an unreachable one. Wrapped (reads and builds only, never a signature or a
   send): `checkInstance`, `checkedRound`, `loadRoundCard`, `loadSales`, `prepareBuy`, `roundRows`, `checkRow`,
   `loadDetail`, `verifyRound`, `prepareCaller` (per build inside `unrefused`), `copyRound`, `findByPayout`,
   `loadVault`; the set's own methods fall back per request for every other call site. `Config.indexerUrl` →
   `indexerUrls`; `Ctx.indexer` is `IndexerApi`. `scripts/csp.mjs` (every listed origin into connect-src; the
   proxy rewrite), `check-deploy.mjs` (each entry; the rewrite; `--vercel`: the rewrite route), `check-csp.mjs`
   (every listed origin probed, and a same-origin `/api/v1/` fetch allowed). `.env.production.example`,
   `web/CLAUDE.md`.
3. **`/v1/status` `error`** is the fixed word `store_error` (`routes::STATUS_STORE_ERROR`) for a remote client and
   for one behind the local proxy; the text only on loopback without `X-Forwarded-For` (`routes::is_local`, shared
   with `/health`). The page never reads the field (types only). Test `hosted::status_error_text_only_on_loopback`
   (both client modes; the page's parser still reads it; control: the local answer carries the path, a clean store
   answers null); mutant (text for everyone) fails it.
4. **Old-M8 preparation:** `campaign reanchor --check` passed (383 live instance UTXOs unchanged, nothing written,
   file hashes unchanged), then the real `campaign reanchor`: resume block `e7df021d…` (DAA 605,601,500, pruned) →
   `05e00563…f9c748` (DAA 608,152,477), caught up to DAA 608,152,503. A copy of the store from before is in this
   session's scratchpad (`m8-store-before-reanchor`). **No transfer:** the M8 campaign key *is* `TN10_PRIVATE_KEY`
   (the campaign Runner signs with the funding key; `.env` holds only it and `TN10_ADMIN_KEY`; every M8 ticket pays
   it), so there is nothing to move, and importing it would put the main test key in a browser. Options for the
   owner below. Logged in `docs/tn10-mass-check.md` "Front end F6 part E".
5. **Vercel proxy: feasible on paper** (`docs/launch/site-deploy.md` "The indexer through the site's own
   address", `indexer-server.md` "Behind the Vercel proxy"): external rewrites work for static prebuilt sites;
   120 s proxy timeout on every plan; no documented body limit; `no-cache` answers are not cached; Vercel overwrites
   `X-Forwarded-For` and sends `x-real-ip`, egress IPs unpublished, so per-client limits need Vercel's documented
   shared-secret header (a `routes` transform) checked by Caddy; bare-IP and `http://` destinations are neither
   allowed nor refused in the docs (private IPs and IPv6 are refused), so one throwaway deploy must confirm; Hobby's
   "non-commercial" wording ("any method of requesting or processing payment") may cover a ticket-selling page even
   on testnet. Prepared: `INDEXER_PROXY_TARGET` (not a page variable) → `csp:write` writes the
   `/api/v1/:path*` rewrite; the secret transform and the Caddy rule are not written (untested).
6. Docs: `docs/launch/beta-plan.md` (the `beta8h` section, the figures, the duties), `beta-journeys.md` (8 h pace,
   J8 fallback journey, the re-anchor, M.1's key finding and options), `site-deploy.md`, `indexer-server.md`.

### Dry-run figures (2026-10-28)

- `beta8h` C0/C1 all ok. **Locked at genesis 547.49 test-KAS** (510 vault + 21 × 1.69 reserves + 2 fee
  accumulator), **fee 0.4607 KAS**. Dry-run covenant ID `b989a09a…1481107b` (changes at the real genesis).
- **Main key (read-only):** 4,789.39 test-KAS, 203 coins, largest 3,728.62.
- **Per day, 3 closes + 3 draws:** fees ≈ 1.96 KAS (close 0.4323, draw 0.2217 at 100 sompi/gram, M8 masses;
  an absorbing draw unmeasured); reserves: the marker's 1.69 KAS back at each draw; up to 7 × 1.69 KAS per draw
  locked about 8 days → at most ≈ 300 KAS in flight. Budget ≈ 950 KAS.
- **6-day close limit:** 5,183,400 DAA = 17.998 rounds of 8 h (miss at most 17 consecutive slots).
- **Checks tripped:** none; the vault cap is the binding one (V0 ≥ 510 KAS). Close within 4 h of T keeps the 8-h
  clock; draw within 24 h of the close or the round can be voided.

### Runs (one cargo process at a time)

- `cargo test -j 4 --no-fail-fast` (debug, default members): **1,111 passed, 0 failed, 4 ignored**; the only failing
  target is `spike_vdf`'s known debug stack overflow (D113). Goldens unchanged (`f1_bytes` passes).
- `cargo test -j 4 --release -p tn10`: **27 passed** (`beta_profiles_pass_genesis_validation` with `beta8h`).
- indexer-api `--features doctored --lib --test api --test doctored --test review_f3 --test review_f4 --test
  hosted`: 6 + 6 + 4 + 2 + 1 + **16** passed (new `status_error_text_only_on_loopback`; its mutant fails it);
  `--test production`: 1 passed.
- The web test command: `-p web --release --test api --test f4_tickets --test f4_review --test f5_rounds --test
  review_f5 --test f6_vault`: 20 (3 ignored) + 6 + 7 + 3 + 6 + 9 passed; **W3 page** (debug): 6 passed; **W1**: 1
  passed (97 digests, 18 API builds, 291 decode/verify calls, 14 signed); **W4**: 1 passed (149 s).
- `npm test`: 31 files, **233** tests passed (221 before: 11 in `src/net/indexers.test.ts`, 1 in `config.test.ts`);
  typecheck, ESLint, Prettier clean. Mutants run by hand and restored: no fallback in `run` (7 of 11 fail);
  `withIndexer` not pinning (2 fail: the lying-first case and the one-indexer-per-step case).
- `npm run e2e` (fresh dev run): **23 passed, 1 skipped** (void, on request). Ports 18610, 8790, 8791, 5173, 4173,
  4174 free afterwards; the run's simnet data (151 MB) deleted.
- `npm run build`: ok (check-bundle and its control). `npm run check:mainnet`: ok. `npm run check:csp`: **34 checks
  ok** (32 before; new: a same-origin `/api/v1/` fetch allowed in the page and the worker; every listed indexer
  origin probed).
- `check:deploy` rehearsal (environment values: the dry-run covenant ID and bundle hash, the owner's referral
  address; `csp:write`, `vite build`): two indexer URLs: every check ok; `/api/v1/` plus a direct URL with
  `INDEXER_PROXY_TARGET=http://203.0.113.5`: every check ok, with the "not encrypted" note. Negative controls: the
  proxy configuration against the old `vercel.json` (connect-src and rewrites FAIL), `/api/v1/` without a target
  plus a placeholder URL, a URL listed twice, a target on this machine: each refused. `vercel.json` restored
  (no diff), `dist` rebuilt with the default configuration.
- Read-only on testnet-10: `tn10 beta genesis --profile beta8h --dry-run`, `tn10 info`, `tn10 masses`,
  `campaign reanchor --check`; local files only: `campaign reanchor`.

### Proposed DECISIONS rows (F6 part E)

- **Beta profile `beta8h`:** 8-hour rounds, d_min 4 h, J(1) 100 KAS, V0 510 KAS (the cap's floor, so the jackpot
  stays at 100 KAS), the owner's fee address, windows at their floors. Reason: the owner's choices of 2026-10-28;
  V0 600 would raise round 2's jackpot to 117.65 KAS.
- **Several indexer URLs with fallback:** the page asks the build's list in order, the last good one per browser
  first, and moves on for unreachable, timed-out, busy or refused answers; one read-and-check step on one indexer;
  never a retry around a signature or a send. Reason: owner item 6; trust unchanged (every answer node-checked).
- **`/v1/status` error text only on loopback:** the public `error` is `store_error`.
- **The M8 campaign key is the main key:** no transfer was made; the M.1 import waits for the owner's choice.
- **Vercel proxy: feasible, not adopted:** prepared behind `INDEXER_PROXY_TARGET`; needs a throwaway deploy, the
  secret header and Caddy rule, and an answer on Hobby's commercial-use wording.

### Owner choices (none decided)

1. **Vault size for the beta:** 510 test-KAS (built; the jackpot stays near 100) or more (the jackpot then starts
   higher from round 2; the vault is your own position, withdrawable).
2. **Old-M8 check M.1, since the M8 key is your main test key:** A. a new main key, moving about 4,739 test-KAS to it
   (one more testnet transfer to approve; about an hour), then import the old key with about 50 test-KAS as planned
   (recommended); B. a `tn10` tool that writes a backup file of the M8 tickets for the page to import, no key in a
   browser (about half a day; whether Return deposit is offered for a ticket paying another address is unchecked);
   C. import the main key into the throwaway profile anyway (no work; the beta's funding key sits in a browser,
   testnet coins only).
3. **Indexer hosting with no domain:** the Vercel proxy (free with the site; plain http between Vercel and the
   server for an IP-only server; a throwaway deploy to confirm; per-client limits need the secret-header setup), or
   a domain for the server (https, the setup already written).
4. **Vercel plan:** Hobby's "non-commercial" rule mentions "requesting or processing payment from visitors"; a
   ticket-selling page, even for test-KAS, may need Vercel support's answer or Pro ($20 a month).
5. Still open from before: the server host, and the go-aheads for genesis (about 548 test-KAS) and the deploy.

### Left

- No e2e test of the fallback in a real browser (the dev run builds one URL); the unit tests run the page's own
  `checkInstance` on the real wasm against a dead, a hanging, a busy and a lying first URL.
- `npm run e2e:void` not run; `rounds/load.ts`, `verify.ts` and `caller.ts` changed (wrapped only), so the rule asks
  for it before committing.
- The Caddy rule and the Vercel secret transform are sketches; nothing on a real host.

## F6 security review and fixes (fix agent, 2026-10-28; not committed)

The review of the whole front-end track found no must-fix, four should-fix items and notes; all are fixed below.
No covenant change; goldens unchanged (`f1_bytes`, W1). Nothing submitted to testnet-10, nothing deployed, no
Vercel command. The reviewer's untracked probe `crates/web/tests/secreview_w3_probe.rs` is folded into
`crates/web/tests/w3_page.rs` and deleted.

### Should-fix, per item: change, test, negative control

1. **S1, one client could block the hosted API during a draw.** `server.rs` guard: at most `per_client_inflight`
   (16) requests of one client at once (`limits::InFlight`, by /64 for IPv6; on with the per-client limit, so the
   dev run and the doctored binary stay unlimited), and anchor-proof requests take a permit of their own pool
   (`max_anchor_waits`, 32), never one of the 64 global ones. Flags `--per-client-inflight`, `--max-anchor-waits`.
   Test `hosted::one_client_cannot_starve_another_during_a_draw`: 40 parallel anchor-proof requests from one
   client with the build slower than the wait, global pool 4: the other client's `/v1/status` is 200 and its own
   anchor-proof request is served; the flooder gets at most 8 through. Negative control in the test: no per-client
   cap and an anchor pool of 8: the other client's anchor-proof request is `busy`. Mutant (run by hand): anchor
   waits back on the global permit: the test fails ("another client's status ... busy"). `in_flight_cap_per_client`.
2. **S2, `verifyRound` overflow on a lying indexer's round number.** `indexer.rs` `classify` and the closing check,
   `rounds.rs` facts and draw checks, `api.rs` (`r + 1`, `mk.round + 1` → `checked_add`), `rounds.rs` caller
   figures (`caller.value + refund`, `n * r_res` checked), `vault.rs` settle case (`refund × dep_rem` checked),
   `tickets.rs` (`h - 1` saturating). The rest of the post-F1 arithmetic works on states bounded by `check_*`
   (vault, position, marker, controller, drawer ranges) or on u128/i128 of u64 values. **W3 extended**
   (`crates/web/tests/w3_page.rs`, debug, seeded, `W3_ITERS`): `checkIndexerRound`, `verifyRound` (closing,
   drawn), every `parseIndexer*`, `anchorProofX`, `vaultView`, `withdrawChoice`, `depositFigures`,
   `withdrawFigures` (partial, whole), `settleFigures` (3 modes), `callerFigures` (close, draw, void),
   `absorbablePools` (draw, void), `claimFigures` (3), `returnFigures` (2), `ticketCard`, `backupRead` (checksum
   recomputed so the record checks run, and raw text), `checkCopy`, `mergeCopy`. Regression
   `a_round_at_i64_max_is_an_error_not_a_panic`; mutant (original `x.round == r + 1` restored by hand): it fails
   with "attempt to add with overflow". Negative control `the_harness_counts_panics`.
3. **S3, `check:mainnet` could scan a stale mainnet wasm.** `scripts/check-mainnet.mjs` rebuilds
   `src/wasm/mainnet` on every run (cargo rebuilds what changed; wasm-bindgen writes the package afresh) and fails
   as stale if the package's `web_bg.wasm`, `web.js` or `web.d.ts` is older than the run's start (the control);
   the positive control (markers present in the normal build) stays.
4. **S4, the hosted indexer followed whatever node the resolver handed out.** `follow.rs`:
   - the followed node is **pinned per run** (the resolver's first pick; reconnects go back to it);
   - a **witness node** (`--witness <url|resolver|off>`, `INDEXER_API_WITNESS`; the env example sets `resolver`):
     each batch is asked of the witness from the same chain block at Low verbosity (`view_of_response`: header
     hash, transaction IDs, signature scripts) and only the common prefix is persisted (`agree`,
     `apply_witnessed`): a reorg only one reports, a tip fork or a witness behind takes less; one chain block with
     different transactions is `DIVERGED`: nothing persisted, `/health` 503 at once until they agree; no progress
     for the staleness limit: 503. The witness must be another host (for `resolver`, up to 8 picks); on the
     private simnet the same node is accepted. A testnet-10 instance refuses to start with `--witness` unset
     (`off` accepts one node).
   - Tests (`hosted`): `the_witness_confirms_the_common_prefix_only` (prefix, behind, ahead, tip fork, other
     chain, other transaction, missing transaction, other signature script, a removal only one reports);
     `a_lying_node_cannot_write_the_store_past_the_witness` on the real indexer over a store: an unconfirmed chain
     leaves the position, `index.json` and the log byte-identical; a divergence too; a confirmed prefix is
     persisted and a restart resumes there; **negative control**: without a witness the same lie is persisted.
     `security_review_flags`. On simnet: `simnet follow-check` now follows with a witness (a second connection to
     the dev node), so a real node's Low answers are shown to agree with its Full ones; `/health`'s local detail
     names the witness. A lying second node could not be run on the simnet (one node per process; a second
     in-process core was not tried).
   - Owner write-up: `docs/launch/indexer-server.md` "Which nodes the server reads" (below).

### Notes fixed

- **Backup format** (`crates/web/src/backup.rs`): `kaspalionare-backup/2` has no `pending` field (code and docs
  now agree: no pending purchase, claim or return is exported); `kaspalionare-backup/1` still imports (its
  checksum checked over the format-1 layout, its pending records never imported). `check_pending` is gone.
  Tests `f4_tickets::backup_format_2_and_format_1_files` (format 2 written, a format-1 file with a pending
  purchase imports without it, a broken format-1 checksum refused), damaged cases "a pending field".
- **Ticket key tied to its outpoint:** every file ticket is filed under its ticket entry's outpoint (the writer
  re-keys, so a ticket after a claim is written under its current entry; a second record of the same outpoint is
  left out by name); the reader refuses a ticket not filed under its outpoint, or twice; a format-1 file is
  re-keyed. The page keeps an imported ticket only if no ticket it follows has that outpoint
  (`src/store/backup.ts`). Tests: damaged cases "a ticket filed under another outpoint", "a ticket twice";
  Vitest `an imported ticket is keyed by its outpoint and never kept twice` (control: an empty store keeps it).
- **Bucket pruning bounded** (`limits.rs`): the sweep runs at most once a second, at most 65,536 buckets, new
  addresses past that share one overflow bucket. `hosted::bucket_memory_and_work_stay_bounded` (70,000 addresses
  in one instant: one sweep, the cap held, the overflow shared; control: a later sweep frees the idle buckets).
- **`/health`** answers a remote client (or one behind the proxy) only `{"ok":…}`; the detail (store and node
  errors, the followed node and the witness) only on loopback without `X-Forwarded-For`, and in the log (store
  errors are now logged when they change). `hosted::health_detail_only_on_loopback` (both client modes; control:
  the detail does carry the path and the URL token the public answer hides). `/v1/status`'s `error` field (part
  of the v1 contract) still carries the store error string: left as is, noted below.
- **Permissions-Policy** on both `vercel.json` rules (camera, microphone, geolocation, payment, usb, serial, hid,
  bluetooth, midi, accelerometer, gyroscope, magnetometer, display-capture denied); `figure.test.ts` and
  `check:csp` check it (served on the page and the figure, equal; the page's own policy denies the three).
- **Figure CSP `sandbox allow-scripts`** (also in `docs/figures/draw-cabinet/build.mjs`, which writes it);
  `check:csp` opens the figure directly: its origin is `null` (control: the site's page has its origin), and the
  framed tour still runs.
- **Docker/compose images pinned by digest** (rust:1.94.0-bookworm, debian:bookworm-slim, caddy:2; index
  digests read from Docker Hub 2026-10-28; how to update in the files). `hosted::deploy_files_parse_and_pin_their_images`
  (the env example parses and sets a witness; every image line pinned; control: an unpinned line is found).
- **`web/.env.production`**: committed on purpose (every value public), as `site-deploy.md` says; `web/.gitignore`
  now ignores Vite's `.env.local` and `.env.*.local` overrides so nothing local changes a build.
- **Caddy**: `max_header_size 16KB` in the Caddyfile's global options (Caddy's default is Go's 1 MB).
- **KasWare account switch**: the adapter reads `getAccounts()` before each signature and refuses
  `wallet_account` (new code 56, "The wallet switched to another account; connect it again.") instead of a
  later `signature_invalid`. Vitest `refuses to sign after the account was switched in KasWare` (nothing sent
  to KasWare; control: switched back, it signs). Kaspire (untested, no `getAccounts` in its API) unchanged.
- **Playwright traces**: the steps that show or type a quick wallet's key (the export and every import, in
  `play.spec.ts` and `tickets.spec.ts`) run inside `untraced(ctx, …)` (`e2e/helpers.ts`): the context's trace chunk
  is dropped before them and a new one starts after, so a trace kept on failure never holds the key; screenshots and
  video are off by default; the backup's no-key check no longer prints the key on a failure.
- **Copy pass** (light process): Play's closed card "Sales closed" (status) with the Rounds link; CallerPanel row
  "close() after" `draw()`, "Back at" `draw()`, "Record back at" `Settlement`; VaultRequestRows "Back at"
  `Settlement`; SettleRows "Shares left". The S7 key line and How it works untouched. The Rust `sales_closed`
  error message is unchanged (an error).

### Runs (one cargo process at a time)

- `cargo test -j 4 --no-fail-fast` (debug, default members): **1,111 passed, 0 failed, 4 ignored**; the only failing
  target is `spike_vdf`'s known debug stack overflow (D113). Goldens unchanged (`f1_bytes` passes).
- The web test command: `cargo test -j 4 -p web --release --test api --test f4_tickets --test f4_review --test
  f5_rounds --test review_f5 --test f6_vault`: 20 (3 ignored) + 6 + 7 + 3 + 6 + 9 passed; **W3 page** (debug,
  `w3_page`): 6 passed, 0 panics in about 7,500 parsed mutations; **W1**: 1 passed (97 library digests, 18 API
  builds, 291 decode/verify calls, 14 signed in wasm); **W4**: 1 passed (119 s).
- indexer-api: `--features doctored --lib --test api --test doctored --test review_f3 --test review_f4 --test
  hosted`: 6 + 6 + 4 + 2 + 1 + **15** passed; `--test production`: 1 passed (100 s).
- `cargo test -j 2 -p simnet --release`: 15 passed. `cargo run -j 2 -p simnet --release -- follow-check`: all 8
  FOLLOW-CHECK lines ok, now with the witness ("pinned to ws://127.0.0.1:18610; witness ... confirms every batch").
- `cargo test -j 1 --release -p tn10`: 27 passed.
- `npm test`: 30 files, **221** tests passed (219 before: the KasWare account test and the import-by-outpoint test).
- `npm run e2e` (fresh dev run): **23 passed, 1 skipped** (the void test, on request only). A first attempt found
  that Playwright refuses `test.use({ trace })` inside a describe group (hence `untraced`), and a second that the
  F4 import step addressed T1's card by its purchase key, which the outpoint filing changes for a ticket after a
  claim (the test now reads the imported card's key and checks it is not the purchase's). Ports 18610, 8790, 8791,
  5173 free afterwards; this session's four simnet data directories (about 400 MB) deleted.
- `npm run build`: ok (check-bundle and its control). `npm run check:mainnet`: ok, "src/wasm/mainnet rebuilt from
  the current sources by this run". `npm run check:csp`: 32 checks ok (27 before; new: the Permissions-Policy on the
  page and the figure, the page's own policy, the figure's sandbox, the figure opened directly in an opaque origin).
  Ports 4173 and 4174 free.
- Mutants run by hand and restored: `x.round == r + 1` (W3 regression fails: "attempt to add with overflow");
  anchor waits on the global permit (S1 test fails: the other client's status `busy`).

### Proposed DECISIONS rows (F6 security review)

- **Hosted indexer: pinned node and a witness.** The one-process API keeps the node it started with for the whole
  run and persists a batch only as far as a second, independent node reports the same chain blocks and accepted
  transactions (ID and signature scripts); a block both report differently stops persisting (`/health` 503).
  A testnet-10 instance must set `--witness` (`off` accepts one node). Reason: a bad public node could otherwise
  write false history into the store and block the page until a manual re-index.
- **Per-client in-flight cap and an anchor-proof pool.** At most 16 requests of one client at once; anchor-proof
  requests wait in their own pool of 32. Reason: S1, a draw-time flood from one client could hold all 64 permits.
- **Backup format 2.** No pending records; each ticket filed under its outpoint; format 1 still read (pending
  left out, re-keyed). Reason: the docs' promise and an unbound pending transaction; a key the file chooses.
- **Code 56 `wallet_account`.** KasWare's account is read again before every signature.
- **Response headers:** a `Permissions-Policy` denying device features on both rules; the figure rule's CSP has
  `sandbox allow-scripts`.
- **`/health` detail only locally.** Remote clients see `{"ok":…}` only.

### Owner choices (none decided)

1. **Which nodes the hosted API reads** (`docs/launch/indexer-server.md`, "Which nodes the server reads"):
   - **A, two public nodes must agree (built, the default):** no extra cost or work; the trust is that two public
     node operators do not lie together; a bad node can only make the API stop and wait, the page then shows it
     as unavailable (players with a local copy can still claim). No money is at risk in any option: every browser
     re-checks against its own node.
   - **B, your own testnet-10 node on the server**, a public node as witness: you trust yourself; a much larger
     server (not measured: our private test node reached about 6.6 GB of memory after 864,000 blocks, about a
     day of testnet-10), several times the €6 plan, plus keeping the node updated. We would measure it before
     you pay.
   - **C, one public node unchecked** (`--witness off`): no extra cost; a bad node can block the page until the
     store is rebuilt by hand.
2. **`/v1/status` `error`** shows the store's error text to anyone (it can contain a file path). Hiding it changes
   the page's API contract (the page only logs it); the choice is to keep it (the path says only where the store
   lives on the server) or to replace it by a fixed word in a follow-up.

### Left

- `npm run e2e:void` not run (no draw or void code changed).
- Kaspire's account is not re-read (untested wallet; its API has no `getAccounts` in our adapter).
- The hosted API's own HTTP header limit is hyper's default; Caddy's 16 KB cap applies in front of it.

### Files

- indexer API: `src/server.rs`, `limits.rs`, `follow.rs`, `config.rs`, `routes.rs`, `tests/hosted.rs`,
  `deploy/` (Dockerfile, compose.yaml, Caddyfile, indexer-api.env.example).
- web crate: `src/indexer.rs`, `rounds.rs`, `api.rs`, `vault.rs`, `tickets.rs`, `backup.rs`; tests `w3_page.rs`
  (new), `f4_tickets.rs`, `f4_review.rs`; the probe deleted. lottery: `src/web/mod.rs` (code 56),
  `tests/f1_web.rs`. simnet: `src/dev.rs` (follow-check with a witness).
- web: `src/wallet/kasware.ts`, `postsign.test.ts`, `src/store/backup.ts`, `src/tickets/tickets.test.ts`,
  `vercel.json`, `scripts/check-csp.mjs`, `check-mainnet.mjs`, `src/screens/how/figure.test.ts`, components
  `CallerPanel`, `SettleRows`, `VaultRequestRows`, `screens/play/BuyCard` (and their tests), `e2e/play.spec.ts`,
  `tickets.spec.ts`, `vault.spec.ts`, `helpers.ts`, `.gitignore`, `.env.production.example`, `CLAUDE.md`.
- docs: `docs/figures/draw-cabinet/build.mjs`, `docs/launch/indexer-server.md`, `site-deploy.md`, `CLAUDE.md`.

## F6 part C (stage agent, 2026-10-27): deploy preparation (nothing deployed or launched; not committed)

Nothing was sent to testnet-10. The only network contact was read-only: the fee estimate, resolver lookups,
`bundle-verify` of the M8 bundle, `tn10 info`, and the container following a public node. No Vercel command was
run. No covenant change; the goldens are untouched.

### What was prepared

1. **The hosted indexer API, one process** (`crates/indexer-api`).
   - **`--node <wss-url|resolver>`** (`follow.rs`) runs `tn10/src/indexer.rs` and `tn10/src/anchor.rs`, compiled
     in by `#[path]` as simnet does. The M8 indexer follows the node into the store, persisting each batch before
     the next call. The API follows that store, as in the store-only mode.
   - **The anchor proof** before the draw is built from the node behind `limits::CachedAnchor`: one build per key
     at a time, kept 60 s, a failure kept 5 s, a slow build never cancelled by a client. A request waits at most
     the deadline minus 1 s, then gets "being built".
   - **Network guard:** the node must report the bundle's network, be synced and have a UTXO index. URLs on
     mainnet ports, or containing "mainnet", are refused.
   - **Node failures:** retries with backoff, and a new connection after two failures in a row.
   - **Shutdown:** SIGTERM or Ctrl-C ends it after the current batch, and a restart resumes from the log.
   - **Per-client token buckets** (`limits.rs`): 120 burst, 20 a second, IPv6 by /64, over that `busy` with
     `Retry-After: 1`. `--client-ip forwarded` trusts the last X-Forwarded-For entry only from a loopback peer.
     The dev run and the doctored binary turn the limit off.
   - **`GET /health`:** 503 when the store does not apply, when the node has not been read within
     `--stale-ms` (120 s), or at once when a retry cannot fix the follower's error (a pruned resume block).
   - New flags in `config.rs`; the page's types are unchanged (the busy code is the existing one).
   - **Packaging** in `crates/indexer-api/deploy/`:
     - a Dockerfile (rust:1.94.0-bookworm, `--locked`, `-j 2`, an `export` target for the bare binary, a
       non-root bookworm-slim runtime) and its `.dockerignore`;
     - `compose.yaml` (the API and stock Caddy on host networking);
     - `Caddyfile` (automatic HTTPS, GET/HEAD/OPTIONS only, 1 KB bodies, X-Forwarded-For, access logs);
     - a systemd unit (DynamicUser, sandboxed, 1 GB memory cap);
     - `indexer-api.env.example`.
   - Guide: `docs/launch/indexer-server.md`, with three hosting options and their prices.
2. **Site production configuration.**
   - `web/.env.production.example` (spec 6.5).
   - **The CSP is narrowed.** `connect-src` is now `'self'`, the indexer origin, and the resolver's four domains
     for https and wss (`*.kaspa.stream`, `*.kaspa.red`, `*.kaspa.green`, `*.kaspa.blue`), or `VITE_NODE_URL`'s
     origin instead of the four domains.
   - `scripts/csp.mjs`: `npm run csp:write` writes it into `vercel.json`, and `--check` compares.
   - `vercel.json` now holds the placeholder `https://indexer.invalid` (the origin `check:csp` builds with) until
     the owner picks a host.
   - The resolver's host list is compiled into the v2.1.0 client: `rpc/wrpc/client/Resolvers.toml` and
     `src/resolver.rs:126-143` at the pinned revision.
   - The nodes the resolvers hand out are not fixed in code. On 2026-10-27, 32 requests to the 16 resolver names
     returned only `muon-10.kaspa.blue`, `vector-10.kaspa.green` and `quark-10.kaspa.red`, with
     `access-control-allow-origin: *`.
   - **The trade-off** (narrow, wide, or one fixed node) is written up in `docs/launch/site-deploy.md` for the
     owner.
   - **`check:csp` gained 9 checks:** no scheme-wide source, and in the page and the worker an unlisted https or
     wss host is refused while a resolver host and the indexer origin are allowed. The check page is now built
     against the indexer origin `vercel.json` lists, and its node URL is an unused name under a resolver domain.
3. **The deploy artifact.**
   - **Decision:** build and check locally, then `vercel build` (which only copies the checked `dist`) and
     `vercel deploy --prebuilt`.
   - `vercel.json` gained `framework: null`, `outputDirectory: dist`, and install and build commands =
     `scripts/vercel-prebuilt.mjs`. That script builds nothing and fails without a built `dist`, so Vercel's own
     builders can never ship an unchecked build.
   - **`npm run check:deploy`** (`scripts/check-deploy.mjs`) runs on the artifact itself:
     - the configuration is complete and not a placeholder;
     - `dist` was built with it;
     - `connect-src` equals `csp.mjs`'s list;
     - `check-bundle`;
     - **W4 on the served `.wasm` after `wasm-opt`**: loaded in Node with the release glue, it starts on
       testnet-10 and refuses simnet, mainnet, a `kaspasim:` address and a `kaspasim:` change; it has the quick
       wallet and no fixtures export; control: the dev package starts on the simnet;
     - `-- --vercel` also checks that `.vercel/output/static` equals `dist` and that `config.json` carries both
       header rules.
   - Guide: `docs/launch/site-deploy.md`.
4. **The beta instance plan.**
   - `tn10 admin-keygen` has run: `TN10_ADMIN_KEY` is in the git-ignored `.env` (checked with
     `git check-ignore`), never printed; admin hash `d8b95fa1…684ad529`.
   - **`tn10 beta genesis --profile <fast|steady> [--fee-address] [--jackpot] [--vault] [--s-c] [--s-d] (--dry-run | --submit)`**
     runs campaign C0 and C1 with `GenesisKnobs`:
     - the windows at their floors (seed_offset 600, draw_window = VOID_MIN, claim_window = CLAIM_WINDOW);
     - round_len 1 h or 24 h, with d_min half of it;
     - the separate admin key.
   - `admin_guard` refuses `ADMIN_SK` in every C1. `bundle` and `bundle-verify` refuse a bundle with the default
     admin hash (the M8 bundle passes: `e913e8dc…`). A live run needs `--submit` and refuses a second instance.
   - Both profiles were dry-run (all checks ok). Genesis locks 137.49 test-KAS plus a 0.46 KAS fee. Larger
     jackpots and 3 % caller shares also validate.
   - Guide: `docs/launch/beta-plan.md`, with the options.
5. **The journeys:** `docs/launch/beta-journeys.md`. A step table per journey for KasWare and the quick wallet,
   giving the buttons to press and the txid to log. It also covers the two old-M8 checks, the return_deposit and
   the absorbing draw.

### Runs (one cargo process at a time)

- **`cargo test -p indexer-api --test hosted`: 7 passed.** It was written before the server code. Mutants:
  - X-Forwarded-For trusted from any peer: `client_address_rules` and `per_client_limit_in_the_router` fail;
  - no single flight in the cache: `anchor_cache_bounds_node_work` fails.
- **`cargo test -j 4 -p indexer-api --features doctored --test api --test doctored --test review_f3 --test review_f4 --test hosted`:**
  6 + 4 + 2 + 1 + 7 passed.
- **`--test production`:** 1 passed (163 s).
- **`--lib`:** 6 passed (tn10's indexer tests, compiled in).
- **`cargo run -j 2 -p simnet --release -- follow-check`: all 8 FOLLOW-CHECK lines ok.**
  - A fresh store at the node's genesis caught up (14 log lines).
  - 13 endpoints answered byte for byte as the dev API, round 2's 272-byte anchor proof built from the node
    included.
  - `/health` 200.
  - 8 concurrent proof requests got the same proof.
  - Stopped by its signal and restarted without `--start`: the same answers.
  - Control: the store-only server has no proof for the closing round.
  - Control: a testnet-10 instance refuses the simnet node.
  - About 25 s; ports 18610, 8790 and 8791 freed.
- **`cargo test -j 2 -p simnet --release`:** 15 passed.
- **`cargo test -j 1 --release -p tn10`:** 27 passed, including `beta_profiles_pass_genesis_validation` and
  `admin_guard_refuses_the_public_default_key`.
- **`tn10 beta genesis --profile fast|steady --dry-run`:** every C0/C1 check ok. Variants (J 50/V 300,
  J 100/V 600, s_C = s_D = 300, J 1000/V 6000) validate. The live path without `--submit` is refused.
- **`tn10 bundle-verify docs/launch/tn10-m8.json`** (read-only): PASS, admin hash not the default's.
- **Docker** (Docker Desktop, started for this and stopped after):
  - `docker build … --target export` built the Linux x86-64 binary in 4 min 54 s (`-j 2`).
  - The `run` image followed a public node on a copy of the M8 store: 107.5 MiB of memory, `/health` and
    `/v1/status` answered, exit code 0 on SIGTERM.
  - It found the M8 resume block pruned. That led to the fix: `/health` is now 503 at once on an error a retry
    cannot fix (test added).
- **`cargo test -j 4`** (debug, default members): **1,111 passed, 0 failed, 4 ignored**. The only failing target is
  `spike_vdf`'s known debug stack overflow (D113). The goldens are unchanged (`f1_bytes` passes).
- **The web test command:**
  - `cargo test -j 4 -p web --release --test api --test f4_tickets --test f4_review --test f5_rounds`: 20 (3
    ignored) + 5 + 7 + 3 passed;
  - W1 (`w1_wasm`): 1 passed;
  - W4 (`w4_mainnet`): 1 passed (102 s).
- **`npm test`:** 30 files, 219 tests passed; typecheck, ESLint and Prettier clean.
- **`npm run build`:** ok. check-bundle and its control pass.
- **`npm run check:mainnet`:** ok.
- **`npm run check:csp`:** 27 checks ok, 9 of them new. Ports 4173 and 4174 freed.
- **`npm run check:deploy`** on that build: the served-wasm W4 checks and the dev control are all ok, and it
  **refuses** the build because the configuration is incomplete (no indexer URL, covenant ID, bundle hash or
  referral), as intended until the owner chooses.
  - Rehearsal with a filled configuration (the dry-run covenant ID and bundle hash, a made-up indexer domain,
    `csp:write`, `vite build`): every check ok. `vercel.json` was restored afterwards.
  - `-- --vercel` on a synthetic `.vercel/output`: the matching one passed; an extra file and a changed CSP were
    each refused.
- **No e2e:** shared page code is unchanged (`config.ts`: a comment only).

### Owner choices (none decided)

1. **Profile** (`docs/launch/beta-plan.md`):
   - **A, steady (recommended):** one round a day. One Close and one Draw press a day, about 0.65 test-KAS a day,
     and up to about 100 test-KAS of reserves in flight.
   - **B, fast:** hourly rounds, 48 presses a day, about 15.6 test-KAS a day.
   - **C:** A with a bigger jackpot (50 or 100 KAS) or 3 % caller shares.
2. **Addresses:** the owner's `kaspatest:` referral and fee addresses.
3. **Indexer host** (`docs/launch/indexer-server.md`):
   - Hetzner CX23, €5.99 a month (often out of stock);
   - DigitalOcean, $12 a month for 2 GB ($6 for 1 GB);
   - Fly.io, about $7.45 a month (config not written).
   - Plus a domain for the API.
4. **CSP width:** narrow (built), wide, or one fixed node.
5. **Vercel plan:** Hobby, free if non-commercial, or Pro at $20 a month.
6. **Go-aheads:** the genesis transaction (about 138 test-KAS), and the deploy.
7. **For M8 check M.1:** whether to import the M8 campaign key into a throwaway browser profile's quick wallet
   (it holds about 4,789 test-KAS), or to move the balance first.

### Proposed DECISIONS rows (F6 part C)

- **Hosting the indexer API.** For the beta it runs as one process (`indexer-api --node`): the M8 indexer
  follows a public testnet-10 node into the store, and the anchor proof is built from the node behind a cache.
  Per-client limits are built in, because stock Caddy has no rate limiter; `X-Forwarded-For` is trusted only
  from a loopback proxy. It is packaged as a Docker image or a systemd binary behind Caddy. Reason: the page
  can draw on testnet-10 only with a node-backed proof (F5); the server stays keyless and untrusted.
- **The site's CSP `connect-src` is narrowed** to the indexer origin plus the v2.1.0 resolver's four domains,
  or the fixed node's origin. If a node domain is not listed, the page shows the node as unavailable (it does
  not fail open). Reason: it limits where an injected script could send data.
- **The site is deployed prebuilt.** It is built and checked locally, then `vercel build` and `vercel deploy
  --prebuilt`; `vercel.json`'s commands build nothing. `check:deploy` runs W4 on the served `.wasm` after
  `wasm-opt`, and compares `.vercel/output` with `dist`. Reason: Vercel cannot build the wasm, and the checks
  must cover the files that ship.
- **The beta's genesis path is `tn10 beta genesis`** only, with a separate `TN10_ADMIN_KEY`. The public default
  admin key is refused at genesis and in `bundle` and `bundle-verify`. The windows are at their floors, and
  d_min is half a round. Reason: F1 re-review N5; the fastest rounds the floors allow.
- **`/health`** is outside `/v1/` and not part of the page's contract.

### What is left (security review and beyond)

- **For the security review:**
  - the follower code compiled into the API (`tn10/src/indexer.rs`; untrusted node answers);
  - the limits (bucket memory pruning; trusting X-Forwarded-For only from loopback);
  - the anchor cache (keys come only from a closing round in the store);
  - the health detail strings;
  - the Docker and systemd hardening;
  - the CSP list and `check:deploy`'s config rules.
- **Not run on a real host:** the compose file and the Caddyfile. The Dockerfile build and the `run` image were
  run locally (Docker Desktop). The container followed a public node from the M8 store copy, used 107.5 MiB,
  answered `/health` and `/v1/status`, and stopped with exit code 0 on SIGTERM.
- **The M8 store's resume block is pruned** on the public node (DAA 605,601,500, seen 2026-10-27), so the M8 checks
  M.1 and M.2 need `tn10 campaign reanchor` first.
- **Re-anchoring a beta store:** `campaign reanchor` works only on `tn10-data/current` (M8). A `--beta` option is
  not written.
- **The Vercel side, unverified:**
  - whether `vercel build` copies `vercel.json`'s headers into `.vercel/output/config.json`
    (`check:deploy -- --vercel` decides it);
  - `vercel pull` needs a linked project;
  - pin a Vercel CLI version for `npx vercel`.
- **The quick wallet's Import format** for a raw testnet key (journeys M.1) has not been checked.
- **The explorer URL:** `https://explorer-tn10.kaspa.org` answered HTTP 402 to a plain request; the owner picks
  one and checks it by hand.
- **The figure's CSP and the live headers** are checked after the first deploy (site-deploy.md "After
  deploying").

### Files

- **Indexer API, new:**
  - `crates/indexer-api/src/follow.rs`, `src/limits.rs`;
  - `tests/hosted.rs`;
  - `deploy/` (Dockerfile, Dockerfile.dockerignore, compose.yaml, Caddyfile, indexer-api.service,
    indexer-api.env.example).
- **Indexer API, changed:**
  - `src/lib.rs`, `server.rs`, `config.rs`, `routes.rs`;
  - `bin/indexer-api.rs`, `bin/indexer-api-doctored.rs`;
  - `Cargo.toml`.
- **simnet:** `src/dev.rs` (`follow_check`; per-client limit off) and `src/main.rs` (the `follow-check` command).
- **tn10:** `src/campaign.rs` (`GenesisKnobs`, `admin_guard`, the separate admin key, C1 checks, 2 tests) and
  `src/main.rs` (`admin-keygen`, `beta genesis`, `page_config`, the default-admin guard in `bundle` and
  `bundle-verify`, `bundle --beta`).
- **web, new:**
  - `scripts/csp.mjs`, `scripts/check-deploy.mjs`, `scripts/vercel-prebuilt.mjs`;
  - `.env.production.example`.
- **web, changed:**
  - `scripts/check-csp.mjs`, `vercel.json`, `package.json` (`check:deploy`, `csp:write`), `.gitignore`
    (`.vercel/`);
  - `src/config.ts` (a comment only);
  - `CLAUDE.md`.
- **docs:**
  - `docs/launch/indexer-server.md`, `site-deploy.md`, `beta-plan.md`, `beta-journeys.md`;
  - `CLAUDE.md` (commands).

## F6 part B (stage agent, 2026-10-27): S6 How it works

Light process (DECISIONS 2026-10-26): web tests, no adversarial review; the owner reads the page. The one money-adjacent rule held: every number of the example round comes from the Rust `lottery` code. No covenant change; goldens unchanged; no `lottery` library change (a test file only).

### D138's three conditions
1. **Real numbers.** `crates/lottery/tests/f6_figure.rs` builds the example round with the library itself on the `CloseCase` instance values (margin 15,000): `close_math`, the web layer's `referral`, `seed_numbers` (an example covenant ID and anchor hash), `win_counts`, `pool_set`, `vault_settle`, `boost_after`, `fix_jackpot` (J(7) and J(8)), `convert`, `settle` per request, `claim_amount`, `round_amounts` for one ticket; only the inputs are invented (counts per shard, 260 referrals, vault 8,000,000 KAS / 7.75e14 shares, four requests, who holds the winning combinations). It writes `web/src/screens/how/cabinet-model.json` (sompi and share counts as decimal strings). `figure_model_is_current` fails when the committed file differs (control: one changed digit → FAILED); `-- --ignored write_figure_model` rewrites it. The JS port `model.mjs` is deleted; `build.mjs` only converts units (sompi → KAS for display). The figure carries the model's sha256; `figure.test.ts` checks it equals the committed model's (control: the same changed digit → 2 Vitest failures). The page imports the same JSON (types inferred from it, no hand-written mirror).
   - The example at margin 15,000: J(7) 688,561.09 KAS (the base 800,000 is cut by the whale check), m(8) = 12, winning numbers 01 11 13 19 30 + 08 (from the example anchor hash), share price 1.03225806 → 0.94167387 sompi/share, the vault ends exactly at L(8) = 900,622.71 KAS (the large withdrawal is capped), 4,330,923,170,847 shares wait for round 8. The figure now shows raw share counts (the page's convention), not shares ÷ 10^8.
   - Outdated text fixed in the figure: the "TEST SEED" plaque reads "ANCHOR X"; the draw stop says X is the first chain block 600 DAA after `close()`, its hash picks the numbers, and states the open withholding item; round numbers, shard count, shares and the 600-DAA delay come from the model.
2. **Rebuildable from a clean clone.** The three kit modules `build.mjs` imports (`iso-kit.mjs`, `lathe.mjs`, `tube.mjs`, unchanged, MIT; `LICENSE` and a README with their sha256) are committed in `tools/anatomy-kit/`; `inline-kit.mjs`'s one function is two lines in `build.mjs`. The committed figure rebuilt byte-identically with them before any change. `node docs/figures/draw-cabinet/build.mjs --all` (Node only, from the repository root) writes the 4 docs variants and the 2 embedded ones. `.claude/skills/*` untouched (the other figure folders still import the skill; they can switch to `tools/anatomy-kit/` when they are embedded).
3. **Embedded in isolation.** `web/public/figures/draw-cabinet-tour{,-light}.html` (`--embed`: the page's tokens via `docs/figures/draw-cabinet/embed.css`, the card's border and radius, no caption (the page text replaces it), keys kept, its height posted to the parent). `FigureFrame` shows it in an iframe with `sandbox="allow-scripts"` (opaque origin: the figure cannot reach the page, its IndexedDB or the quick wallet's key), a `title`, `loading="lazy"`, only when S6 renders, the theme's file (`key={theme}` reloads it on a switch); the height only from that frame's own message, bounded 240–3000 px (`figureHeight`). `vercel.json` has two disjoint rules: the site (`/((?!figures/).*)`: adds `frame-src 'self'`, keeps `frame-ancestors 'none'` and DENY) and the figures (`/figures/(.*)`: `default-src 'none'`, the module script and both style elements by sha256, style attributes `'unsafe-inline'` (stroke widths, `display`), `frame-ancestors 'self'`, SAMEORIGIN). The build writes the hashes into `vercel.json` and normalises the page to LF; `.gitattributes` pins `web/public/figures/*.html -text`, so a Windows checkout and Vercel hold the hashed bytes. `vite preview` applies vercel.json per path (`vercelHeaders`, replacing `siteHeaders`). Reduced motion: the tour holds on its first stop (e2e). At 360 px the tour puts its callout under the drawing (its own phone layout).

### The page (`web/src/screens/HowItWorks.tsx`)
Cards: the figure; One round, four calls (anyone can run close/draw; the reward is `s_C`/`s_D` of sales, usually below the fee on the beta, so the owner runs them); Where 1.00 KAS goes (`MoneySplit`: a decorative bar plus a named row per part with its exact amount; the example round's sales, house share and caller reward); Why sales stop before the draw; Where the numbers come from (the open withholding item in plain words: the anchor block's miner could throw it away and try again; the fix is required before real money and not built; "not fully fair against such a miner"); Prizes, deposits and voids (at least 7 days, shared prizes, P_MIN accrual, deposits, voids: no winners, prices not refunded, Return deposit from 14 days after the deadline); What the page checks (indexer untrusted, Verify in this browser, Provisional, Unverified); Backing the vault (requests irrevocable, next draw's price, the L cap, refund mode, and the owner's decision: positions found through the indexer only, a left-out position safe on chain and withdrawable with another indexer or the repository's tools); A testnet beta (valueless coins, not audited, the open item). Durations and percentages printed from the model's constants (`daaSpan`, `percentOfBp` in `format.ts`). Never "provably fair" or "guaranteed" (unit and e2e checks). `Placeholder.tsx` deleted (no screen uses it). Catalogue: Money split.

### Runs
- `cargo test -j 4 -p lottery --test f6_figure`: 2 passed, 1 ignored (the writer). Full `cargo test -j 4 --no-fail-fast` (debug, default members): **1,111 passed, 0 failed, 4 ignored** (part A review: 1,109 and 3; +2 and the writer from `f6_figure`); the only failing target is `spike_vdf`'s known debug stack overflow (D113).
- `npm test`: tsc, ESLint (0 errors), Prettier clean; Vitest **30 files, 219 tests passed** (part A review: 28, 205): `src/screens/how/figure.test.ts` (7: the model hash in both figures, one module script and no network, script and styles hashed in the figure CSP, frame rules, rules disjoint, embed colours = tokens), `how.test.tsx` (5), `format.test.ts` (+2).
- e2e (a dev run started by Playwright): `npx playwright test e2e/how.spec.ts e2e/smoke.spec.ts` **8 passed** (how 4: loaded only on S6, sandboxed; dark and light at 360/390/430/1280 px with axe (the figure frame included; the first run found the light figure's corner and key text below AA, fixed in embed.css) and named-and-tagged; reduced motion holds still). Full `npm run e2e` not run (no shared flow touched; App only routes S6).
- `npm run build` (placeholder indexer URL and covenant ID): ok; index JS 514.8 kB (part A review 503.7 kB), the two figures 629 kB each in `dist/figures/`; check-bundle and its control ok; `npm run check:mainnet` ok; `npm run check:csp` **18 ok lines** (8 new: the figure's headers, its script running in the sandboxed frame and sizing it, an inline script refused there, no CSP report, another origin cannot frame it). Control: a wrong script hash in vercel.json → 2 FAIL (the figure does not run).
- Screenshots for the owner: `web/test-results/shots/how-{dark,light}-{390,1280}.png` (full page; the fixed bottom bar shows mid-page in a full-page shot) and `how-{dark,light}-{390,1280}-figure.png` (the figure alone, mid-tour).
- Dev runs stopped; ports 18610, 8790, 8791, 5173, 4173-4174 free.

### Files
- Rust: `crates/lottery/tests/f6_figure.rs` (new).
- Figure: `docs/figures/draw-cabinet/build.mjs`, `live.js`, `embed.css` (new), the 4 built HTML; `model.mjs` deleted; `tools/anatomy-kit/` (new: 3 modules, LICENSE, README); `.gitattributes`.
- Page: `src/screens/HowItWorks.tsx`, `src/screens/how/{cabinet-model.json, figure.test.ts, how.test.tsx}`, `src/components/{FigureFrame.tsx, figureHeight.ts, MoneySplit.tsx}`, `src/app/App.tsx`, `src/lib/format.ts` (+test), `src/dev/Catalogue.tsx`, `src/screens/Placeholder.tsx` (deleted), `public/figures/` (new), `vercel.json`, `vite.config.ts`, `scripts/check-csp.mjs`, `e2e/how.spec.ts` (new), `.prettierignore`; `web/CLAUDE.md`, root `CLAUDE.md`.
- Not touched: covenants, templates, goldens, `Cargo.lock`, `package-lock.json`, `.claude/skills/*`, the other figure folders, `docs/research/seed-fairness-brief.md`, `.agents/`, `skills-lock.json`, `docs/figures/draw-cabinet/shots/` (the owner's old screenshots still show "TEST SEED").

### Proposed DECISIONS rows (F6 part B)
- (a) **The How it works example round is generated by the lottery code** (`crates/lottery/tests/f6_figure.rs` → `web/src/screens/how/cabinet-model.json`, checked by `cargo test`), read by the figure build and the page; the JS port of `game.rs` is deleted (D138 condition 1).
- (b) **The anatomy kit's three modules are vendored in `tools/anatomy-kit/`** (MIT, unchanged, hashes in its README); the figure builds with Node alone (D138 condition 2).
- (c) **Figures are framed in isolation**: their own files under `/figures/`, a sandboxed iframe (`allow-scripts` only), their own CSP (script and style elements by hash, style attributes inline, `frame-ancestors 'self'`); the site's CSP gains `frame-src 'self'` and keeps `frame-ancestors 'none'`; the two header rules are disjoint; the figure files are committed byte-exact (`-text`) (D138 condition 3).
- (d) **The embedded figure uses the page's tokens** (embed.css, checked against tokens.css) and system fonts (its CSP loads no font; Geist is not available inside it).

### Open items and owner decisions (needed; nothing was decided)
1. **The example round looks different with the real numbers.** At the real margin the jackpot is 688,561 KAS, and the large backer's withdrawal now drains the vault down to exactly its liability line (900,623 of 8,000,000 KAS). Trust and money: none (an illustration with invented sales). Work: changing the invented inputs (for example a smaller withdrawal) is a one-line change plus a rebuild. Recommendation: watch the tour's "Backers convert" and "settle()" stops and say whether the drop reads well.
2. **The figure is small on a phone** (the drawing is about 330 px wide at 390 px; the page column is 560 px on desktop too). Trust and money: none. Work: about an hour to give S6 a wider desktop column, or to open the figure full screen on a tap. Recommendation: look at the screenshots first.
3. **Shares are shown as raw counts** in the figure ("4,330,923,170,847 shares"), as the Vault page shows them; the old figure divided by 10^8. Recommendation: keep (consistent with S5).
4. **The other figures** (round machine, shard rack; uncommitted) still import the skill; embedding them later follows the same three conditions with the same kit and rules.

## F6 part A (stage agent, 2026-10-27): S5 Vault

Committed `0dcb01a`; reviewed, see "F6 part A review and fixes" (money path: the full process applies).

### What was built
- **Rust (`crates/web/src/vault.rs`, new; exports in `bindings.rs`).** `vaultView` (the vault's figures: value A, share price (A + VA) / (S + VS) in sompi per share with 8 decimals, L, pending deposits and withdrawal shares with their worth today, R, the round a request converts at, `requestFrom` = vault creation DAA + VAULT_REQ_DELAY and `requestOpen`, MIN_DEPOSIT, wmin; the wallet's positions with worth today, the owner's x-only key when Schnorr, the W4b part range; its requests: pending / settle due with what `settle()` does now (mode, shares minted, payout, converted, still pending, accrued, the record drained and its reserve's funder) / unknown; another owner's objects left out and counted as `foreign`; an object given twice, two live records of one round refused). `withdrawChoice` (the shares worth at most an amount today, or the whole; `vault_amount` unless W4/W4b take it). `depositFigures`, `withdrawFigures`, `settleFigures`: every row bound to the built transaction (inputs spend the given objects; each covenant output's script and value; payout and record-reserve outputs; lock time = creation DAA + VAULT_REQ_DELAY; the rest back to the wallet; what the wallet pays = D + R + fee / fee (+ R for a part) / fee less what comes back). Code 55 `vault_amount`. No `lottery` change except the code (appended); no covenant change; goldens unchanged.
- **Page.** `src/vault/load.ts` (indexer `GET /v1/vault`, `/v1/positions?owner=`, `/v1/requests?owner=`; each object read from the node by outpoint, spent ones left out, `checkIndexerObjects`, then `vaultView`), `src/vault/flow.ts` (prepare deposit / withdraw / settle from the live checked objects, rows from the figures calls, `sendVault` through `sendAndWait`: post-sign check, accepted on the transaction's own outputs), `screens/Vault.tsx` and `screens/vault/useVault.ts` (read every 10 s; each action built as soon as its inputs are known, per key = action + amount + wallet + live objects, rebuilt after every press; one action at a time; `submit_unknown` keeps that press disabled while the same objects are live; a page-level "last transaction" progress, since a whole withdrawal's or settled request's card leaves the page), components Vault figures, Vault position card (position, pending, settle due, unknown), Deposit panel, Withdraw panel (Whole / Part, Least / Most from the wasm), Settle rows, Vault terms (Revocable No, Price Round N draw, Refund Possible), Shares, Estimating; catalogue entries for every state; `format.ts` `sompiOfKas` (input reading only). Indexer client `vault()`, `positions()`, `requests()`.
- **simnet:** `POST /mine?to=vault` (TEST-ONLY, simnet binary only, same rules as `/mine`): mines past the live vault's creation DAA + VAULT_REQ_DELAY.
- Docs: `web/CLAUDE.md` (Vault rules, e2e), root `CLAUDE.md` (f6_vault, `to=vault`).

### Tests (written before the page code; negative controls)
- `crates/web/tests/f6_vault.rs` (6): the view from the vault and the rules, the owner filter, W4b ranges, request status, refusals; withdraw choices (W4/W4b, accepted by the builder); deposit / withdraw / settle figures bound (another amount, owner, vault, position, shares, request or record than the build's refused; an edited transaction refused); every settlement mode (convert, refund, final, pay, accrue, a draining one) through the API, equal to `game::settle`, signed by the quick wallet and assembled. Test helpers `common::vault_settles` (refund-mode, pay, accrue and draining records), `vault_at`.
- Page fixtures (`page-inputs.json`, entries added only): the vault requests, every settlement mode, `vault at 2`.
- Vitest: `src/vault/vault.test.ts` (8: node-checked objects only, foreign and spent left out, stale vault and lying state refused; deposit rows bound, a wallet changing the request refused before sending, accepted on the request; withdrawal whole / part / `vault_amount`, an unsigned answer refused; each settlement mode built, bound and accepted; no record listed: not settle due), `src/screens/vault/useVault.test.tsx` (3: a new amount drops the old build and its late answer; a vault the chain moved drops the build; `submit_unknown` blocks the same key), `src/components/vault.test.tsx` (4), `format.test.ts` (+1).
- Negative controls (guard disabled, test run and seen failing, file restored): the owner filter off → `the_vault_view_comes_from_the_vault` FAILED; W4b in `withdraw_figures` off → `withdraw_figures_are_bound_to_the_transaction` FAILED; spent objects not left out in `load.ts` → the Vitest view test FAILED; the hook's two late-build guards (the build answer's key check and the view's key check) are redundant: either alone off passes, both off → the hook test FAILED. Not caught by any test (defence in depth, no honest build reaches it): the lock-time check in `depositFigures`/`withdrawFigures`, the "wallet pays exactly" sums in `settleFigures`.

### e2e (`web/e2e/vault.spec.ts`, quick wallet, simnet)
First run alone in a dev run: **3 passed (59.6 s)**. 1) `POST /close`, `POST /mine?to=vault`; the vault from the chain; deposit 25 KAS (reserve 1.69 KAS, fee 0.0388 KAS, total 26.73 KAS; terms No / Round 3 draw / Possible), accepted, request pending; quality (axe both themes, 360/430 px, named and tagged). 2) `POST /close` draws round 3: settle due (convert, fee 0.0366 KAS), Settle → a position worth 25.00 KAS. 3) a part withdrawn (12 KAS of the 10.0–15.x KAS range; 1 KAS refused as `vault_amount`, nothing built), a second deposit of 10 KAS, the page's own `draw()` of round 4 (its panel shows "Record back: With settlement"), both settled: convert and final (payout 12 KAS + R back; the last settlement drains the record and its reserve comes back to this wallet, total -15.34 KAS); two positions, no requests. Screenshots `test-results/shots/f6-vault-*`.
- **Refund mode is not reachable on a simnet** (it needs a share price above MIN_DEPOSIT = 10^9 sompi per share, about 10^12 times the empty vault price VA / VS = 0.001 sompi per share, or a mint above SHARES_MAX; D84; corrected in the F6 part A review): covered by Rust (`settle refund`) and Vitest.
- Full `npm run e2e` runs (fresh dev runs): run 1 **18 passed, 1 failed, 1 skipped**: the vault spec's third test failed in `/mine?to=vault`, a test-side race (the dev tool read its index before it saw the vault the withdrawal had just moved, so it mined nothing; the page waited 120 s at 5 DAA/s for 600 DAA). Fixed: `mine_to` catches its index up first, and the spec asks again on every look. Run 2 (after the fix): **18 passed, 1 failed, 1 skipped (the void), 44.2 min**: vault 3/3; the failure is the **F4 two-claimer race in `tickets.spec.ts`** (not touched here): T2's claim refused (`node_conflict`), T3's accepted, re-press 1 refused, then the test waited for a claim progress element that never came until its 40-minute timeout (trace `web/test-results/tickets-F4-a-win-a-shared--…/trace.zip`). It passed in run 1. Same symptom family as F5's "What is left"; needs its own look (the test's `outcomeAfter`, or claims rebuilding after a conflict).

### Commands (F6 part A; one cargo process at a time)
- `cargo test -j 4 --no-fail-fast` (debug, default members): **1,109 passed, 0 failed, 3 ignored** (spike_vdf's debug stack overflow as before, D113).
- `cargo test -j 4 -p web --release --lib --test api --test f4_tickets --test f4_review --test f5_rounds --test review_f5 --test f6_vault`: **60 passed, 3 ignored** (13, 20 + 3, 7, 5, 3, 6, 6). W1 (wasm32): 1 passed, "18 builds … 97 digest lines … 291 decode and verify calls … equal the native golden file" (goldens unchanged). W4: 1 passed.
- indexer-api (`--features doctored`, api, doctored, review_f3, review_f4): **13 passed**; `--test production`: 1 passed. simnet: **15 passed** (with `to=vault` in `mine_targets_parse`). tn10: **25 passed**.
- `npm test`: tsc, ESLint, Prettier clean; Vitest **27 files, 202 tests passed** (F5: 24, 186).
- `npm run build` (placeholder indexer URL and covenant ID): release wasm 6,798,011 B (F5 review: 6,743,659), index JS 502.5 kB (476.8); check-bundle and its control ok; `npm run check:mainnet` ok; `npm run check:csp` 10 ok lines.
- Dev runs stopped; ports 18610, 8790, 8791, 5173, 4173-4174 free; this stage's simnet data (393 MB) deleted.

### Files
- Rust: `crates/web/src/vault.rs` (new), `bindings.rs`, `lib.rs`, `rounds.rs` (`Outpoint::of` crate-visible); `crates/web/tests/f6_vault.rs` (new), `tests/common/mod.rs`, `tests/api.rs` (page fixtures); `crates/lottery/src/web/mod.rs` (code 55), `crates/lottery/tests/f1_web.rs`; `crates/simnet/src/{dev.rs, faucet.rs}`.
- Page: `web/src/vault/` (new), `screens/Vault.tsx`, `screens/vault/` (new), components `VaultFigures`, `VaultPositionCard`, `DepositPanel`, `WithdrawPanel`, `VaultRequestRows`, `SettleRows`, `VaultTerms`, `Shares`, `Estimating`, `vaultStates.ts`, `vault.test.tsx` (new); `dev/vaultSamples.ts` (new), `dev/Catalogue.tsx`, `app/App.tsx`, `net/indexer.ts`, `worker/protocol.ts`, `lib/format.ts`, `test/ticketWorld.ts`, `test/fixtures/page-inputs.json`; `e2e/vault.spec.ts` (new); `web/CLAUDE.md`, root `CLAUDE.md`.
- Not touched: `covenants/`, `crates/lottery/templates/`, the golden files (`w1_api.txt`, `w1_verify.txt`, `f1_digests.txt`), `Cargo.lock`, `package-lock.json`, the unrelated uncommitted files.

### Proposed DECISIONS rows (F6 part A)
- (a) **Vault figures from Rust over node-checked objects** (`vaultView`): the indexer's vault, positions, requests and records are read from the node by outpoint and checked; a position or request of another owner is left out (counted), never shown as the wallet's, and not a refusal (an indexer listing extra objects is harmless once filtered).
- (b) **Code 55 `vault_amount`**: an amount the vault does not take (deposit below MIN_DEPOSIT; withdrawal neither whole nor W4b).
- (c) **Withdrawal amounts are typed in KAS**: `withdrawChoice` takes the shares worth at most that amount today, floor(a (S + VS) / (A + VA)); the page shows the W4b range as Least / Most worth today and builds nothing outside it; Whole is always offered.
- (d) **Terms as rows from Rust fields** (`revocable: false`, `refundable: true` for deposits, Price = Round N draw); "Worth today" names every estimate at the current price.
- (e) **"Vault value" is A (total_assets)**; the UTXO value (A + pending deposits + R) is in the view but not shown. Share price in sompi per share, 8 decimals, rounded down.
- (f) **No pending record for vault actions** (as F5 row (f)): `submit_unknown` keeps the press disabled while the same objects are live; the next request needs the vault 600 DAA old anyway.
- (g) **Accepted on the action's own outputs**: request, position left, wallet outputs, vault (requests); wallet outputs, new position, request left, record left (settlements).
- (h) **simnet `POST /mine?to=vault`** (TEST-ONLY); **refund mode has no simnet e2e** (unreachable), covered by Rust and Vitest.

### Left (parts B+)
- How it works (S6), deploy preparation, the security review (What F6 needs).
- The settle list shows the wallet's own requests only; anyone may settle any (D77). A "settle others' requests" helper is out of scope.
- A position whose owner is not Schnorr shows `owner_type` and no Withdraw (made by other tools only).

### What the reviewer should attack
1. `vaultView`'s owner filter and status: can an indexer make another owner's position or request show as the wallet's, or a request show settle due against the wrong record (another round's, a stale one)?
2. The figures' bindings: any built deposit, withdrawal or settlement whose shown rows differ from what it does (owner, amount, shares, reserve, payout recipient, record refund recipient, the totals' signs).
3. `withdrawChoice` and the Part range at the W4b edges (wmin, shares − wmin), `pend_wsh`, q = 0 vaults (wmin = 1).
4. `useVault`'s build keys: can a press sign a build made for another amount, position, vault or wallet; the `gen` rebuild after a press; the `submit_unknown` block.
5. The withdrawal's owner input: the coin chosen, the owner key from the position state, a wallet whose address is not the owner's.
6. `POST /mine?to=vault` exists in the simnet binary only.

### Owner decisions (needed; nothing was decided)
1. **Refund mode cannot be tried end to end on the private simnet.** A deposit comes back as a refund only when one share costs more than 10 KAS (about a million million, 10^12, times the starting price of 0.001 sompi a share; corrected in the review) or the share count would overflow; no test instance can get there. Trust: none changes. Money: refunds are tested in Rust and in the page's unit tests with a made-up refund record, and the covenant tests cover it. Work: reaching it for real would need a test-only vault genesis, which the owner already ruled out for similar shortcuts. Recommendation: accept the unit-test coverage.
2. **A deposit or withdrawal request whose outcome the page never learned is not remembered after a reload** (as for close and draw). Trust: none. Money: none is lost; the worst case is that the backer, not seeing the request yet, makes a second one on purpose, which is a valid second deposit (the vault takes the next request only 60 seconds later, by which time the first one shows). Work: keeping them like purchases is about half a day. Recommendation: leave as is for the beta.

## F6 part A review and fixes (fix agent, 2026-10-27; not committed)

`covenant-reviewer` on F6 part A (`0dcb01a`): no must-fix, two should-fix, notes. Fixed with tests first and negative controls (guard off, test seen failing, file restored). The reviewer's probe `crates/web/tests/review_f6_probe.rs` is folded into `f6_vault.rs` and removed. No covenant change; goldens unchanged.

- **S1 Least / Most were amounts the page then refused** (money path). `PartRange` now carries the amounts `withdrawChoice` takes: `leastAmount` = ceil(wmin (A + VA) / (S + VS)), `mostAmount` = the largest amount whose shares stay <= min(shares − wmin, S − pend_wsh) (W4b, W4), and `leastShown` / `mostShown` = those rounded up / down to 0.01 KAS when both stay inside, else the exact amounts; `partial` is None when no amount reaches a part. The panel prints the shown values exactly (`kasExact`: 2–8 decimals, no grouping, the form the amount field reads). Test `least_and_most_are_amounts_the_vault_takes` (403 vaults, 396 with a range: 303 shown at 0.01 KAS, 93 exact): exact and shown Least / Most taken, one sompi outside refused; control: the old values → FAILED (and `withdraw_choices_are_what_the_vault_takes`, whose `+ 2` slack is gone, FAILED). Vitest: `vault.test.ts` types the shown and exact values back through `sompiOfKas(kasExact(…))` (taken), least − 1 refused; `format.test.ts` `kasExact` round trip.
- **S2 the F4 two-claimer e2e hang** (test fix). `progressOf` reads the card's claimed state and the row in one `evaluateAll` (no locator read that waits for an element with no limit); `claim()` and `outcomeAfter` check the claimed card first; the re-press loop's `isEnabled` has a 2 s limit. Stability: see the e2e runs below.
- **N1 a refused transaction ID sent again** (orphan). `signAndSubmit` keeps the IDs the node refused as a conflict (10 minutes, `noteRefused` / `refusedLately` in `buyFlow.ts`) and refuses one before the wallet signs (`node_conflict`); claims, returns, the three vault actions and the caller's close/draw/void build through `unrefused` (`tickets/send.ts`: a build that comes out as a refused ID is built again after 2 s, up to 5 times, else `node_conflict`); a purchase moves to another shard as before. Tests `src/tickets/refused.test.ts` (2: a re-press of the refused build is refused before signing and nothing sent; a rebuild while the node still lists the spent coin is never offered, and once the coin moves a new transaction is built). Controls: the signing guard off → test 1 FAILED; `unrefused` off → test 2 FAILED. `buyFlow.test.ts` resets the refusals per test (its stand-in reuses IDs).
- **N2 lock-time control**: `a_consistent_build_with_another_lock_time_is_refused` (deposit, partial and whole withdrawal): a self-consistent build with lock time + 1 is `built_mismatch` (only the page's check catches it: the covenant checks lower bounds only), − 1 is refused already by the engine. Control: the lock-time check off → FAILED. Also kept from the probe: `vault_builders_cap_the_fee_rate`.
- **N6 positions found only through the indexer: written up, not built** (owner decision 1 below). Not small: the page's local store has no record kind for positions (a new `webstore.rs` record, a `webstore/3` migration, the backup format, the load merge), and positions minted by someone else's `settle()` (anyone may settle, D77) would still be found only through an indexer.
- **Handoff corrections**: refund mode needs a share price above 10^9 sompi per share, about 10^12 times the empty-vault price of 0.001 sompi per share (the part A notes said 10^9; fixed above); "Vault value" shows A (`totalAssets`), not the UTXO value (A + pending deposits + R), as proposed row (e) says.

### Commands (final set, one cargo process at a time)
- `cargo test -j 4 --no-fail-fast` (debug, default members): **1,109 passed, 0 failed, 3 ignored**; the only failing target is `spike_vdf`'s known debug stack overflow (D113, as in part A).
- `cargo test -j 4 -p web --release --lib --test api --test f4_tickets --test f4_review --test f5_rounds --test review_f5 --test f6_vault`: **63 passed, 3 ignored** (13, 20 + 3, 7, 5, 3, 6, 9). W1 (wasm32): 1 passed (18 builds, 97 digest lines, 291 decode and verify calls equal the native golden file; goldens unchanged). W4: 1 passed.
- `npm test`: tsc, ESLint, Prettier clean; Vitest **28 files, 205 tests passed** (part A: 27, 202).
- `npm run e2e` (fresh dev run): **19 passed, 1 skipped (the void), 5.4 min**. Then `npx playwright test e2e/tickets.spec.ts` three more times on the same dev run: **2 passed each** (2.1 min). The two-claimer race came out both ways (T2 accepted / T3 refused once, the reverse three times); every refused claim ended accepted on its second re-press. The spec now logs the page's warnings in the race: the refused claim's re-press 1 was a *new* transaction (rebuilt; never the refused ID) that the node refused as an orphan, because the press came before the node had caught up with the other claim (the race "What F6 needs" lists); re-press 2 was accepted.
- `npm run build` (placeholder indexer URL and covenant ID): release wasm 6,798,517 B, index JS 503.7 kB; check-bundle and its control ok; `npm run check:mainnet` ok; `npm run check:csp` 10 ok lines.
- Dev run stopped; ports 18610, 8790, 8791, 5173, 4173-4174 free; this run's simnet data (275 MB and its dev store) deleted.

### Files
- Rust: `crates/web/src/vault.rs` (`PartRange`, `part_range`, `shares_for`, `SHOWN_STEP`), `crates/web/tests/f6_vault.rs` (+3 tests, the `+ 2` slack removed); `crates/web/tests/review_f6_probe.rs` removed (folded).
- Page: `src/lib/format.ts` (`kasExact`), `components/Amount.tsx` (`exact`), `components/WithdrawPanel.tsx`, `dev/vaultSamples.ts`; `play/buyFlow.ts` (refused IDs, the signing guard), `tickets/send.ts` (`unrefused`), `tickets/claimFlow.ts`, `vault/flow.ts`, `rounds/caller.ts` (prepares through `unrefused`); tests `tickets/refused.test.ts` (new), `vault/vault.test.ts`, `components/vault.test.tsx`, `lib/format.test.ts`, `play/buyFlow.test.ts`; `e2e/tickets.spec.ts`; `web/CLAUDE.md` (Wallets and Vault rules).

### Proposed DECISIONS rows (F6 part A review)
- (i) **Least and Most are the amounts the vault takes**, rounded inward to 0.01 KAS when the range allows, else exact; printed exactly (`kasExact`).
- (j) **A transaction ID the node refused as a conflict is never signed or sent again** for 10 minutes; the flows build again (`unrefused`).

### Owner decisions (needed; nothing was decided)
1. **A backer's position is found only through the indexer** (N6). Trust: an indexer that leaves a position out hides it from the page; it cannot take it or move it (only the owner's key can), and the page never shows another owner's position as yours. Money: none at risk; the position stays on chain and can be withdrawn with another indexer or the repository's tools. Work: about a day to remember, in this browser, the positions the page itself saw created (a new kind of record in the local store, its migration, the backup file), and it would still not cover a position created by someone else settling your request. Recommendation: leave as is for the beta and list it on How it works; revisit with the hosted indexer.

## F4 review and fixes (summary; full notes in `git show 66a2db7:docs/specs/frontend-handoff.md`)

`covenant-reviewer` on F4 found 2 must-fix and 7 should-fix items; all fixed in `8a264ac` with regression tests (`crates/web/tests/f4_review.rs`, `crates/indexer-api/tests/review_f4.rs`, `web/src/tickets/review.test.ts`) and negative controls. The three owner questions it raised are answered (DECISIONS 2026-10-26, "F4 leftovers kept for the testnet beta").

## F5 review and fixes (stage agent, 2026-10-27)

`covenant-reviewer` on F5: no must-fix, six should-fix items and four notes; two e2e tests were red. All fixed, tests first, each with a negative control (the fix disabled by a temporary edit, the test run and seen failing, the file restored and its time refreshed so cargo rebuilds). Regression tests: `crates/web/tests/review_f5.rs` (6; the reviewer's three Rust probes folded in, the probe file removed), `web/src/rounds/review_f5.test.ts` (5; the reviewer's Vitest probe folded in and removed), `web/src/screens/rounds/useRoundDetail.test.tsx` (1). The reviewer's `crates/lottery/tests/review_f5_claimlog.rs` stays as an ignored dev tool (it settled the F4 race below; documented in root `CLAUDE.md`). No covenant change; every golden unchanged.

| Item | What changed | Test (negative control) |
|---|---|---|
| S1 the page's draw never absorbed expired pools | `GET /v1/pools` (indexer API: live pools, oldest claim deadline first, at most 256; also in the dev self-check). `absorbablePools` (Rust): each candidate read from the node by outpoint and its state checked (`check_object`); kept when a pool of an earlier round, deadline below the node's virtual DAA and, for a draw, below the void lock, R = the controller's, value ≥ R; oldest first, A_MAX. `prepareCaller` passes them to `buildDraw` and `callerFigures` (draw and void); without the list nothing is absorbed. `callerFigures` takes the vault and the absorbed pools: the kind from the exact lock time (max(D or D + draw_window, the deadlines); refused when both match), every covenant input named, new fields `pools`, `record`, `absorbed`; "Back" is the new pools' date only; a settlement record's R "With settlement"; row "Expired pools". | `s1_absorbing_draws_are_the_native_builders`: 5 new fixture calls (`fixtures::draw_calls`: draws absorbing 3 and A_MAX, a void absorbing 2, a void with purchases, a draw with a record): chain-object path = native `build::draw` byte for byte, API = library, figures; absorbed list missing or reordered refused. `s1_absorbable_pools_are_checked_and_ordered`. Vitest: a void absorbing 2 and a draw absorbing 3 through `prepareCaller` (real wasm, anchor bound to the fixture header), byte-identical to `buildDraw` of the fixture with that set; without the list none; a listed pool the node lacks left out. Controls: round filter off, void-lock filter off, input count off, `absorbed: []` in `caller.ts`: each fails. |
| S2 another round's answer under the requested number | `net/indexer.ts`: `round`, `purchases`, `anchorProof` refuse an answer whose round is not the one asked for (`chain_mismatch`); so do the list row and the detail. | Vitest: the client with a fake fetch; the reviewer's probe (round N under 999) now a row error and a refused detail. Control: the comparison off → both fail. |
| S3 a settlement record alone gave a checked "Drawn" | `check_round`: a record alone shows no status (drawn or void unchecked; open/closing contradict it: `chain_mismatch`); `mChecked` (marker, pool, open controller or shard). RoundDetail's m uses it; Verify's Status item is "not checkable". | `s3_a_record_alone_is_no_checked_status`. Controls: record → Drawn restored; m checked by the record: fail. |
| S4 close and draw IDs shown without "Unverified" | `facts.closeTx` (the marker, else round r + 1's controller with drawn = 0) and `facts.drawTx` (the drawer of round r + 1, else r + 1's controller with drawn = 1); a pool or record never binds them (claims and settlements re-create those). `check_round` refuses an indexer ID differing from a bound one; RoundDetail shows unbound IDs "Unverified". `parse_round` (and objects, tickets) refuse any transaction ID or hash that is not 64 hex digits. | `s4_close_and_draw_ids_are_bound_to_their_objects`, `s4_round_answers_carry_64_hex_ids`; e2e asserts round 1's draw ID checked, its close ID unverified, round 2's close ID checked. Controls: the draw-ID check off; the hex check off: fail. |
| S5 a void needed the indexer's purchases | `cases::draw`: a void with no purchases uses k empty trees; `build::draw` checks the roots of a draw only (purchases given for a void are still checked in `cases::draw`). `prepareCaller` asks no purchases for a void. | `s5_a_void_needs_no_purchases` (with real holdings: the reviewer's probe was vacuous, its fixture trees were empty); Vitest void with the purchases endpoint down. Controls: `void_without_purchases` off; roots checked for voids; `caller.ts` asking purchases: fail. |
| S6 stale built figures | `useRoundDetail`: a build belongs to (round, transition, wallet, the round's live objects); another key drops it at once, a late answer of an old build is ignored, not-due clears it; only the current key's build is shown and signed. | `useRoundDetail.test.tsx` (objects move, transition changes, late build). Controls: objects left out of the key; late-answer guard off; not-due clearing off: each fails. |
| Notes | X before the draw "Provisional" (hash and DAA); after the draw the detail binds X to the node's block itself (cached per round once bound) and shows its DAA score; Verify items' `detail` no longer shown (`data-code`). | e2e (rounds.spec) asserts the anchor states. |

**The F4 two-claimer race (`tickets.spec.ts`).** The dev stores settle it (claim log tool): in the failing F5 run (store `fcbfba0b…`) T3's claim landed first and T2's last claim `67f8…` landed 6 DAA later, built on T3's pool successor, i.e. a re-press that went through; the test's `outcome()` read the row's earlier "failed" progress right after the click, reported the re-press as failed, then waited for a Claim button a claimed card never shows. Test problem, not a page bug. Fix: `press`/`outcomeAfter` follow the press's own attempt (an earlier failure is never read as this press's; a card already claimed needs no press). A first fix attempt missed a refusal that came before any non-failed state was seen (seen in a run: T3 refused for a wallet coin T2's claim had spent, `node_conflict`); corrected. Runs: before the fix 1 pass; after: 1 pass (T2 accepted, T3 refused, re-press 1 refused as the known orphan case, re-press 2 accepted).

**Void e2e on request** (owner, DECISIONS 2026-10-27): `npm run e2e:void` (`scripts/e2e-void.mjs`, E2E_VOID=1); a plain `npm run e2e` skips `void.spec.ts`. Documented in `web/CLAUDE.md` and root `CLAUDE.md`.

**Not done: an absorbing draw on the simnet e2e.** Pools expire at least D + 864,000 + 6,048,000 DAA after their draw (about 4 hours of mining and far more than this PC's memory); a short claim window would need test-only bypasses in genesis validation and in the bundle check the page itself runs, and the owner ruled out a test-only short window (DECISIONS 2026-10-26). Covered by the Rust and Vitest tests above; the real-chain check moves to F6 ("What F6 needs").

### Commands (final set, one cargo process at a time)
- `cargo test -j 4 --no-fail-fast` (debug, default members): **1,109 passed, 0 failed, 3 ignored**; the one red target is `spike_vdf` overflowing its stack in debug, as before (D113).
- `cargo test -j 4 -p web --release --lib --test api --test f4_tickets --test f4_review --test f5_rounds --test review_f5`: **54 passed** (13, 20 + 3 ignored, 7, 5, 3, 6). W1 (wasm32): 1 passed ("18 builds … 97 digest lines … 291 decode and verify calls … equal the native golden file": goldens unchanged). W4: 1 passed.
- `cargo test -j 4 -p indexer-api --features doctored --test api --test doctored --test review_f3 --test review_f4`: **13 passed**; `--test production`: 1 passed. `cargo test -j 4 -p simnet --release`: 15 passed. `cargo test -j 1 --release -p tn10`: 25 passed.
- `npm test`: tsc, ESLint, Prettier clean; Vitest **24 files, 186 tests passed**.
- `npm run e2e` in a fresh dev run: **16 passed, 1 skipped (the void), 5.2 min**; the race went the order that failed in F5 (T2 refused, T3 accepted; T2's re-press accepted). `npm run e2e:void` in the same run: **1 passed (55.4 min**, 3,270 s of mining; void reward 0.005 KAS, fee 0.2017 KAS, total -1.49 KAS).
- `npm run build` (placeholder indexer URL and covenant ID): release wasm 6,743,659 B (F5: 6,710,306), index JS 476.8 kB; check-bundle and its control ok; `npm run check:mainnet` ok; `npm run check:csp` 10 ok lines.
- Dev runs stopped, ports 18610, 8790, 8791, 5173, 4173-4174 free; the simnet data this stage made (112 MB, 2.47 GB) deleted.

### Files
- Rust: `crates/lottery/src/web/{build.rs, cases.rs, fixtures.rs (draw_calls)}`, `crates/web/src/{rounds.rs, indexer.rs, bindings.rs}`, `crates/web/tests/{review_f5.rs (new), f5_rounds.rs, api.rs (page fixtures)}`, `crates/indexer-api/src/{routes.rs, server.rs}`, `crates/indexer-api/tests/api.rs`, `crates/simnet/src/dev.rs` (self-check), `crates/lottery/tests/review_f5_claimlog.rs` (reviewer's, kept).
- Page: `web/src/{net/indexer.ts, rounds/caller.ts, rounds/load.ts, screens/rounds/RoundDetail.tsx, screens/rounds/useRoundDetail.ts, components/CallerPanel.tsx, components/VerifyPanel.tsx, dev/roundSamples.ts, dev/Catalogue.tsx, worker/protocol.ts, test/ticketWorld.ts, test/fixtures/page-inputs.json}`, tests `rounds/review_f5.test.ts`, `screens/rounds/useRoundDetail.test.tsx`, `components/rounds.test.tsx`; e2e `rounds.spec.ts`, `tickets.spec.ts`, `void.spec.ts`; `scripts/e2e-void.mjs`, `package.json` (`e2e:void`); `web/CLAUDE.md`, root `CLAUDE.md`.

### Left
- An absorbing draw on a real chain (F6, testnet-10; above).
- Not caught by any test: the lock-time "ambiguous kind" refusal in `callerFigures` (no honest build reaches it: `absorbablePools` keeps a draw's deadlines below the void lock); defence in depth.
- A record posted by a draw comes back "With settlement"; settlements are F6's Vault work.

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
