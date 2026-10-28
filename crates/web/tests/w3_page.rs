//! W3 for the page-facing calls added after F1 (F6 security review S2; the reviewer's probe folded in): no call
//! panics on any input it can be given (a panic aborts the wasm, so a lying indexer, a doctored backup file or a
//! damaged store must end in an error code, never a stopped worker). Seeded and deterministic; run it in debug
//! (overflow checks on), as `lottery`'s `f1_w3`:
//!
//!   cargo test -j 4 -p web --test w3_page -- --nocapture --test-threads=1
//!
//! `W3_ITERS=n` multiplies the iterations (default 1,500 mutated inputs per call and case).
//!
//! Each input starts from an honest one (checked to pass first), then one to three leaves of its JSON are replaced
//! by hostile values (empty, long hex, i64/u64 extremes, -1, 0, wrong types, a digit changed, truncated); what
//! still parses as the call's input type is run under `catch_unwind`. Covered: `checkIndexerRound` and
//! `verifyRound` (closing and drawn), every `parseIndexer*` on mutated text, `anchorProofX`, `vaultView`,
//! `withdrawChoice`, `depositFigures`, `withdrawFigures`, `settleFigures`, `callerFigures` (close, draw, void),
//! `absorbablePools`, `claimFigures`, `returnFigures`, `ticketCard`, `backupRead` (mutated files with the
//! checksum recomputed, so the record checks run), `checkCopy`, `mergeCopy`.
//!
//! Regression: an indexer stating round 9223372036854775807 overflowed `r + 1` in `classify` (debug: a panic;
//! the probe hit it 11 times in 25,000 mutations); `a_round_at_i64_max_is_an_error_not_a_panic` pins it.
//! Negative control: `the_harness_counts_panics` (a call that panics is counted by the same harness).

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{Input, drawn, fixture_input, input, vault_at, vault_settles};
use lottery::game;
use lottery::web::fixtures::{calls, claim_calls};
use web::convert::{Net, hex, obj};
use web::dto::*;
use web::indexer::*;

const N: Net = Net::Testnet10;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn iters() -> usize {
    1_500 * std::env::var("W3_ITERS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1)
}

fn paths(v: &serde_json::Value, p: Vec<String>, out: &mut Vec<Vec<String>>) {
    out.push(p.clone());
    match v {
        serde_json::Value::Object(m) => m.iter().for_each(|(k, x)| {
            let mut q = p.clone();
            q.push(k.clone());
            paths(x, q, out)
        }),
        serde_json::Value::Array(a) => a.iter().enumerate().for_each(|(i, x)| {
            let mut q = p.clone();
            q.push(i.to_string());
            paths(x, q, out)
        }),
        _ => {}
    }
}

fn mutate(r: &mut Rng, s: &str) -> serde_json::Value {
    match r.below(16) {
        0 => "".into(),
        1 => "ff".repeat(r.below(80)).into(),
        2 => "18446744073709551615".into(),
        3 => "9223372036854775807".into(),
        4 => "-1".into(),
        5 => "0".into(),
        6 => serde_json::json!(4294967295u64),
        7 => serde_json::json!(0),
        8 => serde_json::json!(255),
        9 => serde_json::json!([]),
        10 => serde_json::json!(null),
        11 => format!("{s}00").into(),
        12 => s.get(..s.len() / 2).unwrap_or("").into(),
        13 => "00".repeat(32).into(),
        14 => serde_json::json!(true),
        _ => {
            let mut b = s.as_bytes().to_vec();
            if !b.is_empty() {
                let k = r.below(b.len());
                b[k] = b"0123456789abcdef"[r.below(16)];
            }
            String::from_utf8_lossy(&b).to_string().into()
        }
    }
}

fn mutated(r: &mut Rng, base: &serde_json::Value, ps: &[Vec<String>]) -> serde_json::Value {
    let mut v = base.clone();
    for _ in 0..1 + r.below(3) {
        let p = &ps[r.below(ps.len())];
        let ptr = if p.is_empty() { String::new() } else { format!("/{}", p.join("/")) };
        let Some(x) = v.pointer_mut(&ptr) else { continue };
        let cur = x.to_string();
        *x = mutate(r, cur.trim_matches('"'));
    }
    v
}

/// Mutate `base` `n` times; run `f` on every mutation that parses as `T`. Returns (parsed, panics).
fn fuzz_n<T: serde::de::DeserializeOwned, R>(name: &str, base: serde_json::Value, n: usize, r: &mut Rng, f: impl Fn(&T) -> R) -> (usize, usize) {
    let mut ps = vec![];
    paths(&base, vec![], &mut ps);
    let (mut parsed, mut panics) = (0, 0);
    for _ in 0..n {
        let v = mutated(r, &base, &ps);
        let Ok(t) = serde_json::from_value::<T>(v.clone()) else { continue };
        parsed += 1;
        if catch_unwind(AssertUnwindSafe(|| f(&t))).is_err() {
            panics += 1;
            let s = v.to_string();
            println!("PANIC {name}: {}", &s[..s.len().min(600)]);
        }
    }
    println!("{name}: {parsed} mutated inputs parsed, {panics} panics");
    (parsed, panics)
}

/// `fuzz_n` with the default iterations; an input type that never parses would test nothing, so at least one
/// mutation must parse.
fn fuzz<T: serde::de::DeserializeOwned, R>(name: &str, base: serde_json::Value, r: &mut Rng, f: impl Fn(&T) -> R) -> usize {
    let (parsed, panics) = fuzz_n(name, base, iters(), r, f);
    assert!(parsed > 0, "{name}: no mutation parsed (the case tests nothing)");
    panics
}

fn text_fuzz(name: &str, base: &str, r: &mut Rng, f: impl Fn(&str)) -> usize {
    let mut panics = 0;
    let alphabet = b"0123456789abcdef{}[],:-";
    for _ in 0..iters() {
        let mut s = base.as_bytes().to_vec();
        let k = r.below(s.len());
        match r.below(4) {
            0 => s.truncate(k),
            1 => s[k] = alphabet[r.below(alphabet.len())],
            2 => s[k] = b'"',
            _ => s.insert(k, b'9'),
        }
        let s = String::from_utf8_lossy(&s).to_string();
        if catch_unwind(AssertUnwindSafe(|| f(&s))).is_err() {
            panics += 1;
            println!("PANIC {name}");
        }
    }
    println!("{name}: {} mutated texts, {panics} panics", iters());
    panics
}

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

/// The honest closing and drawn round inputs of the `drawn()` fixture (both pass `check_round`).
fn round_inputs() -> (RoundCheckInput, RoundCheckInput, ApiAnchorProof) {
    let x = drawn();
    let cov = hex(&x.cov.as_bytes());
    let mk = lottery::web::cases::marker(&obj("m", &x.marker).unwrap(), &x.cov).unwrap();
    let ct = lottery::web::cases::controller(&obj("c", &x.controller).unwrap(), &x.cov).unwrap();
    let d = game::anchor_daa(x.marker.utxo.block_daa_score.parse().unwrap(), ct.seed_offset);
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
    let ap = ApiAnchorProof { round: mk.round.to_string(), anchor_proof: hex(&x.proof), source: ProofSource::Node };
    let i = RoundCheckInput {
        covenant_id: cov.clone(),
        round: closing.clone(),
        utxos: vec![x.marker.utxo.clone(), x.controller.utxo.clone(), x.drawer_before.utxo.clone()],
        purchases: None,
        anchor_proof: Some(ap.clone()),
        anchor_block: Some(x.anchor_block.clone()),
    };
    assert!(check_round(N, &i).is_ok(), "honest closing round");
    let pool = lottery::web::cases::pool(&obj("p", &x.pools[0].0).unwrap(), &x.cov).unwrap();
    let drawn_round = ApiRound {
        status: RoundStatus::Drawn,
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
    let j = RoundCheckInput {
        round: drawn_round,
        utxos: vec![x.pools[0].0.utxo.clone(), x.drawer.utxo.clone()],
        anchor_proof: Some(ApiAnchorProof { source: ProofSource::Draw, ..ap.clone() }),
        ..i.clone()
    };
    assert!(check_round(N, &j).is_ok(), "honest drawn round");
    (i, j, ap)
}

/// Negative control: the harness counts a call that panics (so a zero below means no call panicked, not that
/// panics went unseen).
#[test]
fn the_harness_counts_panics() {
    let (i, _, _) = round_inputs();
    let mut r = Rng(7);
    let (parsed, panics) = fuzz_n::<RoundCheckInput, ()>("control", serde_json::to_value(&i).unwrap(), 20, &mut r, |_| panic!("control"));
    assert!(parsed > 0 && panics == parsed, "every parsed input of a panicking call is counted: {panics} of {parsed}");
}

/// S2 regression: a lying indexer's round number at i64::MAX (and at i64::MIN, and -1) is an error code from both
/// checks, never a panic (debug: overflow checks on).
#[test]
fn a_round_at_i64_max_is_an_error_not_a_panic() {
    let (i, j, _) = round_inputs();
    for base in [&i, &j] {
        for r in ["9223372036854775807", "-9223372036854775808", "-1", "0"] {
            let mut x = base.clone();
            x.round.round = r.into();
            let a = catch_unwind(AssertUnwindSafe(|| check_round(N, &x).map(|_| ())));
            let b = catch_unwind(AssertUnwindSafe(|| web::rounds::verify_round(N, &x).map(|_| ())));
            assert!(a.is_ok() && b.is_ok(), "round {r}: a check panicked");
            assert!(a.unwrap().is_err(), "round {r}: checkIndexerRound accepted another round's objects");
        }
    }
}

#[test]
fn round_checks_and_parsers_never_panic() {
    let mut r = Rng(0x2545_F491_4F6C_DD1D);
    let mut total = 0;
    let (i, j, ap) = round_inputs();
    for (name, inp) in [("closing", &i), ("drawn", &j)] {
        let base = serde_json::to_value(inp).unwrap();
        total += fuzz::<RoundCheckInput, _>(&format!("checkIndexerRound {name}"), base.clone(), &mut r, |t| check_round(N, t).map(|_| ()));
        total += fuzz::<RoundCheckInput, _>(&format!("verifyRound {name}"), base, &mut r, |t| web::rounds::verify_round(N, t).map(|_| ()));
    }
    for (name, round) in [("closing", &i.round), ("drawn", &j.round)] {
        let text = serde_json::to_string(round).unwrap();
        total += text_fuzz(&format!("parseIndexer* {name}"), &text, &mut r, |s| {
            let _ = parse_round(s);
            let _ = parse_rounds(s);
            let _ = parse_objects(s);
            let _ = parse_tickets(s);
            let _ = parse_status(s);
            let _ = parse_error(s);
        });
    }
    let apt = serde_json::to_string(&ap).unwrap();
    total += text_fuzz("parseIndexerAnchorProof", &apt, &mut r, |s| {
        let _ = parse_anchor_proof(s);
    });
    total += text_fuzz("anchorProofX", &ap.anchor_proof, &mut r, |s| {
        let _ = web::rounds::anchor_x(s);
    });
    println!("TOTAL PANICS {total}");
    assert_eq!(total, 0, "a page-facing call panicked (wasm aborts on panic)");
}

#[test]
fn vault_calls_never_panic() {
    use web::vault::*;
    let mut r = Rng(0xD1B5_4A32_D192_ED03);
    let mut total = 0;
    let Input::Deposit(dep) = fixture_input("request_deposit") else { unreachable!() };
    let v = VaultViewInput { covenant_id: dep.covenant_id.clone(), vault: vault_at(&dep.covenant_id, 3), positions: vec![], requests: vec![], records: vec![], owner: None, virtual_daa_score: "3000000".into() };
    total += fuzz::<VaultViewInput, _>("vaultView", serde_json::to_value(&v).unwrap(), &mut r, |t| vault_view(N, t).map(|_| ()));
    let built = web::api::build_request_deposit(N, &dep).unwrap();
    let df = DepositFiguresInput { covenant_id: dep.covenant_id.clone(), vault: dep.vault.clone(), amount: dep.amount.clone(), owner: dep.owner.clone(), built };
    assert!(deposit_figures(N, &df).is_ok(), "honest deposit figures");
    total += fuzz::<DepositFiguresInput, _>("depositFigures", serde_json::to_value(&df).unwrap(), &mut r, |t| deposit_figures(N, t).map(|_| ()));
    for name in ["request_withdraw 0 partial", "request_withdraw 0 whole"] {
        let Input::Withdraw(mut w) = fixture_input(name) else { unreachable!() };
        w.funds.change = web::api::address_of(N, &w.owner.coin.script_public_key).unwrap();
        let wc = WithdrawChoiceInput { covenant_id: w.covenant_id.clone(), vault: w.vault.clone(), position: w.position.clone(), amount: Some("100000000".into()) };
        total += fuzz::<WithdrawChoiceInput, _>(&format!("withdrawChoice {name}"), serde_json::to_value(&wc).unwrap(), &mut r, |t| withdraw_choice(N, t).map(|_| ()));
        let built = web::api::build_request_withdraw(N, &w).unwrap();
        let wf = WithdrawFiguresInput { covenant_id: w.covenant_id.clone(), vault: w.vault.clone(), position: w.position.clone(), shares: w.shares.clone(), built };
        assert!(withdraw_figures(N, &wf).is_ok(), "honest withdraw figures {name}");
        total += fuzz::<WithdrawFiguresInput, _>(&format!("withdrawFigures {name}"), serde_json::to_value(&wf).unwrap(), &mut r, |t| withdraw_figures(N, t).map(|_| ()));
    }
    for (name, si) in vault_settles().into_iter().take(3) {
        let built = web::api::build_settle(N, &si).unwrap();
        let sf = SettleFiguresInput { covenant_id: si.covenant_id.clone(), request: si.request.clone(), record: si.record.clone(), built };
        total += fuzz::<SettleFiguresInput, _>(&format!("settleFigures {name}"), serde_json::to_value(&sf).unwrap(), &mut r, |t| settle_figures(N, t).map(|_| ()));
    }
    println!("TOTAL PANICS {total}");
    assert_eq!(total, 0, "a vault call panicked (wasm aborts on panic)");
}

#[test]
fn caller_and_claim_figures_never_panic() {
    use web::rounds::{AbsorbInput, CallerFiguresInput, absorbable_pools, caller_figures};
    use web::tickets::{self, ClaimFiguresInput, ReturnFiguresInput};
    let mut r = Rng(0x94D0_49BB_1331_11EB);
    let mut total = 0;
    let all = calls().unwrap();
    let find = |n: &str| input(&all.iter().find(|(x, _, _)| x == n).unwrap_or_else(|| panic!("fixture {n}")).1);
    let Input::Close(ci) = find("close k=4") else { unreachable!() };
    let built = Input::Close(ci.clone()).run().unwrap();
    let close = CallerFiguresInput { covenant_id: ci.covenant_id.clone(), controller: ci.controller.clone(), shards: ci.shards.clone(), marker: None, drawer: None, vault: None, absorbed: vec![], built };
    assert!(caller_figures(N, &close).is_ok(), "honest close figures");
    total += fuzz::<CallerFiguresInput, _>("callerFigures close", serde_json::to_value(&close).unwrap(), &mut r, |t| caller_figures(N, t).map(|_| ()));
    for name in ["draw k=4", "void k=4"] {
        let Input::Draw(di) = find(name) else { unreachable!() };
        let built = Input::Draw(di.clone()).run().unwrap();
        let v = serde_json::json!({
            "covenantId": di.covenant_id, "controller": di.controller, "shards": [], "marker": di.marker,
            "drawer": di.drawer, "vault": di.vault, "absorbed": di.absorbed, "built": built,
        });
        let c: CallerFiguresInput = serde_json::from_value(v.clone()).unwrap();
        assert!(caller_figures(N, &c).is_ok(), "honest {name} figures");
        total += fuzz::<CallerFiguresInput, _>(&format!("callerFigures {name}"), v, &mut r, |t| caller_figures(N, t).map(|_| ()));
        let x = drawn();
        let ab = AbsorbInput {
            covenant_id: di.covenant_id.clone(),
            marker: di.marker.clone(),
            controller: di.controller.clone(),
            drawer: di.drawer.clone(),
            void: name.starts_with("void"),
            virtual_daa_score: "100000000".into(),
            objects: vec![api_obj("pool", &x.pools[0].0)],
            utxos: vec![x.pools[0].0.utxo.clone()],
        };
        total += fuzz::<AbsorbInput, _>(&format!("absorbablePools {name}"), serde_json::to_value(&ab).unwrap(), &mut r, |t| absorbable_pools(N, t).map(|_| ()));
    }
    for (name, c, _) in claim_calls().unwrap().into_iter().take(3) {
        let Input::Claim(i) = input(&c) else { unreachable!() };
        let built = web::api::build_claim(N, &i).unwrap();
        let cf = ClaimFiguresInput { covenant_id: i.covenant_id.clone(), ticket: i.ticket.clone(), pools: i.pools.clone(), purchases: i.purchases.clone(), index: i.index.unwrap_or(0), built };
        total += fuzz::<ClaimFiguresInput, _>(&format!("claimFigures {name}"), serde_json::to_value(&cf).unwrap(), &mut r, |t| tickets::claim_figures(N, t).map(|_| ()));
    }
    for (name, c, _) in all.iter().filter(|(n, _, _)| n.starts_with("return")) {
        let Input::Return(i) = input(c) else { unreachable!() };
        let built = web::api::build_return_deposit(N, &i).unwrap();
        let rf = ReturnFiguresInput { covenant_id: i.covenant_id.clone(), ticket: i.ticket.clone(), pool: i.pool.clone(), built };
        assert!(tickets::return_figures(N, &rf).is_ok(), "honest {name}");
        total += fuzz::<ReturnFiguresInput, _>(&format!("returnFigures {name}"), serde_json::to_value(&rf).unwrap(), &mut r, |t| tickets::return_figures(N, t).map(|_| ()));
    }
    println!("TOTAL PANICS {total}");
    assert_eq!(total, 0, "a caller or claim call panicked (wasm aborts on panic)");
}

/// A doctored backup file (checksum recomputed after every mutation, so the record checks run), `checkCopy`,
/// `mergeCopy` and `ticketCard` with mutated inputs.
#[test]
fn backup_copy_and_card_never_panic() {
    use web::backup::{self, BackupReadInput, BackupWriteInput, CopyCheckInput, MergeCopyInput};
    use web::tickets::{self, TicketCardInput};
    use web::views::{RoleName, StateView};
    use web::webstore::{RoundCopyStage, StoredObject, StoredResult, StoredRound, StoredTicket};
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    let mut all: Vec<(String, lottery::web::fixtures::Call)> = calls().unwrap().into_iter().filter(|(n, _, _)| n == "claim k=4").map(|(n, c, _)| (n, c)).collect();
    all.extend(claim_calls().unwrap().into_iter().map(|(n, c, _)| (n, c)));
    let (_, c) = all.into_iter().next().unwrap();
    let Input::Claim(i) = input(&c) else { unreachable!() };
    let StateView::Pool(pool) = web::api::decode_state(N, RoleName::Pool, &i.pools[0].state).unwrap() else { unreachable!() };
    let StateView::Ticket(t) = web::api::decode_state(N, RoleName::Ticket, &i.ticket.state).unwrap() else { unreachable!() };
    let cov = i.covenant_id.clone();
    let card_in = TicketCardInput { covenant_id: cov.clone(), ticket: i.ticket.clone(), pools: i.pools.clone(), absorbed: vec![], complete: false, purchases: Some(i.purchases.clone()), winning: None, claim_deadline: None, void: false, virtual_daa_score: "1".into() };
    let card = tickets::ticket_card(N, &card_in).unwrap();
    let key = format!("{}:{}", i.ticket.utxo.transaction_id, i.ticket.utxo.index);
    let ticket = StoredTicket { key, covenant_id: cov.clone(), network: "testnet-10".into(), round: t.round.clone(), shard_index: Some(0), ticket: i.ticket.clone(), bundle: card.combos.iter().map(|c| c.combination).collect(), payout: card.payout.clone(), buy: None, ended: None, pending: None, saved_ms: "1".into() };
    let roots = web::api::check_roots(&RootsInput { covenant_id: cov.clone(), against: i.pools[0].clone(), purchases: i.purchases.clone() }).unwrap();
    let po = StoredObject { role: RoleName::Pool, state: i.pools[0].state.clone(), script_public_key: i.pools[0].utxo.script_public_key.clone() };
    let round = StoredRound { key: format!("{cov}:{}", t.round), covenant_id: cov.clone(), network: "testnet-10".into(), round: t.round.clone(), m: pool.m.parse().unwrap(), k: pool.k.parse().unwrap(), stage: RoundCopyStage::Drawn, purchases: i.purchases.clone(), roots: Some(roots.roots), against: Some(po.clone()), result: Some(StoredResult { winning: card.winning.unwrap(), claim_deadline: pool.deadline.clone(), pools: vec![po] }), marker: None, checked_ms: "1".into() };
    let w = backup::backup_write(N, &BackupWriteInput { covenant_id: cov.clone(), created_ms: "7".into(), tickets: vec![ticket.clone()], rounds: vec![round.clone()] }).unwrap();
    assert!(w.skipped.is_empty(), "{:?}", w.skipped);
    let text = w.text;
    assert!(backup::backup_read(N, &BackupReadInput { covenant_id: cov.clone(), text: text.clone() }).is_ok(), "honest backup");
    let mut total = 0;
    let base: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut ps = vec![];
    paths(&base, vec![], &mut ps);
    let (mut reached, mut panics) = (0, 0);
    for _ in 0..iters() {
        let v = mutated(&mut r, &base, &ps);
        let Ok(mut f) = serde_json::from_value::<backup::BackupFile>(v) else { continue };
        f.checksum = String::new();
        f.checksum = hex(&web::indexer::bundle_hash(serde_json::to_string(&f).unwrap().as_bytes()));
        let s = serde_json::to_string(&f).unwrap();
        reached += 1;
        if catch_unwind(AssertUnwindSafe(|| backup::backup_read(N, &BackupReadInput { covenant_id: cov.clone(), text: s.clone() }))).is_err() {
            panics += 1;
            println!("PANIC backupRead: {}", &s[..s.len().min(600)]);
        }
    }
    println!("backupRead (doctored, checksum fixed): {reached} files reached the record checks, {panics} panics");
    assert!(reached > 0);
    total += panics;
    total += text_fuzz("backupRead text", &text, &mut r, |s| {
        let _ = backup::backup_read(N, &BackupReadInput { covenant_id: cov.clone(), text: s.to_string() });
    });
    let cc = CopyCheckInput { covenant_id: cov.clone(), round: round.clone(), live: vec![i.pools[0].utxo.clone()], virtual_daa_score: Some("1".into()) };
    total += fuzz::<CopyCheckInput, _>("checkCopy", serde_json::to_value(&cc).unwrap(), &mut r, |x| backup::check_copy(N, x).map(|_| ()));
    let mc = MergeCopyInput { covenant_id: cov.clone(), prev: Some(round.clone()), next: round.clone() };
    total += fuzz::<MergeCopyInput, _>("mergeCopy", serde_json::to_value(&mc).unwrap(), &mut r, |x| backup::merge_copy(N, x).map(|_| ()));
    total += fuzz::<TicketCardInput, _>("ticketCard", serde_json::to_value(&card_in).unwrap(), &mut r, |x| tickets::ticket_card(N, x).map(|_| ()));
    println!("TOTAL PANICS {total}");
    assert_eq!(total, 0);
}
