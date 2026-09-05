//! Backend comparison benchmark: wedb_embed::Fjall (local disk) vs ObjectLsm
//! over the in-memory Store vs ObjectLsm over Cloudflare R2.
//!
//! Run:
//! ```sh
//! cargo bench -p wedb_object_lsm --features "r2 wedb" --bench bench_remote
//! # R2 benches require R2_* env; without them only local backends run
//! ```
//!
//! Timings are measured with std::time at engine level (Engine/Partition):
//! - insert: one commit per key (ObjectLsm commits are durable R2 PUTs; fjall
//!   writes into its WAL with manual journal persist semantics)
//! - point reads after a flush/compact (block cache warm)
//! - full scans

#![cfg(all(feature = "r2", feature = "wedb"))]

use std::{sync::Arc, time::Instant};

use tempfile::tempdir;
use wedb_embed::Fjall;
use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{
  Config, FileStore, MemoryStore, ObjectLsm, R2Store, Store,
  keys::{journal_prefix, manifest_prefix},
};

fn env_ok() -> bool {
  ["R2_BUCKET", "R2_ACCESS_KEY_ID", "R2_SECRET_ACCESS_KEY"]
    .iter()
    .all(|k| std::env::var(k).map(|v| !v.is_empty()).unwrap_or(false))
}

fn n_default() -> usize {
  std::env::var("BENCH_N")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(200)
}

/// Optional group-commit window from `OBJLSM_WINDOW_MS` (None = strict).
fn window_ms() -> Option<u64> {
  std::env::var("OBJLSM_WINDOW_MS")
    .ok()
    .and_then(|s| s.parse().ok())
}

fn label(name: &str) -> String {
  match window_ms() {
    Some(ms) => format!("{name} (grouped {ms}ms)"),
    None => name.to_string(),
  }
}

fn mk_cfg(prefix: &str) -> Config {
  Config::new(prefix)
    .max_memtable_bytes(1 << 20)
    .block_size(4096)
    // Avoid compaction inside the hot write loop: tiny datasets used to reach
    // 16 segments early and repeatedly fold the same data during writes.
    .max_segments_before_compact(1_000_000)
    .background_flush(matches!(std::env::var("OBJLSM_BACKGROUND_FLUSH"), Ok(v) if v != "0"))
    .journal_window_ms(window_ms())
}

fn key(i: usize) -> Vec<u8> {
  format!("bench-key-{i:08}").into_bytes()
}

const VAL: &[u8] = b"payload-64-bytes-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";

fn report(name: &str, what: &str, n: usize, dur: std::time::Duration) {
  let per_ns = dur.as_nanos() as f64 / n as f64;
  let per_us = per_ns / 1000.0;
  println!(
    "{name:>22} {what:<22} {n:>7} ops  total {:>10.1?}  per-op {:>10.2} us",
    dur, per_us
  );
}

fn bench_engine<E: Engine>(name: &str, eng: &E, n: usize) {
  let p = eng.partition("bench").expect("partition");
  // insert (commit per key)
  let t = Instant::now();
  for i in 0..n {
    p.insert(&key(i), VAL).expect("insert");
  }
  report(name, "insert (commit)", n, t.elapsed());

  // flush / compact so reads hit durable segments
  let t = Instant::now();
  eng.compact().expect("compact");
  report(name, "compact/flush", n, t.elapsed());

  // point reads (warm cache)
  let t = Instant::now();
  for i in 0..n {
    assert!(p.get(&key(i)).expect("get").is_some());
  }
  report(name, "point read (warm)", n, t.elapsed());

  // full scan
  let t = Instant::now();
  let mut count = 0usize;
  for e in p.iter() {
    let _ = e.expect("iter");
    count += 1;
  }
  report(name, "scan entries", count, t.elapsed());
}

fn percentiles(mut xs: Vec<u128>) -> Option<(u128, u128, u128)> {
  if xs.is_empty() {
    return None;
  }
  xs.sort_unstable();
  let pick = |p: f64| -> u128 {
    let idx = ((xs.len() as f64 - 1.0) * p).round() as usize;
    xs[idx.min(xs.len() - 1)]
  };
  Some((pick(0.50), pick(0.95), pick(0.99)))
}

fn report_object_stats(eng: &ObjectLsm, store: &Arc<R2Store>, prefix: &str) {
  let journals = store.list(&journal_prefix(prefix)).unwrap_or_default();
  let manifests = store.list(&manifest_prefix(prefix)).unwrap_or_default();
  let segments = store.list(&format!("{prefix}/seg/")).unwrap_or_default();
  println!(
    "{:>22} objects journal={} segment={} manifest={} disk_space={}",
    "object stats",
    journals.len(),
    segments.len(),
    manifests.len(),
    eng.disk_space().unwrap_or(0)
  );
}

fn main() {
  let n = n_default();
  println!("ObjectLsm backend comparison  (n = {n} keys)");
  println!();

  // fjall: local disk
  let dir = tempdir().expect("tempdir");
  let fj = Fjall::open(dir.path()).expect("fjall open");
  bench_engine("fjall (local disk)", &fj, n);

  // objectlsm over in-memory store
  let store = MemoryStore::new();
  let mem = ObjectLsm::open(Arc::new(store), mk_cfg("bench/remote/mem")).expect("obj mem open");
  bench_engine(&label("objectlsm (memory)"), &mem, n);

  // objectlsm over a local directory (FileStore)
  let dir = tempdir().expect("tempdir file");
  let file = ObjectLsm::open(
    Arc::new(FileStore::new(dir.path()).expect("file store")),
    mk_cfg("bench/remote/file"),
  )
  .expect("obj file open");
  bench_engine(&label("objectlsm (file)"), &file, n);

  // objectlsm over Cloudflare R2
  if !env_ok() {
    println!("R2 env not configured; skipping R2 backend benches");
    return;
  }
  let r2store = Arc::new(R2Store::from_env().expect("r2 store"));
  let r2 = ObjectLsm::open(r2store.clone(), mk_cfg("wedb_bench/remote")).expect("obj r2 open");
  bench_engine(&label("objectlsm (R2)"), &r2, n);

  // R2 object footprint + reopen/recovery validation
  report_object_stats(&r2, &r2store, "wedb_bench/remote");
  {
    let p = r2.partition("bench").expect("reopen partition");
    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
      let t = Instant::now();
      let v = p.get(&key(i)).expect("cold get");
      assert_eq!(v.as_deref(), Some(VAL));
      samples.push(t.elapsed().as_micros());
    }
    if let Some((p50, p95, p99)) = percentiles(samples) {
      println!(
        "{:>22} point read cold        us p50={} p95={} p99={}",
        label("objectlsm (R2)"),
        p50,
        p95,
        p99
      );
    }
  }

  drop(r2);
  {
    let t = Instant::now();
    let r2_reopen =
      ObjectLsm::open(r2store.clone(), mk_cfg("wedb_bench/remote")).expect("reopen r2");
    let reopen_us = t.elapsed().as_micros();
    let p = r2_reopen.partition("bench").expect("partition");
    let mut count = 0usize;
    for e in p.iter() {
      assert!(e.is_ok());
      count += 1;
    }
    assert_eq!(count, n, "R2 reopen lost keys");
    println!(
      "{:>22} reopen/recovery       {} keys in {} us",
      label("objectlsm (R2)"),
      n,
      reopen_us
    );
    report_object_stats(&r2_reopen, &r2store, "wedb_bench/remote");
    drop(r2_reopen);
  }

  // cleanup R2 prefix (best effort)
  for key in r2store.list("wedb_bench/remote").expect("list") {
    let _ = r2store.delete(&key);
  }
}
