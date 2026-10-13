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
/// M6 (spec A6): run for b = B_MAX and for b = 1 (each with a referral at s_R = 10 %, so the
/// b = 1 referral is 0.1 KAS = M_REF and the shape is D80's worst `buy()`, 84.5 % of storage
/// at d = 0.13 with a large change), each on its own genesis shard.
pub async fn buy_m1(ctx: &mut Ctx) -> Result<(), String> {
    for b in [lottery::buy::b_max(), 1] {
        buy_m1_b(ctx, b).await?;
    }
    Ok(())
}

async fn buy_m1_b(ctx: &mut Ctx, b: usize) -> Result<(), String> {
    use lottery::buy::{BuyCase, Templates, b_max};
    use lottery::combo::{Combo, unrank};
    use lottery::state::Payout;

    use kaspa_rpc_core::api::rpc::RpcApi;
    let daa = ctx.client.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    let bm = b_max();
    let mut c = BuyCase::new(bm);
    c.shard.deadline = (daa + 10_000_000) as i64;
    c.shard_daa = daa;
    c.bundle = (0..b as u32).map(|i| Combo::new(unrank(7 + i * 13_001), 1 + i as u8)).collect();
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
    if !ctx.submit(&format!("setup: buy-m1 genesis shard (b = {b})"), &s, true).await? {
        return Err("buy-m1 setup rejected".into());
    }
    let so = outs_of(&s);
    let funding = so[1].clone();
    c.cov = so[0].entry.covenant_id.ok_or("no genesis covenant id")?;
    c.funding = vec![funding.entry.amount];

    let mut bl = c.build();
    bl.outpoints = vec![so[0].op, funding.op];
    let fin = bl.finish();
    fin.run().map_err(|(i, e)| format!("local engine rejects buy-m1 (b = {b}) at input {i}: {e:?}"))?;
    let mut tx = fin.tx.clone();
    tx.finalize();
    let m = harness::masses(&tx, &fin.entries);
    let sent = Sent { fee: fin.fee(), tx, m };
    let ok = ctx
        .submit(
            &format!(
                "buy() M1 real shard script, b = {b} (B_MAX = {bm}), referral {:.2} KAS, d = {} KAS, R = {:.2} KAS",
                c.referral() as f64 / KAS as f64,
                c.shard.d as f64 / KAS as f64,
                c.reserve as f64 / KAS as f64
            ),
            &sent,
            true,
        )
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
async fn merged(ctx: &Ctx, start: kaspa_rpc_core::RpcHash, op: kaspa_consensus_core::tx::TransactionOutpoint, spk: &kaspa_consensus_core::tx::ScriptPublicKey) -> Result<(Utxo, f64), String> {
    merged_poll(ctx, start, op, spk, std::time::Duration::from_secs(1)).await
}

/// `merged` with a chosen poll interval (M6 `conflict-m6` polls every 200 ms for τ_merge).
/// Returns the entry and the seconds from the call until it was found.
async fn merged_poll(
    ctx: &Ctx,
    start: kaspa_rpc_core::RpcHash,
    op: kaspa_consensus_core::tx::TransactionOutpoint,
    spk: &kaspa_consensus_core::tx::ScriptPublicKey,
    poll: std::time::Duration,
) -> Result<(Utxo, f64), String> {
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
                            let secs = t0.elapsed().as_secs_f64();
                            println!("{op}: merged by chain block {} at DAA {daa}; node entry DAA {} (after {secs:.1} s)", a.accepting_block_hash, ne.block_daa_score);
                            return Ok((Utxo { op, entry: ne, padded: false }, secs));
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
        tokio::time::sleep(poll).await;
    }
    Err(format!("{op}: no chain-merged UTXO entry within 300 s ({last})"))
}

/// M3 (spec M3-g), M7 (spec D103): genesis -> buy -> close -> `draw()` at K = 16 with the real
/// scripts, each built by the `lottery` builder and checked in the local v2.1.0 engine before
/// submission.
/// - Genesis as close-m2 (J(1) = 10 KAS, V0 = 60 KAS) but with T = virtual DAA + 600: buy()
///   needs the shard created before T (shard.sil check 6), close() needs lock_time = T final,
///   so the probe waits for T (about a minute at 10 BPS) between the buy and the close.
/// - buy(): shard 0 sells one fixed combination (M7: the outcome is unknown until the anchor
///   block exists, so the draw is normally a "jackpot not held" draw, DP4 shape; the lower-tier
///   pools are still created from the 1 KAS sale).
/// - close(): K = 16, funded by the buy change. D = the marker's creation DAA score (the
///   chain block that accepted close()) + seed_offset (600 DAA, spec M7 D93).
/// - draw(): drawer, marker, vault, controller and one funding input (the close caller
///   output); lock_time = D. The anchor X is the first chain block with DAA >= D, found on the
///   virtual chain (`anchor::find_anchor`); X's and P's proof fields are rebuilt from RPC and
///   self-checked against the headers (`anchor::anchor_from_rpc`, D103); the local engine runs
///   the draw against a `ChainSim` of the real P and X (`ChainSim::from_real_chain`). Every
///   input's entry (and creation DAA score) is the node's own, read only after a chain block
///   merged the creating transaction (`merged`, O-S4-4), re-read just before submission, and
///   the draw is rebuilt if it moved.
/// - Void (`void-m7`): the same with a TEST-ONLY genesis (`allow_short_void_window`,
///   draw_window = 1,200 DAA) and `draw()` with an empty anchor at lock_time = D + 1,200.
/// Locked on testnet afterwards: round 2's objects, the drawer', vault', controller', the
/// new pools and the ticket. The caller output is ours.
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
    /// M7: the real anchor (X, P) the draw used; `None` for a void.
    pub anchor: Option<std::sync::Arc<crate::anchor::RpcAnchor>>,
}

/// M5 vault requests sent between genesis and the buy (`round1_drawn_with`): a deposit of
/// `d` sompi and a partial withdrawal of `w` shares of the deployer position.
#[derive(Clone, Copy)]
pub struct VaultRequests {
    pub d: i64,
    pub w: i64,
}

/// M7 (spec D103): `probe draw-m7` (also run as `probe draw-m3`: the TEST-ONLY seed path of
/// M3 is deleted, D101). A rejected draw is a report row and an error: the stop rule D104
/// applies to any rejection other than fees or the resolver.
pub async fn draw_m7(ctx: &mut Ctx, tag: &str) -> Result<(), String> {
    round1_drawn(ctx, tag).await.map(|_| ())
}

/// M7 (spec D103): `probe void-m7`. Genesis with the TEST-ONLY `allow_short_void_window`
/// (draw_window = 1,200 DAA; refused by `GenesisCase::validate` unless the network is testnet
/// or devnet), buy, close, then `draw()` in void mode (empty anchor) once the virtual DAA
/// score passes D + 1,200.
pub async fn void_m7(ctx: &mut Ctx) -> Result<(), String> {
    round1_drawn_with(ctx, "void-m7", None, true).await.map(|_| ())
}

/// Genesis -> buy -> close -> draw() of round 1 with the real scripts and the real anchor
/// (the draw-m7 sequence); `tag` names the probe in every report row and error.
pub async fn round1_drawn(ctx: &mut Ctx, tag: &str) -> Result<Drawn, String> {
    round1_drawn_with(ctx, tag, None, false).await
}

/// TEST-ONLY draw window of `void-m7` (DAA): the void lock D + 1,200 is about 2 minutes after D.
pub const VOID_M7_DRAW_WINDOW: i64 = 1_200;

/// `round1_drawn` with, if `vr` is set, a `request_deposit()` and a `request_withdraw()`
/// (both with the real vault and position scripts) between genesis and the buy, so the draw
/// converts both and writes a settlement record (spec M5-f); with `void`, a TEST-ONLY short
/// draw window at genesis and a void instead of a draw (spec M7 D96, D103).
pub async fn round1_drawn_with(ctx: &mut Ctx, tag: &str, vr: Option<VaultRequests>, void: bool) -> Result<Drawn, String> {
    use kaspa_consensus_core::tx::TransactionOutpoint as Op;
    use kaspa_rpc_core::api::rpc::RpcApi;
    use lottery::buy::{BuyCase, Templates, b_max};
    use lottery::close::{CloseCase, FundingIn, ShardIn};
    use lottery::combo::{Combo, unrank};
    use lottery::draw::{DrawCase, RoundResult};
    use lottery::genesis::{GenesisCase, covenant_id};
    use lottery::smt::Tree;
    use lottery::state::Payout;

    const K: usize = 16;
    let daa = ctx.client.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    // M7 Stage 5 fix A: the production drawer requires draw_window >= VOID_MIN = 2F (check 1);
    // the void probe runs under the TEST-ONLY short-void drawer variant, which only the
    // genesis flag `allow_short_void_window` selects (`GenesisCase::templates()`, D103).
    let t = if void { Templates::test_only_short_void(b_max()) } else { Templates::get(b_max()) };
    let me = Payout::Schnorr(secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &ctx.sk).unwrap().x_only_public_key().0.serialize());
    // vault-m5 waits VAULT_REQ_DELAY before each of its two requests (U-4), so T is set that
    // much later; draw-m3 and claim-m4 are unchanged.
    let deadline = daa as i64 + 600 + if vr.is_some() { 3 * lottery::game::VAULT_REQ_DELAY } else { 0 };

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
    if void {
        // TEST-ONLY (spec M7 D103): a void after 2 minutes instead of 24 h. validate() refuses
        // the flag unless the network is testnet or devnet (GenesisCase.network = Testnet).
        g.set_test_only_short_void_window(VOID_M7_DRAW_WINDOW);
        if !std::sync::Arc::ptr_eq(&g.templates(), &t) {
            return Err(format!("{tag}: the TEST-ONLY flag did not select the short-void templates"));
        }
        g.validate_with(&t).map_err(|e| format!("{tag}: genesis refused: {e}"))?;
    }
    let mut gf = g.build_with(t.clone()).finish();
    g.fee = lottery_fee(&gf);
    gf = g.build_with(t.clone()).finish();
    gf.run().map_err(|(i, e)| format!("local engine rejects {tag} genesis at input {i}: {e:?}"))?;
    let cov = covenant_id(&gf);
    let gs = sent_of(&gf);
    let g_start = sink(ctx).await?;
    if !ctx
        .submit(
            &format!(
                "setup: {tag} genesis, K = {K}, real templates, R = {:.2} KAS, seed_offset {}, draw_window {}{}",
                g.reserve() as f64 / KAS as f64,
                g.controller.seed_offset,
                g.drawer.draw_window,
                if void { " (TEST-ONLY allow_short_void_window, short-void drawer variant)" } else { "" }
            ),
            &gs,
            true,
        )
        .await?
    {
        return Err(format!("{tag} genesis rejected"));
    }
    let gid = gs.tx.id();
    let gn = gs.tx.outputs.len() - 1;
    let gchange = merged(ctx, g_start, Op::new(gid, gn as u32), &gs.tx.outputs[gn].script_public_key).await?.0;
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
        // M5 Stage 7 (U-4): lock_time = the vault's creation + VAULT_REQ_DELAY (check D10);
        // the transaction is final once the virtual DAA score passes it.
        dc.at_age(gdaa);
        wait_daa(ctx, dc.lock_time + 20, "vault age (request_deposit)").await?;
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
        // Negative case (M5 re-review should-fix #2): the same request one DAA too early,
        // lock_time = vault creation + VAULT_REQ_DELAY - 1. The lock time is final (the
        // virtual DAA is past it), so only the script's age check D10 can refuse it. The local
        // engine must reject it at input 0, and so must the node; if the node accepted it,
        // it would spend the vault and the probe stops (a finding).
        {
            let mut early = dc.clone();
            early.lock_time = dc.lock_time - 1;
            let ef = dep_fin(&early);
            match ef.run() {
                Err((0, e)) => println!("local engine rejects the early request_deposit at input 0, as expected: {e:?}"),
                other => return Err(format!("{tag}: local engine did not reject the early request_deposit at input 0: {other:?}")),
            }
            let es = sent_of(&ef);
            let case = format!(
                "negative: request_deposit() at vault age VAULT_REQ_DELAY - 1 (lock_time = creation {} + 599 = {}); expected REJECTED by check D10",
                gdaa, early.lock_time
            );
            if ctx.submit(&case, &es, false).await? {
                return Err(format!("{tag}: FINDING: the node ACCEPTED request_deposit at lock_time = vault creation + VAULT_REQ_DELAY - 1"));
            }
        }
        let df = dep_fin(&dc);
        df.run().map_err(|(i, e)| format!("local engine rejects {tag} request_deposit at input {i}: {e:?}"))?;
        let ds = sent_of(&df);
        let d_start = sink(ctx).await?;
        if !ctx.submit(&format!("request_deposit() M5 real vault script: D = {:.2} KAS, request value R", vr.d as f64 / KAS as f64), &ds, true).await? {
            return Err(format!("{tag} request_deposit rejected"));
        }
        let did = ds.tx.id();
        let dn = ds.tx.outputs.len() - 1;
        let dchange = merged(ctx, d_start, Op::new(did, dn as u32), &ds.tx.outputs[dn].script_public_key).await?.0;
        let dx = dc.expected();
        vault_in = (Op::new(did, 0), dx.vault.clone(), dchange.entry.block_daa_score, d_start);

        // request_withdraw (partial) of the deployer position: the vault', the genesis
        // position and the deposit change as the owner input (our Schnorr P2PK = the owner).
        let mut wc = WithdrawCase::new_with(&t, OwnerKey::Schnorr(ctx.sk));
        wc.vault = dx.vault.clone();
        wc.reserve = g.reserve();
        // U-4: vault' (created by the deposit request) must be VAULT_REQ_DELAY old (W12).
        wc.at_age(vault_in.2);
        wait_daa(ctx, wc.lock_time + 20, "vault age (request_withdraw)").await?;
        wc.position = g.position.clone();
        wc.position_daa = gdaa;
        wc.w = vr.w;
        // M6 D86 builder policy (the W4b mirror): refuse a partial withdrawal below wmin.
        let mut wc = wc.checked().map_err(|e| format!("{tag}: builder policy refuses request_withdraw: {e}"))?;
        println!("request_withdraw: w = {} shares, wmin = {} at total_assets = {}, total_shares = {}", wc.w, wc.w_min(), wc.vault.total_assets, wc.vault.total_shares);
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
        if !ctx.submit(&format!("request_withdraw() M5 real vault and position scripts: partial, w = {} of {} shares (wmin {}), P2PK Schnorr owner input", vr.w, g.position.shares, wc.w_min()), &ws, true).await? {
            return Err(format!("{tag} request_withdraw rejected"));
        }
        let wid = ws.tx.id();
        let wn = ws.tx.outputs.len() - 1;
        let wchange = merged(ctx, w_start, Op::new(wid, wn as u32), &ws.tx.outputs[wn].script_public_key).await?.0;
        vault_in = (Op::new(wid, 0), wc.expected().vault.clone(), wchange.entry.block_daa_score, w_start);
        buy_funding = wchange;
        dep = Some((dc, ds, d_start));
        wdr = Some((wc, ws, w_start));
    }

    // ---- buy(): one fixed combination on shard 0 (M7: the outcome is unknown until the
    // anchor block exists, after close()).
    let m = g.controller.m;
    let combo = Combo::new(unrank(0), 1);
    let mut bc = BuyCase::new(t.b_max).with_tree(Tree::new(m as usize));
    assert_eq!(bc.shard.root, g.shards[0].root, "genesis shard 0 holds the empty tree");
    bc.shard = g.shards[0].clone();
    bc.reserve = g.reserve();
    bc.shard_daa = gdaa;
    bc.cov = cov;
    bc.bundle = vec![combo];
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
    if !ctx.submit(&format!("setup: {tag} buy() of the fixed combination {:?} + 1 on shard 0", unrank(0)), &bs, true).await? {
        return Err(format!("{tag} buy rejected"));
    }
    let bid = bs.tx.id();
    let bn = bs.tx.outputs.len() - 1;
    let bchange = merged(ctx, b_start, Op::new(bid, bn as u32), &bs.tx.outputs[bn].script_public_key).await?.0;
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
    let caller = merged(ctx, c_start, Op::new(cid, cn as u32), &cs.tx.outputs[cn].script_public_key).await?.0;
    let cdaa = caller.entry.block_daa_score;
    // Spec M7 D93: D = the marker's creation DAA score (the chain block that accepted close())
    // + seed_offset. The marker (close output K + 2) and the caller output share that score.
    let d_ref = lottery::game::anchor_daa(cdaa as i64, x.controller.seed_offset);
    println!("close {cid} at DAA {cdaa}; D {d_ref}{}", if void { format!("; void lock D + draw_window = {}", d_ref + g.drawer.draw_window) } else { String::new() });

    // ---- draw() (or the void)
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
    d.cov = cov;
    d.funder = me;
    d.funding_sk = ctx.sk;
    d.funding = vec![FundingIn { value: caller.entry.amount, daa: cdaa }];
    d.absorbed = vec![];
    d.set_void(void);
    assert_eq!(d.d(), d_ref, "DrawCase D = marker creation DAA + seed_offset");
    let dops = vec![Op::new(gid, K as u32 + 3), Op::new(cid, K as u32 + 2), vault_in.0, Op::new(cid, 0), caller.op];
    let draw_fin = |d: &DrawCase| {
        let mut b = d.build_with(t.clone());
        b.outpoints = dops.clone();
        b.finish()
    };
    // Lock time (D51, spec M7 check 7): a draw needs tx.daa >= D, a void D + draw_window.
    wait_daa(ctx, d.lock_time + 20, if void { "void lock D + draw_window" } else { "D" }).await?;
    let anchor = if void {
        None
    } else {
        // The anchor X (first chain block with DAA >= D) and P, from RPC, self-checked
        // (D103). c_start is the sink before close() was submitted, so its DAA is < D.
        let a = crate::anchor::rpc_anchor(ctx, c_start, d_ref as u64).await?;
        // A few confirmations of X before the draw is built (X must stay a chain block).
        crate::anchor::wait_blue_score(ctx, a.x.sim.blue_score + 50).await?;
        let sim = harness::ChainSim::from_real_chain(harness::Hash::from_bytes(a.p.fields.parent_commit), &[a.p.sim, a.x.sim]);
        d.sim = Some(std::sync::Arc::new(sim));
        d.x_hash = a.proof.x_hash();
        if d.anchor_bytes() != a.proof.to_bytes() || d.x_hash() != a.proof.x_hash() {
            return Err(format!("{tag}: the builder's anchor bytes differ from the RPC proof"));
        }
        let row = format!(
            "| anchor for D = {}: X `{}` DAA {} blue score {} (mergeset {} blues + {} reds, coinbase lane active {}); P `{}` DAA {} blue score {} (mergeset {} + {}); mergeset order matched: X [{}], P [{}] = {}; SeqCommit(X) `{}` and SeqCommit(P) `{}` recomputed = headers | | | | | | | | |",
            a.d,
            a.x.sim.hash,
            a.x.sim.daa_score,
            a.x.sim.blue_score,
            a.x.mergeset_blues,
            a.x.mergeset_reds,
            a.x.lane_active,
            a.p.sim.hash,
            a.p.sim.daa_score,
            a.p.sim.blue_score,
            a.p.mergeset_blues,
            a.p.mergeset_reds,
            a.x.order,
            a.p.order,
            crate::anchor::ORDERS[a.x.order],
            a.x.header_commit,
            a.p.header_commit
        );
        println!("{row}");
        ctx.report.push(row);
        Some(std::sync::Arc::new(a))
    };
    // O-S4-4 (Stage 5): build the draw only from the node's own UTXO entries of inputs whose
    // creating transaction a chain block has merged (`merged`), then re-read them just
    // before submission; if any entry changed (e.g. a reorganisation moved the merging
    // chain block), rebuild from the new entries instead of submitting. A moved marker entry
    // moves D and the anchor: that is an error, not a rebuild.
    let starts = [g_start, c_start, vault_in.3, c_start, c_start];
    let spks: Vec<_> = draw_fin(&d).entries.iter().map(|e| e.script_public_key.clone()).collect();
    let mut built = None;
    for attempt in 1..=3 {
        let mut node = Vec::new();
        for (i, op) in dops.iter().enumerate() {
            node.push(merged(ctx, starts[i], *op, &spks[i]).await?.0.entry);
        }
        d.drawer_daa = node[0].block_daa_score;
        d.marker_daa = node[1].block_daa_score;
        d.vault_daa = node[2].block_daa_score;
        d.controller_daa = node[3].block_daa_score;
        d.funding = vec![FundingIn { value: node[4].amount, daa: node[4].block_daa_score }];
        if d.d() != d_ref {
            return Err(format!("{tag}: the marker's node entry DAA moved ({} -> {}); D and the anchor changed", cdaa, d.marker_daa));
        }
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
            again.push(merged(ctx, starts[i], *op, &spks[i]).await?.0.entry);
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
    // X must still be a chain block when the draw is submitted (else BlockNotSelected).
    if let Some(a) = &anchor {
        let xb = ctx.client.get_block(a.x.sim.hash, false).await.map_err(|e| e.to_string())?;
        if !xb.verbose_data.map(|v| v.is_chain_block).unwrap_or(false) {
            return Err(format!("{tag}: X {} left the selected chain before submission", a.x.sim.hash));
        }
    }
    let dx = d.expected();
    let ds = sent_of(&df);
    let d_start = sink(ctx).await?;
    let ok = ctx
        .submit(
            &format!(
                "{}{} {} real scripts, K = {K}, a = 0, np = {}, h_J = {}, n2 = {}, fee paid {}, R = {:.2} KAS, lock_time {}{}{}",
                if tag == "claim-m4" { "setup: " } else { "" },
                if void { "void: draw() with an empty anchor (D96)" } else { "draw() with the real anchor (D93-D95)" },
                if vr.is_some() { "M5+M7" } else { "M7" },
                dx.np(),
                dx.h_j,
                dx.n2,
                dx.fee.paid,
                d.reserve() as f64 / KAS as f64,
                d.lock_time,
                match &anchor {
                    Some(a) => format!(", X DAA {} (D = {d_ref})", a.x.sim.daa_score),
                    None => format!(", D = {d_ref}, D + draw_window = {}", d_ref + d.drawer.draw_window),
                },
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
        return Err(format!("{tag} draw rejected (stop rule D104: the node error is in the row above)"));
    }
    let rr = RoundResult::from_tx(&ds.tx, &t).map_err(|e| format!("round result: {e}"))?;
    let pools: Vec<(u8, i64, u64)> = rr.pools.iter().map(|p| (p.tier, p.slice, p.value)).collect();
    let xh = harness::Hash::from_bytes(rr.x_hash);
    println!("round result: round {}, void {}, X.hash {xh}, win {:?}, D_claim {}, pools (tier, slice, value) {pools:?}", rr.round, rr.void, rr.win, rr.d_claim);
    ctx.report.push(format!(
        "| round result from the draw tx bytes: round {}, void {}, X.hash `{xh}`, mk_daa {}, win {:?}, D_claim {}, pools (tier, slice, value) {pools:?} | | | | | | | | `{}` |",
        rr.round,
        rr.void,
        rr.mk_daa,
        rr.win,
        rr.d_claim,
        ds.tx.id()
    ));
    Ok(Drawn { t, cov, bc, bs, b_start, d, ds, d_start, rr, dep, wdr, anchor })
}

/// M4 (spec M4-i), M7 Stage 4: genesis -> buy -> close -> draw (the draw-m7 sequence, K = 16,
/// real scripts, the real anchor) -> one `claim()` with the real ticket and pool scripts
/// through the public mempool, **if the ticket wins**.
/// - M7 change (D101 deleted the TEST-ONLY seed): the outcome is fixed by the anchor block,
///   which exists only after close(), so no winning ticket can be prepared. The b = 1 ticket
///   (buy output 1) holds one fixed combination; the claim runs only if that combination falls
///   into a tier whose pool the draw created (lower tiers: 4 matches, or 3 matches with the
///   bonus; about 0.3 % at m = 10), else the probe records that the claim was not exercised
///   and stops without error. The M4 testnet run (`results-claim-m4.md`, 2026-10-11) stays the
///   claim evidence: `ticket.sil` and `pool.sil` changed only in comments in M7.
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
    use lottery::game::{ClaimMode, tier_of};

    const TAG: &str = "claim-m4";
    let r = round1_drawn(ctx, TAG).await?;
    let t = r.t.clone();
    let did = r.ds.tx.id();

    // ---- does the ticket win? (its only combination against the drawn numbers)
    let combo = r.bc.bundle[0];
    let tier = tier_of(&combo.bytes(), &r.rr.win);
    let p = match tier.and_then(|tc| r.rr.pools.iter().position(|p| p.tier == tc)) {
        Some(p) => p,
        None => {
            let note = format!(
                "{TAG}: the ticket's combination {:?} does not win against {:?} drawn from X {} (tier {tier:?}; pools {:?}); claim() not exercised (M7: no winning ticket can be prepared before the anchor exists)",
                combo.bytes(),
                r.rr.win,
                r.anchor.as_ref().map(|a| a.x.sim.hash.to_string()).unwrap_or_default(),
                r.rr.pools.iter().map(|p| p.tier).collect::<Vec<_>>()
            );
            println!("{note}");
            ctx.report.push(format!("| {note} | | | | | | | | |"));
            return Ok(());
        }
    };
    println!("{TAG}: the ticket wins tier {tier:?}; claiming from pool {p}");

    // ---- the claim's inputs: ticket, the winning tier's pool, funding (the draw's caller output)
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
            node.push(merged(ctx, starts[i], *op, &spks[i]).await?.0.entry);
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
            return Err(format!("{TAG}: expected a FINAL claim (b = 1, the ticket is consumed), got {:?}", x.mode));
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
            again.push(merged(ctx, starts[i], *op, &spks[i]).await?.0.entry);
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
            "claim() M4 real scripts after the M7 draw, FINAL on the tier {} pool: K = {}, m = {}, h = {}, prize {:.4} KAS, payout {:.4} KAS, pool' {:.4} KAS",
            c.pool.tier,
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
/// - M6 (D85, D86): D = 12 KAS (>= MIN_DEPOSIT = 10 KAS); w = wmin of the vault' the
///   withdrawal spends (`game::w_min(total_assets, total_shares)`, the W4b boundary; at
///   V0 = 60 KAS and S0 = V0 / SHARE_UNIT that is 1,000,000,166 shares, about 10 KAS worth,
///   within the draw's slack V' + D - L', so wc = W). The withdrawal passes the builder policy
///   `WithdrawCase::checked()` before it is built.
/// - The record's state is rebuilt from the draw transaction bytes alone
///   (`RoundResult::record`, R4); every input of every step is the node's own entry, read
///   once a chain block merged the creating transaction (`merged`, O-S4-4).
/// - Negative case (U-4 age window): before the valid `request_deposit()`, the same
///   transaction with lock_time = vault creation + VAULT_REQ_DELAY - 1 is submitted and must
///   be rejected by the node (vault check D10); acceptance is a finding and stops the probe.
/// Locked on testnet afterwards: round 2's objects, the new pools, vault', the new position
/// and position' (no further entry is exercised). Payouts and changes are ours.
pub async fn vault_m5(ctx: &mut Ctx) -> Result<(), String> {
    use kaspa_consensus_core::tx::TransactionOutpoint as Op;
    use lottery::close::FundingIn;
    use lottery::game::{MIN_DEPOSIT, SHARE_UNIT, SettleMode, w_min};
    use lottery::state::{KIND_DEPOSIT, KIND_WITHDRAW, Payout, RequestState};
    use lottery::vault::SettleCase;

    const TAG: &str = "vault-m5";
    let v0 = 60 * KAS as i64;
    let s0 = v0 / SHARE_UNIT;
    // The deposit request only raises pend_dep, so the vault' the withdrawal spends still has
    // total_assets = V0 and total_shares = S0; round1_drawn_with re-checks w against that
    // state through `WithdrawCase::checked()`.
    let vr = VaultRequests { d: 12 * KAS as i64, w: w_min(v0, s0) };
    if vr.d < MIN_DEPOSIT {
        return Err(format!("{TAG}: D = {} < MIN_DEPOSIT = {MIN_DEPOSIT}", vr.d));
    }
    println!("{TAG}: D = {} sompi, w = wmin({v0}, {s0}) = {} shares", vr.d, vr.w);
    let r = round1_drawn_with(ctx, TAG, Some(vr), false).await?;
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
        node.push(merged(ctx, starts1[i], *op, &spks1[i]).await?.0.entry);
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
        node.push(merged(ctx, starts2[i], *op, &spks2[i]).await?.0.entry);
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


/// Mean, median and max of `v` (seconds); `None` if empty.
fn stats(v: &[f64]) -> Option<(f64, f64, f64)> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = s.len();
    let median = if n % 2 == 1 { s[n / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 };
    Some((s.iter().sum::<f64>() / n as f64, median, s[n - 1]))
}

/// Poll the virtual chain from `start` every `poll` until every id of `all` and (if `any` is not
/// empty) at least one id of `any` has been accepted by a chain block, or `limit` passes.
/// Returns the instant at which each watched id was first seen accepted (taken when the RPC
/// response arrived, so an upper bound by one poll cycle plus the response time).
async fn watch_accepted(
    ctx: &Ctx,
    start: kaspa_rpc_core::RpcHash,
    all: &[kaspa_consensus_core::tx::TransactionId],
    any: &[kaspa_consensus_core::tx::TransactionId],
    poll: std::time::Duration,
    limit: std::time::Duration,
) -> Result<std::collections::HashMap<kaspa_consensus_core::tx::TransactionId, std::time::Instant>, String> {
    use kaspa_rpc_core::api::rpc::RpcApi;
    let t0 = std::time::Instant::now();
    let mut seen = std::collections::HashMap::new();
    loop {
        let vc = ctx.client.get_virtual_chain_from_block(start, true, None).await.map_err(|e| e.to_string())?;
        let now = std::time::Instant::now();
        for a in &vc.accepted_transaction_ids {
            for id in &a.accepted_transaction_ids {
                if (all.contains(id) || any.contains(id)) && !seen.contains_key(id) {
                    seen.insert(*id, now);
                }
            }
        }
        let all_done = all.iter().all(|id| seen.contains_key(id));
        let any_done = any.is_empty() || any.iter().any(|id| seen.contains_key(id));
        if (all_done && any_done) || t0.elapsed() > limit {
            return Ok(seen);
        }
        tokio::time::sleep(poll).await;
    }
}

/// Submit the alternative transactions `cands` in order, each round, every `poll` until one is
/// accepted or `limit` passes (orphans disallowed). Returns (accepted index, attempts, the
/// instant the accepting call returned) and every distinct error as "<name>: <text>".
async fn submit_until(
    ctx: &Ctx,
    cands: &[(&str, &Sent)],
    poll: std::time::Duration,
    limit: std::time::Duration,
) -> (Option<(usize, u32, std::time::Instant)>, Vec<String>) {
    let t0 = std::time::Instant::now();
    let mut errs: Vec<String> = vec![];
    let mut attempts = 0;
    loop {
        for (k, (name, s)) in cands.iter().enumerate() {
            attempts += 1;
            match ctx.submit_raw(s).await {
                Ok(()) => return (Some((k, attempts, std::time::Instant::now())), errs),
                Err(e) => {
                    let e = format!("{name}: {e}");
                    if !errs.contains(&e) {
                        println!("rejected (attempt {attempts}) {e}");
                        errs.push(e);
                    }
                }
            }
        }
        if t0.elapsed() > limit {
            return (None, errs);
        }
        tokio::time::sleep(poll).await;
    }
}

/// M6 (spec A6; D87, D88; research F3): purchase conflicts on one shard, with the real
/// templates, through the public mempool. Not part of `probe all` (it locks about 160 KAS: genesis, then 30 accepted buys at price + d).
/// - Genesis as draw-m3 (K = 16, J(1) = 10 KAS, V0 = 60 KAS), but with T far ahead (no
///   close() follows), then a fan-out of 5N plain funding outputs (one per buy) from the genesis
///   change; both chain-merged before the first repetition.
/// - N = 10 repetitions; repetition i uses the fresh genesis shard i and then its successor.
///   Part R (spec A6 steps 1-3):
///   1. Race: buys A and B (b = 1, different combinations and funding inputs, the same shard
///      state; B pays twice A's fee, a strictly higher feerate). A is submitted, then B at once
///      through `submit_transaction` (RbfPolicy::Forbidden). Expected: B rejected as a double spend.
///   2. Replace: the same B through `submit_transaction_replacement` (RbfPolicy::Mandatory). The
///      node returns the replaced transaction; the probe records whether it is A. Which of A and
///      B a chain block then accepts is read from the virtual chain (the node's acceptance of
///      the replacement does not decide it: a block that already holds A removes B,
///      `mining/src/mempool/handle_new_block_transactions.rs`).
///   3. Chain: buy C on the unaccepted successor shard of the node's winner (B if the
///      replacement was accepted), built with the creation DAA score the mempool gives an
///      in-pool parent's output, UNACCEPTED_DAA_SCORE (pushed as -1), and submitted right after
///      step 2 (RBF evicts descendants, `replace_by_fee.rs:124-129`); if rejected, the C built on
///      the other candidate is tried next, every 200 ms, for up to 60 s. Both Cs are built before
///      the race. Every distinct error is recorded.
///   Part F (clean timing, step 4): on the shard's chain-merged tip after part R, a buy P and,
///   at once, a buy Q chained on P's unaccepted successor (Q built before P is submitted), with
///   no replacement in between. τ_chain = start of P's submission -> Q accepted (Q retried every
///   200 ms on rejection). τ_merge = start of a winner's submission -> a chain block accepted it
///   (virtual chain polled every 200 ms), for the part R winner and for P. The node entry of
///   each merged change output is read with `merged()` (its DAA = the merging chain block's).
/// Locked on testnet afterwards: every object of the instance. Unused funding and changes are ours.
pub async fn conflict_m6(ctx: &mut Ctx) -> Result<(), String> {
    use kaspa_consensus_core::constants::UNACCEPTED_DAA_SCORE;
    use kaspa_consensus_core::tx::TransactionOutpoint as Op;
    use kaspa_rpc_core::api::rpc::RpcApi;
    use lottery::buy::{BuyCase, Final, Templates, b_max};
    use lottery::combo::{Combo, unrank};
    use lottery::genesis::{GenesisCase, covenant_id};
    use lottery::smt::Tree;
    use lottery::state::Payout;
    use std::time::{Duration, Instant};

    const TAG: &str = "conflict-m6";
    const K: usize = 16;
    const N: usize = 10;
    const PER: usize = 5;
    const FUND_EACH: u64 = 3 * KAS;
    const POLL: Duration = Duration::from_millis(200);
    const CHAIN_RETRY_FOR: Duration = Duration::from_secs(60);
    const MERGE_LIMIT: Duration = Duration::from_secs(300);
    let daa = ctx.client.get_server_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    let t = Templates::get(b_max());
    let sk = ctx.sk;
    let me = Payout::Schnorr(secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &sk).unwrap().x_only_public_key().0.serialize());
    // T far ahead (about 11.6 days at 10 BPS): shard check 6 needs creation < T; no close().
    let deadline = daa as i64 + 10_000_000;

    // ---- genesis (as round1_drawn, T far ahead)
    let f = fund(ctx).await?;
    let mut g = GenesisCase::new_with(K, vec![(f.op, f.entry.clone())], &t);
    g.funding_sk = sk;
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
    gf.run().map_err(|(i, e)| format!("local engine rejects {TAG} genesis at input {i}: {e:?}"))?;
    let cov = covenant_id(&gf);
    let gs = sent_of(&gf);
    let g_start = sink(ctx).await?;
    if !ctx.submit(&format!("setup: {TAG} genesis, K = {K}, real templates, R = {:.2} KAS, T = DAA + 10^7", g.reserve() as f64 / KAS as f64), &gs, true).await? {
        return Err(format!("{TAG} genesis rejected"));
    }
    let gid = gs.tx.id();
    let gn = gs.tx.outputs.len() - 1;
    let (gchange, _) = merged(ctx, g_start, Op::new(gid, gn as u32), &gs.tx.outputs[gn].script_public_key).await?;
    let gdaa = gchange.entry.block_daa_score;
    println!("genesis {gid} at DAA {gdaa}; covenant id {cov}");

    // ---- fan-out: one confirmed plain funding output per buy
    let outs: Vec<TransactionOutput> = (0..PER * N).map(|_| plain(FUND_EACH, &ctx.spk())).collect();
    let fs = ctx.build(&[p2pk_in(&gchange)], &outs, &[])?;
    let f_start = sink(ctx).await?;
    if !ctx.submit(&format!("setup: {TAG} fan-out, {} funding outputs of {} KAS", PER * N, FUND_EACH / KAS), &fs, true).await? {
        return Err(format!("{TAG} fan-out rejected"));
    }
    let fid = fs.tx.id();
    let fl = fs.tx.outputs.len() - 1;
    merged(ctx, f_start, Op::new(fid, fl as u32), &fs.tx.outputs[fl].script_public_key).await?;
    let funds = outs_of(&fs);

    let m = g.controller.m;
    let empty = Tree::new(m as usize);
    let fin = |bc: &BuyCase, ops: &[Op]| -> Final {
        let mut b = bc.build_with(t.clone());
        b.outpoints = ops.to_vec();
        b.finish()
    };
    // A b = 1 buy of `combo` on shard state `shard` (tree `tree`, creation DAA `sdaa`) at
    // outpoint `sop`, funded by `fu`; fee = `fee` or, if 0, the priority fee. Checked in the
    // local engine (with `sdaa`, as the node will see it).
    let buy = |shard: lottery::state::ShardState, tree: Tree, sdaa: u64, sop: Op, fu: &Utxo, combo: Combo, fee: u64, what: &str| -> Result<(BuyCase, Sent), String> {
        let mut bc = BuyCase::new(t.b_max).with_tree(tree);
        bc.shard = shard;
        bc.reserve = g.reserve();
        bc.shard_daa = sdaa;
        bc.cov = cov;
        bc.payout = me;
        bc.funding_sk = sk;
        bc.bundle = vec![combo];
        bc.funding = vec![fu.entry.amount];
        let ops = [sop, fu.op];
        bc.fee = if fee > 0 { fee } else { lottery_fee(&fin(&bc, &ops)) };
        let bf = fin(&bc, &ops);
        bf.run().map_err(|(k, e)| format!("local engine rejects {TAG} {what} at input {k}: {e:?}"))?;
        Ok((bc, sent_of(&bf)))
    };
    // A buy chained on `w`'s successor (output 0 of `wid`), creation DAA unaccepted.
    let chain_on = |w: &BuyCase, wid: kaspa_consensus_core::tx::TransactionId, fu: &Utxo, combo: Combo, what: &str| {
        buy(w.successor(), w.proofs().2, UNACCEPTED_DAA_SCORE, Op::new(wid, 0), fu, combo, 0, what)
    };
    let feerate = |s: &Sent| s.fee as f64 / s.m.priority as f64;
    let secs = |a: Instant, b: Instant| b.saturating_duration_since(a).as_secs_f64();

    let (mut tau_chain_f, mut tau_merge_r, mut tau_merge_f, mut q_merge, mut tau_chain_r) = (vec![], vec![], vec![], vec![], vec![]);
    let (mut race_rejected, mut replace_ok, mut replaced_a, mut chain_win_a, mut chain_win_b) = (0, 0, 0, 0, 0);
    let (mut c_ok, mut q_ok, mut q_first, mut q_before_p) = (0, 0, 0, 0);
    let mut build_secs = vec![];
    for i in 0..N {
        let fu = &funds[PER * i..PER * (i + 1)];
        let combo = |j: usize| Combo::new(unrank((PER * i + j) as u32), 1 + j as u8);
        assert_eq!(g.shards[i].root, empty.root(), "genesis shard {i} holds the empty tree");
        let shard_op = Op::new(gid, GenesisCase::shard_output(i) as u32);

        // ======== Part R: race, replace, chain
        let (a, asent) = buy(g.shards[i].clone(), empty.clone(), gdaa, shard_op, &fu[0], combo(0), 0, &format!("rep {i} buy A"))?;
        let (b, bsent) = buy(g.shards[i].clone(), empty.clone(), gdaa, shard_op, &fu[1], combo(1), 2 * asent.fee, &format!("rep {i} buy B"))?;
        if feerate(&bsent) <= feerate(&asent) {
            return Err(format!("{TAG} rep {i}: B's feerate {} is not above A's {}", feerate(&bsent), feerate(&asent)));
        }
        let (aid, bid) = (asent.tx.id(), bsent.tx.id());
        let tb = Instant::now();
        let (ca, c_on_a) = chain_on(&a, aid, &fu[2], combo(2), &format!("rep {i} C on A"))?;
        let (cb, c_on_b) = chain_on(&b, bid, &fu[2], combo(2), &format!("rep {i} C on B"))?;
        build_secs.push(tb.elapsed().as_secs_f64() / 2.0);

        let start = sink(ctx).await?;
        let t_a = Instant::now();
        let ra = ctx.submit_raw(&asent).await;
        let rtt_a = t_a.elapsed().as_secs_f64();
        let rb = ctx.submit_raw(&bsent).await;
        let t_r = Instant::now();
        let rr = ctx.submit_replacement_raw(&bsent).await;
        let rtt_r = t_r.elapsed().as_secs_f64();
        let replaced = rr.is_ok();
        let (first, second) = if replaced { (("C on B", &c_on_b), ("C on A", &c_on_a)) } else { (("C on A", &c_on_a), ("C on B", &c_on_b)) };
        let t_c = Instant::now();
        let (cres, cerrs) = submit_until(ctx, &[first, second], POLL, CHAIN_RETRY_FOR).await;

        let fee_note = |s: &Sent| format!("fee {} sompi, feerate {:.4} sompi/g", s.fee, feerate(s));
        ctx.row(
            &format!("rep {i} R1: race, buy A (b = 1, shard {i}, {})", fee_note(&asent)),
            &asent,
            &match &ra {
                Ok(()) => format!("accepted (submit {rtt_a:.3} s)"),
                Err(e) => format!("REJECTED: {e}"),
            },
        );
        if let Err(e) = &ra {
            return Err(format!("{TAG} rep {i}: buy A rejected: {e}"));
        }
        let rb_detail = match &rb {
            Ok(()) => "FINDING: ACCEPTED (expected a double-spend rejection)".to_string(),
            Err(e) => {
                race_rejected += 1;
                format!("REJECTED (expected): {e}")
            }
        };
        ctx.row(&format!("rep {i} R1: race, buy B on the same shard state via submit_transaction ({})", fee_note(&bsent)), &bsent, &rb_detail);
        match &rr {
            Ok(tx) => {
                replace_ok += 1;
                let is_a = tx.id() == aid;
                if is_a {
                    replaced_a += 1;
                }
                let d = format!("accepted by the node (submit {rtt_r:.3} s); replaced {} ({})", tx.id(), if is_a { "= A" } else { "NOT A" });
                ctx.row(&format!("rep {i} R2: replace, the same B via submit_transaction_replacement"), &bsent, &d);
            }
            Err(e) => ctx.row(&format!("rep {i} R2: replace, the same B via submit_transaction_replacement"), &bsent, &format!("REJECTED: {e}")),
        }
        let cands = [first, second];
        let (c_case, c_parent) = match cres {
            Some((k, attempts, t_acc)) => {
                c_ok += 1;
                let parent_is_a = cands[k].0 == "C on A";
                let tw = if parent_is_a { t_a } else { t_r };
                tau_chain_r.push(secs(tw, t_acc));
                let d = format!(
                    "{} accepted on attempt {attempts} ({:.3} s after step 2; {:.3} s after its parent's submission){}",
                    cands[k].0,
                    secs(t_c, t_acc),
                    secs(tw, t_acc),
                    if cerrs.is_empty() { String::new() } else { format!("; errors before: {}", cerrs.join(" / ")) }
                );
                ctx.row(&format!("rep {i} R3: chain, buy C on the unaccepted successor shard (creation DAA UNACCEPTED, -1 in the script)"), cands[k].1, &d);
                (Some(if parent_is_a { (ca.clone(), c_on_a.clone()) } else { (cb.clone(), c_on_b.clone()) }), Some(if parent_is_a { aid } else { bid }))
            }
            None => {
                ctx.row(
                    &format!("rep {i} R3: chain, buy C on the unaccepted successor shard"),
                    first.1,
                    &format!("REJECTED for {} s, both candidates: {}", CHAIN_RETRY_FOR.as_secs(), cerrs.join(" / ")),
                );
                (None, None)
            }
        };
        // Which of A and B did a chain block accept?
        let seen = watch_accepted(ctx, start, &[], &[aid, bid], POLL, MERGE_LIMIT).await?;
        let (wa, wb) = (seen.contains_key(&aid), seen.contains_key(&bid));
        let (wname, wcase, wsent, t_w) = match (wa, wb) {
            (true, false) => {
                chain_win_a += 1;
                ("A", &a, &asent, t_a)
            }
            (false, true) => {
                chain_win_b += 1;
                ("B", &b, &bsent, t_r)
            }
            (true, true) => return Err(format!("{TAG} rep {i}: FINDING: both A and B accepted by the chain")),
            (false, false) => return Err(format!("{TAG} rep {i}: neither A nor B accepted within {} s", MERGE_LIMIT.as_secs())),
        };
        let wid = wsent.tx.id();
        let tm = secs(t_w, seen[&wid]);
        tau_merge_r.push(tm);
        let wn = wsent.tx.outputs.len() - 1;
        let (we, we_s) = merged(ctx, start, Op::new(wid, wn as u32), &wsent.tx.outputs[wn].script_public_key).await?;
        // C, if accepted on the chain's winner: wait for it too; the shard tip is then C's successor.
        let mut tip: (BuyCase, Sent, u64) = (wcase.clone(), wsent.clone(), we.entry.block_daa_score);
        let mut cnote = "no C".to_string();
        if let (Some((cc, cs)), Some(cp)) = (&c_case, c_parent) {
            if cp == wid {
                let cid = cs.tx.id();
                let cseen = watch_accepted(ctx, start, &[cid], &[], POLL, MERGE_LIMIT).await?;
                if cseen.contains_key(&cid) {
                    let cn = cs.tx.outputs.len() - 1;
                    let (ce, _) = merged(ctx, start, Op::new(cid, cn as u32), &cs.tx.outputs[cn].script_public_key).await?;
                    cnote = format!("C (on {wname}) merged at DAA {}", ce.entry.block_daa_score);
                    tip = (cc.clone(), cs.clone(), ce.entry.block_daa_score);
                } else {
                    cnote = format!("C (on {wname}) NOT merged within {} s", MERGE_LIMIT.as_secs());
                }
            } else {
                cnote = "FINDING: C accepted by the node on the parent the chain rejected".to_string();
            }
        }
        let summary = format!(
            "rep {i} R summary: node replacement {}; chain winner {wname} (merged at DAA {}, entry seen {we_s:.1} s later); τ_merge = {tm:.2} s; {cnote}",
            if replaced { "accepted" } else { "rejected" },
            we.entry.block_daa_score
        );
        println!("{summary}");
        ctx.report.push(format!("| {summary} | | | | | | | | `{wid}` |"));

        // ======== Part F: P on the merged tip, Q chained on P at once
        let (tcase, tsent, tdaa) = tip;
        let tid = tsent.tx.id();
        let (p, psent) = buy(tcase.successor(), tcase.proofs().2, tdaa, Op::new(tid, 0), &fu[3], combo(3), 0, &format!("rep {i} buy P"))?;
        let pid = psent.tx.id();
        let (_q, qsent) = chain_on(&p, pid, &fu[4], combo(4), &format!("rep {i} chained buy Q"))?;
        let qid = qsent.tx.id();
        let fstart = sink(ctx).await?;
        let t_p = Instant::now();
        let rp = ctx.submit_raw(&psent).await;
        let rtt_p = t_p.elapsed().as_secs_f64();
        ctx.row(
            &format!("rep {i} F: buy P on the merged tip (shard {i}, {} sales before)", tcase.successor().sales / KAS as i64),
            &psent,
            &match &rp {
                Ok(()) => format!("accepted (submit {rtt_p:.3} s)"),
                Err(e) => format!("REJECTED: {e}"),
            },
        );
        if let Err(e) = rp {
            return Err(format!("{TAG} rep {i}: buy P rejected: {e}"));
        }
        let (qres, qerrs) = submit_until(ctx, &[("Q", &qsent)], POLL, CHAIN_RETRY_FOR).await;
        let q_acc = match qres {
            Some((_, attempts, t_acc)) => {
                q_ok += 1;
                if attempts == 1 {
                    q_first += 1;
                }
                let s = secs(t_p, t_acc);
                tau_chain_f.push(s);
                let d = format!(
                    "accepted on attempt {attempts}; τ_chain = {s:.3} s{}",
                    if qerrs.is_empty() { String::new() } else { format!("; errors before: {}", qerrs.join(" / ")) }
                );
                ctx.row(&format!("rep {i} F: buy Q chained on P's unaccepted successor (creation DAA UNACCEPTED, -1 in the script)"), &qsent, &d);
                Some(t_acc)
            }
            None => {
                ctx.row(&format!("rep {i} F: buy Q chained on P's unaccepted successor"), &qsent, &format!("REJECTED for {} s: {}", CHAIN_RETRY_FOR.as_secs(), qerrs.join(" / ")));
                None
            }
        };
        let watch: Vec<_> = if q_acc.is_some() { vec![pid, qid] } else { vec![pid] };
        let fseen = watch_accepted(ctx, fstart, &watch, &[], POLL, MERGE_LIMIT).await?;
        let pm = fseen.get(&pid).map(|&x| secs(t_p, x));
        if let Some(pm) = pm {
            tau_merge_f.push(pm);
        }
        let mut qnote = String::new();
        if let Some(t_acc) = q_acc {
            if let (Some(&pq), Some(&pp)) = (fseen.get(&qid), fseen.get(&pid)) {
                q_merge.push(secs(t_acc, pq));
                let before = t_acc < pp;
                if before {
                    q_before_p += 1;
                }
                qnote = format!("; Q accepted by the mempool {} P was seen chain-accepted; Q chain-accepted {:.2} s after its acceptance", if before { "BEFORE" } else { "after" }, secs(t_acc, pq));
            } else {
                qnote = "; Q NOT chain-accepted within the limit".to_string();
            }
        }
        let pn = psent.tx.outputs.len() - 1;
        let (pe, _) = merged(ctx, fstart, Op::new(pid, pn as u32), &psent.tx.outputs[pn].script_public_key).await?;
        let fsum = format!(
            "rep {i} F summary: P merged at DAA {}; τ_merge(P) = {}{qnote}",
            pe.entry.block_daa_score,
            pm.map(|x| format!("{x:.2} s")).unwrap_or("not seen".into())
        );
        println!("{fsum}");
        ctx.report.push(format!("| {fsum} | | | | | | | | `{pid}` |"));
    }

    let line = |name: &str, v: &[f64]| match stats(v) {
        Some((mean, med, max)) => format!("{name}: n = {}, mean {mean:.3} s, median {med:.3} s, max {max:.3} s", v.len()),
        None => format!("{name}: no samples"),
    };
    let mut pooled = tau_merge_r.clone();
    pooled.extend(&tau_merge_f);
    let r3 = |v: &[f64]| v.iter().map(|x| (x * 1000.0).round() / 1000.0).collect::<Vec<_>>();
    let lines = [
        format!(
            "{TAG} R: race B rejected {race_rejected}/{N}; node accepted the replacement {replace_ok}/{N} (replaced A: {replaced_a}); chain accepted A {chain_win_a}/{N}, B {chain_win_b}/{N}; chained C accepted {c_ok}/{N}"
        ),
        format!("{TAG} F: chained Q accepted {q_ok}/{N} ({q_first} on the first attempt); Q accepted before P was seen chain-accepted: {q_before_p}/{q_ok}"),
        line("τ_chain (F: P submit -> chained Q accepted)", &tau_chain_f),
        line("τ_chain (R: parent submit -> C accepted)", &tau_chain_r),
        line("τ_merge (R: winner submit -> chain-accepted)", &tau_merge_r),
        line("τ_merge (F: P submit -> chain-accepted)", &tau_merge_f),
        line("τ_merge (pooled R + F)", &pooled),
        line("Q merge (Q accepted -> chain-accepted)", &q_merge),
        line("local build time per chained buy (not in τ)", &build_secs),
        format!("τ_chain F samples (s): {:?}", r3(&tau_chain_f)),
        format!("τ_merge R samples (s): {:?}", r3(&tau_merge_r)),
        format!("τ_merge F samples (s): {:?}", r3(&tau_merge_f)),
    ];
    for l in lines {
        println!("{l}");
        ctx.report.push(format!("| {l} | | | | | | | | |"));
    }
    Ok(())
}
