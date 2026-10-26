//! SN1-SN5 (spec 8 A5b, D114): forced jackpot, duplicated jackpot and held second prize on
//! the private simnet, with the **production** templates (`Templates::get(b_max())`, no
//! test-only game or seed variant). One fresh node and one genesis per scenario.
//!
//! Every lottery transaction is built by the `lottery` builders, checked in the local v2.1.0
//! engine, submitted with `submit_transaction` and mined from the node's own templates. The
//! only block the runner changes is the anchor X: the grinder (`grind`) takes the template
//! whose DAA score is the first >= D (its selected parent, the sink, has DAA D - 1), varies
//! the nonce only, hashes the header locally with the consensus function
//! (`kaspa_consensus_core::hashing::header::hash_override_nonce_time`,
//! `refs/rusty-kaspa-v2.1.0/consensus/core/src/hashing/header.rs:5-35`; it covers nonce and
//! timestamp), maps the hash through the seed (`lottery::game::seed_numbers`, D42/D94) and
//! submits the first header whose numbers give the wanted tier to the chosen held
//! combinations. Simnet skips the proof-of-work check (`consensus/core/src/config/params.rs:737`).
//! The draw's 272-byte anchor proof is then rebuilt from RPC by `tn10/src/anchor.rs`
//! (`get_seq_commit_lane_proof`), unchanged.
//!
//! What this shows and what it does not: a real node's mempool and consensus accept the
//! shipping scripts' jackpot, duplicate and second-prize draws and claims; and a sole miner
//! picks X.hash for one header hash per try (D100). It is not a testnet or mainnet result
//! and not a mainnet cost.

use std::time::{Duration, Instant};

use kaspa_addresses::Prefix;
use kaspa_consensus_core::hashing::header::hash_override_nonce_time;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionOutpoint as Op, UtxoEntry};
use kaspa_hashes::Hash;
use kaspa_rpc_core::{RpcTransaction, SubmitBlockReport, api::rpc::RpcApi};
use lottery::buy::{BuyCase, Final, OwnerKey, Templates, b_max};
use lottery::claim::{ClaimCase, ClaimExpected};
use lottery::close::{CloseCase, FundingIn, ShardIn};
use lottery::combo::{Combo, unrank};
use lottery::draw::{DrawCase, DrawExpected, RoundResult};
use lottery::game::{self, ClaimMode, N, SeedNumbers, muldiv, seed_numbers, tier_of};
use lottery::genesis::{GenesisCase, NetworkType, covenant_id};
use lottery::smt::Tree;
use lottery::state::{Payout, RequestState, ShardState, TIER_JACKPOT, TIER_SECOND, TicketState};
use lottery::vault::{SettleCase, WithdrawCase};

use crate::miner::{mine, mine_one, mine_past_daa, mine_until_accepted};
use crate::node::Node;
use crate::wallet::{COINBASE_MATURITY, KAS, Utxo, Wallet};

const K: usize = 16;

pub(crate) fn kas(v: i64) -> String {
    format!("{:.8}", v as f64 / KAS as f64)
}

pub(crate) fn lottery_fee(f: &Final) -> u64 {
    let m = harness::masses(&f.tx, &f.entries);
    harness::priority_fee(&m).max(m.compute.max(m.transient.div_ceil(2)) * 100) + 10_000
}

pub(crate) fn finalized(f: &Final) -> Transaction {
    let mut tx = f.tx.clone();
    tx.finalize();
    tx
}

fn check(ok: bool, what: String) -> Result<(), String> {
    if ok {
        println!("CHECK ok: {what}");
        Ok(())
    } else {
        Err(format!("CHECK FAILED: {what}"))
    }
}

/// One scenario's log: transactions and checks, printed as the report.
#[derive(Default)]
pub struct Log {
    pub id: &'static str,
    pub txs: Vec<(String, String)>,
    pub checks: Vec<String>,
    pub grind: Option<Grind>,
}

impl Log {
    fn tx(&mut self, what: &str, id: impl std::fmt::Display) {
        self.txs.push((what.to_string(), id.to_string()));
    }
    fn ok(&mut self, ok: bool, what: String) -> Result<(), String> {
        check(ok, what.clone())?;
        self.checks.push(what);
        Ok(())
    }
}

/// Submit `tx`, mine until a chain block accepts it; returns (accepting DAA, sink before).
pub(crate) async fn submit_and_accept<C: RpcApi + ?Sized>(c: &C, w: &Wallet, what: &str, tx: &Transaction, entries: &[UtxoEntry]) -> Result<(u64, Hash), String> {
    let m = harness::masses(tx, entries);
    let start = c.get_block_dag_info().await.map_err(|e| e.to_string())?.sink;
    c.submit_transaction(RpcTransaction::from(tx), false).await.map_err(|e| format!("{what}: REJECTED by the node: {e}"))?;
    let (blk, daa) = mine_until_accepted(c, &w.addr, start, tx.id(), 5).await.map_err(|e| format!("{what}: {e}"))?;
    let fee: u64 = entries.iter().map(|e| e.amount).sum::<u64>() - tx.outputs.iter().map(|o| o.value).sum::<u64>();
    println!(
        "ACCEPTED {what}: tx {} | in {} out {} | compute {} transient {} storage {} | fee {:.4} KAS | chain block {blk} DAA {daa}",
        tx.id(),
        tx.inputs.len(),
        tx.outputs.len(),
        m.compute,
        m.transient,
        m.storage,
        fee as f64 / KAS as f64
    );
    Ok((daa, start))
}

/// The node's own UTXO entry of `op` (script `spk`), from the UTXO index.
pub(crate) async fn node_entry<C: RpcApi + ?Sized>(c: &C, op: Op, spk: &ScriptPublicKey) -> Result<UtxoEntry, String> {
    let addr = kaspa_txscript::extract_script_pub_key_address(spk, Prefix::Simnet).map_err(|e| e.to_string())?;
    crate::net::check_simnet(&addr)?;
    let v = c.get_utxos_by_addresses(vec![addr]).await.map_err(|e| e.to_string())?;
    v.into_iter().find(|e| Op::from(e.outpoint.clone()) == op).map(|e| e.utxo_entry.into()).ok_or(format!("{op} not in the node's UTXO index"))
}

/// The builder's covenant input entries must equal the node's (amount, script, DAA, covenant ID).
pub(crate) async fn same_entries<C: RpcApi + ?Sized>(c: &C, what: &str, ops: &[Op], built: &[UtxoEntry]) -> Result<(), String> {
    for (i, op) in ops.iter().enumerate() {
        let n = node_entry(c, *op, &built[i].script_public_key).await?;
        if n != built[i] {
            return Err(format!(
                "{what}: input {i} builder entry differs from the node's (amount {} / {}, daa {} / {}, cov {:?} / {:?})",
                built[i].amount, n.amount, built[i].block_daa_score, n.block_daa_score, built[i].covenant_id, n.covenant_id
            ));
        }
    }
    println!("{what}: {} covenant input entries = the node's", ops.len());
    Ok(())
}

pub(crate) async fn own_utxo<C: RpcApi + ?Sized>(c: &C, w: &Wallet, op: Op) -> Result<Utxo, String> {
    w.utxos(c).await?.into_iter().find(|u| u.op == op).ok_or(format!("{op} not in the UTXO index"))
}

// ------------------------------------------------------------------ grinder

/// One ground anchor block X (SN5's numbers).
#[derive(Clone, Debug)]
pub struct Grind {
    pub d: u64,
    pub m: i64,
    /// Header hashes evaluated, the template's own nonce included.
    pub tries: u64,
    pub secs: f64,
    /// Expected tries 1 / p for the scenario's target.
    pub expected: f64,
    pub target: String,
    pub x: Hash,
    pub nonce: u64,
    pub won: SeedNumbers,
    pub honest_nonce: u64,
    pub honest_x: Hash,
    pub honest: SeedNumbers,
    pub honest_note: String,
}

/// Mine until the sink's DAA score is `target` (linear chain: +1 per block).
async fn mine_to_sink_daa<C: RpcApi + ?Sized>(c: &C, w: &Wallet, target: u64) -> Result<(), String> {
    loop {
        let sink = c.get_block_dag_info().await.map_err(|e| e.to_string())?.sink;
        let daa = c.get_block(sink, false).await.map_err(|e| e.to_string())?.header.daa_score;
        if daa == target {
            return Ok(());
        }
        if daa > target {
            return Err(format!("sink DAA {daa} already past {target}"));
        }
        mine_one(c, &w.addr).await?;
    }
}

/// Grind the anchor X for D (see the module doc). `want` decides on the seed numbers;
/// `p` is its probability per try (for the expected count); `note` describes a result.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn grind<C: RpcApi + ?Sized>(
    c: &C,
    w: &Wallet,
    cov: &[u8; 32],
    round: i64,
    m: i64,
    d: u64,
    target: &str,
    p: f64,
    want: impl Fn(&SeedNumbers) -> bool,
    note: impl Fn(&SeedNumbers) -> String,
) -> Result<Grind, String> {
    mine_to_sink_daa(c, w, d - 1).await?;
    let sink = c.get_block_dag_info().await.map_err(|e| e.to_string())?.sink;
    let template = c.get_block_template(w.addr.clone(), vec![]).await.map_err(|e| format!("get_block_template: {e}"))?;
    let mut header: Header = (&template.block.header).try_into().map_err(|e| format!("template header: {e:?}"))?;
    if header.daa_score != d || header.direct_parents() != [sink] {
        return Err(format!("template DAA {} parents {:?}: expected DAA D = {d} on the sink {sink} (DAA D - 1)", header.daa_score, header.direct_parents()));
    }
    if template.block.transactions.len() != 1 {
        return Err(format!("anchor template carries {} transactions besides the coinbase; expected none", template.block.transactions.len() - 1));
    }
    let honest_nonce = header.nonce;
    let honest_x = hash_override_nonce_time(&header, honest_nonce, header.timestamp);
    if honest_x != header.hash {
        return Err(format!("local header hash {honest_x} != the template's {}", header.hash));
    }
    let honest = seed_numbers(cov, round, &honest_x.as_bytes(), m);
    let honest_note = note(&honest);
    println!("grind: D = {d}, template nonce {honest_nonce}, honest X {honest_x}: {honest_note}");
    let limit = (p.recip() * 40.0) as u64;
    let t0 = Instant::now();
    let mut tries = 1u64;
    let mut nonce = honest_nonce;
    let (mut x, mut won) = (honest_x, honest);
    if !want(&honest) {
        loop {
            nonce = nonce.wrapping_add(1);
            tries += 1;
            x = hash_override_nonce_time(&header, nonce, header.timestamp);
            won = seed_numbers(cov, round, &x.as_bytes(), m);
            if want(&won) {
                break;
            }
            if tries > limit {
                return Err(format!("grind: no hit within {tries} tries (expected {:.0})", p.recip()));
            }
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    // The block as submitted: the template with only the nonce changed.
    header.nonce = nonce;
    header.finalize();
    if header.hash != x {
        return Err(format!("finalized header hash {} != the ground hash {x}", header.hash));
    }
    let mut block = template.block;
    block.header.nonce = nonce;
    let r = c.submit_block(block, false).await.map_err(|e| format!("submit_block (ground X): {e}"))?;
    if r.report != SubmitBlockReport::Success {
        return Err(format!("submit_block (ground X {x}): {:?}", r.report));
    }
    let t1 = Instant::now();
    loop {
        let s = c.get_block_dag_info().await.map_err(|e| e.to_string())?.sink;
        if s == x {
            break;
        }
        if t1.elapsed() > Duration::from_secs(10) {
            return Err(format!("ground block {x} is not the sink after 10 s (sink {s})"));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let g = Grind {
        d,
        m,
        tries,
        secs,
        expected: p.recip(),
        target: target.to_string(),
        x,
        nonce,
        won,
        honest_nonce,
        honest_x,
        honest,
        honest_note,
    };
    println!(
        "GROUND X {x} (DAA {d}, nonce {nonce}): {} after {tries} tries in {secs:.3} s (expected {:.0} for {target}); win {:?}+{}",
        note(&won),
        g.expected,
        won.main,
        won.bonus
    );
    Ok(g)
}

// ------------------------------------------------------------------ one round

#[allow(dead_code)] // id: for readers of the scenario table
pub struct Cfg {
    pub id: &'static str,
    pub m: i64,
    pub v0: i64,
    pub jackpot: i64,
    pub j_boost: i64,
    pub pot: u64,
    pub g: Option<i64>,
    /// (shard, bundle) per buy, in order.
    pub buys: Vec<(usize, Vec<Combo>)>,
    /// Request the deployer position's full withdrawal in round 1 (SN4).
    pub withdraw_full: bool,
    /// A last change to the genesis case before it is validated (the dev run's short rounds;
    /// None in SN1-SN4).
    pub edit: Option<fn(&mut GenesisCase)>,
}

#[derive(Clone)]
pub struct Ticket {
    pub op: Op,
    pub state: TicketState,
    pub value: u64,
    pub daa: u64,
}

#[allow(dead_code)] // dtx, ddaa, grind: kept for callers that report them
pub struct Round {
    pub t: std::sync::Arc<Templates>,
    pub cov: Hash,
    pub g: GenesisCase,
    pub tickets: Vec<Ticket>,
    pub d: DrawCase,
    pub dx: DrawExpected,
    pub dtx: Transaction,
    pub ddaa: u64,
    pub rr: RoundResult,
    pub grind: Grind,
    /// The vault the draw spent: (state, total_assets).
    pub vault_in: lottery::state::VaultState,
    /// The withdrawal request (op, state, value, daa) if any.
    pub wreq: Option<(Op, RequestState, u64, u64)>,
    /// O before close (the genesis pot) and the marker's frozen values.
    pub marker: lottery::state::MarkerState,
    /// A plain UTXO of ours for the next step (the draw's caller output).
    pub fund: Utxo,
    /// The genesis transaction as built (for the launch bundle; the dev run).
    pub gf: Final,
}

pub(crate) async fn run_round<C: RpcApi + ?Sized>(
    c: &C,
    w: &Wallet,
    cfg: &Cfg,
    log: &mut Log,
    target: &str,
    p: f64,
    want: impl Fn(&SeedNumbers) -> bool,
    note: impl Fn(&SeedNumbers) -> String,
) -> Result<Round, String> {
    // ---- funds: mine past coinbase maturity, consolidate 40 coinbase outputs (2,000 KAS).
    let t0 = Instant::now();
    mine(c, &w.addr, COINBASE_MATURITY + 60).await?;
    let vdaa = c.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    let mut mature = w.spendable(c, vdaa).await?;
    mature.truncate(40);
    let ctx_ = w.consolidate(&mature)?;
    let entries: Vec<UtxoEntry> = mature.iter().map(|u| u.entry.clone()).collect();
    submit_and_accept(c, w, "consolidation (40 coinbase inputs)", &ctx_, &entries).await?;
    let fund = own_utxo(c, w, Op::new(ctx_.id(), 0)).await?;
    println!("funds ready in {:.1} s: {:.2} KAS", t0.elapsed().as_secs_f64(), fund.entry.amount as f64 / KAS as f64);

    // ---- genesis, production templates, network simnet
    let t = Templates::get(b_max());
    if t.short_void_window {
        return Err("the production templates must not be the short-void variant".into());
    }
    let me = Payout::Schnorr(w.xonly());
    let vdaa = c.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    let deadline = vdaa as i64 + if cfg.withdraw_full { 1_400 } else { 600 };
    let mut g = GenesisCase::new_with(K, vec![(fund.op, fund.entry.clone())], &t);
    g.network = NetworkType::Simnet;
    g.funding_sk = w.sk;
    g.set_owner(&me);
    g.controller.m = cfg.m;
    for s in &mut g.shards {
        s.m = cfg.m;
        s.deadline = deadline;
    }
    if let Some(gg) = cfg.g {
        g.controller.g = gg;
    }
    g.set_vault(cfg.v0);
    g.set_jackpot(cfg.jackpot);
    if cfg.j_boost > 0 {
        let cc = &mut g.controller;
        cc.j_boost = cfg.j_boost;
        cc.liability = cc.jackpot - cc.j_boost + cc.g2 * (cc.m - 1);
        g.vault.liability = g.controller.liability;
    }
    g.pot = cfg.pot;
    g.controller.deadline = deadline;
    if let Some(edit) = cfg.edit {
        edit(&mut g);
    }
    g.validate_with(&t).map_err(|e| format!("genesis refused: {e}"))?;
    let mut gf = g.build_with(t.clone()).finish();
    g.fee = lottery_fee(&gf);
    gf = g.build_with(t.clone()).finish();
    gf.run().map_err(|(i, e)| format!("local engine rejects genesis at input {i}: {e:?}"))?;
    let cov = covenant_id(&gf);
    let gtx = finalized(&gf);
    let gc = &g.controller;
    let (gdaa, _) = submit_and_accept(
        c,
        w,
        &format!(
            "genesis K = {K}, m = {}, V0 = {} KAS, J(1) = {} KAS (J_boost {}, G2 {}, L(1) {}), pot {} KAS, cap {}, production templates, network simnet",
            gc.m,
            kas(cfg.v0),
            kas(gc.jackpot),
            kas(gc.j_boost),
            kas(gc.g2),
            kas(gc.liability),
            kas(g.pot as i64),
            gc.cap
        ),
        &gtx,
        &gf.entries,
    )
    .await?;
    let gid = gtx.id();
    log.tx("genesis", gid);
    let mut fund = own_utxo(c, w, Op::new(gid, gtx.outputs.len() as u32 - 1)).await?;
    println!("covenant id {cov}; genesis at DAA {gdaa}; deadline T {deadline}");

    // ---- SN4: the deployer position's full withdrawal request (vault age >= VAULT_REQ_DELAY)
    let mut vault_in = (Op::new(gid, g.vault_output() as u32), g.vault.clone());
    let mut wreq = None;
    if cfg.withdraw_full {
        let mut wc = WithdrawCase::new_with(&t, OwnerKey::Schnorr(w.sk));
        wc.vault = g.vault.clone();
        wc.reserve = g.reserve();
        wc.at_age(gdaa);
        mine_past_daa(c, &w.addr, wc.lock_time + 5).await?;
        wc.position = g.position.clone();
        wc.position_daa = gdaa;
        let mut wc = wc.whole().checked().map_err(|e| format!("builder policy refuses the withdrawal: {e}"))?;
        wc.owner_value = fund.entry.amount;
        wc.owner_daa = fund.entry.block_daa_score;
        wc.funding = vec![];
        wc.cov = cov;
        wc.funding_sk = w.sk;
        let wops = vec![vault_in.0, Op::new(gid, g.position_output() as u32), fund.op];
        let fin = |wc: &WithdrawCase| {
            let mut b = wc.build_with(t.clone());
            b.outpoints = wops.clone();
            b.finish()
        };
        wc.fee = lottery_fee(&fin(&wc));
        let wf = fin(&wc);
        same_entries(c, "request_withdraw", &wops[..2], &wf.entries[..2]).await?;
        wf.run().map_err(|(i, e)| format!("local engine rejects request_withdraw at input {i}: {e:?}"))?;
        let wx = wc.expected();
        let wtx = finalized(&wf);
        let (wdaa, _) =
            submit_and_accept(c, w, &format!("request_withdraw() whole deployer position, w = {} shares (all)", wc.w), &wtx, &wf.entries).await?;
        log.tx("request_withdraw (full)", wtx.id());
        let wid = wtx.id();
        vault_in = (Op::new(wid, 0), wx.vault.clone());
        wreq = Some((Op::new(wid, 1), wx.request.clone(), wtx.outputs[1].value, wdaa));
        fund = own_utxo(c, w, Op::new(wid, wtx.outputs.len() as u32 - 1)).await?;
    }
    let vault_daa = node_entry(c, vault_in.0, &t.vault_spk(&vault_in.1)).await?.block_daa_score;

    // ---- buys
    let m = cfg.m;
    let mut shards: Vec<(ShardState, u64, Op)> = g.shards.iter().enumerate().map(|(i, s)| (s.clone(), gdaa, Op::new(gid, 2 + i as u32))).collect();
    let mut trees: Vec<Tree> = (0..K).map(|_| Tree::new(m as usize)).collect();
    let mut tickets = vec![];
    for (i, bundle) in &cfg.buys {
        let i = *i;
        let mut bc = BuyCase::new(t.b_max).with_tree(trees[i].clone());
        bc.shard = shards[i].0.clone();
        bc.reserve = g.reserve();
        bc.shard_daa = shards[i].1;
        bc.cov = cov;
        bc.bundle = bundle.clone();
        bc.payout = me;
        bc.funding = vec![fund.entry.amount];
        bc.funding_sk = w.sk;
        let bops = vec![shards[i].2, fund.op];
        let fin = |bc: &BuyCase| {
            let mut b = bc.build_with(t.clone());
            b.outpoints = bops.clone();
            b.finish()
        };
        bc.fee = lottery_fee(&fin(&bc));
        let bf = fin(&bc);
        same_entries(c, "buy", &bops[..1], &bf.entries[..1]).await?;
        bf.run().map_err(|(k, e)| format!("local engine rejects buy at input {k}: {e:?}"))?;
        let btx = finalized(&bf);
        let desc: Vec<String> = bundle.iter().map(|x| format!("{:?}+{}", x.main, x.bonus)).collect();
        let (bdaa, _) = submit_and_accept(c, w, &format!("buy() on shard {i}, b = {}: {}", bundle.len(), desc.join(", ")), &btx, &bf.entries).await?;
        let bid = btx.id();
        log.tx(&format!("buy shard {i} [{}]", desc.join(", ")), bid);
        tickets.push(Ticket { op: Op::new(bid, 1), state: bc.ticket(), value: btx.outputs[1].value, daa: bdaa });
        shards[i] = (bc.successor(), bdaa, Op::new(bid, 0));
        trees[i] = bc.proofs().2;
        fund = own_utxo(c, w, Op::new(bid, btx.outputs.len() as u32 - 1)).await?;
    }

    // ---- close(), once T is final
    mine_past_daa(c, &w.addr, deadline as u64 + 5).await?;
    let mut cc = CloseCase::new_with(K, &t);
    cc.controller = g.controller.clone();
    cc.controller_daa = gdaa;
    cc.pot = g.pot;
    cc.params = g.params.clone();
    cc.params_daa = gdaa;
    cc.shards = shards.iter().map(|(s, daa, _)| ShardIn { state: s.clone(), daa: *daa }).collect();
    cc.funding = vec![FundingIn { value: fund.entry.amount, daa: fund.entry.block_daa_score }];
    cc.funder = me;
    cc.funding_sk = w.sk;
    cc.cov = cov;
    cc.lock_time = deadline as u64;
    let mut cops = vec![Op::new(gid, 0), Op::new(gid, 1)];
    cops.extend(shards.iter().map(|s| s.2));
    cops.push(fund.op);
    let fin = |cc: &CloseCase| {
        let mut b = cc.build_with(t.clone());
        b.outpoints = cops.clone();
        b.finish()
    };
    cc.fee = lottery_fee(&fin(&cc));
    let cf = fin(&cc);
    same_entries(c, "close", &cops[..K + 2], &cf.entries[..K + 2]).await?;
    cf.run().map_err(|(i, e)| format!("local engine rejects close at input {i}: {e:?}"))?;
    let x = cc.expected();
    let ctx_close = finalized(&cf);
    let (cdaa, c_start) = submit_and_accept(c, w, &format!("close() K = {K}, sales {} KAS", kas(x.marker.sales)), &ctx_close, &cf.entries).await?;
    let cid = ctx_close.id();
    log.tx("close", cid);
    let caller = own_utxo(c, w, Op::new(cid, ctx_close.outputs.len() as u32 - 1)).await?;
    let d_ref = game::anchor_daa(cdaa as i64, x.controller.seed_offset);
    println!("close at DAA {cdaa}; D = {d_ref}; marker J {} J_boost {} G2 {} m {}", kas(x.marker.jackpot), kas(x.marker.j_boost), kas(x.marker.g2), x.marker.m);

    // ---- the anchor X, ground
    let gr = grind(c, w, &cov.as_bytes(), x.marker.round, x.marker.m, d_ref as u64, target, p, &want, &note).await?;
    // 50 more blocks on X before the proof is read (as tn10's 50 blue score).
    mine(c, &w.addr, 50).await?;
    let to = crate::anchor::Timeouts { anchor: Duration::from_secs(30), blue_score: Duration::from_secs(30), poll: Duration::from_millis(20) };
    let a = crate::anchor::rpc_anchor(c, c_start, d_ref as u64, &to).await?;
    if a.x.sim.hash != gr.x {
        return Err(format!("the anchor found by RPC {} is not the ground block {}", a.x.sim.hash, gr.x));
    }
    if a.proof.to_bytes().len() != 272 {
        return Err(format!("anchor proof is {} bytes, expected 272", a.proof.to_bytes().len()));
    }

    // ---- draw()
    let mut d = DrawCase::new_with(K, &t);
    d.drawer = g.drawer.clone();
    d.drawer_daa = gdaa;
    d.marker = x.marker.clone();
    d.marker_daa = cdaa;
    d.vault = vault_in.1.clone();
    d.vault_daa = vault_daa;
    d.controller = x.controller.clone();
    d.pot = x.values[0] - x.controller.acc as u64 - x.controller.reserve as u64;
    d.controller_daa = cdaa;
    d.trees = trees.clone();
    let roots = d.marker.roots.clone();
    d.sync_roots();
    if d.marker.roots != roots {
        return Err("the marker roots are not the shard trees' roots".into());
    }
    d.cov = cov;
    d.funder = me;
    d.funding_sk = w.sk;
    d.funding = vec![FundingIn { value: caller.entry.amount, daa: caller.entry.block_daa_score }];
    d.absorbed = vec![];
    d.set_void(false);
    if d.d() != d_ref {
        return Err(format!("DrawCase D {} != {d_ref}", d.d()));
    }
    let sim = harness::ChainSim::from_real_chain(harness::Hash::from_bytes(a.p.fields.parent_commit), &[a.p.sim, a.x.sim]);
    d.sim = Some(std::sync::Arc::new(sim));
    d.x_hash = a.proof.x_hash();
    if d.anchor_bytes() != a.proof.to_bytes() || d.x_hash() != gr.x.as_bytes() {
        return Err("the builder's anchor bytes differ from the RPC proof".into());
    }
    d.fix_lock_time();
    let dops = vec![Op::new(gid, K as u32 + 3), Op::new(cid, K as u32 + 2), vault_in.0, Op::new(cid, 0), caller.op];
    let fin = |d: &DrawCase| {
        let mut b = d.build_with(t.clone());
        b.outpoints = dops.clone();
        b.finish()
    };
    d.fee = lottery_fee(&fin(&d));
    let df = fin(&d);
    same_entries(c, "draw", &dops[..4], &df.entries[..4]).await?;
    df.run().map_err(|(i, e)| format!("local engine rejects draw at input {i}: {e:?}"))?;
    let dx = d.expected();
    let dtx = finalized(&df);
    let (ddaa, _) = submit_and_accept(
        c,
        w,
        &format!("draw() real anchor X {} (ground), h_J = {}, n2 = {}, np = {}, record {}", gr.x, dx.h_j, dx.n2, dx.np(), dx.rec),
        &dtx,
        &df.entries,
    )
    .await?;
    log.tx("draw", dtx.id());
    let rr = RoundResult::from_tx(&dtx, &t).map_err(|e| format!("round result: {e}"))?;
    println!(
        "round result: win {:?}, X.hash {}, pools (tier, slice, value) {:?}",
        rr.win,
        Hash::from_bytes(rr.x_hash),
        rr.pools.iter().map(|p| (p.tier, p.slice, p.value)).collect::<Vec<_>>()
    );
    // Model = chain: every draw output value equals DrawCase::expected.
    let vals: Vec<u64> = dtx.outputs.iter().map(|o| o.value).collect();
    log.ok(vals == dx.values, format!("draw output values = DrawCase::expected ({} outputs)", vals.len()))?;
    log.ok(rr.x_hash == gr.x.as_bytes() && rr.win[..5] == gr.won.main && rr.win[5] == gr.won.bonus, "round result X.hash and win = the ground header's seed numbers".into())?;
    let fund = own_utxo(c, w, Op::new(dtx.id(), dx.caller_output() as u32)).await?;
    log.grind = Some(gr.clone());
    Ok(Round { t, cov, g, tickets, d, dx, dtx, ddaa, rr, grind: gr, vault_in: vault_in.1, wreq, marker: x.marker, fund, gf })
}

// ------------------------------------------------------------------ claims

pub struct PoolNow {
    pub op: Op,
    pub state: lottery::state::PoolState,
    pub value: u64,
}

impl PoolNow {
    fn of(r: &Round, tier: u8) -> Option<PoolNow> {
        let p = r.rr.pools.iter().position(|p| p.tier == tier)?;
        Some(PoolNow { op: r.rr.pools[p].outpoint, state: r.rr.pool_state(p), value: r.rr.pools[p].value })
    }
}

pub struct Claimed {
    pub x: ClaimExpected,
    pub tx: Transaction,
    pub daa: u64,
}

/// Build, check locally and submit `claim(idx)` of `ticket` against `pool`, funded by
/// `fund`. With `refused`, the local engine and the node must both reject it.
#[allow(clippy::too_many_arguments)]
async fn claim<C: RpcApi + ?Sized>(
    c: &C,
    w: &Wallet,
    r: &Round,
    log: &mut Log,
    what: &str,
    ticket: &Ticket,
    pool: &PoolNow,
    idx: i64,
    fund: &Utxo,
    refused: Option<&str>,
) -> Result<Option<Claimed>, String> {
    let t = r.t.clone();
    let mut cl = ClaimCase::new(K);
    cl.b_max = t.b_max;
    cl.ticket = ticket.state.clone();
    cl.ticket_value = ticket.value;
    cl.ticket_daa = ticket.daa;
    cl.pool = pool.state.clone();
    cl.pool_value = pool.value;
    cl.trees = r.d.trees.clone();
    cl.idx = idx;
    cl.change = true;
    cl.lock_time = 0;
    cl.cov = r.cov;
    cl.funding_sk = w.sk;
    cl.funding = vec![FundingIn { value: fund.entry.amount, daa: fund.entry.block_daa_score }];
    let ops = vec![ticket.op, pool.op, fund.op];
    let pool_daa = node_entry(c, pool.op, &t.pool_spk(&pool.state)).await?.block_daa_score;
    cl.pool_daa = pool_daa;
    let fin = |cl: &ClaimCase| {
        let mut b = cl.build_with(t.clone());
        b.outpoints = ops.clone();
        b.finish()
    };
    cl.fee = lottery_fee(&fin(&cl));
    let cf = fin(&cl);
    let x = cl.expected();
    same_entries(c, what, &ops[..2], &cf.entries[..2]).await?;
    let ctx_ = finalized(&cf);
    if let Some(only) = refused {
        if x.rejects.is_empty() || !x.rejects.iter().all(|r| r.starts_with(only)) {
            return Err(format!("{what}: the model's rejections {:?} are not only {only}", x.rejects));
        }
        let local = cf.run();
        let local_err = match local {
            Err((i, e)) => format!("input {i}: {e:?}"),
            Ok(()) => return Err(format!("{what}: the local engine ACCEPTS the claim that must be refused")),
        };
        let start = c.get_block_dag_info().await.map_err(|e| e.to_string())?.sink;
        let node = c.submit_transaction(RpcTransaction::from(&ctx_), false).await;
        let node_err = match node {
            Err(e) => e.to_string(),
            Ok(_) => {
                let acc = mine_until_accepted(c, &w.addr, start, ctx_.id(), 3).await;
                return Err(format!("{what}: FINDING: the node ACCEPTED the claim that must be refused (tx {}, accepted {:?})", ctx_.id(), acc));
            }
        };
        println!("REFUSED {what}: model rejects {:?}; local engine {local_err}; node: {node_err}", x.rejects);
        log.ok(true, format!("{what}: refused by the model ({:?}), the local engine ({local_err}) and the node ({node_err})", x.rejects))?;
        log.tx(&format!("{what} (refused, not mined)"), ctx_.id());
        return Ok(None);
    }
    if !x.rejects.is_empty() {
        return Err(format!("{what}: the model rejects the claim: {:?}", x.rejects));
    }
    cf.run().map_err(|(i, e)| format!("local engine rejects {what} at input {i}: {e:?}"))?;
    let (daa, _) = submit_and_accept(
        c,
        w,
        &format!("{what}: claim(idx {idx}) {:?} on tier {} pool, h = {}, prize {} KAS, mode {:?}", x.mode, pool.state.tier, x.h, kas(x.prize), x.mode),
        &ctx_,
        &cf.entries,
    )
    .await?;
    let vals: Vec<u64> = ctx_.outputs.iter().map(|o| o.value).collect();
    log.ok(vals == x.values, format!("{what}: output values = ClaimCase::expected"))?;
    log.tx(what, ctx_.id());
    Ok(Some(Claimed { x, tx: ctx_, daa }))
}

fn pool_after(prev: &PoolNow, cl: &Claimed) -> PoolNow {
    PoolNow { op: Op::new(cl.tx.id(), 0), state: prev.state.clone(), value: cl.x.values[0] }
}

fn ticket_after(cl: &Claimed) -> Ticket {
    Ticket { op: Op::new(cl.tx.id(), 1), state: cl.x.ticket.clone().expect("ticket'"), value: cl.x.values[1], daa: cl.daa }
}

async fn change_of<C: RpcApi + ?Sized>(c: &C, w: &Wallet, cl: &Claimed) -> Result<Utxo, String> {
    own_utxo(c, w, Op::new(cl.tx.id(), cl.x.change_output.expect("change") as u32)).await
}

// ------------------------------------------------------------------ scenarios

/// The fixed main numbers every scenario holds (colex rank 77,777).
const R0: u32 = 77_777;

fn main0() -> [u8; 5] {
    unrank(R0)
}

fn desc(s: &SeedNumbers) -> String {
    format!("{:?}+{}", s.main, s.bonus)
}

async fn with_node<F, Fut>(id: &'static str, f: F) -> Result<Log, String>
where
    F: FnOnce(kaspa_wrpc_client::KaspaRpcClient, Wallet, Log) -> Fut,
    Fut: std::future::Future<Output = Result<Log, String>>,
{
    println!("\n======== {id}: fresh private simnet node ========");
    let node = Node::start()?;
    let r = async {
        let c = node.client().await?;
        let w = Wallet::random();
        println!("miner and lottery address (throwaway key, this run only): {}", w.addr);
        let out = f(c.clone(), w, Log { id, ..Default::default() }).await;
        let _ = c.disconnect().await;
        out
    }
    .await;
    node.shutdown();
    r
}

/// SN1: m = 3, a single holding of c; c = jackpot.
pub async fn sn1() -> Result<Log, String> {
    with_node("SN1", |c, w, mut log| async move {
        let cmb = Combo::new(main0(), 2);
        let cfg = Cfg {
            id: "SN1",
            m: 3,
            v0: 1_000 * KAS as i64,
            jackpot: 100 * KAS as i64,
            j_boost: 20 * KAS as i64,
            pot: 30 * KAS,
            g: None,
            buys: vec![(0, vec![cmb])],
            withdraw_full: false,
            edit: None,
        };
        let held = cmb.bytes();
        let r = run_round(
            &c,
            &w,
            &cfg,
            &mut log,
            "c = jackpot (rank and bonus)",
            1.0 / (N as f64 * 3.0),
            |s| s.rank == cmb.rank() && s.bonus == cmb.bonus,
            |s| format!("win {}, c {:?} tier {:?}", desc(s), held, tier_of(&held, &[s.main[0], s.main[1], s.main[2], s.main[3], s.main[4], s.bonus])),
        )
        .await?;
        let (dx, mk) = (&r.dx, &r.marker);
        let j = mk.jackpot;
        let j_house = mk.jackpot - mk.j_boost;
        log.ok(mk.jackpot == cfg.jackpot && mk.j_boost == cfg.j_boost, format!("marker J = {} KAS, J_boost = {} KAS (round 1, from genesis)", kas(j), kas(mk.j_boost)))?;
        log.ok(dx.h_j == 1 && dx.n2 == 0, format!("h_J = {}, n2 = {}", dx.h_j, dx.n2))?;
        let jp = PoolNow::of(&r, TIER_JACKPOT).ok_or("no jackpot pool")?;
        log.ok(jp.value == j as u64 + r.g.reserve() && jp.state.slice == j, format!("jackpot pool = J = {} KAS (+ R; slice {})", kas(j), jp.state.slice))?;
        let v_paid = r.vault_in.total_assets + dx.amounts.h - dx.v_prime;
        log.ok(
            v_paid == j_house && dx.v_prime == game::vault_settle(r.vault_in.total_assets, dx.amounts.h, j_house, true, mk.g2, 0, false).unwrap(),
            format!("the vault pays J_house = {} KAS: V' = V {} + H {} - J_house = {} KAS (game::vault_settle)", kas(j_house), kas(r.vault_in.total_assets), kas(dx.amounts.h), kas(dx.v_prime)),
        )?;
        let o_no_hit = game::boost_after(r.d.pot as i64, dx.amounts.b_r, mk.j_boost, false, &dx.amounts.b, &dx.pools.lp, &dx.q).unwrap();
        log.ok(
            dx.o_prime == o_no_hit - mk.j_boost && dx.o_prime == game::boost_after(r.d.pot as i64, dx.amounts.b_r, mk.j_boost, true, &dx.amounts.b, &dx.pools.lp, &dx.q).unwrap(),
            format!(
                "the boost pot pays J_boost = {} KAS: it left the pot at close (O {} -> {} KAS) into the marker, and O' = {} KAS does not get it back (game::boost_after with jp)",
                kas(mk.j_boost),
                kas(cfg.pot as i64),
                kas(r.d.pot as i64),
                kas(dx.o_prime)
            ),
        )?;
        let cap = dx.controller.cap;
        log.ok(
            dx.controller.liability <= muldiv(dx.vault.total_assets, cap),
            format!("afterwards L' = {} KAS <= cap x V = {} x {} KAS = {} KAS", kas(dx.controller.liability), cap, kas(dx.vault.total_assets), kas(muldiv(dx.vault.total_assets, cap))),
        )?;
        let fund = r.fund.clone();
        let cl = claim(&c, &w, &r, &mut log, "SN1 jackpot claim", &r.tickets[0], &jp, 0, &fund, None).await?.unwrap();
        log.ok(
            cl.x.mode == ClaimMode::Final && cl.x.h == 1 && cl.x.prize == j && cl.tx.outputs[1].value == j as u64 + r.tickets[0].value && cl.tx.outputs[0].value == r.g.reserve(),
            format!("the claim pays all of J: prize {} KAS (FINAL, h = 1), payout {} KAS = J + ticket value; pool' = R", kas(cl.x.prize), kas(cl.tx.outputs[1].value as i64)),
        )?;
        Ok(log)
    })
    .await
}

/// SN2: m = 2, c in two tickets on different shards (h = 2); c = jackpot.
pub async fn sn2() -> Result<Log, String> {
    with_node("SN2", |c, w, mut log| async move {
        let cmb = Combo::new(main0(), 2);
        let filler = Combo::new(unrank(R0 + 1), 1);
        let cfg = Cfg {
            id: "SN2",
            m: 2,
            v0: 1_000 * KAS as i64,
            // Odd, so floor(J / 2) leaves 1 sompi in the pool.
            jackpot: 100 * KAS as i64 + 1,
            j_boost: 0,
            pot: 0,
            g: None,
            buys: vec![(0, vec![cmb]), (1, vec![cmb, filler])],
            withdraw_full: false,
            edit: None,
        };
        let held = cmb.bytes();
        let r = run_round(
            &c,
            &w,
            &cfg,
            &mut log,
            "c = jackpot (rank and bonus)",
            1.0 / (N as f64 * 2.0),
            |s| s.rank == cmb.rank() && s.bonus == cmb.bonus,
            |s| format!("win {}, c {:?} tier {:?}", desc(s), held, tier_of(&held, &[s.main[0], s.main[1], s.main[2], s.main[3], s.main[4], s.bonus])),
        )
        .await?;
        let j = r.marker.jackpot;
        log.ok(r.dx.h_j == 2, format!("h_J = {} (c on shards 0 and 1)", r.dx.h_j))?;
        let jp = PoolNow::of(&r, TIER_JACKPOT).ok_or("no jackpot pool")?;
        log.ok(jp.value == j as u64 + r.g.reserve(), format!("jackpot pool = J = {} KAS (+ R)", kas(j)))?;
        let half = game::claim_amount(j, 2).unwrap();
        let fund = r.fund.clone();
        // Order: the b = 2 ticket's index 0 first (PAY, ticket' keeps index 1), then the same
        // index of ticket' again while the pool still covers the prize (so only the claimed
        // bit, check 4, can refuse it), then the b = 1 ticket (FINAL).
        let c1 = claim(&c, &w, &r, &mut log, "SN2 claim 1 (shard 1 ticket, b = 2, idx 0)", &r.tickets[1], &jp, 0, &fund, None).await?.unwrap();
        log.ok(
            c1.x.prize == half && c1.x.h == 2 && c1.x.mode == ClaimMode::Pay && c1.tx.outputs[2].value == half as u64,
            format!("claim 1 pays floor(J / 2) = {} KAS (h = 2, PAY; ticket' keeps index 1)", kas(half)),
        )?;
        let jp1 = pool_after(&jp, &c1);
        let f1 = change_of(&c, &w, &c1).await?;
        let t1 = ticket_after(&c1);
        log.ok(jp1.value - r.g.reserve() >= half as u64, format!("before the repeat claim the pool still covers floor(J / 2): remaining {} sompi", jp1.value - r.g.reserve()))?;
        claim(&c, &w, &r, &mut log, "SN2 claim 3 (shard 1 ticket' again, idx 0)", &t1, &jp1, 0, &f1, Some("check 4")).await?;
        let c2 = claim(&c, &w, &r, &mut log, "SN2 claim 2 (shard 0 ticket, b = 1)", &r.tickets[0], &jp1, 0, &f1, None).await?.unwrap();
        log.ok(c2.x.prize == half && c2.x.h == 2 && c2.x.mode == ClaimMode::Final, format!("claim 2 pays floor(J / 2) = {} KAS (h = 2, FINAL)", kas(half)))?;
        let jp2 = pool_after(&jp1, &c2);
        log.ok(
            jp2.value == (j - 2 * half) as u64 + r.g.reserve(),
            format!("the remainder J - 2 floor(J / 2) = {} sompi stays in the pool (pool' = {} sompi = remainder + R)", j - 2 * half, jp2.value),
        )?;
        Ok(log)
    })
    .await
}

/// SN3: m = 3, the same main numbers held with bonuses 1 and 2; the drawn bonus is 3.
pub async fn sn3() -> Result<Log, String> {
    with_node("SN3", |c, w, mut log| async move {
        let c1 = Combo::new(main0(), 1);
        let c2 = Combo::new(main0(), 2);
        let cfg = Cfg {
            id: "SN3",
            m: 3,
            v0: 1_000 * KAS as i64,
            jackpot: 100 * KAS as i64,
            j_boost: 0,
            pot: 0,
            g: Some(1_000),
            buys: vec![(0, vec![c1]), (1, vec![c2])],
            withdraw_full: false,
            edit: None,
        };
        let r = run_round(
            &c,
            &w,
            &cfg,
            &mut log,
            "main numbers = held, bonus 3 (not held)",
            1.0 / (N as f64 * 3.0),
            |s| s.rank == R0 && s.bonus == 3,
            |s| format!("win {}, held main {:?} with bonuses 1, 2: {}", desc(s), main0(), if s.rank == R0 { "main matches" } else { "main differs" }),
        )
        .await?;
        let (dx, mk) = (&r.dx, &r.marker);
        let g2 = mk.g2;
        let j_house = mk.jackpot - mk.j_boost;
        log.ok(dx.h_j == 0 && dx.n2 == 2, format!("h_J = {}, n2 = {}", dx.h_j, dx.n2))?;
        log.ok(PoolNow::of(&r, TIER_JACKPOT).is_none(), "no jackpot pool".into())?;
        let sp = PoolNow::of(&r, TIER_SECOND).ok_or("no second-prize pool")?;
        log.ok(sp.value == (2 * g2) as u64 + r.g.reserve() && sp.state.slice == g2, format!("second-prize pool = 2 x G2 = 2 x {} KAS (+ R)", kas(g2)))?;
        log.ok(
            dx.v_prime == r.vault_in.total_assets + dx.amounts.h - 2 * g2,
            format!("J_house = {} KAS stays in the vault: V' = V {} + H {} - 2 G2 = {} KAS", kas(j_house), kas(r.vault_in.total_assets), kas(dx.amounts.h), kas(dx.v_prime)),
        )?;
        let fund = r.fund.clone();
        let k1 = claim(&c, &w, &r, &mut log, "SN3 claim (main+1)", &r.tickets[0], &sp, 0, &fund, None).await?.unwrap();
        log.ok(k1.x.prize == g2 && k1.x.h == 1, format!("claim (main+1) pays G2 = {} KAS (h = 1)", kas(g2)))?;
        let sp1 = pool_after(&sp, &k1);
        let f1 = change_of(&c, &w, &k1).await?;
        let k2 = claim(&c, &w, &r, &mut log, "SN3 claim (main+2)", &r.tickets[1], &sp1, 0, &f1, None).await?.unwrap();
        log.ok(k2.x.prize == g2 && k2.x.h == 1, format!("claim (main+2) pays G2 = {} KAS (h = 1)", kas(g2)))?;
        log.ok(k2.tx.outputs[0].value == r.g.reserve(), "second-prize pool drained to R".into())?;
        Ok(log)
    })
    .await
}

/// SN4: m = 2, main+1 and main+2 held (jackpot and second prize whichever bonus is drawn);
/// the deployer position's full withdrawal requested in the round.
pub async fn sn4() -> Result<Log, String> {
    with_node("SN4", |c, w, mut log| async move {
        let c1 = Combo::new(main0(), 1);
        let c2 = Combo::new(main0(), 2);
        let cfg = Cfg {
            id: "SN4",
            m: 2,
            v0: 1_000 * KAS as i64,
            jackpot: 100 * KAS as i64,
            j_boost: 0,
            pot: 0,
            g: Some(1_000),
            buys: vec![(0, vec![c1]), (1, vec![c2])],
            withdraw_full: true,
            edit: None,
        };
        let r = run_round(
            &c,
            &w,
            &cfg,
            &mut log,
            "main numbers = held (either bonus: jackpot + second prize)",
            1.0 / N as f64,
            |s| s.rank == R0,
            |s| format!("win {}, held main {:?} with bonuses 1, 2: {}", desc(s), main0(), if s.rank == R0 { "main matches" } else { "main differs" }),
        )
        .await?;
        let (dx, mk) = (&r.dx, &r.marker);
        let (j, g2) = (mk.jackpot, mk.g2);
        let j_house = mk.jackpot - mk.j_boost;
        // L(1) = J_house + G2 (m - 1), the liability of the round whose jackpot is fixed.
        let l1 = j_house + g2 * (mk.m - 1);
        log.ok(dx.h_j == 1 && dx.n2 == 1, format!("h_J = {}, n2 = {}", dx.h_j, dx.n2))?;
        let paid = r.vault_in.total_assets + dx.amounts.h - dx.v_prime;
        log.ok(paid == j_house + g2 * dx.n2 && paid <= l1, format!("the vault pays J_house + G2 x n2 = {} + {} = {} KAS <= L(1) = {} KAS", kas(j_house), kas(g2), kas(paid), kas(l1)))?;
        let vt = &r.vault_in;
        let model = game::convert(dx.v_prime, vt.total_shares, vt.pend_dep, vt.pend_wsh, dx.fix.liability).ok_or("game::convert rejects")?;
        log.ok(
            model == dx.conv && dx.vault.total_assets == model.total_assets() && dx.vault.total_shares == model.total_shares(),
            format!(
                "conversion after the hit = game::convert(V' {}, S {}, D {}, W {}, L' {}): wc = {} of W = {} shares, out_c = {} KAS, slack {} KAS",
                kas(dx.v_prime),
                vt.total_shares,
                vt.pend_dep,
                vt.pend_wsh,
                kas(dx.fix.liability),
                model.wc,
                model.w,
                kas(model.out_c),
                kas(model.slack)
            ),
        )?;
        log.ok(
            dx.vault.total_assets >= dx.fix.liability && dx.vault.liability == dx.fix.liability,
            format!("the conversion keeps L': vault' total_assets {} KAS >= L' = {} KAS", kas(dx.vault.total_assets), kas(dx.fix.liability)),
        )?;
        // The withdrawal's settle() against the record (game::settle).
        let rec = r.rr.record.clone().ok_or("the draw wrote no record")?;
        let (rq_op, rq, rq_val, rq_daa) = r.wreq.clone().unwrap();
        let t = r.t.clone();
        let mut s = SettleCase::new_with(&t, rec.state.clone(), rq.clone());
        s.cov = r.cov;
        s.funding_sk = w.sk;
        s.request_value = rq_val;
        s.request_daa = rq_daa;
        let rnode = node_entry(&c, rec.outpoint, &t.record_spk(&rec.state)).await?;
        s.record_reserve = rnode.amount - (rec.state.out_rem as u64 + rec.state.refund as u64 * rec.state.dep_rem as u64);
        s.record_daa = rnode.block_daa_score;
        s.funding = vec![FundingIn { value: r.fund.entry.amount, daa: r.fund.entry.block_daa_score }];
        let sops = vec![rq_op, rec.outpoint, r.fund.op];
        let fin = |s: &SettleCase| {
            let mut b = s.build_with(t.clone());
            b.outpoints = sops.clone();
            b.finish()
        };
        s.fee = lottery_fee(&fin(&s));
        let sf = fin(&s);
        same_entries(&c, "settle", &sops[..2], &sf.entries[..2]).await?;
        sf.run().map_err(|(i, e)| format!("local engine rejects settle at input {i}: {e:?}"))?;
        let sx = s.expected();
        let model = game::settle(&rec.state, &rq).ok_or("game::settle rejects")?;
        log.ok(
            sx.reference.as_ref() == Some(&model) && model.conv == dx.conv.wc,
            format!("withdrawal settle = game::settle: mode {:?}, conv {} shares, pay {} KAS, left {} shares", model.mode, model.conv, kas(model.pay), model.left),
        )?;
        let stx = finalized(&sf);
        submit_and_accept(&c, &w, &format!("settle() withdrawal {:?}, pay {} KAS", model.mode, kas(model.pay)), &stx, &sf.entries).await?;
        let svals: Vec<u64> = stx.outputs.iter().map(|o| o.value).collect();
        log.ok(svals == sx.values, "settle output values = SettleCase::expected".into())?;
        log.tx("settle (withdrawal)", stx.id());
        let mut fund = own_utxo(&c, &w, Op::new(stx.id(), sx.change_output.ok_or("settle change")? as u32)).await?;
        // Claims: the jackpot ticket and the second-prize ticket.
        let b = r.rr.win[5];
        let (jt, st) = if b == 1 { (0, 1) } else { (1, 0) };
        let jp = PoolNow::of(&r, TIER_JACKPOT).ok_or("no jackpot pool")?;
        let k1 = claim(&c, &w, &r, &mut log, &format!("SN4 jackpot claim (main+{b})"), &r.tickets[jt], &jp, 0, &fund, None).await?.unwrap();
        log.ok(k1.x.prize == j && k1.x.h == 1, format!("jackpot claim pays J = {} KAS", kas(j)))?;
        fund = change_of(&c, &w, &k1).await?;
        let sp = PoolNow::of(&r, TIER_SECOND).ok_or("no second-prize pool")?;
        log.ok(sp.value == g2 as u64 + r.g.reserve(), format!("second-prize pool = 1 x G2 = {} KAS (+ R)", kas(g2)))?;
        let k2 = claim(&c, &w, &r, &mut log, &format!("SN4 second-prize claim (main+{})", 3 - b), &r.tickets[st], &sp, 0, &fund, None).await?.unwrap();
        log.ok(k2.x.prize == g2 && k2.x.h == 1, format!("second-prize claim pays G2 = {} KAS", kas(g2)))?;
        Ok(log)
    })
    .await
}

/// What one ground anchor shows about grinding (SN5). The grinder evaluates the template's own
/// nonce first and stops there when it already meets the target (tries 1, the ground result is
/// the honest one); otherwise it grinds to a hit, which then differs from the honest candidate
/// (the target is a function of the numbers alone). `Ok(true)`: grinding changed the result;
/// `Ok(false)`: the first candidate already met the target, nothing was ground; `Err`: a record
/// that is neither.
pub(crate) fn grind_outcome(g: &Grind) -> Result<bool, String> {
    let same = g.honest.rank == g.won.rank && g.honest.bonus == g.won.bonus;
    match (g.tries, same) {
        (1, true) if g.nonce == g.honest_nonce && g.x == g.honest_x => Ok(false),
        (t, false) if t > 1 => Ok(true),
        _ => Err(format!(
            "inconsistent grind record: {} tries, template nonce {} -> {}, ground nonce {} -> {}",
            g.tries,
            g.honest_nonce,
            desc(&g.honest),
            g.nonce,
            desc(&g.won)
        )),
    }
}

/// SN5: the grinding numbers of every round run, and the honest-candidate check. With
/// `require_grind` (the SN scenarios, whose jackpot targets make a first-candidate hit about one
/// in 4 × 10^5) a round whose first candidate already met the target fails: that run shows
/// nothing about grinding. Without it (the dev run, whose target is any prize tier) such a round
/// is reported as "no grind needed", never as a failure.
pub fn sn5(logs: &[Log], require_grind: bool) -> Result<Vec<String>, String> {
    let mut rows = vec![];
    let mut tries_total = 0u64;
    let mut exp_total = 0f64;
    for l in logs {
        let g = l.grind.as_ref().ok_or(format!("{}: no grind", l.id))?;
        let ground = grind_outcome(g).map_err(|e| format!("CHECK FAILED: SN5 {}: {e}", l.id))?;
        if ground {
            check(true, format!("SN5 {}: the ungrounded first candidate (template nonce {}) gives {}, the ground result {} (after {} tries)", l.id, g.honest_nonce, desc(&g.honest), desc(&g.won), g.tries))?;
        } else {
            check(
                !require_grind,
                format!("SN5 {}: the template's first candidate (nonce {}) already gives the ground result {}: no grind needed", l.id, g.honest_nonce, desc(&g.won)),
            )?;
        }
        tries_total += g.tries;
        exp_total += g.expected;
        rows.push(format!(
            "| {} | m = {}, D = {} | {} | {} | {:.0} | {:.3} | {:.2} | X `{}` nonce {} -> {} | `{}` nonce {} -> {} ({}{}) |",
            l.id,
            g.m,
            g.d,
            g.target,
            g.tries,
            g.expected,
            g.secs,
            g.tries as f64 / g.expected,
            g.x,
            g.nonce,
            desc(&g.won),
            g.honest_x,
            g.honest_nonce,
            desc(&g.honest),
            g.honest_note,
            if ground { "" } else { "; no grind needed" }
        ));
    }
    println!("SN5: {} grinds, {tries_total} header hashes in total, expected {exp_total:.0}", logs.len());
    Ok(rows)
}

pub fn print_report(logs: &[Log]) {
    println!("\n======== REPORT ========");
    for l in logs {
        println!("## {}", l.id);
        for (w, id) in &l.txs {
            println!("- {w}: `{id}`");
        }
        for c in &l.checks {
            println!("- ok: {c}");
        }
    }
    match sn5(logs, false) {
        Ok(rows) => {
            println!("| SN | round | ground for | tries | expected 1/p | seconds | tries / expected | ground X | honest first candidate |");
            println!("|---|---|---|---|---|---|---|---|---|");
            for r in rows {
                println!("{r}");
            }
        }
        Err(e) => println!("SN5: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(tries: u64, honest: [u8; 32], won: [u8; 32], nonce: u64) -> Grind {
        let n = |h: &[u8; 32]| game::seed_numbers(&[7; 32], 1, h, 3);
        Grind {
            d: 100,
            m: 3,
            tries,
            secs: 0.0,
            expected: 1425.0,
            target: "t".into(),
            x: Hash::from_bytes(won),
            nonce,
            won: n(&won),
            honest_nonce: 5,
            honest_x: Hash::from_bytes(honest),
            honest: n(&honest),
            honest_note: "n".into(),
        }
    }

    /// The owner's dev run (2026-10-26): the template's first nonce already met the dev target,
    /// so the grinder stopped at once (tries 1, the ground result the honest one) and SN5 printed
    /// "CHECK FAILED ... gives X != the ground result X". Such a round is "no grind needed".
    #[test]
    fn a_first_candidate_that_already_wins_is_no_grind_not_a_failure() {
        let one = g(1, [1; 32], [1; 32], 5);
        assert_eq!(grind_outcome(&one), Ok(false));
        let log = Log { id: "DEV", grind: Some(one), ..Default::default() };
        let rows = sn5(std::slice::from_ref(&log), false).expect("the dev run never fails on it");
        assert!(rows[0].contains("no grind needed"), "{}", rows[0]);
        // The SN scenarios still require a real grind.
        let e = sn5(std::slice::from_ref(&log), true).unwrap_err();
        assert!(e.contains("already gives the ground result") && !e.contains("!="), "{e}");
    }

    #[test]
    fn a_ground_result_differs_from_the_first_candidate() {
        let ground = g(42, [1; 32], [2; 32], 46);
        assert_ne!(ground.honest.rank * 100 + ground.honest.bonus as u32, ground.won.rank * 100 + ground.won.bonus as u32);
        assert_eq!(grind_outcome(&ground), Ok(true));
        let log = Log { id: "SN1", grind: Some(ground), ..Default::default() };
        assert!(sn5(std::slice::from_ref(&log), true).is_ok());
    }

    #[test]
    fn an_inconsistent_record_fails() {
        // Ground for many tries yet equal to the first candidate, or one try that differs.
        assert!(grind_outcome(&g(9, [1; 32], [1; 32], 13)).is_err());
        assert!(grind_outcome(&g(1, [1; 32], [2; 32], 5)).is_err());
        // One try with the same numbers but another nonce.
        assert!(grind_outcome(&g(1, [1; 32], [1; 32], 6)).is_err());
    }
}
