//! Spec M8 A3/A4 (`tn10 bundle`, `tn10 bundle-verify <file>`, D116): the launch bundle.
//!
//! - `bundle` takes the campaign's genesis bundle (`tn10-data/<cov>/bundle-genesis.json`,
//!   written by C1 before genesis was submitted), refreshes its live UTXOs and snapshot from
//!   the index (after a poll), runs `lottery::bundle::verify`, and writes `bundle/1`.
//! - `bundle-verify <file>` runs `lottery::bundle::verify` (covenant ID and template hashes
//!   recomputed, every genesis check, each live UTXO's redeem script rebuilt from its role's
//!   template and its state bytes and hashed to its script), then compares every live UTXO
//!   in the file (outpoint, script, value, creation DAA, covenant ID) with the node's UTXO
//!   index; any mismatch is a non-zero exit.

use kaspa_consensus_core::Hash;
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint};
use lottery::bundle::{Bundle, BundleLive, Snapshot, verify};
use lottery::store::{self, IndexSnapshot};

use crate::campaign::Net;

type R<T> = Result<T, String>;

pub fn read(path: &std::path::Path) -> R<Bundle> {
    store::read(&std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?, store::BUNDLE)
}

/// Every live UTXO of `b` that the node does not hold as stated (missing, or another value,
/// creation DAA, covenant ID or script). `verify` has already checked that each stated
/// script is the P2SH of the redeem script rebuilt from its template and state bytes.
pub async fn compare_live<N: Net>(net: &mut N, b: &Bundle) -> R<Vec<String>> {
    let mut bad = vec![];
    for l in &b.live_utxos {
        let op = TransactionOutpoint::new(Hash::from_bytes(l.outpoint.txid), l.outpoint.index);
        let spk = ScriptPublicKey::from_vec(l.spk_version, l.spk.clone());
        match net.utxo(op, &spk).await? {
            None => bad.push(format!("{op}: not in the node's UTXO set")),
            Some(e) => {
                if e.amount != l.value || e.block_daa_score != l.daa || e.covenant_id.map(|c| c.as_bytes()) != Some(l.covenant_id) || e.script_public_key != spk {
                    bad.push(format!(
                        "{op}: node (value {}, DAA {}, cov {:?}) != bundle (value {}, DAA {}, cov {})",
                        e.amount,
                        e.block_daa_score,
                        e.covenant_id,
                        l.value,
                        l.daa,
                        store::to_hex(&l.covenant_id)
                    ));
                }
            }
        }
    }
    Ok(bad)
}

/// The genesis bundle with the index's live UTXOs and snapshot, verified.
pub fn refreshed(genesis: &Bundle, ix: &lottery::index::Rebuild, last: Hash, last_daa: u64) -> R<Bundle> {
    let mut b = genesis.clone();
    let t = ix.templates();
    b.live_utxos = IndexSnapshot::of(ix, Some(last)).live_utxos.iter().map(|l| BundleLive::of(l, t)).collect::<R<Vec<_>>>()?;
    b.snapshot = Some(Snapshot { daa: last_daa, chain_block: last.as_bytes() });
    verify(&b)?;
    Ok(b)
}

/// `bundle-verify`: local verification, then the node. Returns the report lines; `Err` on
/// any failure.
pub async fn verify_against<N: Net>(net: &mut N, b: &Bundle) -> R<Vec<String>> {
    let v = verify(b)?;
    let mut out = vec![format!(
        "local: covenant ID {} recomputed, {} templates = the local compilation, genesis checks pass, {} live UTXOs: redeem = template + state bytes, P2SH = script",
        v.cov,
        b.templates.len(),
        b.live_utxos.len()
    )];
    let bad = compare_live(net, b).await?;
    if !bad.is_empty() {
        return Err(format!("{} of {} live UTXOs differ from the node {}:\n  {}", bad.len(), b.live_utxos.len(), net.node(), bad.join("\n  ")));
    }
    out.push(format!("node {}: every live UTXO present with the stated script, value, creation DAA and covenant ID", net.node()));
    Ok(out)
}
