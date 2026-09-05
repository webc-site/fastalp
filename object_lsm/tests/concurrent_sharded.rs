//! Concurrency regression tests for the sharded partition data plane.

use std::{
  sync::{Arc, Barrier},
  thread,
  time::Duration,
};

use wedb_embed_engine::{Batch, Engine, Partition};
use wedb_object_lsm::{Config, MemoryStore, ObjectLsm, Result, Store};

#[derive(Clone)]
struct BlockingJournalStore {
  inner: MemoryStore,
  entered: Arc<Barrier>,
  release: Arc<Barrier>,
}

impl Store for BlockingJournalStore {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    self.inner.get(key)
  }

  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
    self.inner.get_range(key, offset, len)
  }

  fn put(&self, key: &str, data: &[u8]) -> Result<()> {
    if key.contains("/journal/") {
      // Hold the partition locks while the journal PUT is in flight. This lets
      // the test observe whether a concurrent reader can see a half-applied
      // cross-partition batch.
      self.entered.wait();
      self.release.wait();
    }
    self.inner.put(key, data)
  }

  fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key)
  }

  fn list(&self, prefix: &str) -> Result<Vec<String>> {
    self.inner.list(prefix)
  }
}

#[test]
fn cross_partition_batch_blocks_readers_until_fully_applied() {
  let entered = Arc::new(Barrier::new(2));
  let release = Arc::new(Barrier::new(2));
  let store = BlockingJournalStore {
    inner: MemoryStore::new(),
    entered: entered.clone(),
    release: release.clone(),
  };
  let cfg = Config::new("conc/atomic")
    .max_memtable_bytes(16 * 1024 * 1024)
    .max_segments_before_compact(1_000_000);
  let db = Arc::new(ObjectLsm::open(Arc::new(store), cfg).unwrap());
  let a = db.partition("a").unwrap();
  let b = db.partition("b").unwrap();

  let writer_db = db.clone();
  let writer_a = a.clone();
  let writer_b = b.clone();
  let writer = thread::spawn(move || {
    let mut batch = writer_db.batch();
    batch.insert(&writer_a, b"atomic", b"1");
    batch.insert(&writer_b, b"atomic", b"1");
    batch.commit().unwrap();
  });

  // The writer has allocated its journal seq and is now blocked inside the
  // journal PUT while holding both partition write locks.
  entered.wait();

  let reader_a = a.clone();
  let reader_b = b.clone();
  let reader = thread::spawn(move || {
    (
      reader_a.get(b"atomic").unwrap(),
      reader_b.get(b"atomic").unwrap(),
    )
  });

  // Give the reader time to hit the partition locks before releasing the
  // journal PUT and allowing the batch to become visible.
  thread::sleep(Duration::from_millis(100));
  release.wait();

  writer.join().unwrap();
  let (va, vb) = reader.join().unwrap();
  assert_eq!(va.as_deref(), Some(&b"1"[..]));
  assert_eq!(vb.as_deref(), Some(&b"1"[..]));
  assert_eq!(
    va, vb,
    "cross-partition batch must never expose a half state"
  );
}

#[test]
fn concurrent_writes_to_distinct_partitions_do_not_deadlock() {
  let store = MemoryStore::new();
  let cfg = Config::new("conc/sharded")
    .max_memtable_bytes(16 * 1024 * 1024)
    .max_segments_before_compact(1_000_000);
  let db = Arc::new(ObjectLsm::open(Arc::new(store), cfg).unwrap());

  const PARTITIONS: usize = 8;
  const WRITES: usize = 500;
  let barrier = Arc::new(Barrier::new(PARTITIONS));
  let mut handles = Vec::new();
  for i in 0..PARTITIONS {
    let db = db.clone();
    let barrier = barrier.clone();
    handles.push(thread::spawn(move || {
      let p = db.partition(&format!("p{i}")).unwrap();
      barrier.wait();
      for j in 0..WRITES {
        let key = format!("k{j:04}");
        let val = format!("p{i}-{j}");
        p.insert(key.as_bytes(), val.as_bytes()).unwrap();
      }
    }));
  }

  for handle in handles {
    handle.join().unwrap();
  }

  for i in 0..PARTITIONS {
    let p = db.partition(&format!("p{i}")).unwrap();
    assert_eq!(p.len().unwrap(), WRITES);
    assert_eq!(
      p.get(format!("k{:04}", WRITES - 1).as_bytes()).unwrap(),
      Some(format!("p{i}-{}", WRITES - 1).into_bytes())
    );
  }
}
