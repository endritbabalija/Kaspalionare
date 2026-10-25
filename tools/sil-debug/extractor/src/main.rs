//! sil-debug extractor (M8 S5 review tools; tools/sil-debug, not part of crates/). Reads
//! the campaign's index log and bundle, and for each requested txid writes:
//!   <out>/<label>.preflight.json  the transaction in the RPC/SDK JSON shape with each
//!                                 input's spent UTXO (amount, script, DAA, covenant ID)
//!   <out>/<label>.test.json       (with :sdb) a cli-debugger test file for input 0
//!   <out>/<label>.env             (with :sdb) per-input DAA scores and, for a draw, the
//!                                 anchor commitment, for the patched driver
//! Usage: silx <data-dir> <out-dir> <label>=<txid>[:sdb] ...
use std::collections::HashMap;
use std::fs;

use harness::ArtifactValue;
use kaspa_consensus_core::tx::{Transaction, TransactionOutpoint, TransactionOutput};
use lottery::buy::Templates;
use lottery::index::State;
use lottery::store::IndexLog;
use serde_json::{Value, json};
use silverscript_abi::TypeArtifact;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn pushes(script: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![];
    let mut i = 0;
    while i < script.len() {
        let op = script[i];
        i += 1;
        let n = match op {
            0x00 => 0,
            0x01..=0x4b => op as usize,
            0x4c => {
                let n = script[i] as usize;
                i += 1;
                n
            }
            0x4d => {
                let n = u16::from_le_bytes([script[i], script[i + 1]]) as usize;
                i += 2;
                n
            }
            0x4e => {
                let n = u32::from_le_bytes([script[i], script[i + 1], script[i + 2], script[i + 3]]) as usize;
                i += 4;
                n
            }
            0x51..=0x60 => {
                out.push(vec![op - 0x50]);
                continue;
            }
            _ => panic!("non-push opcode {op:#x}"),
        };
        out.push(script[i..i + n].to_vec());
        i += n;
    }
    out
}

fn script_num(b: &[u8]) -> i64 {
    let Some((&last, _)) = b.split_last() else { return 0 };
    let mut mag: u64 = 0;
    for (i, &x) in b.iter().enumerate() {
        let x = if i == b.len() - 1 { x & 0x7f } else { x };
        mag |= (x as u64) << (8 * i);
    }
    let v = mag as i64;
    if last & 0x80 != 0 { -v } else { v }
}

fn arg_str(v: &ArtifactValue) -> String {
    match v {
        ArtifactValue::Int(i) => i.to_string(),
        ArtifactValue::Bool(b) => b.to_string(),
        ArtifactValue::Byte(b) => format!("0x{b:02x}"),
        ArtifactValue::Bytes(b) => format!("0x{}", hex(b)),
        other => panic!("unsupported ctor value {other:?}"),
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = &a[1];
    let out = &a[2];
    fs::create_dir_all(out).unwrap();
    let t = Templates::compile(4);
    let log = IndexLog::from_jsonl(&fs::read_to_string(format!("{data}/index-log.jsonl")).unwrap()).unwrap();
    let bundle: Value = serde_json::from_str(&fs::read_to_string(format!("{data}/bundle.json")).unwrap()).unwrap();
    let gtx: Transaction = serde_json::from_value(bundle["genesis_tx"].clone()).unwrap();

    // Every output any logged instance transaction (or the genesis) created.
    let mut outs: HashMap<TransactionOutpoint, TransactionOutput> = HashMap::new();
    let mut accepted = HashMap::new();
    let mut add = |tx: &Transaction, outs: &mut HashMap<TransactionOutpoint, TransactionOutput>| {
        for (i, o) in tx.outputs.iter().enumerate() {
            outs.insert(TransactionOutpoint::new(tx.id(), i as u32), o.clone());
        }
    };
    add(&gtx, &mut outs);
    for l in &log.lines {
        let acc = l.accepted().unwrap();
        add(&acc.tx, &mut outs);
        accepted.insert(hex(&acc.tx.id().as_bytes()), acc);
    }

    for spec in &a[3..] {
        let (label, rest) = spec.split_once('=').unwrap();
        let (txid, sdb) = match rest.split_once(':') {
            Some((x, "sdb")) => (x, true),
            _ => (rest, false),
        };
        let acc = accepted.get(txid).unwrap_or_else(|| panic!("{txid} not in the index log"));
        let tx = &acc.tx;
        // Funding inputs are not instance outputs: use the zero-ID change output's script
        // (the runner pays its change to its own funding address; checked by the node's
        // signature verification when preflight runs the funding input).
        let change_spk = tx.outputs.iter().rev().find(|o| o.covenant.is_none()).map(|o| o.script_public_key.clone());
        let mut inputs = vec![];
        let mut sdb_inputs = vec![];
        let mut daas = vec![];
        for (i, inp) in tx.inputs.iter().enumerate() {
            let sp = acc.spent[i];
            let known = outs.get(&inp.previous_outpoint);
            let (spk, cov, src) = match known {
                Some(o) => (o.script_public_key.clone(), o.covenant.map(|c| c.covenant_id), "instance"),
                None => (change_spk.clone().expect("no change output"), None, "funding (spk assumed = change output)"),
            };
            daas.push(sp.daa);
            let mut utxo = json!({
                "amount": sp.amount,
                "scriptPublicKey": {"version": spk.version(), "script": hex(spk.script())},
                "blockDaaScore": sp.daa,
                "isCoinbase": false,
            });
            if let Some(c) = cov {
                utxo["covenantId"] = json!(hex(&c.as_bytes()));
            }
            let budget = inp.compute_commit.compute_budget();
            let mut j = json!({
                "previousOutpoint": {"transactionId": hex(&inp.previous_outpoint.transaction_id.as_bytes()), "index": inp.previous_outpoint.index},
                "signatureScript": hex(&inp.signature_script),
                "sequence": inp.sequence,
                "utxo": utxo,
            });
            match budget {
                Some(b) => j["computeBudget"] = json!(b),
                None => j["sigOpCount"] = json!(inp.compute_commit.sig_op_count().unwrap()),
            }
            inputs.push(j);
            eprintln!("{label} in {i}: {src} value {} daa {} cov {}", sp.amount, sp.daa, cov.map(|c| hex(&c.as_bytes())).unwrap_or_default());
            let mut s = json!({
                "prev_txid": format!("0x{}", hex(&inp.previous_outpoint.transaction_id.as_bytes())),
                "prev_index": inp.previous_outpoint.index,
                "sequence": inp.sequence,
                "utxo_value": sp.amount,
            });
            if let Some(c) = cov {
                s["covenant_id"] = json!(format!("0x{}", hex(&c.as_bytes())));
            }
            if i != 0 {
                s["utxo_script_hex"] = json!(format!("0x{}", hex(spk.script())));
                s["signature_script_hex"] = json!(format!("0x{}", hex(&inp.signature_script)));
            }
            sdb_inputs.push(s);
        }
        let mut outputs = vec![];
        let mut sdb_outputs = vec![];
        for o in &tx.outputs {
            let mut j = json!({"value": o.value, "scriptPublicKey": {"version": o.script_public_key.version(), "script": hex(o.script_public_key.script())}});
            let mut s = json!({"value": o.value, "script_hex": format!("0x{}", hex(o.script_public_key.script()))});
            if let Some(c) = o.covenant {
                j["covenant"] = json!({"authorizingInput": c.authorizing_input, "covenantId": hex(&c.covenant_id.as_bytes())});
                s["covenant_id"] = json!(format!("0x{}", hex(&c.covenant_id.as_bytes())));
                s["authorizing_input"] = json!(c.authorizing_input);
            }
            outputs.push(j);
            sdb_outputs.push(s);
        }
        let body = json!({
            "version": tx.version,
            "inputs": inputs,
            "outputs": outputs,
            "lockTime": tx.lock_time,
            "subnetworkId": hex(tx.subnetwork_id.as_ref()),
            "gas": tx.gas,
            "payload": hex(&tx.payload),
        });
        fs::write(format!("{out}/{label}.preflight.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();

        if sdb {
            // Input 0: decode its state and entry arguments.
            let p = pushes(&tx.inputs[0].signature_script);
            let redeem = p[p.len() - 1].clone();
            let tag = p[p.len() - 2].clone();
            let st = State::decode(&t, &redeem).unwrap();
            let (ctor, tpl, file) = match &st {
                State::Ticket(s) => (s.ctor(t.pool.hash), &t.ticket, "ticket.sil"),
                State::Drawer(s) => (s.ctor(t.controller.hash), &t.drawer, "drawer.sil"),
                other => panic!("unsupported role {:?}", other.role()),
            };
            assert_eq!(harness::bytecode(&tpl.art).len() > 0, true);
            let c = tpl.art.contracts.values().next().unwrap();
            let (name, entry) = c.entries.iter().find(|(_, e)| e.dispatch_tag.as_bytes().as_slice() == tag.as_slice()).unwrap();
            let raw = &p[..p.len() - 2];
            let mut args = vec![];
            for (param, b) in entry.params.iter().zip(raw) {
                args.push(match &param.ty {
                    TypeArtifact::Int | TypeArtifact::Temporal => script_num(b).to_string(),
                    TypeArtifact::Bool => (b.iter().any(|&x| x != 0)).to_string(),
                    TypeArtifact::Byte => format!("0x{:02x}", b[0]),
                    _ => format!("0x{}", hex(b)),
                });
            }
            let test = json!({"tests": [{
                "name": label,
                "function": name,
                "constructor_args": ctor.iter().map(arg_str).collect::<Vec<_>>(),
                "args": args,
                "expect": "pass",
                "tx": {
                    "version": tx.version,
                    "lock_time": tx.lock_time,
                    "active_input_index": 0,
                    "inputs": sdb_inputs,
                    "outputs": sdb_outputs,
                }
            }]});
            fs::write(format!("{out}/{label}.test.json"), serde_json::to_string_pretty(&test).unwrap()).unwrap();
            let mut env = format!("SDB_INPUT_DAA={}\nSDB_REDEEM={}\nSDB_CONTRACT={file}\n", daas.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(","), hex(&redeem));
            if name == "draw" {
                // anchor = first argument; xc as the drawer recomputes it (check 8).
                let an = &raw[0];
                if an.len() == 272 {
                    let k_seq: [u8; 32] = *b"SeqCommitmentMerkleBranchHash\0\0\0";
                    let k_mc: [u8; 32] = *b"SeqCommitMergesetContext\0\0\0\0\0\0\0\0";
                    let h = |k: &[u8; 32], d: &[u8]| *blake3::keyed_hash(k, d).as_bytes();
                    let ctx = h(&k_mc, &an[128..152]);
                    let pcd = h(&k_seq, &[&ctx[..], &an[96..128]].concat());
                    let sr = h(&k_seq, &[&an[64..96], &pcd[..]].concat());
                    let xc = h(&k_seq, &[&an[32..64], &sr[..]].concat());
                    env.push_str(&format!("SDB_SEQ_COMMIT={}={}\n", hex(&an[0..32]), hex(&xc)));
                }
            }
            fs::write(format!("{out}/{label}.env"), env).unwrap();
            eprintln!("{label}: entry {name}, {} args, ctor {} values, redeem {} bytes", args.len(), ctor.len(), redeem.len());
        }
        println!("{label} {txid} inputs {} outputs {} committed_storage_mass {}", tx.inputs.len(), tx.outputs.len(), tx.mass());
    }
    // The committed sources the templates were compiled from (to diff with covenants/).
    let src = lottery::buy::Sources::committed(4);
    fs::write(format!("{out}/committed-ticket.sil"), &src.ticket).unwrap();
    fs::write(format!("{out}/committed-drawer.sil"), &src.drawer).unwrap();
}
