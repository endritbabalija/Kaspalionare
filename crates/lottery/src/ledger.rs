//! A tiny UTXO set for tests (spec M2 D33): it rejects spending an outpoint that does not
//! exist or was already spent, so the late-buy cut-off of `close()` (CA3) is shown
//! structurally, the way consensus enforces it. It does not run scripts: callers run
//! `Final::run` (or `run_all`) and then `apply`.

use std::collections::HashMap;

use harness::{ScriptPublicKey, Transaction, TransactionId, TransactionOutpoint, UtxoEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerError {
    /// Input `index` spends an outpoint that is not in the set (never existed or spent).
    Missing { index: usize, outpoint: TransactionOutpoint },
    /// Input `index`'s claimed entry differs from the stored one.
    EntryMismatch { index: usize },
}

#[derive(Default, Clone)]
pub struct Ledger {
    utxos: HashMap<TransactionOutpoint, UtxoEntry>,
    next_fund: u32,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a synthetic UTXO (faucet funding or a fabricated fixture, D33) and return its
    /// outpoint.
    pub fn fund(&mut self, value: u64, spk: ScriptPublicKey, daa: u64) -> TransactionOutpoint {
        self.insert(UtxoEntry::new(value, spk, daa, false, None))
    }

    /// Insert any entry (e.g. a fabricated covenant UTXO) under a fresh synthetic outpoint.
    pub fn insert(&mut self, entry: UtxoEntry) -> TransactionOutpoint {
        let mut id = [0xf0u8; 32];
        id[..4].copy_from_slice(&self.next_fund.to_le_bytes());
        self.next_fund += 1;
        let op = TransactionOutpoint::new(TransactionId::from_bytes(id), 0);
        self.utxos.insert(op, entry);
        op
    }

    pub fn get(&self, op: &TransactionOutpoint) -> Option<&UtxoEntry> {
        self.utxos.get(op)
    }

    pub fn contains(&self, op: &TransactionOutpoint) -> bool {
        self.utxos.contains_key(op)
    }

    /// The stored entries of `tx`'s inputs, in order.
    pub fn entries(&self, tx: &Transaction) -> Result<Vec<UtxoEntry>, LedgerError> {
        tx.inputs
            .iter()
            .enumerate()
            .map(|(index, i)| {
                self.utxos.get(&i.previous_outpoint).cloned().ok_or(LedgerError::Missing { index, outpoint: i.previous_outpoint })
            })
            .collect()
    }

    /// Spend `tx`'s inputs and add its outputs, created at `daa`. `entries` are the entries
    /// the transaction was built and run against; they must match the stored ones.
    /// Returns the outpoints of the new outputs. Nothing changes on error.
    pub fn apply(&mut self, tx: &Transaction, entries: &[UtxoEntry], daa: u64) -> Result<Vec<TransactionOutpoint>, LedgerError> {
        let stored = self.entries(tx)?;
        for (index, (s, e)) in stored.iter().zip(entries).enumerate() {
            if s != e {
                return Err(LedgerError::EntryMismatch { index });
            }
        }
        for i in &tx.inputs {
            self.utxos.remove(&i.previous_outpoint);
        }
        let id = tx.id();
        Ok(tx
            .outputs
            .iter()
            .enumerate()
            .map(|(n, o)| {
                let op = TransactionOutpoint::new(id, n as u32);
                let cov = o.covenant.map(|c| c.covenant_id);
                self.utxos.insert(op, UtxoEntry::new(o.value, o.script_public_key.clone(), daa, false, cov));
                op
            })
            .collect())
    }
}
