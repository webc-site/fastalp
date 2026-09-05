//! Block cache + segment-index cache.
//!
//! The block cache keeps decoded block entry lists in memory with
//! insertion-order eviction (a cheap stand-in for LRU, M2). Segment indexes are
//! cached separately because they are tiny and hot.

use std::{
  collections::{HashMap, VecDeque},
  sync::{Arc, Mutex},
};

use crate::segment::{SegmentEntries, SegmentIndex};

struct CachedBlock {
  entries: Arc<SegmentEntries>,
  size: u64,
}

struct BlockCacheInner {
  capacity: u64,
  used: u64,
  map: HashMap<(u64, u32), CachedBlock>,
  order: VecDeque<(u64, u32)>,
}

/// Bounded cache of decoded segment blocks, keyed by `(segment_id, block_idx)`.
#[derive(Clone)]
pub struct BlockCache {
  inner: Arc<Mutex<BlockCacheInner>>,
}

impl BlockCache {
  pub fn new(capacity: u64) -> Self {
    Self {
      inner: Arc::new(Mutex::new(BlockCacheInner {
        capacity,
        used: 0,
        map: HashMap::new(),
        order: VecDeque::new(),
      })),
    }
  }

  pub fn get(&self, seg: u64, block: u32) -> Option<Arc<SegmentEntries>> {
    self
      .inner
      .lock()
      .unwrap()
      .map
      .get(&(seg, block))
      .map(|c| c.entries.clone())
  }

  pub fn insert(&self, seg: u64, block: u32, entries: Arc<SegmentEntries>, size: u64) {
    let mut g = self.inner.lock().unwrap();
    if g.capacity == 0 {
      return;
    }
    // Refresh in place when the key already exists so `order` and `used` stay
    // consistent (no duplicate FIFO entries, no double accounting).
    if let Some(old_size) = g.map.get(&(seg, block)).map(|c| c.size) {
      g.used = g.used.saturating_sub(old_size).saturating_add(size);
      let existing = g.map.get_mut(&(seg, block)).expect("cached block present");
      existing.entries = entries;
      existing.size = size;
      return;
    }
    g.map.insert((seg, block), CachedBlock { entries, size });
    g.used += size;
    g.order.push_back((seg, block));
    while g.used > g.capacity {
      let Some(key) = g.order.pop_front() else {
        break;
      };
      if let Some(removed) = g.map.remove(&key) {
        g.used = g.used.saturating_sub(removed.size);
      }
    }
  }

  pub fn used(&self) -> u64 {
    self.inner.lock().unwrap().used
  }

  pub fn capacity(&self) -> u64 {
    self.inner.lock().unwrap().capacity
  }
}

struct IndexCacheInner {
  capacity: usize,
  map: HashMap<u64, Arc<SegmentIndex>>,
  order: VecDeque<u64>,
}

/// FIFO-bounded cache of parsed segment block indexes (small, hot metadata).
#[derive(Clone)]
pub struct IndexCache {
  inner: Arc<Mutex<IndexCacheInner>>,
}

impl Default for IndexCache {
  fn default() -> Self {
    Self::new(4096)
  }
}

impl IndexCache {
  pub fn new(capacity: usize) -> Self {
    Self {
      inner: Arc::new(Mutex::new(IndexCacheInner {
        capacity,
        map: HashMap::new(),
        order: VecDeque::new(),
      })),
    }
  }

  pub fn get(&self, seg: u64) -> Option<Arc<SegmentIndex>> {
    self.inner.lock().unwrap().map.get(&seg).cloned()
  }

  pub fn insert(&self, seg: u64, index: Arc<SegmentIndex>) {
    let mut g = self.inner.lock().unwrap();
    if g.map.insert(seg, index).is_none() {
      g.order.push_back(seg);
    }
    while g.order.len() > g.capacity {
      if let Some(old) = g.order.pop_front() {
        g.map.remove(&old);
      }
    }
  }

  pub fn remove(&self, seg: u64) {
    let mut g = self.inner.lock().unwrap();
    g.map.remove(&seg);
    g.order.retain(|k| *k != seg);
  }
}
