//! In-memory engine state (partitions + counters).

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use parking_lot::RwLock;

use crate::{config::Config, manifest::PartitionMeta};

/// A value in a partition memtable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemEntry {
  Value(Vec<u8>),
  Tombstone,
}

impl MemEntry {
  /// Estimated contribution of an entry to the memtable byte budget.
  pub fn contrib(key_len: usize, e: Option<&MemEntry>) -> u64 {
    let overhead = 48u64;
    match e {
      Some(MemEntry::Value(v)) => overhead + key_len as u64 + v.len() as u64,
      Some(MemEntry::Tombstone) => overhead + key_len as u64 + 8,
      None => 0,
    }
  }
}

/// Runtime state of a single partition.
///
/// The memtable is guarded by this partition's own lock so different
/// partitions can be read/written concurrently. Durable metadata is mirrored
/// into [`EngineState::partitions`] whenever it changes; that mirror is what
/// manifest publishing and global GC read under the global lock.
#[derive(Debug)]
pub struct PartitionState {
  pub name: String,
  /// Newest layer: unflushed entries (values and tombstones).
  pub mem: BTreeMap<Vec<u8>, MemEntry>,
  pub mem_bytes: u64,
  /// Flushed immutable segments + durable metadata.
  pub meta: PartitionMeta,
  /// A detached compaction is in flight for this partition.
  pub compacting: bool,
}

impl PartitionState {
  pub fn new(name: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      mem: BTreeMap::new(),
      mem_bytes: 0,
      meta: PartitionMeta::default(),
      compacting: false,
    }
  }

  /// Apply one mutation to the memtable, maintaining the byte budget.
  pub fn apply(&mut self, key: &[u8], value: Option<&[u8]>) {
    let prev_contrib = MemEntry::contrib(key.len(), self.mem.get(key));
    let new = match value {
      Some(v) => MemEntry::Value(v.to_vec()),
      None => MemEntry::Tombstone,
    };
    self.mem.insert(key.to_vec(), new);
    let new_contrib = MemEntry::contrib(key.len(), self.mem.get(key));
    self.mem_bytes = self
      .mem_bytes
      .saturating_add(new_contrib)
      .saturating_sub(prev_contrib);
  }
}

/// Shared, independently lockable partition state.
pub type PartitionLock = Arc<RwLock<PartitionState>>;

/// Reader/writer gate guarding segment-object deletion against in-flight
/// readers. `ReadGuard` is owned (no borrow), so an iterator can hold it for
/// its whole lifetime; `WriteGuard` is acquired only while deleting old
/// segment objects and waits for every active reader to drain first.
pub struct ReaderGate {
  inner: Arc<GateInner>,
}

struct GateInner {
  mu: parking_lot::Mutex<GateState>,
  cv: parking_lot::Condvar,
}

impl Default for GateInner {
  fn default() -> Self {
    Self {
      mu: parking_lot::Mutex::new(GateState::default()),
      cv: parking_lot::Condvar::new(),
    }
  }
}

#[derive(Default)]
struct GateState {
  readers: usize,
  writer: bool,
}

impl Default for ReaderGate {
  fn default() -> Self {
    Self {
      inner: Arc::new(GateInner::default()),
    }
  }
}

pub struct ReadGuard {
  inner: Arc<GateInner>,
}

pub struct WriteGuard {
  inner: Arc<GateInner>,
}

impl ReaderGate {
  pub fn enter(&self) -> ReadGuard {
    let inner = self.inner.clone();
    let mut g = inner.mu.lock();
    while g.writer {
      inner.cv.wait(&mut g);
    }
    g.readers += 1;
    drop(g);
    ReadGuard { inner }
  }

  pub fn exclusive(&self) -> WriteGuard {
    let inner = self.inner.clone();
    let mut g = inner.mu.lock();
    g.writer = true;
    inner.cv.notify_all();
    while g.readers > 0 {
      inner.cv.wait(&mut g);
    }
    drop(g);
    WriteGuard { inner }
  }
}

impl Drop for ReadGuard {
  fn drop(&mut self) {
    let notify = {
      let mut g = self.inner.mu.lock();
      g.readers = g.readers.saturating_sub(1);
      g.readers == 0
    };
    if notify {
      self.inner.cv.notify_all();
    }
  }
}

impl Drop for WriteGuard {
  fn drop(&mut self) {
    {
      let mut g = self.inner.mu.lock();
      g.writer = false;
    }
    self.inner.cv.notify_all();
  }
}

/// Lock table for the per-partition data plane.
///
/// This lives *outside* [`EngineState`]'s global lock: acquiring a partition
/// lock never blocks on a writer holding the global metadata lock, which is
/// what lets reads/writes to unrelated partitions proceed while one partition
/// flushes or compacts.
#[derive(Debug, Default)]
pub struct PartitionTable {
  map: RwLock<BTreeMap<String, PartitionLock>>,
}

impl PartitionTable {
  /// Return a clone of the lock for `name`, if it exists.
  pub fn get(&self, name: &str) -> Option<PartitionLock> {
    self.map.read().get(name).cloned()
  }

  /// Return (and create, if missing) the lock for `name`.
  pub fn create(&self, name: &str) -> PartitionLock {
    if let Some(lock) = self.get(name) {
      return lock;
    }
    let lock = Arc::new(RwLock::new(PartitionState::new(name.to_string())));
    let mut map = self.map.write();
    map
      .entry(name.to_string())
      .or_insert_with(|| lock.clone())
      .clone()
  }

  /// Snapshot every lock currently in the table.
  pub fn snapshot(&self) -> Vec<PartitionLock> {
    self.map.read().values().cloned().collect()
  }

  /// All partition names currently in the table.
  pub fn names(&self) -> Vec<String> {
    self.map.read().keys().cloned().collect()
  }
}

/// A detached memtable snapshot being uploaded by a background flush worker.
#[derive(Debug, Clone)]
pub struct PendingFlush {
  pub partition: String,
  pub watermark: u64,
  pub segment_id: u64,
  pub encoded: Vec<u8>,
  pub entries: Vec<(Vec<u8>, Option<Vec<u8>>)>,
}

/// Whole-engine state guarded by a single read/write lock.
///
/// Only global metadata lives here. `partitions` is the durable/manifest view;
/// per-partition memtables are mutated while holding the corresponding
/// [`PartitionTable`] lock, never while holding this global lock.
#[derive(Debug)]
pub struct EngineState {
  pub cfg: Config,
  /// Durable per-partition metadata used by manifest publishing and GC.
  pub partitions: BTreeMap<String, PartitionMeta>,
  /// Last assigned journal group seq (0 = none).
  pub journal_seq: u64,
  /// Last manifest seq written (0 = none).
  pub manifest_seq: u64,
  pub next_segment_id: u64,
  /// Journal object end-seqs whose objects still exist (maintained for GC).
  pub journal_seqs: BTreeSet<u64>,
  /// Byte size of each live journal object (end seq -> bytes).
  pub journal_sizes: BTreeMap<u64, u64>,
  /// Byte size of the latest manifest snapshot.
  pub manifest_bytes: u64,
  /// Encoded groups waiting for group-commit flush (windowed mode).
  pub pending: Vec<u8>,
  /// First seq present in `pending` (0 when empty).
  pub pending_lo: u64,
  /// A flusher is currently uploading the detached pending buffer.
  pub journal_flushing: bool,
  /// Detached memtable snapshots uploaded by background flush workers.
  pub pending_flushes: BTreeMap<String, PendingFlush>,
  /// Completed merge compactions (metrics).
  pub compactions_completed: u64,
  /// Fencing epoch for leased engines (0 = unfenced).
  pub fence_epoch: u128,
  /// Raw bytes of the current manifest pointer as last seen/published.
  pub current_bytes: Option<Vec<u8>>,
  /// Min partition watermark at the time of the LAST SUCCESSFUL manifest
  /// publish. Journal GC only deletes objects at/below this bound, so a flush
  /// whose manifest publish failed can never have its (still-only-durable)
  /// journal objects garbage-collected.
  pub published_min_wm: u64,
}

impl EngineState {
  pub fn new(cfg: Config) -> Self {
    Self {
      cfg,
      partitions: BTreeMap::new(),
      journal_seq: 0,
      manifest_seq: 0,
      next_segment_id: 0,
      journal_seqs: BTreeSet::new(),
      journal_sizes: BTreeMap::new(),
      manifest_bytes: 0,
      compactions_completed: 0,
      pending: Vec::new(),
      pending_lo: 0,
      journal_flushing: false,
      pending_flushes: BTreeMap::new(),
      fence_epoch: 0,
      current_bytes: None,
      published_min_wm: 0,
    }
  }

  /// Ensure a durable metadata mirror entry exists for `name`.
  pub fn ensure_meta(&mut self, name: &str) {
    self.partitions.entry(name.to_string()).or_default();
  }
}
