//! The web-facing layer of the library (spec frontend 6.1, 6.2, 8 W3; front-end F1 part B).
//!
//! Everything a browser page reaches goes through this module, and every function here
//! returns `Result<_, web::Error>`: a wasm page aborts on a panic (`panic = "abort"`), so a
//! decoder or builder fed untrusted bytes must return an error instead (`index::model`'s
//! `catch_unwind` does nothing there). `tests/f1_w3.rs` (W3) feeds every function here
//! truncated, oversized and random input and fails on any panic.
//!
//! - `verify`: decoders (every state, a redeem script, an anchor proof, the store and backup
//!   formats), the checks against the chain (a state against its UTXO script, trees rebuilt
//!   from compact purchases against the frozen roots, the winning numbers from an anchor),
//!   and the covenants' own state ranges (`check_*`), which the builders apply first.
//! - `build`: the unsigned builders, one per entry point (buy, claim, return_deposit,
//!   close, draw and its void, request_deposit, request_withdraw, settle), funded from a
//!   browser wallet's coins (`Funds`), and the fee loop.
//! - `sign`: the signing split: the unsigned transaction with its budgets and storage mass
//!   committed, the inputs to sign, the masses and the fee (`Unsigned`); then the post-sign
//!   check (spec 6.2) and the final transaction from our own transaction plus the wallet's
//!   signature scripts for the listed inputs only (`assemble`).
//!
//! Errors are typed codes with one message each (spec 1.1 AX); `Error::detail` says what
//! exactly failed, for logs and developers, never for the page's copy.

pub mod build;
pub mod sign;
pub mod verify;

use std::fmt;

/// A stable error code (spec 1.1 AX). The number and the name never change meaning; new
/// codes are appended. `message()` is the one sentence the page may show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Code {
    /// Input bytes or a field have the wrong length.
    BytesLength,
    /// The bytes are not a state (or script) of the expected template.
    NotTemplate,
    /// A decoded state does not re-encode to the same bytes.
    NotCanonical,
    /// A state field is outside the range its covenant accepts.
    StateRange,
    /// A state or script given by the indexer does not match the chain's UTXO (spec 6.3).
    ChainMismatch,
    /// A tree rebuilt from purchases does not give the chain's frozen root (spec 6.3).
    TreeMismatch,
    /// A purchase is not a canonical combination of the round, or a count overflows.
    PurchaseInvalid,
    /// A store or backup file is malformed (format, hex, numbers above 2^53, fields).
    StoreFormat,
    /// A store lacks what the transaction needs (a ticket, a pool, a closed round).
    StoreIncomplete,
    /// The anchor proof is malformed or does not anchor the round's reference D.
    AnchorInvalid,
    /// A build input is outside the range its builder accepts.
    InputRange,
    /// The wallet's coins do not cover the transaction.
    FundingShort,
    /// A wallet coin or the change address is not a standard script of the wallet.
    WalletScript,
    /// A wallet coin is not confirmed yet (no creation DAA score).
    CoinUnconfirmed,
    /// An amount is outside the 64-bit range.
    Overflow,
    /// The lottery rules (the Rust reference of the covenant) refuse this transaction.
    Refused,
    /// A builder safety rule refuses this transaction (D24 stale close, D86 wmin, the
    /// minimum caller output).
    Policy,
    /// A script could not be encoded or budgeted.
    Encode,
    /// The transaction's mass cannot be computed.
    Mass,
    /// The local script engine rejects a covenant input before signing.
    Engine,
    /// The embedded templates are unavailable.
    Templates,
    /// The fee rate or fee is outside the accepted range.
    FeeRate,
    /// The wallet changed the transaction (spec 6.2).
    SignedChanged,
    /// The wallet left an input it was asked to sign unsigned.
    SignedMissing,
    /// A signature script is not in the form its input needs (SIG_HASH_ALL, sizes).
    SignatureForm,
    /// A returned signature does not verify against our transaction.
    SignatureInvalid,
}

impl Code {
    /// Every code, in number order.
    pub const ALL: [Code; 26] = [
        Code::BytesLength,
        Code::NotTemplate,
        Code::NotCanonical,
        Code::StateRange,
        Code::ChainMismatch,
        Code::TreeMismatch,
        Code::PurchaseInvalid,
        Code::StoreFormat,
        Code::StoreIncomplete,
        Code::AnchorInvalid,
        Code::InputRange,
        Code::FundingShort,
        Code::WalletScript,
        Code::CoinUnconfirmed,
        Code::Overflow,
        Code::Refused,
        Code::Policy,
        Code::Encode,
        Code::Mass,
        Code::Engine,
        Code::Templates,
        Code::FeeRate,
        Code::SignedChanged,
        Code::SignedMissing,
        Code::SignatureForm,
        Code::SignatureInvalid,
    ];

    /// The stable number (1-based, in `ALL` order).
    pub fn number(self) -> u16 {
        Self::ALL.iter().position(|&c| c == self).map(|i| i as u16 + 1).unwrap_or(0)
    }

    /// The stable name (snake case), for logs and the generated TypeScript type.
    pub fn as_str(self) -> &'static str {
        match self {
            Code::BytesLength => "bytes_length",
            Code::NotTemplate => "not_template",
            Code::NotCanonical => "not_canonical",
            Code::StateRange => "state_range",
            Code::ChainMismatch => "chain_mismatch",
            Code::TreeMismatch => "tree_mismatch",
            Code::PurchaseInvalid => "purchase_invalid",
            Code::StoreFormat => "store_format",
            Code::StoreIncomplete => "store_incomplete",
            Code::AnchorInvalid => "anchor_invalid",
            Code::InputRange => "input_range",
            Code::FundingShort => "funding_short",
            Code::WalletScript => "wallet_script",
            Code::CoinUnconfirmed => "coin_unconfirmed",
            Code::Overflow => "overflow",
            Code::Refused => "refused",
            Code::Policy => "policy",
            Code::Encode => "encode",
            Code::Mass => "mass",
            Code::Engine => "engine",
            Code::Templates => "templates",
            Code::FeeRate => "fee_rate",
            Code::SignedChanged => "signed_changed",
            Code::SignedMissing => "signed_missing",
            Code::SignatureForm => "signature_form",
            Code::SignatureInvalid => "signature_invalid",
        }
    }

    /// The one message of the code (what the page may show).
    pub fn message(self) -> &'static str {
        match self {
            Code::BytesLength => "The data has the wrong length.",
            Code::NotTemplate => "The data is not a lottery object of the expected kind.",
            Code::NotCanonical => "The data is not in its canonical encoding.",
            Code::StateRange => "A lottery object holds a value its covenant does not accept.",
            Code::ChainMismatch => "The indexer's copy does not match the chain.",
            Code::TreeMismatch => "The purchases do not match the chain's frozen record.",
            Code::PurchaseInvalid => "A purchase is not a valid combination for this round.",
            Code::StoreFormat => "The file is not a valid lottery file.",
            Code::StoreIncomplete => "The saved data lacks what this transaction needs.",
            Code::AnchorInvalid => "The anchor proof does not fit this round.",
            Code::InputRange => "A value is outside its allowed range.",
            Code::FundingShort => "The wallet's coins do not cover this transaction.",
            Code::WalletScript => "A wallet coin or address is not a standard wallet script.",
            Code::CoinUnconfirmed => "A wallet coin is not confirmed yet.",
            Code::Overflow => "An amount is out of range.",
            Code::Refused => "The lottery rules refuse this transaction.",
            Code::Policy => "A safety rule of the builder refuses this transaction.",
            Code::Encode => "The transaction could not be encoded.",
            Code::Mass => "The transaction's mass cannot be computed.",
            Code::Engine => "The local script check rejects this transaction.",
            Code::Templates => "The built-in lottery templates are unavailable.",
            Code::FeeRate => "The fee is outside the accepted range.",
            Code::SignedChanged => "The wallet changed the transaction.",
            Code::SignedMissing => "The wallet did not sign every input it was asked to.",
            Code::SignatureForm => "A signature is not in the expected form.",
            Code::SignatureInvalid => "A signature does not verify.",
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl serde::Serialize for Code {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// An error of the web-facing layer: a code and the detail behind it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub code: Code,
    /// What exactly failed (for logs and developers; the page shows `code.message()`).
    pub detail: String,
}

impl Error {
    pub fn new(code: Code, detail: impl Into<String>) -> Self {
        Self { code, detail: detail.into() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}): {}", self.code.as_str(), self.code.number(), self.detail)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Attach a code to a `String` error of the layers below.
pub(crate) trait WithCode<T> {
    fn code(self, c: Code) -> Result<T>;
}

impl<T> WithCode<T> for std::result::Result<T, String> {
    fn code(self, c: Code) -> Result<T> {
        self.map_err(|d| Error::new(c, d))
    }
}

/// `Err(Error)` unless `cond`.
pub(crate) fn ensure(cond: bool, code: Code, detail: impl FnOnce() -> String) -> Result<()> {
    if cond { Ok(()) } else { Err(Error::new(code, detail())) }
}
