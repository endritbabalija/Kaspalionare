//! Decoders for transaction bytes (spec M8 A4): the pushes of a signature script, a
//! script-number push, a template instance's state fields, and an entry's arguments by
//! the template's ABI. Moved out of `draw.rs` (M8 S2) so `index` and `draw` share them.
//! M8 S2c (review MF-C): a signature script is read as the engine reads it, not as the
//! ABI writes it. Consensus requires only push-only signature scripts (rusty-kaspa v2.1.0
//! `crypto/txscript/src/lib.rs:618-631`, `opcodes/mod.rs:142-144`; no minimal-push rule) and
//! decodes numbers without requiring a minimal encoding (`data_stack.rs:163-167`), so
//! anyone may submit a transition whose pushes are not the ABI's canonical bytes. `pushes`
//! gives the stack items the push opcodes put on the stack, and `entry_args` decodes each
//! argument from its stack item with the engine's own semantics (`script_num` =
//! `deserialize_i64`, `script_bool` = `OpcodeData<bool>`); it no longer requires the
//! canonical re-encoding.

use std::collections::BTreeMap;

use harness::ArtifactValue;
use silverscript_abi::TypeArtifact;

use crate::buy::Template;

/// The stack items a push-only script (a signature script) leaves, in order, as the
/// engine's push opcodes put them (rusty-kaspa v2.1.0 `crypto/txscript/src/opcodes/mod.rs`):
/// OP_0 -> empty (250), OP_DATA_1..75 and OP_PUSHDATA1/2/4 -> the data as it is, minimal or
/// not (255-331), OP_1NEGATE -> [0x81] (333, `push_literal_number(-1)`), OP_1..OP_16 -> [n]
/// (337-352). OP_RESERVED (0x50) fails when executed (335); every other opcode is not a
/// push (`is_push_opcode`: code <= OP_16, `opcodes/mod.rs:142-144`) and consensus rejects it
/// in a signature script (`lib.rs:629-631`).
pub(crate) fn pushes(script: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut out = vec![];
    let mut i = 0;
    let take = |i: &mut usize, n: usize| -> Result<Vec<u8>, String> {
        let end = i.checked_add(n).filter(|&e| e <= script.len()).ok_or("push past the end of the script")?;
        let v = script[*i..end].to_vec();
        *i = end;
        Ok(v)
    };
    while i < script.len() {
        let op = script[i];
        i += 1;
        let data = match op {
            0x00 => vec![],
            0x01..=0x4b => take(&mut i, op as usize)?,
            0x4c => {
                let n = take(&mut i, 1)?[0] as usize;
                take(&mut i, n)?
            }
            0x4d => {
                let b = take(&mut i, 2)?;
                take(&mut i, u16::from_le_bytes([b[0], b[1]]) as usize)?
            }
            0x4e => {
                let b = take(&mut i, 4)?;
                take(&mut i, u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)?
            }
            0x4f => vec![0x81],
            0x51..=0x60 => vec![op - 0x50],
            _ => return Err(format!("opcode {op:#04x} in a push-only script")),
        };
        out.push(data);
    }
    Ok(out)
}

/// The state field values of `redeem` under template `t` (prefix and suffix must match).
pub(crate) fn state_values(t: &Template, redeem: &[u8]) -> Result<BTreeMap<String, ArtifactValue>, String> {
    if redeem.len() < t.prefix.len() + t.suffix.len() || !redeem.starts_with(&t.prefix) || !redeem.ends_with(&t.suffix) {
        return Err(format!("redeem script is not a {} instance", t.name()));
    }
    let state = &redeem[t.prefix.len()..redeem.len() - t.suffix.len()];
    let c = t.art.contracts.values().next().unwrap();
    silverscript_abi::decode_runtime_state_script(&t.art, &c.runtime_state, state).map_err(|e| format!("{} state: {e}", t.name()))
}

/// A number stack item as the engine reads it (`deserialize_i64`, rusty-kaspa v2.1.0
/// `crypto/txscript/src/data_stack.rs:163-178`): little-endian sign-magnitude, at most 8
/// bytes, empty = 0, not required to be minimal ([0x01, 0x00] = 1, [0x80] = 0).
pub(crate) fn script_num(b: &[u8]) -> Result<i64, String> {
    if b.len() > 8 {
        return Err(format!("script number of {} bytes", b.len()));
    }
    let Some((&last, _)) = b.split_last() else { return Ok(0) };
    let mut mag: u64 = 0;
    for (i, &x) in b.iter().enumerate() {
        let x = if i == b.len() - 1 { x & 0x7f } else { x };
        mag |= (x as u64) << (8 * i);
    }
    let v = i64::try_from(mag).map_err(|_| "script number out of range".to_string())?;
    Ok(if last & 0x80 != 0 { -v } else { v })
}

/// A bool stack item as the engine reads it (`OpcodeData<bool>`, rusty-kaspa v2.1.0
/// `crypto/txscript/src/data_stack.rs:229-238`, what OP_IF, OP_NOTIF and OP_VERIFY pop):
/// false iff every byte is zero, the last one ignoring its sign bit (negative zero).
pub(crate) fn script_bool(b: &[u8]) -> bool {
    match b.split_last() {
        None => false,
        Some((&last, rest)) => last & 0x7f != 0 || rest.iter().any(|&x| x != 0),
    }
}

/// The entry a signature script calls on template `t`, with its arguments and the redeem
/// script: the stack items are the arguments, the 4-byte dispatch tag, then the redeem. The
/// entry is the one whose dispatch tag matches; each argument is decoded from its stack item
/// by the entry's ABI parameter type, with the engine's semantics: int as a number
/// (`script_num`), bool as a truth value (`script_bool`; the ABI pushes 0 or 1), byte as
/// its one byte, byte strings as they are, fixed sizes checked. The sizes are the ones the
/// compiled entry itself checks before anything else (pinned SilverScript
/// `compiler/compile/statement.rs:105-150`: a byte exactly 1 byte, so the empty item is
/// refused, S2d; a bool 0 or 1 byte; an int at most 8 bytes; fixed sizes exact), so an
/// argument the engine refuses is refused here too. The
/// pushes need not be the ABI's canonical encoding (S2c MF-C): two signature scripts whose
/// items decode to the same values give the same arguments. A value the scripts read
/// differently from this decoding (a byte string where the script reads a number, say)
/// would rebuild a successor that differs from the transaction's output, which the index
/// reports as an error, never accepts silently. The redeem must be an instance of `t`.
pub(crate) fn entry_args(t: &Template, sig: &[u8]) -> Result<(String, Vec<ArtifactValue>, Vec<u8>), String> {
    let p = pushes(sig)?;
    if p.len() < 2 {
        return Err(format!("{} pushes: no dispatch tag and redeem", p.len()));
    }
    let redeem = p[p.len() - 1].clone();
    state_values(t, &redeem)?;
    let tag = &p[p.len() - 2];
    let c = t.art.contracts.values().next().unwrap();
    let (name, entry) = c
        .entries
        .iter()
        .find(|(_, e)| e.dispatch_tag.as_bytes().as_slice() == tag.as_slice())
        .ok_or_else(|| format!("{}: no entry with dispatch tag {}", t.name(), hex(tag)))?;
    let raw = &p[..p.len() - 2];
    if raw.len() != entry.params.len() {
        return Err(format!("{}.{name}: {} arguments, the ABI has {}", t.name(), raw.len(), entry.params.len()));
    }
    let mut args = Vec::with_capacity(raw.len());
    for (param, b) in entry.params.iter().zip(raw) {
        let v = match &param.ty {
            TypeArtifact::Int | TypeArtifact::Temporal => ArtifactValue::Int(script_num(b)?),
            TypeArtifact::Bool => {
                // The entry checks OP_SIZE < 2 (statement.rs:143-147).
                if b.len() > 1 {
                    return Err(format!("{}.{name} {}: bool of {} bytes", t.name(), param.name, b.len()));
                }
                ArtifactValue::Bool(script_bool(b))
            }
            // The entry checks OP_SIZE == 1 (statement.rs:115-116, 130-134); OP_0 pushes
            // the empty item, which it refuses.
            TypeArtifact::Byte => match b.len() {
                1 => ArtifactValue::Byte(b[0]),
                n => return Err(format!("{}.{name} {}: byte of {n} bytes", t.name(), param.name)),
            },
            TypeArtifact::Bytes => ArtifactValue::Bytes(b.clone()),
            // Fixed sizes as the ABI pushes them (silverscript-abi `push_sig_arg`: pubkey 32,
            // sig 65, datasig 64, byte[n] n bytes).
            TypeArtifact::Pubkey | TypeArtifact::Sig | TypeArtifact::Datasig | TypeArtifact::FixedBytes { .. } => {
                let len = match &param.ty {
                    TypeArtifact::Pubkey => 32,
                    TypeArtifact::Sig => 65,
                    TypeArtifact::Datasig => 64,
                    TypeArtifact::FixedBytes { len } => *len,
                    _ => unreachable!(),
                };
                if b.len() != len {
                    return Err(format!("{}.{name} {}: {} bytes, expected {len}", t.name(), param.name, b.len()));
                }
                ArtifactValue::Bytes(b.clone())
            }
            other => return Err(format!("{}.{name} {}: unsupported argument type {other:?}", t.name(), param.name)),
        };
        args.push(v);
    }
    Ok((name.clone(), args, redeem))
}

/// Lowercase hex.
pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Typed accessors of decoded state fields and arguments.
pub(crate) fn int(v: &BTreeMap<String, ArtifactValue>, k: &str) -> Result<i64, String> {
    match v.get(k) {
        Some(ArtifactValue::Int(x)) => Ok(*x),
        other => Err(format!("state field {k}: {other:?}")),
    }
}

pub(crate) fn byte(v: &BTreeMap<String, ArtifactValue>, k: &str) -> Result<u8, String> {
    match v.get(k) {
        Some(ArtifactValue::Byte(x)) => Ok(*x),
        other => Err(format!("state field {k}: {other:?}")),
    }
}

pub(crate) fn bytes(v: &BTreeMap<String, ArtifactValue>, k: &str) -> Result<Vec<u8>, String> {
    match v.get(k) {
        Some(ArtifactValue::Bytes(x)) => Ok(x.clone()),
        other => Err(format!("state field {k}: {other:?}")),
    }
}

pub(crate) fn bytes_n<const N: usize>(v: &BTreeMap<String, ArtifactValue>, k: &str) -> Result<[u8; N], String> {
    let b = bytes(v, k)?;
    b.as_slice().try_into().map_err(|_| format!("state field {k}: {} bytes, expected {N}", b.len()))
}

pub(crate) fn arg_int(a: &[ArtifactValue], i: usize) -> Result<i64, String> {
    match a.get(i) {
        Some(ArtifactValue::Int(x)) => Ok(*x),
        other => Err(format!("argument {i}: {other:?}, expected int")),
    }
}

pub(crate) fn arg_bool(a: &[ArtifactValue], i: usize) -> Result<bool, String> {
    match a.get(i) {
        Some(ArtifactValue::Bool(x)) => Ok(*x),
        other => Err(format!("argument {i}: {other:?}, expected bool")),
    }
}

pub(crate) fn arg_byte(a: &[ArtifactValue], i: usize) -> Result<u8, String> {
    match a.get(i) {
        Some(ArtifactValue::Byte(x)) => Ok(*x),
        other => Err(format!("argument {i}: {other:?}, expected byte")),
    }
}

pub(crate) fn arg_bytes(a: &[ArtifactValue], i: usize) -> Result<&[u8], String> {
    match a.get(i) {
        Some(ArtifactValue::Bytes(x)) => Ok(x),
        other => Err(format!("argument {i}: {other:?}, expected bytes")),
    }
}
