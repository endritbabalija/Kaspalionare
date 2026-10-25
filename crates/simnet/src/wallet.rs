//! A throwaway simnet key per run (never stored; the chain is fresh per run) and plain
//! Schnorr P2PK spends of its coinbase outputs.

use harness::{masses, priority_fee};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::{
    hashing::sighash_type::SIG_HASH_ALL,
    sign::sign_input,
    subnets::SubnetworkId,
    tx::{PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_txscript::pay_to_address_script;

use crate::net;

pub const KAS: u64 = 100_000_000;
/// Coinbase maturity on simnet (10 BPS x COINBASE_MATURITY_SECONDS 100 = 1,000 DAA;
/// `consensus/core/src/config/bps.rs:119-121`, `config/constants.rs:91`).
pub const COINBASE_MATURITY: u64 = 1_000;
/// Schnorr P2PK signature script: OP_DATA_65 <sig || sighash type>.
const P2PK_SIG_LEN: usize = 66;

#[derive(Clone)]
pub struct Utxo {
    pub op: TransactionOutpoint,
    pub entry: UtxoEntry,
}

pub struct Wallet {
    pub sk: [u8; 32],
    pub addr: Address,
}

impl Wallet {
    pub fn random() -> Wallet {
        let kp = secp256k1::Keypair::new(secp256k1::SECP256K1, &mut secp256k1::rand::thread_rng());
        let addr = Address::new(Prefix::Simnet, Version::PubKey, &kp.x_only_public_key().0.serialize());
        Wallet { sk: kp.secret_bytes(), addr }
    }

    pub fn spk(&self) -> ScriptPublicKey {
        pay_to_address_script(&self.addr)
    }

    pub fn xonly(&self) -> [u8; 32] {
        secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &self.sk).unwrap().x_only_public_key().0.serialize()
    }

    /// Our UTXOs (from the node's UTXO index), largest first.
    pub async fn utxos<C: RpcApi + ?Sized>(&self, c: &C) -> Result<Vec<Utxo>, String> {
        net::check_simnet(&self.addr)?;
        let mut v: Vec<Utxo> = c
            .get_utxos_by_addresses(vec![self.addr.clone()])
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|e| Utxo { op: e.outpoint.into(), entry: e.utxo_entry.into() })
            .collect();
        v.sort_by_key(|u| std::cmp::Reverse(u.entry.amount));
        Ok(v)
    }

    /// Coinbase outputs mature at virtual DAA `vdaa` (with a margin of 10), and plain outputs.
    pub async fn spendable<C: RpcApi + ?Sized>(&self, c: &C, vdaa: u64) -> Result<Vec<Utxo>, String> {
        Ok(self.utxos(c).await?.into_iter().filter(|u| !u.entry.is_coinbase || u.entry.block_daa_score + COINBASE_MATURITY + 10 < vdaa).collect())
    }

    /// Pay `amount` from `ins` (our P2PK) to `to`, the rest back to us (the dev faucet, F3),
    /// signed; the fee as `consolidate`'s.
    pub fn pay(&self, ins: &[Utxo], to: &ScriptPublicKey, amount: u64) -> Result<Transaction, String> {
        let entries: Vec<UtxoEntry> = ins.iter().map(|u| u.entry.clone()).collect();
        let total: u64 = entries.iter().map(|e| e.amount).sum();
        let mk = |fee: u64| -> Result<Transaction, String> {
            let inputs = ins.iter().map(|u| TransactionInput::new(u.op, vec![0; P2PK_SIG_LEN], 0, 1)).collect();
            let change = total.checked_sub(amount).and_then(|x| x.checked_sub(fee)).ok_or("insufficient value")?;
            let outs = vec![TransactionOutput::new(amount, to.clone()), TransactionOutput::new(change, self.spk())];
            let mut tx = Transaction::new(0, inputs, outs, 0, SubnetworkId::default(), 0, vec![]);
            tx.finalize();
            Ok(tx)
        };
        let mut fee = 100_000;
        let mut tx = mk(fee)?;
        for _ in 0..4 {
            let m = masses(&tx, &entries);
            let need = priority_fee(&m).max(m.compute.max(m.transient.div_ceil(2)) * 100) + 10_000;
            if need <= fee {
                break;
            }
            fee = need;
            tx = mk(fee)?;
        }
        let m = masses(&tx, &entries);
        tx.set_storage_mass(m.storage);
        for i in 0..ins.len() {
            tx.inputs[i].signature_script = sign_input(&PopulatedTransaction::new(&tx, entries.clone()), i, &self.sk, SIG_HASH_ALL);
        }
        tx.finalize();
        Ok(tx)
    }

    /// Spend `ins` (our P2PK) to one output of everything minus the fee (min feerate over
    /// the priority mass, also covering the relay floor), signed.
    pub fn consolidate(&self, ins: &[Utxo]) -> Result<Transaction, String> {
        let entries: Vec<UtxoEntry> = ins.iter().map(|u| u.entry.clone()).collect();
        let total: u64 = entries.iter().map(|e| e.amount).sum();
        let mk = |fee: u64| -> Result<Transaction, String> {
            let inputs = ins.iter().map(|u| TransactionInput::new(u.op, vec![0; P2PK_SIG_LEN], 0, 1)).collect();
            let out = TransactionOutput::new(total.checked_sub(fee).ok_or("insufficient value")?, self.spk());
            let mut tx = Transaction::new(0, inputs, vec![out], 0, SubnetworkId::default(), 0, vec![]);
            tx.finalize();
            Ok(tx)
        };
        let mut fee = 100_000;
        let mut tx = mk(fee)?;
        for _ in 0..4 {
            let m = masses(&tx, &entries);
            let need = priority_fee(&m).max(m.compute.max(m.transient.div_ceil(2)) * 100) + 10_000;
            if need <= fee {
                break;
            }
            fee = need;
            tx = mk(fee)?;
        }
        let m = masses(&tx, &entries);
        tx.set_storage_mass(m.storage);
        for i in 0..ins.len() {
            tx.inputs[i].signature_script = sign_input(&PopulatedTransaction::new(&tx, entries.clone()), i, &self.sk, SIG_HASH_ALL);
        }
        tx.finalize();
        Ok(tx)
    }
}
