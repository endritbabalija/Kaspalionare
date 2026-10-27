//! Configuration: command-line flags, each with an environment fallback.
//!
//! | Flag | Environment | Default |
//! |---|---|---|
//! | `--store <dir>` | `INDEXER_API_STORE` | required: the directory holding `index-log.jsonl` |
//! | `--bundle <file>` | `INDEXER_API_BUNDLE` | `<store>/bundle.json` |
//! | `--cov <hex>` | `INDEXER_API_COV` | required: the instance's covenant ID (the bundle's must equal it) |
//! | `--bind <addr:port>` | `INDEXER_API_BIND` | `127.0.0.1:8790` |
//! | `--allow-origin <origin>` (repeatable) | `INDEXER_API_ALLOW_ORIGIN` (comma list) | any origin |
//! | `--poll-ms <n>` | `INDEXER_API_POLL_MS` | 1000 |
//! | `--timeout-ms <n>` | `INDEXER_API_TIMEOUT_MS` | 10000 |
//! | `--max-connections <n>` | `INDEXER_API_MAX_CONNECTIONS` | 256 |
//! | `--per-client-burst <n>` | `INDEXER_API_PER_CLIENT_BURST` | 120 requests |
//! | `--per-client-rate <n>` | `INDEXER_API_PER_CLIENT_RATE` | 20 requests a second (`off`: no per-client limit) |
//! | `--client-ip <peer\|forwarded>` | `INDEXER_API_CLIENT_IP` | `peer`; `forwarded`: the last `X-Forwarded-For` entry when the peer is loopback (a local reverse proxy) |
//! | `--node <url\|resolver>` | `INDEXER_API_NODE` | none: serve the store only; with it, the one-process mode (`follow`): `wss://…`/`ws://…` a node's Borsh wRPC, `resolver` a public node |
//! | `--start <hash>` | `INDEXER_API_START` | a new store's first chain block (before genesis); a store with a log resumes from it |
//! | `--node-poll-ms <n>` | `INDEXER_API_NODE_POLL_MS` | 10000 |
//! | `--stale-ms <n>` | `INDEXER_API_STALE_MS` | 120000: `/health` is 503 when the node was not read for this long |
//!
//! Unknown flags are refused (so a misspelt one never goes unnoticed, and the production
//! binary has no `--doctor`).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kaspa_hashes::Hash;

use crate::follow::FollowOpts;
use crate::limits::ClientIp;
use crate::server::AnchorProvider;

#[derive(Clone)]
pub struct Config {
    pub store: PathBuf,
    pub bundle: PathBuf,
    pub cov: Hash,
    pub bind: SocketAddr,
    pub allow_origins: Vec<String>,
    pub poll: Duration,
    pub request_timeout: Duration,
    pub header_timeout: Duration,
    pub connection_lifetime: Duration,
    pub max_connections: usize,
    pub max_requests: usize,
    pub anchor: Option<Arc<dyn AnchorProvider>>,
    /// Per-client (burst, rate per second); `None`: no per-client limit.
    pub per_client: Option<(u64, u64)>,
    pub client_ip: ClientIp,
    /// The one-process mode (`--node`).
    pub follow: Option<FollowOpts>,
    /// `/health`'s staleness limit for the follower.
    pub stale: Duration,
    /// TEST-ONLY (feature `doctored`).
    #[cfg(feature = "doctored")]
    pub doctor: Option<crate::doctor::Mode>,
}

pub const DEFAULT_BIND: &str = "127.0.0.1:8790";

impl Config {
    /// A configuration with the defaults for `store` and `cov`.
    pub fn new(store: PathBuf, cov: Hash) -> Self {
        Config {
            bundle: store.join(crate::store::BUNDLE_FILE),
            store,
            cov,
            bind: DEFAULT_BIND.parse().expect("default bind"),
            allow_origins: vec![],
            poll: Duration::from_millis(1_000),
            request_timeout: Duration::from_millis(10_000),
            header_timeout: Duration::from_secs(10),
            connection_lifetime: Duration::from_secs(120),
            max_connections: 256,
            max_requests: 64,
            anchor: None,
            per_client: Some((120, 20)),
            client_ip: ClientIp::Peer,
            follow: None,
            stale: Duration::from_secs(120),
            #[cfg(feature = "doctored")]
            doctor: None,
        }
    }
}

pub const USAGE: &str = "usage: indexer-api --store <dir> --cov <hex> [--bundle <file>] [--bind <addr:port>] [--allow-origin <origin>]... [--poll-ms <n>] [--timeout-ms <n>] [--max-connections <n>] [--per-client-burst <n>] [--per-client-rate <n|off>] [--client-ip <peer|forwarded>] [--node <url|resolver> [--start <hash>] [--node-poll-ms <n>] [--stale-ms <n>]]";

fn hash_of(s: &str) -> Result<Hash, String> {
    let b = lottery::store::from_hex(&s.to_ascii_lowercase()).map_err(|e| format!("--cov: {e}"))?;
    let a: [u8; 32] = b.try_into().map_err(|_| "--cov: not 32 bytes".to_string())?;
    Ok(Hash::from_bytes(a))
}

fn num(what: &str, s: &str) -> Result<u64, String> {
    s.parse::<u64>().ok().filter(|&n| n > 0).ok_or_else(|| format!("{what}: `{s}` is not a positive integer"))
}

/// Parse `args` (without the program name). `extra` takes flags of a calling binary (the
/// doctored one): it gets the flag and its value and returns whether it took them.
pub fn parse(args: &[String], env: impl Fn(&str) -> Option<String>, mut extra: impl FnMut(&str, &str) -> Result<bool, String>) -> Result<Config, String> {
    let mut store = env("INDEXER_API_STORE").map(PathBuf::from);
    let mut bundle = env("INDEXER_API_BUNDLE").map(PathBuf::from);
    let mut cov = env("INDEXER_API_COV");
    let mut bind = env("INDEXER_API_BIND");
    let mut origins: Vec<String> = env("INDEXER_API_ALLOW_ORIGIN").map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default();
    let mut poll = env("INDEXER_API_POLL_MS");
    let mut timeout = env("INDEXER_API_TIMEOUT_MS");
    let mut maxc = env("INDEXER_API_MAX_CONNECTIONS");
    let mut burst = env("INDEXER_API_PER_CLIENT_BURST");
    let mut rate = env("INDEXER_API_PER_CLIENT_RATE");
    let mut client_ip = env("INDEXER_API_CLIENT_IP");
    let mut node = env("INDEXER_API_NODE");
    let mut start = env("INDEXER_API_START");
    let mut node_poll = env("INDEXER_API_NODE_POLL_MS");
    let mut stale = env("INDEXER_API_STALE_MS");
    let mut cli_origins = vec![];
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = || it.clone().next().cloned().ok_or_else(|| format!("{flag} needs a value"));
        let v = value()?;
        it.next();
        match flag.as_str() {
            "--store" => store = Some(PathBuf::from(v)),
            "--bundle" => bundle = Some(PathBuf::from(v)),
            "--cov" => cov = Some(v),
            "--bind" => bind = Some(v),
            "--allow-origin" => cli_origins.push(v),
            "--poll-ms" => poll = Some(v),
            "--timeout-ms" => timeout = Some(v),
            "--max-connections" => maxc = Some(v),
            "--per-client-burst" => burst = Some(v),
            "--per-client-rate" => rate = Some(v),
            "--client-ip" => client_ip = Some(v),
            "--node" => node = Some(v),
            "--start" => start = Some(v),
            "--node-poll-ms" => node_poll = Some(v),
            "--stale-ms" => stale = Some(v),
            f => {
                if !extra(f, &v)? {
                    return Err(format!("unknown flag {f}\n{USAGE}"));
                }
            }
        }
    }
    if !cli_origins.is_empty() {
        origins = cli_origins;
    }
    let store = store.ok_or_else(|| format!("--store <dir> is required\n{USAGE}"))?;
    let cov = hash_of(&cov.ok_or_else(|| format!("--cov <hex> is required\n{USAGE}"))?)?;
    let mut c = Config::new(store, cov);
    if let Some(b) = bundle {
        c.bundle = b;
    }
    if let Some(b) = bind {
        c.bind = b.parse().map_err(|e| format!("--bind {b}: {e}"))?;
    }
    c.allow_origins = origins;
    if let Some(p) = poll {
        c.poll = Duration::from_millis(num("--poll-ms", &p)?);
    }
    if let Some(t) = timeout {
        c.request_timeout = Duration::from_millis(num("--timeout-ms", &t)?);
    }
    if let Some(m) = maxc {
        c.max_connections = num("--max-connections", &m)? as usize;
    }
    match rate.as_deref() {
        Some("off") => c.per_client = None,
        Some(r) => c.per_client = Some((c.per_client.map(|p| p.0).unwrap_or(120), num("--per-client-rate", r)?)),
        None => {}
    }
    if let Some(b) = burst {
        let b = num("--per-client-burst", &b)?;
        c.per_client = c.per_client.map(|(_, r)| (b, r));
    }
    if let Some(m) = client_ip {
        c.client_ip = ClientIp::parse(&m)?;
    }
    if let Some(s) = stale {
        c.stale = Duration::from_millis(num("--stale-ms", &s)?);
    }
    if start.is_some() && node.is_none() {
        return Err("--start needs --node (the one-process mode)".into());
    }
    if let Some(n) = node {
        let url = match n.as_str() {
            "resolver" => None,
            u if u.starts_with("wss://") || u.starts_with("ws://") => {
                crate::follow::check_url(u)?;
                Some(u.to_string())
            }
            u => return Err(format!("--node {u}: a wss:// or ws:// wRPC (Borsh) URL, or `resolver`")),
        };
        let start = start.map(|h| hash_of(&h).map_err(|e| e.replace("--cov", "--start"))).transpose()?;
        let every = Duration::from_millis(node_poll.map(|p| num("--node-poll-ms", &p)).transpose()?.unwrap_or(10_000));
        c.follow = Some(FollowOpts { node: url, start, every });
    }
    Ok(c)
}
