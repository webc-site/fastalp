//! Process-level crash injection: spawn a child test binary that aborts at a
//! precise durability step, then reopen the same local FileStore and verify
//! crash consistency.

use std::{process::Command, sync::Arc};

use tempfile::tempdir;
use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, FileStore, LeaseOptions, ObjectLsm, Store};

fn spawn_child(mode: &str, dir: &str) {
  let status = Command::new(std::env::current_exe().unwrap())
    .args(["--exact", "crash_child", "--nocapture"])
    .env("CRASH_MODE", mode)
    .env("CRASH_DIR", dir)
    .status()
    .expect("spawn child");
  assert!(!status.success(), "child should have aborted (mode {mode})");
}

fn reopen(dir: &str, memtable: u64) -> ObjectLsm {
  ObjectLsm::open(
    Arc::new(FileStore::new(dir).unwrap()),
    Config::new("crash").max_memtable_bytes(memtable),
  )
  .unwrap()
}

#[test]
fn journal_recovery_after_crash() {
  let d = tempdir().unwrap();
  spawn_child("journal", d.path().to_str().unwrap());
  let db = reopen(d.path().to_str().unwrap(), 1 << 20);
  let p = db.partition("data").unwrap();
  assert_eq!(p.len().unwrap(), 20);
  assert_eq!(p.get(b"k019").unwrap().unwrap(), b"v19");
}

#[test]
fn flush_recovery_after_crash() {
  let d = tempdir().unwrap();
  spawn_child("flush", d.path().to_str().unwrap());
  let db = reopen(d.path().to_str().unwrap(), 1 << 20);
  let p = db.partition("data").unwrap();
  assert_eq!(p.len().unwrap(), 20);
  assert_eq!(p.get(b"k019").unwrap().unwrap(), b"v19");
}

#[test]
fn segment_put_crash_recovers_from_journal() {
  let d = tempdir().unwrap();
  spawn_child("seg_put", d.path().to_str().unwrap());
  let db = reopen(d.path().to_str().unwrap(), 1 << 20);
  let p = db.partition("data").unwrap();
  assert_eq!(p.len().unwrap(), 20);
}

#[test]
fn clear_crash_keeps_state_consistent() {
  let d = tempdir().unwrap();
  spawn_child("clear_delete", d.path().to_str().unwrap());
  let db = reopen(d.path().to_str().unwrap(), 1 << 20);
  let p = db.partition("data").unwrap();
  assert!(
    p.is_empty().unwrap(),
    "cleared state must remain cleared after crash"
  );
}

#[test]
fn compact_crash_keeps_state_consistent() {
  let d = tempdir().unwrap();
  spawn_child("compact_delete", d.path().to_str().unwrap());
  let db = reopen(d.path().to_str().unwrap(), 1 << 20);
  let p = db.partition("data").unwrap();
  assert_eq!(p.len().unwrap(), 20);
  assert_eq!(p.get(b"k007").unwrap().unwrap(), b"v7");
}

#[test]
fn crash_child() {
  let Some(mode) = std::env::var("CRASH_MODE").ok() else {
    return;
  };
  let dir = std::env::var("CRASH_DIR").unwrap();
  let store = Arc::new(FileStore::new(&dir).unwrap());
  match mode.as_str() {
    "journal" => {
      let cfg = Config::new("crash").max_memtable_bytes(1 << 20);
      let db = ObjectLsm::open(store, cfg).unwrap();
      let p = db.partition("data").unwrap();
      for i in 0..20u32 {
        p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
          .unwrap();
      }
      std::process::abort();
    }
    "flush" => {
      let cfg = Config::new("crash").max_memtable_bytes(1 << 20);
      let db = ObjectLsm::open(store, cfg).unwrap();
      let p = db.partition("data").unwrap();
      for i in 0..20u32 {
        p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
          .unwrap();
      }
      db.compact().unwrap();
      std::process::abort();
    }
    "seg_put" => {
      // Abort inside FileStore::put for the segment object: the segment upload
      // dies before the manifest is published, so recovery must replay journal.
      unsafe { std::env::set_var("WEDB_FS_ABORT_ON_PUT_SEGMENT", "1") };
      let cfg = Config::new("crash").max_memtable_bytes(1 << 20);
      let db = ObjectLsm::open(store, cfg).unwrap();
      let p = db.partition("data").unwrap();
      for i in 0..20u32 {
        p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
          .unwrap();
      }
      let _ = db.compact();
      std::process::abort();
    }
    "clear_delete" => {
      // Abort inside FileStore::delete for the old segment objects, after
      // clear() has already published the cleared manifest.
      unsafe { std::env::set_var("WEDB_FS_ABORT_ON_DELETE_SEGMENT", "1") };
      let cfg = Config::new("crash")
        .max_memtable_bytes(40)
        .max_segments_before_compact(1_000_000);
      let db = ObjectLsm::open(store, cfg).unwrap();
      let p = db.partition("data").unwrap();
      for i in 0..20u32 {
        p.insert(format!("k{i:03}").as_bytes(), b"v").unwrap();
      }
      p.clear().unwrap();
      std::process::abort();
    }
    "compact_delete" => {
      // Abort inside FileStore::delete for the superseded segments, after
      // compaction has published the merged manifest.
      unsafe { std::env::set_var("WEDB_FS_ABORT_ON_DELETE_SEGMENT", "1") };
      let cfg = Config::new("crash")
        .max_memtable_bytes(40)
        .max_segments_before_compact(1_000_000);
      let db = ObjectLsm::open(store, cfg).unwrap();
      let p = db.partition("data").unwrap();
      for i in 0..20u32 {
        p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
          .unwrap();
      }
      db.compact().unwrap();
      std::process::abort();
    }
    "leased_anchor" => {
      // Leased writer acks 20 strict-mode writes and dies before publishing its
      // first data-bearing manifest: recovery + takeover must replay the acked
      // journals.
      let cfg = Config::new("crash").max_memtable_bytes(1 << 20);
      let db = ObjectLsm::open_leased(
        store,
        cfg,
        LeaseOptions {
          owner: "w0".into(),
          ttl: std::time::Duration::from_millis(250),
          timeout: std::time::Duration::from_millis(1_000),
          heartbeat: false,
        },
      )
      .unwrap();
      let p = db.partition("data").unwrap();
      for i in 0..20u32 {
        p.insert(format!("k{i:03}").as_bytes(), format!("v{i}").as_bytes())
          .unwrap();
      }
      std::process::abort();
    }
    _ => {}
  }
}

/// A leased writer aborts via SIGABRT after acknowledged strict-mode writes but
/// BEFORE its first manifest publish. The lease object is left in place; after
/// it expires a successor takes over and must recover every acked write from
/// the predecessor's journals (process-level crash, FileStore backend).
#[test]
fn leased_crash_before_first_manifest_takeover_recovers() {
  let d = tempdir().unwrap();
  spawn_child("leased_anchor", d.path().to_str().unwrap());
  std::thread::sleep(std::time::Duration::from_millis(600)); // lease expires

  let store = Arc::new(FileStore::new(d.path().to_str().unwrap()).unwrap());
  let cfg = Config::new("crash").max_memtable_bytes(1 << 20);
  let opts = |owner: &str| LeaseOptions {
    owner: owner.into(),
    ttl: std::time::Duration::from_secs(60),
    timeout: std::time::Duration::from_millis(2_000),
    heartbeat: false,
  };
  let db = ObjectLsm::open_leased(store.clone(), cfg, opts("w1")).unwrap();
  let p = db.partition("data").unwrap();
  assert_eq!(
    p.len().unwrap(),
    20,
    "takeover recovers pre-manifest acked writes"
  );
  for i in (0..20u32).step_by(5) {
    assert_eq!(
      p.get(format!("k{i:03}").as_bytes()).unwrap().unwrap(),
      format!("v{i}").into_bytes()
    );
  }
  // The predecessor's journals were folded by the takeover anchor and GC'd.
  let root = wedb_object_lsm::keys::journal_prefix("crash");
  assert!(
    store.list(&root).unwrap().is_empty(),
    "superseded-epoch journals cleaned after process-level takeover"
  );
}
