//! The fixture cases as chain objects (front-end F1 part C, W1).
//!
//! Each fixture case of the web builders (`BuyCase::new` and the other `*Case` fixtures, as
//! `tests/f1_web.rs` funds them) is turned into the call a page makes: the covenant UTXOs it
//! spends (outpoint, entry, state bytes), the purchases behind its trees, the user's choices
//! and the wallet's coins (`Call`). `expected` is `web::build`'s transaction for the fixture
//! case itself, so `run(call) == expected` proves the chain-object path (`web::cases`)
//! derives exactly the fixture case. `crates/web` runs the same calls through its JSON API,
//! natively and in wasm (W1). Deterministic: no randomness, no signatures (the transactions
//! are unsigned).

use harness::{Hash, TransactionOutpoint, UtxoEntry, outpoint};

use super::build::{self, Coin, Funds, OwnerCoin};
use super::cases::{self, BuyChoice, DrawObjs, Obj};
use super::sign::Unsigned;
use super::verify::state_bytes;
use super::{Code, Error, Result};
use crate::anchor::AnchorProof;
use crate::buy::{BuyCase, KAS, OwnerKey, Templates, b_max, spk_of};
use crate::claim::{ClaimCase, ReturnCase, ReturnPath};
use crate::close::{CloseCase, FundingIn};
use crate::draw::DrawCase;
use crate::index::State;
use crate::state::Payout;
use crate::vault::{DepositCase, OWNER_SK, SettleCase, WithdrawCase};

/// The fixture wallet's key (as `tests/f1_web.rs`). It signs nothing here.
pub const WALLET_SK: [u8; 32] = [0x21; 32];
/// `close()`'s virtual DAA score in the fixtures (as `tests/f1_web.rs`).
pub const CLOSE_NOW: u64 = 1_000_100;

/// A page's build call: the chain objects and choices of one builder.
#[derive(Clone, Debug)]
pub enum Call {
    Buy { cov: Hash, shard: Obj, purchases: Vec<u8>, choice: BuyChoice, funds: Funds },
    Claim { cov: Hash, ticket: Obj, pools: Vec<Obj>, purchases: Vec<Vec<u8>>, idx: Option<i64>, funds: Funds },
    Return { cov: Hash, ticket: Obj, pool: Option<Obj>, funds: Funds },
    Close { cov: Hash, controller: Obj, params: Obj, shards: Vec<Obj>, funder: Payout, funds: Funds, now: u64 },
    Draw { cov: Hash, drawer: Obj, marker: Obj, vault: Obj, controller: Obj, absorbed: Vec<Obj>, purchases: Vec<Vec<u8>>, proof: Option<Vec<u8>>, funder: Payout, funds: Funds },
    Deposit { cov: Hash, vault: Obj, d: i64, owner: Payout, funds: Funds },
    Withdraw { cov: Hash, vault: Obj, position: Obj, w: i64, owner: OwnerCoin, funds: Funds },
    Settle { cov: Hash, request: Obj, record: Obj, funds: Funds },
}

/// Run a call through `web::cases`.
pub fn run(c: &Call) -> Result<Unsigned> {
    match c {
        Call::Buy { cov, shard, purchases, choice, funds } => cases::buy(cov, shard, purchases, choice, funds),
        Call::Claim { cov, ticket, pools, purchases, idx, funds } => cases::claim(cov, ticket, pools, purchases, *idx, funds),
        Call::Return { cov, ticket, pool, funds } => cases::return_deposit(cov, ticket, pool.as_ref(), funds),
        Call::Close { cov, controller, params, shards, funder, funds, now } => cases::close(cov, controller, params, shards, *funder, funds, *now),
        Call::Draw { cov, drawer, marker, vault, controller, absorbed, purchases, proof, funder, funds } => {
            let o = DrawObjs { drawer, marker, vault, controller, absorbed };
            cases::draw(cov, &o, purchases, proof.as_deref(), *funder, funds)
        }
        Call::Deposit { cov, vault, d, owner, funds } => cases::request_deposit(cov, vault, *d, *owner, funds),
        Call::Withdraw { cov, vault, position, w, owner, funds } => cases::request_withdraw(cov, vault, position, *w, owner, funds),
        Call::Settle { cov, request, record, funds } => cases::settle(cov, request, record, funds),
    }
}

fn wallet_funds(v: &[(u64, u64)], first: u32, fee: u64) -> Funds {
    let spk = spk_of(&WALLET_SK);
    let coins = v.iter().enumerate().map(|(n, &(value, daa))| Coin { outpoint: outpoint(first + n as u32), entry: UtxoEntry::new(value, spk.clone(), daa, false, None) }).collect();
    Funds { coins, change: spk, fee }
}

fn fd(f: &[FundingIn]) -> Vec<(u64, u64)> {
    f.iter().map(|x| (x.value, x.daa)).collect()
}

/// The covenant objects of a built transaction: input i is `states[i]` at its outpoint and
/// entry as built.
fn objs(u: &Unsigned, states: &[State]) -> Result<Vec<Obj>> {
    states
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let inp = u.tx.inputs.get(i).ok_or_else(|| Error::new(Code::Encode, format!("no input {i}")))?;
            Ok(Obj { outpoint: inp.previous_outpoint, entry: u.entries[i].clone(), state: state_bytes(s)? })
        })
        .collect()
}

fn ops(n: u32) -> Vec<TransactionOutpoint> {
    (0..n).map(outpoint).collect()
}

/// Every fixture call, named, with `web::build`'s transaction for the fixture case.
pub fn calls() -> Result<Vec<(String, Call, Unsigned)>> {
    let t = Templates::try_get(b_max()).map_err(|e| Error::new(Code::Templates, e))?;
    let mut out = vec![];
    // buy, b = 1 with referral (paid), and b = 4 with two coins and no referrer.
    for (name, b, referrer) in [("buy b=1 ref", 1usize, true), ("buy b=4", 4, false)] {
        let mut c = BuyCase::new(b_max());
        c.bundle = (0..b).map(|i| crate::combo::Combo::new([1 + i as u8, 7, 12, 19, 28], 1 + i as u8)).collect();
        c.with_ref = referrer;
        let f = if b == 1 { wallet_funds(&[(100 * KAS, 0)], 1, c.fee) } else { wallet_funds(&[(60 * KAS, 0), (50 * KAS, 0)], 1, c.fee) };
        let u = build::buy(&c, outpoint(0), &f)?;
        let o = objs(&u, &[State::Shard(c.shard.clone())])?;
        let choice = BuyChoice { bundle: c.bundle.clone(), payout: c.payout, referrer: referrer.then_some(c.referrer) };
        out.push((name.into(), Call::Buy { cov: c.cov, shard: o[0].clone(), purchases: c.tree.purchases(), choice, funds: f }, u));
    }
    // claim, k = 4 (the fixture index).
    let c = ClaimCase::new(4);
    let f = wallet_funds(&fd(&c.funding), 2, c.fee);
    let u = build::claim(&c, outpoint(0), outpoint(1), &f)?;
    let o = objs(&u, &[State::Ticket(c.ticket.clone()), State::Pool(c.pool.clone())])?;
    let purchases = c.trees.iter().map(|t| t.purchases()).collect();
    out.push(("claim k=4".into(), Call::Claim { cov: c.cov, ticket: o[0].clone(), pools: vec![o[1].clone()], purchases, idx: Some(c.idx), funds: f }, u));
    // return_deposit, both paths.
    for (name, c) in [("return after", ReturnCase::after()), ("return with pool", ReturnCase::with_pool())] {
        let n_cov = if matches!(c.path, ReturnPath::After) { 1 } else { 2 };
        let f = wallet_funds(&fd(&c.funding), n_cov, c.fee);
        let pool_op = (n_cov == 2).then(|| outpoint(1));
        let u = build::return_deposit(&c, outpoint(0), pool_op, &f)?;
        let mut states = vec![State::Ticket(c.ticket.clone())];
        if let ReturnPath::WithPool { pool, .. } = &c.path {
            states.push(State::Pool(pool.clone()));
        }
        let o = objs(&u, &states)?;
        out.push((name.into(), Call::Return { cov: c.cov, ticket: o[0].clone(), pool: o.get(1).cloned(), funds: f }, u));
    }
    // close, K = 4 and K = 16.
    for k in [4usize, 16] {
        let c = CloseCase::new_with(k, &t);
        let f = wallet_funds(&fd(&c.funding), k as u32 + 2, c.fee);
        let u = build::close(&c, &ops(k as u32 + 2), &f, CLOSE_NOW)?;
        let mut states = vec![State::Controller(c.controller.clone()), State::Params(c.params.clone())];
        states.extend(c.shards.iter().map(|s| State::Shard(s.state.clone())));
        let o = objs(&u, &states)?;
        let call = Call::Close { cov: c.cov, controller: o[0].clone(), params: o[1].clone(), shards: o[2..].to_vec(), funder: c.funder, funds: f, now: CLOSE_NOW };
        out.push((format!("close k={k}"), call, u));
    }
    // draw with its anchor proof and the void, K = 4.
    for void in [false, true] {
        let mut c = if void { DrawCase::new_void(4) } else { DrawCase::new(4) };
        if !void {
            let p = AnchorProof::from_sim(&c.chain(), c.d() as u64);
            c.x_hash = p.x_hash();
            c.proof = Some(p);
        }
        let f = wallet_funds(&fd(&c.funding), 4, c.fee);
        let u = build::draw(&c, &ops(4), &f)?;
        let states = [State::Drawer(c.drawer.clone()), State::Marker(c.marker.clone()), State::Vault(c.vault.clone()), State::Controller(c.controller.clone())];
        let o = objs(&u, &states)?;
        let call = Call::Draw {
            cov: c.cov,
            drawer: o[0].clone(),
            marker: o[1].clone(),
            vault: o[2].clone(),
            controller: o[3].clone(),
            absorbed: vec![],
            purchases: c.trees.iter().map(|t| t.purchases()).collect(),
            proof: c.proof.as_ref().map(|p| p.to_bytes().to_vec()),
            funder: c.funder,
            funds: f,
        };
        out.push((if void { "void k=4" } else { "draw k=4" }.into(), call, u));
    }
    // request_deposit.
    let c = DepositCase::new_with(&t);
    let f = wallet_funds(&fd(&c.funding), 1, c.fee);
    let u = build::request_deposit(&c, outpoint(0), &f)?;
    let o = objs(&u, &[State::Vault(c.vault.clone())])?;
    out.push(("request_deposit".into(), Call::Deposit { cov: c.cov, vault: o[0].clone(), d: c.d, owner: c.owner, funds: f }, u));
    // request_withdraw: each owner class, partial and whole.
    for key in [OwnerKey::Schnorr(OWNER_SK), OwnerKey::Ecdsa(OWNER_SK), OwnerKey::P2sh(OWNER_SK)] {
        for whole in [false, true] {
            let mut c = WithdrawCase::new_with(&t, key.clone());
            if whole {
                c = c.whole();
            }
            let oc = OwnerCoin { coin: Coin { outpoint: outpoint(2), entry: UtxoEntry::new(c.owner_value, key.spk(), c.owner_daa, false, None) }, owner: key.public() };
            let f = wallet_funds(&fd(&c.funding), 3, c.fee);
            let u = build::request_withdraw(&c, outpoint(0), outpoint(1), &oc, &f)?;
            let o = objs(&u, &[State::Vault(c.vault.clone()), State::Position(c.position.clone())])?;
            let name = format!("request_withdraw {:?} {}", key.public().class(), if whole { "whole" } else { "partial" });
            out.push((name, Call::Withdraw { cov: c.cov, vault: o[0].clone(), position: o[1].clone(), w: c.w, owner: oc, funds: f }, u));
        }
    }
    // settle, both kinds.
    for (name, c) in [("settle deposit", SettleCase::deposit()), ("settle withdrawal", SettleCase::withdrawal())] {
        let f = wallet_funds(&fd(&c.funding), 2, c.fee);
        let u = build::settle(&c, outpoint(0), outpoint(1), &f)?;
        let o = objs(&u, &[State::Request(c.request.clone()), State::Record(c.record.clone())])?;
        out.push((name.into(), Call::Settle { cov: c.cov, request: o[0].clone(), record: o[1].clone(), funds: f }, u));
    }
    Ok(out)
}

/// Front end F4: one claim of each mode (D56), as page calls: PAY on a combination held twice
/// (h = 2, prize 0.1 KAS), ACCRUE (a prize below P_MIN), and FINAL (the ticket's last unclaimed
/// index: the prize plus the ticket's value). Not part of `calls()`, whose digests are golden.
pub fn claim_calls() -> Result<Vec<(String, Call, Unsigned)>> {
    let mut fin = ClaimCase::new(4);
    fin.make_final();
    let cases = [
        ("claim pay h=2", ClaimCase::fabricated(4, 10, crate::state::TIER_LOWER0, 20_000_000, &[1, 1])),
        ("claim accrue", ClaimCase::fabricated(4, 10, crate::state::TIER_LOWER0, 1_000_000, &[1])),
        ("claim final", fin),
    ];
    let mut out = vec![];
    for (name, c) in cases {
        let f = wallet_funds(&fd(&c.funding), 2, c.fee);
        let u = build::claim(&c, outpoint(0), outpoint(1), &f)?;
        let o = objs(&u, &[State::Ticket(c.ticket.clone()), State::Pool(c.pool.clone())])?;
        let purchases = c.trees.iter().map(|t| t.purchases()).collect();
        out.push((name.into(), Call::Claim { cov: c.cov, ticket: o[0].clone(), pools: vec![o[1].clone()], purchases, idx: Some(c.idx), funds: f }, u));
    }
    Ok(out)
}
