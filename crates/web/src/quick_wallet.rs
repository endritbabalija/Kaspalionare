//! The quick testnet wallet's key hook (spec 6.2, S7; the wallet itself is F3). Compiled only
//! in a non-`mainnet` build: a `mainnet` build has none of these symbols (W4,
//! `tests/w4_mainnet.rs`). The key is a secp256k1 key from the browser's
//! `crypto.getRandomValues` (getrandom `wasm_js`); its address is a Schnorr P2PK address on the
//! page's network; it signs with rusty-kaspa v2.1.0's own signer (`consensus/core/src/sign.rs`
//! `sign_input`, SIG_HASH_ALL), only the listed inputs whose UTXO it owns. The page passes
//! the result to `assemble`, the same post-sign check as any wallet's.

#[cfg(feature = "mainnet")]
compile_error!("the quick wallet must not be compiled into a mainnet build (spec 6.6, W4)");

use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::sign::sign_input;
use kaspa_consensus_core::tx::PopulatedTransaction;
use lottery::web::{Code, Error, Result};

use crate::api::unsigned_of;
use crate::convert::{Net, hash32, hex};
use crate::dto::{KeyKind, QuickKey, QuickSignInput};
use crate::safe_json;

/// A marker string kept in the binary while the quick wallet is compiled in, so W4's scan has
/// a fixed name to look for besides the function names.
#[used]
static QUICK_WALLET_MARKER: [u8; 26] = *b"KASPALIONARE_QUICK_WALLET1";

fn keypair(sk: &[u8; 32]) -> Result<secp256k1::Keypair> {
    secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, sk).map_err(|_| Error::new(Code::InputFormat, "not a valid secret key"))
}

fn key_of(net: Net, sk: [u8; 32]) -> Result<QuickKey> {
    let kp = keypair(&sk)?;
    let (x, _) = kp.x_only_public_key();
    let xb = x.serialize();
    Ok(QuickKey { secret_key: hex(&sk), public_key: hex(&xb), address: net.pubkey_address(&xb) })
}

/// A new key from the platform's random source.
pub fn quick_wallet_new(net: Net) -> Result<QuickKey> {
    std::hint::black_box(&QUICK_WALLET_MARKER);
    for _ in 0..8 {
        let mut sk = [0u8; 32];
        getrandom::fill(&mut sk).map_err(|e| Error::new(Code::Encode, format!("no random source: {e}")))?;
        if let Ok(k) = key_of(net, sk) {
            return Ok(k);
        }
    }
    Err(Error::new(Code::Encode, "no valid key after 8 draws"))
}

/// The key of an exported secret key (import).
pub fn quick_wallet_import(net: Net, secret_key: &str) -> Result<QuickKey> {
    key_of(net, hash32("secretKey", secret_key)?)
}

/// Sign the listed inputs of `built` that this key owns (Schnorr P2PK of its x-only key),
/// SIG_HASH_ALL, and return the signed Safe JSON for `assemble`.
pub fn quick_wallet_sign(net: Net, i: &QuickSignInput) -> Result<String> {
    let sk = hash32("secretKey", &i.secret_key)?;
    let (x, _) = keypair(&sk)?.x_only_public_key();
    let xb = hex(&x.serialize());
    // The figures the page showed are checked against the transaction before signing.
    let u = unsigned_of(net, &i.built)?;
    let mine: Vec<usize> = i.built.sign.iter().filter(|s| s.kind == KeyKind::Schnorr && s.public_key == xb).map(|s| s.index as usize).collect();
    if mine.is_empty() {
        return Err(Error::new(Code::SignedMissing, "no listed input belongs to this key"));
    }
    let mut tx = u.tx.clone();
    for &n in &mine {
        let p = PopulatedTransaction::new(&u.tx, u.entries.clone());
        tx.inputs[n].signature_script = sign_input(&p, n, &sk, SIG_HASH_ALL);
    }
    tx.finalize();
    safe_json::reserialize(&tx, &u.entries, net.prefix()).map_err(|e| Error::new(Code::Encode, e))
}
