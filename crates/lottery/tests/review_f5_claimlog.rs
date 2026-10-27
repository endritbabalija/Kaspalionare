//! F5 review probe (reviewer, not a regression test; ignored by default). Reads every
//! `crates/target/simnet-dev/<cov>/index-log.jsonl` the F5 stage's dev runs left and prints each
//! claim: the txid, the ticket outpoint it spends, the ticket's round, b, the claimed bits and
//! index, the pool tier and outpoint, and the payout output values. Evidence for the
//! two-claimers-one-wallet race (`tickets.spec.ts:185`).
//! Run: `cargo test -j 4 -p lottery --test review_f5_claimlog -- --ignored --nocapture`
use lottery::index::{Event, State, classify};

#[test]
#[ignore]
fn print_claims_of_dev_stores() {
    let t = lottery::buy::Templates::get(lottery::buy::b_max());
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/simnet-dev");
    let only = std::env::var("F5_STORE").ok();
    for d in std::fs::read_dir(&root).unwrap().flatten() {
        let p = d.path().join("index-log.jsonl");
        if !p.exists() || only.as_deref().is_some_and(|o| !d.path().ends_with(o)) {
            continue;
        }
        let log = lottery::store::IndexLog::from_jsonl(&std::fs::read_to_string(&p).unwrap()).unwrap();
        println!("== store {}", d.file_name().to_string_lossy());
        for l in &log.lines {
            let a = l.accepted().unwrap();
            let ev = classify(&a.tx, &t, log.cov);
            let name = ev.as_ref().map(|e| e.name()).unwrap_or("foreign");
            if let Some(Event::Claim(c)) = &ev {
                let ins: Vec<String> = a.tx.inputs.iter().map(|i| format!("{}:{}", &i.previous_outpoint.transaction_id.to_string()[..8], i.previous_outpoint.index)).collect();
                let outs: Vec<String> = a.tx.outputs.iter().map(|o| format!("{}{}", o.value, if o.covenant.is_some() { "c" } else { "" })).collect();
                println!(
                    "claim {} daa {} round {} b {} claimed {:#04x} idx {} tier {} | in {:?} | out {:?}",
                    &a.tx.id().to_string()[..12], a.daa, c.ticket.round, c.ticket.b, c.ticket.claimed.first().copied().unwrap_or(0), c.idx, c.pool.tier, ins, outs
                );
            } else if matches!(name, "draw" | "void" | "close") {
                println!("{name} {} daa {}", &a.tx.id().to_string()[..12], a.daa);
            } else if let Some(Event::Buy(_)) = &ev {
                println!("buy {} daa {} outs {}", &a.tx.id().to_string()[..12], a.daa, a.tx.outputs.len());
            }
            let _ = State::Ticket;
        }
    }
}
