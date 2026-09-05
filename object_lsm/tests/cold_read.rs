//! Cold-start point reads must not perform tail/index Range GETs: the block
//! index is embedded in the manifest and preloaded into the IndexCache at
//! open, so a lookup only fetches the candidate block range.

use std::sync::{Arc, Mutex};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, MemoryStore, ObjectLsm, Result, Store};

#[derive(Debug, Clone)]
struct RangeCall {
  key: String,
  offset: u64,
  len: u64,
}

struct CountingStore {
  inner: MemoryStore,
  ranges: Arc<Mutex<Vec<RangeCall>>>,
}

impl Store for CountingStore {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    self.inner.get(key)
  }

  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
    self.ranges.lock().unwrap().push(RangeCall {
      key: key.to_string(),
      offset,
      len,
    });
    self.inner.get_range(key, offset, len)
  }

  fn put(&self, key: &str, data: &[u8]) -> Result<()> {
    self.inner.put(key, data)
  }

  fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key)
  }

  fn list(&self, prefix: &str) -> Result<Vec<String>> {
    self.inner.list(prefix)
  }
}

fn key(i: u32) -> Vec<u8> {
  format!("key{i:03}").into_bytes()
}

fn val(i: u32) -> Vec<u8> {
  format!("v{i}").into_bytes()
}

#[test]
fn cold_read_after_open_only_fetches_block_range() {
  const PREFIX: &str = "t/cold-read";
  let cfg = || {
    Config::new(PREFIX)
      .max_memtable_bytes(48)
      .block_size(64)
      .max_segments_before_compact(1_000_000)
  };

  // Phase 1: create durable segments without the counting wrapper.
  let mem = MemoryStore::new();
  {
    let db = ObjectLsm::open(Arc::new(mem.clone()), cfg()).unwrap();
    let p = db.partition("data").unwrap();
    for i in 0..20 {
      p.insert(&key(i), &val(i)).unwrap();
    }
    assert!(p.table_count() >= 1);
  }

  // Phase 2: reopen through the counting store and clear open-time noise.
  let ranges = Arc::new(Mutex::new(Vec::new()));
  let counting = CountingStore {
    inner: mem.clone(),
    ranges: ranges.clone(),
  };
  let db = ObjectLsm::open(Arc::new(counting), cfg()).unwrap();
  ranges.lock().unwrap().clear();

  let p = db.partition("data").unwrap();
  assert_eq!(p.get(&key(7)).unwrap().unwrap(), val(7));

  let calls = ranges.lock().unwrap().clone();
  assert_eq!(
    calls.len(),
    1,
    "cold point read should perform exactly one block Range GET, got {calls:?}"
  );
  let call = &calls[0];
  assert!(
    call.key.starts_with(&format!("{PREFIX}/seg/data/")),
    "unexpected range key: {}",
    call.key
  );
  assert!(
    call.offset < 1_000_000 && call.len > 20,
    "expected a data-block range near the object start, got {call:?}"
  );
}
