//! Front end F5 (S3 Rounds, J6 verify, S4 Run the round): the money and verification paths of
//! `crates/web` this stage adds. Each guards what the page shows as checked or what it signs:
//! - `after_the_draw_a_forged_anchor_hash_is_not_shown_as_checked` (the F5 item recorded in F2/F4:
//!   after the draw `anchorHash` was the proof's only): a 32-byte value ground to give the pool's
//!   numbers passes the drawn path's numbers check, so on its own it must never read as bound; a
//!   public node's anchor block binds X.hash, and refuses the forged one;
//! - `the_round_facts_come_from_the_chain_objects`: D, the claim deadline, the void lock, the roots,
//!   the reward and R of `checkIndexerRound`'s facts against the fixtures' own cases;
//! - `caller_figures_are_bound_to_the_built_transaction`: close, draw (with and without winners) and
//!   void: the reward, the fee, the reserves and the total, each from the transaction and the chain
//!   objects; the kind from the lock time; objects that are not the transaction's inputs refused.
//!
//! Run: `cargo test -j 4 -p web --release --test f5_rounds -- --nocapture`.

mod common;

use common::{Input, RATE, addr, drawn, input};
use lottery::game;
use lottery::state::Payout;
use lottery::web::Code;
use lottery::web::fixtures::{Call, calls};
use web::api;
use web::convert::{Net, hex, obj};
use web::dto::*;
use web::rounds::{CallerFigures, CallerFiguresInput, CallerKind, caller_figures};

const N: Net = Net::Testnet10;

fn find(name: &str) -> (Call, Input) {
    let (_, c, _) = calls().unwrap().into_iter().find(|(n, _, _)| n == name).unwrap_or_else(|| panic!("no fixture {name}"));
    let i = input(&c);
    (c, i)
}

fn cov_of(c: &CovUtxo) -> kaspa_hashes::Hash {
    kaspa_hashes::Hash::from_bytes(web::convert::hash32("cov", c.utxo.covenant_id.as_deref().unwrap()).unwrap())
}

#[test]
fn after_the_draw_a_forged_anchor_hash_is_not_shown_as_checked() {
    let x = drawn();
    let cov = hex(&x.cov.as_bytes());
    let pool = lottery::web::cases::pool(&obj("pool", &x.pools[0].0).unwrap(), &x.cov).unwrap();
    let from = |b: Option<AnchorBlock>| RoundObjs::Drawn { pool: x.pools[0].0.clone(), drawer: x.drawer.clone(), anchor_block: b };
    let win = |b: Option<AnchorBlock>, proof: &[u8]| api::winning_numbers_bound(&WinningInput { covenant_id: cov.clone(), from: from(b), anchor_proof: hex(proof) });

    // The honest proof: final numbers; X.hash bound only with the node's anchor block.
    let (w, bound) = win(None, &x.proof).unwrap();
    assert_eq!((w.status, bound), (NumbersStatus::Final, false), "without the node's block X.hash is the proof's only");
    let (w2, bound) = win(Some(x.anchor_block.clone()), &x.proof).unwrap();
    assert!(bound, "the node's header for X.hash binds it");
    assert_eq!(w2.anchor_hash, w.anchor_hash);

    // A forger grinds a 32-byte value giving the pool's numbers (about 1 in N x m tries; no
    // proof of work: nothing but the numbers depends on it once the draw is done).
    let mut h = [0u8; 32];
    let mut tries = 0u64;
    loop {
        tries += 1;
        h[..8].copy_from_slice(&tries.to_le_bytes());
        let s = game::seed_numbers(&x.cov.as_bytes(), pool.round, &h, pool.m);
        if [s.main[0], s.main[1], s.main[2], s.main[3], s.main[4], s.bonus] == pool.win {
            break;
        }
        assert!(tries < 60_000_000, "no forged hash found");
    }
    println!("forged X.hash with the pool's numbers after {tries} tries");
    let mut forged = x.proof.clone();
    forged[..32].copy_from_slice(&h);
    // Its numbers are the pool's: on the numbers alone it passes, with the forged hash. So the
    // page must not show X.hash as checked from this alone (`bound` false).
    let (wf, bound) = win(None, &forged).unwrap();
    assert_eq!((wf.status, bound, wf.anchor_hash.clone()), (NumbersStatus::Final, false, hex(&h)));
    assert_eq!(wf.winning, w.winning);
    // The node has no block with the forged hash (the page then shows the hash not checkable);
    // X's real header does not bind it: anchor_invalid.
    let e = win(Some(x.anchor_block.clone()), &forged).unwrap_err();
    assert_eq!(e.code, Code::AnchorInvalid, "{}", e.detail);
    // A block the node does not report on its selected chain does not bind X either.
    let mut off = x.anchor_block.clone();
    off.is_chain_block = false;
    assert_eq!(win(Some(off), &x.proof).unwrap_err().code, Code::AnchorInvalid);
    // A header of another block (another nonce) does not hash to X.hash.
    let mut other = x.anchor_block.clone();
    other.header.nonce = (other.header.nonce.parse::<u64>().unwrap() + 1).to_string();
    assert_eq!(win(Some(other), &x.proof).unwrap_err().code, Code::AnchorInvalid);
    // The page input as JSON: `anchorBlock` is optional on the drawn path.
    let j = serde_json::json!({"covenantId": cov, "from": {"drawn": {"pool": x.pools[0].0, "drawer": x.drawer}}, "anchorProof": hex(&x.proof)});
    assert!(serde_json::from_value::<WinningInput>(j).is_ok());
}

#[test]
fn the_round_facts_come_from_the_chain_objects() {
    use web::indexer::*;
    let x = drawn();
    let cov = hex(&x.cov.as_bytes());
    let mk = lottery::web::cases::marker(&obj("m", &x.marker).unwrap(), &x.cov).unwrap();
    let ct = lottery::web::cases::controller(&obj("c", &x.controller).unwrap(), &x.cov).unwrap();
    let dr = lottery::web::cases::drawer(&obj("d", &x.drawer_before).unwrap(), &x.cov).unwrap();
    let api_obj = |role: &str, c: &CovUtxo| ApiObject {
        role: serde_json::from_value(serde_json::json!(role)).unwrap(),
        transaction_id: c.utxo.transaction_id.clone(),
        index: c.utxo.index,
        amount: c.utxo.amount.clone(),
        block_daa_score: c.utxo.block_daa_score.clone(),
        script_public_key: c.utxo.script_public_key.clone(),
        state: c.state.clone(),
    };
    let d = game::anchor_daa(x.marker.utxo.block_daa_score.parse().unwrap(), ct.seed_offset);
    let roots: Vec<String> = mk.roots.chunks_exact(32).take(mk.k as usize).map(hex).collect();
    // Closing: the marker, the next controller and the drawer.
    let closing = ApiRound {
        round: mk.round.to_string(),
        status: RoundStatus::Closing,
        m: Some(mk.m as u32),
        k: mk.k as u32,
        deadline: None,
        anchor_daa_score: Some(d.to_string()),
        close: None,
        draw: None,
        buys: 0,
        combinations: 0,
        objects: vec![api_obj("marker", &x.marker), api_obj("controller", &x.controller), api_obj("drawer", &x.drawer_before)],
        incomplete: None,
        gaps: vec![],
    };
    let utxos = vec![x.marker.utxo.clone(), x.controller.utxo.clone(), x.drawer_before.utxo.clone()];
    let ap = ApiAnchorProof { round: mk.round.to_string(), anchor_proof: hex(&x.proof), source: ProofSource::Node };
    let i = RoundCheckInput { covenant_id: cov.clone(), round: closing.clone(), utxos: utxos.clone(), purchases: None, anchor_proof: Some(ap.clone()), anchor_block: Some(x.anchor_block.clone()) };
    let c = check_round(N, &i).unwrap();
    let f = &c.facts;
    assert!(c.anchor_bound, "the closed path binds X.hash");
    assert_eq!(f.d.as_ref().unwrap().value, d.to_string());
    assert_eq!(f.d.as_ref().unwrap().sources.len(), 2, "D from the marker and the next controller");
    assert_eq!(f.void_from.as_ref().unwrap().value, game::void_lock(d, dr.draw_window).unwrap().to_string());
    assert_eq!(f.claim_deadline.as_ref().unwrap().value, game::claim_deadline(d, dr.draw_window, dr.claim_window).unwrap().to_string());
    assert_eq!(f.sales.as_ref().unwrap().value, mk.sales.to_string());
    assert_eq!(f.jackpot.as_ref().unwrap().value, mk.jackpot.to_string());
    assert_eq!(f.reward.as_ref().unwrap().value, game::draw_amounts(&mk).c_draw.to_string(), "C_draw from the marker");
    assert_eq!(f.reserve.as_ref().unwrap().value, ct.reserve.to_string());
    assert_eq!(f.roots.as_ref().unwrap(), &roots);
    assert_eq!(c.winning.as_ref().unwrap().d, d.to_string());
    // Without the drawer: no void lock and no claim deadline (nothing fixes them).
    let mut j = i.clone();
    j.round.objects.pop();
    let c2 = check_round(N, &j).unwrap();
    assert!(c2.facts.void_from.is_none() && c2.facts.claim_deadline.is_none() && c2.facts.d.is_some());

    // Drawn: a pool and the drawer that drew it; D from the pool's claim deadline.
    let pool = lottery::web::cases::pool(&obj("p", &x.pools[0].0).unwrap(), &x.cov).unwrap();
    let drawn_round = ApiRound {
        status: RoundStatus::Drawn,
        anchor_daa_score: Some(d.to_string()),
        objects: vec![api_obj("pool", &x.pools[0].0), api_obj("drawer", &x.drawer)],
        draw: Some(ApiDraw {
            transaction_id: x.pools[0].0.utxo.transaction_id.clone(),
            void: false,
            winning: Some(Combination { main: [pool.win[0], pool.win[1], pool.win[2], pool.win[3], pool.win[4]], bonus: pool.win[5] }),
            anchor_hash: hex(&x.proof[..32]),
            claim_deadline: pool.deadline.to_string(),
            pools: vec![],
        }),
        ..closing.clone()
    };
    let utxos = vec![x.pools[0].0.utxo.clone(), x.drawer.utxo.clone()];
    let mut i = RoundCheckInput { covenant_id: cov.clone(), round: drawn_round, utxos, purchases: None, anchor_proof: Some(ApiAnchorProof { source: ProofSource::Draw, ..ap }), anchor_block: None };
    let c = check_round(N, &i).unwrap();
    assert_eq!((c.status, c.status_checked, c.anchor_bound), (RoundStatus::Drawn, true, false), "drawn without the node's block: X.hash not bound");
    assert_eq!(c.facts.d.as_ref().unwrap().value, d.to_string());
    assert_eq!(c.facts.claim_deadline.as_ref().unwrap().value, pool.deadline.to_string());
    assert!(c.facts.sales.is_none() && c.facts.reward.is_none(), "no live object of a drawn round holds S");
    i.anchor_block = Some(x.anchor_block.clone());
    let c = check_round(N, &i).unwrap();
    assert!(c.anchor_bound, "drawn with the node's block: X.hash bound");
    // The verify items: every one a match for this honest round.
    let v = web::rounds::verify_round(N, &i).unwrap();
    println!("{:#?}", v.items.iter().map(|x| (x.kind, x.result)).collect::<Vec<_>>());
    assert!(v.consistent && v.anchor_bound);
    assert!(v.items.iter().all(|x| x.result == web::rounds::VerifyResult::Match), "{:?}", v.items);
    // The same round with the anchor block of another header: the anchor item is the mismatch,
    // the numbers still match.
    let mut other = x.anchor_block.clone();
    other.header.nonce = (other.header.nonce.parse::<u64>().unwrap() + 1).to_string();
    i.anchor_block = Some(other);
    let v = web::rounds::verify_round(N, &i).unwrap();
    let res = |k: web::rounds::VerifyKind| v.items.iter().find(|x| x.kind == k).unwrap().result;
    assert_eq!(res(web::rounds::VerifyKind::Numbers), web::rounds::VerifyResult::Match);
    assert_eq!(res(web::rounds::VerifyKind::Anchor), web::rounds::VerifyResult::Mismatch);
    assert!(!v.consistent && !v.anchor_bound);
}

fn figures(i: &CallerFiguresInput) -> lottery::web::Result<CallerFigures> {
    caller_figures(N, i)
}

fn check_total(f: &CallerFigures) {
    let n = |s: &str| s.parse::<i128>().unwrap();
    assert_eq!(n(&f.total), n(&f.reserve) + n(&f.fee) - n(&f.reward) - n(&f.refund), "the rows add up to the total");
    assert_eq!(f.fee_above_reward, n(&f.fee) > n(&f.reward));
}

#[test]
fn caller_figures_are_bound_to_the_built_transaction() {
    // close(): one reserve (the marker's R), C_close from the controller and the k shards.
    let (call, inp) = find("close k=4");
    let Input::Close(ci) = &inp else { unreachable!() };
    let built = inp.run().unwrap();
    let Call::Close { cov, controller, shards, .. } = &call else { unreachable!() };
    let ct = lottery::web::cases::controller(controller, cov).unwrap();
    let ss: Vec<_> = shards.iter().map(|s| lottery::web::cases::shard(s, cov).unwrap()).collect();
    let sales: i64 = ss.iter().map(|s| s.sales).sum();
    let paid: i64 = ss.iter().map(|s| s.ref_paid).sum();
    let c_close = game::round_amounts(sales, paid, &game::Shares::of(&ct)).c_close;
    let input = CallerFiguresInput { covenant_id: ci.covenant_id.clone(), controller: ci.controller.clone(), shards: ci.shards.clone(), marker: None, drawer: None, vault: None, absorbed: vec![], built: built.clone() };
    let f = figures(&input).unwrap();
    println!("close: {f:?}");
    assert_eq!((f.kind, f.reserves, f.reward.clone(), f.reserve.clone()), (CallerKind::Close, 1, c_close.to_string(), ct.reserve.to_string()));
    assert!(f.reserve_back_with_draw && f.reserve_back_from.is_none());
    assert_eq!(f.fee, built.fee);
    assert_eq!(f.caller_output as usize, built.summary.outputs.len() - 1);
    check_total(&f);
    // The shards of the close missing one: refused (k shards).
    let mut short = input.clone();
    short.shards.pop();
    assert_eq!(figures(&short).unwrap_err().code, Code::BuiltMismatch);
    // A controller that is not the transaction's input: refused.
    let (_, dinp) = find("draw k=4");
    let Input::Draw(di) = &dinp else { unreachable!() };
    let mut wrong = input.clone();
    wrong.controller = di.controller.clone();
    assert!(figures(&wrong).is_err());
    // A built whose summary was edited: refused before any figure (built_mismatch).
    let mut edited = input.clone();
    edited.built.summary.wallet_back = "1".into();
    assert_eq!(figures(&edited).unwrap_err().code, Code::BuiltMismatch);

    // draw() without winners and the void: no pools, so no reserve; C_draw from the marker.
    for (name, kind) in [("draw k=4", CallerKind::Draw), ("void k=4", CallerKind::Void)] {
        let (_, inp) = find(name);
        let Input::Draw(di) = &inp else { unreachable!() };
        let built = inp.run().unwrap();
        let mk = lottery::web::cases::marker(&obj("m", &di.marker).unwrap(), &cov_of(&di.marker)).unwrap();
        let input = CallerFiguresInput { covenant_id: di.covenant_id.clone(), controller: di.controller.clone(), shards: vec![], marker: Some(di.marker.clone()), drawer: Some(di.drawer.clone()), vault: Some(di.vault.clone()), absorbed: vec![], built };
        let f = figures(&input).unwrap();
        println!("{name}: {f:?}");
        assert_eq!((f.kind, f.reserves, f.reward.clone()), (kind, 0, game::draw_amounts(&mk).c_draw.to_string()));
        assert!(!f.reserve_back_with_draw && f.reserve_back_from.is_none());
        check_total(&f);
        // The marker without the drawer: refused.
        let mut half = input.clone();
        half.drawer = None;
        assert_eq!(figures(&half).unwrap_err().code, Code::BuiltMismatch);
    }

    // draw() with winners: one reserve per pool, back from the claim deadline.
    let x = drawn();
    let wallet: Payout = lottery::web::build::payout_of_spk(&lottery::buy::spk_of(&lottery::web::fixtures::WALLET_SK)).unwrap();
    let di = DrawInput {
        covenant_id: hex(&x.cov.as_bytes()),
        drawer: x.drawer_before.clone(),
        marker: x.marker.clone(),
        vault: x.vault.clone(),
        controller: x.controller.clone(),
        absorbed: vec![],
        purchases: x.purchases.iter().map(|p| hex(p)).collect(),
        anchor_proof: Some(hex(&x.proof)),
        funds: Funds { coins: x.coins.clone(), change: addr(&wallet), fee_rate: RATE },
    };
    let built = api::build_draw(N, &di).unwrap();
    let input = CallerFiguresInput { covenant_id: di.covenant_id.clone(), controller: di.controller.clone(), shards: vec![], marker: Some(di.marker.clone()), drawer: Some(di.drawer.clone()), vault: Some(di.vault.clone()), absorbed: vec![], built };
    let f = figures(&input).unwrap();
    println!("draw with winners: {f:?}");
    let pool = lottery::web::cases::pool(&obj("p", &x.pools[0].0).unwrap(), &x.cov).unwrap();
    let ct = lottery::web::cases::controller(&obj("c", &x.controller).unwrap(), &x.cov).unwrap();
    assert_eq!((f.kind, f.reserves as usize), (CallerKind::Draw, x.pools.len()), "one R per pool the draw creates");
    assert_eq!(f.reserve, (x.pools.len() as i64 * ct.reserve).to_string());
    assert_eq!(f.reserve_back_from.as_deref(), Some(pool.deadline.to_string().as_str()), "the pools' R come back from their claim deadline");
    check_total(&f);
}
