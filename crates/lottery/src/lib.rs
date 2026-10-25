//! Off-chain protocol code for the Kaspa L1 lottery: combinations, the counter tree,
//! state encoders, the arithmetic reference and the transaction builders. It only
//! assembles transactions and proofs; the covenants decide.
//!
//! Spec M7 D101: the TEST-ONLY seed path, the `test-seed` feature and its `compile_error!`
//! gate are deleted; `draw` compiles unconditionally. The `mainnet` feature stays for the
//! genesis network refusal.
//!
//! Front-end F1: the templates are embedded (`embedded`); the compiler and the `.sil` source
//! paths are behind the default `compiler` feature. The TEST-ONLY short-void drawer variant
//! exists only with `compiler` and without `mainnet` (SP12).

pub mod anchor;
pub mod buy;
pub mod bundle;
pub mod claim;
pub mod close;
pub mod combo;
pub(crate) mod decode;
pub mod digest;
pub mod draw;
pub mod embedded;
pub mod game;
pub mod genesis;
pub mod index;
pub mod ledger;
pub mod params;
pub mod smt;
pub mod state;
pub mod store;
pub mod vault;
pub mod web;
