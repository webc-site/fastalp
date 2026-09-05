//! Fault-injection and concurrency regression tests for crash-consistency and
//! reader/deletion safety fixes.

use std::{
  sync::{Arc, Barrier},
  thread,
};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, MemoryStore, ObjectLsm, Result, Store, keys::segment_root};

/// Store that fails every `delete` (used to prove clear/rm publish the manifest
/// before deleting, so a delete failure only leaves orphan objects).
struct FailingDeleteStore {
  inner: MemoryStore,
}

impl Store for FailingDeleteStore {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    self.inner.get(key)
  }
  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
    self.inner.get_range(key, offset, len)
  }
  fn put(&self, key: &str, data: &[u8]) -> Result<()> {
    self.inner.put(key, data)
  }
  fn delete(&self, _key: &str) -> Result<()> {
    Err(wedb_object_lsm::Error::store("injected delete failure"))
  }
  fn list(&self, prefix: &str) -> Result<Vec<String>> {
    self.inner.list(prefix)
  }
}

/// Store that fails every segment `put` (used to prove compaction keeps the old
/// run intact when the merged upload fails).
struct FailingSegmentPutStore {
  inner: MemoryStore,
}

impl Store for FailingSegmentPutStore {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    self.inner.get(key)
  }
  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
    self.inner.get_range(key, offset, len)
  }
  fn put(&self, key: &str, data: &[u8]) -> Result<()> {
    if key.contains("/seg/") {
      return Err(wedb_object_lsm::Error::store(
        "injected segment put failure",
      ));
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
fn clear_publishes_manifest_even_when_delete_fails() {
  let inner = MemoryStore::new();
  let store = FailingDeleteStore {
    inner: inner.clone(),
  };
  let cfg = Config::new("inj/clear")
    .max_memtable_bytes(40)
    .max_segments_before_compact(1_000_000);
  let db = ObjectLsm::open(Arc::new(store), cfg).unwrap();
  let p = db.partition("data").unwrap();
  for i in 0..20u32 {
    p.insert(format!("k{i:03}").as_bytes(), b"v").unwrap();
  }
  assert!(p.table_count() >= 2);
  // clear publishes the cleared manifest first; the injected delete failures
  // are ignored, leaving orphan segment objects behind.
  p.clear().unwrap();
  assert!(p.is_empty().unwrap());
  assert!(
    !inner.list(&segment_root("inj/clear")).unwrap().is_empty(),
    "orphans must remain when delete fails"
  );
  drop(p);
  drop(db);
}

#[test]
fn compaction_upload_failure_keeps_old_run() {
  let base = MemoryStore::new();
  // Build several segments with a normal store first.
  {
    let cfg = Config::new("inj/compact")
      .max_memtable_bytes(40)
      .max_segments_before_compact(1_000_000);
    let db = ObjectLsm::open(Arc::new(base.clone()), cfg).unwrap();
    let p = db.partition("data").unwrap();
    for i in 0..20u32 {
      p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
        .unwrap();
    }
    assert!(p.table_count() > 1);
  }

  // Reopen through a store whose segment PUT always fails.
  let failing = FailingSegmentPutStore {
    inner: base.clone(),
  };
  let cfg = Config::new("inj/compact")
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000);
  let db = ObjectLsm::open(Arc::new(failing), cfg).unwrap();
  let p = db.partition("data").unwrap();
  assert!(db.compact().is_err(), "merged segment upload should fail");
  // The old run must remain readable (compaction uploads before swapping).
  assert_eq!(p.get(b"k007").unwrap().unwrap(), b"v7");
  assert_eq!(p.len().unwrap(), 20);
}

#[test]
fn concurrent_reads_during_compaction_never_error() {
  let store = MemoryStore::new();
  let cfg = Config::new("inj/conc")
    .max_memtable_bytes(1024)
    .block_size(128)
    .max_segments_before_compact(4);
  let db = ObjectLsm::open(Arc::new(store), cfg).unwrap();
  let p = db.partition("data").unwrap();
  let writer_p = p.clone();
  let reader_p = p.clone();
  let barrier = Arc::new(Barrier::new(3));
  let b1 = barrier.clone();
  let b2 = barrier.clone();

  let writer = thread::spawn(move || {
    b1.wait();
    for i in 0..400u32 {
      writer_p
        .insert(
          format!("k{i:04}").as_bytes(),
          format!("value-{i}").as_bytes(),
        )
        .unwrap();
    }
  });

  let reader = thread::spawn(move || {
    b2.wait();
    for round in 0..2000u32 {
      let key = format!("k{:04}", round % 400).into_bytes();
      let _ = reader_p.get(&key).expect("point read must not error");
      let mut n = 0;
      for e in reader_p.iter() {
        e.expect("scan must not error");
        n += 1;
        if n >= 40 {
          break;
        }
      }
    }
  });

  barrier.wait();
  writer.join().unwrap();
  reader.join().unwrap();
}
