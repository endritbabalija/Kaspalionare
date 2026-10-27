//! Front end F6 part A: S5 Vault (spec frontend 3 S5, 4 "Vault position card", 7, 8), natively.
//!
//! Money paths, attack tests first (each check that guards money has a refusal here):
//! - `the_vault_view_comes_from_the_vault`: every vault figure from the live vault state and the
//!   `lottery` rules (price, wmin, the request age, MIN_DEPOSIT), the owner filter (another owner's
//!   position or request is left out, never shown as the wallet's), the W4b ranges, request status
//!   (pending, settle due with the settlement, unknown without its record), a twice-given object
//!   and two records of one round refused.
//! - `withdraw_choices_are_what_the_vault_takes`: W4/W4b on amounts (whole, the least and most
//!   partial, below wmin, a remainder below wmin, more than the position) as `vault_amount`, and
//!   the chosen shares accepted by the builder.
//! - `least_and_most_are_amounts_the_vault_takes` (F6 part A review S1): over 400 vaults, the exact
//!   and the shown Least and Most are taken by `withdrawChoice` and one sompi outside is not.
//! - `vault_builders_cap_the_fee_rate`, `a_consistent_build_with_another_lock_time_is_refused` (the
//!   review's probe; N2: the lock-time check's negative control).
//! - `deposit_figures_are_bound_to_the_transaction`, `withdraw_figures_are_bound_to_the_transaction`,
//!   `settle_figures_are_bound_to_the_transaction`: each figure set equals what the transaction does;
//!   any input field that differs from the build's (amount, owner, vault, position, shares, request,
//!   record) is `built_mismatch` (or `vault_amount` / `owner_type`), and a transaction edited after
//!   the build is refused before any figure is read.
//! - `every_settlement_mode_settles_from_the_page`: the five modes (convert, refund, final, pay,
//!   accrue) and a draining settlement build through the API, their figures from
//!   `SettleCase::expected` (the covenant reference), and are signed by the quick wallet and
//!   assembled (every input run in the engine).

mod common;

use common::{Input, addr, fixture_input, live_utxo, vault_at, vault_settles, wallet_owner};
use lottery::game::{self, MIN_DEPOSIT, VAULT_REQ_DELAY};
use lottery::index::State;
use lottery::state::{KIND_DEPOSIT, KIND_WITHDRAW, Payout, PositionState, RequestState};
use lottery::web::Code;
use web::api;
use web::convert::Net;
use web::dto::*;
use web::vault::*;

const N: Net = Net::Testnet10;

fn deposit() -> DepositInput {
    let Input::Deposit(d) = fixture_input("request_deposit") else { unreachable!() };
    d
}

/// A withdrawal fixture as the page builds it: the change goes to the owner's own address (the
/// fixture sends it to another wallet's, so its summary would not count it as coming back).
fn withdraw(name: &str) -> WithdrawInput {
    let Input::Withdraw(mut w) = fixture_input(name) else { unreachable!() };
    w.funds.change = api::address_of(N, &w.owner.coin.script_public_key).unwrap();
    w
}

fn settle(name: &str) -> SettleInput {
    let Input::Settle(s) = fixture_input(name) else { unreachable!() };
    s
}

fn vault_state(c: &CovUtxo) -> lottery::state::VaultState {
    let State::Vault(v) = lottery::web::verify::decode_state(lottery::index::Role::Vault, &web::convert::unhex("v", &c.state).unwrap()).unwrap() else { unreachable!() };
    v
}

fn position_state(c: &CovUtxo) -> PositionState {
    let State::Position(p) = lottery::web::verify::decode_state(lottery::index::Role::Position, &web::convert::unhex("p", &c.state).unwrap()).unwrap() else { unreachable!() };
    p
}

fn code<T: std::fmt::Debug>(r: lottery::web::Result<T>) -> Code {
    r.expect_err("refused").code
}

fn n(s: &str) -> i64 {
    s.parse().unwrap()
}

fn view_input(vault: CovUtxo, owner: Option<String>) -> VaultViewInput {
    let d = deposit();
    VaultViewInput { covenant_id: d.covenant_id, vault, positions: vec![], requests: vec![], records: vec![], owner, virtual_daa_score: "3000000".into() }
}

#[test]
fn the_vault_view_comes_from_the_vault() {
    let d = deposit();
    let cov = d.covenant_id.clone();
    // The vault alone (no wallet): every figure from its state and the rules.
    let v = vault_state(&d.vault);
    let daa: u64 = d.vault.utxo.block_daa_score.parse().unwrap();
    for (now, open) in [(daa + VAULT_REQ_DELAY as u64, false), (daa + VAULT_REQ_DELAY as u64 + 1, true)] {
        let x = vault_view(N, &VaultViewInput { virtual_daa_score: now.to_string(), ..view_input(d.vault.clone(), None) }).unwrap();
        assert_eq!(x.vault.request_open, open, "the node takes a lock time below its DAA score");
        assert_eq!(x.vault.request_from, (daa + VAULT_REQ_DELAY as u64).to_string());
    }
    let x = vault_view(N, &view_input(d.vault.clone(), None)).unwrap();
    let s = &x.vault;
    assert_eq!((n(&s.round), n(&s.next_draw)), (v.round, v.round + 1));
    assert_eq!((n(&s.total_assets), n(&s.total_shares), n(&s.liability)), (v.total_assets, v.total_shares, v.liability));
    assert_eq!((n(&s.pending_deposits), n(&s.pending_withdrawals)), (v.pend_dep, v.pend_wsh));
    assert_eq!(n(&s.reserve) as u64, d.vault.utxo.amount.parse::<u64>().unwrap() - (v.total_assets + v.pend_dep) as u64);
    assert_eq!(n(&s.min_deposit), MIN_DEPOSIT);
    assert_eq!(n(&s.w_min), game::w_min(v.total_assets, v.total_shares));
    assert_eq!(n(&s.w_min_worth), lottery::vault::position_worth(n(&s.w_min), v.total_assets, v.total_shares));
    // (A + VA) / (S + VS) = (10^15 + 1) / (10^15 + 1000): just below 1 sompi per share.
    assert_eq!(s.share_price, "0.99999999");
    assert_eq!(s.source.transaction_id, d.vault.utxo.transaction_id);
    println!("vault: price {} sompi/share, wmin {} shares ({} sompi), next request from DAA {}", s.share_price, s.w_min, s.w_min_worth, s.request_from);

    // Positions: the owner's are listed with the W4b range; another owner's are left out.
    let wp = withdraw("request_withdraw 0 partial");
    let p = position_state(&wp.position);
    let owner = addr(&Payout::from_fields(p.owner_class, &p.owner_key));
    let other = live_utxo(&cov, &State::Position(PositionState::new(1, p.shares, &Payout::Schnorr([0x55; 32]))), 0x56, 0, lottery::params::TEST_RESERVE);
    let ecdsa = withdraw("request_withdraw 1 partial").position;
    let ecdsa_owner = { let e = position_state(&ecdsa); addr(&Payout::from_fields(e.owner_class, &e.owner_key)) };
    let x = vault_view(N, &VaultViewInput { positions: vec![wp.position.clone(), other.clone()], ..view_input(wp.vault.clone(), Some(owner.clone())) }).unwrap();
    assert_eq!((x.positions.len(), x.foreign), (1, 1), "another owner's position is not shown as the wallet's");
    let pc = &x.positions[0];
    assert_eq!((pc.owner.as_str(), n(&pc.shares)), (owner.as_str(), p.shares));
    assert_eq!(pc.owner_key.as_deref(), Some(web::convert::hex(&p.owner_key[..32]).as_str()));
    let pr = pc.partial.as_ref().expect("a 5 x 10^10-share position can be withdrawn in part");
    let wv = vault_state(&wp.vault);
    for (w, ok) in [(n(&pr.least), true), (n(&pr.most), true), (n(&pr.least) - 1, false), (n(&pr.most) + 1, false), (p.shares, true)] {
        assert_eq!(game::w4b_ok(wv.total_assets, wv.total_shares, p.shares, w), ok, "w = {w}");
    }
    // An ECDSA owner's position: listed (it is the owner's) but not withdrawable from the page.
    let x = vault_view(N, &VaultViewInput { positions: vec![ecdsa], ..view_input(wp.vault.clone(), Some(ecdsa_owner)) }).unwrap();
    assert_eq!(x.positions[0].owner_key, None, "the page's signers sign Schnorr P2PK only");
    // A small position (below 2 wmin): only the whole.
    let small = live_utxo(&cov, &State::Position(PositionState::new(1, 2 * n(&pr.least) - 1, &Payout::from_fields(p.owner_class, &p.owner_key))), 0x57, 0, lottery::params::TEST_RESERVE);
    let x = vault_view(N, &VaultViewInput { positions: vec![small], ..view_input(wp.vault.clone(), Some(owner.clone())) }).unwrap();
    assert!(x.positions[0].partial.is_none());

    // Requests: pending before their draw; settle due with their record after it; unknown without it.
    let me = wallet_owner();
    let my = addr(&me);
    let (_, si) = vault_settles().into_iter().find(|(n, _)| n == "settle withdrawal pay").unwrap();
    let pending = live_utxo(&cov, &State::Request(RequestState::new(v.round + 1, KIND_DEPOSIT, 10 * MIN_DEPOSIT, &me)), 0x58, 0, lottery::params::TEST_RESERVE);
    let theirs = live_utxo(&cov, &State::Request(RequestState::new(2, KIND_WITHDRAW, 5, &Payout::Schnorr([0x55; 32]))), 0x59, 0, lottery::params::TEST_RESERVE);
    let at2 = vault_at(&cov, 2);
    let full = VaultViewInput { requests: vec![si.request.clone(), theirs.clone()], records: vec![si.record.clone()], ..view_input(at2.clone(), Some(my.clone())) };
    let x = vault_view(N, &full).unwrap();
    assert_eq!((x.requests.len(), x.foreign), (1, 1));
    let rq = &x.requests[0];
    assert_eq!((rq.kind, rq.status, rq.round.as_str()), (RequestKindName::Withdrawal, RequestStatus::SettleDue, "2"));
    let sv = rq.settle.as_ref().unwrap();
    assert_eq!(sv.mode, SettleModeName::Pay);
    assert!(n(&sv.converted) > 0 && n(&sv.left) > 0 && n(&sv.payout) >= game::P_MIN, "{sv:?}");
    assert_eq!(rq.record.as_ref(), Some(&si.record));
    let x = vault_view(N, &VaultViewInput { records: vec![], ..full.clone() }).unwrap();
    assert_eq!((x.requests[0].status, x.requests[0].settle.is_none()), (RequestStatus::Unknown, true), "no record given: not settle due");
    let x = vault_view(N, &VaultViewInput { requests: vec![pending], records: vec![], ..view_input(d.vault.clone(), Some(my.clone())) }).unwrap();
    assert_eq!((x.requests[0].status, x.requests[0].round.clone()), (RequestStatus::Pending, (v.round + 1).to_string()));
    // Refusals: the same object twice, two live records of one round, objects without an owner, a
    // vault of another instance, a request whose state is not its UTXO's.
    assert_eq!(code(vault_view(N, &VaultViewInput { requests: vec![si.request.clone(), si.request.clone()], ..full.clone() })), Code::InputRange);
    let mut rec2 = si.record.clone();
    rec2.utxo.transaction_id = web::convert::hex(&[0x73; 32]);
    assert_eq!(code(vault_view(N, &VaultViewInput { records: vec![si.record.clone(), rec2], ..full.clone() })), Code::ChainMismatch);
    assert_eq!(code(vault_view(N, &VaultViewInput { owner: None, ..full.clone() })), Code::InputRange);
    let mut lie = si.request.clone();
    lie.state = theirs.state.clone();
    assert_eq!(code(vault_view(N, &VaultViewInput { requests: vec![lie], ..full.clone() })), Code::ChainMismatch);
    let mut alien = at2.clone();
    alien.utxo.covenant_id = Some(web::convert::hex(&[0x99; 32]));
    assert_eq!(code(vault_view(N, &view_input(alien, None))), Code::ChainMismatch);
}

#[test]
fn withdraw_choices_are_what_the_vault_takes() {
    let wp = withdraw("request_withdraw 0 partial");
    let v = vault_state(&wp.vault);
    let p = position_state(&wp.position);
    let wm = game::w_min(v.total_assets, v.total_shares);
    let choice = |amount: Option<u64>| withdraw_choice(N, &WithdrawChoiceInput { covenant_id: wp.covenant_id.clone(), vault: wp.vault.clone(), position: wp.position.clone(), amount: amount.map(|a| a.to_string()) });
    let whole = choice(None).unwrap();
    assert!(whole.whole && n(&whole.shares) == p.shares && whole.remainder == "0");
    // An amount takes the shares worth at most that much today: floor(a (S + VS) / (A + VA)).
    let shares_for = |a: u64| ((a as u128 * (v.total_shares as u128 + 1_000)) / (v.total_assets as u128 + 1)) as i64;
    // The view's Least and Most (F6 part A review S1): the exact bounds, no slack.
    let p_owner = addr(&Payout::from_fields(p.owner_class, &p.owner_key));
    let view = vault_view(N, &VaultViewInput { positions: vec![wp.position.clone()], ..view_input(wp.vault.clone(), Some(p_owner)) }).unwrap();
    let pr = view.positions[0].partial.clone().expect("a part range");
    let (lo, hi) = (n(&pr.least_amount) as u64, n(&pr.most_amount) as u64);
    for (a, ok) in [
        (20 * MIN_DEPOSIT as u64, true),
        (lo, true),
        (lo - 1, false),
        (hi, true),
        (hi + 1, false),
        (5 * lottery::buy::KAS, false),
        (lottery::vault::position_worth(p.shares - wm + 2, v.total_assets, v.total_shares) as u64, false),
        (lottery::vault::position_worth(p.shares, v.total_assets, v.total_shares) as u64 + 10 * lottery::buy::KAS, false),
        (0, false),
    ] {
        let w = shares_for(a);
        let r = choice(Some(a));
        let want = w >= 1 && w <= p.shares && game::w4b_ok(v.total_assets, v.total_shares, p.shares, w);
        assert_eq!(want, ok, "fixture expectation for {a}");
        match r {
            Ok(c) => {
                assert!(ok, "{a}: accepted {c:?}");
                assert_eq!(n(&c.shares), w);
                assert_eq!(n(&c.remainder), p.shares - w);
                // What the choice gives, the builder takes.
                let b = api::build_request_withdraw(N, &WithdrawInput { shares: c.shares.clone(), ..wp.clone() }).unwrap();
                assert!(!b.id.is_empty());
            }
            Err(e) => {
                assert!(!ok, "{a}: refused {e}");
                assert_eq!(e.code, Code::VaultAmount, "{a}: {e}");
            }
        }
    }
}

#[test]
fn deposit_figures_are_bound_to_the_transaction() {
    let d = deposit();
    let built = api::build_request_deposit(N, &d).unwrap();
    let fi = |f: &dyn Fn(&mut DepositFiguresInput)| {
        let mut i = DepositFiguresInput { covenant_id: d.covenant_id.clone(), vault: d.vault.clone(), amount: d.amount.clone(), owner: d.owner.clone(), built: built.clone() };
        f(&mut i);
        deposit_figures(N, &i)
    };
    let f = fi(&|_| {}).unwrap();
    let v = vault_state(&d.vault);
    let r = d.vault.utxo.amount.parse::<u64>().unwrap() - (v.total_assets + v.pend_dep) as u64;
    assert_eq!((n(&f.round), f.amount.as_str(), n(&f.reserve) as u64), (v.round + 1, d.amount.as_str(), r));
    assert_eq!(n(&f.total), n(&f.amount) + n(&f.reserve) + n(&built.fee), "deposit + R + fee");
    assert_eq!(n(&built.summary.wallet_in) - n(&built.summary.wallet_back), n(&f.total), "what the wallet's coins lose");
    assert!(!f.revocable && f.refundable);
    assert_eq!((f.request.index, f.vault_next.index), (1, 0));
    assert_eq!(n(&f.request_from) as u64, d.vault.utxo.block_daa_score.parse::<u64>().unwrap() + VAULT_REQ_DELAY as u64);
    println!("deposit {}: R {} fee {} total {} (round {})", f.amount, f.reserve, f.fee, f.total, f.round);
    // Every field that fixes the request must be the build's.
    let me = addr(&wallet_owner());
    assert_eq!(code(fi(&|i| i.amount = (n(&d.amount) + 1).to_string())), Code::BuiltMismatch, "another amount");
    assert_eq!(code(fi(&|i| i.owner = me.clone())), Code::BuiltMismatch, "another owner");
    assert_eq!(code(fi(&|i| i.vault = vault_at(&d.covenant_id, 1))), Code::BuiltMismatch, "another vault");
    assert_eq!(code(fi(&|i| i.amount = (MIN_DEPOSIT - 1).to_string())), Code::VaultAmount, "below MIN_DEPOSIT");
    let ecdsa = addr(&Payout::Ecdsa([2; 33]));
    assert_eq!(code(fi(&|i| i.owner = ecdsa.clone())), Code::OwnerType);
    // The transaction edited after the build: refused before any figure (the summary is rebuilt).
    let mut t: serde_json::Value = serde_json::from_str(&built.transaction).unwrap();
    let o = &mut t["outputs"][1]["value"];
    *o = serde_json::json!(o.as_u64().map(|x| x + 1).or_else(|| o.as_str().map(|s| s.parse::<u64>().unwrap() + 1)).unwrap().to_string());
    let edited = BuiltTx { transaction: t.to_string(), ..built.clone() };
    let e = deposit_figures(N, &DepositFiguresInput { covenant_id: d.covenant_id.clone(), vault: d.vault.clone(), amount: d.amount.clone(), owner: d.owner.clone(), built: edited }).unwrap_err();
    assert!(matches!(e.code, Code::BuiltMismatch | Code::InputFormat), "{e}");
    // A built withdrawal is not a deposit.
    let w = api::build_request_withdraw(N, &withdraw("request_withdraw 0 whole")).unwrap();
    assert_eq!(code(fi(&|i| i.built = w.clone())), Code::BuiltMismatch);
}

#[test]
fn withdraw_figures_are_bound_to_the_transaction() {
    for name in ["request_withdraw 0 partial", "request_withdraw 0 whole"] {
        let wi = withdraw(name);
        let built = api::build_request_withdraw(N, &wi).unwrap();
        let fi = |f: &dyn Fn(&mut WithdrawFiguresInput)| {
            let mut i = WithdrawFiguresInput { covenant_id: wi.covenant_id.clone(), vault: wi.vault.clone(), position: wi.position.clone(), shares: wi.shares.clone(), built: built.clone() };
            f(&mut i);
            withdraw_figures(N, &i)
        };
        let f = fi(&|_| {}).unwrap();
        let p = position_state(&wi.position);
        let partial = n(&wi.shares) < p.shares;
        assert_eq!((f.whole, n(&f.position_shares), n(&f.remainder)), (!partial, p.shares, p.shares - n(&wi.shares)), "{name}");
        let r = if partial { lottery::params::TEST_RESERVE as i64 } else { 0 };
        assert_eq!((n(&f.reserve), n(&f.total)), (r, n(&built.fee) + r), "{name}: fee (+ R for a partial)");
        assert_eq!(n(&built.summary.wallet_in) - n(&built.summary.wallet_back), n(&f.total), "{name}");
        assert_eq!(f.position_next.is_some(), partial, "{name}");
        assert!(!f.revocable);
        println!("{name}: {} of {} shares, worth {} today, R {} fee {} total {}", f.shares, f.position_shares, f.worth, f.reserve, f.fee, f.total);
        assert_eq!(code(fi(&|i| i.shares = (n(&wi.shares) - 1).to_string())), if partial { Code::BuiltMismatch } else { Code::VaultAmount }, "{name}: other shares");
        let other = live_utxo(&wi.covenant_id, &State::Position(PositionState { shares: p.shares * 2, ..p.clone() }), 0x5a, 0, lottery::params::TEST_RESERVE);
        assert_eq!(code(fi(&|i| i.position = other.clone())), Code::BuiltMismatch, "{name}: another position");
        assert_eq!(code(fi(&|i| i.vault = vault_at(&wi.covenant_id, 7))), Code::BuiltMismatch, "{name}: another vault");
    }
    // W4b on the figures too: a share count the vault does not take is refused before anything else.
    let wi = withdraw("request_withdraw 0 partial");
    let v = vault_state(&wi.vault);
    let built = api::build_request_withdraw(N, &wi).unwrap();
    let wm = game::w_min(v.total_assets, v.total_shares);
    let e = withdraw_figures(N, &WithdrawFiguresInput { covenant_id: wi.covenant_id.clone(), vault: wi.vault.clone(), position: wi.position.clone(), shares: (wm - 1).to_string(), built }).unwrap_err();
    assert_eq!(e.code, Code::VaultAmount);
    // The builder refuses the same (D86).
    assert_eq!(code(api::build_request_withdraw(N, &WithdrawInput { shares: (wm - 1).to_string(), ..wi.clone() })), Code::Policy);
}

fn settle_inputs() -> Vec<(String, SettleInput)> {
    let mut v = vec![("settle deposit".to_string(), settle("settle deposit")), ("settle withdrawal".to_string(), settle("settle withdrawal"))];
    v.extend(vault_settles());
    v
}

#[test]
fn settle_figures_are_bound_to_the_transaction() {
    let all = settle_inputs();
    for (name, si) in &all {
        let built = api::build_settle(N, si).unwrap_or_else(|e| panic!("{name}: {e}"));
        let fi = |req: &CovUtxo, rec: &CovUtxo| settle_figures(N, &SettleFiguresInput { covenant_id: si.covenant_id.clone(), request: req.clone(), record: rec.clone(), built: built.clone() });
        let f = fi(&si.request, &si.record).unwrap_or_else(|e| panic!("{name}: {e}"));
        // The wallet loses the fee less what comes back to it (its payout, the record's reserve).
        let back = if f.payout_to_wallet { n(&f.settle.payout) } else { 0 } + if f.record_refund_to_wallet { n(&f.settle.record_refund) } else { 0 };
        assert_eq!(n(&f.total), n(&f.fee) - back, "{name}");
        assert_eq!(n(&built.summary.wallet_in) - n(&built.summary.wallet_back), n(&f.total), "{name}");
        println!("{name}: {:?} shares {} payout {} left {} accrued {} drained {} (refund {}), fee {}, total {}", f.settle.mode, f.settle.shares, f.settle.payout, f.settle.left, f.settle.accrued, f.settle.drained, f.settle.record_refund, f.fee, f.total);
        // Another request or record than the build's: refused.
        for (other, o) in all.iter().filter(|(o, _)| o != name) {
            if o.request != si.request {
                assert!(fi(&o.request, &si.record).is_err(), "{name} with {other}'s request");
            }
            if o.record != si.record {
                assert!(fi(&si.request, &o.record).is_err(), "{name} with {other}'s record");
            }
        }
    }
}

#[test]
fn every_settlement_mode_settles_from_the_page() {
    use web::quick_wallet::quick_wallet_sign;
    let mut modes = vec![];
    for (name, si) in settle_inputs() {
        let built = api::build_settle(N, &si).unwrap();
        let f = settle_figures(N, &SettleFiguresInput { covenant_id: si.covenant_id.clone(), request: si.request.clone(), record: si.record.clone(), built: built.clone() }).unwrap();
        // The figures are the covenant reference's (game::settle).
        let unhex = |s: &str| web::convert::unhex("s", s).unwrap();
        let State::Request(rq) = lottery::web::verify::decode_state(lottery::index::Role::Request, &unhex(&si.request.state)).unwrap() else { unreachable!() };
        let State::Record(rc) = lottery::web::verify::decode_state(lottery::index::Role::Record, &unhex(&si.record.state)).unwrap() else { unreachable!() };
        let g = game::settle(&rc, &rq).expect("the reference settles it");
        assert_eq!(SettleModeName::from(g.mode), f.settle.mode, "{name}");
        assert_eq!((n(&f.settle.shares), n(&f.settle.converted), n(&f.settle.left), f.settle.drained), (g.sh, g.conv, g.left, g.drained), "{name}");
        let value: i64 = si.request.utxo.amount.parse().unwrap();
        assert_eq!(n(&f.settle.payout), g.payout(rq.amount, value).unwrap_or(0), "{name}: payout");
        assert_eq!(f.position_next.is_some(), f.settle.mode == SettleModeName::Convert, "{name}");
        assert_eq!(f.request_next.is_some(), matches!(f.settle.mode, SettleModeName::Pay | SettleModeName::Accrue), "{name}");
        assert_eq!(f.record_next.is_none(), f.settle.drained, "{name}");
        // The wallet signs its coins only; assembled with every input run in the engine.
        let signed = quick_wallet_sign(N, &QuickSignInput { built: built.clone(), secret_key: web::convert::hex(&lottery::web::fixtures::WALLET_SK) }).unwrap();
        let s = api::assemble(N, &AssembleInput { built: built.clone(), signed }).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(s.id, built.id);
        modes.push((f.settle.mode, f.settle.drained));
    }
    for m in [SettleModeName::Convert, SettleModeName::Refund, SettleModeName::Final, SettleModeName::Pay, SettleModeName::Accrue] {
        assert!(modes.iter().any(|x| x.0 == m), "{m:?} not exercised");
    }
    assert!(modes.iter().any(|x| x.1) && modes.iter().any(|x| !x.1), "drained and not drained");
}

/// A test position (A, S, shares, pending withdrawal shares) on the fixture vault's other fields.
fn part_case(cov: &str, i: usize, a: i64, s: i64, shares: i64, pend_wsh: i64) -> (CovUtxo, CovUtxo, VaultPage) {
    let t = lottery::buy::Templates::get(lottery::buy::b_max());
    let base = lottery::vault::fixture_vault(&t);
    let me = wallet_owner();
    let vs = lottery::state::VaultState { total_assets: a, total_shares: s, pend_dep: 0, pend_wsh, liability: 0, ..base };
    let v = live_utxo(cov, &State::Vault(vs), 0x70, 0, a as u64 + lottery::params::TEST_RESERVE);
    let p = live_utxo(cov, &State::Position(PositionState::new(1, shares, &Payout::from_fields(me.class(), &me.key33()))), 0x71 ^ (i as u8), 1, lottery::params::TEST_RESERVE);
    let view = vault_view(N, &VaultViewInput { covenant_id: cov.into(), vault: v.clone(), positions: vec![p.clone()], requests: vec![], records: vec![], owner: Some(addr(&me)), virtual_daa_score: "3000000".into() }).unwrap();
    (v, p, view)
}

#[test]
fn least_and_most_are_amounts_the_vault_takes() {
    let cov = deposit().covenant_id;
    // (A, S, position shares, pending withdrawal shares)
    let mut cases: Vec<(i64, i64, i64, i64)> = vec![
        (2_500_000_000, 2_500_000_000_000, 2_500_000_000_000, 0), // the e2e's first deposit (price 0.001)
        (3_500_000_000, 3_400_000_000_000, 2_500_000_000_000, 0),
        (1_000_000_000_000_000, 1_000_000_000_000_000, 50_000_000_000, 0), // the fixture vault
    ];
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut rnd = |m: u64| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x % m
    };
    for _ in 0..400 {
        let a = 1_000_000_000 + rnd(100_000_000_000_000) as i64;
        let k = 1 + rnd(3000) as i64;
        let s = (a / 1000) * k + rnd(1000) as i64;
        let wm = game::w_min(a, s);
        // Mostly wide ranges, some within a few shares of 2 wmin (narrower than 0.01 KAS).
        let shares = if rnd(4) == 0 { 2 * wm + rnd(4) as i64 } else { 2 * wm + rnd((8 * wm) as u64) as i64 };
        if shares > s {
            continue;
        }
        cases.push((a, s, shares, 0));
    }
    let take = |v: &CovUtxo, p: &CovUtxo, a: u64| withdraw_choice(N, &WithdrawChoiceInput { covenant_id: cov.clone(), vault: v.clone(), position: p.clone(), amount: Some(a.to_string()) });
    let (mut ranges, mut rounded, mut none) = (0, 0, 0);
    for (i, (a, s, shares, pend)) in cases.into_iter().enumerate() {
        let (v, p, view) = part_case(&cov, i, a, s, shares, pend);
        let Some(pr) = view.positions[0].partial.clone() else {
            none += 1;
            continue;
        };
        ranges += 1;
        let (lo, hi, ls, ms) = (n(&pr.least_amount) as u64, n(&pr.most_amount) as u64, n(&pr.least_shown) as u64, n(&pr.most_shown) as u64);
        let ctx = format!("A {a} S {s} shares {shares} pend {pend}: {pr:?}");
        assert!(lo <= ls && ls <= ms && ms <= hi, "{ctx}");
        if ls % SHOWN_STEP == 0 && ms % SHOWN_STEP == 0 {
            rounded += 1;
        } else {
            assert_eq!((ls, ms), (lo, hi), "shown is rounded or exact: {ctx}");
        }
        for amount in [lo, hi, ls, ms] {
            let c = take(&v, &p, amount).unwrap_or_else(|e| panic!("{amount} refused: {e} ({ctx})"));
            assert!(!c.whole && n(&c.shares) >= n(&pr.least) && n(&c.shares) <= n(&pr.most), "{ctx}");
        }
        for amount in [lo - 1, hi + 1] {
            assert_eq!(take(&v, &p, amount).map(|c| c.shares).unwrap_err().code, Code::VaultAmount, "{amount}: {ctx}");
        }
    }
    println!("part ranges {ranges} (shown at 0.01 KAS: {rounded}, exact: {}), none {none}", ranges - rounded);
    assert!(ranges > 300 && rounded > 0 && rounded < ranges, "both kinds of shown range exercised");
}

#[test]
fn vault_builders_cap_the_fee_rate() {
    let d = deposit();
    let w = withdraw("request_withdraw 0 partial");
    let s = settle("settle deposit");
    for rate in [1000u32, 1001, u32::MAX] {
        let dr = api::build_request_deposit(N, &DepositInput { funds: Funds { fee_rate: rate, ..d.funds.clone() }, ..d.clone() });
        let wr = api::build_request_withdraw(N, &WithdrawInput { funds: Funds { fee_rate: rate, ..w.funds.clone() }, ..w.clone() });
        let sr = api::build_settle(N, &SettleInput { funds: Funds { fee_rate: rate, ..s.funds.clone() }, ..s.clone() });
        for r in [&dr, &wr, &sr] {
            if rate > 1000 {
                assert_eq!(r.as_ref().unwrap_err().code, Code::FeeRate, "rate {rate}");
            } else {
                assert!(r.is_ok(), "rate {rate}");
            }
        }
    }
}

/// N2: a self-consistent build (summary, id and masses recomputed by `built_tx`) whose lock time is
/// not the vault's creation DAA + VAULT_REQ_DELAY is refused by the figures (the check's control).
#[test]
fn a_consistent_build_with_another_lock_time_is_refused() {
    let d = deposit();
    let built = api::build_request_deposit(N, &d).unwrap();
    let df = |b: BuiltTx| deposit_figures(N, &DepositFiguresInput { covenant_id: d.covenant_id.clone(), vault: d.vault.clone(), amount: d.amount.clone(), owner: d.owner.clone(), built: b });
    assert!(df(built.clone()).is_ok());
    let relock = |b: &BuiltTx, delta: i64| {
        let mut u = api::unsigned_of(N, b).unwrap();
        u.tx.lock_time = (u.tx.lock_time as i64 + delta) as u64;
        u.tx.finalize();
        api::built_tx(N, &u).unwrap()
    };
    // + 1: the covenant takes a later lock time (lower-bound checks only), so only the page's check
    // refuses it; - 1: the engine refuses it already (vault D10).
    assert_eq!(code(df(relock(&built, 1))), Code::BuiltMismatch, "deposit lock time + 1");
    assert!(df(relock(&built, -1)).is_err(), "deposit lock time - 1");
    for name in ["request_withdraw 0 partial", "request_withdraw 0 whole"] {
        let wi = withdraw(name);
        let built = api::build_request_withdraw(N, &wi).unwrap();
        let wf = |b: BuiltTx| withdraw_figures(N, &WithdrawFiguresInput { covenant_id: wi.covenant_id.clone(), vault: wi.vault.clone(), position: wi.position.clone(), shares: wi.shares.clone(), built: b });
        assert!(wf(built.clone()).is_ok(), "{name}");
        assert_eq!(code(wf(relock(&built, 1))), Code::BuiltMismatch, "{name}: lock time + 1");
        assert!(wf(relock(&built, -1)).is_err(), "{name}: lock time - 1");
    }
}
