//! The v1 endpoints: each builds a `web::indexer` response from the current snapshot.
//! Read-only: there is no write endpoint, and nothing here touches a file or a node.

use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use lottery::index::{Role, State as St};
use lottery::state::Payout;
use serde::Serialize;
use web::convert::{Net, hex};
use web::dto::Combination;
use web::indexer::*;

use crate::server::AppState;
use crate::store::{LiveObj, RoundSnap, Snapshot};

/// Apply the doctored mode, if this is the test-only doctored build and one is set.
macro_rules! doctor {
    ($st:expr, $f:ident, $v:expr) => {
        #[cfg(feature = "doctored")]
        if let Some(m) = $st.doctor {
            crate::doctor::$f(m, $v);
        }
        #[cfg(not(feature = "doctored"))]
        {
            let _ = (&$st, $v);
        }
    };
}

// ------------------------------------------------------------------------------ responses

/// A JSON response of at most `cap` bytes (`too_large` otherwise).
pub fn json<T: Serialize>(cap: u32, v: &T) -> Response {
    match serde_json::to_vec(v) {
        Ok(b) if b.len() <= cap as usize => body(StatusCode::OK, b),
        Ok(b) => error(ApiErrorCode::TooLarge, format!("{} bytes, the cap is {cap}", b.len())),
        Err(e) => error(ApiErrorCode::Unavailable, format!("encode: {e}")),
    }
}

fn body(status: StatusCode, b: Vec<u8>) -> Response {
    let mut r = (status, b).into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    r
}

/// A typed JSON error (`ApiErrorBody`).
pub fn error(code: ApiErrorCode, detail: impl Into<String>) -> Response {
    let b = serde_json::to_vec(&ApiErrorBody::new(code, detail)).unwrap_or_default();
    body(StatusCode::from_u16(code.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), b)
}

pub async fn not_found() -> Response {
    error(ApiErrorCode::NotFound, "see /v1/status for what this server serves")
}

pub async fn method_not_allowed() -> Response {
    error(ApiErrorCode::MethodNotAllowed, "GET only")
}

// ------------------------------------------------------------------------------ helpers

fn round_of(s: &str) -> Result<i64, Response> {
    let ok = !s.is_empty() && s.len() <= 15 && s.bytes().all(|c| c.is_ascii_digit()) && !s.starts_with('0');
    ok.then(|| s.parse::<i64>().ok()).flatten().ok_or_else(|| error(ApiErrorCode::BadRequest, format!("round `{}` is not a round number", s.chars().take(20).collect::<String>())))
}

fn round<'a>(snap: &'a Snapshot, r: &str) -> Result<&'a RoundSnap, Response> {
    let r = round_of(r)?;
    snap.rounds.get(&r).ok_or_else(|| error(ApiErrorCode::RoundUnknown, format!("round {r}")))
}

/// The one query parameter `name` (anything else is `bad_request`).
fn query_param(q: Option<String>, name: &str) -> Result<String, Response> {
    let q = q.unwrap_or_default();
    let mut found = None;
    for pair in q.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k != name || found.is_some() {
            return Err(error(ApiErrorCode::BadRequest, format!("query: only one `{name}` parameter is accepted")));
        }
        let v = percent_encoding::percent_decode_str(v).decode_utf8().map_err(|_| error(ApiErrorCode::BadRequest, "query: not UTF-8"))?;
        found = Some(v.into_owned());
    }
    found.filter(|v| !v.is_empty()).ok_or_else(|| error(ApiErrorCode::BadRequest, format!("query: `{name}` is required")))
}

/// An address of the server's network to its D11 payout (spec 6.6: other networks refused).
fn payout_of(net: Net, what: &str, a: &str) -> Result<Payout, Response> {
    if a.len() > 200 {
        return Err(error(ApiErrorCode::BadRequest, format!("{what}: too long")));
    }
    net.payout_of(what, a).map_err(|e| match e.code {
        lottery::web::Code::AddressNetwork => error(ApiErrorCode::AddressNetwork, e.detail),
        _ => error(ApiErrorCode::BadRequest, e.detail),
    })
}

fn obj(l: &LiveObj) -> ApiObject {
    api_object(l.role, &l.outpoint, l.value, l.daa, &l.spk, &l.state_bytes)
}

fn combination(c: &[u8; 6]) -> Combination {
    Combination { main: [c[0], c[1], c[2], c[3], c[4]], bonus: c[5] }
}

fn status_of(r: &RoundSnap) -> RoundStatus {
    match (&r.close, &r.draw) {
        (None, _) => RoundStatus::Open,
        (Some(_), None) => RoundStatus::Closing,
        (Some(_), Some(d)) if d.result.void => RoundStatus::Void,
        _ => RoundStatus::Drawn,
    }
}

fn live_of<'a>(snap: &'a Snapshot, role: Role) -> impl Iterator<Item = &'a LiveObj> + 'a {
    snap.live.iter().filter(move |l| l.role == role)
}

/// The round's live shards (open) and their sales.
fn open_shards<'a>(snap: &'a Snapshot, r: i64) -> impl Iterator<Item = &'a LiveObj> + 'a {
    live_of(snap, Role::Shard).filter(move |l| matches!(&l.state, St::Shard(s) if s.round == r))
}

fn controller_of(snap: &Snapshot, r: i64) -> Option<&LiveObj> {
    live_of(snap, Role::Controller).find(|l| matches!(&l.state, St::Controller(c) if c.round == r))
}

/// T of a round: the live controller's or shards' (open), the spent controller's (closed).
fn deadline_of(snap: &Snapshot, r: &RoundSnap) -> Option<i64> {
    if let Some(c) = &r.close {
        return c.deadline;
    }
    match controller_of(snap, r.round).map(|l| &l.state) {
        Some(St::Controller(c)) => Some(c.deadline),
        _ => open_shards(snap, r.round).find_map(|l| if let St::Shard(s) = &l.state { Some(s.deadline) } else { None }),
    }
}

fn anchor_daa_of(r: &RoundSnap) -> Option<i64> {
    let c = r.close.as_ref()?;
    Some(lottery::game::anchor_daa(c.marker_daa as i64, c.seed_offset?))
}

fn row(snap: &Snapshot, r: &RoundSnap) -> ApiRoundRow {
    let status = status_of(r);
    let (sales, jackpot) = match &r.close {
        Some(c) => (Some(c.marker.sales), Some(c.marker.jackpot)),
        None => {
            let sales: i64 = open_shards(snap, r.round).filter_map(|l| if let St::Shard(s) = &l.state { Some(s.sales) } else { None }).sum();
            let jackpot = controller_of(snap, r.round).and_then(|l| if let St::Controller(c) = &l.state { Some(c.jackpot) } else { None });
            (Some(sales), jackpot)
        }
    };
    ApiRoundRow {
        round: r.round.to_string(),
        status,
        m: r.m.map(|m| m as u32),
        k: r.k as u32,
        buys: r.buys.len() as u32,
        combinations: r.buys.iter().map(|b| b.combos.len() as u32).sum(),
        sales: sales.map(|s| s.to_string()),
        jackpot: jackpot.map(|j| j.to_string()),
        incomplete: r.incomplete.is_some() || !r.broken.is_empty(),
    }
}

/// The live objects a page needs for a round (see `ApiRound`).
fn round_objects(snap: &Snapshot, r: &RoundSnap) -> Vec<ApiObject> {
    let n = r.round;
    let pick = |l: &LiveObj| match (status_of(r), &l.state) {
        (RoundStatus::Open, St::Controller(c)) => c.round == n,
        (RoundStatus::Open, St::Params(_)) => true,
        (RoundStatus::Open, St::Shard(s)) => s.round == n,
        (RoundStatus::Closing, St::Marker(m)) => m.round == n,
        (RoundStatus::Closing, St::Controller(c)) => c.round == n + 1,
        (RoundStatus::Closing, St::Drawer(_) | St::Vault(_)) => true,
        (RoundStatus::Drawn | RoundStatus::Void, St::Pool(p)) => p.round == n,
        (RoundStatus::Drawn | RoundStatus::Void, St::Record(x)) => x.round == n,
        (RoundStatus::Drawn | RoundStatus::Void, St::Drawer(_)) => true,
        _ => false,
    };
    snap.live.iter().filter(|l| pick(l)).map(obj).collect()
}

fn api_round(snap: &Snapshot, r: &RoundSnap) -> ApiRound {
    let close = r.close.as_ref().map(|c| ApiClose {
        transaction_id: hex(&c.txid.as_bytes()),
        marker_daa_score: c.marker_daa.to_string(),
        frozen_roots: c.frozen_roots.iter().map(|x| hex(x)).collect(),
        m_next: c.m_next as u32,
    });
    let draw = r.draw.as_ref().map(|d| {
        let x = &d.result;
        ApiDraw {
            transaction_id: hex(&d.txid.as_bytes()),
            void: x.void,
            winning: (!x.void).then(|| combination(&x.win)),
            anchor_hash: hex(&x.x_hash),
            claim_deadline: x.d_claim.to_string(),
            pools: x
                .pools
                .iter()
                .map(|p| ApiResultPool { tier: p.tier, slice: p.slice.to_string(), value: p.value.to_string(), transaction_id: hex(&p.outpoint.transaction_id.as_bytes()), index: p.outpoint.index })
                .collect(),
        }
    });
    let row = row(snap, r);
    ApiRound {
        round: row.round,
        status: row.status,
        m: row.m,
        k: row.k,
        deadline: deadline_of(snap, r).map(|d| d.to_string()),
        anchor_daa_score: anchor_daa_of(r).map(|d| d.to_string()),
        close,
        draw,
        buys: row.buys,
        combinations: row.combinations,
        objects: round_objects(snap, r),
        incomplete: r.incomplete.clone().or_else(|| (!r.broken.is_empty()).then(|| r.broken.values().cloned().collect::<Vec<_>>().join("; "))),
        gaps: r.gaps.clone(),
    }
}

/// Purchases are served only from a round the index follows in full (spec M8 A4: no proof
/// from an incomplete round).
fn complete(snap: &Snapshot, r: &RoundSnap, shard: Option<i64>) -> Result<(), Response> {
    if !snap.gaps.is_empty() {
        return Err(error(ApiErrorCode::Unavailable, format!("the index has missed transitions: {}", snap.gaps.join("; "))));
    }
    if let Some(why) = &r.incomplete {
        return Err(error(ApiErrorCode::Unavailable, format!("round {} is incomplete: {why}", r.round)));
    }
    let broken: Vec<&String> = match shard {
        Some(i) => r.broken.get(&i).into_iter().collect(),
        None => r.broken.values().collect(),
    };
    if !broken.is_empty() {
        return Err(error(ApiErrorCode::Unavailable, format!("round {}: {}", r.round, broken.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("; "))));
    }
    if r.m.is_none() {
        return Err(error(ApiErrorCode::Unavailable, format!("round {}: m is not known yet", r.round)));
    }
    Ok(())
}

fn purchases_hex(p: &[[u8; 6]]) -> String {
    hex(&p.concat())
}

// ------------------------------------------------------------------------------ handlers

pub async fn status(State(st): State<AppState>) -> Response {
    let s = st.snapshot();
    json(
        LIMITS.small,
        &ApiStatus {
            api: API_VERSION.into(),
            network: s.net.name().into(),
            covenant_id: hex(&s.cov.as_bytes()),
            log_lines: s.log_lines.to_string(),
            last_chain_block: s.last_chain.map(|(b, _)| hex(&b)),
            last_chain_daa_score: s.last_chain.map(|(_, d)| d.to_string()),
            current_round: s.rounds.keys().next_back().map(|r| r.to_string()),
            gaps: s.gaps.clone(),
            error: s.error.clone(),
            updated: s.updated_ms.to_string(),
        },
    )
}

/// The launch bundle, byte for byte as the file holds it (the page checks its hash).
pub async fn instance(State(st): State<AppState>) -> Response {
    let s = st.snapshot();
    if s.bundle.len() > LIMITS.instance as usize {
        return error(ApiErrorCode::TooLarge, "bundle");
    }
    body(StatusCode::OK, s.bundle.as_ref().clone())
}

pub async fn rounds(State(st): State<AppState>, RawQuery(q): RawQuery) -> Response {
    let s = st.snapshot();
    let before = match q.as_deref() {
        None | Some("") => None,
        Some(_) => match query_param(q, "before").and_then(|b| round_of(&b)) {
            Ok(b) => Some(b),
            Err(e) => return e,
        },
    };
    let mut rows: Vec<ApiRoundRow> = s.rounds.values().rev().filter(|r| before.is_none_or(|b| r.round < b)).take(ROUNDS_PAGE as usize + 1).map(|r| row(&s, r)).collect();
    let next = (rows.len() > ROUNDS_PAGE as usize).then(|| {
        rows.truncate(ROUNDS_PAGE as usize);
        rows.last().map(|r| r.round.clone()).unwrap_or_default()
    });
    json(LIMITS.round, &ApiRounds { covenant_id: hex(&s.cov.as_bytes()), rounds: rows, next })
}

pub async fn round_one(State(st): State<AppState>, Path(r): Path<String>) -> Response {
    let s = st.snapshot();
    let r = match round(&s, &r) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let mut v = api_round(&s, r);
    doctor!(st, round, &mut v);
    json(LIMITS.round, &v)
}

pub async fn purchases(State(st): State<AppState>, Path(r): Path<String>) -> Response {
    let s = st.snapshot();
    let r = match round(&s, &r).and_then(|r| complete(&s, r, None).map(|_| r)) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let mut v = ApiPurchases {
        round: r.round.to_string(),
        m: r.m.unwrap_or(0) as u32,
        k: r.k as u32,
        shards: (0..r.k as i64).map(|i| purchases_hex(&r.purchases(i))).collect(),
    };
    doctor!(st, purchases, &mut v);
    json(LIMITS.purchases, &v)
}

pub async fn shard(State(st): State<AppState>, Path((r, i)): Path<(String, String)>) -> Response {
    let s = st.snapshot();
    let r = match round(&s, &r) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let canonical = !i.is_empty() && i.len() <= 3 && i.bytes().all(|c| c.is_ascii_digit()) && (i == "0" || !i.starts_with('0'));
    let i = match canonical.then(|| i.parse::<u32>().ok()).flatten().filter(|&x| (x as usize) < r.k) {
        Some(x) => x,
        None => return error(ApiErrorCode::ShardUnknown, format!("shard `{}` of round {} (k = {})", i.chars().take(8).collect::<String>(), r.round, r.k)),
    };
    if let Err(e) = complete(&s, r, Some(i as i64)) {
        return e;
    }
    let p = r.purchases(i as i64);
    let mut v = ApiShard { round: r.round.to_string(), shard_index: i, m: r.m.unwrap_or(0) as u32, count: p.len() as u32, purchases: purchases_hex(&p) };
    doctor!(st, shard, &mut v);
    json(LIMITS.purchases, &v)
}

pub async fn anchor_proof(State(st): State<AppState>, Path(r): Path<String>) -> Response {
    let s = st.snapshot();
    let r = match round(&s, &r) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let (proof, source) = match (&r.close, &r.draw) {
        (_, Some(d)) if d.result.void => return error(ApiErrorCode::AnchorUnavailable, format!("round {} was voided: it has no anchor", r.round)),
        (_, Some(d)) => match &d.anchor {
            Some(a) => (a.clone(), ProofSource::Draw),
            None => return error(ApiErrorCode::AnchorUnavailable, "the draw's anchor argument is not known"),
        },
        (Some(c), None) => {
            let (Some(p), Some(d)) = (st.anchor.clone(), anchor_daa_of(r)) else {
                return error(ApiErrorCode::AnchorUnavailable, format!("round {} is not drawn, and this server follows no node to build the proof", r.round));
            };
            match p.proof(r.round, d as u64, c.marker_daa).await {
                Ok(b) => (b, ProofSource::Node),
                Err(e) => return error(ApiErrorCode::AnchorUnavailable, e),
            }
        }
        (None, _) => return error(ApiErrorCode::AnchorUnavailable, format!("round {} is not closed", r.round)),
    };
    let mut v = ApiAnchorProof { round: r.round.to_string(), anchor_proof: hex(&proof), source };
    doctor!(st, anchor, &mut v);
    json(LIMITS.small, &v)
}

pub async fn tickets(State(st): State<AppState>, RawQuery(q): RawQuery) -> Response {
    let s = st.snapshot();
    let (addr, payout) = match query_param(q, "payout").and_then(|a| payout_of(s.net, "payout", &a).map(|p| (a, p))) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let mine = |t: &lottery::state::TicketState| Payout::from_fields(t.payout_class, &t.payout_key) == payout;
    let mut tickets: Vec<ApiObject> = live_of(&s, Role::Ticket).filter(|l| matches!(&l.state, St::Ticket(t) if mine(t))).map(obj).collect();
    let bought: Vec<ApiBuy> = s
        .rounds
        .values()
        .flat_map(|r| r.buys.iter().filter(|b| mine(&b.ticket)).map(move |b| (r.round, b)))
        .map(|(r, b)| ApiBuy {
            round: r.to_string(),
            shard_index: b.shard_index as u32,
            transaction_id: hex(&b.txid.as_bytes()),
            ticket_index: b.ticket_index,
            combinations: b.combos.iter().map(combination).collect(),
            referral: b.referral.to_string(),
        })
        .collect();
    if tickets.len() > OBJECTS_MAX as usize || bought.len() > OBJECTS_MAX as usize {
        return error(ApiErrorCode::TooLarge, format!("{} tickets, {} purchases", tickets.len(), bought.len()));
    }
    doctor!(st, objects, &mut tickets);
    json(LIMITS.objects, &ApiTickets { payout: addr, tickets, bought })
}

pub async fn vault(State(st): State<AppState>) -> Response {
    let s = st.snapshot();
    let newest = s.rounds.keys().next_back().copied();
    let mut objects: Vec<ApiObject> = s
        .live
        .iter()
        .filter(|l| match &l.state {
            St::Vault(_) | St::Drawer(_) | St::Record(_) => true,
            St::Controller(c) => Some(c.round) == newest,
            _ => false,
        })
        .map(obj)
        .collect();
    doctor!(st, objects, &mut objects);
    json(LIMITS.objects, &ApiObjects { owner: None, objects })
}

/// The most live pools `GET /v1/pools` lists (the oldest claim deadlines first): far more than
/// one `draw()` absorbs (A_MAX = 7), so a page always finds the oldest expired ones.
pub const POOLS_MAX: usize = 256;

/// `GET /v1/pools` (F5 review S1): the instance's live prize pools, oldest claim deadline first
/// (then round and tier), at most `POOLS_MAX`. A page's `draw()` reads each from a node by its
/// outpoint and absorbs those past their deadline (`absorbablePools`); this list only says where
/// to look.
pub async fn pools(State(st): State<AppState>) -> Response {
    let s = st.snapshot();
    let mut v: Vec<(i64, i64, u8, &LiveObj)> = live_of(&s, Role::Pool)
        .filter_map(|l| match &l.state {
            St::Pool(p) => Some((p.deadline, p.round, p.tier, l)),
            _ => None,
        })
        .collect();
    v.sort_by_key(|x| (x.0, x.1, x.2));
    let mut objects: Vec<ApiObject> = v.into_iter().take(POOLS_MAX).map(|x| obj(x.3)).collect();
    doctor!(st, objects, &mut objects);
    json(LIMITS.objects, &ApiObjects { owner: None, objects })
}

fn owned(st: &AppState, q: Option<String>, role: Role) -> Response {
    let s = st.snapshot();
    let (addr, owner) = match query_param(q, "owner").and_then(|a| payout_of(s.net, "owner", &a).map(|p| (a, p))) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let of = |class: u8, key: &[u8; 33]| Payout::from_fields(class, key) == owner;
    let mine: Vec<&LiveObj> = live_of(&s, role)
        .filter(|l| match &l.state {
            St::Position(p) => of(p.owner_class, &p.owner_key),
            St::Request(q) => of(q.owner_class, &q.owner_key),
            _ => false,
        })
        .collect();
    let rounds: Vec<i64> = mine.iter().map(|l| l.state.round()).collect();
    let records = live_of(&s, Role::Record).filter(|l| role == Role::Request && rounds.contains(&l.state.round()));
    let mut objects: Vec<ApiObject> = mine.into_iter().chain(records).map(obj).collect();
    if objects.len() > OBJECTS_MAX as usize {
        return error(ApiErrorCode::TooLarge, format!("{} objects", objects.len()));
    }
    doctor!(st, objects, &mut objects);
    json(LIMITS.objects, &ApiObjects { owner: Some(addr), objects })
}

pub async fn positions(State(st): State<AppState>, RawQuery(q): RawQuery) -> Response {
    owned(&st, q, Role::Position)
}

pub async fn requests(State(st): State<AppState>, RawQuery(q): RawQuery) -> Response {
    owned(&st, q, Role::Request)
}
