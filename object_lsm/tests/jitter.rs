//! Deterministic network-jitter regression: intermittent object-store PUT
//! failures (1 in N) must never cause an acknowledged write to be lost; failed
//! commits stay invisible and are retried; reopen recovers every acked key.

use std::sync::{
  Arc,
  atomic::{AtomicUsize, Ordering},
};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, MemoryStore, ObjectLsm, Result, Store};

/// Fails every `fail_every`-th PUT once (transient). Reads/listing pass through.
#[derive(Clone)]
struct JitterStore {
  inner: MemoryStore,
  calls: Arc<AtomicUsize>,
  fail_every: usize,
}

impl Store for JitterStore {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    self.inner.get(key)
  }
  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
    self.inner.get_range(key, offset, len)
  }
  fn put(&self, key: &str, data: &[u8]) -> Result<()> {
    let i = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
    if i.is_multiple_of(self.fail_every) {
      return Err(wedb_object_lsm::Error::store(
        "injected transient PUT failure",
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
fn intermittent_put_failures_never_lose_acked_writes() {
  let store = JitterStore {
    inner: MemoryStore::new(),
    calls: Arc::new(AtomicUsize::new(0)),
    fail_every: 5,
  };
  let cfg = Config::new("jit/1")
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000);
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg.clone()).unwrap();
  let p = db.partition("data").unwrap();

  // Strict-mode commits: a transient journal PUT returns Err (not acked); the
  // caller retries until success.
  let mut injected = 0usize;
  for i in 0..200u32 {
    let key = format!("k{i:03}");
    let value = format!("v{i}");
    loop {
      match p.insert(key.as_bytes(), value.as_bytes()) {
        Ok(()) => break,
        Err(_) => injected += 1,
      }
    }
  }
  assert!(
    injected > 0,
    "jitter must have injected at least one failure"
  );
  assert_eq!(p.len().unwrap(), 200, "every acked write is visible");

  drop(p);
  drop(db);

  let db2 = ObjectLsm::open(Arc::new(store), cfg).unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 200, "reopen recovers every acked write");
  for i in (0..200u32).step_by(13) {
    assert_eq!(
      p2.get(format!("k{i:03}").as_bytes()).unwrap().unwrap(),
      format!("v{i}").into_bytes()
    );
  }
}
