//! M2 streaming iterator tests: multi-segment merges with overwrites and
//! tombstones, forward/backward/mixed-direction scans, bounds, multi-block.

use std::{collections::BTreeMap, sync::Arc};

use wedb_embed_engine::{Engine, KvEntry, Partition};
use wedb_object_lsm::{Config, MemoryStore, ObjectLsm};

const N: u32 = 600;

fn key(i: u32) -> Vec<u8> {
  format!("k{i:03}").into_bytes()
}

fn val(i: u32, g: u32) -> Vec<u8> {
  format!("v{i:03}-{g}").into_bytes()
}

fn open(prefix: &str) -> (ObjectLsm, MemoryStore) {
  let store = MemoryStore::new();
  let cfg = Config::new(prefix)
    .max_memtable_bytes(1 << 20)
    .block_size(64);
  let db = ObjectLsm::open(Arc::new(store.clone()), cfg).unwrap();
  (db, store)
}

/// Build 3 segments with overwrites + deletes; returns the expected live map.
fn build(db: &ObjectLsm) -> BTreeMap<Vec<u8>, Vec<u8>> {
  let p = db.partition("data").unwrap();
  let mut expect: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();

  // segment 1: all keys, gen 0
  for i in 0..N {
    let k = key(i);
    let v = val(i, 0);
    p.insert(&k, &v).unwrap();
    expect.insert(k, v);
  }
  db.compact().unwrap();

  // segment 2: overwrite i%3==0, delete i%7==0
  for i in 0..N {
    let k = key(i);
    if i % 7 == 0 {
      p.rm(&k).unwrap();
      expect.remove(&k);
    } else if i % 3 == 0 {
      let v = val(i, 1);
      p.insert(&k, &v).unwrap();
      expect.insert(k.clone(), v);
    }
  }
  db.compact().unwrap();

  // segment 3: overwrite i%5==0 (and not deleted), delete i%11==0
  for i in 0..N {
    let k = key(i);
    if i % 11 == 0 {
      p.rm(&k).unwrap();
      expect.remove(&k);
    } else if i % 5 == 0 {
      let v = val(i, 2);
      p.insert(&k, &v).unwrap();
      expect.insert(k.clone(), v);
    }
  }
  db.compact().unwrap();
  expect
}

fn collect_fwd(p: &impl Partition) -> Vec<(Vec<u8>, Vec<u8>)> {
  p.iter()
    .map(|e| {
      let e = e.unwrap();
      (e.key().to_vec(), e.value().to_vec())
    })
    .collect()
}

fn expect_vec(map: &BTreeMap<Vec<u8>, Vec<u8>>) -> Vec<(Vec<u8>, Vec<u8>)> {
  map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

#[test]
fn forward_scan_merges_segments_and_shadows_deletes() {
  let (db, _store) = open("s/fwd");
  let p = db.partition("data").unwrap();
  let expect = build(&db);
  let got = collect_fwd(&p);
  assert_eq!(got, expect_vec(&expect));
  // every point read must match the simulated live map (no wrong-neighbor,
  // no stale resurrection that the later writes did not really recreate)
  for i in 0..N {
    let k = key(i);
    assert_eq!(p.get(&k).unwrap(), expect.get(&k).cloned(), "key {i}");
  }
}

#[test]
fn backward_scan_matches_reversed_forward() {
  let (db, _store) = open("s/back");
  let p = db.partition("data").unwrap();
  let expect = build(&db);
  let fwd = collect_fwd(&p);
  let mut it = p.iter();
  let mut back = Vec::new();
  loop {
    match it.next_back() {
      Some(Ok(e)) => back.push((e.key().to_vec(), e.value().to_vec())),
      Some(Err(e)) => panic!("scan error: {e}"),
      None => break,
    }
  }
  let mut rev = fwd.clone();
  rev.reverse();
  assert_eq!(back, rev);
  assert_eq!(fwd, expect_vec(&expect));
}

#[test]
fn mixed_direction_no_overlap_no_dup() {
  let (db, _store) = open("s/mixed");
  let p = db.partition("data").unwrap();
  let expect = build(&db);
  let full = expect_vec(&expect);

  let mut it = p.iter();
  let mut head = Vec::new();
  for _ in 0..5 {
    let e = it.next().unwrap().unwrap();
    head.push((e.key().to_vec(), e.value().to_vec()));
  }
  let mut tail = Vec::new();
  for _ in 0..7 {
    let e = it.next_back().unwrap().unwrap();
    tail.push((e.key().to_vec(), e.value().to_vec()));
  }
  let mut rest = Vec::new();
  loop {
    match it.next() {
      Some(Ok(e)) => rest.push((e.key().to_vec(), e.value().to_vec())),
      Some(Err(e)) => panic!("scan error: {e}"),
      None => break,
    }
  }

  assert_eq!(head, full[..5]);
  let mut exp_tail = full[full.len() - 7..].to_vec();
  exp_tail.reverse();
  assert_eq!(tail, exp_tail);
  assert_eq!(rest, full[5..full.len() - 7]);
}

#[test]
fn streaming_range_and_prefix_bounds() {
  let (db, _store) = open("s/range");
  let p = db.partition("data").unwrap();
  let expect = build(&db);

  use std::ops::Bound;
  let lo = Bound::Included(&key(150)[..]);
  let hi = Bound::Excluded(&key(170)[..]);
  let ranged: Vec<Vec<u8>> = p
    .range((lo, hi))
    .map(|e| e.unwrap().key().to_vec())
    .collect();
  let exp: Vec<Vec<u8>> = expect
    .range(key(150)..key(170))
    .map(|(k, _)| k.clone())
    .collect();
  assert_eq!(ranged, exp);

  let prefixed: Vec<Vec<u8>> = p
    .prefix(b"k12")
    .map(|e| e.unwrap().key().to_vec())
    .collect();
  let exp_pre: Vec<Vec<u8>> = expect
    .keys()
    .filter(|k| k.starts_with(b"k12"))
    .cloned()
    .collect();
  assert_eq!(prefixed, exp_pre);
  assert!(!prefixed.is_empty());
}

#[test]
fn reopen_streaming_matches_forward() {
  let (db, store) = open("s/reopen");
  let expect = build(&db);
  drop(db);
  let db2 = ObjectLsm::open(
    Arc::new(store),
    Config::new("s/reopen")
      .max_memtable_bytes(1 << 20)
      .block_size(64),
  )
  .unwrap();
  let p2 = db2.partition("data").unwrap();
  assert_eq!(collect_fwd(&p2), expect_vec(&expect));
}
#[test]
fn mixed_direction_interleaved_matches_full() {
  let (db, _store) = open("s/mixed-interleaved");
  let p = db.partition("data").unwrap();
  let expect = build(&db);
  let full = expect_vec(&expect);

  let mut it = p.iter();
  let mut got = Vec::new();
  let mut front_done = false;
  let mut back_done = false;
  while !front_done || !back_done {
    if !front_done {
      match it.next() {
        Some(Ok(e)) => got.push((e.key().to_vec(), e.value().to_vec())),
        Some(Err(e)) => panic!("scan error: {e}"),
        None => front_done = true,
      }
    }
    if !back_done {
      match it.next_back() {
        Some(Ok(e)) => got.push((e.key().to_vec(), e.value().to_vec())),
        Some(Err(e)) => panic!("scan error: {e}"),
        None => back_done = true,
      }
    }
  }

  let mut sorted = got.clone();
  sorted.sort();
  assert_eq!(sorted, full);

  let mut seen = std::collections::BTreeSet::new();
  for (k, _) in &got {
    assert!(
      seen.insert(k.clone()),
      "duplicate key {}",
      String::from_utf8_lossy(k)
    );
  }
}

#[test]
fn mixed_direction_boundary_duplicate_and_tombstone() {
  let (db, _store) = open("s/mixed-boundary");
  let p = db.partition("data").unwrap();
  let mut expect: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();

  // Two segments: the first contains every key, the second overwrites k012 and
  // deletes k013, so the newest-wins duplicate and the tombstone both sit on
  // the boundary between the forward prefix and the backward suffix below.
  for i in 0..26 {
    let k = key(i);
    let v = val(i, 0);
    p.insert(&k, &v).unwrap();
    expect.insert(k, v);
  }
  db.compact().unwrap();

  let k_dup = key(12);
  let v_new = val(12, 1);
  p.insert(&k_dup, &v_new).unwrap();
  expect.insert(k_dup.clone(), v_new.clone());
  let k_tomb = key(13);
  p.rm(&k_tomb).unwrap();
  expect.remove(&k_tomb);
  db.compact().unwrap();

  let full = expect_vec(&expect);
  let mut it = p.iter();

  let mut head = Vec::new();
  for _ in 0..13 {
    let e = it.next().unwrap().unwrap();
    head.push((e.key().to_vec(), e.value().to_vec()));
  }
  let mut tail = Vec::new();
  for _ in 0..5 {
    let e = it.next_back().unwrap().unwrap();
    tail.push((e.key().to_vec(), e.value().to_vec()));
  }
  let mut rest = Vec::new();
  loop {
    match it.next() {
      Some(Ok(e)) => rest.push((e.key().to_vec(), e.value().to_vec())),
      Some(Err(e)) => panic!("scan error: {e}"),
      None => break,
    }
  }

  assert_eq!(head, full[..13]);
  let mut exp_tail = full[full.len() - 5..].to_vec();
  exp_tail.reverse();
  assert_eq!(tail, exp_tail);
  assert_eq!(rest, full[13..full.len() - 5]);
  assert!(!head.iter().any(|(k, _)| *k == k_tomb));
  assert!(!rest.iter().any(|(k, _)| *k == k_tomb));
  assert!(head.iter().any(|(k, v)| *k == key(12) && *v == val(12, 1)));
}
