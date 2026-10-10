//! Shared helpers for the M1 tests (spec M1 section 8) and, in `m2`, the M2 tests.
#![allow(dead_code)]

pub mod m2;
pub mod m3;
pub mod m4;

use harness::TxScriptError;
use lottery::buy::{BuyCase, Built, Final, b_max};
use lottery::combo::Combo;

/// The spec's base case: a valid one-combination buy (see `BuyCase::new`).
pub fn case() -> BuyCase {
    BuyCase::new(b_max())
}

/// `n` distinct canonical combinations.
pub fn distinct(n: usize, m: u8) -> Vec<Combo> {
    (0..n as u32).map(|i| Combo::new(lottery::combo::unrank(7 + i * 13_001), 1 + (i % m as u32) as u8)).collect()
}

/// Every input must pass under its committed budget.
pub fn accept(b: &Built) -> Final {
    let f = b.finish();
    if let Err((i, e)) = f.run() {
        panic!("valid transaction rejected at input {i}: {e:?}");
    }
    f
}

/// The shard (input 0) must reject; returns the error.
pub fn shard_error(b: &Built) -> TxScriptError {
    let f = b.finish();
    match harness::execute(&f.tx, &f.entries, 0, true) {
        Ok(_) => panic!("attack accepted by the shard"),
        Err(e) => e,
    }
}

/// The shard must reject with a failed check (`require` -> VerifyError), not a budget,
/// encoding or index error.
pub fn assert_check_fails(b: &Built) {
    let e = shard_error(b);
    assert!(matches!(e, TxScriptError::VerifyError), "expected VerifyError, got {e:?}");
}

/// Build the base case, prove it is accepted, apply one mutation, prove the shard rejects
/// it through a failed check.
pub fn attack(base: &BuyCase, mutate: impl FnOnce(&mut Built)) {
    let built = base.build();
    accept(&built);
    let mut m = built.clone();
    mutate(&mut m);
    assert_check_fails(&m);
}

/// As `attack`, for an attack that is itself a different case (no shared base transaction):
/// `base` must be accepted, `bad` rejected.
pub fn attack_case(base: &BuyCase, bad: &BuyCase) {
    accept(&base.build());
    assert_check_fails(&bad.build());
}
