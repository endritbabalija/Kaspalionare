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

use kaspa_rpc_core::api::rpc::RpcApi;
use lottery::buy::Final;

/// The rate the campaign pays: whole sompi per gram, at least the floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rate {
    /// The estimator's normal-bucket rate as returned (sompi/gram).
    pub raw: f64,
    /// `raw` rounded up, then raised to the floor (sompi/gram).
    pub sompi_per_gram: u64,
}

/// Round an estimator rate up to whole sompi per gram and apply the 100 sompi/gram floor.
/// A rate that is not a finite non-negative number, or does not fit in u64, is an error
/// (never a silent floor: a broken estimator is worth a stop).
pub fn round_rate(raw: f64) -> Result<Rate, String> {
    if !raw.is_finite() || raw < 0.0 {
        return Err(format!("fee estimate {raw} is not a finite non-negative rate"));
    }
    let up = raw.ceil();
    if up > u64::MAX as f64 / 2.0 {
        return Err(format!("fee estimate {raw} sompi/gram is out of range"));
    }
    Ok(Rate { raw, sompi_per_gram: (up as u64).max(harness::MIN_FEERATE_SOMPI_PER_GRAM) })
}

/// The node's normal-bucket rate (spec A5 C0: rate >= 100 sompi/gram after the floor).
pub async fn estimate<C: RpcApi + ?Sized>(c: &C) -> Result<Rate, String> {
    let e = c.get_fee_estimate().await.map_err(|e| format!("get_fee_estimate: {e}"))?;
    let b = e.normal_buckets.first().ok_or("get_fee_estimate: no normal bucket (the API guarantees one)")?;
    round_rate(b.feerate)
}

/// The fee of a transaction of these masses at `rate` (D120): `fee_for(priority, rate)`.
pub fn fee_of(m: &harness::Masses, rate: u64) -> Result<u64, String> {
    lottery::buy::fee_for(m.priority, rate)
}

/// The fee a built transaction needs at `rate`, from its own masses.
pub fn fee_needed(f: &Final, rate: u64) -> Result<u64, String> {
    fee_of(&harness::masses(&f.tx, &f.entries), rate)
}

/// Build a transaction whose fee covers its own exact mass at `rate`. `mk(fee)` builds it
/// with that fee (the change absorbs it). The mass depends on the fee only through the
/// change output's storage mass, so this settles in a few rounds; the fee returned is
/// never below what the final transaction needs.
pub fn with_fee(rate: u64, mut mk: impl FnMut(u64) -> Result<Final, String>) -> Result<Final, String> {
    let mut fee = lottery::buy::fee_for(1_000, rate)?;
    for _ in 0..6 {
        let f = mk(fee)?;
        let need = fee_needed(&f, rate)?;
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
        assert_eq!(round_rate(1e6).unwrap().sompi_per_gram, 1_000_000);
        for bad in [f64::NAN, f64::INFINITY, -1.0, -0.5, 1e30] {
            assert!(round_rate(bad).is_err(), "{bad} accepted");
        }
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
