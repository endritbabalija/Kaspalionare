//! CP8: the game-size grid (spec M2 A7, D25 revised). For each grid point the script
//! accepts the reference m(r+1) and rejects m - 1 and m + 1 (A4 check 19, input 0).
//!
//! Grid: V_rec in {0, 10^4, 10^6, 10^7, 10^8 KAS, >= SAT}, boost pot in {0, 10^5, 10^7 KAS},
//! price in {0.1, 1, 10 KAS}, plus explicit points where m_min passes, where no m passes
//! (m' = m_max) and the boost-dominated point (e = 5,000, g = 1,000, boost 539,000 KAS:
//! m' = 12). One shard has 1,000 x price of sales, so H and b_r enter the estimate.

mod common;

use common::accept;
use common::m2::*;
use harness::ArtifactValue;
use lottery::buy::{Built, KAS};
use lottery::close::CloseCase;
use lottery::game::SAT;

const K: usize = 4;

fn grid_case(v_rec: i64, boost: u64, price: i64) -> CloseCase {
    let mut c = CloseCase::new(K);
    c.controller.v_rec = v_rec;
    c.controller.j_boost = 0;
    c.pot = boost;
    c.controller.price = price;
    for s in &mut c.shards {
        s.state.price = price;
    }
    c.set_sales(1, 1_000 * price, 0);
    c
}

/// m_next argument (index 0 of close()) and every output state that carries it.
fn with_m(c: &CloseCase, m: i64) -> Built {
    let mut bad = c.clone();
    bad.m_next = Some(m);
    let b = bad.build();
    assert_eq!(b.inputs.len(), c.build().inputs.len());
    b
}

#[derive(Default, Debug)]
struct Coverage {
    m_min_passes: u32,
    none_passes: u32,
    interior: u32,
    bounded_above: u32,
}

/// Reference part of a grid point: m' equals brute force; records which D25 branch it hits.
fn reference(c: &CloseCase, cov: &mut Coverage, label: &str) -> i64 {
    let x = c.expected();
    let rule = x.math.rule;
    let m = x.m_next;
    assert_eq!(m, rule.choose_brute(), "{label}: reference = brute force");
    if rule.pass(m) {
        if m == rule.m_min { cov.m_min_passes += 1 } else { cov.interior += 1 }
        if !rule.pass(rule.m_max) {
            cov.bounded_above += 1;
        }
    } else {
        assert_eq!(m, rule.m_max);
        cov.none_passes += 1;
    }
    println!("| {label} | {} | {} | {} | {} | {m} |", rule.est.j_house, rule.est.j_boost, rule.a, rule.q);
    m
}

/// Script part: accepts the reference; rejects m - 1 and m + 1 at input 0 (each a full
/// one-field change: the argument and the controller' and new-shard states that carry m).
fn run_point(c: &CloseCase) {
    let m = c.expected().m_next;
    let good = c.build();
    accept(&good);
    for d in [-1i64, 1] {
        attack_pair(&good, &with_m(c, m + d), CloseCase::CONTROLLER);
    }
}

#[test]
fn cp8_game_size_grid() {
    let v_recs = [0, 10_000 * KAS as i64, 1_000_000 * KAS as i64, 10_000_000 * KAS as i64, 100_000_000 * KAS as i64, SAT + 1];
    let boosts = [0u64, 100_000 * KAS, 10_000_000 * KAS];
    let prices = [KAS as i64 / 10, KAS as i64, 10 * KAS as i64];
    let mut cov = Coverage::default();
    let mut points = vec![];
    println!("
CP8 grid
| point | Ĵ_house | Ĵ_boost | a | q | m' |
|---|---|---|---|---|---|");
    for &v in &v_recs {
        for &o in &boosts {
            for &p in &prices {
                let c = grid_case(v, o, p);
                reference(&c, &mut cov, &format!("V_rec {v}, O {o}, p {p}"));
                points.push(c);
            }
        }
    }

    // m_min passes: no house estimate and no boost.
    let c = grid_case(0, 0, KAS as i64);
    assert_eq!(reference(&c, &mut cov, "m_min passes"), c.controller.m_min);
    points.push(c);

    // None passes: a saturated estimate at the lowest price -> m_max.
    let c = grid_case(SAT + 1, 10_000_000 * KAS, KAS as i64 / 10);
    assert!(!c.expected().math.rule.pass(c.controller.m_max));
    assert_eq!(reference(&c, &mut cov, "none passes"), c.controller.m_max);
    points.push(c);

    // Boost-dominated (CA4): edge slope negative, interval bounded above, m' = 23 at the
    // fixture margin 1.5 (spec M8 A0; 12 at the former 1.17).
    let mut c = grid_case(0, 539_000 * KAS, KAS as i64);
    c.set_sales(1, 0, 0);
    c.controller.e = 5_000;
    c.controller.g = 1_000;
    assert!(c.expected().math.rule.checks()[0].1 < 0);
    assert_eq!(reference(&c, &mut cov, "boost-dominated"), 23);
    points.push(c);

    // Edge-bound point (M8 S1 review): at margin 1.5 and e = 15 % the whale check binds at
    // every grid point (q < a), so e = 4,000 (a = 0.444 p) puts the positive-slope edge
    // branch back in charge: m' - 1 passes the whale check and fails only the edge rule.
    let mut c = grid_case(10_000_000 * KAS as i64, 0, KAS as i64);
    c.controller.e = 4_000;
    let m = reference(&c, &mut cov, "edge-bound (e 4,000)");
    let rule = c.expected().math.rule;
    assert!(rule.edge(m) && rule.whale(m - 1) && !rule.edge(m - 1), "edge rule sets m' = {m}");
    points.push(c);

    println!("coverage: {cov:?}");
    assert!(cov.m_min_passes >= 1 && cov.none_passes >= 1 && cov.interior >= 1 && cov.bounded_above >= 1, "{cov:?}");

    for c in &points {
        run_point(c);
    }
}

/// The m_next argument alone (output states left at the reference m) is also rejected: the
/// argument is bound to the output states (check 21/23) and to the closed form (check 19).
#[test]
fn cp8_argument_alone_changed() {
    let c = grid_case(10_000_000 * KAS as i64, 100_000 * KAS, KAS as i64);
    let m = c.expected().m_next;
    let good = c.build();
    for d in [-1i64, 1] {
        attack_at(&good, CloseCase::CONTROLLER, |b| b.entry_args_mut(0)[0] = ArtifactValue::Int(m + d));
    }
}
