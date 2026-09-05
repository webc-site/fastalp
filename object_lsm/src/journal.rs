//! Journal record-group format.
//!
//! A committed batch is serialized as one [`Group`] containing an ordered op
//! list that may span multiple partitions (`data` + `meta` in wedb_embed).
//! Each group is stored as its own immutable journal object, which makes a PUT
//! the atomic durability point: an uploaded group is either fully present or
//! absent — there is no torn tail to repair on recovery.

use crate::{
  codec::{Reader, put_bytes, put_str, put_u32, put_u64},
  error::{Error, Result},
};

/// Magic identifying a journal group payload: `"WGL1"`.
pub const GROUP_MAGIC: u32 = 0x5747_4C31;

/// A single key-value mutation targeting one partition.
///
/// `value == None` encodes a delete (tombstone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
  /// Target partition (keyspace) name.
  pub part: String,
  /// Key bytes.
  pub key: Vec<u8>,
  /// New value, or `None` to delete.
  pub value: Option<Vec<u8>>,
}

impl Op {
  pub fn put(part: impl Into<String>, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Self {
    Self {
      part: part.into(),
      key: key.into(),
      value: Some(value.into()),
    }
  }

  pub fn delete(part: impl Into<String>, key: impl Into<Vec<u8>>) -> Self {
    Self {
      part: part.into(),
      key: key.into(),
      value: None,
    }
  }
}

/// An atomically committed group of mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
  /// Monotonic sequence assigned at commit time.
  pub seq: u64,
  /// Fencing epoch (0 for unfenced engines).
  pub epoch: u128,
  pub ops: Vec<Op>,
}

pub fn encode_group(g: &Group) -> Result<Vec<u8>> {
  let mut out = Vec::with_capacity(64 + g.ops.len() * 32);
  put_u32(&mut out, GROUP_MAGIC);
  put_u64(&mut out, g.seq);
  put_u64(&mut out, (g.epoch >> 64) as u64);
  put_u64(&mut out, g.epoch as u64);
  put_u32(&mut out, g.ops.len() as u32);
  for op in &g.ops {
    put_str(&mut out, &op.part)?;
    put_bytes(&mut out, &op.key)?;
    match &op.value {
      Some(v) => {
        out.push(1u8);
        put_bytes(&mut out, v)?;
      }
      None => out.push(0u8),
    }
  }
  Ok(out)
}

/// Read one group from the front of `r` (used by single & stream decoding).
fn read_group(r: &mut Reader<'_>) -> Result<Group> {
  let magic = r.u32()?;
  if magic != GROUP_MAGIC {
    return Err(Error::Corrupt(format!("bad journal magic {magic:#x}")));
  }
  let seq = r.u64()?;
  let epoch = ((r.u64()? as u128) << 64) | (r.u64()? as u128);
  let count = r.u32()?;
  let mut ops = Vec::with_capacity(count as usize);
  for _ in 0..count {
    let part = r.str()?;
    let key = r.bytes()?;
    let flag = r.u8()?;
    let value = match flag {
      0 => None,
      1 => Some(r.bytes()?),
      other => return Err(Error::Corrupt(format!("bad journal op flag {other}"))),
    };
    ops.push(Op { part, key, value });
  }
  Ok(Group { seq, epoch, ops })
}

pub fn decode_group(buf: &[u8]) -> Result<Group> {
  let mut r = Reader::new(buf);
  let group = read_group(&mut r)?;
  if r.remaining() != 0 {
    return Err(Error::Corrupt("trailing bytes after journal group".into()));
  }
  Ok(group)
}

/// Decode a journal object that may contain several concatenated groups
/// (group-commit batching) into its individual groups, in order.
pub fn decode_group_stream(buf: &[u8]) -> Result<Vec<Group>> {
  let mut r = Reader::new(buf);
  let mut out = Vec::new();
  while r.remaining() > 0 {
    out.push(read_group(&mut r)?);
  }
  Ok(out)
}
