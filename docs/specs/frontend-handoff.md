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
| F0 wallet experiment | part A done, waiting for owner | Probe `tn10 wallet-probe` built and self-tested on testnet-10 (stand-in signer). Part B (the owner's clicks, KasWare only; Kaspire not available, untested) is scheduled for 2026-10-26: see "F0 part B: owner steps". |
| F1 builder refactor for wasm | part A done | Templates embedded (W2), compiler behind `compiler`, one rusty-kaspa version (vendored `silverscript-abi` on v2.1.0, no encoding change, proven by golden digests), wasm cdylib links with no extra flag (5.43 MB at O3, 5.01 MB at Os). Part B: the `crates/web` API, `Result` everywhere and W3, the signing split per case, W1, W4. See "F1 part A". |
| F2 indexer API | not started | |
| F3 web shell and Play | not started | |
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
