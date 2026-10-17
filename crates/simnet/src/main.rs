//! Private simnet runner (M8 S4, spec A5b, D114, D125). Run from `crates/`:
//!   cargo run -j 2 -p simnet --release -- <subcommand>
//!
//! Subcommands:
//! - `mine [n]`: start a fresh node, mine n blocks (default 100) to a throwaway simnet
//!   address, print the DAG, stop.
//! - `smoke [n]`: start a fresh node, mine max(n, 1,060) blocks (coinbase maturity is
//!   1,000 DAA on simnet), then genesis, buy() and close() of round 1 with the production
//!   templates, each mined from the node's templates.
//!
//! The node lives only as long as the command (in-process), with no peers, and listens on
//! 127.0.0.1 only. Every run uses a fresh data directory under `crates/target/simnet-data/`.

// Global allocator of the node (as kaspad's main, `kaspad/src/main.rs:7,23`).
use kaspa_alloc::init_allocator_with_default_settings;

// The KIP-21 anchor proof from RPC, shared with tn10 unchanged (generic over `RpcApi`; S3a).
// Used by the part-2 grinder (draw() on simnet).
#[allow(dead_code)]
#[path = "../../tn10/src/anchor.rs"]
mod anchor;
mod miner;
mod net;
mod node;
mod smoke;
mod wallet;

fn usage() -> ! {
    eprintln!("usage: simnet <mine [n] | smoke [n]>");
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
        _ => usage(),
    };
    if let Err(e) = r {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
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
