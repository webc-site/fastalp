//! Configurable object-deletion policy tests.
//!
//! `Config::eager_object_delete = false` makes compaction, `clear_partition`,
//! `rm_partition` and startup GC leave replaced segment objects in the store
//! for an external lifecycle rule to reclaim. Reads and recovery stay correct;
//! only the object-store DELETE volume changes.

use std::sync::Arc;

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{
  Config, MemoryStore, ObjectLsm, ObjectLsmPartition, Store, keys::segment_root,
};

const N: u32 = 60;

fn key(i: u32) -> Vec<u8> {
  format!("k{i:03}").into_bytes()
}

fn cfg(prefix: &str, eager: bool) -> Config {
  Config::new(prefix)
    .max_memtable_bytes(40)
    .block_size(64)
    .max_segments_before_compact(1_000_000)
    .eager_object_delete(eager)
}

fn seed(p: &ObjectLsmPartition) {
  for i in 0..N {
    p.insert(&key(i), format!("v{i}-0").as_bytes()).unwrap();
  }
  for i in 0..N {
    let k = key(i);
    if i % 7 == 0 {
      p.rm(&k).unwrap();
    } else if i % 3 == 0 {
      p.insert(&k, format!("v{i}-1").as_bytes()).unwrap();
    }
  }
}

fn assert_expected_data(p: &ObjectLsmPartition) {
  let mut expect = 0usize;
  for i in 0..N {
    let k = key(i);
    if i % 7 == 0 {
      assert_eq!(
        p.get(&k).unwrap(),
        None,
        "deleted key {k:?} should be absent"
      );
    } else {
      expect += 1;
      let want = if i % 3 == 0 {
        format!("v{i}-1")
      } else {
        format!("v{i}-0")
      };
      assert_eq!(p.get(&k).unwrap().unwrap(), want.into_bytes());
    }
  }
  assert_eq!(p.len().unwrap(), expect);
}

#[test]
fn lazy_compaction_keeps_replaced_segments_and_recovers() {
  let store = MemoryStore::new();
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg("lazy/merge", false)).unwrap();
  let p = db.partition("data").unwrap();
  seed(&p);
  assert!(
    p.table_count() > 1,
    "expected multiple pre-compaction segments, got {}",
    p.table_count()
  );

  let before = store.list(&segment_root("lazy/merge")).unwrap();
  assert_eq!(before.len(), p.table_count());
  db.compact().unwrap();
  assert_eq!(p.table_count(), 1, "compaction must merge to one segment");

  let after = store.list(&segment_root("lazy/merge")).unwrap();
  assert!(
    after.len() > before.len(),
    "replaced segment objects must not be deleted (before={}, after={})",
    before.len(),
    after.len()
  );
  for old in &before {
    assert!(
      after.contains(old),
      "old segment object {old} missing after lazy compaction"
    );
  }

  drop(db);
  let db2 = ObjectLsm::open(Arc::new(store), cfg("lazy/merge", false)).unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_expected_data(&p2);
  assert_eq!(p2.table_count(), 1);
}

#[test]
fn eager_compaction_deletes_replaced_segments() {
  let store = MemoryStore::new();
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg("lazy/merge-eager", true)).unwrap();
  let p = db.partition("data").unwrap();
  seed(&p);
  assert!(p.table_count() > 1);

  let before = store.list(&segment_root("lazy/merge-eager")).unwrap();
  db.compact().unwrap();
  assert_eq!(p.table_count(), 1);

  let after = store.list(&segment_root("lazy/merge-eager")).unwrap();
  assert_eq!(
    after.len(),
    1,
    "only the merged segment should remain, got {after:?}"
  );
  for old in &before {
    assert!(
      !after.contains(old),
      "old segment object {old} should be deleted eagerly"
    );
  }

  drop(db);
  let db2 = ObjectLsm::open(Arc::new(store), cfg("lazy/merge-eager", true)).unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_expected_data(&p2);
}

#[test]
fn lazy_clear_keeps_segment_objects() {
  let store = MemoryStore::new();
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg("lazy/clear", false)).unwrap();
  let p = db.partition("data").unwrap();
  for i in 0..20u32 {
    p.insert(&key(i), b"v").unwrap();
  }
  assert!(p.table_count() >= 2);

  let before = store.list(&segment_root("lazy/clear")).unwrap();
  p.clear().unwrap();
  assert!(p.is_empty().unwrap());
  assert_eq!(p.table_count(), 0);

  let after = store.list(&segment_root("lazy/clear")).unwrap();
  assert_eq!(
    before.len(),
    after.len(),
    "clear must keep segment objects when eager_object_delete is false"
  );

  drop(db);
  let db2 = ObjectLsm::open(Arc::new(store), cfg("lazy/clear", false)).unwrap();
  let p2 = db2.partition("data").unwrap();
  assert!(p2.is_empty().unwrap());
}

#[test]
fn lazy_rm_partition_keeps_segment_objects() {
  let store = MemoryStore::new();
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg("lazy/rm", false)).unwrap();
  let p = db.partition("data").unwrap();
  for i in 0..20u32 {
    p.insert(&key(i), b"v").unwrap();
  }
  assert!(p.table_count() >= 2);

  let before = store.list(&segment_root("lazy/rm")).unwrap();
  db.rm_partition(&p).unwrap();
  assert!(!db.partition_exists("data"));

  let after = store.list(&segment_root("lazy/rm")).unwrap();
  assert_eq!(
    before.len(),
    after.len(),
    "rm_partition must keep segment objects when eager_object_delete is false"
  );

  drop(db);
  let db2 = ObjectLsm::open(Arc::new(store), cfg("lazy/rm", false)).unwrap();
  assert!(!db2.partition_exists("data"));
  let p2 = db2.partition("data").unwrap();
  assert!(p2.is_empty().unwrap());
}
