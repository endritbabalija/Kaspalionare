//! M7 Stage 1b: the anchor fixtures (`ChainSim` chains around D) and the `draw()` attacks of
//! spec M7 A8 (SA1-SA9) that the spec model (`common::m5_spec`) covers, as data (`Atk5`), so
//! `m7_attacks.rs` strikes them, `m7_mutants.rs` re-runs them under the mutants, and
//! `M5_PREFLIGHT=1` / `M5_ATTRIB=1` cover them (the drawer labels of `m5_spec`: 1, 3, 5, 7,
//! 8, 9, 13, 14, 17, C2, 20-26).
//!
//! Each attack changes exactly one thing a cheating caller (or, for SA3, the chain) controls
//! against a draw or void that is otherwise accepted: one `Lie` field (a consistent false
//! claim, the builder follows it everywhere), one case field, or one thing in the `Built`.
//! The id names the attack and, in parentheses, the M7 mutant tag whose line rejects it
//! (Stage 2 places the tags; `docs/specs/7-handoff.md` Stage 1b has the line table). The
//! check is the `m5_spec` label of the first failing check (`drawer:<A4 check>`).
//!
//! SA3 (the chain's point of view and the opcode without an accessor), SA8's marker refresh,
//! the SA9 leg attacks (inputs 1-4, not modelled) and SA10 (`close()`, controller check 24)
//! are in `m7_attacks.rs` as direct strikes.

use std::sync::Arc;

use harness::{ArtifactValue, ChainSim, CovenantBinding, Hash, SimBlock, TransactionOutput};
use lottery::buy::{Built, KAS, spk_of};
use lottery::combo::{RANKS, unrank};
use lottery::draw::*;
use lottery::game::seed_numbers;

use super::m3::colex;
use super::m4::set_input_daa;
use super::m5::{Atk5, Case5};

pub const DRW: usize = DrawCase::DRAWER;
pub const MK: usize = DrawCase::MARKER;

// ---------------------------------------------------------------- chains around D

/// Blue score of the first block of the fixture chains (distinct from every DAA score, so a
/// mix-up of the two fields cannot pass by accident).
pub const B0: u64 = 7_000_000;

/// The attack chain around reference `d`: chain blocks at DAA d - 20 .. d + 30 (+1 DAA and +1
/// blue score each, blue from B0), so X (DAA d) has P = d - 1 as selected parent, X+ = d + 1,
/// X++ = d + 2, the tip = d + 30; a known side block N at DAA d whose selected parent is P;
/// point of view = the tip (every block in the window). `x_hash` places a chosen hash at X.
/// Deterministic: the same `d` gives the same bytes (N included).
pub fn line_sim_x(d: u64, x_hash: Option<Hash>) -> (ChainSim, SimBlock) {
    let mut s = ChainSim::line(d - 20, B0, 20);
    let p = s.tip();
    s.push_chain_with_hash(x_hash, 1, 1);
    s.extend(30);
    let n = s.add_side_block(p, d);
    let n = *s.block(n).unwrap();
    (s, n)
}

pub fn line_sim(d: u64) -> (ChainSim, SimBlock) {
    line_sim_x(d, None)
}

/// The chain block at DAA score `daa` (unique on the fixture lines).
pub fn at_daa(s: &ChainSim, daa: u64) -> SimBlock {
    s.chain_blocks().into_iter().find(|b| b.daa_score == daa).unwrap_or_else(|| panic!("no chain block at DAA {daa}"))
}

/// A chain with irregular DAA steps around `d` (0, 1, 2, 3 and 5 per block; blue +1 per
/// block): the SP1 chain. X is the first chain block with DAA >= d.
pub fn irregular_sim(d: u64) -> ChainSim {
    let mut s = ChainSim::line(d - 40, B0, 1);
    let steps = [1u64, 2, 0, 1, 3, 1, 5, 2];
    let mut i = 0;
    while s.block(s.tip()).unwrap().daa_score < d + 40 {
        s.push_chain(steps[i % steps.len()], 1);
        i += 1;
    }
    s
}

/// `c` running against `sim` (X.hash = the sim's anchor for D).
pub fn with_sim(mut c: DrawCase, sim: ChainSim) -> DrawCase {
    c.sim = Some(Arc::new(sim));
    c
}

/// The anchor block X of a case (its own chain, its true D).
pub fn anchor_x(c: &DrawCase) -> SimBlock {
    c.chain().anchor_for(c.d() as u64).0
}

// ---------------------------------------------------------------- bases

/// The m3_attacks base on the attack line (`line_sim`): round 2, K = 4, sales 157 KAS (5 KAS
/// referral), the jackpot held once (shard 1), one second-prize bonus held twice (shard 2,
/// n2 = 1), another rank held in shard 3, one absorbed round 1 pool, fee paid; np = 5. The
/// holdings follow the line's X.hash.
pub fn sa_base() -> DrawCase {
    let mut c = DrawCase::at_round(4, 2);
    let d = c.d() as u64;
    c = with_sim(c, line_sim(d).0);
    c.set_sales(157 * KAS as i64, 5 * KAS as i64);
    c.hold_jackpot(1, 1);
    let b = c.other_bonus(0);
    c.hold(2, b, 2);
    let other = (c.seed().rank + 1) % RANKS;
    c.hold_at(3, other, 1, 1);
    c.absorb(1);
    let x = c.expected();
    assert_eq!((x.h_j, x.n2, x.np(), x.a), (1, 1, 5, 1));
    assert!(x.fee.paid);
    assert_eq!(x.d, d as i64);
    assert_eq!(c.lock_time, d, "lock = D (the absorbed deadline is D - 100,000)");
    c
}

/// A void (spec M7 D96) on the attack line: round 2, K = 4, sales 157 KAS (so H > 0 and a draw
/// would create lower-tier pools), one absorbed pool, fee paid, lock = D + draw_window.
pub fn void_base() -> DrawCase {
    let mut c = DrawCase::at_round(4, 2);
    let d = c.d() as u64;
    c = with_sim(c, line_sim(d).0);
    c.set_sales(157 * KAS as i64, 5 * KAS as i64);
    c.absorb(1);
    let draw = c.expected();
    assert!(draw.pools.lp.iter().any(|&p| p), "a draw of this round creates lower-tier pools");
    c.set_void(true);
    let x = c.expected();
    assert!(x.void && x.np() == 0 && x.amounts.h > 0 && x.fee.paid);
    assert_eq!(c.lock_time, d + c.drawer.draw_window as u64);
    c
}

/// A line chain with anchors for both 599 (= -1 + SEED_OFFSET, the reference an unaccepted
/// marker would give) and `d`: blocks at DAA 590..601, one block jumping to d - 1, X at d,
/// then 10 more. Only for SA5's unaccepted marker (the jump is far above MERGESET_MAX; the
/// sim does not enforce it).
pub fn low_and_d_sim(d: u64) -> ChainSim {
    let mut s = ChainSim::line(590, B0, 12);
    let tip = s.block(s.tip()).unwrap().daa_score;
    s.push_chain(d - 1 - tip, 1);
    s.push_chain(1, 1);
    s.extend(10);
    s
}

// ---------------------------------------------------------------- edits

/// The anchor argument (draw arguments) of a built draw.
pub fn anchor_arg(b: &mut Built) -> &mut Vec<u8> {
    match &mut b.entry_args_mut(DRW)[ARG_ANCHOR] {
        ArtifactValue::Bytes(a) => a,
        a => panic!("anchor: {a:?}"),
    }
}

fn bytes_arg(b: &mut Built, i: usize) -> &mut Vec<u8> {
    match &mut b.entry_args_mut(DRW)[i] {
        ArtifactValue::Bytes(a) => a,
        a => panic!("argument {i}: {a:?}"),
    }
}

/// Flip bit 0 of byte `off` of the anchor argument. Offsets >= 32 never touch X.hash, so the
/// seed (and every output) is unchanged: only the preimage is wrong.
fn flip(off: usize) -> impl Fn(&mut Built, &Case5) + 'static {
    move |b, _| anchor_arg(b)[off] ^= 1
}

/// The proof bytes of (x, p) on the case's chain.
fn proof(c: &DrawCase, x: &SimBlock, p: &SimBlock) -> Vec<u8> {
    c.chain().proof_bytes(x, p)
}

/// Field offsets inside X's 152 bytes of the anchor (spec M7 A3). P's fields follow at 152
/// without P.hash (M7 Stage 5 fix B), so P's field at X-offset `o` is at 152 + o - 32.
pub const FIELDS: [(&str, usize); 6] =
    [("parent_commit", 32), ("activity_root", 64), ("miner_payload_root", 96), ("sp_timestamp", 128), ("daa_score", 136), ("blue_score", 144)];

/// Offset of P's preimage fields in the anchor (after X's 152 bytes; P.hash is not in it).
pub const P_AT: usize = harness::BLOCK_PROOF_LEN;

// ---------------------------------------------------------------- SA1

/// SA1 (check 8: dP, dX): a neighbouring chain block presented as the anchor, each with its
/// genuine selected parent and genuine preimages (the opcode accepts both blocks; pc ==
/// x_pc holds), the seed following the claimed X.hash.
pub fn sa1() -> Vec<Atk5> {
    // (claimed X, claimed P) as DAA offsets from D on the attack line.
    let atk = |id: &str, xo: i64, po: i64| {
        Atk5::new(id, DRW, "drawer:8", sa_base()).dr(move |c| {
            let s = c.chain();
            let (x, p) = (at_daa(&s, (c.d() + xo) as u64), at_daa(&s, (c.d() + po) as u64));
            c.lie.anchor = Some(s.proof_bytes(&x, &p));
        })
    };
    vec![
        atk("SA1 X+ (X's chain child) with X as P: dP = daa(X) = D (M7A_dP)", 1, 0),
        atk("SA1 X++ with X+ as P: dP = D + 1 (M7A_dP)", 2, 1),
        atk("SA1 P as the anchor with its parent: dX = D - 1 < D (M7A_dX)", -1, -2),
        atk("SA1 a chain block far after D (the tip, D + 30) with its parent (M7A_dP)", 30, 29),
    ]
}

// ---------------------------------------------------------------- SA2

/// SA2 (check 8: the opcode, pc == x_pc): a block that is not the selected chain's anchor.
///
/// M7 Stage 5 fix B removed the strike "an unknown P.hash with P's genuine preimage:
/// BlockAlreadyPruned (M7A_pc)": the anchor no longer carries P.hash and the drawer makes no
/// opcode call on it (P is authenticated by `pc == x_pc` alone; a wrong P preimage is SA4,
/// a P that is not X's selected parent is the M7A_pceq strike below).
pub fn sa2() -> Vec<Atk5> {
    vec![
        Atk5::new("SA2 a known side block N at DAA D whose selected parent is P: BlockNotSelected (M7A_xc)", DRW, "drawer:8", sa_base()).dr(|c| {
            let (_, n) = line_sim(c.d() as u64);
            let p = at_daa(&c.chain(), c.d() as u64 - 1);
            c.lie.anchor = Some(proof(c, &n, &p));
        }),
        Atk5::new("SA2 X+ with P as its claimed parent (P is on the chain, not X+'s selected parent): pc != x_pc (M7A_pceq)", DRW, "drawer:8", sa_base()).dr(|c| {
            let s = c.chain();
            let (x, p) = (at_daa(&s, c.d() as u64 + 1), at_daa(&s, c.d() as u64 - 1));
            c.lie.anchor = Some(proof(c, &x, &p));
        }),
        Atk5::new("SA2 an unknown X.hash with X's genuine preimage: BlockAlreadyPruned; the seed follows the claimed hash (M7A_xc)", DRW, "drawer:8", sa_base()).dr(|c| {
            let mut a = c.anchor_bytes();
            a[..32].copy_from_slice(&[0xee; 32]);
            c.lie.anchor = Some(a);
        }),
    ]
}

// ---------------------------------------------------------------- SA4

/// SA4 (checks 1 and 8: xc, pceq, dX, dP): a wrong preimage: each field of X and of P altered
/// by one bit (X: the opcode's commitment differs, M7A_xc; P: its recomputed commitment differs
/// from X's parent_commit, M7A_pceq; since M7 Stage 5 fix B there is no opcode call on P), X's
/// DAA forged to D for the block whose real DAA is D - 1, X and P swapped, the anchor 271 and
/// 273 bytes long (ANCHOR_LEN = 272).
pub fn sa4() -> Vec<Atk5> {
    let mut v = vec![];
    for (name, off) in FIELDS {
        v.push(Atk5::new(format!("SA4 X.{name} altered by one bit (M7A_xc)"), DRW, "drawer:8", sa_base()).built(flip(off)));
    }
    for (name, off) in FIELDS {
        v.push(Atk5::new(format!("SA4 P.{name} altered by one bit (M7A_pceq)"), DRW, "drawer:8", sa_base()).built(flip(P_AT + off - 32)));
    }
    v.push(Atk5::new("SA4 P (real DAA D - 1) as X with its DAA field forged to D, its parent as P (M7A_xc)", DRW, "drawer:8", sa_base()).dr(|c| {
        let s = c.chain();
        let (x, p) = (at_daa(&s, c.d() as u64 - 1), at_daa(&s, c.d() as u64 - 2));
        let mut a = proof(c, &x, &p);
        a[136..144].copy_from_slice(&(c.d() as u64).to_le_bytes());
        c.lie.anchor = Some(a);
    }));
    // In the 272-byte layout the swap puts P (with its hash, as the claimed anchor) first and
    // X's preimage (without X.hash) in P's place: the opcode accepts P's own commitment, the
    // recomputed "parent" commitment is X's, not P's parent's, and the seed follows P.hash.
    v.push(Atk5::new("SA4 X and P swapped (P with its hash first, X's preimage as P): pc != x_pc (M7A_pceq)", DRW, "drawer:8", sa_base()).dr(|c| {
        let s = c.chain();
        let (x, p) = s.anchor_for(c.d() as u64);
        c.lie.anchor = Some([&s.block_bytes(&p)[..], &s.block_bytes(&x)[32..]].concat());
    }));
    v.push(Atk5::new("SA4 anchor length 271 (the last byte dropped) (M7A_len)", DRW, "drawer:1", sa_base()).dr(|c| {
        let mut a = c.anchor_bytes();
        a.pop();
        c.lie.anchor = Some(a);
    }));
    v.push(Atk5::new("SA4 anchor length 273 (a zero byte appended) (M7A_len)", DRW, "drawer:1", sa_base()).dr(|c| {
        let mut a = c.anchor_bytes();
        a.push(0);
        c.lie.anchor = Some(a);
    }));
    v
}

// ---------------------------------------------------------------- SA5

/// SA5 (check 7): a caller-supplied reference.
pub fn sa5() -> Vec<Atk5> {
    let mut v = vec![];
    for (dd, name) in [(1i64, "+ 1"), (-1, "- 1")] {
        v.push(
            Atk5::new(format!("SA5 mk_daa argument {name}, every output from the true D (M7D_mk)"), DRW, "drawer:7", sa_base())
                .built(move |b, c| b.entry_args_mut(DRW)[ARG_MK_DAA] = ArtifactValue::Int(c.draw_case().unwrap().marker_daa as i64 + dd)),
        );
    }
    v.push(Atk5::new("SA5 mk_daa - 1 with the anchor for D - 1 and every output following it (a re-roll by reference) (M7D_mk)", DRW, "drawer:7", sa_base()).dr(|c| {
        let d1 = c.d() as u64 - 1;
        c.lie.anchor = Some(c.chain().anchor_proof(d1));
        c.lie.mk_daa = Some(c.marker_daa as i64 - 1);
    }));
    v.push(Atk5::new("SA5 lock_time = D - 1 (M7D_lock)", DRW, "drawer:7", sa_base()).dr(|c| c.lock_time -= 1));
    // The unaccepted marker: OpTxInputDaaScore(1) = -1 (UNACCEPTED_DAA_SCORE = u64::MAX,
    // consensus/core/src/constants.rs:51), mk_daa = -1 and the anchor for -1 + 600 = 599 on a
    // chain that has one, every output following D' = 599: only `mkDaa >= 0` rejects it.
    let mut good = sa_base();
    good.sim = Some(Arc::new(low_and_d_sim(good.d() as u64)));
    let x = anchor_x(&good);
    assert_eq!(x.daa_score, good.d() as u64);
    good = rehold(good);
    v.push(
        Atk5::new("SA5 a draw chained on an unaccepted marker (creation -1), mk_daa = -1 and the anchor for 599 (range mkDaa >= 0)", DRW, "drawer:7", good)
            .dr(|c| {
                c.lie.mk_daa = Some(-1);
                c.lie.anchor = Some(c.chain().anchor_proof(599));
            })
            .built(|b, _| set_input_daa(b, MK, u64::MAX)),
    );
    v
}

/// `sa_base()`'s holdings rebuilt for the case's current seed (after its chain changed).
pub fn rehold(mut c: DrawCase) -> DrawCase {
    let m = c.marker.m as usize;
    c.trees = (0..c.k()).map(|_| lottery::smt::Tree::new(m)).collect();
    c.sync_roots();
    c.hold_jackpot(1, 1);
    let b = c.other_bonus(0);
    c.hold(2, b, 2);
    let other = (c.seed().rank + 1) % RANKS;
    c.hold_at(3, other, 1, 1);
    let x = c.expected();
    assert_eq!((x.h_j, x.n2, x.np()), (1, 1, 5));
    c
}

// ---------------------------------------------------------------- SA6

/// A round 1 draw on the attack line whose X.hash gives a rank with a same-rank
/// non-canonical win5 (n1 + C(n2 - 1, 2), 1, n3, n4, n5); the bad win5.
pub fn noncanonical_base() -> (DrawCase, [u8; 5]) {
    for i in 0u32..10_000 {
        let mut h = [0x58u8; 32];
        h[..4].copy_from_slice(&i.to_le_bytes());
        let mut c = DrawCase::new(4);
        let d = c.d() as u64;
        c = with_sim(c, line_sim_x(d, Some(Hash::from_bytes(h))).0);
        c.set_sales(100 * KAS as i64, 5 * KAS as i64);
        let w = unrank(c.seed().rank);
        let y2 = w[1] as u32 - 1;
        let n1 = w[0] as u32 + y2 * (y2.saturating_sub(1)) / 2;
        if y2 >= 2 && n1 <= 255 {
            let bad = [n1 as u8, 1, w[2], w[3], w[4]];
            assert_eq!(colex(&bad), c.seed().rank as i64);
            return (c, bad);
        }
    }
    panic!("no X.hash with a same-rank non-canonical win5");
}

/// SA6 (checks 8, 9; as M3 DA12): the seed.
pub fn sa6() -> Vec<Atk5> {
    let (nc, bad) = noncanonical_base();
    vec![
        Atk5::new("SA6 win5 of another rank (the seed rank's proofs)", DRW, "drawer:8", sa_base()).dr(|c| c.lie.win5 = Some(unrank((c.seed().rank + 1) % RANKS))),
        Atk5::new("SA6 win5 and the proofs of another rank (rank + 1), counts and pools following it", DRW, "drawer:8", sa_base()).dr(|c| c.lie.rank = Some((c.seed().rank + 1) % RANKS)),
        Atk5::new("SA6 a non-canonical win5 with the seed's colex rank", DRW, "drawer:8", nc).dr(move |c| c.lie.win5 = Some(bad)),
        Atk5::new("SA6 bonus not from h (the held second-prize bonus), counts and pools following it", DRW, "drawer:25", sa_base()).dr(|c| c.lie.bonus = Some(c.other_bonus(0))),
        Atk5::new("SA6 leaves and siblings of another rank (rank + 1) with the seed's win5", DRW, "drawer:9", sa_base()).dr(|c| {
            let r = c.seed().rank;
            c.lie.rank = Some((r + 1) % RANKS);
            c.lie.win5 = Some(unrank(r));
        }),
    ]
}

// ---------------------------------------------------------------- SA7

/// SA7 (checks 7, 8, 12, 14, 15, 20, 23): void misuse. (A 272-byte anchor with lock_time >= D +
/// 2F is a draw, not a void: SP2's last point and SA3.)
pub fn sa7() -> Vec<Atk5> {
    vec![
        Atk5::new("SA7 void at lock_time = D + draw_window - 1 (M7V_lock)", DRW, "drawer:7", void_base()).dr(|c| c.lock_time -= 1),
        Atk5::new("SA7 void with win5 = the X.hash seed's main numbers (M7V_win5)", DRW, "drawer:8", void_base()).dr(|c| {
            let s = seed_numbers(&c.cov.as_bytes(), c.marker.round, &c.x_hash, c.marker.m);
            c.lie.win5 = Some(s.main);
        }),
        Atk5::new("SA7 void with non-empty leaves (the seed rank's count leaves) (M7V_leaves)", DRW, "drawer:8", void_base()).built(|b, c| {
            let c = c.draw_case().unwrap();
            let rank = seed_numbers(&c.cov.as_bytes(), c.marker.round, &c.x_hash, c.marker.m).rank;
            *bytes_arg(b, ARG_LEAVES) = lottery::smt::proofs_at(&c.trees, rank).0;
        }),
        Atk5::new("SA7 void with non-empty siblings (the seed rank's paths) (M7V_leaves)", DRW, "drawer:8", void_base()).built(|b, c| {
            let c = c.draw_case().unwrap();
            let rank = seed_numbers(&c.cov.as_bytes(), c.marker.round, &c.x_hash, c.marker.m).rank;
            *bytes_arg(b, ARG_SIBLINGS) = lottery::smt::proofs_at(&c.trees, rank).1;
        }),
        Atk5::new("SA7 void creating the lower-tier pools a draw would (their budgets out of O') (M7V_lp)", DRW, "drawer:13", void_base()).dr(|c| c.lie.void_pools = Some((0, 0))),
        Atk5::new("SA7 void paying H to the vault (V' = V + H, O' without H) (M7V_H)", DRW, "drawer:20", void_base()).dr(|c| {
            let h = c.expected().amounts.h;
            c.lie.v_delta = h;
            c.lie.o_delta = -h;
        }),
        Atk5::new("SA7 void paying J_house (a jackpot pool out of the vault, h_J = 1)", DRW, "drawer:13", void_base()).dr(|c| c.lie.void_pools = Some((1, 0))),
        Atk5::new("SA7 void paying n2 G2 (a second-prize pool out of the vault, n2 = 1)", DRW, "drawer:13", void_base()).dr(|c| c.lie.void_pools = Some((0, 1))),
        Atk5::new("SA7 void with J_boost to the vault instead of the boost pot", DRW, "drawer:20", void_base()).dr(|c| {
            let jb = c.marker.j_boost;
            c.lie.v_delta = jb;
            c.lie.o_delta = -jb;
        }),
    ]
}

// ---------------------------------------------------------------- SA8

/// SA8 (check 3): re-roll attempts: a second draw or void of a round already drawn or voided
/// (the round r + 1 controller has drawn = 1). The marker refresh is SP10 / `m7_attacks.rs`.
pub fn sa8() -> Vec<Atk5> {
    vec![
        Atk5::new("SA8 a draw after a draw or a void (controller drawn = 1) (D3_drawn)", DRW, "drawer:3", sa_base()).dr(|c| c.controller.drawn = 1),
        Atk5::new("SA8 a void after a draw or a void (controller drawn = 1) (D3_drawn)", DRW, "drawer:3", void_base()).dr(|c| c.controller.drawn = 1),
    ]
}

// ---------------------------------------------------------------- SA9 (modelled part)

/// Every new pool output rewritten with `edit` (the transaction otherwise as built).
fn pools_out(edit: fn(&mut lottery::state::PoolState, &DrawExpected)) -> impl Fn(&mut Built, &Case5) + 'static {
    move |b, c| {
        let x = c.draw_case().unwrap().expected();
        for p in 0..x.np() {
            let mut s = x.new_pools[p].clone();
            edit(&mut s, &x);
            b.outputs[x.pool_output(p)].script_public_key = b.templates.pool_spk(&s);
        }
    }
}

/// SA9 (as M3 DA8, DA13, DA14, DA16) on the anchor line, the attacks the model covers; the
/// leg attacks are in `m7_attacks.rs`.
pub fn sa9() -> Vec<Atk5> {
    vec![
        Atk5::new("SA9 DA8 an extra plain output", DRW, "drawer:20", sa_base()).built(|b, _| {
            let last = b.outputs.len() - 1;
            b.outputs[last].value -= KAS;
            b.outputs.push(TransactionOutput::new(KAS, spk_of(&[9u8; 32])));
        }),
        Atk5::new("SA9 DA8 an extra covenant output (a copy of the last pool)", DRW, "drawer:13", sa_base()).built(|b, c| {
            let x = c.draw_case().unwrap().expected();
            let first_plain = 3 + x.np();
            let o = b.outputs[first_plain - 1].clone();
            let last = b.outputs.len() - 1;
            b.outputs[last].value -= o.value;
            b.outputs.insert(first_plain, o);
        }),
        Atk5::new("SA9 DA8 a pool output authorized by the marker (parallel auth group)", DRW, "drawer:13", sa_base())
            .built(|b, c| b.outputs[3].covenant = Some(CovenantBinding::new(MK as u16, c.draw_case().unwrap().cov))),
        Atk5::new("SA9 DA13 every new pool's deadline D + claim_window, without the draw window (M7C_claim)", DRW, "drawer:25", sa_base())
            .built(pools_out(|s, x| s.deadline = x.d + x.drawer.claim_window)),
        Atk5::new("SA9 DA13 every new pool's deadline + 1", DRW, "drawer:25", sa_base()).built(pools_out(|s, _| s.deadline += 1)),
        Atk5::new("SA9 DA13 every new pool's deadline - 1", DRW, "drawer:25", sa_base()).built(pools_out(|s, _| s.deadline -= 1)),
        Atk5::new("SA9 DA14 controller' with drawn = 0", DRW, "drawer:24", sa_base()).built(|b, c| {
            let mut s = c.draw_case().unwrap().expected().controller;
            s.drawn = 0;
            b.outputs[2].script_public_key = b.templates.controller_spk(&s);
        }),
        Atk5::new("SA9 DA14 drawer' with draw_window + 1", DRW, "drawer:22", sa_base()).built(|b, c| {
            let mut s = c.draw_case().unwrap().expected().drawer;
            s.draw_window += 1;
            b.outputs[0].script_public_key = b.templates.drawer_spk(&s);
        }),
        Atk5::new("SA9 DA16 vault input value + 1", DRW, "drawer:5", sa_base()).built(|b, _| b.inputs[DrawCase::VAULT].entry.amount += 1),
        Atk5::new("SA9 DA16 vault input value - 1", DRW, "drawer:5", sa_base()).built(|b, _| b.inputs[DrawCase::VAULT].entry.amount -= 1),
    ]
}

// ---------------------------------------------------------------- SA12 (void floor)

/// `c` as the round of an instance whose drawer has draw_window = `dw` (a drawer UTXO the
/// builder's `validate()` would refuse; the drawer' output keeps it), lock re-fixed.
pub fn with_draw_window(mut c: DrawCase, dw: i64) -> DrawCase {
    c.drawer.draw_window = dw;
    c.fix_lock_time();
    c
}

/// SA12 (check 1; M7 Stage 5 fix A, review should-fix 1): the void floor on chain. An instance
/// with draw_window = VOID_MIN - 1 (= 2F - 1) could void at D + draw_window while X is still
/// drawable from some point of view (spec A5), so a backer could void a jackpot-hit round.
/// The drawer's check 1 requires draw_window >= VOID_MIN on every spend: the void at D +
/// draw_window and a draw of that instance are both rejected there. (The good bases are the
/// VOID_MIN instances of `void_base` / `sa_base`; the chain-level test with the unchecked
/// genesis and the negative control are `sa12_*` in `m7_attacks.rs`.)
pub fn sa12() -> Vec<Atk5> {
    let short = lottery::game::VOID_MIN - 1;
    vec![
        Atk5::new("SA12 void of an instance with draw_window = VOID_MIN - 1 at D + draw_window (M7V_floor)", DRW, "drawer:1", void_base()).dr(move |c| {
            *c = with_draw_window(c.clone(), short);
            assert_eq!(c.lock_time, c.d() as u64 + short as u64);
        }),
        Atk5::new("SA12 draw of an instance with draw_window = VOID_MIN - 1 (check 1 runs on every spend) (M7V_floor)", DRW, "drawer:1", sa_base()).dr(move |c| {
            *c = with_draw_window(c.clone(), short);
        }),
    ]
}

// ---------------------------------------------------------------- registry

pub fn all_atks7() -> Vec<Atk5> {
    [sa1(), sa2(), sa4(), sa5(), sa6(), sa7(), sa8(), sa9(), sa12()].into_iter().flatten().collect()
}

pub fn atk7(id: &str) -> Atk5 {
    let mut v: Vec<Atk5> = all_atks7().into_iter().filter(|a| a.id == id).collect();
    assert_eq!(v.len(), 1, "attack `{id}`: {} matches", v.len());
    v.pop().unwrap()
}
