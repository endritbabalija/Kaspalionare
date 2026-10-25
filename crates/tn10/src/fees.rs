//! Spec M8 D120: transaction fees from the node's fee estimator.
//!
//! Sources (rusty-kaspa v2.1.0, `refs/rusty-kaspa-v2.1.0`):
//! - `RpcApi::get_fee_estimate()` returns `RpcFeeEstimate { priority_bucket, normal_buckets,
//!   low_buckets }`; each bucket is `{ feerate: f64 (sompi/gram), estimated_seconds }`, and
//!   `normal_buckets[0]` is guaranteed to exist and targets sub-minute inclusion
//!   (`rpc/core/src/model/feerate_estimate.rs:7-35`; served from a cache over the mining
//!   manager's real-time estimate, `rpc/service/src/service.rs:958-967`).
//! - The feerate the mempool ranks by is fee / the normalized max of compute, transient and
//!   storage mass (`consensus/core/src/tx.rs:662-668`), i.e. `harness::Masses::priority`.
//! - The relay floor is fee >= max(compute, normalized transient) x 100,000 / 1,000
//!   (`mining/src/mempool/check_transaction_standard.rs:67-104`, `mining/src/mempool/config.rs:20,
//!   138-141`); it ignores storage, so a fee over the priority mass at >= 100 sompi/gram
//!   always clears it.
//!
//! The campaign therefore pays `lottery::buy::fee_for(priority mass, ceil(normal rate))`:
//! the estimator's fractional rate rounded up to whole sompi per gram, raised to the 100
//! sompi/gram floor by `fee_for` (design section 9), times the builder's exact mass.
//!
//! Caps (M8 S5, D115: the node is not trusted off-chain; a wrong estimate must stop the
//! runner, never drain the change into fees):
//! - `MAX_RATE_SOMPI_PER_GRAM` = 1,000 sompi/gram, 10 x the floor. The floor is the
//!   mempool's DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE = 100,000 sompi per kg = 100 sompi per
//!   gram (`mining/src/mempool/config.rs:20`, applied at `:138-141`;
//!   `harness::MIN_FEERATE_SOMPI_PER_GRAM`). The M8 campaign's C0 recorded "normal bucket
//!   100 -> 100 sompi/gram" (`docs/tn10-mass-check.md`). A higher normal-bucket rate is a
//!   fee spike worth a human decision or a lying node; `round_rate` refuses it (an error,
//!   not a clamp: a clamped rate would quietly underpay).
//! - `MAX_FEE_SOMPI` = 5 KAS per transaction, whatever the rate and mass: the largest
//!   priority mass a block admits (the compute limit, 500,000 grams, is the reference every
//!   mass is normalized to: `consensus/core/src/config/params.rs:626`,
//!   `consensus/core/src/mass/mod.rs:175-182, 225-228, 299-305`) at the maximum rate. No
//!   transaction a block can hold needs more at an accepted rate, so a fee above it is a
//!   builder fault: a mass above the block limit, or change miscounted so the fee (inputs -
//!   outputs) swallows it. `fee_of` and `with_fee` refuse it. For scale: the heaviest
//!   campaign transaction, close() with K = 16 (priority mass 432,306 grams = transient
//!   864,612 x 0.5), pays 0.43 KAS at the floor, 4.32 KAS at the maximum rate.

use kaspa_rpc_core::api::rpc::RpcApi;
use lottery::buy::Final;

/// The highest node rate the campaign accepts (sompi/gram; module doc).
pub const MAX_RATE_SOMPI_PER_GRAM: u64 = 1_000;
/// The most any one transaction may pay in fees (sompi; module doc).
pub const MAX_FEE_SOMPI: u64 = 500_000 * MAX_RATE_SOMPI_PER_GRAM;

/// The rate the campaign pays: whole sompi per gram, at least the floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rate {
    /// The estimator's normal-bucket rate as returned (sompi/gram).
    pub raw: f64,
    /// `raw` rounded up, then raised to the floor (sompi/gram).
    pub sompi_per_gram: u64,
}

/// Round an estimator rate up to whole sompi per gram and apply the 100 sompi/gram floor.
/// A rate that is not a finite non-negative number, or is above `MAX_RATE_SOMPI_PER_GRAM`
/// after rounding up, is an error (never a silent floor or clamp: a broken or hostile
/// estimator is worth a stop).
pub fn round_rate(raw: f64) -> Result<Rate, String> {
    if !raw.is_finite() || raw < 0.0 {
        return Err(format!("fee estimate {raw} is not a finite non-negative rate"));
    }
    let up = raw.ceil();
    if up > MAX_RATE_SOMPI_PER_GRAM as f64 {
        return Err(format!("fee estimate {raw} sompi/gram is above the maximum of {MAX_RATE_SOMPI_PER_GRAM} sompi/gram (10 x the floor; D115: refused, not clamped)"));
    }
    Ok(Rate { raw, sompi_per_gram: (up as u64).max(harness::MIN_FEERATE_SOMPI_PER_GRAM) })
}

/// A fee within the per-transaction cap.
fn capped(fee: u64, what: &str) -> Result<u64, String> {
    if fee > MAX_FEE_SOMPI {
        return Err(format!("{what} {fee} sompi is above the per-transaction fee cap of {MAX_FEE_SOMPI} sompi (5 KAS)"));
    }
    Ok(fee)
}

/// The node's normal-bucket rate (spec A5 C0: rate >= 100 sompi/gram after the floor).
pub async fn estimate<C: RpcApi + ?Sized>(c: &C) -> Result<Rate, String> {
    let e = c.get_fee_estimate().await.map_err(|e| format!("get_fee_estimate: {e}"))?;
    let b = e.normal_buckets.first().ok_or("get_fee_estimate: no normal bucket (the API guarantees one)")?;
    round_rate(b.feerate)
}

/// The fee of a transaction of these masses at `rate` (D120): `fee_for(priority, rate)`.
/// Above `MAX_FEE_SOMPI` it is an error.
pub fn fee_of(m: &harness::Masses, rate: u64) -> Result<u64, String> {
    capped(lottery::buy::fee_for(m.priority, rate)?, &format!("fee for priority mass {} at {rate} sompi/gram:", m.priority))
}

/// The fee a built transaction needs at `rate`, from its own masses.
pub fn fee_needed(f: &Final, rate: u64) -> Result<u64, String> {
    fee_of(&harness::masses(&f.tx, &f.entries), rate)
}

/// Build a transaction whose fee covers its own exact mass at `rate`. `mk(fee)` builds it
/// with that fee (the change absorbs it). The mass depends on the fee only through the
/// change output's storage mass, so this settles in a few rounds; the fee returned is
/// never below what the final transaction needs. A rate above `MAX_RATE_SOMPI_PER_GRAM`, a
/// needed fee above `MAX_FEE_SOMPI`, or a built transaction that pays more than the cap is
/// an error.
pub fn with_fee(rate: u64, mut mk: impl FnMut(u64) -> Result<Final, String>) -> Result<Final, String> {
    if rate > MAX_RATE_SOMPI_PER_GRAM {
        return Err(format!("fee rate {rate} sompi/gram is above the maximum of {MAX_RATE_SOMPI_PER_GRAM} sompi/gram"));
    }
    let mut fee = lottery::buy::fee_for(1_000, rate)?;
    for _ in 0..6 {
        let f = mk(fee)?;
        let need = fee_needed(&f, rate)?;
        capped(f.fee(), &format!("transaction {} pays a fee of", f.tx.id()))?;
        if f.fee() >= need {
            return Ok(f);
        }
        fee = need.max(fee + 1);
    }
    Err(format!("fee did not settle at {rate} sompi/gram after 6 rebuilds"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_rounds_up_and_floors() {
        assert_eq!(round_rate(0.0).unwrap().sompi_per_gram, 100);
        assert_eq!(round_rate(1.0).unwrap().sompi_per_gram, 100);
        assert_eq!(round_rate(99.01).unwrap().sompi_per_gram, 100);
        assert_eq!(round_rate(100.0).unwrap().sompi_per_gram, 100);
        assert_eq!(round_rate(100.0001).unwrap().sompi_per_gram, 101);
        assert_eq!(round_rate(250.5).unwrap().sompi_per_gram, 251);
        // M8 S5: 1e6 sompi/gram (10,000 x the floor) was accepted before; it is now above
        // MAX_RATE_SOMPI_PER_GRAM and refused (see `rate_above_the_max_is_an_error`).
        for bad in [f64::NAN, f64::INFINITY, -1.0, -0.5, 1e30, 1e6] {
            assert!(round_rate(bad).is_err(), "{bad} accepted");
        }
    }

    /// M8 S5 (D115: the node is not trusted): a node rate above the maximum is an error, not
    /// a clamp; the maximum itself is accepted.
    #[test]
    fn rate_above_the_max_is_an_error() {
        assert_eq!(MAX_RATE_SOMPI_PER_GRAM, 1_000);
        assert_eq!(round_rate(1_000.0).unwrap().sompi_per_gram, 1_000);
        assert_eq!(round_rate(999.5).unwrap().sompi_per_gram, 1_000);
        for bad in [1_000.0001, 1_001.0, 5_000.0, 1e9] {
            let e = round_rate(bad).unwrap_err();
            assert!(e.contains("above the maximum"), "{bad}: {e}");
        }
        // with_fee refuses a rate above the maximum before building anything.
        let mut built = 0;
        let Err(e) = with_fee(MAX_RATE_SOMPI_PER_GRAM + 1, |_| {
            built += 1;
            Err("must not be called".into())
        }) else {
            panic!("a rate above the maximum was accepted")
        };
        assert!(e.contains("above the maximum"), "{e}");
        assert_eq!(built, 0);
    }

    /// M8 S5: no transaction pays more than MAX_FEE_SOMPI, whatever the rate and mass.
    #[test]
    fn fee_above_the_per_transaction_cap_is_an_error() {
        assert_eq!(MAX_FEE_SOMPI, 5 * lottery::buy::KAS);
        // The largest priority mass a block admits (compute limit 500,000 grams) at the
        // maximum rate is exactly the cap; one gram more is refused.
        let big = harness::Masses { compute: 500_000, transient: 1_000_000, storage: 500_000, priority: 500_000 };
        assert_eq!(fee_of(&big, MAX_RATE_SOMPI_PER_GRAM).unwrap(), MAX_FEE_SOMPI);
        let over = harness::Masses { priority: 500_001, ..big };
        assert!(fee_of(&over, MAX_RATE_SOMPI_PER_GRAM).unwrap_err().contains("per-transaction fee cap"));
        assert!(fee_of(&harness::Masses { priority: 5_000_001, ..big }, 100).unwrap_err().contains("per-transaction fee cap"));
        // A builder that pays more than the cap from change (the fee is inputs - outputs) is
        // refused even when the rate and mass are fine.
        let c = lottery::buy::BuyCase::new(lottery::buy::b_max());
        let Err(e) = with_fee(100, |_| {
            let mut c = c.clone();
            c.fee = MAX_FEE_SOMPI + 1;
            Ok(c.build().finish())
        }) else {
            panic!("a fee above the per-transaction cap was accepted")
        };
        assert!(e.contains("per-transaction fee cap"), "{e}");
    }

    #[test]
    fn fee_is_priority_mass_times_rate() {
        let m = harness::Masses { compute: 1_000, transient: 3_000, storage: 40_000, priority: 40_000 };
        assert_eq!(fee_of(&m, 100).unwrap(), 4_000_000);
        assert_eq!(fee_of(&m, 1).unwrap(), 4_000_000, "floor 100");
        assert_eq!(fee_of(&m, 101).unwrap(), 4_040_000);
        // The relay floor (compute / normalized transient only) is always covered.
        assert!(fee_of(&m, 100).unwrap() >= lottery::buy::relay_floor_fee(&m));
        assert!(fee_of(&harness::Masses { priority: u64::MAX, ..m }, 200).is_err(), "overflow is an error");
    }

    /// A real buy built through `with_fee` pays at least its own mass at the rate.
    #[test]
    fn with_fee_settles_on_a_buy() {
        let c = lottery::buy::BuyCase::new(lottery::buy::b_max());
        for rate in [100u64, 137, 1_000] {
            let f = with_fee(rate, |fee| {
                let mut c = c.clone();
                c.fee = fee;
                Ok(c.build().finish())
            })
            .unwrap();
            let need = fee_needed(&f, rate).unwrap();
            assert!(f.fee() >= need, "rate {rate}: fee {} < need {need}", f.fee());
            assert!(f.fee() <= need + lottery::buy::fee_for(10, rate).unwrap(), "rate {rate}: overpaid {} for {need}", f.fee());
        }
    }
}
