//! `crates/web`: the `lottery` code for the browser (spec frontend 6.1; F1 part C).
//!
//! A wasm-bindgen API over `lottery::web`, run in a Web Worker. Entry points, all returning
//! `{ ok: T } | { error: ApiError }` (never a throw, never a panic on any input):
//! - `start(network)`: "testnet-10" (a `simnet` build also knows "simnet"); a `mainnet`
//!   build refuses and throws at instantiation (spec 6.6, W4);
//! - decode: `decodeState(role, stateHex)`, `decodeRedeem(redeemHex)`;
//! - verify: `verifyState(covenantId, role, {utxo, state})` (spec 6.3), `checkRoots`
//!   (trees from purchases against a marker's or pool's frozen roots), `winningNumbers`
//!   (from the anchor proof, J6: `final` after the draw, `provisional` before it, with the
//!   anchor block's header from a public node binding X, F1 re-review S1); both take chain
//!   objects only and derive the round, m and D in Rust (F1 review). No call checks that a
//!   UTXO entry is live (unspent): the page reads every entry it passes from a public node
//!   by outpoint, never from the indexer (F1 re-review N1);
//! - build: `buildBuy`, `buildClaim`, `buildReturnDeposit`, `buildClose`, `buildDraw` (the
//!   void without a proof), `buildRequestDeposit`, `buildRequestWithdraw`, `buildSettle`:
//!   each from the covenant UTXOs as a node shows them plus their state bytes, the user's
//!   choices and the wallet's coins and fee rate; each returns the unsigned Safe JSON the
//!   wallet signs (`signPskt`'s `txJsonString`), the inputs to sign (`signInputs`), the
//!   masses, the fee and the figures;
//! - `assemble({built, signed})`: every figure of `built` checked against its transaction
//!   (spec 7, F1 review), the post-sign check (spec 6.2) and the final transaction;
//! - estimate: `feeFor`, `wMin`; and `addressOf`, `scriptOf`;
//! - the indexer API (F2): `indexerLimits`, `parseIndexer*` (each response kind, its size
//!   capped before parsing) and `checkIndexerInstance`, `checkIndexerObjects`,
//!   `checkIndexerRound` (the API's copy against the chain; any difference is
//!   `chain_mismatch`), with the response types in `indexer`;
//! - S3/S4 (F5): `verifyRound` (J6, each check with its own result: match, mismatch, not
//!   checkable) and `callerFigures` (the caller panel's rows of a built `close()`, `draw()` or
//!   void, bound to it); `checkIndexerRound` also returns the round's facts and `anchorBound`;
//! - the quick testnet wallet's key hook (`quickWalletNew`, `quickWalletImport`,
//!   `quickWalletSign`), absent from a `mainnet` build.
//!
//! Layers: `dto` (the boundary types, declared to TypeScript by tsify), `convert` (hex,
//! decimals, UTXOs, addresses and the network), `views` (decoded states), `api` (the logic in
//! plain Rust, which native tests call), `bindings` (the exports), `safe_json` (the wallets'
//! format, shared with tn10), `quick_wallet`. No lottery rule lives here: every derived value
//! comes from `lottery::web` (`cases`, `build`, `sign`, `verify`).

pub mod api;
/// The backup file and the re-check of a kept round copy against the chain (spec S8, 6.3; F4).
pub mod backup;
pub mod bindings;
pub mod convert;
pub mod dto;
/// The indexer API's response types (served by `crates/indexer-api`) and the page's checks of
/// them against the chain (spec 6.3; F2).
pub mod indexer;
/// The page's network layer, plain-Rust half (F3): URLs, the node's network, fee rates, which
/// entries to read for the indexer's objects, the error table and the jackpot odds.
pub mod node;
/// The page's node client (F3, feature `node`, wasm32): the rusty-kaspa v2.1.0 wRPC client.
/// Compiled out of a `mainnet` build (which refuses to start anyway, W4), like the quick wallet.
#[cfg(all(feature = "node", target_arch = "wasm32", not(feature = "mainnet")))]
pub mod node_client;
#[cfg(not(feature = "mainnet"))]
pub mod quick_wallet;
/// S3 Rounds and S4 Run the round (spec 3, 4; F5): the round's facts, "Verify in this browser"
/// item by item, and the caller panel's figures.
pub mod rounds;
pub mod safe_json;
/// My tickets (spec S2; F4): the ticket card, the claim row and the deposit return, from Rust.
pub mod tickets;
/// S5 Vault (spec S5; F6 part A): the vault's figures, the wallet's positions and requests, and the
/// rows of a built request or settlement, bound to it.
pub mod vault;
pub mod views;
/// The page's local store format (`webstore/2`, spec 6.3; front end F3 part B, F4): the records the
/// page keeps in IndexedDB, declared here so the page has no hand-written copy of them.
pub mod webstore;
/// The wRPC feasibility probe (feature `wrpc-probe`, wasm32 only; spec 6.1 "Anchor proof").
#[cfg(all(feature = "wrpc-probe", target_arch = "wasm32"))]
pub mod wrpc_probe;
