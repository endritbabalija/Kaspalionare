//! M6 purchase-conflict model (spec 6 D88, A7, A8; research F1, F3).
//!
//! A seeded discrete-event simulation of buyers racing for shards. No network, no scripts:
//! the model only answers "how often does a wallet's `buy()` lose a race for its shard, and
//! how many attempts does a purchase take", as a function of the shard count K, the buy
//! arrival rate λ, the per-shard busy time τ and the wallet's rebuild delay ρ.
//!
//! # Model
//!
//! * K shards. Buys arrive as a Poisson process of rate λ per second (all shards together).
//! * Each first attempt picks a shard uniformly at random (research F1).
//! * **Shard state (protocol semantics, the model).** Each shard remembers the time `t_s` of
//!   the last buy on it that was **accepted**. An attempt at time t on shard s conflicts iff
//!   `t < t_s + τ`: within τ of an accepted buy the wallet still holds the spent (pre-buy)
//!   outpoint, so its transaction is a double spend and the mempool rejects it. A rejected
//!   transaction creates no successor and changes nothing on chain, so **a conflicting
//!   attempt does not restart the busy window**; only an accepted attempt sets `t_s = t`.
//!   τ is the time from an accepted buy until another wallet can build on its successor:
//!   τ_chain if the mempool accepts a buy chained on the unaccepted successor (research F3),
//!   τ_merge if wallets wait until a chain block merges the successor.
//! * **Retry (S3b-1).** A conflicting buy is rebuilt and resubmitted after a fixed delay ρ on
//!   a shard drawn uniformly from the K − 1 shards **other than the one it just lost**. The
//!   wallet has just learned that this shard was spent and, at the moment of the rejection,
//!   does not yet hold its successor; F1 says "a buyer who loses a race retries on another
//!   shard". Only the last lost shard is excluded (memory one); earlier ones may be drawn
//!   again. A retry is an attempt like any other (it can conflict and retry again).
//! * ρ → ∞ means retries never come back within the horizon, so they add no load.
//!
//! # Analytic first-attempt conflict rate at ρ → ∞ (correction S3b-2)
//!
//! With ρ → ∞ the arrivals at one shard are Poisson with rate λ/K (thinning). Under the
//! protocol semantics a shard alternates between idle periods (exponential with mean K/λ,
//! memoryless, ended by the next arrival, which is accepted) and busy periods of exactly τ
//! (arrivals in them are lost and do not extend them). By renewal-reward the busy fraction is
//! τ / (τ + K/λ) and by PASTA an arrival finds the shard busy with probability
//!
//! ```text
//!     B(a) = a / (1 + a),     a = λτ/K
//! ```
//!
//! which is Erlang B with one server (an M/G/1/1 loss system; insensitive to the busy-time
//! distribution).
//!
//! The spec (A7, A8) states `1 − exp(−λτ/K)`. That is the probability that **at least one
//! other arrival** hit the shard in the preceding τ, i.e. the conflict rate when **every**
//! arrival, including a rejected one, restarts the busy window (or "was this outpoint touched
//! within τ"). That does not match the protocol: a rejected double spend leaves the shard's
//! UTXO unchanged. The two agree to first order (`a − a²/2` vs `a − a²`) and differ by more
//! than 0.01 once a ≳ 0.14, so the spec's acceptance criterion cannot hold for the correct
//! semantics. This file asserts the sanity grid against `a / (1 + a)` for the protocol
//! semantics, prints `1 − exp(−a)` next to it, and additionally simulates the
//! reset-on-every-arrival semantics (`Semantics::ResetOnEveryArrival`, not the protocol) and
//! asserts that one against `1 − exp(−a)`, which validates the simulator on both closed forms
//! and shows where the spec's formula comes from.
//!
//! # Finite ρ: the first-attempt rate is the shard utilisation (S3b-5)
//!
//! For any finite ρ and λ < K/τ the system has a steady state in which every buy eventually
//! succeeds, so buys are accepted at rate λ, each shard is busy a fraction (λ/K)·τ = a of the
//! time, and fresh (Poisson) arrivals see time averages (PASTA):
//!
//! ```text
//!     first-attempt conflict rate = a = λτ/K     (finite ρ, steady state)
//! ```
//!
//! independent of ρ, of the retry rule and of the busy-time distribution. Retries are not
//! Poisson, so their conflict rate (and hence mean attempts, P(≥ 3)) has no such closed form
//! and is simulated. The ρ → ∞ value a/(1+a) is a singular limit (retried load leaves the
//! system); the operational figure is a. Test 2 asserts |sim − a| ≤ 0.01 in every unsaturated
//! cell at ρ = RHO_DEFAULT.
//!
//! # Saturation (S3b-4)
//!
//! A shard accepts at most one buy per τ, so the system's saturation throughput is K/τ buys
//! per second. For λ ≥ K/τ the retry population grows without bound and there is no steady
//! state; those table cells are simulated as a **burst**: λ for `BURST_SECS` from idle shards,
//! then no new arrivals, run until every buy has succeeded. Their numbers depend on the burst
//! length and are flagged `SAT`. Cells with λ < K/τ are simulated in steady state (warm-up,
//! then a tagged window of buys, run until every tagged buy has succeeded).
//!
//! # Measured τ (Stage 4)
//!
//! `TAU_CHAIN` and `TAU_MERGE` are the medians measured by the testnet-10 probe `conflict-m6`
//! (spec A6 step 4; run of 2026-10-13, n = 10 each, public testnet-10 node, recorded in
//! `docs/tn10-mass-check.md` M6): τ_chain = 0.919 s (part F: start of the parent buy's
//! submission -> the chained buy accepted; mean 0.920, max 0.925; two RPC round trips to the
//! public node) and τ_merge = 1.655 s (part F: start of the buy's submission -> a chain block
//! accepted it, virtual chain polled about every 0.6 s, so an upper bound by one poll; mean
//! 1.650, max 1.962). Without editing, the table can be printed at other values through the
//! environment: `M6_TAU_CHAIN`, `M6_TAU_MERGE` (seconds, f64 > 0) and
//! `M6_RHO` (rebuild delay, seconds, f64 > 0).
//!
//! Run: `cargo test -p lottery --release --test m6_conflict -- --nocapture --test-threads=1`.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::OnceLock;

/// Busy time when the mempool accepts a buy chained on the unaccepted successor shard
/// (research F3): the median τ_chain measured by `probe conflict-m6` (2026-10-13, n = 10: start
/// of the parent's submission -> chained submission accepted). Override: `M6_TAU_CHAIN`.
pub const TAU_CHAIN: f64 = 0.919;
/// Busy time when wallets wait until a chain block merges the successor shard.
/// The median τ_merge measured by `probe conflict-m6` (2026-10-13, n = 10: start of the
/// submission -> a chain block accepted it). Override: `M6_TAU_MERGE`.
pub const TAU_MERGE: f64 = 1.655;
/// Default rebuild delay ρ (S3b-3): the wallet receives the double-spend rejection, reads
/// another shard's UTXO, rebuilds and signs. Override: `M6_RHO`.
pub const RHO_DEFAULT: f64 = 1.0;

/// Length of the demand burst used for saturated cells (λ ≥ K/τ), seconds.
const BURST_SECS: f64 = 600.0;
/// A tagged buy still failing after this many attempts is counted as censored.
const MAX_ATTEMPTS: u32 = 1_000_000;
/// Hard stop on attempts per simulation run; outstanding tagged buys are then censored.
const EVENT_CAP: u64 = 60_000_000;

const KS: [usize; 3] = [4, 8, 16];
const LAMBDAS: [f64; 5] = [0.1, 0.5, 1.0, 2.0, 5.0];
const TAUS: [f64; 3] = [1.0, 2.0, 5.0];

/// Spec A8 sanity tolerance on the first-attempt conflict rate at ρ → ∞.
const SANITY_TOL: f64 = 0.01;
/// The tolerance must be at least this many standard errors in every cell.
const SANITY_MIN_SE_MULTIPLE: f64 = 5.0;
const SANITY_REPS: u64 = 8;
const SANITY_TAG_PER_REP: usize = 50_000;
/// Tagged buys per steady-state cell at finite ρ (monotonicity test and printed table).
const FINITE_TAG: usize = 200_000;

// ---------------------------------------------------------------------------------------
// Deterministic PRNG (splitmix64), inline so the model needs no new dependency.
// ---------------------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in (0, 1].
    fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 + 1.0) * (1.0 / (1u64 << 53) as f64)
    }
    fn exp(&mut self, rate: f64) -> f64 {
        -self.unit().ln() / rate
    }
    /// Uniform in 0..n (multiply-shift; bias < n / 2^64).
    fn below(&mut self, n: usize) -> usize {
        ((self.next_u64() as u128 * n as u128) >> 64) as usize
    }
}

fn seed_for(tag: u64, k: usize, lambda: f64, tau: f64, rho: f64, rep: u64) -> u64 {
    let mut r = Rng::new(
        tag ^ ((k as u64) << 48)
            ^ lambda.to_bits().rotate_left(17)
            ^ tau.to_bits().rotate_left(33)
            ^ rho.to_bits().rotate_left(49)
            ^ rep.wrapping_mul(0xA24B_AED4_963E_E407),
    );
    r.next_u64();
    r.next_u64()
}

// ---------------------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Semantics {
    /// The protocol: only an accepted buy starts a busy window of τ (loss system).
    Protocol,
    /// NOT the protocol: every arrival, accepted or rejected, restarts the busy window.
    /// Used only to show that it reproduces the spec's `1 − exp(−λτ/K)`.
    ResetOnEveryArrival,
}

#[derive(Clone, Copy, Debug)]
struct Params {
    k: usize,
    lambda: f64,
    tau: f64,
    /// Rebuild delay; `f64::INFINITY` means retries never return (ρ → ∞).
    rho: f64,
    sem: Semantics,
}

impl Params {
    fn a(&self) -> f64 {
        self.lambda * self.tau / self.k as f64
    }
    fn saturation(&self) -> f64 {
        self.k as f64 / self.tau
    }
    fn saturated(&self) -> bool {
        self.lambda >= self.saturation()
    }
}

#[derive(Clone, Copy, Debug)]
enum Mode {
    /// Arrivals forever; tag the first `n_tag` buys arriving at or after `warmup`; stop when
    /// every tagged buy has finished.
    Steady { warmup: f64, n_tag: usize },
    /// Arrivals during `[0, secs]` only, all tagged; run until every buy has finished.
    Burst { secs: f64 },
}

#[derive(Default, Clone, Copy, Debug)]
struct Stats {
    tagged: u64,
    /// Tagged buys whose first attempt conflicted.
    first_conflicts: u64,
    /// Tagged buys that made a third attempt.
    ge3: u64,
    /// Tagged buys that eventually succeeded, and their attempts.
    completed: u64,
    attempts_sum: u64,
    max_attempts: u32,
    /// Tagged buys that never succeeded (attempt cap, event cap). With ρ → ∞ conflicting
    /// buys are not censored; they are dropped and only the first attempt is measured.
    censored: u64,
    events: u64,
}

impl Stats {
    fn first_rate(&self) -> f64 {
        self.first_conflicts as f64 / self.tagged as f64
    }
    fn p_ge3(&self) -> f64 {
        self.ge3 as f64 / self.tagged as f64
    }
    fn mean_attempts(&self) -> f64 {
        self.attempts_sum as f64 / self.completed as f64
    }
}

const UNTAGGED: u32 = u32::MAX;

fn simulate(p: Params, mode: Mode, seed: u64) -> Stats {
    assert!(p.k >= 2 && p.lambda > 0.0 && p.tau > 0.0 && p.rho > 0.0);
    let k = p.k;
    let mut rng = Rng::new(seed);
    let mut busy_until = vec![f64::NEG_INFINITY; k];
    // (time bits, sequence, tagged buy id or UNTAGGED, shard just lost). Times are > 0 so
    // their IEEE bit patterns order like the values.
    let mut heap: BinaryHeap<Reverse<(u64, u64, u32, u32)>> = BinaryHeap::new();
    let mut seq: u64 = 0;
    let mut attempts: Vec<u32> = Vec::new();
    let mut st = Stats::default();
    let mut outstanding: u64 = 0;
    let mut tagging_done = false;
    let mut next_arr = rng.exp(p.lambda);

    loop {
        let arrivals_open = match mode {
            Mode::Steady { .. } => true,
            Mode::Burst { secs } => next_arr <= secs,
        };
        if !arrivals_open {
            tagging_done = true;
        }
        if tagging_done && outstanding == 0 {
            break;
        }
        if st.events >= EVENT_CAP {
            st.censored += outstanding;
            break;
        }
        let retry_first = match heap.peek() {
            Some(Reverse((tb, ..))) => !arrivals_open || f64::from_bits(*tb) < next_arr,
            None => false,
        };
        let (t, id, shard) = if retry_first {
            let Reverse((tb, _, id, lost)) = heap.pop().unwrap();
            // S3b-1: uniform over the K − 1 shards other than the one just lost.
            let mut s = rng.below(k - 1);
            if s >= lost as usize {
                s += 1;
            }
            (f64::from_bits(tb), id, s)
        } else if arrivals_open {
            let t = next_arr;
            next_arr = t + rng.exp(p.lambda);
            let tag = match mode {
                Mode::Steady { warmup, n_tag } => !tagging_done && t >= warmup && {
                    if attempts.len() + 1 == n_tag {
                        tagging_done = true;
                    }
                    true
                },
                Mode::Burst { .. } => true,
            };
            let id = if tag {
                attempts.push(0);
                outstanding += 1;
                (attempts.len() - 1) as u32
            } else {
                UNTAGGED
            };
            (t, id, rng.below(k)) // F1: first attempt uniform over all K shards
        } else {
            break; // no arrivals left and no retries pending
        };

        st.events += 1;
        let conflict = t < busy_until[shard];
        match p.sem {
            Semantics::Protocol => {
                if !conflict {
                    busy_until[shard] = t + p.tau;
                }
            }
            Semantics::ResetOnEveryArrival => busy_until[shard] = t + p.tau,
        }

        let mut retry = conflict && p.rho.is_finite();
        if id != UNTAGGED {
            let a = &mut attempts[id as usize];
            *a += 1;
            let n = *a;
            if n == 1 && conflict {
                st.first_conflicts += 1;
            }
            if n == 3 {
                st.ge3 += 1;
            }
            if !conflict {
                st.completed += 1;
                st.attempts_sum += n as u64;
                st.max_attempts = st.max_attempts.max(n);
                outstanding -= 1;
            } else if !p.rho.is_finite() {
                outstanding -= 1; // ρ → ∞: the retry never returns; first attempt measured
            } else if n >= MAX_ATTEMPTS {
                st.censored += 1;
                outstanding -= 1;
                retry = false;
            }
        }
        if retry {
            seq += 1;
            heap.push(Reverse(((t + p.rho).to_bits(), seq, id, shard as u32)));
        }
    }
    st.tagged = attempts.len() as u64;
    st
}

/// The protocol's closed form at ρ → ∞: Erlang B with one server, a / (1 + a).
fn erlang_b1(a: f64) -> f64 {
    a / (1.0 + a)
}

/// The spec's (A7, A8) closed form: 1 − exp(−a). Correct only for `ResetOnEveryArrival`.
fn spec_formula(a: f64) -> f64 {
    1.0 - (-a).exp()
}

fn env_secs(name: &str, default: f64) -> f64 {
    match std::env::var(name) {
        Ok(v) => {
            let x: f64 = v.trim().parse().unwrap_or_else(|_| panic!("{name}={v:?} is not an f64"));
            assert!(x.is_finite() && x > 0.0, "{name} must be a finite number of seconds > 0");
            x
        }
        Err(_) => default,
    }
}

// ---------------------------------------------------------------------------------------
// ρ → ∞ sanity grid (shared by tests 1 and 2)
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct GridCell {
    k: usize,
    lambda: f64,
    tau: f64,
    /// Protocol semantics: pooled rate and the standard error across replications.
    proto: f64,
    proto_se: f64,
    /// Reset-on-every-arrival semantics: pooled rate and SE.
    reset: f64,
    reset_se: f64,
}

impl GridCell {
    fn a(&self) -> f64 {
        self.lambda * self.tau / self.k as f64
    }
}

fn replicate(sem: Semantics, k: usize, lambda: f64, tau: f64) -> (f64, f64) {
    let p = Params { k, lambda, tau, rho: f64::INFINITY, sem };
    let mode = Mode::Steady { warmup: 20.0 * tau, n_tag: SANITY_TAG_PER_REP };
    let tag = match sem {
        Semantics::Protocol => 0x6D36_5F69_6E66_0001,
        Semantics::ResetOnEveryArrival => 0x6D36_5F69_6E66_0002,
    };
    let (mut hits, mut n) = (0u64, 0u64);
    let mut rates = Vec::with_capacity(SANITY_REPS as usize);
    for rep in 0..SANITY_REPS {
        let st = simulate(p, mode, seed_for(tag, k, lambda, tau, p.rho, rep));
        assert_eq!(st.tagged, SANITY_TAG_PER_REP as u64);
        assert_eq!(st.censored, 0);
        hits += st.first_conflicts;
        n += st.tagged;
        rates.push(st.first_rate());
    }
    let r = SANITY_REPS as f64;
    let mean = rates.iter().sum::<f64>() / r;
    let var = rates.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (r - 1.0);
    // SE of the pooled estimate: the larger of the between-replication SE and the binomial SE
    // (the binomial floor guards against a lucky small spread over 8 replications).
    let pooled = hits as f64 / n as f64;
    let se = (var / r).sqrt().max((pooled * (1.0 - pooled) / n as f64).sqrt());
    (pooled, se)
}

fn grid_inf() -> &'static Vec<GridCell> {
    static GRID: OnceLock<Vec<GridCell>> = OnceLock::new();
    GRID.get_or_init(|| {
        let t0 = std::time::Instant::now();
        let mut out = Vec::new();
        for &k in &KS {
            for &lambda in &LAMBDAS {
                for &tau in &TAUS {
                    let (proto, proto_se) = replicate(Semantics::Protocol, k, lambda, tau);
                    let (reset, reset_se) =
                        replicate(Semantics::ResetOnEveryArrival, k, lambda, tau);
                    out.push(GridCell { k, lambda, tau, proto, proto_se, reset, reset_se });
                }
            }
        }
        println!("(rho -> inf grid simulated in {:.2?})", t0.elapsed());
        out
    })
}

fn steady_or_burst(k: usize, lambda: f64, tau: f64, rho: f64, n_tag: usize, tag: u64) -> (Params, Stats) {
    let p = Params { k, lambda, tau, rho, sem: Semantics::Protocol };
    let mode = if p.saturated() {
        Mode::Burst { secs: BURST_SECS }
    } else {
        Mode::Steady { warmup: 100.0 * tau + 100.0 * rho, n_tag }
    };
    (p, simulate(p, mode, seed_for(tag, k, lambda, tau, rho, 0)))
}

fn check_bounds(p: &Params, st: &Stats, what: &str) {
    assert_eq!(st.censored, 0, "{what}: censored buys");
    assert_eq!(st.completed, st.tagged, "{what}: every tagged buy must succeed");
    let (p1, p3, mean) = (st.first_rate(), st.p_ge3(), st.mean_attempts());
    assert!((0.0..=1.0).contains(&p1), "{what}: first-attempt rate {p1}");
    assert!(p3 <= p1, "{what}: P(>=3) {p3} > first-attempt rate {p1}");
    assert!(mean >= 1.0, "{what}: mean attempts {mean} < 1");
    // Integer identity: Σ attempts ≥ n + #(≥2) + #(≥3).
    assert!(mean + 1e-12 >= 1.0 + p1 + p3, "{what}: mean {mean} < 1 + p1 + p3");
    assert!(st.max_attempts as f64 >= mean, "{what}: max < mean");
    let _ = p;
}

// ---------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------

/// Test 1 (spec A8 sanity, corrected per S3b-2): at ρ → ∞ the simulated first-attempt
/// conflict rate is within 0.01 of the closed form for every cell of
/// K ∈ {4, 8, 16} × λ ∈ {0.1, 0.5, 1, 2, 5}/s × τ ∈ {1, 2, 5} s:
/// protocol semantics against a/(1+a) (asserted), reset-on-every-arrival against
/// 1 − exp(−a) (asserted, simulator cross-check). The spec formula is printed beside the
/// protocol result with the number of cells where it misses by more than 0.01.
#[test]
fn m6_conflict_sanity_grid_rho_infinite() {
    let t0 = std::time::Instant::now();
    let grid = grid_inf();
    println!(
        "M6-conflict sanity grid, rho -> inf, {} reps x {} tagged buys per cell and semantics",
        SANITY_REPS, SANITY_TAG_PER_REP
    );
    println!(
        "{:>3} {:>5} {:>4} {:>7} | {:>8} {:>8} {:>8} {:>6} | {:>8} {:>8} | {:>8} {:>8} {:>8}",
        "K", "lam", "tau", "a", "sim", "a/(1+a)", "|dev|", "dev/SE", "1-e^-a", "|sim-sp|",
        "sim_rst", "1-e^-a", "|dev|"
    );
    let (mut max_dev, mut max_dev_se, mut max_reset_dev, mut max_se) = (0f64, 0f64, 0f64, 0f64);
    let mut spec_miss = 0usize;
    let mut failures = Vec::new();
    for c in grid {
        let a = c.a();
        let want = erlang_b1(a);
        let dev = (c.proto - want).abs();
        let spec = spec_formula(a);
        let dev_spec = (c.proto - spec).abs();
        let dev_reset = (c.reset - spec).abs();
        println!(
            "{:>3} {:>5.1} {:>4.0} {:>7.4} | {:>8.5} {:>8.5} {:>8.5} {:>6.2} | {:>8.5} {:>8.5} | {:>8.5} {:>8.5} {:>8.5}",
            c.k, c.lambda, c.tau, a, c.proto, want, dev, dev / c.proto_se, spec, dev_spec,
            c.reset, spec, dev_reset
        );
        max_dev = max_dev.max(dev);
        max_dev_se = max_dev_se.max(dev / c.proto_se);
        max_reset_dev = max_reset_dev.max(dev_reset);
        max_se = max_se.max(c.proto_se).max(c.reset_se);
        if dev_spec > SANITY_TOL {
            spec_miss += 1;
        }
        if dev > SANITY_TOL {
            failures.push(format!("protocol K={} lam={} tau={}: |{} - {}| > {}", c.k, c.lambda, c.tau, c.proto, want, SANITY_TOL));
        }
        if dev_reset > SANITY_TOL {
            failures.push(format!("reset K={} lam={} tau={}: |{} - {}| > {}", c.k, c.lambda, c.tau, c.reset, spec, SANITY_TOL));
        }
        if SANITY_MIN_SE_MULTIPLE * c.proto_se.max(c.reset_se) > SANITY_TOL {
            failures.push(format!("K={} lam={} tau={}: SE too large for the 0.01 tolerance", c.k, c.lambda, c.tau));
        }
    }
    println!("max |sim - a/(1+a)| (protocol)            = {max_dev:.5}  (max {max_dev_se:.2} SE)");
    println!("max |sim - (1-e^-a)| (reset semantics)    = {max_reset_dev:.5}");
    println!(
        "max SE over cells                         = {max_se:.5}  (tolerance {SANITY_TOL} = {:.1} x max SE)",
        SANITY_TOL / max_se
    );
    println!(
        "SPEC CORRECTION S3b-2: protocol sim misses the spec formula 1-e^-a by > {SANITY_TOL} in {spec_miss} of {} cells",
        grid.len()
    );
    println!("sanity grid time: {:.2?}", t0.elapsed());
    assert!(failures.is_empty(), "sanity grid failures:\n{}", failures.join("\n"));
}

/// Test 2: monotonicity and bounds. At ρ → ∞ (protocol) the first-attempt rate rises in λ
/// and τ and falls in K in every cell. At finite ρ = RHO_DEFAULT, for every pair of
/// unsaturated cells, the first-attempt rate and the mean attempts are monotone the same way
/// (small noise tolerance), retries only add load (rate ≥ the ρ → ∞ rate), and every cell
/// (saturated cells as bursts) satisfies 0 ≤ P(≥3) ≤ p1 ≤ 1 and mean ≥ 1 + p1 + P(≥3) ≥ 1.
#[test]
fn m6_conflict_monotone_and_bounds() {
    let t0 = std::time::Instant::now();
    let grid = grid_inf();
    let at = |k: usize, l: f64, t: f64| {
        grid.iter().find(|c| c.k == k && c.lambda == l && c.tau == t).unwrap().proto
    };
    for &k in &KS {
        for &t in &TAUS {
            for w in LAMBDAS.windows(2) {
                assert!(at(k, w[1], t) > at(k, w[0], t), "rho=inf: not increasing in lambda at K={k} tau={t}");
            }
        }
        for &l in &LAMBDAS {
            for w in TAUS.windows(2) {
                assert!(at(k, l, w[1]) > at(k, l, w[0]), "rho=inf: not increasing in tau at K={k} lam={l}");
            }
        }
    }
    for &l in &LAMBDAS {
        for &t in &TAUS {
            for w in KS.windows(2) {
                assert!(at(w[1], l, t) < at(w[0], l, t), "rho=inf: not decreasing in K at lam={l} tau={t}");
            }
        }
    }

    let rho = RHO_DEFAULT;
    let mut max_util_dev = 0f64;
    let mut fin = Vec::new();
    for &k in &KS {
        for &l in &LAMBDAS {
            for &t in &TAUS {
                let n_tag = FINITE_TAG / 2;
                let (p, st) = steady_or_burst(k, l, t, rho, n_tag, 0x6D36_5F6D_6F6E_0001);
                check_bounds(&p, &st, &format!("K={k} lam={l} tau={t} rho={rho}"));
                if !p.saturated() {
                    // S3b-5: steady state at finite ρ, first-attempt rate = a = λτ/K (PASTA).
                    assert!(
                        (st.first_rate() - p.a()).abs() <= SANITY_TOL,
                        "K={k} lam={l} tau={t}: finite-rho rate {} not within {SANITY_TOL} of a = {}",
                        st.first_rate(),
                        p.a()
                    );
                    max_util_dev = max_util_dev.max((st.first_rate() - p.a()).abs());
                    let inf = at(k, l, t);
                    assert!(
                        st.first_rate() >= inf - 0.005,
                        "K={k} lam={l} tau={t}: finite-rho rate {} below rho=inf rate {inf}",
                        st.first_rate()
                    );
                }
                fin.push((p, st));
            }
        }
    }
    let get = |k: usize, l: f64, t: f64| {
        fin.iter().find(|(p, _)| p.k == k && p.lambda == l && p.tau == t).unwrap()
    };
    const TOL: f64 = 0.003;
    let mut pairs = 0;
    let mut cmp = |lo: &(Params, Stats), hi: &(Params, Stats), what: String| {
        if lo.0.saturated() || hi.0.saturated() {
            return;
        }
        pairs += 1;
        assert!(hi.1.first_rate() + TOL >= lo.1.first_rate(), "rho={rho}: first-attempt rate not monotone: {what}");
        assert!(
            hi.1.mean_attempts() * (1.0 + TOL) >= lo.1.mean_attempts(),
            "rho={rho}: mean attempts not monotone: {what}"
        );
    };
    for &k in &KS {
        for &t in &TAUS {
            for w in LAMBDAS.windows(2) {
                cmp(get(k, w[0], t), get(k, w[1], t), format!("lambda {}->{} K={k} tau={t}", w[0], w[1]));
            }
        }
        for &l in &LAMBDAS {
            for w in TAUS.windows(2) {
                cmp(get(k, l, w[0]), get(k, l, w[1]), format!("tau {}->{} K={k} lam={l}", w[0], w[1]));
            }
        }
    }
    for &l in &LAMBDAS {
        for &t in &TAUS {
            for w in KS.windows(2) {
                // Larger K is the "lower" cell.
                cmp(get(w[1], l, t), get(w[0], l, t), format!("K {}->{} lam={l} tau={t}", w[1], w[0]));
            }
        }
    }
    println!(
        "monotone/bounds: rho=inf grid monotone; rho={rho}: {pairs} unsaturated pairs monotone, {} cells bounded, max |first-attempt - lambda*tau/K| (unsaturated) = {max_util_dev:.5}; time {:.2?}",
        fin.len(),
        t0.elapsed()
    );
}

/// Test 3: the printed table at τ_chain and τ_merge (measured medians, `probe conflict-m6`; env
/// `M6_TAU_CHAIN`, `M6_TAU_MERGE`, `M6_RHO`) for K = 16 (and 8, 4 for context),
/// λ ∈ {0.1, 0.5, 1, 2, 5}/s at finite ρ.
#[test]
fn m6_conflict_table() {
    let t0 = std::time::Instant::now();
    let tau_chain = env_secs("M6_TAU_CHAIN", TAU_CHAIN);
    let tau_merge = env_secs("M6_TAU_MERGE", TAU_MERGE);
    let rho = env_secs("M6_RHO", RHO_DEFAULT);
    let src = |name: &str| if std::env::var(name).is_ok() { "env" } else { "measured median, tn10 conflict-m6 2026-10-13, n = 10" };
    println!(
        "M6-conflict table: tau_chain = {tau_chain} s ({}), tau_merge = {tau_merge} s ({}), rho = {rho} s ({})",
        src("M6_TAU_CHAIN"),
        src("M6_TAU_MERGE"),
        if std::env::var("M6_RHO").is_ok() { "env" } else { "default" }
    );
    println!(
        "steady cells: {} tagged buys after warm-up; SAT cells (lambda >= K/tau): {BURST_SECS} s burst from idle shards, drained",
        FINITE_TAG
    );
    for (name, tau) in [("tau_chain", tau_chain), ("tau_merge", tau_merge)] {
        println!();
        println!("## {name} = {tau} s, rho = {rho} s");
        println!(
            "| {:>2} | {:>5} | {:>7} | {:>6} | {:>10} | {:>9} | {:>9} | {:>9} | {:>9} | {:>9} | {:<6} |",
            "K", "lam/s", "K/tau/s", "load", "first-conf", "a/(1+a)", "1-e^-a", "mean att", "P(>=3)", "max att", "regime"
        );
        for &k in &[16usize, 8, 4] {
            for &l in &LAMBDAS {
                let (p, st) = steady_or_burst(k, l, tau, rho, FINITE_TAG, 0x6D36_5F74_6162_0001);
                check_bounds(&p, &st, &format!("table {name} K={k} lam={l}"));
                println!(
                    "| {:>2} | {:>5.1} | {:>7.3} | {:>6.3} | {:>10.4} | {:>9.4} | {:>9.4} | {:>9.3} | {:>9.4} | {:>9} | {:<6} |",
                    k,
                    l,
                    p.saturation(),
                    l / p.saturation(),
                    st.first_rate(),
                    erlang_b1(p.a()),
                    spec_formula(p.a()),
                    st.mean_attempts(),
                    st.p_ge3(),
                    st.max_attempts,
                    if p.saturated() { "SAT" } else { "steady" }
                );
            }
        }
    }
    println!();
    println!("first-conf: simulated first-attempt conflict rate at rho; load = a = lambda*tau/K = lambda / (K/tau), which is also the");
    println!("steady-state first-attempt rate at any finite rho (S3b-5); a/(1+a): the rho -> inf protocol limit (S3b-2);");
    println!("1-e^-a: spec A7 formula (not the protocol semantics, S3b-2); SAT rows (lambda >= K/tau) depend on the burst length.");
    println!("table time: {:.2?}", t0.elapsed());
}
