//! M0 questions 4 and 5: storage mass, the protocol-object reserve R, the
//! minimum ticket deposit and claim payout, and the largest K for close().
//! Measurement only. Masses use MAINNET_PARAMS of the pinned rusty-kaspa v2.1.0;
//! limits are the block limits (refs/rusty-kaspa-v2.1.0/consensus/core/src/config/params.rs:626),
//! which the mempool also applies per transaction (mining/src/mempool/check_transaction_limits.rs:14-29, 38-40).
//!
//! Run: cargo test --test m0_mass -- --nocapture --test-threads=1

use harness::*;

mod common;
use common::*;

const COMPUTE_LIMIT: u64 = 500_000;
const STORAGE_LIMIT: u64 = 500_000;
const TRANSIENT_LIMIT: u64 = 1_000_000;
const fn limit_hard() -> u64 {
    STORAGE_LIMIT
}
const FEE: u64 = KAS / 100;
const COV: Hash = PROBE_COV;

/// Measured in m0_merkle (depth 23, OpBlake2b): sigscript bytes and compute budget of one Merkle update.
const MERKLE_SIGSCRIPT: usize = 3_418;
const MERKLE_BUDGET: u16 = 6;
/// Schnorr signature push (65 bytes) for a P2PK funding input.
const P2PK_SIGSCRIPT: usize = 66;
/// One signature check costs 1000 grams = 100,000 script units (units.rs:5-6); budget covers it.
fn p2pk_budget() -> u16 {
    budget_for(100_000)
}

fn cov_spk() -> ScriptPublicKey {
    // Any 35-byte P2SH script; protocol objects and tickets are P2SH with a covenant binding.
    pay_to_script_hash_script(&[0x51])
}

fn cov_out(v: u64, auth: u16) -> TransactionOutput {
    TransactionOutput::with_covenant(v, cov_spk(), Some(CovenantBinding::new(auth, COV)))
}

fn cov_entry(v: u64) -> UtxoEntry {
    UtxoEntry::new(v, cov_spk(), 0, false, Some(COV))
}

fn plain_entry(v: u64) -> UtxoEntry {
    UtxoEntry::new(v, p2pk_spk(), 0, false, None)
}

/// Smallest value in [1, hi] for which `ok` holds, given `ok` is monotone (false..true).
fn min_value(hi: u64, ok: impl Fn(u64) -> bool) -> Option<u64> {
    if !ok(hi) {
        return None;
    }
    let (mut lo, mut hi) = (1u64, hi);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if ok(mid) { hi = mid } else { lo = mid + 1 }
    }
    Some(lo)
}

fn kas(v: u64) -> String {
    format!("{:.4}", v as f64 / KAS as f64)
}

/// A minimum, rounded *up* to 0.0001 KAS, with the exact sompi value.
fn kas_up(v: u64) -> String {
    format!("{:.4} ({v} sompi)", v.div_ceil(10_000) as f64 / 10_000.0)
}

// ---------------------------------------------------------------- close()

/// close() under decision "reserve R": inputs controller (R + O, O = boost pot and fee
/// accumulator), K shards (R + S/K), caller funding (R + funding_extra); outputs marker
/// (R + S - reward), controller' (R + O, design 5.5), K shards' (R), caller output
/// (funding - R + reward - fee). Sigscript sizes and budgets given.
#[allow(clippy::too_many_arguments)]
fn close_tx(
    k: usize,
    r: u64,
    sales: u64,
    o: u64,
    funding_extra: u64,
    ctl_sig: usize,
    shard_sig: usize,
    ctl_budget: u16,
    shard_budget: u16,
) -> (Transaction, Vec<UtxoEntry>) {
    let reward = sales / 200; // 0.5 % caller reward, placeholder share
    let funding = r + funding_extra; // caller posts the marker's reserve
    let mut inputs = vec![input(0, vec![0; ctl_sig], ctl_budget)];
    let mut entries = vec![cov_entry(r + o)];
    for i in 0..k {
        inputs.push(input(1 + i as u32, vec![0; shard_sig], shard_budget));
        entries.push(cov_entry(r + sales / k as u64));
    }
    inputs.push(input(1 + k as u32, vec![0; P2PK_SIGSCRIPT], p2pk_budget()));
    entries.push(plain_entry(funding));
    let mut outputs = vec![cov_out(r + sales - reward, 0), cov_out(r + o, 0)];
    for i in 0..k {
        outputs.push(cov_out(r, 1 + i as u16));
    }
    outputs.push(TransactionOutput::new(funding - r + reward - FEE, p2pk_spk()));
    (tx(inputs, outputs), entries)
}

/// Gross sales, controller value O (boost pot + fee accumulator) and caller funding (beyond the
/// posted reserve) for the worst case. A large O or funding input removes the input relief term;
/// a ~1 KAS caller output adds about C/0.99 KAS = 10,101. Below about 3.96 KAS (K < 48) the binding
/// case is O = 0 with large funding (K + 2 R-outputs, no relief); above it, a large O with a 1 KAS
/// caller output (K + 1 R-outputs plus the small caller output, no relief).
const SALES: [u64; 3] = [0, 10_000 * KAS, 1_000_000 * KAS];
const BOOST: [u64; 3] = [0, 10_000 * KAS, 1_000_000 * KAS];
const FUNDING_EXTRA: [u64; 3] = [KAS, 1_000 * KAS, 10_000_000 * KAS];

/// Minimum reserve R such that close() storage mass <= `limit`, worst case over sales and funding.
fn min_reserve_close(k: usize, limit: u64) -> u64 {
    let mut worst = 0;
    for sales in SALES {
        for o in BOOST {
            for extra in FUNDING_EXTRA {
                let r = min_value(10_000 * KAS, |r| {
                    let (t, e) = close_tx(k, r, sales, o, extra, 0, 0, 0, 0);
                    masses(&t, &e).storage <= limit
                })
                .expect("some reserve fits");
                worst = worst.max(r);
            }
        }
    }
    worst
}

#[test]
fn q4_reserve_r_by_k() {
    println!("\nMinimum reserve R per protocol object; close() creates marker + controller + K shards (K + 2 covenant outputs)");
    println!("Worst case over gross sales {{0, 1e4, 1e6}} KAS, controller value O {{0, 1e4, 1e6}} KAS and caller funding {{1, 1e3, 1e7}} KAS beyond the posted reserve.");
    println!("| K | min R (KAS), storage <= 500,000 | min R (KAS), storage <= 50,000 (10 % of block) |");
    println!("|---|---|---|");
    for k in [1usize, 4, 8, 16, 32, 64, 128] {
        let hard = min_reserve_close(k, STORAGE_LIMIT);
        let soft = min_reserve_close(k, STORAGE_LIMIT / 10);
        println!("| {k} | {} | {} |", kas_up(hard), kas_up(soft));
        // Cross-check against the no-relief closed form. Each fresh covenant output (plurality 2)
        // adds 4C/R. Either all K + 2 outputs sit at R (O = 0, large funding), or the controller
        // keeps a large O and a ~0.99 KAS caller output adds C/0.99 KAS = 10,101.
        let c4 = 4 * 1_000_000_000_000u64;
        let closed_form = (c4 * (k as u64 + 2) / limit_hard()).max(c4 * (k as u64 + 1) / (limit_hard() - 10_101));
        assert!(hard <= closed_form + closed_form / 200, "k={k}: measured {hard} above closed form {closed_form}");
        assert!(hard * 1000 >= closed_form * 995, "k={k}: measured {hard} below closed form {closed_form}");
    }
}

/// Controller (sigscript bytes, committed budget, script units, redeem bytes) at K, from the
/// probe run in the engine under committed budgets (all shard inputs and the funding input run too).
fn controller_at(k: usize) -> (usize, u16, u64, usize) {
    let s = Shape { max_k: k, ..Shape::ok(k) };
    let (t, _, units) = run_close(&s).unwrap_or_else(|e| panic!("k={k}: probe rejected: {e:?}"));
    let ctl_budget = t.inputs[0].compute_commit.compute_budget().unwrap();
    (t.inputs[0].signature_script.len(), ctl_budget, units[0], bytecode(&controller(7, k)).len())
}

fn close_fits(k: usize, shard_sig: usize, ctl: (usize, u16, u64, usize)) -> (bool, Masses) {
    let (t, e) = close_tx(k, 10 * KAS, 0, 0, 1_000 * KAS, ctl.0, shard_sig, ctl.1, 1);
    let m = masses(&t, &e);
    (m.compute <= COMPUTE_LIMIT && m.transient <= TRANSIENT_LIMIT, m)
}

const SHARD_SIGS: [usize; 3] = [MERKLE_SIGSCRIPT, 6_000, 10_000];

#[test]
fn q5_largest_k_for_close() {
    println!("\nclose() size and mass by K. Controller = compiled controller_probe.sil with MAX_K = K, run in the engine under its committed budget.");
    println!("Shard inputs are sized as a shard sigscript of S bytes with compute budget 1 (assumption: close leg < 19,999 units).");
    println!("| K | controller redeem B | controller units | S = 3,418 B: compute / transient | S = 6,000 B | S = 10,000 B |");
    println!("|---|---|---|---|---|---|");
    for k in [8usize, 16, 32, 48, 64] {
        let ctl = controller_at(k);
        let cells: Vec<String> = SHARD_SIGS
            .iter()
            .map(|&sz| {
                let (fits, m) = close_fits(k, sz, ctl);
                format!("{} / {}{}", m.compute, m.transient, if fits { "" } else { " (over)" })
            })
            .collect();
        println!("| {k} | {} | {} | {} | {} | {} |", ctl.3, ctl.2, cells[0], cells[1], cells[2]);
    }
    // Exact largest K per shard sigscript size (fits is monotone in K: every mass term grows with K).
    let mut largest = vec![];
    for sz in SHARD_SIGS {
        let (mut lo, mut hi) = (1usize, 128usize);
        assert!(close_fits(lo, sz, controller_at(lo)).0);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if close_fits(mid, sz, controller_at(mid)).0 { lo = mid } else { hi = mid - 1 }
        }
        largest.push(lo);
    }
    println!(
        "largest K within compute 500,000 and transient 1,000,000: S=3,418 -> {}, S=6,000 -> {}, S=10,000 -> {}",
        largest[0], largest[1], largest[2]
    );
    assert!(largest[0] >= 16, "design 8.3 expects K in 16-64");
}

// ---------------------------------------------------------------- buy()

/// buy() of a bundle of b combinations (design 7.1): inputs shard (R + pot), funding; outputs
/// shard' (R + pot + b*price - referral), one ticket holding `ticket` (= d x b), the optional
/// plain referral output, change. `funding_extra` is what the buyer's change output keeps.
fn buy_tx_ref(b: usize, ticket: u64, price: u64, r: u64, pot: u64, funding_extra: u64, referral: Option<u64>) -> (Transaction, Vec<UtxoEntry>) {
    let paid = b as u64 * price;
    let rf = referral.unwrap_or(0);
    let funding = paid + ticket + funding_extra + FEE;
    let inputs = vec![input(0, vec![0; MERKLE_SIGSCRIPT], MERKLE_BUDGET), input(1, vec![0; P2PK_SIGSCRIPT], p2pk_budget())];
    let entries = vec![cov_entry(r + pot), plain_entry(funding)];
    let mut outputs = vec![cov_out(r + pot + paid - rf, 0), cov_out(ticket, 0)];
    if let Some(v) = referral {
        outputs.push(TransactionOutput::new(v, p2pk_spk()));
    }
    outputs.push(TransactionOutput::new(funding_extra, p2pk_spk()));
    (tx(inputs, outputs), entries)
}

/// One-combination buy() without a referral output.
fn buy_tx(n: usize, d: u64, price: u64, r: u64, pot: u64, funding_extra: u64) -> (Transaction, Vec<UtxoEntry>) {
    buy_tx_ref(n, d * n as u64, price, r, pot, funding_extra, None)
}

/// Worst case over the shard's pot (0 = first buy on a fresh shard holding only R, or 1,000 KAS)
/// and the buyer's change (1 KAS, which adds C/1 KAS, or 1e7 KAS, which removes input relief).
const POTS: [u64; 2] = [0, 1_000 * KAS];
const CHANGE: [u64; 2] = [KAS, 10_000_000 * KAS];

fn worst_buy_storage(n: usize, d: u64, price: u64, r: u64) -> u64 {
    worst_buy_storage_ref(n, d * n as u64, price, r, None)
}

fn worst_buy_storage_ref(b: usize, ticket: u64, price: u64, r: u64, referral: Option<u64>) -> u64 {
    let mut worst = 0;
    for pot in POTS {
        for change in CHANGE {
            let (t, e) = buy_tx_ref(b, ticket, price, r, pot, change, referral);
            worst = worst.max(masses(&t, &e).storage);
        }
    }
    worst
}

/// Minimum ticket value (d x b) with every buy() of this shape valid, worst case; None if no value fits.
fn min_ticket_value(b: usize, price: u64, r: u64, referral: Option<u64>) -> Option<u64> {
    min_value(100 * KAS, |v| worst_buy_storage_ref(b, v, price, r, referral) <= STORAGE_LIMIT)
}

/// Minimum deposit d with every buy() of n tickets valid (storage <= 500,000), worst case.
fn min_deposit(n: usize, price: u64, r: u64) -> u64 {
    min_value(100 * KAS, |d| worst_buy_storage(n, d, price, r) <= STORAGE_LIMIT).unwrap()
}

/// Ticket prices tested; the lower one is the worst case (smaller shard successor after a first buy).
const PRICES: [u64; 2] = [KAS / 10, KAS];

#[test]
fn q4_minimum_ticket_deposit() {
    let r16 = min_reserve_close(16, STORAGE_LIMIT);
    let (t, e) = buy_tx(1, KAS, KAS, r16, 1_000 * KAS, KAS);
    let m1 = masses(&t, &e);
    let by_compute = COMPUTE_LIMIT / m1.compute;
    let by_transient = TRANSIENT_LIMIT / m1.transient;
    println!(
        "
buy() with one ticket (shard sigscript {MERKLE_SIGSCRIPT} B): compute {}, transient {}, min fee {} KAS",
        m1.compute,
        m1.transient,
        kas(min_relay_fee(&m1))
    );
    println!("buys per block limited by compute: {by_compute}, by transient: {by_transient}");
    println!("Worst case over shard pot {{0 (fresh shard holding only R), 1,000}} KAS and buyer change {{1, 1e7}} KAS.");

    println!("
One ticket, R = {} KAS (K = 16), price 0.1 KAS:", kas_up(r16));
    println!("| deposit d (KAS) | worst storage mass | buys/block by storage |");
    println!("|---|---|---|");
    for d in [KAS / 100, 8 * KAS / 100, 9 * KAS / 100, KAS / 10, KAS / 2, KAS, 2 * KAS, 5 * KAS, 10 * KAS] {
        let s = worst_buy_storage(1, d, KAS / 10, r16);
        let per_block = if s == 0 { u64::MAX } else { STORAGE_LIMIT / s };
        println!("| {} | {s} | {} |", kas(d), if s > STORAGE_LIMIT { "invalid".into() } else { per_block.to_string() });
    }

    println!("
Minimum deposit, one ticket (every buy() valid):");
    println!("| K (sets R) | R (KAS) | price 0.1 KAS | price 1 KAS |");
    println!("|---|---|---|---|");
    for k in [16usize, 32, 64] {
        let r = min_reserve_close(k, STORAGE_LIMIT);
        let cells: Vec<String> = PRICES.iter().map(|&p| kas_up(min_deposit(1, p, r))).collect();
        println!("| {k} | {} | {} | {} |", kas_up(r), cells[0], cells[1]);
    }

    // Throughput: in the worst case a buy() carries a storage floor that no deposit removes
    // (fresh shard successor at R + price, small buyer change). The deposit can only make the
    // ticket's own share small, so the balanced deposit is measured on a steady-state buy
    // (pot 1,000 KAS, large change) where the ticket output dominates.
    let bottleneck = by_compute.min(by_transient);
    let floor = worst_buy_storage(1, 1_000_000 * KAS, KAS / 10, r16);
    let balanced = min_value(100 * KAS, |d| {
        let (t, e) = buy_tx(1, d, KAS / 10, r16, 1_000 * KAS, 10_000_000 * KAS);
        let s = masses(&t, &e).storage;
        s == 0 || STORAGE_LIMIT / s >= bottleneck
    })
    .unwrap();
    println!(
        "
worst-case storage floor per buy() independent of d (fresh shard, 1 KAS change): {floor} ({} buys/block)",
        STORAGE_LIMIT / floor
    );
    println!(
        "steady-state minimum deposit so storage is not the block bottleneck (>= {bottleneck} buys/block), K = 16, price 0.1: {} KAS",
        kas_up(balanced)
    );
    // Bundles (design 7.1): one ticket UTXO holds d x b, so the floor applies to the ticket's value.
    println!("\nBundle of b combinations, one ticket UTXO (K = 16, price 0.1 KAS, no referral): minimum ticket value d x b");
    for b in [1usize, 2, 5, 10] {
        println!("  b = {b}: {} KAS", kas_up(min_ticket_value(b, KAS / 10, r16, None).unwrap()));
    }

    // With the plain referral output (design 5.1): it adds C / referral, so the floor depends on M_ref.
    println!("\nOne combination with a referral output (K = 16): minimum ticket value");
    println!("| referral output (KAS) | price 0.1 KAS | price 1 KAS |");
    println!("|---|---|---|");
    for rf in [None, Some(KAS / 100), Some(KAS / 10), Some(KAS / 2), Some(KAS)] {
        let cells: Vec<String> = PRICES
            .iter()
            .map(|&p| match rf {
                Some(v) if v > p => "n/a (referral above price)".to_string(),
                _ => min_ticket_value(1, p, r16, rf).map(kas_up).unwrap_or_else(|| "none".into()),
            })
            .collect();
        let label = rf.map(kas).unwrap_or_else(|| "none".into());
        println!("| {label} | {} | {} |", cells[0], cells[1]);
    }
}

// ---------------------------------------------------------------- claim()

/// claim(): inputs pool (R + rem), ticket (d), fee funding; outputs pool' (R + rem - prize),
/// payout (prize + d) to the ticket's payout script, submitter change.
fn claim_tx(r: u64, rem: u64, d: u64, prize: u64, funding: u64) -> (Transaction, Vec<UtxoEntry>) {
    let inputs = vec![
        input(0, vec![0; MERKLE_SIGSCRIPT], MERKLE_BUDGET),
        input(1, vec![0; 200], 1),
        input(2, vec![0; P2PK_SIGSCRIPT], p2pk_budget()),
    ];
    let entries = vec![cov_entry(r + rem), cov_entry(d), plain_entry(funding)];
    let outputs = vec![
        cov_out(r + rem - prize, 0),
        TransactionOutput::new(prize + d, p2pk_spk()),
        TransactionOutput::new(funding - FEE, p2pk_spk()),
    ];
    (tx(inputs, outputs), entries)
}

const CLAIM_FUNDING: [u64; 2] = [10 * KAS, 10_000_000 * KAS];

/// Non-final claim on a bundle (design 7.4): inputs pool (R + rem), ticket (d x b), fee funding;
/// outputs pool' (R + rem - prize), ticket successor (d x b), payout (prize only), submitter change.
fn nonfinal_claim_tx(r: u64, rem: u64, ticket: u64, prize: u64, funding: u64) -> (Transaction, Vec<UtxoEntry>) {
    let inputs = vec![
        input(0, vec![0; MERKLE_SIGSCRIPT], MERKLE_BUDGET),
        input(1, vec![0; 200], 1),
        input(2, vec![0; P2PK_SIGSCRIPT], p2pk_budget()),
    ];
    let entries = vec![cov_entry(r + rem), cov_entry(ticket), plain_entry(funding)];
    let outputs = vec![
        cov_out(r + rem - prize, 0),
        cov_out(ticket, 1),
        TransactionOutput::new(prize, p2pk_spk()),
        TransactionOutput::new(funding - FEE, p2pk_spk()),
    ];
    (tx(inputs, outputs), entries)
}

#[test]
fn q4_minimum_claim_payout() {
    let r = min_reserve_close(16, STORAGE_LIMIT);
    let d = min_deposit(1, KAS / 10, r);
    println!(
        "\nclaim() (worst case over submitter funding 10 and 1e7 KAS) with pool reserve R = {} KAS (min for K = 16) and deposit d = {} KAS (hard minimum)",
        kas_up(r),
        kas_up(d)
    );
    println!("| prize (KAS) | pool after claim | storage mass |");
    println!("|---|---|---|");
    for prize in [0, KAS / 1000, KAS / 100, KAS, 100 * KAS] {
        // Worst case: this claim takes the pool down to its reserve.
        let worst = CLAIM_FUNDING
            .into_iter()
            .map(|f| {
                let (t, e) = claim_tx(r, prize, d, prize, f);
                masses(&t, &e).storage
            })
            .max()
            .unwrap();
        println!("| {} | R | {worst} |", kas(prize));
        assert!(worst <= STORAGE_LIMIT);
    }
    // Smallest payout output (prize + deposit) any claim can have, with the ticket input of the same value.
    let min_payout = CLAIM_FUNDING
        .into_iter()
        .map(|f| {
            min_value(100 * KAS, |p| {
                let (t, e) = claim_tx(r, 0, p, 0, f);
                masses(&t, &e).storage <= STORAGE_LIMIT
            })
            .unwrap()
        })
        .max()
        .unwrap();
    println!("minimum payout output (prize + deposit), pool at R: {} KAS", kas_up(min_payout));
    assert!(min_payout <= d, "deposit must cover the minimum payout on a ticket's last claim");

    // Earlier claims on a bundle pay the prize alone (the deposit stays in the ticket successor),
    // so the prize itself needs a floor; a zero prize would be a zero-value output, which
    // consensus rejects (tx_validation_in_isolation.rs:150-151).
    println!("\nNon-final claim on a bundle (payout = prize only), pool taken to R, d = {} KAS per combination:", kas_up(d));
    for b in [2u64, 5, 10] {
        let min_prize = CLAIM_FUNDING
            .into_iter()
            .map(|f| {
                min_value(100 * KAS, |p| {
                    let (t, e) = nonfinal_claim_tx(r, p, d * b, p, f);
                    masses(&t, &e).storage <= STORAGE_LIMIT
                })
                .unwrap()
            })
            .max()
            .unwrap();
        println!("  b = {b}: minimum prize {} KAS", kas_up(min_prize));
    }
}

// ---------------------------------------------------------------- fee for priority

/// The relay floor ignores storage mass, but the mempool ranks transactions by fee over the
/// normalized max of compute, transient and storage mass (feerate_key.rs:86-98). Shows the fee
/// each worst-case shape needs to rank at the minimum feerate of 100 sompi per gram.
#[test]
fn fee_for_priority_includes_storage() {
    let r16 = min_reserve_close(16, STORAGE_LIMIT);
    let d = min_deposit(1, KAS / 10, r16);
    println!("\n| shape | compute | transient | storage | relay floor (KAS) | fee at 100 sompi/gram of priority mass (KAS) |");
    println!("|---|---|---|---|---|---|");
    let row = |name: &str, t: &Transaction, e: &[UtxoEntry]| {
        let m = masses(t, e);
        println!("| {name} | {} | {} | {} | {} | {} |", m.compute, m.transient, m.storage, kas(min_relay_fee(&m)), kas(priority_fee(&m)));
        m
    };
    let (t, e) = buy_tx(1, d, KAS / 10, r16, 0, KAS);
    let (t, e) = buy_tx(1, d, KAS / 10, r16, 0, 10_000_000 * KAS);
    let worst_buy = row("buy(), d at minimum, fresh shard, large change (worst)", &t, &e);
    let (t, e) = buy_tx(1, 5 * KAS, KAS / 10, r16, 1_000 * KAS, 10_000_000 * KAS);
    row("buy(), d = 5 KAS, steady state", &t, &e);
    let ctl = controller_at(16);
    let (t, e) = close_tx(16, r16, 0, 0, 10_000_000 * KAS, ctl.0, MERKLE_SIGSCRIPT, ctl.1, 1);
    let worst_close = row("close(), K = 16, R at minimum, O = 0, large funding (worst at K = 16)", &t, &e);
    assert!(priority_fee(&worst_buy) > min_relay_fee(&worst_buy));
    assert!(priority_fee(&worst_close) > min_relay_fee(&worst_close));
}
