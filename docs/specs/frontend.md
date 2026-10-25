# Front end spec: the Kaspalionare web app (testnet-10 beta)

Status: approved 2026-10-25 (owner, design session), with the owner's two build rules added (D137). Implementation in a fresh chat, stages F0–F6, handoff `docs/specs/frontend-handoff.md`. Decisions D129–D137 (section 11). Design references are to `docs/design.md` v0.4.1. rusty-kaspa paths are relative to `refs/rusty-kaspa-v2.1.0`.

This is a track of its own, like the seed track. It runs alongside M8 and needs no covenant change. `docs/milestones.md` listed "any front end" as out of scope; D129 opens it for testnet only. Mainnet stays out of scope.

## 0. Interview answers (owner, 2026-10-25)

- **Audience:** a public testnet-10 beta. Strangers with testnet KAS play real rounds.
- **Scope of the first version:** play (buy, my tickets, claim), the run-the-round buttons (`close()`, `draw()`), the backer vault page, the round explorer with verification, and a how-it-works page.
- **Referral default:** referral links. With no link, the referral goes to the site owner's address.
- **Feel:** calm, transparent, technical. The mock-up "Instrument panel" was approved as is: https://claude.ai/artifact/DiZopDGHYt1a74ZYvvyozv
- **Name:** Kaspalionare. English only. Phone first.
- **Claims and deposit returns:** the player presses the button. There is no helper service.
- **Signing:** both paths. The player's own wallet (KasWare, Kaspire) is the main path, plus a quick testnet wallet kept in the page.
- **Data:** a hosted indexer API that nobody has to trust, plus a local copy of each round the player is in, plus a backup download.
- **Stack:** accepted as proposed (section 6).

## 1. Goal and rules the site must keep

A player can buy, follow, claim and get their deposit back on a phone without knowing what a covenant is. Anyone can check every number the site shows.

- **The site only builds transactions.** Every rule is enforced by the covenants (design 1.2). The page contains no lottery logic of its own: it calls the same `lottery` Rust code the covenant tests use, compiled for the browser (6.1). The page has no TypeScript copy of prize, tier or game-size arithmetic. Displayed figures are decoded from on-chain state or computed by the shared Rust code.
- **The site survives its own services disappearing** (design 1.4, property 7). If the indexer API or the website goes away, a player who kept the local copy or the backup file can still claim (6.3), from a fresh copy of the site built from the repo.
- **The site never asks a wallet to sign a covenant input.** After signing, the page checks that nothing it built has changed (6.2).
- **Testnet-10 only** (CLAUDE.md Safety). The page refuses `kaspa:` addresses and any network other than testnet-10. The quick testnet wallet and the testnet network choice are absent from any `mainnet` build, which is refused anyway (6.6).

### 1.1 Guiding rule: UX, DX and AX together (owner, D137)

Make the decision that improves all three without breaking anything: the user experience (UX), the developer experience (DX) and the agent experience (AX, meaning the agents that build, test and review this code). When the three pull apart, choose the option that hurts none of them and say so in the stage report. "Without breaking anything" means:
- no covenant change;
- `cargo test` stays green at every stage;
- the native builder's transactions stay byte-identical (W1).

- **UX:**
  - The copy rule (section 5, rule 6).
  - Every figure can be tapped through to its on-chain source.
  - One job per screen.
- **DX:**
  - One command starts the site against the private simnet with fixture data.
  - TypeScript types for the wasm API are generated from the Rust code (the `.d.ts` that wasm-bindgen emits), never written by hand to mirror it.
  - Design tokens live in one file.
  - A component catalogue page exists in development builds only.
  - TypeScript runs in strict mode, with lint and format checks in the test command.
- **AX:**
  - Every interactive element has an accessible name and a stable `data-testid`, so Playwright, agents and screen readers find it the same way.
  - Errors are typed codes with one message each, read the same way by people and agents.
  - Fixtures are deterministic (simnet snapshots).
  - Each component lives in its own small file.
  - A `web/CLAUDE.md` holds the front end's conventions and commands. The root `CLAUDE.md` gains the web build and test commands.
  - Each stage ends with the handoff file updated.

## 2. Users and journeys

| ID | Who | Journey |
|---|---|---|
| J1 | New player | Opens the site (maybe through a referral link) → connects KasWare or Kaspire, or creates a quick testnet wallet and funds it from the faucet → picks numbers or uses quick pick → adds up to 4 combinations to one ticket → reviews the cost → signs → watches built, signed, sent, accepted → sees the ticket under My tickets |
| J2 | Returning player | Opens My tickets after the draw → sees the winning numbers with matches highlighted → claims each winning combination (fee shown first) → gets the deposit back with the last claim, or by "Return deposit" once the claim window has passed |
| J3 | Player on a new device, or after clearing the browser | Imports the backup file, or connects the same wallet: tickets are found by payout address through the indexer API, and the trees come from the API, checked against the roots |
| J4 | Caller | Opens Rounds → sees "close is due" or "draw is due" with the caller reward → presses it → signs the fee input → the reward arrives in their own output |
| J5 | Backer | Opens Vault → sees the share price, the liability lock and their position → requests a deposit (at least 10 KAS) or a withdrawal → waits for the next draw → settles |
| J6 | Sceptic | Opens a round → sees sales, m, the jackpot, the anchor block, the winning numbers, the frozen roots and every pool → presses "Verify in this browser": the page recomputes the winning numbers from the anchor hash and the roots from the trees, and shows match or mismatch |
| J7 | Referrer | Copies their referral link (`?ref=kaspatest:…`) from the menu |

## 3. Screens

Bottom navigation on a phone: **Play · My tickets · Rounds**. The menu holds **Vault, How it works, Backup, Wallet**. On desktop the same layout is centred in a wider column. Every screen has loading, empty, error and stale-data states. Every amount is shown in KAS with two decimals, and with exact sompi on tap.

**S1 Play** (mock-up board 2).
- **Round card:** round number, status, jackpot J, time until the deadline T, jackpot odds 1 in 142,506 × m, and a link to the round's explorer page.
- **Number picker:** 5 of 1..30, then a bonus in 1..m with m read from the shard. Quick pick and Clear.
- **Ticket builder:** 1..B_MAX = 4 combinations. Duplicates and shared main numbers are allowed (design 4.3).
- **Cost panel:**
  - price b × p
  - deposit d × b and when it comes back
  - network fee from the node's fee estimator
  - the referral share and its destination
  - total
- **Buy button.**
- After the deadline: "Sales closed; waiting for `close()`", with a link to S4.
- On shard contention, the page rebuilds on another shard (up to 3 tries) and says so.

**S2 My tickets** (mock-up board 3).
- One card per ticket: the round, the combinations, and each one's tier result and prize.
- A **Claim** button per winning combination. It shows the prize; "shared with h − 1 others" when h > 1; and the fee before signing, with a warning when the fee is larger than the prize.
- A prize below P_MIN = 0.05 KAS accrues in the ticket and is paid with its last claim (D56). The card explains this.
- **Return deposit** appears when `return_deposit()` is valid (7.5). Open tickets show the deadline.
- Void rounds show "No draw: void" and what the player can still do.
- The "Saved on this device" box shows the local copy's status and a backup download.

**S3 Rounds and round detail.**
- The list: one row per round with status (Open, Closing, Drawn, Void), sales and jackpot.
- The detail shows:
  - m, S, the deadline T, D and the claim deadline (clock and DAA score)
  - the anchor block hash and DAA score
  - the winning numbers
  - the 16 frozen roots
  - the pools and their remaining value
  - the vault settlement
  - links to a testnet block explorer for every transaction
- **Verify in this browser** (J6) runs in the worker. It recomputes the winning numbers from the anchor hash with the shared Rust code and compares them with the record. It rebuilds the 16 shard trees and compares their roots. It decodes each state and compares it with its live UTXO (6.3).

**S4 Run the round** (a panel on S3).
- "`close()` is due" when T has passed and the previous round is drawn. "`draw()` is due" from D while the anchor proof can be built. "Void is due" from D + draw_window.
- Shows the caller reward C_close or C_draw and the fee. When the fee exceeds the reward, the panel says so (design 12 liveness note).
- The press flows through the same build, sign and send steps.

**S5 Vault.**
- Shows the vault value, the share price (A + VA) / (S + VS), the locked liability L and the pending totals. Shows "next request possible in …", from VAULT_REQ_DELAY = 600 DAA since the last vault spend.
- Shows the user's positions, requests and settle buttons.
- Deposit request: at least MIN_DEPOSIT = 10 KAS.
- Withdrawal request: the whole position, or w ≥ wmin with a remainder ≥ wmin (design 5.4). The page computes wmin with the shared code and offers only valid amounts.
- The terms are written out: requests are irrevocable, they convert at the next draw's price, and a deposit can come back as a refund (refund mode).

**S6 How it works.**
- A plain-language explainer: the four functions, where the money goes (design 5.1 and 5.2 as a picture), why counts are frozen before the seed, the claim window, and what "verify" checks.
- It reuses the isometric figures in `docs/figures/` (round machine, draw cabinet, shard rack) as static or interactive figures.
- It states the open release-gate items honestly: the seed's miner-withholding bias (D100) and that this is a testnet beta.

**S7 Wallet sheet.**
- Connect KasWare or Kaspire, or create, import or export the quick testnet wallet.
- Shows the address, the balance and the network.
- The quick wallet shows a permanent "testnet key stored in this browser; anyone with this device can spend it" line, and a faucet link.

**S8 Backup.** Download or import the local store (6.3) as one file, and see what it contains per round.

## 4. Components

Each component lists its states.

| Component | States |
|---|---|
| Number ball | free, picked, matched, full (no more picks), disabled |
| Bonus tile | free, picked, matched |
| Round status pill | Open, Closing, Drawn, Void |
| Jackpot and countdown block | live, deadline passed, stale data |
| Ticket builder row | combination, remove |
| Cost panel | empty, estimating, ready, fee-too-high warning |
| Transaction progress | built → signed → sent → accepted; failed with reason; conflict → rebuilt |
| Ticket card | open, drawn (win, no win, partly claimed, fully claimed), void, deposit returnable, deposit returned |
| Claim row | claimable, claiming, claimed, accrued below P_MIN, fee exceeds prize |
| Round row and round detail | the S3 fields; verify result: match, mismatch, not checkable |
| Caller panel | not due, due with reward, reward below fee |
| Vault position card | position, pending request, settle due |
| Wallet sheet | disconnected, connecting, wrong network, connected, quick wallet |
| Local-copy box | complete and verified, incomplete, mismatch, not needed |
| Amount | KAS with 2 decimals; exact sompi on tap |
| Time | local clock plus DAA score; DAA converted at 10 blocks per second |
| Hash or address | shortened mono with copy and explorer link |

## 5. Visual design

- **The approved mock-up is the reference:** https://claude.ai/artifact/DiZopDGHYt1a74ZYvvyozv
- **Colours** (tokens):

  | Token | Hex | Use |
  |---|---|---|
  | paper | #F6F6F3 | ground |
  | ink | #15171A | text, selected |
  | graphite | #565C63 | labels |
  | rule | #DFE1DC | lines |
  | teal | #0E7A70 | actions, links |
  | mint | #74CDBF | wins, fill only |
  | amber | #A14F08 | time, warnings |
  | white | #FFFFFF | cards |

- **Type:** Geist for text and Geist Mono for every number, hash and address. The fonts are served by the site itself: no third-party font requests.
- **Rules:**
  1. Every figure links to where it lives on-chain.
  2. State what it costs before anyone signs.
  3. One job per screen; on a phone the buy card comes first.
  4. Celebrate only a real win.
  5. Times are shown as a clock and as a DAA score.
  6. **No taglines, helper text or secondary labels (owner, D137).** If a screen needs one to make sense, fix the layout. What stays:
     - figures and their names (a cost row is data, not helper text);
     - status;
     - errors;
     - the testnet badge;
     - field labels that a control needs for accessibility (visually hidden where the layout already says it).

     A fact the player needs becomes a value, not a sentence: the deposit row shows its return date ("back 2 Nov"), and the referral row shows its destination. Explanations live only on S6 How it works.
- **The mock-up's colours, type and layout are the reference; its explanatory lines are not.** Rule 6 supersedes the explanatory lines in the mock-up: the footer note on Play, the referral paragraph, the deposit sentence and the notes under ticket results.
- **Light mode at launch.** The tokens are built so a dark mode can be added later.
- **Accessibility:** WCAG 2.1 AA contrast, touch targets of at least 44 px, real buttons and labels, keyboard use on desktop, and no meaning carried by colour alone (matched balls also carry a label for screen readers).

## 6. Architecture

```
 phone / desktop browser
 ┌───────────────────────────────────────────────┐
 │ React + TypeScript page (web/)                │
 │   ├─ wallet adapters: KasWare · Kaspire ·     │──► wallet (signs P2PK funding inputs only)
 │   │                   quick testnet wallet    │
 │   ├─ local store (IndexedDB) + backup file    │
 │   └─ Web Worker: wasm (crates/web)            │
 │        lottery builders, decoders, verify     │──► public testnet-10 nodes (wRPC via resolver):
 │        rusty-kaspa v2.1.0 RPC client          │    live UTXOs, fee estimate, submit, anchor proof
 └───────────────────────────────────────────────┘
          │ HTTPS, read-only JSON
          ▼
 indexer API (crates/indexer-api) on a small server ◄── M8 indexer (lottery::index) following testnet-10
```

### 6.1 The builder in the browser: `crates/web`

A wasm crate (wasm-bindgen) that exposes the `lottery` builders and decoders to the page. It runs in a Web Worker, so the page never freezes. The feasibility check on 2026-10-25 compiled the library for wasm32, built a real `buy()` and passed the shard covenant in the engine (`Ok(354210)`), with a probe-only link flag. A browser build first needs these changes:

- **Templates embedded at build time.** `lottery` reads `.sil` files from disk (`buy.rs:83, 93, 109, 156`) and compiles them (`buy.rs:245-259`, `draw.rs:53`). A generator writes the compiled artifacts (`SilAbiArtifact`, serde) into the crate. A test checks that they equal a fresh compile. The SilverScript compiler moves behind a `harness` feature that the web build leaves off.
- **One rusty-kaspa version.** `silverscript-abi` pulls a second `kaspa-consensus-core` (rev `a41a333`), so the link has duplicate symbols. The fix: build the ABI against v2.1.0 (2 API errors found: `lib.rs:901`, `lib.rs:1204`). If that fix changes encoding in any way, it is a stop-and-ask.
- **The wasm toolchain:** getrandom's `wasm_js` option, and a wasm-capable clang for `secp256k1-sys`.
- **No panics on untrusted input.** wasm aborts on panic (`panic = "abort"`), so the `catch_unwind` guards (`index.rs:952`) do nothing there. Every decoder and builder reachable from the page returns `Result`. The page restarts the worker if it dies.
- **Signing split from building.** Today `Built::finish` signs with a raw key (`buy.rs:1049`). The web API returns an unsigned transaction with budgets and storage mass committed, the indices of the inputs the user must sign, the masses and the fee. A second call checks and assembles the signed result.
- **The API** covers:
  - decode: every state
  - build: buy, claim, return_deposit, close, draw (including void), request_deposit, request_withdraw, settle
  - verify: trees against roots, winning numbers from an anchor hash, a state against its UTXO
  - estimate: fee, wmin

  It is grouped by entry point (`BuyCase`, `ClaimCase::from_store`, `ReturnCase`, `CloseCase`, `DrawCase::from_store`, `DepositCase`, `WithdrawCase`, `SettleCase`).
- **Anchor proof.** `GetSeqCommitLaneProof` is in the Rust wRPC client only (`rpc/wrpc/client/src/client.rs:667`), not in the JS binding. The worker calls it through the Rust client compiled into the wasm. If that client does not run in a browser, the indexer API serves the proof, unverified, and the chain checks it in `draw()`.

### 6.2 Signing

- **The wallet adapter interface:** `connect`, `network`, `address`, `utxos`, `signInputs(tx, indices)`.
- **KasWare:** `window.kasware.signPskt({txJsonString, options: {signInputs}})` with the Safe JSON transaction. Only the user's P2PK input indices are listed. Network `kaspa_testnet_10`.
- **Kaspire:** `window.kaspire.signPskt` on desktop, and WalletConnect `kaspa_signPskt` on Android.
- **Quick testnet wallet:** a key made in the browser with the rusty-kaspa wasm `PrivateKey` and signer. It is stored in IndexedDB and can be exported and imported. It is compiled out of a `mainnet` build.
- **The post-sign check** (an attack-test target). Before sending, the page compares the returned transaction with the one it built. It refuses to send if:
  - any output, the version, the lock time, the subnetwork, the gas or the payload differs;
  - any input's outpoint, sequence or sig-op count differs;
  - any covenant input's signature script differs;
  - any input other than the listed ones gained or lost a signature.
- **The wallet's own unknowns.** Its documented behaviour on inputs it does not own, its Safe JSON round trip at our masses, and whether Kaspire accepts unknown covenant scripts are all unverified. F0 settles them on testnet-10. If KasWare fails F0, the beta ships with the quick wallet only, and the wallet path waits. The owner is told.

### 6.3 Data: the indexer API and the local copy

**The indexer API** (`crates/indexer-api`) is a read-only HTTP JSON server over the M8 indexer's store (`lottery::index`, `index/1`). It holds no keys and makes no transactions. Endpoints (v1):

| Endpoint | Returns |
|---|---|
| `GET /v1/instance` | the launch bundle (`bundle/1`): genesis outpoint, covenant ID, template hashes, parameters |
| `GET /v1/rounds`, `GET /v1/rounds/{r}` | status, outpoints and state bytes of the controller, shards, marker, pools and record; T, D, anchor, winning numbers |
| `GET /v1/rounds/{r}/shards/{i}` | the shard's purchases in compact form (combination, bonus, order), enough to rebuild the tree |
| `GET /v1/tickets?payout={address}` | tickets by payout script: outpoint, state bytes, round |
| `GET /v1/vault`, `GET /v1/positions?owner=`, `GET /v1/requests?owner=` | vault state, share positions, requests, records |
| `GET /v1/rounds/{r}/anchor-proof` | fallback proof for `draw()` (6.1) |

**Nothing from the API is trusted.**
- Before the page uses a state, it rebuilds the object's script from the embedded template plus the state, and checks it against the live UTXO's script public key from a public node.
- Trees are rebuilt and their roots compared with the roots in a state checked that way.
- On a mismatch the page refuses to build and says "the indexer's copy does not match the chain".
- So a lying or dead API can only stop the page from building. It cannot move money, because the chain rejects anything wrong and a rejected transaction costs nothing.

**The local copy** (IndexedDB, format `webstore/1`, versioned like `wallet/1`). For each round the player holds tickets in, it keeps:
- the ticket outpoints and full state bytes (R1);
- the compact purchase list of all 16 shards (about 6 bytes per combination in compact form, estimate; about 60 KB at 10,000 combinations);
- the round result once drawn.

The copy is taken while the round is open and completed right after `close()`. It is checked against the frozen roots once the marker exists. **Backup** writes the whole store to one file, and **import** reads it back with the same checks. A claim built from the local copy needs no API: only a public node for the live UTXOs.

### 6.4 Stack and layout

| Piece | Choice |
|---|---|
| Page | React + TypeScript, Vite, static build, in `web/` |
| Styling and components | Tailwind CSS with shadcn/ui (Radix primitives), restyled with the section 5 tokens |
| Lottery code | `crates/web` (wasm-bindgen) in a Web Worker |
| Kaspa network | rusty-kaspa v2.1.0 wasm RPC client, public testnet-10 nodes through the resolver (TLS from an https page) |
| Data | `crates/indexer-api` (Rust, read-only), on a small server; provider chosen in F6 |
| Site hosting | Vercel, static |
| Tests | Rust tests for `crates/web` and the API; Vitest for page logic; Playwright end to end against the private simnet (D125) and testnet-10 |

### 6.5 Configuration

Build-time variables, none of them secret:
- the indexer API URL;
- the site owner's referral address (a `kaspatest:` address);
- the instance's launch bundle hash;
- the testnet block explorer URL.

### 6.6 Safety and security

- **Network refusals:**
  - Any address that is not `kaspatest:` is refused in every input, the `ref` parameter included.
  - A wallet on another network is refused.
  - A `mainnet` build of `crates/web` refuses to start, and does not contain the quick wallet (the same pattern as the genesis refusal and the SP12 seed gate).
- **Page hardening:**
  - A strict Content Security Policy. No third-party scripts and no analytics in the beta.
  - The `ref` value is parsed and validated as a standard script before use, never inserted as HTML.
- **Repo hygiene:** no keys, seeds or credentials in the repo. The quick wallet's key lives only in the user's browser.

## 7. Money and wording rules

- **Before any signature**, the page shows, as figure rows and not prose (section 5, rule 6): what leaves the wallet, what comes back and when, the fee, the referral destination, and the total. Amounts come from the built transaction, not from separate arithmetic.
- **Referral.**
  - `?ref=kaspatest:…` is remembered in this browser until another valid link replaces it.
  - With no stored referral, the referral output pays the site owner's address.
  - The referral r = floor(b × p × s_R / 10⁴) is paid only if r ≥ M_ref = 0.1 KAS. Below that, the output is omitted and the share stays in the round (it reaches later jackpots), and the cost panel says so.
  - A buyer may name themselves (design 5.1). The page offers this only through a referral link, not as a default.
- **Claims.** The claim panel shows:
  - the prize, floor(slice / h);
  - the fee, from the estimator;
  - "accrues, paid with your last claim" for a prize below P_MIN;
  - the claim deadline;
  - a warning when the fee exceeds the prize.

  Under load a small claim can cost several times P_MIN in fees (`docs/costs.md`, D82).
- **Deposits.** Each combination's deposit d = 0.13 KAS is returned with the ticket's last claim, or by `return_deposit()` after the claim window.
- **Voids.** A void round is shown as such, with what its tickets can still do. The exact wording comes from the M7 void rules (D96).
- **Honesty.** The site never says "provably fair" without the D100 caveat, and never "guaranteed". Every page shows that this is testnet-10 with valueless coins.

## 8. Tests and done criteria

Attack tests are written first for every check that guards money or data.

**`crates/web`**
- W1: for every transaction shape in the M1–M8 fixtures, the wasm build and the native build produce byte-identical transactions and masses.
- W2: embedded templates equal a fresh compile.
- W3: every decoder and builder fed truncated, oversized or random bytes returns an error, never a panic (property test).
- W4: a `mainnet` build of `crates/web` has no quick-wallet symbol and refuses to start.

**Post-sign check (6.2).** A mock wallet that alters each of these is refused before sending:
- a covenant signature script
- an output value
- an output script
- an extra output
- the change address
- an unlisted input's signature
- the lock time
- the payload

**Untrusted API (6.3).** An API that serves each of these makes the page refuse to build, with the mismatch message:
- a wrong state
- a wrong tree
- a missing purchase
- an extra purchase
- a wrong winning number
- a wrong anchor proof (refused by the node)

**Local copy.** With the API stopped, a claim built from the local copy is accepted (simnet). After clearing the browser and importing the backup file, a claim is accepted. A corrupted backup is refused at import.

**End to end on the private simnet (Playwright), with the quick wallet:** buy (b = 1 and 4, with and without a referral link, below M_ref), shard-conflict rebuild, close, draw, claim (a win, a shared win, a below-P_MIN accrual, the last claim with the deposit), return_deposit, a void, vault deposit, withdrawal and settle, and verify on the round page (match, and mismatch against a doctored API).

**Testnet-10:**
- F0's wallet experiment, recorded in `docs/tn10-mass-check.md`.
- The same flows with KasWare and the quick wallet, run by hand with a written log (Kaspire too if F0 passes), on a beta instance with the fastest genesis knobs the floors allow (chosen with the owner in F6).

**Quality:**
- axe-core finds no serious or critical issues on S1–S8.
- **The copy pass:** every visible string on S1–S5, S7 and S8 is a figure, a name, a status, an error or a control label (rule 6). The list of strings is checked in the F6 review.
- **The DX and AX checks** (1.1):
  - a fresh clone runs the site against simnet with one documented command;
  - no hand-written mirror of a Rust type exists in `web/`;
  - every interactive element has a `data-testid` and an accessible name (a Playwright check).
- Layouts hold at 360–430 px wide.
- Touch targets are at least 44 px.
- The size of the compressed wasm bundle and the time to build a buy on a mid-range phone are measured and recorded (no fixed budget until measured).

**Reviews:**
- `covenant-reviewer` on the `crates/web` and `lottery` refactor: byte identity, no rule logic outside Rust.
- A separate security review of the web app: keys, CSP, the post-sign check, untrusted input.

**Done when:**
- the beta is live on Vercel against a testnet-10 instance and the indexer API;
- every journey J1–J7 has passed on testnet-10 with a logged transaction ID;
- every test above passes in CI-equivalent local runs, with output shown;
- the reviewers' must-fix items are fixed;
- D129–D137 and any build decisions are rows in `docs/DECISIONS.md`;
- `docs/milestones.md` shows the track as done.

## 9. Build plan (implementation in a fresh chat; each stage a fresh agent, handoff `docs/specs/frontend-handoff.md`)

| Stage | Work | Ends when |
|---|---|---|
| F0 | Wallet experiment: a minimal `crates/web` (or a probe) builds a `buy()` and a `claim()` for a KasWare testnet-10 account; `signPskt` with only the P2PK index, then with no index list; `pushPskt`; repeat with Kaspire | Accepted transaction IDs, or a documented failure; result row in DECISIONS; owner told if the wallet path waits |
| F1 | Builder refactor for wasm (6.1): embedded templates, compiler behind a feature, ABI on v2.1.0, `Result` everywhere, sign split; `crates/web` API | W1–W4 pass; `cargo test` suite green; `covenant-reviewer` pass |
| F2 | `crates/indexer-api` (6.3) on the M8 indexer, with untrusted-API fixtures | API tests pass; serves a simnet instance |
| F3 | `web/` shell with `web/CLAUDE.md`, the one-command simnet dev run, generated types and the component catalogue (1.1); tokens, components (section 4), wallet adapters with the post-sign check, local store, S1 Play and S7 | Buy works end to end on simnet with the quick wallet; post-sign attack tests pass |
| F4 | S2 My tickets, claims, deposit return, S8 backup and import | Claim from the local copy with the API stopped, and from an imported backup, on simnet |
| F5 | S3 Rounds, verify, S4 run the round (close, draw, void) | Simnet end-to-end suite green, including the doctored-API mismatch |
| F6 | S5 Vault, S6 How it works; beta instance genesis on testnet-10; indexer server; Vercel deploy | Testnet-10 journeys J1–J7 logged; security review; records |

The F0 result can change F3: if the wallet path waits, the quick wallet carries the beta alone.

## 10. Out of scope

- Mainnet and real money. A legal and compliance review is required before any real-money front end (operating a lottery is regulated in most places). It goes on the release gate.
- Dark mode (the tokens are ready), languages other than English, native apps, and iPhone wallet support (no iOS wallet with a dApp API was found).
- A claim or deposit-return helper service, notifications, email, and accounts.
- A `refresh()` UI (the local copy keeps the ticket state; refresh stays a library function).
- Ticket transfer, `set_params()` (the admin uses the existing tools), analytics, and SEO or a marketing site.

## 11. Decisions (each becomes a DECISIONS row on approval)

- **D129:** a front-end track opens for a testnet-10 public beta. Mainnet stays out of scope. No covenant change.
- **D130:** scope is play, run-the-round, vault, explorer with verification, and how it works; English; phone first; brand Kaspalionare.
- **D131:** signing uses the player's wallet (`signPskt`, P2PK inputs only) plus a quick testnet wallet that a `mainnet` build does not contain. Every signed transaction passes the post-sign byte check before it is sent.
- **D132:** data comes from a hosted, read-only, untrusted indexer API. Every state is checked against its live UTXO and every tree against its roots. Each player keeps a local copy and can download a backup, so claims survive the API disappearing.
- **D133:** the stack is React + TypeScript + Vite, Tailwind + shadcn/ui, the `lottery` code as wasm in a Web Worker (`crates/web`), the rusty-kaspa v2.1.0 wasm RPC client, `crates/indexer-api`, Vercel, Vitest and Playwright.
- **D134:** referral links set the referrer, remembered per browser. The default referrer is the site owner's `kaspatest:` address.
- **D135:** the look is "Instrument panel" (section 5, mock-up approved).
- **D136:** claims and deposit returns are made by the player. No helper service.
- **D137:** two owner rules for the build. Choose what improves UX, DX and AX together without breaking anything (1.1). The UI carries no taglines, helper text or secondary labels; if a screen needs them, fix the layout (section 5, rule 6).

## 12. Risks and open items

- **Wallets.** KasWare's behaviour on inputs it does not own, and its Safe JSON round trip at our masses, are undocumented (F0). Kaspire may refuse unknown covenant scripts.
- **The ABI fix.** Porting `silverscript-abi` to v2.1.0 must keep encoding identical (W1 and review). Otherwise stop and ask.
- **Proof RPC.** `GetSeqCommitLaneProof` from a browser is unverified (6.1 fallback).
- **The wasm bundle.** 6.97 MB uncompressed with the compiler included (probe). It should shrink without the compiler; measured in F1.
- **Coverage.** Public resolver nodes may be slow or time out (seen in `vault-m5`). The page retries and shows node status.
- **Pruning.** A player who never opened the site during a round, and whose browser has no copy, depends on the API (or any copy) to claim. This is the design's residual dependency (design 9), and the page says so.
- **The legal question** before real money (section 10).
