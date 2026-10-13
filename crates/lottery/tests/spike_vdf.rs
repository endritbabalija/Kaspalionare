//! Spike VDF+ZK seed feasibility (spec `docs/specs/spike-vdf.md`, approved 2026-10-13). Local
//! engine only; not a milestone suite. Measurements:
//! `cargo test -p lottery --release --test spike_vdf -- --nocapture --test-threads=1`.
//!
//! Question: does seed = H(VDF(X.hash)), the VDF evaluation proven with `OpZkPrecompile`, fit
//! inside the worst M7 `draw()` under D104? The production `drawer.sil` is not changed: each
//! variant is a test-time source patch (the short-void mechanism, `buy::short_void_drawer_source`)
//! compiled in memory, and none of it is reachable from the library or a `mainnet` build.
//!
//! The VDF output and the proof travel inside the existing `anchor` argument, after the
//! 272-byte anchor proof (no new entry parameter, so no new live stack binding across check 24).
//! Variants (S1):
//! - V1: plain Groth16 (`g16.verify`) over a MinRoot circuit on BN254 Fr. Public inputs: the two
//!   16-byte halves of X.hash (zero-padded, so canonical), and the output (x_T, y_T). anchor =
//!   272 B anchor || x_T (32) || y_T (32) || proof (128). Seed source yS = blake2b(x_T || y_T);
//!   the seed is D94's blake2b("KLOT/seed" || cov || round || yS). The VK is a source constant.
//! - V2: RISC Zero receipt wrapped in Groth16 (`r0.g16.verify`). anchor = 272 || y (32) || proof
//!   (128). The production check would be journal_hash = sha256(X.hash || y); the only receipt
//!   available is SilverScript's fixture (its journal is not ours, RISC Zero proving needs Linux),
//!   so the script computes that hash (cost counted) but verifies against the fixture's journal
//!   hash: mass and stack only, binding not exercised (S1 item 2).
//! - V3: RISC Zero succinct (`r0.succinct.verify`), the rusty-kaspa fixture receipt, same
//!   binding caveat. anchor = 272 || y (32) || claim (32) || control index (4) || control
//!   digests (256) || seal (222,668).

mod common;

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use ark_bn254::{Bn254, Fr};
use ark_ff::{Field, PrimeField};
use ark_groth16::{Groth16, Proof, ProvingKey, VerifyingKey};
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, LinearCombination, SynthesisError, Variable};
use ark_relations::lc;
use ark_serialize::CanonicalSerialize;
use ark_snark::{CircuitSpecificSetupSNARK, SNARK};
use ark_std::rand::SeedableRng;
use ark_std::rand::rngs::StdRng;

use common::m2::{accept_draw, reject_at, tpl};
use common::measure::*;
use harness::{Masses, masses};
use lottery::buy::{Built, Final, KAS, Sources, Template, Templates, b_max};
use lottery::draw::{DrawCase, compile_drawer};
use lottery::game::{A_MAX, BP, M_DRAW, Shares, muldiv, seed_numbers, whale_allowance};
use lottery::params::blake2b;
use lottery::smt::Tree;

const K_FINAL: usize = 16;
/// D104: free stack slots required at the drawer's check 24.
const STACK_MIN_FREE: usize = 4;
/// MinRoot iterations of the circuit used in the drawer tests. The on-chain cost of a Groth16
/// proof does not depend on T (constant proof size, fixed public-input count).
const T_SPIKE: usize = 64;
const ANCHOR_LEN: usize = 272;

fn hex(s: &str) -> Vec<u8> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

macro_rules! fixture {
    ($f:literal) => {
        hex(include_str!(concat!("fixtures/spike_vdf/", $f)))
    };
}

// ------------------------------------------------------------------ MinRoot over BN254 Fr

/// d with 5 d = 1 mod (r - 1): x -> x^d is the inverse of x -> x^5 on Fr (gcd(5, r - 1) = 1,
/// asserted below). Computed as (k (r - 1) + 1) / 5 for the k in 1..=4 that makes it exact.
fn fifth_root_exp() -> [u64; 4] {
    let mut m = Fr::MODULUS.0;
    m[0] -= 1; // r is odd, no borrow
    for k in 1u128..5 {
        // t = k m + 1 as 5 limbs
        let mut t = [0u64; 5];
        let mut carry: u128 = 1;
        for i in 0..4 {
            let v = m[i] as u128 * k + carry;
            t[i] = v as u64;
            carry = v >> 64;
        }
        t[4] = carry as u64;
        // divide by 5 from the top
        let mut rem: u128 = 0;
        let mut q = [0u64; 5];
        for i in (0..5).rev() {
            let cur = (rem << 64) | t[i] as u128;
            q[i] = (cur / 5) as u64;
            rem = cur % 5;
        }
        if rem == 0 {
            assert_eq!(q[4], 0);
            return [q[0], q[1], q[2], q[3]];
        }
    }
    panic!("5 divides r - 1: x^5 is not a permutation of Fr");
}

fn d5() -> &'static [u64; 4] {
    static D: OnceLock<[u64; 4]> = OnceLock::new();
    D.get_or_init(fifth_root_exp)
}

/// The MinRoot input from X.hash: (lo, hi) = the two 16-byte halves read little-endian
/// (below 2^128, so the script's zero-padded 32-byte encodings are the canonical Fr bytes).
fn minroot_input(x_hash: &[u8; 32]) -> (Fr, Fr) {
    (Fr::from_le_bytes_mod_order(&x_hash[..16]), Fr::from_le_bytes_mod_order(&x_hash[16..]))
}

/// MinRoot (Khovratovich, Maller, Tiwari 2022): x' = (x + y)^(1/5), y' = x + i. Evaluation
/// takes a fifth root per step (sequential, ~254-bit exponentiation); the circuit checks x'^5.
fn minroot(x0: Fr, y0: Fr, t: usize) -> (Fr, Fr) {
    let (mut x, mut y) = (x0, y0);
    for i in 0..t {
        let nx = (x + y).pow(d5());
        let ny = x + Fr::from(i as u64);
        x = nx;
        y = ny;
    }
    (x, y)
}

#[derive(Clone)]
struct MinRootCircuit {
    t: usize,
    x0: Option<Fr>,
    y0: Option<Fr>,
}

impl ConstraintSynthesizer<Fr> for MinRootCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let miss = || SynthesisError::AssignmentMissing;
        // Witness trace (None in setup).
        let trace: Option<Vec<(Fr, Fr)>> = match (self.x0, self.y0) {
            (Some(x0), Some(y0)) => {
                let mut v = Vec::with_capacity(self.t + 1);
                let (mut x, mut y) = (x0, y0);
                v.push((x, y));
                for i in 0..self.t {
                    let nx = (x + y).pow(d5());
                    let ny = x + Fr::from(i as u64);
                    x = nx;
                    y = ny;
                    v.push((x, y));
                }
                Some(v)
            }
            _ => None,
        };
        let out = trace.as_ref().map(|v| v[self.t]);
        // Public inputs in order: x0, y0, x_T, y_T.
        let x0 = cs.new_input_variable(|| self.x0.ok_or_else(miss))?;
        let y0 = cs.new_input_variable(|| self.y0.ok_or_else(miss))?;
        let xt = cs.new_input_variable(|| out.map(|o| o.0).ok_or_else(miss))?;
        let yt = cs.new_input_variable(|| out.map(|o| o.1).ok_or_else(miss))?;
        let mut x: LinearCombination<Fr> = lc!() + x0;
        let mut y: LinearCombination<Fr> = lc!() + y0;
        for i in 0..self.t {
            let nxv = trace.as_ref().map(|v| v[i + 1].0);
            let nx = cs.new_witness_variable(|| nxv.ok_or_else(miss))?;
            let sq = cs.new_witness_variable(|| nxv.map(|a| a.square()).ok_or_else(miss))?;
            let q4 = cs.new_witness_variable(|| nxv.map(|a| a.square().square()).ok_or_else(miss))?;
            cs.enforce_r1cs_constraint(|| lc!() + nx, || lc!() + nx, || lc!() + sq)?;
            cs.enforce_r1cs_constraint(|| lc!() + sq, || lc!() + sq, || lc!() + q4)?;
            let sum = x.clone() + y.clone();
            cs.enforce_r1cs_constraint(|| lc!() + q4, || lc!() + nx, || sum.clone())?;
            let ny = x.clone() + (Fr::from(i as u64), Variable::One);
            x = lc!() + nx;
            y = ny;
        }
        let ex = x - xt;
        let ey = y - yt;
        cs.enforce_r1cs_constraint(|| ex.clone(), || lc!() + Variable::One, || lc!())?;
        cs.enforce_r1cs_constraint(|| ey.clone(), || lc!() + Variable::One, || lc!())?;
        Ok(())
    }
}

fn fr_bytes(f: &Fr) -> [u8; 32] {
    let mut v = Vec::with_capacity(32);
    f.serialize_uncompressed(&mut v).unwrap();
    v.try_into().unwrap()
}

fn setup(t: usize, seed: u64) -> (ProvingKey<Bn254>, VerifyingKey<Bn254>) {
    let mut rng = StdRng::seed_from_u64(seed);
    Groth16::<Bn254>::setup(MinRootCircuit { t, x0: None, y0: None }, &mut rng).expect("setup")
}

fn prove(pk: &ProvingKey<Bn254>, t: usize, x_hash: &[u8; 32]) -> Proof<Bn254> {
    let (x0, y0) = minroot_input(x_hash);
    let mut rng = StdRng::seed_from_u64(7);
    Groth16::<Bn254>::prove(pk, MinRootCircuit { t, x0: Some(x0), y0: Some(y0) }, &mut rng).expect("prove")
}

fn proof_bytes(p: &Proof<Bn254>) -> Vec<u8> {
    let mut v = vec![];
    p.serialize_compressed(&mut v).unwrap();
    v
}

/// The spike's V1 keys (local insecure setup: the "toxic waste" is the seeded rng).
struct V1Keys {
    pk: ProvingKey<Bn254>,
    vk: VerifyingKey<Bn254>,
    vk_bytes: Vec<u8>,
}

fn v1_keys() -> &'static V1Keys {
    static K: OnceLock<V1Keys> = OnceLock::new();
    K.get_or_init(|| {
        let (pk, vk) = setup(T_SPIKE, 1);
        let mut vk_bytes = vec![];
        vk.serialize_compressed(&mut vk_bytes).unwrap();
        V1Keys { pk, vk, vk_bytes }
    })
}

/// V1 VDF bytes for X.hash: x_T || y_T || proof (192 B), with the proof from `pk`.
fn v1_vdf_bytes_with(pk: &ProvingKey<Bn254>, x_hash: &[u8; 32]) -> Vec<u8> {
    let (x0, y0) = minroot_input(x_hash);
    let (xt, yt) = minroot(x0, y0, T_SPIKE);
    let mut v = fr_bytes(&xt).to_vec();
    v.extend_from_slice(&fr_bytes(&yt));
    v.extend(proof_bytes(&prove(pk, T_SPIKE, x_hash)));
    v
}

fn v1_vdf_bytes(x_hash: &[u8; 32]) -> Vec<u8> {
    v1_vdf_bytes_with(&v1_keys().pk, x_hash)
}

// ------------------------------------------------------------------ RISC Zero fixtures

/// SilverScript `tests/examples_tests.rs:161-172` (`r0_groth16_fixture`): journal hash, image id
/// and the 128-byte prepared proof of the vendored receipt.
fn r0_g16_fixture() -> ([u8; 32], [u8; 32], Vec<u8>) {
    let jh: [u8; 32] = hex("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456").try_into().unwrap();
    let image: [u8; 32] = hex("75641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0").try_into().unwrap();
    let receipt_bytes = fixture!("r0_groth16.rcpt.hex");
    let receipt: risc0_zkvm::Groth16Receipt<risc0_zkvm::ReceiptClaim> = borsh::from_slice(&receipt_bytes).expect("receipt");
    let proof = kaspa_txscript_zk_sdk::prepare_r0_groth16_proof(&receipt).expect("prepare");
    (jh, image, proof)
}

struct Succinct {
    claim: Vec<u8>,
    control_index: Vec<u8>,
    control_digests: Vec<u8>,
    seal: Vec<u8>,
    journal: Vec<u8>,
    image: Vec<u8>,
    control_id: Vec<u8>,
}

/// rusty-kaspa v2.1.0 `crypto/txscript/src/zk_precompiles/tests/data/succinct.*.hex`.
fn succinct_fixture() -> &'static Succinct {
    static S: OnceLock<Succinct> = OnceLock::new();
    S.get_or_init(|| {
        let s = Succinct {
            claim: fixture!("succinct.claim.hex"),
            control_index: fixture!("succinct.control_index.hex"),
            control_digests: fixture!("succinct.control_digests.hex"),
            seal: fixture!("succinct.seal.hex"),
            journal: fixture!("succinct.journal.hex"),
            image: fixture!("succinct.image.hex"),
            control_id: fixture!("succinct.control_id.hex"),
        };
        assert_eq!(fixture!("succinct.hashfn.hex"), vec![1u8], "Poseidon2");
        assert_eq!((s.claim.len(), s.control_index.len(), s.control_digests.len(), s.journal.len()), (32, 4, 256, 32));
        s
    })
}

/// The stand-in VDF output y of V2 and V3 (no RISC Zero prover here): any 32 bytes.
fn r0_y(x_hash: &[u8; 32]) -> [u8; 32] {
    let mut m = b"spike-r0-y".to_vec();
    m.extend_from_slice(x_hash);
    blake2b(&m)
}

// ------------------------------------------------------------------ drawer variants

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum V {
    V1,
    V2,
    V3,
}

const LEN_LINE: &str = "require(anchor.length == 0 || anchor.length == ANCHOR_LEN); // mut:M7A_len";
const ANCHOR_CONST: &str = "    int constant ANCHOR_LEN = 272;";
const SEED_LINE: &str = "            byte[32] h = blake2b(byte[](0x4b4c4f542f73656564) + cov + (round as byte[8]) + xHash);";
/// Marks the statement after the verify (the M7-f instrument's "used after" point for the
/// headroom at the verify call).
const AFTER_VERIFY: &str = "            // SPIKE: after verify";
/// Marks the start of the verify block (inside its braces): ints declared here are live only
/// inside the block, so the headroom measured between the two markers is at the verify.
const VERIFY_START: &str = "            // SPIKE: verify block start";

fn vdf_len(v: V) -> usize {
    match v {
        V::V1 => 64 + 128,
        V::V2 => 32 + 128,
        V::V3 => 32 + 32 + 4 + 256 + succinct_fixture().seal.len(),
    }
}

/// The verify block and the seed line of variant `v` (`seed_from_x`: mutant SM2, the seed
/// from X.hash again; `no_verify`: mutant SM1; `x_from_p`: mutant SM3, the X inputs taken
/// from P's parent_commit bytes instead of X.hash).
fn verify_block(v: V, no_verify: bool, x_from_p: bool) -> String {
    let xs = if x_from_p { 152 } else { 0 };
    let z16 = "byte[16](0x00000000000000000000000000000000)";
    let call = match v {
        V::V1 => format!(
            "                g16.verify(vdfVk, anchor.slice(336, 464), byte[32](anchor.slice({}, {}) + byte[]({z16})), byte[32](anchor.slice({}, {}) + byte[]({z16})), byte[32](anchor.slice(272, 304)), byte[32](anchor.slice(304, 336))); // mut:SV_verify\n",
            xs,
            xs + 16,
            xs + 16,
            xs + 32
        ),
        V::V2 => "                r0.g16.verify(r0Jh, anchor.slice(304, 432), r0Image); // mut:SV_verify\n".to_string(),
        V::V3 => "                r0.succinct.verify(byte[32](anchor.slice(304, 336)), byte[4](anchor.slice(336, 340)), anchor.slice(340, 596), anchor.slice(596, anchor.length), r0Journal, r0Image, r0Control); // mut:SV_verify\n".to_string(),
    };
    let call = if no_verify { String::new() } else { call };
    let pre = match v {
        V::V1 => {
            let vk = &v1_keys().vk_bytes;
            format!("                byte[] vdfVk = byte[](0x{});\n", to_hex(vk))
        }
        V::V2 => {
            let (jh, image, _) = r0_g16_fixture();
            format!(
                "                // production binding: journal_hash = sha256(X.hash || y); computed (cost) but the\n                // fixture's journal is verified (S1 item 2).\n                byte[32] jhOwn = sha256(byte[](xHash) + anchor.slice(272, 304));\n                require(jhOwn != byte[32](0x{}));\n                byte[32] r0Jh = byte[32](0x{});\n                byte[32] r0Image = byte[32](0x{});\n",
                "00".repeat(32),
                to_hex(&jh),
                to_hex(&image)
            )
        }
        V::V3 => {
            let s = succinct_fixture();
            format!(
                "                byte[32] jhOwn = sha256(byte[](xHash) + anchor.slice(272, 304));\n                require(jhOwn != byte[32](0x{}));\n                byte[32] r0Journal = byte[32](0x{});\n                byte[32] r0Image = byte[32](0x{});\n                byte[32] r0Control = byte[32](0x{});\n",
                "00".repeat(32),
                to_hex(&s.journal),
                to_hex(&s.image),
                to_hex(&s.control_id)
            )
        }
    };
    let ys = match v {
        V::V1 => "            byte[32] yS = blake2b(anchor.slice(272, 336));\n",
        V::V2 | V::V3 => "            byte[32] yS = byte[32](anchor.slice(272, 304));\n",
    };
    format!("            {{\n{VERIFY_START}\n{pre}{call}{AFTER_VERIFY}\n            }}\n{ys}")
}

#[derive(Clone, Copy, Default)]
struct Mutant {
    no_verify: bool,
    seed_from_x: bool,
    x_from_p: bool,
}

fn variant_drawer_source(src: &str, v: V, mu: Mutant) -> String {
    for (l, what) in [(LEN_LINE, "length check"), (ANCHOR_CONST, "ANCHOR_LEN"), (SEED_LINE, "seed line")] {
        assert_eq!(src.matches(l).count(), 1, "drawer source: the {what} must occur exactly once");
    }
    let seed = if mu.seed_from_x { SEED_LINE.to_string() } else { SEED_LINE.replace("+ xHash);", "+ yS); // mut:SV_seed") };
    src.replacen(ANCHOR_CONST, &format!("{ANCHOR_CONST}\n    // SPIKE VDF (test-only, docs/specs/spike-vdf.md): VDF output and proof after the anchor.\n    int constant VDF_LEN = {};", vdf_len(v)), 1)
        .replacen(LEN_LINE, "require(anchor.length == 0 || anchor.length == ANCHOR_LEN + VDF_LEN); // mut:M7A_len", 1)
        .replacen(SEED_LINE, &format!("{}{seed}", verify_block(v, mu.no_verify, mu.x_from_p)), 1)
}

fn committed_drawer() -> String {
    std::fs::read_to_string(harness::covenant_path("drawer.sil")).expect("drawer.sil").replace("\r\n", "\n")
}

fn templates(v: V, mu: Mutant) -> Arc<Templates> {
    let key = format!("spike {v:?} nv{} sx{} xp{}", mu.no_verify, mu.seed_from_x, mu.x_from_p);
    variant(&key, || {
        let mut s = Sources::committed(b_max());
        s.drawer = variant_drawer_source(&s.drawer.replace("\r\n", "\n"), v, mu);
        s
    })
    .unwrap_or_else(|e| panic!("{key}: compile failed: {e}"))
}

// ------------------------------------------------------------------ cases

/// The VDF bytes appended to the anchor for variant `v` and X.hash `x`.
fn vdf_bytes(v: V, x: &[u8; 32]) -> Vec<u8> {
    match v {
        V::V1 => v1_vdf_bytes(x),
        V::V2 => {
            let mut b = r0_y(x).to_vec();
            b.extend(r0_g16_fixture().2);
            b
        }
        V::V3 => {
            let s = succinct_fixture();
            let mut b = r0_y(x).to_vec();
            for p in [&s.claim, &s.control_index, &s.control_digests, &s.seal] {
                b.extend_from_slice(p);
            }
            b
        }
    }
}

/// yS, the 32-byte seed source the variant's script derives from its VDF bytes.
fn seed_src(v: V, vdf: &[u8]) -> [u8; 32] {
    match v {
        V::V1 => blake2b(&vdf[..64]),
        V::V2 | V::V3 => vdf[..32].try_into().unwrap(),
    }
}

/// Turn a valid M7 draw case into the variant's: the anchor carries the VDF bytes, the
/// winning numbers follow the seed from yS (`lie.rank` / `lie.bonus`, so leaves, siblings and
/// win5 follow), and, when `rehold`, every shard holds the new winning combination at every
/// bonus (DP21's holdings moved to the new rank).
fn with_vdf(mut c: DrawCase, v: V, vdf: Vec<u8>, rehold: bool) -> DrawCase {
    let anchor = c.anchor_bytes();
    assert_eq!(anchor.len(), ANCHOR_LEN);
    let ys = seed_src(v, &vdf);
    let sd = seed_numbers(&c.cov.as_bytes(), c.marker.round, &ys, c.marker.m);
    if rehold {
        let m = c.marker.m as usize;
        c.trees = (0..c.k())
            .map(|_| {
                let mut t = Tree::new(m);
                t.set(sd.rank, vec![1; m]);
                t
            })
            .collect();
        c.sync_roots();
    }
    c.lie.rank = Some(sd.rank);
    c.lie.bonus = Some(sd.bonus);
    let mut a = anchor;
    a.extend(vdf);
    c.lie.anchor = Some(a);
    c
}

fn worst_m7(t: &Templates) -> DrawCase {
    with_pending(dp21_with(t, K_FINAL, M_DRAW, A_MAX, F_MAX, 7), Pending::Cap)
}

/// m7_measure.rs `typical_draw` (K = 16, round 2, nobody holds the jackpot or a second prize,
/// a = 0, three lower-tier pools, one funding input).
fn typical_m7(t: &Templates) -> DrawCase {
    let mut c = DrawCase::at_round_with(K_FINAL, 2, t);
    for s in [10u64, 100, 1_000, 10_000, 100_000] {
        c.set_sales((s * KAS) as i64, (s * KAS / 20) as i64);
        let x = c.expected();
        if x.np() == 3 && x.h_j == 0 && x.n2 == 0 {
            return c;
        }
    }
    panic!("no typical draw with np = 3");
}

fn worst_void(t: &Templates) -> DrawCase {
    let mut c = dp21_with(t, K_FINAL, M_DRAW, A_MAX, F_MAX, 7);
    c.set_void(true);
    with_pending(c, Pending::Cap)
}

/// The worst M7 draw under variant `v`, built against the templates `t` the case runs on (the
/// controller's marker_tpl and the marker's drawer hash follow the drawer variant).
fn worst(v: V, t: &Templates) -> DrawCase {
    let c = worst_m7(t);
    let x = c.x_hash();
    let c = with_vdf(c, v, vdf_bytes(v, &x), true);
    let e = c.expected();
    assert_eq!((e.h_j, e.n2, e.np(), e.a), (K_FINAL as i64, M_DRAW - 1, 7, A_MAX), "worst shape kept");
    c
}

fn typical(v: V, t: &Templates) -> DrawCase {
    let c = typical_m7(t);
    let x = c.x_hash();
    with_vdf(c, v, vdf_bytes(v, &x), false)
}

fn build(c: &DrawCase, t: &Arc<Templates>) -> Built {
    c.build_with(t.clone())
}

fn run(c: &DrawCase, t: &Arc<Templates>) -> (Final, Masses) {
    let f = accept_draw(&build(c, t));
    let m = masses(&f.tx, &f.entries);
    (f, m)
}

// ------------------------------------------------------------------ SV1: V1 inside the worst draw

#[test]
fn sv1_v1_worst_draw_accepted_with_the_seed_from_the_vdf_output() {
    let t = templates(V::V1, Mutant::default());
    let c = worst(V::V1, &t);
    let (f, _) = run(&c, &t);
    // The script's check 8 accepted win5 = unrank(rank from yS): the seed came from y.
    let ys = seed_src(V::V1, &c.lie.anchor.as_ref().unwrap()[ANCHOR_LEN..]);
    let sd = seed_numbers(&c.cov.as_bytes(), c.marker.round, &ys, c.marker.m);
    let sx = seed_numbers(&c.cov.as_bytes(), c.marker.round, &c.x_hash(), c.marker.m);
    assert_ne!(sd.rank, sx.rank, "fixture: the y seed and the X.hash seed must differ for this test to mean anything");
    assert_eq!(f.tx.outputs.len(), c.expected().n_outputs());
    // The same case with the seed from X.hash (the M7 numbers) is refused by the variant.
    let mut bad = c.clone();
    bad.lie.rank = Some(sx.rank);
    bad.lie.bonus = Some(sx.bonus);
    reject_at(&honest_budget(&build(&c, &t), build(&bad, &t)), 0);
}

// ------------------------------------------------------------------ SV2: attacks on V1

fn v1_attack(mutate: impl FnOnce(&mut Vec<u8>)) -> (Built, Built) {
    let t = templates(V::V1, Mutant::default());
    let (good, bad) = v1_attack_cases(&t, mutate);
    (build(&good, &t), build(&bad, &t))
}

/// (honest typical V1 case, the same case with the VDF bytes mutated).
fn v1_attack_cases(t: &Templates, mutate: impl FnOnce(&mut Vec<u8>)) -> (DrawCase, DrawCase) {
    let base = typical_m7(t);
    let x = base.x_hash();
    let good = with_vdf(base.clone(), V::V1, v1_vdf_bytes(&x), false);
    let mut vdf = v1_vdf_bytes(&x);
    mutate(&mut vdf);
    // The winning numbers follow whatever y the caller claims (the strongest form of the lie).
    let bad = with_vdf(base, V::V1, vdf, false);
    (good, bad)
}

/// Which check must refuse an SV2 strike: the precompile (`ZkIntegrity`), or check 1's length
/// test (`VerifyError`) for a changed anchor length.
#[derive(Clone, Copy, PartialEq)]
enum Want {
    Zk,
    Len,
}

fn v1_strike(want: Want, mutate: impl FnOnce(&mut Vec<u8>)) {
    let (good, bad) = v1_attack(mutate);
    let e = reject_at(&honest_budget(&good, bad), 0);
    println!("SV2 rejection: {e:?}");
    match want {
        Want::Zk => assert!(matches!(e, harness::TxScriptError::ZkIntegrity(_)), "expected the precompile to refuse, got {e:?}"),
        Want::Len => assert!(matches!(e, harness::TxScriptError::VerifyError), "expected check 1's length test, got {e:?}"),
    }
    accept_draw(&good);
}

/// `bad` with input 0 committed to the honest draw's budget: a failing dry run would otherwise
/// get FAILED_RUN_UNITS (2,000,000 units, `buy.rs:42`), below the more than 15.25 M units the drawer
/// and the precompile use (the precompile alone: 14,000,000 + 5 x 250,000), and the attack would fail on the budget instead of the proof check.
fn honest_budget(good: &Built, mut bad: Built) -> Built {
    let u = good.finish().units[0];
    bad.shard_budget = Some(harness::budget_for(u));
    bad
}

#[test]
fn sv2a_wrong_y_with_a_valid_proof_is_rejected() {
    // Change x_T by one (still canonical): the proof no longer matches.
    v1_strike(Want::Zk, |v| {
        let xt = Fr::from_le_bytes_mod_order(&v[..32]) + Fr::from(1u64);
        v[..32].copy_from_slice(&fr_bytes(&xt));
    });
}

#[test]
fn sv2b_proof_and_output_for_another_x_hash_are_rejected() {
    v1_strike(Want::Zk, |v| {
        *v = v1_vdf_bytes(&[0x77; 32]);
    });
}

#[test]
fn sv2c_proof_from_another_setup_is_rejected() {
    // The caller runs its own setup (it knows that setup's trapdoor, so it could prove any y);
    // the VK is a template constant, so its proof does not verify.
    let (pk2, _) = setup(T_SPIKE, 99);
    v1_strike(Want::Zk, |v| {
        let x = typical_m7(&tpl()).x_hash();
        *v = v1_vdf_bytes_with(&pk2, &x);
    });
}

#[test]
fn sv2d_truncated_padded_or_garbage_proof_is_rejected() {
    v1_strike(Want::Len, |v| {
        v.pop();
    });
    v1_strike(Want::Len, |v| v.push(0));
    v1_strike(Want::Zk, |v| {
        let n = v.len();
        v[n - 1] ^= 0x01;
    });
}

#[test]
fn sv2e_non_canonical_output_encoding_is_rejected() {
    // x_T + r < 2^256 encodes the same field element with other bytes, so it would change yS
    // (the seed) without changing the proven value; the precompile's Fr parse must refuse it.
    let mut found = false;
    v1_strike(Want::Zk, |v| {
        let xt = &v[..32];
        let mut limbs = [0u64; 4];
        for i in 0..4 {
            limbs[i] = u64::from_le_bytes(xt[8 * i..8 * i + 8].try_into().unwrap());
        }
        let r = Fr::MODULUS.0;
        let mut carry = 0u128;
        let mut out = [0u64; 4];
        for i in 0..4 {
            let s = limbs[i] as u128 + r[i] as u128 + carry;
            out[i] = s as u64;
            carry = s >> 64;
        }
        assert_eq!(carry, 0, "x_T + r overflows 256 bits for this fixture");
        found = true;
        for i in 0..4 {
            v[8 * i..8 * i + 8].copy_from_slice(&out[i].to_le_bytes());
        }
    });
    assert!(found);
}

#[test]
fn sv2f_the_public_input_count_is_fixed_by_the_template() {
    // n is a compile-time constant of g16.verify (4 inputs) and the VK is a constant: the
    // caller controls neither. The precompile rejects a VK whose gamma_abc length is not n + 1
    // (`zk_precompiles/groth16/mod.rs:42-44`); recorded here as a property of the source.
    let src = variant_drawer_source(&committed_drawer(), V::V1, Mutant::default());
    assert_eq!(src.matches("g16.verify(").count(), 1);
    let call = src.lines().find(|l| l.contains("g16.verify(")).unwrap();
    assert_eq!(call.matches("byte[32](").count(), 4, "four public inputs");
    assert_eq!(v1_keys().vk.gamma_abc_g1.len(), 5);
}

// ------------------------------------------------------------------ SV3/SV4 + mutants

#[test]
fn sv4_void_needs_no_proof_under_every_variant() {
    for v in [V::V1, V::V2, V::V3] {
        let t = templates(v, Mutant::default());
        let c = worst_void(&t);
        assert!(c.anchor_bytes().is_empty());
        accept_draw(&build(&c, &t));
    }
}

#[test]
fn sv3_v2_executes_inside_the_worst_draw() {
    let t = templates(V::V2, Mutant::default());
    run(&worst(V::V2, &t), &t);
    run(&typical(V::V2, &t), &t);
}

#[test]
fn spike_mutants_are_killed() {
    // SM1 no verify: the wrong-y lie is accepted (SV2a). SM2 seed from X.hash: the honest V1
    // case is refused. SM3 X inputs from P's bytes: the honest proof is refused.
    let wrong_y = |v: &mut Vec<u8>| {
        let xt = Fr::from_le_bytes_mod_order(&v[..32]) + Fr::from(1u64);
        v[..32].copy_from_slice(&fr_bytes(&xt));
    };

    for (name, mu) in [
        ("SM1 no verify", Mutant { no_verify: true, ..Default::default() }),
        ("SM2 seed from X.hash", Mutant { seed_from_x: true, ..Default::default() }),
        ("SM3 X inputs from P", Mutant { x_from_p: true, ..Default::default() }),
    ] {
        let t = templates(V::V1, mu);
        let (honest, bad) = v1_attack_cases(&t, wrong_y);
        // Both under the budget of the honest draw on the unmutated variant.
        let t0 = templates(V::V1, Mutant::default());
        let b0 = build(&v1_attack_cases(&t0, wrong_y).0, &t0);
        let honest_r = honest_budget(&b0, build(&honest, &t)).finish().run_input(0);
        let lie_r = honest_budget(&b0, build(&bad, &t)).finish().run_input(0);
        println!("{name}: honest {honest_r:?}, wrong-y lie {lie_r:?}");
        // Killed iff the lie is accepted, or the honest case is refused by the script itself
        // (not by the budget: the budget is the unmutated variant's honest one).
        let killed = lie_r.is_ok() || matches!(&honest_r, Err(e) if common::m2::is_script_rejection(e));
        assert!(killed, "{name} survives");
    }
}

// ------------------------------------------------------------------ measurements

fn print_m(name: &str, m: &Masses) {
    let (pc, pt, ps) = pct(m);
    let ok = within(m, MARGIN_PCT).iter().all(|&b| b);
    println!("| {name} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {ok} |", m.compute, m.transient, m.storage);
}

/// The drawer with `n` extra live ints declared at `decl_at` and used at `used_at` (the
/// m7_measure.rs M7-f instrument with both points as parameters). Declared before check 1 they
/// stay live to the end of the entry; declared at VERIFY_START they die with the verify block.
fn with_live_ints(src: &str, n: usize, decl_at: &str, used_at: &str) -> String {
    let c1 = src.find(decl_at).expect("declaration marker");
    let at = src.find(used_at).expect("use marker");
    assert!(c1 < at);
    let decl: String = (0..n).map(|i| format!("        int xtraLive{i} = tx.inputs.length + {i};\n")).collect();
    let used = if n == 0 { String::new() } else { format!("        require({} >= 0);\n", (0..n).map(|i| format!("xtraLive{i}")).collect::<Vec<_>>().join(" + ")) };
    format!("{}{decl}{}{used}{}", &src[..c1], &src[c1..at], &src[at..])
}

fn try_drawer(src: &str) -> Result<Template, String> {
    let ctrl = tpl().controller.hash;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compile_drawer(src, ctrl)))
        .map_err(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "compile panicked".into()))
}

fn headroom(src: &str, decl_at: &str, used_at: &str) -> (usize, String) {
    try_drawer(src).expect("variant compiles");
    for n in 1..=244 {
        if let Err(e) = try_drawer(&with_live_ints(src, n, decl_at, used_at)) {
            assert!(e.contains("stack"), "the first failure is not the stack limit: {e}");
            return (n - 1, e);
        }
    }
    panic!("244 extra live ints compile");
}

/// S1 item 4 / S2: masses of the worst and typical draw() and the worst void under each
/// variant, the drawer size, and the stack headroom at check 24 and at the verify call.
#[test]
fn sv_measure_masses_and_stack() {
    let base_t = tpl();
    let (_, base_w) = run(&worst_m7(&base_t), &base_t);
    let (_, base_ty) = run(&typical_m7(&base_t), &base_t);
    let (_, base_v) = run(&worst_void(&base_t), &base_t);
    println!("\nSpike VDF: masses (limits 500,000 / 1,000,000 / 500,000; D104 90 %)");
    println!("| shape | compute | transient | storage | % c / t / s | <= 90 % |\n|---|---|---|---|---|---|");
    print_m("M7 worst draw (baseline)", &base_w);
    print_m("M7 typical draw (baseline)", &base_ty);
    print_m("M7 worst void (baseline)", &base_v);
    let redeem = |t: &Templates| t.drawer_redeem(&DrawCase::new(4).drawer).len();
    let mut verdict = vec![];
    for v in [V::V1, V::V2, V::V3] {
        let t = templates(v, Mutant::default());
        let mut fits = true;
        for (name, c) in [("worst draw", worst(v, &t)), ("typical draw", typical(v, &t))] {
            // V3 cannot be encoded: the signature script exceeds MAX_SIGNATURE_SCRIPT_LEN =
            // 250,000 B (rusty-kaspa v2.1.0 consensus/core/src/config/params.rs:25, all networks).
            let built = build(&c, &t);
            let f = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| built.finish())) {
                Ok(f) => f,
                Err(e) => {
                    let msg = e.downcast_ref::<String>().cloned().unwrap_or_default();
                    println!("| {v:?} {name}: NOT ENCODABLE ({msg}) | | | | | false |");
                    fits = false;
                    continue;
                }
            };
            let m = masses(&f.tx, &f.entries);
            let exec = f.run_input(0).map(|_| "accepted".to_string()).unwrap_or_else(|e| format!("input 0 refused: {e:?}"));
            print_m(&format!("{v:?} {name} ({exec}; sigscript {} B)", f.tx.inputs[0].signature_script.len()), &m);
            fits &= within(&m, MARGIN_PCT).iter().all(|&b| b) && exec == "accepted";
            if name == "worst draw" {
                println!(
                    "|  {v:?} worst draw minus baseline | {:+} | {:+} | {:+} | | |",
                    m.compute as i64 - base_w.compute as i64,
                    m.transient as i64 - base_w.transient as i64,
                    m.storage as i64 - base_w.storage as i64
                );
            }
        }
        let (_, mv) = run(&worst_void(&t), &t);
        print_m(&format!("{v:?} worst void"), &mv);
        fits &= within(&mv, MARGIN_PCT).iter().all(|&b| b);
        verdict.push((v, fits, redeem(&t)));
    }
    println!("\nDrawer redeem: M7 {} B; {}", redeem(&base_t), verdict.iter().map(|(v, _, r)| format!("{v:?} {r} B")).collect::<Vec<_>>().join(", "));

    println!("\nStack headroom (extra live int locals that still compile; D104 needs >= {STACK_MIN_FREE} at check 24):");
    let src = committed_drawer();
    const CHECK1: &str = "        // ==== check 1:";
    let (h24, _) = headroom(&src, CHECK1, "        // ==== check 25:");
    println!("| drawer | at check 24 | at the verify call |\n|---|---|---|\n| M7 | {h24} | - |");
    let mut stack_ok = vec![];
    for v in [V::V1, V::V2, V::V3] {
        let s = variant_drawer_source(&src, v, Mutant::default());
        let (a, _) = headroom(&s, CHECK1, "        // ==== check 25:");
        let (b, _) = headroom(&s, VERIFY_START, AFTER_VERIFY);
        println!("| {v:?} | {a} | {b} |");
        stack_ok.push((v, a >= STACK_MIN_FREE));
    }
    println!("\nVerdict per variant (all shapes <= 90 %, accepted, stack >= {STACK_MIN_FREE} at check 24):");
    for ((v, fits, _), (_, st)) in verdict.iter().zip(&stack_ok) {
        println!("- {v:?}: {}", if *fits && *st { "fits as is" } else { "does not fit as is" });
    }
}

// ------------------------------------------------------------------ Q1: prover time vs C_draw

/// S5 Q1: V1 prover and evaluator times on this machine at T = 2^10 .. 2^16, the per-iteration
/// slopes, and the round sales S at which C_draw = S x s_D / 10^4 covers the proving cost.
/// Cost inputs are parameters (machine price in KAS per hour), never from memory.
#[test]
fn sv_prover_time_and_c_draw() {
    const RUNS: usize = 5;
    println!("\nSpike VDF Q1: MinRoot / Groth16 (BN254, ark-groth16 0.6, this machine, {} threads; median of {RUNS} runs, [min, max])", std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1));
    println!("| T | constraints | eval (s) | setup (s) | prove (s) | proving key (MB) | prove / eval (medians) |\n|---|---|---|---|---|---|---|");
    let med = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    // (T, eval runs, prove runs, pk bytes)
    let mut rows: Vec<(f64, Vec<f64>, Vec<f64>, f64)> = vec![];
    for e in [10u32, 12, 14, 16] {
        let t = 1usize << e;
        let x = [0x58u8; 32];
        let (x0, y0) = minroot_input(&x);
        let t1 = Instant::now();
        let (pk, vk) = setup(t, 3);
        let st = t1.elapsed().as_secs_f64();
        let (mut ev, mut pr) = (vec![], vec![]);
        for _ in 0..RUNS {
            let t0 = Instant::now();
            let (xt, yt) = minroot(x0, y0, t);
            ev.push(t0.elapsed().as_secs_f64());
            let t2 = Instant::now();
            let p = prove(&pk, t, &x);
            pr.push(t2.elapsed().as_secs_f64());
            assert!(Groth16::<Bn254>::verify(&vk, &[x0, y0, xt, yt], &p).unwrap());
        }
        let mut pkb = vec![];
        pk.serialize_compressed(&mut pkb).unwrap();
        let (em, pm) = (med(&mut ev.clone()), med(&mut pr.clone()));
        let span = |v: &Vec<f64>| (v.iter().cloned().fold(f64::MAX, f64::min), v.iter().cloned().fold(0.0, f64::max));
        let ((e0, e1), (p0, p1)) = (span(&ev), span(&pr));
        println!("| 2^{e} | {} | {em:.3} [{e0:.3}, {e1:.3}] | {st:.2} | {pm:.2} [{p0:.2}, {p1:.2}] | {:.1} | {:.1} |", 3 * t + 2, pkb.len() as f64 / 1e6, pm / em);
        rows.push((t as f64, ev, pr, pkb.len() as f64));
    }
    // Per-iteration slopes between 2^14 and 2^16 from the medians, and the worst case (the
    // largest prove time and the smallest evaluation time at 2^16 against the medians at 2^14).
    let (a, b) = (&rows[2], &rows[3]);
    let dt = b.0 - a.0;
    let eval_it = (med(&mut b.1.clone()) - med(&mut a.1.clone())) / dt;
    let prove_it = (med(&mut b.2.clone()) - med(&mut a.2.clone())) / dt;
    let eval_fast = (b.1.iter().cloned().fold(f64::MAX, f64::min) - med(&mut a.1.clone())) / dt;
    let prove_slow = (b.2.iter().cloned().fold(0.0, f64::max) - med(&mut a.2.clone())) / dt;
    let pk_it = (b.3 - a.3) / dt;
    println!(
        "Slopes (medians): eval {:.2} us/iter, prove {:.2} us/iter (ratio {:.1}); pessimistic (fastest eval, slowest prove): ratio {:.1}; proving key {:.0} B/iter",
        eval_it * 1e6,
        prove_it * 1e6,
        prove_it / eval_it,
        prove_slow / eval_fast,
        pk_it
    );
    // The draw's own fee comes out of the same C_draw: S must cover fee + proving together.
    let t1 = templates(V::V1, Mutant::default());
    let (_, m) = run(&worst(V::V1, &t1), &t1);
    let fee = harness::priority_fee(&m);
    let s_d = Shares::TEST.s_d;
    let s_fee = fee as f64 * BP as f64 / s_d as f64 / KAS as f64;
    println!("V1 worst draw priority fee {:.5} KAS: C_draw covers the fee alone from S = {s_fee:.2} KAS", fee as f64 / KAS as f64);
    println!("\nExtrapolated (linear, far outside the measured range) to a delay `D_eval` of honest evaluation on this machine (T = D_eval / eval per iter):");
    println!("| D_eval | T | prove (h), median slope | prove (h), pessimistic ratio | eval + prove (h), pessimistic | proving key (GB) | S at which C_draw covers fee + proving at 1 KAS per machine-hour (KAS), median / pessimistic |\n|---|---|---|---|---|---|---|");
    for (label, secs) in [("10 s", 10.0f64), ("1 min", 60.0), ("10 min", 600.0), ("1 h", 3600.0)] {
        let t = secs / eval_it;
        let ph = t * prove_it / 3600.0;
        let ph_bad = secs * (prove_slow / eval_fast) / 3600.0;
        // C_draw = S s_D / BP >= fee + ph x price: S >= (fee + ph x price) BP / s_D.
        let s = s_fee + ph * BP as f64 / s_d as f64;
        let s_bad = s_fee + ph_bad * BP as f64 / s_d as f64;
        println!("| {label} | {t:.3e} | {ph:.2} | {ph_bad:.2} | {:.2} | {:.1} | {s:.0} / {s_bad:.0} |", secs / 3600.0 + ph_bad, t * pk_it / 1e9);
    }
    println!("(C_draw = floor(S x s_D / 10^4), s_D = {s_d} bp, game::round_amounts; the machine price is a parameter: the proving part of S scales with it, the fee part ({s_fee:.2} KAS) does not.)");
    let _ = muldiv; // the same muldiv as round_amounts, kept for the reader's cross-check
}

// ------------------------------------------------------------------ S3: margin floor

/// S3: the minimum ε (expected loss fraction of a player) the game-size rule allows where the
/// whale check binds, by margin floor, at the fixture shares (D100 amended (a): EV per
/// combination of J + G2 (m - 1) reaches q = p w / margin, the player's own lower-tier share
/// counted as fully returned, an upper bound on EV). Ordinary, self-referring, and also
/// taking both caller rewards.
#[test]
fn sv_margin_floor_table() {
    let sh = Shares::TEST;
    let p = 1_000_000_000i64; // 10 KAS in sompi: rounding of whale_allowance below 1e-9
    let w = sh.w();
    let st: i64 = sh.st_sum();
    println!("\nSpike VDF S3: minimum ε by margin floor (fixture shares s_F {} s_R {} s_C {} s_D {} s_t {} bp; w = {w} bp)", sh.s_f, sh.s_r, sh.s_c, sh.s_d, st);
    println!("| margin | q / p | ε ordinary | ε self-referring | ε self-referring + both caller rewards |\n|---|---|---|---|---|");
    let mut at = std::collections::HashMap::new();
    for margin in [10_000i64, 10_500, 11_000, 11_700, 12_500, 13_000] {
        let q = whale_allowance(p, w, margin) as f64 / p as f64;
        let ret = |extra: i64| 1.0 - q - (st + extra) as f64 / BP as f64;
        let e = (ret(0), ret(sh.s_r), ret(sh.s_r + sh.s_c + sh.s_d));
        println!("| {:.2} | {q:.4} | {:.4} | {:.4} | {:.4} |", margin as f64 / BP as f64, e.0, e.1, e.2);
        at.insert(margin, e);
    }
    // Cross-check against the values recorded in DECISIONS (D100 amended): margin 1.17 ->
    // 0.2248 / 0.1248 / 0.1148; margin 1.0 -> 0.110 / 0.010 / 0.000.
    let (a, b, c) = at[&11_700];
    assert!((a - 0.2248).abs() < 5e-4 && (b - 0.1248).abs() < 5e-4 && (c - 0.1148).abs() < 5e-4);
    let (a, b, c) = at[&10_000];
    assert!((a - 0.110).abs() < 5e-4 && (b - 0.010).abs() < 5e-4 && c.abs() < 5e-4);
    // The margin at which the self-referring caller's ε reaches the ordinary edge value 0.271.
    let need = |target: f64, extra: i64| (w as f64 / BP as f64) / (1.0 - target - (st + extra) as f64 / BP as f64);
    println!(
        "Margin floor for ε >= 0.271 (the edge-rule ordinary value): ordinary {:.3}, self-referring {:.3}, + caller rewards {:.3}",
        need(0.271, 0),
        need(0.271, sh.s_r),
        need(0.271, sh.s_r + sh.s_c + sh.s_d)
    );
}

// ------------------------------------------------------------------ V3 standalone (split step 1 floor)

/// S5 Q3: RISC Zero succinct measured once. It cannot enter draw() (signature script over
/// 250,000 B), so it is measured alone: one input whose script recomputes the journal binding
/// sha256(X.hash || y) and verifies the fixture receipt, one output. This is a lower bound on
/// step 1 of a split draw() (the real step would add the anchor proof and a covenant output).
/// V1 and V2 are measured the same way for comparison.
#[test]
fn sv3_standalone_verifier_masses() {
    use harness::{ArtifactValue, TransactionOutput, UtxoEntry, budget_for, bytecode, compile_source, entry_sigscript, execute, input, p2sh};
    let z = "00".repeat(32);
    let x = [0x58u8; 32];
    let y = r0_y(&x);
    let s = succinct_fixture();
    let (jh, image, g16proof) = r0_g16_fixture();
    let mut rows = vec![];
    for v in [V::V1, V::V2, V::V3] {
        let (body, args): (String, Vec<ArtifactValue>) = match v {
            V::V1 => {
                let vdf = v1_vdf_bytes(&x);
                (
                    format!(
                        "entry run(byte[32] xh, byte[] vdf) {{
 byte[] vk = byte[](0x{vk});
 g16.verify(vk, vdf.slice(64, 192), byte[32](xh.slice(0, 16) + byte[](byte[16](0x{z16}))), byte[32](xh.slice(16, 32) + byte[](byte[16](0x{z16}))), byte[32](vdf.slice(0, 32)), byte[32](vdf.slice(32, 64)));
 }}",
                        vk = to_hex(&v1_keys().vk_bytes),
                        z16 = "00".repeat(16)
                    ),
                    vec![ArtifactValue::Bytes(x.to_vec()), ArtifactValue::Bytes(vdf)],
                )
            }
            V::V2 => (
                format!(
                    "entry run(byte[32] xh, byte[32] y, byte[] proof) {{
 byte[32] j = sha256(byte[](xh) + byte[](y));
 require(j != byte[32](0x{z}));
 r0.g16.verify(byte[32](0x{}), proof, byte[32](0x{}));
 }}",
                    to_hex(&jh),
                    to_hex(&image)
                ),
                vec![ArtifactValue::Bytes(x.to_vec()), ArtifactValue::Bytes(y.to_vec()), ArtifactValue::Bytes(g16proof.clone())],
            ),
            V::V3 => (
                format!(
                    "entry run(byte[32] xh, byte[32] y, byte[32] claim, byte[4] ci, byte[] cd, byte[] seal) {{
 byte[32] j = sha256(byte[](xh) + byte[](y));
 require(j != byte[32](0x{z}));
 r0.succinct.verify(claim, ci, cd, seal, byte[32](0x{}), byte[32](0x{}), byte[32](0x{}));
 }}",
                    to_hex(&s.journal),
                    to_hex(&s.image),
                    to_hex(&s.control_id)
                ),
                vec![
                    ArtifactValue::Bytes(x.to_vec()),
                    ArtifactValue::Bytes(y.to_vec()),
                    ArtifactValue::Bytes(s.claim.clone()),
                    ArtifactValue::Bytes(s.control_index.clone()),
                    ArtifactValue::Bytes(s.control_digests.clone()),
                    ArtifactValue::Bytes(s.seal.clone()),
                ],
            ),
        };
        let src = format!("pragma silverscript ^0.1.0;
contract SpikeStep() {{
{body}
}}
");
        let art = compile_source(&src, &[]);
        let sig = entry_sigscript(&art, "run", &args);
        let mk = |budget: u16| {
            let t = harness::tx(vec![input(0, sig.clone(), budget)], vec![TransactionOutput::new(19 * KAS, p2sh(&art))]);
            (t, vec![UtxoEntry::new(20 * KAS, p2sh(&art), 0, false, None)])
        };
        let (t0, e0) = mk(0);
        let units = execute(&t0, &e0, 0, false).unwrap_or_else(|e| panic!("{v:?} standalone refused: {e:?}"));
        let (t, e) = mk(budget_for(units));
        execute(&t, &e, 0, true).expect("within the committed budget");
        let m = masses(&t, &e);
        rows.push((v, sig.len(), bytecode(&art).len(), units, m));
    }
    println!("
Spike VDF: verifier alone (1 input, 1 output; a floor for step 1 of a split draw())");
    println!("| variant | sigscript B | redeem B | script units | compute | transient | storage | % c / t / s | <= 90 % |
|---|---|---|---|---|---|---|---|---|");
    for (v, sl, rl, u, m) in &rows {
        let (pc, pt, ps) = pct(m);
        println!("| {v:?} | {sl} | {rl} | {u} | {} | {} | {} | {pc:.1} / {pt:.1} / {ps:.1} | {} |", m.compute, m.transient, m.storage, within(m, MARGIN_PCT).iter().all(|&b| b));
    }
}
