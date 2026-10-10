//! Off-chain protocol code for the Kaspa L1 lottery: combinations, the counter tree,
//! state encoders and the transaction builder. It only assembles transactions and proofs;
//! the covenants decide.

pub mod buy;
pub mod combo;
pub mod smt;
pub mod state;
