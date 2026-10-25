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
//! - `cases`: the same builders from chain objects (each covenant UTXO with the state bytes
//!   the indexer gives, checked against the UTXO), so a page never fills a `*Case` itself:
//!   the derived fields (reserves, the pot, lock times, trees) are lottery logic (F1 part C).
//! - `sign`: the signing split: the unsigned transaction with its budgets and storage mass
//!   committed, the inputs to sign, the masses and the fee (`Unsigned`); then the post-sign
//!   check (spec 6.2) and the final transaction from our own transaction plus the wallet's
//!   signature scripts for the listed inputs only (`assemble`).
//!
//! Errors are typed codes with one message each (spec 1.1 AX); `Error::detail` says what
//! exactly failed, for logs and developers, never for the page's copy.

pub mod build;
pub mod cases;
/// The fixture calls (tests and the dev build only: feature `fixtures`, F1 review).
#[cfg(feature = "fixtures")]
pub mod fixtures;
pub mod sign;
pub mod verify;

use std::fmt;

/// The code table: variant, number, name and message, written once. The macro gives the
/// enum, `ALL`, `number`, `as_str`, `message` and `TS_DECL` (the TypeScript union of the
/// names, emitted into the wasm crate's generated `.d.ts`, F1 part C), so no list can drift
/// from another. The number and the name never change meaning; new codes are appended.
macro_rules! codes {
    ($($(#[doc = $doc:literal])* $v:ident = $n:literal, $name:literal, $msg:literal;)*) => {
        /// A stable error code (spec 1.1 AX). The number and the name never change meaning;
        /// new codes are appended. `message()` is the one sentence the page may show.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Code {
            $($(#[doc = $doc])* $v,)*
        }

        impl Code {
            /// Every code, in number order.
            pub const ALL: &'static [Code] = &[$(Code::$v),*];

            /// The TypeScript declaration of the names' union (`export type ErrorName =
            /// "bytes_length" | ... | never;`), for the wasm crate's generated `.d.ts`.
            pub const TS_DECL: &'static str = concat!(
                "/** Every error code name (`lottery::web::Code`; number = position, 1-based). */\nexport type ErrorName = ",
                $("\"", $name, "\" | ",)*
                "never;\n"
            );

            /// The stable number (1-based).
            pub fn number(self) -> u16 {
                match self { $(Code::$v => $n,)* }
            }

            /// The stable name (snake case), for logs and the generated TypeScript type.
            pub fn as_str(self) -> &'static str {
                match self { $(Code::$v => $name,)* }
            }

            /// The one message of the code (what the page may show).
            pub fn message(self) -> &'static str {
                match self { $(Code::$v => $msg,)* }
            }

            /// The code of a name (`as_str`'s inverse).
            pub fn from_name(s: &str) -> Option<Code> {
                match s { $($name => Some(Code::$v),)* _ => None }
            }
        }
    };
}

codes! {
    /// Input bytes or a field have the wrong length.
    BytesLength = 1, "bytes_length", "The data has the wrong length.";
    /// The bytes are not a state (or script) of the expected template.
    NotTemplate = 2, "not_template", "The data is not a lottery object of the expected kind.";
    /// A decoded state does not re-encode to the same bytes.
    NotCanonical = 3, "not_canonical", "The data is not in its canonical encoding.";
    /// A state field is outside the range its covenant accepts.
    StateRange = 4, "state_range", "A lottery object holds a value its covenant does not accept.";
    /// A state or script given by the indexer does not match the chain's UTXO (spec 6.3).
    ChainMismatch = 5, "chain_mismatch", "The indexer's copy does not match the chain.";
    /// A tree rebuilt from purchases does not give the chain's frozen root (spec 6.3).
    TreeMismatch = 6, "tree_mismatch", "The purchases do not match the chain's frozen record.";
    /// A purchase is not a canonical combination of the round, or a count overflows.
    PurchaseInvalid = 7, "purchase_invalid", "A purchase is not a valid combination for this round.";
    /// A store or backup file is malformed (format, hex, numbers above 2^53, fields).
    StoreFormat = 8, "store_format", "The file is not a valid lottery file.";
    /// A store lacks what the transaction needs (a ticket, a pool, a closed round).
    StoreIncomplete = 9, "store_incomplete", "The saved data lacks what this transaction needs.";
    /// The anchor proof is malformed or does not anchor the round's reference D.
    AnchorInvalid = 10, "anchor_invalid", "The anchor proof does not fit this round.";
    /// A build input is outside the range its builder accepts.
    InputRange = 11, "input_range", "A value is outside its allowed range.";
    /// The wallet's coins do not cover the transaction.
    FundingShort = 12, "funding_short", "The wallet's coins do not cover this transaction.";
    /// A wallet coin or the change address is not a standard script of the wallet.
    WalletScript = 13, "wallet_script", "A wallet coin or address is not a standard wallet script.";
    /// A wallet coin is not confirmed yet (no creation DAA score).
    CoinUnconfirmed = 14, "coin_unconfirmed", "A wallet coin is not confirmed yet.";
    /// An amount is outside the 64-bit range.
    Overflow = 15, "overflow", "An amount is out of range.";
    /// The lottery rules (the Rust reference of the covenant) refuse this transaction.
    Refused = 16, "refused", "The lottery rules refuse this transaction.";
    /// A builder safety rule refuses this transaction (D24 stale close, D86 wmin, the
    /// minimum caller output).
    Policy = 17, "policy", "A safety rule of the builder refuses this transaction.";
    /// A script could not be encoded or budgeted.
    Encode = 18, "encode", "The transaction could not be encoded.";
    /// The transaction's mass cannot be computed.
    Mass = 19, "mass", "The transaction's mass cannot be computed.";
    /// The local script engine rejects a covenant input before signing.
    Engine = 20, "engine", "The local script check rejects this transaction.";
    /// The embedded templates are unavailable.
    Templates = 21, "templates", "The built-in lottery templates are unavailable.";
    /// The fee rate or fee is outside the accepted range.
    FeeRate = 22, "fee_rate", "The fee is outside the accepted range.";
    /// The wallet changed the transaction (spec 6.2).
    SignedChanged = 23, "signed_changed", "The wallet changed the transaction.";
    /// The wallet left an input it was asked to sign unsigned.
    SignedMissing = 24, "signed_missing", "The wallet did not sign every input it was asked to.";
    /// A signature script is not in the form its input needs (SIG_HASH_ALL, sizes).
    SignatureForm = 25, "signature_form", "A signature is not in the expected form.";
    /// A returned signature does not verify against our transaction.
    SignatureInvalid = 26, "signature_invalid", "A signature does not verify.";
    /// An address is not of the network the page runs on (testnet-10's `kaspatest:`; spec
    /// 6.6). F1 part C.
    AddressNetwork = 27, "address_network", "The address is not on this site's network.";
    /// A value crossing into the library is malformed (hex, decimal, JSON shape). F1 part C.
    InputFormat = 28, "input_format", "A value is not in the expected format.";
    /// The library was called before `start` (F1 part C).
    NotStarted = 29, "not_started", "The lottery code has not been started.";
    /// A `mainnet` build refuses to start (spec 6.6, W4). F1 part C.
    MainnetRefused = 30, "mainnet_refused", "This build refuses to run on mainnet.";
    /// A built transaction's figures (fee, ID, masses, units, sign list, summary) differ
    /// from what its transaction gives (spec 7: amounts come from the built transaction).
    /// F1 review.
    BuiltMismatch = 31, "built_mismatch", "The figures shown do not match the transaction.";
    /// A vault owner address of a type the page cannot withdraw for (its signers sign
    /// Schnorr P2PK only; D67 allows more at the covenant level). F1 review.
    OwnerType = 32, "owner_type", "Vault positions need a Schnorr wallet address.";
    /// The coins cover the transaction without change, but the change left over is too
    /// small to pay for its own output (its storage mass). F1 review.
    ChangeDust = 33, "change_dust", "The change left over is too small to send back.";
    /// The inputs to sign belong to more than one key: a page build has one payer, so every
    /// coin it signs is of one wallet address (F1 re-review N2: otherwise a summary could
    /// show a second key's output as coming back to the wallet).
    MixedSigners = 34, "mixed_signers", "All the coins to sign must come from one wallet address.";
    /// Front end F3: the public node (wRPC) could not be reached, or did not answer in time.
    NodeUnavailable = 35, "node_unavailable", "The network node cannot be reached.";
    /// Front end F3: the node reports another network than the one the page runs on, or has
    /// no UTXO index (spec 6.6: the page refuses it).
    NodeNetwork = 36, "node_network", "The network node is on another network.";
    /// Front end F3: the node answered with an error (for example, it rejected a transaction).
    NodeRefused = 37, "node_refused", "The network node refused the request.";
    /// Front end F3 (the page, not the wasm): the indexer API could not be reached or answered
    /// with an error status.
    IndexerUnavailable = 38, "indexer_unavailable", "The indexer cannot be reached.";
    /// Front end F3 (the page, not the wasm): the worker running the lottery code stopped (wasm
    /// aborts on a panic); the page restarted it.
    WorkerStopped = 39, "worker_stopped", "The lottery code stopped and was restarted.";
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
