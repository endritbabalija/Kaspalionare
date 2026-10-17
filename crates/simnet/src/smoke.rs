//! `smoke`: start a fresh private simnet node, mine past coinbase maturity, consolidate
//! coinbase outputs, then genesis -> buy() -> close() of round 1 with the **production**
//! templates (`Templates::get`, no test-only variant), each built by the `lottery` builder,
//! checked in the local v2.1.0 engine, submitted with `submit_transaction` and mined from the
//! node's own templates. The sequence follows `tn10/src/cases.rs` `round1_drawn_with`
//! (K = 16, J(1) = 10 KAS, V0 = 60 KAS, T = genesis DAA + 600).

use std::time::Instant;

use kaspa_consensus_core::tx::{Transaction, TransactionOutpoint as Op, UtxoEntry};
use kaspa_rpc_core::{RpcTransaction, api::rpc::RpcApi};
use lottery::buy::{BuyCase, Final, Templates, b_max};
use lottery::close::{CloseCase, FundingIn, ShardIn};
use lottery::combo::{Combo, unrank};
use lottery::genesis::{GenesisCase, NetworkType, covenant_id};
use lottery::smt::Tree;
use lottery::state::Payout;

use crate::miner::{mine, mine_one, mine_past_daa, mine_until_accepted};
use crate::node::Node;
use crate::wallet::{COINBASE_MATURITY, KAS, Utxo, Wallet};

fn lottery_fee(f: &Final) -> u64 {
    let m = harness::masses(&f.tx, &f.entries);
    harness::priority_fee(&m).max(m.compute.max(m.transient.div_ceil(2)) * 100) + 10_000
}

fn finalized(f: &Final) -> Transaction {
    let mut tx = f.tx.clone();
    tx.finalize();
    tx
}

/// Submit `tx` (orphans not allowed), mine until a chain block accepts it; returns the
/// accepting block's DAA score.
async fn submit_and_accept<C: RpcApi + ?Sized>(c: &C, w: &Wallet, what: &str, tx: &Transaction, entries: &[UtxoEntry]) -> Result<u64, String> {
    let m = harness::masses(tx, entries);
    let start = c.get_block_dag_info().await.map_err(|e| e.to_string())?.sink;
    c.submit_transaction(RpcTransaction::from(tx), false).await.map_err(|e| format!("{what}: REJECTED by the node: {e}"))?;
    let (blk, daa) = mine_until_accepted(c, &w.addr, start, tx.id(), 5).await.map_err(|e| format!("{what}: {e}"))?;
    let out_in: u64 = entries.iter().map(|e| e.amount).sum::<u64>() - tx.outputs.iter().map(|o| o.value).sum::<u64>();
    println!(
        "ACCEPTED {what}: tx {} | inputs {} outputs {} | compute {} transient {} storage {} | fee {:.4} KAS | accepted by chain block {blk} at DAA {daa}",
        tx.id(),
        tx.inputs.len(),
        tx.outputs.len(),
        m.compute,
        m.transient,
        m.storage,
        out_in as f64 / KAS as f64
    );
    Ok(daa)
}

pub async fn run(blocks: u64) -> Result<(), String> {
    let node = Node::start()?;
    let r = run_on(&node, blocks).await;
    node.shutdown();
    r
}

async fn run_on(node: &Node, blocks: u64) -> Result<(), String> {
    let c = node.client().await?;
    let w = Wallet::random();
    println!("miner and lottery address (throwaway key, this run only): {}", w.addr);

    // ---- mine past coinbase maturity
    let t0 = Instant::now();
    let warm = blocks.max(COINBASE_MATURITY + 60);
    let last = mine(&c, &w.addr, warm).await?.ok_or("no block mined")?;
    let secs = t0.elapsed().as_secs_f64();
    println!("mined {warm} blocks in {secs:.1} s ({:.0} blocks/s); last {} at DAA {}, timestamp {}, {} txs", warm as f64 / secs, last.hash, last.daa, last.timestamp, last.txs);
    let dag = c.get_block_dag_info().await.map_err(|e| e.to_string())?;
    println!("DAG: block count {}, tips {}, virtual DAA {}", dag.block_count, dag.tip_hashes.len(), dag.virtual_daa_score);
    if dag.tip_hashes.len() != 1 {
        return Err(format!("expected a linear chain (1 tip), got {}", dag.tip_hashes.len()));
    }

    // ---- consolidate 40 mature coinbase outputs (50 KAS each: 50e9 / BPS sompi, `bps.rs:135-137`) into one
    let mut mature = w.spendable(&c, dag.virtual_daa_score).await?;
    println!("spendable outputs: {} ({:.2} KAS)", mature.len(), mature.iter().map(|u| u.entry.amount).sum::<u64>() as f64 / KAS as f64);
    mature.truncate(40);
    let ctx_ = w.consolidate(&mature)?;
    let entries: Vec<UtxoEntry> = mature.iter().map(|u| u.entry.clone()).collect();
    let cdaa = submit_and_accept(&c, &w, "consolidation (40 coinbase P2PK inputs)", &ctx_, &entries).await?;
    let fund = w.utxos(&c).await?.into_iter().find(|u| u.op == Op::new(ctx_.id(), 0)).ok_or("consolidated output not in the UTXO index")?;
    if fund.entry.block_daa_score != cdaa {
        return Err(format!("funding entry DAA {} != accepting DAA {cdaa}", fund.entry.block_daa_score));
    }

    // ---- genesis, production templates, network = simnet
    const K: usize = 16;
    let t = Templates::get(b_max());
    if t.short_void_window {
        return Err("the production templates must not be the short-void variant".into());
    }
    let me = Payout::Schnorr(w.xonly());
    let vdaa = c.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    let deadline = vdaa as i64 + 600;
    let mut g = GenesisCase::new_with(K, vec![(fund.op, fund.entry.clone())], &t);
    g.network = NetworkType::Simnet;
    g.funding_sk = w.sk;
    g.set_owner(&me);
    g.set_jackpot(10 * KAS as i64);
    g.set_vault(60 * KAS as i64);
    g.controller.deadline = deadline;
    for s in &mut g.shards {
        s.deadline = deadline;
    }
    g.validate_with(&t).map_err(|e| format!("genesis refused: {e}"))?;
    let mut gf = g.build_with(t.clone()).finish();
    g.fee = lottery_fee(&gf);
    gf = g.build_with(t.clone()).finish();
    gf.run().map_err(|(i, e)| format!("local engine rejects genesis at input {i}: {e:?}"))?;
    let cov = covenant_id(&gf);
    let gtx = finalized(&gf);
    let gdaa = submit_and_accept(
        &c,
        &w,
        &format!("genesis K = {K}, production templates, R = {:.2} KAS, network simnet", g.reserve() as f64 / KAS as f64),
        &gtx,
        &gf.entries,
    )
    .await?;
    let gid = gtx.id();
    let gn = gtx.outputs.len() - 1;
    let gchange = w.utxos(&c).await?.into_iter().find(|u| u.op == Op::new(gid, gn as u32)).ok_or("genesis change not in the UTXO index")?;
    println!("covenant id {cov}; genesis {gid} at DAA {gdaa}; deadline T {deadline}");

    // ---- buy(): one fixed combination on shard 0
    let m = g.controller.m;
    let mut bc = BuyCase::new(t.b_max).with_tree(Tree::new(m as usize));
    bc.shard = g.shards[0].clone();
    bc.reserve = g.reserve();
    bc.shard_daa = gdaa;
    bc.cov = cov;
    bc.bundle = vec![Combo::new(unrank(0), 1)];
    bc.payout = me;
    bc.funding = vec![gchange.entry.amount];
    bc.funding_sk = w.sk;
    let bops = vec![Op::new(gid, 2), gchange.op];
    let buy_fin = |bc: &BuyCase| {
        let mut b = bc.build_with(t.clone());
        b.outpoints = bops.clone();
        b.finish()
    };
    bc.fee = lottery_fee(&buy_fin(&bc));
    let bf = buy_fin(&bc);
    bf.run().map_err(|(i, e)| format!("local engine rejects buy at input {i}: {e:?}"))?;
    let btx = finalized(&bf);
    let bdaa = submit_and_accept(&c, &w, &format!("buy() of {:?} + 1 on shard 0", unrank(0)), &btx, &bf.entries).await?;
    let bid = btx.id();
    let bn = btx.outputs.len() - 1;
    let bchange: Utxo = w.utxos(&c).await?.into_iter().find(|u| u.op == Op::new(bid, bn as u32)).ok_or("buy change not in the UTXO index")?;

    // ---- close(), K = 16, once T is final (mine past it)
    let v = mine_past_daa(&c, &w.addr, deadline as u64 + 5).await?;
    println!("mined past the deadline: virtual DAA {v} > T {deadline}");
    let mut cc = CloseCase::new_with(K, &t);
    cc.controller = g.controller.clone();
    cc.controller_daa = gdaa;
    cc.pot = g.pot;
    cc.params = g.params.clone();
    cc.params_daa = gdaa;
    cc.shards = g
        .shards
        .iter()
        .enumerate()
        .map(|(i, s)| if i == 0 { ShardIn { state: bc.successor(), daa: bdaa } } else { ShardIn { state: s.clone(), daa: gdaa } })
        .collect();
    cc.funding = vec![FundingIn { value: bchange.entry.amount, daa: bdaa }];
    cc.funder = me;
    cc.funding_sk = w.sk;
    cc.cov = cov;
    cc.lock_time = deadline as u64;
    let mut cops = vec![Op::new(gid, 0), Op::new(gid, 1), Op::new(bid, 0)];
    cops.extend((3..=K as u32 + 1).map(|i| Op::new(gid, i)));
    cops.push(bchange.op);
    let close_fin = |cc: &CloseCase| {
        let mut b = cc.build_with(t.clone());
        b.outpoints = cops.clone();
        b.finish()
    };
    cc.fee = lottery_fee(&close_fin(&cc));
    let cf = close_fin(&cc);
    cf.run().map_err(|(i, e)| format!("local engine rejects close at input {i}: {e:?}"))?;
    let x = cc.expected();
    let ctx_close = finalized(&cf);
    let close_daa = submit_and_accept(&c, &w, &format!("close() K = {K}, one sale"), &ctx_close, &cf.entries).await?;
    let d_ref = lottery::game::anchor_daa(close_daa as i64, x.controller.seed_offset);
    // The marker (output K + 2) must be in the node's UTXO set with the close's script.
    let marker_op = Op::new(ctx_close.id(), K as u32 + 2);
    let marker_spk = ctx_close.outputs[K + 2].script_public_key.clone();
    let marker_addr = kaspa_txscript::extract_script_pub_key_address(&marker_spk, kaspa_addresses::Prefix::Simnet).map_err(|e| e.to_string())?;
    crate::net::check_simnet(&marker_addr)?;
    let found = c.get_utxos_by_addresses(vec![marker_addr]).await.map_err(|e| e.to_string())?;
    let marker = found.iter().find(|e| Op::from(e.outpoint.clone()) == marker_op).ok_or("marker not in the node's UTXO index")?;
    println!(
        "close accepted: round 2 controller deadline {}, marker {marker_op} value {:.4} KAS at DAA {} (round 1 D = {d_ref})",
        x.controller.deadline,
        marker.utxo_entry.amount as f64 / KAS as f64,
        marker.utxo_entry.block_daa_score
    );
    // A few more blocks so the close is buried; then report.
    let tip = mine_one(&c, &w.addr).await?;
    println!("SMOKE OK: genesis, buy() and close() accepted by the private simnet node with the production templates (tip {} at DAA {})", tip.hash, tip.daa);
    let _ = c.disconnect().await;
    Ok(())
}
