//! Live Cloudflare R2 integration (feature `r2`).
//!
//! Reads `R2_BUCKET` / `R2_ACCESS_KEY_ID` / `R2_SECRET_ACCESS_KEY` and
//! `R2_ENDPOINT` (or `R2_ACCOUNT_ID`) from the environment; skips when absent.
//! Uses a unique prefix and cleans up after itself.

#![cfg(feature = "r2")]

use std::{
  sync::{
    Arc, Barrier,
    atomic::{AtomicBool, Ordering},
  },
  thread,
};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, LeaseOptions, ObjectLsm, R2Store, Store, keys::segment_root};

fn env_ok() -> bool {
  ["R2_BUCKET", "R2_ACCESS_KEY_ID", "R2_SECRET_ACCESS_KEY"]
    .iter()
    .all(|k| std::env::var(k).map(|v| !v.is_empty()).unwrap_or(false))
}

fn prefix() -> String {
  let pid = std::process::id();
  let nanos = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_nanos();
  format!("wedb_test/r2e_{pid}_{nanos}")
}

#[test]
fn r2_engine_roundtrip() -> wedb_object_lsm::Result<()> {
  if !env_ok() {
    eprintln!("R2 env not configured; skipping live test");
    return Ok(());
  }
  let prefix = prefix();
  let cfg = Config::new(&prefix)
    .max_memtable_bytes(16 * 1024)
    .block_size(2048)
    .max_segments_before_compact(1_000_000);
  let store = Arc::new(R2Store::from_env()?);

  // phase 1: writes + flush + point reads + scan
  let eng = ObjectLsm::open(store.clone(), cfg.clone())?;
  let p = eng.partition("data")?;
  let n = 120u32;
  for i in 0..n {
    p.insert(format!("k{i:04}").as_bytes(), format!("v{i}").as_bytes())?;
  }
  eng.compact()?; // flush memtable into one remote segment
  assert_eq!(p.table_count(), 1);
  for i in (0..n).step_by(5) {
    assert_eq!(
      p.get(format!("k{i:04}").as_bytes())?,
      Some(format!("v{i}").into_bytes())
    );
  }
  // overwrite every 3rd, delete every 7th
  let mut expected = 0u64;
  for i in 0..n {
    let k = format!("k{i:04}").into_bytes();
    if i % 7 == 0 {
      p.rm(&k)?;
    } else {
      expected += 1;
      if i % 3 == 0 {
        p.insert(&k, format!("v{i}-x").as_bytes())?;
      }
    }
  }
  eng.compact()?; // rewrite to a single compacted segment
  assert_eq!(p.len()?, expected as usize);
  assert_eq!(p.get(b"k0000")?, None, "deleted key must stay gone");

  // phase 2: reopen from R2 and verify persistence
  drop(eng);
  let store2 = Arc::new(R2Store::from_env()?);
  let eng2 = ObjectLsm::open(store2.clone(), cfg)?;
  let p2 = eng2.partition("data")?;
  assert_eq!(p2.len()?, expected as usize);
  for i in 1..n {
    if i % 7 != 0 {
      let want = if i % 3 == 0 {
        format!("v{i}-x")
      } else {
        format!("v{i}")
      };
      assert_eq!(
        p2.get(format!("k{i:04}").as_bytes())?,
        Some(want.into_bytes()),
        "key {i}"
      );
    }
  }
  let seg_objects = store2.list(&segment_root(&prefix))?;
  assert_eq!(
    seg_objects.len(),
    p2.table_count(),
    "segment object count matches manifest"
  );

  // cleanup: best-effort delete every object under the prefix
  for key in store2.list(&prefix)? {
    let _ = store2.delete(&key);
  }
  Ok(())
}

#[test]
fn r2_multiple_instances_concurrent() -> wedb_object_lsm::Result<()> {
  if !env_ok() {
    eprintln!("R2 env not configured; skipping live test");
    return Ok(());
  }
  let workers: usize = std::env::var("OBJLSM_CONCURRENCY")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(4);
  let writes: usize = std::env::var("OBJLSM_WRITES")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(120);
  let prefix = prefix();
  let barrier = Arc::new(Barrier::new(workers));
  let mut handles = Vec::with_capacity(workers);

  for w in 0..workers {
    let barrier = barrier.clone();
    let base = prefix.clone();
    handles.push(thread::spawn(move || -> wedb_object_lsm::Result<usize> {
      let store = Arc::new(R2Store::from_env()?);
      let cfg = Config::for_shard(&base, w as u64)
        .max_memtable_bytes(64 * 1024)
        .block_size(2048)
        .max_segments_before_compact(1_000_000);
      let eng = ObjectLsm::open(store.clone(), cfg.clone())?;
      let p = eng.partition("data")?;
      barrier.wait();
      for j in 0..writes {
        let k = format!("{w:02}-{j:05}");
        let v = format!("w{w}:{j}");
        p.insert(k.as_bytes(), v.as_bytes())?;
      }
      eng.compact()?;
      drop(eng);

      // Reopen from R2 and verify this writer's data survived.
      let eng2 = ObjectLsm::open(store, cfg)?;
      let p2 = eng2.partition("data")?;
      assert_eq!(p2.len()?, writes, "writer {w} lost keys");
      for j in (0..writes).step_by(7) {
        let k = format!("{w:02}-{j:05}");
        let v = format!("w{w}:{j}");
        assert_eq!(p2.get(k.as_bytes())?, Some(v.into_bytes()));
      }
      Ok(writes)
    }));
  }

  let mut total = 0usize;
  for h in handles {
    total += h
      .join()
      .map_err(|_| wedb_object_lsm::Error::store("writer panicked"))??;
  }
  assert_eq!(total, workers * writes);

  // Cleanup: best-effort delete every object under the shared base prefix.
  let store = Arc::new(R2Store::from_env()?);
  for key in store.list(&prefix)? {
    let _ = store.delete(&key);
  }
  Ok(())
}

fn lease_opts(owner: &str) -> LeaseOptions {
  LeaseOptions {
    owner: owner.into(),
    ttl: std::time::Duration::from_secs(300),
    timeout: std::time::Duration::from_millis(800),
    heartbeat: false,
  }
}

#[test]
fn r2_same_prefix_lease_fencing() -> wedb_object_lsm::Result<()> {
  if !env_ok() {
    eprintln!("R2 env not configured; skipping live test");
    return Ok(());
  }
  let prefix = prefix();
  let store = Arc::new(R2Store::from_env()?);

  let e0 = ObjectLsm::open_leased(
    store.clone(),
    Config::new(&prefix)
      .max_memtable_bytes(1 << 20)
      .journal_window_ms(Some(10)),
    lease_opts("w0"),
  )?;
  let p0 = e0.partition("data")?;
  for j in 0..5u32 {
    p0.insert(format!("w0-{j:02}").as_bytes(), b"v")?;
  }
  e0.compact()?; // fold into a manifest segment so the next epoch can read it

  // A second writer on the SAME prefix must be refused while w0 holds lease.
  let second = ObjectLsm::open_leased(
    store.clone(),
    Config::new(&prefix)
      .max_memtable_bytes(1 << 20)
      .journal_window_ms(Some(10)),
    lease_opts("w1"),
  );
  assert!(
    second.is_err(),
    "second writer should not acquire the same-prefix lease"
  );

  // Dropping w0 releases the lease; a new writer can take over and see data.
  drop(e0);
  drop(p0);
  let e1 = ObjectLsm::open_leased(
    store.clone(),
    Config::new(&prefix)
      .max_memtable_bytes(1 << 20)
      .journal_window_ms(Some(10)),
    lease_opts("w1"),
  )?;
  let p1 = e1.partition("data")?;
  assert_eq!(p1.len()?, 5, "takeover writer must see w0 data");
  for j in 0..5u32 {
    assert_eq!(
      p1.get(format!("w0-{j:02}").as_bytes())?,
      Some(b"v".to_vec())
    );
  }
  for j in 0..5u32 {
    p1.insert(format!("w1-{j:02}").as_bytes(), b"v")?;
  }
  e1.compact()?;
  drop(e1);
  drop(p1);

  // Reopen after takeover and verify all writers' data is durable.
  let store2 = Arc::new(R2Store::from_env()?);
  let e2 = ObjectLsm::open_leased(
    store2.clone(),
    Config::new(&prefix)
      .max_memtable_bytes(1 << 20)
      .journal_window_ms(Some(10)),
    lease_opts("w2"),
  )?;
  let p2 = e2.partition("data")?;
  assert_eq!(p2.len()?, 10);
  assert_eq!(p2.get(b"w0-00")?, Some(b"v".to_vec()));
  assert_eq!(p2.get(b"w1-04")?, Some(b"v".to_vec()));

  // Cleanup.
  for key in store2.list(&prefix)? {
    let _ = store2.delete(&key);
  }
  Ok(())
}

struct R2Flaky {
  inner: Arc<R2Store>,
  fail_seg_put: AtomicBool,
}

impl Store for R2Flaky {
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
    if key.contains("/seg/") && self.fail_seg_put.swap(false, Ordering::SeqCst) {
      return Err(wedb_object_lsm::Error::store(
        "injected segment put failure",
      ));
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

#[test]
fn r2_transient_segment_put_failure_keeps_old_run() -> wedb_object_lsm::Result<()> {
  if !env_ok() {
    eprintln!("R2 env not configured; skipping live test");
    return Ok(());
  }
  let prefix = prefix();
  let base = Arc::new(R2Store::from_env()?);
  let flaky = Arc::new(R2Flaky {
    inner: base.clone(),
    fail_seg_put: AtomicBool::new(false),
  });
  let cfg = Config::new(&prefix)
    .max_memtable_bytes(1 << 20)
    .block_size(512)
    .journal_window_ms(Some(5))
    .max_segments_before_compact(1_000_000);
  let eng = ObjectLsm::open(flaky.clone(), cfg.clone())?;
  let p = eng.partition("data")?;
  let n = 80u32;
  for i in 0..n {
    p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())?;
  }

  // Fail the first segment upload during compact; journal writes are already
  // durable, so a reopen must recover every key.
  flaky.fail_seg_put.store(true, Ordering::SeqCst);
  assert!(
    eng.compact().is_err(),
    "injected segment-upload failure must fail compact"
  );
  drop(eng);
  drop(p);

  let eng2 = ObjectLsm::open(Arc::new(R2Store::from_env()?), cfg)?;
  let p2 = eng2.partition("data")?;
  assert_eq!(
    p2.len()?,
    n as usize,
    "journal replay must recover all keys"
  );
  assert_eq!(p2.get(b"k007")?.unwrap(), b"v7");
  eng2.compact()?;
  assert_eq!(p2.table_count(), 1);
  drop(eng2);
  drop(p2);

  for key in base.list(&prefix)? {
    let _ = base.delete(&key);
  }
  Ok(())
}

/// Live multi-instance HA: a leader crashes (lease left to expire) after
/// acknowledged strict-mode writes that were never flushed to a manifest; a
/// standby polls `try_open_leased` and promotes once the lease expires. The
/// successor must recover the leader's pre-manifest journals on real R2, then
/// keep writing under its own epoch.
#[test]
fn r2_same_prefix_auto_failover_after_leader_crash() -> wedb_object_lsm::Result<()> {
  if !env_ok() {
    eprintln!("R2 env not configured; skipping live test");
    return Ok(());
  }
  let prefix = prefix();
  let cfg = Config::new(&prefix)
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000);
  let leader_opts = LeaseOptions {
    owner: "w0".into(),
    ttl: std::time::Duration::from_secs(10),
    timeout: std::time::Duration::from_millis(1_000),
    heartbeat: false,
  };
  let standby_opts = LeaseOptions {
    owner: "w1".into(),
    ttl: std::time::Duration::from_secs(300),
    timeout: std::time::Duration::from_millis(1_000),
    heartbeat: false,
  };

  let store = Arc::new(R2Store::from_env()?);
  let leader = ObjectLsm::try_open_leased(store.clone(), cfg.clone(), leader_opts.clone())?
    .expect("first writer must win the lease");
  // The standby observes the held lease without blocking (checked immediately,
  // before the write phase could let a short un-renewed lease lapse).
  assert!(
    ObjectLsm::try_open_leased(store.clone(), cfg.clone(), standby_opts.clone())?.is_none(),
    "standby must be refused while the leader holds the lease"
  );
  let p = leader.partition("data")?;
  let n = 5u32;
  for i in 0..n {
    p.insert(format!("w0-{i:03}").as_bytes(), format!("v{i}").as_bytes())?;
  }
  // ...then the leader crashes without releasing it (lease expires in ~10 s).
  std::mem::forget(leader);
  std::mem::forget(p);

  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
  let standby = loop {
    if let Some(e) = ObjectLsm::try_open_leased(store.clone(), cfg.clone(), standby_opts.clone())? {
      break e;
    }
    assert!(
      std::time::Instant::now() < deadline,
      "standby never promoted on R2"
    );
    std::thread::sleep(std::time::Duration::from_millis(250));
  };

  // The successor recovered every acked pre-manifest journal of the old epoch.
  let p2 = standby.partition("data")?;
  assert_eq!(
    p2.len()?,
    n as usize,
    "successor must recover the crashed leader's acked writes on R2"
  );
  assert_eq!(p2.get(b"w0-000")?.unwrap(), b"v0");
  assert_eq!(
    p2.get(format!("w0-{:03}", n - 1).as_bytes())?.unwrap(),
    format!("v{}", n - 1).into_bytes()
  );

  // The new epoch keeps writing and folds everything into one manifest.
  for i in 0..n {
    p2.insert(
      format!("w1-{i:03}").as_bytes(),
      format!("v{}", n + i).as_bytes(),
    )?;
  }
  standby.compact()?;
  drop(standby);
  drop(p2);

  // Reopen and verify both writers' data is durable.
  let store2 = Arc::new(R2Store::from_env()?);
  let e2 = ObjectLsm::open(store2.clone(), cfg)?;
  let p3 = e2.partition("data")?;
  assert_eq!(p3.len()?, 2 * n as usize);
  assert_eq!(p3.get(b"w0-000")?.unwrap(), b"v0");
  assert_eq!(p3.get(b"w1-000")?.unwrap(), b"v5");

  for key in store2.list(&prefix)? {
    let _ = store2.delete(&key);
  }
  Ok(())
}

/// Live read-replica over a shared bucket prefix on real R2: a leased leader
/// publishes segments and keeps writing strict-mode journals; a follower that
/// never acquires the lease tracks the leader's published state through
/// refresh() and rejects mutations.
#[test]
fn r2_follower_reads_shared_bucket_prefix() -> wedb_object_lsm::Result<()> {
  if !env_ok() {
    eprintln!("R2 env not configured; skipping live test");
    return Ok(());
  }
  let prefix = prefix();
  let cfg = Config::new(&prefix)
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000);
  let leader_opts = LeaseOptions {
    owner: "w0".into(),
    ttl: std::time::Duration::from_secs(300),
    timeout: std::time::Duration::from_millis(1_000),
    heartbeat: false,
  };

  let leader_store = Arc::new(R2Store::from_env()?);
  let follower_store = Arc::new(R2Store::from_env()?);
  let leader = ObjectLsm::open_leased(leader_store.clone(), cfg.clone(), leader_opts.clone())?;
  let p = leader.partition("data")?;
  let n = 8u32;
  for i in 0..n {
    p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())?;
  }
  leader.compact()?; // publish the first manifest

  let follower = ObjectLsm::open_follower(follower_store, cfg.clone(), None)?;
  follower.refresh()?;
  let pf = follower.partition("data")?;
  assert_eq!(
    pf.len()?,
    n as usize,
    "follower sees published segments on R2"
  );
  assert_eq!(pf.get(b"k007")?.unwrap(), b"v7");

  // Unflushed strict-mode journals above the watermark stay visible through the
  // durable journal tail.
  for i in n..n + 3 {
    p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())?;
  }
  follower.refresh()?;
  assert_eq!(
    pf.len()?,
    (n + 3) as usize,
    "follower replays the leader's unflushed R2 journals"
  );
  assert_eq!(pf.get(b"k010")?.unwrap(), b"v10");

  // A follower is read-only even against live R2.
  let err = pf.insert(b"x", b"y").unwrap_err().to_string();
  assert!(err.contains("read-only"), "follower write must fail: {err}");

  // Fold again; the follower still agrees after a fresh manifest.
  leader.compact()?;
  follower.refresh()?;
  assert_eq!(pf.len()?, (n + 3) as usize);
  assert_eq!(pf.get(b"k003")?.unwrap(), b"v3");
  drop(follower);
  drop(leader);

  for key in leader_store.list(&prefix)? {
    let _ = leader_store.delete(&key);
  }
  Ok(())
}

/// Two processes (separate engine instances) sharing ONE real R2 bucket: each
/// owns a disjoint shard prefix as a leased writer, writes concurrently, and
/// reads the other process's published data through a follower on that shard.
/// Verifies per-shard durability + cross-shard read convergence with no lost
/// or mixed keys.
#[test]
fn r2_two_writers_one_bucket_cross_reads() -> wedb_object_lsm::Result<()> {
  if !env_ok() {
    eprintln!("R2 env not configured; skipping live test");
    return Ok(());
  }
  let base = prefix(); // unique base; shards live under <base>/shard-N
  let mk = |shard: u64| {
    Config::for_shard(&base, shard)
      .max_memtable_bytes(1 << 20)
      .max_segments_before_compact(1_000_000)
  };
  let cfg_a = mk(0);
  let cfg_b = mk(1);

  let store = Arc::new(R2Store::from_env()?);
  let a = ObjectLsm::open_leased(store.clone(), cfg_a.clone(), lease_opts("procA"))?;
  let b = ObjectLsm::open_leased(store.clone(), cfg_b.clone(), lease_opts("procB"))?;
  let pa = a.partition("data")?;
  let pb = b.partition("data")?;
  let n = 5u32;
  for i in 0..n {
    pa.insert(format!("a-{i:02}").as_bytes(), format!("va{i}").as_bytes())?;
    pb.insert(format!("b-{i:02}").as_bytes(), format!("vb{i}").as_bytes())?;
  }
  a.compact()?;
  b.compact()?;

  // Each process reads the other's published data via a follower on real R2.
  let follower_a = ObjectLsm::open_follower(Arc::new(R2Store::from_env()?), cfg_b.clone(), None)?;
  follower_a.refresh()?;
  let pfb = follower_a.partition("data")?;
  assert_eq!(pfb.len()?, n as usize, "A sees B's keys");
  assert_eq!(pfb.get(b"b-04")?.unwrap(), b"vb4");

  let follower_b = ObjectLsm::open_follower(Arc::new(R2Store::from_env()?), cfg_a.clone(), None)?;
  follower_b.refresh()?;
  let pfa = follower_b.partition("data")?;
  assert_eq!(pfa.len()?, n as usize, "B sees A's keys");

  // A keeps writing; B's follower converges.
  for i in n..n + 3 {
    pa.insert(format!("a-{i:02}").as_bytes(), format!("va{i}").as_bytes())?;
  }
  a.compact()?;
  follower_b.refresh()?;
  assert_eq!(pfa.len()?, (n + 3) as usize, "B's follower converges on A");
  assert_eq!(pfa.get(b"a-07")?.unwrap(), b"va7");

  drop(pa);
  drop(pb);
  drop(a);
  drop(b);
  drop(follower_a);
  drop(follower_b);

  // Reopen each shard and verify nothing was lost or mixed.
  let ra = ObjectLsm::open_leased(Arc::new(R2Store::from_env()?), cfg_a, lease_opts("reopenA"))?;
  let p_ra = ra.partition("data")?;
  assert_eq!(p_ra.len()?, (n + 3) as usize);
  let rb = ObjectLsm::open_leased(Arc::new(R2Store::from_env()?), cfg_b, lease_opts("reopenB"))?;
  let p_rb = rb.partition("data")?;
  assert_eq!(p_rb.len()?, n as usize);
  assert_eq!(p_rb.get(b"b-00")?.unwrap(), b"vb0");

  let store2 = Arc::new(R2Store::from_env()?);
  for key in store2.list(&base)? {
    let _ = store2.delete(&key);
  }
  Ok(())
}
