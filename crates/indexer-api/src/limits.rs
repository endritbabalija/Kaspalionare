//! Per-client request limits and the anchor-proof cache (F6 part C, the hosted API).
//!
//! - **Per-client limit.** A token bucket per client: `burst` requests at once, refilled at
//!   `rate` per second. The client is the connection's peer address; behind a reverse proxy
//!   on the same machine (`--client-ip forwarded`) it is the last `X-Forwarded-For` entry, and
//!   only when the peer itself is a loopback address (a header from anyone else is ignored,
//!   so a client cannot pick its own bucket). IPv6 clients are keyed by their /64 (one
//!   household or server gets one bucket, not 2^64). A request over the limit is `busy`
//!   (503, `Retry-After: 1`), the code the page already treats as "the indexer is
//!   unavailable, retry". Requests that carry no peer (in-process tests, an embedding
//!   process that calls the router directly) are not limited here.
//! - **Bounded bucket memory and work** (F6 security review): idle full buckets are dropped by a
//!   sweep that runs at most once a second (the sweep is O(buckets), so it is amortised over a
//!   second of requests, never paid per request), and at most `BUCKETS_MAX` buckets exist: past
//!   that, new addresses share one overflow bucket until a sweep frees room (a flood of addresses
//!   then limits itself, and one map entry per address is never kept beyond the cap).
//! - **Per-client requests in flight** (F6 security review S1): at most `cap` requests of one
//!   client run at once (`InFlight`), well below the global limit, so one client holding slow
//!   requests (anchor-proof waits during a draw) cannot hold every slot. Over it, `busy`.
//! - **Anchor-proof cache.** Building an anchor proof before the draw costs a node dozens of
//!   RPC calls (the anchor's mergeset, `tn10/src/anchor.rs`). `CachedAnchor` builds each
//!   (round, D, marker DAA) at most once at a time (one build in flight; every request for it
//!   waits on that build), keeps a proof for `ok_ttl` and a failure for `err_ttl`, and lets a
//!   request wait at most `wait` (below the request deadline) before answering
//!   "being built, retry". Only a closing round has such a key, and rounds close one at a
//!   time, so the node sees at most one build per `ok_ttl` / `err_ttl` whatever the number
//!   of clients. A build is never cancelled by a client giving up.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use tokio::sync::watch;

use crate::server::AnchorProvider;

/// The connection's peer address, added to every request by `serve`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Peer(pub SocketAddr);

/// Which address names the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientIp {
    /// The TCP peer (the server faces the internet itself).
    Peer,
    /// The last `X-Forwarded-For` entry when the peer is a loopback address (a reverse proxy
    /// on the same machine); otherwise the peer.
    Forwarded,
}

impl ClientIp {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "peer" => Ok(ClientIp::Peer),
            "forwarded" => Ok(ClientIp::Forwarded),
            _ => Err(format!("--client-ip {s}: `peer` or `forwarded`")),
        }
    }
}

/// The bucket key of a client: the IPv4 address, or the /64 of an IPv6 one (an IPv4-mapped
/// IPv6 address is its IPv4 address).
pub fn bucket_of(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let s = v6.segments();
                IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
        },
    }
}

/// The client address of a request (see `ClientIp`); `None` without a peer.
pub fn client_of(peer: Option<Peer>, headers: &HeaderMap, mode: ClientIp) -> Option<IpAddr> {
    let p = peer?.0.ip();
    if mode == ClientIp::Forwarded && p.is_loopback() {
        let last = headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .next_back();
        if let Some(ip) = last.and_then(|s| s.parse::<IpAddr>().ok()) {
            return Some(ip);
        }
    }
    Some(p)
}

/// Token buckets per client.
pub struct ClientLimiter {
    burst: f64,
    rate: f64,
    buckets: Mutex<Buckets>,
}

struct Buckets {
    map: HashMap<IpAddr, (f64, Instant)>,
    last_sweep: Option<Instant>,
    sweeps: u64,
}

/// Above this many buckets, idle full ones are swept (at most once a second).
const PRUNE_AT: usize = 10_000;
/// At most this many buckets (a few dozen bytes each); past it new addresses share `OVERFLOW`.
pub const BUCKETS_MAX: usize = 65_536;
/// The shared bucket of addresses that arrive while the map is full.
pub const OVERFLOW: IpAddr = IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED);
const SWEEP_EVERY: Duration = Duration::from_secs(1);

impl ClientLimiter {
    pub fn new(burst: u64, rate: u64) -> Self {
        ClientLimiter { burst: burst as f64, rate: rate as f64, buckets: Mutex::new(Buckets { map: HashMap::new(), last_sweep: None, sweeps: 0 }) }
    }

    /// Take one token for `ip` at `now`; false when its bucket is empty.
    pub fn allow_at(&self, ip: IpAddr, now: Instant) -> bool {
        let mut key = bucket_of(ip);
        let mut b = match self.buckets.lock() {
            Ok(m) => m,
            Err(p) => p.into_inner(),
        };
        if b.map.len() >= PRUNE_AT && !b.map.contains_key(&key) && b.last_sweep.is_none_or(|t| now.saturating_duration_since(t) >= SWEEP_EVERY) {
            let full = Duration::from_secs_f64(self.burst / self.rate.max(1e-9));
            b.map.retain(|k, (_, at)| *k == OVERFLOW || now.saturating_duration_since(*at) < full);
            b.last_sweep = Some(now);
            b.sweeps += 1;
        }
        if b.map.len() >= BUCKETS_MAX && !b.map.contains_key(&key) {
            key = OVERFLOW;
        }
        let (tokens, at) = b.map.entry(key).or_insert((self.burst, now));
        let refill = now.saturating_duration_since(*at).as_secs_f64() * self.rate;
        *tokens = (*tokens + refill).min(self.burst);
        *at = now;
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }

    pub fn allow(&self, ip: IpAddr) -> bool {
        self.allow_at(ip, Instant::now())
    }

    pub fn clients(&self) -> usize {
        self.buckets.lock().map(|m| m.map.len()).unwrap_or(0)
    }

    /// How many sweeps ran (for tests).
    pub fn sweeps(&self) -> u64 {
        self.buckets.lock().map(|m| m.sweeps).unwrap_or(0)
    }
}

/// Requests in flight per client (see the module doc): `enter` gives a guard while the client
/// has fewer than `cap` running, `None` otherwise. Entries leave the map when their count is 0,
/// so it holds at most one entry per running request.
pub struct InFlight {
    cap: usize,
    m: Mutex<HashMap<IpAddr, usize>>,
}

impl InFlight {
    pub fn new(cap: usize) -> Arc<Self> {
        Arc::new(InFlight { cap, m: Mutex::new(HashMap::new()) })
    }

    pub fn enter(self: &Arc<Self>, ip: IpAddr) -> Option<InFlightGuard> {
        let key = bucket_of(ip);
        let mut m = match self.m.lock() {
            Ok(m) => m,
            Err(p) => p.into_inner(),
        };
        let n = m.entry(key).or_insert(0);
        if *n >= self.cap {
            return None;
        }
        *n += 1;
        Some(InFlightGuard { f: self.clone(), key })
    }

    /// Requests of `ip` running now.
    pub fn running(&self, ip: IpAddr) -> usize {
        self.m.lock().map(|m| m.get(&bucket_of(ip)).copied().unwrap_or(0)).unwrap_or(0)
    }
}

pub struct InFlightGuard {
    f: Arc<InFlight>,
    key: IpAddr,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        let mut m = match self.f.m.lock() {
            Ok(m) => m,
            Err(p) => p.into_inner(),
        };
        if let Some(n) = m.get_mut(&self.key) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                m.remove(&self.key);
            }
        }
    }
}

type Key = (i64, u64, u64);
type Res = Result<Vec<u8>, String>;

enum Slot {
    Ready { at: Instant, r: Res },
    Building(watch::Receiver<Option<Res>>),
}

/// An `AnchorProvider` in front of another: one build at a time per key, results kept for a
/// while (see the module doc).
pub struct CachedAnchor {
    inner: Arc<dyn AnchorProvider>,
    ok_ttl: Duration,
    err_ttl: Duration,
    wait: Duration,
    slots: Arc<Mutex<HashMap<Key, Slot>>>,
    builds: Arc<std::sync::atomic::AtomicU64>,
}

impl CachedAnchor {
    pub fn new(inner: Arc<dyn AnchorProvider>, ok_ttl: Duration, err_ttl: Duration, wait: Duration) -> Self {
        CachedAnchor { inner, ok_ttl, err_ttl, wait, slots: Default::default(), builds: Default::default() }
    }

    /// The number of builds started (for tests and the log).
    pub fn builds(&self) -> u64 {
        self.builds.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl AnchorProvider for CachedAnchor {
    fn proof(&self, round: i64, d: u64, marker_daa: u64) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + '_>> {
        Box::pin(async move {
            let key = (round, d, marker_daa);
            let mut rx = {
                let mut m = self.slots.lock().map_err(|e| e.to_string())?;
                let fresh = match m.get(&key) {
                    Some(Slot::Ready { at, r }) => {
                        let ttl = if r.is_ok() { self.ok_ttl } else { self.err_ttl };
                        if at.elapsed() < ttl {
                            return r.clone();
                        }
                        None
                    }
                    Some(Slot::Building(rx)) => Some(rx.clone()),
                    None => None,
                };
                match fresh {
                    Some(rx) => rx,
                    None => {
                        // Keep the map small: only the newest keys matter (one closing round).
                        if m.len() > 8 {
                            m.retain(|_, s| matches!(s, Slot::Building(_)));
                        }
                        let (tx, rx) = watch::channel(None);
                        m.insert(key, Slot::Building(rx.clone()));
                        self.builds.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let (inner, slots) = (self.inner.clone(), self.slots.clone());
                        tokio::spawn(async move {
                            let r = inner.proof(round, d, marker_daa).await;
                            if let Ok(mut m) = slots.lock() {
                                m.insert(key, Slot::Ready { at: Instant::now(), r: r.clone() });
                            }
                            let _ = tx.send(Some(r));
                        });
                        rx
                    }
                }
            };
            let waited = tokio::time::timeout(self.wait, async {
                loop {
                    if let Some(r) = rx.borrow().clone() {
                        return r;
                    }
                    if rx.changed().await.is_err() {
                        return Err("the anchor proof build stopped".to_string());
                    }
                }
            })
            .await;
            match waited {
                Ok(r) => r,
                Err(_) => Err(format!("round {round}: the anchor proof is being built from the node; ask again in a few seconds")),
            }
        })
    }
}
