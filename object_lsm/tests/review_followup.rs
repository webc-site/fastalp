//! Deterministic follow-up tests for the independent-review checklist.

use std::{
  sync::{
    Arc, Barrier,
    atomic::{AtomicBool, Ordering},
  },
  thread,
  time::Duration,
};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{
  Config, Lease, LeaseOptions, MemoryStore, ObjectLsm, Result, Store, keys::journal_prefix,
  lease::lease_key,
};

#[test]
fn heartbeat_then_release_leaves_no_lease() {
  let store = MemoryStore::new();
  let lease = Lease::acquire(
    Arc::new(store.clone()),
    "rf/release",
    LeaseOptions {
      owner: "w".into(),
      ttl: Duration::from_secs(1),
      timeout: Duration::from_secs(1),
      heartbeat: true,
    },
  )
  .unwrap();
  thread::sleep(Duration::from_millis(80)); // let the heartbeat renew at least once
  lease.release();
  assert_eq!(
    store.get(&lease_key("rf/release")).unwrap(),
    None,
    "release must delete the lease"
  );

  let next = Lease::acquire(
    Arc::new(store.clone()),
    "rf/release",
    LeaseOptions {
      owner: "next".into(),
      ttl: Duration::from_secs(30),
      timeout: Duration::from_millis(200),
      heartbeat: false,
    },
  )
  .unwrap();
  assert_eq!(next.owner(), "next");
}

#[test]
fn dropping_one_clone_keeps_flusher_alive() {
  let store = MemoryStore::new();
  let cfg = Config::new("rf/clone")
    .max_memtable_bytes(1 << 20)
    .journal_window_ms(Some(50));
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  let db2 = db.clone();
  drop(db2);

  let p = db.partition("data").unwrap();
  for i in 0..10u32 {
    p.insert(format!("k{i}").as_bytes(), b"v").unwrap();
  }
  thread::sleep(Duration::from_millis(250));
  assert!(
    !store.list(&journal_prefix("rf/clone")).unwrap().is_empty(),
    "background flusher must still run after a clone is dropped"
  );
}

#[test]
fn windowed_commit_is_durable_after_drop_flush() {
  let store = MemoryStore::new();
  let cfg = Config::new("rf/ack")
    .max_memtable_bytes(1 << 20)
    .journal_window_ms(Some(60_000));
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  let p = db.partition("data").unwrap();
  for i in 0..20u32 {
    p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  assert!(
    store.list(&journal_prefix("rf/ack")).unwrap().is_empty(),
    "ack does not mean flushed yet"
  );
  drop(p);
  drop(db); // Inner::drop flushes the pending journal buffer

  let objs = store.list(&journal_prefix("rf/ack")).unwrap();
  assert!(!objs.is_empty(), "drop must flush buffered journal groups");

  let db2 = ObjectLsm::open(
    Arc::new(store),
    Config::new("rf/ack")
      .max_memtable_bytes(1 << 20)
      .journal_window_ms(Some(60_000)),
  )
  .unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 20);
  assert_eq!(p2.get(b"k019").unwrap().unwrap(), b"v19");
}

struct LeaseLossStore {
  inner: MemoryStore,
  drop_lease: Arc<AtomicBool>,
}

impl Store for LeaseLossStore {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    if key.ends_with("/lease") && self.drop_lease.load(Ordering::SeqCst) {
      return Ok(None);
    }
    self.inner.get(key)
  }
  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
    self.inner.get_range(key, offset, len)
  }
  fn put(&self, key: &str, data: &[u8]) -> Result<()> {
    self.inner.put(key, data)
  }
  fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key)
  }
  fn create(&self, key: &str, data: &[u8]) -> Result<bool> {
    self.inner.create(key, data)
  }
  fn list(&self, prefix: &str) -> Result<Vec<String>> {
    self.inner.list(prefix)
  }
}

#[test]
fn lost_lease_blocks_writes() {
  let drop_lease = Arc::new(AtomicBool::new(false));
  let store = LeaseLossStore {
    inner: MemoryStore::new(),
    drop_lease: drop_lease.clone(),
  };
  let cfg = Config::new("rf/fence").max_memtable_bytes(1 << 20);
  let db = ObjectLsm::open_leased(
    Arc::new(store),
    cfg,
    LeaseOptions {
      owner: "w".into(),
      ttl: Duration::from_millis(120),
      timeout: Duration::from_millis(500),
      heartbeat: true,
    },
  )
  .unwrap();
  let p = db.partition("data").unwrap();
  p.insert(b"k", b"v").unwrap();

  drop_lease.store(true, Ordering::SeqCst);

  let deadline = std::time::Instant::now() + Duration::from_secs(3);
  loop {
    match p.insert(b"k2", b"v2") {
      Err(e) if e.to_string().contains("lease lost") => break,
      Err(_) | Ok(()) => {
        if std::time::Instant::now() > deadline {
          panic!("writes were never fenced after lease loss");
        }
        thread::sleep(Duration::from_millis(20));
      }
    }
  }
}

#[test]
fn three_way_stale_takeover_yields_single_owner() {
  let store = MemoryStore::new();
  let prefix = "rf/three";
  store.put(&lease_key(prefix), b"old-owner\n1").unwrap(); // expired long ago

  let opts = |owner: &str| LeaseOptions {
    owner: owner.to_string(),
    ttl: Duration::from_secs(30),
    timeout: Duration::from_millis(900),
    heartbeat: false,
  };

  let barrier = Arc::new(Barrier::new(3));
  let mut handles = Vec::new();
  for i in 0..3 {
    let s = Arc::new(store.clone());
    let b = barrier.clone();
    let owner = format!("w{i}");
    handles.push(thread::spawn(move || {
      b.wait();
      // Keep the Lease alive: dropping it would release (delete) the record.
      Lease::acquire(s, prefix, opts(&owner))
    }));
  }

  let mut winners = 0;
  let mut losers = 0;
  let mut held = Vec::new();
  for h in handles {
    match h.join().unwrap() {
      Ok(lease) => {
        winners += 1;
        held.push(lease);
      }
      Err(_) => losers += 1,
    }
  }
  assert_eq!(
    winners, 1,
    "exactly one contender must win with CAS takeover"
  );
  assert_eq!(losers, 2);

  let bytes = store
    .get(&lease_key(prefix))
    .unwrap()
    .expect("lease present");
  let owner = String::from_utf8(bytes)
    .unwrap()
    .split_once('\n')
    .unwrap()
    .0
    .to_string();
  assert!(owner == "w0" || owner == "w1" || owner == "w2");
}

#[test]
fn memory_cas_sanity() {
  let s = MemoryStore::new();
  s.put("cas/k", b"a").unwrap();
  assert!(s.put_if_matches("cas/k", b"a", b"b").unwrap());
  assert!(!s.put_if_matches("cas/k", b"a", b"c").unwrap());
  assert_eq!(s.get("cas/k").unwrap().unwrap(), b"b");
}
