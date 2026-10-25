//! `claim()`, `return_deposit()` and `refresh()` (spec M4 A4-A6, D53-D63): the transaction
//! builders and the reference for every output.
//!
//! Shapes (spec M4):
//!   claim (A4)           inputs : 0 ticket (leader, `claim`); 1 pool (`ticket_leg`);
//!                                 2 .. 1 + n funding (zero ID, 1 <= n <= F_MAX)
//!                        outputs: 0 pool'; then FINAL: 1 payout (prize + value);
//!                                 PAY: 1 ticket', 2 payout (prize); ACCRUE: 1 ticket' (+ prize);
//!                                 then at most one change output (last)
//!   return, path b (A5)  inputs : 0 ticket (`return_deposit_after`); 1 .. funding
//!                        outputs: 0 payout (ticket value); [change]
//!   return, path a (A5)  inputs : 0 ticket (`return_deposit_with_pool`); 1 pool
//!                                 (`ticket_leg`); 2 .. funding
//!                        outputs: 0 pool' (byte-identical, same value); 1 payout; [change]
//!   refresh (A6)         inputs : 0 marker, pool or ticket (`refresh`); 1 .. funding
//!                        outputs: 0 the object (byte-identical, same value); [change]
//!
//! Every covenant output is authorized by input 0 under the instance covenant ID (D60). The
//! caller pays the fee from funding: change = funding - fee in every shape (no covenant value
//! pays fees).
//!
//! As `draw.rs`: a `*Case` has public fields; `expected()` derives every output from the
//! `game` reference (never from the script); `build()` returns an editable `Built`, whose
//! `finish()` sizes each input's compute budget with
//! `ComputeBudget::checked_covering_script_units` (harness `budget_for`), commits the storage
//! mass and signs the funding inputs. Inputs use sequence 0 and the transaction lock_time is
//! the case's `lock_time` (DECISIONS fact row on check 7: OpCheckLockTimeVerify needs
//! sequence != MAX and lock_time in the DAA domain; `require(tx.daa >= x)` compiles to it).
//!
//! Builder policy (S5-5): claims, returns and refreshes spend only chain-merged inputs, with
//! the node's UTXO entry (refresh's idle check and nothing else here reads an input DAA
//! score; an unmerged parent has DAA score -1, fact row O-S4-4). In the local engine every
//! input entry carries the creation DAA score the case gives it.
//!
//! Entries are called by full dispatch signature (`SIG_*`, S2-1): until stage 3 compiles
//! them, the ticket, pool and marker reject these transactions at dispatch.

use std::sync::Arc;

use harness::{ArtifactValue, CovenantBinding, Hash, ScriptPublicKey, TransactionOutput, UtxoEntry, outpoint};

use crate::buy::{Built, COV, FUNDING_SK, In, InKind, KAS, TEST_DEPOSIT, Template, Templates, b_max, spk_of};
use crate::close::{FundingIn, TEST_DEADLINE};
use crate::combo::{Combo, bundle_bytes};
use crate::game::{ClaimMode, D_IDLE, D_RET, M_CLAIM, P_MIN, TICKET_B_MAX, claim_amount, claim_mode, tier_of};
use crate::params::{TEST_RESERVE, xonly};
use crate::smt::{PATH_BYTES, Tree, counts_bytes};
use crate::state::{K_MAX, MarkerState, Payout, PoolState, PositionState, RecordState, RequestState, TIER_LOWER0, TicketState};

// ------------------------------------------------------------------------ entry contract

/// Ticket `claim(int idx, byte[] leaves, byte[] siblings, int pool_pl, int pool_sl)` (A4).
pub const SIG_CLAIM: &str = "claim(int,byte[],byte[],int,int)";
/// Ticket `return_deposit_after()` (A5 path b).
pub const SIG_RETURN_AFTER: &str = "return_deposit_after()";
/// Ticket `return_deposit_with_pool(int pool_pl, int pool_sl)` (A5 path a).
pub const SIG_RETURN_WITH_POOL: &str = "return_deposit_with_pool(int,int)";
/// Ticket, pool and marker `refresh()` (A6).
pub const SIG_REFRESH: &str = "refresh()";
/// Pool `ticket_leg(int ticket_pl, int ticket_sl)` (A4 PT1-PT4), the delegate of `claim`
/// and `return_deposit_with_pool`.
pub const SIG_TICKET_LEG: &str = "ticket_leg(int,int)";

// Argument positions of `claim` (A4 "Arguments").
pub const CLAIM_ARG_IDX: usize = 0;
pub const CLAIM_ARG_LEAVES: usize = 1;
pub const CLAIM_ARG_SIBLINGS: usize = 2;
pub const CLAIM_ARG_POOL_PL: usize = 3;
pub const CLAIM_ARG_POOL_SL: usize = 4;

// ------------------------------------------------------------------------ fixtures

/// Winning main numbers of the standalone fixtures.
pub const TEST_WIN5: [u8; 5] = [3, 7, 12, 19, 28];
/// Claim deadline of the standalone fixture pools (D_claim; about the value a round 1 draw
/// of the genesis fixture gives).
pub const TEST_POOL_DEADLINE: i64 = 7_500_000;
/// Lower-tier shares with all five lower slots funded (KP10; share room 1,400 + 100 + 2,500
/// <= 10^4).
pub const ST_ALL: [i64; 5] = [400, 300, 300, 200, 200];
/// Payout of the standalone fixture tickets.
pub const TEST_PAYOUT: Payout = Payout::Schnorr([0x22; 32]);

/// (matched main numbers, bonus match) of a tier code (spec M3 A2 / M4 A4 check 6).
pub fn tier_shape(tier: u8) -> (usize, bool) {
    match tier {
        0 => (5, true),
        1 => (5, false),
        2 => (4, true),
        3 => (4, false),
        4 => (3, true),
        5 => (3, false),
        6 => (2, true),
        _ => panic!("tier {tier} out of range"),
    }
}

/// A canonical combination with exactly `matched` main numbers of `win` (the `variant`-th
/// rotation of which ones) and the winning bonus if `bm`, else another bonus in 1..=m (the
/// `variant`-th one). Different variants give different combinations where possible.
pub fn combo_matching(win: &[u8; 6], matched: usize, bm: bool, m: i64, variant: usize) -> Combo {
    assert!(matched <= 5 && m >= 2);
    let mut main: Vec<u8> = (0..matched).map(|j| win[(variant + j) % 5]).collect();
    let others: Vec<u8> = (1..=crate::combo::MAIN_MAX).filter(|n| !win[..5].contains(n)).collect();
    for j in 0..5 - matched {
        main.push(others[(variant * 3 + j) % others.len()]);
    }
    main.sort_unstable();
    main.dedup();
    assert_eq!(main.len(), 5, "combo_matching: duplicate numbers");
    let bonus = if bm {
        win[5]
    } else {
        let bs: Vec<u8> = (1..=m as u8).filter(|&b| b != win[5]).collect();
        bs[variant % bs.len()]
    };
    Combo::new(main.try_into().unwrap(), bonus)
}

/// A canonical combination of tier `tier` against `win`.
pub fn combo_for_tier(win: &[u8; 6], tier: u8, m: i64, variant: usize) -> Combo {
    let (matched, bm) = tier_shape(tier);
    combo_matching(win, matched, bm, m, variant)
}

/// The payout script of a (class, key) state pair; a class above 2 (range attacks) gets the
/// Schnorr script of the key's first 32 bytes so the builder does not panic (the script
/// rejects the class at check 1).
pub fn payout_spk(class: u8, key: &[u8; 33]) -> ScriptPublicKey {
    if class <= 2 { Payout::from_fields(class, key).spk() } else { Payout::from_fields(0, key).spk() }
}

fn funding_inputs(funding: &[FundingIn], sk: &[u8; 32]) -> Vec<In> {
    funding.iter().map(|f| In { entry: UtxoEntry::new(f.value, spk_of(sk), f.daa, false, None), kind: InKind::Funding }).collect()
}

fn built(t: Arc<Templates>, inputs: Vec<In>, outputs: Vec<TransactionOutput>, lock_time: u64, budget_delta: i32, sk: [u8; 32]) -> Built {
    let outpoints = (0..inputs.len() as u32).map(outpoint).collect();
    Built {
        templates: t,
        args: vec![],
        inputs,
        outputs,
        genesis: vec![],
        lock_time,
        shard_budget: None,
        budget_delta,
        outpoints,
        funding_sk: sk,
        sim: None,
    }
}

/// change = funding - fee (every M4 shape); panics if the funding does not cover the fee.
fn change_value(funding: &[FundingIn], fee: u64) -> u64 {
    let total: u64 = funding.iter().map(|f| f.value).sum();
    total.checked_sub(fee).expect("funding covers the fee")
}

// ------------------------------------------------------------------------ claim()

/// TEST-ONLY consistent false claims (stage 2b attacks, as M3 `Lie`). Each set field replaces
/// one value the reference would derive; the builder computes every output from it, so the
/// transaction is self-consistent except for that claim.
#[derive(Clone, Debug, Default)]
pub struct ClaimLie {
    /// Per-shard count strings (each m counts) instead of the trees'; the leaves argument
    /// and h follow, the siblings stay the real ones (KA2 stale leaf, Z_0 for a non-empty
    /// leaf).
    pub counts: Option<Vec<Vec<u16>>>,
    /// Supply only the first n proofs (KA2 k - 1); h follows.
    pub k_proofs: Option<usize>,
    /// Swap the proofs (leaf and siblings) of shards i and j (KA2); h is unchanged.
    pub swap: Option<(usize, usize)>,
    /// Prove this rank instead of the combination's (KA2 another rank's leaf); h follows.
    pub rank: Option<u32>,
    /// h instead of the sum the leaves show; the prize follows (KA2 under-reported h).
    pub h: Option<i64>,
    /// The prize instead of floor(slice / h) (KA5 inflated or deflated payout).
    pub prize: Option<i64>,
    /// The mode (and so the shape) instead of the reference's (KA8).
    pub mode: Option<ClaimMode>,
}

/// One claim (A4): ticket at input 0, pool at input 1, then funding.
#[derive(Clone)]
pub struct ClaimCase {
    pub b_max: usize,
    pub ticket: TicketState,
    /// d x b + accrued.
    pub ticket_value: u64,
    pub ticket_daa: u64,
    pub pool: PoolState,
    /// Pool value = remaining + pool.reserve (A3.3).
    pub pool_value: u64,
    pub pool_daa: u64,
    /// The K shard trees of the pool's round (the count proofs come from them; their roots
    /// must be pool.roots for a valid case).
    pub trees: Vec<Tree>,
    /// The claimed bundle index (i64: range attacks use -1 and b).
    pub idx: i64,
    pub funding: Vec<FundingIn>,
    /// With a change output (funding - fee) last; without it the funding must equal the fee.
    pub change: bool,
    pub fee: u64,
    /// No time check in `claim()`: 0 by default.
    pub lock_time: u64,
    pub lie: ClaimLie,
    pub cov: Hash,
    pub funding_sk: [u8; 32],
    /// Added to input 0's covering budget (KA16 uses -1).
    pub budget_delta: i32,
}

/// The reference for every output of a claim (A4 checks 1-15).
#[derive(Clone, Debug)]
pub struct ClaimExpected {
    pub idx: i64,
    /// The claimed combination's 6 bytes (zero if idx is outside the bundle).
    pub comb: [u8; 6],
    /// The proven rank (the combination's unless a `Lie` replaces it).
    pub rank: u32,
    /// tier_of(comb, pool.win); `None` = no winning tier.
    pub tier: Option<u8>,
    pub leaves: Vec<u8>,
    pub siblings: Vec<u8>,
    /// The supplied count strings (one per proof).
    pub counts: Vec<Vec<u16>>,
    pub h: i64,
    pub prize: i64,
    /// pool value - reserve.
    pub remaining: i64,
    pub bit: u8,
    pub mode: ClaimMode,
    /// pool' (the pool state unchanged, check 14).
    pub pool: PoolState,
    /// ticket' with the bit set (not FINAL, check 15).
    pub ticket: Option<TicketState>,
    /// Every output value in order (change last, if present).
    pub values: Vec<u64>,
    pub change_output: Option<usize>,
    /// The reference checks that fail ("check N: ..."); empty for a valid claim. Attack
    /// cases may build a transaction the reference rejects; positive tests assert empty.
    pub rejects: Vec<String>,
}

impl ClaimExpected {
    pub fn n_cov(&self) -> usize {
        self.mode.n_cov()
    }
    pub fn payout_output(&self) -> Option<usize> {
        self.mode.payout_output()
    }
    pub fn n_outputs(&self) -> usize {
        self.values.len()
    }
}

impl ClaimCase {
    // Input indices (A4 shape).
    pub const TICKET: usize = 0;
    pub const POOL: usize = 1;
    pub fn funding_input(n: usize) -> usize {
        2 + n
    }

    /// The standalone base (D63 fabricated UTXOs under the instance ID): K = `k`, m = 10, a
    /// b = 2 ticket whose index 0 is a tier 2 (4 + bonus) combination held once (shard 0),
    /// against a tier 2 pool with slice 0.2 KAS: a PAY claim of 0.2 KAS.
    pub fn new(k: usize) -> Self {
        Self::fabricated(k, 10, TIER_LOWER0, 20_000_000, &[1])
    }

    /// A fabricated round 1 claim (D63): `k` shard trees with bonus range `m`; the ticket
    /// (b = 2, shard 0) holds at index 0 a combination of tier `tier` and at index 1 a
    /// non-winning one; shard i holds the tier combination `holders[i]` times (missing
    /// entries are 0; holders[0] >= 1 counts the ticket itself) and one non-winning
    /// combination (every shard non-empty). The pool has the trees' roots, `slice`, and
    /// remaining = slice + 1 KAS.
    pub fn fabricated(k: usize, m: i64, tier: u8, slice: i64, holders: &[u16]) -> Self {
        Self::fabricated_with(k, m, tier, slice, holders, &Templates::get(b_max()))
    }

    pub fn fabricated_with(k: usize, m: i64, tier: u8, slice: i64, holders: &[u16], t: &Templates) -> Self {
        assert!((1..=K_MAX).contains(&k) && (2..=M_CLAIM).contains(&m));
        assert!(holders.len() <= k && holders.first().copied().unwrap_or(0) >= 1, "holders[0] >= 1: the ticket holds the combination");
        let wb = if m >= 4 { 4 } else { 1 };
        let win = [TEST_WIN5[0], TEST_WIN5[1], TEST_WIN5[2], TEST_WIN5[3], TEST_WIN5[4], wb];
        let comb = combo_for_tier(&win, tier, m, 0);
        let filler = combo_matching(&win, 0, false, m, 1);
        let mut trees: Vec<Tree> = (0..k).map(|_| Tree::new(m as usize)).collect();
        for (i, tr) in trees.iter_mut().enumerate() {
            for _ in 0..holders.get(i).copied().unwrap_or(0) {
                tr.increment(comb.rank(), comb.bonus);
            }
            // Shard i's own non-winning filler (shard 0's is the ticket's index 1).
            let f = if i == 0 { filler } else { combo_matching(&win, 1, false, m, 2 + i) };
            tr.increment(f.rank(), f.bonus);
        }
        let roots: Vec<[u8; 32]> = trees.iter().map(|tr| tr.root()).collect();
        let ticket = fabricated_ticket(&[comb, filler], t.b_max);
        let mut pool = PoolState {
            round: 1,
            tier,
            slice,
            win,
            m,
            k: k as i64,
            roots: MarkerState::roots_bytes(&roots),
            deadline: TEST_POOL_DEADLINE,
            ticket_tpl: t.ticket.hash,
            reserve: TEST_RESERVE as i64,
            funder_class: 0,
            funder_key: [0; 33],
        };
        pool.set_funder(&Payout::Schnorr(xonly(&FUNDING_SK)));
        let pool_value = (slice + KAS as i64) as u64 + TEST_RESERVE;
        Self {
            b_max: t.b_max,
            ticket_value: 2 * TEST_DEPOSIT,
            ticket,
            ticket_daa: 300_000,
            pool,
            pool_value,
            pool_daa: 2_000_000,
            trees,
            idx: 0,
            funding: vec![FundingIn { value: 10 * KAS, daa: 2_100_000 }],
            change: true,
            fee: KAS / 10,
            lock_time: 0,
            lie: ClaimLie::default(),
            cov: COV,
            funding_sk: FUNDING_SK,
            budget_delta: 0,
        }
    }

    pub fn k(&self) -> usize {
        self.trees.len()
    }

    /// pool value - reserve.
    pub fn remaining(&self) -> i64 {
        self.pool_value as i64 - self.pool.reserve
    }

    pub fn bit(&self) -> u8 {
        if (0..8).contains(&self.idx) { 1u8 << self.idx } else { 0 }
    }

    /// Mark every index except `idx` claimed, so the claim is FINAL.
    pub fn make_final(&mut self) {
        let full = ((1u16 << self.ticket.b) - 1) as u8;
        self.ticket.claimed[0] = full & !self.bit();
    }

    /// The pool's roots = the trees' roots (zero-padded); k = the number of trees.
    pub fn sync_roots(&mut self) {
        let roots: Vec<[u8; 32]> = self.trees.iter().map(|t| t.root()).collect();
        self.pool.roots = MarkerState::roots_bytes(&roots);
        self.pool.k = self.trees.len() as i64;
    }

    pub fn funding_total(&self) -> u64 {
        self.funding.iter().map(|f| f.value).sum()
    }

    /// The combination at index `idx` (zero bytes if idx is outside the bundle).
    pub fn comb(&self) -> [u8; 6] {
        // F1 part B: the slice bounds in checked arithmetic (an idx near i64::MAX overflowed
        // 6 x idx; W3). The same bytes for every idx that fits.
        let b = &self.ticket.bundle;
        let at = usize::try_from(self.idx).ok().and_then(|i| i.checked_mul(6)).filter(|&s| s.checked_add(6).is_some_and(|e| e <= b.len()));
        match at {
            Some(s) => [b[s], b[s + 1], b[s + 2], b[s + 3], b[s + 4], b[s + 5]],
            None => [0; 6],
        }
    }

    /// Every output of the claim (A4 checks 1-15) from the `game` reference; reference
    /// failures are listed in `rejects` instead of panicking, so attack cases build.
    pub fn expected(&self) -> ClaimExpected {
        let t = &self.ticket;
        let p = &self.pool;
        let mut rejects: Vec<String> = vec![];
        let mut rej = |s: &str| rejects.push(s.to_string());
        // Check 1: own ranges and the index.
        if !(1..crate::game::TOTAL_MAX).contains(&t.round) || !(0..crate::game::DAA_MAX).contains(&t.deadline) {
            rej("check 1: ticket round or deadline");
        }
        if !(1..=TICKET_B_MAX).contains(&t.b) || t.payout_class > 2 {
            rej("check 1: ticket b or payout_class");
        }
        if !(0..t.b).contains(&self.idx) {
            rej("check 1: idx outside [0, b)");
        }
        // Check 3: pool ranges and round.
        if p.tier > 6
            || !(0..=crate::game::TOTAL_MAX).contains(&p.slice)
            || !(2..=M_CLAIM).contains(&p.m)
            || !(1..=K_MAX as i64).contains(&p.k)
            || !(0..crate::game::DAA_MAX).contains(&p.deadline)
            || !(0..=crate::game::CLAIM_RESERVE_MAX).contains(&p.reserve)
            || (self.pool_value as i64) < p.reserve
        {
            rej("check 3: pool ranges");
        }
        if p.round != t.round {
            rej("check 3: pool round != ticket round");
        }
        // Check 5: the combination.
        let comb = self.comb();
        let combo = Combo::new(comb[..5].try_into().unwrap(), comb[5]);
        let canonical = combo.is_canonical(p.m.clamp(0, 255) as u8);
        if !canonical {
            rej("check 5: combination not canonical");
        }
        let rank = self.lie.rank.unwrap_or(if canonical { combo.rank() } else { 0 });
        // Check 6: tier.
        let tier = tier_of(&comb, &p.win);
        if tier != Some(p.tier) {
            rej("check 6: tier");
        }
        // Check 7: proofs.
        let mut counts: Vec<Vec<u16>> = match &self.lie.counts {
            Some(c) => c.clone(),
            None => self.trees.iter().map(|tr| tr.counts(rank)).collect(),
        };
        let mut sibs: Vec<Vec<u8>> = self.trees.iter().map(|tr| tr.siblings(rank)).collect();
        if let Some((i, j)) = self.lie.swap {
            counts.swap(i, j);
            sibs.swap(i, j);
        }
        if let Some(n) = self.lie.k_proofs {
            counts.truncate(n);
            sibs.truncate(n);
        }
        let leaves: Vec<u8> = counts.iter().flat_map(|c| counts_bytes(c)).collect();
        let siblings: Vec<u8> = sibs.concat();
        debug_assert!(siblings.len() == PATH_BYTES * sibs.len());
        let bonus = comb[5] as usize;
        let h_sum: i64 = counts.iter().map(|c| if (1..=c.len()).contains(&bonus) { c[bonus - 1] as i64 } else { 0 }).sum();
        let h = self.lie.h.unwrap_or(h_sum);
        // Check 8: prize.
        let prize = match (self.lie.prize, claim_amount(p.slice, h)) {
            (Some(x), _) => x,
            (None, Some(x)) => x,
            (None, None) => {
                rej("check 8: h < 1");
                0
            }
        };
        let remaining = self.remaining();
        if prize > remaining {
            rej("check 8: prize > remaining");
        }
        // Checks 4 and 9: index unclaimed, mode.
        let c = t.claimed.first().copied().unwrap_or(0);
        let bit = self.bit();
        let reference_mode = claim_mode(c, t.b, self.idx, prize);
        if reference_mode.is_none() && (0..t.b).contains(&self.idx) {
            rej("check 4: index already claimed");
        }
        let mode = self.lie.mode.or(reference_mode).unwrap_or_else(|| {
            // The shape a cheater builds when the reference rejects (KA1, KA12).
            let full = ((1u16 << t.b.clamp(0, 8)) - 1) as u8;
            if c | bit == full {
                ClaimMode::Final
            } else if prize >= P_MIN {
                ClaimMode::Pay
            } else {
                ClaimMode::Accrue
            }
        });
        // Check 12 guard: prize <= TOTAL_MAX - in0.value (FINAL, ACCRUE).
        if mode != ClaimMode::Pay && prize as i128 > crate::game::TOTAL_MAX as i128 - self.ticket_value as i128 {
            rej("check 12: prize + ticket value > TOTAL_MAX");
        }
        // Outputs (checks 10-15).
        // Checked (S4 simnet SN2): a prize above the pool value (e.g. a repeat claim on a
        // 1-sompi remainder) is a refusal, never a pool' that wraps to a huge u64. A value
        // outside [0, u64::MAX] is listed as a check-8/12 reject and its output set to 0, so
        // attack cases still build a well-formed transaction for the script to refuse.
        let mut out = |v: i128, why: &str| -> u64 {
            u64::try_from(v).unwrap_or_else(|_| {
                rejects.push(why.to_string());
                0
            })
        };
        let mut values = vec![out(self.pool_value as i128 - prize as i128, "check 8: prize > pool value (pool' would be negative)")];
        match mode {
            ClaimMode::Final | ClaimMode::Accrue => values.push(out(self.ticket_value as i128 + prize as i128, "check 12: ticket value + prize outside u64")),
            ClaimMode::Pay => {
                values.push(self.ticket_value);
                values.push(out(prize as i128, "check 8: prize negative"));
            }
        }
        let ticket_next = (mode != ClaimMode::Final).then(|| {
            let mut s = t.clone();
            s.claimed[0] = c | bit;
            s
        });
        let change_output = self.change.then(|| {
            values.push(change_value(&self.funding, self.fee));
            values.len() - 1
        });
        if !self.change {
            assert_eq!(self.funding_total(), self.fee, "a claim without change needs funding = fee");
        }
        ClaimExpected {
            idx: self.idx,
            comb,
            rank,
            tier,
            leaves,
            siblings,
            counts,
            h,
            prize,
            remaining,
            bit,
            mode,
            pool: p.clone(),
            ticket: ticket_next,
            values,
            change_output,
            rejects,
        }
    }

    /// The model as a refusal: `Err` (every failing reference check, e.g. "check 4: index
    /// already claimed", "check 8: prize > remaining") instead of a transaction the script
    /// would reject. Offline builders (`from_store_index`, the tn10 runner) use this.
    pub fn checked_expected(&self) -> Result<ClaimExpected, String> {
        let x = self.expected();
        if x.rejects.is_empty() { Ok(x) } else { Err(format!("the claim reference rejects it: {:?}", x.rejects)) }
    }

    /// claim(idx, leaves, siblings, pool_pl, pool_sl).
    pub fn args(&self, t: &Templates, x: &ClaimExpected) -> Vec<ArtifactValue> {
        use ArtifactValue::*;
        vec![Int(self.idx), Bytes(x.leaves.clone()), Bytes(x.siblings.clone()), Int(t.pool.prefix.len() as i64), Int(t.pool.suffix.len() as i64)]
    }

    pub fn build(&self) -> Built {
        self.build_with(Templates::get(self.b_max))
    }

    /// Build against given templates (mutants and look-alikes).
    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        let x = self.expected();
        let cov = Some(self.cov);
        let mut inputs = vec![
            In {
                entry: UtxoEntry::new(self.ticket_value, t.ticket_spk(&self.ticket), self.ticket_daa, false, cov),
                kind: InKind::Entry {
                    template: t.ticket.clone(),
                    entry: SIG_CLAIM.into(),
                    args: self.args(&t, &x),
                    redeem: t.ticket.redeem(&self.ticket.values()),
                },
            },
            In {
                entry: UtxoEntry::new(self.pool_value, t.pool_spk(&self.pool), self.pool_daa, false, cov),
                kind: InKind::Entry { template: t.pool.clone(), entry: SIG_TICKET_LEG.into(), args: ticket_leg_args(&t), redeem: t.pool_redeem(&self.pool) },
            },
        ];
        inputs.extend(funding_inputs(&self.funding, &self.funding_sk));

        let bind = Some(CovenantBinding::new(0, self.cov));
        let payout = payout_spk(self.ticket.payout_class, &self.ticket.payout_key);
        let mut outputs = vec![TransactionOutput::with_covenant(x.values[0], t.pool_spk(&x.pool), bind)];
        match x.mode {
            ClaimMode::Final => outputs.push(TransactionOutput::new(x.values[1], payout)),
            ClaimMode::Pay => {
                outputs.push(TransactionOutput::with_covenant(x.values[1], t.ticket_spk(x.ticket.as_ref().unwrap()), bind));
                outputs.push(TransactionOutput::new(x.values[2], payout));
            }
            ClaimMode::Accrue => outputs.push(TransactionOutput::with_covenant(x.values[1], t.ticket_spk(x.ticket.as_ref().unwrap()), bind)),
        }
        if let Some(i) = x.change_output {
            outputs.push(TransactionOutput::new(x.values[i], spk_of(&self.funding_sk)));
        }
        built(t, inputs, outputs, self.lock_time, self.budget_delta, self.funding_sk)
    }
}

impl ClaimCase {
    /// Spec M8 A4: the next claim of the live ticket `ticket`, built offline from a store
    /// (an index or a wallet, `ClaimStore`) and the caller's funding UTXOs; no RPC type in
    /// the signature. It claims the ticket's first unclaimed index whose combination wins a
    /// tier that has a live pool of the ticket's round (`from_store_index`).
    pub fn from_store<S: crate::store::ClaimStore + ?Sized>(
        store: &S,
        ticket: harness::TransactionOutpoint,
        funding: &[crate::store::Funding],
        payer: &crate::store::Payer,
    ) -> Result<(Self, Vec<harness::TransactionOutpoint>), String> {
        let (tk, _, _) = store.ticket(&ticket)?;
        let pools = store.pools(tk.round)?;
        let claimed = tk.claimed.first().copied().unwrap_or(0xff);
        for idx in 0..tk.b.clamp(0, 8) {
            if claimed & (1u8 << idx) != 0 || tk.bundle.len() < 6 * (idx as usize + 1) {
                continue;
            }
            let comb: [u8; 6] = tk.bundle[6 * idx as usize..6 * idx as usize + 6].try_into().unwrap();
            if pools.iter().any(|p| tier_of(&comb, &p.1.win) == Some(p.1.tier)) {
                return Self::from_store_index(store, ticket, idx, funding, payer);
            }
        }
        Err(format!("ticket {ticket}: no unclaimed index wins a tier with a live pool of round {}", tk.round))
    }

    /// The claim of index `idx` of the live ticket `ticket` from a store: the ticket and the
    /// live pool of its tier come from the store, the count proofs from the round's frozen
    /// trees (`ClaimStore::trees` refuses an incomplete or open round). The proofs are not
    /// checked here against the pool's roots: the store is untrusted and the ticket script
    /// checks every proof against them (D115; `roots_match` is the offline pre-check). The
    /// funding key and the fee are the caller's `payer` (S2c should-fix 2: no default); the
    /// change goes back to the funding key. Returns the case and the outpoints in input order.
    pub fn from_store_index<S: crate::store::ClaimStore + ?Sized>(
        store: &S,
        ticket: harness::TransactionOutpoint,
        idx: i64,
        funding: &[crate::store::Funding],
        payer: &crate::store::Payer,
    ) -> Result<(Self, Vec<harness::TransactionOutpoint>), String> {
        let t = store.templates()?;
        let (tk, tv, tdaa) = store.ticket(&ticket)?;
        let trees = store.trees(tk.round)?;
        if !(0..tk.b).contains(&idx) || tk.bundle.len() < 6 * (idx as usize + 1) {
            return Err(format!("ticket {ticket}: index {idx} outside the bundle"));
        }
        let comb: [u8; 6] = tk.bundle[6 * idx as usize..6 * idx as usize + 6].try_into().unwrap();
        let (pop, pool, pv, pdaa) = store
            .pools(tk.round)?
            .into_iter()
            .find(|p| tier_of(&comb, &p.1.win) == Some(p.1.tier))
            .ok_or_else(|| format!("ticket {ticket} index {idx}: no live pool of a tier it wins"))?;
        let k = trees.len();
        if !(1..=K_MAX).contains(&k) || !(2..=M_CLAIM).contains(&pool.m) {
            return Err(format!("round {}: k = {k}, m = {}", tk.round, pool.m));
        }
        let mut c = Self::fabricated_with(k, pool.m, TIER_LOWER0, 20_000_000, &[1], &t);
        c.b_max = t.b_max;
        c.ticket = tk;
        c.ticket_value = tv;
        c.ticket_daa = tdaa;
        c.pool = pool;
        c.pool_value = pv;
        c.pool_daa = pdaa;
        c.trees = trees;
        c.idx = idx;
        c.cov = store.cov();
        c.funding = funding.iter().map(|f| FundingIn { value: f.value, daa: f.daa }).collect();
        // S2c should-fix 2: the caller's funding key and fee, never the builder defaults.
        c.funding_sk = payer.funding_sk;
        c.fee = payer.fee;
        // S2c should-fix 3 (D115): the store is untrusted; a model panic is an error.
        crate::index::model("claim", || c.checked_expected())?.map_err(|e| format!("ticket {ticket} index {idx}: {e}"))?;
        let mut ops = vec![ticket, pop];
        ops.extend(funding.iter().map(|f| f.outpoint));
        Ok((c, ops))
    }

    /// The offline pre-check of a store's trees: their roots are the pool's frozen roots.
    pub fn roots_match(&self) -> bool {
        let roots: Vec<[u8; 32]> = self.trees.iter().map(|t| t.root()).collect();
        MarkerState::roots_bytes(&roots) == self.pool.roots && self.pool.k == self.trees.len() as i64
    }
}

/// A fabricated ticket (D63) of round 1 with deadline TEST_DEADLINE and TEST_PAYOUT.
pub fn fabricated_ticket(bundle: &[Combo], b_max: usize) -> TicketState {
    let mut bytes = bundle_bytes(bundle);
    bytes.resize(6 * b_max, 0);
    TicketState {
        round: 1,
        deadline: TEST_DEADLINE,
        b: bundle.len() as i64,
        bundle: bytes,
        claimed: vec![0; b_max.div_ceil(8)],
        payout_class: TEST_PAYOUT.class(),
        payout_key: TEST_PAYOUT.key33(),
    }
}

/// ticket_leg(ticket_pl, ticket_sl) of the pool (PT4).
pub fn ticket_leg_args(t: &Templates) -> Vec<ArtifactValue> {
    vec![ArtifactValue::Int(t.ticket.prefix.len() as i64), ArtifactValue::Int(t.ticket.suffix.len() as i64)]
}

// ------------------------------------------------------------------------ return_deposit()

/// The two `return_deposit()` paths (A5, D58).
#[derive(Clone, Debug)]
pub enum ReturnPath {
    /// (b) `return_deposit_after()`: the ticket alone, tx.daa >= T + D_RET.
    After,
    /// (a) `return_deposit_with_pool(pool_pl, pool_sl)`: co-spends a pool of the ticket's
    /// round (value remaining + R) after its deadline; the pool is recreated unchanged.
    WithPool { pool: PoolState, pool_value: u64, pool_daa: u64 },
}

#[derive(Clone)]
pub struct ReturnCase {
    pub b_max: usize,
    pub ticket: TicketState,
    pub ticket_value: u64,
    pub ticket_daa: u64,
    pub path: ReturnPath,
    pub funding: Vec<FundingIn>,
    pub change: bool,
    pub fee: u64,
    /// T + D_RET (path b) or pool.deadline (path a) for a valid case.
    pub lock_time: u64,
    pub cov: Hash,
    pub funding_sk: [u8; 32],
    pub budget_delta: i32,
}

/// The reference outputs of a return (A5).
#[derive(Clone, Debug)]
pub struct ReturnExpected {
    /// pool' (path a): the pool state unchanged.
    pub pool: Option<PoolState>,
    /// Every output value in order (change last, if present).
    pub values: Vec<u64>,
    pub payout_output: usize,
    pub change_output: Option<usize>,
}

impl ReturnCase {
    pub const TICKET: usize = 0;
    pub const POOL: usize = 1;

    /// Path b on the standalone ticket of `ClaimCase::new(1)` at lock_time = T + D_RET.
    pub fn after() -> Self {
        let c = ClaimCase::new(1);
        Self::after_of(c.ticket, c.ticket_value, c.ticket_daa)
    }

    pub fn after_of(ticket: TicketState, ticket_value: u64, ticket_daa: u64) -> Self {
        let lock_time = (ticket.deadline + D_RET) as u64;
        Self {
            b_max: b_max(),
            ticket,
            ticket_value,
            ticket_daa,
            path: ReturnPath::After,
            funding: vec![FundingIn { value: 10 * KAS, daa: 2_100_000 }],
            change: true,
            fee: KAS / 10,
            lock_time,
            cov: COV,
            funding_sk: FUNDING_SK,
            budget_delta: 0,
        }
    }

    /// Path a on the standalone ticket and pool of `ClaimCase::new(1)` at lock_time =
    /// pool.deadline.
    pub fn with_pool() -> Self {
        let c = ClaimCase::new(1);
        Self::with_pool_of(c.ticket, c.ticket_value, c.ticket_daa, c.pool, c.pool_value, c.pool_daa)
    }

    pub fn with_pool_of(ticket: TicketState, ticket_value: u64, ticket_daa: u64, pool: PoolState, pool_value: u64, pool_daa: u64) -> Self {
        let lock_time = pool.deadline as u64;
        Self { path: ReturnPath::WithPool { pool, pool_value, pool_daa }, lock_time, ..Self::after_of(ticket, ticket_value, ticket_daa) }
    }

    pub fn funding_input(&self, n: usize) -> usize {
        match self.path {
            ReturnPath::After => 1 + n,
            ReturnPath::WithPool { .. } => 2 + n,
        }
    }

    pub fn expected(&self) -> ReturnExpected {
        let (pool, mut values) = match &self.path {
            ReturnPath::After => (None, vec![self.ticket_value]),
            ReturnPath::WithPool { pool, pool_value, .. } => (Some(pool.clone()), vec![*pool_value, self.ticket_value]),
        };
        let payout_output = values.len() - 1;
        let change_output = self.change.then(|| {
            values.push(change_value(&self.funding, self.fee));
            values.len() - 1
        });
        if !self.change {
            assert_eq!(self.funding.iter().map(|f| f.value).sum::<u64>(), self.fee, "a return without change needs funding = fee");
        }
        ReturnExpected { pool, values, payout_output, change_output }
    }

    pub fn build(&self) -> Built {
        self.build_with(Templates::get(self.b_max))
    }

    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        let x = self.expected();
        let cov = Some(self.cov);
        let ticket_entry = UtxoEntry::new(self.ticket_value, t.ticket_spk(&self.ticket), self.ticket_daa, false, cov);
        let redeem = t.ticket.redeem(&self.ticket.values());
        let bind = Some(CovenantBinding::new(0, self.cov));
        let payout = payout_spk(self.ticket.payout_class, &self.ticket.payout_key);
        let mut outputs = vec![];
        let mut inputs = match &self.path {
            ReturnPath::After => {
                vec![In { entry: ticket_entry, kind: InKind::Entry { template: t.ticket.clone(), entry: SIG_RETURN_AFTER.into(), args: vec![], redeem } }]
            }
            ReturnPath::WithPool { pool, pool_value, pool_daa } => {
                outputs.push(TransactionOutput::with_covenant(*pool_value, t.pool_spk(pool), bind));
                vec![
                    In {
                        entry: ticket_entry,
                        kind: InKind::Entry {
                            template: t.ticket.clone(),
                            entry: SIG_RETURN_WITH_POOL.into(),
                            args: vec![ArtifactValue::Int(t.pool.prefix.len() as i64), ArtifactValue::Int(t.pool.suffix.len() as i64)],
                            redeem,
                        },
                    },
                    In {
                        entry: UtxoEntry::new(*pool_value, t.pool_spk(pool), *pool_daa, false, cov),
                        kind: InKind::Entry { template: t.pool.clone(), entry: SIG_TICKET_LEG.into(), args: ticket_leg_args(&t), redeem: t.pool_redeem(pool) },
                    },
                ]
            }
        };
        inputs.extend(funding_inputs(&self.funding, &self.funding_sk));
        outputs.push(TransactionOutput::new(x.values[x.payout_output], payout));
        if let Some(i) = x.change_output {
            outputs.push(TransactionOutput::new(x.values[i], spk_of(&self.funding_sk)));
        }
        built(t, inputs, outputs, self.lock_time, self.budget_delta, self.funding_sk)
    }
}

// ------------------------------------------------------------------------ refresh()

/// The object `refresh()` re-creates (A6, D59). Spec M7 D99: the marker has no `refresh()`
/// (a refreshed marker would have a new creation DAA score, so a new anchor: a re-roll).
#[derive(Clone, Debug)]
pub enum RefreshObj {
    Pool(PoolState),
    Ticket(TicketState),
    /// Spec M5 A7 (D76): the share position, the vault request and the settlement record.
    Position(PositionState),
    Request(RequestState),
    Record(RecordState),
}

impl RefreshObj {
    pub fn template(&self, t: &Templates) -> Arc<Template> {
        match self {
            RefreshObj::Pool(_) => t.pool.clone(),
            RefreshObj::Ticket(_) => t.ticket.clone(),
            RefreshObj::Position(_) => t.position.clone(),
            RefreshObj::Request(_) => t.request.clone(),
            RefreshObj::Record(_) => t.record.clone(),
        }
    }

    pub fn redeem(&self, t: &Templates) -> Vec<u8> {
        match self {
            RefreshObj::Pool(s) => t.pool_redeem(s),
            RefreshObj::Ticket(s) => t.ticket.redeem(&s.values()),
            RefreshObj::Position(s) => t.position_redeem(s),
            RefreshObj::Request(s) => t.request_redeem(s),
            RefreshObj::Record(s) => t.record_redeem(s),
        }
    }

    pub fn spk(&self, t: &Templates) -> ScriptPublicKey {
        harness::pay_to_script_hash_script(&self.redeem(t))
    }

    /// The object's name (`pool`, `ticket`, `position`, `request`, `record`).
    pub fn name(&self) -> &'static str {
        match self {
            RefreshObj::Pool(_) => "pool",
            RefreshObj::Ticket(_) => "ticket",
            RefreshObj::Position(_) => "position",
            RefreshObj::Request(_) => "request",
            RefreshObj::Record(_) => "record",
        }
    }
}

#[derive(Clone)]
pub struct RefreshCase {
    pub b_max: usize,
    pub obj: RefreshObj,
    pub value: u64,
    /// Creation DAA score of the object's UTXO (OpTxInputDaaScore(0)).
    pub daa: u64,
    pub funding: Vec<FundingIn>,
    pub change: bool,
    pub fee: u64,
    /// creation + D_IDLE for a valid case.
    pub lock_time: u64,
    pub cov: Hash,
    pub funding_sk: [u8; 32],
    pub budget_delta: i32,
}

impl RefreshCase {
    /// A valid refresh of `obj` (value `value`, created at `daa`) at lock_time = daa + D_IDLE,
    /// with one funding input of 10 KAS and change.
    pub fn new(obj: RefreshObj, value: u64, daa: u64) -> Self {
        Self {
            b_max: b_max(),
            obj,
            value,
            daa,
            funding: vec![FundingIn { value: 10 * KAS, daa: daa + 1_000 }],
            change: true,
            fee: KAS / 10,
            lock_time: daa + D_IDLE as u64,
            cov: COV,
            funding_sk: FUNDING_SK,
            budget_delta: 0,
        }
    }

    /// Output values: the object (same value), then change.
    pub fn values(&self) -> Vec<u64> {
        let mut v = vec![self.value];
        if self.change {
            v.push(change_value(&self.funding, self.fee));
        } else {
            assert_eq!(self.funding.iter().map(|f| f.value).sum::<u64>(), self.fee, "a refresh without change needs funding = fee");
        }
        v
    }

    pub fn build(&self) -> Built {
        self.build_with(Templates::get(self.b_max))
    }

    pub fn build_with(&self, t: Arc<Templates>) -> Built {
        let values = self.values();
        let spk = self.obj.spk(&t);
        let mut inputs = vec![In {
            entry: UtxoEntry::new(self.value, spk.clone(), self.daa, false, Some(self.cov)),
            kind: InKind::Entry { template: self.obj.template(&t), entry: SIG_REFRESH.into(), args: vec![], redeem: self.obj.redeem(&t) },
        }];
        inputs.extend(funding_inputs(&self.funding, &self.funding_sk));
        let mut outputs = vec![TransactionOutput::with_covenant(values[0], spk, Some(CovenantBinding::new(0, self.cov)))];
        if self.change {
            outputs.push(TransactionOutput::new(values[1], spk_of(&self.funding_sk)));
        }
        built(t, inputs, outputs, self.lock_time, self.budget_delta, self.funding_sk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buy::{dispatch_tag, entry_sigscript, planned_sig_script};
    use silverscript_abi::encode_contract_entry_sig_script;

    /// Stage 1 open item (D54, M4 Stage 3a): the compiled ticket binds POOL_TPL. Recompiling
    /// the committed ticket with the committed pool hash reproduces `t.ticket.hash`; with any
    /// other pool hash (one bit flipped) the ticket hash changes. (SilverScript drops an
    /// unused constructor constant, so this fails if the ticket code stops reading it.)
    #[test]
    fn ticket_hash_depends_on_pool_template() {
        let t = Templates::get(b_max());
        let ticket = ClaimCase::new(1).ticket;
        let compile = |h: [u8; 32]| Template::from(harness::compile_source(&crate::buy::ticket_source(b_max()), &ticket.ctor(h))).hash;
        assert_eq!(compile(t.pool.hash), t.ticket.hash, "recompiling reproduces the committed ticket hash");
        for byte in [0usize, 31] {
            let mut other = t.pool.hash;
            other[byte] ^= 1;
            assert_ne!(compile(other), t.ticket.hash, "a different POOL_TPL must change the ticket hash");
        }
    }

    /// S2-1: the hand encoding of a planned entry equals the ABI's for compiled entries
    /// (ints: the marker's draw_leg; bytes, bool, byte and fixed bytes: the shard's buy).
    #[test]
    fn planned_encoding_matches_the_abi() {
        let t = Templates::get(b_max());
        let ints = vec![ArtifactValue::Int(1234), ArtifactValue::Int(0)];
        let abi = encode_contract_entry_sig_script(&t.marker.art, t.marker.name(), "draw_leg", &ints).unwrap();
        assert_eq!(planned_sig_script("draw_leg(int,int)", &ints), abi);
        let buy = crate::buy::BuyCase::new(b_max());
        let args = buy.args(&t);
        let abi = encode_contract_entry_sig_script(&t.shard.art, t.shard.name(), "buy", &args).unwrap();
        let sig = "buy(int,byte[],byte[],byte[],bool,byte,byte[33],byte[],byte[])";
        assert_eq!(planned_sig_script(sig, &args), abi);
        // A signature that names an existing entry goes through the ABI with its tag checked.
        let redeem = vec![0x51];
        assert_eq!(entry_sigscript(&t.marker, "draw_leg(int,int)", &ints, &redeem), entry_sigscript(&t.marker, "draw_leg", &ints, &redeem));
        // The planned tags are distinct.
        let tags: std::collections::HashSet<[u8; 4]> =
            [SIG_CLAIM, SIG_RETURN_AFTER, SIG_RETURN_WITH_POOL, SIG_REFRESH, SIG_TICKET_LEG, "locked()", "absorb_leg(int,int)", "claim()"]
                .iter()
                .map(|s| dispatch_tag(s))
                .collect();
        assert_eq!(tags.len(), 8);
    }

    #[test]
    #[should_panic(expected = "signature differs")]
    fn drifted_signature_panics() {
        let t = Templates::get(b_max());
        entry_sigscript(&t.marker, "draw_leg(int)", &[ArtifactValue::Int(1)], &[0x51]);
    }

    #[test]
    fn combo_helpers_hit_every_tier() {
        let win = [3, 7, 12, 19, 28, 4];
        for m in [2, 4, 10, 50] {
            let mut w = win;
            w[5] = if m >= 4 { 4 } else { 1 };
            for tier in 0..=6u8 {
                for v in 0..5 {
                    let c = combo_for_tier(&w, tier, m, v);
                    assert!(c.is_canonical(m as u8), "{c:?}");
                    assert_eq!(tier_of(&c.bytes(), &w), Some(tier), "m {m} tier {tier} variant {v}");
                }
            }
            assert_eq!(tier_of(&combo_matching(&w, 0, false, m, 1).bytes(), &w), None);
        }
    }

    /// S4 simnet SN2 model bug: on an h = 2 jackpot of J = 100 KAS + 1 sompi, both paying
    /// claims leave the pool at R + 1 sompi. A third claim (the first ticket's index 0
    /// again) must be refused by the model, naming check 4 (claimed bit) and check 8
    /// (prize > remaining), and pool' must never wrap below zero into a huge u64.
    #[test]
    fn sn2_third_claim_on_a_one_sompi_remainder_is_refused_without_wrap() {
        let j = 100 * KAS as i64 + 1;
        let mut c = ClaimCase::fabricated(2, 2, crate::state::TIER_JACKPOT, j, &[1, 1]);
        c.pool_value = j as u64 + TEST_RESERVE;
        // Claim 1 (PAY, index 0 of the b = 2 ticket).
        let x1 = c.checked_expected().expect("claim 1 valid");
        assert_eq!((x1.h, x1.prize, x1.mode), (2, j / 2, ClaimMode::Pay));
        // Claim 2 (the other holder) takes the same floor(J / 2): pool' = R + 1 sompi.
        let pool_after_two = x1.values[0] - (j / 2) as u64;
        assert_eq!(pool_after_two, TEST_RESERVE + 1);
        // Claim 3: ticket' (index 0 claimed) against the 1-sompi pool.
        let mut c3 = c.clone();
        c3.ticket = x1.ticket.clone().unwrap();
        c3.pool_value = pool_after_two;
        let x3 = c3.expected();
        assert!(x3.rejects.iter().any(|r| r.starts_with("check 4")), "{:?}", x3.rejects);
        assert!(x3.rejects.iter().any(|r| r.starts_with("check 8")), "{:?}", x3.rejects);
        assert!(x3.values.iter().all(|&v| v <= crate::game::TOTAL_MAX as u64), "a wrapped output value: {:?}", x3.values);
        let e = c3.checked_expected().expect_err("the model must refuse the third claim");
        assert!(e.contains("check 4") && e.contains("check 8"), "{e}");
        // The same pool with a fresh (unclaimed) index: only the insufficient pool refuses.
        let mut c4 = c3.clone();
        c4.ticket = c.ticket.clone();
        let e = c4.checked_expected().expect_err("insufficient pool");
        assert!(e.contains("check 8") && !e.contains("check 4"), "{e}");
        assert!(c4.expected().values.iter().all(|&v| v <= crate::game::TOTAL_MAX as u64));
        // The script refuses the well-formed (unwrapped) third claim in the local engine
        // (ticket.sil check 4, line 158; check 8, line 259), not only consensus.
        assert!(c3.build().finish().run().is_err(), "the engine must reject the third claim");
        assert!(c4.build().finish().run().is_err(), "the engine must reject a claim above the remaining pool");
        // The claimed bit alone (pool still covering the prize) is refused too (check 4).
        let mut c5 = c3.clone();
        c5.pool_value = x1.values[0];
        let e = c5.checked_expected().expect_err("claimed bit");
        assert!(e.contains("check 4") && !e.contains("check 8"), "{e}");
        assert!(c5.build().finish().run().is_err(), "the engine must reject a repeat claim at check 4");
    }

    #[test]
    fn standalone_cases_are_valid_on_the_rust_side() {
        let c = ClaimCase::new(4);
        let x = c.expected();
        assert!(x.rejects.is_empty(), "{:?}", x.rejects);
        assert_eq!((x.tier, x.h, x.prize, x.mode), (Some(TIER_LOWER0), 1, 20_000_000, ClaimMode::Pay));
        assert_eq!(x.values.len(), 4);
        let built = c.build();
        assert_eq!(built.inputs.len(), 3);
        assert_eq!(built.outputs.len(), 4);
        let r = ReturnCase::after().build();
        assert_eq!((r.inputs.len(), r.outputs.len()), (2, 2));
        let r = ReturnCase::with_pool().build();
        assert_eq!((r.inputs.len(), r.outputs.len()), (3, 3));
    }
}
