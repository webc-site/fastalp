//! Production metrics: cumulative counters maintained by the engine plus a
//! snapshot of derived storage state. Counters use atomics so the hot paths do
//! not take locks.

use std::sync::{
  Arc,
  atomic::{AtomicU64, Ordering},
};

/// Prometheus metric name prefix.
const METRIC_PREFIX: &str = "wedb_object_lsm_";

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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
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

impl MetricsSnapshot {
  /// Prometheus text exposition (counters/gauges with `# TYPE` lines, so
  /// `rate()` and friends work). Values beyond `2^53` may lose precision in
  /// float-based consumers; with these counters that is not reachable in
  /// practice.
  pub fn to_prometheus(&self) -> String {
    let counters: [(&str, u64); 6] = [
      ("commits", self.commits),
      ("commit_failures", self.commit_failures),
      ("puts", self.puts),
      ("deletes", self.deletes),
      ("gets", self.gets),
      ("refreshes", self.refreshes),
    ];
    let gauges: [(&str, u64); 5] = [
      ("journal_count", self.journal_count as u64),
      ("journal_bytes", self.journal_bytes),
      ("segments", self.segments as u64),
      ("segment_bytes", self.segment_bytes),
      ("memtable_bytes", self.memtable_bytes),
    ];
    let mut out = String::new();
    for (name, v) in counters {
      out.push_str(&format!(
        "# TYPE {METRIC_PREFIX}{name} counter\n{METRIC_PREFIX}{name} {v}\n"
      ));
    }
    for (name, v) in gauges {
      out.push_str(&format!(
        "# TYPE {METRIC_PREFIX}{name} gauge\n{METRIC_PREFIX}{name} {v}\n"
      ));
    }
    out
  }

  /// JSON representation (stable field names via `serde`).
  pub fn to_json(&self) -> String {
    serde_json::to_string(self).expect("metrics serialize")
  }
}
