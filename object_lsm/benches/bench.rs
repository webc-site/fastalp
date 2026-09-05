//! Benchmarks: memtable hot reads/writes, flushed-segment point reads, scans
//! and merge compaction on the in-memory Store.

use std::sync::{
  Arc, OnceLock,
  atomic::{AtomicU64, Ordering},
};

use divan::Bencher;
use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, MemoryStore, ObjectLsm};

fn main() {
  divan::main();
}

const N: usize = 50_000;

struct BenchDb {
  _store: MemoryStore,
  part: wedb_object_lsm::ObjectLsmPartition,
}

fn build() -> &'static BenchDb {
  static DB: OnceLock<BenchDb> = OnceLock::new();
  DB.get_or_init(|| {
    let store = MemoryStore::new();
    let cfg = Config::new("bench")
      .max_memtable_bytes(64 * 1024)
      .block_size(4 * 1024);
    let eng = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
    let part = eng.partition("data").unwrap();
    for i in 0..N {
      part
        .insert(
          format!("k{i:08}").as_bytes(),
          b"payload-32-bytes-xxxxxxxxxxxxxxxxxx",
        )
        .unwrap();
    }
    eng.compact().unwrap();
    BenchDb {
      _store: store,
      part,
    }
  })
}

#[divan::bench]
fn flushed_point_read(b: Bencher) {
  let db = build();
  let key = format!("k{:08}", N / 2).into_bytes();
  b.bench(|| db.part.get(&key).unwrap().is_some());
}

#[divan::bench]
fn flush_scan_10k(b: Bencher) {
  let db = build();
  b.bench(|| {
    let mut n = 0usize;
    for e in db.part.iter() {
      let _ = e.unwrap();
      n += 1;
      if n >= 10_000 {
        break;
      }
    }
    n
  });
}

#[divan::bench]
fn insert_memtable(b: Bencher) {
  let store = MemoryStore::new();
  let cfg = Config::new("bench-insert").max_memtable_bytes(1 << 20);
  let eng = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  let part = eng.partition("data").unwrap();
  let i = AtomicU64::new(0);
  b.bench(|| {
    let n = i.fetch_add(1, Ordering::Relaxed);
    part
      .insert(
        format!("w{n:016}").as_bytes(),
        b"payload-24-bytes-xxxxxxxxxxx",
      )
      .unwrap();
  });
}
