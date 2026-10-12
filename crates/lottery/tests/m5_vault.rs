//! M5 positive tests (spec M5 A10: VP2-VP11, VP13, VP14 and the script half of VP12; VP1 and
//! the Rust half of VP12 are in m5_game.rs).
//!
//! As M4: full transactions in the v2.1.0 engine with production flags and committed
//! budgets. Every request, settlement and conversion is checked output by output against
//! the builders' reference (`game::convert`, `game::settle`) and for conservation (VP13) by
//! `common::m5::check_*`. Main positives run real genesis -> requests -> buys -> `close()` ->
//! `draw()` -> settlements (D79); boundary cases use fabricated vaults, records, requests and
//! positions under the instance covenant ID.
//!
//! M5 Stage 2 (tests first): until Stages 3-5 write the scripts, each test fails at its
//! first M5 transaction: a vault, request, record or position placeholder rejects at
//! dispatch (EarlyReturn), or the M4 drawer rejects a converting draw (VerifyError). Only
//! VP10 (nothing pending: the M4 draw) passes. `M5_PREFLIGHT=1` runs every test end to end
//! with the spec model (`common::m5_spec`) standing in for the missing scripts.

mod common;

use common::m2::*;
use common::m5::*;
use lottery::buy::{KAS, OwnerKey};
use lottery::close::FundingIn;
use lottery::combo::Combo;
use lottery::draw::{DrawCase, RoundResult};
use lottery::game::{MIN_DEPOSIT, P_MIN, SHARES_MAX, SettleMode, VS, convert, withdraw_out};
use lottery::state::{KIND_DEPOSIT, KIND_WITHDRAW, Payout, RequestState};
use lottery::vault::*;

fn key(n: u8) -> OwnerKey {
    OwnerKey::Schnorr([0x40 + n; 32])
}

/// Accept a standalone draw (`accept5`) and check its M5 outputs (VP13).
fn draw5(c: &DrawCase) -> lottery::draw::DrawExpected {
    let f = accept5(&Case5::Draw(c.clone()).step(), &Case5::Draw(c.clone()), &c.build());
    check_draw5(c, &f)
}

// ---------------------------------------------------------------- VP2

/// VP2: `request_deposit`: vault' pend_dep + D and value + D; the request {r + 1, 0, D,
/// owner} with value R; with and without change; with F_MAX funding inputs; on a chain.
#[test]
fn vp2_request_deposit() {
    let c = dep();
    let f = accept5("request_deposit", &c.clone().into(), &c.build());
    check_deposit(&c, &f);
    // Without change: funding = D + R + fee.
    let mut nc = dep();
    nc.change = false;
    nc.funding = vec![FundingIn { value: nc.d as u64 + nc.reserve + nc.fee, daa: 1_100_000 }];
    let f = accept5("request_deposit (no change)", &nc.clone().into(), &nc.build());
    check_deposit(&nc, &f);
    assert_eq!(f.tx.outputs.len(), 2);
    // F_MAX = 8 funding inputs.
    let mut fm = dep();
    fm.funding = (0..8).map(|_| FundingIn { value: 3 * KAS, daa: 1_100_000 }).collect();
    let f = accept5("request_deposit (F_MAX funding)", &fm.clone().into(), &fm.build());
    check_deposit(&fm, &f);
    // The smallest deposit, and an ECDSA and a P2SH owner.
    for owner in [key(1).payout(), OwnerKey::Ecdsa(OWNER_SK).payout(), OwnerKey::P2sh(OWNER_SK).payout()] {
        let mut m = dep();
        m.d = MIN_DEPOSIT;
        m.owner = owner;
        let f = accept5("request_deposit (MIN_DEPOSIT)", &m.clone().into(), &m.build());
        check_deposit(&m, &f);
    }
    // On a chain: two requests in one round accumulate in pend_dep.
    let mut ch = chain5();
    ch.deposit_now(&key(1).payout(), 10 * KAS as i64, 300_000);
    ch.deposit_now(&key(2).payout(), MIN_DEPOSIT, 300_500);
    assert_eq!(ch.vault.1.pend_dep, 10 * KAS as i64 + MIN_DEPOSIT);
    assert_eq!(ch.requests.len(), 2);
    assert!(ch.requests.iter().all(|q| q.1.round == 1 && q.1.kind == KIND_DEPOSIT && q.2 == ch.reserve()));
    ch.assert_share_invariants("VP2 chain");
}

// ---------------------------------------------------------------- VP3

/// VP3: `request_withdraw`, whole and partial, with the owner input as P2PK Schnorr, P2PK
/// ECDSA and P2SH; vault' pend_wsh + w; request value R; position' shares - w.
#[test]
fn vp3_request_withdraw() {
    for (name, k) in keys() {
        for whole in [false, true] {
            let c = if whole { wd_whole(k.clone()) } else { wd(k.clone()) };
            let f = accept5(&format!("request_withdraw ({name}, whole {whole})"), &c.clone().into(), &c.build());
            check_withdraw(&c, &f);
            assert_eq!(c.expected().partial, !whole);
        }
    }
    // The owner input after another funding input (owner_idx = 3).
    let mut c = wd(OwnerKey::Ecdsa(OWNER_SK));
    c.funding = vec![FundingIn { value: 5 * KAS, daa: 1_100_000 }];
    c.owner_at = 1;
    let f = accept5("request_withdraw (owner_idx 3)", &c.clone().into(), &c.build());
    check_withdraw(&c, &f);
    // On a chain: the deployer withdraws part of the genesis position, then the rest.
    let mut ch = chain5();
    let s0 = ch.positions[0].1.shares;
    let (c, ops) = ch.withdraw_case(0, s0 / 4, deployer(), 300_000);
    ch.withdraw(&c, ops, 300_000);
    let last = ch.positions.len() - 1;
    let (c, ops) = ch.withdraw_case(last, s0 - s0 / 4, deployer(), 300_500);
    ch.withdraw(&c, ops, 300_500);
    assert!(ch.positions.is_empty());
    assert_eq!(ch.vault.1.pend_wsh, s0);
    ch.assert_share_invariants("VP3 chain");
}

// ---------------------------------------------------------------- VP4

/// One round of a chain: `deposits` (owner, D) and `withdrawals` (position index, w) are
/// requested, one buy in shard 0 of `bundle`, then close and draw with `seed`. Returns the
/// draw case and its transaction.
fn round(ch: &mut Chain, seed: [u8; 32], deposits: &[(Payout, i64)], withdrawals: &[(usize, i64)], bundle: Option<Vec<Combo>>) -> (DrawCase, lottery::buy::Final) {
    let base = ch.controller.1.deadline as u64 - 60_000;
    for (n, (o, d)) in deposits.iter().enumerate() {
        ch.deposit_now(o, *d, base + 100 * n as u64);
    }
    // Withdrawals from the highest index first (removing a position shifts the later ones).
    let mut ws = withdrawals.to_vec();
    ws.sort_by(|a, b| b.0.cmp(&a.0));
    for (n, (pi, w)) in ws.iter().enumerate() {
        let (c, ops) = ch.withdraw_case(*pi, *w, deployer_or_owner(ch, *pi), base + 5_000 + 100 * n as u64);
        ch.withdraw(&c, ops, base + 5_000 + 100 * n as u64);
    }
    let (mut b, ops) = ch.buy_case(0);
    // Bonus 1 is valid for every m (a later round's m may be 2).
    b.bundle = bundle.unwrap_or_else(|| vec![Combo::new([3, 7, 12, 19, 28], 1)]);
    ch.buy(0, &b, ops, base + 10_000);
    ch.assert_share_invariants("before close");
    ch.close_now();
    let (c, f) = ch.draw_now(seed);
    ch.assert_share_invariants("after draw");
    // The liability of the round whose jackpot is now fixed stays covered (VP14).
    assert!(ch.vault.1.total_assets >= ch.vault.1.liability, "total_assets' {} >= L' {}", ch.vault.1.total_assets, ch.vault.1.liability);
    (c, f)
}

/// The owner key of position `pi` (the deployer's or one of `key(n)`).
fn deployer_or_owner(ch: &Chain, pi: usize) -> OwnerKey {
    let p = Payout::from_fields(ch.positions[pi].1.owner_class, &ch.positions[pi].1.owner_key);
    if p == deployer().payout() {
        return deployer();
    }
    (1..=9).map(key).find(|k| k.payout() == p).expect("a known owner")
}

/// VP4: a draw with pending deposits: mint = `game::convert`; vault', the record and
/// controller' v_rec equal the reference; J', G2' and L' equal those of the same draw with
/// nothing pending (J is fixed before any conversion); the round result rebuilds the record
/// from the transaction bytes (R4).
#[test]
fn vp4_draw_with_deposits() {
    let mut ch = chain5();
    let mut twin = ch.clone();
    let (c, f) = round(&mut ch, [1; 32], &[(key(1).payout(), 10 * KAS as i64), (key(2).payout(), 25 * KAS as i64)], &[], None);
    let x = c.expected();
    assert!(x.rec && x.conv.ok && !x.conv.refund);
    assert_eq!(x.conv.d_c, 35 * KAS as i64);
    assert_eq!(x.conv.mint, convert(x.v_prime, c.vault.total_shares, c.vault.pend_dep, 0, x.fix.liability).unwrap().mint);
    let (c0, _) = round(&mut twin, [1; 32], &[], &[], None);
    let x0 = c0.expected();
    assert!(!x0.rec);
    assert_eq!((x.fix.jackpot, x.fix.g2, x.fix.liability), (x0.fix.jackpot, x0.fix.g2, x0.fix.liability), "J', G2', L' independent of the deposits");
    assert_eq!(x.controller.v_rec, x0.controller.v_rec + 35 * KAS as i64, "v_rec = total_assets' = V' + D_c");
    // The record is rebuilt from the bytes only with the M5 entry (its wc argument, S2-2);
    // an M4-signature draw with something pending is refused.
    if lottery::draw::drawer_is_m5(&ch.t) {
        let rr = RoundResult::from_tx(&f.tx, &ch.t).expect("round result from the bytes");
        assert_eq!(rr, RoundResult::new(&c, &x, f.tx.id()));
        assert_eq!(rr.record.as_ref().map(|r| &r.state), x.record.as_ref());
    } else {
        assert_eq!(RoundResult::from_tx(&f.tx, &ch.t), Err("an M4 draw_test_seed with something pending".to_string()));
    }
}

// ---------------------------------------------------------------- VP5

/// VP5: pending withdrawals within the slack: wc = W, out_c = floor(W (V' + 1) / (S + VS)).
#[test]
fn vp5_withdrawals_within_slack() {
    let mut ch = chain5();
    let w = ch.positions[0].1.shares / 10;
    let (c, _) = round(&mut ch, [1; 32], &[], &[(0, w)], None);
    let x = c.expected();
    assert_eq!(x.conv.wc, w, "wc = W");
    let want = ((w as i128 * (x.v_prime as i128 + 1)) / (c.vault.total_shares as i128 + VS as i128)) as i64;
    assert_eq!(x.conv.out_c, want);
    assert_eq!(ch.vault.1.pend_wsh, 0);
    assert_eq!(ch.vault.1.total_shares, c.vault.total_shares - w);
    // The standalone base with both sides pending.
    let x = draw5(&d_both());
    assert_eq!(x.conv.wc, x.conv.w);
}

// ---------------------------------------------------------------- VP6

/// VP6: the withdrawal cap binds: wc < W, total_assets' >= L', out(wc + 1) > slack;
/// pend_wsh' = W - wc; same-draw deposits raise wc (D69).
#[test]
fn vp6_cap_binds() {
    let c = d_cap();
    let x = draw5(&c);
    let cv = x.conv;
    assert!(cv.wc < cv.w);
    assert!(x.vault.total_assets >= x.fix.liability, "total_assets' >= L'");
    assert!(withdraw_out(x.v_prime, cv.s, cv.wc + 1).unwrap() > cv.slack, "out(wc + 1) > slack");
    assert_eq!(x.vault.pend_wsh, cv.w - cv.wc);
    // Without the same draw's deposits, fewer shares convert.
    let mut nd = c.clone();
    nd.vault.pend_dep = 0;
    let y = draw5(&nd);
    assert!(y.conv.wc < cv.wc, "deposits raise wc: {} < {}", y.conv.wc, cv.wc);
    // On a chain: the deployer asks for 95 % of the shares; the rest is carried over.
    let mut ch = chain5();
    let w = ch.positions[0].1.shares / 100 * 95;
    let (c, _) = round(&mut ch, [1; 32], &[], &[(0, w)], None);
    let x = c.expected();
    assert!(x.conv.wc < w && ch.vault.1.pend_wsh == w - x.conv.wc);
}

// ---------------------------------------------------------------- VP7

/// VP7 (a): three depositors drain the record's mint exactly in two orders: sum sh =
/// mint_c, the last settler gets the remainder, the record is then consumed and its R goes
/// to the funder.
#[test]
fn vp7_deposit_drain_two_orders() {
    let mut ch = chain5();
    let deps = [(key(1).payout(), 10 * KAS as i64), (key(2).payout(), 27 * KAS as i64 + 3), (key(3).payout(), MIN_DEPOSIT + 7)];
    let (c, _) = round(&mut ch, [1; 32], &deps, &[], None);
    let mint_c = c.expected().conv.mint_c;
    for order in [[0usize, 1, 2], [2, 0, 1]] {
        let mut o = ch.clone();
        let mut total = 0;
        for (n, &k) in order.iter().enumerate() {
            let owner = deps[k].0;
            let qi = o.requests.iter().position(|q| Payout::from_fields(q.1.owner_class, &q.1.owner_key) == owner).unwrap();
            let rem = o.records[o.record_of(1)].1.mint_rem;
            let x = o.settle_now(qi, 1_500_000 + 1_000 * n as u64);
            assert_eq!(x.mode, SettleMode::DepositConvert);
            assert!(x.sh >= 1);
            if n == 2 {
                assert_eq!(x.sh, rem, "the last settler gets the remainder");
                assert!(x.drained && x.refund_output.is_some(), "drained: R to the funder");
            }
            total += x.sh;
            o.assert_share_invariants("VP7 drain");
        }
        assert_eq!(total, mint_c, "sum sh = mint_c ({order:?})");
        assert!(o.records.is_empty(), "the record is consumed");
        assert_eq!(o.positions.len(), 4);
    }
}

/// VP7 (b): withdrawal FINAL (payout pay + R); a capped withdrawal settles PAY, its
/// request' {r + 1, left} settles against the next draw's record; PAY at pay = P_MIN and
/// ACCRUE at P_MIN - 1 (fabricated records).
#[test]
fn vp7_withdrawal_modes() {
    // FINAL on a chain (within the slack).
    let mut ch = chain5();
    let w = ch.positions[0].1.shares / 10;
    round(&mut ch, [1; 32], &[], &[(0, w)], None);
    let x = ch.settle_now(0, 1_500_000);
    assert_eq!(x.mode, SettleMode::Final);
    assert!(x.drained && ch.records.is_empty());
    // A capped withdrawal: PAY, then request' against round 2's record.
    let mut ch = chain5();
    let w = ch.positions[0].1.shares / 100 * 95;
    round(&mut ch, [1; 32], &[], &[(0, w)], None);
    let x = ch.settle_now(0, 1_500_000);
    assert_eq!(x.mode, SettleMode::Pay, "{x:?}");
    let rq = x.request.clone().unwrap();
    assert_eq!((rq.round, rq.kind, rq.amount), (2, KIND_WITHDRAW, x.left));
    assert_eq!(ch.vault.1.pend_wsh, x.left, "request' is covered by the carried pend_wsh");
    round(&mut ch, [2; 32], &[], &[], None);
    let qi = ch.requests.iter().position(|q| q.1.round == 2).unwrap();
    let y = ch.settle_now(qi, ch.controller.1.deadline as u64 - 50_000);
    assert!(matches!(y.mode, SettleMode::Final | SettleMode::Pay | SettleMode::Accrue));
    ch.assert_share_invariants("VP7 carried withdrawal");
    // PAY at pay = P_MIN; ACCRUE at P_MIN - 1.
    let pay = SettleCase::new(record(0, 0, 0, 3_000_000, 1_500_000, 3 * P_MIN), RequestState::new(2, KIND_WITHDRAW, 1_000_000, &owner().payout()));
    let xp = pay.expected();
    assert_eq!((xp.mode, xp.pay), (SettleMode::Pay, P_MIN));
    let f = accept5("settle PAY at P_MIN", &pay.clone().into(), &pay.build());
    check_settle(&pay, &f);
    let acc = st_accrue();
    assert_eq!(acc.expected().pay, P_MIN - 1);
    let f = accept5("settle ACCRUE at P_MIN - 1", &acc.clone().into(), &acc.build());
    check_settle(&acc, &f);
    // Every standalone settlement base.
    for (name, c) in settle_bases() {
        let f = accept5(&format!("settle {name}"), &c.clone().into(), &c.build());
        check_settle(&c, &f);
    }
}

// ---------------------------------------------------------------- VP8

/// VP8 (milestone fairness): a backer deposits during round r. (a) The jackpot is unheld,
/// so the vault gains H; (b) the jackpot is held. In both, the new position's value at the
/// post-draw price equals the deposit within rounding, and the existing backer (the
/// deployer) carries the whole gain or loss.
#[test]
fn vp8_fairness() {
    let d = 1_000 * KAS as i64;
    for held in [false, true] {
        let mut ch = chain5();
        let seed = [3u8; 32];
        let w = ch.winning(&seed);
        let bundle = held.then(|| vec![Combo::new(w.main, w.bonus)]);
        let (v0, s0) = (ch.vault.1.total_assets, ch.vault.1.total_shares);
        let before = position_worth(s0, v0, s0);
        let (c, _) = round(&mut ch, seed, &[(key(1).payout(), d)], &[], bundle);
        let x = c.expected();
        assert_eq!(x.pools.jp, held, "jackpot held: {held}");
        let q = ch.requests.iter().position(|q| q.1.kind == KIND_DEPOSIT).unwrap();
        let st = ch.settle_now(q, ch.controller.1.deadline as u64 - 50_000);
        let (a, s) = (ch.vault.1.total_assets, ch.vault.1.total_shares);
        let price = (a + 1) / (s + VS) + 1;
        let worth = position_worth(st.sh, a, s);
        assert!(worth <= d && d - worth <= 2 * price, "held {held}: the new position is worth {worth} for a deposit of {d}");
        let gain = x.v_prime - v0;
        let after = position_worth(s0, a, s);
        let carried = after - before;
        let slack = (gain.abs() as i128 * VS as i128 / s0 as i128) as i64 + 4 * price;
        assert!((carried - gain).abs() <= slack, "held {held}: the deployer carries {carried} of the gain {gain}");
        assert_eq!(held, gain < 0, "held: a loss; unheld: H");
    }
}

// ---------------------------------------------------------------- VP9

/// VP9: refund mode. A fabricated vault whose price makes mint < ceil(D / MIN_DEPOSIT),
/// and one where mint would exceed SHARES_MAX - S. The draw writes refund = 1 and keeps
/// everything else; the deposit settlement pays s + R.
#[test]
fn vp9_refund_mode() {
    let mut big = DrawCase::new(4);
    big.vault.total_assets = 10 * KAS as i64;
    big.vault.total_shares = SHARES_MAX - 5;
    big.vault.pend_dep = MIN_DEPOSIT;
    for (name, c) in [("price", d_refund_price()), ("share room", big)] {
        let x = draw5(&c);
        let rc = x.record.clone().unwrap();
        assert_eq!((rc.refund, rc.mint_rem, rc.dep_rem), (1, 0, c.vault.pend_dep), "{name}");
        assert_eq!(x.vault.total_shares, c.vault.total_shares, "{name}: no share minted");
        assert_eq!(x.vault.total_assets, x.v_prime, "{name}: D stays in the record");
        // The single depositor settles: refund of s + R, the record drained.
        let rq = RequestState::new(rc.round, KIND_DEPOSIT, rc.dep_rem, &owner().payout());
        let mut s = SettleCase::new(rc, rq);
        s.record_reserve = c.reserve();
        let sx = s.expected();
        assert_eq!((sx.mode, sx.drained), (SettleMode::DepositRefund, true));
        let f = accept5(&format!("settle refund ({name})"), &s.clone().into(), &s.build());
        check_settle(&s, &f);
        assert_eq!(f.tx.outputs[sx.payout_output.unwrap()].value, s.request.amount as u64 + s.request_value);
    }
    // The mint guard boundaries (VA6): both sides of the pre-guard are valid draws.
    for above in [false, true] {
        draw5(&d_mint_guard(above));
    }
}

// ---------------------------------------------------------------- VP10

/// VP10: nothing pending: no record, the M4 output shape, accepted by the drawer as in M4
/// (regression; the M3/M4 draw tests run unchanged apart from the layouts).
#[test]
fn vp10_nothing_pending() {
    let c = common::m3::base();
    assert!(!c.pending());
    let x = c.expected();
    assert!(!x.rec && x.record.is_none());
    assert_eq!(x.n_cov(), 3 + x.np());
    let f = accept_draw(&c.build());
    assert_eq!(f.tx.outputs.iter().filter(|o| o.covenant.is_some()).count(), 3 + x.np());
    assert_eq!(x.vault.total_shares, c.vault.total_shares);
    assert_eq!(RoundResult::from_tx(&f.tx, &tpl()).unwrap().record, None);
}

// ---------------------------------------------------------------- VP11

/// VP11: `refresh()` of a position, a request and a record at creation + D_IDLE, then their
/// use (a withdrawal from the refreshed position; the refreshed request settled against the
/// refreshed record).
#[test]
fn vp11_refresh_then_use() {
    for (name, c) in refresh5_bases() {
        let f = accept5(&format!("refresh {name}"), &c.clone().into(), &c.build());
        assert_eq!(f.tx.outputs[0].script_public_key, f.entries[0].script_public_key, "byte-identical");
        assert_eq!(f.tx.outputs[0].value, c.value);
    }
    let mut ch = chain5();
    let qi = ch.deposit_now(&key(1).payout(), 10 * KAS as i64, 300_000);
    let (b, ops) = ch.buy_case(0);
    ch.buy(0, &b, ops, 300_500);
    ch.close_now();
    ch.draw_now([1; 32]);
    // Refresh the genesis position, the request and the record, each at creation + D_IDLE.
    let at = |daa: u64| daa + lottery::game::D_IDLE as u64;
    let pd = ch.positions[0].3;
    let (c, ops) = ch.refresh5_case("position", 0, at(pd));
    ch.refresh5(&c, ops, at(pd));
    let qd = ch.requests[qi].3;
    let (c, ops) = ch.refresh5_case("request", qi, at(qd).max(at(pd)) + 10);
    ch.refresh5(&c, ops, at(qd).max(at(pd)) + 10);
    let rd = ch.records[0].3;
    let (c, ops) = ch.refresh5_case("record", 0, at(rd) + 20);
    ch.refresh5(&c, ops, at(rd) + 20);
    let now = ch.controller.1.deadline as u64 - 50_000;
    let x = ch.settle_now(qi, now.max(at(rd) + 100));
    assert_eq!(x.mode, SettleMode::DepositConvert);
    let (c, ops) = ch.withdraw_case(0, 1_000, deployer(), now.max(at(rd) + 200));
    ch.withdraw(&c, ops, now.max(at(rd) + 200));
    ch.assert_share_invariants("VP11");
}

// ---------------------------------------------------------------- VP12 (script half)

/// VP12 (script half): the script accepts the reference outputs over a grid of pending
/// deposits and withdrawals (including the cap and refund branches).
#[test]
fn vp12_script_accepts_reference_grid() {
    let s = common::m3::base().vault.total_shares;
    let mut branches = (false, false, false);
    for d in [0, MIN_DEPOSIT, 30 * KAS as i64 + 1, 100_000 * KAS as i64] {
        for w in [0, 1, 3_000_000, s / 2, s / 100 * 95] {
            if d == 0 && w == 0 {
                continue;
            }
            let mut c = d_pending(d, w);
            c.funding[0].value = 200 * KAS;
            let x = draw5(&c);
            branches.0 |= x.conv.wc < x.conv.w;
            branches.1 |= x.conv.ok;
            branches.2 |= x.conv.wc == x.conv.w && w > 0;
        }
    }
    assert_eq!(branches, (true, true, true), "capped, converted and uncapped branches reached");
    draw5(&d_refund_price());
}

// ---------------------------------------------------------------- VP14

/// VP14: a chain over three draws: deposits and withdrawals in each round, the jackpot held
/// once, a capped withdrawal carried over, every request settled. After each step:
/// total_shares = positions + pend_wsh (+ undrained records' mint_rem, S2-9); total_assets
/// >= L of the fixed round; at the end every record is drained.
#[test]
fn vp14_three_draws() {
    let mut ch = chain5();
    let s0 = ch.positions[0].1.shares;
    // Round 1: two deposits, the deployer asks for 95 % (capped).
    round(&mut ch, [1; 32], &[(key(1).payout(), 50 * KAS as i64), (key(2).payout(), 20 * KAS as i64)], &[(0, s0 / 100 * 95)], None);
    settle_all(&mut ch);
    assert!(ch.vault.1.pend_wsh > 0, "a capped withdrawal carried over");
    // Round 2: the jackpot held; a deposit and a withdrawal by key(1).
    let seed = [2u8; 32];
    let w = ch.winning(&seed);
    let p1 = ch.positions.iter().position(|p| Payout::from_fields(p.1.owner_class, &p.1.owner_key) == key(1).payout()).unwrap();
    let half = ch.positions[p1].1.shares / 2;
    round(&mut ch, seed, &[(key(3).payout(), 5 * KAS as i64)], &[(p1, half)], Some(vec![Combo::new(w.main, w.bonus)]));
    settle_all(&mut ch);
    // Round 3: nothing new but the carried request'.
    round(&mut ch, [3; 32], &[(key(4).payout(), MIN_DEPOSIT)], &[], None);
    settle_all(&mut ch);
    assert!(ch.records.is_empty(), "every record drained");
    ch.assert_share_invariants("VP14 end");
}

/// Settle every live request whose round has a live record (repeatedly, so request's of
/// this round's record are left for the next draw).
fn settle_all(ch: &mut Chain) {
    let now = ch.controller.1.deadline as u64 - 50_000;
    let mut n = 0;
    while let Some(qi) = ch.requests.iter().position(|q| ch.records.iter().any(|r| r.1.round == q.1.round)) {
        ch.settle_now(qi, now + n);
        ch.assert_share_invariants("settle");
        assert!(ch.vault.1.total_assets >= ch.vault.1.liability);
        n += 10;
    }
}
