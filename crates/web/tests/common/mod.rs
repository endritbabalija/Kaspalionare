//! Shared by the native API tests (`tests/api.rs`) and the wasm W1 test (`tests/w1_wasm.rs`):
//! the fixture calls of `lottery::web::fixtures` as the page's JSON inputs, and the digest of
//! a built transaction's boundary form.

#![allow(dead_code)]

use lottery::buy::OwnerPub;
use lottery::state::Payout;
use lottery::web::fixtures::Call;
use web::convert::{Net, hex, obj_dto, utxo_dto};
use web::dto::*;

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
