//! TEST-ONLY: the untrusted-API fixtures (spec frontend 8, "Untrusted API"). Compiled only
//! with the `doctored` feature, and reachable only from the `indexer-api-doctored` binary
//! (loopback only, every response marked `x-indexer-doctored`) and from the tests. Each mode
//! makes the server lie in one way; the page's checks (`web::indexer`) must refuse each one
//! with `chain_mismatch`.
//!
//! KASPALIONARE_DOCTORED_API: the marker `tests/production.rs` looks for in the binaries.

use lottery::web::verify;
use web::convert::{hex, unhex};
use web::indexer::{ApiAnchorProof, ApiObject, ApiPurchases, ApiRound, ApiShard};

pub const MARKER: &str = "KASPALIONARE_DOCTORED_API";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Every object's state bytes describe another state (one integer field changed).
    WrongState,
    /// The first purchase of the first non-empty shard has another bonus.
    WrongTree,
    /// The last purchase of the first non-empty shard is left out.
    MissingPurchase,
    /// A purchase is added to shard 0.
    ExtraPurchase,
    /// The round's winning numbers have another bonus.
    WrongWinning,
    /// The anchor proof's X.hash has one byte changed.
    WrongAnchorProof,
}

pub const ALL: [Mode; 6] = [Mode::WrongState, Mode::WrongTree, Mode::MissingPurchase, Mode::ExtraPurchase, Mode::WrongWinning, Mode::WrongAnchorProof];

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::WrongState => "wrong_state",
            Mode::WrongTree => "wrong_tree",
            Mode::MissingPurchase => "missing_purchase",
            Mode::ExtraPurchase => "extra_purchase",
            Mode::WrongWinning => "wrong_winning",
            Mode::WrongAnchorProof => "wrong_anchor_proof",
        }
    }

    pub fn parse(s: &str) -> Result<Mode, String> {
        ALL.into_iter().find(|m| m.name() == s).ok_or_else(|| format!("doctored mode `{s}`: one of {}", ALL.map(|m| m.name()).join(", ")))
    }
}

/// One integer field of the state changed by one, re-encoded (or, if that cannot be
/// encoded, the last byte flipped).
fn wrong_state(o: &mut ApiObject) {
    let Ok(bytes) = unhex("state", &o.state) else { return };
    let role = o.role.role();
    let changed = verify::decode_state(role, &bytes).ok().and_then(|s| {
        let mut v = s.values();
        let key = v.iter().find(|(k, x)| k.as_str() != "round" && matches!(x, harness::ArtifactValue::Int(_))).map(|(k, _)| k.clone())?;
        if let Some(harness::ArtifactValue::Int(x)) = v.get_mut(&key) {
            *x = if *x > 0 { *x - 1 } else { *x + 1 };
        }
        let s2 = lottery::index::State::from_values(role, &v).ok()?;
        verify::state_bytes(&s2).ok()
    });
    o.state = match changed {
        Some(b) => hex(&b),
        None => {
            let mut b = bytes;
            if let Some(x) = b.last_mut() {
                *x ^= 1;
            }
            hex(&b)
        }
    };
}

pub fn objects(m: Mode, v: &mut [ApiObject]) {
    if m == Mode::WrongState {
        v.iter_mut().for_each(wrong_state);
    }
}

pub fn round(m: Mode, v: &mut ApiRound) {
    match m {
        Mode::WrongState => objects(m, &mut v.objects),
        Mode::WrongWinning => {
            let mm = v.m.unwrap_or(2).max(2) as u8;
            if let Some(w) = v.draw.as_mut().and_then(|d| d.winning.as_mut()) {
                w.bonus = w.bonus % mm + 1;
            }
        }
        _ => {}
    }
}

/// Six-byte purchases of a list, and back.
fn list(h: &str) -> Vec<[u8; 6]> {
    unhex("purchases", h).unwrap_or_default().chunks_exact(6).map(|c| [c[0], c[1], c[2], c[3], c[4], c[5]]).collect()
}

fn unlist(p: &[[u8; 6]]) -> String {
    hex(&p.concat())
}

fn lists(m: Mode, lists_: &mut [String], bonus_m: u32) {
    let mut ps: Vec<Vec<[u8; 6]>> = lists_.iter().map(|h| list(h)).collect();
    let first = ps.iter().position(|p| !p.is_empty());
    match (m, first) {
        (Mode::WrongTree, Some(i)) => {
            let c = &mut ps[i][0];
            c[5] = c[5] % bonus_m.max(2) as u8 + 1;
        }
        (Mode::MissingPurchase, Some(i)) => {
            ps[i].pop();
        }
        (Mode::ExtraPurchase, _) if !ps.is_empty() => {
            let extra = first.map(|i| ps[i][0]).unwrap_or([1, 2, 3, 4, 5, 1]);
            ps[0].push(extra);
        }
        _ => return,
    }
    for (h, p) in lists_.iter_mut().zip(&ps) {
        *h = unlist(p);
    }
}

pub fn purchases(m: Mode, v: &mut ApiPurchases) {
    lists(m, &mut v.shards, v.m);
}

pub fn shard(m: Mode, v: &mut ApiShard) {
    let mut one = [v.purchases.clone()];
    lists(m, &mut one, v.m);
    v.purchases = one[0].clone();
    v.count = (v.purchases.len() / 12) as u32;
}

pub fn anchor(m: Mode, v: &mut ApiAnchorProof) {
    if m == Mode::WrongAnchorProof {
        if let Ok(mut b) = unhex("anchorProof", &v.anchor_proof) {
            if let Some(x) = b.first_mut() {
                *x ^= 0x01;
            }
            v.anchor_proof = hex(&b);
        }
    }
}
