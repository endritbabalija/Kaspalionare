//! The dev faucet (front end F3, spec 1.1 DX): funds the page's quick testnet wallet on the
//! private simnet. It exists only in this binary (`simnet dev`), never in the page or the
//! indexer API: a production build of the site has no faucet to call (`web/` checks its
//! bundle for the faucet path).
//!
//! `POST /faucet?address=kaspasim:...` pays `AMOUNT` from the dev run's throwaway wallet to a
//! `kaspasim:` address and answers `{"transactionId": "<hex>", "amount": "<sompi>"}` (the dev
//! loop's next blocks accept it). Anything else is a JSON error. Loopback only, at most
//! `MAX_REQUESTS` payments per run, one at a time. Cross-origin requests are refused (F2/F3
//! review note): a browser request must come from the dev page's origin (`PAGE_ORIGINS`, the
//! fixed `npm run dev:simnet` address), so another site open in the same browser cannot drain
//! the dev wallet with a no-cors POST; CORS names that origin only. A request with no `Origin`
//! (a local tool such as curl, which can reach the loopback node anyway) is served.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use kaspa_consensus_core::tx::TransactionOutpoint as Op;
use kaspa_rpc_core::RpcTransaction;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::KaspaRpcClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::wallet::{KAS, Wallet};

/// What one request pays: enough for many b = 4 tickets at the dev knobs.
pub const AMOUNT: u64 = 100 * KAS;
/// The most payments in one dev run.
pub const MAX_REQUESTS: usize = 200;
/// The request head cap.
const HEAD_MAX: usize = 8 << 10;
/// The dev page's origins (`web/scripts/dev-simnet.mjs` serves it on 127.0.0.1:5173, strict port).
pub const PAGE_ORIGINS: [&str; 2] = ["http://127.0.0.1:5173", "http://localhost:5173"];

/// The request's `Origin` when a browser sent one: Ok(None) without one, Ok(Some(origin)) for the
/// dev page, Err for any other origin (and for a repeated header).
pub fn origin_check(head: &str) -> Result<Option<&'static str>, String> {
    let mut found: Vec<&str> = head.lines().skip(1).filter_map(|l| l.split_once(':')).filter(|(k, _)| k.trim().eq_ignore_ascii_case("origin")).map(|(_, v)| v.trim()).collect();
    match found.len() {
        0 => Ok(None),
        1 => {
            let o = found.remove(0);
            PAGE_ORIGINS.iter().find(|p| **p == o).map(|p| Some(*p)).ok_or_else(|| format!("origin {o} is not the dev page"))
        }
        _ => Err("more than one Origin header".into()),
    }
}

/// The dev wallet's coins the faucet has spent and its payment count; also the lock a dev round
/// (`/round`, below) holds while it builds from the same wallet, so the two never spend one coin.
pub type Spent = Arc<Mutex<(HashSet<Op>, usize)>>;

/// F4 (TEST-ONLY dev tool, simnet binary only): a request to run the dev instance's rounds forward so
/// that the page's combination `target` wins: the closing round is drawn, the open round closed, its
/// anchor ground for `target` (the jackpot) and drawn (`dev.rs`, `advance`).
/// With `target` none (`POST /close`, F4 review e2e): the closing round is drawn and the open round
/// closed, and nothing more, so the page sees a round closing with its marker live.
pub struct DevRound {
    pub target: Option<([u8; 5], u8)>,
    /// F5 (`POST /mine?to=deadline|anchor|void`, TEST-ONLY): mine instead, so the page's own S4
    /// panel finds `close()`, `draw()` or the void due (`dev.rs`, `mine_to`).
    pub mine: Option<MineTo>,
    pub reply: tokio::sync::oneshot::Sender<Result<serde_json::Value, String>>,
}

/// F5 (TEST-ONLY dev tool): how far `POST /mine` mines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MineTo {
    /// Past the open round's deadline T (its `close()` valid).
    Deadline,
    /// Past the closing round's D, and 60 blocks on (its anchor proof can be built).
    Anchor,
    /// Past the closing round's D + draw_window (only its void valid; about 864,000 blocks).
    Void,
    /// F6: past the live vault's creation DAA + VAULT_REQ_DELAY (the vault takes a request).
    Vault,
}

impl MineTo {
    pub fn parse(q: &str) -> Result<MineTo, String> {
        match q.split('&').find_map(|kv| kv.strip_prefix("to=")) {
            Some("deadline") => Ok(MineTo::Deadline),
            Some("anchor") => Ok(MineTo::Anchor),
            Some("void") => Ok(MineTo::Void),
            Some("vault") => Ok(MineTo::Vault),
            _ => Err("to=deadline, to=anchor, to=void or to=vault is required".into()),
        }
    }
}

struct State {
    c: Arc<KaspaRpcClient>,
    w: Wallet,
    spent: Spent,
    rounds: Option<tokio::sync::mpsc::Sender<DevRound>>,
}

/// Start the faucet on `bind` (loopback only); runs until the task is dropped. With `rounds`, it
/// also takes `POST /round?main=a,b,c,d,e&bonus=n` (F4, the e2e suite's deterministic draws),
/// `POST /close` (F4 review) and `POST /mine?to=deadline|anchor|void|vault` (F5, F6).
pub async fn serve(bind: SocketAddr, c: Arc<KaspaRpcClient>, w: Wallet, spent: Spent, rounds: Option<tokio::sync::mpsc::Sender<DevRound>>) -> Result<tokio::task::JoinHandle<()>, String> {
    if !bind.ip().is_loopback() {
        return Err("--faucet-bind: the dev faucet binds to a loopback address only".into());
    }
    let l = tokio::net::TcpListener::bind(bind).await.map_err(|e| format!("faucet bind {bind}: {e}"))?;
    let st = Arc::new(State { c, w, spent, rounds });
    println!("dev faucet: POST http://{bind}/faucet?address=<kaspasim:...> pays {} KAS (simnet only)", AMOUNT / KAS);
    Ok(tokio::spawn(async move {
        loop {
            let Ok((s, peer)) = l.accept().await else { continue };
            if !peer.ip().is_loopback() {
                continue;
            }
            let st = st.clone();
            tokio::spawn(async move {
                // A dev round mines a whole round forward: minutes, not seconds (a void, F5: about
                // 864,000 blocks, 15-40 minutes on this PC as the node's store grows).
                let _ = tokio::time::timeout(Duration::from_secs(5_400), handle(s, st)).await;
            });
        }
    }))
}

fn reply(status: &str, origin: Option<&str>, body: &str) -> String {
    let cors = origin.map(|o| format!("access-control-allow-origin: {o}\r\nvary: origin\r\naccess-control-allow-methods: POST, OPTIONS\r\n")).unwrap_or_default();
    format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\n{cors}cache-control: no-store\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len())
}

fn err(status: &str, origin: Option<&str>, msg: &str) -> String {
    reply(status, origin, &serde_json::json!({ "error": msg }).to_string())
}

async fn handle(mut s: tokio::net::TcpStream, st: Arc<State>) {
    let mut buf = vec![0u8; HEAD_MAX];
    let mut n = 0;
    while n < HEAD_MAX {
        match tokio::time::timeout(Duration::from_secs(5), s.read(&mut buf[n..])).await {
            Ok(Ok(0)) | Err(_) | Ok(Err(_)) => break,
            Ok(Ok(k)) => n += k,
        }
        if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let head = String::from_utf8_lossy(&buf[..n]).into_owned();
    let line = head.lines().next().unwrap_or("");
    let mut parts = line.split(' ');
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let origin = match origin_check(&head) {
        Ok(o) => o,
        Err(e) => {
            println!("dev faucet: refused a cross-origin request: {e}");
            let _ = s.write_all(err("403 Forbidden", None, &e).as_bytes()).await;
            let _ = s.shutdown().await;
            return;
        }
    };
    // A path with no query (`POST /close`) reads as one with an empty query.
    let resp = match (method, Some(target.split_once('?').unwrap_or((target, "")))) {
        ("OPTIONS", _) => reply("204 No Content", origin, ""),
        ("POST", Some(("/faucet", q))) => match q.split('&').find_map(|kv| kv.strip_prefix("address=")) {
            Some(a) => match pay(&st, &a.replace("%3A", ":").replace("%3a", ":")).await {
                Ok((id, amount)) => {
                    println!("dev faucet: paid {} KAS to {a}: {id}", amount / KAS);
                    reply("200 OK", origin, &serde_json::json!({ "transactionId": id, "amount": amount.to_string() }).to_string())
                }
                Err(e) => err("400 Bad Request", origin, &e),
            },
            None => err("400 Bad Request", origin, "address= is required"),
        },
        ("POST", Some(("/round", q))) | ("POST", Some(("/close", q))) | ("POST", Some(("/mine", q))) => match (
            &st.rounds,
            if target.starts_with("/close") {
                Ok((None, None))
            } else if target.starts_with("/mine") {
                MineTo::parse(q).map(|m| (None, Some(m)))
            } else {
                parse_round(q).map(|t| (Some(t), None))
            },
        ) {
            (None, _) => err("404 Not Found", origin, "no dev rounds in this run"),
            (_, Err(e)) => err("400 Bad Request", origin, &e),
            (Some(tx), Ok((target, mine))) => {
                let (back, rx) = tokio::sync::oneshot::channel();
                if tx.send(DevRound { target, mine, reply: back }).await.is_err() {
                    err("503 Service Unavailable", origin, "the dev loop stopped")
                } else {
                    match rx.await {
                        Ok(Ok(v)) => reply("200 OK", origin, &v.to_string()),
                        Ok(Err(e)) => err("500 Internal Server Error", origin, &e),
                        Err(_) => err("503 Service Unavailable", origin, "the dev loop dropped the request"),
                    }
                }
            }
        },
        ("POST", _) | ("GET", _) => err("404 Not Found", origin, "POST /faucet?address=<kaspasim:...>"),
        _ => err("405 Method Not Allowed", origin, "POST only"),
    };
    let _ = s.write_all(resp.as_bytes()).await;
    let _ = s.shutdown().await;
}

/// `main=a,b,c,d,e&bonus=n`: five ascending distinct numbers in 1..=30 and a bonus of at least 1.
pub fn parse_round(q: &str) -> Result<([u8; 5], u8), String> {
    let get = |k: &str| q.split('&').find_map(|kv| kv.strip_prefix(k)).ok_or(format!("{k} is required"));
    let main: Vec<u8> = get("main=")?.replace("%2C", ",").replace("%2c", ",").split(',').map(|n| n.parse::<u8>().map_err(|_| format!("main: {n}"))).collect::<Result<_, _>>()?;
    let bonus: u8 = get("bonus=")?.parse().map_err(|_| "bonus: not a number".to_string())?;
    let main: [u8; 5] = main.try_into().map_err(|_| "main: five numbers".to_string())?;
    if !main.windows(2).all(|w| w[0] < w[1]) || main[0] < 1 || main[4] > 30 || bonus < 1 {
        return Err("main: five ascending numbers in 1..=30; bonus >= 1".into());
    }
    Ok((main, bonus))
}

/// The most coins one faucet payment spends (its mass stays far below the standard limit).
const MAX_INPUTS: usize = 40;

async fn pay(st: &State, address: &str) -> Result<(String, u64), String> {
    let to = crate::net::parse_simnet(address)?;
    let mut g = st.spent.lock().await;
    if g.1 >= MAX_REQUESTS {
        return Err(format!("the faucet paid {MAX_REQUESTS} times this run; restart the dev run"));
    }
    let vdaa = st.c.get_block_dag_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    // F3 part B: the largest unspent coins (the dev loop mines small coinbase coins to this wallet), up to
    // MAX_INPUTS of them, until they cover the payment and its fee; one coin when one is large enough.
    let mut ins = vec![];
    let mut total = 0u64;
    for u in st.w.spendable(&*st.c, vdaa).await?.into_iter().filter(|u| !g.0.contains(&u.op)) {
        if total >= AMOUNT + KAS || ins.len() >= MAX_INPUTS {
            break;
        }
        total += u.entry.amount;
        ins.push(u);
    }
    if total < AMOUNT + KAS {
        return Err("the dev wallet has no spendable coins large enough yet; retry in a minute".into());
    }
    let tx = st.w.pay(&ins, &kaspa_txscript::pay_to_address_script(&to), AMOUNT)?;
    st.c.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| format!("the node rejected the faucet payment: {e}"))?;
    g.0.extend(ins.iter().map(|u| u.op));
    g.1 += 1;
    Ok((tx.id().to_string(), AMOUNT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_round_requests_are_parsed() {
        assert_eq!(parse_round("main=1,5,9,14,30&bonus=2"), Ok(([1, 5, 9, 14, 30], 2)));
        assert_eq!(parse_round("bonus=1&main=1%2C2%2C3%2C4%2C5"), Ok(([1, 2, 3, 4, 5], 1)));
        for bad in ["main=1,2,3,4&bonus=1", "main=5,4,3,2,1&bonus=1", "main=1,2,3,4,31&bonus=1", "main=1,2,3,4,5&bonus=0", "main=1,2,3,4,5", "bonus=1"] {
            assert!(parse_round(bad).is_err(), "{bad}");
        }
    }

    /// F2/F3 review note: only the dev page's origin (or no browser at all) may ask for coins.
    #[test]
    fn mine_targets_parse() {
        assert_eq!(MineTo::parse("to=deadline"), Ok(MineTo::Deadline));
        assert_eq!(MineTo::parse("x=1&to=anchor"), Ok(MineTo::Anchor));
        assert_eq!(MineTo::parse("to=void"), Ok(MineTo::Void));
        assert_eq!(MineTo::parse("to=vault"), Ok(MineTo::Vault));
        assert!(MineTo::parse("").is_err() && MineTo::parse("to=draw").is_err());
    }

    #[test]
    fn cross_origin_requests_are_refused() {
        let head = |o: &str| format!("POST /faucet?address=kaspasim:qq HTTP/1.1\r\nHost: 127.0.0.1:8791\r\n{o}\r\n");
        assert_eq!(origin_check(&head("Origin: http://127.0.0.1:5173\r\n")), Ok(Some("http://127.0.0.1:5173")));
        assert_eq!(origin_check(&head("origin: http://localhost:5173\r\n")), Ok(Some("http://localhost:5173")));
        assert_eq!(origin_check(&head("")), Ok(None), "curl and other local tools send no Origin");
        for bad in ["Origin: https://evil.example\r\n", "Origin: http://127.0.0.1:5174\r\n", "Origin: null\r\n", "Origin: http://127.0.0.1:5173\r\nOrigin: http://127.0.0.1:5173\r\n"] {
            assert!(origin_check(&head(bad)).is_err(), "{bad}");
        }
    }
}
