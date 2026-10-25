//! Private simnet runner (M8 S4, spec A5b, D114, D125). Run from `crates/`:
//!   cargo run -j 2 -p simnet --release -- <subcommand>
//!
//! Subcommands:
//! - `mine [n]`: start a fresh node, mine n blocks (default 100) to a throwaway simnet
//!   address, print the DAG, stop.
//! - `smoke [n]`: start a fresh node, mine max(n, 1,060) blocks (coinbase maturity is
//!   1,000 DAA on simnet), then genesis, buy() and close() of round 1 with the production
//!   templates, each mined from the node's templates.
//! - `dev [--api-bind <addr:port>] [--node-port <port>] [--faucet-bind <addr:port>] [--for <seconds>]
//!   [--block-ms <n>]` (front-end F2, F3): one instance with a drawn, a closing and an open
//!   round, indexed and served by the indexer API, the node's Borsh wRPC on a fixed loopback
//!   port (default 18610) and the dev faucet (`faucet.rs`), then blocks mined every
//!   `--block-ms` until Ctrl-C (`dev.rs`).
//! - `sn1` .. `sn4`: the forced-jackpot scenarios of spec A5b (one fresh node and genesis
//!   each; the anchor block ground, `sn.rs`); `sn5` runs SN1 and reports its grinding numbers;
//!   `all` runs SN1-SN4, each in a child process (one node per process), and SN5 over
//!   their four grinds.
//!
//! The node lives only as long as the command (in-process), with no peers, and listens on
//! 127.0.0.1 only. Every run uses a fresh data directory under `crates/target/simnet-data/`.

// Global allocator of the node (as kaspad's main, `kaspad/src/main.rs:7,23`).
use kaspa_alloc::init_allocator_with_default_settings;

// The KIP-21 anchor proof from RPC, shared with tn10 unchanged (generic over `RpcApi`; S3a).
// Used by the grinder's draw() (sn.rs).
#[allow(dead_code)]
#[path = "../../tn10/src/anchor.rs"]
mod anchor;
mod dev;
mod faucet;
// The M8 indexer, shared with tn10 unchanged (generic over `RpcApi`); the dev run follows the
// node with it and writes `index/1` for the indexer API.
#[allow(dead_code)]
#[path = "../../tn10/src/indexer.rs"]
mod indexer;
mod miner;
mod net;
mod node;
mod smoke;
mod sn;
mod wallet;

fn usage() -> ! {
    eprintln!("usage: simnet <mine [n] | smoke [n] | sn1 | sn2 | sn3 | sn4 | sn5 | all | dev [--api-bind <addr:port>] [--node-port <port>] [--faucet-bind <addr:port>] [--for <seconds>] [--block-ms <n>]>");
    std::process::exit(2);
}

fn main() {
    init_allocator_with_default_settings();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let n = |d: u64| args.get(1).map(|s| s.parse::<u64>().unwrap_or_else(|_| usage())).unwrap_or(d);
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio runtime");
    let r = match args.first().map(String::as_str) {
        Some("mine") => rt.block_on(mine_cmd(n(100))),
        Some("smoke") => rt.block_on(smoke::run(n(0))),
        Some("dev") => dev_opts(&args[1..]).and_then(|o| rt.block_on(dev::run(o))),
        Some(id @ ("sn1" | "sn2" | "sn3" | "sn4" | "sn5" | "all")) => rt.block_on(sn_cmd(id)),
        _ => usage(),
    };
    if let Err(e) = r {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}

fn dev_opts(a: &[String]) -> Result<dev::Opts, String> {
    let mut o = dev::Opts {
        api_bind: "127.0.0.1:8790".parse().expect("bind"),
        node_port: dev::NODE_PORT,
        faucet_bind: "127.0.0.1:8791".parse().expect("bind"),
        run_for: None,
        block_ms: 200,
    };
    let mut it = a.iter();
    while let Some(f) = it.next() {
        let v = it.next().ok_or(format!("{f} needs a value"))?;
        match f.as_str() {
            "--api-bind" => {
                o.api_bind = v.parse().map_err(|e| format!("--api-bind {v}: {e}"))?;
                if !o.api_bind.ip().is_loopback() {
                    return Err("--api-bind: the dev API binds to a loopback address only".into());
                }
            }
            "--node-port" => o.node_port = v.parse().map_err(|e| format!("--node-port {v}: {e}"))?,
            "--faucet-bind" => {
                o.faucet_bind = v.parse().map_err(|e| format!("--faucet-bind {v}: {e}"))?;
                if !o.faucet_bind.ip().is_loopback() {
                    return Err("--faucet-bind: the dev faucet binds to a loopback address only".into());
                }
            }
            "--for" => o.run_for = Some(std::time::Duration::from_secs(v.parse().map_err(|e| format!("--for {v}: {e}"))?)),
            "--block-ms" => o.block_ms = v.parse().map_err(|e| format!("--block-ms {v}: {e}"))?,
            _ => return Err(format!("unknown flag {f}")),
        }
    }
    Ok(o)
}

async fn mine_cmd(n: u64) -> Result<(), String> {
    use kaspa_rpc_core::api::rpc::RpcApi;
    let node = node::Node::start()?;
    let r = async {
        let c = node.client().await?;
        let w = wallet::Wallet::random();
        let t0 = std::time::Instant::now();
        let last = miner::mine(&c, &w.addr, n).await?;
        let dag = c.get_block_dag_info().await.map_err(|e| e.to_string())?;
        println!(
            "mined {n} blocks to {} in {:.1} s; last {:?}; block count {}, tips {}, virtual DAA {}",
            w.addr,
            t0.elapsed().as_secs_f64(),
            last,
            dag.block_count,
            dag.tip_hashes.len(),
            dag.virtual_daa_score
        );
        let _ = c.disconnect().await;
        Ok::<(), String>(())
    }
    .await;
    node.shutdown();
    r
}

async fn sn_cmd(id: &str) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    if id == "all" {
        // One node per process: the node's logger initialises once per process
        // (`core/src/log/mod.rs:53`), so each scenario runs as a child of this binary.
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut rows = vec![];
        for s in ["sn1", "sn2", "sn3", "sn4"] {
            let out = std::process::Command::new(&exe).arg(s).output().map_err(|e| format!("{s}: {e}"))?;
            let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
            print!("{text}");
            if !out.status.success() {
                return Err(format!("{s} failed ({})", out.status));
            }
            rows.extend(text.lines().filter(|l| l.starts_with("| SN") && !l.starts_with("| SN |")).map(String::from));
        }
        println!("
======== SN5 (all grinds) ========");
        println!("| SN | round | ground for | tries | expected 1/p | seconds | tries / expected | ground X | honest first candidate |");
        println!("|---|---|---|---|---|---|---|---|---|");
        for r in &rows {
            println!("{r}");
        }
        if rows.len() != 4 {
            return Err(format!("SN5: {} grind rows, expected 4", rows.len()));
        }
        println!("SN5 PASS (each honest first candidate differs from the ground result; checked in each child)");
        println!("all: done in {:.1} s", t0.elapsed().as_secs_f64());
        return Ok(());
    }
    let r = match id {
        "sn1" | "sn5" => sn::sn1().await,
        "sn2" => sn::sn2().await,
        "sn3" => sn::sn3().await,
        _ => sn::sn4().await,
    }?;
    println!("{} PASS", r.id);
    let logs = vec![r];
    sn::print_report(&logs);
    sn::sn5(&logs)?;
    if id == "sn5" {
        println!("SN5 PASS (grind of SN1)");
    }
    println!("{id}: done in {:.1} s", t0.elapsed().as_secs_f64());
    Ok(())
}
