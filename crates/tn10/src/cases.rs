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
    let ok = ctx.submit(&format!("buy() M1 real shard script, b = {bm}, referral, d = 0.11 KAS, R = 1.44 KAS"), &sent, true).await?;
    if ok {
        let outs = outs_of(&sent);
        ctx.sweep(vec![outs[2].clone(), outs[3].clone()]).await?;
    } else {
        ctx.sweep(vec![funding]).await?;
    }
    Ok(())
}
