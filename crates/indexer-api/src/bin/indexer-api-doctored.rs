//! TEST-ONLY: `indexer-api-doctored` (feature `doctored`): the server with one doctored mode
//! (`--doctor <mode>`, required), for the untrusted-API tests of the page (spec frontend 8).
//! It binds to a loopback address only and marks every response `x-indexer-doctored`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut mode = None;
    let parsed = indexer_api::config::parse(
        &args,
        |k| std::env::var(k).ok(),
        |f, v| match f {
            "--doctor" => {
                mode = Some(indexer_api::doctor::Mode::parse(v)?);
                Ok(true)
            }
            _ => Ok(false),
        },
    );
    let mut cfg = match parsed {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}\n  (doctored build: also --doctor <mode>)");
            std::process::exit(2);
        }
    };
    let Some(m) = mode else {
        eprintln!("--doctor <mode> is required: one of {}", indexer_api::doctor::ALL.map(|m| m.name()).join(", "));
        std::process::exit(2);
    };
    if !cfg.bind.ip().is_loopback() {
        eprintln!("the doctored server binds to a loopback address only (got {})", cfg.bind);
        std::process::exit(2);
    }
    cfg.doctor = Some(m);
    eprintln!("{}: TEST-ONLY doctored indexer API, mode {}: every response may lie", indexer_api::doctor::MARKER, m.name());
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio runtime");
    if let Err(e) = rt.block_on(indexer_api::run(cfg)) {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}
