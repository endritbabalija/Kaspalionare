//! Parsing and printing at the boundary: hex, decimal strings, outpoints, UTXO entries and
//! addresses. Every 64-bit integer crosses as a decimal string (JSON- and IndexedDB-safe; the
//! Safe JSON convention), bytes as lowercase hex. Nothing here panics on any input.

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash;
use lottery::state::Payout;
use lottery::web::build::payout_of_spk;
use lottery::web::{Code, Error, Result};

use crate::dto::{BlockHeader, CovUtxo, Utxo};

/// The longest hex string accepted (2 MiB of bytes; the decoders cap at 1 MiB).
const HEX_MAX: usize = 4 << 20;

fn fmt(what: &str, detail: impl std::fmt::Display) -> Error {
    Error::new(Code::InputFormat, format!("{what}: {detail}"))
}

/// Lowercase hex.
pub fn hex(b: &[u8]) -> String {
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(2 * b.len());
    for &x in b {
        s.push(D[(x >> 4) as usize] as char);
        s.push(D[(x & 15) as usize] as char);
    }
    s
}

/// Hex (either case, even length) to bytes.
pub fn unhex(what: &str, s: &str) -> Result<Vec<u8>> {
    if s.len() > HEX_MAX {
        return Err(Error::new(Code::BytesLength, format!("{what}: {} hex digits", s.len())));
    }
    if s.len() % 2 != 0 {
        return Err(fmt(what, "odd number of hex digits"));
    }
    let v = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    s.as_bytes().chunks_exact(2).map(|p| Some((v(p[0])? << 4) | v(p[1])?)).collect::<Option<Vec<u8>>>().ok_or_else(|| fmt(what, "not hex"))
}

/// Exactly 32 bytes of hex.
pub fn hash32(what: &str, s: &str) -> Result<[u8; 32]> {
    unhex(what, s)?.try_into().map_err(|v: Vec<u8>| Error::new(Code::BytesLength, format!("{what}: {} bytes, expected 32", v.len())))
}

/// A decimal u64 (digits only, no sign, no leading zeros except "0", at most u64::MAX).
pub fn dec(what: &str, s: &str) -> Result<u64> {
    let ok = !s.is_empty() && s.len() <= 20 && s.bytes().all(|c| c.is_ascii_digit()) && (s == "0" || !s.starts_with('0'));
    if !ok {
        return Err(fmt(what, format!("`{}` is not a decimal integer", s.chars().take(24).collect::<String>())));
    }
    s.parse::<u64>().map_err(|_| Error::new(Code::Overflow, format!("{what}: {s} above u64")))
}

/// A decimal i64 (as `dec`, with an optional leading '-').
pub fn dec_i64(what: &str, s: &str) -> Result<i64> {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, s),
    };
    let u = dec(what, digits)?;
    if neg {
        if u == 0 {
            return Err(fmt(what, "-0"));
        }
        0i64.checked_sub_unsigned(u).ok_or_else(|| Error::new(Code::Overflow, format!("{what}: {s} below i64")))
    } else {
        i64::try_from(u).map_err(|_| Error::new(Code::Overflow, format!("{what}: {s} above i64")))
    }
}

/// A script public key in the Safe JSON form (hex of the 2-byte big-endian version, then the
/// script; rusty-kaspa's own `ScriptPublicKey::from_str`).
pub fn spk(what: &str, s: &str) -> Result<ScriptPublicKey> {
    if s.len() > HEX_MAX {
        return Err(Error::new(Code::BytesLength, format!("{what}: {} hex digits", s.len())));
    }
    let b = unhex(what, s)?;
    if b.len() < 2 {
        return Err(fmt(what, "shorter than its 2-byte version"));
    }
    s.parse::<ScriptPublicKey>().map_err(|e| fmt(what, e))
}

/// `spk` printed back (the Safe JSON form).
pub fn spk_hex(s: &ScriptPublicKey) -> String {
    format!("{}{}", hex(&s.version().to_be_bytes()), hex(s.script()))
}

/// A UTXO's outpoint and entry.
pub fn utxo(u: &Utxo) -> Result<(TransactionOutpoint, UtxoEntry)> {
    let txid = Hash::from_bytes(hash32("utxo.transactionId", &u.transaction_id)?);
    let cov = match &u.covenant_id {
        None => None,
        Some(c) => Some(Hash::from_bytes(hash32("utxo.covenantId", c)?)),
    };
    let entry = UtxoEntry::new(dec("utxo.amount", &u.amount)?, spk("utxo.scriptPublicKey", &u.script_public_key)?, dec("utxo.blockDaaScore", &u.block_daa_score)?, u.is_coinbase, cov);
    Ok((TransactionOutpoint::new(txid, u.index), entry))
}

/// A UTXO printed back.
pub fn utxo_dto(op: &TransactionOutpoint, e: &UtxoEntry) -> Utxo {
    Utxo {
        transaction_id: hex(&op.transaction_id.as_bytes()),
        index: op.index,
        amount: e.amount.to_string(),
        script_public_key: spk_hex(&e.script_public_key),
        block_daa_score: e.block_daa_score.to_string(),
        is_coinbase: e.is_coinbase,
        covenant_id: e.covenant_id.map(|c| hex(&c.as_bytes())),
    }
}

/// A covenant object (its UTXO and the state bytes given for it).
pub fn obj(what: &str, c: &CovUtxo) -> Result<lottery::web::cases::Obj> {
    let (outpoint, entry) = utxo(&c.utxo)?;
    Ok(lottery::web::cases::Obj { outpoint, entry, state: unhex(what, &c.state)? })
}

/// A covenant object printed back.
pub fn obj_dto(o: &lottery::web::cases::Obj) -> CovUtxo {
    CovUtxo { utxo: utxo_dto(&o.outpoint, &o.entry), state: hex(&o.state) }
}

/// The most parents a header may list here: the consensus bound on a header's parents
/// (`consensus/core/src/header.rs:87`, 255 levels x 2,048) is far above any real header
/// (one direct-parent level of at most a few dozen, then fewer per level); this caps what
/// the page can make the worker parse.
const HEADER_PARENTS_MAX: usize = 255 * 64;

/// A node's block header (F1 re-review S1). Its `hash` field is taken as stated;
/// `lottery::anchor::BlockFields::check_header` compares it with the recomputed hash.
pub fn header(h: &BlockHeader) -> Result<kaspa_consensus_core::header::Header> {
    let n: usize = h.parents_by_level.iter().map(Vec::len).fold(0, usize::saturating_add);
    if h.parents_by_level.len() > 255 || n > HEADER_PARENTS_MAX {
        return Err(Error::new(Code::InputRange, format!("header.parentsByLevel: {} levels, {n} parents", h.parents_by_level.len())));
    }
    let hash = |what: &str, s: &str| hash32(what, s).map(Hash::from_bytes);
    let parents: Vec<Vec<Hash>> =
        h.parents_by_level.iter().map(|l| l.iter().map(|p| hash("header.parentsByLevel", p)).collect::<Result<Vec<_>>>()).collect::<Result<_>>()?;
    let parents = parents.try_into().map_err(|e| fmt("header.parentsByLevel", e))?;
    let blue_work = kaspa_consensus_core::BlueWorkType::from_hex(&h.blue_work).map_err(|e| fmt("header.blueWork", e))?;
    Ok(kaspa_consensus_core::header::Header {
        hash: hash("header.hash", &h.hash)?,
        version: h.version,
        parents_by_level: parents,
        hash_merkle_root: hash("header.hashMerkleRoot", &h.hash_merkle_root)?,
        accepted_id_merkle_root: hash("header.acceptedIdMerkleRoot", &h.accepted_id_merkle_root)?,
        utxo_commitment: hash("header.utxoCommitment", &h.utxo_commitment)?,
        timestamp: dec("header.timestamp", &h.timestamp)?,
        bits: h.bits,
        nonce: dec("header.nonce", &h.nonce)?,
        daa_score: dec("header.daaScore", &h.daa_score)?,
        blue_work,
        blue_score: dec("header.blueScore", &h.blue_score)?,
        pruning_point: hash("header.pruningPoint", &h.pruning_point)?,
    })
}

/// A header printed back (`blueWork` as 48 hex digits, as rusty-kaspa's serde writes it).
pub fn header_dto(h: &kaspa_consensus_core::header::Header) -> BlockHeader {
    let parents: Vec<Vec<Hash>> = (&h.parents_by_level).into();
    BlockHeader {
        hash: hex(&h.hash.as_bytes()),
        version: h.version,
        parents_by_level: parents.iter().map(|l| l.iter().map(|p| hex(&p.as_bytes())).collect()).collect(),
        hash_merkle_root: hex(&h.hash_merkle_root.as_bytes()),
        accepted_id_merkle_root: hex(&h.accepted_id_merkle_root.as_bytes()),
        utxo_commitment: hex(&h.utxo_commitment.as_bytes()),
        timestamp: h.timestamp.to_string(),
        bits: h.bits,
        nonce: h.nonce.to_string(),
        daa_score: h.daa_score.to_string(),
        blue_work: hex(&h.blue_work.to_be_bytes()),
        blue_score: h.blue_score.to_string(),
        pruning_point: hex(&h.pruning_point.as_bytes()),
    }
}

/// The networks the page may run on (CLAUDE.md Safety: testnet-10 and the private simnet,
/// never mainnet). The beta build knows testnet-10 only; the `simnet` feature adds the
/// simnet for the local dev run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Net {
    Testnet10,
    #[cfg(all(feature = "simnet", not(feature = "mainnet")))]
    Simnet,
}

impl Net {
    pub fn parse(s: &str) -> Result<Net> {
        match s {
            "testnet-10" => Ok(Net::Testnet10),
            #[cfg(all(feature = "simnet", not(feature = "mainnet")))]
            "simnet" => Ok(Net::Simnet),
            _ => Err(Error::new(Code::AddressNetwork, format!("network `{}` is not one this build runs on", s.chars().take(24).collect::<String>()))),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Net::Testnet10 => "testnet-10",
            #[cfg(all(feature = "simnet", not(feature = "mainnet")))]
            Net::Simnet => "simnet",
        }
    }

    pub fn prefix(self) -> Prefix {
        match self {
            Net::Testnet10 => Prefix::Testnet,
            #[cfg(all(feature = "simnet", not(feature = "mainnet")))]
            Net::Simnet => Prefix::Simnet,
        }
    }

    /// An address of this network (spec 6.6: anything else, `kaspa:` first of all, is
    /// refused) to its script.
    pub fn script_of(self, what: &str, s: &str) -> Result<ScriptPublicKey> {
        let want = format!("{}:", self.prefix());
        if !s.starts_with(&want) {
            return Err(Error::new(Code::AddressNetwork, format!("{what}: `{}…` is not a {want} address", s.chars().take(12).collect::<String>())));
        }
        let a = Address::try_from(s).map_err(|e| fmt(what, e))?;
        if a.prefix != self.prefix() {
            return Err(Error::new(Code::AddressNetwork, format!("{what}: not a {want} address")));
        }
        Ok(kaspa_txscript::pay_to_address_script(&a))
    }

    /// An address of this network to its standard payout script (D11: P2PK Schnorr, P2PK
    /// ECDSA or P2SH).
    pub fn payout_of(self, what: &str, s: &str) -> Result<Payout> {
        payout_of_spk(&self.script_of(what, s)?).ok_or_else(|| Error::new(Code::WalletScript, format!("{what}: not a standard address script")))
    }

    /// The address of a script on this network (None for a non-standard script).
    pub fn address(self, spk: &ScriptPublicKey) -> Option<String> {
        kaspa_txscript::extract_script_pub_key_address(spk, self.prefix()).ok().map(|a| a.to_string())
    }

    /// The address of a (class, 33-byte key) payout field pair (None unless D11).
    pub fn payout_address(self, class: u8, key: &[u8; 33]) -> Option<String> {
        (class <= 2 && (class == 1 || key[32] == 0)).then(|| self.address(&Payout::from_fields(class, key).spk())).flatten()
    }

    /// The address of a Schnorr x-only key (the quick wallet's).
    pub fn pubkey_address(self, xonly: &[u8; 32]) -> String {
        Address::new(self.prefix(), Version::PubKey, xonly).to_string()
    }
}
