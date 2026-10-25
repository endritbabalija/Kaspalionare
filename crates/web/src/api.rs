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

/// Spec 6.3: a state from the indexer against its live UTXO (script and instance).
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

/// Spec 6.3, J6: the shard trees rebuilt from their purchases against a live marker's or
/// pool's frozen roots.
pub fn check_roots(i: &RootsInput) -> Result<RootsOut> {
    let cov = cov_of(&i.covenant_id)?;
    let o = obj("against", &i.against)?;
    let (roots, k) = match cases::live(lottery::index::Role::Marker, &o, &cov).or_else(|_| cases::live(lottery::index::Role::Pool, &o, &cov))? {
        lottery::index::State::Marker(m) => (m.roots, m.k),
        lottery::index::State::Pool(p) => (p.roots, p.k),
        _ => return Err(Error::new(Code::NotTemplate, "neither a marker nor a pool")),
    };
    let trees = cases::trees(i.m as i64, &purchase_lists(&i.purchases)?)?;
    verify::check_roots(&trees, &roots, k)?;
    Ok(RootsOut { roots: trees.iter().map(|t| hex(&t.root())).collect() })
}

/// Spec J6: the winning numbers from the anchor proof (checked to anchor D).
pub fn winning_numbers(i: &WinningInput) -> Result<WinningOut> {
    let cov = hash32("covenantId", &i.covenant_id)?;
    let proof = unhex("anchorProof", &i.anchor_proof)?;
    let p = verify::anchor_proof(&proof)?;
    let w = verify::winning_numbers_from_anchor(&cov, dec_i64("round", &i.round)?, &proof, dec_i64("d", &i.d)?, i.m as i64)?;
    Ok(WinningOut {
        winning: Combination { main: [w[0], w[1], w[2], w[3], w[4]], bonus: w[5] },
        anchor_hash: hex(&p.x_hash()),
        anchor_daa_score: p.x.daa_score.to_string(),
    })
}

// ------------------------------------------------------------------------------ estimate

pub fn fee_for(i: &FeeInput) -> Result<String> {
    let m = harness::Masses { compute: 0, transient: 0, storage: 0, priority: dec("priorityMass", &i.priority_mass)? };
    Ok(build::fee_of(&m, i.fee_rate as u64)?.to_string())
}

/// The withdrawal minimum of a live vault (spec S5, D86).
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
fn built(net: Net, f: &Funds, extra_wallet: &[(u64, &kaspa_consensus_core::tx::ScriptPublicKey)], mk: impl Fn(&LFunds) -> Result<Unsigned>) -> Result<BuiltTx> {
    let base = funds(net, f, 0)?;
    let u = build::with_fee(f.fee_rate as u64, |fee| mk(&LFunds { fee, ..base.clone() }))?;
    let wallet_in = base.coins.iter().map(|c| c.entry.amount).chain(extra_wallet.iter().map(|x| x.0)).try_fold(0u64, |a, v| a.checked_add(v)).ok_or_else(|| Error::new(Code::Overflow, "wallet total"))?;
    built_tx(net, &u, &base.change, wallet_in)
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

/// The boundary form of an unsigned transaction.
pub fn built_tx(net: Net, u: &Unsigned, change: &kaspa_consensus_core::tx::ScriptPublicKey, wallet_in: u64) -> Result<BuiltTx> {
    let idx = u.sign_indices();
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
            let to_wallet = o.script_public_key == *change;
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
    built(net, &i.funds, &[], |f| cases::buy(&cov, &shard, &purchases, &choice, f))
}

pub fn build_claim(net: Net, i: &ClaimInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let ticket = obj("ticket", &i.ticket)?;
    if i.pools.len() > 64 {
        return Err(Error::new(Code::InputRange, format!("{} pools", i.pools.len())));
    }
    let pools = i.pools.iter().map(|p| obj("pools", p)).collect::<Result<Vec<_>>>()?;
    let purchases = purchase_lists(&i.purchases)?;
    built(net, &i.funds, &[], |f| cases::claim(&cov, &ticket, &pools, &purchases, i.index.map(|x| x as i64), f))
}

pub fn build_return_deposit(net: Net, i: &ReturnInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let ticket = obj("ticket", &i.ticket)?;
    let pool = i.pool.as_ref().map(|p| obj("pool", p)).transpose()?;
    built(net, &i.funds, &[], |f| cases::return_deposit(&cov, &ticket, pool.as_ref(), f))
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
    built(net, &i.funds, &[], |f| cases::close(&cov, &controller, &params, &shards, funder, f, now))
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
    built(net, &i.funds, &[], |f| {
        let o = DrawObjs { drawer: &drawer, marker: &marker, vault: &vault, controller: &controller, absorbed: &absorbed };
        cases::draw(&cov, &o, &purchases, proof.as_deref(), funder, f)
    })
}

pub fn build_request_deposit(net: Net, i: &DepositInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let vault = obj("vault", &i.vault)?;
    let d = dec_i64("amount", &i.amount)?;
    let owner = net.payout_of("owner", &i.owner)?;
    built(net, &i.funds, &[], |f| cases::request_deposit(&cov, &vault, d, owner, f))
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
    built(net, &i.funds, &[(entry.amount, &entry.script_public_key)], |f| cases::request_withdraw(&cov, &vault, &position, w, &owner, f))
}

pub fn build_settle(net: Net, i: &SettleInput) -> Result<BuiltTx> {
    let cov = cov_of(&i.covenant_id)?;
    let (request, record) = (obj("request", &i.request)?, obj("record", &i.record)?);
    built(net, &i.funds, &[], |f| cases::settle(&cov, &request, &record, f))
}

// ------------------------------------------------------------------------------ assemble

/// The `Unsigned` behind a `BuiltTx` (as `built_tx` wrote it), checked for self-consistency:
/// the inputs with an empty signature script are exactly the listed ones, and each listed
/// owner is the one its UTXO's script names.
pub fn unsigned_of(b: &BuiltTx) -> Result<Unsigned> {
    let (tx, entries, form) = safe_json::from_json(&b.transaction).map_err(|e| Error::new(Code::InputFormat, format!("built.transaction: {e}")))?;
    if form != "safe" {
        return Err(Error::new(Code::InputFormat, "built.transaction is not Safe JSON"));
    }
    let entries: Vec<UtxoEntry> = entries.into_iter().collect::<Option<_>>().ok_or_else(|| Error::new(Code::InputFormat, "built.transaction lacks a UTXO entry"))?;
    if entries.len() != tx.inputs.len() || b.units.len() != tx.inputs.len() {
        return Err(Error::new(Code::InputFormat, "built: entries or units do not match the inputs"));
    }
    let mut sign = vec![];
    for s in &b.sign {
        let i = s.index as usize;
        let owner = owner_pub(s.kind, &unhex("sign.publicKey", &s.public_key)?)?;
        let e = entries.get(i).ok_or_else(|| Error::new(Code::InputRange, format!("sign index {i}")))?;
        if owner.payout().spk() != e.script_public_key || s.sighash_type != SIG_HASH_ALL {
            return Err(Error::new(Code::InputFormat, format!("sign item {i}: not the owner of its UTXO, or not SIG_HASH_ALL")));
        }
        sign.push(SignInput { index: i, owner });
    }
    for (i, inp) in tx.inputs.iter().enumerate() {
        let listed = sign.iter().any(|s| s.index == i);
        if listed != inp.signature_script.is_empty() {
            return Err(Error::new(Code::InputFormat, format!("input {i}: listed = {listed} but its signature script is {} bytes", inp.signature_script.len())));
        }
    }
    let m = &b.masses;
    let masses = harness::Masses {
        compute: dec("masses.compute", &m.compute)?,
        transient: dec("masses.transient", &m.transient)?,
        storage: dec("masses.storage", &m.storage)?,
        priority: dec("masses.priority", &m.priority)?,
    };
    let units = b.units.iter().map(|u| dec("units", u)).collect::<Result<Vec<_>>>()?;
    Ok(Unsigned { tx, entries, sign, masses, fee: dec("fee", &b.fee)?, units })
}

/// Spec 6.2: the post-sign check on the wallet's returned Safe JSON, then the final
/// transaction (ours, with only the listed signature scripts taken from the wallet's), each
/// signed input checked in the local engine.
pub fn assemble(net: Net, i: &AssembleInput) -> Result<Signed> {
    let u = unsigned_of(&i.built)?;
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
