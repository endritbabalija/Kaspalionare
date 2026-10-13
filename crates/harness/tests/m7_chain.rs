//! M7 Stage 0: `harness::ChainSim` pins (spec A7, SP11) and the compiler-path smoke test for
//! `OpChainblockSeqCommit` + `blake3WithKey` through `execute_on_chain`.

use harness::chain::{block_fields_bytes, mergeset_context, seq_commit_with_parent};
use harness::*;
use kaspa_hashes::{HasherBase, SeqCommitMergesetContext, SeqCommitMerkleBranch, ZERO_HASH};
use kaspa_seq_commit::hashing as ksc;
use kaspa_seq_commit::types::{MergesetContext, SeqCommitInput, SeqState};

fn h(b: u8) -> Hash {
    let mut bytes = [0u8; 32];
    bytes[0] = b;
    Hash::from_bytes(bytes)
}

fn keyed(key: &[u8; 32], data: &[u8]) -> Hash {
    Hash::from_bytes(*blake3::Hasher::new_keyed(key).update(data).finalize().as_bytes())
}

// ---------------------------------------------------------------------------------------------
// SP11: key bytes and the commitment formula
// ---------------------------------------------------------------------------------------------

#[test]
fn sp11_k_seq_pins_seq_commit_merkle_branch() {
    for data in [&b""[..], b"abc", &[0x5au8; 64][..], &[0x01u8; 200][..]] {
        let mut hs = SeqCommitMerkleBranch::new();
        hs.update(data);
        assert_eq!(hs.finalize(), keyed(&K_SEQ, data), "K_SEQ over {} bytes", data.len());
    }
    // Golden vector of the pinned release: test_seq_commit_golden
    // (refs/rusty-kaspa-v2.1.0 consensus/seq-commit/src/hashing.rs) = H_seq(h(1) ‖ h(2)).
    let golden = Hash::from_bytes([
        0x7e, 0x9e, 0x79, 0x76, 0x99, 0x56, 0x7a, 0xb5, 0xcb, 0xcc, 0xc5, 0xa7, 0xe7, 0x20, 0xc6, 0x27, 0x09, 0x9d, 0xf8, 0x31, 0x86,
        0xe1, 0xd1, 0xe1, 0xca, 0xe8, 0x8d, 0x86, 0x46, 0xc0, 0x63, 0xdf,
    ]);
    let mut pre = h(1).as_bytes().to_vec();
    pre.extend_from_slice(&h(2).as_bytes());
    assert_eq!(keyed(&K_SEQ, &pre), golden);
}

#[test]
fn sp11_k_mc_pins_seq_commit_mergeset_context() {
    for data in [&b""[..], b"abc", &[0xa5u8; 24][..]] {
        let mut hm = SeqCommitMergesetContext::new();
        hm.update(data);
        assert_eq!(hm.finalize(), keyed(&K_MC, data), "K_MC over {} bytes", data.len());
    }
    let ctx = MergesetContext { timestamp: 1_700_000_000_123, daa_score: 1_000_600, blue_score: 987_654 };
    assert_eq!(ksc::mergeset_context_hash(&ctx), mergeset_context(ctx.timestamp, ctx.daa_score, ctx.blue_score));
}

/// The consensus composition of `consensus/seq-commit/src/hashing.rs:80-84, 153-173`.
fn consensus_seq_commit(parent: &Hash, b: &SimBlock) -> Hash {
    let ctx = ksc::mergeset_context_hash(&MergesetContext {
        timestamp: b.sp_timestamp,
        daa_score: b.daa_score,
        blue_score: b.blue_score,
    });
    let pcd = ksc::payload_and_context_digest(&ctx, &b.miner_payload_root);
    let state_root = ksc::seq_state_root(&SeqState { activity_root: &b.activity_root, payload_and_ctx_digest: &pcd });
    ksc::seq_commit(&SeqCommitInput { parent_seq_commit: parent, state_root: &state_root })
}

#[test]
fn sp11_seq_commit_equals_kaspa_seq_commit() {
    let fixed = [
        SimBlock {
            hash: h(1),
            parent: ZERO_HASH,
            blue_score: 0,
            daa_score: 0,
            sp_timestamp: 0,
            activity_root: ZERO_HASH,
            miner_payload_root: ZERO_HASH,
        },
        SimBlock {
            hash: h(2),
            parent: h(1),
            blue_score: 50_000,
            daa_score: 100_000,
            sp_timestamp: 1_700_000_000,
            activity_root: h(7),
            miner_payload_root: h(9),
        },
        SimBlock {
            hash: h(3),
            parent: h(2),
            blue_score: u64::MAX,
            daa_score: u64::MAX - 1,
            sp_timestamp: u64::MAX - 2,
            activity_root: Hash::from_bytes([0xff; 32]),
            miner_payload_root: Hash::from_bytes([0x80; 32]),
        },
    ];
    for parent in [ZERO_HASH, h(99), Hash::from_bytes([0x33; 32])] {
        for b in &fixed {
            assert_eq!(seq_commit_with_parent(&parent, b), consensus_seq_commit(&parent, b));
        }
    }
    // Every block of a sim, with its parent's commitment from the sim (ZERO_HASH at the root).
    let mut sim = ChainSim::line(1_000, 500, 5).with_jump(248).with_jump(0).with_extend(3);
    let side = sim.add_side_block(sim.chain_blocks()[2].hash, 1_003);
    let mut all = sim.chain_blocks();
    all.push(*sim.block(side).unwrap());
    for b in &all {
        let parent_commit = if b.parent == ZERO_HASH { ZERO_HASH } else { sim.commit_of(b.parent).unwrap() };
        assert_eq!(sim.commit_of(b.hash).unwrap(), consensus_seq_commit(&parent_commit, b));
        assert_eq!(sim.seq_commit(b), consensus_seq_commit(&parent_commit, b));
    }
}

#[test]
fn proof_bytes_layout() {
    let sim = ChainSim::line(10, 20, 4);
    let (x, p) = sim.anchor_for(12);
    let proof = sim.proof_bytes(&x, &p);
    assert_eq!(proof.len(), 304);
    assert_eq!(&proof[..152], &block_fields_bytes(&x, &sim.commit_of(p.hash).unwrap())[..]);
    assert_eq!(&proof[0..32], &x.hash.as_bytes());
    assert_eq!(&proof[32..64], &sim.commit_of(p.hash).unwrap().as_bytes());
    assert_eq!(&proof[64..96], &x.activity_root.as_bytes());
    assert_eq!(&proof[96..128], &x.miner_payload_root.as_bytes());
    assert_eq!(&proof[128..136], &x.sp_timestamp.to_le_bytes());
    assert_eq!(&proof[136..144], &12u64.to_le_bytes());
    assert_eq!(&proof[144..152], &22u64.to_le_bytes());
    assert_eq!(&proof[152..184], &p.hash.as_bytes());
    assert_eq!(&proof[288..296], &11u64.to_le_bytes());
}

// ---------------------------------------------------------------------------------------------
// Smoke contract: the script recomputes a block's commitment from its 152-byte fields and
// compares it with OpChainblockSeqCommit.
// ---------------------------------------------------------------------------------------------

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn probe() -> SilAbiArtifact {
    let src = format!(
        r#"pragma silverscript ^0.1.0;

// M7 Stage 0 probe (not protocol code): recompute SeqCommit(B) from B's 152-byte proof fields.
contract SeqCommitProbe() {{
    entry check(byte[] blk) {{
        require(blk.length == 152);
        byte[32] kSeq = byte[32](0x{kseq});
        byte[32] kMc = byte[32](0x{kmc});
        byte[32] ctx = blake3WithKey(blk.slice(128, 152), kMc);
        byte[32] pcd = blake3WithKey(byte[](ctx) + blk.slice(96, 128), kSeq);
        byte[32] sr = blake3WithKey(blk.slice(64, 96) + byte[](pcd), kSeq);
        byte[32] c = blake3WithKey(blk.slice(32, 64) + byte[](sr), kSeq);
        require(OpChainblockSeqCommit(byte[32](blk.slice(0, 32))) == c);
    }}
}}
"#,
        kseq = hex(&K_SEQ),
        kmc = hex(&K_MC)
    );
    compile_source(&src, &[])
}

fn probe_tx(art: &SilAbiArtifact, blk: &[u8], budget: u16) -> (Transaction, Vec<UtxoEntry>) {
    let sig = entry_sigscript(art, "check", &[ArtifactValue::Bytes(blk.to_vec())]);
    let t = tx(vec![input(0, sig, budget)], vec![TransactionOutput::new(9_000, p2sh(art))]);
    (t, vec![UtxoEntry::new(10_000, p2sh(art), 0, false, None)])
}

fn run(sim: &ChainSim, blk: &[u8]) -> Result<u64, TxScriptError> {
    let art = probe();
    let (t, e) = probe_tx(&art, blk, 0);
    execute_on_chain(&t, &e, 0, false, sim)
}

#[test]
fn smoke_probe_verifies_x_and_p_on_chain() {
    let sim = ChainSim::anchored(1_000_600, Hash::from_bytes([0x42; 32]));
    let (x, p) = sim.anchor_for(1_000_600);
    let art = probe();
    for b in [&x, &p] {
        let blk = sim.block_bytes(b);
        let units = run(&sim, &blk).expect("probe must accept a genuine chain block");
        // Same transaction under the committed budget, as consensus runs it.
        let (t, e) = probe_tx(&art, &blk, budget_for(units));
        let used = execute_on_chain(&t, &e, 0, true, &sim).expect("within committed budget");
        println!("probe: redeem {} bytes, {} script units, budget {}", bytecode(&art).len(), used, budget_for(units));
    }
    // A longer chain with a DAA jump: every chain block verifies.
    let sim = ChainSim::line(5_000, 7_000, 4).with_jump(248).with_extend(2);
    for b in sim.chain_blocks() {
        run(&sim, &sim.block_bytes(&b)).expect("every chain block verifies");
    }
}

#[test]
fn smoke_probe_fails_without_accessor() {
    let sim = ChainSim::anchored(1_000_600, Hash::from_bytes([0x42; 32]));
    let (x, _) = sim.anchor_for(1_000_600);
    let art = probe();
    let (t, e) = probe_tx(&art, &sim.block_bytes(&x), 0);
    let err = execute(&t, &e, 0, false).expect_err("no accessor: the opcode is disabled");
    assert!(matches!(err, TxScriptError::InvalidOpcode(_)), "{err:?}");
}

#[test]
fn smoke_probe_rejects_each_altered_field() {
    let sim = ChainSim::anchored(1_000_600, Hash::from_bytes([0x42; 32]));
    let (x, _) = sim.anchor_for(1_000_600);
    let good = sim.block_bytes(&x);
    // One bit in each of parent_commit, activity_root, miner_payload_root, ts, daa, bs.
    for off in [32usize, 64, 96, 128, 136, 144] {
        let mut bad = good;
        bad[off] ^= 1;
        let err = run(&sim, &bad).expect_err("altered preimage must fail");
        assert!(matches!(err, TxScriptError::EvalFalse | TxScriptError::VerifyError), "offset {off}: {err:?}");
    }
    // An altered hash names an unknown block.
    let mut bad = good;
    bad[0] ^= 1;
    assert!(matches!(run(&sim, &bad), Err(TxScriptError::BlockAlreadyPruned(_))));
}

#[test]
fn smoke_probe_accessor_failure_modes() {
    // Side block: known, not on the chain.
    let mut sim = ChainSim::line(100, 100, 3);
    let parent = sim.chain_blocks()[1].hash;
    let side = sim.add_side_block(parent, 102);
    let side_b = *sim.block(side).unwrap();
    let err = run(&sim, &sim.block_bytes(&side_b)).expect_err("side block");
    assert!(matches!(err, TxScriptError::BlockNotSelected(_)), "{err:?}");

    // Too deep: admitted at pov = bs + f - 1, refused at pov = bs + f.
    let mut sim = ChainSim::line(100, 1_000, 2);
    let tip = *sim.block(sim.tip()).unwrap();
    let blk = sim.block_bytes(&tip);
    let f = sim.finality_depth();
    sim.set_pov(tip.blue_score + f - 1);
    run(&sim, &blk).expect("admitted at bs + f - 1");
    sim.set_pov(tip.blue_score + f);
    let err = run(&sim, &blk).expect_err("too deep");
    assert!(matches!(err, TxScriptError::BlockIsTooDeep(_)), "{err:?}");

    // Reorg: the old tip is no longer selected.
    let mut sim = ChainSim::line(100, 100, 3);
    let old = *sim.block(sim.tip()).unwrap();
    let root = sim.chain_blocks()[0].hash;
    let new_tip = sim.fork(root, 3);
    run(&sim, &sim.block_bytes(&old)).expect("on chain before the reorg");
    sim.reorg_to(new_tip);
    let err = run(&sim, &sim.block_bytes(&old)).expect_err("after the reorg");
    assert!(matches!(err, TxScriptError::BlockNotSelected(_)), "{err:?}");
}
