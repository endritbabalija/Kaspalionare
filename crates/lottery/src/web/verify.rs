//! Decoders, chain checks and state ranges of the web-facing layer (spec frontend 6.1
//! "decode" and "verify", 6.3 "nothing from the API is trusted"; F1 part B). No function
//! here panics on any input (W3, `tests/f1_w3.rs`).
//!
//! - Decoders: `decode_state` (a role's encoded state bytes, as the indexer API and
//!   `bundle/1` serve them), `decode_redeem` (a full redeem script), `anchor_proof`,
//!   `read_wallet`, `read_index_log`, `read_index_snapshot`, `read_bundle`.
//! - Chain checks: `verify_state` (the state's P2SH equals the given UTXO entry's script,
//!   6.3; that the entry is live, i.e. unspent, is the caller's to know: it reads the entry
//!   from a node by outpoint),
//!   `rebuild_tree` and `check_roots` (purchases rebuilt into a shard tree and compared with
//!   the frozen roots), `winning_numbers` (the seed mapping from the anchor's X.hash, D42/D94),
//!   `check_anchor_block` (the proof's X against the anchor block's header from a node).
//! - Ranges: `check_shard`, `check_ticket`, `check_pool`, `check_controller`, `check_params`,
//!   `check_marker`, `check_drawer`, `check_vault`, `check_position`, `check_request`,
//!   `check_record`: the range checks each covenant applies to its own state (cited per
//!   function), which the builders apply before any arithmetic so the Rust reference never
//!   leaves its domain. A state that fails one is refused (`Code::StateRange`); no
//!   chain-valid state fails one, since each covenant refuses to create such a state.

use std::sync::Arc;

use harness::{ScriptPublicKey, pay_to_script_hash_script};

use super::{Code, Error, Result, WithCode, ensure};
use crate::anchor::AnchorProof;
use crate::buy::{Templates, b_max};
use crate::combo::{Combo, MAIN_MAX};
use crate::decode::state_values;
use crate::game::{
    ASSETS_MAX, BP, DAA_MAX, F_MIN_FLOOR, F_MIN_MAX, M_CLAIM, M_DRAW, M_HARD, MARGIN_MIN, MIN_DEPOSIT, S_F_CAP, S_R_CAP, SHARES_MAX, SeedNumbers,
    TICKET_B_MAX, TOTAL_MAX, VOID_MIN,
};
use crate::index::{Role, State, template_of, typed};
use crate::smt::Tree;
use crate::state::{
    ControllerState, DrawerState, KIND_DEPOSIT, KIND_WITHDRAW, K_MAX, MarkerState, ParamsState, Payout, PoolState, PositionState, RecordState,
    RequestState, ShardState, TicketState, VaultState,
};

/// Covenant script constants mirrored here (each `int constant` of the covenant named).
/// shard.sil, controller.sil and drawer.sil `PRICE_MAX`.
pub const PRICE_MAX: i64 = 100_000_000_000;
/// shard.sil and controller.sil `DEPOSIT_MAX`.
pub const DEPOSIT_MAX: i64 = 10_000_000_000;
/// controller.sil, drawer.sil, ticket.sil, vault.sil and request.sil `RESERVE_MAX`.
pub const RESERVE_MAX: i64 = 10_000_000_000;
/// The longest state-bytes or redeem input the decoders read (twice the largest committed
/// template script, the drawer's; anything longer is refused before it is copied).
pub const INPUT_MAX: usize = 1 << 20;

/// The embedded templates (the committed B_MAX).
pub fn templates() -> Result<Arc<Templates>> {
    Templates::try_get(b_max()).code(Code::Templates)
}

// ------------------------------------------------------------------------------ decode

/// The state of `role` whose encoded state bytes are `bytes` (the indexer API's and
/// `bundle/1`'s form): decoded under the role's embedded template and required to re-encode
/// to the same bytes.
pub fn decode_state(role: Role, bytes: &[u8]) -> Result<State> {
    ensure(bytes.len() <= INPUT_MAX, Code::BytesLength, || format!("{} state bytes", bytes.len()))?;
    let t = templates()?;
    let tpl = template_of(&t, role);
    let redeem = [tpl.prefix.as_slice(), bytes, &tpl.suffix].concat();
    let v = state_values(tpl, &redeem).code(Code::NotTemplate)?;
    let s = typed(role, &v).code(Code::NotTemplate)?;
    let again = s.try_redeem(&t).code(Code::NotCanonical)?;
    ensure(again == redeem, Code::NotCanonical, || format!("{role:?} state bytes do not re-encode to themselves"))?;
    Ok(s)
}

/// The state of a full redeem script, by the embedded template whose prefix and suffix it
/// carries (re-encoding checked).
pub fn decode_redeem(redeem: &[u8]) -> Result<State> {
    ensure(redeem.len() <= INPUT_MAX, Code::BytesLength, || format!("{} redeem bytes", redeem.len()))?;
    let t = templates()?;
    State::decode(&t, redeem).code(Code::NotTemplate)
}

/// The encoded state bytes of a state (the inverse of `decode_state`).
pub fn state_bytes(s: &State) -> Result<Vec<u8>> {
    s.try_state_bytes(&*templates()?).code(Code::Encode)
}

/// The P2SH script public key of a state (what its UTXO carries).
pub fn state_spk(s: &State) -> Result<ScriptPublicKey> {
    Ok(pay_to_script_hash_script(&s.try_redeem(&*templates()?).code(Code::Encode)?))
}

/// Spec 6.3: the state of `role` from the indexer's `bytes`, checked against the live
/// UTXO's script public key read from a public node. A mismatch is `ChainMismatch` ("the
/// indexer's copy does not match the chain").
pub fn verify_state(role: Role, bytes: &[u8], utxo_spk: &ScriptPublicKey) -> Result<State> {
    let s = decode_state(role, bytes)?;
    ensure(state_spk(&s)? == *utxo_spk, Code::ChainMismatch, || format!("{role:?}: the state's P2SH is not the UTXO's script"))?;
    Ok(s)
}

/// The 272-byte anchor proof of `draw()` (spec M7 A3).
pub fn anchor_proof(bytes: &[u8]) -> Result<AnchorProof> {
    AnchorProof::from_bytes(bytes).code(Code::BytesLength)
}

/// The off-chain part of drawer check 8 for reference `d` (P is X's selected parent by
/// commitment, dX >= D > dP).
pub fn check_anchor(proof: &AnchorProof, d: i64) -> Result<()> {
    ensure(proof.consistent_with(d), Code::AnchorInvalid, || format!("the anchor proof does not anchor D = {d}"))
}

/// F1 re-review S1: the proof's X bound to the anchor block as a public node reports it,
/// the way `draw()` binds it on chain (`OpChainblockSeqCommit(X.hash)` returns the stored
/// header's commitment only for a selected-chain block, rusty-kaspa v2.1.0
/// `crypto/txscript/src/opcodes/mod.rs:1389-1404`): `header` hashes to X.hash and commits
/// to X's fields (`BlockFields::check_header`), and the node reports X on its selected
/// chain (`is_chain_block`, its `getBlock` verbose data). Before `draw()` this is the node's
/// view, which can still change (a reorg), so numbers checked this way are provisional.
pub fn check_anchor_block(proof: &AnchorProof, header: &kaspa_consensus_core::header::Header, is_chain_block: bool) -> Result<()> {
    proof.x.check_header(header).map_err(|e| Error::new(Code::AnchorInvalid, format!("anchor block: {e}")))?;
    ensure(is_chain_block, Code::AnchorInvalid, || "anchor block: the node does not report X on its selected chain".to_string())
}

/// The winning numbers of a round (D42, spec M7 D94): instance ID `cov`, the marker's round
/// and m, and the anchor block's hash `x_hash`. `m` must be one the drawer draws (2..=M_DRAW).
pub fn winning_numbers(cov: &[u8; 32], round: i64, x_hash: &[u8; 32], m: i64) -> Result<SeedNumbers> {
    ensure((2..=M_DRAW).contains(&m), Code::InputRange, || format!("m = {m} outside [2, {M_DRAW}]"))?;
    ensure((1..TOTAL_MAX).contains(&round), Code::InputRange, || format!("round {round}"))?;
    Ok(crate::game::seed_numbers(cov, round, x_hash, m))
}

/// The winning numbers from the anchor proof bytes, after checking the proof anchors `d`
/// (verify on the round page: the page recomputes them from the chain's anchor).
pub fn winning_numbers_from_anchor(cov: &[u8; 32], round: i64, anchor: &[u8], d: i64, m: i64) -> Result<[u8; 6]> {
    let p = anchor_proof(anchor)?;
    check_anchor(&p, d)?;
    let s = winning_numbers(cov, round, &p.x_hash(), m)?;
    Ok([s.main[0], s.main[1], s.main[2], s.main[3], s.main[4], s.bonus])
}

// ------------------------------------------------------------------------------ trees

/// One shard's counter tree rebuilt from its purchases in compact form (spec 6.3: each
/// combination's six bytes, in the shard's purchase order), for a round with bonus range m.
pub fn rebuild_tree(m: i64, purchases: &[[u8; 6]]) -> Result<Tree> {
    ensure((2..=M_HARD).contains(&m), Code::InputRange, || format!("m = {m} outside [2, {M_HARD}]"))?;
    let mut tree = Tree::new(m as usize);
    for (n, c) in purchases.iter().enumerate() {
        let combo = Combo::new([c[0], c[1], c[2], c[3], c[4]], c[5]);
        ensure(combo.is_canonical(m as u8), Code::PurchaseInvalid, || format!("purchase {n}: not a canonical combination for m = {m}"))?;
        tree.try_increment(combo.rank(), combo.bonus).map_err(|e| Error::new(Code::PurchaseInvalid, format!("purchase {n}: {e}")))?;
    }
    Ok(tree)
}

/// `rebuild_tree` from the concatenated six-byte purchases.
pub fn rebuild_tree_bytes(m: i64, bytes: &[u8]) -> Result<Tree> {
    ensure(bytes.len() % 6 == 0, Code::BytesLength, || format!("{} purchase bytes, not a multiple of 6", bytes.len()))?;
    ensure(bytes.len() <= INPUT_MAX, Code::BytesLength, || format!("{} purchase bytes", bytes.len()))?;
    let p: Vec<[u8; 6]> = bytes.chunks_exact(6).map(|c| [c[0], c[1], c[2], c[3], c[4], c[5]]).collect();
    rebuild_tree(m, &p)
}

/// Spec 6.3: the trees' roots against a marker's or pool's frozen `roots` field (32 x K_MAX
/// bytes, the first k used) and its `k`. Any difference is `TreeMismatch`.
pub fn check_roots(trees: &[Tree], roots: &[u8], k: i64) -> Result<()> {
    ensure(roots.len() == 32 * K_MAX, Code::BytesLength, || format!("roots field of {} bytes", roots.len()))?;
    ensure(k == trees.len() as i64 && (1..=K_MAX as i64).contains(&k), Code::TreeMismatch, || format!("{} trees for k = {k}", trees.len()))?;
    let mut want = vec![0u8; 32 * K_MAX];
    for (i, t) in trees.iter().enumerate() {
        want[32 * i..32 * (i + 1)].copy_from_slice(&t.root());
    }
    ensure(want == roots, Code::TreeMismatch, || "a rebuilt shard root differs from the frozen root".into())
}

// ------------------------------------------------------------------------------ stores

/// A `wallet/1` file (spec M8 A3), with the bounds its readers rely on checked up front.
pub fn read_wallet(json: &str) -> Result<crate::store::Wallet> {
    let w: crate::store::Wallet = crate::store::read(json, crate::store::WALLET).code(Code::StoreFormat)?;
    crate::store::checked_b_max(w.b_max).code(Code::StoreFormat)?;
    ensure((2..=M_HARD).contains(&w.m), Code::StoreFormat, || format!("wallet m = {}", w.m))?;
    ensure((1..=K_MAX).contains(&w.shards.len()), Code::StoreFormat, || format!("wallet with {} shards", w.shards.len()))?;
    ensure(w.frozen_roots.is_empty() || w.frozen_roots.len() == w.shards.len(), Code::StoreFormat, || "frozen roots do not match the shards".into())?;
    let b = w.b_max as usize;
    for t in &w.tickets {
        ensure(t.bundle.len() == 6 * b && t.claimed.len() == b.div_ceil(8), Code::StoreFormat, || "a ticket's bundle or claimed field has the wrong length".into())?;
    }
    Ok(w)
}

/// An `index-log.jsonl` (`index/1`).
pub fn read_index_log(jsonl: &str) -> Result<crate::store::IndexLog> {
    crate::store::IndexLog::from_jsonl(jsonl).code(Code::StoreFormat)
}

/// An `index.json` snapshot (`index/1`); every live UTXO's state is decoded and checked
/// against its role and script.
pub fn read_index_snapshot(json: &str) -> Result<crate::store::IndexSnapshot> {
    let s: crate::store::IndexSnapshot = crate::store::read(json, crate::store::INDEX).code(Code::StoreFormat)?;
    let t = templates()?;
    for l in &s.live_utxos {
        l.decode(&t).code(Code::ChainMismatch)?;
    }
    Ok(s)
}

/// A launch bundle (`bundle/1`, the indexer API's `GET /v1/instance`), parsed and verified
/// locally (`bundle::verify`: toolchain, templates, genesis transaction, states, values).
pub fn read_bundle(json: &str) -> Result<crate::bundle::Verified> {
    let b: crate::bundle::Bundle = crate::store::read(json, crate::store::BUNDLE).code(Code::StoreFormat)?;
    crate::bundle::verify(&b).code(Code::ChainMismatch)
}

// ------------------------------------------------------------------------------ ranges

fn range(what: &str, field: &str, v: i64, lo: i64, hi: i64) -> Result<()> {
    ensure(lo <= v && v <= hi, Code::StateRange, || format!("{what} {field} = {v} outside [{lo}, {hi}]"))
}

/// A D11 (class, key) pair: class <= 2, and the 33rd key byte zero unless ECDSA.
fn payout_fields(what: &str, class: u8, key: &[u8; 33]) -> Result<Payout> {
    ensure(class <= 2 && (class == 1 || key[32] == 0), Code::StateRange, || format!("{what}: payout class {class} is not a D11 script"))?;
    Ok(Payout::from_fields(class, key))
}

/// shard.sil buy() check 1 (lines 154-169), with sales <= TOTAL_MAX (controller.sil close()).
pub fn check_shard(s: &ShardState) -> Result<()> {
    let w = "shard";
    range(w, "round", s.round, 1, TOTAL_MAX - 1)?;
    range(w, "shard_index", s.shard_index, 0, K_MAX as i64 - 1)?;
    range(w, "m", s.m, 2, M_HARD)?;
    range(w, "price", s.price, 1, PRICE_MAX)?;
    range(w, "d", s.d, 1, DEPOSIT_MAX)?;
    range(w, "s_r", s.s_r, 0, S_R_CAP)?;
    range(w, "deadline", s.deadline, 0, DAA_MAX - 1)?;
    range(w, "sales", s.sales, 0, TOTAL_MAX)?;
    range(w, "ref_paid", s.ref_paid, 0, s.sales)
}

/// ticket.sil check 1 (lines 102-110) and the template's fixed field lengths.
pub fn check_ticket(t: &TicketState, b_max: usize) -> Result<()> {
    let w = "ticket";
    range(w, "round", t.round, 1, TOTAL_MAX - 1)?;
    range(w, "deadline", t.deadline, 0, DAA_MAX - 1)?;
    range(w, "b", t.b, 1, (b_max as i64).min(TICKET_B_MAX))?;
    ensure(t.bundle.len() == 6 * b_max && t.claimed.len() == b_max.div_ceil(8), Code::StateRange, || "ticket bundle or claimed has the wrong length".into())?;
    payout_fields(w, t.payout_class, &t.payout_key).map(|_| ())
}

/// ticket.sil check 3 (lines 136-147) on a pool, and the roots field's length.
pub fn check_pool(p: &PoolState) -> Result<()> {
    let w = "pool";
    ensure(p.tier <= 6, Code::StateRange, || format!("pool tier {}", p.tier))?;
    range(w, "round", p.round, 1, TOTAL_MAX - 1)?;
    range(w, "slice", p.slice, 0, TOTAL_MAX)?;
    range(w, "m", p.m, 2, M_CLAIM)?;
    range(w, "k", p.k, 1, K_MAX as i64)?;
    range(w, "deadline", p.deadline, 0, DAA_MAX - 1)?;
    range(w, "reserve", p.reserve, 0, RESERVE_MAX)?;
    ensure(p.roots.len() == 32 * K_MAX, Code::StateRange, || format!("pool roots of {} bytes", p.roots.len()))?;
    payout_fields(w, p.funder_class, &p.funder_key).map(|_| ())
}

/// controller.sil close() check 2 (lines 194-266), as `GenesisCase::validate` mirrors it.
pub fn check_controller(c: &ControllerState) -> Result<()> {
    let w = "controller";
    range(w, "round", c.round, 1, TOTAL_MAX - 1)?;
    range(w, "k", c.k, 1, K_MAX as i64)?;
    range(w, "m_min", c.m_min, 2, M_HARD)?;
    range(w, "m_max", c.m_max, c.m_min, M_HARD)?;
    range(w, "m", c.m, c.m_min, c.m_max)?;
    range(w, "price", c.price, 1, PRICE_MAX)?;
    range(w, "d", c.d, 1, DEPOSIT_MAX)?;
    range(w, "s_f", c.s_f, 0, S_F_CAP)?;
    range(w, "s_r", c.s_r, 0, S_R_CAP)?;
    range(w, "s_c", c.s_c, 0, BP)?;
    range(w, "s_d", c.s_d, 0, BP)?;
    for (i, &st) in c.st.iter().enumerate() {
        range(w, &format!("st{}", i + 1), st, 0, BP)?;
    }
    // controller.sil line 231: sum(st) + s_C + s_D + S_F_CAP + S_R_CAP <= BP.
    let room: i64 = c.st.iter().sum::<i64>() + c.s_c + c.s_d + S_F_CAP + S_R_CAP;
    ensure(room <= BP, Code::StateRange, || format!("controller share room {room} > {BP}"))?;
    range(w, "e", c.e, 0, 5_000)?;
    range(w, "j", c.j, 0, 5_000)?;
    range(w, "g", c.g, 0, 1_000)?;
    range(w, "margin", c.margin, MARGIN_MIN, 2 * BP)?;
    range(w, "cap", c.cap, 0, BP)?;
    for (f, v) in [("jackpot", c.jackpot), ("j_boost", c.j_boost), ("g2", c.g2), ("liability", c.liability), ("v_rec", c.v_rec), ("acc", c.acc)] {
        range(w, f, v, 0, TOTAL_MAX)?;
    }
    range(w, "reserve", c.reserve, 1, RESERVE_MAX)?;
    for (f, v) in [("deadline", c.deadline), ("round_len", c.round_len), ("d_min", c.d_min)] {
        range(w, f, v, 0, DAA_MAX - 1)?;
    }
    range(w, "seed_offset", c.seed_offset, 1, DAA_MAX - 1)?;
    ensure(c.drawn <= 1, Code::StateRange, || format!("controller drawn = {}", c.drawn))?;
    payout_fields(w, c.fee_class, &c.fee_key).map(|_| ())
}

/// controller.sil close() checks on the params (lines 309-314) and the D11 fee fields.
pub fn check_params(p: &ParamsState) -> Result<()> {
    let w = "params";
    range(w, "round", p.round, 1, TOTAL_MAX - 1)?;
    range(w, "next_s_f", p.next_s_f, 0, S_F_CAP)?;
    range(w, "next_s_r", p.next_s_r, 0, S_R_CAP)?;
    ensure(p.used <= 1, Code::StateRange, || format!("params used = {}", p.used))?;
    payout_fields(w, p.next_fee_class, &p.next_fee_key).map(|_| ())
}

/// drawer.sil check 4 on the marker (drawer.sil lines 420-461) and the roots field's length.
pub fn check_marker(mk: &MarkerState) -> Result<()> {
    let w = "marker";
    range(w, "round", mk.round, 1, TOTAL_MAX - 1)?;
    range(w, "k", mk.k, 1, K_MAX as i64)?;
    range(w, "m", mk.m, 2, M_DRAW)?;
    range(w, "s_f", mk.s_f, 0, S_F_CAP)?;
    range(w, "s_r", mk.s_r, 0, S_R_CAP)?;
    range(w, "s_c", mk.s_c, 0, BP)?;
    range(w, "s_d", mk.s_d, 0, BP)?;
    for (i, &st) in mk.st.iter().enumerate() {
        range(w, &format!("st{}", i + 1), st, 0, BP)?;
    }
    let room: i64 = mk.st.iter().sum::<i64>() + mk.s_f + mk.s_r + mk.s_c + mk.s_d;
    ensure(room <= BP, Code::StateRange, || format!("marker shares sum to {room} > {BP}"))?;
    range(w, "sales", mk.sales, 0, TOTAL_MAX)?;
    range(w, "ref_paid", mk.ref_paid, 0, mk.sales)?;
    range(w, "jackpot", mk.jackpot, 0, TOTAL_MAX)?;
    range(w, "j_boost", mk.j_boost, 0, mk.jackpot)?;
    range(w, "g2", mk.g2, 0, TOTAL_MAX)?;
    ensure(mk.roots.len() == 32 * K_MAX, Code::StateRange, || format!("marker roots of {} bytes", mk.roots.len()))?;
    payout_fields(w, mk.funder_class, &mk.funder_key).map(|_| ())
}

/// drawer.sil check 1 (lines 327-334).
pub fn check_drawer(d: &DrawerState) -> Result<()> {
    let w = "drawer";
    range(w, "round", d.round, 1, TOTAL_MAX - 1)?;
    range(w, "f_min", d.f_min, F_MIN_FLOOR, F_MIN_MAX)?;
    range(w, "claim_window", d.claim_window, 0, DAA_MAX - 1)?;
    range(w, "draw_window", d.draw_window, VOID_MIN, DAA_MAX - 1)
}

/// vault.sil check 1 of request_deposit() and request_withdraw() (lines 153-172, 265-281).
pub fn check_vault(v: &VaultState) -> Result<()> {
    let w = "vault";
    range(w, "round", v.round, 0, TOTAL_MAX - 1)?;
    range(w, "total_assets", v.total_assets, 0, ASSETS_MAX)?;
    range(w, "total_shares", v.total_shares, 0, SHARES_MAX)?;
    range(w, "pend_dep", v.pend_dep, 0, TOTAL_MAX)?;
    range(w, "pend_wsh", v.pend_wsh, 0, v.total_shares)?;
    range(w, "liability", v.liability, 0, TOTAL_MAX)
}

/// position.sil and vault.sil request_withdraw() ranges on a position.
pub fn check_position(p: &PositionState) -> Result<()> {
    let w = "position";
    range(w, "round", p.round, 0, TOTAL_MAX - 1)?;
    range(w, "shares", p.shares, 1, SHARES_MAX)?;
    payout_fields(w, p.owner_class, &p.owner_key).map(|_| ())
}

/// request.sil settle() check 1 (lines 141-163), `game::request_in_range`.
pub fn check_request(r: &RequestState) -> Result<()> {
    ensure(crate::game::request_in_range(r), Code::StateRange, || format!("request out of range (kind {}, amount {})", r.kind, r.amount))?;
    ensure(r.kind == KIND_DEPOSIT || r.kind == KIND_WITHDRAW, Code::StateRange, || format!("request kind {}", r.kind))?;
    if r.kind == KIND_DEPOSIT {
        range("request", "amount", r.amount, MIN_DEPOSIT, TOTAL_MAX)?;
    }
    Ok(())
}

/// request.sil settle() check 3 on the record (lines 183-203), `game::record_in_range`.
pub fn check_record(r: &RecordState) -> Result<()> {
    ensure(crate::game::record_in_range(r), Code::StateRange, || "record out of range".into())?;
    payout_fields("record", r.funder_class, &r.funder_key).map(|_| ())
}

/// A combination's six bytes as a `Combo` (no range check; `is_canonical` is the check).
pub fn combo(c: &[u8; 6]) -> Combo {
    Combo::new([c[0], c[1], c[2], c[3], c[4]], c[5])
}

/// Whether a combination is canonical for bonus range `m` (1 <= n1 < ... < n5 <= 30,
/// 1 <= bonus <= m).
pub fn canonical(c: &[u8; 6], m: i64) -> bool {
    (2..=M_HARD).contains(&m) && c[4] <= MAIN_MAX && combo(c).is_canonical(m as u8)
}
