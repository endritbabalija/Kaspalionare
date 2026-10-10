//! Off-chain protocol code for the Kaspa L1 lottery: combinations, the counter tree,
//! state encoders, the arithmetic reference and the transaction builders. It only
//! assembles transactions and proofs; the covenants decide.

// D43 (spec M3): the drawer's entry takes a TEST-ONLY seed. It must never be built for mainnet.
#[cfg(all(feature = "test-seed", feature = "mainnet"))]
compile_error!("TEST-ONLY seed cannot be built for mainnet");

pub mod buy;
pub mod claim;
pub mod close;
pub mod combo;
#[cfg(feature = "test-seed")]
pub mod draw;
pub mod game;
pub mod genesis;
pub mod ledger;
pub mod params;
pub mod smt;
pub mod state;
