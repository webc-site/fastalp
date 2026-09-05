//! [`Partition`] trait implementation with streaming iterators.

use std::{ops::Bound, sync::Arc};

use wedb_embed_engine::{KvEntry, Partition};

use crate::{
  engine::Inner,
  error::{Error, Result},
  journal::Op,
  scan::{BackMerge, Bounds, FwdMerge, Snap, snap_from},
  state::ReadGuard,
};

/// Partition handle: identifies a keyspace by name and shares engine state.
#[derive(Clone)]
pub struct ObjectLsmPartition {
  pub name: String,
  pub(crate) inner: Arc<Inner>,
}

/// Owned key-value entry produced by iterators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectLsmEntry {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
}

impl KvEntry for ObjectLsmEntry {
  type Key = Vec<u8>;
  type Value = Vec<u8>;

  fn key(&self) -> &Self::Key {
    &self.key
  }

  fn value(&self) -> &Self::Value {
    &self.value
  }
}

/// Ordered iterator over a partition snapshot.
///
/// Pure `next()` streams through a forward K-way merge and pure `next_back()`
/// streams backward the same way. When the two directions are mixed, both
/// streaming cursors are advanced independently and guarded by the two
/// delivered-key watermarks below, so the iterator never materializes the
/// remaining result set.
pub struct ObjectLsmIter {
  inner: Arc<Inner>,
  part: String,
  bounds: Bounds,
  fwd: Option<FwdMerge>,
  back: Option<BackMerge>,
  /// Consistent partition snapshot cloned under the owned read guard below.
  snap: Snap,
  /// Owned partition read guard kept for the iterator's lifetime: compaction /
  /// clear / rm must wait for an active scan before deleting segment objects.
  _gate: ReadGuard,
  /// Largest key already delivered by `next()` (the forward low-water mark).
  front_watermark: Option<Vec<u8>>,
  /// Smallest key already delivered by `next_back()` (the backward high-water mark).
  back_watermark: Option<Vec<u8>>,
  err: Option<Error>,
}

impl ObjectLsmIter {
  fn new(inner: Arc<Inner>, part: String, bounds: Bounds) -> Self {
    // Owned reader token blocks segment-object deletion for the iterator's
    // whole lifetime; the snapshot is cloned under a brief partition read lock.
    let gate = inner.readers.enter();
    let prefix = inner.state.read().cfg.prefix.clone();
    let snap = match inner.partitions.get(&part) {
      Some(lock) => {
        let guard = lock.read();
        snap_from(prefix, &guard, &bounds)
      }
      None => Snap {
        prefix,
        mem: Vec::new(),
        segments: Vec::new(),
      },
    };
    Self {
      inner,
      part,
      bounds,
      snap,
      _gate: gate,
      fwd: None,
      back: None,
      front_watermark: None,
      back_watermark: None,
      err: None,
    }
  }

  fn ensure_fwd(&mut self) -> Result<()> {
    if self.fwd.is_none() {
      self.fwd = Some(FwdMerge::new(
        self.inner.clone(),
        self.part.clone(),
        self.bounds.clone(),
        self.snap.clone(),
      )?);
    }
    Ok(())
  }

  fn ensure_back(&mut self) -> Result<()> {
    if self.back.is_none() {
      self.back = Some(BackMerge::new(
        self.inner.clone(),
        self.part.clone(),
        self.bounds.clone(),
        self.snap.clone(),
      )?);
    }
    Ok(())
  }
}

/// Lexicographic successor of `prefix`: the exclusive upper bound of the key
/// range that starts with `prefix`. Trailing `0xFF` bytes carry correctly, and
/// an all-`0xFF` prefix has no successor (the upper bound is unbounded, which
/// is still exact because every larger key shares the prefix).
fn prefix_successor(prefix: &[u8]) -> Bound<Vec<u8>> {
  let mut end = prefix.to_vec();
  while let Some(last) = end.last_mut() {
    if *last == u8::MAX {
      end.pop();
    } else {
      *last += 1;
      return Bound::Excluded(end);
    }
  }
  Bound::Unbounded
}

fn bound_owned(b: Bound<&[u8]>) -> Bound<Vec<u8>> {
  match b {
    Bound::Included(x) => Bound::Included(x.to_vec()),
    Bound::Excluded(x) => Bound::Excluded(x.to_vec()),
    Bound::Unbounded => Bound::Unbounded,
  }
}

impl Iterator for ObjectLsmIter {
  type Item = Result<ObjectLsmEntry>;

  fn next(&mut self) -> Option<Self::Item> {
    if let Some(e) = self.err.take() {
      return Some(Err(e));
    }
    if let Err(e) = self.ensure_fwd() {
      self.err = Some(e.clone());
      return Some(Err(e));
    }
    loop {
      match self.fwd.as_mut().unwrap().next() {
        Ok(Some((key, value))) => {
          // The backward cursor has already delivered every live key greater
          // than or equal to its smallest delivered key.
          if let Some(hi) = &self.back_watermark
            && key.as_slice() >= hi.as_slice()
          {
            continue;
          }
          self.front_watermark = Some(key.clone());
          return Some(Ok(ObjectLsmEntry { key, value }));
        }
        Ok(None) => return None,
        Err(e) => {
          self.err = Some(e.clone());
          return Some(Err(e));
        }
      }
    }
  }
}

impl DoubleEndedIterator for ObjectLsmIter {
  fn next_back(&mut self) -> Option<Self::Item> {
    if let Some(e) = self.err.take() {
      return Some(Err(e));
    }
    if let Err(e) = self.ensure_back() {
      self.err = Some(e.clone());
      return Some(Err(e));
    }
    loop {
      match self.back.as_mut().unwrap().next() {
        Ok(Some((key, value))) => {
          // The forward cursor has already delivered every live key less than
          // or equal to its largest delivered key.
          if let Some(lo) = &self.front_watermark
            && key.as_slice() <= lo.as_slice()
          {
            continue;
          }
          self.back_watermark = Some(key.clone());
          return Some(Ok(ObjectLsmEntry { key, value }));
        }
        Ok(None) => return None,
        Err(e) => {
          self.err = Some(e.clone());
          return Some(Err(e));
        }
      }
    }
  }
}

impl ObjectLsmPartition {
  fn stream(&self, lower: Bound<&[u8]>, upper: Bound<&[u8]>) -> ObjectLsmIter {
    ObjectLsmIter::new(
      self.inner.clone(),
      self.name.clone(),
      Bounds {
        lower: bound_owned(lower),
        upper: bound_owned(upper),
      },
    )
  }
}

impl Partition for ObjectLsmPartition {
  type Error = Error;
  type Value = Vec<u8>;
  type Entry<'a> = ObjectLsmEntry;
  type Iter<'a> = ObjectLsmIter;

  fn get(&self, key: &[u8]) -> Result<Option<Self::Value>> {
    self.inner.lookup(&self.name, key)
  }

  fn insert(&self, key: &[u8], value: &[u8]) -> Result<()> {
    self.inner.commit_ops(vec![Op::put(
      self.name.clone(),
      key.to_vec(),
      value.to_vec(),
    )])
  }

  fn rm(&self, key: &[u8]) -> Result<()> {
    self
      .inner
      .commit_ops(vec![Op::delete(self.name.clone(), key.to_vec())])
  }

  fn clear(&self) -> Result<()> {
    self.inner.clear_partition(&self.name)
  }

  fn iter(&self) -> Self::Iter<'_> {
    self.stream(Bound::Unbounded, Bound::Unbounded)
  }

  fn prefix(&self, prefix: &[u8]) -> Self::Iter<'_> {
    let upper = prefix_successor(prefix);
    let upper = match &upper {
      Bound::Included(v) => Bound::Included(v.as_slice()),
      Bound::Excluded(v) => Bound::Excluded(v.as_slice()),
      Bound::Unbounded => Bound::Unbounded,
    };
    self.stream(Bound::Included(prefix), upper)
  }

  fn range(&self, range: (Bound<&[u8]>, Bound<&[u8]>)) -> Self::Iter<'_> {
    self.stream(range.0, range.1)
  }

  fn approximate_len(&self) -> Result<usize> {
    let Some(lock) = self.inner.partition_lock(&self.name) else {
      return Ok(0);
    };
    let ps = lock.read();
    // O(#segments + memtable), never a full partition scan: live values in the
    // memtable plus per-segment raw entries minus tombstones (duplicates across
    // segments intentionally overcount, matching an approximate LSM count).
    let seg: usize = ps
      .meta
      .segments
      .iter()
      .map(|s| (s.count - s.tombstones) as usize)
      .sum();
    let mem = ps
      .mem
      .values()
      .filter(|e| matches!(e, crate::state::MemEntry::Value(_)))
      .count();
    Ok(seg + mem)
  }

  fn table_count(&self) -> usize {
    self
      .inner
      .partition_lock(&self.name)
      .map(|lock| lock.read().meta.segments.len())
      .unwrap_or(0)
  }

  fn disk_space(&self) -> Result<u64> {
    Ok(
      self
        .inner
        .partition_lock(&self.name)
        .map(|lock| {
          lock
            .read()
            .meta
            .segments
            .iter()
            .map(|s| s.bytes)
            .sum::<u64>()
        })
        .unwrap_or(0),
    )
  }

  fn compact(&self) -> Result<()> {
    self.inner.compact_partition(&self.name)
  }
}
