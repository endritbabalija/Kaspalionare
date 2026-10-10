//! Shared helpers for the M1 tests (spec M1 section 8).
#![allow(dead_code)]

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

pub const SHARES_F: u64 = 500;
pub const SHARES_C: u64 = 50;
pub const SHARES_D: u64 = 50;
pub const SHARES_T: [u64; 3] = [200, 300, 500];

/// Round amounts derived from S and P_R (design 5.1) with the spec's test shares.
/// Returns (F, C_close, C_draw, sum B_t, H, b_r).
pub fn round_amounts(sales: u64, ref_paid: u64, s_r: u64) -> (u64, u64, u64, u64, u64, i128) {
    let s_h = 10_000 - SHARES_F - s_r - SHARES_C - SHARES_D - SHARES_T.iter().sum::<u64>();
    let f = sales * SHARES_F / 10_000;
    let cc = sales * SHARES_C / 10_000;
    let cd = sales * SHARES_D / 10_000;
    let bt: u64 = SHARES_T.iter().map(|s| sales * s / 10_000).sum();
    let h = sales * s_h / 10_000;
    let br = sales as i128 - ref_paid as i128 - (f + cc + cd + bt + h) as i128;
    (f, cc, cd, bt, h, br)
}
