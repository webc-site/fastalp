//! Read-replica (follower) tests: a read-only engine over the SAME shared
//! bucket prefix as a writer/leader, tracking the leader's published state via
//! `refresh()` without ever writing to the store.

use std::{sync::Arc, time::Duration};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, LeaseOptions, MemoryStore, ObjectLsm, Result, Store};

fn cfg(prefix: &str) -> Config {
  Config::new(prefix)
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000)
}

fn leased_opts(owner: &str, ttl_ms: u64) -> LeaseOptions {
  LeaseOptions {
    owner: owner.into(),
    ttl: Duration::from_millis(ttl_ms),
    timeout: Duration::from_millis(2_000),
    heartbeat: false,
  }
}

/// Snapshot of every object key in the store.
fn keys(s: &MemoryStore) -> Vec<String> {
  let mut all = s.list("").unwrap();
  all.sort();
  all
}

#[test]
fn follower_reads_published_state_and_tracks_new_commits() {
  let s = MemoryStore::new();
  let cfg = cfg("fo/t1");
  let leader = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  let p = leader.partition("data").unwrap();
  for i in 0..5u32 {
    p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  // A follower opened before anything is published sees an empty prefix.
  let follower = ObjectLsm::open_follower(Arc::new(s.clone()), cfg.clone(), None).unwrap();
  assert!(!follower.partition_exists("data"));

  // Fold everything into a manifest; the follower picks it up on refresh.
  leader.compact().unwrap();
  follower.refresh().unwrap();
  let pf = follower.partition("data").unwrap();
  assert_eq!(pf.len().unwrap(), 5, "follower sees published segments");
  assert_eq!(pf.get(b"k004").unwrap().unwrap(), b"v4");

  // Strict-mode commits that are NOT yet folded stay visible through the
  // durable journal tail above the manifest watermark.
  p.insert(b"k005", b"v5").unwrap();
  p.insert(b"k006", b"v6").unwrap();
  p.rm(b"k000").unwrap();
  follower.refresh().unwrap();
  assert_eq!(pf.len().unwrap(), 6, "follower replays unflushed journals");
  assert_eq!(pf.get(b"k006").unwrap().unwrap(), b"v6");
  assert_eq!(
    pf.get(b"k000").unwrap(),
    None,
    "follower observes the delete tombstone"
  );

  // After the leader folds again the follower still agrees.
  leader.compact().unwrap();
  follower.refresh().unwrap();
  assert_eq!(pf.len().unwrap(), 6);
  assert_eq!(pf.get(b"k005").unwrap().unwrap(), b"v5");
  assert_eq!(pf.get(b"k000").unwrap(), None);
}

#[test]
fn follower_never_writes_to_the_store() {
  let s = MemoryStore::new();
  let cfg = cfg("fo/t2");
  let leader = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  let p = leader.partition("data").unwrap();
  for i in 0..10u32 {
    p.insert(format!("k{i:02}").as_bytes(), b"v").unwrap();
  }
  leader.compact().unwrap();
  let before = keys(&s);

  let follower = ObjectLsm::open_follower(Arc::new(s.clone()), cfg.clone(), None).unwrap();
  let pf = follower.partition("data").unwrap();
  for _ in 0..3 {
    follower.refresh().unwrap();
    let _ = pf.get(b"k00").unwrap();
    let _ = pf.len().unwrap();
  }
  drop(pf);
  drop(follower);

  assert_eq!(
    keys(&s),
    before,
    "opening/refreshing/reading a follower must not add or remove objects"
  );
}

#[test]
fn follower_rejects_all_mutations() {
  let s = MemoryStore::new();
  let cfg = cfg("fo/t3");
  let leader = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  let p = leader.partition("data").unwrap();
  p.insert(b"k", b"v").unwrap();
  leader.compact().unwrap();

  let follower = ObjectLsm::open_follower(Arc::new(s.clone()), cfg.clone(), None).unwrap();
  let pf = follower.partition("data").unwrap();
  let read_only = |e: wedb_object_lsm::Error| e.to_string().contains("read-only");

  assert!(
    pf.insert(b"x", b"y")
      .unwrap_err()
      .to_string()
      .contains("read-only")
  );
  assert!(read_only(pf.rm(b"k").unwrap_err()));
  assert!(read_only(pf.clear().unwrap_err()));
  assert!(read_only(pf.compact().unwrap_err()));
  assert!(read_only(follower.persist().unwrap_err()));
  assert!(read_only(follower.compact().unwrap_err()));
  assert!(read_only(follower.rm_partition(&pf).unwrap_err()));
}

#[test]
fn follower_tracks_epoch_change_across_failover() {
  let s = MemoryStore::new();
  let cfg = cfg("fo/t4");
  let l1 =
    ObjectLsm::open_leased(Arc::new(s.clone()), cfg.clone(), leased_opts("w0", 250)).unwrap();
  let p1 = l1.partition("data").unwrap();
  for i in 0..5u32 {
    p1.insert(format!("w0-{i:02}").as_bytes(), b"v").unwrap();
  }
  l1.compact().unwrap();

  let follower = ObjectLsm::open_follower(Arc::new(s.clone()), cfg.clone(), None).unwrap();
  follower.refresh().unwrap();
  let pf = follower.partition("data").unwrap();
  assert_eq!(pf.len().unwrap(), 5);

  // Leader crashes (lease left to expire) and a new epoch takes over.
  std::mem::forget(l1);
  std::mem::forget(p1);
  std::thread::sleep(Duration::from_millis(600));

  let t0 = std::time::Instant::now();
  let l2 = loop {
    if let Some(e) =
      ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), leased_opts("w1", 60_000))
        .unwrap()
    {
      break e;
    }
    assert!(t0.elapsed() < Duration::from_secs(5), "w1 never promoted");
    std::thread::sleep(Duration::from_millis(30));
  };
  let p2 = l2.partition("data").unwrap();
  for i in 0..5u32 {
    p2.insert(format!("w1-{i:02}").as_bytes(), b"v").unwrap();
  }
  l2.compact().unwrap();

  // The follower crosses the epoch boundary: it still sees the old data and
  // now also the new writer's keys, all from the new manifest.
  follower.refresh().unwrap();
  assert_eq!(pf.len().unwrap(), 10, "follower tracks the failover");
  assert_eq!(pf.get(b"w0-04").unwrap().unwrap(), b"v");
  assert_eq!(pf.get(b"w1-00").unwrap().unwrap(), b"v");
}

#[test]
fn follower_observes_partition_removal() {
  let s = MemoryStore::new();
  let cfg = cfg("fo/t5");
  let leader = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  let pa = leader.partition("a").unwrap();
  let pb = leader.partition("b").unwrap();
  pa.insert(b"k", b"a").unwrap();
  pb.insert(b"k", b"b").unwrap();
  leader.compact().unwrap();

  let follower = ObjectLsm::open_follower(Arc::new(s.clone()), cfg.clone(), None).unwrap();
  follower.refresh().unwrap();
  assert!(follower.partition_exists("a"));
  assert!(follower.partition_exists("b"));

  leader.rm_partition(&pb).unwrap();
  follower.refresh().unwrap();
  assert!(follower.partition_exists("a"));
  assert!(
    !follower.partition_exists("b"),
    "removed partition disappears from the follower"
  );
  let fpa = follower.partition("a").unwrap();
  assert_eq!(fpa.get(b"k").unwrap().unwrap(), b"a");
}

#[test]
fn refresh_is_a_noop_on_a_writer() {
  let s = MemoryStore::new();
  let cfg = cfg("fo/t6");
  let db = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  let p = db.partition("data").unwrap();
  p.insert(b"k", b"v").unwrap();
  // refresh() on a writer must not clobber live in-memory state.
  db.refresh().unwrap();
  assert_eq!(
    p.get(b"k").unwrap().unwrap(),
    b"v",
    "writer refresh must not drop unflushed memtable data"
  );
}

#[test]
fn follower_opens_empty_prefix_and_then_sees_first_publish() {
  let s = MemoryStore::new();
  let cfg = cfg("fo/t7");
  // Follower opens before the leader has ever published a manifest.
  let follower = ObjectLsm::open_follower(Arc::new(s.clone()), cfg.clone(), None).unwrap();
  assert!(!follower.partition_exists("data"));

  let leader = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  let p = leader.partition("data").unwrap();
  p.insert(b"k", b"v").unwrap();
  follower.refresh().unwrap(); // journal only, no manifest yet -> still empty
  assert!(!follower.partition_exists("data"));

  leader.compact().unwrap();
  follower.refresh().unwrap();
  let pf = follower.partition("data").unwrap();
  assert_eq!(pf.get(b"k").unwrap().unwrap(), b"v");
}

#[test]
fn follower_background_refresh_tracks_leader() {
  let s = MemoryStore::new();
  let cfg = cfg("fo/t8");
  let leader = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  let follower = ObjectLsm::open_follower(
    Arc::new(s.clone()),
    cfg.clone(),
    Some(Duration::from_millis(15)),
  )
  .unwrap();
  let p = leader.partition("data").unwrap();
  p.insert(b"k", b"v").unwrap();
  leader.compact().unwrap();

  // The handle is only granted once the background refresh has seen the
  // leader's first manifest, so poll for it.
  let deadline = std::time::Instant::now() + Duration::from_secs(5);
  let pf = loop {
    match follower.partition("data") {
      Ok(h) => break h,
      Err(_) => {
        assert!(
          std::time::Instant::now() < deadline,
          "follower never saw the manifest"
        );
        std::thread::sleep(Duration::from_millis(10));
      }
    }
  };
  loop {
    if pf.get(b"k").unwrap() == Some(b"v".to_vec()) {
      break;
    }
    assert!(
      std::time::Instant::now() < deadline,
      "background refresh never caught up"
    );
    std::thread::sleep(Duration::from_millis(20));
  }
}

/// Blind-spot regression (review): a follower must refuse handles for
/// partitions the leader removed or never published — and must NOT write
/// anything (touch_partition used to resurrect dropped partitions by
/// publishing a manifest).
#[test]
fn follower_partition_handle_refused_for_unpublished_or_removed_partitions() {
  let s = MemoryStore::new();
  let cfg = cfg("fo/t9");
  let leader = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  let pa = leader.partition("a").unwrap();
  let pb = leader.partition("b").unwrap();
  pa.insert(b"k", b"a").unwrap();
  pb.insert(b"k", b"b").unwrap();
  leader.compact().unwrap();

  let follower = ObjectLsm::open_follower(Arc::new(s.clone()), cfg.clone(), None).unwrap();
  follower.refresh().unwrap();

  leader.rm_partition(&pb).unwrap();
  follower.refresh().unwrap();
  let before = keys(&s);

  // Removed partition: handle refused, nothing written.
  let err = match follower.partition("b") {
    Ok(_) => panic!("removed partition must be refused"),
    Err(e) => e.to_string(),
  };
  assert!(
    err.contains("read-only"),
    "removed partition must be refused: {err}"
  );
  // Never-published partition: handle refused, nothing written.
  let err2 = match follower.partition("never") {
    Ok(_) => panic!("unknown partition must be refused"),
    Err(e) => e.to_string(),
  };
  assert!(
    err2.contains("read-only"),
    "unknown partition must be refused: {err2}"
  );
  // Live partition: handle still granted and readable.
  let fpa = follower.partition("a").unwrap();
  assert_eq!(fpa.get(b"k").unwrap().unwrap(), b"a");

  assert_eq!(
    keys(&s),
    before,
    "follower handle lookups must not write objects"
  );
}

/// Counting store: counts every list() (each follower refresh lists journals).
#[derive(Clone)]
struct CountingStore {
  inner: MemoryStore,
  lists: Arc<std::sync::atomic::AtomicUsize>,
}

impl Store for CountingStore {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
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
  fn list(&self, prefix: &str) -> Result<Vec<String>> {
    self.lists.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    self.inner.list(prefix)
  }
}

/// Blind-spot regression (review): the background refresh thread must exit
/// when the follower is dropped (weak-upgrade, like writer threads) instead of
/// pinning the engine and polling the store forever.
#[test]
fn dropping_follower_stops_background_polls() {
  let inner_s = MemoryStore::new();
  let cfg = cfg("fo/t10");
  let leader = ObjectLsm::open(Arc::new(inner_s.clone()), cfg.clone()).unwrap();
  let p = leader.partition("data").unwrap();
  p.insert(b"k", b"v").unwrap();
  leader.compact().unwrap();
  drop(leader);
  drop(p);

  let lists = Arc::new(std::sync::atomic::AtomicUsize::new(0));
  let store = CountingStore {
    inner: inner_s.clone(),
    lists: lists.clone(),
  };
  {
    let follower =
      ObjectLsm::open_follower(Arc::new(store), cfg, Some(Duration::from_millis(10))).unwrap();
    let pf = follower.partition("data").unwrap();
    assert_eq!(pf.get(b"k").unwrap().unwrap(), b"v");
    // Let the background poller run a few cycles.
    std::thread::sleep(Duration::from_millis(80));
  } // follower (and partition handle) dropped here

  let after_drop = lists.load(std::sync::atomic::Ordering::SeqCst);
  std::thread::sleep(Duration::from_millis(120));
  let later = lists.load(std::sync::atomic::Ordering::SeqCst);
  assert!(
    later <= after_drop + 1,
    "background polls must stop after the follower drops (after_drop={after_drop}, later={later})"
  );
}

/// A stale follower snapshot must surface an error (not a silent miss) when the
/// leader has already compacted away a segment the follower still references,
/// and must recover once it refreshes to the new manifest.
#[test]
fn follower_stale_read_errors_after_leader_deletes_segment() {
  let s = MemoryStore::new();
  let cfg = Config::new("fo/t11")
    .max_memtable_bytes(1 << 20)
    .block_size(64)
    .max_segments_before_compact(2);
  let leader = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  let p = leader.partition("data").unwrap();
  for i in 0..4u32 {
    p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  leader.compact().unwrap(); // manifest references segment S1

  // Follower loads S1 but does NOT warm any block (so a later stale read must
  // hit the store instead of the block cache).
  let follower = ObjectLsm::open_follower(Arc::new(s.clone()), cfg.clone(), None).unwrap();
  follower.refresh().unwrap();
  let pf = follower.partition("data").unwrap();
  assert_eq!(pf.table_count(), 1, "follower references the first segment");

  // Leader writes more and compacts: S1 is merged away and its object deleted.
  for i in 4..8u32 {
    p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  leader.compact().unwrap();

  // The follower still points at S1; reading a not-yet-cached key of it must
  // error loudly instead of silently returning None or stale data.
  let res = pf.get(b"k001");
  assert!(
    res.is_err(),
    "stale segment read must surface an error, got {res:?}"
  );

  follower.refresh().unwrap();
  assert_eq!(pf.get(b"k001").unwrap().unwrap(), b"v1");
  assert_eq!(pf.get(b"k007").unwrap().unwrap(), b"v7");
}

/// Two processes sharing ONE bucket: each owns a disjoint shard prefix
/// (leased writer), writes concurrently, and reads the OTHER process's
/// published data through a follower on that shard. Per-shard writes are
/// strongly consistent under the single-writer lease; cross-shard reads
/// converge once the follower refreshes — no lost, overwritten or mixed keys.
#[test]
fn two_writers_share_bucket_with_cross_read_consistency() {
  let s = MemoryStore::new();
  let base = "tp/base";
  let mk = |shard: u64| {
    Config::for_shard(base, shard)
      .max_memtable_bytes(1 << 20)
      .max_segments_before_compact(1_000_000)
  };
  let cfg_a = mk(0);
  let cfg_b = mk(1);

  // Process A and process B both open leased writers on the SAME bucket but
  // on disjoint prefixes.
  let a = ObjectLsm::open_leased(
    Arc::new(s.clone()),
    cfg_a.clone(),
    leased_opts("procA", 60_000),
  )
  .unwrap();
  let b = ObjectLsm::open_leased(
    Arc::new(s.clone()),
    cfg_b.clone(),
    leased_opts("procB", 60_000),
  )
  .unwrap();

  let pa = a.partition("data").unwrap();
  let pb = b.partition("data").unwrap();
  for i in 0..25u32 {
    pa.insert(format!("a-{i:02}").as_bytes(), format!("va{i}").as_bytes())
      .unwrap();
    pb.insert(format!("b-{i:02}").as_bytes(), format!("vb{i}").as_bytes())
      .unwrap();
  }
  a.compact().unwrap();
  b.compact().unwrap();

  // Each process reads the other's published data through a follower.
  let fb = ObjectLsm::open_follower(Arc::new(s.clone()), cfg_b.clone(), None).unwrap();
  fb.refresh().unwrap();
  let pfb = fb.partition("data").unwrap();
  assert_eq!(pfb.len().unwrap(), 25, "A sees B's 25 keys");
  for i in (0..25u32).step_by(5) {
    assert_eq!(
      pfb.get(format!("b-{i:02}").as_bytes()).unwrap().unwrap(),
      format!("vb{i}").into_bytes()
    );
  }

  let fa = ObjectLsm::open_follower(Arc::new(s.clone()), cfg_a.clone(), None).unwrap();
  fa.refresh().unwrap();
  let pfa = fa.partition("data").unwrap();
  assert_eq!(pfa.len().unwrap(), 25, "B sees A's 25 keys");
  assert_eq!(pfa.get(b"a-00").unwrap().unwrap(), b"va0");

  // A keeps writing while B's follower keeps reading: the reader converges.
  for i in 25..30u32 {
    pa.insert(format!("a-{i:02}").as_bytes(), format!("va{i}").as_bytes())
      .unwrap();
  }
  a.compact().unwrap();
  fa.refresh().unwrap();
  assert_eq!(
    pfa.len().unwrap(),
    30,
    "B's follower converges on A's new keys"
  );

  // No cross-shard leakage and nothing lost after reopen of each shard.
  assert_eq!(
    pfb.get(b"a-00").unwrap(),
    None,
    "shards must not leak into each other"
  );
  drop(pa);
  drop(pb);
  drop(a);
  drop(b);
  drop(fa);
  drop(fb);

  let ra = ObjectLsm::open_leased(
    Arc::new(s.clone()),
    cfg_a.clone(),
    leased_opts("reopenA", 60_000),
  )
  .unwrap();
  let p_ra = ra.partition("data").unwrap();
  assert_eq!(p_ra.len().unwrap(), 30, "A's shard intact after reopen");
  let rb = ObjectLsm::open_leased(
    Arc::new(s.clone()),
    cfg_b.clone(),
    leased_opts("reopenB", 60_000),
  )
  .unwrap();
  let p_rb = rb.partition("data").unwrap();
  assert_eq!(p_rb.len().unwrap(), 25, "B's shard intact after reopen");
}
