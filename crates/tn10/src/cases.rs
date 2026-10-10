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

/// Our confirmed UTXO at (txid, index), with its creation DAA score from the UTXO index.
async fn ours(ctx: &Ctx, txid: kaspa_consensus_core::tx::TransactionId, index: usize) -> Result<Utxo, String> {
    ctx.funds()
        .await?
        .into_iter()
        .find(|u| u.op.transaction_id == txid && u.op.index as usize == index)
        .ok_or_else(|| format!("output {txid}:{index} not found in our UTXO set"))
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
///   caller output); lock_time = seed_ref. Every input's creation DAA score comes from our
///   UTXO index (outputs of one transaction share it).
/// Locked on testnet afterwards: round 2's objects, the drawer', vault', controller', the
/// new pools (no claim path in M3) and the ticket. The caller output is ours.
pub async fn draw_m3(ctx: &mut Ctx) -> Result<(), String> {
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
    g.set_jackpot(10 * KAS as i64);
    g.set_vault(60 * KAS as i64);
    g.controller.deadline = deadline;
    for s in &mut g.shards {
        s.deadline = deadline;
    }
    let mut gf = g.build_with(t.clone()).finish();
    g.fee = lottery_fee(&gf);
    gf = g.build_with(t.clone()).finish();
    gf.run().map_err(|(i, e)| format!("local engine rejects draw-m3 genesis at input {i}: {e:?}"))?;
    let cov = covenant_id(&gf);
    let gs = sent_of(&gf);
    if !ctx.submit(&format!("setup: draw-m3 genesis, K = {K}, real templates, R = {:.2} KAS", g.reserve() as f64 / KAS as f64), &gs, true).await? {
        return Err("draw-m3 genesis rejected".into());
    }
    let gid = gs.tx.id();
    let gchange = ours(ctx, gid, gs.tx.outputs.len() - 1).await?;
    let gdaa = gchange.entry.block_daa_score;
    println!("genesis {gid} at DAA {gdaa}; covenant id {cov}");

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
    bc.funding = vec![gchange.entry.amount];
    bc.funding_sk = ctx.sk;
    let bops = vec![Op::new(gid, 2), gchange.op];
    let buy_fin = |bc: &BuyCase| {
        let mut b = bc.build_with(t.clone());
        b.outpoints = bops.clone();
        b.finish()
    };
    let bf = buy_fin(&bc);
    bc.fee = lottery_fee(&bf);
    let bf = buy_fin(&bc);
    bf.run().map_err(|(i, e)| format!("local engine rejects draw-m3 buy at input {i}: {e:?}"))?;
    let bs = sent_of(&bf);
    if !ctx.submit(&format!("setup: draw-m3 buy() of the jackpot combination {:?} + {} on shard 0", unrank(sn.rank), sn.bonus), &bs, true).await? {
        return Err("draw-m3 buy rejected".into());
    }
    let bid = bs.tx.id();
    let bchange = ours(ctx, bid, bs.tx.outputs.len() - 1).await?;
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
    cf.run().map_err(|(i, e)| format!("local engine rejects draw-m3 close at input {i}: {e:?}"))?;
    let x = c.expected();
    let cs = sent_of(&cf);
    if !ctx.submit(&format!("setup: draw-m3 close(), K = {K}, one sale of 1 KAS"), &cs, true).await? {
        return Err("draw-m3 close rejected".into());
    }
    let cid = cs.tx.id();
    let caller = ours(ctx, cid, cs.tx.outputs.len() - 1).await?;
    let cdaa = caller.entry.block_daa_score;
    let seed_ref = x.marker.seed_ref;
    println!("close {cid} at DAA {cdaa}; seed_ref {seed_ref}");

    // ---- draw_test_seed()
    let mut d = DrawCase::new_with(K, &t);
    d.drawer = g.drawer.clone();
    d.drawer_daa = gdaa;
    d.marker = x.marker.clone();
    d.marker_daa = cdaa;
    d.vault = g.vault.clone();
    d.vault_daa = gdaa;
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
    let dops = vec![Op::new(gid, K as u32 + 3), Op::new(cid, K as u32 + 2), Op::new(gid, K as u32 + 2), Op::new(cid, 0), caller.op];
    let draw_fin = |d: &DrawCase| {
        let mut b = d.build_with(t.clone());
        b.outpoints = dops.clone();
        b.finish()
    };
    let df = draw_fin(&d);
    d.fee = lottery_fee(&df);
    let df = draw_fin(&d);
    df.run().map_err(|(i, e)| format!("local engine rejects draw-m3 draw at input {i}: {e:?}"))?;
    let dx = d.expected();
    // Wait for the lock time (D51): the virtual DAA score must pass seed_ref.
    wait_daa(ctx, seed_ref as u64 + 20, "seed_ref").await?;
    // Check the builder's input entries against the node's UTXO set (amount, script, creation
    // DAA score, covenant ID) and re-run the local engine on the node's entries.
    let mut node_entries = df.entries.clone();
    for (i, op) in dops.iter().enumerate() {
        let spk = &df.entries[i].script_public_key;
        let addr = kaspa_txscript::extract_script_pub_key_address(spk, kaspa_addresses::Prefix::Testnet).map_err(|e| e.to_string())?;
        let found = crate::net::utxos(&ctx.client, &addr).await?.into_iter().find(|e| Op::from(e.outpoint.clone()) == *op);
        match found {
            Some(e) => {
                let ne: kaspa_consensus_core::tx::UtxoEntry = e.utxo_entry.into();
                if ne != df.entries[i] {
                    println!("input {i} ENTRY MISMATCH: node amount {} daa {} cov {:?}; builder amount {} daa {} cov {:?}", ne.amount, ne.block_daa_score, ne.covenant_id, df.entries[i].amount, df.entries[i].block_daa_score, df.entries[i].covenant_id);
                } else {
                    println!("input {i}: node entry = builder entry (amount {}, daa {})", ne.amount, ne.block_daa_score);
                }
                node_entries[i] = ne;
            }
            None => println!("input {i}: {op} not found at {addr}"),
        }
    }
    for i in 0..df.tx.inputs.len() {
        println!("input {i} on the node's entries: {:?}", harness::execute(&df.tx, &node_entries, i, true));
    }
    let ds = sent_of(&df);
    let ok = ctx
        .submit(
            &format!(
                "draw_test_seed() M3 real scripts, K = {K}, a = 0, np = {}, h_J = {}, n2 = {}, fee paid {}, R = {:.2} KAS",
                dx.np(),
                dx.h_j,
                dx.n2,
                dx.fee.paid,
                d.reserve() as f64 / KAS as f64
            ),
            &ds,
            true,
        )
        .await?;
    if ok {
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
    }
    Ok(())
}
