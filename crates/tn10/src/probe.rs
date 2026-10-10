//! Probe cases for the mass-limit checks (research G1–G3), submitted to a testnet-10 mempool.
//!
//! Covenant objects use a padded P2SH script (redeem `OP_DROP OP_TRUE`) whose signature
//! script is sized to M0's measured shard and controller sigscripts, with M0's committed
//! compute budgets. Storage, transient and compute mass therefore match M0's shapes; the
//! protocol logic itself is not under test here. Funding inputs are real Schnorr P2PK spends.

use std::time::{Duration, Instant};

use harness::{Masses, masses, priority_fee};
use kaspa_addresses::Address;
use kaspa_consensus_core::{
    hashing::sighash_type::SIG_HASH_ALL,
    sign::sign_input,
    subnets::SubnetworkId,
    tx::{
        CovenantBinding, GenesisCovenantGroup, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput,
        TransactionOutpoint, TransactionOutput, UtxoEntry,
    },
};
use kaspa_rpc_core::{RpcTransaction, api::rpc::RpcApi};
use kaspa_txscript::{pay_to_address_script, pay_to_script_hash_script, script_builder::ScriptBuilder};
use kaspa_wrpc_client::KaspaRpcClient;

pub const KAS: u64 = 100_000_000;
const C: u64 = 1_000_000_000_000;
const STORAGE_LIMIT: u64 = 500_000;
const PADDED_REDEEM: [u8; 2] = [0x75, 0x51]; // OP_DROP OP_TRUE
/// Schnorr P2PK signature script: OP_DATA_65 <sig || sighash type>.
const P2PK_SIG_LEN: usize = 66;
/// M0 shard sigscript (depth-23 Merkle update, feasibility Q2) and budgets.
const SHARD_SIG: usize = 3_418;
const SHARD_BUY_BUDGET: u16 = 6;

#[derive(Clone)]
pub struct Utxo {
    pub op: TransactionOutpoint,
    pub entry: UtxoEntry,
    pub padded: bool,
}

#[derive(Clone)]
pub struct In {
    pub utxo: Utxo,
    pub sig_len: usize,
    pub budget: u16,
}

pub struct Ctx {
    pub client: KaspaRpcClient,
    pub addr: Address,
    pub sk: [u8; 32],
    pub report: Vec<String>,
}

pub struct Sent {
    pub tx: Transaction,
    pub m: Masses,
    pub fee: u64,
}

pub fn padded_spk() -> ScriptPublicKey {
    pay_to_script_hash_script(&PADDED_REDEEM)
}

fn push_len(n: usize) -> usize {
    match n {
        0 => 1,
        1..=75 => 1 + n,
        76..=255 => 2 + n,
        256..=65_535 => 3 + n,
        _ => 5 + n,
    }
}

/// Signature script of exactly `len` bytes: push(filler) push(redeem).
fn padded_sigscript(len: usize) -> Vec<u8> {
    let redeem_push = push_len(PADDED_REDEEM.len());
    let mut f = len.saturating_sub(redeem_push + 1);
    while f > 0 && push_len(f) + redeem_push > len {
        f -= 1;
    }
    let mut sb = ScriptBuilder::new();
    sb.add_data(&vec![0xAB; f]).unwrap();
    sb.add_data(&PADDED_REDEEM).unwrap();
    let s = sb.drain();
    assert_eq!(s.len(), len, "padded sigscript length");
    s
}

/// Script units a padded input needs: one per pushed byte plus margin (runtime_resource_meter.rs).
pub fn padded_budget(sig_len: usize) -> u16 {
    harness::budget_for(sig_len as u64 + 2_000)
}

pub fn p2pk_in(u: &Utxo) -> In {
    In { utxo: u.clone(), sig_len: P2PK_SIG_LEN, budget: harness::budget_for(100_000) }
}

pub fn pad_in(u: &Utxo, sig_len: usize, budget: u16) -> In {
    In { utxo: u.clone(), sig_len, budget: budget.max(padded_budget(sig_len)) }
}

pub fn plain(v: u64, spk: &ScriptPublicKey) -> TransactionOutput {
    TransactionOutput::new(v, spk.clone())
}

pub fn padded_out(v: u64) -> TransactionOutput {
    TransactionOutput::new(v, padded_spk())
}

pub fn cont(v: u64, auth: u16, cov: kaspa_consensus_core::Hash) -> TransactionOutput {
    TransactionOutput::with_covenant(v, padded_spk(), Some(CovenantBinding::new(auth, cov)))
}

impl Ctx {
    pub fn spk(&self) -> ScriptPublicKey {
        pay_to_address_script(&self.addr)
    }

    /// Confirmed P2PK UTXOs of our address, largest first.
    pub async fn funds(&self) -> Result<Vec<Utxo>, String> {
        let mut v: Vec<Utxo> = crate::net::utxos(&self.client, &self.addr)
            .await?
            .into_iter()
            .map(|e| Utxo { op: e.outpoint.into(), entry: e.utxo_entry.into(), padded: false })
            .collect();
        v.sort_by_key(|u| std::cmp::Reverse(u.entry.amount));
        Ok(v)
    }

    fn assemble(&self, ins: &[In], outs: &[TransactionOutput], genesis: &[GenesisCovenantGroup], change: Option<u64>) -> Transaction {
        let inputs = ins
            .iter()
            .map(|i| {
                let sig = if i.utxo.padded { padded_sigscript(i.sig_len) } else { vec![0; P2PK_SIG_LEN] };
                TransactionInput::new_with_compute_budget(i.utxo.op, sig, 0, i.budget)
            })
            .collect();
        let mut outputs = outs.to_vec();
        if let Some(c) = change {
            outputs.push(plain(c, &self.spk()));
        }
        let mut tx = Transaction::new(1, inputs, outputs, 0, SubnetworkId::default(), 0, vec![]);
        if !genesis.is_empty() {
            tx.populate_genesis_covenants(genesis).expect("genesis groups");
        }
        tx.finalize();
        tx
    }

    /// Build with change to our address, fee at the minimum feerate over the priority mass
    /// (covers the relay floor too), sign every P2PK input, commit the storage mass.
    pub fn build(&self, ins: &[In], outs: &[TransactionOutput], genesis: &[GenesisCovenantGroup]) -> Result<Sent, String> {
        let entries: Vec<UtxoEntry> = ins.iter().map(|i| i.utxo.entry.clone()).collect();
        let total_in: u64 = entries.iter().map(|e| e.amount).sum();
        let total_out: u64 = outs.iter().map(|o| o.value).sum();
        let mut fee = 100_000;
        let mut tx = self.assemble(ins, outs, genesis, None);
        for _ in 0..4 {
            let change = total_in.checked_sub(total_out + fee).ok_or("insufficient input value")?;
            tx = self.assemble(ins, outs, genesis, if change > 0 { Some(change) } else { None });
            let m = masses(&tx, &entries);
            let need = priority_fee(&m).max(harness_min_relay(&m)) + 10_000;
            if need <= fee {
                break;
            }
            fee = need;
        }
        let m = masses(&tx, &entries);
        tx.set_storage_mass(m.storage);
        for (idx, i) in ins.iter().enumerate() {
            if !i.utxo.padded {
                let sig = sign_input(&PopulatedTransaction::new(&tx, entries.clone()), idx, &self.sk, SIG_HASH_ALL);
                tx.inputs[idx].signature_script = sig;
            }
        }
        tx.finalize();
        let fee = total_in - tx_out_total(&tx);
        Ok(Sent { tx, m, fee })
    }

    /// Submit; report accept/reject and, if accepted, seconds until the change output (last) is in the UTXO set.
    pub async fn submit(&mut self, case: &str, s: &Sent, wait: bool) -> Result<bool, String> {
        let id = s.tx.id();
        let res = self.client.submit_transaction(RpcTransaction::from(&s.tx), false).await;
        let (ok, detail) = match res {
            Ok(_) => {
                let mined = if wait { self.wait_mined(&s.tx).await } else { None };
                (true, match mined {
                    Some(t) => format!("accepted; output visible after {t:.1} s"),
                    None if wait => "accepted; not seen in the UTXO set within 120 s".into(),
                    None => "accepted".into(),
                })
            }
            Err(e) => (false, format!("REJECTED: {e}")),
        };
        let row = format!(
            "| {case} | {} | {} | {} | {} | {} | {:.4} | {detail} | `{id}` |",
            s.m.compute,
            s.m.transient,
            s.m.storage,
            s.m.priority,
            s.tx.inputs.len(),
            s.fee as f64 / KAS as f64
        );
        println!("{row}");
        self.report.push(row);
        Ok(ok)
    }

    /// Poll until the last output of `tx` (our change) appears in our address's UTXO set.
    async fn wait_mined(&self, tx: &Transaction) -> Option<f64> {
        let start = Instant::now();
        let target = TransactionOutpoint::new(tx.id(), (tx.outputs.len() - 1) as u32);
        let our = tx.outputs.last().map(|o| o.script_public_key == self.spk()).unwrap_or(false);
        if !our {
            return None;
        }
        while start.elapsed() < Duration::from_secs(120) {
            if let Ok(v) = crate::net::utxos(&self.client, &self.addr).await {
                if v.iter().any(|e| TransactionOutpoint::from(e.outpoint.clone()) == target) {
                    return Some(start.elapsed().as_secs_f64());
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        None
    }

    /// Spend every listed output back to our address in one transaction and wait for it.
    pub async fn sweep(&mut self, utxos: Vec<Utxo>) -> Result<(), String> {
        if utxos.is_empty() {
            return Ok(());
        }
        let ins: Vec<In> = utxos.iter().map(|u| if u.padded { pad_in(u, 4, 0) } else { p2pk_in(u) }).collect();
        let s = self.build(&ins, &[], &[])?;
        self.client.submit_transaction(RpcTransaction::from(&s.tx), false).await.map_err(|e| format!("sweep: {e}"))?;
        self.wait_mined(&s.tx).await.ok_or("sweep not mined within 120 s")?;
        Ok(())
    }
}

fn tx_out_total(tx: &Transaction) -> u64 {
    tx.outputs.iter().map(|o| o.value).sum()
}

fn harness_min_relay(m: &Masses) -> u64 {
    m.compute.max(m.transient.div_ceil(2)) * 100
}

/// Outputs of a sent transaction as spendable UTXOs (unconfirmed; DAA score unknown -> 0).
pub fn outs_of(s: &Sent) -> Vec<Utxo> {
    s.tx.outputs
        .iter()
        .enumerate()
        .map(|(i, o)| Utxo {
            op: TransactionOutpoint::new(s.tx.id(), i as u32),
            entry: UtxoEntry::new(o.value, o.script_public_key.clone(), 0, false, o.covenant.map(|c| c.covenant_id)),
            padded: o.script_public_key == padded_spk(),
        })
        .collect()
}

/// Value of a plain output whose storage mass alone is about `target` (C / v).
pub fn value_for_storage(target: u64) -> u64 {
    C / target
}

/// M0 controller probe at K (feasibility Q5 table; costs.md): sigscript bytes and budget.
pub fn controller_sig(k: usize) -> (usize, u16) {
    let redeem = 190 + 420 * k; // 6,910 B at K = 16 (measured 6,908), 13,630 at 32, 27,070 at 64
    let units = 1_550 + 3_840 * k as u64; // about 62,990 at 16 (measured 62,928), 124,430 at 32
    (redeem + 4, harness::budget_for(units))
}

/// M0 minimum R for K (q4_reserve_r_by_k / closed form).
pub fn r_min(k: usize) -> u64 {
    let k = k as u64;
    (4 * C * (k + 2) / STORAGE_LIMIT).max(4 * C * (k + 1) / (STORAGE_LIMIT - 10_101)) + 1
}

pub const SHARD_SIG_LEN: usize = SHARD_SIG;
pub const SHARD_BUDGET: u16 = SHARD_BUY_BUDGET;

/// Fee for a lottery-built transaction: the minimum feerate over its priority mass (also
/// covers the relay floor), plus 0.0001 KAS.
pub fn lottery_fee(f: &lottery::buy::Final) -> u64 {
    let m = harness::masses(&f.tx, &f.entries);
    harness::priority_fee(&m).max(m.compute.max(m.transient.div_ceil(2)) * 100) + 10_000
}

pub fn sent_of(f: &lottery::buy::Final) -> Sent {
    let mut tx = f.tx.clone();
    tx.finalize();
    let m = harness::masses(&tx, &f.entries);
    Sent { fee: f.fee(), tx, m }
}
