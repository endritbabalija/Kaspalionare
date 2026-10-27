//! Front end F6 part B (spec frontend S6, D138): the example round of How it works.
//!
//! The draw-cabinet figure (`docs/figures/draw-cabinet/`) and the How it works page show one
//! invented example round. Its inputs (ticket counts per shard, referrals, the vault, the
//! backers' requests, an example covenant ID and anchor hash, who holds the winning
//! combinations) are made up here; every amount, m, the winning numbers, the pools, the
//! jackpots, the conversion and each settlement are computed by the `lottery` code itself
//! (`close_math`, `seed_numbers`, `win_counts`, `pool_set`, `vault_settle`, `boost_after`,
//! `fix_jackpot`, `convert`, `settle`, the web layer's `referral`) on the instance values of
//! the `CloseCase` fixture (margin 15,000, D113). No second copy of the game arithmetic exists
//! in JavaScript (spec 1).
//!
//! The model is written to `web/src/screens/how/cabinet-model.json`, which the figure build
//! (`node docs/figures/draw-cabinet/build.mjs`) and the page read. `figure_model_is_current`
//! fails when the committed file differs from a fresh generation. After a deliberate change
//! (a game rule or the example), from `crates/`:
//! `cargo test -p lottery --test f6_figure -- --ignored write_figure_model`
//! then rebuild the figure (`node ../docs/figures/draw-cabinet/build.mjs --all`).

use lottery::buy::{KAS, M_REF};
use lottery::close::CloseCase;
use lottery::game::*;
use lottery::params::blake2b;
use lottery::state::{Payout, RecordState, RequestState, KIND_DEPOSIT, KIND_WITHDRAW};
use serde_json::{json, Value};

const MODEL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/src/screens/how/cabinet-model.json");
const K: i64 = KAS as i64;

/// The example's inputs (invented for the figure).
const ROUND: i64 = 7;
const SHARD_COUNT: usize = 16;
/// Tickets of one combination each, per shard.
const COUNTS: [i64; SHARD_COUNT] = [27, 24, 26, 25, 29, 23, 26, 28, 24, 25, 27, 26, 25, 24, 26, 27];
/// Tickets per shard that named a referrer (260 in all).
const REFS: [i64; SHARD_COUNT] = [17, 15, 17, 16, 19, 15, 17, 18, 15, 16, 17, 17, 16, 15, 17, 13];
/// The vault entering the round: assets and shares (genesis mints one share per sompi, D66).
const A0: i64 = 8_000_000 * K;
const S0: i64 = 7_750_000 * K;
/// Backer requests made during the round, in order (deposits in sompi, withdrawals in shares).
const REQUESTS: [(u8, i64); 4] =
    [(KIND_DEPOSIT, 30_000 * K), (KIND_WITHDRAW, 19_000_000_000_000), (KIND_DEPOSIT, 20_000 * K), (KIND_WITHDRAW, 670_000_000_000_000)];
/// Shards holding the jackpot combination, and (shard, another bonus) holding second-prize combinations.
const JACKPOT_HOLDERS: [usize; 2] = [2, 10];
const SECOND_HOLDERS: [usize; 2] = [5, 13];

fn s(v: i64) -> Value {
    Value::String(v.to_string())
}

/// (A + VA) / (S + VS) in sompi per share, 8 decimals, rounded down (display only).
fn price(a: i64, sh: i64) -> String {
    let (n, d) = ((a + VA) as u128 * 100_000_000, (sh + VS) as u128);
    let q = n / d;
    format!("{}.{:08}", q / 100_000_000, q % 100_000_000)
}

fn amounts(a: &RoundAmounts, referral: i64) -> Value {
    json!({ "F": s(a.f), "R": s(referral), "C_close": s(a.c_close), "C_draw": s(a.c_draw), "B": a.b.map(s), "Bsum": s(a.b_sum()), "H": s(a.h), "b": s(a.b_r) })
}

fn model() -> Value {
    let case = CloseCase::new(SHARD_COUNT);
    let shard = &case.shards[0].state;
    let mut c = case.controller.clone();
    c.round = ROUND;
    let sh = Shares::of(&c);
    let (r1, paid) = lottery::web::cases::referral(shard, 1).expect("referral");
    assert!(paid && r1 >= M_REF, "a one-combination ticket's referral is paid");
    let r1 = r1 as i64;

    // J(7), fixed by the draw of round 6 from V'(6) = A0 with an empty boost pot.
    let fix7 = fix_jackpot(&JackpotRule::new(&c, A0), A0, 0, c.j);
    c.v_rec = A0;
    c.jackpot = fix7.jackpot;
    c.j_boost = fix7.j_boost;
    c.g2 = fix7.g2;
    c.liability = fix7.liability;

    // close() of round 7: the controller holds acc + reserve + the boost pot O = J_boost.
    let shards: Vec<(i64, i64)> = (0..SHARD_COUNT).map(|i| (COUNTS[i] * c.price, REFS[i] * r1)).collect();
    let cm = close_math(&c, &case.params, &shards, c.acc + c.reserve + c.j_boost, 0);
    let am = cm.amounts;

    // draw() of round 7: the seed from an example anchor hash, the holders' counts at the winning rank.
    let cov = blake2b(b"Kaspalionare how it works: example covenant");
    let x_hash = blake2b(b"Kaspalionare how it works: example anchor block");
    let seed = seed_numbers(&cov, ROUND, &x_hash, c.m);
    let mut strings = vec![vec![0u16; c.m as usize]; SHARD_COUNT];
    for &i in &JACKPOT_HOLDERS {
        strings[i][seed.bonus as usize - 1] += 1;
    }
    for (n, &i) in SECOND_HOLDERS.iter().enumerate() {
        let other = (seed.bonus as i64 - 1 + 1 + n as i64) % c.m; // a bonus value other than the winning one
        strings[i][other as usize] += 1;
    }
    let (h_j, n2) = win_counts(&strings, seed.bonus, c.m);
    let pools = pool_set(h_j, n2, c.jackpot, c.g2, &am.b, c.m);
    let v_prime = vault_settle(A0, am.h, c.jackpot - c.j_boost, pools.jp, c.g2, n2, pools.sp).expect("V'");
    let o_prime = boost_after(0, am.b_r, c.j_boost, pools.jp, &am.b, &pools.lp, &[]).expect("O'");
    let mut c8 = c.clone();
    c8.round = ROUND + 1;
    c8.m = cm.m_next;
    let fix8 = fix_jackpot(&JackpotRule::new(&c8, v_prime), v_prime, o_prime, c8.j);

    // The backers' conversion at the post-settlement price, then each settle() in order.
    let dep: i64 = REQUESTS.iter().filter(|r| r.0 == KIND_DEPOSIT).map(|r| r.1).sum();
    let wsh: i64 = REQUESTS.iter().filter(|r| r.0 == KIND_WITHDRAW).map(|r| r.1).sum();
    let conv = convert(v_prime, S0, dep, wsh, fix8.liability).expect("conversion");
    let owner = Payout::Schnorr([7; 32]);
    let mut rec: RecordState = conv.record(ROUND, [0; 32], &owner).expect("a record");
    let mut settled = vec![];
    for &(kind, amount) in &REQUESTS {
        let st = settle(&rec, &RequestState::new(ROUND, kind, amount, &owner)).expect("settle");
        let mode = format!("{:?}", st.mode);
        settled.push(if kind == KIND_DEPOSIT {
            json!({ "kind": "deposit", "amount": s(amount), "mode": mode, "shares": s(st.sh) })
        } else {
            json!({ "kind": "withdraw", "amount": s(amount), "mode": mode, "conv": s(st.conv), "pay": s(st.pay), "left": s(st.left) })
        });
        rec = st.record.clone();
        if st.drained {
            assert_eq!(settled.len(), REQUESTS.len(), "the last settlement drains the record");
        }
    }

    // One ticket of one combination with a referral, split by the same rule (design 5.1).
    let one = round_amounts(c.price, r1, &sh);

    json!({
        "about": "The How it works example round. Inputs invented; every amount computed by the lottery code. Written by crates/lottery/tests/f6_figure.rs (write_figure_model); do not edit.",
        "units": "Amounts in sompi (1 KAS = 100,000,000 sompi) and vault shares as decimal strings; shares of the price in basis points",
        "round": ROUND,
        "k": SHARD_COUNT,
        "m": c.m,
        "mNext": cm.m_next,
        "price": s(c.price),
        "deposit": s(c.d),
        "counts": COUNTS,
        "sold": COUNTS.iter().sum::<i64>(),
        "referrals": REFS.iter().sum::<i64>(),
        "sales": s(cm.sales),
        "shares": { "F": sh.s_f, "R": sh.s_r, "C": sh.s_c, "D": sh.s_d, "T": sh.st, "H": sh.s_h() },
        "perTicket": amounts(&one, r1),
        "amounts": amounts(&am, cm.ref_paid),
        "seed": { "covenant": hex(&cov), "anchorHash": hex(&x_hash), "rank": seed.rank },
        "win": seed.main,
        "bonus": seed.bonus,
        "holders": JACKPOT_HOLDERS,
        "h": h_j,
        "n2": n2,
        "prize": s(claim_amount(c.jackpot, h_j).expect("prize")),
        "pools": pools.pools.iter().map(|p| json!({ "tier": p.tier, "slice": s(p.slice), "value": s(p.value) })).collect::<Vec<_>>(),
        "vault": {
            "V": s(A0),
            "J": s(c.jackpot),
            "jBase": s(fix7.x_b),
            "G2": s(c.g2),
            "L": s(c.liability),
            "after": s(v_prime),
            "boostAfter": s(o_prime),
            "Jnext": s(fix8.jackpot),
            "JboostNext": s(fix8.j_boost),
            "G2next": s(fix8.g2),
            "Lnext": s(fix8.liability),
            "final": s(conv.total_assets()),
        },
        "backers": {
            "S0": s(S0),
            "S1": s(conv.total_shares()),
            "price0": price(A0, S0),
            "priceDraw": price(v_prime, S0),
            "price1": price(conv.total_assets(), conv.total_shares()),
            "D": s(dep),
            "W": s(wsh),
            "mint": s(conv.mint_c),
            "refund": conv.refund,
            "slack": s(conv.slack),
            "wc": s(conv.wc),
            "out": s(conv.out_c),
            "pendingAfter": s(wsh - conv.wc),
            "settled": settled,
            "drained": rec.dep_rem == 0 && rec.wsh_rem == 0,
        },
        "constants": {
            "combinations": N,
            "bMax": TICKET_B_MAX,
            "mRef": s(M_REF as i64),
            "pMin": s(P_MIN),
            "minDeposit": s(MIN_DEPOSIT),
            "wMinValue": s(W_MIN_VALUE),
            "seedOffsetDaa": SEED_OFFSET,
            "drawWindowDaa": DRAW_WINDOW,
            "claimWindowDaa": CLAIM_WINDOW,
            "returnAfterDaa": D_RET,
            "vaultRequestDelayDaa": VAULT_REQ_DELAY,
            "marginMin": MARGIN_MIN,
        },
    })
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn text() -> String {
    serde_json::to_string_pretty(&model()).expect("json") + "\n"
}

#[test]
fn figure_model_is_current() {
    let want = std::fs::read_to_string(MODEL).expect("web/src/screens/how/cabinet-model.json (write it with write_figure_model)");
    let got = text();
    assert!(
        want.replace("\r\n", "\n") == got,
        "the committed How it works model differs from the lottery code's: run `cargo test -p lottery --test f6_figure -- --ignored write_figure_model`, then rebuild the figure (node docs/figures/draw-cabinet/build.mjs --all)"
    );
}

#[test]
fn the_example_round_is_a_drawn_round_with_winners() {
    let m = model();
    assert_eq!(m["h"], 2, "two holders of the jackpot combination");
    assert_eq!(m["n2"], 2, "two held second-prize combinations");
    assert_eq!(m["backers"]["refund"], false);
    assert_eq!(m["backers"]["drained"], true, "the last settlement drains the record");
    let w: Vec<u64> = m["win"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert!(w.windows(2).all(|p| p[0] < p[1]) && w[0] >= 1 && w[4] <= 30);
    let bonus = m["bonus"].as_i64().unwrap();
    assert!((1..=m["m"].as_i64().unwrap()).contains(&bonus));
}

#[test]
#[ignore = "writes web/src/screens/how/cabinet-model.json; run after a deliberate change"]
fn write_figure_model() {
    std::fs::create_dir_all(std::path::Path::new(MODEL).parent().unwrap()).unwrap();
    std::fs::write(MODEL, text()).unwrap();
    println!("wrote {MODEL}");
}
