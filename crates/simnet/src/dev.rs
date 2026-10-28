//! `dev` (front-end F2, spec frontend 1.1 DX): a private simnet instance with every round
//! status at once, indexed, and served by the indexer API; the base of F3's one-command dev
//! run. On a fresh in-process node (no peers, loopback only, D125):
//! 1. round 1 with the production templates and the M8 testnet's round knobs (round_len =
//!    d_min = 6,000 DAA, 10 minutes at 10 BPS; 600 was too short: the anchor wait after a
//!    close passes the next round's deadline, and a shard takes one buy after it): buys, `close()`, a `draw()` whose anchor block is ground so that a held
//!    combination wins a prize tier (`sn::run_round`, D114): **drawn**;
//! 2. the M8 indexer (`tn10/src/indexer.rs`, unchanged) follows the node from its genesis
//!    and writes `index/1` to `crates/target/simnet-dev/<covenant-id>/`, with the launch
//!    bundle `bundle.json`;
//! 3. round 2: buys on its shards (states from the index), then `close()`: **closing** (its
//!    draw is left to the page), mined past D so its anchor proof can be built;
//! 4. round 3: buys: **open**;
//! 5. the indexer API (`crates/indexer-api`) serves the store on `--api-bind` (default
//!    127.0.0.1:8790), with the node as its anchor-proof source for undrawn rounds;
//! 6. then it mines one block every `--block-ms` (default 200) and follows the chain with the
//!    indexer, until Ctrl-C or `--for <seconds>`.
//!
//! F3 (the page's one-command dev run, `web/scripts/dev-simnet.mjs`): the node's Borsh wRPC
//! listens on the fixed loopback port `--node-port` (default 18610), and the dev faucet
//! (`faucet.rs`) on `--faucet-bind` (default 127.0.0.1:8791) pays the page's quick wallet.
//! `current.json` is written only once the instance is ready (then the self-check runs), with
//! `startedMs`, so a starter script can wait for a fresh one.
//!
//! Every transaction is built by the `lottery` builders, checked in the local engine and
//! submitted to the node's mempool, as in SN1-SN4. `crates/target/simnet-dev/current.json`
//! names the API URL, the node's wRPC URL and the instance. The keys are throwaway simnet
//! keys of this run, never printed.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kaspa_consensus_core::tx::{TransactionOutpoint as Op, UtxoEntry};
use kaspa_hashes::Hash;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::KaspaRpcClient;
use lottery::buy::{BuyCase, Templates};
use lottery::close::{CloseCase, FundingIn, ShardIn};
use lottery::combo::{Combo, unrank};
use lottery::game::{N, SeedNumbers, tier_of};
use lottery::index::State;
use lottery::state::Payout;

use crate::indexer::{self, Indexer};
use crate::miner::{mine, mine_one, mine_past_daa};
use crate::node::Node;
use crate::sn::{self, Cfg, Log, finalized, lottery_fee, own_utxo, run_round, submit_and_accept};
use crate::wallet::{KAS, Utxo, Wallet};

pub struct Opts {
    pub api_bind: std::net::SocketAddr,
    /// The node's Borsh wRPC port on 127.0.0.1 (fixed, so the page's dev config can name it).
    pub node_port: u16,
    /// The dev faucet (`faucet.rs`) for the page's quick wallet.
    pub faucet_bind: std::net::SocketAddr,
    pub run_for: Option<Duration>,
    pub block_ms: u64,
    /// F6 part C (`simnet follow-check`): after the setup, check the indexer API's
    /// one-process mode against this node (`follow_check`), then stop.
    pub follow_check: bool,
}

/// The dev run's fixed node port (F3): Borsh wRPC on 127.0.0.1:18610 (not a rusty-kaspa default).
pub const NODE_PORT: u16 = 18_610;

/// The M8 testnet instance's round knobs (spec 8 A2): ten minutes of sales at 10 BPS. Front end
/// F3 part B: the referral share s_R = 500 (5 %) in every round, so that at p = 1 KAS a
/// one-combination referral (0.05 KAS) is below M_REF = 0.1 KAS and stays in the round, while
/// a four-combination one (0.2 KAS) is paid: both spec 8 cases on one instance.
pub const DEV_S_R: i64 = 500;

fn short_rounds(g: &mut lottery::genesis::GenesisCase) {
    g.controller.round_len = 6_000;
    g.controller.d_min = 6_000;
    g.controller.s_r = DEV_S_R;
    g.params.next_s_r = DEV_S_R;
    for s in &mut g.shards {
        s.s_r = DEV_S_R;
    }
}

fn combo(n: u32, bonus: u8) -> Combo {
    Combo::new(unrank(1 + n * 9_973), bonus)
}

fn win6(s: &SeedNumbers) -> [u8; 6] {
    [s.main[0], s.main[1], s.main[2], s.main[3], s.main[4], s.bonus]
}

/// The chain block that accepted each round's `close()`, by the marker's creation DAA (the
/// start of the anchor search: `find_anchor` reads one bounded batch of the virtual chain
/// from its start, so the start must be just below D).
type Starts = Arc<std::sync::Mutex<std::collections::BTreeMap<u64, Hash>>>;

/// The undrawn rounds' anchor proofs, built from the node (the API's fallback, spec 6.1).
struct NodeAnchor {
    c: Arc<KaspaRpcClient>,
    starts: Starts,
}

impl indexer_api::AnchorProvider for NodeAnchor {
    fn proof(&self, round: i64, d: u64, marker_daa: u64) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + '_>> {
        Box::pin(async move {
            let start = self.starts.lock().map_err(|e| e.to_string())?.get(&marker_daa).copied().ok_or(format!("round {round}: its close's chain block is not known yet"))?;
            let to = crate::anchor::Timeouts { anchor: Duration::from_secs(5), blue_score: Duration::from_secs(5), poll: Duration::from_millis(50) };
            let a = crate::anchor::rpc_anchor(&*self.c, start, d, &to).await?;
            Ok(a.proof.to_bytes().to_vec())
        })
    }
}

/// Record the accepting chain block of every closed round's `close()`.
fn note_closes(x: &Indexer, starts: &Starts) {
    let Ok(mut m) = starts.lock() else { return };
    for r in x.ix.rounds().values() {
        if let Some(cl) = &r.close {
            if let Some(h) = x.chain_block_of(&cl.txid) {
                m.insert(cl.marker_daa, h);
            }
        }
    }
}

fn dev_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("target").join("simnet-dev")
}

/// The live object of `role` in round `round` with the predicate, from the index.
fn live_where<'a>(x: &'a Indexer, f: impl Fn(&State) -> bool + 'a) -> impl Iterator<Item = &'a lottery::index::Live> + 'a {
    x.ix.live().values().filter(move |l| l.state.as_ref().is_some_and(&f))
}

async fn catch_up(c: &KaspaRpcClient, x: &mut Indexer) -> Result<(), String> {
    let st = indexer::poll(c, x).await?;
    if st.kept == 0 {
        return Ok(());
    }
    println!("indexer: {} chain blocks, {} kept, {} events; last DAA {}; gaps {:?}", st.blocks, st.kept, st.events.len(), x.last_daa, x.ix.gaps());
    Ok(())
}

/// One buy on shard `i` of `round`, the shard's state and tree from the index.
#[allow(clippy::too_many_arguments)]
async fn buy(c: &KaspaRpcClient, w: &Wallet, x: &mut Indexer, t: &Arc<Templates>, reserve: u64, round: i64, i: i64, bundle: Vec<Combo>, fund: Utxo) -> Result<Utxo, String> {
    let l = live_where(x, |s| matches!(s, State::Shard(sh) if sh.round == round && sh.shard_index == i)).next().ok_or(format!("no live shard {i} of round {round} in the index"))?.clone();
    let Some(State::Shard(s)) = l.state.clone() else { unreachable!() };
    let tree = x.ix.round(round).and_then(|r| r.shards.get(&i)).and_then(|tr| tr.tree.clone()).ok_or(format!("the index has no tree for shard {i} of round {round}"))?;
    let me = Payout::Schnorr(w.xonly());
    let mut bc = BuyCase::new(t.b_max).with_tree(tree);
    bc.shard = s;
    bc.reserve = reserve;
    bc.shard_daa = l.daa;
    bc.cov = x.cov;
    bc.bundle = bundle.clone();
    bc.payout = me;
    bc.funding = vec![fund.entry.amount];
    bc.funding_sk = w.sk;
    let ops = vec![l.outpoint, fund.op];
    let fin = |bc: &BuyCase| {
        let mut b = bc.build_with(t.clone());
        b.outpoints = ops.clone();
        b.finish()
    };
    bc.fee = lottery_fee(&fin(&bc));
    let f = fin(&bc);
    f.run().map_err(|(k, e)| format!("local engine rejects buy at input {k}: {e:?}"))?;
    let tx = finalized(&f);
    let desc: Vec<String> = bundle.iter().map(|x| format!("{:?}+{}", x.main, x.bonus)).collect();
    submit_and_accept(c, w, &format!("round {round} buy() on shard {i}: {}", desc.join(", ")), &tx, &f.entries).await?;
    let next = own_utxo(c, w, Op::new(tx.id(), tx.outputs.len() as u32 - 1)).await?;
    catch_up(c, x).await?;
    Ok(next)
}

/// `close()` of `round` from the index's live controller, params and shards, after T.
async fn close(c: &KaspaRpcClient, w: &Wallet, x: &mut Indexer, t: &Arc<Templates>, round: i64, fund: Utxo) -> Result<Utxo, String> {
    let ctrl = live_where(x, |s| matches!(s, State::Controller(cc) if cc.round == round)).next().ok_or("no live controller")?.clone();
    let Some(State::Controller(cs)) = ctrl.state.clone() else { unreachable!() };
    let par = live_where(x, |s| matches!(s, State::Params(_))).next().ok_or("no live params")?.clone();
    let Some(State::Params(ps)) = par.state.clone() else { unreachable!() };
    let mut shards: Vec<lottery::index::Live> = live_where(x, |s| matches!(s, State::Shard(sh) if sh.round == round)).cloned().collect();
    shards.sort_by_key(|l| if let Some(State::Shard(s)) = &l.state { s.shard_index } else { 0 });
    if shards.len() != cs.k as usize {
        return Err(format!("{} live shards of round {round}, k = {}", shards.len(), cs.k));
    }
    mine_past_daa(c, &w.addr, cs.deadline as u64 + 5).await?;
    let k = shards.len();
    let mut cc = CloseCase::new_with(k, t);
    cc.controller = cs.clone();
    cc.controller_daa = ctrl.daa;
    cc.pot = ctrl.value - cs.acc as u64 - cs.reserve as u64;
    cc.params = ps;
    cc.params_daa = par.daa;
    cc.shards = shards.iter().map(|l| ShardIn { state: if let Some(State::Shard(s)) = &l.state { s.clone() } else { unreachable!() }, daa: l.daa }).collect();
    cc.funding = vec![FundingIn { value: fund.entry.amount, daa: fund.entry.block_daa_score }];
    cc.funder = Payout::Schnorr(w.xonly());
    cc.funding_sk = w.sk;
    cc.cov = x.cov;
    cc.lock_time = cs.deadline as u64;
    let mut ops = vec![ctrl.outpoint, par.outpoint];
    ops.extend(shards.iter().map(|l| l.outpoint));
    ops.push(fund.op);
    let fin = |cc: &CloseCase| {
        let mut b = cc.build_with(t.clone());
        b.outpoints = ops.clone();
        b.finish()
    };
    cc.fee = lottery_fee(&fin(&cc));
    let f = fin(&cc);
    f.run().map_err(|(i, e)| format!("local engine rejects close at input {i}: {e:?}"))?;
    let xx = cc.expected();
    let tx = finalized(&f);
    let entries: Vec<UtxoEntry> = f.entries.clone();
    let (cdaa, _) = submit_and_accept(c, w, &format!("round {round} close() K = {k}, sales {} sompi", xx.marker.sales), &tx, &entries).await?;
    println!("round {round} closed at DAA {cdaa}: D = {}; round {} m = {}", lottery::game::anchor_daa(cdaa as i64, xx.controller.seed_offset), round + 1, xx.controller.m);
    let next = own_utxo(c, w, Op::new(tx.id(), tx.outputs.len() as u32 - 1)).await?;
    catch_up(c, x).await?;
    Ok(next)
}

// ------------------------------------------------------------------ dev rounds (F4, TEST-ONLY)

/// `draw()` of the closed `round` from the index, with the anchor proof read from the node; when
/// `target` is given, the anchor X is ground first so that `target` is the winning combination (the
/// jackpot; `sn::grind`, D114). Returns the caller output for the next step.
#[allow(clippy::too_many_arguments)]
async fn draw_round(c: &KaspaRpcClient, w: &Wallet, x: &mut Indexer, t: &Arc<Templates>, round: i64, target: Option<Combo>, fund: Utxo) -> Result<(Utxo, [u8; 6]), String> {
    use lottery::store::{Funding, Payer};
    let close = x.ix.round(round).and_then(|r| r.close.clone()).ok_or(format!("round {round} is not closed in the index"))?;
    let start = x.chain_block_of(&close.txid).ok_or(format!("round {round}: its close's chain block is not known"))?;
    let (seed_offset, m) = live_where(x, |s| matches!(s, State::Marker(mk) if mk.round == round))
        .find_map(|l| if let Some(State::Marker(mk)) = &l.state { Some(mk.m) } else { None })
        .zip(live_where(x, |s| matches!(s, State::Controller(cc) if cc.round == round + 1)).find_map(|l| if let Some(State::Controller(cc)) = &l.state { Some(cc.seed_offset) } else { None }))
        .map(|(m, so)| (so, m))
        .ok_or(format!("round {round}: no live marker and next controller"))?;
    let d = lottery::game::anchor_daa(close.marker_daa as i64, seed_offset) as u64;
    match target {
        Some(tg) => {
            let hb = tg.bytes();
            let p = 1.0 / (N as f64 * m as f64);
            sn::grind(c, w, &x.cov.as_bytes(), round, m, d, "the page's combination wins the jackpot", p, move |s| win6(s) == hb, move |s| format!("win {:?}+{}", s.main, s.bonus)).await?;
            mine(c, &w.addr, 50).await?;
        }
        None => {
            mine_past_daa(c, &w.addr, d + 60).await?;
        }
    }
    let to = crate::anchor::Timeouts { anchor: Duration::from_secs(30), blue_score: Duration::from_secs(30), poll: Duration::from_millis(20) };
    let a = crate::anchor::rpc_anchor(c, start, d, &to).await?;
    let sim = Arc::new(harness::ChainSim::from_real_chain(harness::Hash::from_bytes(a.p.fields.parent_commit), &[a.p.sim, a.x.sim]));
    let funding = [Funding { outpoint: fund.op, value: fund.entry.amount, daa: fund.entry.block_daa_score }];
    let me = Payout::Schnorr(w.xonly());
    let fin = |fee: u64| -> Result<(lottery::buy::Final, lottery::draw::DrawCase), String> {
        let (mut dc, ops) = lottery::draw::DrawCase::from_store(&x.ix, Some(a.proof), &funding, me, &Payer { funding_sk: w.sk, fee })?;
        dc.sim = Some(sim.clone());
        let mut b = dc.build_with(t.clone());
        b.outpoints = ops;
        Ok((b.finish(), dc))
    };
    let fee = lottery_fee(&fin(0)?.0);
    let (f, dc) = fin(fee)?;
    f.run().map_err(|(i, e)| format!("local engine rejects draw at input {i}: {e:?}"))?;
    let dx = dc.expected();
    let tx = finalized(&f);
    submit_and_accept(c, w, &format!("dev round: round {round} draw()"), &tx, &f.entries).await?;
    let win = [dc.seed().main[0], dc.seed().main[1], dc.seed().main[2], dc.seed().main[3], dc.seed().main[4], dc.seed().bonus];
    println!("dev round: round {round} drawn, winning {win:?}, pools {:?}", dx.values.len());
    let next = own_utxo(c, w, Op::new(tx.id(), dx.caller_output() as u32)).await?;
    catch_up(c, x).await?;
    Ok((next, win))
}

/// F4 (`POST /round` on the dev faucet; TEST-ONLY, simnet only): the closing round is drawn, the open
/// round mined past its deadline, closed, its anchor ground so that the page's combination `target`
/// wins the jackpot, and drawn. With no `target` (`POST /close`) it stops once the open round is closed
/// (the next call draws that round honestly first). The faucet's lock is held throughout, so the faucet
/// and this never spend one coin of the dev wallet.
async fn advance(c: &KaspaRpcClient, w: &Wallet, x: &mut Indexer, t: &Arc<Templates>, starts: &Starts, spent: &crate::faucet::Spent, target: Option<Combo>) -> Result<serde_json::Value, String> {
    let g = spent.lock().await;
    let vdaa = c.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    let mut coins: Vec<Utxo> = w.spendable(c, vdaa).await?.into_iter().filter(|u| !g.0.contains(&u.op)).collect();
    coins.sort_by_key(|u| std::cmp::Reverse(u.entry.amount));
    let mut fund = coins.into_iter().next().filter(|u| u.entry.amount >= 20 * KAS).ok_or("the dev wallet has no coin of 20 KAS; retry in a minute")?;
    let mut drawn = vec![];
    // The closing round (a live marker), drawn honestly.
    let closing = live_where(x, |s| matches!(s, State::Marker(_))).find_map(|l| if let Some(State::Marker(mk)) = &l.state { Some(mk.round) } else { None });
    if let Some(r) = closing {
        fund = draw_round(c, w, x, t, r, None, fund).await?.0;
        drawn.push(r);
    }
    // The open round: closed, then drawn with the page's combination winning.
    let open = live_where(x, |s| matches!(s, State::Controller(cc) if cc.drawn == 1)).find_map(|l| if let Some(State::Controller(cc)) = &l.state { Some(cc.round) } else { None }).ok_or("no open round whose predecessor is drawn")?;
    fund = close(c, w, x, t, open, fund).await?;
    note_closes(x, starts);
    let Some(target) = target else {
        drop(g);
        return Ok(serde_json::json!({ "drawn": drawn, "round": open.to_string(), "closed": true }));
    };
    let (_, win) = draw_round(c, w, x, t, open, Some(target), fund).await?;
    note_closes(x, starts);
    drop(g);
    Ok(serde_json::json!({ "drawn": drawn, "round": open.to_string(), "winning": { "main": &win[..5], "bonus": win[5] } }))
}

/// F5 (`POST /mine?to=…` on the dev faucet; TEST-ONLY, simnet only): mine until the page's own S4
/// panel finds a transition due (F6: or the Vault page a request possible, `to=vault`): past the open round's T (`close()`), past the closing round's D and
/// 60 blocks on (`draw()`, its anchor proof buildable), or past its D + draw_window (the void; about
/// 864,000 blocks, mined to a throwaway address so the dev wallet's coin list stays small, with the
/// indexer catching up every 20,000 blocks). Nothing is built or submitted: the page does that.
async fn mine_to(c: &KaspaRpcClient, x: &mut Indexer, starts: &Starts, to: crate::faucet::MineTo, pay: &kaspa_addresses::Address) -> Result<serde_json::Value, String> {
    use crate::faucet::MineTo;
    let t0 = Instant::now();
    catch_up(c, x).await?;
    let (round, target) = match to {
        MineTo::Deadline => {
            let (r, t) = live_where(x, |s| matches!(s, State::Controller(_)))
                .filter_map(|l| if let Some(State::Controller(cc)) = &l.state { Some((cc.round, cc.deadline)) } else { None })
                .max()
                .ok_or("no live controller")?;
            (r, t as u64 + 5)
        }
        MineTo::Vault => {
            // F6: the vault takes a request once the node's DAA score is past its creation DAA +
            // VAULT_REQ_DELAY (vault checks D10, W12). The index first catches up, so the vault is the
            // live one (a request the page just made moved it).
            let v = live_where(x, |s| matches!(s, State::Vault(_))).next().ok_or("no live vault")?;
            let r = live_where(x, |s| matches!(s, State::Controller(_))).filter_map(|l| if let Some(State::Controller(cc)) = &l.state { Some(cc.round) } else { None }).max().unwrap_or(0);
            (r, v.daa + lottery::game::VAULT_REQ_DELAY as u64 + 5)
        }
        MineTo::Anchor | MineTo::Void => {
            let mk = live_where(x, |s| matches!(s, State::Marker(_))).find_map(|l| if let Some(State::Marker(mk)) = &l.state { Some(mk.round) } else { None }).ok_or("no closing round (no live marker)")?;
            let close = x.ix.round(mk).and_then(|r| r.close.clone()).ok_or(format!("round {mk} is not closed in the index"))?;
            let so = live_where(x, |s| matches!(s, State::Controller(cc) if cc.round == mk + 1)).find_map(|l| if let Some(State::Controller(cc)) = &l.state { Some(cc.seed_offset) } else { None }).ok_or("no next controller")?;
            let dw = live_where(x, |s| matches!(s, State::Drawer(_))).find_map(|l| if let Some(State::Drawer(dr)) = &l.state { Some(dr.draw_window) } else { None }).ok_or("no live drawer")?;
            let d = lottery::game::anchor_daa(close.marker_daa as i64, so);
            match to {
                MineTo::Anchor => (mk, d as u64 + 60),
                _ => (mk, lottery::game::void_lock(d, dw).ok_or("no void lock")? as u64 + 5),
            }
        }
    };
    let start = c.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    println!("dev mine: {to:?} of round {round}: virtual DAA {start} -> past {target}");
    loop {
        let v = c.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
        if v > target {
            break;
        }
        let n = (target + 1 - v).min(20_000);
        mine(c, pay, n).await?;
        catch_up(c, x).await?;
        note_closes(x, starts);
        if target - start > 50_000 {
            println!("dev mine: virtual DAA {} of {target} ({:.0} s)", v + n, t0.elapsed().as_secs_f64());
        }
    }
    mine(c, pay, 5).await?;
    catch_up(c, x).await?;
    note_closes(x, starts);
    let v = c.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    println!("dev mine: {to:?} of round {round} reached: virtual DAA {v} in {:.0} s", t0.elapsed().as_secs_f64());
    Ok(serde_json::json!({ "to": format!("{to:?}").to_lowercase(), "round": round.to_string(), "target": target.to_string(), "virtualDaaScore": v.to_string() }))
}

// ------------------------------------------------------------------ self-check (as the page)

/// GET `path` on the local API (HTTP/1.1, `Connection: close`): (status, body).
async fn http_get(addr: std::net::SocketAddr, path: &str) -> Result<(u16, String), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(addr).await.map_err(|e| format!("connect {addr}: {e}"))?;
    s.write_all(format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").as_bytes()).await.map_err(|e| e.to_string())?;
    let mut buf = vec![];
    s.read_to_end(&mut buf).await.map_err(|e| e.to_string())?;
    let text = String::from_utf8(buf).map_err(|e| e.to_string())?;
    let (head, body) = text.split_once("\r\n\r\n").ok_or("no HTTP header end")?;
    let status = head.split(' ').nth(1).and_then(|x| x.parse().ok()).ok_or("no HTTP status")?;
    Ok((status, body.to_string()))
}

/// The node's entries for the objects' outpoints, read by their scripts' addresses (what the
/// page does: never the API's entries).
async fn node_utxos(c: &KaspaRpcClient, objects: &[web::indexer::ApiObject]) -> Result<Vec<web::dto::Utxo>, String> {
    let mut addrs = vec![];
    for o in objects {
        let spk = web::convert::spk("spk", &o.script_public_key).map_err(|e| e.to_string())?;
        let a = kaspa_txscript::extract_script_pub_key_address(&spk, kaspa_addresses::Prefix::Simnet).map_err(|e| e.to_string())?;
        if !addrs.contains(&a) {
            addrs.push(a);
        }
    }
    let v = c.get_utxos_by_addresses(addrs).await.map_err(|e| e.to_string())?;
    Ok(v.into_iter()
        .map(|e| {
            let op: Op = e.outpoint.into();
            let entry: UtxoEntry = e.utxo_entry.into();
            web::convert::utxo_dto(&op, &entry)
        })
        .filter(|u| objects.iter().any(|o| o.transaction_id == u.transaction_id && o.index == u.index))
        .collect())
}

/// Every endpoint fetched and checked as the page will (`web::indexer`), against the node.
async fn self_check(c: &KaspaRpcClient, api: std::net::SocketAddr, cov: Hash, me: &str) -> Result<(), String> {
    use web::indexer::*;
    let net = web::convert::Net::Simnet;
    let covh = lottery::store::to_hex(&cov.as_bytes());
    let get = move |p: String| async move { http_get(api, &p).await };
    let e = |x: lottery::web::Error| x.to_string();
    let (_, b) = get("/v1/instance".into()).await?;
    let hash = web::convert::hex(&bundle_hash(b.as_bytes()));
    let inst = check_instance(net, &InstanceCheckInput { bundle: b, covenant_id: covh.clone(), bundle_hash: Some(hash.clone()) }).map_err(e)?;
    println!("SELF-CHECK instance: ok (k {}, B_MAX {}, bundle hash {hash})", inst.k, inst.b_max);
    let (_, b) = get("/v1/rounds".into()).await?;
    for row in parse_rounds(&b).map_err(e)?.rounds {
        let r = row.round.clone();
        let (_, b) = get(format!("/v1/rounds/{r}")).await?;
        let round = parse_round(&b).map_err(e)?;
        let (s, b) = get(format!("/v1/rounds/{r}/purchases")).await?;
        let purchases = if s == 200 { Some(parse_purchases(&b).map_err(e)?) } else { None };
        let (s, b) = get(format!("/v1/rounds/{r}/anchor-proof")).await?;
        let anchor_proof = if s == 200 { Some(parse_anchor_proof(&b).map_err(e)?) } else { None };
        let anchor_block = match (&anchor_proof, round.status) {
            (Some(p), RoundStatus::Closing) => {
                let x = Hash::from_bytes(web::convert::hash32("x", &p.anchor_proof[..64]).map_err(e)?);
                let blk = c.get_block(x, false).await.map_err(|e| e.to_string())?;
                let h = kaspa_consensus_core::header::Header::try_from(&blk.header).map_err(|e| format!("header: {e}"))?;
                let is_chain_block = blk.verbose_data.as_ref().is_some_and(|v| v.is_chain_block);
                Some(web::dto::AnchorBlock { header: web::convert::header_dto(&h), is_chain_block })
            }
            _ => None,
        };
        let utxos = node_utxos(c, &round.objects).await?;
        let n = round.objects.len();
        let checked = check_round(net, &RoundCheckInput { covenant_id: covh.clone(), round, utxos, purchases, anchor_proof, anchor_block }).map_err(e)?;
        println!(
            "SELF-CHECK round {r}: ok, {:?} (from the chain objects: {}), {n} objects against the node, roots {}, numbers {}",
            checked.status,
            checked.status_checked,
            checked.roots.as_ref().map(|x| x.roots.len().to_string()).unwrap_or("-".into()),
            checked.winning.as_ref().map(|w| format!("{:?}+{} {:?} (D {})", w.winning.main, w.winning.bonus, w.status, w.d)).unwrap_or("-".into())
        );
    }
    let (_, b) = get(format!("/v1/tickets?payout={me}")).await?;
    let t = parse_tickets(&b).map_err(e)?;
    let utxos = node_utxos(c, &t.tickets).await?;
    let n = check_objects(net, &ObjectsCheckInput { covenant_id: covh.clone(), objects: t.tickets, utxos }).map_err(e)?.len();
    println!("SELF-CHECK tickets: ok, {n} live tickets of {me} against the node ({} purchases)", t.bought.len());
    for p in ["/v1/vault".to_string(), "/v1/pools".to_string(), format!("/v1/positions?owner={me}"), format!("/v1/requests?owner={me}")] {
        let (_, b) = get(p.clone()).await?;
        let o = parse_objects(&b).map_err(e)?;
        let utxos = node_utxos(c, &o.objects).await?;
        let n = check_objects(net, &ObjectsCheckInput { covenant_id: covh.clone(), objects: o.objects, utxos }).map_err(e)?.len();
        println!("SELF-CHECK {p}: ok, {n} objects against the node");
    }
    Ok(())
}

// ------------------------------------------------- F6 part C: the one-process mode, checked

/// A free loopback port (bound and released).
fn free_port() -> Result<std::net::SocketAddr, String> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    l.local_addr().map_err(|e| e.to_string())
}

async fn wait_api(addr: std::net::SocketAddr, secs: u64) -> Result<(), String> {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(secs) {
        if http_get(addr, "/v1/status").await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(format!("{addr}: no answer within {secs} s"))
}

fn log_lines_of(body: &str) -> Result<String, String> {
    Ok(web::indexer::parse_status(body).map_err(|e| e.to_string())?.log_lines)
}

/// The follower of `indexer-api --node` on `fdir`, started and waited for until its index
/// has `want_lines` log lines: (its address, its stop sender, its task).
async fn start_follower(
    fdir: PathBuf,
    cov: Hash,
    node_url: String,
    start: Option<Hash>,
    want_lines: String,
) -> Result<(std::net::SocketAddr, tokio::sync::oneshot::Sender<()>, tokio::task::JoinHandle<Result<(), String>>), String> {
    use indexer_api::follow::{FollowOpts, Witness, run_following};
    let bind = free_port()?;
    let mut cfg = indexer_api::Config::new(fdir, cov);
    cfg.bind = bind;
    cfg.poll = Duration::from_millis(200);
    cfg.per_client = None;
    // F6 security review S4: a second connection to the same node as the witness (the private simnet has one
    // node; `connect_witness` accepts it there only): every batch is confirmed at Low verbosity before it is
    // persisted, so this run checks that a real node's Low answer agrees with its Full one.
    let fo = FollowOpts { node: Some(node_url.clone()), start, every: Duration::from_millis(500), witness: Witness::Url(node_url) };
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let h = tokio::spawn(run_following(cfg, fo, async move {
        let _ = rx.await;
    }));
    wait_api(bind, 60).await?;
    let t0 = Instant::now();
    loop {
        let (_, b) = http_get(bind, "/v1/status").await?;
        if log_lines_of(&b)? == want_lines {
            break;
        }
        if t0.elapsed() > Duration::from_secs(60) {
            return Err(format!("the follower did not catch up: {b}"));
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    Ok((bind, tx, h))
}

const FOLLOW_PATHS: [&str; 13] = [
    "/v1/instance",
    "/v1/rounds",
    "/v1/rounds/1",
    "/v1/rounds/2",
    "/v1/rounds/3",
    "/v1/rounds/1/purchases",
    "/v1/rounds/2/purchases",
    "/v1/rounds/3/purchases",
    "/v1/rounds/3/shards/0",
    "/v1/rounds/1/anchor-proof",
    "/v1/rounds/2/anchor-proof",
    "/v1/vault",
    "/v1/pools",
];

/// Every `FOLLOW_PATHS` answer of `bind` equals the dev API's, and is a 200.
async fn same_answers(dev_api: std::net::SocketAddr, bind: std::net::SocketAddr, label: &str) -> Result<(), String> {
    for p in FOLLOW_PATHS {
        let (sa, a) = http_get(dev_api, p).await?;
        let (sb, b) = http_get(bind, p).await?;
        if (sa, &a) != (sb, &b) {
            return Err(format!("{label}: {p}: dev API {sa} {} / follower {sb} {}", &a[..a.len().min(200)], &b[..b.len().min(200)]));
        }
        if sa != 200 {
            return Err(format!("{label}: {p}: status {sa}: {a}"));
        }
    }
    Ok(())
}

/// `n` concurrent GETs of round 2's anchor proof (bodies).
async fn concurrent_proofs(bind: std::net::SocketAddr, n: usize) -> Result<Vec<String>, String> {
    let hs: Vec<_> = (0..n).map(|_| tokio::spawn(async move { http_get(bind, "/v1/rounds/2/anchor-proof").await })).collect();
    let mut out = vec![];
    for h in hs {
        let (s, b) = h.await.map_err(|e| e.to_string())??;
        if s != 200 {
            return Err(format!("concurrent request: {s} {b}"));
        }
        out.push(b);
    }
    Ok(out)
}

/// The F6 part C check of `indexer-api --node` (`indexer_api::follow`) on this private node,
/// against the dev run's own API (its indexer driven by the dev loop, its proofs from
/// `NodeAnchor`):
/// 1. a fresh store holding only the bundle, started at the node's genesis: the follower
///    catches up and every endpoint answers byte for byte as the dev API does, the closing
///    round's anchor proof (built from the node) included; `/health` is 200;
/// 2. eight concurrent anchor-proof requests get the same proof;
/// 3. stopped by its signal and started again on the same store with no `--start`: it
///    resumes from its log and still answers the same;
/// 4. negative controls: the same store served without a node has no proof for the closing
///    round (`anchor_unavailable`), so the proof above came from the node; and the follower's
///    network guard refuses this simnet node for a testnet-10 instance.
async fn follow_check(node_url: &str, dev_dir: &std::path::Path, cov: Hash, start: Hash, dev_api: std::net::SocketAddr) -> Result<(), String> {
    let ok = |what: &str| println!("FOLLOW-CHECK ok   {what}");
    let fdir = dev_root().join(format!("follow-{}", lottery::store::to_hex(&cov.as_bytes())));
    let _ = std::fs::remove_dir_all(&fdir);
    std::fs::create_dir_all(&fdir).map_err(|e| e.to_string())?;
    std::fs::copy(dev_dir.join("bundle.json"), fdir.join("bundle.json")).map_err(|e| e.to_string())?;
    let (_, dev_status) = http_get(dev_api, "/v1/status").await?;
    let want = log_lines_of(&dev_status)?;

    // 1.
    let (bind, stop, h) = start_follower(fdir.clone(), cov, node_url.to_string(), Some(start), want.clone()).await?;
    ok(&format!("a fresh store started at the node's genesis caught up ({want} log lines) on {bind}"));
    same_answers(dev_api, bind, "fresh").await?;
    let (_, proof) = http_get(bind, "/v1/rounds/2/anchor-proof").await?;
    let p = web::indexer::parse_anchor_proof(&proof).map_err(|e| e.to_string())?;
    if p.source != web::indexer::ProofSource::Node {
        return Err(format!("round 2's proof is not from the node: {proof}"));
    }
    ok(&format!("{} endpoints byte for byte as the dev API; round 2's anchor proof built from the node ({} bytes)", FOLLOW_PATHS.len(), p.anchor_proof.len() / 2));
    let (hs, hb) = http_get(bind, "/health").await?;
    if hs != 200 || !hb.contains("\"ok\":true") || !hb.contains("\"following\":true") || !hb.contains(&format!("\"witness\":\"{node_url}\"")) {
        return Err(format!("/health: {hs} {hb}"));
    }
    ok("/health 200, following the node, every batch confirmed by the witness (a second connection)");
    // 2.
    let many = concurrent_proofs(bind, 8).await?;
    if many.iter().any(|b| *b != proof) {
        return Err("concurrent anchor-proof requests disagree".into());
    }
    ok("8 concurrent anchor-proof requests: the same proof");
    // 3.
    let _ = stop.send(());
    h.await.map_err(|e| e.to_string())??;
    let lines_file = std::fs::read_to_string(fdir.join("index-log.jsonl")).map_err(|e| e.to_string())?.lines().count();
    ok(&format!("stopped by its signal; the store's log has {lines_file} lines (header included)"));
    let (bind2, stop2, h2) = start_follower(fdir.clone(), cov, node_url.to_string(), None, want.clone()).await?;
    same_answers(dev_api, bind2, "restarted").await?;
    ok("restarted on the same store without --start: resumed from its log, the same answers");
    let _ = stop2.send(());
    h2.await.map_err(|e| e.to_string())??;
    // 4.
    let bind3 = free_port()?;
    let mut cfg = indexer_api::Config::new(fdir.clone(), cov);
    cfg.bind = bind3;
    let plain = tokio::spawn(indexer_api::run(cfg));
    wait_api(bind3, 30).await?;
    let (s3, b3) = http_get(bind3, "/v1/rounds/2/anchor-proof").await?;
    plain.abort();
    if s3 != 404 || !b3.contains("anchor_unavailable") {
        return Err(format!("negative control: the store-only server answered {s3} {b3}"));
    }
    ok("negative control: the same store without a node has no proof for the closing round (anchor_unavailable)");
    match indexer_api::follow::connect(Some(node_url), web::convert::Net::Testnet10).await {
        Err(e) if e.contains("reports network simnet") => ok(&format!("negative control: a testnet-10 instance refuses this node ({e})")),
        Err(e) => return Err(format!("network guard: unexpected error {e}")),
        Ok(_) => return Err("network guard: a testnet-10 instance accepted a simnet node".into()),
    }
    println!("FOLLOW-CHECK: all checks passed");
    Ok(())
}

pub async fn run(o: Opts) -> Result<(), String> {
    let node = Node::start_on(Some(o.node_port))?;
    let r = run_on(&node, &o).await;
    node.shutdown();
    r
}

async fn run_on(node: &Node, o: &Opts) -> Result<(), String> {
    let t0 = Instant::now();
    let c = Arc::new(node.client().await?);
    let w = Wallet::random();
    let start = c.get_block_dag_info().await.map_err(|e| e.to_string())?.pruning_point_hash;
    println!("dev: node genesis (the indexer's start) {start}");

    // ---- 1. round 1, drawn (ground so that the first held combination wins a prize tier).
    let held = combo(0, 2);
    let buys = vec![(0, vec![held, combo(1, 1)]), (1, vec![combo(2, 3)]), (2, vec![combo(3, 1), combo(4, 2), combo(5, 3)]), (5, vec![combo(6, 2)])];
    let cfg = Cfg { id: "DEV", m: 3, v0: 1_000 * KAS as i64, jackpot: 100 * KAS as i64, j_boost: 20 * KAS as i64, pot: 30 * KAS, g: None, buys, withdraw_full: false, edit: Some(short_rounds) };
    let mut log = Log { id: "DEV", ..Default::default() };
    let hb = held.bytes();
    let r = run_round(
        &*c,
        &w,
        &cfg,
        &mut log,
        "a held combination wins a prize tier",
        1.0 / (N as f64 / 100.0),
        move |s| tier_of(&hb, &win6(s)).is_some(),
        move |s| format!("win {:?}+{}, held {:?} tier {:?}", s.main, s.bonus, hb, tier_of(&hb, &win6(s))),
    )
    .await?;
    let cov = r.cov;
    let t = r.t.clone();
    println!("dev: round 1 drawn; winning {:?}; pools (tier, value) {:?}", r.rr.win, r.rr.pools.iter().map(|p| (p.tier, p.value)).collect::<Vec<_>>());

    // ---- 2. the store: the launch bundle and the index, from the node's genesis.
    let dir = dev_root().join(lottery::store::to_hex(&cov.as_bytes()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let b = lottery::bundle::Bundle::new(&r.g, &r.gf, vec![], None)?;
    let v = lottery::bundle::verify(&b)?;
    if v.cov != cov {
        return Err(format!("the bundle's covenant ID {} is not the instance's {cov}", v.cov));
    }
    indexer::write_atomic(&dir.join("bundle.json"), &lottery::store::write(&b, lottery::store::BUNDLE)?)?;
    let mut x = Indexer::open(Some(&dir), t.clone(), cov, Some(v.genesis_states.clone()), start, 0)?;
    catch_up(&c, &mut x).await?;
    if x.ix.round(1).and_then(|r| r.draw.as_ref()).is_none() {
        return Err("the index does not show round 1's draw".into());
    }

    // ---- 3. round 2: buys, then close() (closing).
    let reserve = r.g.reserve();
    let mut fund = r.fund.clone();
    for (i, bundle) in [(0, vec![combo(7, 1)]), (3, vec![combo(8, 2), combo(9, 2)]), (3, vec![combo(10, 1)])] {
        fund = buy(&c, &w, &mut x, &t, reserve, 2, i, bundle, fund).await?;
    }
    fund = close(&c, &w, &mut x, &t, 2, fund).await?;

    // ---- 4. round 3: buys (open).
    let m3 = x.ix.round(3).and_then(|r| r.m).unwrap_or(2) as u8;
    for (i, bundle) in [(4, vec![combo(11, 1)]), (4, vec![combo(12, m3)]), (9, vec![combo(13, 1), combo(14, 1)])] {
        fund = buy(&c, &w, &mut x, &t, reserve, 3, i, bundle, fund).await?;
    }
    // Past round 2's D (and 60 blocks on), so its anchor proof can be built.
    let close2 = x.ix.round(2).and_then(|r| r.close.clone()).ok_or("round 2 not closed in the index")?;
    let seed_offset = live_where(&x, |s| matches!(s, State::Controller(_))).find_map(|l| if let Some(State::Controller(cc)) = &l.state { Some(cc.seed_offset) } else { None }).unwrap_or(600);
    let d2 = lottery::game::anchor_daa(close2.marker_daa as i64, seed_offset) as u64;
    mine_past_daa(&*c, &w.addr, d2 + 60).await?;
    mine(&*c, &w.addr, 5).await?;
    catch_up(&c, &mut x).await?;
    let _ = fund;

    // ---- 5. the indexer API over the store.
    let mut acfg = indexer_api::Config::new(dir.clone(), cov);
    acfg.bind = o.api_bind;
    acfg.poll = Duration::from_millis(300);
    // Every dev and e2e page shares 127.0.0.1: no per-client limit on the private dev run.
    acfg.per_client = None;
    let starts: Starts = Default::default();
    note_closes(&x, &starts);
    acfg.anchor = Some(Arc::new(NodeAnchor { c: c.clone(), starts: starts.clone() }));
    let api = tokio::spawn(indexer_api::run(acfg));
    let spent: crate::faucet::Spent = Default::default();
    let (round_tx, mut round_rx) = tokio::sync::mpsc::channel::<crate::faucet::DevRound>(1);
    let faucet = crate::faucet::serve(o.faucet_bind, c.clone(), Wallet { sk: w.sk, addr: w.addr.clone() }, spent.clone(), Some(round_tx)).await?;
    // The page's default referrer (spec 6.5, 7: the site owner's address) for the dev run: a
    // fresh address whose key is dropped at once (nobody spends what it receives on this
    // throwaway node); only its address goes into current.json.
    let owner = Wallet::random().addr.to_string();
    let info = serde_json::json!({
        "network": "simnet",
        "api": format!("http://{}/v1/", o.api_bind),
        "wrpcBorsh": node.url,
        "faucet": format!("http://{}/faucet", o.faucet_bind),
        "referralAddress": owner,
        "startedMs": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
        "covenantId": lottery::store::to_hex(&cov.as_bytes()),
        "store": dir.display().to_string(),
        "rounds": { "1": "drawn", "2": "closing", "3": "open" },
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    self_check(&c, o.api_bind, cov, &w.addr.to_string()).await.map_err(|e| format!("self-check: {e}"))?;
    std::fs::write(dev_root().join("current.json"), serde_json::to_string_pretty(&info).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    println!("dev: ready in {:.0} s: {}", t0.elapsed().as_secs_f64(), info);
    println!("dev: {} checks passed in round 1 ({} transactions)", log.checks.len(), log.txs.len());
    sn::print_report(&[log]);
    if o.follow_check {
        let r = follow_check(&node.url, &dir, cov, start, o.api_bind).await;
        api.abort();
        faucet.abort();
        let _ = c.disconnect().await;
        return r;
    }

    // ---- 6. keep the chain moving and the index following.
    let until = o.run_for.map(|d| Instant::now() + d);
    let mut stop = Box::pin(tokio::signal::ctrl_c());
    loop {
        if until.is_some_and(|u| Instant::now() >= u) {
            println!("dev: --for elapsed; stopping");
            break;
        }
        if api.is_finished() {
            return Err(format!("the indexer API stopped: {:?}", api.await));
        }
        tokio::select! {
            _ = &mut stop => { println!("dev: Ctrl-C; stopping"); break; }
            _ = tokio::time::sleep(Duration::from_millis(o.block_ms)) => {}
            Some(req) = round_rx.recv() => {
                if let Some(to) = req.mine {
                    // The void's blocks go to a throwaway address (its key dropped at once), not the dev wallet.
                    let pay = if to == crate::faucet::MineTo::Void { Wallet::random().addr } else { w.addr.clone() };
                    let r = mine_to(&c, &mut x, &starts, to, &pay).await;
                    if let Err(e) = &r {
                        eprintln!("dev mine: {e}");
                    }
                    let _ = req.reply.send(r);
                    continue;
                }
                let target = req.target.map(|(main, bonus)| Combo::new(main, bonus));
                match &target {
                    Some(tc) => println!("dev round: the page's combination {:?}+{} to win", tc.main, tc.bonus),
                    None => println!("dev round: close the open round only"),
                }
                let r = advance(&c, &w, &mut x, &t, &starts, &spent, target).await;
                if let Err(e) = &r {
                    eprintln!("dev round: {e}");
                }
                let _ = req.reply.send(r);
                continue;
            }
        }
        mine_one(&*c, &w.addr).await?;
        if let Err(e) = catch_up(&c, &mut x).await {
            eprintln!("dev: indexer: {e}");
        }
        note_closes(&x, &starts);
    }
    api.abort();
    faucet.abort();
    let _ = c.disconnect().await;
    Ok(())
}
