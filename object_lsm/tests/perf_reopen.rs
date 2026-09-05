//! Performance probe (run with `--release -- --ignored`): reopen latency for a
//! strict-mode journal tail that must be replayed. Not part of the regular
//! suite.

use std::{sync::Arc, time::Instant};

use tempfile::tempdir;
use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, FileStore, ObjectLsm};

#[test]
#[ignore]
fn perf_reopen_60k_strict_replay() {
  const N: u32 = 60_000;
  let d = tempdir().unwrap();
  let store = Arc::new(FileStore::new(d.path().to_str().unwrap()).unwrap());
  let cfg = Config::new("perf")
    .max_memtable_bytes(1 << 30)
    .max_segments_before_compact(1_000_000);
  let t0 = Instant::now();
  {
    let db = ObjectLsm::open(store.clone(), cfg.clone()).unwrap();
    let p = db.partition("data").unwrap();
    for i in 0..N {
      p.insert(format!("k{i:06}").as_bytes(), format!("v{i}").as_bytes())
        .unwrap();
    }
  }
  let write_ms = t0.elapsed().as_millis();
  let t1 = Instant::now();
  let db = ObjectLsm::open(store, cfg).unwrap();
  let p = db.partition("data").unwrap();
  assert_eq!(p.len().unwrap(), N as usize);
  let reopen_ms = t1.elapsed().as_millis();
  assert_eq!(p.get(b"k059999").unwrap().unwrap(), b"v59999");
  eprintln!("PERF N={N} write_ms={write_ms} reopen_replay_ms={reopen_ms}");
}
