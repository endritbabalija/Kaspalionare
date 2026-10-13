//! M6 positive tests LP1-LP8 (spec M6 A8; the Rust halves of LP4 and LA6 are unit tests in
//! `game` and `genesis`).
//!
//! As M5: full transactions in the v2.1.0 engine with production flags and committed
//! budgets; every request, settlement and conversion is checked output by output against
//! the builders' reference and for conservation (inputs = outputs + the case's fee,
//! `common::m5::check_*`). Fabricated vaults and records run under the instance covenant
//! ID (D79); values above MAX_SOMPI (total_assets = ASSETS_MAX) run in the engine only (M5
//! S4-3).
//!
//! M6 Stage 1 (tests first, scripts untouched): LP1 fails (genesis accepts V0 < V0_MIN and
//! S0 > SHARES_MAX), LP4 fails (the vault has no W4b: partial withdrawals below wmin are
//! accepted), LP5 fails (the drawer's C1 guard still uses MIN_DEPOSIT = 1 KAS and refuses
//! the conversion of a 10 KAS deposit at about 10 KAS per share) and LP8's constant mirror
//! fails (vault.sil, drawer.sil and request.sil still say MIN_DEPOSIT = 10^8, vault.sil has
//! no W_MIN_VALUE). LP2, LP3, LP6 and LP7 pass with the old scripts (they are regression
//! positives).

mod common;

use common::m2::*;
use common::m5::*;
use common::m6::*;
use harness::covenant_path;
use lottery::buy::{FUNDING_SK, Final, KAS, OwnerKey, funding_spk};
use lottery::combo::Combo;
use lottery::draw::{DrawCase, RoundResult};
use lottery::game::{ASSETS_MAX, MIN_DEPOSIT, SHARES_MAX, SettleMode, VA, VS, W_MIN_VALUE, w_min};
use lottery::genesis::{D_FLOOR, GENESIS_FUNDING, GenesisCase, V0_MIN, covenant_id};
use lottery::ledger::Ledger;
use lottery::params::xonly;
use lottery::state::{KIND_DEPOSIT, KIND_WITHDRAW, Payout, PositionState, RequestState, TIER_JACKPOT};
use lottery::vault::*;

fn key(n: u8) -> OwnerKey {
    OwnerKey::Schnorr([0x60 + n; 32])
}

/// Conservation of a step: inputs = outputs + the case's fee.
fn conserve(step: &str, f: &Final, fee: u64) {
    let ins: u64 = f.entries.iter().map(|e| e.amount).sum();
    let outs: u64 = f.tx.outputs.iter().map(|o| o.value).sum();
    assert_eq!(ins, outs + fee, "{step}: inputs = outputs + fee");
    assert_eq!(f.fee(), fee, "{step}: fee");
}

// ---------------------------------------------------------------- LP1

fn genesis_case() -> (Ledger, GenesisCase) {
    let mut l = Ledger::new();
    let op = l.fund(GENESIS_FUNDING, funding_spk(), 100);
    let e = l.get(&op).unwrap().clone();
    let g = GenesisCase::new(4, vec![(op, e)]);
    (l, g)
}

/// LP1: genesis with SHARE_UNIT = 1 (D84): S0 = V0 and the deployer position holds V0
/// shares (built and run); V0 = V0_MIN accepted, V0_MIN - 1 refused (names D84); d =
/// D_FLOOR accepted, D_FLOOR - 1 refused (names D81); a V0 with S0 = V0 > SHARES_MAX refused
/// (names D84). Each refusal also makes the builder panic.
#[test]
fn lp1_genesis_floors() {
    let (_, mut base) = genesis_case();
    base.set_jackpot(0);
    assert_eq!(base.validate(), Ok(()));
    // Accepted: V0 = V0_MIN with d = D_FLOOR, built and run; S0 = V0 in the vault and the
    // deployer position.
    let mut g = base.clone();
    g.set_vault(V0_MIN);
    assert_eq!(g.controller.d, D_FLOOR, "the fixture deposit is D_FLOOR");
    assert_eq!(g.validate(), Ok(()), "V0 = V0_MIN, d = D_FLOOR");
    assert_eq!((g.vault.total_assets, g.vault.total_shares, g.position.shares), (V0_MIN, V0_MIN, V0_MIN), "S0 = V0");
    let f = g.build().finish();
    f.run().unwrap_or_else(|(i, e)| panic!("genesis input {i} rejected: {e:?}"));
    let cov = covenant_id(&f);
    let t = tpl();
    let pos = &f.tx.outputs[g.position_output()];
    assert_eq!(pos.script_public_key, t.position_spk(&PositionState::new(0, V0_MIN, &Payout::Schnorr(xonly(&FUNDING_SK)))));
    assert_eq!(pos.covenant.map(|b| b.covenant_id), Some(cov));
    assert_eq!(f.tx.outputs[g.vault_output()].script_public_key, t.vault_spk(&g.vault));
    assert_eq!(f.tx.outputs[g.vault_output()].value, V0_MIN as u64 + g.reserve());
    // Refused.
    let set_d = |g: &mut GenesisCase, d: i64| {
        g.controller.d = d;
        for s in &mut g.shards {
            s.d = d;
        }
    };
    let mut bad: Vec<(&str, GenesisCase, &str)> = vec![];
    let mut add = |name: &'static str, f: &dyn Fn(&mut GenesisCase), needle: &'static str| {
        let mut g = base.clone();
        f(&mut g);
        bad.push((name, g, needle));
    };
    add("V0 = V0_MIN - 1", &|g| g.set_vault(V0_MIN - 1), "D84");
    add("d = D_FLOOR - 1", &|g| set_d(g, D_FLOOR - 1), "D81");
    add("S0 = V0 = SHARES_MAX + 1", &|g| g.set_vault(SHARES_MAX + 1), "D84");
    let mut accepted = vec![];
    for (name, g, needle) in &bad {
        match g.validate() {
            Err(e) => assert!(e.contains(needle), "{name}: refused for another reason: {e}"),
            Ok(()) => {
                accepted.push(*name);
                continue;
            }
        }
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.build()));
        assert!(r.is_err(), "{name}: builder accepted the instance");
    }
    assert!(accepted.is_empty(), "LP1: genesis accepts {accepted:?}");
}

// ---------------------------------------------------------------- LP2

/// LP2: `request_deposit` with D = MIN_DEPOSIT (10 KAS) is accepted (standalone and on a
/// chain), converted at the next draw and settled to a position of >= 1 share.
#[test]
fn lp2_min_deposit_converts() {
    let c = dep_min();
    let f = accept5("request_deposit D = MIN_DEPOSIT", &c.clone().into(), &c.build());
    check_deposit(&c, &f);
    let mut ch = chain5();
    let qi = ch.deposit_now(&key(1).payout(), MIN_DEPOSIT, 300_000);
    let (b, ops) = ch.buy_case(0);
    ch.buy(0, &b, ops, 300_500);
    ch.close_now();
    let (dc, _) = ch.draw_now([1; 32]);
    let x = dc.expected();
    assert!(x.rec && x.conv.ok && !x.conv.refund && x.conv.d_c == MIN_DEPOSIT, "{:?}", x.conv);
    let s = ch.settle_now(qi, 2_000_000);
    assert_eq!(s.mode, SettleMode::DepositConvert);
    assert!(s.sh >= 1, "a position of >= 1 share: {}", s.sh);
    assert_eq!(s.sh, x.conv.mint_c, "the only depositor gets the whole mint");
    ch.assert_share_invariants("LP2");
}

// ---------------------------------------------------------------- LP3

/// LP3: `request_withdraw` at the W4b edges, every one accepted and checked output by
/// output: a partial w = wmin (q >= 1, three owner key kinds); a whole position whose
/// shares are below wmin; at q = 0 (total_assets + VA < W_MIN_VALUE) a partial w = 1; at
/// q = 1 exactly (total_assets + VA = W_MIN_VALUE: wmin = S + VS > S, so no partial fits
/// W4) the whole position. LA2 is the rejecting half.
#[test]
fn lp3_withdrawal_minimum_edges() {
    for (name, k) in keys() {
        let c = wd_at_wmin(k);
        assert_eq!(c.w, 1_000_000_000, "wmin at the fixture vault (V = S = 10^15)");
        let f = accept5(&format!("request_withdraw w = wmin ({name})"), &c.clone().into(), &c.build());
        check_withdraw(&c, &f);
        assert!(c.expected().partial);
    }
    let cases = [("whole position below wmin", wd_whole_small(owner()), false), ("q = 0: partial w = 1", wd_q0_one(owner()), true), ("q = 1: the whole position", wd_q1_whole(owner()), false)];
    for (name, c, partial) in cases {
        assert_eq!(c.expected().partial, partial, "{name}");
        let wm = w_min(c.vault.total_assets, c.vault.total_shares);
        assert!(c.w >= wm || c.w == c.position.shares, "{name}: the Rust mirror accepts");
        let f = accept5(&format!("request_withdraw ({name})"), &c.clone().into(), &c.build());
        check_withdraw(&c, &f);
    }
}

// ---------------------------------------------------------------- LP4 (script half)

/// LP4 (script half; the Rust half and the value bound are `game::tests::lp4_*`):
/// `request_withdraw` on fabricated vaults over A in {0, W_V - 2, W_V - 1, 10^15,
/// ASSETS_MAX}, S in {0, 10^15, SHARES_MAX}, two position sizes and w in {wmin - 1, wmin,
/// shares}: the script accepts exactly when 1 <= w <= shares, w <= S - pend_wsh (W4) and
/// (w >= `game::w_min(A, S)` or w = shares) (W4b), and never rejects by an engine
/// arithmetic error.
#[test]
fn lp4_w_min_matches_the_script() {
    let mut mismatches = vec![];
    let mut n = 0;
    for a in [0, W_MIN_VALUE - VA - 1, W_MIN_VALUE - VA, 10_000_000 * KAS as i64, ASSETS_MAX] {
        for s in [0, 10_000_000 * KAS as i64, SHARES_MAX] {
            let wm = w_min(a, s);
            for shares in [50_000_000_000, (wm - 1).clamp(1, SHARES_MAX)] {
                for w in [wm - 1, wm, shares] {
                    let mut c = wd(owner());
                    c.vault.total_assets = a;
                    c.vault.total_shares = s;
                    c.vault.liability = 0;
                    c.position.shares = shares;
                    c.w = w;
                    let expect = w >= 1 && w <= shares && w <= s - c.vault.pend_wsh && (w >= wm || w == shares);
                    let f = c.build().finish();
                    let r = f.run_input(VLT0);
                    let v = verdict(&r);
                    assert_ne!(v, "ARITHMETIC", "A = {a}, S = {s}, shares = {shares}, w = {w}: {r:?}");
                    if expect {
                        if let Err((i, e)) = f.run() {
                            mismatches.push(format!("A = {a}, S = {s}, shares = {shares}, w = {w} (wmin {wm}): expected accepted, input {i} rejects: {e:?}"));
                        }
                    } else if v == "accepted" {
                        mismatches.push(format!("A = {a}, S = {s}, shares = {shares}, w = {w} (wmin {wm}): expected rejected, the vault accepts"));
                    }
                    n += 1;
                }
            }
        }
    }
    for m in &mismatches {
        println!("LP4 MISMATCH {m}");
    }
    assert!(mismatches.is_empty(), "LP4: {} of {n} grid points differ from game::w_min; first: {}", mismatches.len(), mismatches[0]);
}

// ---------------------------------------------------------------- LP5

/// LP5 (lock-out headroom, D84, D85; VP9 adapted): a fabricated vault at a price of exactly
/// MIN_DEPOSIT per share (V' + VA = MIN_DEPOSIT (S + VS)) and one sompi below converts a
/// MIN_DEPOSIT deposit (mint 1, refund = 0) and the settlement mints a 1-share position;
/// one sompi above, the mint is 0 < 1 and the draw writes refund mode, and the settlement
/// pays s + R.
#[test]
fn lp5_lock_out_headroom() {
    let s = 4_000;
    let at = MIN_DEPOSIT * (s + VS);
    for (name, m, converts) in [("price just below MIN_DEPOSIT per share", at - 1, true), ("price = MIN_DEPOSIT per share", at, true), ("price just above MIN_DEPOSIT per share", at + 1, false)] {
        let c = d_at_price(m, s, MIN_DEPOSIT);
        let cv = c.expected().conv;
        assert_eq!((cv.ok, cv.refund, cv.mint), (converts, !converts, converts as i64), "{name}: {cv:?}");
        let f = accept5(&format!("draw ({name})"), &Case5::Draw(c.clone()), &c.build());
        let x = check_draw5(&c, &f);
        let rc = x.record.clone().unwrap();
        assert_eq!(rc.refund, !converts as u8, "{name}: record refund flag");
        let rq = RequestState::new(rc.round, KIND_DEPOSIT, rc.dep_rem, &owner().payout());
        let mut st = SettleCase::new(rc, rq);
        st.record_reserve = c.reserve();
        let sx = st.expected();
        let f = accept5(&format!("settle ({name})"), &st.clone().into(), &st.build());
        check_settle(&st, &f);
        if converts {
            assert_eq!((sx.mode, sx.sh), (SettleMode::DepositConvert, 1), "{name}");
        } else {
            assert_eq!(sx.mode, SettleMode::DepositRefund, "{name}");
            assert_eq!(f.tx.outputs[sx.payout_output.unwrap()].value, st.request.amount as u64 + st.request_value);
        }
    }
}

// ---------------------------------------------------------------- LP6

/// LP6: one round at the final constants on a real chain (genesis -> requests -> buy ->
/// `close()` -> `draw()` -> claim -> settlements): buys at d = D_FLOOR = 0.13 KAS, a deposit
/// of MIN_DEPOSIT = 10 KAS, a partial withdrawal of the deployer at exactly wmin, a
/// converting `draw()` (the jackpot combination held), the FINAL jackpot claim and both
/// settlements; conservation at every step and the share invariants after each.
#[test]
fn lp6_chain_at_the_final_constants() {
    let mut ch = chain5();
    assert_eq!(ch.controller.1.d, D_FLOOR, "the chain sells at d = D_FLOOR");
    let seed = [6u8; 32];
    let win = ch.winning(&seed);
    // Deposit of MIN_DEPOSIT.
    let (c, ops) = ch.deposit_case(&key(1).payout(), MIN_DEPOSIT, 300_000);
    let f = ch.deposit(&c, ops, 300_000);
    conserve("request_deposit", &f, c.fee);
    let qd = ch.requests.len() - 1;
    // Partial withdrawal at exactly wmin.
    let wm = w_min(ch.vault.1.total_assets, ch.vault.1.total_shares);
    assert!(wm >= 2 && wm < ch.positions[0].1.shares);
    let (c, ops) = ch.withdraw_case(0, wm, deployer(), 300_000);
    let f = ch.withdraw(&c, ops, 300_000);
    conserve("request_withdraw at wmin", &f, c.fee);
    assert!(c.expected().partial);
    let qw = ch.requests.len() - 1;
    ch.assert_share_invariants("LP6 requests");
    // A buy of the jackpot combination at d = 0.13 KAS.
    let (mut b, ops) = ch.buy_case(0);
    b.bundle = vec![Combo::new(win.main, win.bonus)];
    assert_eq!(b.shard.d, D_FLOOR);
    let f = ch.buy(0, &b, ops, 310_000);
    conserve("buy", &f, b.fee);
    assert_eq!(f.tx.outputs[1].value, D_FLOOR as u64, "ticket value d x b");
    // close() and the converting draw().
    let (cc, ops) = ch.close_case(ch.controller.1.deadline as u64 - 500);
    let mut cc = cc;
    cc.lock_time = ch.controller.1.deadline as u64;
    let f = ch.close(&cc, ops, cc.lock_time + 10);
    conserve("close", &f, cc.fee);
    let (dc, f) = ch.draw_now(seed);
    let x = dc.expected();
    assert!(x.rec && x.conv.ok && x.conv.wc == wm && x.pools.jp, "converting draw with the jackpot held: {:?}", x.conv);
    conserve("draw", &f, dc.fee);
    ch.assert_share_invariants("LP6 after draw");
    // The FINAL jackpot claim.
    let ti = ch.tickets.len() - 1;
    let pi = ch.pool_of(1, TIER_JACKPOT);
    let now = ch.drawer.2 + 1_000;
    let (cl, ops) = ch.claim_case(ti, pi, 0, now - 100);
    let cx = cl.expected();
    assert!(cx.rejects.is_empty() && cx.prize > 0, "{:?}", cx.rejects);
    let f = ch.claim(&cl, ops, now);
    conserve("claim", &f, cl.fee);
    // Both settlements.
    let sd = ch.settle_now(qd, now + 100);
    assert_eq!(sd.mode, SettleMode::DepositConvert);
    let qw = ch.requests.iter().position(|q| q.1.kind == KIND_WITHDRAW).unwrap_or(qw);
    let sw = ch.settle_now(qw, now + 200);
    assert_eq!(sw.mode, SettleMode::Final, "the wmin withdrawal converts whole");
    assert!(ch.records.is_empty(), "the record is drained");
    ch.assert_share_invariants("LP6 end");
}

// ---------------------------------------------------------------- LP7

/// LP7: `RoundResult::from_tx` (from the transaction bytes alone) equals the builder's
/// `RoundResult::new` on every draw shape the suite builds: the M3 bases (jackpot held or
/// not, absorption or not, small sales, fee below F_MIN, D45 zero pools, a late absorbed
/// deadline), round 1, the M5 conversion bases (deposits, withdrawals, both, cap,
/// refund by price, the mint guard both sides, J from V' with large deposits, no pools) and
/// the M6 guard edge; plus chain draws with nothing pending and with conversions.
#[test]
fn lp7_round_result_from_tx() {
    let t = tpl();
    let shapes: Vec<(&str, DrawCase)> = vec![
        ("m3 base", common::m3::base()),
        ("m3 no jackpot", common::m3::base_no_jp()),
        ("m3 no absorb", common::m3::base_no_absorb()),
        ("m3 small sales", common::m3::small_sales()),
        ("m3 fee low", common::m3::fee_low()),
        ("m3 zero jackpot held", common::m3::zero_jackpot_held()),
        ("m3 zero G2 held", common::m3::zero_g2_held()),
        ("m3 late deadline", common::m3::late_deadline()),
        ("round 1", DrawCase::new(4)),
        ("m5 deposits", d_dep()),
        ("m5 withdrawals", d_wd()),
        ("m5 both", d_both()),
        ("m5 cap", d_cap()),
        ("m5 refund by price", d_refund_price()),
        ("m5 mint guard at", d_mint_guard(false)),
        ("m5 mint guard above", d_mint_guard(true)),
        ("m5 big deposit", d_big_dep()),
        ("m5 no pools", d_no_pools(MIN_DEPOSIT, 3_000_000)),
        ("m6 guard edge", d_guard_edge()),
    ];
    for (name, c) in shapes {
        let f = accept5(&format!("draw ({name})"), &Case5::Draw(c.clone()), &c.build());
        let x = c.expected();
        assert_eq!(RoundResult::from_tx(&f.tx, &t), Ok(RoundResult::new(&c, &x, f.tx.id())), "{name}");
    }
    // Chain draws: nothing pending, then deposits and a withdrawal pending.
    let mut ch = chain5();
    for (round, pending) in [(1, false), (2, true)] {
        let base = ch.controller.1.deadline as u64 - 60_000;
        if pending {
            ch.deposit_now(&key(1).payout(), MIN_DEPOSIT, base);
            let s0 = ch.positions[0].1.shares;
            let (c, ops) = ch.withdraw_case(0, s0 / 10, deployer(), base + 5_000);
            ch.withdraw(&c, ops, base + 5_000);
        }
        let (b, ops) = ch.buy_case(0);
        ch.buy(0, &b, ops, base + 10_000);
        ch.close_now();
        let (c, f) = ch.draw_now([round as u8; 32]);
        let x = c.expected();
        assert_eq!(x.rec, pending, "chain round {round}");
        assert_eq!(RoundResult::from_tx(&f.tx, &ch.t), Ok(RoundResult::new(&c, &x, f.tx.id())), "chain round {round}");
    }
}

// ---------------------------------------------------------------- LP8

/// LP8: every M1-M5 test passes at the final constants (fixtures updated where they
/// hard-coded d = 0.11 KAS, 1 KAS deposits or SHARE_UNIT = 1,000; each change is in the
/// handoff log and becomes a spec Correction): that is the full `cargo test` run, not one
/// test. This test pins the precondition for it: the Rust mirrors equal the template
/// constants (MIN_DEPOSIT in vault.sil, drawer.sil and request.sil; W_MIN_VALUE, VS and VA
/// in vault.sil), so the fixtures and the scripts use the same numbers.
#[test]
fn lp8_constants_mirror_the_scripts() {
    fn constant(file: &str, name: &str) -> Option<i64> {
        let src = std::fs::read_to_string(covenant_path(file)).unwrap_or_else(|e| panic!("read {file}: {e}"));
        let pat = format!("int constant {name} = ");
        src.lines().map(str::trim).find_map(|l| l.strip_prefix(&pat)).map(|v| v.trim_end_matches(';').trim().parse().unwrap_or_else(|e| panic!("{file} {name}: {e}")))
    }
    let mut diffs = vec![];
    let want = [
        ("vault.sil", "MIN_DEPOSIT", MIN_DEPOSIT),
        ("drawer.sil", "MIN_DEPOSIT", MIN_DEPOSIT),
        ("request.sil", "MIN_DEPOSIT", MIN_DEPOSIT),
        ("vault.sil", "W_MIN_VALUE", W_MIN_VALUE),
        ("vault.sil", "VS", VS),
        ("vault.sil", "VA", VA),
        ("drawer.sil", "VS", VS),
        ("drawer.sil", "VA", VA),
    ];
    for (file, name, v) in want {
        let got = constant(file, name);
        if got != Some(v) {
            diffs.push(format!("{file} {name} = {got:?}, game = {v}"));
        }
    }
    assert!(diffs.is_empty(), "LP8: template constants differ from the Rust mirror: {diffs:?}");
}

