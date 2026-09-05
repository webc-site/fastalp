//! Streaming ordered iteration over a partition (memtable + segment blocks).
//!
//! Forward (`next`) and backward (`next_back`) merge iterators load at most
//! one decoded block per source at a time, so a scan touches only the blocks
//! it actually reads. Duplicate keys across layers resolve newest-wins;
//! tombstones suppress older copies without materializing anything.
//!
//! Pure forward and pure backward scans stream. When both directions are used
//! on one iterator, [`crate::partition::ObjectLsmIter`] keeps both streaming
//! cursors and uses delivered-key watermarks to avoid producing the same key
//! twice, so mixed-direction scans stay streaming as well.

use std::{collections::BinaryHeap, sync::Arc};

use crate::{
  engine::Inner,
  error::Result,
  segment::{SegmentIndex, SegmentMeta},
  state::{MemEntry, PartitionState},
};

/// Owned inclusive/exclusive scan bounds (cloned from caller slices).
#[derive(Clone, Debug)]
pub struct Bounds {
  pub lower: std::ops::Bound<Vec<u8>>,
  pub upper: std::ops::Bound<Vec<u8>>,
}

pub fn ge_lower(k: &[u8], lower: &std::ops::Bound<Vec<u8>>) -> bool {
  match lower {
    std::ops::Bound::Unbounded => true,
    std::ops::Bound::Included(x) => k >= x.as_slice(),
    std::ops::Bound::Excluded(x) => k > x.as_slice(),
  }
}

pub fn gt_upper(k: &[u8], upper: &std::ops::Bound<Vec<u8>>) -> bool {
  match upper {
    std::ops::Bound::Unbounded => false,
    std::ops::Bound::Included(x) => k > x.as_slice(),
    std::ops::Bound::Excluded(x) => k >= x.as_slice(),
  }
}

/// True when a run whose greatest key is `max` lies entirely below `lower`.
fn max_below_lower(max: &[u8], lower: &std::ops::Bound<Vec<u8>>) -> bool {
  match lower {
    std::ops::Bound::Unbounded => false,
    std::ops::Bound::Included(x) => max < x.as_slice(),
    std::ops::Bound::Excluded(x) => max <= x.as_slice(),
  }
}

/// Greatest key a block can contain (exclusive upper for non-last blocks).
fn block_max(index: &SegmentIndex, seg: &SegmentMeta, i: usize) -> Vec<u8> {
  index
    .blocks
    .get(i + 1)
    .map(|b| b.first.clone())
    .unwrap_or_else(|| seg.last.clone())
}

fn seg_overlaps(seg: &SegmentMeta, b: &Bounds) -> bool {
  let lo_ok = match &b.lower {
    std::ops::Bound::Unbounded => true,
    std::ops::Bound::Included(x) => seg.last.as_slice() >= x.as_slice(),
    std::ops::Bound::Excluded(x) => seg.last.as_slice() > x.as_slice(),
  };
  let hi_ok = match &b.upper {
    std::ops::Bound::Unbounded => true,
    std::ops::Bound::Included(x) => seg.first.as_slice() <= x.as_slice(),
    std::ops::Bound::Excluded(x) => seg.first.as_slice() < x.as_slice(),
  };
  lo_ok && hi_ok
}

type KV = (Vec<u8>, Option<Vec<u8>>);

/// Consistent snapshot of one partition, cloned while holding the partition
/// read lock (the caller keeps the owned read guard alive for the scan's life).
#[derive(Clone)]
pub(crate) struct Snap {
  pub(crate) prefix: String,
  pub(crate) mem: Vec<KV>,
  pub(crate) segments: Vec<SegmentMeta>,
}

/// Clone the bounded view of `ps` without releasing its read lock.
pub(crate) fn snap_from(prefix: String, ps: &PartitionState, b: &Bounds) -> Snap {
  let mem = ps
    .mem
    .iter()
    .filter(|(k, _)| ge_lower(k, &b.lower) && !gt_upper(k, &b.upper))
    .map(|(k, e)| {
      let v = match e {
        MemEntry::Value(v) => Some(v.clone()),
        MemEntry::Tombstone => None,
      };
      (k.clone(), v)
    })
    .collect();
  let segments = ps
    .meta
    .segments
    .iter()
    .filter(|s| seg_overlaps(s, b))
    .cloned()
    .collect();
  Snap {
    prefix,
    mem,
    segments,
  }
}

// ---------------------------------------------------------------------------
// forward cursor sources
// ---------------------------------------------------------------------------

enum FwdKind {
  Mem {
    entries: Vec<KV>,
    pos: usize,
  },
  Seg {
    seg: SegmentMeta,
    index: Arc<SegmentIndex>,
    block_i: usize,
    cur: Option<Arc<Vec<KV>>>,
    pos: usize,
  },
}

impl FwdKind {
  fn next_entry(
    &mut self,
    eng: &Inner,
    part: &str,
    prefix: &str,
    bounds: &Bounds,
  ) -> Result<Option<KV>> {
    match self {
      FwdKind::Mem { entries, pos } => {
        while *pos < entries.len() {
          let e = entries[*pos].clone();
          *pos += 1;
          if !ge_lower(&e.0, &bounds.lower) {
            continue;
          }
          if gt_upper(&e.0, &bounds.upper) {
            *pos = entries.len();
            return Ok(None);
          }
          return Ok(Some(e));
        }
        Ok(None)
      }
      FwdKind::Seg {
        seg,
        index,
        block_i,
        cur,
        pos,
      } => loop {
        if cur.is_none() {
          if *block_i >= index.blocks.len() {
            return Ok(None);
          }
          let i = *block_i;
          let bm = index.blocks[i].clone();
          let max = block_max(index, seg, i);
          if max_below_lower(&max, &bounds.lower) {
            *block_i += 1;
            continue;
          }
          if gt_upper(&bm.first, &bounds.upper) {
            *block_i = index.blocks.len();
            return Ok(None);
          }
          let entries = eng.load_block(prefix, part, seg, &bm)?;
          *cur = Some(entries);
          *pos = 0;
        }
        let entries = cur.as_ref().unwrap();
        while *pos < entries.len() {
          let e = entries[*pos].clone();
          *pos += 1;
          if !ge_lower(&e.0, &bounds.lower) {
            continue;
          }
          if gt_upper(&e.0, &bounds.upper) {
            *block_i = index.blocks.len();
            return Ok(None);
          }
          return Ok(Some(e));
        }
        *cur = None;
        *block_i += 1;
      },
    }
  }
}

struct FwdItem {
  priority: u32,
  entry: Option<KV>,
  kind: FwdKind,
}

impl PartialEq for FwdItem {
  fn eq(&self, other: &Self) -> bool {
    self.cmp(other) == std::cmp::Ordering::Equal
  }
}
impl Eq for FwdItem {}
impl PartialOrd for FwdItem {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}
impl Ord for FwdItem {
  /// Reversed ordering: the min-heap pops the smallest key, newest layer on ties.
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    let a = self.entry.as_ref().map(|e| e.0.as_slice());
    let b = other.entry.as_ref().map(|e| e.0.as_slice());
    match (a, b) {
      (Some(a), Some(b)) => b.cmp(a).then_with(|| other.priority.cmp(&self.priority)),
      (Some(_), None) => std::cmp::Ordering::Greater,
      (None, Some(_)) => std::cmp::Ordering::Less,
      (None, None) => std::cmp::Ordering::Equal,
    }
  }
}

/// Forward streaming merge over memtable + segments (ascending live keys).
pub struct FwdMerge {
  eng: Arc<Inner>,
  part: String,
  prefix: String,
  bounds: Bounds,
  heap: BinaryHeap<FwdItem>,
  last_key: Option<Vec<u8>>,
  failed: bool,
}

impl FwdMerge {
  pub fn new(eng: Arc<Inner>, part: String, bounds: Bounds, snap: Snap) -> Result<Self> {
    let mut heap = BinaryHeap::new();
    if !snap.mem.is_empty() {
      let mut kind = FwdKind::Mem {
        entries: snap.mem,
        pos: 0,
      };
      if let Some(e) = kind.next_entry(&eng, &part, &snap.prefix, &bounds)? {
        heap.push(FwdItem {
          priority: 0,
          entry: Some(e),
          kind,
        });
      }
    }
    for (i, seg) in snap.segments.iter().rev().enumerate() {
      let index = eng.load_index(&snap.prefix, &part, seg)?;
      let mut kind = FwdKind::Seg {
        seg: seg.clone(),
        index,
        block_i: 0,
        cur: None,
        pos: 0,
      };
      if let Some(e) = kind.next_entry(&eng, &part, &snap.prefix, &bounds)? {
        heap.push(FwdItem {
          priority: 1 + i as u32,
          entry: Some(e),
          kind,
        });
      }
    }
    Ok(Self {
      eng,
      part,
      prefix: snap.prefix,
      bounds,
      heap,
      last_key: None,
      failed: false,
    })
  }

  /// Next live entry in ascending order.
  pub fn next(&mut self) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    if self.failed {
      return Ok(None);
    }
    loop {
      let Some(mut item) = self.heap.pop() else {
        return Ok(None);
      };
      let Some((key, value)) = item.entry.take() else {
        continue;
      };
      match item
        .kind
        .next_entry(&self.eng, &self.part, &self.prefix, &self.bounds)
      {
        Ok(next) => {
          if let Some(e) = next {
            item.entry = Some(e);
            self.heap.push(item);
          }
        }
        Err(e) => {
          self.failed = true;
          return Err(e);
        }
      }
      if self.last_key.as_deref() == Some(key.as_slice()) {
        continue;
      }
      self.last_key = Some(key.clone());
      if let Some(v) = value {
        return Ok(Some((key, v)));
      }
    }
  }
}

// ---------------------------------------------------------------------------
// backward cursor sources
// ---------------------------------------------------------------------------

enum BackKind {
  Mem {
    entries: Vec<KV>,
    pos: usize,
  },
  Seg {
    seg: SegmentMeta,
    index: Arc<SegmentIndex>,
    /// Next block index to open (descending); `None` when exhausted.
    block_i: Option<usize>,
    cur: Option<Arc<Vec<KV>>>,
    pos: usize,
  },
}

impl BackKind {
  fn next_entry(
    &mut self,
    eng: &Inner,
    part: &str,
    prefix: &str,
    bounds: &Bounds,
  ) -> Result<Option<KV>> {
    match self {
      BackKind::Mem { entries, pos } => {
        while *pos > 0 {
          let e = entries[*pos - 1].clone();
          *pos -= 1;
          if gt_upper(&e.0, &bounds.upper) {
            continue;
          }
          if !ge_lower(&e.0, &bounds.lower) {
            *pos = 0;
            return Ok(None);
          }
          return Ok(Some(e));
        }
        Ok(None)
      }
      BackKind::Seg {
        seg,
        index,
        block_i,
        cur,
        pos,
      } => loop {
        if cur.is_none() {
          let Some(i) = *block_i else { return Ok(None) };
          let bm = index.blocks[i].clone();
          let max = block_max(index, seg, i);
          if gt_upper(&bm.first, &bounds.upper) {
            *block_i = i.checked_sub(1);
            continue;
          }
          if max_below_lower(&max, &bounds.lower) {
            *block_i = None;
            return Ok(None);
          }
          let entries = eng.load_block(prefix, part, seg, &bm)?;
          let n = entries.len();
          *cur = Some(entries);
          *pos = n;
        }
        let entries = cur.as_ref().unwrap();
        while *pos > 0 {
          let e = entries[*pos - 1].clone();
          *pos -= 1;
          if gt_upper(&e.0, &bounds.upper) {
            continue;
          }
          if !ge_lower(&e.0, &bounds.lower) {
            *block_i = None;
            return Ok(None);
          }
          return Ok(Some(e));
        }
        *cur = None;
        let next_i = block_i.unwrap_or(0).checked_sub(1);
        *block_i = next_i;
      },
    }
  }
}

struct BackItem {
  priority: u32,
  entry: Option<KV>,
  kind: BackKind,
}

impl PartialEq for BackItem {
  fn eq(&self, other: &Self) -> bool {
    self.cmp(other) == std::cmp::Ordering::Equal
  }
}
impl Eq for BackItem {}
impl PartialOrd for BackItem {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}
impl Ord for BackItem {
  /// Max-heap ordering: pops the largest key, newest layer on ties.
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    let a = self.entry.as_ref().map(|e| e.0.as_slice());
    let b = other.entry.as_ref().map(|e| e.0.as_slice());
    match (a, b) {
      (Some(a), Some(b)) => a.cmp(b).then_with(|| other.priority.cmp(&self.priority)),
      (Some(_), None) => std::cmp::Ordering::Greater,
      (None, Some(_)) => std::cmp::Ordering::Less,
      (None, None) => std::cmp::Ordering::Equal,
    }
  }
}

/// Backward streaming merge over memtable + segments (descending live keys).
pub struct BackMerge {
  eng: Arc<Inner>,
  part: String,
  prefix: String,
  bounds: Bounds,
  heap: BinaryHeap<BackItem>,
  last_key: Option<Vec<u8>>,
  failed: bool,
}

impl BackMerge {
  pub fn new(eng: Arc<Inner>, part: String, bounds: Bounds, snap: Snap) -> Result<Self> {
    let mut heap = BinaryHeap::new();
    if !snap.mem.is_empty() {
      let n = snap.mem.len();
      let mut kind = BackKind::Mem {
        entries: snap.mem,
        pos: n,
      };
      if let Some(e) = kind.next_entry(&eng, &part, &snap.prefix, &bounds)? {
        heap.push(BackItem {
          priority: 0,
          entry: Some(e),
          kind,
        });
      }
    }
    for (i, seg) in snap.segments.iter().rev().enumerate() {
      let index = eng.load_index(&snap.prefix, &part, seg)?;
      let first = index.blocks.len().checked_sub(1);
      let mut kind = BackKind::Seg {
        seg: seg.clone(),
        index,
        block_i: first,
        cur: None,
        pos: 0,
      };
      if let Some(e) = kind.next_entry(&eng, &part, &snap.prefix, &bounds)? {
        heap.push(BackItem {
          priority: 1 + i as u32,
          entry: Some(e),
          kind,
        });
      }
    }
    Ok(Self {
      eng,
      part,
      prefix: snap.prefix,
      bounds,
      heap,
      last_key: None,
      failed: false,
    })
  }

  /// Next live entry in descending order.
  pub fn next(&mut self) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    if self.failed {
      return Ok(None);
    }
    loop {
      let Some(mut item) = self.heap.pop() else {
        return Ok(None);
      };
      let Some((key, value)) = item.entry.take() else {
        continue;
      };
      match item
        .kind
        .next_entry(&self.eng, &self.part, &self.prefix, &self.bounds)
      {
        Ok(next) => {
          if let Some(e) = next {
            item.entry = Some(e);
            self.heap.push(item);
          }
        }
        Err(e) => {
          self.failed = true;
          return Err(e);
        }
      }
      if self.last_key.as_deref() == Some(key.as_slice()) {
        continue;
      }
      self.last_key = Some(key.clone());
      if let Some(v) = value {
        return Ok(Some((key, v)));
      }
    }
  }
}
