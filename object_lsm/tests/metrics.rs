//! Metrics counters: commit/put/delete/get accounting plus storage snapshot.

use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, MemoryStore, ObjectLsm, Result, Store};

fn cfg(prefix: &str) -> Config {
  Config::new(prefix)
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000)
}

#[derive(Clone)]
struct FailFirstJournalPut {
  inner: MemoryStore,
  armed: Arc<AtomicBool>,
}

impl Store for FailFirstJournalPut {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    self.inner.get(key)
  }
  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
    self.inner.get_range(key, offset, len)
  }
  fn put(&self, key: &str, data: &[u8]) -> Result<()> {
    if key.contains("/journal/") && self.armed.swap(false, Ordering::SeqCst) {
      return Err(wedb_object_lsm::Error::store(
        "injected journal put failure",
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
fn metrics_track_ops_errors_and_storage() {
  let armed = Arc::new(AtomicBool::new(false));
  let store = FailFirstJournalPut {
    inner: MemoryStore::new(),
    armed: armed.clone(),
  };
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg("mt/1")).unwrap();
  let p = db.partition("data").unwrap();

  assert_eq!(p.get(b"none").unwrap(), None); // gets = 1
  p.insert(b"k1", b"v1").unwrap(); // commit ok, put = 1
  armed.store(true, Ordering::SeqCst);
  assert!(p.insert(b"k2", b"v2").is_err()); // commit error, no put counted
  p.insert(b"k2", b"v2").unwrap(); // retry ok, put = 2
  assert_eq!(p.get(b"k1").unwrap().unwrap(), b"v1"); // gets = 2
  p.rm(b"k1").unwrap(); // delete = 1

  let m = db.metrics();
  assert_eq!(m.commits, 4);
  assert_eq!(m.commit_failures, 1);
  assert_eq!(m.puts, 2);
  assert_eq!(m.deletes, 1);
  assert_eq!(m.gets, 2);
  assert!(m.memtable_bytes > 0);
  assert!(m.journal_bytes > 0);
  assert_eq!(m.segments, 0);

  db.compact().unwrap();
  let m2 = db.metrics();
  assert!(m2.segments >= 1);
  assert!(m2.segment_bytes > 0);
  assert!(
    m2.journal_count <= m.journal_count,
    "compact folds journals"
  );
}

#[test]
fn follower_metrics_count_refresh_and_reads() {
  let s = MemoryStore::new();
  let cfg = cfg("mt/2");
  let leader = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  let p = leader.partition("data").unwrap();
  p.insert(b"k", b"v").unwrap();
  leader.compact().unwrap();

  let follower = ObjectLsm::open_follower(Arc::new(s.clone()), cfg.clone(), None).unwrap();
  assert_eq!(follower.metrics().refreshes, 0);
  follower.refresh().unwrap();
  assert_eq!(follower.metrics().refreshes, 1);
  let pf = follower.partition("data").unwrap();
  assert_eq!(pf.get(b"k").unwrap().unwrap(), b"v");
  let m = follower.metrics();
  assert_eq!(m.refreshes, 1);
  assert_eq!(m.gets, 1);
  assert_eq!(m.commits, 0, "follower never commits");
}

#[test]
fn metrics_export_prometheus_and_json() {
  let s = MemoryStore::new();
  let cfg = cfg("mt/3");
  let db = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  let p = db.partition("data").unwrap();
  p.insert(b"k1", b"v1").unwrap();
  p.get(b"k1").unwrap();
  db.compact().unwrap();

  let m = db.metrics();
  let json: serde_json::Value = serde_json::from_str(&m.to_json()).unwrap();
  assert!(json.get("puts").unwrap().as_u64().unwrap() >= 1);
  assert!(json.get("gets").unwrap().as_u64().unwrap() >= 1);
  assert!(json.get("segments").unwrap().as_u64().unwrap() >= 1);

  let prom = m.to_prometheus();
  let mut names = std::collections::BTreeMap::new();
  for line in prom.lines() {
    if line.starts_with('#') {
      continue;
    }
    let (name, val) = line.split_once(' ').expect("prometheus line");
    names.insert(name.to_string(), val.parse::<u64>().unwrap());
  }
  assert_eq!(names.len(), 11);
  assert_eq!(names["wedb_object_lsm_puts"], m.puts);
  assert_eq!(names["wedb_object_lsm_gets"], m.gets);
  assert_eq!(names["wedb_object_lsm_segments"], m.segments as u64);
  assert_eq!(names["wedb_object_lsm_memtable_bytes"], m.memtable_bytes);
}
