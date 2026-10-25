//! The API's logic, in plain Rust (no JS types): each function takes the boundary types of
//! `dto`, converts them (`convert`), calls `lottery::web`, and converts back. `bindings`
//! wraps each one for wasm-bindgen; native tests call these directly. Every function
//! returns `Result<_, lottery::web::Error>` and none panics on any input (W3 for the
//! boundary: `tests/api.rs`).

use kaspa_consensus_core::tx::{Transaction, UtxoEntry};
use kaspa_hashes::Hash;
use lottery::buy::{OwnerPub, b_max};
use lottery::combo::Combo;
use lottery::state::K_MAX;
use lottery::web::build::{self, Coin, Funds as LFunds, OwnerCoin};
use lottery::web::cases::{self, BuyChoice, DrawObjs};
use lottery::web::sign::{self, SignInput, Unsigned};
use lottery::index::{Role, State};
use lottery::state::Payout;
use lottery::web::{Code, Error, Result, verify};

use crate::convert::{self, Net, dec, dec_i64, hash32, hex, obj, unhex, utxo};
use crate::dto::*;
use crate::safe_json;
use crate::views::{RoleName, StateView};

/// SIG_HASH_ALL (rusty-kaspa v2.1.0 `consensus/core/src/hashing/sighash_type.rs`): the only
/// sighash the builders ask for.
const SIG_HASH_ALL: u8 = 1;

// ------------------------------------------------------------------------------ start

/// Spec 6.6, W4: a `mainnet` build refuses to start, whatever the network asked for.
#[cfg(feature = "mainnet")]
pub fn start(_network: &str) -> Result<(Net, Info)> {
    Err(Error::new(Code::MainnetRefused, "this is a mainnet build; the Kaspalionare page runs on testnet-10 only (spec 6.6)"))
}

/// Start on `network` ("testnet-10"; "simnet" in a `simnet` build): load the embedded
/// templates and report what this build is.
#[cfg(not(feature = "mainnet"))]
pub fn start(network: &str) -> Result<(Net, Info)> {
    let net = Net::parse(network)?;
    let t = verify::templates()?;
    let templates = lottery::digest::template_list(&t).into_iter().map(|(name, tpl)| TemplateHash { name: name.into(), hash: hex(&tpl.hash) }).collect();
    let info = Info {
        network: net.name().into(),
        address_prefix: net.prefix().to_string(),
        version: env!("CARGO_PKG_VERSION").into(),
        b_max: b_max() as u32,
        k_max: K_MAX as u32,
        templates,
    };
    Ok((net, info))
}

// ------------------------------------------------------------------------------ decode, verify

pub fn decode_state(net: Net, role: RoleName, state: &str) -> Result<StateView> {
    Ok(StateView::of(&verify::decode_state(role.role(), &unhex("state", state)?)?, net))
}

pub fn decode_redeem(net: Net, redeem: &str) -> Result<StateView> {
    Ok(StateView::of(&verify::decode_redeem(&unhex("redeem", redeem)?)?, net))
}

/// Spec 6.3: a state from the indexer against its UTXO entry (the state rebuilds the
/// entry's script, under the instance's covenant ID). That the entry is live (unspent) is
/// not checked here: the page reads it from a public node by outpoint (F1 re-review N1).
pub fn verify_state(net: Net, covenant_id: &str, role: RoleName, o: &CovUtxo) -> Result<StateView> {
    let cov = cov_of(covenant_id)?;
    Ok(StateView::of(&cases::live(role.role(), &obj("state", o)?, &cov)?, net))
}

fn purchase_lists(p: &[String]) -> Result<Vec<Vec<u8>>> {
    if p.len() > K_MAX {
        return Err(Error::new(Code::InputRange, format!("{} purchase lists (at most {K_MAX})", p.len())));
    }
    p.iter().enumerate().map(|(i, s)| unhex(&format!("purchases[{i}]"), s)).collect()
}

/// Spec 6.3, J6: the shard trees rebuilt from their purchases against a marker's or pool's
/// frozen roots (the object checked against its UTXO entry as in `verify_state`; liveness
/// is the page's, F1 re-review N1). The round, m, k and roots are the chain object's (F1
/// review).
pub fn check_roots(i: &RootsInput) -> Result<RootsOut> {
    let cov = cov_of(&i.covenant_id)?;
    let o = obj("against", &i.against)?;
    let (round, m, roots, k) = match cases::live(Role::Marker, &o, &cov).or_else(|_| cases::live(Role::Pool, &o, &cov))? {
        State::Marker(mk) => {
            verify::check_marker(&mk)?;
            (mk.round, mk.m, mk.roots, mk.k)
        }
        State::Pool(p) => {
            verify::check_pool(&p)?;
            (p.round, p.m, p.roots, p.k)
        }
        _ => return Err(Error::new(Code::NotTemplate, "neither a marker nor a pool")),
    };
    let trees = cases::trees(m, &purchase_lists(&i.purchases)?)?;
    verify::check_roots(&trees, &roots, k)?;
    Ok(RootsOut { round: round.to_string(), m: m as u32, roots: trees.iter().map(|t| hex(&t.root())).collect() })
}

/// Spec J6: the winning numbers from the anchor proof, for the round the chain objects fix
/// (F1 review: round, m and D are derived here, never passed by the page). Each object is
/// checked against its UTXO entry (state, script, covenant ID); that the entries are live
/// is the page's (it reads them from a public node by outpoint, F1 re-review N1).
/// - `closed`: the round r marker and the round r + 1 controller (`draw()`'s inputs 1 and
///   3): D = the marker's creation DAA score + seed_offset. F1 re-review S1: off-chain,
///   the proof alone leaves X.hash free (anyone can grind it to pick the numbers), so the
///   anchor block as a public node reports it must bind X: its header hashes to X.hash and
///   commits to X's fields, and the node reports X on its selected chain
///   (`verify::check_anchor_block`). The result is `provisional`: the node's chain can
///   still change until `draw()`.
/// - `drawn`: a pool of round r and the drawer: D = the pool's claim deadline -
///   draw_window - claim_window (drawer check 19; draw() copies both windows unchanged),
///   and the proof's numbers must be the pool's `win`, which `draw()` wrote after checking
///   the anchor on chain: `final`.
///
/// A proof that does not anchor D, or an anchor block that does not bind it, is
/// `anchor_invalid`; objects that do not belong together, or numbers that are not the
/// pool's, are `chain_mismatch`.
pub fn winning_numbers(i: &WinningInput) -> Result<WinningOut> {
    let cov = cov_of(&i.covenant_id)?;
    let proof = unhex("anchorProof", &i.anchor_proof)?;
    let p = verify::anchor_proof(&proof)?;
    let mismatch = |what: &str| Error::new(Code::ChainMismatch, what.to_string());
    let (round, m, d, pool_win) = match &i.from {
        RoundObjs::Closed { marker, controller, anchor_block } => {
            verify::check_anchor_block(&p, &convert::header(&anchor_block.header)?, anchor_block.is_chain_block)?;
            let mo = obj("marker", marker)?;
            let mk = cases::marker(&mo, &cov)?;
            verify::check_marker(&mk)?;
            let c = cases::controller(&obj("controller", controller)?, &cov)?;
            verify::check_controller(&c)?;
            if c.round != mk.round + 1 || c.drawn != 0 {
                return Err(mismatch("the controller is not the undrawn controller of the marker's next round"));
            }
            (mk.round, mk.m, lottery::game::anchor_daa(mo.entry.block_daa_score as i64, c.seed_offset), None)
        }
        RoundObjs::Drawn { pool, drawer } => {
            let pl = cases::pool(&obj("pool", pool)?, &cov)?;
            verify::check_pool(&pl)?;
            let dr = cases::drawer(&obj("drawer", drawer)?, &cov)?;
            verify::check_drawer(&dr)?;
            if dr.round <= pl.round {
                return Err(mismatch("the drawer has not drawn the pool's round"));
            }
            let d = pl
                .deadline
                .checked_sub(dr.draw_window)
                .and_then(|x| x.checked_sub(dr.claim_window))
                .ok_or_else(|| mismatch("the pool's deadline is below the drawer's windows"))?;
            if lottery::game::claim_deadline(d, dr.draw_window, dr.claim_window) != Some(pl.deadline) {
                return Err(mismatch("the pool's deadline is not a claim deadline of the drawer's windows"));
            }
            (pl.round, pl.m, d, Some(pl.win))
        }
    };
    let w = verify::winning_numbers_from_anchor(&cov.as_bytes(), round, &proof, d, m)?;
    if pool_win.is_some_and(|win| win != w) {
        return Err(mismatch("the anchor proof's numbers are not the pool's"));
    }
    Ok(WinningOut {
        winning: Combination { main: [w[0], w[1], w[2], w[3], w[4]], bonus: w[5] },
        status: if pool_win.is_some() { NumbersStatus::Final } else { NumbersStatus::Provisional },
        round: round.to_string(),
        m: m as u32,
        d: d.to_string(),
        anchor_hash: hex(&p.x_hash()),
        anchor_daa_score: p.x.daa_score.to_string(),
    })
}

// ------------------------------------------------------------------------------ estimate

pub fn fee_for(i: &FeeInput) -> Result<String> {
    let m = harness::Masses { compute: 0, transient: 0, storage: 0, priority: dec("priorityMass", &i.priority_mass)? };
    Ok(build::fee_of(&m, i.fee_rate as u64)?.to_string())
}

/// The withdrawal minimum of a vault (spec S5, D86; checked against its UTXO entry).
pub fn w_min(covenant_id: &str, vault: &CovUtxo) -> Result<String> {
    let cov = cov_of(covenant_id)?;
    let v = cases::vault(&obj("vault", vault)?, &cov)?;
    Ok(build::w_min(&v)?.to_string())
}

// ------------------------------------------------------------------------------ build

fn cov_of(s: &str) -> Result<Hash> {
    Ok(Hash::from_bytes(hash32("covenantId", s)?))
}

fn coins(c: &[Utxo]) -> Result<Vec<Coin>> {
    if c.len() > build::F_MAX {
        return Err(Error::new(Code::InputRange, format!("{} coins (at most {})", c.len(), build::F_MAX)));
    }
    c.iter().map(|u| utxo(u).map(|(outpoint, entry)| Coin { outpoint, entry })).collect()
}

/// The wallet's funds at `fee`.
fn funds(net: Net, f: &Funds, fee: u64) -> Result<LFunds> {
    Ok(LFunds { coins: coins(&f.coins)?, change: net.script_of("funds.change", &f.change)?, fee })
}

/// Build with the fee loop at `f.fee_rate`, then the boundary form.
fn built(net: Net, f: &Funds, mk: impl Fn(&LFunds) -> Result<Unsigned>) -> Result<BuiltTx> {
    let base = funds(net, f, 0)?;
    let u = build::with_fee(f.fee_rate as u64, |fee| mk(&LFunds { fee, ..base.clone() }))?;
    built_tx(net, &u)
}

fn kind_of(o: &OwnerPub) -> (KeyKind, Vec<u8>) {
    match o {
        OwnerPub::Schnorr(k) => (KeyKind::Schnorr, k.to_vec()),
        OwnerPub::Ecdsa(k) => (KeyKind::Ecdsa, k.to_vec()),
        OwnerPub::P2sh(k) => (KeyKind::P2sh, k.to_vec()),
    }
}

fn owner_pub(kind: KeyKind, key: &[u8]) -> Result<OwnerPub> {
    let bad = || Error::new(Code::BytesLength, format!("public key of {} bytes for {kind:?}", key.len()));
    Ok(match kind {
        KeyKind::Schnorr => OwnerPub::Schnorr(key.try_into().map_err(|_| bad())?),
        KeyKind::Ecdsa => OwnerPub::Ecdsa(key.try_into().map_err(|_| bad())?),
        KeyKind::P2sh => OwnerPub::P2sh(key.try_into().map_err(|_| bad())?),
    })
}

fn masses_out(m: &harness::Masses) -> MassesOut {
    MassesOut { compute: m.compute.to_string(), transient: m.transient.to_string(), storage: m.storage.to_string(), priority: m.priority.to_string() }
}

/// The boundary form of an unsigned transaction. Every field is a function of the
/// transaction, its entries and the inputs to sign (F1 review), so `unsigned_of` can
/// recompute it: the wallet's coins are the inputs it signs, and an output comes back to the
/// wallet when it is a plain output paying the script of one of them.
///
/// F1 re-review N2: a page build has one payer. Every input to sign must be of one key (one
/// owner, so one wallet address), or the build is refused (`mixed_signers`): otherwise an
/// output paying a second key would be shown as coming back to the wallet. A withdrawal's
/// owner coin and its funding coins are then the same wallet's, as on the page.
pub fn built_tx(net: Net, u: &Unsigned) -> Result<BuiltTx> {
    if let Some(first) = u.sign.first() {
        if let Some(s) = u.sign.iter().find(|s| s.owner != first.owner) {
            return Err(Error::new(Code::MixedSigners, format!("inputs {} and {} to sign are of two keys", first.index, s.index)));
        }
    }
    let idx = u.sign_indices();
    let mut wallet_in = 0u64;
    let mut wallet_scripts = vec![];
    for &i in &idx {
        let e = u.entries.get(i).ok_or_else(|| Error::new(Code::InputRange, format!("sign index {i}")))?;
        wallet_in = wallet_in.checked_add(e.amount).ok_or_else(|| Error::new(Code::Overflow, "wallet total"))?;
        wallet_scripts.push(&e.script_public_key);
    }
    let transaction = safe_json::to_safe_json(&u.tx, &u.entries, &idx, net.prefix()).map_err(|e| Error::new(Code::Encode, e))?;
    let sign = u
        .sign
        .iter()
        .map(|s| {
            let (kind, key) = kind_of(&s.owner);
            SignItem { index: s.index as u32, sighash_type: SIG_HASH_ALL, kind, public_key: hex(&key), address: u.entries.get(s.index).and_then(|e| net.address(&e.script_public_key)) }
        })
        .collect();
    let mut back = 0u64;
    let outputs = u
        .tx
        .outputs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let to_wallet = o.covenant.is_none() && wallet_scripts.contains(&&o.script_public_key);
            if to_wallet {
                back = back.saturating_add(o.value);
            }
            OutputItem { index: i as u32, value: o.value.to_string(), address: net.address(&o.script_public_key), covenant: o.covenant.is_some(), to_wallet }
        })
        .collect();
    Ok(BuiltTx {
        transaction,
        id: u.tx.id().to_string(),
        sign,
        masses: masses_out(&u.masses),
        fee: u.fee.to_string(),
        units: u.units.iter().map(|x| x.to_string()).collect(),
        summary: Summary { wallet_in: wallet_in.to_string(), wallet_back: back.to_string(), outputs },
    })
}

fn combos(b: &[Combination]) -> Result<Vec<Combo>> {
    if b.is_empty() || b.len() > b_max() {
        return Err(Error::new(Code::InputRange, format!("{} combinations (1..={})", b.len(), b_max())));
    }
    Ok(b.iter().map(|c| Combo::new(c.main, c.bonus)).collect())
}

pub fn build_buy(net: Net, i: &BuyInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let shard = obj("shard", &i.shard)?;
    let purchases = unhex("purchases", &i.purchases)?;
    let choice = BuyChoice {
        bundle: combos(&i.bundle)?,
        payout: net.payout_of("payout", &i.payout)?,
        referrer: i.referrer.as_deref().map(|r| net.payout_of("referrer", r)).transpose()?,
    };
    built(net, &i.funds, |f| cases::buy(&cov, &shard, &purchases, &choice, f))
}

/// Spec S1, 7 (front end F3 part B): the cost panel of a built `buy()`, every figure computed
/// here and checked against the transaction (`BuyFigures`). `built` must pass `unsigned_of`
/// first (each of its own figures bound to its transaction), and the shard must be a live
/// object of the instance (`cases::live`; its entry from a public node by outpoint). A figure
/// the transaction does not bear out is `built_mismatch`.
pub fn buy_figures(net: Net, i: &BuyFiguresInput) -> Result<BuyFigures> {
    use lottery::state::TicketState;
    let cov = cov_of(&i.covenant_id)?;
    let u = unsigned_of(net, &i.built)?;
    let so = obj("shard", &i.shard)?;
    let s = cases::shard(&so, &cov)?;
    verify::check_shard(&s)?;
    let bad = |what: &str| Error::new(Code::BuiltMismatch, format!("buy figures: {what}"));
    if u.tx.inputs.first().map(|x| x.previous_outpoint) != Some(so.outpoint) || u.entries.first() != Some(&so.entry) {
        return Err(bad("input 0 does not spend the given shard"));
    }
    let bundle = combos(&i.bundle)?;
    let payout = net.payout_of("payout", &i.payout)?;
    let referrer = i.referrer.as_deref().map(|r| net.payout_of("referrer", r)).transpose()?;
    let b = bundle.len() as u64;
    let of = |what: &str| Error::new(Code::Overflow, what.to_string());
    let price = b.checked_mul(s.price as u64).ok_or_else(|| of("b x price"))?;
    let deposit = b.checked_mul(s.d as u64).ok_or_else(|| of("b x d"))?;
    let ticket = State::Ticket(TicketState::new(&s, &bundle, &payout, b_max()));
    let tspk = verify::state_spk(&ticket)?;
    let out = |n: usize| u.tx.outputs.get(n).ok_or_else(|| bad(&format!("no output {n}")));
    let t = out(1)?;
    if t.script_public_key != tspk || t.value != deposit || t.covenant.is_none() {
        return Err(bad("output 1 is not this bundle's ticket holding b x d"));
    }
    let (r, enough) = cases::referral(&s, bundle.len())?;
    let paid = enough && referrer.is_some();
    let r_out = if paid { r } else { 0 };
    let succ = so.entry.amount.checked_add(price).and_then(|x| x.checked_sub(r_out)).ok_or_else(|| of("shard successor"))?;
    let s0 = out(0)?;
    if s0.value != succ || s0.covenant.is_none() {
        return Err(bad("output 0 is not the shard's successor value"));
    }
    let mut rest = 2;
    if paid {
        let ro = out(2)?;
        let want = referrer.as_ref().map(|p| p.spk());
        if ro.value != r || ro.covenant.is_some() || Some(&ro.script_public_key) != want.as_ref() {
            return Err(bad("output 2 is not the referral"));
        }
        rest = 3;
    }
    for (n, o) in i.built.summary.outputs.iter().enumerate().skip(rest) {
        if !o.to_wallet {
            return Err(bad(&format!("output {n} is neither the referral nor back to the wallet")));
        }
    }
    let fee = u.fee;
    let wallet_in = dec("summary.walletIn", &i.built.summary.wallet_in)?;
    let wallet_back = dec("summary.walletBack", &i.built.summary.wallet_back)?;
    let total = wallet_in.checked_sub(wallet_back).ok_or_else(|| of("wallet total"))?;
    if Some(total) != price.checked_add(deposit).and_then(|x| x.checked_add(fee)) {
        return Err(bad("what the wallet loses is not price + deposit + fee"));
    }
    let back = s.deadline.checked_add(lottery::game::D_RET).ok_or_else(|| of("T + D_RET"))?;
    let tbytes = verify::state_bytes(&ticket)?;
    let referral_address = if paid { referrer.as_ref().and_then(|p| net.address(&p.spk())) } else { None };
    Ok(BuyFigures {
        round: s.round.to_string(),
        shard_index: s.shard_index as u32,
        b: b as u32,
        price: price.to_string(),
        deposit: deposit.to_string(),
        deposit_back: back.to_string(),
        referral: ReferralFigure { amount: r.to_string(), paid, address: referral_address },
        fee: fee.to_string(),
        total: total.to_string(),
        ticket: TicketOut {
            index: 1,
            state: hex(&tbytes),
            script_public_key: convert::spk_hex(&tspk),
            address: net.address(&tspk).ok_or_else(|| bad("the ticket has no address"))?,
        },
    })
}

pub fn build_claim(net: Net, i: &ClaimInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let ticket = obj("ticket", &i.ticket)?;
    if i.pools.len() > 64 {
        return Err(Error::new(Code::InputRange, format!("{} pools", i.pools.len())));
    }
    let pools = i.pools.iter().map(|p| obj("pools", p)).collect::<Result<Vec<_>>>()?;
    let purchases = purchase_lists(&i.purchases)?;
    built(net, &i.funds, |f| cases::claim(&cov, &ticket, &pools, &purchases, i.index.map(|x| x as i64), f))
}

pub fn build_return_deposit(net: Net, i: &ReturnInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let ticket = obj("ticket", &i.ticket)?;
    let pool = i.pool.as_ref().map(|p| obj("pool", p)).transpose()?;
    built(net, &i.funds, |f| cases::return_deposit(&cov, &ticket, pool.as_ref(), f))
}

pub fn build_close(net: Net, i: &CloseInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let controller = obj("controller", &i.controller)?;
    let params = obj("params", &i.params)?;
    if i.shards.len() > K_MAX {
        return Err(Error::new(Code::InputRange, format!("{} shards", i.shards.len())));
    }
    let shards = i.shards.iter().map(|s| obj("shards", s)).collect::<Result<Vec<_>>>()?;
    let now = dec("virtualDaaScore", &i.virtual_daa_score)?;
    let funder = net.payout_of("funds.change", &i.funds.change)?;
    built(net, &i.funds, |f| cases::close(&cov, &controller, &params, &shards, funder, f, now))
}

pub fn build_draw(net: Net, i: &DrawInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let (drawer, marker, vault, controller) = (obj("drawer", &i.drawer)?, obj("marker", &i.marker)?, obj("vault", &i.vault)?, obj("controller", &i.controller)?);
    if i.absorbed.len() > lottery::game::A_MAX {
        return Err(Error::new(Code::InputRange, format!("{} absorbed pools", i.absorbed.len())));
    }
    let absorbed = i.absorbed.iter().map(|a| obj("absorbed", a)).collect::<Result<Vec<_>>>()?;
    let purchases = purchase_lists(&i.purchases)?;
    let proof = i.anchor_proof.as_deref().map(|p| unhex("anchorProof", p)).transpose()?;
    let funder = net.payout_of("funds.change", &i.funds.change)?;
    built(net, &i.funds, |f| {
        let o = DrawObjs { drawer: &drawer, marker: &marker, vault: &vault, controller: &controller, absorbed: &absorbed };
        cases::draw(&cov, &o, &purchases, proof.as_deref(), funder, f)
    })
}

/// `request_deposit()` for `owner`. F1 review: the owner must be a Schnorr P2PK address
/// (`owner_type` otherwise). D67 accepts any P2PK or P2SH owner at the covenant level, but
/// the page can only withdraw for owners its signers sign: KasWare and the quick wallet sign
/// Schnorr P2PK inputs only, so a position of any other owner type would be unwithdrawable
/// from the page.
pub fn build_request_deposit(net: Net, i: &DepositInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let vault = obj("vault", &i.vault)?;
    let d = dec_i64("amount", &i.amount)?;
    let owner = net.payout_of("owner", &i.owner)?;
    if !matches!(owner, Payout::Schnorr(_)) {
        return Err(Error::new(Code::OwnerType, format!("owner of class {}: the page withdraws for Schnorr P2PK owners only", owner.class())));
    }
    built(net, &i.funds, |f| cases::request_deposit(&cov, &vault, d, owner, f))
}

pub fn build_request_withdraw(net: Net, i: &WithdrawInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let (vault, position) = (obj("vault", &i.vault)?, obj("position", &i.position)?);
    let w = dec_i64("shares", &i.shares)?;
    let (outpoint, entry) = utxo(&i.owner.coin)?;
    let owner = OwnerCoin { coin: Coin { outpoint, entry: entry.clone() }, owner: owner_pub(i.owner.kind, &unhex("owner.publicKey", &i.owner.public_key)?)? };
    // The owner coin must be at a standard address (the builder then requires it to be the
    // position owner's script, D67).
    net.address(&entry.script_public_key).ok_or_else(|| Error::new(Code::WalletScript, "the owner coin's script has no address"))?;
    built(net, &i.funds, |f| cases::request_withdraw(&cov, &vault, &position, w, &owner, f))
}

pub fn build_settle(net: Net, i: &SettleInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let (request, record) = (obj("request", &i.request)?, obj("record", &i.record)?);
    built(net, &i.funds, |f| cases::settle(&cov, &request, &record, f))
}

// ------------------------------------------------------------------------------ assemble

/// The `Unsigned` behind a `BuiltTx` (as `built_tx` wrote it), checked for self-consistency
/// and against its own figures (spec 7, F1 review):
/// - the inputs with an empty signature script are exactly the listed ones, in input order,
///   each listed owner is the one its UTXO's script names, and each is asked for
///   SIG_HASH_ALL;
/// - every figure is recomputed from the transaction and must equal the given one: the fee
///   (inputs - outputs), the masses (each listed input at its signed size), the units (a
///   listed input's are its signer class's, and every input's give its committed budget),
///   the ID, the sign items and the whole summary (`built_tx` again, field by field).
///
/// A structural fault is `input_format`; a figure that differs is `built_mismatch`. So what
/// the page showed is what the transaction does, or nothing is signed or sent.
pub fn unsigned_of(net: Net, b: &BuiltTx) -> Result<Unsigned> {
    let (tx, entries, form) = safe_json::from_json(&b.transaction).map_err(|e| Error::new(Code::InputFormat, format!("built.transaction: {e}")))?;
    if form != "safe" {
        return Err(Error::new(Code::InputFormat, "built.transaction is not Safe JSON"));
    }
    let entries: Vec<UtxoEntry> = entries.into_iter().collect::<Option<_>>().ok_or_else(|| Error::new(Code::InputFormat, "built.transaction lacks a UTXO entry"))?;
    if entries.len() != tx.inputs.len() || b.units.len() != tx.inputs.len() {
        return Err(Error::new(Code::InputFormat, "built: entries or units do not match the inputs"));
    }
    let mut sign: Vec<SignInput> = vec![];
    for s in &b.sign {
        let i = s.index as usize;
        let owner = owner_pub(s.kind, &unhex("sign.publicKey", &s.public_key)?)?;
        let e = entries.get(i).ok_or_else(|| Error::new(Code::InputRange, format!("sign index {i}")))?;
        if owner.payout().spk() != e.script_public_key || s.sighash_type != SIG_HASH_ALL {
            return Err(Error::new(Code::InputFormat, format!("sign item {i}: not the owner of its UTXO, or not SIG_HASH_ALL")));
        }
        if sign.last().is_some_and(|l| l.index >= i) {
            return Err(Error::new(Code::InputFormat, format!("sign item {i}: not in input order")));
        }
        sign.push(SignInput { index: i, owner });
    }
    for (i, inp) in tx.inputs.iter().enumerate() {
        let listed = sign.iter().any(|s| s.index == i);
        if listed != inp.signature_script.is_empty() {
            return Err(Error::new(Code::InputFormat, format!("input {i}: listed = {listed} but its signature script is {} bytes", inp.signature_script.len())));
        }
    }
    let units = b.units.iter().map(|u| dec("units", u)).collect::<Result<Vec<_>>>()?;
    let differs = |what: &str| Error::new(Code::BuiltMismatch, format!("built.{what} is not what its transaction gives"));
    // The fee: inputs - outputs.
    let ins = entries.iter().try_fold(0u64, |a, e| a.checked_add(e.amount));
    let outs = tx.outputs.iter().try_fold(0u64, |a, o| a.checked_add(o.value));
    let fee = ins.zip(outs).and_then(|(i, o)| i.checked_sub(o)).ok_or_else(|| Error::new(Code::Overflow, "built: inputs below outputs, or a total overflows"))?;
    if dec("fee", &b.fee)? != fee {
        return Err(differs("fee"));
    }
    // The masses of the signed transaction: each listed input at its signed size.
    let mut sized = tx.clone();
    for s in &sign {
        sized.inputs[s.index].signature_script = s.owner.placeholder();
    }
    let masses = harness::try_masses(&sized, &entries).map_err(|e| Error::new(Code::Mass, e))?;
    let m = &b.masses;
    let given = (dec("masses.compute", &m.compute)?, dec("masses.transient", &m.transient)?, dec("masses.storage", &m.storage)?, dec("masses.priority", &m.priority)?);
    if given != (masses.compute, masses.transient, masses.storage, masses.priority) || tx.storage_mass() != masses.storage {
        return Err(differs("masses"));
    }
    // The units: a listed input's are its signer class's; a covenant input's are those of its
    // run here (its signature script is final), except the drawer's, whose
    // OpChainblockSeqCommit needs the node's chain: its units are bound by the committed
    // budget alone. Every input's units give its committed budget.
    for (i, (inp, &u)) in tx.inputs.iter().zip(&units).enumerate() {
        let want = match sign.iter().find(|s| s.index == i) {
            Some(s) => Some(lottery::buy::external_signer_units(&s.owner).map_err(|e| Error::new(Code::Encode, e))?),
            None => match harness::execute(&sized, &entries, i, false) {
                Ok(n) => Some(n),
                // OpChainblockSeqCommit is opcode 0xd4 (rusty-kaspa v2.1.0
                // `crypto/txscript/src/opcodes/mod.rs:1387`); with no chain accessor it is
                // InvalidOpcode, printed `Opcode<0xd4>`.
                Err(harness::TxScriptError::InvalidOpcode(op)) if op.contains("<0xd4>") => None,
                Err(e) => return Err(Error::new(Code::Engine, format!("covenant input {i}: {e}"))),
            },
        };
        let budget = harness::try_budget_for(u).map_err(|e| Error::new(Code::Encode, e))?;
        if want.is_some_and(|w| w != u) || inp.compute_commit.compute_budget() != Some(budget) {
            return Err(differs(&format!("units[{i}]")));
        }
    }
    let u = Unsigned { tx, entries, sign, masses, fee, units };
    // Everything else, field by field: the boundary form recomputed from `u`.
    let again = built_tx(net, &u)?;
    for (what, same) in [
        ("transaction", again.transaction == b.transaction),
        ("id", again.id == b.id),
        ("sign", again.sign == b.sign),
        ("masses", again.masses == b.masses),
        ("fee", again.fee == b.fee),
        ("units", again.units == b.units),
        ("summary.walletIn", again.summary.wallet_in == b.summary.wallet_in),
        ("summary.walletBack", again.summary.wallet_back == b.summary.wallet_back),
        ("summary.outputs", again.summary.outputs == b.summary.outputs),
    ] {
        if !same {
            return Err(differs(what));
        }
    }
    if again != *b {
        return Err(differs("(a field)"));
    }
    Ok(u)
}

/// Spec 6.2: the post-sign check on the wallet's returned Safe JSON, then the final
/// transaction (ours, with only the listed signature scripts taken from the wallet's), each
/// signed input checked in the local engine. `built` must be exactly what a builder returned:
/// `unsigned_of` checks every figure the page showed against the transaction first.
pub fn assemble(net: Net, i: &AssembleInput) -> Result<Signed> {
    let u = unsigned_of(net, &i.built)?;
    let (got, got_entries, _) = safe_json::from_json(&i.signed).map_err(|e| Error::new(Code::InputFormat, format!("signed: {e}")))?;
    let tx: Transaction = sign::assemble(&u, &got, &got_entries)?;
    let transaction = safe_json::to_safe_json(&tx, &u.entries, &[], net.prefix()).map_err(|e| Error::new(Code::Encode, e))?;
    Ok(Signed { transaction, id: tx.id().to_string() })
}

/// The address of a script public key (Safe JSON form) on the page's network.
pub fn address_of(net: Net, spk: &str) -> Result<String> {
    net.address(&convert::spk("scriptPublicKey", spk)?).ok_or_else(|| Error::new(Code::WalletScript, "not a standard script"))
}

/// The script public key (Safe JSON form) of an address of the page's network.
pub fn script_of(net: Net, address: &str) -> Result<String> {
    Ok(convert::spk_hex(&net.script_of("address", address)?))
}
