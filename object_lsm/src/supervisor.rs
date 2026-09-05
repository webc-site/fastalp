//! Minimal failover supervisor: a standby loop that promotes this process to
//! writer whenever the shared bucket prefix has no active lease, hands the
//! engine to a handler for one leadership term, then releases it and returns
//! to standby. This is the engine-side primitive for a single-writer HA
//! process; node membership, transport and client routing remain an
//! application concern.

use std::{
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use crate::{Config, LeaseOptions, ObjectLsm, Result, Store};

/// Minimum standby poll interval (prevents a `Duration::ZERO` busy loop).
const MIN_POLL: Duration = Duration::from_millis(10);

/// A standby supervisor over one shared prefix.
pub struct Supervisor {
  store: Arc<dyn Store>,
  cfg: Config,
  opts: LeaseOptions,
  poll: Duration,
}

impl Supervisor {
  /// `poll` is how often a standby re-checks the lease while another writer is
  /// healthy; values below [`MIN_POLL`] are clamped to it.
  pub fn new(store: Arc<dyn Store>, cfg: Config, opts: LeaseOptions, poll: Duration) -> Self {
    Self {
      store,
      cfg,
      opts,
      poll: poll.max(MIN_POLL),
    }
  }

  /// Run the standby/promote/handle loop until `stop` is set.
  ///
  /// Each leadership term: block until this instance wins the lease, call
  /// `handle(engine)`, then drop the engine (releasing the lease) and return
  /// to standby. If a handler detects lease loss / a shutdown and returns, the
  /// loop simply waits for the next free lease. Returns the number of terms
  /// served.
  ///
  /// `stop` is observed at most `poll` after it is set while in standby (and
  /// only after the current handler returns while leading), so a graceful stop
  /// may serve at most one extra term. Store/recovery errors abort with `Err`;
  /// a transient object-store failure should be handled by an outer restart.
  pub fn run<F>(&self, stop: &AtomicBool, mut handle: F) -> Result<usize>
  where
    F: FnMut(ObjectLsm) -> Result<()>,
  {
    let mut terms = 0usize;
    while !stop.load(Ordering::SeqCst) {
      // Standby: poll the non-blocking acquisition until we win.
      let engine = loop {
        if stop.load(Ordering::SeqCst) {
          return Ok(terms);
        }
        match ObjectLsm::try_open_leased(self.store.clone(), self.cfg.clone(), self.opts.clone())? {
          Some(engine) => break engine,
          None => std::thread::sleep(self.poll),
        }
      };
      // One leadership term. `engine` is dropped at the end of this call,
      // which releases the lease and hands control back to the standby loop.
      handle(engine)?;
      terms += 1;
    }
    Ok(terms)
  }
}
