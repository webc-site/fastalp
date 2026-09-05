//! Writer lease for sharing one bucket prefix between instances.
//!
//! A lease is an object at `<prefix>/lease` whose payload is
//! `owner\n<expiry-ms>`. Acquisition is atomic via `Store::create`
//! (create-if-absent); a stale lease (past expiry) can be deleted and
//! re-created. The owner renews on a heartbeat; losing renewal marks the lease
//! lost. A separate instance/shard uses its own prefix.

use std::{
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  thread,
  time::{Duration, Instant},
};

use crate::{
  error::{Error, Result},
  store::Store,
};

/// Object key holding the lease record.
pub fn lease_key(prefix: &str) -> String {
  format!("{prefix}/lease")
}

/// Lease acquisition parameters.
#[derive(Clone, Debug)]
pub struct LeaseOptions {
  pub owner: String,
  /// How long a lease lives without renewal.
  pub ttl: Duration,
  /// How long to keep retrying acquisition before giving up.
  pub timeout: Duration,
  /// Whether to run a background heartbeat renewal thread.
  pub heartbeat: bool,
}

impl Default for LeaseOptions {
  fn default() -> Self {
    Self {
      owner: "writer".into(),
      ttl: Duration::from_secs(30),
      timeout: Duration::from_secs(5),
      heartbeat: true,
    }
  }
}

fn epoch_now() -> u128 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_nanos()
}

fn now_ms() -> u128 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis()
}

fn payload(owner: &str, ttl_ms: u128, epoch: u128) -> Vec<u8> {
  format!("{owner}\n{}\n{epoch}", now_ms() + ttl_ms).into_bytes()
}

fn parse_payload(bytes: &[u8]) -> Result<(String, u128, u128)> {
  let text =
    std::str::from_utf8(bytes).map_err(|e| Error::Corrupt(format!("lease not utf-8: {e}")))?;
  let mut parts = text.split('\n');
  let owner = parts
    .next()
    .ok_or_else(|| Error::Corrupt("lease payload malformed".into()))?;
  let expiry_ms: u128 = parts
    .next()
    .ok_or_else(|| Error::Corrupt("lease payload malformed".into()))?
    .trim()
    .parse()
    .map_err(|_| Error::Corrupt("lease expiry malformed".into()))?;
  let epoch: u128 = parts
    .next()
    .map(|s| s.trim().parse().unwrap_or(0))
    .unwrap_or(0);
  Ok((owner.to_string(), expiry_ms, epoch))
}

struct LeaseInner {
  store: Arc<dyn Store>,
  key: String,
  owner: String,
  ttl_ms: u128,
  epoch: u128,
  stop: AtomicBool,
  lost: Arc<AtomicBool>,
  /// Serializes renew / release / heartbeat get-check-write sequences within
  /// this lease instance (cross-instance fencing is still best-effort).
  op: parking_lot::Mutex<()>,
  stop_tx: Option<std::sync::mpsc::Sender<()>>,
}

/// A held writer lease on `<prefix>/lease`.
#[derive(Clone)]
pub struct Lease {
  inner: Arc<LeaseInner>,
}

impl std::fmt::Debug for Lease {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Lease")
      .field("key", &self.inner.key)
      .field("owner", &self.inner.owner)
      .field("lost", &self.inner.lost.load(Ordering::SeqCst))
      .finish()
  }
}

impl Lease {
  /// Try to acquire the lease, retrying until `opts.timeout`.
  pub fn acquire(store: Arc<dyn Store>, prefix: &str, opts: LeaseOptions) -> Result<Self> {
    let deadline = Instant::now() + opts.timeout;
    loop {
      if let Some(lease) = Self::try_acquire_once(store.clone(), prefix, &opts)? {
        return Ok(lease);
      }
      if Instant::now() >= deadline {
        return Err(Error::store(format!(
          "lease {} held by another writer",
          lease_key(prefix)
        )));
      }
      thread::sleep(Duration::from_millis(50));
    }
  }

  /// One non-blocking acquisition attempt.
  ///
  /// Returns `Ok(Some(lease))` when this instance won the lease, `Ok(None)`
  /// when another owner currently holds it (or a concurrent contender won the
  /// race), and `Err` only on a store failure. A stale (expired) lease is
  /// taken over atomically with a compare-and-swap so two contenders can never
  /// both win. Standby/polling supervisors call this instead of [`Self::acquire`]
  /// to avoid blocking for `opts.timeout` while the active writer is healthy.
  pub fn try_acquire_once(
    store: Arc<dyn Store>,
    prefix: &str,
    opts: &LeaseOptions,
  ) -> Result<Option<Self>> {
    let key = lease_key(prefix);
    let ttl_ms = opts.ttl.as_millis().max(1);
    let epoch = epoch_now();
    match store.get(&key) {
      Ok(None) => {
        if store.create(&key, &payload(&opts.owner, ttl_ms, epoch))? {
          // Verify we still own the lease; a concurrent stale-takeover may
          // have deleted us already (best-effort fencing).
          let owned = matches!(store.get(&key)?, Some(b) if parse_payload(&b).ok().map(|(o, _, _)| o) == Some(opts.owner.clone()));
          if owned {
            return Ok(Some(finish_acquire(store, key, opts, ttl_ms, epoch)));
          }
        }
        Ok(None)
      }
      Ok(Some(bytes)) => {
        // Atomically take over an expired lease with compare-and-swap: it only
        // succeeds if the object still holds the exact expired payload we just
        // read, so two contenders cannot both win.
        if let Ok((_, expiry, _)) = parse_payload(&bytes)
          && expiry <= now_ms()
          && store.put_if_matches(&key, &bytes, &payload(&opts.owner, ttl_ms, epoch))?
        {
          return Ok(Some(finish_acquire(store, key, opts, ttl_ms, epoch)));
        }
        Ok(None)
      }
      Err(e) => Err(e),
    }
  }

  pub fn owner(&self) -> &str {
    &self.inner.owner
  }

  pub fn is_lost(&self) -> bool {
    self.inner.lost.load(Ordering::SeqCst)
  }

  /// Fencing epoch: monotonically unique per acquisition, embedded in every
  /// journal group and manifest publish for strong visibility fencing.
  pub fn epoch(&self) -> u128 {
    self.inner.epoch
  }

  /// Shared lost flag for the engine's best-effort write fencing.
  pub fn lost_flag(&self) -> Arc<AtomicBool> {
    self.inner.lost.clone()
  }

  /// Renew by extending the expiry, provided the record still belongs to us.
  pub fn renew(&self) -> Result<bool> {
    let _g = self.inner.op.lock();
    let store = &self.inner.store;
    match store.get(&self.inner.key)? {
      None => {
        self.inner.lost.store(true, Ordering::SeqCst);
        Ok(false)
      }
      Some(bytes) => {
        let (owner, ..) = parse_payload(&bytes)?;
        if owner != self.inner.owner {
          self.inner.lost.store(true, Ordering::SeqCst);
          return Ok(false);
        }
        let renewed = store.put_if_matches(
          &self.inner.key,
          &bytes,
          &payload(&self.inner.owner, self.inner.ttl_ms, self.inner.epoch),
        )?;
        if !renewed {
          self.inner.lost.store(true, Ordering::SeqCst);
        }
        Ok(renewed)
      }
    }
  }

  /// Stop the heartbeat and delete the lease if it still belongs to us.
  pub fn release(&self) {
    if let Some(tx) = &self.inner.stop_tx {
      let _ = tx.send(());
    }
    self.inner.stop.store(true, Ordering::SeqCst);
    let _g = self.inner.op.lock();
    if let Ok(Some(bytes)) = self.inner.store.get(&self.inner.key)
      && let Ok((owner, ..)) = parse_payload(&bytes)
      && owner == self.inner.owner
    {
      let _ = self.inner.store.delete(&self.inner.key);
    }
  }
}

impl Drop for Lease {
  fn drop(&mut self) {
    self.release();
  }
}

/// Build a [`Lease`] handle around a won acquisition and (optionally) start its
/// background heartbeat renewal thread.
fn finish_acquire(
  store: Arc<dyn Store>,
  key: String,
  opts: &LeaseOptions,
  ttl_ms: u128,
  epoch: u128,
) -> Lease {
  let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
  let inner = Arc::new(LeaseInner {
    store,
    key,
    owner: opts.owner.clone(),
    ttl_ms,
    epoch,
    stop: AtomicBool::new(false),
    lost: Arc::new(AtomicBool::new(false)),
    op: parking_lot::Mutex::new(()),
    stop_tx: Some(stop_tx),
  });
  if opts.heartbeat {
    spawn_heartbeat(inner.clone(), stop_rx);
  } else {
    // Keep the receiver alive by dropping it; the sender simply fails to send.
    let _ = stop_rx;
  }
  Lease { inner }
}

fn spawn_heartbeat(inner: Arc<LeaseInner>, stop_rx: std::sync::mpsc::Receiver<()>) {
  let interval = Duration::from_millis((inner.ttl_ms / 3).max(50) as u64);
  thread::Builder::new()
    .name("objectlsm-lease".into())
    .spawn(move || {
      loop {
        if inner.stop.load(Ordering::SeqCst) {
          return;
        }
        if stop_rx.recv_timeout(interval).is_ok() {
          return;
        }
        if inner.stop.load(Ordering::SeqCst) {
          return;
        }
        // Renew; on failure (owner changed / deleted) mark lost and quit.
        let _g = inner.op.lock();
        let ok = match inner.store.get(&inner.key) {
          Ok(Some(bytes)) => match parse_payload(&bytes) {
            Ok((owner, ..)) if owner == inner.owner => inner
              .store
              .put_if_matches(
                &inner.key,
                &bytes,
                &payload(&inner.owner, inner.ttl_ms, inner.epoch),
              )
              .unwrap_or(false),
            _ => false,
          },
          _ => false,
        };
        if !ok {
          inner.lost.store(true, Ordering::SeqCst);
          return;
        }
      }
    })
    .ok();
}
