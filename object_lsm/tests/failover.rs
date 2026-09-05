//! HA / automatic failover tests: non-blocking lease acquisition, standby
//! promotion after leader loss, cross-epoch recovery of acked-but-unflushed
//! journals, stale-leader fencing and object-store fault rows.
//!
//! Multi-instance behaviour is exercised against the in-memory store so the
//! races are deterministic; the same scenarios run against live Cloudflare R2
//! in `tests/r2.rs`.

use std::{
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  thread,
  time::{Duration, Instant},
};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, Lease, LeaseOptions, MemoryStore, ObjectLsm, Result, Store};

fn opts(owner: &str, heartbeat: bool, ttl_ms: u64) -> LeaseOptions {
  LeaseOptions {
    owner: owner.to_string(),
    ttl: Duration::from_millis(ttl_ms),
    timeout: Duration::from_millis(2_000),
    heartbeat,
  }
}

fn cfg(prefix: &str) -> Config {
  Config::new(prefix)
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000)
}

#[test]
fn try_acquire_once_is_nonblocking_and_release_handoff() {
  let s = MemoryStore::new();
  let a = Lease::acquire(Arc::new(s.clone()), "ha/t1", opts("a", false, 60_000)).unwrap();
  assert_eq!(a.owner(), "a");

  let t0 = Instant::now();
  let b = Lease::try_acquire_once(Arc::new(s.clone()), "ha/t1", &opts("b", false, 60_000)).unwrap();
  assert!(b.is_none(), "held lease must not be acquirable");
  assert!(
    t0.elapsed() < Duration::from_millis(500),
    "try_acquire_once must not block"
  );

  a.release();
  let c = Lease::try_acquire_once(Arc::new(s.clone()), "ha/t1", &opts("c", false, 60_000)).unwrap();
  assert!(c.is_some(), "released lease must be acquirable");
}

#[test]
fn try_open_leased_returns_none_while_held_then_promotes() {
  let s = MemoryStore::new();
  let cfg = cfg("ha/t2");
  let leader =
    ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w0", false, 60_000))
      .unwrap()
      .expect("first writer wins");

  let second =
    ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w1", false, 60_000))
      .unwrap();
  assert!(second.is_none(), "second writer must be refused while held");

  drop(leader);
  let next =
    ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg, opts("w1", false, 60_000)).unwrap();
  assert!(
    next.is_some(),
    "standby must promote after the leader releases"
  );
}

/// A leased leader crashes (engine forgotten, lease left to expire) after
/// acknowledged strict-mode writes but BEFORE its first manifest publish. The
/// successor must recover those pre-manifest journals across the epoch bump —
/// they are the only durable record of the acked writes.
#[test]
fn successor_recovers_pre_manifest_journals_after_crash() {
  let s = MemoryStore::new();
  let cfg = cfg("ha/t3");
  let leader = ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w0", false, 300))
    .unwrap()
    .expect("leader");
  let p = leader.partition("data").unwrap();
  for i in 0..50u32 {
    p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  assert!(
    leader.fence_epoch() != 0,
    "leased engine must carry a fencing epoch"
  );
  // Crash: no Drop, so the lease object stays until it expires (ttl 300 ms).
  std::mem::forget(leader);
  std::mem::forget(p);

  thread::sleep(Duration::from_millis(600));

  let t0 = Instant::now();
  let standby = loop {
    if let Some(e) =
      ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w1", false, 60_000))
        .unwrap()
    {
      break e;
    }
    assert!(
      t0.elapsed() < Duration::from_secs(5),
      "standby never promoted"
    );
    thread::sleep(Duration::from_millis(50));
  };

  let p2 = standby.partition("data").unwrap();
  assert_eq!(
    p2.len().unwrap(),
    50,
    "successor must recover acked pre-manifest journals"
  );
  for i in (0..50u32).step_by(7) {
    assert_eq!(
      p2.get(format!("k{i:03}").as_bytes()).unwrap(),
      Some(format!("v{i}").into_bytes())
    );
  }

  // The successor keeps writing under its own (new) epoch.
  p2.insert(b"k050", b"v50").unwrap();
  standby.compact().unwrap();
  drop(standby);
  drop(p2);

  let db = ObjectLsm::open(Arc::new(s), cfg).unwrap();
  let p3 = db.partition("data").unwrap();
  assert_eq!(p3.len().unwrap(), 51);
  assert_eq!(p3.get(b"k049").unwrap().unwrap(), b"v49");
  assert_eq!(p3.get(b"k050").unwrap().unwrap(), b"v50");
}

/// Same recovery guarantee when the old leader stops cleanly (lease released)
/// without ever having flushed a manifest.
#[test]
fn successor_recovers_pre_manifest_journals_after_clean_drop() {
  let s = MemoryStore::new();
  let cfg = cfg("ha/t4");
  let leader =
    ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w0", false, 60_000))
      .unwrap()
      .expect("leader");
  let p = leader.partition("data").unwrap();
  for i in 0..30u32 {
    p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  drop(p);
  drop(leader); // releases the lease, still no manifest was ever published

  let next = ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg, opts("w1", false, 60_000))
    .unwrap()
    .expect("clean handoff");
  let p2 = next.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 30);
  assert_eq!(p2.get(b"k029").unwrap().unwrap(), b"v29");
}

/// One writer's view of a shared object store. When `cut` is raised, THIS view
/// can no longer see the lease object (a one-way network partition), so its
/// heartbeat fails and it is fenced; the other view still sees the expired
/// lease and CAS-takes-over. The acked (but not yet flushed) writes of the old
/// epoch survive into the successor.
#[derive(Clone)]
struct PartitionView {
  inner: Arc<MemoryStore>,
  me: &'static str,
  cut: Arc<AtomicBool>,
}

impl Store for PartitionView {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    if key.ends_with("/lease")
      && self.cut.load(Ordering::SeqCst)
      && let Some(b) = self.inner.get(key)?
    {
      let owner = String::from_utf8_lossy(&b)
        .split_once('\n')
        .map(|(o, _)| o.to_string())
        .unwrap_or_default();
      if owner == self.me {
        return Ok(None); // this writer can no longer see/renew its lease
      }
      return Ok(Some(b));
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
  fn put_if_matches(&self, key: &str, expected: &[u8], new: &[u8]) -> Result<bool> {
    self.inner.put_if_matches(key, expected, new)
  }
  fn list(&self, prefix: &str) -> Result<Vec<String>> {
    self.inner.list(prefix)
  }
}

#[test]
fn stale_leader_is_fenced_after_takeover_and_successor_keeps_its_acks() {
  let cut = Arc::new(AtomicBool::new(false));
  let shared = MemoryStore::new();
  // Two one-way views over the same objects: only w0's view is cut off from
  // the lease, so w0's heartbeat fails while a standby can still take over.
  let w0_store = Arc::new(PartitionView {
    inner: Arc::new(shared.clone()),
    me: "w0",
    cut: cut.clone(),
  });
  let w1_store = Arc::new(PartitionView {
    inner: Arc::new(shared.clone()),
    me: "w1",
    cut: cut.clone(),
  });
  let cfg = cfg("ha/t5");

  let w0 = ObjectLsm::open_leased(w0_store.clone(), cfg.clone(), opts("w0", true, 300)).unwrap();
  let p0 = w0.partition("data").unwrap();
  for i in 0..5u32 {
    p0.insert(format!("w0-{i:02}").as_bytes(), b"v").unwrap();
  }

  // Cut w0's view: its heartbeat renewal now fails and the lease expires ~ttl
  // after the last successful renewal; w1's view is unaffected.
  cut.store(true, Ordering::SeqCst);

  let t0 = Instant::now();
  let w1 = loop {
    if let Some(e) =
      ObjectLsm::try_open_leased(w1_store.clone(), cfg.clone(), opts("w1", false, 60_000)).unwrap()
    {
      break e;
    }
    assert!(
      t0.elapsed() < Duration::from_secs(5),
      "standby never took over the expired lease"
    );
    thread::sleep(Duration::from_millis(30));
  };

  // w1 sees w0's acked (never-flushed) writes.
  let p1 = w1.partition("data").unwrap();
  assert_eq!(p1.len().unwrap(), 5);
  assert_eq!(p1.get(b"w0-04").unwrap().unwrap(), b"v");

  // w0 is fenced: once its heartbeat notices the loss, every mutation errors.
  let deadline = Instant::now() + Duration::from_secs(5);
  loop {
    match p0.insert(b"w0-late", b"v") {
      Err(e) if e.to_string().contains("lease lost") => break,
      Err(_) | Ok(()) => {
        assert!(Instant::now() < deadline, "stale writer was never fenced");
        thread::sleep(Duration::from_millis(20));
      }
    }
  }
}

/// One-shot journal PUT failure: the commit reports an error and nothing is
/// half-visible; a later retry succeeds and is durable.
#[derive(Clone)]
struct FailOnceJournalStore {
  inner: MemoryStore,
  armed: Arc<AtomicBool>,
}

impl Store for FailOnceJournalStore {
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
fn journal_put_failure_never_exposes_partial_state() {
  let armed = Arc::new(AtomicBool::new(true));
  let store = FailOnceJournalStore {
    inner: MemoryStore::new(),
    armed: armed.clone(),
  };
  let cfg = cfg("ha/t6");
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg.clone()).unwrap();
  let p = db.partition("data").unwrap();

  assert!(
    p.insert(b"k1", b"v1").is_err(),
    "first journal PUT is injected to fail"
  );
  assert_eq!(
    p.get(b"k1").unwrap(),
    None,
    "failed commit must not be visible"
  );

  p.insert(b"k2", b"v2").unwrap();
  assert_eq!(p.get(b"k1").unwrap(), None);
  assert_eq!(p.get(b"k2").unwrap().unwrap(), b"v2");
  drop(p);
  drop(db);

  let db2 = ObjectLsm::open(Arc::new(store), cfg).unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.get(b"k1").unwrap(), None);
  assert_eq!(p2.get(b"k2").unwrap().unwrap(), b"v2");
}

/// After a leader crash several standbys race to promote: exactly one wins
/// (epoch fencing + CAS), the rest observe the winner's lease.
#[test]
fn concurrent_standbys_promote_exactly_one_writer() {
  let s = MemoryStore::new();
  let cfg = cfg("ha/t7");
  let leader = ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w0", false, 250))
    .unwrap()
    .expect("leader");
  let p = leader.partition("data").unwrap();
  for i in 0..10u32 {
    p.insert(format!("k{i:02}").as_bytes(), b"v").unwrap();
  }
  std::mem::forget(leader);
  std::mem::forget(p);
  thread::sleep(Duration::from_millis(500)); // lease expired

  let winners = Arc::new(std::sync::Mutex::new(Vec::<ObjectLsm>::new()));
  let mut handles = Vec::new();
  for i in 0..4u32 {
    let s = Arc::new(s.clone());
    let cfg = cfg.clone();
    let winners = winners.clone();
    handles.push(thread::spawn(move || {
      let t0 = Instant::now();
      while t0.elapsed() < Duration::from_millis(1_500) {
        if let Some(e) = ObjectLsm::try_open_leased(
          s.clone(),
          cfg.clone(),
          opts(&format!("w{}", i + 1), false, 60_000),
        )
        .unwrap()
        {
          winners.lock().unwrap().push(e);
          return true;
        }
        thread::sleep(Duration::from_millis(20));
      }
      false
    }));
  }

  let mut promoted = 0;
  for h in handles {
    if h.join().unwrap() {
      promoted += 1;
    }
  }
  assert_eq!(
    promoted, 1,
    "exactly one standby must become the writer after a crash"
  );

  let held = winners.lock().unwrap();
  assert_eq!(held.len(), 1);
  let w = held.first().unwrap();
  let p = w.partition("data").unwrap();
  assert_eq!(
    p.len().unwrap(),
    10,
    "winner must recover the crashed leader"
  );
}

/// Regression for the reviewer finding: a takeover whose replay exceeds the
/// memtable budget used to auto-flush a manifest before the new fencing epoch
/// was installed, anchoring `current` to the WRONG epoch. The successor then
/// fenced off this writer's own acked journals on the next handoff. Takeover
/// must publish a durable anchor under its own epoch, so a writer that acks
/// further writes and then hands off without flushing never loses them.
#[test]
fn takeover_anchor_preserves_successor_acks_across_handoff() {
  let s = MemoryStore::new();
  // Tiny budget so the successor's replay exceeds it and recovery would try to
  // auto-flush (the path that used to publish a stale-epoch manifest).
  let cfg = Config::new("ha/t8")
    .max_memtable_bytes(2048)
    .max_segments_before_compact(1_000_000);
  let l1 = ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w0", false, 250))
    .unwrap()
    .expect("first writer");
  let p1 = l1.partition("data").unwrap();
  for i in 0..50u32 {
    let key = format!("k{i:03}");
    let value = vec![b'x'; 96];
    p1.insert(key.as_bytes(), &value).unwrap();
  }
  std::mem::forget(l1);
  std::mem::forget(p1);
  thread::sleep(Duration::from_millis(500)); // lease expired

  let t0 = Instant::now();
  let l2 = loop {
    if let Some(e) =
      ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w1", false, 60_000))
        .unwrap()
    {
      break e;
    }
    assert!(t0.elapsed() < Duration::from_secs(5), "l2 never promoted");
    thread::sleep(Duration::from_millis(30));
  };
  let p2 = l2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 50, "l2 must recover the crashed l1");

  // l2 acks three more writes under its OWN epoch, then hands off cleanly
  // WITHOUT flushing them to a manifest.
  for i in 50..53u32 {
    let key = format!("k{i:03}");
    let value = vec![b'y'; 96];
    p2.insert(key.as_bytes(), &value).unwrap();
  }
  drop(p2);
  drop(l2);

  let l3 = ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg, opts("w2", false, 60_000))
    .unwrap()
    .expect("third writer promotes");
  let p3 = l3.partition("data").unwrap();
  assert_eq!(
    p3.len().unwrap(),
    53,
    "l2's acked journals must survive the handoff (anchor under l2's epoch)"
  );
  assert_eq!(p3.get(b"k052").unwrap().unwrap(), vec![b'y'; 96]);
}
/// Journal GC must never delete objects whose fold was not durably published:
/// a flush that uploads a segment but fails to publish its manifest leaves the
/// journal objects as the only durable record, and deleting them would lose
/// acknowledged writes on recovery.
#[derive(Clone)]
struct FailManifestOnceStore {
  inner: MemoryStore,
  armed: Arc<AtomicBool>,
}

impl Store for FailManifestOnceStore {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    self.inner.get(key)
  }
  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
    self.inner.get_range(key, offset, len)
  }
  fn put(&self, key: &str, data: &[u8]) -> Result<()> {
    // Fail the manifest SNAPSHOT put (not the current-pointer flip) exactly
    // once, after the segment upload has already succeeded.
    if key.contains("/manifest/")
      && !key.ends_with("/current")
      && self.armed.swap(false, Ordering::SeqCst)
    {
      return Err(wedb_object_lsm::Error::store(
        "injected manifest put failure",
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
fn failed_manifest_publish_keeps_journals_for_recovery() {
  let armed = Arc::new(AtomicBool::new(true));
  let store = FailManifestOnceStore {
    inner: MemoryStore::new(),
    armed: armed.clone(),
  };
  // A one-byte budget forces a flush on the very first commit, so the segment
  // upload happens and the manifest publish fails in the same commit.
  let cfg = Config::new("ha/t9")
    .max_memtable_bytes(1)
    .max_segments_before_compact(1_000_000);
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg.clone()).unwrap();
  let p = db.partition("data").unwrap();
  // The journal PUT succeeds; the maintenance flush fails on manifest publish
  // and that error is swallowed by the commit path.
  p.insert(b"k1", b"v1").unwrap();
  drop(p);
  drop(db);

  let db2 = ObjectLsm::open(Arc::new(store), cfg).unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(
    p2.get(b"k1").unwrap().unwrap(),
    b"v1",
    "acked write must survive a failed manifest publish (journal kept)"
  );
}

/// Superseded-epoch journal objects must be garbage-collected once a takeover
/// has folded them into its anchor manifest; otherwise they accumulate in the
/// bucket forever (gc_journal only removes current-epoch keys).
#[test]
fn takeover_gc_superseded_epoch_journals() {
  let s = MemoryStore::new();
  let cfg = Config::new("ha/t11")
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000);
  let l1 = ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w0", false, 250))
    .unwrap()
    .expect("first writer");
  let p1 = l1.partition("data").unwrap();
  for i in 0..5u32 {
    p1.insert(format!("k{i:02}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  // Crash before any manifest: the acked writes live only as epoch-1 journals.
  std::mem::forget(l1);
  std::mem::forget(p1);
  thread::sleep(Duration::from_millis(500));

  let t0 = Instant::now();
  let l2 = loop {
    if let Some(e) =
      ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w1", false, 60_000))
        .unwrap()
    {
      break e;
    }
    assert!(t0.elapsed() < Duration::from_secs(5), "w1 never promoted");
    thread::sleep(Duration::from_millis(30));
  };
  let p2 = l2.partition("data").unwrap();
  assert_eq!(
    p2.len().unwrap(),
    5,
    "successor must recover the crashed leader"
  );

  let root = wedb_object_lsm::keys::journal_prefix("ha/t11");
  let left = s.list(&root).unwrap();
  assert!(
    left.is_empty(),
    "superseded-epoch journals must be GC'd after takeover, left: {left:?}"
  );
  assert_eq!(p2.get(b"k04").unwrap().unwrap(), b"v4");
}

/// A dropped partition whose watermark stayed 0 (removed before any commit)
/// must not pin published_min_wm at 0 and disable journal GC for live
/// partitions forever.
#[test]
fn dropped_partition_does_not_pin_journal_gc() {
  let s = MemoryStore::new();
  let cfg = Config::new("ha/t12")
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000);
  let db = ObjectLsm::open(Arc::new(s.clone()), cfg.clone()).unwrap();
  // Create and remove an empty partition BEFORE any commit: rm publishes a
  // manifest marking it dropped with watermark = journal_seq = 0.
  let trash = db.partition("trash").unwrap();
  db.rm_partition(&trash).unwrap();

  // A live partition then commits and folds everything.
  let p = db.partition("data").unwrap();
  for i in 0..5u32 {
    p.insert(format!("k{i:02}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  db.compact().unwrap();

  let root = wedb_object_lsm::keys::journal_prefix("ha/t12");
  assert!(
    s.list(&root).unwrap().is_empty(),
    "folded journals must be GC'd even with a dropped wm=0 partition present"
  );

  let db2 = ObjectLsm::open(Arc::new(s.clone()), cfg).unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 5);
  assert_eq!(p2.get(b"k04").unwrap().unwrap(), b"v4");
  assert!(!db2.partition_exists("trash"));
}

/// The recover()-tail cleanup (every writer open) also removes folded journals
/// of superseded epochs, not just the takeover path.
#[test]
fn open_gc_superseded_epoch_journals_at_recover() {
  let s = MemoryStore::new();
  let cfg = Config::new("ha/t13")
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000);
  let l1 = ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w0", false, 250))
    .unwrap()
    .expect("first writer");
  let e1_epoch = l1.fence_epoch();
  let p1 = l1.partition("data").unwrap();
  for i in 0..5u32 {
    p1.insert(format!("k{i:02}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  std::mem::forget(l1);
  std::mem::forget(p1);
  thread::sleep(Duration::from_millis(500));

  // Takeover folds epoch-1 journals into an epoch-2 anchor and cleans them.
  let t0 = Instant::now();
  let l2 = loop {
    if let Some(e) =
      ObjectLsm::try_open_leased(Arc::new(s.clone()), cfg.clone(), opts("w1", false, 60_000))
        .unwrap()
    {
      break e;
    }
    assert!(t0.elapsed() < Duration::from_secs(5), "w1 never promoted");
    thread::sleep(Duration::from_millis(30));
  };
  drop(l2);

  // Re-inject a folded epoch-1 journal object by hand, then a plain (writer)
  // open must remove it via the recover()-tail gc_foreign_journals pass.
  let key = wedb_object_lsm::keys::journal_key_epoch("ha/t13", 3, e1_epoch);
  s.put(&key, b"stale").unwrap();

  let db = ObjectLsm::open(Arc::new(s.clone()), cfg).unwrap();
  let p = db.partition("data").unwrap();
  assert_eq!(p.len().unwrap(), 5, "recover still sees the folded data");
  assert_eq!(
    s.get(&key).unwrap(),
    None,
    "folded superseded-epoch journal removed by the recover()-tail GC"
  );
}

/// A failed flip of the `current` pointer (the manifest object was written but
/// visibility never advanced) must leave journals intact so a reopen recovers
/// every acked write — never a silent ack of an unflushed state.
#[test]
fn fenced_current_flip_failure_preserves_journals() {
  #[derive(Clone)]
  struct FailCurrentOnce {
    inner: MemoryStore,
    armed: Arc<AtomicBool>,
  }
  impl Store for FailCurrentOnce {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
      self.inner.get(key)
    }
    fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
      self.inner.get_range(key, offset, len)
    }
    fn put(&self, key: &str, data: &[u8]) -> Result<()> {
      if key.ends_with("/manifest/current") && self.armed.swap(false, Ordering::SeqCst) {
        return Err(wedb_object_lsm::Error::store(
          "injected current flip failure",
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

  let armed = Arc::new(AtomicBool::new(true));
  let store = FailCurrentOnce {
    inner: MemoryStore::new(),
    armed: armed.clone(),
  };
  let cfg = Config::new("ha/t14")
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000);
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg.clone()).unwrap();
  let p = db.partition("data").unwrap();
  for i in 0..5u32 {
    p.insert(format!("k{i:02}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  assert!(
    db.compact().is_err(),
    "injected current-pointer failure must fail the publish"
  );
  drop(p);
  drop(db);

  let db2 = ObjectLsm::open(Arc::new(store), cfg).unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 5, "reopen recovers via journals");
  assert_eq!(p2.get(b"k04").unwrap().unwrap(), b"v4");
}

/// Repeated journal-object PUT failures must never acknowledge a write: a
/// failed commit is invisible and must be retried; after success everything is
/// durable with no holes or duplicates.
#[test]
fn repeated_journal_put_failures_need_success_before_ack() {
  #[derive(Clone)]
  struct FailJournalPuts {
    inner: MemoryStore,
    n: Arc<std::sync::atomic::AtomicUsize>,
    fail_at: Vec<usize>,
  }
  impl Store for FailJournalPuts {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
      self.inner.get(key)
    }
    fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
      self.inner.get_range(key, offset, len)
    }
    fn put(&self, key: &str, data: &[u8]) -> Result<()> {
      if key.contains("/journal/") {
        let i = self.n.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_at.contains(&i) {
          return Err(wedb_object_lsm::Error::store(
            "injected journal put failure",
          ));
        }
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

  let store = FailJournalPuts {
    inner: MemoryStore::new(),
    n: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    fail_at: vec![2, 4],
  };
  let cfg = Config::new("ha/t15")
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000);
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg.clone()).unwrap();
  let p = db.partition("data").unwrap();

  // The store fails two journal-object PUTs (ordinals 2 and 4) wherever they
  // land. A failed commit returns Err and is invisible; the caller retries
  // until success. No acknowledged write may ever be lost or duplicated.
  for i in 1..=5u32 {
    let key = format!("k{i}");
    let value = format!("v{i}");
    loop {
      match p.insert(key.as_bytes(), value.as_bytes()) {
        Ok(()) => break,
        Err(_) => continue,
      }
    }
    assert_eq!(
      p.get(key.as_bytes()).unwrap().unwrap(),
      value.into_bytes(),
      "acked write must be immediately visible"
    );
  }
  assert_eq!(p.len().unwrap(), 5);
  drop(p);
  drop(db);

  let db2 = ObjectLsm::open(Arc::new(store), cfg).unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 5, "no holes or duplicates after reopen");
  for i in 1..=5u32 {
    assert_eq!(
      p2.get(format!("k{i}").as_bytes()).unwrap().unwrap(),
      format!("v{i}").into_bytes()
    );
  }
}
