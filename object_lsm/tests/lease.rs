//! Lease (shared-bucket writer lock) + shard-isolation tests.

use std::{sync::Arc, thread, time::Duration};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, Lease, LeaseOptions, MemoryStore, ObjectLsm};

fn store() -> MemoryStore {
  MemoryStore::new()
}

fn opts(owner: &str, heartbeat: bool) -> LeaseOptions {
  LeaseOptions {
    owner: owner.to_string(),
    ttl: Duration::from_millis(120),
    timeout: Duration::from_millis(300),
    heartbeat,
  }
}

#[test]
fn acquire_release_handoff() {
  let s = store();
  let a = Lease::acquire(Arc::new(s.clone()), "l/handoff", opts("a", false)).unwrap();
  assert_eq!(a.owner(), "a");
  assert!(!a.is_lost());
  a.release();

  let b = Lease::acquire(Arc::new(s.clone()), "l/handoff", opts("b", false)).unwrap();
  assert_eq!(b.owner(), "b");
  b.release();
}

#[test]
fn contention_times_out_then_releases() {
  let s = store();
  let a = Lease::acquire(Arc::new(s.clone()), "l/contend", opts("a", true)).unwrap();
  let t0 = std::time::Instant::now();
  let err = Lease::acquire(Arc::new(s.clone()), "l/contend", opts("b", false)).unwrap_err();
  assert!(
    t0.elapsed() >= Duration::from_millis(250),
    "should have waited for timeout"
  );
  assert!(err.to_string().contains("held"));
  a.release();

  let c = Lease::acquire(Arc::new(s.clone()), "l/contend", opts("c", false)).unwrap();
  assert_eq!(c.owner(), "c");
}

#[test]
fn stale_lease_can_be_taken_over() {
  let s = store();
  // No heartbeat: lease expires on its own.
  let a = Lease::acquire(Arc::new(s.clone()), "l/stale", opts("a", false)).unwrap();
  thread::sleep(Duration::from_millis(200));
  let b = Lease::acquire(Arc::new(s.clone()), "l/stale", opts("b", false)).unwrap();
  assert_eq!(b.owner(), "b");
  // The original owner's renewal now fails (it lost the lease).
  assert!(!a.renew().unwrap());
}

#[test]
fn leased_engine_is_exclusive_writer() {
  let s = store();
  let cfg = Config::new("l/eng");
  let e1 = ObjectLsm::open_leased(Arc::new(s.clone()), cfg.clone(), opts("w1", true)).unwrap();
  let p1 = e1.partition("data").unwrap();
  p1.insert(b"k", b"v1").unwrap();

  let err = ObjectLsm::open_leased(Arc::new(s.clone()), cfg, opts("w2", false)).unwrap_err();
  assert!(
    err.to_string().contains("held"),
    "second writer must be rejected: {err}"
  );

  // With epoch fencing, only manifest-published state survives a writer
  // handoff; flush the memtable first (unflushed old-epoch journal groups are
  // intentionally fenced off for the new writer).
  e1.compact().unwrap();
  drop(e1); // releases the lease
  let e3 =
    ObjectLsm::open_leased(Arc::new(s.clone()), Config::new("l/eng"), opts("w3", false)).unwrap();
  let p3 = e3.partition("data").unwrap();
  assert_eq!(
    p3.get(b"k").unwrap(),
    Some(b"v1".to_vec()),
    "data persists across writer handoff"
  );
}

#[test]
fn shard_prefixes_allow_parallel_writers() {
  let s = store();
  let c0 = Config::for_shard("l/shard", 0);
  let c1 = Config::for_shard("l/shard", 1);
  assert_ne!(c0.prefix, c1.prefix);

  let e0 = ObjectLsm::open_leased(Arc::new(s.clone()), c0, opts("w0", false)).unwrap();
  let e1 = ObjectLsm::open_leased(Arc::new(s.clone()), c1, opts("w1", false)).unwrap();
  let p0 = e0.partition("data").unwrap();
  let p1 = e1.partition("data").unwrap();
  p0.insert(b"k", b"shard0").unwrap();
  p1.insert(b"k", b"shard1").unwrap();
  assert_eq!(p0.get(b"k").unwrap().unwrap(), b"shard0");
  assert_eq!(p1.get(b"k").unwrap().unwrap(), b"shard1");
}
