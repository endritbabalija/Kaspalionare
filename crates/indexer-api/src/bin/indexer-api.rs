//! `indexer-api`: the production server (spec frontend 6.3). Flags: `config.rs`. It has no
//! doctored mode in any build (`--doctor` is an unknown flag; `tests/production.rs`).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = match indexer_api::config::parse(&args, |k| std::env::var(k).ok(), |_, _| Ok(false)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio runtime");
    if let Err(e) = rt.block_on(indexer_api::run(cfg)) {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}
