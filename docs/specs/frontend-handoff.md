# Front-end track: build handoff

Spec: `docs/specs/frontend.md` (approved 2026-10-25, D129–D139). Delete this file when the track closes and its content is in `docs/DECISIONS.md`.

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

## Facts from the design session (2026-10-25), for F0 and F1

- **Wallets** (web research, docs read 2026-10-25):
  - **KasWare:** `window.kasware.signPskt({txJsonString, options: {signInputs: [{index, sighashType}]}})` and `pushPskt`, with `switchNetwork("kaspa_testnet_10")` (switching is disabled in its mobile app). It "only signs inputs that belong to addresses derived from the current account's public key"; what it does with other inputs is undocumented. `pushPskt` is documented to keep KIP-20 v1 covenant bindings. Sources: docs.kasware.xyz (dev-integration, kaspa-transaction, kaspa-connection, KCC-20 pages).
  - **Kaspire:** `window.kaspire.signPskt` with `signInputs`, and WalletConnect `kaspa_signPskt` on Android. Supports `testnet-10`. It may refuse "unknown templates and generic covenant requests"; whether that applies to `signPskt` is unverified (github.com/KaspaHUB21/Kaspire-Kaspa-Wallet).
  - **Kastle:** `signTx` leaves covenant inputs empty (issue forbole/kastle#353, open). Not used.
  - **Not usable:** Kaspium, Kaspa NG, Tangem and OKX have no documented dApp signing.
  - **No standard yet:** the draft cross-wallet standard KCC-12 (kaspanet/kccs PR #24) is unmerged.
- **The wasm probe** (scratch build, not in the repo):
  - **First errors:** `cargo check -p lottery --target wasm32-unknown-unknown` first fails on getrandom (needs `wasm_js`), then on `secp256k1-sys` (needs a wasm-capable clang via `CC_wasm32_unknown_unknown`). With both fixed, the check passes.
  - **Link blocker:** duplicate symbols from two `kaspa-consensus-core` copies (ours v2.1.0, and `silverscript-abi`/`silverscript-lang` at rev `a41a333`). Rebuilding `silverscript-abi` against v2.1.0 showed 2 API errors (`lib.rs:901` `covenants_enabled`, `lib.rs:1204` `deserialize_script_i64` arity).
  - **Forced link** (`--allow-multiple-definition`, probe only): the 11 templates compiled in wasm in about 1.0–1.6 s, and a `buy()` passed the shard covenant (`Ok(354210)`). Size 6.97 MB at `opt-level = "s"` with the compiler.
  - **Disk reads at runtime:** `lottery` reads `.sil` from disk at `buy.rs:83, 93, 109, 156` and compiles at `buy.rs:245-259` and `draw.rs:53` (and `genesis.rs:289`). `Templates::get(b_max)` (`buy.rs:283-292`) is the single entry point. The 11 artifacts are 568,526 B of JSON (154,481 B of script).
  - **Panics abort:** wasm uses `panic = "abort"`, so `catch_unwind` (`index.rs:952`) does not protect the page.
  - **Signing inside the library:** `Built::finish` (`buy.rs:1049`) signs with a raw key; signing has to be split from building.
  - **Anchor proof RPC:** `GetSeqCommitLaneProof` is in the Rust wRPC client only (`rpc/wrpc/client/src/client.rs:667`), not in the JS binding.
  - **Builder entry points:** `BuyCase` (`buy.rs:567`), `ClaimCase::from_store` (`claim.rs:592`), `ReturnCase` (`claim.rs:705`), `RefreshCase` (`claim.rs:910`), `CloseCase` (`close.rs:76`), `DrawCase::from_store` (`draw.rs:996`), `DepositCase`/`WithdrawCase`/`SettleCase` (`vault.rs:150, 278, 541`). Filled by hand today in `tn10/src/campaign.rs:1148-1374`.

## Stage log

| Stage | Status | Notes |
|---|---|---|
| F0 wallet experiment | done (2026-10-26) | Part B on testnet-10 with KasWare (owner): `signPskt` with the P2PK index list signed only that input for `buy()` and an 18-covenant-input `close()`; both passed the post-sign check and were accepted. Without a list KasWare overwrote covenant sigscripts and the check refused both. `pushPskt` is not exposed. So the page always passes `signInputs` and submits itself. Kaspire untested (not installed). Records: `docs/tn10-mass-check.md` "Front end F0", DECISIONS 2026-10-26 row (`eedf3c5`). |
| F1 builder refactor for wasm | done (parts A, B, C; review and re-review fixed) | Re-review (`covenant-reviewer` on `f7f8102`): one should-fix, fixed test first with negative controls: before the draw the anchor proof's X.hash was free, so whoever served the proof chose the winning numbers; now the anchor block's header from a public node binds X (`anchor_invalid`) and `winningNumbers` says `final` (drawn) or `provisional` (closed). Hardening: one signing key per build (34 `mixed_signers`); liveness docs fixed; W1 signs 14 of 18 shapes in wasm. Requirements for F3, F5 and F6 recorded (node-only entries and headers, the deployed artifact in W4, no beta launch through `cases.rs`/`wallet_probe.rs`). One owner decision (show provisional numbers before the draw). See "F1 re-review and fixes". Review (`covenant-reviewer` on `b3b3520`): no must-fix; the six should-fix items and four notes are fixed, each test first, with negative controls: page values out of `checkRoots`/`winningNumbers` (round, m, D derived in Rust), every `BuiltTx` figure bound to its transaction (`built_mismatch`), Schnorr-only deposit owners (`owner_type`), W1 runs signing, verification, decode and verify in wasm (97 + 18 + 290 lines, 7 signed), 20 golden lines appended (77 untouched), W4 checks the release artifact for the simnet and the fixtures; `change_dust`, the draw's `funding_short`, redacted keys, the `fixtures` feature. One owner decision (small change: refuse or pay as fee). See "F1 review and fixes". Before the review: A: the templates embedded (W2), the compiler behind `compiler`, one rusty-kaspa version, the wasm links. B: `lottery::web` (decode, verify, the 8 unsigned builders, the signing split, the post-sign check, typed errors, W3). C: the builders from chain objects (`lottery::web::cases`), the wasm-bindgen API in `crates/web` with a generated, typed `.d.ts`, one Safe JSON serialiser (tn10 calls it), W1 in wasm under Node (77 + 18 digests equal), W4 (the mainnet build has no quick-wallet symbol and refuses to load), the quick-wallet key hook, the wRPC client running in wasm against testnet-10, sizes (1.69 MB brotli at its smallest). Owner questions: simnet addresses; an npm accident in the home folder. See "F1 part A", "F1 part B" and "F1 part C". |
| F2 indexer API | done (not reviewed) | `crates/indexer-api`: a read-only HTTP JSON server (axum 0.8.9 on hyper 1 and tokio, all already in Cargo.lock) over the M8 store it follows (`index-log.jsonl` + the verified `bundle/1`); 11 GET endpoints under `/v1/`, typed JSON errors, CORS, size caps, a request deadline, a concurrency limit, header-read and connection timeouts; no keys, no writes. The response types and the page's checks are `web::indexer` (tsify, in the `.d.ts`): `parseIndexer*` (size-capped before parsing) and `checkIndexerInstance`/`checkIndexerObjects`/`checkIndexerRound`, which reuse the F1 checks; any disagreement with the chain is `chain_mismatch`. Six TEST-ONLY doctored modes behind the `doctored` feature and a separate loopback-only binary; each is refused by the page's checks (tests, negative controls); the production binary has none (byte scan). Served a simnet instance (`simnet dev`: round 1 drawn, 2 closing with a node-built anchor proof, 3 open; every endpoint self-checked against the node) and, read-only, the M8 testnet-10 store. See "F2". |
| F3 web shell and Play | done: part A (2026-10-25), part B (2026-10-26); not reviewed | Part B: wallet adapters behind one interface (KasWare with the explicit index list, Kaspire untested, the quick wallet with its key in its own IndexedDB database; coins from the node; one request at a time); the post-sign attack tests through the page's real path on the real wasm (spec 8's eight alterations plus three, each refused before sending; negative control); `webstore/1` (record types in Rust) with the ticket and the round's checked copy after each purchase; S1 Play (picker, ticket, cost panel from the new Rust `buyFigures`, buy, progress, rebuild on shard conflict as code 40 `node_conflict`); referral links validated by the wasm; S7 wallet sheet; change dust exact-zero (owner); dark for first-time visitors (owner, amends D139); codes 40-44; simnet dev s_R = 500 and a coin-combining faucet; e2e 10/10 on simnet (b = 1 and 4, with and without a link, below M_ref, a real shard conflict). Two owner questions (faucet link, deposit date). See "F3 part B". Part A: `web/` (React 19, TypeScript strict, Vite 7, Tailwind 4 with shadcn/ui-style Radix primitives, ESLint, Prettier, Vitest, Playwright + axe-core), `web/CLAUDE.md`; tokens in one file (spec 5 exactly, both themes, contrast pinned by a test), Geist fonts vendored (OFL), theme dark / light / device remembered; the wasm in a Web Worker with types from the generated `.d.ts`, size caps and restart on a crash; the page's node client is the rusty-kaspa v2.1.0 wRPC client compiled into `crates/web` (feature `node`; a workflow-core timer panic inside workers found and fixed); `npm run dev:simnet` starts `simnet dev` (fixed node port 18610, dev faucet) and the page; 9 base components with every state; a dev-only catalogue; the shell and the live round card on Play from the simnet, checked against the node. Codes 35–39 appended; W4 now checks the shipped (`node`) builds. One owner decision (no-preference theme). See "F3 part A". |
| F4 My tickets, claims, backup | not started | |
| F5 Rounds, verify, run the round | not started | |
| F6 Vault, How it works, beta instance, deploy, security review | not started | |

## F0 part A (stage agent, 2026-10-25)

### What was built

- **The probe** `cargo run -p tn10 --release -- wallet-probe serve [--port 8710]` (native, no wasm, no F1 work):
  - serves one page on `http://localhost:8710/`. It is bound to 127.0.0.1 only, and POSTs from any other page origin get 403.
  - The page connects `window.kasware` and/or `window.kaspire`. It refuses unless the wallet reports testnet-10 (`kaspa_testnet_10` for KasWare, `testnet-10` for Kaspire) and the address starts with `kaspatest:`. The server refuses the same again, and also refuses non-P2PK addresses.
  - The server builds each transaction natively with the `lottery` builders:
    - covenant inputs from a probe instance;
    - the wallet's own confirmed P2PK UTXOs (read from the node by its address) as funding;
    - change or caller output to the wallet;
    - fee from the node's estimator (`fees::with_fee`).

    Every covenant input runs in the local engine before anything goes to the wallet.
  - It sends the transaction as Safe JSON, with the funding inputs' `signatureScript` empty, plus the indices to sign (`[{index, sighashType: 1}]`).
  - The page calls the wallet and posts the answer back. The server runs the post-sign check (spec 6.2), then the local engine on every input, and submits to testnet-10 only if both pass.
  - For the push variants the wallet submits. The server then compares the txid with the built one: the v1 id covers every field except the signature scripts and the mass commit (`consensus/core/src/hashing/tx.rs:207-235`). It reads the submitted transaction back (the mempool entry, else the virtual chain at Full verbosity) and runs the same check after the fact.
- **Buttons** (each has a `data-testid`): `kasware-switch`, `kasware-connect`, `kasware-{buy,close}-{indices,nolist,push}`, `kaspire-connect`, `kaspire-{buy,close}-{indices,nolist,push}`. Status lines: `state`, `kasware-status`, `kaspire-status`. Log pane: `log`.
  - `indices` = `signPskt` with `signInputs` set to the P2PK funding indices only.
  - `nolist` = `signPskt` with `options: {}` (no `signInputs`).
  - `push` = KasWare `pushPskt`, with the P2PK indices listed. Kaspire documents no `pushPskt`, so its `push` is `signPskt` (P2PK indices), then the server's check, then `kaspire.pushTx(signed)`.
- **Probe instances.**
  - When: the server makes a fresh K = 16 genesis at start, and again after every successful close or when all 16 shards are used.
  - Cost: `TN10_PRIVATE_KEY` pays, and about 94 KAS stays locked per instance on testnet.
  - Timing: T = genesis + 600 DAA, so `close()` opens about 1 minute after genesis.
  - Buys: each buy uses a fresh shard, so an instance takes 16 buys.
  - Closes: a close consumes the instance, because round 2 cannot close without a draw. Each close variant therefore runs on its own instance, and the next one is created automatically right after a close.
- **The log.** Every step is one JSON line in `tn10-data/wallet-probe/<unix>-serve.jsonl` (git-ignored). Steps:
  - `connect` / `connect-refused`;
  - `build` (with `unsigned_json`);
  - `signed-raw` (the wallet's answer, verbatim);
  - `post-sign` (pass, refusals, gained and lost signatures, notes);
  - `engine` (per input);
  - `submit` (the txid or the node's error);
  - `push-result`, `push-check` (with `submitted_json`);
  - `wallet-error` (the message and the raw error object);
  - `page` (provider missing, refusals);
  - `instance`, `next-instance`.

  stdout prints one summary line per step.
- **Funding.** `cargo run -p tn10 --release -- send <kaspatest:address> <KAS> [--parts n]` pays from `TN10_PRIVATE_KEY` (never printed) in n equal outputs (default 3). It refuses any address that does not start with `kaspatest:`, before connecting.
- **Self-test.** `cargo run -p tn10 --release -- wallet-probe selftest` runs the same handlers with a stand-in signer: the TN10 key acts as the wallet.
  - The stand-in parses the Safe JSON as kaspa-wasm does, signs only the listed indices (for `nolist`, every input paying its own script), and re-serializes.
  - The self-test feeds five altered signed JSONs, which are refused.
  - It then submits a buy (indices), a buy (no list) and a close.

### Signing split (lottery)

- `Built::finish_unsigned()` (`crates/lottery/src/buy.rs`) is `finish()` without the signatures.
  - `finish()` is now `finish_inner(true)`. With `false`, the only difference is that the funding and owner signing loop is skipped.
  - It returns `(Final, funding indices)`.
  - Funding inputs keep the 66-byte zero placeholder, so the budgets, the masses and the storage-mass commit equal the signed transaction's.
  - Owner inputs (M5 D67) are refused, because their budget depends on the signed run.
  - Existing callers are unchanged: `finish()` takes the same path, byte for byte.
- New test `crates/lottery/tests/f0_unsigned.rs` (a buy with two funding inputs, a buy with referral, a close at K = 16). It checks that:
  - every byte except the funding sigscripts equals `finish()`'s;
  - the entries, units and masses are equal;
  - the unsigned transaction fails only at the funding inputs;
  - signed externally, it passes every input with the same txid.
- No covenant change.

### The JSON format: source verified

- **KasWare docs** (docs.kasware.xyz, `wallet/dev-base/kaspa-transaction`, read 2026-10-25):
  - `signPskt({txJsonString, options})`: the `txJsonString` is "in "Safe JSON" format that can be deserialized by kaspa-wasm's `Transaction.deserializeFromSafeJSON()`".
  - `options.signInputs` is `[{index, sighashType}]`, where `sighashType` 1 = All.
  - With no `signInputs`, "the wallet will attempt to sign all inputs with SighashType.All". It "only signs inputs that belong to addresses derived from the current account's public key".
  - It returns the signed Safe JSON string.
  - `pushPskt` signs and submits "as the live object rather than through a re-parse", so "a transaction carrying KIP-20 v1 covenant bindings submits with those bindings intact". It returns the txid. `pushTx(txJsonString)` broadcasts.
  - The doc's own sample is a v0 transaction with a `mass` field and no `computeBudget` or `covenant`.
  - `getNetwork` returns `kaspa_testnet_10` (the `kaspa-connection` page).
- **Kaspire docs** (kaspire.kaslab.space/developers/extension, read 2026-10-25):
  - `signPskt({sender, txJsonString, options: {signInputs: [{index, sighashType}]}})` returns signed SafeJSON and does not broadcast.
  - `pushTx(signedJsonString)` returns a txid.
  - `getNetwork` returns `"testnet-10"`.
  - There is no `pushPskt`.
- **rusty-kaspa v2.1.0:**
  - `deserializeFromSafeJSON` and `serializeToSafeJSON` are `string::SerializableTransaction` (`refs/rusty-kaspa-v2.1.0/consensus/client/src/transaction.rs:526-547`; the schema is in `consensus/client/src/serializable/string.rs`).
  - The probe uses that same type from the pinned `kaspa-consensus-client`, so there is no hand-written mirror. The crate was already in Cargo.lock through the wRPC client and is now a direct `tn10` dependency.
  - In that format, u64 fields are decimal strings, scripts and the payload are hex, and `scriptPublicKey` is the hex of the 2-byte version plus the script.
  - v1 inputs carry `computeBudget` with `sigOpCount: 0`; `string.rs:135-150` refuses the other mix.
  - Outputs carry `covenant: {authorizingInput, covenantId}`, and entries carry `covenantId`.
  - The mass is written as `storageMass`; `mass` is also accepted when reading (`#[serde(alias = "mass")]`).
  - The probe fills each input's `utxo.address` from its script: P2PK gives `kaspatest:q…`, a covenant P2SH gives `kaspatest:p…`.
- A real buy as sent (from the self-test log), trimmed:

```json
{"id":"8a89c536…","version":1,
 "inputs":[{"transactionId":"32cde66e…","index":2,"sequence":"0","sigOpCount":0,"computeBudget":35,
            "signatureScript":"5106080a…(37,378 bytes)",
            "utxo":{"address":"kaspatest:pzysuj6w…","amount":"169000000","scriptPublicKey":"0000aa20890e…87",
                    "blockDaaScore":"605615295","isCoinbase":false,"covenantId":"884eedd5…"}},
           {"transactionId":"32cde66e…","index":21,"sequence":"0","sigOpCount":0,"computeBudget":10,"signatureScript":"",
            "utxo":{"address":"kaspatest:qqdasdj3…","amount":"426447279262","scriptPublicKey":"0000201bd8…ac",
                    "blockDaaScore":"605615295","isCoinbase":false,"covenantId":null}}],
 "outputs":[{"value":"269000000","scriptPublicKey":"0000aa2045…","covenant":{"authorizingInput":0,"covenantId":"884eedd5…"}},
            {"value":"13000000","scriptPublicKey":"0000aa20808a…","covenant":{"authorizingInput":0,"covenantId":"884eedd5…"}},
            {"value":"426302025062","scriptPublicKey":"0000201bd8…","covenant":null}],
 "subnetworkId":"0000000000000000000000000000000000000000","lockTime":"0","gas":"0","storageMass":"322542","payload":""}
```

- Sizes:
  - buy: 76.5 KB of JSON (the shard sigscript alone is 37,378 B);
  - close: 440.7 KB of JSON (19 inputs, 20 outputs).

  If KasWare's popup or its message channel has a size limit, the close will show it.

### Post-sign check (spec 6.2), as implemented (`crates/tn10/src/pskt.rs`)

- The returned transaction is compared against the transaction exactly as sent: covenant sigscripts final, the inputs to sign empty.
- It is refused if:
  - the version, lock time, subnetwork, gas or payload differs;
  - the output count differs, or any output's value, script or covenant binding differs;
  - the input count differs, or any input's outpoint, sequence or compute commit differs (the compute commit is v1's form of the sig-op count);
  - any input not listed changed its signature script (this covers every covenant input);
  - a listed input came back unsigned, or not as `OP_DATA_65 <64-byte sig> 0x01` (SIG_HASH_ALL).
- A different storage-mass commit or UTXO entry is logged as a note, not refused.
  - Why it is safe: neither is ever taken from the wallet. What is sent is ours, with only the listed signature scripts copied in. The engine checks those signatures against our own entries, so a signature made over another entry or mass fails there (unit test).
  - Why not refuse: an older kaspa-wasm inside the wallet may drop or rename these fields (the doc's sample uses `mass`). Refusing on them would hide every signature result.
- Gained and lost signatures are recorded for every input.
- Unit tests (`cargo test -p tn10 --release pskt`, 3 tests):
  - the Safe JSON round trip keeps `computeBudget`, `covenant`, `covenantId`, `storageMass`, `address` and the txid;
  - an honest stand-in passes, with and without a list, and the engine accepts;
  - spec 8's mock-wallet list is refused: a covenant signature script, an output value, an output script, an extra output, the change address, an unlisted input's signature, the lock time and the payload. A dropped covenant binding, a changed sequence and a changed compute budget are refused too;
  - a changed storage mass or entry gives a note, and a signature over a lied entry fails in the engine.

### The claim-type pick: `close()` with the wallet as the caller

Options evaluated:
- **`return_deposit()` path b** (the ticket alone): needs `tx.daa >= T + D_RET`, with D_RET = 12,096,000 DAA = 14 days (`game.rs:652`, `ticket.sil` check A5b.3). The ticket's T is its shard's, and a shard only sells if it was created before T (`shard.sil:151`). So the earliest return is 14 days after a buy.
- **`return_deposit()` path a**: needs a pool of the round past its claim deadline, which is 7 days (CLAIM_WINDOW = 6,048,000) after a real draw.
- **A void** (the TEST-ONLY short-void drawer, `void-m7`): comes 2 minutes after D, but it creates no pools (`results-void-m7.md`: pools `[]`). Its tickets can then only use path b, 14 days out.
- **A claim on a forced win**: the winning numbers come from the anchor block hash after close (M7), so a win cannot be forced on a live node; the campaign's ForceWin is a dry-run hook only. A chance win needs the whole draw sequence (genesis, buys, close, about 2-3 minutes to the anchor and its 50 blue score, the draw), and usually leaves nothing to claim.
- **The `close()` caller (picked):**
  - Reachable about 1 minute after a probe genesis.
  - Shape: 18 covenant inputs (the controller, the params and 16 shards, each with its real script), then the wallet's fee input at index 18. The caller output and the marker's reserve funder are the wallet.
  - Size: mass about 432,000, 440 KB of JSON.
  - Why: it is the same pattern as `claim()` (covenant inputs plus the user's P2PK input; claim has 2 covenant inputs, close has 18). It is the strongest test of "the Safe JSON round trip at our masses" (spec 6.2, 12). It is also journey J4.
- **The `draw()` caller**: the same pattern (4 covenant inputs plus the fee input). Reachable about 3 minutes after a close, but it needs the anchor-proof path in the probe. Not built; it can be added if part B wants a second covenant shape.
- A real `claim()` from a wallet is first exercised on the simnet in F4, and on testnet-10 in the F6 journeys.

### Commands run and their output (part A)

- `cargo test -p lottery --release --test f0_unsigned`: `buy_unsigned_is_finish_without_signatures ... ok`, `close_unsigned_is_finish_without_signatures ... ok`; `test result: ok. 2 passed`.
- `cargo test -p tn10 --release pskt`: `safe_json_round_trip_keeps_every_field ... ok`, `honest_wallet_passes_and_engine_accepts ... ok`, `mock_wallet_alterations_are_refused ... ok`; `test result: ok. 3 passed`.
- **Final run on the final binary**: `cargo run -p tn10 --release -j 2 -- wallet-probe selftest` (log `tn10-data/wallet-probe/1792901131-selftest.jsonl`).
  - The same five alterations were refused at the post-sign check.
  - Accepted: buy/indices `43cec33fe34d2b22b88a1a91e6089fe62f4c9c671841d946ab76b3036d24c652`, buy/nolist `0ec079c45ed8f7d86a6b23d0ec0f87c4239d7f181344b0aec11afbfce65b765f`, close/indices `f80a2b45869ebedcfacb50b139032091cb2454d06f2eef2ab6c73b36378e8298`.
  - The next instance (`53d5c8f6…`) was made right after the close.
- The earlier run in full (log `tn10-data/wallet-probe/1792895009-selftest.jsonl`):

```
[connect-refused] standin reports network `kaspa_testnet_11`, expected `kaspa_testnet_10` (testnet-10); refusing
[connect-refused] `kaspadev:qqdasdj3…` is not a kaspatest: address; refusing
[instance] instance 1: genesis 32cde66e384177a73390d9a3ba4821aa9e91b88a407bc1128841660ef97c9505 at DAA 605615295, covenant id 884eedd5…, T = 605615884
[post-sign] #1: REFUSED (safe JSON); gained [1], lost []; output 1 value: built 13000000, returned 1
[post-sign] #2: REFUSED (safe JSON); gained [1], lost []; input 0 (not listed, covenant): signature script changed: built 37378 B (blake2b a4b38f686cf33357), returned 1 B (…)
[post-sign] #3: REFUSED (safe JSON); gained [1], lost []; output count: built 3, returned 4; output 3 extra: …
[post-sign] #4: REFUSED (safe JSON); gained [1], lost []; lock time: built 0, returned 1
[post-sign] #5: REFUSED (safe JSON); gained [1], lost []; output 2 script: built v0 `201bd836…ac`, returned v0 `2056b328…ac`
[build] #6 buy/indices … 2 inputs, 3 outputs, sign [1], fee 0.32254200 KAS, masses { compute: 43472, transient: 151488, storage: 322542, priority: 322542 }
[post-sign] #6: PASS (safe JSON); gained [1], lost []
[engine] #6: local engine accepts every input
[submit] #6: ACCEPTED by the node, txid dd8240960bb58145dbcb069370eb17b115b18c2487f614ce32b8509d4db7f67b
[post-sign] #7: PASS (safe JSON); gained [1], lost []
[submit] #7: ACCEPTED by the node, txid fcc55f2a0104079a341beb1d56baece959a5ebb45da625f2ca932285f3360cdb
[build] #8 close/indices … 19 inputs, 20 outputs, sign [18], fee 0.43230600 KAS, masses { compute: 293743, transient: 864612, storage: 420885, … }, JSON 440749 B
[post-sign] #8: PASS (safe JSON); gained [18], lost []
[engine] #8: local engine accepts every input
[submit] #8: ACCEPTED by the node, txid aac1401c92e2625f7efd7c14285c880283b29380fa303fe5eea212d972f55a1f
[instance] instance 2: genesis 29610df6… (the next instance, made right after the close)
[selftest-done] PASS: buy/indices dd824096…, buy/nolist fcc55f2a…, close/indices aac1401c…
```

- An earlier run (`1792894758-selftest.jsonl`) also had both buys and the close accepted (`63b99f2c…`, `c5b3ba22…`, close `e2eee9a5…`). It showed one defect, now fixed: the next instance's genesis picked the UTXO the close had just spent. That only happens when the "wallet" is the TN10 key. Genesis now skips spent outpoints, and the next-instance result is logged.
- HTTP smoke test (`wallet-probe serve`, then curl):
  - `GET /` serves the page with all 19 `data-testid`s;
  - `GET /api/state` returns the instance, the DAA and `close_in_s`;
  - `connect` with network `kaspa_testnet_11` is refused, and so is a `kaspasim:` address;
  - a POST with `Origin: https://evil.example` gets 403;
  - `build` with no wallet gives `no wallet connected`.
- `tn10 send`: a `kaspadev:…` address is refused before connecting. `tn10 send <TN10 address> 1 --parts 2` was accepted (txid `4e8f8abe2fe5b6e0d104603a5baa8362884b702cba85ca45adc228c013e95630`).
- Suite: see "Suite at the end of part A" below.

### Files changed (part A)

- `crates/lottery/src/buy.rs`: `finish_unsigned`, and `finish` split into `finish_inner`.
- `crates/lottery/tests/f0_unsigned.rs` (new).
- `crates/tn10/src/pskt.rs` (new): Safe JSON and the post-sign check.
- `crates/tn10/src/wallet_probe.rs` and `crates/tn10/src/wallet_probe.html` (new).
- `crates/tn10/src/main.rs`: the `wallet-probe` and `send` commands.
- `crates/tn10/src/cases.rs`: `wait_daa`, `sink` and `merged` are now `pub(crate)`.
- `crates/tn10/Cargo.toml`: tokio `net`, `io-util` and `sync`; `kaspa-consensus-client` (same rev), `serde`, `serde_json`, `blake2b_simd`.
- `crates/Cargo.lock`: 4 dependency lines for tn10.

### Open questions and notes for the orchestrator

- **A concurrent session edited the tree during part A.**
  - Files: `crates/lottery/src/close.rs`, `crates/lottery/tests/m8_index.rs`, `crates/tn10/src/{campaign,fees,indexer}.rs` and M8 docs. It looked like M8 S5 work. Part A did not touch these files.
  - Midway, 6 of its new test-first tests failed in `cargo test -p tn10`. By the end of part A its changes were no longer in the working tree (committed), and `cargo test -p tn10` was 25/25 green.
  - The F0 files are listed below; commit them on their own.
  - That session also uses `TN10_PRIVATE_KEY`. Do not run the probe while another session submits with that key, or both may pick the same UTXO.
- **Kaspire is not available to the owner** (KasWare in Chrome only). The Kaspire buttons and their `signPskt` + `pushTx` path are built but **untested**. Kaspire's network string (`testnet-10`) and its `sender` parameter come from its docs only.
- **An abandoned build** (the page reloaded while the wallet window was open) holds its shard and funding for 10 minutes, then is released and logged as `expired`. Rejecting in the wallet releases it at once.
- **Proposed DECISIONS rows** (for the orchestrator to record):
  - (a) F0 builds natively, in a `tn10` probe, not in `crates/web`.
  - (b) The claim-type transaction is the `close()` caller's fee input (reasons above).
  - (c) `Built::finish_unsigned` is the signing split; existing callers are unchanged.
  - (d) The post-sign check treats storage-mass and UTXO-entry differences as notes, because neither is taken from the wallet. Spec 6.2's list holds in full, plus the check on the signature's form.
  - (e) A `pushPskt`/`pushTx` path cannot run the post-sign check before sending; the probe checks the txid and the submitted transaction after the fact. For F3: if only the push path works in KasWare, the page cannot refuse before sending. That is then an owner decision:
    - trust: the wallet sends what it signed, and the chain still enforces every covenant;
    - money: at worst, the wallet's own funds;
    - work: none.
- **Unknowns that part B settles:**
  - whether KasWare's wasm parses v1 Safe JSON with `computeBudget`, `covenant`, `covenantId` and `storageMass`;
  - whether `signPskt` keeps covenant sigscripts and bindings (the doc says only `pushPskt` keeps "the live object");
  - what KasWare does with no list, and with 440 KB;
  - the same for Kaspire, whose docs say it may refuse "generic covenant requests".
- **Cost:** each probe instance locks about 94 testnet KAS of `TN10_PRIVATE_KEY` (the balance is about 5,700 KAS after part A). Each owner action costs 0.32-0.43 KAS in fees from the owner's testnet wallet.

## F0 part B: owner steps

This part needs no coding and takes about 20-30 minutes. Everything uses testnet-10 coins, which have no value.

**What you trust and what it costs.**
- You install KasWare, a Kaspa wallet made by a third party, as a Chrome extension.
- You create a new account in it, used only for this test.
- The project's own testnet key sends that account 30 test KAS, so no faucet is needed.
- Each button press spends about 0.3-1.5 test KAS from that account.
- Never put real (mainnet) coins in this account, and never type in the recovery phrase of a real wallet.

### 1. Set up KasWare (once)

1. In Chrome, open the Chrome Web Store and search for **KasWare Wallet**. Check that the publisher is KasWare (kasware.xyz), then click **Add to Chrome**.
2. Click the KasWare icon (pin it from the puzzle-piece menu) and choose **Create new wallet**. Set a password and write down the recovery phrase. It is a test wallet, but keep the phrase private anyway.
3. In KasWare's settings (or the network selector at the top), choose **Kaspa Testnet 10**.
4. Click your address to copy it. It must start with `kaspatest:`. Send it to the chat.

### 2. The orchestrator runs (from `crates/`)

1. Fund the address: `cargo run -p tn10 --release -j 2 -- send <the kaspatest: address> 30`. This sends 3 outputs of 10 KAS. Wait for "sent 30 KAS …".
2. Start the probe and leave it running: `cargo run -p tn10 --release -j 2 -- wallet-probe serve`.
   - It takes about 10-60 s to print `[serve] open http://localhost:8710/`.
   - It then prints one line per step.
   - Stop it with Ctrl+C when done.

### 3. In Chrome, open http://localhost:8710/

1. **Under KasWare, press Connect.** Approve the connection in the KasWare popup. The KasWare line then shows your address and about 30 KAS. If it says "refused: network …", press **Switch to testnet-10** (or switch inside KasWare) and connect again.
2. **Press the KasWare buttons in this order.** Do one at a time, and wait for its green or orange line in the log before pressing the next.
   1. **buy · 1 signPskt, P2PK indices.** KasWare opens a signing window; approve it. Expected: a green "ACCEPTED by the node" line with a transaction ID.
   2. **buy · 2 signPskt, no list.** Approve. If KasWare shows a warning or an error, take a screenshot first. Then approve, or reject if it will not sign.
   3. **buy · 3 pushPskt.** Approve. KasWare sends the transaction itself, and the page shows its ID and a check result.
   4. **close · 4 signPskt, P2PK indices.** Press it only when the top line says "close() open" (about a minute after the server started). This transaction is large, so the window may take a few seconds to open. Approve.
   5. **close · 5 signPskt, no list.** After each successful close the server makes a new round. Wait until the top line says "close() open" again (about 1-2 minutes), then press and approve.
   6. **close · 6 pushPskt.** Same as button 5: wait for "close() open", press, approve.
3. **What to approve.**
   - Approve only the KasWare signing windows these buttons open. Each one is for your test account and spends a little test KAS.
   - If a window shows something you do not understand, take a screenshot and approve anyway (this is test money). The server refuses to send anything that was changed.
   - If a button gives an orange line, note it and go on to the next button.
4. **Kaspire: skip it.** It is not installed (KasWare only), so Kaspire stays untested in F0. For a later run:
   1. Install the Kaspire extension (kaspire.kaslab.space) and create a new account.
   2. Choose the network **TN10**.
   3. Send its `kaspatest:` address to the chat; it is funded the same way.
   4. Press **Connect** under Kaspire, then buttons 1-6 under Kaspire, in the same order and with the same waits. Buttons 3 and 6 are "signPskt + pushTx" there, because Kaspire has no pushPskt.

### 4. What to send back

- "Done", plus which buttons gave green lines and which gave orange ones. A screenshot of the page's log is enough.
- Screenshots of any KasWare (or Kaspire) window that showed a warning or an error, or refused.
- Nothing else. Every step, including the exact transactions and the wallet's answers and errors, is in the log file `tn10-data/wallet-probe/<number>-serve.jsonl`, which the orchestrator reads.

### For the orchestrator, after part B

- Read the newest `tn10-data/wallet-probe/*-serve.jsonl`. For each variant, look at:
  - `post-sign` (pass, refusals, notes, gained);
  - `engine`;
  - `submit` or `push-check`;
  - `wallet-error` (and its `raw`).
- One-liner to list the steps: `python -c "import json,sys;[print(o['step'],'|',o.get('summary','')) for o in map(json.loads,open(sys.argv[1]))]" <file>`.
- Record the results in `docs/tn10-mass-check.md` and as a DECISIONS row (spec 9 F0). Tell the owner whether the wallet path holds. If KasWare fails, the beta ships with the quick wallet only (spec 6.2).

### Suite at the end of part A

- `cargo build --tests -j 2`: `Finished `dev` profile [unoptimized + debuginfo] target(s)`.
- `cargo test -p lottery --release -j 2`: exit 0, 40 test binaries, 1,062 passed, 0 failed, 1 ignored (the pre-existing `m4_mutants` ignore). This includes `f0_unsigned`: `test result: ok. 2 passed`.
- `cargo test -p tn10 --release -j 2`: `test result: ok. 25 passed; 0 failed`, including the 3 `pskt` tests.

## F1 part A (stage agent, 2026-10-25)

### What was done

1. **Templates embedded at build time** (spec 6.1, W2).
   - `crates/lottery/templates/`: the 11 compiled templates as SilverScript ABI artifacts in the `silverc` JSON format (`silverscript_abi::to_pretty_json`), plus `b_max.in` (the committed B_MAX, 4). 795,518 B of JSON; 157,113 B of it is script. The artifacts contain no machine paths (`source_path` is `sil/<Name>.sil`).
   - `lottery::embedded` includes them with `include_str!`, parses them once and runs `SilAbiArtifact::check_consistency` on each.
   - `Templates::get(b_max())` now uses the embedded set in every build, native too. Any other B_MAX is compiled, which needs the `compiler` feature. The new `Templates::try_get` returns an error instead of panicking. `b_max()` is the embedded constant, and `source_b_max()` (compiler only) reads shard.sil.
   - Generator: `cargo run -p lottery --bin regen-templates` (from `crates/`; the bin needs `compiler`). It writes only the files that changed.
   - W2: `crates/lottery/tests/f1_templates.rs`, 4 tests:
     - every generated file equals the embedded file;
     - the embedded B_MAX equals shard.sil's;
     - the runtime templates equal a fresh compile (artifact, prefix, suffix and hash);
     - another B_MAX still compiles.

     Each failure message gives the regenerate command. Checked by hand: with one embedded file altered (`compiler_version` in `marker.json`), 2 W2 tests failed with "regenerate them from crates/ with: cargo run -p lottery --bin regen-templates". Running the generator restored the file byte for byte.
2. **Compiler behind a feature.**
   - `harness` and `lottery` have a `compiler` feature, on by default. `harness/compiler` makes `silverscript-lang` optional and gates `compile_file` and `compile_source`.
   - In `lottery`, `compiler` gates the `.sil` source paths: `Sources`, `ticket_source`, `shard_source`, `read_covenant`, `Templates::compile*`, `compile_drawer` and the `dummy_*` placeholder states.
   - `lottery` depends on `harness` with `default-features = false`.
   - `cargo test` from `crates/` is unchanged.
   - The TEST-ONLY short-void drawer variant (all seven SP12 symbols) is now gated on `all(not(feature = "mainnet"), feature = "compiler")`, where it was `not(mainnet)`. A build without the compiler, such as the web build, has none of them.
   - SP12's probe crate (`m7_seed.rs`) now enables `lottery/compiler` in its positive control. Its mainnet check still fails on every symbol.
3. **One rusty-kaspa version.**
   - `crates/vendor/silverscript-abi/` is a copy of SilverScript v1.0.0's `silverscript-abi` (`3ed97333`, ISC, LICENSE copied) built against v2.1.0. `[patch."https://github.com/kaspanet/silverscript"]` in `crates/Cargo.toml` puts it in place of the git crate everywhere, including inside `silverscript-lang`.
   - The compiler still links `a41a333`, natively only.
   - It is a non-default workspace member, so its upstream unit tests run with `cargo test -p silverscript-abi`.
   - The diff from upstream is 10 lines added and 4 removed, each marked `Kaspalionare F1 vendor patch`:
     - `script_builder()`: `ScriptBuilder::with_flags(EngineFlags { covenants_enabled: true, .. })` becomes `ScriptBuilder::new()`. v2.1.0 deleted the flag. Its builder always applies the limits the flag selected (1,000,000-byte scripts and elements, explicit push opcodes allowed), and it stores its flags without reading them.
     - `deserialize_fixed_i64`: `deserialize_i64(bytes, false)` becomes `deserialize_i64(bytes)`. v2.1.0 dropped `enforce_minimal`, and its function is the `false` branch.
     - one unit test reads `max_signature_script_len` (renamed in v2.1.0; the value is still 250,000).
   - Cargo.lock: `silverscript-abi` now comes from the path and depends on `kaspa-txscript 2.1.0` and `kaspa-consensus-core 2.1.0`. Apart from that, only `web` is added.
   - `cargo tree -p web --target wasm32-unknown-unknown -e normal`: 0 crates at `a41a333`, 0 `silverscript-lang`, one `kaspa-consensus-core` (v2.1.0).
4. **wasm toolchain.**
   - `wasm32-unknown-unknown` was already installed for 1.94.0.
   - getrandom 0.2.17 needed nothing.
   - getrandom 0.3.4 (through ahash, from kaspa-txscript's arkworks crates) needs two things:
     - its `wasm_js` feature: `lottery/Cargo.toml` has a wasm32-only target dependency `getrandom = "=0.3.4"` with `wasm_js`;
     - its cfg: `crates/.cargo/config.toml` sets `--cfg getrandom_backend="wasm_js"` for that target only.
   - `secp256k1-sys` needs a wasm-capable clang and ar. The pinned LLVM 23.1.3 at `C:\Program Files\LLVM\bin` has `clang.exe` (wasm32 target), `llvm-ar.exe` and `wasm-ld.exe`. They are given by environment variables, not in the config, because the path is machine-specific. The exact commands for both shells are in CLAUDE.md, under "wasm build".
   - `crates/web` (new) is a cdylib with one export, `f1_link_probe`. It runs `lottery::digest::all_lines()`, which keeps every builder, the engine, the mass calculator and the ABI codec in the binary. Its dependency is `lottery = { default-features = false }`, and it has a `mainnet` feature.
5. **Size measured** (`crates/web`, release, no compiler):

| opt-level | raw | gzip -9 | code | data | `name` section | `__wasm_bindgen_unstable` section |
|---|---|---|---|---|---|---|
| 3 (default release) | 5,432,679 B | 2,095,987 B | 2,221,598 | 2,339,744 | 471,181 | 259,359 |
| "s" (as the probe) | 5,010,233 B | 1,955,816 B | 1,604,814 | 2,327,329 | 676,202 | 259,359 |

   - The probe had 6.97 MB at "s" with the compiler. Without the compiler it is 5.01 MB, and about 4.07 MB once the `name` and wasm-bindgen sections are removed (wasm-bindgen and wasm-opt remove them).
   - The data section carries the 795 KB of pretty JSON. A compact encoding would save about 230 KB raw (compact JSON is 568,526 B), more with a binary form.
   - 128 KB of exports are rusty-kaspa's own `#[wasm_bindgen]` items (`networkid_*` and others), which kaspa-consensus-core compiles in on wasm32.

### Encoding identity: the evidence (no stop-and-ask needed)

- **Golden digests written before any F1 change** (`crates/lottery/tests/fixtures/f1_digests.txt`, 77 lines, by `lottery::digest`). At that point only `digest.rs` itself had been added; templates were still compiled from disk and the ABI was on `a41a333`.
  - **ABI part.** For each of the 11 templates:
    - the template hash and the artifact JSON;
    - every entry's signature script on 12 argument sets plus 2 error cases;
    - the runtime state on 12 value sets: encode, decode round trip, and the decode of 3 altered byte strings (one flipped bit, truncated, trailing push).

    The values cover the script-number boundaries (0, ±1, 16/17, ±127/128/129, 255/256, 32,767/32,768, 2^31, 2^40, ±i64::MAX, i64::MIN), push lengths 0/1/75/76/255/256/520/521/1,000, and single bytes 0x00/0x01/0x05/0x10/0x11/0x80/0x81. A sample run (`F1_DIGEST_VERBOSE=1`) had 380 successful encodings, 107 decodes and 96 altered-byte outcomes, plus errors.
  - **Transaction part.** 21 fixture transactions:

    | Builder | Cases |
    |---|---|
    | buy | b = 1, b = 1 with referral, b = 4 with referral and two funding inputs |
    | close | K = 16, K = 4 |
    | draw | K = 16, K = 4, and a void at K = 16 |
    | claim | k = 1, k = 16 |
    | return and refresh | `return_deposit` after the window, refresh of a ticket, refresh of a pool |
    | params | `set_params` |
    | vault | `request_deposit`, `request_withdraw`, both settlements |
    | genesis | K = 4, K = 16 |

    Each digest covers the whole transaction (`Debug`), the entries, the script units, the masses and the storage-mass commit. Random-nonce signatures are replaced by their length; they are detected as the signature scripts that differ between two `finish()` runs.
- **After the embedded templates and after the `[patch]`:** `cargo test -p lottery --release --test f1_bytes` gave `f1_encoding_and_transactions_unchanged ... ok`. All 77 lines are identical.
- **W2 after the patch:** a fresh compile through the patched ABI types equals the files generated before the patch, byte for byte.
- **Upstream ABI unit tests on v2.1.0:** `cargo test -p silverscript-abi --release` gave `test result: ok. 21 passed; 0 failed`.
- **Source check** of every txscript item the ABI calls (`git` in `~/.cargo/git/db/rusty-kaspa-*`):
  - `a41a333` is an ancestor of v2.1.0 (`git merge-base --is-ancestor`: yes, 13 commits);
  - these bodies are identical in both revisions: `serialize_i64`, `ScriptBuilder::canonical_data_size`, `add_data`, `add_raw_data*` and `explicit_push_encoded_size`;
  - `add_i64` changed only in its size limit, which is 1,000,000 in both revisions when covenants are on;
  - the opcode values `Op0`, `Op1`, `Op1Negate`, `Op16`, `OpData1`, `OpData75` and `OpPushData1/2/4` are identical (md5 of their definition lines);
  - `deserialize_i64(v, false)` at `a41a333` is the v2.1.0 function line for line.
- **The full suite** (below) runs every M1–M8 transaction through the v2.1.0 engine with the patched ABI.

### Commands run (outputs)

- `cargo test -j 2 -p lottery --release --test f1_bytes -- --ignored write_golden` (before the changes): `wrote 77 lines`.
- `cargo test -j 2 -p lottery --release --test f1_templates --test f1_bytes` (after the patch): `w2_* ... ok` ×4, `f1_encoding_and_transactions_unchanged ... ok`.
- `cargo test -j 2 -p silverscript-abi --release`: `21 passed; 0 failed`.
- `cargo check -j 2 -p lottery --no-default-features` and `cargo check -p lottery --no-default-features --features mainnet`: both `Finished`, with no warnings.
- `cargo check -j 2 -p lottery --no-default-features --target wasm32-unknown-unknown` (with the CC and AR variables): `Finished`. Without them it fails at `secp256k1-sys` with `failed to find tool "clang"`. Without the getrandom settings it fails at `getrandom 0.3.4`, "wasm32-unknown-unknown targets are not supported by default".
- `cargo build -j 2 -p web --target wasm32-unknown-unknown --release`: `Finished release profile [optimized] target(s) in 2m 06s`. No linker flag, no duplicate symbols. `web.wasm` is 5,432,679 B.
- The same build with `--config 'profile.release.opt-level="s"'`: 5,010,233 B.
- Suite: see "Suite at the end of F1 part A".

### Files changed (F1 part A)

- `crates/Cargo.toml`: members `web` and `vendor/silverscript-abi` (not default); the `[patch]`.
- `crates/Cargo.lock`: the abi entry (path source, v2.1.0 dependencies), plus `web`.
- `crates/.cargo/config.toml` (new): the getrandom cfg for wasm32.
- `crates/harness/Cargo.toml` and `src/lib.rs`: the `compiler` feature.
- `crates/lottery/Cargo.toml`: features `default = ["compiler"]` and `compiler`; the `regen-templates` bin; `harness` with `default-features = false`; the wasm32 getrandom dependency.
- `crates/lottery/src/buy.rs`: compile paths gated; embedded `b_max()`; `source_b_max()`; `Templates::{get, try_get, embedded}`; short-void cfg.
- `crates/lottery/src/draw.rs`: `compile_drawer` and `dummy_drawer` gated.
- `crates/lottery/src/genesis.rs`: short-void cfg.
- `crates/lottery/src/lib.rs`: modules `digest` and `embedded`, plus docs.
- New in `crates/lottery`:
  - `src/embedded.rs`;
  - `src/digest.rs`;
  - `src/bin/regen-templates.rs`;
  - `templates/` (12 files);
  - `tests/f1_templates.rs`;
  - `tests/f1_bytes.rs`;
  - `tests/fixtures/f1_digests.txt`.
- `crates/lottery/tests/m7_seed.rs`: the SP12 probe's positive control enables `compiler` (1 manifest line and a doc comment).
- `crates/vendor/silverscript-abi/` (new): Cargo.toml, LICENSE, src.
- `crates/web/` (new): Cargo.toml, src/lib.rs.
- `CLAUDE.md`: the layout line, the templates rule (it replaces "compiled at test time") and the wasm build command.
- No `covenants/*.sil` change; nothing in `refs/` (it is empty on this machine; the sources were read from `~/.cargo/git`).

### Proposed DECISIONS rows (for the orchestrator)

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

### What part B must do

- **The `crates/web` API** (spec 6.1): wasm-bindgen, decode, build, verify and estimate, grouped by entry point. Replace `f1_link_probe`. Add `wasm-bindgen` at the version rusty-kaspa v2.1.0 already links (in Cargo.lock), so there is a single copy.
- **`Result` everywhere and W3:**
  - `Template::from` asserts (`expect("state span")`, the template-hash `assert_eq!`), and `Template::state_bytes` panics on encode errors;
  - the builders panic in many places (`unwrap_or_else(panic)` in `harness::entry_sigscript` and others).

  Every function reachable from the page must return `Result` (wasm uses `panic = "abort"`).
- **The signing split for every case:** `Built::finish_unsigned` exists (F0) but refuses owner inputs (M5 D67).
- **W1:** run `lottery::digest::all_lines()` in wasm (under node or wasm-bindgen-test) and compare with `tests/fixtures/f1_digests.txt`. Note:
  - `finish()` signs with random nonces through getrandom, so the JS host needs `crypto`;
  - the digest masks those signatures, so the lines are host-independent.
- **W4:** a `mainnet` build of `crates/web` refuses to start and has no quick-wallet symbol. `web` already has a `mainnet` feature that passes through to `lottery/mainnet`.
- **Size:** optionally store the embedded templates compactly (compact JSON or a binary form), and strip the `name` section with wasm-opt. Record the compressed size again.

### Open questions

- None that need the owner. Nothing changed encoding, so the stop-and-ask rule was not triggered.
- The wasm binary has been linked but not yet run in a JS host; part B's W1 runs it.

### Suite at the end of F1 part A

- `cargo build --tests -j 2`: `Finished dev profile [unoptimized + debuginfo] target(s) in 1m 51s`.
- `cargo test -j 2` (from `crates/`, debug; default members `harness` and `lottery`): 48 binaries ran, 1,088 passed, 0 failed, 2 ignored (`f1_bytes::write_golden` and the old `m4_mutants` one).
  - This includes `f1_bytes` 1/1, `f1_templates` 4/4, `f0_unsigned` 2/2 and `m7_seed` 15/15 (SP12: `mainnet,test-seed` fails, `mainnet` builds, and the probe has every short-void symbol and loses them under `mainnet`).
  - The last binary, `spike_vdf`, overflowed its stack in debug (`sv_measure_masses_and_stack`, STATUS_STACK_OVERFLOW). This was already true before F1 (DECISIONS D113: "`spike_vdf` overflows its stack in debug as before S1; 14/14 in `--release`").
- `cargo test -j 2 -p lottery --release --test spike_vdf`: `14 passed; 0 failed` (358 s).
- `cargo test -p lottery -p harness --doc`: 0 doc tests, ok.
- `cargo test -j 1 --release -p tn10`: `25 passed; 0 failed`.
- `cargo check -p web --features mainnet --target wasm32-unknown-unknown`: `Finished`.
- Not built: `simnet` (RocksDB, about 14 min). It uses `lottery` with default features, whose API is unchanged except the additions.

## F1 part B (stage agent, 2026-10-25)

### What was done

1. **The page-facing layer `lottery::web`** (new, `crates/lottery/src/web/`). Everything a browser page reaches goes through it, and every function returns `Result<_, web::Error>`. Part C wraps it with wasm-bindgen; nothing in it is wasm-specific.
   - `web::verify`: decoders, chain checks and ranges.
     - Decoders: `decode_state(role, bytes)` (all 11 roles, the indexer API's and `bundle/1`'s form, re-encoding checked), `decode_redeem`, `state_bytes`, `state_spk`, `anchor_proof`, and the store readers `read_wallet` (`wallet/1`), `read_index_log` and `read_index_snapshot` (`index/1`), `read_bundle` (`bundle/1`, verified with `bundle::verify`).
     - Chain checks (spec 6.3): `verify_state(role, bytes, utxo_spk)` (`chain_mismatch` = "the indexer's copy does not match the chain"); `rebuild_tree(m, purchases)` and `rebuild_tree_bytes` (compact six-byte purchases in shard order, canonical and count-checked); `check_roots(trees, roots, k)` (`tree_mismatch`); `check_anchor(proof, d)`; `winning_numbers` and `winning_numbers_from_anchor` (the D42/D94 seed mapping recomputed from the anchor's X.hash).
     - Ranges: `check_shard`, `check_ticket`, `check_pool`, `check_controller`, `check_params`, `check_marker`, `check_drawer`, `check_vault`, `check_position`, `check_request`, `check_record`. Each mirrors the covenant's own range checks on its state (the `.sil` lines are cited in the code). No chain-valid state fails one, because each covenant refuses to create such a state. The builders apply them before any arithmetic, so the Rust reference never leaves its domain.
   - `web::build`: one unsigned builder per entry point: `buy`, `claim` (and `claim_from_store`), `return_deposit` (both paths), `close`, `draw` (with an anchor proof; the void without one) and `draw_from_store`, `request_deposit`, `request_withdraw`, `settle`; plus `fee_of`, `with_fee` (the tn10 fee loop, same caps: 1,000 sompi/gram, 5 KAS per transaction) and `w_min`. Each takes the entry point's `*Case`, the covenant inputs' outpoints and `Funds` (the wallet's coins, its change script, the fee). Then:
     1. It checks the funds (1 to 8 confirmed, zero-ID Schnorr P2PK coins; a standard change script; the fee within the cap) and every case field the builder reads (the ranges above, the reference's preconditions, the fixture-only knobs at their defaults).
     2. It builds with the library's own builder (`build_with`, the native code path), the funding and change on a placeholder key.
     3. It puts the wallet's coins and change script in place (`bind`) and refuses the transaction if the placeholder key's script is left anywhere.
     4. It returns `sign::unsigned`.
   - `web::sign`: the signing split.
     - `unsigned(&Built)` returns `Unsigned`: the transaction as sent to the wallet (the inputs to sign with an empty signature script, every covenant signature script final, the budgets and storage-mass commit of the signed transaction), the entries, `sign: Vec<SignInput>` (index and the owner each input's script names), the masses of the signed transaction, the fee and the units. Every covenant input runs in the engine under its committed budget first (`engine` otherwise), as the F0 probe required.
     - `post_sign` is the spec 6.2 check, moved here from `tn10/src/pskt.rs` with the same refusal texts. It is generalized to owner inputs: the form is checked per input (Schnorr or ECDSA `OP_DATA_65 <64-byte sig> SIG_HASH_ALL`; P2SH that push followed by `OP_DATA_34 <the position's redeem>`).
     - `assemble(&Unsigned, returned, returned_entries)` builds the final transaction: the check, then OUR transaction with only the listed inputs' signature scripts taken from the wallet's, then the masses compared with the built ones, then the engine on every listed input against our entries. The output is ready to submit.
     - `signers_for` gives the tn10 F0 probe its forms. `blanked` and `with_signatures` are the plain helpers.
   - `web::Code` and `web::Error { code, detail }`: 26 stable codes (list below), each with a number, a snake-case name and one message (spec 1.1 AX).
2. **The signing split for every case** (`crates/lottery/src/buy.rs`).
   - `Built::finish_unsigned()` now covers owner inputs; F0's refusal is gone. The new `InKind::OwnerPub(OwnerPub)` is an owner input whose owner signs outside the library: the public side of a D67 key (`Schnorr(x-only)`, `Ecdsa(33 bytes)`, `P2sh(x-only of the wrapped Schnorr redeem)`). `finish()` refuses it. An `InKind::Owner(OwnerKey)` input counts as its public side in `finish_unsigned`.
   - `Built::external_signers()` lists the inputs to sign: funding inputs, whose script must be a Schnorr P2PK, and owner inputs.
   - `finish()` and `finish_unsigned()` share `try_finish_inner(sign)`: the old body, with `?` where it panicked. On success it computes exactly what it did before; `finish()` unwraps it with the same panic text.
   - The owner-input budget is designed and proven below.
3. **No panics on untrusted input** (spec 6.1, W3).
   - New fallible primitives, each with the old function as a thin wrapper (same bytes, same panic on failure):
     - `Template::try_from_artifact` (the embedded templates load through it), `try_state_bytes`, `try_redeem`;
     - `try_entry_sigscript`, `try_planned_sig_script`, `try_push_redeem`;
     - `harness::try_budget_for` and `try_masses`;
     - `Final::try_fee`, `DrawCase::try_expected`, `Tree::try_increment`;
     - `State::try_redeem`, `try_state_bytes`, `from_values`.
   - The decoders' re-encoding (`State::decode`, `State::from_state_bytes`) now uses `try_redeem`: a value the decoder accepts but the encoder refuses is an error.
   - Fixed in place (each was found by the audit or by W3):
     - `ClaimCase::comb()` computed `6 * idx` unchecked (W3: an idx near i64::MAX overflowed).
     - The pinned storage-mass calculator divides by every output value and by input values on its relaxed path, and sums input values unchecked (rusty-kaspa v2.1.0 `consensus/core/src/mass/mod.rs:458-466, 489-494, 502-503`; W3: a zero-value output divided by zero). `try_masses` refuses a zero-value output and an input total above u64 (neither ever gave a mass), and `sign::unsigned` refuses a zero-value input (no such UTXO exists on chain).
     - `DrawCase::fix_lock_time` no longer panics when D + draw_window >= DAA_MAX (`try_expected` refuses at check 19).
     - `DrawCase::from_store` uses `try_expected`, so it no longer relies on `index::model`'s `catch_unwind`.
     - `bundle::verify` uses `Templates::try_get` (a file B_MAX other than the embedded one is an error without the compiler) and bounds K <= K_MAX before it sizes the genesis case.
     - `ResultJson::result` bounds k, the roots and m before `RoundResult::pool_state` packs them.
     - `Wallet::shard_tree` uses `try_increment` (32,768 updates of one combination overflowed the count).
     - `Wallet::trees` uses `get(i)` for the frozen roots.
     - `Wallet::apply_claim` bounds the claimed index before the shift and uses `first()` and `get()` for the inputs and the claimed byte.
4. **W3** (`crates/lottery/tests/f1_w3.rs`): the property test, a seeded deterministic fuzz loop. Coverage is in the list below.
5. **Line endings.**
   - `.gitattributes` (new) sets `-text` on `crates/lottery/templates/**` and `crates/lottery/tests/fixtures/f1_digests.txt`: no end-of-line conversion for them, `core.autocrlf` included, so a Windows clone holds the committed LF bytes.
   - `git check-attr` reports `text: unset` for them, and `git ls-files --eol` shows `i/lf w/lf attr/-text` for all 13 files.
   - Byte sensitivity: `include_str!`'s JSON is whitespace-insensitive, `embedded_file` and `regen-templates` normalize CRLF, and `f1_bytes` compares with `str::lines()`. So the tests already tolerated CRLF, which the experiment below shows. The attribute pins the embedded bytes anyway.
6. **tn10** keeps its Safe JSON. `pskt::post_sign`, `Check` and `blanked` now call `lottery::web::sign`, with identical refusal texts; `pskt::assemble` is unchanged.
7. **CLAUDE.md**: one command line for the F1 tests.
8. **No covenant change.** No `covenants/*.sil`, no template, no golden digest touched.

### The owner-input budget: design and proof

- **The problem.** `finish()` budgets an owner input (D67) at one signature check (P2PK_UNITS = 100,000), signs it, runs it, and re-budgets it if the signed run needs a larger budget, which a P2SH owner might. Without the key there is no signed run.
- **The design** (`buy::owner_signed_units`). An owner input's script is a P2PK Schnorr, a P2PK ECDSA, or a P2SH-wrapped Schnorr P2PK. It reads no other input or output, and its pushes have fixed sizes (a 65-byte signature push, a 32- or 33-byte key, a 34-byte redeem). So the signed run's script units are a constant of the class, independent of the key, the signature and the surrounding transaction.
  - The library measures each class once, by signing a one-input throwaway transaction with a fixed stand-in key of that class and running it.
  - `finish_unsigned` then applies `finish()`'s own rule: the measured units if their budget exceeds one signature check's, else P2PK_UNITS. That is exactly where `finish()`'s re-run loop ends, and the placeholder has the signed script's size.
- **The proof** (`tests/f1_unsigned.rs`).
  - `owner_budget_depends_on_the_class_only` uses 24 random keys of each class, all 72 valid. For each it checks that the signed run's units equal the class constant, that `finish()` and `finish_unsigned()` commit the same budget, and that the budget follows the rule. Measured:
    - Schnorr: 100,001 units;
    - ECDSA: 100,001 units;
    - P2SH: 100,102 units.

    All three commit budget 10, the budget of one signature check, which covers up to 109,999 units. So the signed path never re-budgets an owner input, and the unsigned budget equals it for every key.
  - `every_web_case_builds_unsigned_and_equals_the_signed_path` covers buy b = 1 and b = 4 with referral and 2 funding inputs, claim PAY K = 4 and FINAL K = 1, return_deposit both paths, close K = 4 and K = 16, draw and void K = 4, request_deposit, request_withdraw for each owner class (partial, whole, and owner second among 2 plain inputs), and settle deposit and withdrawal. For each it checks:
    - the inputs to sign are exactly the funding and owner inputs;
    - every byte except their signature scripts is `finish()`'s (header, outputs, outpoints, sequences, compute budgets, storage-mass commit, covenant signature scripts);
    - the placeholders have the signed sizes;
    - the transaction ids, entries, units and masses are equal;
    - signed externally (SIG_HASH_ALL, as a wallet signs), every input passes the engine under its committed budget.
  - `owner_pub_cannot_be_signed_here`: an `OwnerPub` input builds unsigned (the same transaction as with the key held here), and `finish()` refuses it.
- **What a wallet must do for a P2SH owner:** return the signature push followed by the 34-byte redeem push. `post_sign` checks the redeem byte for byte. *(Corrected in the F1 review: this said the quick testnet wallet could sign a P2SH owner. It cannot: the quick wallet signs Schnorr P2PK inputs only, and KasWare signs P2PK only. D67 allows P2SH owners of any wallet type at the covenant level; the page restricts what it creates instead: `request_deposit` takes Schnorr P2PK owners only (`owner_type` otherwise), so every position the page creates can be withdrawn and signed from the page. A P2SH or ECDSA position made by another tool needs a wallet that returns the full signature script.)*

### Error codes (`lottery::web::Code`; number, name: message)

| # | name | message |
|---|---|---|
| 1 | bytes_length | The data has the wrong length. |
| 2 | not_template | The data is not a lottery object of the expected kind. |
| 3 | not_canonical | The data is not in its canonical encoding. |
| 4 | state_range | A lottery object holds a value its covenant does not accept. |
| 5 | chain_mismatch | The indexer's copy does not match the chain. |
| 6 | tree_mismatch | The purchases do not match the chain's frozen record. |
| 7 | purchase_invalid | A purchase is not a valid combination for this round. |
| 8 | store_format | The file is not a valid lottery file. |
| 9 | store_incomplete | The saved data lacks what this transaction needs. |
| 10 | anchor_invalid | The anchor proof does not fit this round. |
| 11 | input_range | A value is outside its allowed range. |
| 12 | funding_short | The wallet's coins do not cover this transaction. |
| 13 | wallet_script | A wallet coin or address is not a standard wallet script. |
| 14 | coin_unconfirmed | A wallet coin is not confirmed yet. |
| 15 | overflow | An amount is out of range. |
| 16 | refused | The lottery rules refuse this transaction. |
| 17 | policy | A safety rule of the builder refuses this transaction. |
| 18 | encode | The transaction could not be encoded. |
| 19 | mass | The transaction's mass cannot be computed. |
| 20 | engine | The local script check rejects this transaction. |
| 21 | templates | The built-in lottery templates are unavailable. |
| 22 | fee_rate | The fee is outside the accepted range. |
| 23 | signed_changed | The wallet changed the transaction. |
| 24 | signed_missing | The wallet did not sign every input it was asked to. |
| 25 | signature_form | A signature is not in the expected form. |
| 26 | signature_invalid | A signature does not verify. |

`tests/f1_web.rs::error_codes_are_stable` pins the numbers and names and checks the messages are distinct. New codes are appended; a number never changes meaning. The messages are a first draft for the copy pass (spec 8), so wording changes there are free.

### W3 coverage (`tests/f1_w3.rs`, debug build, default iterations)

The calls marked OK passed every check and built; the rest returned a typed error. None panicked or aborted. `F1_W3_ITERS=5` (5x) also passes.

| Target | Inputs | Calls | OK |
|---|---|---|---|
| `decode_state` (11 roles), `decode_redeem`, `verify_state` | each fixture state's bytes and redeem truncated, extended, bit-flipped, byte-replaced, spliced, or random; random roles; random scripts | 10,500 | 800 |
| `anchor_proof`, `check_anchor`, `sim_from_proof`, `winning_numbers(_from_anchor)` | the 272-byte proof mutated; random D, m, round, X.hash | 3,632 | 1,098 |
| `rebuild_tree(_bytes)`, `check_roots`, `payout_of_spk` | random purchases (canonical and not), random m including out of range, random roots and k; 32,768 purchases of one combination (count overflow, `purchase_invalid`) | 817 | 17 |
| `read_wallet` + `claim_from_store` | a fixture `wallet/1` that claims, its JSON mutated structurally (a node replaced by an edge number, hex, text, null, bool, empty array; array elements duplicated or removed; keys removed) or as text (truncated, flipped); claims built from every wallet that reads, with fuzzed funds | 487 | 127 (40 claims built) |
| `read_index_log`, `read_index_snapshot` | a fixture log line and a snapshot with all 11 states, mutated the same way | 500 | 127 |
| `read_bundle` | a fixture K = 4 launch bundle, mutated the same way | 150 | 27 |
| `buy` | shard state fields (any field, any type, removed), reserve, DAA, tree, bundle (0-6 combinations, canonical or not), referral, payouts, b_max; funds (coin count 0-10, value, DAA, script, covenant ID, coinbase flag, change script, fee) | 300 | 63 |
| `claim` | ticket and pool fields, values, idx, trees (0-17, random m), DAAs, lock time; funds | 250 | 33 |
| `return_deposit` (both paths) | ticket and pool fields, values, path switched, pool outpoint dropped; funds | 300 | 88 |
| `close` (K = 4) | controller, params and shard fields, pot, DAAs, shard count 0-17, funder, outpoint count, `now`; funds | 150 | 5 |
| `draw` and void (K = 4) | drawer, marker, vault and controller fields, pot, DAAs, trees, a mutated proof, draw/void switched, absorbed pools added (mutated); outpoint count; funds | 240 | 21 |
| `request_deposit`, `w_min` | vault fields, reserve, d, owner; funds | 400 | 253 |
| `request_withdraw` (3 owner classes) | vault and position fields, w, reserve, the owner key, the owner coin (value, script, DAA, coinbase), extra funding; funds | 300 | 81 |
| `settle` (both kinds) | request and record fields, values; funds | 240 | 76 |
| `with_fee` | random rates 0-1,999 | 30 | 15 |
| `post_sign`, `assemble`, `with_signatures`, `blanked`, `signers_for` | the returned transaction: inputs and outputs truncated, signature scripts mutated or replaced by well-formed random signatures, inputs duplicated, values, payload, lock time; partial or missing entries; random index lists | 2,400 | 635 |

- **How W3 detects a panic.** Each call runs under `catch_unwind`, and an unwind fails the test with the input that caused it. A wasm page would have aborted on the same panic. The test hook silences only those caught panics, so a bug in the test itself still prints.
- **What it found, fixed above:**
  - the `ClaimCase::comb` overflow;
  - the storage-mass division by zero;
  - an allocation abort from the test's own random tree (a mutated `m` sized a count vector). The builders check `m` before they touch a tree. A memory-allocation failure is an abort that `catch_unwind` cannot catch, so see "Open questions" for API response sizes.

### Commands run (outputs)

- `cargo test -j 2 -p lottery --release --test f1_templates --test f1_bytes` with all 13 pinned files converted to CRLF (`sed 's/$/\r/'`), then restored, after which `git status` was clean: `w2_* ... ok` ×4, `f1_encoding_and_transactions_unchanged ... ok`.
- `cargo test -j 2 -p lottery --release --lib --test f1_bytes --test f0_unsigned --test f1_templates`, after the refactor: lib `46 passed`, `f0_unsigned` `2 passed`, `f1_bytes` `1 passed; 1 ignored` (the 77 golden lines are equal), `f1_templates` `4 passed`.
- `cargo test -j 2 -p lottery --release --test f1_unsigned -- --nocapture --test-threads=1`: `3 passed`. The 23 cases each print "equal to finish() but the signatures", and the owner table is above.
- `cargo test -j 2 -p lottery --release --test f1_web -- --nocapture --test-threads=1`: `6 passed`.
  - `web_builders_equal_native_and_assemble` covers buy, claim, both returns, close, draw, void, request_deposit, 12 request_withdraw variants and both settles. For each: "web = native", a stand-in wallet signs, `assemble` passes, and every input passes the engine.
  - Draw with `sim_from_proof` equals the native build on the fabricated chain byte for byte.
- `cargo test -j 2 -p lottery --test f1_w3 -- --nocapture --test-threads=1` (debug): `4 passed`, with the counts in the table above. With `F1_W3_ITERS=5`: `4 passed`.
- `cargo test -j 2 -p tn10 --release pskt`: `3 passed` against the moved check.
- `cargo check -p lottery --no-default-features --target wasm32-unknown-unknown` (with the CC and AR variables), `--no-default-features --features mainnet`, and `--no-default-features`: all `Finished` with no warnings.
- Suite: see "Suite at the end of F1 part B".

### Files changed (F1 part B)

- New:
  - `crates/lottery/src/web/mod.rs`, `web/verify.rs`, `web/build.rs`, `web/sign.rs`;
  - `crates/lottery/tests/f1_unsigned.rs`, `f1_web.rs`, `f1_w3.rs`;
  - `.gitattributes`.
- `crates/lottery/src/buy.rs`:
  - `Template::try_from_artifact`, `try_state_bytes`, `try_redeem`;
  - `OwnerPub`, `InKind::OwnerPub`, `OwnerKey::public`, `OwnerKey::sign` public, `owner_signed_units`;
  - `try_entry_sigscript`, `try_planned_sig_script`, `try_push_redeem`;
  - `Built::external_signers`, `try_finish_inner` (shared by `finish` and `finish_unsigned`), `Final::try_fee`.
- `crates/lottery/src/draw.rs`: `try_expected`; `fix_lock_time` without a panic; `from_store` uses `try_expected`.
- `crates/lottery/src/claim.rs`: `comb()` with checked bounds.
- `crates/lottery/src/index.rs`: `State::try_redeem`, `try_state_bytes`, `from_values`; the decoders re-encode with `try_redeem`; `typed` is `pub(crate)`.
- `crates/lottery/src/smt.rs`: `Tree::try_increment`.
- `crates/lottery/src/store.rs`: the `wallet/1` reader bounds (`ResultJson::result`, `shard_tree`, `trees`, `apply_claim`).
- `crates/lottery/src/bundle.rs`: `Templates::try_get`; K bounded.
- `crates/lottery/src/lib.rs`: `pub mod web`.
- `crates/harness/src/lib.rs`: `try_budget_for`, `try_masses`; `budget_for` and `masses` wrap them.
- `crates/tn10/src/pskt.rs`: `post_sign`, `Check` and `blanked` delegate to `lottery::web::sign`.
- `CLAUDE.md`: the F1 test command.
- No change to `covenants/`, `crates/lottery/templates/`, `tests/fixtures/f1_digests.txt` or `crates/web`.

### Proposed DECISIONS rows (for the orchestrator)

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

### What part C must do

- **The wasm-bindgen API in `crates/web`.** Replace `f1_link_probe` with wrappers over `lottery::web`:
  - decode: `decode_state`, `decode_redeem`, the store readers;
  - build: the 8 builders and `claim_from_store`;
  - verify: `verify_state`, `rebuild_tree`, `check_roots`, `winning_numbers_from_anchor`;
  - estimate: `fee_of`, `with_fee`, `w_min`;
  - sign: `unsigned`'s result out, `assemble` in.

  Each returns `{ok}` or `{error: {code, number, message, detail}}`. Generate the `.d.ts` (the `Code` list becomes a TS union; no hand-written mirror). Add `wasm-bindgen` at the version Cargo.lock already has.
- **Safe JSON in the browser.** The wallet's format is `kaspa-consensus-client`'s `SerializableTransaction` (tn10 `pskt.rs`). Check it builds for wasm32 with our feature set, or use rusty-kaspa's own wasm binding for the JSON step. `post_sign` and `assemble` take parsed `Transaction`s.
- **W1:** run `lottery::digest::all_lines()` in wasm and compare it with `tests/fixtures/f1_digests.txt`. Also run the `f1_web` cases in wasm and compare their `Unsigned` bytes with native (the JS host needs `crypto` for getrandom).
- **W4:** a `mainnet` build of `crates/web` refuses to start and has no quick-wallet symbol.
- **Size:** measure and record it again with `lottery::web` reachable, which it is not yet: the link probe does not call it.
- **The anchor proof's source:** the wRPC client in wasm, or the indexer API's unverified copy (spec 6.1). `build::draw` checks the proof off-chain (`check_anchor`), and the chain checks it in `draw()`.
- **Notes for the page logic** (F3+):
  - Before building, filter out unconfirmed coins (DAA `u64::MAX`); the builders refuse them (`coin_unconfirmed`).
  - Set `with_ref` only when r >= M_REF (spec 7). The builder refuses a requested referral below M_REF (`refused`) instead of dropping it.
  - `close()` needs `now` (the virtual DAA) for the D24 guard.
  - A withdrawal's owner coin is a coin at the position owner's address, and `OwnerCoin.owner` is the wallet's public key; KasWare gives the x-only key.

### Open questions

- **None need the owner now.** No money, trust or covenant question came up. The stop-and-ask rule was not triggered: every native transaction is byte-identical (the 77 golden digest lines), and the web transactions equal the native ones.
- **Allocation aborts.** A failed allocation aborts in wasm as in native, and `catch_unwind` cannot catch it.
  - The decoders cap their byte inputs at 1 MiB (`verify::INPUT_MAX`), and the builders check every size-giving field (m, k, b, counts) before they allocate.
  - The JSON readers allocate in proportion to their input, so the page (F3) should cap the size of what it fetches from the indexer API and of an imported backup file. A few MB is far above any real store: 10,000 combinations are about 60 KB compact.
- **`draw_from_store` is not covered by W3** (it needs a built index). The page path is `build::draw` with a `DrawCase` from verified states, which W3 covers. `draw_from_store` serves a host that holds an index (tn10, the indexer API).
- **Part B wrote the primitive refactor before the property test.** The test-first rule held for the builders' attack lists: the spec 8 post-sign list and the guard refusals were written as tests and passed against the layer. W3 then found 3 panics, fixed above, and is green.

### Suite at the end of F1 part B

- `cargo test -j 2 --no-fail-fast` (from `crates/`, debug; default members `harness` and `lottery`): 53 binaries ran, 1,101 passed, 0 failed, 2 ignored (`f1_bytes::write_golden` and the old `m4_mutants` one).
  - This includes `f1_w3` 4/4, `f1_web` 6/6, `f1_unsigned` 3/3, `f1_bytes` 1/1, `f0_unsigned` 2/2, `f1_templates` 4/4 and `m7_seed` 15/15 (the SP12 seed gate: `mainnet,test-seed` fails, `mainnet` builds, and the short-void symbols disappear under `mainnet`).
  - As before F1 (D113), `spike_vdf` overflowed its stack in debug (STATUS_STACK_OVERFLOW).
- `cargo test -j 2 -p lottery --release --test spike_vdf`: `14 passed; 0 failed` (366 s).
- `cargo test -j 1 --release -p tn10`: `25 passed; 0 failed` (the 3 `pskt` tests run against the moved check).
- `cargo check -j 2 -p lottery --no-default-features --target wasm32-unknown-unknown`, with `--features mainnet`, and without the compiler: `Finished`, no warnings.
- Not run: the `crates/web` wasm link (part A's link probe is unchanged and does not reach `lottery::web`; part C builds it with the API) and `simnet` (RocksDB, about 14 min; it uses `lottery`'s default features, whose API only gained additions).

## F1 part C (stage agent, 2026-10-25)

### What was done

1. **The builders from chain objects** (`crates/lottery/src/web/cases.rs`, new).
   - Why: part B's builders take a `*Case`. Filling one is lottery logic (reserves from UTXO values, the boost pot, lock times, trees, the referral decision), so a page must not do it. `web::cases` does it in Rust, one function per entry point: `buy`, `claim`, `return_deposit`, `close`, `draw` (with its void), `request_deposit`, `request_withdraw`, `settle`.
   - Inputs: the covenant UTXOs as a node shows them, with the state bytes the indexer or the local copy gives (`Obj`); the instance's covenant ID; the purchases where a tree is needed (compact six-byte form, shard order); the user's choices.
   - Checks on every object:
     - the UTXO carries the instance's covenant ID, is confirmed and is not a coinbase;
     - the state bytes decode under the role's template, re-encode to themselves, and give the UTXO's P2SH (`verify::verify_state`);
     - after the build, the transaction spends exactly the given entry and outpoint (`chain_inputs`), so every derived value gives back the chain's UTXO value.

     Any mismatch is `chain_mismatch`.
   - The derivations mirror the native hosts that fill cases from an index: `DrawCase::from_store_absorbing`, `ClaimCase::from_store_index` and tn10's campaign builders.
   - `claim` implements `ClaimStore` over the chain objects and uses `from_store` or `from_store_index`. It first checks the rebuilt trees against every given pool's frozen roots, so a wrong purchase list is `tree_mismatch` before the reference reads a count.
   - The referral (spec 7) is decided here: it is paid only if a referrer is named and r = floor(b × p × s_R / 10⁴) ≥ M_REF. Otherwise the output is left out.
   - The caller's change address funds the reserves: the marker's in `close()` and the new pools' in `draw()`.
2. **`lottery::web::fixtures`** (new): every web fixture case as the chain-object call a page makes, with `web::build`'s transaction for the fixture case. There are 18 calls:
   - buy: b = 1 with a paid referral, and b = 4 with two coins;
   - claim, k = 4;
   - return_deposit: both paths;
   - close: k = 4 and k = 16;
   - draw and void: k = 4;
   - request_deposit;
   - request_withdraw: each owner class, partial and whole;
   - settle: both kinds.

   `Tree::purchases()` (new, `smt.rs`) gives a purchase list that rebuilds a fixture tree.
3. **The error table as one macro** (`lottery::web::Code`).
   - Each code's variant, number, name and message are written once. The macro gives the enum, `ALL`, `number`, `as_str`, `message`, `from_name` and `TS_DECL` (the `ErrorName` TypeScript union, emitted into the `.d.ts`).
   - Codes 1–26 are unchanged; `error_codes_are_stable` pins them.
   - Appended: 27 `address_network`, 28 `input_format`, 29 `not_started`, 30 `mainnet_refused`.
4. **`crates/web`: the wasm-bindgen API** (it replaces part A's link probe).
   - **Layers:**
     - `dto`: the boundary types, declared to TypeScript by tsify;
     - `convert`: hex, decimals, UTXOs, addresses, the network;
     - `views`: decoded states;
     - `api`: the logic in plain Rust, which the native tests call;
     - `bindings`: the exports;
     - `safe_json`: the wallets' format;
     - `quick_wallet`;
     - `wrpc_probe`: behind a feature.
   - **Exports, by group:**

     | Group | Exports |
     |---|---|
     | Start | `start(network)` |
     | Decode | `decodeState(role, stateHex)`, `decodeRedeem(redeemHex)` |
     | Verify | `verifyState(covenantId, role, {utxo, state})`; `checkRoots({m, purchases[], against, covenantId})`; `winningNumbers({covenantId, round, d, m, anchorProof})` |
     | Build | `buildBuy`, `buildClaim`, `buildReturnDeposit`, `buildClose`, `buildDraw` (the void when `anchorProof` is absent), `buildRequestDeposit`, `buildRequestWithdraw`, `buildSettle` |
     | Sign | `assemble({built, signed})` |
     | Estimate | `feeFor({priorityMass, feeRate})`, `wMin(covenantId, vault)` |
     | Addresses | `addressOf`, `scriptOf` |
     | Quick wallet (absent from a `mainnet` build) | `quickWalletNew`, `quickWalletImport`, `quickWalletSign` |
   - **Results:**
     - Every export returns `Outcome<T> = { ok: T } | { error: ApiError }` and never throws. `ApiError` is `{code: ErrorName, number, message, detail}`.
     - Inputs are JS objects; a malformed one is `input_format`.
     - Every call except `start` needs a started session (`not_started` otherwise).
   - **Builders:**
     - Each takes `funds: {coins, change, feeRate}` (the wallet's coins, its change address and a fee rate) and runs the fee loop (`build::with_fee`) itself.
     - Each returns a `BuiltTx`:

       | Field | Content |
       |---|---|
       | `transaction` | the Safe JSON for `signPskt`'s `txJsonString`, with the inputs to sign blank |
       | `id` | final already: v1 transaction IDs exclude signature scripts |
       | `sign` | `{index, sighashType: 1, kind, publicKey, address}`: KasWare's `signInputs` items, plus who signs |
       | `masses`, `fee`, `units` | from the built transaction |
       | `summary` | the wallet's coins in, what comes back, and every output with its address (spec 7: figures come from the built transaction) |
   - **`BuiltTx` is plain data**, not a handle into wasm memory. So it survives a worker restart, crosses `postMessage` and can be stored. `assemble` first rebuilds the `Unsigned` from it and checks that it is self-consistent:
     - the blank inputs are exactly the listed ones;
     - each listed owner is the one its UTXO's script names;
     - every signature is asked for SIG_HASH_ALL.

     Then it runs part B's `sign::assemble`: the post-sign check, our transaction with only the listed signature scripts taken, the masses, and the engine on every listed input.
   - **Types (spec 1.1):**
     - Every input and output type derives `tsify::Tsify`, so `wasm-bindgen` writes it into `web.d.ts`.
     - The exports carry `unchecked_param_type` and `unchecked_return_type`, so the functions are typed with those types, for example `buildBuy(input: BuyInput): Outcome<BuiltTx>`.
     - Named string types:
       - `Dec` is every 64-bit integer, as a decimal string, so values above 2^53 survive JSON, IndexedDB and `postMessage`;
       - `Hex`, `Address` and `Shares`.
     - Views:
       - there is one interface per role, and `StateView` is the union tagged on `role`;
       - a macro generates them by destructuring each `lottery::state` struct in full, so a new state field without a view field is a compile error;
       - each D11 (class, key) pair also gets its address.
     - Why tsify, not opaque strings: the `.d.ts` gets real field types and doc comments, at no run-time cost. tsify is used for declarations only. Values cross through `serde-wasm-bindgen` as plain objects, with `undefined` for absent options, matching the declarations.
     - Why tsify 0.5.5: it is the last release on wasm-bindgen 0.2.100 (0.5.8 needs 0.2.104), so the build still has one wasm-bindgen. `tsify-macros` is pinned to 0.5.5 to match.
   - **Extra items in the `.d.ts`:** it also lists rusty-kaspa's own wasm-bindgen items (`ITransaction`, `initConsolePanicHook`, …), which `kaspa-consensus-client` compiles in. They are not our API, and the page imports only ours.
5. **One Safe JSON serialiser.**
   - `web::safe_json` is the only copy. It moved from `tn10/src/pskt.rs`, with the network prefix as a parameter.
   - tn10's `pskt::{to_safe_json, from_json, reserialize}` call it, and tn10 no longer depends on `kaspa-consensus-client` directly.
   - `from_json` refuses input above 8 MB.
6. **Networks and addresses** (spec 6.6).
   - The beta build starts on testnet-10 only (`start("testnet-10")`).
   - Every address input (payout, referrer, change, deposit owner, `scriptOf`) must carry the started network's prefix (`kaspatest:`); anything else is `address_network`. Output addresses use the same network.
   - The `simnet` feature (off by default) adds `start("simnet")` with `kaspasim:` addresses, for the private-simnet dev run (spec 8's Playwright suite). See open question 1.
7. **W4: the mainnet gate.** In a `--features mainnet` build:
   - `api::start` always returns `mainnet_refused`;
   - a `#[wasm_bindgen(start)]` function throws it when the module is instantiated;
   - the session can never start, so every other call is `not_started`. The optimizer then drops the whole API: the mainnet wasm is 1.37 MB, against 6.32 MB.
   - The quick wallet is compiled out: the `quick_wallet` module, its exports, its `QuickKey` and `QuickSignInput` types and a marker string (`cfg(not(feature = "mainnet"))`). The module also has a `compile_error!` guard.
8. **The quick wallet's Rust hook** (spec 6.2, S7; the wallet UI is F3):
   - `quickWalletNew` makes a secp256k1 key from the browser's `crypto.getRandomValues` (getrandom `wasm_js`); the address is the Schnorr P2PK address on the started network.
   - `quickWalletImport(secretKey)`.
   - `quickWalletSign({built, secretKey})` signs only the listed inputs whose key it is, with rusty-kaspa v2.1.0's `sign_input` (SIG_HASH_ALL). Its signed Safe JSON then goes through `assemble`, like any wallet's.
9. **The wRPC probe** (feature `wrpc-probe`, never shipped; `src/wrpc_probe.rs`): see "Anchor proof from a browser" below.
10. **CLAUDE.md** gains:
    - the layout line for `web`;
    - `f1_cases` in the F1 test command;
    - a "Web API" command line: build, `.d.ts`, native API tests, W1, W4 and the features.
11. **Unchanged:** no covenant, template or `f1_digests.txt` change.
    - `crates/.cargo/config.toml` gains the wasm32 test runner.
    - `.gitattributes` pins `crates/web/tests/fixtures/w1_api.txt`.

### Tests (all new in part C)

- **`lottery/tests/f1_cases.rs`** (written before the derivations were run):
  - `chain_object_calls_equal_the_fixture_cases`: all 18 calls build exactly `web::build`'s transaction for the fixture case (transaction, entries, inputs to sign, masses, fee, units).
  - `untrusted_objects_are_refused`, each with its code:

    | Refused input | Code |
    |---|---|
    | a state that is not the UTXO's | `chain_mismatch` |
    | a UTXO of another instance, or with no covenant ID | `chain_mismatch` |
    | an unconfirmed UTXO | `coin_unconfirmed` |
    | a coinbase UTXO | `chain_mismatch` |
    | a ticket's bytes given as a shard | `not_template` |
    | a value below what the state accounts for (shard, vault, record) | `chain_mismatch` |
    | an extra or a missing purchase (buy, claim, draw) | `tree_mismatch` |
    | no live pool | `store_incomplete` |
    | a draw with one shard's purchase list missing | `store_incomplete` |
    | an anchor proof with P's fields altered | `anchor_invalid` |
    | a truncated anchor proof | `anchor_invalid` |
  - Two boundaries the test documents:
    - A shard UTXO one sompi richer is a different, valid UTXO, because the value comes from the node. The build spends it, and the successor keeps the extra sompi.
    - A proof with another X.hash builds off-chain, because only `OpChainblockSeqCommit` on the node knows the chain. Spec 8 says such a proof is "refused by the node".
  - `referral_below_m_ref_is_omitted`.
- **`web/tests/api.rs`** (native):
  - `api_builds_equal_the_library`: every call, as the page's JSON, builds what `lottery::web` builds at the same fee rate, and the `BuiltTx` reads back to the same `Unsigned`.
  - `api_digests_match_the_golden_file`.
  - `assemble_takes_a_wallets_signatures_only`:
    - a stand-in wallet signs all 18 calls, and `assemble` accepts them;
    - spec 8's mock-wallet list, applied to the returned JSON, is refused: a covenant signature script, an output value, an output script, an extra output, the change address, the lock time and the payload (`signed_changed`), and a listed input left unsigned (`signed_missing`);
    - our own `BuiltTx` with its sign list removed is refused (`input_format`).
  - `other_network_addresses_are_refused`:
    - refused: mainnet, devnet and simnet addresses, a malformed `kaspatest:` string and an empty one;
    - tried in every builder's change field, and in the payout, the referrer, the deposit owner and `scriptOf`;
    - only testnet-10 starts.
  - `untrusted_inputs_never_panic` (W3 at the boundary): mutated JSON inputs; mutated returned Safe JSON into `assemble` and `from_json`; random hex into the decoders. `F1_API_ITERS=n` changes the number of rounds.
  - `quick_wallet_signs_what_it_owns`.
  - `views_decode_every_role`: all 11 roles, and every `*Address` field is a `kaspatest:` address.
  - `verify_roots_and_numbers`.
- **`web/tests/w1_wasm.rs`** (W1, wasm under Node) and **`web/tests/w4_mainnet.rs`** (W4): below.

### W1 evidence (wasm vs native byte identity)

- Command: `cargo test -j 2 -p web --target wasm32-unknown-unknown --release --test w1_wasm -- --nocapture`, with the CC/AR variables set. The runner is `wasm-bindgen-test-runner` 0.2.100, under Node v24.14.0.
- Output:
```
running 2 tests
W1 library: 77 digest lines equal the native golden file
test w1_library_digests_equal_native ... ok
W1 API: 18 builds through the JS boundary equal the native golden file
test w1_api_through_the_js_boundary ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.36s
```
- What it covers:
  - **The library half:** `lottery::digest::all_lines()`, computed in wasm, against `lottery/tests/fixtures/f1_digests.txt`, the native golden file written before F1. It covers every template's ABI output and the 21 M1–M8 fixture transactions, with their entries, script units, masses and storage-mass commits.
  - **The API half:** the 18 web fixture calls, passed as JS objects through the real exports. Each `BuiltTx` is read back from JS, digested, and compared with `web/tests/fixtures/w1_api.txt`, the same file the native `api_digests_match_the_golden_file` compares.
  - It also checks that calls before `start` return `not_started`.
- Negative control: with one digit of `w1_api.txt` line 3 changed, the wasm test failed with `claim k=4 (buildClaim): the wasm BuiltTx differs from the native one`. The file was then restored.

### W4 evidence (mainnet gate)

- Command: `cargo test -j 2 -p web --release --test w4_mainnet -- --nocapture` (206 s; it builds in `target/w4`).
```
beta build 6315531 B, mainnet build 1368162 B
quickWalletNew               beta    4  mainnet    0
quickWalletImport            beta    4  mainnet    0
quickWalletSign              beta    5  mainnet    0
quick_wallet                 beta    8  mainnet    0
KASPALIONARE_QUICK_WALLET    beta    1  mainnet    0
beta: started on testnet-10 ; start(mainnet) refused: address_network
mainnet: load refused: mainnet_refused - This build refuses to run on mainnet.
test w4_mainnet_build_has_no_quick_wallet_and_refuses_to_start ... ok
```
- The scan searches the bytes of the whole `.wasm` file. That covers:
  - the exports;
  - the `name` section, with every mangled `web::quick_wallet::…` symbol;
  - the data section;
  - the custom sections, such as the TypeScript declarations.

  The beta build is the positive control: it must contain every name.
- The start check runs each build through `wasm-bindgen --target nodejs` and then Node.

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

### Sizes (release, the beta build with the whole API reachable)

| Build | raw | gzip -9 | brotli -q 11 |
|---|---|---|---|
| opt-level 3, cargo output | 6,315,533 | 2,326,230 | 1,933,984 |
| opt-level 3, after wasm-bindgen | 5,423,397 | 2,174,290 | 1,846,862 |
| opt-level 3, + wasm-opt -O2 | 4,707,149 | 2,108,039 | 1,813,946 |
| opt-level 3, + wasm-opt -Oz | 4,506,713 | 2,097,974 | 1,807,866 |
| opt-level "s", after wasm-bindgen | 4,975,004 | 2,032,482 | 1,763,138 |
| opt-level "s", + wasm-opt -Oz | **3,933,392** | **1,914,548** | **1,685,156** |
| mainnet build (refuses to start) | 1,368,162 | | |
| with `wrpc-probe`, opt-level 3, after wasm-bindgen | 7,284,023 | 2,752,194 | 2,218,944 |

- Part A's link probe was 5.43 MB at opt-level 3 before wasm-bindgen. It did not reach `lottery::web` and had no Safe JSON. This build adds the API, `kaspa-consensus-client` and the views (6.32 MB).
- The smallest shipped form is opt-level "s", then wasm-bindgen, then `wasm-opt -Oz`: 1.91 MB gzip, 1.69 MB brotli. F3's build pipeline should use it. It needs binaryen, with `wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals`.
- **Embedded templates: keep the pretty JSON.**
  - Compact JSON would save 227 KB raw (795,518 → 568,526 B), but only 13.4 KB gzip (41,964 → 28,525 B) and 9.6 KB brotli (24,176 → 14,613 B).
  - That is about 0.6% of the compressed bundle, and the format is guarded by W2 and pinned byte for byte. Not worth changing.

### Tool versions (installed or used in part C)

- `wasm-bindgen-cli` 0.2.100, installed with `cargo install wasm-bindgen-cli --version 0.2.100 --locked`. It puts `wasm-bindgen`, `wasm-bindgen-test-runner` and `wasm2es6js` in `~/.cargo/bin`.
- Node v24.14.0, already installed. The `wasm-bindgen-test` crate is 0.3.50, which matches 0.2.100.
- binaryen `wasm-opt` 131, from the npm package `binaryen@131.0.0`. Installing it caused the incident in open question 2.
- brotli 1.2.0 and gzip 1.14 (Git Bash). LLVM clang and llvm-ar as in part A.
- Crates added to Cargo.lock:
  - `tsify` 0.5.5, `tsify-macros` 0.5.5 and `serde_derive_internals` 0.29.1;
  - `wasm-bindgen-test` 0.3.50, with its macro crate, `minicov`, `walkdir` and `same-file`.
- `web` also depends on crates that were already in the lock: `kaspa-consensus-client`, `kaspa-addresses` and `serde-wasm-bindgen`. For `wrpc-probe` it adds the wRPC client, `kaspa-smt`, `kaspa-seq-commit`, `ring` and `wasm-bindgen-futures`.

### Commands run (outputs)

- `cargo test -j 2 -p lottery --release --test f1_cases --test f1_web -- --nocapture --test-threads=1`:
  - `f1_cases` `3 passed`; all 18 calls print "chain objects = fixture case";
  - `f1_web` `6 passed`, including the extended `error_codes_are_stable`.
- `cargo test -j 2 -p web --release --test api -- --nocapture --test-threads=1`: `8 passed; 0 failed; 1 ignored` (the ignored one is `write_golden`).
  - The eight mock-wallet edits are each refused with their code.
  - The fuzz test printed "138 mutated calls parsed, 10 built, none panicked" at 40 rounds. The default is now 100 rounds.
- `cargo test -j 2 -p web --release --features simnet --test api other_network`: `1 passed`.
- `cargo test -j 2 -p web --release --test api -- --ignored write_golden`: wrote the 18 lines of `w1_api.txt`.
- W1 and W4: above.
- `cargo build -j 2 -p web --target wasm32-unknown-unknown --release`: `Finished`. `wasm-bindgen --target web --out-dir target/web-pkg …` writes `web.d.ts` (1,725 lines), `web.js` and `web_bg.wasm`.
- `cargo check -j 2 -p web --features simnet` and `--features simnet,mainnet`: `Finished`, with no warnings.
- `cargo build -j 2 -p web --target wasm32-unknown-unknown --release --features wrpc-probe --target-dir target/wrpc`:
  - before `ring/wasm32_unknown_unknown_js` was added: 6 rustls errors (`SystemRandom: ring::rand::SecureRandom` not satisfied);
  - after: `Finished`. The Node run is quoted above.

### Files changed (F1 part C)

- `crates/lottery/src/web/mod.rs`: the code table macro, codes 27–30, `TS_DECL`, `from_name`, and the modules `cases` and `fixtures`.
- `crates/lottery/src/web/cases.rs` and `crates/lottery/src/web/fixtures.rs` (both new).
- `crates/lottery/src/smt.rs`: `Tree::purchases`.
- `crates/lottery/tests/f1_cases.rs` (new); `crates/lottery/tests/f1_web.rs` (the code test extended).
- `crates/web/Cargo.toml`.
- `crates/web/src/{lib,api,bindings,convert,dto,views,safe_json,quick_wallet,wrpc_probe}.rs`; all are new except `lib.rs`.
- `crates/web/tests/{api.rs, w1_wasm.rs, w4_mainnet.rs, common/mod.rs, fixtures/w1_api.txt}` (all new).
- `crates/tn10/Cargo.toml`: `web` replaces `kaspa-consensus-client`.
- `crates/tn10/src/pskt.rs`: the Safe JSON functions call `web::safe_json`.
- `crates/.cargo/config.toml` (the wasm32 runner), `crates/Cargo.lock`, `.gitattributes`, `CLAUDE.md`, and this file.

### Proposed DECISIONS rows (for the orchestrator)

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

### Open questions and owner decisions

1. **Simnet addresses: needs a decision.**
   - The conflict: spec 6.6 says every address that is not `kaspatest:` is refused, but spec 8 runs the whole end-to-end suite on the private simnet, whose addresses are `kaspasim:`.
   - What part C did: the beta build stays testnet-10 only, and a `simnet` build feature adds the simnet, for the local dev run only.
   - Trust: none; simnet coins exist only on this PC.
   - Money: none.
   - Work: none now. F3 builds the dev page with `--features simnet`.
   - The alternative: run the Playwright suite on testnet-10 only. That is slower and needs faucet coins.
2. **An accident outside the repo: the owner should know.**
   - What happened: I ran `npm install` to get binaryen from the scratchpad folder. npm walked up the folders to `C:\Users\endri`, which has a `node_modules` folder, and treated the home folder as its project. It:
     - created `C:\Users\endri\package.json` and `package-lock.json`, listing only `binaryen`;
     - removed the 491 packages that were in `C:\Users\endri\node_modules` (641 counting nested ones). They are web3 libraries and related tools, such as wagmi, viem, ethers, hardhat, rainbowkit, walletconnect, supabase and react.
   - Likely impact: no project under `Documents\Kaspalionare` used them. They were not listed in any `package.json` in the home folder, so they were probably left over from an install run there.
   - If another project of yours relied on them through the home folder, run `npm install` in that project.
   - Records: the list of removed packages and npm's log are in the session scratchpad (`home-node_modules-removed.txt`, `npm-install-log-that-pruned-home.log`). Their exact versions cannot be recovered.
   - Nothing was done to undo it, because reinstalling at guessed versions could make it worse.
3. **wRPC in the page, or the indexer's proof (F2/F5).**
   - Trust: with the client in the page, the page does not depend on the indexer for the anchor proof. The chain checks the proof in `draw()` either way, so this is about availability, not money.
   - Cost: +372 KB brotli for the page.
   - Work: sharing `anchor.rs`.
4. **Allocation limits on the page** (from part B, unchanged).
   - JSON inputs are bounded: Safe JSON 8 MB, hex 4 M digits, coins ≤ 8, pools ≤ 64, shards ≤ 16, purchase lists ≤ 16.
   - But a huge JS object is converted before those checks run. The worker should cap what it accepts from the indexer and from backup files (F3/F4).
5. **Left for later stages:**
   - the time to build a buy in a browser (spec 8 quality); in Node, the whole W1 run took 0.36 s for all builds;
   - `webstore/1` (F4);
   - the view models for the claim panel (prize, h, accrual), which should also come from Rust.

### What the reviewer should attack

- **Byte identity:**
  - W1, both halves;
  - `f1_cases::chain_object_calls_equal_the_fixture_cases`;
  - `api_builds_equal_the_library`.

  Is any M1–M8 fixture shape missing from `web::fixtures`? `refresh` and `set_params` are not page actions, so they are covered only by the library half.
- **Rule logic outside Rust:** by construction, the API's callers hold none. Check `cases.rs` instead:
  - its derivations against the covenants: `reserve_of`, the pot, the lock times (`T + D_RET`, `pool.deadline`), `at_age`, `set_void`;
  - that `chain_inputs` really covers every derived value.
- **The post-sign check:**
  - `api::unsigned_of`'s self-consistency check: could a stored or tampered `BuiltTx` make `assemble` accept a transaction other than the one shown?
  - how `assemble` uses part B's check.
- **Untrusted input:**
  - the indexer's state bytes and purchases (`chain_mismatch`, `tree_mismatch`);
  - the returned Safe JSON, parsed by kaspa-consensus-client under mutation;
  - `deny_unknown_fields` and the size caps;
  - the anchor-proof boundary: X.hash can only be checked on-chain.
- **W4:**
  - could a byte scan miss a quick-wallet path, for example inlined code with no name?
  - `--features mainnet,simnet` cannot start, because `start` refuses first.

### Suite at the end of F1 part C

- `cargo test -j 2 --no-fail-fast` (from `crates/`, debug, the default members `harness` and `lottery`): 54 test binaries, 1,104 passed, 0 failed, 2 ignored (`f1_bytes::write_golden` and the old `m4_mutants` one).
  - This includes `f1_cases` 3/3, `f1_web` 6/6, `f1_w3` 4/4, `f1_unsigned` 3/3, `f1_bytes` 1/1, `f1_templates` 4/4 and `f0_unsigned` 2/2.
  - It also includes `m7_seed` 15/15: the SP12 seed gate still works.
  - As before F1 (D113), `spike_vdf` overflowed its stack in debug (`sv_measure_masses_and_stack`, STATUS_STACK_OVERFLOW).
- `cargo test -j 2 -p lottery --release --test spike_vdf`: `14 passed; 0 failed` (359 s).
- `cargo test -j 1 --release -p tn10`: `25 passed; 0 failed`. The 3 `pskt` tests now run through `web::safe_json`.
- `crates/web`:
  - `api` 8/8, with 1 ignored;
  - W1 2/2 in wasm;
  - W4 1/1.
- Native transactions are byte-identical: `f1_bytes` (77 golden lines) is unchanged and green, and W1 holds in wasm.
- No covenant or template change.
- Not run: `simnet` (RocksDB, about 14 minutes). It uses `lottery`'s default features, and the library API only gained additions.

## F1 review and fixes (stage agent, 2026-10-25)

### The review

- **Who and what:** `covenant-reviewer` on the F1 commit `b3b3520` (parts A, B and C).
- **Verdict:** no must-fix item. Six should-fix items and four notes (the table gives them as the orchestrator condensed the reviewer's report; the reviewer's own report is not in the repo). The reviewer left four untracked test files with its probes; all four are folded in (see "Files changed").
- **Per area:**

  | Area | Reviewer's finding | Verdict |
  |---|---|---|
  | Byte identity (W1, golden digests) | holds; 20 builder shapes were outside the 77 golden lines; the reviewer digested them on the pre-F1 commit `6633ba3` (a worktree) and on `b3b3520`: all 20 equal | should-fix 5 (coverage) |
  | Rule logic outside Rust | `winningNumbers` and `checkRoots` took round, m and D from the page; D would have to be computed in TypeScript (spec 1 forbids it), and an indexer serving a wrong round or m made "verify" show wrong numbers as verified | should-fix 1 |
  | Post-sign check and assembly | the check itself held (R1-R3: only listed signature scripts are taken; non-minimal and extra pushes are refused; every covenant input that loses or changes its signature script is refused); but `assemble` did not bind the displayed figures: a `BuiltTx` with altered fee, ID, summary or output values still assembled (P6) | should-fix 2 |
  | Vault owners | `build_request_deposit` accepted any `kaspatest:` owner (P2: a P2SH 2-of-2 built), while the page's signers sign Schnorr P2PK only (P1: the quick wallet returns `signed_missing` for ECDSA and P2SH owner inputs), so the page could create positions it can never withdraw; the handoff (F1 part B, "What a wallet must do for a P2SH owner") and the `OwnerPub` comment were wrong about it | should-fix 3 |
  | wasm coverage | W1 never ran signing or signature verification in wasm, nor decode and verify | should-fix 4 |
  | Deployed artifact | nothing kept the dev-only `simnet` feature out of the deployed wasm | should-fix 6 |
  | Notes | change below dust reported as `funding_short` (P5); a draw's funding shortfall reported as `refused`; `QuickKey`'s `Debug` printed the secret key; `lottery::web::fixtures` and the fixture keys in the production wasm | notes |

### What changed, per item (each test written first; each failed, or showed the fault, before its fix)

1. **`checkRoots` and `winningNumbers` take chain objects only** (`crates/web/src/api.rs`, `dto.rs`).
   - `RootsInput` is `{covenantId, against, purchases}`: no `m`. The round, m, k and roots come from `against`, a live marker or pool (checked against its UTXO). `RootsOut` reports the round and m it read.
   - `WinningInput` is `{covenantId, from, anchorProof}`, with `from` one of:
     - `{closed: {marker, controller}}` (before the draw): D = the marker UTXO's creation DAA + the controller's `seed_offset`, exactly as `draw()` reads it (`game::anchor_daa`); the controller must be the undrawn controller of round r + 1;
     - `{drawn: {pool, drawer}}` (after the draw): D = the pool's claim deadline − `draw_window` − `claim_window` (drawer check 19; `draw()` copies both windows unchanged, `drawer.sil:972-973`), checked with `game::claim_deadline`; the drawer must have drawn the pool's round; and the numbers from the proof must equal the pool's `win`, which `draw()` wrote.
   - `WinningOut` reports the round, m and D it derived. Unknown fields (such as a page-given `m`) are refused.
   - Codes: a proof that does not anchor D is `anchor_invalid`; objects that do not belong together, or numbers that are not the pool's, are `chain_mismatch`.
   - Test: `web/tests/api.rs::roots_and_numbers_come_from_chain_objects` (on a draw with a jackpot and two second-prize holders, since the fixture draw creates no pool): both paths give the library's numbers and the same D; an extra or missing purchase list is `tree_mismatch` against a marker and a pool; a later marker DAA is `anchor_invalid`; a controller of another round and a drawer in the controller's place are refused; a proof with another X.hash, which off-chain still anchors D, is `chain_mismatch` on the drawn path; the round r drawer on the drawn path is `chain_mismatch`; extra `m` fields are refused.
2. **`assemble` binds every displayed figure to the transaction** (`api::unsigned_of`, used by `assemble` and `quickWalletSign`).
   - Every field of a `BuiltTx` is recomputed from its `transaction` and refused on any difference with the new code 31 `built_mismatch` ("The figures shown do not match the transaction."):
     - the fee (inputs − outputs);
     - the masses (each listed input at its signed size; all four, and the storage-mass commit);
     - the units: a listed input's must be its signer class's (`buy::external_signer_units`, new), a covenant input's must be those of its own run here, and every input's must give its committed budget;
     - the ID, the sign items (with their addresses), and the whole summary (`built_tx` run again, field by field), and the `transaction` string itself (a reformatted copy is refused: pass it back unchanged).
   - To make the summary a function of the transaction alone, `built_tx` no longer takes the change script: `walletIn` is the total of the inputs the wallet signs, and `toWallet` marks a plain output paying the script of one of those inputs (the wallet's own address). For every page flow, where the change address is the wallet's address, the figures are the same as before. Proposed row (b).
   - The known limit: the drawer's input runs `OpChainblockSeqCommit`, which needs the node's chain (it is `InvalidOpcode`, `Opcode<0xd4>`, with no accessor; `crypto/txscript/src/opcodes/mod.rs:1387`), so its units are bound by the committed budget only (steps of 10,000 units). The test pins this: +1 unit passes, +10,000 is refused. The budget, which is what the chain charges, is bound exactly.
   - Tests: `api.rs::assemble_binds_every_figure`: 19 alterations on 8 transactions (buy, claim, close, draw, void, two withdrawals, a settlement), 152 tampered `BuiltTx`, each refused with `built_mismatch` by `assemble` and by `quickWalletSign`; the honest one assembles. The existing `assemble_takes_a_wallets_signatures_only` and `api_builds_equal_the_library` still pass.
   - Golden: `web/tests/fixtures/w1_api.txt` was rewritten for the 6 withdrawal lines only. Their transaction, ID, sign list, masses, fee and units are byte-identical (a field-by-field diff of the 18 `BuiltTx` JSONs before and after: 12 identical; the 6 differ only in `summary.walletBack` and one output's `toWallet`). The reason: the fixture withdrawal's change pays the fixture wallet's address while the only coin it spends is the owner's, at another address, so its change is no longer shown as "back to the wallet". The native transactions (`f1_digests.txt`) are untouched.
3. **The page creates vault positions only for owners it can withdraw for** (`api::build_request_deposit`).
   - A deposit owner must be a Schnorr P2PK address; an ECDSA P2PK or any P2SH owner is the new code 32 `owner_type` ("Vault positions need a Schnorr wallet address."). No covenant change: D67 still accepts every type; the page restricts what it creates. `build_request_withdraw` still takes all three owner classes, for positions made by other tools (a wallet that returns the full signature script can sign them).
   - Test: `api.rs::deposit_owners_are_withdrawable_from_the_page`: the three refused types (ECDSA and wrapped-Schnorr P2SH of the same key, and the reviewer's 2-of-2 P2SH) are `owner_type`; then, end to end for the accepted type: a deposit for a fresh quick-wallet key (its request output carries the key as owner), a settlement of that request (the settle covenant runs in the engine and mints a position of that owner), and the whole withdrawal of that position through the API, signed by `quickWalletSign` and assembled (every input in the engine). The ECDSA and P2SH fixture withdrawals stay `signed_missing` for the quick wallet (the reviewer's P1, kept as an assertion).
   - Text corrected: the F1 part B handoff paragraph "What a wallet must do for a P2SH owner" (marked "Corrected in the F1 review") and the `OwnerPub` comment (`buy.rs`), which said the wrapped Schnorr P2SH was "the only P2SH owner class D67 defines".
4. **W1 runs signing, verification, decode and verify in wasm** (`web/tests/w1_wasm.rs`; the reviewer's `review_f1_wasm_assemble.rs` folded in).
   - New part, the verify half: 290 calls (`common::verify_calls`): `decodeState`, a wrong-role decode, `verifyState`, `verifyState` on another script and `decodeRedeem` for every fixture object; `checkRoots` against a marker and a pool, accepted and refused; `winningNumbers` on both paths, accepted and refused. Each result's JSON through the JS boundary equals the native one, against the new golden `web/tests/fixtures/w1_verify.txt` (written natively by `write_golden`, compared natively by `api.rs::verify_digests_match_the_golden_file`, which also checks that each refused variant is an error and every other call ok).
   - New part, signing: for 7 transactions (two buys, a claim, close K = 16, a draw, a void, a withdrawal) `quickWalletSign` signs in wasm and `assemble` accepts (Schnorr verification in the wasm engine); the assembled transaction is the built one with only the listed signature scripts; a flipped signature byte is `signature_invalid`; a changed figure is `built_mismatch` for both calls.
   - The four parts now run in one ordered test, `w1_wasm_equals_native`: the session is module state, and wasm-bindgen-test does not order separate tests (found when the old `not_started` check ran after another part had started the session).
5. **Golden digests: 20 shapes appended** (`lottery/src/digest/lines.rs::review_digests`, `tests/fixtures/f1_digests.txt` lines 78-97).
   - The shapes: the return with a pool; each owner class's withdrawal, partial, whole and with the owner second among the plain inputs; the final claim; fabricated jackpot, second and lower-tier claims; a draw absorbing three expired pools; a draw with jackpot and second-prize holders; a void at round 3; a buy without change; both converting draws.
   - They use the reviewer's own digest function byte for byte (`review_digest`), so the lines are the ones the reviewer computed. The evidence that they equal the pre-F1 code: the reviewer ran the same test on a worktree at `6633ba3` and on `b3b3520` and found all 20 identical. This stage reran the reviewer's file on the unchanged `b3b3520` tree first and appended exactly its 20 printed lines (`REVIEWDIGEST ` replaced by `review `); the existing 77 lines were not regenerated (`git diff`: 20 insertions, 0 deletions).
   - W1 computes the same 97 lines in wasm and they equal the file.
6. **The release artifact refuses the simnet and has no fixture code** (`web/tests/w4_mainnet.rs`).
   - W4 now builds the release artifact with the documented wasm build command itself (`cargo build -j 2 -p web --target wasm32-unknown-unknown --release`, default target directory, so the checked file is `target/wasm32-unknown-unknown/release/web.wasm`), plus a dev build (`--features simnet,fixtures`, the positive control) and the mainnet build in `target/w4`.
   - Release checks, under Node: `start("simnet")` and `start("mainnet")` are `address_network`; once started on testnet-10, a `kaspasim:` address in `scriptOf` and as a builder's change address is `address_network`; the quick wallet is present; `devFixtureNames` is absent. The dev build starts on the simnet with `kaspasim` addresses and lists the 18 fixture calls.
   - W4 is part of the documented web test command (CLAUDE.md: api, W1, W4).

**Notes:**
- **Change below dust** (`build.rs:207, 587-598`): `build::with_fee` now explains its failure. When the loop fails after a successful build (a `funding_short` from the builder, or a `fee_rate` from a fee above the cap), it tries the same transaction without its change output (`mk(fee + change)`; the builders leave a zero change out); if that build's fee covers its own mass, the error is the new code 33 `change_dust` ("The change left over is too small to send back."), with the extra fee dropping the change would pay in the detail. That diagnostic build is discarded: the money behaviour is unchanged, the coins are refused as before. Tests: `lottery/tests/f1_web.rs::small_change_and_short_draws_have_their_codes` (coins of cost + the no-change fee + 0, 1, 10³, 10⁵, 10⁶, 10⁷ sompi: `change_dust`; one sompi below: `funding_short`; +1 KAS: built with change) and `web/tests/api.rs::small_change_is_change_dust` (the reviewer's P5 amounts). Measured: with coins of exactly cost + the fee the buy needs without change, the loop still refuses (it starts from a lower fee, makes a dust change and overshoots); see the owner decision.
- **Draw shortfall** (`build.rs:442`): `DrawCase::try_expected`'s "funding does not cover the draw" (now the constant `draw::FUNDING_SHORT`) is `funding_short`; every other reference refusal stays `refused`. Test: the same `f1_web` test.
- **`QuickKey` and `QuickSignInput` `Debug`** print the secret key as `<redacted>` (`dto::REDACTED`). Test: `api.rs::quick_keys_are_redacted_in_debug`.
- **Fixtures out of the deployed wasm:** `lottery` has a new feature `fixtures` (default on): `web::fixtures` and the golden-digest lines (`digest::lines`: `abi_digests`, `tx_digests`, `review_digests`, `all_lines`) are behind it; `digest::template_list` stays in every build (`start` reports the template hashes from it). `crates/web` enables it only through a dev-dependency (resolver 2: never in `cargo build -p web`) or its own dev feature `fixtures` (which also exports `devFixtureNames`). W4 scans for the module paths `lottery::web::fixtures` and `lottery::digest::lines` (mangled and plain), the export, and the raw bytes of the fixtures' wallet key (32 × 0x21): release 0, mainnet 0, dev 2 / 5 / 1 (the positive control; nothing links `digest::lines` outside tests, so its line has no positive control of its own).
- **Left (not a fixture module):** the release wasm still holds the bytes of `FUNDING_SK` (32 × 0x07, twice) and `ADMIN_SK` (32 × 0x0a, once). They are the default fields of the `*Case` constructors (`BuyCase::new`, `CloseCase::new_with`, …) that `web::cases` starts from and then overwrites: every web builder sets `funding_sk` to its placeholder and signs nothing (`finish_unsigned`). Removing them means filling the cases without the fixture constructors, a refactor of `cases.rs` with byte-identity risk for no gain in trust or money; left for F3 or later, recorded here.

### Negative controls (each fix disabled by a temporary edit, then restored from a copy)

| Disabled | Test | Result |
|---|---|---|
| the field-by-field figure check in `unsigned_of` | `assemble_binds_every_figure` | FAILED: `buy b=1 ref: id: assembled` |
| the `owner_type` check | `deposit_owners_are_withdrawable_from_the_page` | FAILED: the ECDSA owner built |
| the pool-number comparison on the drawn path | `roots_and_numbers_come_from_chain_objects` | FAILED: another X.hash gave other numbers as Ok |
| the draw `funding_short` mapping | `small_change_and_short_draws_have_their_codes` | FAILED: `a draw funded short: refused (16)` |
| (before the fix) the change-dust diagnosis | the same test | FAILED: `extra 0: funding_short (12)` |
| one digit of `w1_verify.txt` line 200 | W1 in wasm | FAILED: `request_deposit #0 decodeRedeem vault: the wasm result differs from the native one` |
| `default = ["simnet"]` in `web/Cargo.toml` | W4 | FAILED: `start(simnet) before testnet: {"ok":{"network":"simnet",…` |
| `default = ["fixtures"]` in `web/Cargo.toml` | W4 | FAILED: `the release artifact contains lottery::web::fixtures` |
| (found by W4 itself) a doc comment naming `quickWalletSign` on `BuiltTx` | W4 | FAILED: `the mainnet build contains quickWalletSign` (tsify copies doc comments into the `.d.ts`); the comment was reworded |

### Commands run (outputs)

- Reviewer probes on the unchanged tree: `cargo test -j 2 -p web --release --test review_f1_probes -- --nocapture --test-threads=1`: P2 built a 2-of-2 P2SH owner; P4 `checkRoots` passed with m = 2 and 30 against a marker of m = 10, and `winningNumbers` gave other numbers as Ok for m − 1 and round + 1; P5 `funding_short` for every change below about 0.5 KAS; P6 assembled with a tampered fee, ID, wallet-back and output value. `cargo test -j 2 -p lottery --release --test review_f1_extra_digests --test review_f1_sign_attacks -- --nocapture`: 20 `REVIEWDIGEST` lines (saved, then appended); R1-R3 3 passed.
- `cargo test -j 2 -p lottery --release --test f1_web --test f1_bytes --test f1_sign -- --nocapture --test-threads=1` (after the fixes): `f1_bytes` `97 lines equal the golden file`; `f1_sign` 3 passed; `f1_web` 7 passed.
- `cargo test -j 2 -p web --release --test api -- --nocapture --test-threads=1`: 13 passed, 1 ignored (`write_golden`); "152 tampered BuiltTx refused (built_mismatch) by assemble and quickWalletSign", "290 verify lines equal the golden file", "340 mutated calls parsed, 22 built, none panicked".
- `cargo test -j 2 -p web --release --features simnet --test api other_network`: 1 passed.
- W1: `cargo test -j 2 -p web --target wasm32-unknown-unknown --release --test w1_wasm -- --nocapture`:
  ```
  W1 API: 18 builds through the JS boundary equal the native golden file
  W1 library: 97 digest lines equal the native golden file
  W1 verify: 290 decode and verify calls through the JS boundary equal the native golden file
  W1 sign: 7 transactions signed by the quick wallet and assembled in wasm; a flipped signature and a changed figure refused for each
  test w1_wasm_equals_native ... ok
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out; finished in 1.21s
  ```
- W4: `cargo test -j 2 -p web --release --test w4_mainnet -- --nocapture` (77 s with cold `target/w4` builds; 12-16 s warm):
  ```
  release: cargo build -j 2 -p web --target wasm32-unknown-unknown --release -> …\crates\target\wasm32-unknown-unknown\release\web.wasm (6342345 B)
  dev: cargo build … --target-dir …\crates\target\w4 --features simnet,fixtures -> … (6421896 B)
  mainnet: cargo build … --target-dir …\crates\target\w4 --features mainnet -> … (1370285 B)
  quickWalletNew               release    4  mainnet    0
  quickWalletImport            release    4  mainnet    0
  quickWalletSign              release    5  mainnet    0
  quick_wallet                 release    8  mainnet    0
  KASPALIONARE_QUICK_WALLET    release    1  mainnet    0
  lottery::web::fixtures       release    0  dev    2  mainnet    0
  devFixtureNames              release    0  dev    5  mainnet    0
  lottery::digest::lines       release    0  dev    0  mainnet    0
  fixtures WALLET_SK bytes     release    0  dev    1  mainnet    0
  release: started on testnet-10 ; start(simnet), start(mainnet), a kaspasim: address and change refused: address_network
  dev: started on simnet with kaspasim addresses; 18 fixture calls
  test result: ok. 1 passed; 0 failed
  ```
  The nested `cargo build` into the default target directory does not block on the outer `cargo test`'s lock.
- `cargo check -j 2` with: `-p lottery --no-default-features`, `… --features mainnet`, `… --features fixtures`, `… --target wasm32-unknown-unknown`; `-p web`, `--features simnet`, `simnet,mainnet`, `mainnet`, `fixtures`, `--tests --features simnet`; `-p tn10`: all `Finished`, no warnings.
- Suite: see "Suite at the end of the F1 review".

### Files changed

- `crates/lottery/Cargo.toml`: the `fixtures` feature (default on).
- `crates/lottery/src/web/mod.rs`: `fixtures` gated; codes 31 `built_mismatch`, 32 `owner_type`, 33 `change_dust`.
- `crates/lottery/src/web/build.rs`: `with_fee` with `diagnose` (`change_dust`); the draw's `funding_short`.
- `crates/lottery/src/draw.rs`: `FUNDING_SHORT`.
- `crates/lottery/src/buy.rs`: `external_signer_units`; the `OwnerPub` comment.
- `crates/lottery/src/digest.rs` and `src/digest/lines.rs` (new): `template_list` in every build; the lines behind `fixtures`, plus `review_digest` and `review_digests`.
- `crates/lottery/tests/fixtures/f1_digests.txt`: 20 lines appended.
- `crates/lottery/tests/f1_bytes.rs` (doc), `f1_web.rs` (the codes test; `small_change_and_short_draws_have_their_codes`), `f1_sign.rs` (new: the reviewer's `review_f1_sign_attacks.rs`, renamed, with a header). `review_f1_extra_digests.rs` is folded into `digest/lines.rs` and removed.
- `crates/web/Cargo.toml`: the `fixtures` feature; `lottery` with `fixtures` as a dev-dependency.
- `crates/web/src/api.rs`: `check_roots`, `winning_numbers`, `built_tx`, `unsigned_of` (with the figure check), `assemble`, `build_request_deposit`.
- `crates/web/src/dto.rs`: `RootsInput`, `RootsOut`, `RoundObjs`, `WinningInput`, `WinningOut`; the redacted `Debug` of `QuickKey` and `QuickSignInput`; summary docs.
- `crates/web/src/bindings.rs`: `devFixtureNames` (feature `fixtures`). `quick_wallet.rs`: `unsigned_of(net, …)`. `lib.rs`: docs.
- `crates/web/tests/api.rs` (the new tests; the reviewer's `review_f1_probes.rs` folded in and removed), `tests/common/mod.rs` (the draw with winners, the verify calls), `tests/w1_wasm.rs` (the reviewer's `review_f1_wasm_assemble.rs` folded in and removed), `tests/w4_mainnet.rs`, `tests/fixtures/w1_api.txt` (6 lines), `tests/fixtures/w1_verify.txt` (new, 290 lines).
- `.gitattributes` (`w1_verify.txt -text`), `CLAUDE.md` (the F1 and web test commands, the features), this file (this section, the stage log, the corrected part B paragraph).
- Not touched: `covenants/`, `crates/lottery/templates/`, the 77 original golden lines, tn10, simnet.

### Proposed DECISIONS rows (for the orchestrator)

- (a) **The page passes no derived game value.** `checkRoots` takes `{covenantId, against, purchases}` and `winningNumbers` `{covenantId, from: {closed: {marker, controller}} | {drawn: {pool, drawer}}, anchorProof}`. Rust derives the round, m and D: D = marker creation DAA + seed_offset (closed), or the pool's claim deadline − draw_window − claim_window (drawn, drawer check 19); on the drawn path the numbers must equal the pool's `win`.
- (b) **A built transaction's figures are bound to it.** `assemble` and `quickWalletSign` recompute every `BuiltTx` field from its transaction and refuse a difference (31 `built_mismatch`). The summary is a function of the transaction alone: `walletIn` = the inputs the wallet signs; `toWallet` = a plain output paying the script of one of them. The drawer input's units are bound by its committed budget only (its seq-commit opcode needs the node's chain).
- (c) **Vault deposits from the page are Schnorr P2PK owners only** (32 `owner_type`). D67 is unchanged (any P2PK or P2SH owner at the covenant level); the page creates only positions its signers (KasWare, the quick wallet: Schnorr P2PK) can withdraw. Withdrawals still accept all three owner classes, for positions made by other tools.
- (d) **33 `change_dust`:** the fee loop never drops change; when the coins cover the transaction without change but the change cannot pay for its own output, the refusal says so. Money behaviour unchanged (see the owner decision).
- (e) **The `fixtures` feature** (lottery, default on; web, dev only): the fixture calls, their wallet key and the golden-digest lines are out of the release wasm; W4 checks the release artifact, built by the documented wasm build command, for them and for the simnet.
- (f) **Golden digests:** 97 lines; lines 78-97 are the F1 review's 20 shapes, appended with the reviewer's digest function and its pre-F1 evidence, never regenerated. `w1_api.txt`: the 6 withdrawal lines rewritten for the `toWallet` definition (transactions unchanged). New `w1_verify.txt` (290 decode and verify results, native = wasm).
- (g) **W1 is one ordered wasm test** (`w1_wasm_equals_native`: API, library, verify, sign).

### Owner decision (needs the owner; nothing was decided)

**Small change: refuse, or pay it as fee?**
- What happens today: if your coins pay for a ticket with a little left over, but that leftover is too small to be sent back on its own (on Kaspa, a tiny output costs more in fee than it is worth; in the tests, at the floor fee rate, a leftover of 0.1 KAS was refused and 0.5 KAS went through), the page refuses the purchase. It now says why ("The change left over is too small to send back") instead of "your coins do not cover this". Before and after this stage, no money moves in that case.
- The alternative: build the purchase without a change output and let the leftover go to the network as extra fee. The player would lose that leftover (at most the size of the band, so well under 1 KAS at the floor rate, more if fees rise), but the purchase would go through. When the leftover is exactly zero (the coins equal cost plus the fee), nothing is lost at all, yet the page still refuses today because the fee loop overshoots.
- Trust: none either way; the covenants do not care about the change.
- Money: refusing costs the player nothing but a retry with other coins; dropping the change costs the player the leftover. On testnet the coins have no value.
- Work: small (one rule in `build::with_fee`: accept the no-change build when its extra fee is below a limit the owner picks, for example 0, or 0.01 KAS), with tests.
- Recommendation: accept the no-change build only when the extra fee is 0 (the exact case), and keep refusing otherwise with `change_dust`, so the page never silently gives away coins. The page (F3) can then suggest adding or removing a coin.

### Left and open

- The drawer input's units: bound by its budget only (above). A page never displays units today.
- `toWallet` uses the inputs' scripts: a change address that is not the address of a signed coin is shown as not coming back. Every page flow uses the wallet's own address for both.
- `close()`'s caller output below `MIN_CALLER_OUTPUT` stays `policy` even when the coins are short (the reviewer flagged only the draw).
- The drawn path of `winningNumbers` needs a live pool of the round. Once a round's pools are all claimed, expired or absorbed, its numbers can only be verified on the closed path, which needs the spent marker (F5 should decide what the round explorer shows then: the indexer's copy marked unverified, or the record).
- The fixture-constructor keys left in the release wasm (above).
- Not run in this stage: `simnet` (RocksDB, about 14 minutes; it uses `lottery`'s default features, which now include `fixtures`, unchanged API), a fresh pre-F1 worktree run of the 20 review shapes (the reviewer's run is the evidence).

### Suite at the end of the F1 review

- `cargo test -j 2 --no-fail-fast` (from `crates/`, debug; default members `harness` and `lottery`): 55 test binaries, 1,108 passed, 0 failed, 2 ignored (`f1_bytes::write_golden` and the old `m4_mutants` one).
  - This includes `f1_bytes` (97 lines), `f1_web` 7/7, `f1_sign` 3/3, `f1_cases`, `f1_w3`, `f1_unsigned`, `f1_templates` and `f0_unsigned`.
  - It also includes `m7_seed` 15/15: the SP12 seed gate holds.
  - As before F1 (D113), `spike_vdf` overflowed its stack in debug (`sv_measure_masses_and_stack`); it is the one failed target.
- `cargo test -j 2 -p lottery --release --test spike_vdf`: 14 passed, 0 failed (361 s).
- `cargo test -j 1 --release -p tn10`: 25 passed, 0 failed.
- `crates/web` (the web test command): `api` 13 passed, 1 ignored (`write_golden`); W1 1/1 in wasm (18 + 97 + 290 lines, 7 signed and assembled); W4 1/1 (release artifact, dev control, mainnet).
- Native transactions byte-identical: the 77 original lines of `f1_digests.txt` are unchanged and green, plus the 20 appended. No covenant or template change. `Cargo.lock` unchanged.

## F1 re-review and fixes (fix agent, 2026-10-25)

### The re-review

- **Who and what:** `covenant-reviewer` re-reviewed the fix commit `f7f8102` ("F1 review and fixes"). It left one untracked probe file, `crates/web/tests/review_f1fix_probes.rs` (P1, P1b, P2, P3, P5); its report is not in the repo, so the verdicts below are as the orchestrator passed them on, plus what the probes showed when rerun here on the unchanged tree.
- **Verdicts, per item:**

  | Item | Finding | Verdict | Now |
  |---|---|---|---|
  | S1 (P1) | Before the draw, whoever serves the anchor proof chooses the winning numbers: on the closed path nothing checkable off-chain bound X.hash (`anchor.rs` checked only P's commitment and the DAA window). Rerun here: X.hash ground in 1,751,388 tries gave the target `[4, 5, 7, 27, 29] + 1` as Ok; the drawn path gave the real `[3, 5, 7, 27, 29] + 7` for the same round | should-fix | fixed (below) |
  | N1 (P1b) | The calls check state, script and covenant ID agreement, not that an outpoint is live; the doc comments said "a live marker or pool (checked against its UTXO)". Rerun: another instance's covenant ID is `chain_mismatch`; a pool at an outpoint that does not exist is Ok | note | docs fixed; F3 requirement below |
  | N2 (P2) | One payer: `toWallet` = a plain output paying the script of any signed input, and the builder takes Schnorr coins of several keys, so a second key's change showed as "back to the wallet". Rerun: sign items `[1, 2]`, walletBack 9,944,832,000 to the second key's address; the quick wallet then signs its own input only and `assemble` is `signed_missing` | hardening | fixed (below) |
  | P3 | `request_deposit` owners in upper case or without the prefix | holds (`address_network` both) | kept as a test |
  | P5 | The pre-review `toWallet` rule applied to today's builds reproduces the pre-review golden file (`b3b3520`'s `w1_api.txt`) | holds | kept as an ignored test; rerun here: 18 of 18 lines equal (`REVIEW_OLD_W1` = `git show b3b3520:crates/web/tests/fixtures/w1_api.txt`) |
  | N4 | W1's signing half covered 7 of 18 shapes in wasm; F6's W4 must check the actual deploy artifact | note | 14 of 18 now; the rest recorded for F6 |
  | N5 | tn10 instances made through `tn10/src/cases.rs` or `wallet_probe.rs` keep the public default admin key | note | recorded for F6 (no code) |

### S1: what is bound before the draw, and what cannot be

What `draw()` does on chain: `OpChainblockSeqCommit(X.hash)` returns a commitment only if X is a selected-chain ancestor from the point of view of the block that includes the draw (else `BlockNotSelected`), and that commitment is the stored header's `accepted_id_merkle_root` (rusty-kaspa v2.1.0 `crypto/txscript/src/opcodes/mod.rs:1389-1404`, `consensus/src/model/services/seq_commit_accessor.rs:31-43`). The drawer then checks the proof's X fields against it and P's fields against X's parent commitment.

What the page can check off-chain, all verified in the v2.1.0 sources (the cargo checkout of `01b532e`; `refs/` is empty on this machine):
- **The header hash** covers version, parents by level, `hash_merkle_root`, `accepted_id_merkle_root`, `utxo_commitment`, timestamp, bits, nonce, DAA score, blue score, blue work and pruning point (`consensus/core/src/hashing/header.rs:6-35`). So a header that hashes to X.hash fixes X's commitment field.
- **`accepted_id_merkle_root` is the KIP-21 commitment** post-Toccata: consensus recomputes it and refuses a block whose header differs (`consensus/src/pipeline/virtual_processor/utxo_validation.rs:202-207`), and the opcode returns it. The mergeset context in that commitment takes the header's DAA score and the block's blue score (`utxo_validation.rs:614-618`), so both must equal the proof's.
- **But consensus checks that field for chain blocks only.** UTXO validation, which includes the commitment check, runs while the virtual's chain is walked (`consensus/src/pipeline/virtual_processor/processor.rs:424-470`); a block off the chain is never checked. So "a real block whose header matches" is not enough: the node must also say X is on its selected chain (`getBlock` verbose data `is_chain_block`, `rpc/core/src/model/block.rs:67-77`).

The fix (`lottery::anchor::BlockFields::check_header`, `lottery::web::verify::check_anchor_block`, `web::api::winning_numbers`):
- **`winningNumbers` closed path** now takes `{closed: {marker, controller, anchorBlock: {header, isChainBlock}}}`: the anchor block as a public node reports it (`getBlock(X.hash)`: the header and the verbose `isChainBlock`). Rust checks that the header's stated hash is its hash, that it hashes to the proof's X.hash, that its `acceptedIdMerkleRoot` is the KIP-21 commitment of the proof's X fields, that its DAA and blue score are the proof's, and that the node reports X on its selected chain; any failure is `anchor_invalid`. With the existing checks (P's fields hash to X's parent commitment, dP < D ≤ dX), X is then exactly the node's first chain block with DAA ≥ D: the proof server has no choice left.
- **`WinningOut.status`**: `final` on the drawn path (equal to the pool's `win`, which `draw()` wrote after the on-chain check), `provisional` on the closed path. The page shows "provisional" before the draw and never "verified".
- **The header type** `BlockHeader` mirrors rusty-kaspa's `RpcHeader` (`rpc/core/src/model/header.rs:35-51`) with the boundary conventions (64-bit integers as decimal strings, hashes as hex, `blueWork` big-endian hex of up to 24 bytes, leading zeros optional). Parents are capped at 255 levels and 16,320 hashes (`input_range`).

What is left, in plain words:
- **Trust in the public node.** Before the draw, the numbers are as good as the node's word, the same trust the page already places in it for balances and live UTXOs (spec 6.3). The indexer can no longer pick them.
- **The page must fetch the header itself.** A wasm call cannot tell a node's header from a made-up one: anyone can write a header with the right commitment and grind its nonce until its hash gives chosen numbers (the test does it: about 390,000 nonces). So F3 must fetch `getBlock(X.hash)` from a public node, never take the header from the indexer. Even a made-up header gives only a `provisional` result; on the drawn path the same proof is `chain_mismatch`. No ground proof gives an Ok `final` result.
- **Not added:** a proof-of-work check on the header (`bits`, nonce). It would make each grind try cost real mining work, but the header's own `bits` can be set to the easiest target, and a floor needs the network's current difficulty, which is again the node's word. Not worth it while the header comes from the node.
- **Reorgs.** Until `draw()`, the node's chain can still change at the anchor (DECISIONS S4-4: drawers wait 50 blue score). "Provisional" covers it.
- **The drawn path's `anchorHash`.** After the draw the numbers are bound (to the pool), but `anchorHash` is the proof's: a proof whose X.hash was ground to give the same numbers would show a wrong hash next to the right numbers. F5 should show the anchor hash on a drawn round only after the same anchor-block check (cheap: an optional `anchorBlock` on the drawn path), or not show it as verified.

### What changed

1. **S1** (above).
   - `crates/lottery/src/anchor.rs`: `BlockFields::check_header` (the four header checks, with sources); `header_committing_to` (fixtures and tests only: a header committing to given fields); unit test `header_binds_x_hash_to_its_fields`.
   - `crates/lottery/src/web/verify.rs`: `check_anchor_block` (`anchor_invalid`).
   - `crates/web/src/dto.rs`: `BlockHeader`, `AnchorBlock`, `RoundObjs::Closed.anchorBlock`, `NumbersStatus`, `WinningOut.status`; doc comments.
   - `crates/web/src/convert.rs`: `header` (DTO to consensus `Header`, no panic on any input) and `header_dto`.
   - `crates/web/src/api.rs`: `winning_numbers` (the closed path checks the anchor block; the status).
   - `crates/tn10/src/anchor.rs`: `anchor_from_rpc` self-check (5): the node's real header passes `check_header` (real-data evidence below).
2. **N2.** `api::built_tx` refuses sign items of more than one key with the new code 34 `mixed_signers` ("All the coins to sign must come from one wallet address."). Every builder and `unsigned_of` (so `assemble` and `quickWalletSign`) run it. No fixture build changes: every one of the 18 has one key (a withdrawal's owner coin is its funder's).
3. **N1.** Doc comments (`api.rs`, `bindings.rs`, `dto.rs`, `lib.rs`, `verify.rs`) now say what is checked: the state against its UTXO entry (script) and the instance (covenant ID); that the entry is unspent is the page's to ensure.
4. **N4.** W1's signing half signs and assembles 14 of the 18 fixture shapes in wasm (added: both returns, `close k=4`, `request_deposit`, the whole Schnorr withdrawal, both settlements). The other 4 are the ECDSA and P2SH owners' withdrawals, which the quick wallet cannot sign (`signed_missing`, asserted natively).
5. **The probe file** is folded into `crates/web/tests/api.rs` and removed: P1 into `closed_numbers_are_provisional_and_bound_to_the_nodes_block`, P1b into `calls_check_entries_not_liveness`, P2 into `one_key_signs_a_page_build`, P3 into `deposit_owner_encodings_are_refused`, P5 into the ignored `old_to_wallet_rule_reproduces_the_pre_review_golden_file` (needs `REVIEW_OLD_W1`).

### Tests (written first; P1 and P2 reproduced the fault on the unchanged tree before any change)

- `closed_numbers_are_provisional_and_bound_to_the_nodes_block` (web, S1): honest closed path `provisional`, drawn path `final`, same numbers, D and anchor hash; the reviewer's grind (X.hash only; 370,470 tries in the new fixture) refused on the closed path (`anchor_invalid`, "hashes to") and on the drawn path (`chain_mismatch`); negative control: off-chain the ground proof still anchors D and maps to the target; the residual: a made-up header (389,292 nonces) passes as `provisional` only and the drawn path refuses it; each anchor-block check alone (not a chain block, a wrong stated hash, another block's header, a changed commitment, DAA or blue score with the hash re-pointed); malformed headers (`input_format`, `input_range`); the header DTO round trip, `blueWork` without leading zeros.
- `roots_and_numbers_come_from_chain_objects` (updated): another X.hash is now refused on the closed path too; status checked on both paths; the closed numbers equal the pools'; the JSON without `anchorBlock` is refused.
- `one_key_signs_a_page_build` (web, N2): a second key's coin is `mixed_signers` (with the second key's or the wallet's change address); control: two coins of the wallet's key build and the change is `toWallet`; `built_tx` refuses an `Unsigned` whose second sign item is another key's; every fixture build has one key.
- `calls_check_entries_not_liveness` (N1), `deposit_owner_encodings_are_refused` (P3).
- `lottery`: `anchor::tests::header_binds_x_hash_to_its_fields`; `f1_web::error_codes_are_stable` (34 codes).
- **The fixture:** `web/tests/common::drawn()` now uses an X whose hash is a header's (the sim's X fields do not depend on X.hash), so the closed and drawn paths give the same numbers; `Drawn.anchor_block` is that header.

### Negative controls (each fix disabled by a temporary edit, then restored from a copy; `cmp` equal)

| Disabled | Test | Result |
|---|---|---|
| the anchor-block check on the closed path | `closed_numbers_…` | FAILED at the reviewer's ground proof (accepted) |
| the chain-block check | `closed_numbers_…` | FAILED at "not a chain block" (accepted) |
| the header-hash comparison in `check_header` | `header_binds_x_hash_to_its_fields`; `closed_numbers_…` | both FAILED (the ground proof accepted) |
| the status (always `final`) | `closed_numbers_…` | FAILED at the honest closed path |
| the one-key check | `one_key_signs_a_page_build` | FAILED (the two-key buy built) |

### Golden files

- `crates/lottery/tests/fixtures/f1_digests.txt` and `crates/web/tests/fixtures/w1_api.txt`: unchanged (`git diff` empty; `write_golden` rewrote `w1_api.txt` byte-identical). Native transactions byte-identical.
- `crates/web/tests/fixtures/w1_verify.txt`: 290 → 291 lines; only the drawn fixture's lines moved (results of verify calls, no transaction):
  - `checkRoots marker`, `checkRoots pool`: the fixture's winning purchases follow the new X (same length, other roots);
  - `winningNumbers closed` (191 → 214 bytes), `winningNumbers drawn pool 0/1` (191 → 208 bytes): the new `status` field and the new X's numbers;
  - new `winningNumbers closed, another X` (refused, `anchor_invalid`);
  - the 286 other lines are unchanged.

### Real-data evidence (testnet-10)

- `cargo run -j 2 -q --release -p tn10 -- anchor-check 20` (public resolver; reads `TN10_PRIVATE_KEY` only to build its context, submits nothing, prints no key): the anchor for D = 605,950,348, X `fa128009…bfa4` (DAA 605,950,348) and P `bec3b340…e5c9` (DAA 605,950,346), then 20 consecutive chain blocks after `e5bfe181…2c89`: `anchor-check OK … every self-check passed` and `20 consecutive chain blocks … rebuilt; every recomputation = header`. Self-check (5) is new: for all 22 real blocks the node's header (RPC `RpcHeader` to consensus `Header`) hashes to the block's hash, its `accepted_id_merkle_root` is the commitment of the rebuilt fields, and its DAA and blue score are theirs. So the header bind holds on real testnet-10 data; the boundary type `BlockHeader` is pinned by the round trip in the web test (the JS shape of kaspa-wasm's header is F3's to confirm).

### Commands run (outputs)

- Baseline, the reviewer's probes on the unchanged tree: `cargo test -j 2 -p web --release --test review_f1fix_probes -- --nocapture --test-threads=1`: 5 passed (probes print, they do not fail); P1 `after 1751388 tries the forged proof gives [4, 5, 7, 27, 29]+1 … Ok; real numbers [3, 5, 7, 27, 29, 7]`; P1b `a pool at an outpoint that does not exist: ok = true`; P2 `sign items [1, 2] … toWallet outputs [("9944832000", …)]`, `assemble … SignedMissing`; P3 `AddressNetwork` twice.
- `cargo test -j 2 -p lottery --lib anchor`: 3 passed. `cargo test -j 2 -p lottery --test f1_web error_codes`: 1 passed.
- `cargo test -j 2 -p web --release --test api -- --nocapture --test-threads=1`: 17 passed, 0 failed, 2 ignored (`write_golden`, the P5 test); "S1: X.hash ground in 370470 tries to [2, 4, 7, 10, 16, 1]: closed path anchor_invalid …, drawn path chain_mismatch"; "S1 residual: a made-up header (389292 nonces) gives the target as provisional only"; "291 verify lines equal the golden file"; "152 tampered BuiltTx refused"; "340 mutated calls parsed, 22 built, none panicked".
- `cargo test -j 2 -p web --release --test api -- --ignored write_golden`: "wrote 18 + 291 lines"; `git diff --stat` on the fixtures: `w1_verify.txt` only.
- `REVIEW_OLD_W1=… cargo test -j 2 -p web --release --test api -- --ignored old_to_wallet_rule`: 1 passed (18 of 18 lines).
- W1: `cargo test -j 2 -p web --target wasm32-unknown-unknown --release --test w1_wasm -- --nocapture`:
  ```
  W1 API: 18 builds through the JS boundary equal the native golden file
  W1 library: 97 digest lines equal the native golden file
  W1 verify: 291 decode and verify calls through the JS boundary equal the native golden file
  W1 sign: 14 transactions signed by the quick wallet and assembled in wasm; a flipped signature and a changed figure refused for each
  test result: ok. 1 passed; 0 failed
  ```
- W4: `cargo test -j 2 -p web --release --test w4_mainnet -- --nocapture`: 1 passed (79 s); release 6,372,732 B, dev 6,458,629 B, mainnet 1,373,625 B; quick-wallet symbols release 4/4/5/8/1, mainnet 0; fixtures release 0, dev 2/5/1; `start(simnet)`, `start(mainnet)` and `kaspasim:` refused on the release build; the mainnet build refuses to load (`mainnet_refused`).
- `cargo check -j 2` with `-p lottery --no-default-features`, `… --features mainnet`, `… --features fixtures`, `… --target wasm32-unknown-unknown`; `-p web`, `--features simnet`, `mainnet`, `simnet,mainnet`, `fixtures`, `--tests --features simnet`; `-p tn10`; `-p simnet`: all clean, no warnings.

### Files changed

- `crates/lottery/src/anchor.rs`, `src/web/verify.rs`, `src/web/mod.rs` (code 34), `tests/f1_web.rs` (the code table).
- `crates/web/src/api.rs`, `convert.rs`, `dto.rs`, `bindings.rs` and `lib.rs` (docs); `tests/api.rs`, `tests/common/mod.rs`, `tests/w1_wasm.rs`, `tests/fixtures/w1_verify.txt`; `tests/review_f1fix_probes.rs` folded in and removed.
- `crates/tn10/src/anchor.rs` (self-check 5).
- `CLAUDE.md` (the web test description), this file.
- Not touched: `covenants/`, `crates/lottery/templates/`, `f1_digests.txt`, `w1_api.txt`, `Cargo.toml`/`Cargo.lock`, and the unrelated uncommitted files.

### Proposed DECISIONS rows (for the orchestrator)

- (h) **Winning numbers say whether they are final.** `winningNumbers` returns `status`: `final` on the drawn path (the pool's `win`, written by `draw()` after its on-chain anchor check), `provisional` on the closed path. The page never shows provisional numbers as verified.
- (i) **Before the draw, X is bound to the node's anchor block.** The closed path takes `anchorBlock: {header, isChainBlock}` from a public node's `getBlock(X.hash)`: the header must hash to X.hash, commit to X's fields (`acceptedIdMerkleRoot` = SeqCommit) with the same DAA and blue score, and the node must report X on its selected chain (`anchor_invalid` otherwise). The page fetches it from a node by X.hash, never from the indexer. No proof-of-work check (the header comes from the node).
- (j) **One payer per page build:** code 34 `mixed_signers`; every input to sign is of one key.
- (k) **Liveness is the page's:** no call checks that a UTXO entry is unspent; the page reads every entry it passes to the wasm from a public node by outpoint, never from the indexer.
- (l) **W1 signs 14 of 18 shapes in wasm** (all the quick wallet can sign); `w1_verify.txt` is 291 lines and its drawn fixture's X hash is a header's.
- (m) **tn10 `anchor_from_rpc` self-check (5):** the node's header passes `BlockFields::check_header` before anything is built from a proof.

### Requirements recorded for later stages (no code here)

- **F3 (web shell):**
  - Every UTXO entry passed to the wasm (`CovUtxo.utxo`, wallet coins) comes from a public node by outpoint (`getUtxosByAddresses` or the covenant's entry by outpoint), never from the indexer. The calls check state, script and covenant ID, not liveness; a builder given a spent entry builds a transaction the node rejects (no money moves), but a verify call given one shows stale state as current.
  - `winningNumbers` before the draw: take X.hash from the proof, call `getBlock(X.hash, false)` on a public node, and pass its header and `verboseData.isChainBlock` as `anchorBlock`. Never take the header from the indexer. Convert the RPC header's 64-bit integers to decimal strings and `blueWork` to hex; check the JS shape of kaspa-wasm's `getBlock` header on the first browser run (not verified in this stage).
  - Show `provisional` results as provisional (never "verified"); only `final` may be shown as verified.
- **F5 (rounds, verify):** the drawn path's `anchorHash` is the proof's only (see S1 "what is left"); bind it with an optional `anchorBlock` on the drawn path, or do not label it verified. Once a round's pools are gone the drawn path is unavailable (F1 review "Left and open").
- **F6 (beta, deploy):**
  - W4 must check the artifact that is actually deployed (the file Vercel serves, after wasm-bindgen and any optimiser), not only `target/wasm32-unknown-unknown/release/web.wasm` (re-review N4).
  - W1's signing half covers the 14 shapes the quick wallet can sign; the 4 ECDSA and P2SH owner withdrawals are signed in wasm by no test (the page never creates such positions, F1 review item 3).
  - **The beta instance must not be launched through `tn10/src/cases.rs` or `tn10/src/wallet_probe.rs`** (re-review N5). Their instances keep the default admin hash `admin_hash(ADMIN_SK)` (`lottery/src/params.rs:128`, `close.rs:188`; `ADMIN_SK` = 32 × 0x0a, in the repo), so anyone can call `set_params()` on them (fees, the fee address). Only the campaign path sets the operator's own admin key (`tn10/src/campaign.rs:1746`). Launch the beta through the campaign path or another path that sets a fresh admin key, and check the bundle's admin hash before announcing it.

### Owner decision (needs the owner; nothing was decided)

**Show the winning numbers before the draw?**
- What happens now: between `close()` and `draw()` the numbers already exist (they come from a block of the chain), and the page can show them early. After this fix they come from the public node's view of the chain, checked by the page, and are marked "provisional". Once `draw()` is on chain they are marked final.
- Trust: before the draw you trust the public node (the same node the page trusts for balances); the indexer can no longer pick them. If the chain reorganises around the anchor block before the draw, the provisional numbers can change.
- Money: none; showing numbers early moves no money and the draw still checks everything on chain.
- Work: none to keep it; small to hide it (F5 shows nothing until the draw).
- Recommendation: keep it, labelled "provisional".

### Suite at the end of the F1 re-review

- `cargo test -j 2 --no-fail-fast` (from `crates/`, debug; default members `harness` and `lottery`): 55 test binaries, 1,109 passed, 0 failed, 2 ignored (one more than before: `anchor::tests::header_binds_x_hash_to_its_fields`). As before (D113), `spike_vdf` overflows its stack in debug and is the one failed target. Includes `f1_bytes` (97 lines unchanged), `f1_web` 7/7 (34 codes) and `m7_seed` 15/15 (the SP12 seed gate holds).
- `cargo test -j 2 -p lottery --release --test spike_vdf`: 14 passed, 0 failed (359 s).
- `cargo test -j 1 --release -p tn10`: 25 passed, 0 failed.
- `crates/web`: `api` 17 passed, 2 ignored; W1 1/1 in wasm (18 + 97 + 291 lines, 14 signed and assembled); W4 1/1.
- Native transactions byte-identical: `f1_digests.txt` and `w1_api.txt` unchanged. No covenant or template change. `Cargo.toml` and `Cargo.lock` unchanged.
- Not committed (as asked).

## F2 (stage agent, 2026-10-25)

### What was built

1. **`crates/indexer-api`** (new; a workspace member, not a default one): a read-only HTTP JSON server over the M8 indexer's store. It holds no keys, builds no transaction and writes no file.
   - **Source of truth: the store an indexer writes.**
     - It reads `index-log.jsonl` (`index/1`) from the start and applies each add line to `lottery::index::Rebuild`. Every rebuilt state is checked against the transaction's outputs, as the indexer does. It reverts on each remove line and ignores re-anchor lines (they change no state, M8 S4).
     - Then it polls the file (`--poll-ms`, default 1 s). New complete lines are applied, a partial last line waits, and a shorter file is read again from the start.
     - A line that does not apply stops it there. The last good state stays served, and `/v1/status` shows the error.
     - `index.json` is read only for `last_chain_block` and `last_chain_daa` (how far the indexer has got).
     - The genesis states come from the launch bundle (`bundle/1`), verified with `lottery::bundle::verify`. Its covenant ID must be `--cov`.
   - **Why follow the store and not embed the indexer loop:**
     - The server stays small and keyless, with no RPC client in it.
     - The same binary serves any store (tn10's, the simnet's, a copy).
     - It can run next to a live indexer without sharing its process.
     - For DX the one-process case is still there: a process that runs an indexer embeds the server with one call (`indexer_api::run(cfg)`), which is what `simnet dev` does. An embedding process can also give the server an anchor-proof source (`AnchorProvider`) for rounds that are closed but not drawn.
   - **Stack pick:**
     - axum 0.8.9 (`default-features = false, features = ["http1", "tokio"]`), hyper 1 and hyper-util 0.1.21 for the accept loop, tower-http 0.6.11 for CORS only, and tokio.
     - All of them were already in `Cargo.lock` (rusty-kaspa's tonic uses axum). The lock gains only the `indexer-api` package and the axum feature edges to hyper, hyper-util and tokio. No new crate.
     - No JSON or query extractor features: bodies are written with serde_json, and the one query parameter is parsed in `routes.rs`.
   - **Server basics:**
     - GET and HEAD only. Any other method is `method_not_allowed`, and no request body is read.
     - Everything is under `/v1/`; anything else is `not_found`.
     - Errors are typed JSON, `{error: {code, message, detail}}`, with one message per code.
     - CORS: GET and HEAD for any origin by default, or for the `--allow-origin` list.
     - Each response kind has a size cap (`web::indexer::LIMITS`, the same caps the page enforces).
     - Limits: a 10 s request deadline (`timeout`), 64 requests at once (`busy` above that), a 10 s header-read timeout, a 120 s connection lifetime and 256 connections.
     - Headers `cache-control: no-cache` and `x-content-type-options: nosniff`.
   - **Configuration:** flags with `INDEXER_API_*` environment fallbacks (`config.rs`):
     - `--store` and `--cov` (required);
     - `--bundle` (default `<store>/bundle.json`);
     - `--bind` (default `127.0.0.1:8790`);
     - `--allow-origin`, `--poll-ms`, `--timeout-ms`, `--max-connections`.

     Unknown flags are refused.
2. **`web::indexer`** (new module in `crates/web`): the response types, defined once in Rust and declared to TypeScript by tsify, so they are in the `.d.ts` the page imports (spec 1.1). The server serves them and the page parses them. The server depends on `web` natively; the wasm has no server dependency.
   - **Parsing:** `indexerLimits()`, and one `parseIndexer*` per response kind (`Status`, `Rounds`, `Round`, `Shard`, `Purchases`, `AnchorProof`, `Tickets`, `Objects`, `Error`).
     - Each takes the body *text* and checks its size cap before any JSON is read. So a huge response never becomes an object in the worker (F1 part C open question 4).
     - Unknown fields are refused.
   - **`checkIndexerInstance({bundle, covenantId, bundleHash?})`:** the bundle text is verified locally (`bundle::verify`) for this page's network and instance. The blake2b-256 of its bytes is the bundle hash (spec 6.5's build-time "launch bundle hash").
   - **`checkIndexerObjects({covenantId, objects, utxos})`:** each API object against the node's entry for its outpoint.
     - A missing entry means the object is not live: a mismatch.
     - The entry the API states must equal the node's.
     - The state bytes must rebuild the entry's script under the instance's covenant ID (`cases::live`).

     It returns each object as a `CovUtxo` ready for a builder, with its decoded view.
   - **`checkIndexerRound({covenantId, round, utxos, purchases?, anchorProof?, anchorBlock?})`** checks:
     - the objects, as above, each of the round it is served for;
     - the status, m and k against the chain objects (`statusChecked` says whether a live object showed the status);
     - the frozen roots against the marker's or a pool's;
     - the purchases against the chain's roots: `api::check_roots` against the marker or a pool once the round is closed, and each live shard's root while it is open;
     - the winning numbers against every live pool's `win`;
     - the anchor proof, through `api::winning_numbers`: drawn rounds give `final` numbers, equal to the pools'; closing rounds give `provisional` numbers, bound to the node's anchor block (F1 re-review S1);
     - the API's D and anchor hash against the proof's.
   - **One refusal code:** every disagreement these checks find is `chain_mismatch` ("The indexer's copy does not match the chain.", spec 6.3), whatever check found it. The detail names the underlying code (`tree_mismatch: ...`). The F1 exports keep their own codes.
   - Also: `RoleName::of` (`views.rs`) and `api_object` (the server's side of `ApiObject`).
3. **Doctored modes (TEST-ONLY, spec 8 "Untrusted API")**, in `indexer-api/src/doctor.rs`, compiled only with the feature `doctored`. The modes:

   | Mode | The lie |
   |---|---|
   | `wrong_state` | every object's state bytes describe another state (one integer field changed and re-encoded) |
   | `wrong_tree` | the first purchase of the first non-empty shard gets another bonus |
   | `missing_purchase` | the last purchase of that shard is dropped |
   | `extra_purchase` | a purchase is added to shard 0 |
   | `wrong_winning` | the round's winning bonus is changed |
   | `wrong_anchor_proof` | one byte of the proof's X.hash is flipped |

   **Gating:**
   1. The module exists only with the feature.
   2. Only the separate binary `indexer-api-doctored` (`required-features = ["doctored"]`) has a `--doctor` flag. The production binary `indexer-api` has none, in any build.
   3. The doctored binary refuses a non-loopback `--bind`, prints a `KASPALIONARE_DOCTORED_API ... TEST-ONLY` banner, and marks every response `x-indexer-doctored: <mode>`.
   4. `tests/production.rs` builds the production binary without the feature, as it ships. It finds neither the marker nor any mode name in it, and checks that the binary refuses `--doctor`. The doctored binary is the positive control.
4. **`simnet dev`** (new subcommand, `crates/simnet/src/dev.rs`). On a fresh private node:
   1. Round 1 runs through `sn::run_round`, with the production templates and the M8 testnet knobs (round_len = d_min = 6,000): buys, close, an anchor ground so that a held combination wins a prize tier, and the draw.
   2. The M8 indexer follows the node from its genesis into `crates/target/simnet-dev/<cov>/`, with `bundle.json`. It is `tn10/src/indexer.rs`, compiled in unchanged by `#[path]`, as simnet already does with `anchor.rs`.
   3. Round 2: buys made from the index's shard states, then a close. The round is left closing, mined past D + 60.
   4. Round 3: buys (open).
   5. The indexer API serves on `--api-bind`, with the node as its anchor-proof source.
   6. A **self-check** fetches every endpoint and runs the `web::indexer` checks against entries read from the node by address, and against the anchor block read from the node.
   7. Then it mines a block every `--block-ms` and the indexer follows, until Ctrl-C or `--for`.

   `crates/target/simnet-dev/current.json` names the API URL, the node's Borsh wRPC URL, the covenant ID and the store.
   - **`sn.rs` changes** (additive; SN1–SN4 unchanged):
     - the helpers are now `pub(crate)`;
     - `Cfg.edit: Option<fn(&mut GenesisCase)>` (None in SN1–SN4);
     - `Round.gf`: the genesis `Final`, for the bundle.
   - **The first attempt failed with round_len = 600.** The grind and the 50-block wait after round 1's close passed round 2's sales deadline, and a shard takes only one buy after its deadline (design 7.1), so the local engine refused the second buy on a shard. 6,000 fixes it; simnet mines through 6,000 blocks in seconds.
5. **CLAUDE.md:** the layout entry for `indexer-api`, and two command lines (Indexer API, Simnet dev instance).

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

### Tests (`crates/indexer-api/tests/`)

- **`api.rs`** (6 tests):
  - `endpoints_serve_their_types`: every endpoint's body parses as its `web::indexer` type, and the instance is the file byte for byte. It also checks:
    - the statuses (3 open, 2 closing, 1 drawn) and paging;
    - the per-shard lists equal the round's list and sum to its combinations;
    - the anchor proof is 272 bytes, from the draw;
    - tickets by payout, also percent-encoded; positions and requests by owner.
  - `errors_are_typed_and_writes_refused`:
    - 15 bad requests, each with its code;
    - a mainnet `kaspa:` address is `address_network`;
    - POST, PUT, DELETE and PATCH on three paths are `method_not_allowed`; HEAD is ok.
  - `cors_for_a_static_site`: any origin; the preflight allows GET only; with a list, only the listed origin.
  - `follows_a_growing_log`:
    - a partial line waits, and growth is applied;
    - a line whose txid is not its transaction's stops the follower, with the error and the last good state;
    - a rewritten, shorter log is read again.
  - `refuses_another_instance`.
  - `real_socket_and_header_timeout`: on a real socket, an idle connection was closed after 509 ms with a 500 ms header timeout.
- **`doctored.rs`** (3 tests, feature `doctored`):
  - `honest_answers_pass_every_check`, the positive control:
    - rounds 1/2/3 are `drawn`/`closing`/`open` with `statusChecked`, and their roots rebuilt;
    - round 1's numbers are `final`;
    - tickets, vault and positions pass, and so does the instance with its hash.
  - `every_doctored_answer_is_refused`: each mode, on each response it changes, gives `chain_mismatch` with the page message. The test also requires that the doctor changed exactly the expected rounds: 3 for the state and purchase modes, 1 for the winning numbers and the anchor proof.
  - `builders_refuse_doctored_purchases`: a buy built from a doctored shard list is `tree_mismatch` (the F1 builder check).
- **`production.rs`** (1 test): the production binary, built without the feature, contains none of the marker and the 6 mode names (0 each) and refuses `--doctor` (exit 2, "unknown flag --doctor"). The doctored binary contains each one (1 each) and refuses a non-loopback bind.
- **The fixture:** a three-round game on lottery's own test chain (`lottery/tests/common`, compiled into the tests by `#[path]`; every transaction comes from the builders and runs in the engine), written as a store. The chain's ledger plays the public node.
  - Round 1 is drawn: jackpot and lower-tier holdings, a claim, a settled deposit.
  - Round 2 is closing; round 3 is open.
- **Negative controls.** The tests were written before the server ever ran, and all passed on the first run. So three negative controls show they catch faults (each a temporary edit, restored from a copy, `cmp` equal):

  | Disabled | Result |
  |---|---|
  | the `chain_mismatch` mapping in `web::indexer` | FAILED: `wrong_tree round 1: refused with tree_mismatch …, not chain_mismatch` |
  | the winning-number comparisons (pools and proof) | FAILED: `wrong_winning round 1: the doctored response passed the page's check` |
  | the doctor's purchase edits | FAILED: `wrong_tree: rounds whose answer the doctor changed: left 0` |

Output of the doctored test (abridged):
```
mode wrong_state
  wrong_state round 1: chain_mismatch (Drawer: the state's P2SH is not the UTXO's script)
  wrong_state round 3: chain_mismatch (Shard: the state's P2SH is not the UTXO's script)
  wrong_state tickets: chain_mismatch (Ticket: the state's P2SH is not the UTXO's script)
mode wrong_tree
  wrong_tree round 1: chain_mismatch (tree_mismatch: a rebuilt shard root differs from the frozen root)
  wrong_tree round 3: chain_mismatch (tree_mismatch: shard 2's purchases do not rebuild its live root)
mode missing_purchase, extra_purchase: the same three rounds, tree_mismatch under chain_mismatch
mode wrong_winning
  wrong_winning round 1: chain_mismatch (round 1: the indexer's winning numbers are not pool tier 0's)
mode wrong_anchor_proof
  wrong_anchor_proof round 1: chain_mismatch (the anchor proof's numbers are not the pool's)
test result: ok. 3 passed; 0 failed
```
"Refused by the node" for a wrong anchor proof (spec 8): the page refuses such a proof before building. It does so above, and on the closed path through the node's anchor block (F1 re-review S1). A draw built from it anyway is refused on chain by `OpChainblockSeqCommit` (M7). F2 did not submit such a draw on simnet.

### Commands run (outputs)

- `cargo test -j 2 -p indexer-api --features doctored --test api --test doctored -- --nocapture --test-threads=1`: api 6 passed; doctored 3 passed.
- `cargo test -j 2 -p indexer-api --test production -- --nocapture`: 1 passed (168 s, with two nested builds into `target/indexer-api-prod`).
- `cargo build -j 2 -p indexer-api`, with and without `--features doctored`: no warnings. `cargo check -j 2 -p web`: no warnings.
- `cargo build -j 2 -p simnet --release`: 8 min after the lottery and web changes (RocksDB was cached). Then `target/release/simnet.exe dev --for 1500`: ready in 9 s, with the self-check above.
- `cargo build -j 2 -p indexer-api --release`, then `indexer-api --store ../tn10-data/e9f4f810… --cov e9f4f810… --bind 127.0.0.1:8792`: see above.

### Suite at the end of F2

- `cargo test -j 2 --no-fail-fast` (from `crates/`, debug; the default members `harness` and `lottery`): 55 test binaries, 1,109 passed, 0 failed, 2 ignored. This is the same as at the end of the F1 re-review: `lottery` is unchanged.
  - It includes `m7_seed`, so the SP12 seed gate holds.
  - As before (D113), `spike_vdf` overflows its stack in debug and is the one failed target.
- `cargo test -j 2 -p lottery --release --test spike_vdf`: 14 passed (200 s).
- `cargo test -j 1 --release -p tn10`: 25 passed (tn10 now compiles `web` with the new module).
- `crates/web`:
  - `api`: 17 passed, 2 ignored; `--features simnet other_network`: 1 passed;
  - W1 in wasm: 1 passed (18 + 97 + 291 lines, 14 signed and assembled);
  - W4: 1 passed. The release artifact is 6,975,707 B (was 6,372,732); the dev and mainnet builds are the controls. The quick-wallet names: release 3/4/5/8/1 (`quickWalletNew`: 3, was 4; still present, the positive control), mainnet 0; fixtures: release 0, dev 2/5/1.
- `crates/indexer-api`: api 6, doctored 3, production 1: all passed.
- `cargo check -j 2 -p simnet`: clean. The simnet release binary was rebuilt after the last change (a log-line condition in `dev.rs`).
- The `.d.ts` (`wasm-bindgen --target web`) declares `ApiRound`, `ApiObject`, `RoundStatus`, `ApiErrorCode`, `CheckedRound`, `Limits`, and the exports, for example `checkIndexerRound(input: RoundCheckInput): Outcome<CheckedRound>`.
- Size: after wasm-bindgen 6,005,095 B, 1,951,336 B brotli -q 11, against 5,423,397 B and 1,846,862 B at F1 part C (opt-level 3), so +104 KB brotli. Most of it is the bundle check (`bundle::verify` with the genesis validation) and serde_json parsing.
- Native transactions are byte-identical: `git diff` is empty on `lottery/tests/fixtures` and `web/tests/fixtures`. No covenant or template change.
- Not committed (as asked).

### Files changed

- **New:**
  - `crates/indexer-api/`: `Cargo.toml`, `src/{lib,config,store,routes,server,doctor}.rs`, `src/bin/{indexer-api,indexer-api-doctored}.rs`, `tests/{api,doctored,production}.rs`, `tests/common/mod.rs`;
  - `crates/web/src/indexer.rs`;
  - `crates/simnet/src/dev.rs`.
- **Changed:**
  - `crates/Cargo.toml` (the member) and `crates/Cargo.lock` (the package; no new crate);
  - `crates/web/src/{lib,bindings,views}.rs`: the module, 13 exports, `RoleName::of`;
  - `crates/simnet/{Cargo.toml,src/main.rs,src/sn.rs}`;
  - `CLAUDE.md` and this file.
- **Not touched:** `covenants/`, `crates/lottery/` (no change at all), `f1_digests.txt`, `w1_api.txt`, `w1_verify.txt`, tn10, `tn10-data/` (read only), and the unrelated uncommitted files.

### Proposed DECISIONS rows (for the orchestrator)

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

### Open items

- **For F3 (the one-command dev run):**
  - `simnet dev` gives the instance, the store and the API. F3's command still needs:
    - a fixed (or advertised) node wRPC port, and probably the JSON wRPC listener for kaspa-wasm (today: Borsh on a free port, written to `current.json`);
    - a faucet for the page's quick wallet: the dev run's wallet key is a throwaway, never printed. F3 can add an endpoint or a `simnet dev --fund <kaspasim: address>` flag;
    - the site's dev server, started against `current.json`;
    - the dev page built with `web --features simnet`.
  - Pace: on this PC under load, the loop mines about one block a second (one RPC template and submit per block, plus an indexer poll); `--block-ms` is the knob. Round 3's sales close about 6,000 blocks after round 2's deadline.
  - The page must read every entry from the node by outpoint or address, and X's header from the node (the F1 requirements). The `checkIndexer*` calls take them as inputs. The self-check in `dev.rs` (`node_utxos`, the anchor block) is a working example, in Rust, of what the worker will do.
  - Call `parseIndexer*` on the response text, never `JSON.parse` first.
- **For F5:**
  - `checkIndexerRound` is the base of "Verify in this browser". `statusChecked: false` (a void round, or a drawn round whose pools are all gone) must be shown as not checkable.
  - The node-built proof path is shown on simnet. On testnet-10 the server has no node, so a closed round's proof needs one of two things: the browser building it (F1 part C: this needs `anchor.rs` shared), or an indexer process that embeds the API with a node (`AnchorProvider`).
- **For F6 (hosting):**
  - The server is one binary plus the store directory the indexer writes. It needs the indexer running on the same machine (or a synced copy of its directory).
  - It speaks plain HTTP/1.1, so it needs a TLS reverse proxy in front, with `--bind 0.0.0.0:<port>` behind it and `--allow-origin <the site's origin>`.
  - Memory is about the size of the index (the M8 store: 412 lines, starts in 2 s).
  - Rate limiting per client is not implemented (only global limits); the proxy should add it.
  - A security review should look at the follower (untrusted log lines from the local indexer) and at the caps.
- The follower re-reads `index.json` whenever its time changes, to get the two status fields: 18 MB for the M8 store, every 10 s while tn10 writes it. Fine now; worth a lighter source later.
- Not done: a simnet submission of a draw with a doctored proof ("refused by the node").

## F3 part A (stage agent, 2026-10-25)

### What was built

1. **`web/`**: React 19 + TypeScript (strict, `noUncheckedIndexedAccess`) + Vite 7, Tailwind CSS 4 with shadcn/ui's pattern (Button and DropdownMenu written in `src/components/ui/` over Radix, restyled with the tokens; shadcn's CLI was not used, its model is copy-in code). ESLint 9 (typescript-eslint strict, React hooks, a rule refusing `JSON.parse` in `src/`), Prettier, Vitest (jsdom), Playwright with axe-core. Versions pinned exactly (the newest of each known-compatible major published before 11 Oct 2026, two weeks before this stage). No analytics, no third-party runtime request: the fonts are vendored, the only network peers are the configured indexer API and the node.
2. **`web/CLAUDE.md`** (conventions, layout, commands) and the root `CLAUDE.md` (layout entry, the web command line, the wasm build with `--features node`, the new `simnet dev` flags).
3. **Design tokens in one file** (`web/src/theme/tokens.css`): spec 5's table value for value in both themes, plus four role tokens the table implies (`on-teal` white text on teal, `on-mint` dark text on the mint win fill, `status-void-fg`, `focus`). Tailwind utilities map to them (`bg-ground`, `text-ink`, …) in `src/index.css`; no component holds a raw colour. Any element can carry `data-theme` (the catalogue shows both themes side by side). Theme: dark default, the device setting, a menu switch dark / light / device remembered in `localStorage` (`kl.theme`); `public/theme-boot.js` applies it before first paint (an external file, so the CSP needs no inline script). **Fonts:** Geist and Geist Mono variable woff2, vendored from the `geist` npm package 1.7.2 into `src/fonts/` with its licence (`OFL.txt`, SIL Open Font License 1.1: free to bundle and serve, the licence file travels with the fonts). **Contrast:** `tokens.test.ts` pins the table and checks 18 text/background pairs per theme at ≥ 4.5:1 (all pass, e.g. white on teal 5.3:1).
4. **The wasm in a Web Worker.** `scripts/wasm.mjs dev|release` builds `crates/web` (dev: features `simnet,node` in `crates/target/web-dev`, so the release artifact is never overwritten; release: the documented command plus `--features node`, then `wasm-opt -Oz` from the `binaryen` devDependency) and runs `wasm-bindgen --target web` into `web/src/wasm/<variant>/` (git-ignored; `@wasm/*` alias). `src/worker/lottery.worker.ts` loads it; `src/worker/protocol.ts` lists the export names the page may call (`CALLS`, checked by `satisfies (keyof typeof Wasm)[]`), and every argument and result type is `Parameters<>` / `ReturnType<>` of the generated `web.d.ts`: no hand-written mirror. `src/worker/client.ts` (`LotteryClient`): typed `call(name, ...args)`; a 16 MiB cap on arguments (`bytes_length`); a crash (a thrown exception in the worker: wasm traps on a panic) or an `error` event terminates the worker, rejects every pending call with `worker_stopped`, and the next call starts a new worker and replays the session boot (`errorTable`, `start(network)`, `nodeConnect(url)`); more than 3 deaths in a minute and the client stops trying. The panic hook (`initConsolePanicHook`) logs the Rust panic message in the console.
5. **Network layer: the rusty-kaspa v2.1.0 wRPC client compiled into `crates/web`** (new feature `node`, `src/node_client.rs`), not the kaspa-wasm npm package. Why: one rusty-kaspa version in the page (the npm package is a second build of the whole SDK, at whatever version it was published), the types stay generated from Rust, and `GetSeqCommitLaneProof` (the anchor proof) exists only in this client. New async exports, each resolving to `Outcome<T>`: `nodeConnect(url?)` (a URL, or the public resolver on testnet-10; refuses a node on another network or without a UTXO index: `node_network`), `nodeDisconnect`, `nodeInfo` (network, version, synced, virtual DAA), `nodeEntries(objects)` (the node's live entries for the indexer's objects, read by their scripts' addresses and kept for exactly their outpoints: the F1 re-review rule, entries never from the indexer), `nodeCoins(address)` (a wallet's plain coins), `nodeFeeRate` (the normal estimate rounded up, floor 100, refused above 1,000: tn10's D115 rule), `nodeAnchorBlock(hash)` (header plus `isChainBlock`, for `winningNumbers`), `nodeSubmit(signed)` (checks the stated id against the transaction). Plain-Rust half, native-tested: `src/node.rs` (URL rules: `ws://`/`wss://`, loopback only on the simnet; the network check; the rate rounding; address and outpoint selection; `errorTable`; `jackpotOdds(m)` = N × m). The node client is compiled out of a `mainnet` build, like the quick wallet. **Found and fixed on the first browser run:** workflow-core 0.18.0 (the client's timers) treats a Web Worker as a web page and replaces `window.setTimeout` on its first timer; a worker has no `window`, so it panicked (`overrides.rs:251`, `Option::unwrap()` on `None`). The client now calls `workflow_core::task::disable_persistent_timer_overrides()` before connecting (one new dependency edge in `Cargo.lock`, no new crate). After that the worker connected to the simnet node over `ws://127.0.0.1:18610` (Borsh) from Chromium and every call worked; the browser question F1 part C left open (wRPC from a page) is settled for loopback `ws://`; `wss://` through the testnet resolver from an `https` page is still to be seen in F6.
   - **Indexer client** (`src/net/indexer.ts`, `src/net/fetchText.ts`): every response is read as a stream under its kind's cap (`indexerLimits()` from the wasm; the declared `content-length` is checked first; over the cap the stream is cancelled: `bytes_length`), decoded as strict UTF-8, and parsed in the worker by the F2 `parseIndexer*` calls. Unreachable or an error status: `indexer_unavailable`, with the API's own error code in the detail.
   - **A checked round** (`src/net/round.ts`): `indexer.round(r)` → `nodeEntries(round.objects)` → `checkIndexerRound`. The round card's figures come from the checked controller view (J, T, m) and `jackpotOdds`; the clock from `nodeInfo`.
6. **The one-command dev run**, `npm run dev:simnet` in `web/` (`scripts/dev-simnet.mjs`): installs `web/` dependencies if missing (`npm ci` in `web/` only), builds the dev wasm and `simnet` (release), starts `simnet dev` with a fixed node port and the faucet, waits for that run's `crates/target/simnet-dev/current.json` (now written only after the self-check passed, with `startedMs`), then starts Vite on http://127.0.0.1:5173/ with `VITE_NETWORK=simnet`, the node, indexer, faucet URLs and the covenant ID from that file. `--skip-build` reuses the builds. Ctrl-C stops both.
   - **`simnet dev` gained (F2's three asks):** `--node-port` (default 18610: the node's Borsh wRPC on 127.0.0.1, not a rusty-kaspa default; a busy port is an error), `--faucet-bind` (default 127.0.0.1:8791), and `current.json` names `faucet` and `startedMs`. JSON wRPC was not needed: the page uses the Borsh client compiled into the wasm.
   - **The dev faucet** (`crates/simnet/src/faucet.rs`, simnet binary only): `POST /faucet?address=kaspasim:…` pays 100 KAS from the dev run's throwaway wallet (`Wallet::pay`, a plain P2PK spend with change), answers `{transactionId, amount}`; loopback bind and peers only, `kaspasim:` only (`net::parse_simnet`), one payment at a time with the spent outpoints remembered, at most 200 per run, CORS open. The page-side client (`web/src/dev/faucet.ts`) refuses anything but `kaspasim:`; it is not wired to a screen yet (part B's S7). **It never exists in a production build:** the server is in the simnet binary only; `config.ts` drops a faucet URL outside development on the simnet; `vite.config.ts` refuses to build with `VITE_FAUCET_URL` or a non-testnet network; `check-bundle.mjs` fails the build on the faucet marker or path.
   - **Deterministic fixture data:** every run is the same scenario (round 1 drawn with a held winning combination, round 2 closing, round 3 open, the same buys). Hashes, keys, D and the winning numbers differ per run (a fresh node and a random throwaway key per run, F2); the e2e tests therefore compare the page with the dev run's own API, never with fixed values.
7. **Base components** (`web/src/components/`, one file each; every interactive element has an accessible name and a `data-testid`; touch targets ≥ 44 px):

   | Component | States | Test ids |
   |---|---|---|
   | `NumberBall` | free, picked, matched (also "matched" to screen readers), full (dashed, disabled), disabled | `number-ball-<n>` |
   | `BonusTile` | free, picked (teal), matched (mint) | `bonus-tile-<n>` |
   | `RoundStatusPill` | Open, Closing, Drawn, Void (outline) | `round-status` (`data-status`) |
   | `JackpotCountdown` | live (countdown), deadline passed ("Waiting for close()" with a link to S4), stale (status and the time of the last read) | `jackpot-block`, `jackpot`, `countdown`, `deadline`, `sales-closed`, `run-round-link`, `stale` |
   | `Amount` | KAS 2 decimals (rounded half up); tap: exact sompi | `amount` or given (`data-sompi`) |
   | `Time` | clock (date when not today) + DAA score, 10 DAA/s from the node's virtual DAA at a known moment | `time` or given |
   | `HashOrAddress` | short mono, copy (Copied / Copy failed), explorer link when `VITE_EXPLORER_URL` is set | `hash`, `-copy`, `-explorer` |
   | `TxProgress` | built → signed → sent → accepted; failed with the error's message; rebuilt n/max | `tx-progress`, `tx-step-<s>`, `tx-failed`, `tx-rebuilt` |
   | `CostPanel` (shell) | empty, estimating (aria-busy), ready, fee-too-high (alert) | `cost-panel`, `cost-row-<id>`, `cost-<id>`, `cost-total`, `cost-warning` |

   Formatting only in TypeScript (`src/lib/format.ts`: BigInt printing of decimal strings, the DAA clock, short hashes); no lottery arithmetic.
8. **Component catalogue** (`#/dev/catalogue`, `src/dev/Catalogue.tsx`): every component in every state, dark and light side by side, plus the round card's loading, error and ready states. Imported only behind `import.meta.env.DEV`. Checked: the production `dist/` has no catalogue code (`check-bundle.mjs`; negative control: a `NODE_ENV=development` build emits a `Catalogue-*.js` chunk with the marker).
9. **App shell**: header (name, network badge on every page, Connect → Wallet, menu), bottom navigation Play · My tickets · Rounds (`aria-current`), the menu (Radix, non-modal: Vault, How it works, Backup, Wallet, Theme dark / light / device, Copy referral link, disabled until a wallet is connected), hash routes, placeholder screens (title only) for My tickets, Rounds, Vault, How it works, Backup, Wallet. **Play** shows the live round card from the simnet instance: round, status pill, J, the countdown to T with T as clock and DAA, the odds "1 in 285,012" (m = 2, from the wasm), and a link to the round's page; loading, error (message + Retry), stale (after 30 s without a good read), refresh every 10 s.
10. **Production build** (`npm run build`): the CSP meta tag (`default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' data:; connect-src 'self' <indexer> https: wss:; base-uri 'none'; form-action 'none'`), testnet-10 only, then the bundle check. Sizes (release, `--features node`, opt-level 3 + `wasm-opt -Oz`): **6,363,882 B raw, 2,791,416 B gzip -9, 2,273,334 B brotli -q 11** (F1's 1.69 MB brotli had no node client and opt-level "s"); JS 362 KB (116 KB gzip), worker 47 KB, CSS 16 KB, fonts 141 KB. Dev package 7,959,390 B (no wasm-opt).
11. **Rust changes** (no covenant, template or golden change):
    - `lottery::web::Code`: appended 35 `node_unavailable`, 36 `node_network`, 37 `node_refused`, 38 `indexer_unavailable` and 39 `worker_stopped` (the last two raised by the page; one table, one message each); `f1_web::error_codes_are_stable` extended.
    - `crates/web`: `src/node.rs`, `src/node_client.rs`, `errorTable` and `jackpotOdds` exports, the `node` feature (with `workflow-core` 0.18.0, already in the lock), `bindings::{net, out, parse}` now `pub(crate)`.
    - W4 (`web/tests/w4_mainnet.rs`) builds what the page ships: release `--features node` (the artifact `scripts/wasm.mjs release` packages), dev `simnet,fixtures,node`, mainnet `mainnet,node`.
    - `crates/simnet`: `faucet.rs`, `Wallet::pay`, `Node::start_on(port)`, `dev` flags and `current.json` fields.

### Tool versions

Node v24.14.0, npm 11.2.0 (both already installed). npm packages (exact): react / react-dom 19.3.0, @types/react / @types/react-dom 19.3.0, vite 7.3.7, @vitejs/plugin-react 5.2.0, typescript 5.9.3, tailwindcss and @tailwindcss/vite 4.3.3, @radix-ui/react-dropdown-menu 2.1.25, @radix-ui/react-slot 1.4.0, class-variance-authority 0.7.1, clsx 2.1.1, tailwind-merge 3.7.0, eslint and @eslint/js 9.39.5 (npm marks eslint 9 as out of support; 10 is out, the plugins' support for it was not checked), typescript-eslint 8.71.1, eslint-plugin-react-hooks 5.2.0, eslint-plugin-react-refresh 0.5.7, globals 16.5.0, prettier 3.9.10, vitest 3.2.7, jsdom 26.1.0, @testing-library/react 16.3.3, @testing-library/dom 10.4.2, @playwright/test 1.64.0 (playwright-core pinned to 1.64.0 by an `overrides` entry, so @axe-core/playwright uses the same copy), @axe-core/playwright 4.13.0, geist 1.7.2, binaryen 131.0.0, @types/node 22.20.5. Playwright Chromium (build 1248) in `web/.playwright-browsers`. Rust: wasm-bindgen-cli 0.2.100 (installed in F1).

### npm safety (critical rule)

`web/package.json` was written by hand before any npm command; every npm/npx command ran with `web/` as the working directory (`npm prefix` printed `C:\Users\endri\Documents\Kaspalionare\web` before the first install). The scripts run npm only through `lib.mjs` `npm()`, which refuses without `web/package.json` and always uses `cwd: web/`. Nothing was installed globally or into the home folder; Playwright's browsers are inside `web/`.

### Commands run (outputs)

- `npm test` (in `web/`): `tsc -b` clean; `eslint .` clean; `prettier --check .` "All matched files use Prettier code style!"; Vitest `Test Files 9 passed (9)`, `Tests 70 passed (70)` (tokens 38, format 7, client 5, components 6, theme 3, config 3, fetchText 3, faucet 3, routes 2).
- `npm run e2e` against `npm run dev:simnet` (Chromium, 390 × 844):
  ```
  ✓ the round card shows the open round from the simnet, checked against the node
  ✓ the device setting picks the theme, both drawn from the tokens
  ✓ the menu switch overrides the device setting and is remembered
  ✓ the shell: bottom navigation, menu screens and the dev catalogue
  4 passed (8.7s)
  ```
  axe-core (WCAG 2.0/2.1 A and AA tags) reported **no violation of any impact** on Play (dark and light), with the menu open, and on the catalogue. The round card's round, status, J (sompi) and odds equal the dev run's own `/v1/status` and `/v1/rounds`; every interactive element has a `data-testid` and a name. Before the two fixes above, the first run failed at the round card (`worker_stopped`: the workflow-core panic) and at axe `aria-hidden-focus` with the menu open (Radix's modal menu hides `#root`; the menu is now non-modal).
- The dev run: `npm run dev:simnet -- --skip-build`: `simnet dev` ready in 10 s with every `SELF-CHECK` ok (round 3 open, 2 closing with provisional numbers, 1 drawn with final numbers), then `dev:simnet ready: http://127.0.0.1:5173/ (indexer http://127.0.0.1:8790/v1/, node ws://127.0.0.1:18610, faucet http://127.0.0.1:8791/faucet, instance 348a4713…2b5b)`. Faucet by hand: `POST …/faucet?address=kaspasim:qz54…xfvq` → `{"amount":"10000000000","transactionId":"4cc047c1…d775"}`; a `kaspatest:` address → 400 with the address error; `GET /faucet` → 404.
- `npm run build` (placeholder `VITE_INDEXER_URL`, `VITE_COVENANT_ID`): release wasm, `tsc -b`, `vite build` "built in 2.12s", `check-bundle: no catalogue, faucet or simnet configuration in the production bundle`.
- Rust: see "Suite at the end of F3 part A".

### Screenshots

`web/test-results/shots/` (git-ignored, rewritten by each e2e run): `play-dark.png`, `play-light.png`, `catalogue.png`. Checked by eye against the mock-up (BuyDark / Buy boards): header, badge, card, pill, mono figures, amber countdown, bottom nav match its colours, type and layout. The mock's explanatory lines are left out (rule 6).

### Proposed DECISIONS rows (for the orchestrator)

- (a) **The page's network layer is the rusty-kaspa v2.1.0 wRPC client compiled into `crates/web`** (feature `node`, Borsh), not the kaspa-wasm npm package: one rusty-kaspa version, generated types, the anchor-proof RPC reachable. Every entry the page passes to the lottery code comes from `nodeEntries`/`nodeCoins` (node, by outpoint), every X header from `nodeAnchorBlock`.
- (b) **workflow-core's window timer overrides are switched off in the worker** (`disable_persistent_timer_overrides`): a worker has no `window`; its timers are not throttled like a background tab's.
- (c) **Five error codes appended** (35–39): node unavailable, node on another network, node refused, indexer unavailable, worker stopped; the page loads the one table at boot (`errorTable`).
- (d) **The worker is replaced after any crash**, the session boot replayed, pending calls rejected with `worker_stopped`; at most 3 deaths a minute. Arguments to the worker are capped at 16 MiB; indexer bodies at the kinds' caps before parsing.
- (e) **`simnet dev` for the page:** node Borsh wRPC on 127.0.0.1:18610, the dev faucet (simnet binary only, 100 KAS a request, kaspasim: only, loopback), `current.json` written after the self-check with `startedMs`; `npm run dev:simnet` is the one command.
- (f) **Stack versions** as listed (exact pins, newest per known-compatible major at least two weeks old); shadcn/ui components written by hand over Radix (no CLI).
- (g) **Tokens:** spec 5's table plus `on-teal`, `on-mint`, `status-void-fg`, `focus`; contrast pinned by a unit test.
- (h) **W4 checks the shipped builds** (`node` in release, dev and mainnet); the node client is compiled out of a `mainnet` build.
- (i) **The dev catalogue and faucet stay out of production**, checked by `check-bundle.mjs` (with a negative control).

### What part B must do

- **Wallet adapters** (spec 6.2, S7): interface `connect`, `network`, `address`, `utxos`, `signInputs(tx, indices)`.
  - KasWare: `window.kasware.signPskt({txJsonString: built.transaction, options: {signInputs: built.sign.map(({index, sighashType}) => ({index, sighashType}))}})` (F0: always the index list; never `pushPskt`), network `kaspa_testnet_10` (on the simnet the quick wallet only).
  - Quick wallet: `quickWalletNew` / `quickWalletImport` / `quickWalletSign` through `LotteryClient`; key in IndexedDB (`webstore/1` is F4's; the key store is part B's); the S7 line "testnet key stored in this browser; anyone with this device can spend it" is spec text, keep it; on the simnet dev run, a faucet button that calls `requestFaucet(config.faucetUrl, address)` (import it behind `import.meta.env.DEV && config.faucetUrl`, so it stays out of production; `check-bundle.mjs` enforces it).
  - Coins: `nodeCoins(address)`; fee rate: `nodeFeeRate()`; change: the wallet's address (one payer per build, `mixed_signers`).
- **Post-sign attack tests** (spec 8): the check is `assemble({built, signed})` in the wasm. Vitest can load the wasm in Node (`initSync` with the bytes of `src/wasm/dev/web_bg.wasm`) for a mock-wallet suite; spec 8's list (covenant sigscript, output value, output script, extra output, change address, unlisted input's signature, lock time, payload) each refused before `nodeSubmit`.
- **S1 Play**: number picker (5 of 1..30, bonus 1..m from the checked shard state), quick pick, clear, ticket builder (1..B_MAX from `checkIndexerInstance`), the cost panel from `BuiltTx.summary`/`fee` (rows only: price, deposit with its return date as a value, fee, referral with its destination, total), Buy → `buildBuy` (shard from the checked round, purchases from `indexer.purchases(r)` checked by `checkIndexerRound`), sign, `assemble`, `nodeSubmit`, `TxProgress`; shard contention: rebuild on another shard, up to 3 tries; after T: "Sales closed; waiting for close()" (the component exists).
- **Referral**: `?ref=` parsed with `scriptOf` (refuses non-`kaspatest:`), stored per browser; default `VITE_REFERRAL_ADDRESS`. The menu's "Copy referral link" needs the connected address (`Shell` prop `referralAddress`).
- **End to end on simnet**: a buy with the quick wallet funded by the faucet (b = 1 and 4, with and without a referral link).
- Keep: `npm test`, `npm run e2e`, `npm run build` green; extend `namedAndTagged` and axe to the new screens.

### Open questions and owner decisions

1. **"No preference" shows light in Chrome: needs a decision.**
   - What happens: the spec says dark when the device has no colour setting. Chrome (and the Playwright check) reports "light" when the device has none, so the page cannot tell "no setting" from "light" and shows light there. Where a browser does report no preference, the page shows dark.
   - Trust: none. Money: none.
   - Work: none to keep; small to change: start dark until the person picks a theme in the menu (the device setting would then never switch to light by itself, which drops "follows the device").
   - Recommendation: keep following the device (the setting people chose wins), with dark as the fallback.
2. **CSP `connect-src` allows any `https:`/`wss:` host** (the resolver's node hosts are not known in advance). F6 should narrow it to the resolver's domains and the indexer once the beta's hosts are fixed. Not an owner decision unless F6 cannot narrow it.
3. **Bundle size**: 2.27 MB brotli with the node client. F6 can try opt-level "s" for the release wasm (F1 measured about −10%); that changes the artifact W4 checks, so it needs its own measurement.
4. **The dev run's children outlive a killed parent on Windows** when the parent is terminated (as a background task stop does); Ctrl-C in a console stops all. If needed: stop `simnet.exe` and the Vite `node.exe` by the ports 8790/8791/18610/5173.
5. Not done in part A: the time to build a buy in a browser (part B, with the first browser buy); `wss://` through the resolver from an `https` page (F6); a dev-only "kill the worker" hook to exercise the restart end to end (the restart is unit-tested with a fake worker, and the real workflow-core panic exercised it once by accident: the card showed `worker_stopped` and retried).

### Files changed (F3 part A)

- New: `web/` (everything above; `web/src/wasm/`, `web/node_modules/`, `web/dist/`, Playwright output git-ignored), `crates/web/src/node.rs`, `crates/web/src/node_client.rs`, `crates/simnet/src/faucet.rs`.
- Changed: `crates/lottery/src/web/mod.rs` (codes 35–39), `crates/lottery/tests/f1_web.rs`, `crates/web/{Cargo.toml, src/lib.rs, src/bindings.rs, tests/w4_mainnet.rs}`, `crates/simnet/src/{dev.rs, main.rs, node.rs, wallet.rs}`, `crates/Cargo.lock` (one edge: `web` → `workflow-core`), `.gitignore`, `CLAUDE.md`, this file.
- Not touched: `covenants/`, `crates/lottery/templates/`, the golden files (`f1_digests.txt`, `w1_api.txt`, `w1_verify.txt`), and the unrelated uncommitted files.

### Suite at the end of F3 part A

- `cargo test -j 2 --no-fail-fast` (from `crates/`, debug; default members `harness` and `lottery`): 55 test binaries, 1,109 passed, 0 failed, 2 ignored, the same as at the end of F2; `f1_web::error_codes_are_stable` now pins 39 codes. As before (D113), `spike_vdf` overflows its stack in debug and is the one failed target.
- `crates/web`: `cargo test -j 2 -p web --release --test api --lib`: api 17 passed, 2 ignored; lib 4 passed (`node::tests`); `cargo test -j 2 -p web --features simnet --lib`: 5 passed (with the simnet loopback rule). W1 in wasm: 1 passed ("18 builds … 97 digest lines … 291 decode and verify calls … 14 transactions signed … refused for each"). W4: 1 passed, now on the shipped builds: release `--features node` 9,248,676 B (quick wallet present, no fixtures, refuses the simnet), dev `simnet,fixtures,node` (positive control, 18 fixture calls), mainnet `mainnet,node` 1,573,422 B (no quick-wallet name, refuses to load: `mainnet_refused`). Before the node client was compiled out of `mainnet`, that build was 9,228,897 B and a 32-byte run of `0x21` inside the client tripped the fixture-key scan 557 times; now 0.
- `cargo test -j 2 -p indexer-api --features doctored --test api --test doctored`: 6 and 3 passed. `cargo test -j 1 --release -p tn10`: 25 passed. `cargo test -j 2 -p simnet --release`: 9 passed. `cargo check -j 2 -p web --features node` (native) and `-p simnet`: clean.
- Golden files unchanged (`git diff` empty on `crates/lottery/tests/fixtures`, `crates/web/tests/fixtures`, `covenants/`, `crates/lottery/templates/`): native transactions byte-identical. No covenant change.
- `web/`: `npm test` green (70 unit tests), `npm run e2e` 4 passed against the dev run, `npm run build` green with the bundle check.
- Not committed (as asked).

## F3 part B (stage agent, 2026-10-26)

### What was built

1. **Wallet adapters behind one interface** (`web/src/wallet/types.ts`, spec 6.2: `connect`, `network`, `address`, `utxos`, `signInputs(built, indices)`).
   - **Common rules:** `utxos()` is always `nodeCoins(address)` from the node (never the wallet's word, never the indexer); `signInputs` takes exactly the built transaction's `sign` items (`signItems` refuses any other list, empty included); one wallet request at a time (`wallet/serial.ts`, a promise queue: KasWare answers an overlapping request with `4001 please request after current approval resolve`); the session (`wallet/session.tsx`) checks every address a wallet reports with the wasm (`scriptOf`: an address of the page's network, else `wallet_network`).
   - **KasWare** (`wallet/kasware.ts`): `signPskt({txJsonString: built.transaction, options: {signInputs: [{index, sighashType}]}})`, always with the explicit list (F0); never `pushPskt`; the network must be `kaspa_testnet_10` at connect and again before each signature; on the simnet the extension wallets are refused (`walletNetworkFor` gives none: the quick wallet only).
   - **Kaspire** (`wallet/kaspire.ts`): `signPskt({sender, txJsonString, options: {signInputs}})`, network `testnet-10`, same rules. **UNTESTED** (F0 could not run it; marked in the file). WalletConnect `kaspa_signPskt` (Android) is not built.
   - **Quick testnet wallet** (`wallet/quick.ts`): the key from `quickWalletNew`/`quickWalletImport` (the wasm), kept in IndexedDB database `kaspalionare-wallet` (store `keys`, key `quick`, record `StoredQuickKey` with its network), a separate database from the local store so a backup never carries a key; export shows the 64-hex secret on request (Export key, Hide); signing through `quickWalletSign`, then the same `assemble` as any wallet. It reconnects by itself when this browser used it last (`kl.wallet`). The S7 line "Testnet key stored in this browser; anyone with this device can spend it" is permanent; the faucet link is `VITE_FAUCET_LINK` (https only; unset = no link, see open questions); on the simnet the dev faucet button (`dev/DevFaucet.tsx`, DEV only).
   - **Absent from a mainnet page build:** the quick wallet's page code is loaded only behind `QUICK_WALLET` (`wallet/quickEnabled.ts`, `VITE_MAINNET_CHECK`), including its three call names in `CALLS`. `npm run check:mainnet` makes a TEST-ONLY build with the flag and scans the page's JS (the worker chunk is the wasm glue, which W4 covers): 0 finds of the marker, the S7 line and the three call names; the normal build is the positive control (1 each).
2. **The purchase path** (`web/src/play/buyFlow.ts`): `loadSales` (the current round's shard objects from the indexer, each read from the node by outpoint with `nodeEntries`; spent ones the indexer still lists are dropped; the rest checked with `checkIndexerObjects`; m and T from the shard state; open = status open and the node's virtual DAA below T), `prepareBuy` (a live shard, its purchases from the API, the wallet's node coins (largest first, at most 8), `nodeFeeRate`, `buildBuy`, then the cost panel's figures from the new Rust export `buyFigures`), `signAndSubmit` (wallet signs the listed inputs, `assemble` = the post-sign check, only then `nodeSubmit`; the answered id must be the built one), `buy` (the rebuild loop: on `node_conflict` exclude the shard, rebuild on another, up to 3, each shown as "Rebuilt n/3"; a stale indexer copy after a conflict (`chain_mismatch`/`tree_mismatch`) is waited out), `waitAccepted` ("accepted" = the ticket output is in the node's UTXO set, its entry checked with `verifyState` against the ticket's state bytes), then the ticket into the local store and the round's copy in the background.
3. **Rust additions** (`crates/web`; no covenant, template or golden change):
   - `buyFigures({covenantId, shard, bundle, payout, referrer?, built})` → `BuyFigures`: price b × p, deposit b × d, `depositBack` = T + D_RET (DAA; from then on `return_deposit()` is valid on the ticket alone, `ticket.sil` A5b.3), the referral `{amount r, paid, address}` (paid only if a referrer and r ≥ M_REF; else no output, the share stays in the round), fee, total, and the ticket (output 1: full state bytes, script, address). Every figure is checked against the transaction (`unsigned_of` first; input 0 is the given shard's outpoint and entry; output 0 is the successor's value; output 1 is this bundle's and payout's ticket holding b × d; the referral output exactly when paid; every other plain output back to the wallet; total = wallet in − wallet back = price + deposit + fee); anything else is `built_mismatch`.
   - `nodeBalance(address)` (the node's plain coins' total, `node::coins_total`), `webstoreInfo()`.
   - `node::submit_refusal`: a submit refused because an input is already spent is the new code 40 `node_conflict`. The texts are rusty-kaspa v2.1.0 `mining/errors/src/mempool.rs` (line 33 `RejectDoubleSpendInMempool`, 77 `RejectDisallowedOrphan` (the page submits with `allow_orphan = false`, so a spent input reads as an orphan), 18 `RejectMissingOutpoint`), read from the cargo checkout of `01b532e` (`refs/` is empty on this machine).
   - Codes appended (one table, `f1_web::error_codes_are_stable` pins 44): 40 `node_conflict`, 41 `wallet_missing`, 42 `wallet_rejected`, 43 `wallet_network`, 44 `storage_unavailable`.
   - `crates/web/src/webstore.rs`: the `webstore/1` records (below).
4. **Change dust, owner decision (DECISIONS 2026-10-26 (2))** (`lottery::web::build::with_fee`/`diagnose`): when the build without a change output pays exactly the fee its own mass needs (the leftover is exactly zero), that build is the result (no change output, no extra fee); any other sub-dust leftover is still `change_dust`. Test first: `f1_web::small_change_and_short_draws_have_their_codes` now requires extra 0 → built with 2 outputs and fee = need0 (failed before the fix: `change_dust (33): … 0 above what its mass needs`); extra 1, 10³, 10⁵, 10⁶, 10⁷ sompi stay `change_dust`. Native goldens unchanged (`git diff` empty on `crates/lottery/tests/fixtures`, `crates/web/tests/fixtures`); no golden case had an exactly-zero leftover.
5. **Local store `webstore/1`** (below) with the ticket after each accepted purchase and the round's copy (`store/roundCopy.ts`: `indexer.round` + `indexer.purchases` → `nodeEntries` → `checkIndexerRound` with the purchases; while open, each list must rebuild its live shard's root; retried every 3 s up to 20 times while the API catches up).
6. **S1 Play** (`screens/Play.tsx`, `screens/play/`): the round card (part A), the number picker (`NumberPicker.tsx`: 30 balls in a 6-column grid, the bonus tiles 1..m from the live shards, Quick pick (the browser's crypto), Clear, Add), the ticket builder (`components/TicketRow.tsx`: the combination, remove; 1..B_MAX from `checkIndexerInstance`, duplicates allowed), the cost panel (rows Price, Deposit with "back <clock, DAA>", Network fee, Referral with its destination address or "Stays in round", Total; all from `buyFigures`; a build error shows its one message), Buy (or Connect without a wallet), the progress (built → signed → sent → accepted, the reason on failure, "Rebuilt n/3", the transaction id). After T: "Sales closed; waiting for close()" with a link to Rounds. No TypeScript lottery arithmetic: the only number work in TS is sorting the wallet's coins by amount and comparing the node's DAA with T.
7. **Referral** (`referral/referral.ts`, spec 7, D134): `?ref=` (from `location.search`, before the hash route) is validated by the wasm (`scriptOf`: an address of the started network; `kaspa:`, `kaspatest:` on the simnet, HTML and over-long values are refused; nothing is inserted as HTML) and stored in `localStorage` `kl.ref`, replaced only by another valid link; a refused link leaves the stored one and is logged (`console.info`), not shown. Default: `VITE_REFERRAL_ADDRESS` (spec 6.5; the simnet dev run passes `current.json`'s `referralAddress`, a fresh address whose key is dropped). The menu's "Copy referral link" (J7) copies `<page>?ref=<connected address>`.
8. **S7 Wallet sheet** (`screens/Wallet.tsx`, `screens/wallet/QuickWalletPanel.tsx`): status (Disconnected, Connecting, Wrong network, Connected, Quick wallet), network, address (with copy and explorer link), balance (`nodeBalance`, every 10 s), errors by code, KasWare, Kaspire, Disconnect, and the quick wallet's create / use / export / import. The header's Connect button shows `…<last 6>` of the connected address.
9. **Owner decision, theme (2026-10-26, amends D139):** a first-time visitor sees dark whatever the device says (`readChoice` defaults to `dark`; `public/theme-boot.js` the same); the menu's dark / light / device stays, device follows `prefers-color-scheme`, the choice is remembered (`kl.theme`). Unit tests and the Playwright checks updated (emulated light with nothing stored → dark; light chosen → light after reload; device → follows an emulated change; dark again → remembered).
10. **simnet dev** (dev tool only): s_R = 500 (5 %) in every round (`dev::DEV_S_R`), so at p = 1 KAS one combination's referral (0.05 KAS) is below M_REF and four's (0.2 KAS) is paid: both spec 8 cases on one instance; `current.json` gains `referralAddress`; the dev faucet now combines the wallet's coins (largest first, up to 40 inputs) when no single coin covers 101 KAS (the first full e2e run emptied its one large coin after 9 payments while the mined coinbase coins sat unused).
11. **DEV-only e2e hooks** (`dev/hooks.ts`, `window.__klDev`): hold a signed, checked purchase before it is sent (`holdBeforeSubmit`, `release()`), force the shard (`forceShard`), report the last one (`lastShard`). Imported behind `import.meta.env.DEV`; `check-bundle.mjs` now also refuses `KASPALIONARE_DEV_HOOKS`, `__klDev` and `Dev faucet` in `dist/` (0 finds).
12. **Worker handler split** (`worker/handler.ts`): the worker's request handling is one function shared by `lottery.worker.ts` and the in-process test worker (`src/test/wasmWorker.ts`), so the unit tests run the page's own message path on the real dev wasm (`initSync` with `src/wasm/dev/web_bg.wasm`).

### Adapter design in one picture

```
Play / Wallet sheet ──► WalletSession (wallet/session.tsx): state, address (checked by scriptOf), adapter
                            │
          ┌─────────────────┼──────────────────────┐
     KasWareAdapter    KaspireAdapter (untested)  QuickWalletAdapter (lazy; key DB kaspalionare-wallet)
          │  Serial queue, explicit signInputs list, network check        │ quickWalletSign (wasm)
          └──────────────► signed Safe JSON ◄─────────────────────────────┘
                                 │
     signAndSubmit: assemble({built, signed}) in the worker (post-sign check) ─► nodeSubmit (node) ─► id = built id
```

### Post-sign attack tests (spec 8; `web/src/wallet/postsign.test.ts`, through `signAndSubmit` + `LotteryClient` + the worker handler + the real wasm)

The builds: `buy b=1 ref` and `close k=16` from `web/src/test/fixtures/page-inputs.json` (the crates/web fixture inputs as the page's JSON, written by `cargo test -p web --release --test api -- --ignored write_page_fixtures` and checked current by `page_fixtures_are_current`; no key in it, the test asserts the fixture wallet's key is absent), with a fresh quick-wallet key's script put in place of the fixture coins' and its address as change and payout.

| Mock wallet alters | Shape | Result | Submit called |
|---|---|---|---|
| (honest) | buy, close | sent once, steps signed → sent, id = built id | 1 |
| a covenant signature script | buy | `signed_changed` | 0 |
| an output value | buy | `signed_changed` | 0 |
| an output script (the referral's) | buy | `signed_changed` | 0 |
| an extra output | buy | `signed_changed` | 0 |
| the change address | buy | `signed_changed` | 0 |
| an unlisted input's signature (the listed input's copied into covenant input 5) | close K = 16 | `signed_changed` | 0 |
| the lock time | buy | `signed_changed` | 0 |
| the payload | buy | `signed_changed` | 0 |
| a listed input left unsigned | buy | `signed_missing` | 0 |
| a signature byte flipped | buy | `signature_invalid` | 0 |
| our own BuiltTx with its fee changed (a tampered figure) | buy | `built_mismatch` | 0 |

KasWare adapter (a stand-in `window.kasware` that signs with the quick wallet's key and refuses overlapping requests with 4001): the explicit list `[{index: 18, sighashType: 1}]` for the close, and its answer passes `assemble`; `kaspa_mainnet`, `kaspa_testnet_11`, `kaspa_simnet` and the simnet page are `wallet_network`; two concurrent signatures both succeed (serialised); a list that is not the built one's (`[0]`, `[]`) is refused.

**Negative control:** with `assemble` bypassed in `signAndSubmit` (the wallet's JSON sent as it came; a temporary edit, restored, `cmp` equal) the ten alteration tests failed (the submit was called), the honest one passed.

### The `webstore/1` format (`crates/web/src/webstore.rs`; the page reads names from `webstoreInfo()`)

- Database `kaspalionare`, version 1, stores `meta`, `tickets`, `rounds`.
  - `meta`, key `"format"`: `StoreMeta {format: "webstore/1", createdMs}`. A store of another format is refused (`store_format`).
  - `tickets`, key `"<txid>:<index>"`: `StoredTicket {key, covenantId, network, round, shardIndex, ticket: CovUtxo {utxo: the node's entry, state: the full state bytes (R1)}, bundle, payout, buy: {transactionId, price, deposit, depositBack, fee, referral}, savedMs}`.
  - `rounds`, key `"<covenantId>:<round>"`: `StoredRound {key, covenantId, network, round, m, k, stage: open | closed | drawn, purchases: [16 hex lists, shard order], roots?, checkedMs}`. F3 writes `open` copies; F4 moves them to `closed` (checked against the marker's frozen roots) and `drawn` (with the result, a field F4 adds with a default).
- Every record names its instance and network, so one browser holds several instances' copies (a fresh simnet run per dev session, the beta) and a page reads its own only.
- Separate database `kaspalionare-wallet` (store `keys`, key `quick`): `StoredQuickKey {network, key: QuickKey, createdMs}`; never in `webstore/1` or its backup; absent from a `mainnet` wasm build.
- Versioning as `wallet/1`: a reader refuses an unknown `format`; fields are only added with defaults within a format; a new format is `webstore/2` with a database upgrade.
- F4: the backup file and its reader should be Rust (`deny_unknown_fields`, size caps), like `read_wallet`, so a corrupted backup is refused in one place.

### End to end on the private simnet (Playwright, quick wallet; `web/e2e/play.spec.ts`, `web/e2e/smoke.spec.ts`, helpers in `web/e2e/helpers.ts`)

Final run (`npm run e2e` against `npm run dev:simnet -- --skip-build`, instance `a4402deb…a9e0`): **10 passed (1.9 m)**, axe-core reported no violation of any impact on any page checked.

| Test | What it checks | Result |
|---|---|---|
| quick wallet | create, dev faucet 100 KAS (balance from the node), the S7 line, network, header `…<last 6>`, J7 referral link in the clipboard (`?ref=<address>`), export (64 hex), import in another browser → same address; 360 and 430 px: no horizontal scroll, every control ≥ 44 × 44; data-testid + name; axe dark and light | ✓ |
| b = 1, no link | price 1.00 KAS, referral 0.05 KAS "Stays in round", deposit "back <clock> DAA …"; 360/430 px; axe dark, light, and after acceptance; accepted; the API's `bought` row has 1 combination and referral 0; the local store holds the ticket (state bytes, the node's entry) and the round's 16 lists (`open`), format `webstore/1` | ✓ |
| b = 4, no link | referral 0.20 KAS to the site owner (`current.json` `referralAddress`); the API's row: 4 combinations, referral 20,000,000; local store | ✓ |
| with a link | `?ref=<another quick wallet>` stored in `kl.ref`; b = 4 paid to it (API 20,000,000); then without the parameter, still remembered; b = 1 below M_ref, referral 0 | ✓ |
| refused links | malformed `kaspa:`/`kaspatest:` stand-ins (built in the test, no real foreign address; a project hook blocks real `kaspa:` addresses) and an HTML string: nothing stored; the referral stays the site owner's | ✓ |
| shard conflict | A signs and holds (DEV hook) on shard s; B buys on s first (forced) and is accepted; A sends: the node refuses (spent input → `node_conflict`), the page shows "Rebuilt 1/3", rebuilds on another shard and is accepted; both rows in the API | ✓ |
| smoke ×4 | part A's round card; first visit dark under both device settings; the menu switch and device mode; shell, menu screens, catalogue (now with the ticket row) | ✓ |

Indicative timing (spec 8 quality, desktop Chromium on this PC, not a phone): ticket to cost panel ready 286 ms (b = 1) and 304 ms (b = 4), including the 250 ms debounce, the indexer and node reads, `buildBuy` and `buyFigures`.

Screenshots in `web/test-results/shots/` (git-ignored): `wallet-dark.png`, `play-ticket-dark.png`, `play-ticket-light.png`, `play-accepted.png`, `play-rebuilt.png`, plus part A's.

### Test commands and output tails

- `npm test` (in `web/`): `tsc -b` clean, `eslint .` clean (0 warnings), Prettier "All matched files use Prettier code style!", Vitest **Test Files 14 passed (14), Tests 100 passed (100)** (new: postsign 16, referral 3, webstore 2, picker 3, BuyCard 4; theme 4, config 4).
- `npm run build`: release wasm (6,380,405 B; gzip -9 2,792,944 B), `vite build` "built in 2.45s", `check-bundle: no catalogue, faucet or simnet configuration in the production bundle`; page JS: index 396,366 B (125,855 gzip), worker 47,699 B, lazy `QuickWalletPanel` 2,273 B and `quick` 1,491 B.
- `npm run check:mainnet`: each of the 5 markers `normal build 1  mainnet check 0`; "the mainnet check build has no quick wallet page code".
- `npm run e2e`: 10 passed (above).
- `cargo test -j 2 --no-fail-fast` (from `crates/`, debug, default members): 55 test binaries, 1,108 passed, 1 failed, 2 ignored on the first run: the failure was my own code-table test string (a broken line continuation), fixed; `cargo test -j 2 -p lottery --test f1_web` then **7 passed**. As before (D113), `spike_vdf` overflows its stack in debug.
- `cargo test -j 2 -p lottery --release --test f1_web -- --nocapture`: 7 passed; `coin = cost + need0 (29365900) + 0: ok, fee 29365900, 2 outputs`, the other five `change_dust`.
- `cargo test -j 2 -p web --release --test api --lib`: lib 7 passed (new `node::tests::submit_conflicts_are_classified`, `coin_totals`, `webstore::tests::info_names_the_format`), api **19 passed, 3 ignored** (new `buy_figures_are_bound_to_the_transaction`, `page_fixtures_are_current`; ignored `write_golden`, `write_page_fixtures`, the P5 test). `--features simnet --lib`: 8 passed.
- Negative control: with the ticket-output check in `buy_figures` disabled, `buy_figures_are_bound_to_the_transaction` FAILED at "another bundle" (accepted); restored, `cmp` equal.
- W1: `cargo test -j 2 -p web --target wasm32-unknown-unknown --release --test w1_wasm -- --nocapture` (with the CC/AR variables): "18 builds … 97 digest lines … 291 decode and verify calls … 14 transactions signed …", 1 passed.
- W4: 1 passed (149 s): release `--features node` 9,289,189 B, quick-wallet names 3/4/5/8/1, mainnet 1,584,815 B with 0 and refuses to load; fixtures release 0, dev 2/5/1.
- `cargo test -j 1 --release -p tn10`: 25 passed. `cargo test -j 2 -p simnet --release`: 9 passed. `cargo test -j 2 -p indexer-api --features doctored --test api --test doctored`: 6 and 3 passed.
- Goldens: `git diff` empty on `crates/lottery/tests/fixtures`, `crates/web/tests/fixtures`, `covenants/`, `crates/lottery/templates/`, `crates/Cargo.lock`. Native transactions byte-identical; no covenant change.

### Files changed (F3 part B)

- Rust: `crates/lottery/src/web/{build.rs (exact-zero change), mod.rs (codes 40-44)}`, `crates/lottery/tests/f1_web.rs`; `crates/web/src/{api.rs (buy_figures), bindings.rs (buyFigures, webstoreInfo), dto.rs (BuyFiguresInput, BuyFigures, ReferralFigure, TicketOut), lib.rs, node.rs (submit_refusal, coins_total), node_client.rs (nodeBalance, conflicts), webstore.rs (new)}`, `crates/web/tests/api.rs`; `crates/simnet/src/{dev.rs (s_R 500, referralAddress), faucet.rs (combined coins)}`.
- Page, new: `web/src/wallet/{types,serial,kasware,kaspire,quick,quickEnabled,context}.ts`, `session.tsx`, `postsign.test.ts`; `web/src/play/{buyFlow,picker}.ts`, `picker.test.ts`; `web/src/store/{kv,webstore,roundCopy,session}.ts`, `webstore.test.ts`; `web/src/referral/referral.ts`, `referral.test.ts`; `web/src/screens/{Wallet.tsx, play/{NumberPicker,BuyCard}.tsx, play/usePlay.ts, play/BuyCard.test.tsx, wallet/QuickWalletPanel.tsx}`; `web/src/components/TicketRow.tsx`; `web/src/dev/{hooks.ts,DevFaucet.tsx}`; `web/src/worker/handler.ts`; `web/src/test/{wasmWorker.ts, fixtures/page-inputs.json}`; `web/e2e/{helpers.ts,play.spec.ts}`; `web/scripts/check-mainnet.mjs`.
- Page, changed: `web/src/{app/App.tsx, app/Shell.tsx, components/{Amount,CostPanel}.tsx, config.ts, config.test.ts, dev/Catalogue.tsx, screens/Play.tsx, theme/{theme.ts,theme.test.ts}, vite-env.d.ts, worker/{lottery.worker.ts,protocol.ts}}`, `web/public/theme-boot.js`, `web/e2e/smoke.spec.ts`, `web/scripts/{check-bundle,dev-simnet}.mjs`, `web/{package.json (check:mainnet), tsconfig.app.json (resolveJsonModule), eslint.config.js (tests may JSON.parse), .gitignore, .prettierignore, CLAUDE.md}`; root `CLAUDE.md` (web command line).
- No new npm package, no new crate; `package-lock.json` and `Cargo.lock` unchanged. Not touched: the unrelated uncommitted files.

### Proposed DECISIONS rows (for the orchestrator)

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

### What is left (for F4 or a part C, none blocks F3's done criterion)

- **F3's done criterion is met:** a buy works end to end on simnet with the quick wallet (b = 1 and 4, with and without a link, below M_ref, a shard-conflict rebuild), and the post-sign attack tests pass.
- Kaspire: untested (no device); WalletConnect on Android not built. KasWare in a real browser on testnet-10 with this page: F6's journeys (F0 proved the same calls).
- KasWare does not reconnect by itself after a reload (only the quick wallet does); a remembered KasWare could use `getAccounts` silently later.
- The buy's "fee-too-high" state is unused (it belongs to claims, F4/F5).
- The local store: completion after `close()`, the root check against the marker, the round result, backup and import, and My tickets reading it (F4). The backup reader should be Rust.
- The time to build a buy on a mid-range phone (spec 8) is not measured (desktop Chromium: under 0.31 s ticket to cost panel).
- `wss://` through the resolver from an `https` page, the CSP's `connect-src`, the faucet link's URL: F6.
- The copy pass (spec 8; F6 review): the strings added here are figures, names, statuses, errors, control labels, the S7 line and "Sales closed; waiting for close()" (both spec text). Two to check: the heading "What you pay" (part A) and "Stays in round" (a value: where the referral share goes).
- Round 3 of a dev run closes about 10 minutes (DAA at 10/s; longer in wall time at the dev pace) after the run starts; the e2e suite (about 2 minutes) needs a fresh dev run if started later, or the buy tests find sales closed.

### Open questions (owner decisions; nothing was decided)

1. **Which faucet link the wallet sheet shows on testnet-10.** Trust: the page would send players to a third-party site for test coins. Money: none (test coins). Work: one setting (`VITE_FAUCET_LINK`) at the F6 deploy. Until then no link is shown (no faucet URL was verified in this stage). Recommendation: the faucet the Kaspa developers run for testnet-10, checked by hand before the beta.
2. **The deposit row's date.** It shows the date from which a player can always get the deposit back alone (two weeks after the sales deadline). If the round is drawn and the ticket wins, it comes back earlier with the last claim. Trust: none. Money: none. Work: none to keep; small to show a second date after the draw (F4). Recommendation: keep the one date before the draw; F4's ticket card shows the real one after.

### What the reviewer should attack

- **Signing and the post-sign check through the real page path:** can anything reach `nodeSubmit` without `assemble`? (`signAndSubmit` is the only caller in the purchase; `nodeSubmit` itself checks only the stated id.) Is the in-process test worker the same path as the browser's (`worker/handler.ts`)? Do the KasWare and Kaspire adapters ever omit `signInputs`, or accept a list other than the built one's? Does `Serial` keep one request at a time when a request throws?
- **`buyFigures`:** can a figure the panel shows differ from what the transaction does (the referral when unpaid, a second plain output, the shard's successor, the total)? Is T + D_RET the right "back" date (`ticket.sil` A5b.3)?
- **Referral parsing:** `?ref=` reaches the wasm only through `scriptOf` (capped at 128 characters); never rendered as HTML; can a stored value from an older page or another network be used? (`referrerFor` re-validates on every build; the builder refuses another network's address again.)
- **Untrusted indexer data:** the shard list and purchases come from the API; each shard is read from the node by outpoint and checked (`checkIndexerObjects`), the purchases against the shard's root in `buildBuy`, the round copy with `checkIndexerRound`. The ticket's entry comes from the node (`nodeEntries` + `verifyState`).
- **Node-only entries:** wallet coins (`nodeCoins`), shard entries (`nodeEntries`), the ticket's entry, the balance (`nodeBalance`): none from the indexer or the wallet.
- **Key storage:** the quick wallet's secret in IndexedDB (plain, by spec: "anyone with this device can spend it"), its own database, exported only on a button press, never logged (the Rust `Debug` redaction; the page logs only codes and details). The secret crosses `postMessage` to the worker for each signature.
- **The mainnet page check** and **the DEV-only hooks** (`check-bundle.mjs`: is a string scan enough? The hooks are dropped by `import.meta.env.DEV` constant folding).
- **Copy rule** on S1 and S7 (list above).
