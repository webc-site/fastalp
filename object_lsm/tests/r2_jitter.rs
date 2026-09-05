//! Live R2 network-jitter soak harness. Skipped by default (`#[ignore]`).
//!
//! Run (with R2_* env) for a long soak, e.g.:
//! ```text
//! OBJLSM_JITTER_SECONDS=3600 OBJLSM_JITTER_FAIL_EVERY=10 \
//! cargo test --release -p wedb_object_lsm --features r2 --test r2_jitter -- --ignored
//! ```
//! Every failed commit is retried and never acknowledged; at the end the
//! engine is reopened and every acked key must be recovered.

#![cfg(feature = "r2")]

use std::{
  sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  },
  time::{Duration, Instant},
};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, ObjectLsm, R2Store, Result, Store};

fn env_ok() -> bool {
  let creds = ["R2_BUCKET", "R2_ACCESS_KEY_ID", "R2_SECRET_ACCESS_KEY"]
    .iter()
    .all(|k| std::env::var(k).map(|v| !v.is_empty()).unwrap_or(false));
  let endpoint = ["R2_ENDPOINT", "R2_ACCOUNT_ID"]
    .iter()
    .any(|k| std::env::var(k).map(|v| !v.is_empty()).unwrap_or(false));
  creds && endpoint
}

fn seconds() -> u64 {
  std::env::var("OBJLSM_JITTER_SECONDS")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(30)
}

fn fail_every() -> usize {
  std::env::var("OBJLSM_JITTER_FAIL_EVERY")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(10)
    .max(1)
}

#[derive(Clone)]
struct JitterStore {
  inner: Arc<R2Store>,
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
    if key.contains("/journal/") && i.is_multiple_of(self.fail_every) {
      return Err(wedb_object_lsm::Error::store(
        "injected transient R2 PUT failure",
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
#[ignore]
fn r2_jitter_soak_no_acked_loss() -> Result<()> {
  if !env_ok() {
    eprintln!("R2 env not configured; skipping live soak");
    return Ok(());
  }
  let prefix = format!(
    "wedb_test/jitter_{}_{}",
    std::process::id(),
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap()
      .as_nanos()
  );
  let cfg = Config::new(&prefix)
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000);
  let store = Arc::new(R2Store::from_env()?);
  let jitter = Arc::new(JitterStore {
    inner: store.clone(),
    calls: Arc::new(AtomicUsize::new(0)),
    fail_every: fail_every(),
  });

  let eng = ObjectLsm::open(jitter.clone(), cfg.clone())?;
  let p = eng.partition("data")?;
  let deadline = Instant::now() + Duration::from_secs(seconds());
  let mut acked = 0u32;
  let mut injected = 0usize;
  while Instant::now() < deadline {
    let key = format!("k{acked:06}");
    let value = format!("v{acked}");
    loop {
      match p.insert(key.as_bytes(), value.as_bytes()) {
        Ok(()) => {
          acked += 1;
          break;
        }
        Err(_) => injected += 1,
      }
    }
    if acked.is_multiple_of(50) {
      let _ = eng.compact();
    }
  }
  assert_eq!(
    p.len()?,
    acked as usize,
    "all acked writes visible before reopen"
  );
  drop(p);
  drop(eng);

  let eng2 = ObjectLsm::open(store.clone(), cfg)?;
  let p2 = eng2.partition("data")?;
  assert_eq!(
    p2.len()?,
    acked as usize,
    "reopen recovers every acked write"
  );
  for i in (0..acked).step_by(13) {
    assert_eq!(
      p2.get(format!("k{i:06}").as_bytes())?.unwrap(),
      format!("v{i}").into_bytes()
    );
  }
  eprintln!(
    "R2_JITTER acked={acked} injected={injected} fail_every={}",
    fail_every()
  );

  for key in store.list(&prefix)? {
    let _ = store.delete(&key);
  }
  Ok(())
}
