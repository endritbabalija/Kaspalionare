//! Shared by the native API tests (`tests/api.rs`) and the wasm W1 test (`tests/w1_wasm.rs`):
//! the fixture calls of `lottery::web::fixtures` as the page's JSON inputs, and the digest of
//! a built transaction's boundary form.

#![allow(dead_code)]

use lottery::buy::OwnerPub;
use lottery::state::Payout;
use lottery::web::fixtures::{Call, WALLET_SK};
use web::convert::{Net, hex, obj_dto, utxo_dto};
use web::dto::*;
use web::views::RoleName;

/// The fee rate of every fixture input (sompi/gram, the floor).
pub const RATE: u32 = 100;

pub fn addr(p: &Payout) -> String {
    Net::Testnet10.address(&p.spk()).expect("standard payout")
}

fn funds(f: &lottery::web::build::Funds) -> Funds {
    Funds { coins: f.coins.iter().map(|c| utxo_dto(&c.outpoint, &c.entry)).collect(), change: Net::Testnet10.address(&f.change).unwrap(), fee_rate: RATE }
}

/// One page input.
#[derive(Clone, Debug)]
pub enum Input {
    Buy(BuyInput),
    Claim(ClaimInput),
    Return(ReturnInput),
    Close(CloseInput),
    Draw(DrawInput),
    Deposit(DepositInput),
    Withdraw(WithdrawInput),
    Settle(SettleInput),
}

impl Input {
    /// The input as the JSON a page would pass.
    pub fn json(&self) -> serde_json::Value {
        match self {
            Input::Buy(i) => serde_json::to_value(i),
            Input::Claim(i) => serde_json::to_value(i),
            Input::Return(i) => serde_json::to_value(i),
            Input::Close(i) => serde_json::to_value(i),
            Input::Draw(i) => serde_json::to_value(i),
            Input::Deposit(i) => serde_json::to_value(i),
            Input::Withdraw(i) => serde_json::to_value(i),
            Input::Settle(i) => serde_json::to_value(i),
        }
        .unwrap()
    }

    /// The API call on this input (natively).
    pub fn run(&self) -> lottery::web::Result<BuiltTx> {
        let n = Net::Testnet10;
        match self {
            Input::Buy(i) => web::api::build_buy(n, i),
            Input::Claim(i) => web::api::build_claim(n, i),
            Input::Return(i) => web::api::build_return_deposit(n, i),
            Input::Close(i) => web::api::build_close(n, i),
            Input::Draw(i) => web::api::build_draw(n, i),
            Input::Deposit(i) => web::api::build_request_deposit(n, i),
            Input::Withdraw(i) => web::api::build_request_withdraw(n, i),
            Input::Settle(i) => web::api::build_settle(n, i),
        }
    }

    /// The same input re-parsed from `v` (a mutated JSON), or the parse error.
    pub fn with_json(&self, v: serde_json::Value) -> Result<Input, String> {
        let e = |e: serde_json::Error| e.to_string();
        Ok(match self {
            Input::Buy(_) => Input::Buy(serde_json::from_value(v).map_err(e)?),
            Input::Claim(_) => Input::Claim(serde_json::from_value(v).map_err(e)?),
            Input::Return(_) => Input::Return(serde_json::from_value(v).map_err(e)?),
            Input::Close(_) => Input::Close(serde_json::from_value(v).map_err(e)?),
            Input::Draw(_) => Input::Draw(serde_json::from_value(v).map_err(e)?),
            Input::Deposit(_) => Input::Deposit(serde_json::from_value(v).map_err(e)?),
            Input::Withdraw(_) => Input::Withdraw(serde_json::from_value(v).map_err(e)?),
            Input::Settle(_) => Input::Settle(serde_json::from_value(v).map_err(e)?),
        })
    }

    /// The export's name.
    pub fn export(&self) -> &'static str {
        match self {
            Input::Buy(_) => "buildBuy",
            Input::Claim(_) => "buildClaim",
            Input::Return(_) => "buildReturnDeposit",
            Input::Close(_) => "buildClose",
            Input::Draw(_) => "buildDraw",
            Input::Deposit(_) => "buildRequestDeposit",
            Input::Withdraw(_) => "buildRequestWithdraw",
            Input::Settle(_) => "buildSettle",
        }
    }
}

fn owner_input(o: &lottery::web::build::OwnerCoin) -> OwnerInput {
    let (kind, key) = match &o.owner {
        OwnerPub::Schnorr(k) => (KeyKind::Schnorr, k.to_vec()),
        OwnerPub::Ecdsa(k) => (KeyKind::Ecdsa, k.to_vec()),
        OwnerPub::P2sh(k) => (KeyKind::P2sh, k.to_vec()),
    };
    OwnerInput { coin: utxo_dto(&o.coin.outpoint, &o.coin.entry), kind, public_key: hex(&key) }
}

/// A fixture call as the page's input. `close()` and `draw()` take the caller's change
/// address as the reserve funder, so `equivalent` gives the call with that funder.
pub fn input(call: &Call) -> Input {
    match call {
        Call::Buy { cov, shard, purchases, choice, funds: f } => Input::Buy(BuyInput {
            covenant_id: hex(&cov.as_bytes()),
            shard: obj_dto(shard),
            purchases: hex(purchases),
            bundle: choice.bundle.iter().map(|c| Combination { main: c.main, bonus: c.bonus }).collect(),
            payout: addr(&choice.payout),
            referrer: choice.referrer.as_ref().map(addr),
            funds: funds(f),
        }),
        Call::Claim { cov, ticket, pools, purchases, idx, funds: f } => Input::Claim(ClaimInput {
            covenant_id: hex(&cov.as_bytes()),
            ticket: obj_dto(ticket),
            pools: pools.iter().map(obj_dto).collect(),
            purchases: purchases.iter().map(|p| hex(p)).collect(),
            index: idx.map(|i| i as u32),
            funds: funds(f),
        }),
        Call::Return { cov, ticket, pool, funds: f } => Input::Return(ReturnInput { covenant_id: hex(&cov.as_bytes()), ticket: obj_dto(ticket), pool: pool.as_ref().map(obj_dto), funds: funds(f) }),
        Call::Close { cov, controller, params, shards, funder: _, funds: f, now } => Input::Close(CloseInput {
            covenant_id: hex(&cov.as_bytes()),
            controller: obj_dto(controller),
            params: obj_dto(params),
            shards: shards.iter().map(obj_dto).collect(),
            virtual_daa_score: now.to_string(),
            funds: funds(f),
        }),
        Call::Draw { cov, drawer, marker, vault, controller, absorbed, purchases, proof, funder: _, funds: f } => Input::Draw(DrawInput {
            covenant_id: hex(&cov.as_bytes()),
            drawer: obj_dto(drawer),
            marker: obj_dto(marker),
            vault: obj_dto(vault),
            controller: obj_dto(controller),
            absorbed: absorbed.iter().map(obj_dto).collect(),
            purchases: purchases.iter().map(|p| hex(p)).collect(),
            anchor_proof: proof.as_ref().map(|p| hex(p)),
            funds: funds(f),
        }),
        Call::Deposit { cov, vault, d, owner, funds: f } => Input::Deposit(DepositInput { covenant_id: hex(&cov.as_bytes()), vault: obj_dto(vault), amount: d.to_string(), owner: addr(owner), funds: funds(f) }),
        Call::Withdraw { cov, vault, position, w, owner, funds: f } => Input::Withdraw(WithdrawInput {
            covenant_id: hex(&cov.as_bytes()),
            vault: obj_dto(vault),
            position: obj_dto(position),
            shares: w.to_string(),
            owner: owner_input(owner),
            funds: funds(f),
        }),
        Call::Settle { cov, request, record, funds: f } => Input::Settle(SettleInput { covenant_id: hex(&cov.as_bytes()), request: obj_dto(request), record: obj_dto(record), funds: funds(f) }),
    }
}

/// The call the API makes for `call`'s input: the reserve funder of `close()` and `draw()` is
/// the change address.
pub fn equivalent(call: &Call) -> Call {
    let change = |f: &lottery::web::build::Funds| lottery::web::build::payout_of_spk(&f.change).unwrap();
    match call.clone() {
        Call::Close { cov, controller, params, shards, funder: _, funds, now } => Call::Close { cov, controller, params, shards, funder: change(&funds), funds, now },
        Call::Draw { cov, drawer, marker, vault, controller, absorbed, purchases, proof, funder: _, funds } => {
            Call::Draw { cov, drawer, marker, vault, controller, absorbed, purchases, proof, funder: change(&funds), funds }
        }
        c => c,
    }
}

/// The fee of a call's funds set to `fee`.
pub fn with_fee(call: &Call, fee: u64) -> Call {
    let mut c = call.clone();
    let f = match &mut c {
        Call::Buy { funds, .. }
        | Call::Claim { funds, .. }
        | Call::Return { funds, .. }
        | Call::Close { funds, .. }
        | Call::Draw { funds, .. }
        | Call::Deposit { funds, .. }
        | Call::Withdraw { funds, .. }
        | Call::Settle { funds, .. } => funds,
    };
    f.fee = fee;
    c
}

/// The digest line of a built transaction's boundary form: blake2b-256 of its JSON.
pub fn digest_line(name: &str, b: &BuiltTx) -> String {
    let j = serde_json::to_string(b).unwrap();
    let d = blake2b_simd::Params::new().hash_length(32).hash(j.as_bytes());
    format!("{name} {} {}", j.len(), d.to_hex())
}

/// The golden file of the API digests (written natively, compared natively and in wasm).
pub const GOLDEN: &str = include_str!("../fixtures/w1_api.txt");

/// A UTXO the transaction `tx` creates at output `i` (as a node would show it once confirmed
/// at `daa`), with its state bytes.
pub fn created(tx: &kaspa_consensus_core::tx::Transaction, i: usize, cov: &kaspa_hashes::Hash, daa: u64, state: &lottery::index::State) -> CovUtxo {
    let o = &tx.outputs[i];
    let entry = kaspa_consensus_core::tx::UtxoEntry::new(o.value, o.script_public_key.clone(), daa, false, Some(*cov));
    let obj = lottery::web::cases::Obj { outpoint: kaspa_consensus_core::tx::TransactionOutpoint::new(tx.id(), i as u32), entry, state: lottery::web::verify::state_bytes(state).unwrap() };
    web::convert::obj_dto(&obj)
}

/// The output of `tx` whose script is `state`'s P2SH.
pub fn output_of(tx: &kaspa_consensus_core::tx::Transaction, state: &lottery::index::State) -> usize {
    let spk = lottery::web::verify::state_spk(state).unwrap();
    tx.outputs.iter().position(|o| o.script_public_key == spk).unwrap_or_else(|| panic!("no output for {state:?}"))
}

/// A draw with winners (a jackpot holder in shard 2, two second-prize holders in shard 1;
/// the fixture draw has none, so it creates no pool), built by the web builder as the API
/// builds it (the wallet's coins, its change address as the funder), and the objects
/// around it: its marker and next controller (inputs 1 and 3), the round r drawer (input 0),
/// the pools it creates and the next round's drawer.
///
/// F1 re-review S1: its anchor X has a real header hash. The sim's X fields do not depend on
/// X.hash, so the case's X.hash is set to the hash of a header committing to them
/// (`lottery::anchor::header_committing_to`) before the winners are placed; `anchor_block`
/// is that header as a node would report it (a chain block).
pub struct Drawn {
    pub cov: kaspa_hashes::Hash,
    pub marker: CovUtxo,
    pub controller: CovUtxo,
    pub drawer_before: CovUtxo,
    pub purchases: Vec<Vec<u8>>,
    pub proof: Vec<u8>,
    pub pools: Vec<(CovUtxo, [u8; 6])>,
    pub drawer: CovUtxo,
    pub anchor_block: AnchorBlock,
    /// F5: the vault (input 2) and the wallet's coins, so the page's `buildDraw` can rebuild it.
    pub vault: CovUtxo,
    pub coins: Vec<Utxo>,
}

pub fn drawn() -> Drawn {
    use lottery::index::State;
    let mut c = lottery::draw::DrawCase::new(4);
    let fields = lottery::anchor::AnchorProof::from_sim(&c.chain(), c.d() as u64).x;
    let header = lottery::anchor::header_committing_to(&fields, 1);
    c.x_hash = header.hash.as_bytes();
    c.hold_jackpot(2, 1);
    let b2 = c.other_bonus(0);
    c.hold(1, b2, 2);
    let p = lottery::anchor::AnchorProof::from_sim(&c.chain(), c.d() as u64);
    assert_eq!(p.x_hash(), header.hash.as_bytes(), "X's fields do not depend on its hash");
    p.x.check_header(&header).expect("the header binds X");
    c.x_hash = p.x_hash();
    c.proof = Some(p.clone());
    let wspk = lottery::buy::spk_of(&WALLET_SK);
    c.funder = lottery::web::build::payout_of_spk(&wspk).unwrap();
    let coins: Vec<lottery::web::build::Coin> = c
        .funding
        .iter()
        .enumerate()
        .map(|(n, f)| lottery::web::build::Coin { outpoint: harness::outpoint(4 + n as u32), entry: kaspa_consensus_core::tx::UtxoEntry::new(f.value, wspk.clone(), f.daa, false, None) })
        .collect();
    let coin_dtos: Vec<Utxo> = coins.iter().map(|k: &lottery::web::build::Coin| utxo_dto(&k.outpoint, &k.entry)).collect();
    let funds = lottery::web::build::Funds { coins, change: wspk, fee: c.fee };
    let ops: Vec<_> = (0..4).map(harness::outpoint).collect();
    let u = lottery::web::build::draw(&c, &ops, &funds).unwrap();
    let x = c.try_expected().unwrap();
    let input = |i: usize, st: State| {
        let o = lottery::web::cases::Obj { outpoint: u.tx.inputs[i].previous_outpoint, entry: u.entries[i].clone(), state: lottery::web::verify::state_bytes(&st).unwrap() };
        web::convert::obj_dto(&o)
    };
    let pools = x.new_pools.iter().map(|ps| (created(&u.tx, output_of(&u.tx, &State::Pool(ps.clone())), &c.cov, 2_000_000, &State::Pool(ps.clone())), ps.win)).collect::<Vec<_>>();
    assert!(pools.len() >= 2, "the draw creates the jackpot and second-prize pools");
    let dr = State::Drawer(x.drawer.clone());
    Drawn {
        cov: c.cov,
        marker: input(1, State::Marker(c.marker.clone())),
        controller: input(3, State::Controller(c.controller.clone())),
        drawer_before: input(0, State::Drawer(c.drawer.clone())),
        purchases: c.trees.iter().map(|t| t.purchases()).collect(),
        proof: p.to_bytes().to_vec(),
        pools,
        drawer: created(&u.tx, output_of(&u.tx, &dr), &c.cov, 2_000_000, &dr),
        anchor_block: AnchorBlock { header: web::convert::header_dto(&header), is_chain_block: true },
        vault: input(2, State::Vault(c.vault.clone())),
        coins: coin_dtos,
    }
}


/// The golden file of the verify half (written natively, compared natively and in wasm).
pub const GOLDEN_VERIFY: &str = include_str!("../fixtures/w1_verify.txt");

/// A line of a result's JSON text: its length and blake2b-256.
pub fn text_line(name: &str, json: &str) -> String {
    let d = blake2b_simd::Params::new().hash_length(32).hash(json.as_bytes());
    format!("{name} {} {}", json.len(), d.to_hex())
}

/// One decode or verify call (W1's verify half, F1 review item 4).
#[derive(Clone, Debug)]
pub enum Verify {
    Decode(RoleName, String),
    Redeem(String),
    State(String, RoleName, CovUtxo),
    Roots(RootsInput),
    Winning(WinningInput),
}

fn role(r: &str) -> RoleName {
    serde_json::from_value(serde_json::json!(r)).unwrap()
}

/// Every decode and verify call W1 runs on both builds: each fixture object as its role
/// (`decodeState`, `verifyState`), as a wrong role and on another script (errors), its
/// redeem (`decodeRedeem`); `checkRoots` against a marker and a pool, and `winningNumbers`
/// on both paths, each with a refused variant, on a draw with winners.
pub fn verify_calls() -> Vec<(String, Verify)> {
    let mut v = vec![];
    for (name, call, _) in lottery::web::fixtures::calls().unwrap() {
        let objs: Vec<(&str, lottery::web::cases::Obj)> = match &call {
            Call::Buy { shard, .. } => vec![("shard", shard.clone())],
            Call::Claim { ticket, pools, .. } => [vec![("ticket", ticket.clone())], pools.iter().map(|p| ("pool", p.clone())).collect()].concat(),
            Call::Return { ticket, pool, .. } => [vec![("ticket", ticket.clone())], pool.iter().map(|p| ("pool", p.clone())).collect()].concat(),
            Call::Close { controller, params, shards, .. } => {
                [vec![("controller", controller.clone()), ("params", params.clone())], shards.iter().map(|s| ("shard", s.clone())).collect()].concat()
            }
            Call::Draw { drawer, marker, vault, controller, .. } => vec![("drawer", drawer.clone()), ("marker", marker.clone()), ("vault", vault.clone()), ("controller", controller.clone())],
            Call::Deposit { vault, .. } => vec![("vault", vault.clone())],
            Call::Withdraw { vault, position, .. } => vec![("vault", vault.clone()), ("position", position.clone())],
            Call::Settle { request, record, .. } => vec![("request", request.clone()), ("record", record.clone())],
        };
        for (n, (r, o)) in objs.into_iter().enumerate() {
            let dto = obj_dto(&o);
            let cov = hex(&o.entry.covenant_id.unwrap().as_bytes());
            let wrong = if r == "ticket" { "shard" } else { "ticket" };
            v.push((format!("{name} #{n} decodeState {r}"), Verify::Decode(role(r), dto.state.clone())));
            v.push((format!("{name} #{n} decodeState as {wrong}"), Verify::Decode(role(wrong), dto.state.clone())));
            v.push((format!("{name} #{n} verifyState {r}"), Verify::State(cov.clone(), role(r), dto.clone())));
            let mut moved = dto.clone();
            moved.utxo.script_public_key = web::convert::spk_hex(&lottery::buy::spk_of(&WALLET_SK));
            v.push((format!("{name} #{n} verifyState {r} on another script"), Verify::State(cov, role(r), moved)));
            let redeem = lottery::web::verify::decode_state(role(r).role(), &o.state).ok().and_then(|s| s.try_redeem(&lottery::web::verify::templates().unwrap()).ok());
            if let Some(rd) = redeem {
                v.push((format!("{name} #{n} decodeRedeem {r}"), Verify::Redeem(hex(&rd))));
            }
        }
    }
    let x = drawn();
    let cov = hex(&x.cov.as_bytes());
    let lists: Vec<String> = x.purchases.iter().map(|p| hex(p)).collect();
    let mut extra = lists.clone();
    extra[0].push_str("010203040501");
    for (what, against) in [("marker", x.marker.clone()), ("pool", x.pools[0].0.clone())] {
        v.push((format!("checkRoots {what}"), Verify::Roots(RootsInput { covenant_id: cov.clone(), against: against.clone(), purchases: lists.clone() })));
        v.push((format!("checkRoots {what} extra purchase"), Verify::Roots(RootsInput { covenant_id: cov.clone(), against, purchases: extra.clone() })));
    }
    let mut other = lottery::anchor::AnchorProof::from_bytes(&x.proof).unwrap();
    other.x.hash[0] ^= 1;
    let other = hex(&other.to_bytes());
    let proof = hex(&x.proof);
    let closed = RoundObjs::Closed { marker: x.marker.clone(), controller: x.controller.clone(), anchor_block: x.anchor_block.clone() };
    v.push(("winningNumbers closed".into(), Verify::Winning(WinningInput { covenant_id: cov.clone(), from: closed.clone(), anchor_proof: proof.clone() })));
    // F1 re-review S1: another X.hash is refused on the closed path too (the header binds X).
    v.push(("winningNumbers closed, another X".into(), Verify::Winning(WinningInput { covenant_id: cov.clone(), from: closed, anchor_proof: other.clone() })));
    let wrong = RoundObjs::Closed { marker: x.marker.clone(), controller: x.drawer_before.clone(), anchor_block: x.anchor_block.clone() };
    v.push(("winningNumbers closed, a drawer as the controller".into(), Verify::Winning(WinningInput { covenant_id: cov.clone(), from: wrong, anchor_proof: proof.clone() })));
    for (n, (pool, _)) in x.pools.iter().enumerate() {
        let from = RoundObjs::Drawn { pool: pool.clone(), drawer: x.drawer.clone(), anchor_block: None };
        v.push((format!("winningNumbers drawn pool {n}"), Verify::Winning(WinningInput { covenant_id: cov.clone(), from: from.clone(), anchor_proof: proof.clone() })));
        v.push((format!("winningNumbers drawn pool {n}, another X"), Verify::Winning(WinningInput { covenant_id: cov.clone(), from, anchor_proof: other.clone() })));
    }
    v
}

fn outcome_json<T: serde::Serialize>(r: lottery::web::Result<T>) -> String {
    let o = match r {
        Ok(x) => Outcome::Ok(x),
        Err(e) => Outcome::Error(ApiError::from(e)),
    };
    serde_json::to_string(&o).unwrap()
}

/// A verify call natively (the `api` functions), as the JSON text of its outcome.
pub fn verify_native(v: &Verify) -> String {
    let n = Net::Testnet10;
    match v {
        Verify::Decode(r, h) => outcome_json(web::api::decode_state(n, *r, h)),
        Verify::Redeem(h) => outcome_json(web::api::decode_redeem(n, h)),
        Verify::State(c, r, o) => outcome_json(web::api::verify_state(n, c, *r, o)),
        Verify::Roots(i) => outcome_json(web::api::check_roots(i)),
        Verify::Winning(i) => outcome_json(web::api::winning_numbers(i)),
    }
}

// ------------------------------------------------------------------ F4 review: a round's marker

/// A marker for a fixture claim's round (F4 review M2): the pool's round, m, k and roots, and
/// sales and shares chosen so that `draw()`'s pool set holds the pool's tier at its slice
/// (B_1 = muldiv(sales, 1,000) = 125 x slice) and the four other lower tiers. Nobody holds the
/// winning numbers, so there is no jackpot or second-prize pool.
pub fn marker_for(p: &lottery::state::PoolState) -> lottery::state::MarkerState {
    assert_eq!(p.tier, lottery::state::TIER_LOWER0, "the fixture claims are tier 4 + bonus");
    let mut mk = lottery::state::MarkerState {
        round: p.round,
        k: p.k,
        roots: p.roots.clone(),
        sales: p.slice * 1_250,
        ref_paid: 0,
        m: p.m,
        jackpot: 50 * lottery::buy::KAS as i64,
        j_boost: 0,
        g2: 5 * lottery::buy::KAS as i64,
        s_f: 0,
        s_r: 0,
        s_c: 0,
        s_d: 0,
        st: [1_000, 500, 500, 500, 500],
        funder_class: 0,
        funder_key: [0; 33],
    };
    mk.set_funder(&Payout::Schnorr(lottery::params::xonly(&lottery::buy::FUNDING_SK)));
    mk
}

/// A state as a stored chain object (role, state bytes, script).
pub fn stored_object(s: &lottery::index::State) -> web::webstore::StoredObject {
    use lottery::web::verify;
    web::webstore::StoredObject {
        role: RoleName::of(s.role()),
        state: hex(&verify::state_bytes(s).unwrap()),
        script_public_key: web::convert::spk_hex(&verify::state_spk(s).unwrap()),
    }
}

/// A live UTXO of the instance `cov` holding the state `s` (value `amount`), at `<byte x 32>:index`.
pub fn live_utxo(cov: &str, s: &lottery::index::State, byte: u8, index: u32, amount: u64) -> CovUtxo {
    use lottery::web::verify;
    CovUtxo {
        utxo: Utxo {
            transaction_id: hex(&[byte; 32]),
            index,
            amount: amount.to_string(),
            script_public_key: web::convert::spk_hex(&verify::state_spk(s).unwrap()),
            block_daa_score: "2000000".into(),
            is_coinbase: false,
            covenant_id: Some(cov.to_string()),
        },
        state: hex(&verify::state_bytes(s).unwrap()),
    }
}

/// A fixture claim round's marker (`marker_for`) and every pool its draw created, as live UTXOs
/// (the claim's own pool is its fixture UTXO): the page tests' drawn copy with the marker kept.
pub fn marked_round(i: &ClaimInput) -> serde_json::Value {
    use lottery::index::{Role, State};
    use lottery::web::verify;
    let own = &i.pools[0];
    let State::Pool(p) = verify::decode_state(Role::Pool, &web::convert::unhex("state", &own.state).unwrap()).unwrap() else { unreachable!() };
    let mk = marker_for(&p);
    let lists: Vec<Vec<u8>> = i.purchases.iter().map(|x| web::convert::unhex("p", x).unwrap()).collect();
    let set = web::backup::round_pools(&mk, &p, &lists).unwrap();
    let pools: Vec<CovUtxo> = set
        .iter()
        .enumerate()
        .map(|(n, x)| if x == &p { own.clone() } else { live_utxo(&i.covenant_id, &State::Pool(x.clone()), 0x60 + n as u8, n as u32, 2 * lottery::buy::KAS) })
        .collect();
    let marker = State::Marker(mk);
    serde_json::json!({
        "marker": stored_object(&marker),
        "markerUtxo": live_utxo(&i.covenant_id, &marker, 0x4d, 0, 5 * lottery::buy::KAS),
        "pools": pools,
    })
}
