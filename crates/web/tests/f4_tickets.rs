//! Front end F4 (spec S2, S8, 6.3, 7 "Claims", "Deposits"): the ticket card, the claim row,
//! the deposit return and the backup file, natively. Written before the page uses them; each
//! guards money or data:
//! - `claim_figures_are_bound_to_the_transaction`: every claim mode (PAY h = 1 and h = 2, ACCRUE
//!   below P_MIN, FINAL with the ticket's value), also with the payout at the wallet's own
//!   address; a figure the transaction does not bear out, another ticket, another index or a
//!   tampered `BuiltTx` is refused.
//! - `ticket_card_shows_what_the_chain_would_take`: the card's prize, h and mode are the claim's;
//!   matched numbers; the deposit date and when `return_deposit()` is valid (both paths); a
//!   drawn round whose pools are gone, a void, an open round.
//! - `return_figures_are_bound_to_the_transaction`: both paths.
//! - `backups_round_trip_and_damaged_or_doctored_files_are_refused`.
//! - `a_kept_copy_is_checked_against_the_live_pool`.

mod common;

use common::{Input, input};
use lottery::web::Code;
use lottery::web::fixtures::{Call, calls, claim_calls};
use web::backup::{self, BackupReadInput, BackupWriteInput, CopyCheckInput, CopyStatus};
use web::convert::{Net, hex};
use web::dto::*;
use web::tickets::{self, CardStage, ClaimFiguresInput, ClaimModeName, ComboStatus, ReturnFiguresInput, ReturnPathName, TicketCardInput};
use web::views::{RoleName, StateView};
use web::webstore::{RoundCopyStage, StoredObject, StoredResult, StoredRound, StoredTicket};

const N: Net = Net::Testnet10;

fn claim_inputs() -> Vec<(String, ClaimInput)> {
    let mut all: Vec<(String, Call)> = calls().unwrap().into_iter().filter(|(n, _, _)| n == "claim k=4").map(|(n, c, _)| (n, c)).collect();
    all.extend(claim_calls().unwrap().into_iter().map(|(n, c, _)| (n, c)));
    all.into_iter()
        .map(|(n, c)| match input(&c) {
            Input::Claim(i) => (n, i),
            _ => unreachable!(),
        })
        .collect()
}

fn pool_view(i: &ClaimInput) -> web::views::PoolView {
    match web::api::decode_state(N, RoleName::Pool, &i.pools[0].state).unwrap() {
        StateView::Pool(p) => p,
        _ => unreachable!(),
    }
}

fn ticket_view(state: &str) -> web::views::TicketView {
    match web::api::decode_state(N, RoleName::Ticket, state).unwrap() {
        StateView::Ticket(t) => t,
        _ => unreachable!(),
    }
}

fn figures_input(i: &ClaimInput, built: &BuiltTx) -> ClaimFiguresInput {
    ClaimFiguresInput { covenant_id: i.covenant_id.clone(), ticket: i.ticket.clone(), pools: i.pools.clone(), purchases: i.purchases.clone(), index: i.index.unwrap(), built: built.clone() }
}

/// The same claim funded from the ticket's own payout address (the player claims their own
/// ticket: the payout output then comes back to the wallet in the summary).
fn self_funded(i: &ClaimInput, card_payout: &str) -> ClaimInput {
    let mut x = i.clone();
    let spk = web::api::script_of(N, card_payout).unwrap();
    for c in &mut x.funds.coins {
        c.script_public_key = spk.clone();
    }
    x.funds.change = card_payout.to_string();
    x
}

#[test]
fn claim_figures_are_bound_to_the_transaction() {
    let mut modes = vec![];
    for (name, i) in claim_inputs() {
        let pool = pool_view(&i);
        let card = tickets::ticket_card(N, &TicketCardInput {
            covenant_id: i.covenant_id.clone(),
            ticket: i.ticket.clone(),
            pools: i.pools.clone(),
            purchases: Some(i.purchases.clone()),
            winning: None,
            claim_deadline: None,
            void: false,
            virtual_daa_score: "1".into(),
        })
        .unwrap_or_else(|e| panic!("{name}: card: {e}"));
        for (label, inp) in [("wallet", i.clone()), ("own payout", self_funded(&i, &card.payout))] {
            let built = web::api::build_claim(N, &inp).unwrap_or_else(|e| panic!("{name} {label}: build: {e}"));
            let f = tickets::claim_figures(N, &figures_input(&inp, &built)).unwrap_or_else(|e| panic!("{name} {label}: figures: {e}"));
            let slice: i64 = pool.slice.parse().unwrap();
            let h: i64 = f.claim.h.parse().unwrap();
            let prize: u64 = f.claim.prize.parse().unwrap();
            assert_eq!(prize as i64, slice / h, "{name}: prize = floor(slice / h)");
            assert_eq!(f.claim.shared_with, (h - 1).to_string());
            assert_eq!(f.claim.slice, pool.slice);
            assert_eq!(f.claim.below_p_min, (prize as i64) < lottery::game::P_MIN);
            let tv: u64 = inp.ticket.utxo.amount.parse().unwrap();
            let fee: u64 = built.fee.parse().unwrap();
            assert_eq!(f.fee, built.fee);
            assert_eq!(f.fee_exceeds_prize, fee > prize, "{name}: the fee warning");
            match f.claim.mode {
                ClaimModeName::Pay => {
                    assert!(prize as i64 >= lottery::game::P_MIN);
                    assert_eq!((f.paid_now.as_str(), f.value_back.as_str(), f.accrued.as_str()), (prize.to_string().as_str(), "0", "0"));
                }
                ClaimModeName::Accrue => {
                    assert!((prize as i64) < lottery::game::P_MIN);
                    assert_eq!((f.paid_now.as_str(), f.accrued.as_str()), ("0", prize.to_string().as_str()));
                }
                ClaimModeName::Final => {
                    assert_eq!(f.paid_now, (prize + tv).to_string(), "{name}: FINAL pays prize + value");
                    assert_eq!(f.value_back, tv.to_string());
                    assert!(f.ticket_next.is_none());
                }
            }
            if let Some(t) = &f.ticket_next {
                let before = ticket_view(&inp.ticket.state);
                let after = ticket_view(&t.state);
                assert_ne!(before.claimed, after.claimed, "{name}: the index is marked claimed");
                assert_eq!(built.summary.outputs[1].address.as_deref(), Some(t.address.as_str()));
            }
            // The card's claim row is the claim the chain takes.
            let row = card.combos[f.index as usize].claim.as_ref().unwrap_or_else(|| panic!("{name}: no claim on the card"));
            assert_eq!(*row, f.claim, "{name}: card row = built claim");
            assert_eq!(f.pool_next.value, built.summary.outputs[0].value);
            modes.push(format!("{name} {label}: {:?} prize {} h {} fee {} paid {}", f.claim.mode, f.claim.prize, f.claim.h, f.fee, f.paid_now));

            // Refusals: every one before anything is shown.
            let mut fee_lie = figures_input(&inp, &built);
            fee_lie.built.fee = (fee + 1).to_string();
            let mut other_index = figures_input(&inp, &built);
            other_index.index = 1 - other_index.index.min(1);
            // Another ticket: another state (the FINAL fixture's ticket has its other index
            // claimed), or the same state at another outpoint.
            let mut other_ticket = figures_input(&inp, &built);
            let other_name = if name == "claim final" { "claim k=4" } else { "claim final" };
            other_ticket.ticket = claim_inputs().into_iter().find(|(n, _)| n == other_name).unwrap().1.ticket;
            let mut other_outpoint = figures_input(&inp, &built);
            other_outpoint.ticket.utxo.index ^= 1;
            let mut out_value = figures_input(&inp, &built);
            out_value.built.summary.outputs[0].value = "1".into();
            for (what, x) in [("a fee changed", fee_lie), ("another ticket", other_ticket), ("the ticket at another outpoint", other_outpoint), ("an output value changed", out_value)] {
                let e = tickets::claim_figures(N, &x).expect_err(&format!("{name}: {what}"));
                assert!(matches!(e.code, Code::BuiltMismatch | Code::ChainMismatch | Code::Refused | Code::InputFormat), "{name}: {what}: {e}");
            }
            assert!(tickets::claim_figures(N, &other_index).is_err(), "{name}: another index");
        }
    }
    println!("{}", modes.join("\n"));
    for m in ["Pay", "Accrue", "Final"] {
        assert!(modes.iter().any(|x| x.contains(&format!(": {m} "))), "mode {m} exercised");
    }
    assert!(modes.iter().any(|x| x.contains(" h 2 ")), "a shared win exercised");
}

#[test]
fn ticket_card_shows_what_the_chain_would_take() {
    let (_, i) = claim_inputs().into_iter().find(|(n, _)| n == "claim k=4").unwrap();
    let pool = pool_view(&i);
    let t = ticket_view(&i.ticket.state);
    let deadline: i64 = pool.deadline.parse().unwrap();
    let alone = t.deadline.parse::<i64>().unwrap() + lottery::game::D_RET;
    let card = |pools: Vec<CovUtxo>, now: i64, winning: Option<Combination>, void: bool| {
        tickets::ticket_card(N, &TicketCardInput {
            covenant_id: i.covenant_id.clone(),
            ticket: i.ticket.clone(),
            pools,
            purchases: Some(i.purchases.clone()),
            winning,
            claim_deadline: None,
            void,
            virtual_daa_score: now.to_string(),
        })
    };
    let c = card(i.pools.clone(), deadline - 10, None, false).unwrap();
    assert_eq!(c.stage, CardStage::Drawn);
    assert_eq!(c.combos[0].status, ComboStatus::Claimable);
    assert_eq!(c.combos[1].status, ComboStatus::NoWin);
    assert_eq!(c.combos[0].tier, Some(pool.tier));
    assert_eq!(c.combos[0].tier_name.as_deref(), Some("4 + bonus"));
    assert_eq!((0..7).map(|t| tickets::tier_name(t).unwrap()).collect::<Vec<_>>(), ["5 + bonus", "5", "4 + bonus", "4", "3 + bonus", "3", "2 + bonus"]);
    assert_eq!(tickets::tier_name(7), None);
    let mm = c.combos[0].matched_main.unwrap();
    assert_eq!(mm.iter().filter(|x| **x).count(), 4, "tier 2: four main numbers match");
    assert_eq!(c.combos[0].bonus_matched, Some(true));
    assert!(c.back_with_last_claim);
    assert_eq!(c.claim_deadline.as_deref(), Some(pool.deadline.as_str()));
    assert_eq!(c.return_from, deadline.min(alone).to_string());
    assert!(c.returnable.is_none(), "before the claim deadline nothing returns the deposit");
    // Past the pool's claim deadline: path a with that pool.
    let c = card(i.pools.clone(), deadline + 1, None, false).unwrap();
    let r = c.returnable.expect("path a");
    assert_eq!(r.path, ReturnPathName::WithPool);
    assert_eq!(r.pool.as_ref(), Some(&i.pools[0]));
    // From T + D_RET: path b alone.
    let c = card(vec![], alone + 1, None, false).unwrap();
    assert_eq!(c.stage, CardStage::Open);
    assert_eq!(c.returnable.map(|r| r.path), Some(ReturnPathName::Alone));
    assert_eq!(c.return_from, alone.to_string());
    // Drawn, pools gone: the winning combination is expired; nothing to claim.
    let win = Combination { main: [3, 7, 12, 19, 28], bonus: 4 };
    let c = card(vec![], deadline + 1, Some(win), false).unwrap();
    assert_eq!(c.combos[0].status, ComboStatus::Expired);
    assert!(!c.back_with_last_claim);
    // Stored numbers that are not the pools' are refused.
    let other = Combination { main: [1, 2, 3, 4, 5], bonus: 1 };
    assert_eq!(card(i.pools.clone(), 1, Some(other), false).unwrap_err().code, Code::ChainMismatch);
    // A void: no winners, the deposit from T + D_RET.
    let c = card(vec![], 1, None, true).unwrap();
    assert_eq!(c.stage, CardStage::Void);
    assert!(c.combos.iter().all(|x| x.status == ComboStatus::NoWin && x.claim.is_none()));
    assert_eq!(c.return_from, alone.to_string());
    // A pool of another instance is refused.
    let mut wrong = i.pools[0].clone();
    wrong.utxo.covenant_id = Some("ab".repeat(32));
    assert!(card(vec![wrong], 1, None, false).is_err());
    // A drawn card needs the lists for h.
    let e = tickets::ticket_card(N, &TicketCardInput {
        covenant_id: i.covenant_id.clone(),
        ticket: i.ticket.clone(),
        pools: i.pools.clone(),
        purchases: None,
        winning: None,
        claim_deadline: None,
        void: false,
        virtual_daa_score: "1".into(),
    })
    .unwrap_err();
    assert_eq!(e.code, Code::StoreIncomplete);
    // Doctored lists (a purchase dropped) are a tree mismatch, never a different prize.
    let mut lists = i.purchases.clone();
    let n = lists.iter().position(|l| l.len() >= 12).unwrap();
    lists[n] = lists[n][12..].to_string();
    let e = tickets::ticket_card(N, &TicketCardInput {
        covenant_id: i.covenant_id.clone(),
        ticket: i.ticket.clone(),
        pools: i.pools.clone(),
        purchases: Some(lists),
        winning: None,
        claim_deadline: None,
        void: false,
        virtual_daa_score: "1".into(),
    })
    .unwrap_err();
    assert_eq!(e.code, Code::TreeMismatch);
}

#[test]
fn return_figures_are_bound_to_the_transaction() {
    for (name, call, _) in calls().unwrap().into_iter().filter(|(n, _, _)| n.starts_with("return")) {
        let Input::Return(i) = input(&call) else { unreachable!() };
        let built = web::api::build_return_deposit(N, &i).unwrap();
        let fi = ReturnFiguresInput { covenant_id: i.covenant_id.clone(), ticket: i.ticket.clone(), pool: i.pool.clone(), built: built.clone() };
        let f = tickets::return_figures(N, &fi).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(f.amount, i.ticket.utxo.amount, "{name}: the ticket's value");
        assert_eq!(f.path, if i.pool.is_some() { ReturnPathName::WithPool } else { ReturnPathName::Alone });
        assert_eq!(f.fee, built.fee);
        println!("{name}: {:?} amount {} fee {} lock {}", f.path, f.amount, f.fee, f.lock_time);
        let mut fee = fi.clone();
        fee.built.fee = "1".into();
        let mut path = fi.clone();
        path.pool = if i.pool.is_some() { None } else { Some(claim_inputs()[0].1.pools[0].clone()) };
        let mut ticket = fi.clone();
        ticket.ticket.utxo.index ^= 1;
        let mut state = fi.clone();
        state.ticket = claim_inputs().into_iter().find(|(n, _)| n == "claim final").unwrap().1.ticket;
        for (what, x) in [("a fee changed", fee), ("another path", path), ("the ticket at another outpoint", ticket), ("another ticket", state)] {
            assert!(tickets::return_figures(N, &x).is_err(), "{name}: {what}");
        }
    }
}

/// A drawn round's copy and a ticket record, from the PAY fixture.
fn records() -> (String, StoredTicket, StoredRound) {
    let (_, i) = claim_inputs().into_iter().find(|(n, _)| n == "claim k=4").unwrap();
    let pool = pool_view(&i);
    let t = ticket_view(&i.ticket.state);
    let cov = i.covenant_id.clone();
    let card = tickets::ticket_card(N, &TicketCardInput {
        covenant_id: cov.clone(),
        ticket: i.ticket.clone(),
        pools: i.pools.clone(),
        purchases: Some(i.purchases.clone()),
        winning: None,
        claim_deadline: None,
        void: false,
        virtual_daa_score: "1".into(),
    })
    .unwrap();
    let ticket = StoredTicket {
        key: format!("{}:1", "ee".repeat(32)),
        covenant_id: cov.clone(),
        network: "testnet-10".into(),
        round: t.round.clone(),
        shard_index: Some(0),
        ticket: i.ticket.clone(),
        bundle: card.combos.iter().map(|c| c.combination).collect(),
        payout: card.payout.clone(),
        buy: None,
        ended: None,
        saved_ms: "1".into(),
    };
    let roots = web::api::check_roots(&RootsInput { covenant_id: cov.clone(), against: i.pools[0].clone(), purchases: i.purchases.clone() }).unwrap();
    let po = StoredObject { role: RoleName::Pool, state: i.pools[0].state.clone(), script_public_key: i.pools[0].utxo.script_public_key.clone() };
    let round = StoredRound {
        key: format!("{cov}:{}", t.round),
        covenant_id: cov.clone(),
        network: "testnet-10".into(),
        round: t.round.clone(),
        m: pool.m.parse().unwrap(),
        k: pool.k.parse().unwrap(),
        stage: RoundCopyStage::Drawn,
        purchases: i.purchases.clone(),
        roots: Some(roots.roots),
        against: Some(po.clone()),
        result: Some(StoredResult { winning: card.winning.unwrap(), claim_deadline: pool.deadline.clone(), pools: vec![po] }),
        checked_ms: "1".into(),
    };
    (cov, ticket, round)
}

fn write(cov: &str, t: &StoredTicket, r: &StoredRound) -> String {
    backup::backup_write(N, &BackupWriteInput { covenant_id: cov.into(), created_ms: "7".into(), tickets: vec![t.clone()], rounds: vec![r.clone()], pending: vec![] }).unwrap()
}

fn read(cov: &str, text: &str) -> lottery::web::Result<backup::BackupContents> {
    backup::backup_read(N, &BackupReadInput { covenant_id: cov.into(), text: text.into() })
}

/// The file with one JSON value replaced and the checksum made to match again (a doctored file:
/// the checksum is no seal).
fn doctor(cov: &str, text: &str, f: impl Fn(&mut serde_json::Value)) -> String {
    let mut v: serde_json::Value = serde_json::from_str(text).unwrap();
    f(&mut v);
    let mut file: backup::BackupFile = serde_json::from_value(v).unwrap();
    file.checksum = String::new();
    let sum = hex(&web::indexer::bundle_hash(serde_json::to_string(&file).unwrap().as_bytes()));
    file.checksum = sum;
    let _ = cov;
    serde_json::to_string(&file).unwrap()
}

#[test]
fn backups_round_trip_and_damaged_or_doctored_files_are_refused() {
    let (cov, t, r) = records();
    let text = write(&cov, &t, &r);
    assert!(!text.contains(&hex(&lottery::web::fixtures::WALLET_SK)), "no key in a backup");
    let back = read(&cov, &text).unwrap();
    assert_eq!(back.tickets, vec![t.clone()]);
    assert_eq!(back.rounds, vec![r.clone()]);
    assert_eq!(back.summary.len(), 1);
    let s = &back.summary[0];
    assert_eq!((s.tickets, s.combinations, s.stage), (1, 2, Some(RoundCopyStage::Drawn)));
    assert!(s.purchases >= 2 && s.winning.is_some());
    println!("summary: {s:?}");

    let mid = text.len() / 2;
    let mut flipped = text.clone().into_bytes();
    // A digit inside a hex string or a number: still JSON, so only the checksum finds it.
    let at = (mid..text.len()).find(|&n| flipped[n].is_ascii_digit() && flipped[n] != b'7').unwrap();
    flipped[at] = b'7';
    let flipped = String::from_utf8(flipped).unwrap();
    let mut sum = serde_json::from_str::<serde_json::Value>(&text).unwrap();
    sum["checksum"] = serde_json::Value::String("00".repeat(32));
    let damaged: Vec<(&str, String)> = vec![
        ("truncated", text[..mid].to_string()),
        ("one digit changed", flipped),
        ("the checksum changed", sum.to_string()),
        ("empty", String::new()),
        ("not JSON", "hello".into()),
        ("too large", " ".repeat(backup::BACKUP_MAX + 1)),
        ("an unknown field", text.replacen('{', "{\"extra\":1,", 1)),
        ("another instance", doctor(&cov, &text, |v| v["covenantId"] = "ab".repeat(32).into())),
        ("another network", doctor(&cov, &text, |v| v["network"] = "simnet".into())),
        ("another format", doctor(&cov, &text, |v| v["format"] = "kaspalionare-backup/9".into())),
        ("a purchase dropped", doctor(&cov, &text, |v| {
            let l = v["rounds"][0]["purchases"].as_array_mut().unwrap();
            let n = l.iter().position(|x| x.as_str().unwrap().len() >= 12).unwrap();
            let s = l[n].as_str().unwrap()[12..].to_string();
            l[n] = s.into();
        })),
        ("a root changed", doctor(&cov, &text, |v| v["rounds"][0]["roots"][0] = "11".repeat(32).into())),
        ("the winning numbers changed", doctor(&cov, &text, |v| v["rounds"][0]["result"]["winning"]["bonus"] = 9.into())),
        ("a ticket's state changed", doctor(&cov, &text, |v| {
            let s = v["tickets"][0]["ticket"]["state"].as_str().unwrap().to_string();
            let mut b = s.into_bytes();
            let n = b.len() - 3;
            b[n] = if b[n] == b'0' { b'1' } else { b'0' };
            v["tickets"][0]["ticket"]["state"] = String::from_utf8(b).unwrap().into();
        })),
        ("a ticket's bundle changed", doctor(&cov, &text, |v| v["tickets"][0]["bundle"][0]["bonus"] = 9.into())),
        ("a ticket's payout changed", doctor(&cov, &text, |v| v["tickets"][0]["payout"] = common::addr(&lottery::state::Payout::Schnorr([0x44; 32])).into())),
        ("a round of another instance", doctor(&cov, &text, |v| v["rounds"][0]["covenantId"] = "ab".repeat(32).into())),
    ];
    for (what, x) in damaged {
        let e = read(&cov, &x).expect_err(what);
        assert_eq!(e.code, Code::BackupInvalid, "{what}: {e}");
        println!("refused ({what}): {}", e.detail);
    }
    // A wrong page instance refuses an honest file too.
    assert_eq!(read(&"ab".repeat(32), &text).unwrap_err().code, Code::BackupInvalid);
    // The writer refuses records it would not read back.
    let mut bad = r.clone();
    bad.purchases[0] = "zz".into();
    assert!(backup::backup_write(N, &BackupWriteInput { covenant_id: cov.clone(), created_ms: "1".into(), tickets: vec![], rounds: vec![bad], pending: vec![] }).is_err());
}

#[test]
fn a_kept_copy_is_checked_against_the_live_pool() {
    let (cov, _, r) = records();
    let (_, i) = claim_inputs().into_iter().find(|(n, _)| n == "claim k=4").unwrap();
    let live = vec![i.pools[0].utxo.clone()];
    let c = backup::check_copy(&CopyCheckInput { covenant_id: cov.clone(), round: r.clone(), live: live.clone() }).unwrap();
    assert_eq!(c.status, CopyStatus::Verified);
    assert_eq!(c.pools, i.pools);
    // The pool gone: nothing left to claim.
    let c = backup::check_copy(&CopyCheckInput { covenant_id: cov.clone(), round: r.clone(), live: vec![] }).unwrap();
    assert_eq!(c.status, CopyStatus::Gone);
    // Lists that do not rebuild the live pool's roots.
    let mut doctored = r.clone();
    let n = doctored.purchases.iter().position(|l| l.len() >= 12).unwrap();
    doctored.purchases[n] = doctored.purchases[n][12..].to_string();
    let e = backup::check_copy(&CopyCheckInput { covenant_id: cov.clone(), round: doctored, live: live.clone() }).unwrap_err();
    assert_eq!(e.code, Code::CopyMismatch, "{e}");
    // A live entry at the pool's script of another instance.
    let mut other = live[0].clone();
    other.covenant_id = Some("ab".repeat(32));
    assert_eq!(backup::check_copy(&CopyCheckInput { covenant_id: cov.clone(), round: r.clone(), live: vec![other] }).unwrap_err().code, Code::CopyMismatch);
    // An open copy is not checked against frozen roots.
    let mut open = r.clone();
    open.stage = RoundCopyStage::Open;
    assert_eq!(backup::check_copy(&CopyCheckInput { covenant_id: cov, round: open, live }).unwrap().status, CopyStatus::Open);
}
