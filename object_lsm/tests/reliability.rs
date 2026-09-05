//! Reliability / performance alignment tests.

use std::sync::Arc;

use wedb_embed_engine::{Engine, KvEntry, Partition};
use wedb_object_lsm::{Config, MemoryStore, ObjectLsm, Store, keys::journal_prefix};

#[test]
fn graceful_drop_flushes_pending_windowed_journal() {
  let store = MemoryStore::new();
  let cfg = Config::new("rel/drop")
    .max_memtable_bytes(1 << 20)
    .journal_window_ms(Some(60_000)); // long window: nothing auto-flushes
  let eng = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  let p = eng.partition("data").unwrap();
  for i in 0..30u32 {
    p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
      .unwrap();
  }
  assert!(
    store.list(&journal_prefix("rel/drop")).unwrap().is_empty(),
    "long window must not auto-flush yet"
  );
  drop(p);
  drop(eng); // graceful close must flush the pending journal

  let objs = store.list(&journal_prefix("rel/drop")).unwrap();
  assert!(
    !objs.is_empty(),
    "drop should flush buffered journal groups"
  );

  let eng2 = ObjectLsm::open(
    Arc::new(store.clone()),
    Config::new("rel/drop")
      .max_memtable_bytes(1 << 20)
      .journal_window_ms(Some(60_000)),
  )
  .unwrap();
  let p2 = eng2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 30);
  assert_eq!(p2.get(b"k029").unwrap().unwrap(), b"v29");
}

#[test]
fn approximate_len_is_fast_and_overcounts_duplicates() {
  let store = MemoryStore::new();
  let cfg = Config::new("rel/approx")
    .max_memtable_bytes(40) // each insert spills its own segment
    .max_segments_before_compact(1_000_000); // keep duplicates across segments
  let eng = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  let p = eng.partition("data").unwrap();
  p.insert(b"k", b"v0").unwrap();
  p.insert(b"k", b"v1").unwrap();
  p.insert(b"other", b"x").unwrap();

  // Live view has 2 keys, but segments hold 3 raw live entries (k twice).
  assert_eq!(p.len().unwrap(), 2);
  assert_eq!(p.approximate_len().unwrap(), 3);
  // And it must never be less than the exact live count.
  assert!(p.approximate_len().unwrap() >= p.len().unwrap());
}

#[test]
fn prefix_with_trailing_ff_returns_only_matching_keys() {
  let store = MemoryStore::new();
  let cfg = Config::new("rel/prefix-ff").max_memtable_bytes(1 << 20);
  let db = ObjectLsm::open(Arc::new(store), cfg).unwrap();
  let p = db.partition("data").unwrap();
  for k in [&b"a\xff0"[..], b"a\xff1", b"a\xff", b"b", b"c"] {
    p.insert(k, b"v").unwrap();
  }
  let got: Vec<Vec<u8>> = p
    .prefix(b"a\xff")
    .map(|e| e.unwrap().key().to_vec())
    .collect();
  let mut want = vec![b"a\xff".to_vec(), b"a\xff0".to_vec(), b"a\xff1".to_vec()];
  want.sort();
  assert_eq!(got, want, "prefix ending in 0xFF must not leak b/c keys");
}

#[test]
fn disk_space_includes_journal_and_manifest() {
  let store = MemoryStore::new();
  let cfg = Config::new("rel/disk")
    .max_memtable_bytes(1 << 20)
    .journal_window_ms(Some(60_000));
  let db = ObjectLsm::open(Arc::new(store), cfg).unwrap();
  let p = db.partition("data").unwrap();
  for i in 0..10u32 {
    p.insert(format!("k{i}").as_bytes(), b"v").unwrap();
  }
  db.persist().unwrap(); // flush pending journal (memtable not yet flushed)
  assert!(
    db.journal_disk_space().unwrap() > 0,
    "journal object should be counted"
  );
  assert!(db.disk_space().unwrap() > 0);
}
