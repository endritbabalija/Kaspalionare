# Front-end track: build handoff

Spec: `docs/specs/frontend.md` (approved 2026-10-25, D129–D137). Delete this file when the track closes and its content is in `docs/DECISIONS.md`.

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
| F0 wallet experiment | not started | |
| F1 builder refactor for wasm | not started | |
| F2 indexer API | not started | |
| F3 web shell and Play | not started | |
| F4 My tickets, claims, backup | not started | |
| F5 Rounds, verify, run the round | not started | |
| F6 Vault, How it works, beta instance, deploy, security review | not started | |
