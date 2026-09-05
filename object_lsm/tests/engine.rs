//! M1 integration tests: put/get, flush persistence, crash recovery via
//! journal replay, cross-partition atomic batch, tombstones, iteration order,
//! prefix/range scans, clear and rm_partition.

use std::sync::Arc;

use wedb_embed_engine::{Batch, Engine, KvEntry, Partition};
use wedb_object_lsm::{Config, MemoryStore, ObjectLsm};

fn open(prefix: &str, max_memtable: u64) -> (ObjectLsm, MemoryStore) {
  let store = MemoryStore::new();
  let cfg = Config::new(prefix).max_memtable_bytes(max_memtable);
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  (db, store)
}

fn reopen(store: &MemoryStore, prefix: &str, max_memtable: u64) -> ObjectLsm {
  ObjectLsm::open(
    Arc::new(store.clone()),
    Config::new(prefix).max_memtable_bytes(max_memtable),
  )
  .unwrap()
}

fn keys_of(p: &impl Partition) -> Vec<Vec<u8>> {
  p.iter().map(|e| e.unwrap().key().to_vec()).collect()
}

#[test]
fn put_get_rm_basic() {
  let (db, _store) = open("t/basic", 1 << 20);
  let p = db.partition("data").unwrap();
  assert!(!p.contains_key(b"a").unwrap());
  p.insert(b"a", b"1").unwrap();
  p.insert(b"b", b"2").unwrap();
  assert_eq!(p.get(b"a").unwrap().unwrap(), b"1");
  assert_eq!(p.size_of(b"a").unwrap(), Some(1));
  assert!(p.contains_key(b"a").unwrap());
  assert_eq!(p.len().unwrap(), 2);
  p.rm(b"a").unwrap();
  assert_eq!(p.get(b"a").unwrap(), None);
  assert_eq!(p.len().unwrap(), 1);
  p.clear().unwrap();
  assert!(p.is_empty().unwrap());
}

#[test]
fn journal_replay_survives_without_flush() {
  // Large memtable budget: no segment flush; durability comes from journal.
  let (db, store) = open("t/replay", 1 << 20);
  let p = db.partition("data").unwrap();
  for i in 0..100u32 {
    p.insert(format!("k{i:03}").as_bytes(), b"v").unwrap();
  }
  drop(db);

  let db2 = reopen(&store, "t/replay", 1 << 20);
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 100);
  assert_eq!(p2.get(b"k042").unwrap().unwrap(), b"v");
}

#[test]
fn segment_flush_survives_reopen() {
  // Tiny memtable budget forces flush into immutable segments.
  let (db, store) = open("t/flush", 32);
  let p = db.partition("data").unwrap();
  for i in 0..50u32 {
    p.insert(
      format!("key{i:03}").as_bytes(),
      format!("val{i}").as_bytes(),
    )
    .unwrap();
  }
  assert!(
    p.table_count() >= 2,
    "expected multiple segments, got {}",
    p.table_count()
  );
  drop(db);

  let db2 = reopen(&store, "t/flush", 32);
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 50);
  for i in (0..50u32).step_by(7) {
    assert_eq!(
      p2.get(format!("key{i:03}").as_bytes()).unwrap().unwrap(),
      format!("val{i}").as_bytes()
    );
  }
}

#[test]
fn tombstone_shadows_older_segment_after_reopen() {
  let (db, store) = open("t/tomb", 32);
  let p = db.partition("data").unwrap();
  p.insert(b"k", b"v1").unwrap(); // flush seg1
  p.rm(b"k").unwrap(); // flush seg2 with tombstone
  assert_eq!(p.get(b"k").unwrap(), None);
  assert!(p.is_empty().unwrap());
  drop(db);

  let db2 = reopen(&store, "t/tomb", 32);
  let p2 = db2.partition("data").unwrap();
  assert_eq!(
    p2.get(b"k").unwrap(),
    None,
    "tombstone must shadow old segment value"
  );
  assert!(p2.is_empty().unwrap());
}

#[test]
fn cross_partition_batch_is_atomic() {
  let (db, store) = open("t/batch", 32);
  let a = db.partition("a").unwrap();
  let m = db.partition("m").unwrap();
  let mut b = db.batch();
  b.insert(&a, b"k", b"va");
  b.insert(&m, b"meta", b"vm");
  assert_eq!(b.len(), 2);
  b.commit().unwrap();

  let db2 = reopen(&store, "t/batch", 32);
  let a2 = db2.partition("a").unwrap();
  let m2 = db2.partition("m").unwrap();
  assert_eq!(a2.get(b"k").unwrap().unwrap(), b"va");
  assert_eq!(m2.get(b"meta").unwrap().unwrap(), b"vm");
}

#[test]
fn iteration_order_range_prefix_double_ended() {
  let (db, _store) = open("t/iter", 1 << 20);
  let p = db.partition("data").unwrap();
  for (k, v) in [
    ("3", "c"),
    ("1", "a"),
    ("2", "b"),
    ("user:1", "u1"),
    ("user:2", "u2"),
  ] {
    p.insert(k.as_bytes(), v.as_bytes()).unwrap();
  }
  let all = keys_of(&p);
  let expect: Vec<&[u8]> = vec![b"1", b"2", b"3", b"user:1", b"user:2"];
  assert_eq!(all, expect);

  // double-ended from the back
  let mut it = p.iter();
  let last = it.next_back().unwrap().unwrap();
  assert_eq!(last.key().to_vec(), b"user:2");

  // prefix scan
  let prefixed: Vec<Vec<u8>> = p
    .prefix(b"user:")
    .map(|e| e.unwrap().key().to_vec())
    .collect();
  assert_eq!(prefixed, vec![b"user:1".to_vec(), b"user:2".to_vec()]);

  // range scan [2, user:1]
  use std::ops::Bound;
  let ranged: Vec<Vec<u8>> = p
    .range((Bound::Included(&b"2"[..]), Bound::Included(&b"user:1"[..])))
    .map(|e| e.unwrap().key().to_vec())
    .collect();
  assert_eq!(
    ranged,
    vec![b"2".to_vec(), b"3".to_vec(), b"user:1".to_vec()]
  );

  // first / last entry
  assert_eq!(p.first_entry().unwrap().unwrap().key().to_vec(), b"1");
  assert_eq!(p.last_entry().unwrap().unwrap().key().to_vec(), b"user:2");
}

#[test]
fn rm_partition_and_recreate() {
  let (db, store) = open("t/rmpart", 32);
  let p = db.partition("data").unwrap();
  p.insert(b"k", b"v").unwrap();
  assert!(db.partition_exists("data"));
  db.rm_partition(&p).unwrap();
  assert!(!db.partition_exists("data"));
  drop(db);

  // Reopen: partition entry survives (dropped) so stale journal groups never
  // resurrect; a fresh partition() recreates it empty.
  let db2 = reopen(&store, "t/rmpart", 32);
  assert!(!db2.partition_exists("data"));
  let p2 = db2.partition("data").unwrap();
  assert!(db2.partition_exists("data"));
  assert!(p2.is_empty().unwrap());
  assert_eq!(
    p2.get(b"k").unwrap(),
    None,
    "dropped data must not resurrect"
  );
}

#[test]
fn disk_space_and_metrics() {
  let (db, _store) = open("t/metrics", 32);
  let p = db.partition("data").unwrap();
  for i in 0..20u32 {
    p.insert(format!("k{i}").as_bytes(), b"payload-payload")
      .unwrap();
  }
  assert!(db.disk_space().unwrap() > 0);
  assert!(db.write_buffer_size() <= 32);
}

#[test]
fn multi_block_segment_reads_via_range() {
  // One flush with many small blocks: point lookups and scans must hit the
  // right block via the block index (Range GET path), not full-object reads.
  let store = MemoryStore::new();
  let cfg = Config::new("t/blocks")
    .max_memtable_bytes(1 << 20)
    .block_size(64);
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  let p = db.partition("data").unwrap();
  for i in 0..2000u32 {
    p.insert(
      format!("key{i:05}").as_bytes(),
      format!("value-{i}").as_bytes(),
    )
    .unwrap();
  }
  db.compact().unwrap(); // single segment, many blocks
  assert_eq!(p.table_count(), 1);
  for i in (0..2000u32).step_by(13) {
    assert_eq!(
      p.get(format!("key{i:05}").as_bytes()).unwrap().unwrap(),
      format!("value-{i}").as_bytes()
    );
  }
  assert!(
    db.cache_size() > 0,
    "block cache should be populated by reads"
  );
  let prefixed: Vec<Vec<u8>> = p
    .prefix(b"key01")
    .map(|e| e.unwrap().key().to_vec())
    .collect();
  assert_eq!(prefixed.len(), 1000);
  assert_eq!(prefixed.first().unwrap(), b"key01000");
  drop(db);

  let db2 = reopen(&store, "t/blocks", 1 << 20);
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.get(b"key01999").unwrap().unwrap(), b"value-1999");
  assert_eq!(p2.len().unwrap(), 2000);
}
