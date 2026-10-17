//! Private simnet runner (M8 S4, spec A5b, D114, D125). Run from `crates/`:
//!   cargo run -j 2 -p simnet --release -- <subcommand>
//!
//! Subcommands:
//! - `mine [n]`: start a fresh node, mine n blocks (default 100) to a throwaway simnet
//!   address, print the DAG, stop.
//! - `smoke [n]`: start a fresh node, mine max(n, 1,060) blocks (coinbase maturity is
//!   1,000 DAA on simnet), then genesis, buy() and close() of round 1 with the production
//!   templates, each mined from the node's templates.
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
mod miner;
mod net;
mod node;
mod smoke;
mod sn;
mod wallet;

fn usage() -> ! {
    eprintln!("usage: simnet <mine [n] | smoke [n] | sn1 | sn2 | sn3 | sn4 | sn5 | all>");
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
        Some(id @ ("sn1" | "sn2" | "sn3" | "sn4" | "sn5" | "all")) => rt.block_on(sn_cmd(id)),
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
