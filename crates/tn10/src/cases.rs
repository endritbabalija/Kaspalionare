//! The probe cases. Each case funds its own setup outputs, submits the probe, then sweeps
//! every live output back to our address and waits for the sweep, so cases are independent.

use crate::probe::*;
use kaspa_consensus_core::tx::{GenesisCovenantGroup, TransactionOutput};

pub const HEADER: &str = "| case | compute | transient | storage | priority mass | inputs | fee (KAS) | result | tx id |\n|---|---|---|---|---|---|---|---|---|";

async fn fund(ctx: &Ctx) -> Result<Utxo, String> {
    ctx.funds().await?.into_iter().next().ok_or_else(|| "no confirmed funds".to_string())
}

/// Submit a setup transaction (must be accepted) and return its outputs.
async fn setup(ctx: &mut Ctx, name: &str, outs: &[TransactionOutput], genesis: &[GenesisCovenantGroup]) -> Result<Vec<Utxo>, String> {
    let f = fund(ctx).await?;
    let s = ctx.build(&[p2pk_in(&f)], outs, genesis)?;
    if !ctx.submit(&format!("setup: {name}"), &s, false).await? {
        return Err(format!("setup for {name} rejected"));
    }
    Ok(outs_of(&s))
}

/// Submit the probe, then sweep what is left: the probe's outputs if accepted, else the setup outputs.
async fn probe_and_sweep(ctx: &mut Ctx, name: &str, s: Sent, setup_left: Vec<Utxo>, spent_from_setup: &[usize]) -> Result<(), String> {
    let ok = ctx.submit(name, &s, true).await?;
    let mut left: Vec<Utxo> = if ok {
        let mut v = outs_of(&s);
        v.extend(setup_left.iter().enumerate().filter(|(i, _)| !spent_from_setup.contains(i)).map(|(_, u)| u.clone()));
        v
    } else {
        setup_left
    };
    left.retain(|u| u.entry.amount > 0);
    ctx.sweep(left).await
}

pub async fn baseline(ctx: &mut Ctx) -> Result<(), String> {
    let f = fund(ctx).await?;
    let s = ctx.build(&[p2pk_in(&f)], &[plain(KAS, &ctx.spk())], &[])?;
    probe_and_sweep(ctx, "baseline P2PK -> P2PK", s, vec![], &[]).await
}

/// Storage mass just under and over 100,000 and 500,000 from one plain output.
pub async fn storage_bounds(ctx: &mut Ctx) -> Result<(), String> {
    for target in [99_000u64, 101_000, 499_000, 501_000] {
        let f = fund(ctx).await?;
        // One plain output of C / target; the large input and change make relief and the change
        // term negligible, so the transaction's storage mass lands within a few grams of `target`.
        let v = value_for_storage(target);
        let s = ctx.build(&[p2pk_in(&f)], &[plain(v, &ctx.spk())], &[])?;
        probe_and_sweep(ctx, &format!("storage ~{target}: one plain output of {:.6} KAS", v as f64 / KAS as f64), s, vec![], &[])
            .await?;
    }
    Ok(())
}

/// Transient mass about 110,000 / 990,000 / 1,010,000 grams from two padded P2SH inputs.
pub async fn transient_bounds(ctx: &mut Ctx) -> Result<(), String> {
    for target in [110_000u64, 990_000, 1_010_000] {
        let so = setup(ctx, &format!("transient ~{target}"), &[padded_out(KAS), padded_out(KAS)], &[]).await?;
        let change = so[2].clone();
        // Transient mass is 4 grams per serialized byte: size the two signature scripts.
        let mut each = (target as usize / 4).saturating_sub(400) / 2;
        let mk = |each: usize| vec![pad_in(&so[0], each, 0), pad_in(&so[1], each, 0), p2pk_in(&change)];
        let mut s = ctx.build(&mk(each), &[], &[])?;
        for _ in 0..20 {
            let diff = target as i64 - s.m.transient as i64;
            if diff.abs() < 400 {
                break;
            }
            each = (each as i64 + diff / 8) as usize;
            s = ctx.build(&mk(each), &[], &[])?;
        }
        probe_and_sweep(ctx, &format!("transient ~{target}: two padded inputs of {each} B"), s, so, &[0, 1, 2]).await?;
    }
    Ok(())
}

/// Dust floor: single plain and covenant outputs at the values the sources name.
pub async fn dust(ctx: &mut Ctx) -> Result<(), String> {
    for v in [1_900_000u64, 2_000_000, 10_000_000] {
        let f = fund(ctx).await?;
        let s = ctx.build(&[p2pk_in(&f)], &[plain(v, &ctx.spk())], &[])?;
        probe_and_sweep(ctx, &format!("dust: plain output {:.3} KAS", v as f64 / KAS as f64), s, vec![], &[]).await?;
    }
    for v in [8_000_000u64, 10_000_000, 40_000_000] {
        let f = fund(ctx).await?;
        let s = ctx.build(&[p2pk_in(&f)], &[padded_out(v)], &[GenesisCovenantGroup::new(0, vec![0])])?;
        probe_and_sweep(ctx, &format!("dust: covenant output {:.3} KAS", v as f64 / KAS as f64), s, vec![], &[]).await?;
    }
    Ok(())
}

/// buy() shape at M0's minimum deposit: fresh shard (successor at R + price), large funding and
/// change (no input relief), with and without a referral output.
pub async fn buy(ctx: &mut Ctx) -> Result<(), String> {
    let r = r_min(16);
    let price = KAS / 10;
    for (label, ticket, referral) in [("no referral", 8_438_338u64, None), ("referral 0.1 KAS", 10_746_218u64, Some(KAS / 10))] {
        let so = setup(ctx, &format!("buy {label}"), &[padded_out(2 * r)], &[GenesisCovenantGroup::new(0, vec![0])]).await?;
        let cov = so[0].entry.covenant_id.expect("genesis id");
        let ins = vec![pad_in(&so[0], SHARD_SIG_LEN, SHARD_BUDGET), p2pk_in(&so[1])];
        let mut outs = vec![cont(r + price - referral.unwrap_or(0), 0, cov), cont(ticket, 0, cov)];
        if let Some(rf) = referral {
            outs.push(plain(rf, &ctx.spk()));
        }
        // Exact design shape: shard', ticket, optional referral, one change output (built by `build`).
        let s = ctx.build(&ins, &outs, &[])?;
        probe_and_sweep(ctx, &format!("buy(), R = {:.4}, d at M0 minimum, {label}", r as f64 / KAS as f64), s, so, &[0, 1]).await?;
    }
    Ok(())
}

/// claim() shapes: last claim (payout = deposit) and a non-final bundle claim (payout = prize).
pub async fn claim(ctx: &mut Ctx) -> Result<(), String> {
    let r = r_min(16);
    let d = 8_438_338u64;
    // Last claim, prize 0: pool R -> R, payout d. The pool and a minimum-deposit ticket do not
    // fit one setup transaction (4C/R + 4C/d > 500,000), so each is created by its own chained
    // genesis transaction.
    let t = setup(ctx, "claim last: ticket", &[padded_out(d)], &[GenesisCovenantGroup::new(0, vec![0])]).await?;
    let p = ctx.build(&[p2pk_in(&t[1])], &[padded_out(r)], &[GenesisCovenantGroup::new(0, vec![0])])?;
    if !ctx.submit("setup: claim last: pool", &p, false).await? {
        return Err("claim setup rejected".into());
    }
    let po = outs_of(&p);
    let cov = po[0].entry.covenant_id.unwrap();
    // [pool, ticket, funding]
    let so = vec![po[0].clone(), t[0].clone(), po[1].clone()];
    let ins = vec![pad_in(&so[0], SHARD_SIG_LEN, SHARD_BUDGET), pad_in(&so[1], 200, 1), p2pk_in(&so[2])];
    let s = ctx.build(&ins, &[cont(r, 0, cov), plain(d, &ctx.spk())], &[])?;
    probe_and_sweep(ctx, "claim(), last claim, prize 0, payout = d", s, so, &[0, 1, 2]).await?;
    // Non-final claim on a 2-combination bundle at the measured minimum prize.
    let prize = 4_251_502u64;
    let so = setup(ctx, "claim non-final", &[padded_out(r + prize), padded_out(2 * d)], &[GenesisCovenantGroup::new(0, vec![0, 1])]).await?;
    let cov = so[0].entry.covenant_id.unwrap();
    let ins = vec![pad_in(&so[0], SHARD_SIG_LEN, SHARD_BUDGET), pad_in(&so[1], 200, 1), p2pk_in(&so[2])];
    let s = ctx.build(&ins, &[cont(r, 0, cov), cont(2 * d, 1, cov), plain(prize, &ctx.spk())], &[])?;
    probe_and_sweep(ctx, "claim(), non-final, b = 2, minimum prize", s, so, &[0, 1, 2]).await
}

/// close() shape at K with R at the M0 minimum: K + 1 covenant inputs with M0 sigscript sizes
/// and budgets, K + 2 fresh R-outputs, caller change.
pub async fn close(ctx: &mut Ctx, k: usize) -> Result<(), String> {
    let r = r_min(k);
    let mut outs = vec![padded_out(2 * r)];
    outs.extend((0..k).map(|_| padded_out(2 * r)));
    let group = GenesisCovenantGroup::new(0, (0..=k as u32).collect());
    let so = setup(ctx, &format!("close K = {k}"), &outs, &[group]).await?;
    let cov = so[0].entry.covenant_id.unwrap();
    let (ctl_sig, ctl_budget) = controller_sig(k);
    let mut ins = vec![pad_in(&so[0], ctl_sig, ctl_budget)];
    ins.extend((1..=k).map(|i| pad_in(&so[i], SHARD_SIG_LEN, 1)));
    ins.push(p2pk_in(&so[k + 1]));
    let mut pouts = vec![cont(r, 0, cov), cont(r, 0, cov)];
    pouts.extend((1..=k).map(|i| cont(r, i as u16, cov)));
    let s = ctx.build(&ins, &pouts, &[])?;
    let spent: Vec<usize> = (0..=k + 1).collect();
    probe_and_sweep(ctx, &format!("close(), K = {k}, R = {:.4}, controller sig {ctl_sig} B, shard sig {SHARD_SIG_LEN} B", r as f64 / KAS as f64), s, so, &spent)
        .await
}

/// M1 (spec M-e): the real shard script. A genesis transaction creates one shard of the real
/// template (fresh tree, deadline far ahead); the probe is a real `buy()` of B_MAX combinations
/// with a referral output, built by the `lottery` builder and checked locally first. The shard
/// successor and the ticket have no spend path in M1 and stay on testnet; the referral and
/// change outputs are swept back.
pub async fn buy_m1(ctx: &mut Ctx) -> Result<(), String> {
    use lottery::buy::{BuyCase, Templates, b_max};
    use lottery::combo::{Combo, unrank};
    use lottery::state::Payout;

    use kaspa_rpc_core::api::rpc::RpcApi;
    let daa = ctx.client.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    let bm = b_max();
    let mut c = BuyCase::new(bm);
    c.shard.deadline = (daa + 10_000_000) as i64;
    c.shard_daa = daa;
    c.bundle = (0..bm as u32).map(|i| Combo::new(unrank(7 + i * 13_001), 1 + i as u8)).collect();
    c.with_ref = true;
    let me = Payout::Schnorr(secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &ctx.sk).unwrap().x_only_public_key().0.serialize());
    c.referrer = me;
    c.payout = me;
    c.funding_sk = ctx.sk;
    let t = Templates::get(bm);

    // Setup: genesis of the instance covenant with one fresh shard at R.
    let shard_out = plain(c.shard_value(), &t.shard_spk(&c.shard));
    let f = fund(ctx).await?;
    let s = ctx.build(&[p2pk_in(&f)], &[shard_out], &[GenesisCovenantGroup::new(0, vec![0])])?;
    if !ctx.submit("setup: buy-m1 genesis shard", &s, true).await? {
        return Err("buy-m1 setup rejected".into());
    }
    let so = outs_of(&s);
    let funding = so[1].clone();
    c.cov = so[0].entry.covenant_id.ok_or("no genesis covenant id")?;
    c.funding = vec![funding.entry.amount];

    let mut b = c.build();
    b.outpoints = vec![so[0].op, funding.op];
    let fin = b.finish();
    fin.run().map_err(|(i, e)| format!("local engine rejects buy-m1 at input {i}: {e:?}"))?;
    let mut tx = fin.tx.clone();
    tx.finalize();
    let m = harness::masses(&tx, &fin.entries);
    let sent = Sent { fee: fin.fee(), tx, m };
    let ok = ctx
        .submit(&format!("buy() M1 real shard script, b = {bm}, referral, d = 0.11 KAS, R = {:.2} KAS", c.reserve as f64 / KAS as f64), &sent, true)
        .await?;
    if ok {
        let outs = outs_of(&sent);
        ctx.sweep(vec![outs[2].clone(), outs[3].clone()]).await?;
    } else {
        ctx.sweep(vec![funding]).await?;
    }
    Ok(())
}

/// M2 (spec M2-e): genesis plus a real K = 16 `close()`, both built by the `lottery` builder
/// with the real controller, params and shard scripts and checked in the local v2.1.0 engine
/// before submission.
/// - Genesis (D31, D50) creates the round 1 controller (drawn = 1, J(1) = 10 KAS scaled down
///   from the fixture, boost pot 0, acc 2 KAS), the params, 16 fresh shards at
///   R = TEST_RESERVE, the vault (V0 = V_rec = 60 KAS) and the drawer, with T a little below
///   the current DAA so that `close()` is final now. Since M3 the vault (V0 + R) and the
///   drawer (R) also stay locked until a `draw()` spends them.
///   No fabricated controller is needed: genesis writes drawn = 1 (D31, D33).
/// - The probe waits for genesis to be mined, reads the creation DAA score of its outputs
///   (one block, so the change output's score is every output's), then spends the
///   controller, params, 16 shards and the genesis change (caller funding) in `close()`.
///   Zero sales, so C_close = 0; the caller output is the change minus R and the fee.
/// - Round 2's controller, params, 16 shards and the round 1 marker have no spend path yet
///   (round 2 is not drawn; the marker is the D28 placeholder), so 2 + 19 R stay locked on
///   testnet. The caller output is ours.
pub async fn close_m2(ctx: &mut Ctx) -> Result<(), String> {
    use kaspa_rpc_core::api::rpc::RpcApi;
    use lottery::buy::{Templates, b_max};
    use lottery::close::{CloseCase, FundingIn, ShardIn};
    use lottery::genesis::{GenesisCase, covenant_id};
    use lottery::state::Payout;

    const K: usize = 16;
    let daa = ctx.client.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    let t = Templates::get(b_max());
    let me = Payout::Schnorr(secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &ctx.sk).unwrap().x_only_public_key().0.serialize());
    // Round 1 deadline T: below the current DAA, so lock_time = T is final at once.
    let deadline = daa as i64 - 1_000;

    // ---- genesis
    let f = fund(ctx).await?;
    let mut g = GenesisCase::new_with(K, vec![(f.op, f.entry.clone())], &t);
    g.funding_sk = ctx.sk;
    g.set_owner(&me);
    // M3 (D50): genesis also creates the vault holding V0 = v_rec. The fixture's
    // V0 = 10,000,000 KAS is not fundable on testnet, so scale J(1) and V0 down while keeping
    // L(1) = J + G2 (m - 1) <= floor(V0 x cap / 10^4): J = 10 KAS, L = 10.9 KAS, V0 = 60 KAS.
    g.set_jackpot(10 * KAS as i64);
    g.set_vault(60 * KAS as i64);
    g.controller.deadline = deadline;
    for s in &mut g.shards {
        s.deadline = deadline;
    }
    let mut gf = g.build_with(t.clone()).finish();
    g.fee = lottery_fee(&gf);
    gf = g.build_with(t.clone()).finish();
    let cov = covenant_id(&gf);
    let gs = sent_of(&gf);
    if !ctx.submit(&format!("setup: close-m2 genesis, K = {K}, real templates, R = {:.2} KAS", g.reserve() as f64 / KAS as f64), &gs, true).await? {
        return Err("close-m2 genesis rejected".into());
    }
    // Creation DAA score of the genesis outputs (all in one block): the change output's.
    let gid = gs.tx.id();
    let change_idx = gs.tx.outputs.len() - 1;
    let change = ctx
        .funds()
        .await?
        .into_iter()
        .find(|u| u.op.transaction_id == gid && u.op.index as usize == change_idx)
        .ok_or("genesis change not found in the UTXO index")?;
    let gdaa = change.entry.block_daa_score;
    println!("genesis {gid} mined at DAA {gdaa}; covenant id {cov}");

    // ---- close()
    let mut c = CloseCase::new_with(K, &t);
    c.controller = g.controller.clone();
    c.controller_daa = gdaa;
    c.pot = g.pot;
    c.params = g.params.clone();
    c.params_daa = gdaa;
    c.shards = g.shards.iter().map(|s| ShardIn { state: s.clone(), daa: gdaa }).collect();
    c.funding = vec![FundingIn { value: change.entry.amount, daa: gdaa }];
    c.funder = me;
    c.funding_sk = ctx.sk;
    c.cov = cov;
    c.lock_time = deadline as u64;
    // Genesis outputs 0..=K+1 are the controller, the params and the shards; the vault (K + 2)
    // and the drawer (K + 3) come after them (D50) and are not spent by close().
    let ops = |b: &mut lottery::buy::Built| {
        b.outpoints = (0..=K as u32 + 1).map(|i| kaspa_consensus_core::tx::TransactionOutpoint::new(gid, i)).collect();
        b.outpoints.push(change.op);
    };
    let mut b = c.build_with(t.clone());
    ops(&mut b);
    let cf = b.finish();
    c.fee = lottery_fee(&cf);
    let mut b = c.build_with(t.clone());
    ops(&mut b);
    let cf = b.finish();
    cf.run().map_err(|(i, e)| format!("local engine rejects close-m2 at input {i}: {e:?}"))?;
    assert!(cf.fee() >= lottery_fee(&cf) - 10_000, "fee below the priority fee");
    let cs = sent_of(&cf);
    ctx.submit(
        &format!("close() M2 real scripts, K = {K}, B_MAX = {}, zero sales, R = {:.2} KAS", t.b_max, c.reserve() as f64 / KAS as f64),
        &cs,
        true,
    )
    .await?;
    // The caller output (last) is ours and stays as funds; the genesis change was spent.
    Ok(())
}

/// Wait until the virtual DAA score passes `target` (lock times are absolute DAA scores, R3).
async fn wait_daa(ctx: &Ctx, target: u64, what: &str) -> Result<(), String> {
    use kaspa_rpc_core::api::rpc::RpcApi;
    let start = std::time::Instant::now();
    loop {
        let now = ctx.client.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
        if now > target {
            println!("virtual DAA {now} > {what} {target} after {:.0} s", start.elapsed().as_secs_f64());
            return Ok(());
        }
        if start.elapsed() > std::time::Duration::from_secs(1_800) {
            return Err(format!("{what} {target} not reached within 30 min (virtual DAA {now})"));
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

/// The virtual's selected parent (sink). Taken before a submission, it is a start hash for
/// `merged`: the chain block that merges the transaction comes after it.
async fn sink(ctx: &Ctx) -> Result<kaspa_rpc_core::RpcHash, String> {
    use kaspa_rpc_core::api::rpc::RpcApi;
    Ok(ctx.client.get_block_dag_info().await.map_err(|e| e.to_string())?.sink)
}

/// The node's UTXO entry of `op` (script `spk`), once the transaction that created it has
/// been merged by a chain block, and only if the entry carries that chain block's DAA
/// score (O-S4-4, Stage 5 review item 6).
///
/// Why: an output accepted only by the virtual carries the virtual's DAA score
/// (`calculate_utxo_state(.., virtual_daa_window.daa_score)`, rusty-kaspa v2.1.0
/// `consensus/src/pipeline/virtual_processor/processor.rs:571-582`); the chain block that
/// later merges the transaction re-adds the output with its own header DAA score
/// (`pov_daa_score = header.daa_score`, same file `:446-455`). An entry from a mempool
/// parent has DAA `UNACCEPTED_DAA_SCORE` = u64::MAX (`consensus/core/src/constants.rs:51`),
/// which `OpTxInputDaaScore` pushes as `block_daa_score as i64` = -1
/// (`crypto/txscript/src/opcodes/mod.rs:1130`). draw() check 19 requires `own_daa` to equal
/// the maximum input DAA score, so a draw built on a virtual-only DAA score fails on the node.
///
/// The accepting chain block comes from `GetVirtualChainFromBlock(start, accepted ids)`,
/// its DAA score from `GetBlock`; the loop waits until the UTXO index entry equals it.
async fn merged(ctx: &Ctx, start: kaspa_rpc_core::RpcHash, op: kaspa_consensus_core::tx::TransactionOutpoint, spk: &kaspa_consensus_core::tx::ScriptPublicKey) -> Result<Utxo, String> {
    use kaspa_rpc_core::api::rpc::RpcApi;
    let addr = kaspa_txscript::extract_script_pub_key_address(spk, kaspa_addresses::Prefix::Testnet).map_err(|e| e.to_string())?;
    let t0 = std::time::Instant::now();
    let mut last = String::new();
    while t0.elapsed() < std::time::Duration::from_secs(300) {
        let vc = ctx.client.get_virtual_chain_from_block(start, true, None).await.map_err(|e| e.to_string())?;
        let acc = vc.accepted_transaction_ids.iter().find(|a| a.accepted_transaction_ids.contains(&op.transaction_id));
        let note = match acc {
            None => "not yet merged by a chain block".to_string(),
            Some(a) => {
                let daa = ctx.client.get_block(a.accepting_block_hash, false).await.map_err(|e| e.to_string())?.header.daa_score;
                let found = crate::net::utxos(&ctx.client, &addr)
                    .await?
                    .into_iter()
                    .find(|e| kaspa_consensus_core::tx::TransactionOutpoint::from(e.outpoint.clone()) == op);
                match found {
                    Some(e) => {
                        let ne: kaspa_consensus_core::tx::UtxoEntry = e.utxo_entry.into();
                        if ne.block_daa_score == daa {
                            println!("{op}: merged by chain block {} at DAA {daa}; node entry DAA {} (after {:.1} s)", a.accepting_block_hash, ne.block_daa_score, t0.elapsed().as_secs_f64());
                            return Ok(Utxo { op, entry: ne, padded: false });
                        }
                        format!("node entry DAA {} != accepting chain block DAA {daa}", ne.block_daa_score)
                    }
                    None => format!("merged at DAA {daa} but not in the UTXO index"),
                }
            }
        };
        if note != last {
            println!("{op}: waiting ({note})");
            last = note;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    Err(format!("{op}: no chain-merged UTXO entry within 300 s ({last})"))
}

/// M3 (spec M3-g): genesis -> buy -> close -> `draw()` at K = 16 with the real scripts, each
/// built by the `lottery` builder and checked in the local v2.1.0 engine before submission.
/// - Genesis as close-m2 (J(1) = 10 KAS, V0 = 60 KAS) but with T = virtual DAA + 600: buy()
///   needs the shard created before T (shard.sil check 6), close() needs lock_time = T final,
///   so the probe waits for T (about a minute at 10 BPS) between the buy and the close.
/// - buy(): shard 0 sells the round 1 jackpot combination of the TEST-ONLY seed below, so
///   the draw creates a jackpot pool (and lower-tier pools from the 1 KAS sale).
/// - close(): K = 16, funded by the buy change; it sets seed_ref = own + seed_offset (600 DAA,
///   about a minute at 10 BPS), so the probe waits for the virtual DAA score to pass it.
/// - draw_test_seed(): drawer, marker, vault, controller and one funding input (the close
///   caller output); lock_time = seed_ref. Every input's entry (and creation DAA score) is
///   the node's own, read only after a chain block merged the creating transaction
///   (`merged`, O-S4-4), re-read just before submission, and the draw is rebuilt if it moved.
/// Locked on testnet afterwards: round 2's objects, the drawer', vault', controller', the
/// new pools (no claim path in M3) and the ticket. The caller output is ours.
/// The round-1 state the M3/M4 chain probes reach: templates, covenant ID, the
/// buy (its ticket is output 1 of `bs`), the draw case and transaction, and the round
/// result read back from the draw transaction bytes. `*_start` are sink hashes taken before
/// the submissions (for `merged`).
pub struct Drawn {
    pub t: std::sync::Arc<lottery::buy::Templates>,
    pub cov: kaspa_consensus_core::Hash,
    pub bc: lottery::buy::BuyCase,
    pub bs: Sent,
    pub b_start: kaspa_rpc_core::RpcHash,
    pub d: lottery::draw::DrawCase,
    pub ds: Sent,
    pub d_start: kaspa_rpc_core::RpcHash,
    pub rr: lottery::draw::RoundResult,
    /// M5 (`vault-m5`): the deposit and withdrawal requests sent before the buy, each with
    /// its sink start hash.
    pub dep: Option<(lottery::vault::DepositCase, Sent, kaspa_rpc_core::RpcHash)>,
    pub wdr: Option<(lottery::vault::WithdrawCase, Sent, kaspa_rpc_core::RpcHash)>,
}

/// M5 vault requests sent between genesis and the buy (`round1_drawn_with`): a deposit of
/// `d` sompi and a partial withdrawal of `w` shares of the deployer position.
#[derive(Clone, Copy)]
pub struct VaultRequests {
    pub d: i64,
    pub w: i64,
}

pub async fn draw_m3(ctx: &mut Ctx) -> Result<(), String> {
    match round1_drawn(ctx, "draw-m3").await {
        Ok(_) => Ok(()),
        // As before the M4 refactor: a rejected draw is a report row, not a probe error.
        Err(e) if e == "draw-m3 draw rejected" => Ok(()),
        Err(e) => Err(e),
    }
}

/// Genesis -> buy -> close -> draw_test_seed() of round 1 with the real scripts (the draw-m3
/// sequence); `tag` names the probe in every report row and error.
pub async fn round1_drawn(ctx: &mut Ctx, tag: &str) -> Result<Drawn, String> {
    round1_drawn_with(ctx, tag, None).await
}

/// `round1_drawn` with, if `vr` is set, a `request_deposit()` and a `request_withdraw()`
/// (both with the real vault and position scripts) between genesis and the buy, so the draw
/// converts both and writes a settlement record (spec M5-f).
pub async fn round1_drawn_with(ctx: &mut Ctx, tag: &str, vr: Option<VaultRequests>) -> Result<Drawn, String> {
    use kaspa_consensus_core::tx::TransactionOutpoint as Op;
    use kaspa_rpc_core::api::rpc::RpcApi;
    use lottery::buy::{BuyCase, Templates, b_max};
    use lottery::close::{CloseCase, FundingIn, ShardIn};
    use lottery::combo::{Combo, unrank};
    use lottery::draw::{DrawCase, RoundResult};
    use lottery::game::seed_numbers;
    use lottery::genesis::{GenesisCase, covenant_id};
    use lottery::smt::Tree;
    use lottery::state::Payout;

    const K: usize = 16;
    // TEST-ONLY seed (D42): randomness is parked; this is not a seed source.
    const TEST_ONLY_SEED: [u8; 32] = [0x5e; 32];
    let daa = ctx.client.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    let t = Templates::get(b_max());
    let me = Payout::Schnorr(secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &ctx.sk).unwrap().x_only_public_key().0.serialize());
    let deadline = daa as i64 + 600;

    // ---- genesis (as close_m2, T ahead)
    let f = fund(ctx).await?;
    let mut g = GenesisCase::new_with(K, vec![(f.op, f.entry.clone())], &t);
    g.funding_sk = ctx.sk;
    g.set_owner(&me);
    g.set_jackpot(10 * KAS as i64);
    g.set_vault(60 * KAS as i64);
    g.controller.deadline = deadline;
    for s in &mut g.shards {
        s.deadline = deadline;
    }
    let mut gf = g.build_with(t.clone()).finish();
    g.fee = lottery_fee(&gf);
    gf = g.build_with(t.clone()).finish();
    gf.run().map_err(|(i, e)| format!("local engine rejects {tag} genesis at input {i}: {e:?}"))?;
    let cov = covenant_id(&gf);
    let gs = sent_of(&gf);
    let g_start = sink(ctx).await?;
    if !ctx.submit(&format!("setup: {tag} genesis, K = {K}, real templates, R = {:.2} KAS", g.reserve() as f64 / KAS as f64), &gs, true).await? {
        return Err(format!("{tag} genesis rejected"));
    }
    let gid = gs.tx.id();
    let gn = gs.tx.outputs.len() - 1;
    let gchange = merged(ctx, g_start, Op::new(gid, gn as u32), &gs.tx.outputs[gn].script_public_key).await?;
    let gdaa = gchange.entry.block_daa_score;
    println!("genesis {gid} at DAA {gdaa}; covenant id {cov}");

    // ---- M5: request_deposit() and request_withdraw() (vault-m5)
    // The vault the draw spends (outpoint, state, creation DAA, start hash), and the plain
    // UTXO that funds the buy.
    let mut vault_in = (Op::new(gid, g.vault_output() as u32), g.vault.clone(), gdaa, g_start);
    let mut buy_funding = gchange.clone();
    let mut dep = None;
    let mut wdr = None;
    if let Some(vr) = vr {
        use lottery::buy::OwnerKey;
        use lottery::vault::{DepositCase, WithdrawCase};
        // request_deposit: the vault and the genesis change.
        let mut dc = DepositCase::new_with(&t);
        dc.vault = g.vault.clone();
        dc.reserve = g.reserve();
        dc.vault_daa = gdaa;
        dc.d = vr.d;
        dc.owner = me;
        dc.funding = vec![FundingIn { value: gchange.entry.amount, daa: gdaa }];
        dc.cov = cov;
        dc.funding_sk = ctx.sk;
        let dops = vec![vault_in.0, gchange.op];
        let dep_fin = |dc: &DepositCase| {
            let mut b = dc.build_with(t.clone());
            b.outpoints = dops.clone();
            b.finish()
        };
        dc.fee = lottery_fee(&dep_fin(&dc));
        let df = dep_fin(&dc);
        df.run().map_err(|(i, e)| format!("local engine rejects {tag} request_deposit at input {i}: {e:?}"))?;
        let ds = sent_of(&df);
        let d_start = sink(ctx).await?;
        if !ctx.submit(&format!("request_deposit() M5 real vault script: D = {:.2} KAS, request value R", vr.d as f64 / KAS as f64), &ds, true).await? {
            return Err(format!("{tag} request_deposit rejected"));
        }
        let did = ds.tx.id();
        let dn = ds.tx.outputs.len() - 1;
        let dchange = merged(ctx, d_start, Op::new(did, dn as u32), &ds.tx.outputs[dn].script_public_key).await?;
        let dx = dc.expected();
        vault_in = (Op::new(did, 0), dx.vault.clone(), dchange.entry.block_daa_score, d_start);

        // request_withdraw (partial) of the deployer position: the vault', the genesis
        // position and the deposit change as the owner input (our Schnorr P2PK = the owner).
        let mut wc = WithdrawCase::new_with(&t, OwnerKey::Schnorr(ctx.sk));
        wc.vault = dx.vault.clone();
        wc.reserve = g.reserve();
        wc.vault_daa = vault_in.2;
        wc.position = g.position.clone();
        wc.position_daa = gdaa;
        wc.w = vr.w;
        wc.owner_value = dchange.entry.amount;
        wc.owner_daa = dchange.entry.block_daa_score;
        wc.funding = vec![];
        wc.cov = cov;
        wc.funding_sk = ctx.sk;
        let wops = vec![vault_in.0, Op::new(gid, g.position_output() as u32), dchange.op];
        let wd_fin = |wc: &WithdrawCase| {
            let mut b = wc.build_with(t.clone());
            b.outpoints = wops.clone();
            b.finish()
        };
        wc.fee = lottery_fee(&wd_fin(&wc));
        let wf = wd_fin(&wc);
        wf.run().map_err(|(i, e)| format!("local engine rejects {tag} request_withdraw at input {i}: {e:?}"))?;
        let ws = sent_of(&wf);
        let w_start = sink(ctx).await?;
        if !ctx.submit(&format!("request_withdraw() M5 real vault and position scripts: partial, w = {} of {} shares, P2PK Schnorr owner input", vr.w, g.position.shares), &ws, true).await? {
            return Err(format!("{tag} request_withdraw rejected"));
        }
        let wid = ws.tx.id();
        let wn = ws.tx.outputs.len() - 1;
        let wchange = merged(ctx, w_start, Op::new(wid, wn as u32), &ws.tx.outputs[wn].script_public_key).await?;
        vault_in = (Op::new(wid, 0), wc.expected().vault.clone(), wchange.entry.block_daa_score, w_start);
        buy_funding = wchange;
        dep = Some((dc, ds, d_start));
        wdr = Some((wc, ws, w_start));
    }

    // ---- buy(): the jackpot combination of round 1 on shard 0
    let m = g.controller.m;
    let sn = seed_numbers(&cov.as_bytes(), g.controller.round, &TEST_ONLY_SEED, m);
    let mut bc = BuyCase::new(t.b_max).with_tree(Tree::new(m as usize));
    assert_eq!(bc.shard.root, g.shards[0].root, "genesis shard 0 holds the empty tree");
    bc.shard = g.shards[0].clone();
    bc.reserve = g.reserve();
    bc.shard_daa = gdaa;
    bc.cov = cov;
    bc.bundle = vec![Combo::new(unrank(sn.rank), sn.bonus)];
    bc.payout = me;
    bc.funding = vec![buy_funding.entry.amount];
    bc.funding_sk = ctx.sk;
    let bops = vec![Op::new(gid, 2), buy_funding.op];
    let buy_fin = |bc: &BuyCase| {
        let mut b = bc.build_with(t.clone());
        b.outpoints = bops.clone();
        b.finish()
    };
    let bf = buy_fin(&bc);
    bc.fee = lottery_fee(&bf);
    let bf = buy_fin(&bc);
    bf.run().map_err(|(i, e)| format!("local engine rejects {tag} buy at input {i}: {e:?}"))?;
    let bs = sent_of(&bf);
    let b_start = sink(ctx).await?;
    if !ctx.submit(&format!("setup: {tag} buy() of the jackpot combination {:?} + {} on shard 0", unrank(sn.rank), sn.bonus), &bs, true).await? {
        return Err(format!("{tag} buy rejected"));
    }
    let bid = bs.tx.id();
    let bn = bs.tx.outputs.len() - 1;
    let bchange = merged(ctx, b_start, Op::new(bid, bn as u32), &bs.tx.outputs[bn].script_public_key).await?;
    let bdaa = bchange.entry.block_daa_score;
    let tree0 = bc.proofs().2;

    // ---- close(), K = 16, once T is final
    wait_daa(ctx, deadline as u64 + 20, "deadline T").await?;
    let mut c = CloseCase::new_with(K, &t);
    c.controller = g.controller.clone();
    c.controller_daa = gdaa;
    c.pot = g.pot;
    c.params = g.params.clone();
    c.params_daa = gdaa;
    c.shards = g
        .shards
        .iter()
        .enumerate()
        .map(|(i, s)| if i == 0 { ShardIn { state: bc.successor(), daa: bdaa } } else { ShardIn { state: s.clone(), daa: gdaa } })
        .collect();
    c.funding = vec![FundingIn { value: bchange.entry.amount, daa: bdaa }];
    c.funder = me;
    c.funding_sk = ctx.sk;
    c.cov = cov;
    c.lock_time = deadline as u64;
    let mut cops = vec![Op::new(gid, 0), Op::new(gid, 1), Op::new(bid, 0)];
    cops.extend((3..=K as u32 + 1).map(|i| Op::new(gid, i)));
    cops.push(bchange.op);
    let close_fin = |c: &CloseCase| {
        let mut b = c.build_with(t.clone());
        b.outpoints = cops.clone();
        b.finish()
    };
    let cf = close_fin(&c);
    c.fee = lottery_fee(&cf);
    let cf = close_fin(&c);
    cf.run().map_err(|(i, e)| format!("local engine rejects {tag} close at input {i}: {e:?}"))?;
    let x = c.expected();
    let cs = sent_of(&cf);
    let c_start = sink(ctx).await?;
    if !ctx.submit(&format!("setup: {tag} close(), K = {K}, one sale of 1 KAS"), &cs, true).await? {
        return Err(format!("{tag} close rejected"));
    }
    let cid = cs.tx.id();
    let cn = cs.tx.outputs.len() - 1;
    let caller = merged(ctx, c_start, Op::new(cid, cn as u32), &cs.tx.outputs[cn].script_public_key).await?;
    let cdaa = caller.entry.block_daa_score;
    let seed_ref = x.marker.seed_ref;
    println!("close {cid} at DAA {cdaa}; seed_ref {seed_ref}");

    // ---- draw_test_seed()
    let mut d = DrawCase::new_with(K, &t);
    d.drawer = g.drawer.clone();
    d.drawer_daa = gdaa;
    d.marker = x.marker.clone();
    d.marker_daa = cdaa;
    d.vault = vault_in.1.clone();
    d.vault_daa = vault_in.2;
    d.controller = x.controller.clone();
    d.pot = x.values[0] - x.controller.acc as u64 - x.controller.reserve as u64;
    d.controller_daa = cdaa;
    d.trees = (0..K).map(|i| if i == 0 { tree0.clone() } else { Tree::new(m as usize) }).collect();
    let roots = d.marker.roots.clone();
    d.sync_roots();
    assert_eq!(d.marker.roots, roots, "the marker roots are the shard trees' roots");
    d.test_only_seed = TEST_ONLY_SEED;
    d.cov = cov;
    d.funder = me;
    d.funding_sk = ctx.sk;
    d.funding = vec![FundingIn { value: caller.entry.amount, daa: cdaa }];
    d.absorbed = vec![];
    d.fix_lock_time();
    let dops = vec![Op::new(gid, K as u32 + 3), Op::new(cid, K as u32 + 2), vault_in.0, Op::new(cid, 0), caller.op];
    let draw_fin = |d: &DrawCase| {
        let mut b = d.build_with(t.clone());
        b.outpoints = dops.clone();
        b.finish()
    };
    // Wait for the lock time (D51): the virtual DAA score must pass seed_ref.
    wait_daa(ctx, seed_ref as u64 + 20, "seed_ref").await?;
    // O-S4-4 (Stage 5): build the draw only from the node's own UTXO entries of inputs whose
    // creating transaction a chain block has merged (`merged`), then re-read them just
    // before submission; if any entry changed (e.g. a reorganisation moved the merging
    // chain block), rebuild from the new entries instead of submitting.
    let starts = [g_start, c_start, vault_in.3, c_start, c_start];
    let spks: Vec<_> = draw_fin(&d).entries.iter().map(|e| e.script_public_key.clone()).collect();
    let mut built = None;
    for attempt in 1..=3 {
        let mut node = Vec::new();
        for (i, op) in dops.iter().enumerate() {
            node.push(merged(ctx, starts[i], *op, &spks[i]).await?.entry);
        }
        d.drawer_daa = node[0].block_daa_score;
        d.marker_daa = node[1].block_daa_score;
        d.vault_daa = node[2].block_daa_score;
        d.controller_daa = node[3].block_daa_score;
        d.funding = vec![FundingIn { value: node[4].amount, daa: node[4].block_daa_score }];
        d.fix_lock_time();
        let df = draw_fin(&d);
        d.fee = lottery_fee(&df);
        let df = draw_fin(&d);
        if df.entries != node {
            for i in 0..node.len() {
                if df.entries[i] != node[i] {
                    println!(
                        "input {i}: builder entry differs from the node's (amount {} / {}, daa {} / {}, cov {:?} / {:?})",
                        df.entries[i].amount, node[i].amount, df.entries[i].block_daa_score, node[i].block_daa_score, df.entries[i].covenant_id, node[i].covenant_id
                    );
                }
            }
            return Err(format!("{tag}: the builder's input entries do not match the node's beyond the DAA score"));
        }
        df.run().map_err(|(i, e)| format!("local engine rejects {tag} draw at input {i}: {e:?}"))?;
        // Re-read right before submission; rebuild if anything moved.
        let mut again = Vec::new();
        for (i, op) in dops.iter().enumerate() {
            again.push(merged(ctx, starts[i], *op, &spks[i]).await?.entry);
        }
        if again == node {
            for (i, e) in node.iter().enumerate() {
                println!("input {i}: node entry = builder entry (amount {}, daa {})", e.amount, e.block_daa_score);
            }
            built = Some(df);
            break;
        }
        println!("attempt {attempt}: a node entry changed between build and submission; rebuilding");
    }
    let df = built.ok_or(format!("{tag}: node entries kept changing; not submitted"))?;
    let dx = d.expected();
    let ds = sent_of(&df);
    let d_start = sink(ctx).await?;
    let ok = ctx
        .submit(
            &format!(
                "{}draw_test_seed() {} real scripts, K = {K}, a = 0, np = {}, h_J = {}, n2 = {}, fee paid {}, R = {:.2} KAS{}",
                if tag == "draw-m3" || tag == "vault-m5" { "" } else { "setup: " },
                if vr.is_some() { "M5" } else { "M3" },
                dx.np(),
                dx.h_j,
                dx.n2,
                dx.fee.paid,
                d.reserve() as f64 / KAS as f64,
                if dx.rec {
                    format!(
                        ", conversion: D = {:.2} KAS -> mint {} shares (refund {}), W = {} -> wc = {}, out_c = {:.4} KAS, record at output {}",
                        dx.conv.d as f64 / KAS as f64,
                        dx.conv.mint_c,
                        dx.conv.refund,
                        dx.conv.w,
                        dx.conv.wc,
                        dx.conv.out_c as f64 / KAS as f64,
                        dx.record_output().unwrap()
                    )
                } else {
                    String::new()
                }
            ),
            &ds,
            true,
        )
        .await?;
    if !ok {
        return Err(format!("{tag} draw rejected"));
    }
    let rr = RoundResult::from_tx(&ds.tx, &t).map_err(|e| format!("round result: {e}"))?;
    let pools: Vec<(u8, i64, u64)> = rr.pools.iter().map(|p| (p.tier, p.slice, p.value)).collect();
    println!("round result: round {}, win {:?}, D_claim {}, pools (tier, slice, value) {pools:?}", rr.round, rr.win, rr.d_claim);
    ctx.report.push(format!(
        "| round result from the draw tx bytes: round {}, win {:?}, D_claim {}, pools (tier, slice, value) {pools:?} | | | | | | | | `{}` |",
        rr.round,
        rr.win,
        rr.d_claim,
        ds.tx.id()
    ));
    Ok(Drawn { t, cov, bc, bs, b_start, d, ds, d_start, rr, dep, wdr })
}

/// M4 (spec M4-i): genesis -> buy -> close -> draw (the draw-m3 sequence, K = 16, real
/// scripts) -> one `claim()` with the real ticket and pool scripts through the public
/// mempool.
/// - The buy holds the round 1 jackpot combination of the TEST-ONLY seed on a b = 1 ticket
///   (buy output 1), so the draw creates a jackpot pool with slice J and h_J = 1: the claim
///   is FINAL (D56): pool' = R, payout = J + d to our key, the ticket is consumed.
/// - Inputs (S5-5): the ticket (buy output 1), the jackpot pool (its draw output) and one
///   funding input (the draw's caller output), each the node's own entry read only after a
///   chain block merged the creating transaction (`merged`), re-read just before submission
///   (rebuilt if any moved). claim() has no time check (A4), so lock_time = 0.
/// - The claim is built by `lottery::claim::ClaimCase` and checked in the local v2.1.0 engine
///   before submission; the pool state is rebuilt from the draw transaction bytes alone
///   (`RoundResult::pool_state`, R4).
/// Locked on testnet afterwards: round 2's objects, the drawer', vault', controller', the
/// other pools and pool' (R). The payout and the change are ours.
pub async fn claim_m4(ctx: &mut Ctx) -> Result<(), String> {
    use kaspa_consensus_core::tx::TransactionOutpoint as Op;
    use lottery::claim::ClaimCase;
    use lottery::close::FundingIn;
    use lottery::game::ClaimMode;
    use lottery::state::TIER_JACKPOT;

    const TAG: &str = "claim-m4";
    let r = round1_drawn(ctx, TAG).await?;
    let t = r.t.clone();
    let did = r.ds.tx.id();

    // ---- the claim's inputs: ticket, jackpot pool, funding (the draw's caller output)
    let p = r.rr.pools.iter().position(|p| p.tier == TIER_JACKPOT).ok_or("claim-m4: the draw created no jackpot pool")?;
    let pool = r.rr.pool_state(p);
    let pool_op = r.rr.pools[p].outpoint;
    let ticket = r.bc.ticket();
    let bid = r.bs.tx.id();
    let ticket_op = Op::new(bid, 1);
    let caller = r.d.expected().caller_output();
    let fund_op = Op::new(did, caller as u32);
    assert_eq!(r.bs.tx.outputs[1].script_public_key, t.ticket_spk(&ticket), "buy output 1 is the ticket");
    assert_eq!(r.ds.tx.outputs[pool_op.index as usize].script_public_key, t.pool_spk(&pool), "the pool state rebuilt from the draw bytes");
    let ops = vec![ticket_op, pool_op, fund_op];
    let starts = [r.b_start, r.d_start, r.d_start];
    let spks = [r.bs.tx.outputs[1].script_public_key.clone(), r.ds.tx.outputs[pool_op.index as usize].script_public_key.clone(), r.ds.tx.outputs[caller].script_public_key.clone()];

    let mut c = ClaimCase::new(4);
    c.b_max = t.b_max;
    c.ticket = ticket;
    c.pool = pool;
    c.trees = r.d.trees.clone();
    c.idx = 0;
    c.change = true;
    c.lock_time = 0;
    c.cov = r.cov;
    c.funding_sk = ctx.sk;
    let claim_fin = |c: &ClaimCase| {
        let mut b = c.build_with(t.clone());
        b.outpoints = ops.clone();
        b.finish()
    };
    let mut built = None;
    for attempt in 1..=3 {
        let mut node = Vec::new();
        for (i, op) in ops.iter().enumerate() {
            node.push(merged(ctx, starts[i], *op, &spks[i]).await?.entry);
        }
        c.ticket_value = node[0].amount;
        c.ticket_daa = node[0].block_daa_score;
        c.pool_value = node[1].amount;
        c.pool_daa = node[1].block_daa_score;
        c.funding = vec![FundingIn { value: node[2].amount, daa: node[2].block_daa_score }];
        let x = c.expected();
        if !x.rejects.is_empty() {
            return Err(format!("{TAG}: the reference rejects the claim: {:?}", x.rejects));
        }
        if x.mode != ClaimMode::Final {
            return Err(format!("{TAG}: expected a FINAL claim (b = 1), got {:?}", x.mode));
        }
        let cf = claim_fin(&c);
        c.fee = lottery_fee(&cf);
        let cf = claim_fin(&c);
        if cf.entries != node {
            for i in 0..node.len() {
                if cf.entries[i] != node[i] {
                    println!(
                        "input {i}: builder entry differs from the node's (amount {} / {}, daa {} / {}, cov {:?} / {:?})",
                        cf.entries[i].amount, node[i].amount, cf.entries[i].block_daa_score, node[i].block_daa_score, cf.entries[i].covenant_id, node[i].covenant_id
                    );
                }
            }
            return Err(format!("{TAG}: the builder's input entries do not match the node's"));
        }
        cf.run().map_err(|(i, e)| format!("local engine rejects {TAG} claim at input {i}: {e:?}"))?;
        let mut again = Vec::new();
        for (i, op) in ops.iter().enumerate() {
            again.push(merged(ctx, starts[i], *op, &spks[i]).await?.entry);
        }
        if again == node {
            for (i, e) in node.iter().enumerate() {
                println!("input {i}: node entry = builder entry (amount {}, daa {})", e.amount, e.block_daa_score);
            }
            built = Some(cf);
            break;
        }
        println!("attempt {attempt}: a node entry changed between build and submission; rebuilding");
    }
    let cf = built.ok_or(format!("{TAG}: node entries kept changing; not submitted"))?;
    let x = c.expected();
    let cs = sent_of(&cf);
    ctx.submit(
        &format!(
            "claim() M4 real scripts, FINAL on the jackpot pool: K = {}, m = {}, h = {}, prize {:.4} KAS, payout {:.4} KAS, pool' {:.4} KAS (R)",
            c.k(),
            c.pool.m,
            x.h,
            x.prize as f64 / KAS as f64,
            x.values[1] as f64 / KAS as f64,
            x.values[0] as f64 / KAS as f64
        ),
        &cs,
        true,
    )
    .await?;
    Ok(())
}

/// M5 (spec M5-f): genesis -> `request_deposit()` -> `request_withdraw()` (partial, of the
/// deployer position) -> buy -> close -> `draw()` with the conversion and the settlement
/// record -> the deposit's `settle()` (deposit-convert: a new position) -> the withdrawal's
/// `settle()` (FINAL, the record drained: payout and the record's R refunded to the draw
/// caller), every transaction with the real scripts, built by the `lottery` builders and
/// checked in the local v2.1.0 engine before submission through the public mempool.
/// - D = 2 KAS (>= MIN_DEPOSIT); w = 1 % of the deployer's S0 = V0 / SHARE_UNIT shares
///   (0.6 KAS worth, within the draw's slack V' + D - L', so wc = W).
/// - The record's state is rebuilt from the draw transaction bytes alone
///   (`RoundResult::record`, R4); every input of every step is the node's own entry, read
///   once a chain block merged the creating transaction (`merged`, O-S4-4).
/// Locked on testnet afterwards: round 2's objects, the new pools, vault', the new position
/// and position' (no further entry is exercised). Payouts and changes are ours.
pub async fn vault_m5(ctx: &mut Ctx) -> Result<(), String> {
    use kaspa_consensus_core::tx::TransactionOutpoint as Op;
    use lottery::close::FundingIn;
    use lottery::game::{SHARE_UNIT, SettleMode};
    use lottery::state::{KIND_DEPOSIT, KIND_WITHDRAW, Payout, RequestState};
    use lottery::vault::SettleCase;

    const TAG: &str = "vault-m5";
    let s0 = 60 * KAS as i64 / SHARE_UNIT;
    let vr = VaultRequests { d: 2 * KAS as i64, w: s0 / 100 };
    let r = round1_drawn_with(ctx, TAG, Some(vr)).await?;
    let t = r.t.clone();
    let me = Payout::Schnorr(secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &ctx.sk).unwrap().x_only_public_key().0.serialize());
    let rec = r.rr.record.clone().ok_or("vault-m5: the draw wrote no record")?;
    let dx = r.d.expected();
    let did = r.ds.tx.id();
    assert_eq!(rec.outpoint, Op::new(did, dx.record_output().unwrap() as u32), "the record outpoint");
    assert_eq!(r.ds.tx.outputs[rec.outpoint.index as usize].script_public_key, t.record_spk(&rec.state), "the record state rebuilt from the draw bytes");
    let (_, ds, d_start) = r.dep.as_ref().unwrap();
    let (_, ws, w_start) = r.wdr.as_ref().unwrap();
    let (d_start, w_start) = (*d_start, *w_start);
    let caller = dx.caller_output();

    // ---- settle(1): the deposit request (deposit output 1) against the record; funding: the draw's caller output.
    let dreq = RequestState::new(1, KIND_DEPOSIT, vr.d, &me);
    assert_eq!(ds.tx.outputs[1].script_public_key, t.request_spk(&dreq), "deposit output 1 is the request");
    let mut s1 = SettleCase::new_with(&t, rec.state.clone(), dreq);
    s1.cov = r.cov;
    s1.funding_sk = ctx.sk;
    let ops1 = vec![Op::new(ds.tx.id(), 1), rec.outpoint, Op::new(did, caller as u32)];
    let starts1 = [d_start, r.d_start, r.d_start];
    let spks1 = [ds.tx.outputs[1].script_public_key.clone(), r.ds.tx.outputs[rec.outpoint.index as usize].script_public_key.clone(), r.ds.tx.outputs[caller].script_public_key.clone()];
    let mut node = Vec::new();
    for (i, op) in ops1.iter().enumerate() {
        node.push(merged(ctx, starts1[i], *op, &spks1[i]).await?.entry);
    }
    s1.request_value = node[0].amount;
    s1.request_daa = node[0].block_daa_score;
    s1.record_reserve = node[1].amount - (rec.state.out_rem as u64 + rec.state.refund as u64 * rec.state.dep_rem as u64);
    s1.record_daa = node[1].block_daa_score;
    s1.funding = vec![FundingIn { value: node[2].amount, daa: node[2].block_daa_score }];
    let x1 = s1.expected();
    if x1.reference.is_none() || x1.mode != SettleMode::DepositConvert || x1.drained {
        return Err(format!("{TAG}: expected a not-drained deposit-convert settlement, got {:?} drained {} (reference {:?})", x1.mode, x1.drained, x1.reference));
    }
    let fin1 = |c: &SettleCase| {
        let mut b = c.build_with(t.clone());
        b.outpoints = ops1.clone();
        b.finish()
    };
    s1.fee = lottery_fee(&fin1(&s1));
    let f1 = fin1(&s1);
    if f1.entries != node {
        return Err(format!("{TAG}: the deposit settlement's input entries do not match the node's"));
    }
    f1.run().map_err(|(i, e)| format!("local engine rejects {TAG} deposit settle at input {i}: {e:?}"))?;
    let x1 = s1.expected();
    let st1 = sent_of(&f1);
    let st1_start = sink(ctx).await?;
    if !ctx
        .submit(
            &format!("settle() M5 real request and record scripts: deposit-convert, s = {:.2} KAS -> position of {} shares; record' kept", vr.d as f64 / KAS as f64, x1.sh),
            &st1,
            true,
        )
        .await?
    {
        return Err(format!("{TAG} deposit settle rejected"));
    }

    // ---- settle(2): the withdrawal request (withdraw output 1) against record' (settle(1) output 0); funding: settle(1)'s change.
    let wreq = RequestState::new(1, KIND_WITHDRAW, vr.w, &me);
    assert_eq!(ws.tx.outputs[1].script_public_key, t.request_spk(&wreq), "withdraw output 1 is the request");
    let s1id = st1.tx.id();
    let ch1 = x1.change_output.unwrap();
    let mut s2 = SettleCase::new_with(&t, x1.record.clone(), wreq);
    s2.cov = r.cov;
    s2.funding_sk = ctx.sk;
    let ops2 = vec![Op::new(ws.tx.id(), 1), Op::new(s1id, 0), Op::new(s1id, ch1 as u32)];
    let starts2 = [w_start, st1_start, st1_start];
    let spks2 = [ws.tx.outputs[1].script_public_key.clone(), st1.tx.outputs[0].script_public_key.clone(), st1.tx.outputs[ch1].script_public_key.clone()];
    let mut node = Vec::new();
    for (i, op) in ops2.iter().enumerate() {
        node.push(merged(ctx, starts2[i], *op, &spks2[i]).await?.entry);
    }
    s2.request_value = node[0].amount;
    s2.request_daa = node[0].block_daa_score;
    s2.record_reserve = s1.record_reserve;
    s2.record_daa = node[1].block_daa_score;
    s2.funding = vec![FundingIn { value: node[2].amount, daa: node[2].block_daa_score }];
    let x2 = s2.expected();
    if x2.reference.is_none() || x2.mode != SettleMode::Final || !x2.drained {
        return Err(format!("{TAG}: expected a drained FINAL withdrawal settlement, got {:?} drained {} (reference {:?})", x2.mode, x2.drained, x2.reference));
    }
    let fin2 = |c: &SettleCase| {
        let mut b = c.build_with(t.clone());
        b.outpoints = ops2.clone();
        b.finish()
    };
    s2.fee = lottery_fee(&fin2(&s2));
    let f2 = fin2(&s2);
    if f2.entries != node {
        return Err(format!("{TAG}: the withdrawal settlement's input entries do not match the node's"));
    }
    f2.run().map_err(|(i, e)| format!("local engine rejects {TAG} withdrawal settle at input {i}: {e:?}"))?;
    let x2 = s2.expected();
    let st2 = sent_of(&f2);
    ctx.submit(
        &format!(
            "settle() M5 real request and record scripts: withdrawal FINAL, w = {} shares, payout {:.4} KAS (pay + R), record drained, R refunded to the funder",
            vr.w,
            x2.values[x2.payout_output.unwrap()] as f64 / KAS as f64
        ),
        &st2,
        true,
    )
    .await?;
    Ok(())
}
