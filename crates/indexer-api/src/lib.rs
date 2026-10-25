//! `crates/indexer-api` (spec frontend 6.3, F2): a read-only HTTP JSON server over the M8
//! indexer's store (`index/1`, `lottery::index`). It holds no keys and makes no transactions.
//!
//! - `store`: the index rebuilt from `index-log.jsonl` (the log `tn10 indexer` or the
//!   simnet dev run writes) and followed as it grows, with the genesis states from the
//!   verified launch bundle (`bundle/1`).
//! - `routes`: the v1 endpoints, each a `web::indexer` response type (the page parses and
//!   checks the same types; nothing is mirrored by hand).
//! - `server`: the router, its guards (GET only, CORS, size caps, a request deadline, a
//!   concurrency limit, connection timeouts) and the accept loop; `run` is the entry point
//!   of the binary and of an embedding process.
//! - `config`: flags and environment.
//! - `doctor` (feature `doctored`, TEST-ONLY): the untrusted-API fixtures.
//!
//! Nothing it serves is trusted by the page (spec 6.3, D132): see `web::indexer`.

pub mod config;
#[cfg(feature = "doctored")]
pub mod doctor;
pub mod routes;
pub mod server;
pub mod store;

pub use config::Config;
pub use server::{AnchorProvider, AppState, open, router, run, serve};
