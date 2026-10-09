//! Mass-limit checks G2 and G3 of docs/research/kaspa-best-practices.md (local part).
//! G2: storage mass of covenant outputs (plurality 2) and how much KIP-9 input relief
//! offsets the K + 2 fresh reserves of close(). G3: the smallest valid output value.
//! Masses use MAINNET_PARAMS of rusty-kaspa v2.1.0; testnet-10 has the same block mass limits
//! (consensus/core/src/config/params.rs:626, :683). G1 (which limit the network relays) is
//! checked on testnet-10 with `cargo run -p tn10`.
//!
//! Run: cargo test --release --test m0_limits -- --nocapture --test-threads=1

use harness::*;

mod common;
use common::*;

const STORAGE_LIMIT: u64 = 500_000;
/// KIP-9 text's standard transaction mass limit, which the v2.1.0 mempool does not apply.
const KIP9_STANDARD: u64 = 100_000;
const FEE: u64 = KAS / 100;
const COV: Hash = PROBE_COV;
const C: u64 = 1_000_000_000_000;

fn cov_spk() -> ScriptPublicKey {
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
fn kas(v: u64) -> String {
    format!("{:.4}", v as f64 / KAS as f64)
}

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

/// close() with storage-relevant values only (scripts do not affect storage mass):
/// inputs controller (R + O), K shards (R + S/K), caller funding inputs; outputs marker (R + S),
/// controller' (R + O), K shards' (R), caller change (funding - R - fee).
fn close_storage(k: usize, r: u64, sales: u64, o: u64, funding: &[u64]) -> u64 {
    let mut inputs = vec![input(0, vec![], 0)];
    let mut entries = vec![cov_entry(r + o)];
    for i in 0..k {
        inputs.push(input(1 + i as u32, vec![], 0));
        entries.push(cov_entry(r + sales / k as u64));
    }
    for (j, &f) in funding.iter().enumerate() {
        inputs.push(input(1 + k as u32 + j as u32, vec![], 0));
        entries.push(plain_entry(f));
    }
    let total: u64 = funding.iter().sum();
    let mut outputs = vec![cov_out(r + sales, 0), cov_out(r + o, 0)];
    for i in 0..k {
        outputs.push(cov_out(r, 1 + i as u16));
    }
    outputs.push(TransactionOutput::new(total - r - FEE, p2pk_spk()));
    masses(&tx(inputs, outputs), &entries).storage
}

/// Funding profiles for the caller of close(): (label, sales, O, funding inputs).
fn profiles(r: u64) -> Vec<(&'static str, u64, u64, Vec<u64>)> {
    vec![
        ("O = 0, one 1e7 KAS input (sets R below K = 48)", 0, 0, vec![10_000_000 * KAS]),
        ("large O = 1e6 KAS, one input of R + 1 KAS (sets R from K = 48)", 0, 1_000_000 * KAS, vec![r + KAS]),
        ("typical: S = 1,000, O = 100, one 50 KAS input", 1_000 * KAS, 100 * KAS, vec![50 * KAS]),
        ("typical: S = 1,000, O = 100, three 20 KAS inputs", 1_000 * KAS, 100 * KAS, vec![20 * KAS; 3]),
        ("busy: S = 1e5, O = 1e4, one 50 KAS input", 100_000 * KAS, 10_000 * KAS, vec![50 * KAS]),
    ]
}

/// M0 minimum R for K (storage <= 500,000, worst case), from q4_reserve_r_by_k.
fn r_min(k: usize) -> u64 {
    match k {
        16 => 143_998_849,
        32 => 271_997_825,
        62 => {
            // Same closed form the M0 test asserts: max(4C(K+2)/L, 4C(K+1)/(L - 10,101)).
            (4 * C * 64 / STORAGE_LIMIT).max(4 * C * 63 / (STORAGE_LIMIT - 10_101)) + 1
        }
        _ => unreachable!(),
    }
}

#[test]
fn g2_covenant_output_plurality() {
    // One input, one output of the same value: plain P2PK vs covenant-bound P2SH.
    let v = KAS;
    let big = 1_000_000 * KAS;
    let plain = masses(
        &tx(vec![input(0, vec![], 0)], vec![TransactionOutput::new(v, p2pk_spk()), TransactionOutput::new(big - v - FEE, p2pk_spk())]),
        &[plain_entry(big)],
    )
    .storage;
    let cov = masses(
        &tx(vec![input(0, vec![], 0)], vec![cov_out(v, 0), TransactionOutput::new(big - v - FEE, p2pk_spk())]),
        &[plain_entry(big)],
    )
    .storage;
    println!("\nOne 1 KAS output from a 1e6 KAS input: plain P2PK storage {plain}, covenant-bound P2SH storage {cov}");
    // Plurality 1 vs 2 (mass/mod.rs:83-100): C/v vs 4C/v, minus the same small relief.
    assert!(cov > 3 * plain && cov <= 4 * plain + 10);
}

#[test]
fn g2_close_relief_by_funding() {
    println!("\nclose() storage mass with R at the M0 worst-case minimum, by caller funding profile (limit 500,000):");
    println!("| K | R (KAS) | profile | storage | share of 500,000 | under KIP-9's 100,000? |");
    println!("|---|---|---|---|---|---|");
    for k in [16usize, 32, 62] {
        let r = r_min(k);
        for (label, sales, o, funding) in profiles(r) {
            let st = close_storage(k, r, sales, o, &funding);
            println!(
                "| {k} | {} | {label} | {st} | {:.0} % | {} |",
                kas(r),
                st as f64 * 100.0 / STORAGE_LIMIT as f64,
                if st <= KIP9_STANDARD { "yes" } else { "no" }
            );
            assert!(st <= STORAGE_LIMIT, "k={k} {label}: {st}");
        }
    }
    // Reserve needed if a 100,000 per-transaction limit applied (worst case, no relief).
    println!("\nR needed if storage had to stay <= 100,000 per transaction (worst profile):");
    for k in [16usize, 32, 62] {
        let r = min_value(10_000 * KAS, |r| close_storage(k, r, 0, 1_000_000 * KAS, &[r + KAS]) <= KIP9_STANDARD).unwrap();
        println!("  K = {k}: {} KAS (vs {} KAS at 500,000)", kas(r), kas(r_min(k)));
    }
}

/// Storage mass of a transaction spending one large plain input into one output of `v`
/// (plain or covenant-bound) plus large change: the per-output floor with negligible relief.
fn single_output_storage(v: u64, covenant: bool) -> u64 {
    let big = 10_000_000 * KAS;
    let out = if covenant { cov_out(v, 0) } else { TransactionOutput::new(v, p2pk_spk()) };
    let t = tx(vec![input(0, vec![], 0)], vec![out, TransactionOutput::new(big - v - FEE, p2pk_spk())]);
    masses(&t, &[plain_entry(big)]).storage
}

#[test]
fn g3_dust_floor() {
    println!("\nSmallest single output value, large input and change (relief negligible):");
    println!("| output | storage <= 500,000 (v2.1.0 per-tx limit) | storage <= 100,000 (KIP-9 text) | storage <= 10,000 (1/50 of a block) |");
    println!("|---|---|---|---|");
    for (label, cov) in [("plain P2PK", false), ("covenant-bound P2SH", true)] {
        let cells: Vec<String> = [STORAGE_LIMIT, KIP9_STANDARD, 10_000]
            .iter()
            .map(|&lim| kas(min_value(1_000 * KAS, |v| single_output_storage(v, cov) <= lim).unwrap()))
            .collect();
        println!("| {label} | {} | {} | {} |", cells[0], cells[1], cells[2]);
    }
    // Values named by the sources (research G3): Aspectron 0.019 / 0.02 KAS for plain outputs,
    // M0's 0.08 KAS ticket and the research's 0.1 / 0.4 KAS derivations.
    println!("\nNamed values:");
    for (v, cov) in [(1_900_000u64, false), (2_000_000, false), (10_000_000, false), (8_000_000, true), (10_000_000, true), (40_000_000, true)] {
        let st = single_output_storage(v, cov);
        println!(
            "  {} KAS {}: storage {st} -> {}",
            kas(v),
            if cov { "covenant" } else { "plain" },
            if st <= STORAGE_LIMIT { "valid" } else { "over the per-transaction limit" }
        );
    }
    assert!(single_output_storage(1_900_000, false) > STORAGE_LIMIT, "0.019 KAS plain output exceeds 500,000 alone");
    assert!(single_output_storage(2_000_000, false) <= STORAGE_LIMIT);
}
