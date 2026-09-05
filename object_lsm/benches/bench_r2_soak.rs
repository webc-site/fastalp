//! R2-only soak benchmark for P1: write a large key set through ObjectLsm on
//! real Cloudflare R2, then compact, reopen and verify every key.
//!
//! Run:
//! ```sh
//! R2_* env required. BENCH_N overrides the key count (default 100_000).
//! OBJLSM_WINDOW_MS overrides the group-commit window (default 50).
//! cargo bench -p wedb_object_lsm --features "r2 wedb" --bench bench_r2_soak
//! ```

#![cfg(feature = "r2")]

use std::{sync::Arc, time::Instant};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, ObjectLsm, R2Store, Store};

fn env(name: &str) -> Option<String> {
  std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn main() {
  let Some(_) = env("R2_BUCKET") else {
    println!("R2 env not configured; skipping soak");
    return;
  };
  let n: usize = env("BENCH_N")
    .and_then(|v| v.parse().ok())
    .unwrap_or(100_000);
  let window_ms = env("OBJLSM_WINDOW_MS")
    .and_then(|v| v.parse().ok())
    .unwrap_or(50u64);
  let prefix = format!("wedb_soak_{}_{}", std::process::id(), chrono_now());
  let cfg = Config::new(&prefix)
    .max_memtable_bytes(8 << 20)
    .block_size(16 * 1024)
    .max_segments_before_compact(1_000_000)
    .background_flush(true)
    .journal_window_ms(Some(window_ms));
  let store = Arc::new(R2Store::from_env().expect("r2 store"));
  if let Some(p) = env("OBJLSM_CLEANUP_PREFIX") {
    let list = store.list(&p).expect("list");
    for key in &list {
      let _ = store.delete(key);
    }
    println!("cleaned {} objects under {}", list.len(), p);
    return;
  }

  struct Cleanup(Arc<R2Store>, String);
  impl Drop for Cleanup {
    fn drop(&mut self) {
      println!(
        "r2 metrics put_failures={} get_failures={} other_failures={}",
        self.0.put_failures(),
        self.0.get_failures(),
        self.0.other_failures()
      );
      if let Ok(keys) = self.0.list(&self.1) {
        for key in keys {
          let _ = self.0.delete(&key);
        }
      }
    }
  }
  let _cleanup = Cleanup(store.clone(), prefix.clone());
  println!("R2 soak start n={n} prefix={prefix} window={window_ms}ms");
  let eng = ObjectLsm::open(store.clone(), cfg.clone()).expect("open");
  let p = eng.partition("data").expect("partition");

  let t = Instant::now();
  let val = vec![b'x'; 64];
  for i in 0..n {
    let key = format!("soak-{i:08}");
    p.insert(key.as_bytes(), &val).expect("insert");
  }
  let write_us = t.elapsed().as_micros();
  println!(
    "insert n={n} total_ms={:.1} per_op_us={:.1}",
    write_us as f64 / 1000.0,
    write_us as f64 / n as f64
  );

  let t = Instant::now();
  eng.compact().expect("compact");
  let compact_us = t.elapsed().as_micros();
  println!("compact_ms={:.1}", compact_us as f64 / 1000.0);
  println!(
    "table_count={} disk_space={}",
    p.table_count(),
    eng.disk_space().unwrap_or(0)
  );

  drop(eng);
  drop(p);

  let t = Instant::now();
  let eng2 = ObjectLsm::open(store, cfg).expect("reopen");
  let reopen_us = t.elapsed().as_micros();
  let p2 = eng2.partition("data").expect("partition");
  let mut count = 0usize;
  for _ in p2.iter() {
    count += 1;
  }
  println!(
    "reopen_ms={:.1} recovered={count} expected={n}",
    reopen_us as f64 / 1000.0
  );
  assert_eq!(count, n, "R2 soak lost keys");
  drop(eng2);
  drop(p2);

  println!("R2 soak done");
}

fn chrono_now() -> u128 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis()
}
