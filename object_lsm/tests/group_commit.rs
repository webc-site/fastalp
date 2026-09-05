//! Group-commit journal batching tests (Config::journal_window_ms).

use std::{
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  thread,
  time::{Duration, Instant},
};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{
  Config, MemoryStore, ObjectLsm, Store, journal::decode_group_stream, keys::journal_prefix,
};

fn window_cfg(prefix: &str, window_ms: u64) -> Config {
  Config::new(prefix)
    .max_memtable_bytes(1 << 20)
    .block_size(1024)
    .journal_window_ms(Some(window_ms))
}

#[test]
fn grouped_window_batches_many_commits_into_one_object() {
  let store = MemoryStore::new();
  let cfg = window_cfg("gc/batch", 60_000); // long window: no auto flush
  let eng = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  let p = eng.partition("data").unwrap();
  for i in 0..50u32 {
    p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  // Nothing flushed yet (window is long).
  assert!(store.list(&journal_prefix("gc/batch")).unwrap().is_empty());
  // persist() forces one synchronous journal object for all 50 groups.
  eng.persist().unwrap();
  let objs = store.list(&journal_prefix("gc/batch")).unwrap();
  assert_eq!(
    objs.len(),
    1,
    "expected 1 batched journal object, got {objs:?}"
  );
  let bytes = store.get(&objs[0]).unwrap().unwrap();
  let groups = decode_group_stream(&bytes).unwrap();
  assert_eq!(groups.len(), 50);
  assert_eq!(groups[0].seq, 1);
  assert_eq!(groups[49].seq, 50);

  drop(p);
  drop(eng);
  let eng2 = ObjectLsm::open(Arc::new(store.clone()), window_cfg("gc/batch", 60_000)).unwrap();
  let p2 = eng2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 50);
  assert_eq!(p2.get(b"k049").unwrap().unwrap(), b"v49");
}

#[test]
fn background_flusher_persists_without_persist() {
  let store = MemoryStore::new();
  let cfg = window_cfg("gc/auto", 40);
  let eng = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  let p = eng.partition("data").unwrap();
  for i in 0..20u32 {
    p.insert(format!("k{i:03}").as_bytes(), b"v").unwrap();
  }
  // Wait past the flush window; the background thread must have flushed.
  thread::sleep(Duration::from_millis(200));
  let objs = store.list(&journal_prefix("gc/auto")).unwrap();
  assert!(
    !objs.is_empty(),
    "background flusher should have written journal objects"
  );
  drop(p);
  drop(eng);

  let eng2 = ObjectLsm::open(Arc::new(store.clone()), window_cfg("gc/auto", 40)).unwrap();
  let p2 = eng2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 20);
}

#[test]
fn grouped_journal_survives_memtable_flush_and_reopen() {
  let store = MemoryStore::new();
  let cfg = window_cfg("gc/flush", 5).max_memtable_bytes(512);
  let eng = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  let a = eng.partition("a").unwrap();
  let b = eng.partition("b").unwrap();
  for i in 0..80u32 {
    a.insert(format!("a{i:03}").as_bytes(), format!("va{i}").as_bytes())
      .unwrap();
  }
  for i in 0..10u32 {
    b.insert(format!("b{i:03}").as_bytes(), format!("vb{i}").as_bytes())
      .unwrap();
  }
  eng.persist().unwrap();
  assert!(
    a.table_count() >= 1,
    "partition a should have flushed to segments"
  );
  drop(a);
  drop(b);
  drop(eng);

  let eng2 = ObjectLsm::open(
    Arc::new(store.clone()),
    window_cfg("gc/flush", 5).max_memtable_bytes(512),
  )
  .unwrap();
  let a2 = eng2.partition("a").unwrap();
  let b2 = eng2.partition("b").unwrap();
  assert_eq!(a2.len().unwrap(), 80);
  assert_eq!(b2.len().unwrap(), 10);
  assert_eq!(a2.get(b"a042").unwrap().unwrap(), b"va42");
  assert_eq!(b2.get(b"b007").unwrap().unwrap(), b"vb7");
}

#[test]
fn strict_mode_keeps_one_object_per_commit() {
  let store = MemoryStore::new();
  let cfg = Config::new("gc/strict")
    .max_memtable_bytes(1 << 20)
    .journal_window_ms(None);
  let eng = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  let p = eng.partition("data").unwrap();
  for i in 0..5u32 {
    p.insert(format!("k{i}").as_bytes(), b"v").unwrap();
  }
  let objs = store.list(&journal_prefix("gc/strict")).unwrap();
  assert_eq!(
    objs.len(),
    5,
    "strict mode writes one durable object per commit"
  );
}

#[test]
fn background_flush_upload_does_not_hold_state_lock() {
  struct BlockingPutStore {
    inner: MemoryStore,
    uploads: AtomicUsize,
  }

  impl Store for BlockingPutStore {
    fn get(&self, key: &str) -> wedb_object_lsm::Result<Option<Vec<u8>>> {
      self.inner.get(key)
    }

    fn get_range(
      &self,
      key: &str,
      offset: u64,
      len: u64,
    ) -> wedb_object_lsm::Result<Option<Vec<u8>>> {
      self.inner.get_range(key, offset, len)
    }

    fn put(&self, key: &str, data: &[u8]) -> wedb_object_lsm::Result<()> {
      self.uploads.fetch_add(1, Ordering::SeqCst);
      thread::sleep(Duration::from_millis(300));
      self.inner.put(key, data)
    }

    fn delete(&self, key: &str) -> wedb_object_lsm::Result<()> {
      self.inner.delete(key)
    }

    fn list(&self, prefix: &str) -> wedb_object_lsm::Result<Vec<String>> {
      self.inner.list(prefix)
    }
  }

  let store = Arc::new(BlockingPutStore {
    inner: MemoryStore::new(),
    uploads: AtomicUsize::new(0),
  });
  let cfg = window_cfg("gc/nonblocking", 5);
  let eng = ObjectLsm::open(store.clone(), cfg).unwrap();
  let p = eng.partition("data").unwrap();
  p.insert(b"k1", b"v1").unwrap();

  // Wait until the background flusher is inside its remote PUT.
  while store.uploads.load(Ordering::SeqCst) == 0 {
    thread::sleep(Duration::from_millis(5));
  }
  // This commit must not wait behind the detached journal PUT.
  let started = Instant::now();
  p.insert(b"k2", b"v2").unwrap();
  assert!(
    started.elapsed() < Duration::from_millis(200),
    "commit blocked on background journal PUT"
  );

  // Let the flush finish and force one final durable sync.
  thread::sleep(Duration::from_millis(500));
  eng.persist().unwrap();
}

#[test]
fn background_memtable_flush_does_not_block_commit() {
  struct BlockingSegmentStore {
    inner: MemoryStore,
    uploads: AtomicUsize,
  }

  impl Store for BlockingSegmentStore {
    fn get(&self, key: &str) -> wedb_object_lsm::Result<Option<Vec<u8>>> {
      self.inner.get(key)
    }

    fn get_range(
      &self,
      key: &str,
      offset: u64,
      len: u64,
    ) -> wedb_object_lsm::Result<Option<Vec<u8>>> {
      self.inner.get_range(key, offset, len)
    }

    fn put(&self, key: &str, data: &[u8]) -> wedb_object_lsm::Result<()> {
      if key.contains("/seg/") {
        self.uploads.fetch_add(1, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(300));
      }
      self.inner.put(key, data)
    }

    fn delete(&self, key: &str) -> wedb_object_lsm::Result<()> {
      self.inner.delete(key)
    }

    fn list(&self, prefix: &str) -> wedb_object_lsm::Result<Vec<String>> {
      self.inner.list(prefix)
    }
  }

  let store = Arc::new(BlockingSegmentStore {
    inner: MemoryStore::new(),
    uploads: AtomicUsize::new(0),
  });
  let cfg = Config::new("gc/bgflush")
    .max_memtable_bytes(32)
    .block_size(64)
    .background_flush(true)
    .journal_window_ms(Some(5));
  let eng = ObjectLsm::open(store.clone(), cfg).unwrap();
  let p = eng.partition("data").unwrap();
  p.insert(b"k1", b"v1").unwrap();

  while store.uploads.load(Ordering::SeqCst) == 0 {
    thread::sleep(Duration::from_millis(5));
  }
  let started = Instant::now();
  p.insert(b"k2", b"v2").unwrap();
  assert!(
    started.elapsed() < Duration::from_millis(200),
    "commit blocked on background segment upload"
  );
  // Let the blocked segment PUT finish; otherwise persist() competes for the
  // same pending snapshot on the worker thread.
  thread::sleep(Duration::from_millis(500));
  eng.persist().unwrap();
}

#[test]
fn failed_background_journal_upload_keeps_acked_writes() {
  struct FailOnceJournalStore {
    inner: MemoryStore,
    fail_once: AtomicBool,
  }
  impl Store for FailOnceJournalStore {
    fn get(&self, key: &str) -> wedb_object_lsm::Result<Option<Vec<u8>>> {
      self.inner.get(key)
    }
    fn get_range(
      &self,
      key: &str,
      offset: u64,
      len: u64,
    ) -> wedb_object_lsm::Result<Option<Vec<u8>>> {
      self.inner.get_range(key, offset, len)
    }
    fn put(&self, key: &str, data: &[u8]) -> wedb_object_lsm::Result<()> {
      if key.contains("/journal/") && self.fail_once.swap(false, Ordering::SeqCst) {
        return Err(wedb_object_lsm::Error::store("injected journal failure"));
      }
      self.inner.put(key, data)
    }
    fn delete(&self, key: &str) -> wedb_object_lsm::Result<()> {
      self.inner.delete(key)
    }
    fn list(&self, prefix: &str) -> wedb_object_lsm::Result<Vec<String>> {
      self.inner.list(prefix)
    }
  }

  let store = Arc::new(FailOnceJournalStore {
    inner: MemoryStore::new(),
    fail_once: AtomicBool::new(true),
  });
  let cfg = window_cfg("gc/failrecover", 5);
  let eng = ObjectLsm::open(store.clone(), cfg).unwrap();
  let p = eng.partition("data").unwrap();
  let n = 2000u32;
  for i in 0..n {
    p.insert(format!("k{i:05}").as_bytes(), b"v").unwrap();
  }
  // Let the first background flush fail and a later one retry with the
  // restored buffer.
  thread::sleep(Duration::from_millis(300));
  eng.persist().unwrap();
  drop(eng);
  drop(p);

  let eng2 = ObjectLsm::open(store, window_cfg("gc/failrecover", 5)).unwrap();
  let p2 = eng2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), n as usize);
  assert_eq!(p2.get(b"k01999").unwrap().unwrap(), b"v");
}
