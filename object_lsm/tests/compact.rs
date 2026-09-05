//! M3 tests: merge compaction, journal GC and orphan GC at open.

use std::{
  collections::BTreeMap,
  sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  },
  thread,
  time::{Duration, Instant},
};

use wedb_embed_engine::{Engine, KvEntry, Partition};
use wedb_object_lsm::{
  Config, MemoryStore, ObjectLsm, Store,
  keys::{journal_prefix, manifest_prefix, segment_key, segment_root},
};

const N: u32 = 60;

fn key(i: u32) -> Vec<u8> {
  format!("k{i:03}").into_bytes()
}

fn open_no_auto(prefix: &str) -> (ObjectLsm, MemoryStore) {
  // max_segments very high disables auto compaction so tests control it.
  let store = MemoryStore::new();
  let cfg = Config::new(prefix)
    .max_memtable_bytes(40)
    .block_size(64)
    .max_segments_before_compact(1_000_000);
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  (db, store)
}

fn collect_map(p: &impl Partition) -> BTreeMap<Vec<u8>, Vec<u8>> {
  p.iter()
    .map(|e| {
      let e = e.unwrap();
      (e.key().to_vec(), e.value().to_vec())
    })
    .collect()
}

#[test]
fn compaction_merges_segments_and_reclaims() {
  let (db, store) = open_no_auto("m3/merge");
  let p = db.partition("data").unwrap();
  let mut expect: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
  for i in 0..N {
    let (k, v) = (key(i), format!("v{i}-0").into_bytes());
    p.insert(&k, &v).unwrap();
    expect.insert(k, v);
  }
  // overwrite every 3rd, delete every 7th -> many small segments
  for i in 0..N {
    let k = key(i);
    if i % 7 == 0 {
      p.rm(&k).unwrap();
      expect.remove(&k);
    } else if i % 3 == 0 {
      let v = format!("v{i}-1").into_bytes();
      p.insert(&k, &v).unwrap();
      expect.insert(k, v);
    }
  }
  assert!(
    p.table_count() > 4,
    "expected many pre-compaction segments, got {}",
    p.table_count()
  );

  let completed_before = db.compactions_completed();
  db.compact().unwrap();
  assert_eq!(
    p.table_count(),
    1,
    "compaction must merge to a single segment"
  );
  assert!(db.compactions_completed() > completed_before);
  assert_eq!(collect_map(&p), expect, "merged view must equal live map");

  // old segment objects must be deleted from the store
  let seg_objects = store.list(&segment_root("m3/merge")).unwrap();
  assert_eq!(
    seg_objects.len(),
    1,
    "only the merged segment should remain"
  );

  // and survive a reopen
  drop(db);
  let db2 = ObjectLsm::open(
    Arc::new(store.clone()),
    Config::new("m3/merge")
      .max_memtable_bytes(40)
      .block_size(64)
      .max_segments_before_compact(1_000_000),
  )
  .unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(collect_map(&p2), expect);
  assert_eq!(p2.table_count(), 1);
}

#[test]
fn compaction_empty_partition_drops_all_segments() {
  let (db, store) = open_no_auto("m3/empty");
  let p = db.partition("data").unwrap();
  for i in 0..20u32 {
    p.insert(&key(i), b"v").unwrap();
  }
  for i in 0..20u32 {
    p.rm(&key(i)).unwrap();
  }
  assert!(p.table_count() >= 2);
  db.compact().unwrap();
  assert_eq!(p.table_count(), 0);
  assert!(p.is_empty().unwrap());
  drop(db);
  let db2 = ObjectLsm::open(
    Arc::new(store),
    Config::new("m3/empty")
      .max_memtable_bytes(40)
      .block_size(64)
      .max_segments_before_compact(1_000_000),
  )
  .unwrap();
  let p2 = db2.partition("data").unwrap();
  assert!(p2.is_empty().unwrap());
  assert_eq!(p2.table_count(), 0);
}

#[test]
fn auto_compaction_respects_segment_limit() {
  let store = MemoryStore::new();
  let cfg = Config::new("m3/auto")
    .max_memtable_bytes(40)
    .block_size(64)
    .max_segments_before_compact(4);
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  let p = db.partition("data").unwrap();
  for i in 0..N {
    p.insert(&key(i), format!("v{i}").as_bytes()).unwrap();
  }
  assert!(
    p.table_count() <= 4,
    "auto compaction must keep segments <= 4, got {}",
    p.table_count()
  );
  assert!(
    db.compactions_completed() >= 1,
    "auto compaction should have run"
  );
  assert_eq!(p.len().unwrap(), N as usize);
  drop(db);
  let db2 = ObjectLsm::open(
    Arc::new(store),
    Config::new("m3/auto")
      .max_memtable_bytes(40)
      .block_size(64)
      .max_segments_before_compact(4),
  )
  .unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), N as usize);
}

#[test]
fn journal_gc_removes_folded_groups() {
  let (db, store) = open_no_auto("m3/jgc");
  let p = db.partition("data").unwrap();
  for i in 0..N {
    p.insert(&key(i), format!("v{i}").as_bytes()).unwrap();
  }
  // Every insert flushed + advanced the partition watermark, so applied
  // journal groups should have been deleted eagerly.
  let journals = store.list(&journal_prefix("m3/jgc")).unwrap();
  assert!(
    journals.len() < N as usize / 2,
    "journal GC lagging: {} objects left",
    journals.len()
  );
  drop(db);

  let db2 = ObjectLsm::open(
    Arc::new(store.clone()),
    Config::new("m3/jgc")
      .max_memtable_bytes(40)
      .block_size(64)
      .max_segments_before_compact(1_000_000),
  )
  .unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), N as usize);
  // Startup GC should also have run on any leftovers.
  let journals2 = store.list(&journal_prefix("m3/jgc")).unwrap();
  assert!(
    journals2.is_empty(),
    "leftover journal objects: {journals2:?}"
  );
}

#[test]
fn orphan_objects_gc_at_open() {
  let (db, store) = open_no_auto("m3/orphan");
  let p = db.partition("data").unwrap();
  for i in 0..10u32 {
    p.insert(&key(i), b"v").unwrap();
  }
  drop(db);

  // Inject orphan objects that no manifest references.
  store
    .put(
      &segment_key("m3/orphan", "data", 424_242),
      b"orphan segment",
    )
    .unwrap();
  store
    .put(
      &segment_key("m3/orphan", "ghost", 1),
      b"ghost partition segment",
    )
    .unwrap();
  store
    .put(
      &format!("{}/manifest/{:020}", "m3/orphan", 999_999_999u64),
      b"orphan manifest",
    )
    .unwrap();
  store
    .put(
      &format!("{}/manifest/{:020}", "m3/orphan", 999_999_998u64),
      b"orphan manifest2",
    )
    .unwrap();

  let db2 = ObjectLsm::open(
    Arc::new(store.clone()),
    Config::new("m3/orphan")
      .max_memtable_bytes(40)
      .block_size(64)
      .max_segments_before_compact(1_000_000),
  )
  .unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(p2.len().unwrap(), 10);

  let segs = store.list(&segment_root("m3/orphan")).unwrap();
  assert!(
    segs
      .iter()
      .all(|k| !k.ends_with("424242") && !k.contains("ghost")),
    "orphans not removed: {segs:?}"
  );
  assert_eq!(segs.len(), p2.table_count());

  let mans = store.list(&manifest_prefix("m3/orphan")).unwrap();
  let numeric: Vec<&str> = mans
    .iter()
    .filter(|k| !k.ends_with("/current"))
    .map(|s| s.as_str())
    .collect();
  assert_eq!(
    numeric.len(),
    1,
    "only the current manifest should survive, got {numeric:?}"
  );
}

#[test]
fn detached_compaction_does_not_block_writes() {
  // Build several small segments with a normal store first.
  let base = MemoryStore::new();
  let cfg_build = Config::new("m3/detached")
    .max_memtable_bytes(40)
    .block_size(64)
    .max_segments_before_compact(1_000_000);
  {
    let db = ObjectLsm::open(Arc::new(base.clone()), cfg_build).unwrap();
    let p = db.partition("data").unwrap();
    for i in 0..N {
      p.insert(&key(i), format!("v{i}").as_bytes()).unwrap();
    }
  }

  struct BlockingGetStore {
    inner: MemoryStore,
    seg_gets: Arc<AtomicUsize>,
  }
  impl Store for BlockingGetStore {
    fn get(&self, key: &str) -> wedb_object_lsm::Result<Option<Vec<u8>>> {
      if key.contains("/seg/") && self.seg_gets.fetch_add(1, Ordering::SeqCst) == 0 {
        thread::sleep(Duration::from_millis(300));
      }
      self.inner.get(key)
    }
    fn get_range(
      &self,
      key: &str,
      offset: u64,
      len: u64,
    ) -> wedb_object_lsm::Result<Option<Vec<u8>>> {
      self.inner.get_range(key, offset, len)
    }
    fn put(&self, key: &str, data: &[u8]) -> wedb_object_lsm::Result<()> {
      self.inner.put(key, data)
    }
    fn delete(&self, key: &str) -> wedb_object_lsm::Result<()> {
      self.inner.delete(key)
    }
    fn list(&self, prefix: &str) -> wedb_object_lsm::Result<Vec<String>> {
      self.inner.list(prefix)
    }
  }

  let seg_gets = Arc::new(AtomicUsize::new(0));
  let store = Arc::new(BlockingGetStore {
    inner: base,
    seg_gets: seg_gets.clone(),
  });
  let db = ObjectLsm::open(
    store,
    Config::new("m3/detached")
      .max_memtable_bytes(1 << 20)
      .block_size(64)
      .max_segments_before_compact(1_000_000),
  )
  .unwrap();
  let p = db.partition("data").unwrap();
  let cdb = db.clone();
  let compactor = thread::spawn(move || cdb.compact().unwrap());
  while seg_gets.load(Ordering::SeqCst) == 0 {
    thread::sleep(Duration::from_millis(5));
  }
  let started = Instant::now();
  p.insert(b"z-concurrent", b"new").unwrap();
  assert!(
    started.elapsed() < Duration::from_millis(200),
    "write blocked while compaction reads segments"
  );
  compactor.join().unwrap();
  assert_eq!(p.table_count(), 1, "compaction must merge");
  assert_eq!(p.get(&key(3)).unwrap().unwrap(), b"v3");
  assert_eq!(p.get(b"z-concurrent").unwrap().unwrap(), b"new");
  drop(db);
}
