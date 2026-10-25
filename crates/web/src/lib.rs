//! `crates/web`: the `lottery` code for the browser (spec frontend 6.1; F1 part C).
//!
//! A wasm-bindgen API over `lottery::web`, run in a Web Worker. Entry points, all returning
//! `{ ok: T } | { error: ApiError }` (never a throw, never a panic on any input):
//! - `start(network)`: "testnet-10" (a `simnet` build also knows "simnet"); a `mainnet`
//!   build refuses and throws at instantiation (spec 6.6, W4);
//! - decode: `decodeState(role, stateHex)`, `decodeRedeem(redeemHex)`;
//! - verify: `verifyState(covenantId, role, {utxo, state})` (spec 6.3), `checkRoots`
//!   (trees from purchases against a live marker's or pool's frozen roots), `winningNumbers`
//!   (from the anchor proof, J6);
//! - build: `buildBuy`, `buildClaim`, `buildReturnDeposit`, `buildClose`, `buildDraw` (the
//!   void without a proof), `buildRequestDeposit`, `buildRequestWithdraw`, `buildSettle`:
//!   each from the covenant UTXOs as a node shows them plus their state bytes, the user's
//!   choices and the wallet's coins and fee rate; each returns the unsigned Safe JSON the
//!   wallet signs (`signPskt`'s `txJsonString`), the inputs to sign (`signInputs`), the
//!   masses, the fee and the figures;
//! - `assemble({built, signed})`: the post-sign check (spec 6.2) and the final transaction;
//! - estimate: `feeFor`, `wMin`; and `addressOf`, `scriptOf`;
//! - the quick testnet wallet's key hook (`quickWalletNew`, `quickWalletImport`,
//!   `quickWalletSign`), absent from a `mainnet` build.
//!
//! Layers: `dto` (the boundary types, declared to TypeScript by tsify), `convert` (hex,
//! decimals, UTXOs, addresses and the network), `views` (decoded states), `api` (the logic in
//! plain Rust, which native tests call), `bindings` (the exports), `safe_json` (the wallets'
//! format, shared with tn10), `quick_wallet`. No lottery rule lives here: every derived value
//! comes from `lottery::web` (`cases`, `build`, `sign`, `verify`).

pub mod api;
pub mod bindings;
pub mod convert;
pub mod dto;
#[cfg(not(feature = "mainnet"))]
pub mod quick_wallet;
pub mod safe_json;
pub mod views;
/// The wRPC feasibility probe (feature `wrpc-probe`, wasm32 only; spec 6.1 "Anchor proof").
#[cfg(all(feature = "wrpc-probe", target_arch = "wasm32"))]
pub mod wrpc_probe;
