//! F5 review regressions (`covenant-reviewer` on F5; the reviewer's probes, turned into tests).
//! - S1: the page's `draw()` and void absorb expired pools (`absorbablePools`): the chain-object
//!   path is byte-identical to the native builder's draw with the same absorbed set; the caller
//!   panel's figures count them and stay bound to the transaction.
//! - S3: a settlement record alone gives no checked status and no checked m.
//! - S4: the close and draw transaction IDs are bound to the live objects only they create, and
//!   any transaction ID or hash of a round answer is 64 hex digits.
//! - S5: a void needs no purchases (the same transaction with or without them).
//!
//! Run: `cargo test -j 4 -p web --release --test review_f5 -- --nocapture --test-threads=1`

mod common;

use common::{Input, RATE, drawn, equivalent, input, live_utxo, with_fee};
use lottery::index::State;
use lottery::state::{PoolState, RecordState};
use lottery::web::Code;
use lottery::web::fixtures::{Call, draw_calls, run};
use web::convert::{Net, hex, obj};
use web::dto::*;
use web::indexer::*;
use web::rounds::{AbsorbInput, CallerFiguresInput, CallerKind, VerifyKind, VerifyResult, absorbable_pools, caller_figures, verify_round};

const N: Net = Net::Testnet10;

fn api_obj(role: &str, c: &CovUtxo) -> ApiObject {
    ApiObject {
        role: serde_json::from_value(serde_json::json!(role)).unwrap(),
        transaction_id: c.utxo.transaction_id.clone(),
        index: c.utxo.index,
        amount: c.utxo.amount.clone(),
        block_daa_score: c.utxo.block_daa_score.clone(),
        script_public_key: c.utxo.script_public_key.clone(),
        state: c.state.clone(),
    }
}

fn draw_input(call: &Call) -> DrawInput {
    match input(call) {
        Input::Draw(d) => d,
        _ => unreachable!(),
    }
}

fn find(name: &str) -> Call {
    draw_calls().unwrap().into_iter().find(|(n, _, _)| n == name).map(|(_, c, _)| c).unwrap()
}

fn figures_of(di: &DrawInput, built: &BuiltTx) -> CallerFiguresInput {
    CallerFiguresInput {
        covenant_id: di.covenant_id.clone(),
        controller: di.controller.clone(),
        shards: vec![],
        marker: Some(di.marker.clone()),
        drawer: Some(di.drawer.clone()),
        vault: Some(di.vault.clone()),
        absorbed: di.absorbed.clone(),
        built: built.clone(),
    }
}

fn cov_hash(c: &CovUtxo) -> kaspa_hashes::Hash {
    kaspa_hashes::Hash::from_bytes(web::convert::hash32("cov", c.utxo.covenant_id.as_deref().unwrap()).unwrap())
}

// ------------------------------------------------------------------------------ S1

/// S1: a draw or void absorbing expired pools, from chain objects (`web::cases`, as the page
/// builds it), is the native builder's transaction for the same case and absorbed set; the API's
/// build (fee from the rate) equals the library's; the caller figures count the absorbed pools
/// and the new pools, and refuse an absorbed list that is not the transaction's.
#[test]
fn s1_absorbing_draws_are_the_native_builders() {
    let all = draw_calls().unwrap();
    assert_eq!(all.len(), 5);
    for (name, call, native) in all {
        // The chain-object path against the native builder (`web::build::draw` of the case).
        let got = run(&call).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(got.tx, native.tx, "{name}: the chain-object path is the native builder's transaction");
        assert_eq!(got.tx.id(), native.tx.id());
        assert_eq!(got.entries, native.entries);
        // The API's build against the library's (the caller's change address funds the reserves).
        let di = draw_input(&call);
        let b = web::api::build_draw(N, &di).unwrap_or_else(|e| panic!("{name}: api: {e}"));
        let want = lottery::web::build::with_fee(RATE as u64, |fee| run(&with_fee(&equivalent(&call), fee))).unwrap();
        let u = web::api::unsigned_of(N, &b).unwrap();
        assert_eq!(u.tx, want.tx, "{name}: api = library");
        let Call::Draw { absorbed, .. } = &call else { unreachable!() };
        for (n, a) in absorbed.iter().enumerate() {
            assert_eq!(u.tx.inputs[4 + n].previous_outpoint, a.outpoint, "{name}: absorbed pool {n} at input {}", 4 + n);
        }
        // The caller figures.
        let f = caller_figures(N, &figures_of(&di, &b)).unwrap_or_else(|e| panic!("{name}: figures: {e}"));
        println!("{name}: absorbed {} pools {} record {} reserves {} back {:?} lock {} fee {}", f.absorbed, f.pools, f.record, f.reserves, f.reserve_back_from, u.tx.lock_time, f.fee);
        let void = name.starts_with("void");
        assert_eq!(f.kind, if void { CallerKind::Void } else { CallerKind::Draw }, "{name}");
        assert_eq!(f.absorbed as usize, absorbed.len(), "{name}");
        assert_eq!(f.reserves, f.pools + f.record as u32);
        assert_eq!(f.record, name.ends_with("record"), "{name}: the record's reserve");
        assert_eq!(f.reserve_back_from.is_some(), f.pools > 0);
        if !void {
            assert!(f.pools >= 2, "{name}: the draw pays a jackpot holder and second-prize holders");
        } else {
            assert_eq!(f.pools, 0);
        }
        if !absorbed.is_empty() {
            // The absorbed list left out, or in another order: refused (bound to the inputs).
            let mut none = figures_of(&di, &b);
            none.absorbed.clear();
            assert_eq!(caller_figures(N, &none).unwrap_err().code, Code::BuiltMismatch, "{name}: absorbed pools not given");
            if absorbed.len() > 1 {
                let mut swapped = figures_of(&di, &b);
                swapped.absorbed.swap(0, 1);
                assert_eq!(caller_figures(N, &swapped).unwrap_err().code, Code::BuiltMismatch, "{name}: absorbed pools out of order");
            }
        }
        // The vault not given: refused (the record's reserve is not known without it).
        let mut no_vault = figures_of(&di, &b);
        no_vault.vault = None;
        assert_eq!(caller_figures(N, &no_vault).unwrap_err().code, Code::BuiltMismatch);
    }
}

/// S1: `absorbablePools` keeps only expired pools of this instance and of an earlier round that
/// the node holds, with the controller's R, oldest deadline first, at most A_MAX; for a draw only
/// those below the void lock. The indexer's word decides nothing: a candidate the node does not
/// hold, or whose state does not rebuild its script, is skipped.
#[test]
fn s1_absorbable_pools_are_checked_and_ordered() {
    let call = find("draw absorbing A_MAX");
    let di = draw_input(&call);
    let cov = di.covenant_id.clone();
    let covh = cov_hash(&di.marker);
    let mk = lottery::web::cases::marker(&obj("m", &di.marker).unwrap(), &covh).unwrap();
    let ct = lottery::web::cases::controller(&obj("c", &di.controller).unwrap(), &covh).unwrap();
    let dr = lottery::web::cases::drawer(&obj("d", &di.drawer).unwrap(), &covh).unwrap();
    let d = lottery::game::anchor_daa(di.marker.utxo.block_daa_score.parse().unwrap(), ct.seed_offset);
    let void_from = lottery::game::void_lock(d, dr.draw_window).unwrap();
    let base: PoolState = lottery::web::cases::pool(&obj("p", &di.absorbed[0]).unwrap(), &covh).unwrap();
    // Nine pools of round 1 with deadlines 1..=9 below D, in reverse order; one of round 2 (the
    // marker's: not absorbable), one with another R, one at the void lock.
    let mut cands: Vec<(CovUtxo, &str)> = vec![];
    for n in 0..9u8 {
        let p = PoolState { deadline: d - 1_000 + n as i64, tier: 2 + n % 5, ..base.clone() };
        cands.push((live_utxo(&cov, &State::Pool(p), 0x40 + (8 - n), 0, 5_000_000_000), "expired"));
    }
    cands.push((live_utxo(&cov, &State::Pool(PoolState { round: mk.round, deadline: d - 5_000, ..base.clone() }), 0x60, 0, 5_000_000_000), "same round"));
    cands.push((live_utxo(&cov, &State::Pool(PoolState { reserve: base.reserve + 1, deadline: d - 6_000, ..base.clone() }), 0x61, 0, 5_000_000_000), "other R"));
    cands.push((live_utxo(&cov, &State::Pool(PoolState { deadline: void_from, ..base.clone() }), 0x62, 0, 5_000_000_000), "at the void lock"));
    let objects: Vec<ApiObject> = cands.iter().map(|(c, _)| api_obj("pool", c)).collect();
    let utxos: Vec<Utxo> = cands.iter().map(|(c, _)| c.utxo.clone()).collect();
    let input = |void: bool, now: i64, objects: Vec<ApiObject>, utxos: Vec<Utxo>| AbsorbInput {
        covenant_id: cov.clone(),
        marker: di.marker.clone(),
        controller: di.controller.clone(),
        drawer: di.drawer.clone(),
        void,
        virtual_daa_score: now.to_string(),
        objects,
        utxos,
    };
    // A draw at the void lock: the nine expired pools, oldest first, A_MAX of them.
    let a = absorbable_pools(N, &input(false, void_from, objects.clone(), utxos.clone())).unwrap();
    let deadlines: Vec<i64> = a.pools.iter().map(|p| lottery::web::cases::pool(&obj("p", p).unwrap(), &covh).unwrap().deadline).collect();
    println!("draw: {} pools, deadlines {deadlines:?}, more {}, skipped {}", a.pools.len(), a.more, a.skipped);
    assert_eq!(a.pools.len(), lottery::game::A_MAX);
    assert_eq!(deadlines, (0..7).map(|n| d - 1_000 + n).collect::<Vec<_>>(), "oldest deadline first");
    assert_eq!((a.more, a.skipped), (2, 3), "two expired left for a later draw; same round, other R and the void-lock pool skipped");
    // A draw never takes a pool at or past the void lock, even with the node's clock past it (its lock time
    // max(D, deadlines) would read as a void's).
    let late = absorbable_pools(N, &input(false, void_from + 10, objects.clone(), utxos.clone())).unwrap();
    assert_eq!((late.pools.len(), late.more, late.skipped), (7, 2, 3));
    // A void after the void lock also takes the pool at the void lock (its lock time is past it).
    let v = absorbable_pools(N, &input(true, void_from + 1, objects.clone(), utxos.clone())).unwrap();
    assert_eq!((v.pools.len(), v.more, v.skipped), (7, 3, 2));
    // Before a deadline: not absorbed (the lock time would be in the future).
    let early = absorbable_pools(N, &input(false, d - 996, objects.clone(), utxos.clone())).unwrap();
    assert_eq!(early.pools.len(), 4, "deadlines d-1000..d-997 are below d-996");
    // Spent (no node entry): skipped. A state that does not rebuild the node's script: skipped.
    let spent = absorbable_pools(N, &input(false, void_from, objects.clone(), utxos[1..].to_vec())).unwrap();
    assert_eq!((spent.pools.len(), spent.more, spent.skipped), (7, 1, 4));
    let mut lie = objects.clone();
    let mut p0 = lottery::web::cases::pool(&obj("p", &cands[8].0).unwrap(), &covh).unwrap();
    p0.slice += 1;
    lie[8].state = hex(&lottery::web::verify::state_bytes(&State::Pool(p0)).unwrap());
    let l = absorbable_pools(N, &input(false, void_from, lie, utxos.clone())).unwrap();
    assert_eq!((l.pools.len(), l.more, l.skipped), (7, 1, 4), "the edited candidate is skipped, never absorbed");
    // Negative control: the same candidates for the marker's own round's controller swapped are refused.
    let mut wrong = input(false, void_from, objects, utxos);
    wrong.controller = di.marker.clone();
    assert!(absorbable_pools(N, &wrong).is_err());
}

// ------------------------------------------------------------------------------ S3

fn record_of(round: i64) -> RecordState {
    let mut r = RecordState { round, request_tpl: [0; 32], refund: 0, dep_rem: 0, mint_rem: 0, wsh_rem: 0, burn_rem: 0, out_rem: 0, funder_class: 0, funder_key: [0; 33] };
    r.set_funder(&lottery::state::Payout::Schnorr(lottery::params::xonly(&lottery::buy::FUNDING_SK)));
    r
}

fn bare_round(r: i64, status: RoundStatus, objects: Vec<ApiObject>, draw: Option<ApiDraw>) -> ApiRound {
    ApiRound {
        round: r.to_string(),
        status,
        m: Some(9),
        k: 4,
        deadline: None,
        anchor_daa_score: None,
        close: None,
        draw,
        buys: 0,
        combinations: 0,
        objects,
        incomplete: None,
        gaps: vec![],
    }
}

/// S3 (probe 1): a settlement record alone (a void with pending requests leaves one too) lets the
/// indexer say "drawn" or "void"; neither is a checked status, m is not the chain's, and Verify's
/// Status item is not checkable. A record and a non-closed status contradict each other.
#[test]
fn s3_a_record_alone_is_no_checked_status() {
    let x = drawn();
    let cov = hex(&x.cov.as_bytes());
    let pool = lottery::web::cases::pool(&obj("p", &x.pools[0].0).unwrap(), &x.cov).unwrap();
    let r = pool.round;
    let rec = live_utxo(&cov, &State::Record(record_of(r)), 0x77, 5, 169_000_000);
    let objects = vec![api_obj("record", &rec), api_obj("drawer", &x.drawer)];
    let utxos = vec![rec.utxo.clone(), x.drawer.utxo.clone()];
    let lie = ApiDraw {
        transaction_id: x.drawer.utxo.transaction_id.clone(),
        void: false,
        winning: Some(Combination { main: [1, 2, 3, 4, 5], bonus: 1 }),
        anchor_hash: "cc".repeat(32),
        claim_deadline: "5".into(),
        pools: vec![],
    };
    let i = RoundCheckInput { covenant_id: cov.clone(), round: bare_round(r, RoundStatus::Drawn, objects.clone(), Some(lie)), utxos: utxos.clone(), purchases: None, anchor_proof: None, anchor_block: None };
    let c = check_round(N, &i).expect("passes");
    println!("record alone, drawn: status {:?} checked {} m {:?} m checked {}", c.status, c.status_checked, c.m, c.m_checked);
    assert!(!c.status_checked && !c.m_checked && !c.void_supported);
    let v = verify_round(N, &i).unwrap();
    assert_eq!(v.items.iter().find(|x| x.kind == VerifyKind::Status).unwrap().result, VerifyResult::NotCheckable, "Verify's Status item does not match on a record alone");
    // The same objects said void: a supported void (F4 rule), still not a checked status.
    let void = ApiDraw { transaction_id: x.drawer.utxo.transaction_id.clone(), void: true, winning: None, anchor_hash: "00".repeat(32), claim_deadline: "0".into(), pools: vec![] };
    let j = RoundCheckInput { round: bare_round(r, RoundStatus::Void, objects.clone(), Some(void)), ..i.clone() };
    let c2 = check_round(N, &j).unwrap();
    assert!(!c2.status_checked && c2.void_supported);
    // A record and "closing" or "open": refused.
    for st in [RoundStatus::Closing, RoundStatus::Open] {
        let k = RoundCheckInput { round: bare_round(r, st, objects.clone(), None), ..i.clone() };
        assert_eq!(check_round(N, &k).unwrap_err().code, Code::ChainMismatch, "{st:?}");
    }
    // Control: with a live pool of the round the status and m are the chain's.
    let honest = ApiRound {
        m: Some(pool.m as u32),
        k: pool.k as u32,
        draw: None,
        objects: vec![api_obj("pool", &x.pools[0].0), api_obj("drawer", &x.drawer)],
        ..bare_round(r, RoundStatus::Drawn, vec![], None)
    };
    let h = RoundCheckInput { round: honest, utxos: vec![x.pools[0].0.utxo.clone(), x.drawer.utxo.clone()], ..i.clone() };
    let c3 = check_round(N, &h).unwrap();
    assert!(c3.status_checked && c3.m_checked);
}

// ------------------------------------------------------------------------------ S4

/// S4 (probe 2): the draw's transaction ID is bound to the drawer of round r + 1 (only that draw
/// creates it); another ID is refused; with only a pool live (a claim re-creates pools in its own
/// transaction) nothing binds it. The close's ID is bound to the round's marker.
#[test]
fn s4_close_and_draw_ids_are_bound_to_their_objects() {
    let x = drawn();
    let cov = hex(&x.cov.as_bytes());
    let pool = lottery::web::cases::pool(&obj("p", &x.pools[0].0).unwrap(), &x.cov).unwrap();
    let win = Combination { main: [pool.win[0], pool.win[1], pool.win[2], pool.win[3], pool.win[4]], bonus: pool.win[5] };
    let roots: Vec<String> = pool.roots.chunks_exact(32).take(pool.k as usize).map(hex).collect();
    let draw_id = x.drawer.utxo.transaction_id.clone();
    assert_eq!(draw_id, x.pools[0].0.utxo.transaction_id, "the fixture's pools and drawer come from the one draw");
    let round = |draw_tx: &str, objects: Vec<ApiObject>| ApiRound {
        round: pool.round.to_string(),
        status: RoundStatus::Drawn,
        m: Some(pool.m as u32),
        k: pool.k as u32,
        deadline: None,
        anchor_daa_score: None,
        close: Some(ApiClose { transaction_id: "ab".repeat(32), marker_daa_score: "1".into(), frozen_roots: roots.clone(), m_next: 3 }),
        draw: Some(ApiDraw { transaction_id: draw_tx.into(), void: false, winning: Some(win), anchor_hash: "ef".repeat(32), claim_deadline: pool.deadline.to_string(), pools: vec![] }),
        buys: 0,
        combinations: 0,
        objects,
        incomplete: None,
        gaps: vec![],
    };
    let with_drawer = vec![api_obj("pool", &x.pools[0].0), api_obj("drawer", &x.drawer)];
    let utxos = vec![x.pools[0].0.utxo.clone(), x.drawer.utxo.clone()];
    let ok = check_round(N, &RoundCheckInput { covenant_id: cov.clone(), round: round(&draw_id, with_drawer.clone()), utxos: utxos.clone(), purchases: None, anchor_proof: None, anchor_block: None }).unwrap();
    assert_eq!(ok.facts.draw_tx.as_ref().map(|t| t.transaction_id.as_str()), Some(draw_id.as_str()));
    assert!(ok.facts.close_tx.is_none(), "after the draw no live object fixes the close");
    let lie = check_round(N, &RoundCheckInput { covenant_id: cov.clone(), round: round(&"cd".repeat(32), with_drawer), utxos: utxos.clone(), purchases: None, anchor_proof: None, anchor_block: None });
    assert_eq!(lie.unwrap_err().code, Code::ChainMismatch, "another draw ID with the drawer live: refused");
    // A pool alone: the ID is not bound (shown unverified), and not refused.
    let pool_only = check_round(N, &RoundCheckInput { covenant_id: cov.clone(), round: round(&"cd".repeat(32), vec![api_obj("pool", &x.pools[0].0)]), utxos: vec![x.pools[0].0.utxo.clone()], purchases: None, anchor_proof: None, anchor_block: None }).unwrap();
    assert!(pool_only.facts.draw_tx.is_none());
    // Closing: the close's ID is the marker's transaction.
    let closing = |close_tx: &str| ApiRound {
        status: RoundStatus::Closing,
        draw: None,
        close: Some(ApiClose { transaction_id: close_tx.into(), marker_daa_score: x.marker.utxo.block_daa_score.clone(), frozen_roots: roots.clone(), m_next: 3 }),
        objects: vec![api_obj("marker", &x.marker), api_obj("controller", &x.controller), api_obj("drawer", &x.drawer_before)],
        ..round(&draw_id, vec![])
    };
    let cu = vec![x.marker.utxo.clone(), x.controller.utxo.clone(), x.drawer_before.utxo.clone()];
    let mk_tx = x.marker.utxo.transaction_id.clone();
    let c = check_round(N, &RoundCheckInput { covenant_id: cov.clone(), round: closing(&mk_tx), utxos: cu.clone(), purchases: None, anchor_proof: None, anchor_block: None }).unwrap();
    assert_eq!(c.facts.close_tx.map(|t| t.transaction_id), Some(mk_tx));
    let bad = check_round(N, &RoundCheckInput { covenant_id: cov, round: closing(&"ab".repeat(32)), utxos: cu, purchases: None, anchor_proof: None, anchor_block: None });
    assert_eq!(bad.unwrap_err().code, Code::ChainMismatch);
}

/// S4 note: every transaction ID and hash of a round answer is 64 hex digits.
#[test]
fn s4_round_answers_carry_64_hex_ids() {
    let x = drawn();
    let good = ApiRound { objects: vec![api_obj("drawer", &x.drawer)], ..bare_round(3, RoundStatus::Open, vec![], None) };
    let text = serde_json::to_string(&good).unwrap();
    assert!(parse_round(&text).is_ok());
    for (what, f) in [
        ("short object id", Box::new(|r: &mut ApiRound| r.objects[0].transaction_id = "ab".into()) as Box<dyn Fn(&mut ApiRound)>),
        ("non-hex close id", Box::new(|r: &mut ApiRound| r.close = Some(ApiClose { transaction_id: "zz".repeat(32), marker_daa_score: "1".into(), frozen_roots: vec![], m_next: 2 }))),
        ("long draw id", Box::new(|r: &mut ApiRound| r.draw = Some(ApiDraw { transaction_id: "ab".repeat(33), void: true, winning: None, anchor_hash: "00".repeat(32), claim_deadline: "0".into(), pools: vec![] }))),
        ("short anchor hash", Box::new(|r: &mut ApiRound| r.draw = Some(ApiDraw { transaction_id: "ab".repeat(32), void: true, winning: None, anchor_hash: "00".into(), claim_deadline: "0".into(), pools: vec![] }))),
    ] {
        let mut r = good.clone();
        f(&mut r);
        let e = parse_round(&serde_json::to_string(&r).unwrap()).unwrap_err();
        assert_eq!(e.code, Code::InputFormat, "{what}");
    }
}

// ------------------------------------------------------------------------------ S5

/// S5 (probe 3, now with real purchases): a void built with no purchases is the same
/// transaction as with them (a void uses no leaves, siblings or counts); a draw still needs
/// them, and purchases given for a void are still checked against the marker's roots.
#[test]
fn s5_a_void_needs_no_purchases() {
    for name in ["void with purchases", "void absorbing 2"] {
        let di = draw_input(&find(name));
        assert!(di.purchases.iter().any(|p| !p.is_empty()), "{name}: the round has purchases");
        let honest = web::api::build_draw(N, &di).unwrap();
        let none = web::api::build_draw(N, &DrawInput { purchases: vec![], ..di.clone() }).unwrap();
        assert_eq!((none.transaction.clone(), none.id.clone()), (honest.transaction.clone(), honest.id.clone()), "{name}: the same transaction");
        // Purchases given but wrong: refused (checked as before).
        let mut wrong = di.clone();
        wrong.purchases = vec![String::new(); di.purchases.len()];
        assert_eq!(web::api::build_draw(N, &wrong).unwrap_err().code, Code::TreeMismatch, "{name}: wrong purchases for a void");
    }
    // A draw without purchases: refused.
    let di = draw_input(&find("draw absorbing 3"));
    assert_eq!(web::api::build_draw(N, &DrawInput { purchases: vec![], ..di }).unwrap_err().code, Code::StoreIncomplete);
}
