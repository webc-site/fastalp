//! Production metrics: cumulative counters maintained by the engine plus a
//! snapshot of derived storage state. Counters use atomics so the hot paths do
//! not take locks.

use std::sync::{
  Arc,
  atomic::{AtomicU64, Ordering},
};

/// Cumulative operation counters (atomic, cheap to bump from hot paths).
/// Public users interact with [`MetricsSnapshot`], not this type.
#[derive(Clone, Default)]
pub struct Metrics {
  inner: Arc<MetricsInner>,
}

#[derive(Default)]
struct MetricsInner {
  commits: AtomicU64,
  commit_failures: AtomicU64,
  puts: AtomicU64,
  deletes: AtomicU64,
  gets: AtomicU64,
  refreshes: AtomicU64,
}

/// A point-in-time, plain-value snapshot returned by [`crate::ObjectLsm::metrics`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetricsSnapshot {
  /// Commit attempts that passed the writer gate and entered the commit
  /// pipeline (a journal PUT failure is still counted here).
  pub commits: u64,
  /// Strict-mode journal-object PUT failures: the write was NOT acknowledged
  /// and must be retried by the caller.
  pub commit_failures: u64,
  pub puts: u64,
  pub deletes: u64,
  pub gets: u64,
  /// Follower snapshot refreshes (always 0 for a writer).
  pub refreshes: u64,
  pub journal_count: usize,
  pub journal_bytes: u64,
  pub segments: usize,
  pub segment_bytes: u64,
  pub memtable_bytes: u64,
}

impl Metrics {
  pub(crate) fn bump_commit(&self) {
    self.inner.commits.fetch_add(1, Ordering::Relaxed);
  }

  pub(crate) fn bump_commit_failure(&self) {
    self.inner.commit_failures.fetch_add(1, Ordering::Relaxed);
  }

  pub(crate) fn bump_puts(&self, n: u64) {
    self.inner.puts.fetch_add(n, Ordering::Relaxed);
  }

  pub(crate) fn bump_deletes(&self, n: u64) {
    self.inner.deletes.fetch_add(n, Ordering::Relaxed);
  }

  pub(crate) fn bump_get(&self) {
    self.inner.gets.fetch_add(1, Ordering::Relaxed);
  }

  pub(crate) fn bump_refresh(&self) {
    self.inner.refreshes.fetch_add(1, Ordering::Relaxed);
  }

  /// Counter part of the snapshot (storage-derived fields filled in by the
  /// engine, which owns the authoritative state).
  pub(crate) fn counters(&self) -> MetricsSnapshot {
    MetricsSnapshot {
      commits: self.inner.commits.load(Ordering::Relaxed),
      commit_failures: self.inner.commit_failures.load(Ordering::Relaxed),
      puts: self.inner.puts.load(Ordering::Relaxed),
      deletes: self.inner.deletes.load(Ordering::Relaxed),
      gets: self.inner.gets.load(Ordering::Relaxed),
      refreshes: self.inner.refreshes.load(Ordering::Relaxed),
      ..MetricsSnapshot::default()
    }
  }
}
