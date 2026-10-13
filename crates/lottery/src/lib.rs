//! Off-chain protocol code for the Kaspa L1 lottery: combinations, the counter tree,
//! state encoders, the arithmetic reference and the transaction builders. It only
//! assembles transactions and proofs; the covenants decide.
//!
//! Spec M7 D101: the TEST-ONLY seed path, the `test-seed` feature and its `compile_error!`
//! gate are deleted; `draw` compiles unconditionally. The `mainnet` feature stays for the
//! genesis network refusal.

pub mod anchor;
pub mod buy;
pub mod claim;
pub mod close;
pub mod combo;
pub mod draw;
pub mod game;
pub mod genesis;
pub mod ledger;
pub mod params;
pub mod smt;
pub mod state;
pub mod vault;
