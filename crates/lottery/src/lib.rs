//! Off-chain protocol code for the Kaspa L1 lottery: combinations, the counter tree,
//! state encoders, the arithmetic reference and the transaction builders. It only
//! assembles transactions and proofs; the covenants decide.

pub mod buy;
pub mod close;
pub mod combo;
pub mod game;
pub mod genesis;
pub mod ledger;
pub mod params;
pub mod smt;
pub mod state;
