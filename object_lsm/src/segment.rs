//! Immutable sorted segment (SST-like) format — v2, block-indexed.
//!
//! Object layout (all integers little-endian):
//! ```text
//! [data blocks ...]
//!   block := crc32 u32 | payload_len u32 | payload
//!   payload := sorted entries (key bytes | flag u8 | (value bytes)?)
//! [block index]
//!   n u32 | per block: offset u32 | len u32 | first_key
//! [trailer: 20 bytes]
//!   count u64 | index_offset u32 | index_len u32 | TAIL_MAGIC u32
//! ```
//! The fixed 20-byte trailer allows a reader to locate the block index with a
//! single small Range GET at the object tail, then fetch only the blocks it
//! needs via byte-range GETs — the S3-friendly access pattern. `flag == 0`
//! marks a tombstone (deleted key), `flag == 1` a live value.

use serde::{Deserialize, Serialize};

use crate::{
  codec::{Reader, put_bytes, put_u32, put_u64},
  error::{Error, Result},
};

/// Tail magic identifying a v2 segment: `"SEGV"`.
pub const SEG_TAIL_MAGIC: u32 = 0x5345_4756;
/// Fixed trailer byte length (count/index_offset/index_len/magic).
pub const TAIL_LEN: usize = 20;
/// Bytes of per-block framing overhead (crc + payload_len).
pub const BLOCK_HEADER_LEN: usize = 8;
/// Default target data-block payload size.
pub const DEFAULT_BLOCK_SIZE: usize = 32 * 1024;

/// A decoded segment entry list where `None` value = tombstone.
pub type SegmentEntries = Vec<(Vec<u8>, Option<Vec<u8>>)>;

/// Parsed segment trailer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trailer {
  pub count: u64,
  pub index_offset: u32,
  pub index_len: u32,
}

/// One data block's location and its first key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockMeta {
  /// Byte offset of the block (crc header) inside the object.
  pub offset: u32,
  /// Total block byte length (header + payload).
  pub len: u32,
  /// First key of the block.
  pub first: Vec<u8>,
}

/// In-memory block index of one segment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentIndex {
  pub blocks: Vec<BlockMeta>,
}

/// Persistent metadata of one immutable segment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SegmentMeta {
  /// Globally unique segment id (also the object suffix).
  pub id: u64,
  /// Journal watermark reached when this segment was flushed.
  pub seq: u64,
  /// Smallest key (bounds for read skipping).
  pub first: Vec<u8>,
  /// Largest key.
  pub last: Vec<u8>,
  /// Number of encoded entries (including tombstones).
  pub count: u64,
  /// Number of tombstone entries.
  pub tombstones: u64,
  /// Encoded object size in bytes.
  pub bytes: u64,
  /// Number of data blocks.
  #[serde(default)]
  pub blocks: u64,
  /// Embedded block index loaded from the manifest (None for legacy manifests;
  /// falls back to tail + index Range GETs).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub index: Option<SegmentIndex>,
}

/// Parse the fixed trailer from the last `TAIL_LEN` bytes of an object.
pub fn parse_tail(tail: &[u8]) -> Result<Trailer> {
  if tail.len() != TAIL_LEN {
    return Err(Error::Corrupt(format!(
      "trailer length {}, want {TAIL_LEN}",
      tail.len()
    )));
  }
  let mut r = Reader::new(tail);
  let count = r.u64()?;
  let index_offset = r.u32()?;
  let index_len = r.u32()?;
  let magic = r.u32()?;
  if magic != SEG_TAIL_MAGIC {
    return Err(Error::Corrupt(format!("bad segment tail magic {magic:#x}")));
  }
  Ok(Trailer {
    count,
    index_offset,
    index_len,
  })
}

/// Decode a block-index payload (bytes stored between data region and trailer).
pub fn decode_index(buf: &[u8]) -> Result<SegmentIndex> {
  let mut r = Reader::new(buf);
  let n = r.u32()?;
  let mut blocks = Vec::with_capacity(n as usize);
  for _ in 0..n {
    let offset = r.u32()?;
    let len = r.u32()?;
    let first = r.bytes()?;
    blocks.push(BlockMeta { offset, len, first });
  }
  if r.remaining() != 0 {
    return Err(Error::Corrupt("trailing bytes after block index".into()));
  }
  Ok(SegmentIndex { blocks })
}

/// Locate the block that may contain `key` (greatest block whose first key
/// is `<= key`), or `None` when the key sorts before every block.
pub fn find_block(index: &SegmentIndex, key: &[u8]) -> Option<usize> {
  let idx = index.blocks.partition_point(|b| b.first.as_slice() <= key);
  if idx == 0 { None } else { Some(idx - 1) }
}

/// Verify the block framing header + crc and decode its payload entries.
pub fn decode_block(raw: &[u8]) -> Result<SegmentEntries> {
  if raw.len() < BLOCK_HEADER_LEN {
    return Err(Error::Corrupt("block shorter than header".into()));
  }
  let (head, payload) = raw.split_at(BLOCK_HEADER_LEN);
  let expect_crc = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
  let expect_len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
  if payload.len() != expect_len {
    return Err(Error::Corrupt(format!(
      "block payload {} bytes, header says {expect_len}",
      payload.len()
    )));
  }
  if crc32fast::hash(payload) != expect_crc {
    return Err(Error::Corrupt("block checksum mismatch".into()));
  }
  decode_payload(payload)
}

/// Decode a block payload (entry list) into sorted entries.
pub fn decode_payload(payload: &[u8]) -> Result<SegmentEntries> {
  let mut r = Reader::new(payload);
  let mut entries = Vec::new();
  let mut prev: Option<Vec<u8>> = None;
  while r.remaining() > 0 {
    let key = r.bytes()?;
    if let Some(p) = &prev
      && &key <= p
    {
      return Err(Error::Corrupt(
        "segment keys out of order / duplicated".into(),
      ));
    }
    let flag = r.u8()?;
    let value = match flag {
      0 => None,
      1 => Some(r.bytes()?),
      other => return Err(Error::Corrupt(format!("bad segment flag {other}"))),
    };
    prev = Some(key.clone());
    entries.push((key, value));
  }
  Ok(entries)
}

fn append_entry(out: &mut Vec<u8>, key: &[u8], value: Option<&[u8]>) -> Result<()> {
  put_bytes(out, key)?;
  match value {
    Some(v) => {
      out.push(1u8);
      put_bytes(out, v)?;
    }
    None => out.push(0u8),
  }
  Ok(())
}

/// Encode sorted, duplicate-free entries into a block-indexed segment object.
pub fn encode_segment(entries: &SegmentEntries, block_size: usize) -> Result<Vec<u8>> {
  let block_size = block_size.max(1);
  let mut data: Vec<u8> = Vec::new();
  let mut idx: Vec<(u32, u32, Vec<u8>)> = Vec::new();
  let mut payload: Vec<u8> = Vec::new();
  let mut block_first: Option<Vec<u8>> = None;
  let mut count = 0u64;

  let seal = |data: &mut Vec<u8>,
              idx: &mut Vec<(u32, u32, Vec<u8>)>,
              payload: &mut Vec<u8>,
              first: &mut Option<Vec<u8>>|
   -> Result<()> {
    if payload.is_empty() {
      return Ok(());
    }
    let offset = data.len() as u32;
    let len = (BLOCK_HEADER_LEN + payload.len()) as u32;
    let crc = crc32fast::hash(payload);
    put_u32(data, crc);
    put_u32(data, payload.len() as u32);
    data.append(payload);
    let first = first
      .take()
      .ok_or_else(|| Error::Encode("block missing first key".into()))?;
    idx.push((offset, len, first));
    Ok(())
  };

  for (k, v) in entries {
    if payload.is_empty() {
      block_first = Some(k.clone());
    }
    append_entry(&mut payload, k, v.as_deref())?;
    count += 1;
    if payload.len() >= block_size {
      seal(&mut data, &mut idx, &mut payload, &mut block_first)?;
    }
  }
  seal(&mut data, &mut idx, &mut payload, &mut block_first)?;

  let index_offset = data.len() as u32;
  let mut index = Vec::new();
  put_u32(&mut index, idx.len() as u32);
  for (offset, len, first) in &idx {
    put_u32(&mut index, *offset);
    put_u32(&mut index, *len);
    put_bytes(&mut index, first)?;
  }
  data.append(&mut index);

  let index_len = (data.len() - index_offset as usize) as u32;
  put_u64(&mut data, count);
  put_u32(&mut data, index_offset);
  put_u32(&mut data, index_len);
  put_u32(&mut data, SEG_TAIL_MAGIC);
  Ok(data)
}

/// Build [`SegmentMeta`] from the encoded object + source entries.
pub fn build_segment_meta(
  id: u64,
  seq: u64,
  encoded: &[u8],
  entries: &SegmentEntries,
  embed_index: bool,
) -> Result<SegmentMeta> {
  if encoded.len() < TAIL_LEN {
    return Err(Error::Corrupt(
      "encoded segment shorter than trailer".into(),
    ));
  }
  let tail = parse_tail(&encoded[encoded.len() - TAIL_LEN..])?;
  let idx_start = tail.index_offset as usize;
  let idx_end = idx_start + tail.index_len as usize;
  if idx_end > encoded.len() - TAIL_LEN {
    return Err(Error::Corrupt("segment index out of bounds".into()));
  }
  let index = decode_index(&encoded[idx_start..idx_end])?;
  let blocks = index.blocks.len() as u64;
  let tombstones = entries.iter().filter(|(_, v)| v.is_none()).count() as u64;
  Ok(SegmentMeta {
    id,
    seq,
    first: entries.first().map(|(k, _)| k.clone()).unwrap_or_default(),
    last: entries.last().map(|(k, _)| k.clone()).unwrap_or_default(),
    count: tail.count,
    tombstones,
    bytes: encoded.len() as u64,
    blocks,
    index: embed_index.then_some(index),
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sample() -> SegmentEntries {
    let mut e = SegmentEntries::new();
    e.push((b"gone".to_vec(), None));
    for i in 0..500u32 {
      e.push((
        format!("key{i:05}").into_bytes(),
        Some(format!("value-{i}").into_bytes()),
      ));
    }
    e
  }

  #[test]
  fn roundtrip_block_format() {
    let entries = sample();
    let encoded = encode_segment(&entries, 64).unwrap();
    let tail = parse_tail(&encoded[encoded.len() - TAIL_LEN..]).unwrap();
    assert_eq!(tail.count, entries.len() as u64);
    assert!(tail.index_len > 0);
    let idx = decode_index(
      &encoded[tail.index_offset as usize..(tail.index_offset + tail.index_len) as usize],
    )
    .unwrap();
    assert!(idx.blocks.len() >= 2, "expected multiple blocks");
    // each block decodes & concatenated equals full list
    let mut all = SegmentEntries::new();
    for b in &idx.blocks {
      let raw = &encoded[b.offset as usize..(b.offset + b.len) as usize];
      all.extend(decode_block(raw).unwrap());
    }
    assert_eq!(all, entries);
    let meta = build_segment_meta(7, 3, &encoded, &entries, true).unwrap();
    assert_eq!(meta.count, entries.len() as u64);
    assert_eq!(meta.blocks, idx.blocks.len() as u64);
    assert_eq!(meta.first, b"gone".to_vec());
    assert_eq!(meta.last, b"key00499".to_vec());
  }

  #[test]
  fn find_block_targets_correct_block() {
    let entries = sample();
    let encoded = encode_segment(&entries, 64).unwrap();
    let tail = parse_tail(&encoded[encoded.len() - TAIL_LEN..]).unwrap();
    let idx = decode_index(
      &encoded[tail.index_offset as usize..(tail.index_offset + tail.index_len) as usize],
    )
    .unwrap();
    for (k, v) in &entries {
      let bi = find_block(&idx, k).unwrap();
      let raw = &encoded
        [idx.blocks[bi].offset as usize..(idx.blocks[bi].offset + idx.blocks[bi].len) as usize];
      let block = decode_block(raw).unwrap();
      let pos = block.partition_point(|(kk, _)| kk.as_slice() < k.as_slice());
      assert_eq!(
        block.get(pos).map(|(_, vv)| vv),
        Some(v),
        "key {k:?} not found in its block"
      );
    }
    assert_eq!(find_block(&idx, b"aaa"), None);
  }
}
