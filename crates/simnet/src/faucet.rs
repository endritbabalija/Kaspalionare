//! The dev faucet (front end F3, spec 1.1 DX): funds the page's quick testnet wallet on the
//! private simnet. It exists only in this binary (`simnet dev`), never in the page or the
//! indexer API: a production build of the site has no faucet to call (`web/` checks its
//! bundle for the faucet path).
//!
//! `POST /faucet?address=kaspasim:...` pays `AMOUNT` from the dev run's throwaway wallet to a
//! `kaspasim:` address and answers `{"transactionId": "<hex>", "amount": "<sompi>"}` (the dev
//! loop's next blocks accept it). Anything else is a JSON error. Loopback only, at most
//! `MAX_REQUESTS` payments per run, one at a time; CORS open (the page is another loopback
//! origin; a POST with no body and no custom header needs no preflight, but `OPTIONS` is
//! answered anyway).

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

struct State {
    c: Arc<KaspaRpcClient>,
    w: Wallet,
    spent: Mutex<(HashSet<Op>, usize)>,
}

/// Start the faucet on `bind` (loopback only); runs until the task is dropped.
pub async fn serve(bind: SocketAddr, c: Arc<KaspaRpcClient>, w: Wallet) -> Result<tokio::task::JoinHandle<()>, String> {
    if !bind.ip().is_loopback() {
        return Err("--faucet-bind: the dev faucet binds to a loopback address only".into());
    }
    let l = tokio::net::TcpListener::bind(bind).await.map_err(|e| format!("faucet bind {bind}: {e}"))?;
    let st = Arc::new(State { c, w, spent: Mutex::new((HashSet::new(), 0)) });
    println!("dev faucet: POST http://{bind}/faucet?address=<kaspasim:...> pays {} KAS (simnet only)", AMOUNT / KAS);
    Ok(tokio::spawn(async move {
        loop {
            let Ok((s, peer)) = l.accept().await else { continue };
            if !peer.ip().is_loopback() {
                continue;
            }
            let st = st.clone();
            tokio::spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(30), handle(s, st)).await;
            });
        }
    }))
}

fn reply(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\naccess-control-allow-origin: *\r\naccess-control-allow-methods: POST, OPTIONS\r\ncache-control: no-store\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn err(status: &str, msg: &str) -> String {
    reply(status, &serde_json::json!({ "error": msg }).to_string())
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
    let resp = match (method, target.split_once('?')) {
        ("OPTIONS", _) => reply("204 No Content", ""),
        ("POST", Some(("/faucet", q))) => match q.split('&').find_map(|kv| kv.strip_prefix("address=")) {
            Some(a) => match pay(&st, &a.replace("%3A", ":").replace("%3a", ":")).await {
                Ok((id, amount)) => {
                    println!("dev faucet: paid {} KAS to {a}: {id}", amount / KAS);
                    reply("200 OK", &serde_json::json!({ "transactionId": id, "amount": amount.to_string() }).to_string())
                }
                Err(e) => err("400 Bad Request", &e),
            },
            None => err("400 Bad Request", "address= is required"),
        },
        ("POST", _) | ("GET", _) => err("404 Not Found", "POST /faucet?address=<kaspasim:...>"),
        _ => err("405 Method Not Allowed", "POST only"),
    };
    let _ = s.write_all(resp.as_bytes()).await;
    let _ = s.shutdown().await;
}

async fn pay(st: &State, address: &str) -> Result<(String, u64), String> {
    let to = crate::net::parse_simnet(address)?;
    let mut g = st.spent.lock().await;
    if g.1 >= MAX_REQUESTS {
        return Err(format!("the faucet paid {MAX_REQUESTS} times this run; restart the dev run"));
    }
    let vdaa = st.c.get_block_dag_info().await.map_err(|e| e.to_string())?.virtual_daa_score;
    let coin = st
        .w
        .spendable(&*st.c, vdaa)
        .await?
        .into_iter()
        .find(|u| !g.0.contains(&u.op) && u.entry.amount >= AMOUNT + KAS)
        .ok_or("the dev wallet has no spendable coin large enough yet; retry in a minute")?;
    let tx = st.w.pay(std::slice::from_ref(&coin), &kaspa_txscript::pay_to_address_script(&to), AMOUNT)?;
    st.c.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| format!("the node rejected the faucet payment: {e}"))?;
    g.0.insert(coin.op);
    g.1 += 1;
    Ok((tx.id().to_string(), AMOUNT))
}
