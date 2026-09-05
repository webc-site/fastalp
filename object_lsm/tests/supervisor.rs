//! Supervisor (standby -> promote -> handle -> re-standby) tests.

use std::{
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  thread,
  time::Duration,
};

use wedb_embed_engine::{Engine, Partition};
use wedb_object_lsm::{Config, LeaseOptions, MemoryStore, ObjectLsm, Supervisor};

#[test]
fn supervisor_promotes_across_terms_and_persists() {
  let s = MemoryStore::new();
  let cfg = Config::new("sup/1")
    .max_memtable_bytes(1 << 20)
    .max_segments_before_compact(1_000_000);
  let opts = LeaseOptions {
    owner: "w".into(),
    ttl: Duration::from_millis(150),
    timeout: Duration::from_millis(500),
    heartbeat: false,
  };
  let sup = Supervisor::new(
    Arc::new(s.clone()),
    cfg.clone(),
    opts,
    Duration::from_millis(10),
  );
  let stop = Arc::new(AtomicBool::new(false));
  let terms = Arc::new(AtomicUsize::new(0));
  let stop2 = stop.clone();
  let terms2 = terms.clone();
  let handle = thread::spawn(move || {
    sup
      .run(&stop2, |eng| {
        let n = terms2.fetch_add(1, Ordering::SeqCst);
        let p = eng.partition("data").unwrap();
        p.insert(format!("k{n:03}").as_bytes(), format!("v{n}").as_bytes())
          .unwrap();
        eng.compact().unwrap();
        Ok(())
      })
      .unwrap()
  });

  let deadline = std::time::Instant::now() + Duration::from_secs(5);
  while terms.load(Ordering::SeqCst) < 3 {
    assert!(
      std::time::Instant::now() < deadline,
      "supervisor never served 3 terms"
    );
    thread::sleep(Duration::from_millis(10));
  }
  stop.store(true, Ordering::SeqCst);
  let served = handle.join().unwrap();
  assert!(served >= 3, "served at least the observed terms");

  // Every leadership term's write survived, even though the writer changed.
  let db = ObjectLsm::open(Arc::new(s), cfg).unwrap();
  let p = db.partition("data").unwrap();
  assert_eq!(p.len().unwrap(), served);
  assert_eq!(p.get(b"k000").unwrap().unwrap(), b"v0");
  assert_eq!(
    p.get(format!("k{:03}", served - 1).as_bytes())
      .unwrap()
      .unwrap(),
    format!("v{}", served - 1).into_bytes()
  );
}
