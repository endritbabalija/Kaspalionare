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
//!
//! Unknown flags are refused (so a misspelt one never goes unnoticed, and the production
//! binary has no `--doctor`).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kaspa_hashes::Hash;

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
            #[cfg(feature = "doctored")]
            doctor: None,
        }
    }
}

pub const USAGE: &str = "usage: indexer-api --store <dir> --cov <hex> [--bundle <file>] [--bind <addr:port>] [--allow-origin <origin>]... [--poll-ms <n>] [--timeout-ms <n>] [--max-connections <n>]";

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
    Ok(c)
}
