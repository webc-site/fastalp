//! Engine implementation: open/recovery, commit pipeline, segment flush,
//! merge compaction, garbage collection, manifest publishing and the
//! [`Engine`] trait impl.

use std::{
  collections::BTreeMap,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  thread,
  time::{Duration, Instant},
};

use parking_lot::{Mutex, RwLock};
use wedb_embed_engine::Engine;

use crate::{
  batch::ObjectLsmBatch,
  cache::{BlockCache, IndexCache},
  config::Config,
  error::{Error, Result},
  journal::{Group, Op, decode_group_stream, encode_group},
  keys::{
    current_key, journal_key_epoch, journal_prefix, journal_prefix_epoch, manifest_key,
    manifest_prefix, parse_journal_tail, parse_tail_seq, segment_key, segment_root,
  },
  lease::{Lease, LeaseOptions, lease_key},
  manifest::Manifest,
  metrics::{Metrics, MetricsSnapshot},
  partition::ObjectLsmPartition,
  segment::{
    BLOCK_HEADER_LEN, BlockMeta, SegmentEntries, SegmentIndex, SegmentMeta, TAIL_LEN,
    build_segment_meta, decode_block, decode_index, encode_segment, find_block, parse_tail,
  },
  state::{
    EngineState, MemEntry, PartitionLock, PartitionState, PartitionTable, PendingFlush, ReaderGate,
  },
  store::Store,
};

/// Shared internals behind [`ObjectLsm`].
pub struct Inner {
  pub store: Arc<dyn Store>,
  pub state: RwLock<EngineState>,
  /// Serializes journal object PUTs so they never race while the global lock
  /// is free for partition/flush work (strict mode commits PUT outside `state`).
  pub journal_lock: Mutex<()>,
  /// Gates segment-object deletion against in-flight readers.
  pub readers: ReaderGate,
  /// Per-partition data-plane lock table (independent of `state`'s lock).
  pub partitions: PartitionTable,
  pub block_cache: BlockCache,
  pub index_cache: IndexCache,
  /// Stops the background group-commit journal flusher thread.
  pub journal_stop: Arc<AtomicBool>,
  /// Read-only replica flag: followers serve reads over a leader's shared
  /// prefix and reject every store-mutating operation.
  pub read_only: AtomicBool,
  /// Serializes follower snapshot refreshes so a slower loader can never
  /// overwrite a newer view with an older one.
  pub refresh_lock: parking_lot::Mutex<()>,
  pub metrics: Metrics,
  /// Shared lease-lost flag used for best-effort write fencing; the lease
  /// object itself is owned by the `ObjectLsm` handle (not by `Inner`), so
  /// partition handles cannot prolong its lifetime.
  pub lease_lost: parking_lot::Mutex<Option<Arc<AtomicBool>>>,
}

/// Object-storage-backed LSM engine implementing the wedb_embed_engine traits.
///
/// # Consistency model
/// - every [`Batch`] commit first uploads one immutable journal group object
///   (the atomic durability point), then applies the group to memtables;
/// - memtables spill into immutable block-indexed segment objects once they
///   exceed `Config::max_memtable_bytes`;
/// - a single manifest object chain records live segments + per-partition
///   journal watermarks;
/// - opening re-reads `current -> manifest` and replays journal groups newer
///   than each partition watermark, so a crash loses nothing that was acked.
///
/// # Compaction & GC (M3)
/// - partitions with more than `Config::max_segments_before_compact` segments
///   are merged into one fresh segment (newest-wins, tombstones dropped only
///   after every older segment has been folded in);
/// - merge publishing order is `upload new -> publish manifest -> delete old`,
///   so a crash between steps only leaves orphan objects, never lost data;
/// - applied journal groups (`seq <= min partition watermark`) are deleted;
/// - opening garbage-collects segment objects not referenced by the current
///   manifest and superseded manifest snapshots.
///
/// [`Batch`]: wedb_embed_engine::Batch
#[derive(Clone)]
pub struct ObjectLsm {
  pub(crate) inner: Arc<Inner>,
  /// Writer lease (owned by this handle); released when the last handle drops.
  pub(crate) lease: Option<Arc<Lease>>,
}

impl std::fmt::Debug for ObjectLsm {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ObjectLsm")
      .field("prefix", &self.inner.state.read().cfg.prefix)
      .finish()
  }
}

impl ObjectLsm {
  /// Open (or create) an engine instance in `store` under `cfg.prefix`.
  pub fn open(store: Arc<dyn Store>, cfg: Config) -> Result<Self> {
    Self::build(store, cfg)
  }

  /// Open as the exclusive writer of `cfg.prefix`, acquiring an expiring
  /// object lease first (for multi-instance access to a shared bucket).
  ///
  /// Blocks (polling every 50 ms) until the lease is free or
  /// [`LeaseOptions::timeout`] elapses, then fails. The lease is renewed by a
  /// background heartbeat and released when the last handle to this engine is
  /// dropped.
  pub fn open_leased(store: Arc<dyn Store>, cfg: Config, opts: LeaseOptions) -> Result<Self> {
    let deadline = Instant::now() + opts.timeout;
    loop {
      if let Some(engine) = Self::try_open_leased(store.clone(), cfg.clone(), opts.clone())? {
        return Ok(engine);
      }
      if Instant::now() >= deadline {
        return Err(Error::store(format!(
          "lease {} held by another writer",
          lease_key(&cfg.prefix)
        )));
      }
      thread::sleep(Duration::from_millis(50));
    }
  }

  /// One non-blocking attempt to become the exclusive writer of `cfg.prefix`.
  ///
  /// Returns `Ok(Some(engine))` when this instance won the lease and the
  /// engine recovered, `Ok(None)` while another writer holds the lease (or a
  /// concurrent contender won the race), and `Err` on store/recovery failure
  /// (the lease, if acquired, is released again). Standby supervisors poll
  /// this to promote the next writer after the active one crashes: the fence
  /// epoch is bumped on every acquisition, so a stale writer can never publish
  /// after the takeover.
  pub fn try_open_leased(
    store: Arc<dyn Store>,
    cfg: Config,
    opts: LeaseOptions,
  ) -> Result<Option<Self>> {
    let Some(lease) = Lease::try_acquire_once(store.clone(), &cfg.prefix, &opts)? else {
      return Ok(None);
    };
    let mut engine = Self::build(store, cfg)?;
    {
      let mut st = engine.inner.state.write();
      st.fence_epoch = lease.epoch();
    }
    *engine.inner.lease_lost.lock() = Some(lease.lost_flag());
    engine.lease = Some(Arc::new(lease));

    // Establish a durable anchor under THIS instance's epoch before any user
    // write is acknowledged: fold whatever recovery replayed into segments,
    // then publish a manifest so `current` always points at an epoch that can
    // see this writer's own journals. Without it, a writer that crashes after
    // acking (but before its first natural flush/compact) would leave `current`
    // anchored to the previous epoch, and the successor would fence off this
    // epoch's acked journals. If this maintenance fails the lease is released
    // again (engine drop) and the caller may retry.
    let parts: Vec<String> = engine
      .inner
      .state
      .read()
      .partitions
      .keys()
      .cloned()
      .collect();
    for name in &parts {
      engine.inner.flush_partition_detached(name)?;
    }
    {
      let mut st = engine.inner.state.write();
      publish_manifest(&*engine.inner.store, &mut st)?;
    }
    // Folded journals of superseded (predecessor) epochs are only deletable
    // here - gc_journal removes current-epoch keys only - so clean them once,
    // right after this epoch's anchor makes them unreachable.
    {
      let mut st = engine.inner.state.write();
      gc_foreign_journals(&*engine.inner.store, &mut st)?;
    }
    Ok(Some(engine))
  }

  /// Fencing epoch of this instance (`0` for unfenced engines). Bumped on
  /// every lease acquisition; embedded in journal groups and manifest
  /// publishes so a stale writer's state can never become visible after a
  /// takeover.
  pub fn fence_epoch(&self) -> u128 {
    self.inner.state.read().fence_epoch
  }

  /// Snapshot of cumulative counters plus current storage state, for
  /// production observability.
  pub fn metrics(&self) -> MetricsSnapshot {
    let mut m = self.inner.metrics.counters();
    // Never hold the global state lock while taking partition locks: the
    // engine lock order is partition -> state, so nesting them here would
    // deadlock against a concurrent commit. The snapshot is point-in-time-ish.
    {
      let st = self.inner.state.read();
      m.journal_count = st.journal_seqs.len();
      m.journal_bytes = st.journal_sizes.values().copied().sum();
    }
    let mut segments = 0usize;
    let mut segment_bytes = 0u64;
    let mut memtable_bytes = 0u64;
    for lock in self.inner.partitions.snapshot() {
      let ps = lock.read();
      segments += ps.meta.segments.len();
      segment_bytes += ps.meta.segments.iter().map(|s| s.bytes).sum::<u64>();
      memtable_bytes += ps.mem_bytes;
    }
    m.segments = segments;
    m.segment_bytes = segment_bytes;
    m.memtable_bytes = memtable_bytes;
    m
  }

  /// Open a read-only follower of `cfg.prefix` in `store`.
  ///
  /// A follower does NOT acquire the writer lease and never writes to the
  /// store. It periodically re-reads the leader's manifest (`current`) and the
  /// durable journal tail of that manifest's epoch, refreshing its in-memory
  /// view so reads track the leader's *published* state. `refresh` selects the
  /// background poll interval (`None` disables the thread; call [`Self::refresh`]
  /// manually, e.g. in tests). Every store-mutating call on the returned
  /// engine fails with a read-only error.
  ///
  /// Visibility: the follower sees what the leader made durable and published —
  /// segments folded by a manifest publish plus strict-mode / flushed
  /// group-commit journal objects above the manifest watermark. A group-commit
  /// ack still buffered in the leader is not visible.
  pub fn open_follower(
    store: Arc<dyn Store>,
    cfg: Config,
    refresh: Option<Duration>,
  ) -> Result<Self> {
    // Followers never buffer journals or run writer maintenance: force the
    // writer-side options off so no flusher thread is spawned and drop is
    // inert.
    let fcfg = cfg.journal_window_ms(None).background_flush(false);
    let block_cache = BlockCache::new(fcfg.cache_capacity);
    let index_cache = IndexCache::default();
    let partitions = PartitionTable::default();
    let readers = ReaderGate::default();
    let state = load_readonly_snapshot(&*store, &fcfg, &partitions, &index_cache)?;
    let inner = Arc::new(Inner {
      store,
      state: RwLock::new(state),
      journal_lock: Mutex::new(()),
      readers,
      partitions,
      block_cache,
      index_cache,
      journal_stop: Arc::new(AtomicBool::new(false)),
      lease_lost: parking_lot::Mutex::new(None),
      read_only: AtomicBool::new(true),
      refresh_lock: parking_lot::Mutex::new(()),
      metrics: Metrics::default(),
    });
    if let Some(interval) = refresh
      && !interval.is_zero()
    {
      spawn_follower_refresh(inner.clone(), interval);
    }
    Ok(Self { inner, lease: None })
  }

  /// Re-read the leader's published state into this engine's view.
  ///
  /// A no-op on a writer; on a follower it performs one read-only snapshot
  /// refresh. Useful to make tests deterministic instead of sleeping on the
  /// background interval. Serialized with any in-flight background refresh:
  /// a manual call waits for the poller's current store read to finish.
  pub fn refresh(&self) -> Result<()> {
    refresh_follower(&self.inner)
  }

  fn build(store: Arc<dyn Store>, cfg: Config) -> Result<Self> {
    let block_cache = BlockCache::new(cfg.cache_capacity);
    let index_cache = IndexCache::default();
    let partitions = PartitionTable::default();
    let readers = ReaderGate::default();
    let state = recover(&*store, &cfg, &index_cache, &partitions, &readers)?;
    let journal_stop = Arc::new(AtomicBool::new(false));
    let inner = Arc::new(Inner {
      store,
      state: RwLock::new(state),
      journal_lock: Mutex::new(()),
      readers,
      partitions,
      block_cache,
      index_cache,
      journal_stop: journal_stop.clone(),
      lease_lost: parking_lot::Mutex::new(None),
      read_only: AtomicBool::new(false),
      refresh_lock: parking_lot::Mutex::new(()),
      metrics: Metrics::default(),
    });
    if cfg.journal_window_ms.is_some() {
      spawn_journal_flusher(inner.clone());
    }
    if cfg.background_flush {
      spawn_background_flusher(inner.clone());
    }
    Ok(Self { inner, lease: None })
  }
}

impl Drop for ObjectLsm {
  fn drop(&mut self) {
    if let Some(lease) = self.lease.take() {
      drop(lease);
    }
  }
}

impl Drop for Inner {
  fn drop(&mut self) {
    // Graceful shutdown: flush buffered group-commit journal before stopping
    // the background flusher, so a clean close does not lose acked writes.
    if self.state.read().cfg.journal_window_ms.is_some() {
      let mut st = self.state.write();
      let _ = flush_journal_pending(&*self.store, &mut st);
    }
    self.journal_stop.store(true, Ordering::SeqCst);
  }
}

fn sync_partition_meta(st: &mut EngineState, ps: &PartitionState) {
  if let Some(pm) = st.partitions.get_mut(&ps.name) {
    *pm = ps.meta.clone();
  }
}

/// Min watermark across LIVE (non-dropped) partitions.
///
/// A dropped partition must not drag the GC waterline down: rm_partition marks
/// it dropped and (for a partition removed before any commit) leaves its
/// watermark at 0 forever, which would otherwise pin `published_min_wm` at 0
/// and permanently disable journal GC. Groups at/below a live partition's
/// watermark never touch an already-removed partition (removal discarded its
/// history), so excluding dropped partitions is safe.
fn min_live_watermark(st: &EngineState) -> u64 {
  st.partitions
    .values()
    .filter(|pm| !pm.dropped)
    .map(|pm| pm.watermark)
    .min()
    .unwrap_or(0)
}

/// Journal objects decoded per replay wave; bounds peak memory during recovery
/// and follower refresh.
const REPLAY_WAVE_OBJECTS: usize = 512;

/// Number of parallel decode workers for journal replay (bounded so a single
/// open/refresh cannot oversubscribe object-store connections).
fn replay_workers() -> usize {
  std::thread::available_parallelism()
    .map(|n| n.get())
    .unwrap_or(4)
    .clamp(1, 8)
}
/// One worker's decoded journal chunk during parallel recovery replay:
/// `(seq, object bytes len, decoded groups)` per replayed object, `None` when
/// the object disappeared between listing and read.
type DecodedJournalChunk = Vec<Option<(u64, u64, Vec<Group>)>>;

/// Decode a contiguous run of journal objects (`keys` already in apply order)
/// using up to `workers` threads. Short runs decode serially to avoid
/// thread-spawn overhead. `None` marks an object that disappeared between
/// listing and read (skipped, matching serial replay); results stay in input
/// order.
fn decode_journal_wave(
  store: &dyn Store,
  keys: &[(String, u64)],
  workers: usize,
) -> Result<DecodedJournalChunk> {
  if keys.len() < 16 || workers <= 1 {
    let mut out = Vec::with_capacity(keys.len());
    for (key, seq) in keys {
      match store.get(key) {
        Ok(Some(bytes)) => {
          out.push(Some((
            *seq,
            bytes.len() as u64,
            decode_group_stream(&bytes)?,
          )));
        }
        Ok(None) => out.push(None),
        Err(e) => return Err(e),
      }
    }
    return Ok(out);
  }
  let n = workers.min(keys.len());
  let chunk = keys.len().div_ceil(n);
  std::thread::scope(|scope| -> Result<DecodedJournalChunk> {
    let mut handles = Vec::with_capacity(n);
    for part in keys.chunks(chunk) {
      handles.push(scope.spawn(move || -> Result<DecodedJournalChunk> {
        let mut out = Vec::with_capacity(part.len());
        for (key, seq) in part {
          match store.get(key) {
            Ok(Some(bytes)) => {
              out.push(Some((
                *seq,
                bytes.len() as u64,
                decode_group_stream(&bytes)?,
              )));
            }
            Ok(None) => out.push(None),
            Err(e) => return Err(e),
          }
        }
        Ok(out)
      }));
    }
    let mut out = Vec::with_capacity(keys.len());
    for handle in handles {
      out.extend(
        handle
          .join()
          .map_err(|_| Error::store("journal decode worker panicked"))??,
      );
    }
    Ok(out)
  })
}
/// Read-only snapshot of a leader prefix: parse `current` -> manifest and
/// replay that epoch's journal tail above partition watermarks into
/// `partitions`. Never uploads, compacts, publishes or GCs. A prefix with no
/// manifest yet yields an empty snapshot (a follower only tracks published
/// state).
fn load_readonly_snapshot(
  store: &dyn Store,
  cfg: &Config,
  partitions: &PartitionTable,
  index_cache: &IndexCache,
) -> Result<EngineState> {
  let mut st = EngineState::new(cfg.clone());
  let prefix = &cfg.prefix;
  let Some(cur) = store.get(&current_key(prefix))? else {
    return Ok(st);
  };
  let text =
    std::str::from_utf8(&cur).map_err(|e| Error::Corrupt(format!("current not utf-8: {e}")))?;
  let (seq_text, epoch) = match text.split_once('\n') {
    Some((seq, epoch)) => (
      seq,
      Some(
        epoch
          .trim()
          .parse::<u128>()
          .map_err(|e| Error::Corrupt(format!("current epoch: {e}")))?,
      ),
    ),
    None => (text, None),
  };
  let mseq: u64 = seq_text
    .trim()
    .parse()
    .map_err(|e| Error::Corrupt(format!("current not a seq: {e}")))?;
  st.fence_epoch = epoch.unwrap_or(0);
  st.current_bytes = Some(cur);
  let man_bytes = store
    .get(&manifest_key(prefix, mseq))?
    .ok_or_else(|| Error::Corrupt(format!("manifest {mseq} missing")))?;
  let man = Manifest::decode(&man_bytes)?;
  st.manifest_seq = man.seq;
  st.next_segment_id = man.next_segment_id;
  st.journal_seq = man.next_journal_seq;
  for (name, pm) in man.partitions {
    for seg in &pm.segments {
      if let Some(index) = &seg.index {
        index_cache.insert(seg.id, Arc::new(index.clone()));
      }
    }
    let lock = partitions.create(&name);
    lock.write().meta = pm.clone();
    st.partitions.insert(name, pm);
  }
  st.published_min_wm = min_live_watermark(&st);

  // Replay the durable journal tail of this epoch above the watermarks.
  let list = store.list(&journal_prefix_epoch(prefix, st.fence_epoch))?;
  let mut max_seq = st.journal_seq;
  let mut pending: Vec<(String, u64)> = Vec::new();
  for k in &list {
    let Some(s) = parse_tail_seq(k) else {
      continue;
    };
    max_seq = max_seq.max(s);
    st.journal_seqs.insert(s);
    pending.push((k.clone(), s));
  }
  st.journal_seq = max_seq;
  let min_wm = st.published_min_wm;
  pending.sort_by_key(|(_, s)| *s);
  let to_read: Vec<(String, u64)> = pending.into_iter().filter(|(_, s)| *s > min_wm).collect();
  // Followers get the same parallel fetch+decode as writer recovery: the
  // per-object object-store GET latency dominates a refresh when the journal
  // tail is long. Apply stays seq-ordered; the epoch filter mirrors recover().
  let workers = replay_workers();

  let mut start = 0usize;
  while start < to_read.len() {
    let end = (start + REPLAY_WAVE_OBJECTS).min(to_read.len());
    let wave = decode_journal_wave(store, &to_read[start..end], workers)?;
    for item in wave {
      let Some((seq, len, groups)) = item else {
        continue;
      };
      st.journal_sizes.insert(seq, len);
      for group in groups {
        if group.epoch != st.fence_epoch {
          continue;
        }
        apply_group_recover(&mut st, &group, partitions)?;
      }
    }
    start = end;
  }
  Ok(st)
}

/// One follower refresh: load a fresh read-only snapshot and swap it into the
/// engine's view — global state first, then every published partition under
/// its own write lock. Partitions the leader no longer publishes are marked
/// dropped so reads observe the removal. No-op on a writer.
fn refresh_follower(inner: &Inner) -> Result<()> {
  if !inner.read_only.load(Ordering::SeqCst) {
    return Ok(());
  }
  inner.metrics.bump_refresh();
  // Serialize snapshot loads+swaps (a manual refresh() and the background
  // poller share this), so a slow loader can never install an OLDER view over
  // a newer one that has already landed.
  let _refresh = inner.refresh_lock.lock();
  let cfg = inner.state.read().cfg.clone();
  let scratch = PartitionTable::default();
  let fresh = load_readonly_snapshot(&*inner.store, &cfg, &scratch, &inner.index_cache)?;

  // Cheap change detection: skip the swap when the pointer and the journal set
  // are unchanged (avoids lock churn on every poll tick).
  {
    let cur = inner.state.read();
    if cur.current_bytes == fresh.current_bytes && cur.journal_seqs == fresh.journal_seqs {
      return Ok(());
    }
  }

  // Swap order: global metadata first, then each partition under its own write
  // lock. Readers never nest the partition and global locks, so there is no
  // deadlock; the swap is not atomic ACROSS partitions (fine for an
  // eventually-consistent follower view; each partition's read is atomic).
  // Global metadata first...
  {
    let mut st = inner.state.write();
    *st = fresh;
  }
  // ...then upsert every published partition under its own write lock.
  for lock in scratch.snapshot() {
    let target = inner.partitions.create(&lock.read().name.clone());
    let mut t = target.write();
    let s = lock.read();
    t.meta = s.meta.clone();
    t.mem = s.mem.clone();
    t.mem_bytes = s.mem_bytes;
    t.compacting = false;
  }
  // Mark partitions the leader no longer publishes as dropped.
  let alive: std::collections::BTreeSet<String> =
    inner.state.read().partitions.keys().cloned().collect();
  for lock in inner.partitions.snapshot() {
    let name = lock.read().name.clone();
    if !alive.contains(&name) {
      let mut ps = lock.write();
      ps.mem.clear();
      ps.mem_bytes = 0;
      ps.meta.segments.clear();
      ps.meta.watermark = 0;
      ps.meta.dropped = true;
    }
  }
  Ok(())
}

/// Background poller that keeps a follower's view close to the leader's
/// published state. Exits once the engine is dropped (journal_stop).
fn spawn_follower_refresh(inner: Arc<Inner>, interval: Duration) {
  let weak = Arc::downgrade(&inner);
  let stop = inner.journal_stop.clone();
  thread::Builder::new()
    .name("objectlsm-follower".into())
    .spawn(move || {
      loop {
        thread::sleep(interval);
        if stop.load(Ordering::SeqCst) {
          return;
        }
        let Some(inner) = weak.upgrade() else {
          return;
        };
        let _ = refresh_follower(&inner);
      }
    })
    .ok();
}

/// Rebuild in-memory state from the durable manifest + journal tail, then run
/// startup compaction/GC.
fn recover(
  store: &dyn Store,
  cfg: &Config,
  index_cache: &IndexCache,
  partitions: &PartitionTable,
  readers: &ReaderGate,
) -> Result<EngineState> {
  let mut st = EngineState::new(cfg.clone());
  let prefix = &cfg.prefix;

  if let Some(cur) = store.get(&current_key(prefix))? {
    let text =
      std::str::from_utf8(&cur).map_err(|e| Error::Corrupt(format!("current not utf-8: {e}")))?;
    let (seq_text, epoch) = match text.split_once('\n') {
      Some((seq, epoch)) => (
        seq,
        Some(
          epoch
            .trim()
            .parse::<u128>()
            .map_err(|e| Error::Corrupt(format!("current epoch: {e}")))?,
        ),
      ),
      None => (text, None),
    };
    let mseq: u64 = seq_text
      .trim()
      .parse()
      .map_err(|e| Error::Corrupt(format!("current not a seq: {e}")))?;
    st.fence_epoch = epoch.unwrap_or(0);
    st.current_bytes = Some(cur.clone());
    let man_bytes = store
      .get(&manifest_key(prefix, mseq))?
      .ok_or_else(|| Error::Corrupt(format!("manifest {mseq} missing")))?;
    let man = Manifest::decode(&man_bytes)?;
    st.manifest_seq = man.seq;
    st.next_segment_id = man.next_segment_id;
    st.journal_seq = man.next_journal_seq;
    for (name, pm) in man.partitions {
      for seg in &pm.segments {
        if let Some(index) = &seg.index {
          index_cache.insert(seg.id, Arc::new(index.clone()));
        }
      }
      let lock = partitions.create(&name);
      lock.write().meta = pm.clone();
      st.partitions.insert(name, pm);
    }
    st.published_min_wm = min_live_watermark(&st);
  }

  // Replay every journal group newer than its partition watermark.
  //
  // When a manifest exists, only the journals of the epoch recorded in
  // `current` are replayed: a takeover bumps the fencing epoch, so a stale
  // writer's objects can never become visible again. When NO manifest has
  // ever been published, the previous writer crashed before its first flush
  // and its committed journals are still the only durable record, so we
  // replay every journal object under `<prefix>/journal/` across epochs
  // (sorted by seq) — a successor therefore recovers all acked writes, not
  // just the ones already folded into segments.
  let has_manifest = st.current_bytes.is_some();
  let (list, root) = if has_manifest {
    (
      store.list(&journal_prefix_epoch(prefix, st.fence_epoch))?,
      None,
    )
  } else {
    (
      store.list(&journal_prefix(prefix))?,
      Some(journal_prefix(prefix)),
    )
  };
  let mut max_seq = st.journal_seq;
  let mut pending: Vec<(String, u64)> = Vec::new();
  for k in &list {
    let s = match &root {
      Some(root) => match parse_journal_tail(k, root) {
        Some((_, s)) => s,
        None => continue,
      },
      None => match parse_tail_seq(k) {
        Some(s) => s,
        None => continue,
      },
    };
    max_seq = max_seq.max(s);
    st.journal_seqs.insert(s);
    pending.push((k.clone(), s));
  }
  st.journal_seq = max_seq;
  let min_wm = st
    .partitions
    .values()
    .map(|p| p.watermark)
    .min()
    .unwrap_or(0);
  pending.sort_by_key(|(_, s)| *s);
  let to_read: Vec<(String, u64)> = pending
    .into_iter()
    // Groups in an object whose end-seq is at/below every partition watermark
    // are already folded into durable segments; skip the whole object.
    .filter(|(_, s)| *s > min_wm)
    .collect();
  // Fetch and decode journal objects in parallel, in bounded waves — on an
  // object store the per-object GET latency dominates recovery, so serial
  // replay is the slow path of reopen/failover. The apply below stays strictly
  // seq-ordered and any missing object is skipped exactly as serial replay
  // would; one wave's decoded objects are applied before the next wave is
  // fetched, keeping peak memory proportional to a wave instead of the whole
  // journal tail.
  let workers = replay_workers();

  let mut start = 0usize;
  while start < to_read.len() {
    let end = (start + REPLAY_WAVE_OBJECTS).min(to_read.len());
    let wave = decode_journal_wave(store, &to_read[start..end], workers)?;
    for item in wave {
      let Some((seq, len, groups)) = item else {
        continue;
      };
      st.journal_sizes.insert(seq, len);
      for group in groups {
        if has_manifest && group.epoch != st.fence_epoch {
          continue;
        }
        apply_group_recover(&mut st, &group, partitions)?;
      }
    }
    start = end;
  }

  // Flush replayed memtables that already exceeded the budget.
  let over: Vec<String> = partitions
    .snapshot()
    .iter()
    .filter_map(|lock| {
      let ps = lock.read();
      (!ps.meta.dropped && ps.mem_bytes > cfg.max_memtable_bytes).then(|| ps.name.clone())
    })
    .collect();
  for name in over {
    let lock = partitions.get(&name).expect("partition present");
    let mut ps = lock.write();
    flush_partition(store, &mut st, &mut ps)?;
  }
  maybe_compact_all_state(store, &mut st, partitions, readers)?;
  gc_journal(store, &mut st);
  gc_objects_at_open(store, &mut st)?;
  gc_foreign_journals(store, &mut st)?;
  Ok(st)
}

/// Apply a committed group during recovery. Unlike the online commit path,
/// this is single-threaded startup state construction.
fn apply_group_recover(
  st: &mut EngineState,
  group: &Group,
  partitions: &PartitionTable,
) -> Result<()> {
  for op in &group.ops {
    let wm = st
      .partitions
      .get(&op.part)
      .map(|pm| pm.watermark)
      .unwrap_or(0);
    if group.seq <= wm {
      continue;
    }
    let lock = partitions.create(&op.part);
    let mut ps = lock.write();
    ps.apply(&op.key, op.value.as_deref());
    st.ensure_meta(&op.part);
  }
  Ok(())
}

/// Publish a new manifest snapshot: write `manifest/<seq+1>` then flip
/// `manifest/current` (the single atomic visibility point).
fn publish_manifest(store: &dyn Store, st: &mut EngineState) -> Result<()> {
  // In windowed (group-commit) mode make every pending group durable before
  // publishing a manifest whose watermarks may fold those groups into
  // segments or drops.
  if st.cfg.journal_window_ms.is_some() && !st.pending.is_empty() {
    flush_journal_pending(store, st)?;
  }
  let mut man = Manifest {
    seq: st.manifest_seq + 1,
    next_segment_id: st.next_segment_id,
    next_journal_seq: st.journal_seq,
    fence_epoch: st.fence_epoch,
    partitions: BTreeMap::new(),
  };
  for (name, pm) in &st.partitions {
    man.partitions.insert(name.clone(), pm.clone());
  }
  st.manifest_seq = man.seq;
  let bytes = man.encode()?;
  st.manifest_bytes = bytes.len() as u64;
  store.put(&manifest_key(&st.cfg.prefix, man.seq), &bytes)?;

  let new_current = if st.fence_epoch != 0 {
    format!("{}\n{}", man.seq, st.fence_epoch).into_bytes()
  } else {
    man.seq.to_string().into_bytes()
  };
  if st.fence_epoch != 0 {
    let ok = match &st.current_bytes {
      Some(expected) => {
        store.put_if_matches(&current_key(&st.cfg.prefix), expected, &new_current)?
      }
      None => store.create(&current_key(&st.cfg.prefix), &new_current)?,
    };
    if !ok {
      return Err(Error::store("fenced: manifest current changed"));
    }
  } else {
    store.put(&current_key(&st.cfg.prefix), &new_current)?;
  }
  st.current_bytes = Some(new_current);
  // The mirror watermarks are now durable (this manifest references them), so
  // journals folded by them may be garbage-collected from here on.
  st.published_min_wm = min_live_watermark(st);
  Ok(())
}

/// Flush one partition's memtable into an immutable block-indexed segment and
/// advance its watermark. The caller must hold both the partition write lock
/// (via `ps`) and the global write lock (via `st`).
fn flush_partition(store: &dyn Store, st: &mut EngineState, ps: &mut PartitionState) -> Result<()> {
  if ps.meta.dropped || ps.mem.is_empty() {
    return Ok(());
  }
  let entries: SegmentEntries = ps
    .mem
    .iter()
    .map(|(k, e)| {
      let v = match e {
        MemEntry::Value(v) => Some(v.clone()),
        MemEntry::Tombstone => None,
      };
      (k.clone(), v)
    })
    .collect();
  let encoded = encode_segment(&entries, st.cfg.block_size as usize)?;
  let id = st.next_segment_id;
  st.next_segment_id += 1;
  store.put(&segment_key(&st.cfg.prefix, &ps.name, id), &encoded)?;
  let meta = build_segment_meta(
    id,
    st.journal_seq,
    &encoded,
    &entries,
    st.cfg.manifest_embed_index,
  )?;
  ps.meta.segments.push(meta);
  ps.meta.watermark = st.journal_seq;
  ps.mem.clear();
  ps.mem_bytes = 0;
  sync_partition_meta(st, ps);
  publish_manifest(store, st)
}

/// Detach a full memtable snapshot for background upload.
fn take_partition_flush(st: &mut EngineState, ps: &mut PartitionState) -> Option<PendingFlush> {
  if ps.meta.dropped
    || ps.mem.is_empty()
    || st.pending_flushes.contains_key(&ps.name)
    || ps.mem_bytes < st.cfg.max_memtable_bytes
  {
    return None;
  }

  let entries: SegmentEntries = ps
    .mem
    .iter()
    .map(|(k, e)| {
      let v = match e {
        MemEntry::Value(v) => Some(v.clone()),
        MemEntry::Tombstone => None,
      };
      (k.clone(), v)
    })
    .collect();
  let encoded = encode_segment(&entries, st.cfg.block_size as usize).ok()?;
  let segment_id = st.next_segment_id;
  st.next_segment_id += 1;
  let pending = PendingFlush {
    partition: ps.name.clone(),
    watermark: ps.meta.watermark.max(st.journal_seq),
    segment_id,
    encoded,
    entries,
  };
  ps.mem.clear();
  ps.mem_bytes = 0;
  st.pending_flushes
    .insert(pending.partition.clone(), pending.clone());
  Some(pending)
}

/// Publish a completed background segment upload into the partition metadata.
fn finish_partition_flush(inner: &Inner, part: &str) -> Result<()> {
  let Some(active) = ({
    let mut st = inner.state.write();
    st.pending_flushes.remove(part)
  }) else {
    return Ok(());
  };
  let (key, embed_index) = {
    let st = inner.state.read();
    (
      segment_key(&st.cfg.prefix, part, active.segment_id),
      st.cfg.manifest_embed_index,
    )
  };
  inner.store.put(&key, &active.encoded)?;
  let meta = build_segment_meta(
    active.segment_id,
    active.watermark,
    &active.encoded,
    &active.entries,
    embed_index,
  )?;
  let lock = inner
    .partitions
    .get(part)
    .ok_or_else(|| Error::store("flush partition disappeared"))?;
  let mut ps = lock.write();
  let mut st = inner.state.write();
  ps.meta.segments.push(meta);
  ps.meta.watermark = ps.meta.watermark.max(active.watermark);
  sync_partition_meta(&mut st, &ps);
  publish_manifest(&*inner.store, &mut st)?;
  gc_journal(&*inner.store, &mut st);
  Ok(())
}

/// Read every entry of a segment object via tail/index/block Range GETs
/// (no caching; used by the compaction merge).
fn read_segment_entries(
  store: &dyn Store,
  prefix: &str,
  part: &str,
  seg: &SegmentMeta,
) -> Result<SegmentEntries> {
  let key = segment_key(prefix, part, seg.id);
  // Compaction consumes every block of the segment, so fetch the whole object
  // once instead of issuing one Range GET per block. This turns O(blocks)
  // remote requests into one request and keeps the merge fully local.
  let bytes = store
    .get(&key)?
    .ok_or_else(|| Error::Corrupt(format!("segment {} missing", seg.id)))?;
  if bytes.len() < TAIL_LEN {
    return Err(Error::Corrupt(format!(
      "segment {} shorter than trailer",
      seg.id
    )));
  }
  let tail = parse_tail(&bytes[bytes.len() - TAIL_LEN..])?;
  let idx_start = tail.index_offset as usize;
  let idx_end = idx_start.saturating_add(tail.index_len as usize);
  if idx_end > bytes.len() - TAIL_LEN {
    return Err(Error::Corrupt(format!(
      "segment {} index out of bounds",
      seg.id
    )));
  }
  let index = decode_index(&bytes[idx_start..idx_end])?;
  let mut out = SegmentEntries::new();
  for bm in &index.blocks {
    let start = bm.offset as usize;
    let end = start.saturating_add(bm.len as usize);
    if end > bytes.len() {
      return Err(Error::Corrupt(format!(
        "segment {} block out of bounds",
        seg.id
      )));
    }
    out.extend(decode_block(&bytes[start..end])?);
  }
  Ok(out)
}

/// Snapshot of segments being compacted by a detached job.
struct DetachedCompact {
  prefix: String,
  segs: Vec<SegmentMeta>,
  segment_id: u64,
  embed_index: bool,
  block_size: usize,
}

/// Read all `segs` in bounded parallel batches, preserving segment order.
///
/// Compaction consumes every block of every segment, so the R2/S3 latency is
/// dominated by the object GETs. Issuing them from up to a few scoped worker
/// threads overlaps the network round-trips instead of serializing them.
fn read_segments_parallel(
  store: &dyn Store,
  prefix: &str,
  part: &str,
  segs: &[SegmentMeta],
) -> Result<Vec<SegmentEntries>> {
  const WORKERS: usize = 8;
  let n = segs.len();
  if n == 0 {
    return Ok(Vec::new());
  }
  let workers = n.min(WORKERS);
  let per = n.div_ceil(workers);
  std::thread::scope(|scope| -> Result<Vec<SegmentEntries>> {
    let mut handles = Vec::with_capacity(workers);
    for w in 0..workers {
      let start = w * per;
      if start >= n {
        break;
      }
      handles.push(scope.spawn(move || {
        let mut out = Vec::new();
        let end = (start + per).min(n);
        for seg in &segs[start..end] {
          out.push(read_segment_entries(store, prefix, part, seg)?);
        }
        Ok::<_, Error>(out)
      }));
    }
    let mut all = Vec::with_capacity(n);
    for h in handles {
      let chunk = h
        .join()
        .map_err(|_| Error::store("compaction reader panicked"))??;
      all.extend(chunk);
    }
    Ok(all)
  })
}

/// Fold all segment entry lists (ordered newest first) into one live map and
/// return the merged, tombstone-free entries.
fn merge_entries(all: Vec<SegmentEntries>) -> SegmentEntries {
  let mut map: BTreeMap<Vec<u8>, Option<Vec<u8>>> = BTreeMap::new();
  for entries in all.into_iter().rev() {
    for (k, v) in entries {
      map.entry(k).or_insert(v);
    }
  }
  let mut out: SegmentEntries = Vec::with_capacity(map.len());
  for (k, v) in map {
    if let Some(val) = v {
      out.push((k, Some(val)));
    }
  }
  out
}

/// Merge every segment of a partition into one fresh segment. The caller must
/// hold both the partition write lock and the global write lock.
fn compact_partition_locked(
  store: &dyn Store,
  st: &mut EngineState,
  ps: &mut PartitionState,
) -> Result<Option<Vec<SegmentMeta>>> {
  flush_partition(store, st, ps)?;
  let prefix = st.cfg.prefix.clone();
  if !(!ps.meta.dropped && ps.meta.segments.len() > 1) {
    return Ok(None);
  }

  // 1. fetch all segments in parallel, then fold newest -> oldest into a
  //    single decision map (newest entry wins for each key).
  let all = read_segments_parallel(store, &prefix, &ps.name, &ps.meta.segments)?;
  let out = merge_entries(all);

  // 3. upload the merged result first (failure keeps the old run intact),
  //    then atomically swap the segment list and publish the manifest. Old
  //    objects are deleted by the caller after releasing the partition lock.
  let new_meta = if out.is_empty() {
    None
  } else {
    let encoded = encode_segment(&out, st.cfg.block_size as usize)?;
    let id = st.next_segment_id;
    st.next_segment_id += 1;
    store.put(&segment_key(&prefix, &ps.name, id), &encoded)?;
    Some(build_segment_meta(
      id,
      st.journal_seq,
      &encoded,
      &out,
      st.cfg.manifest_embed_index,
    )?)
  };

  let new_segments = new_meta.map(|m| vec![m]).unwrap_or_default();
  let old = std::mem::replace(&mut ps.meta.segments, new_segments);
  ps.meta.watermark = st.journal_seq;
  sync_partition_meta(st, ps);
  publish_manifest(store, st)?;
  st.compactions_completed += 1;
  Ok(Some(old))
}

/// Compact every partition that reached the configured segment limit.
fn maybe_compact_all_state(
  store: &dyn Store,
  st: &mut EngineState,
  partitions: &PartitionTable,
  _readers: &ReaderGate,
) -> Result<()> {
  let limit = st.cfg.max_segments_before_compact;
  for name in partitions.names() {
    let Some(lock) = partitions.get(&name) else {
      continue;
    };
    let mut ps = lock.write();
    if !ps.meta.dropped
      && ps.meta.segments.len() >= limit
      && st.cfg.eager_object_delete
      && let Some(old) = compact_partition_locked(store, st, &mut ps)?
    {
      for seg in old {
        let _ = store.delete(&segment_key(&st.cfg.prefix, &name, seg.id));
      }
    }
  }
  Ok(())
}

/// Delete journal objects whose seq is at or below every partition watermark
/// (they are folded into segments or were deliberately discarded).
fn gc_journal(store: &dyn Store, st: &mut EngineState) {
  // Never delete beyond the watermark of the LAST SUCCESSFULLY PUBLISHED
  // manifest: a flush whose publish failed (or a crash before publish) leaves
  // journal objects as the only durable record, and deleting them would lose
  // acknowledged writes on recovery.
  let min_wm = st.published_min_wm;
  if min_wm == 0 {
    return;
  }
  let doomed: Vec<u64> = st.journal_seqs.range(..=min_wm).copied().collect();
  for s in doomed {
    let _ = store.delete(&journal_key_epoch(&st.cfg.prefix, s, st.fence_epoch));
    st.journal_seqs.remove(&s);
    st.journal_sizes.remove(&s);
  }
}

/// Delete journal objects of superseded (foreign) epochs that have already been
/// folded into the current manifest (`seq <= published_min_wm`).
///
/// [`gc_journal`] only ever deletes the CURRENT epoch's keys, so without this
/// the journals a predecessor writer left behind would accumulate in the
/// bucket forever. Runs once at open (after recovery) and once right after a
/// lease takeover publishes its new-epoch anchor. Foreign journals above the
/// published watermark are kept (a stale, fenced writer may still have written
/// them; they are invisible either way and can be reclaimed by a lifecycle
/// rule).
fn gc_foreign_journals(store: &dyn Store, st: &mut EngineState) -> Result<()> {
  let root = journal_prefix(&st.cfg.prefix);
  let wm = st.published_min_wm;
  if wm == 0 {
    return Ok(());
  }
  let epoch = st.fence_epoch;
  let keys = store.list(&root)?;
  for key in keys {
    let Some((e, s)) = parse_journal_tail(&key, &root) else {
      continue;
    };
    if e != epoch && s <= wm {
      let _ = store.delete(&key);
      st.journal_seqs.remove(&s);
      st.journal_sizes.remove(&s);
    }
  }
  Ok(())
}

/// Startup GC: when eager deletion is enabled, delete segment objects not
/// referenced by the current manifest; always delete manifest snapshots
/// superseded by `current` (they are cheap metadata, not the high-volume
/// segment DELETE path).
fn gc_objects_at_open(store: &dyn Store, st: &mut EngineState) -> Result<()> {
  let prefix = st.cfg.prefix.clone();
  if st.cfg.eager_object_delete {
    let seg_root = segment_root(&prefix);
    let seg_prefix = format!("{prefix}/seg/");
    let live: BTreeMap<String, Vec<u64>> = st
      .partitions
      .iter()
      .filter(|(_, pm)| !pm.dropped)
      .map(|(name, pm)| (name.clone(), pm.segments.iter().map(|s| s.id).collect()))
      .collect();
    for key in store.list(&seg_root)? {
      let rest = match key.strip_prefix(&seg_prefix) {
        Some(r) => r,
        None => continue,
      };
      let Some((part, id_s)) = rest.rsplit_once('/') else {
        continue;
      };
      let Ok(id) = id_s.parse::<u64>() else {
        continue;
      };
      let referenced = live.get(part).map(|ids| ids.contains(&id)).unwrap_or(false);
      if !referenced {
        let _ = store.delete(&key);
      }
    }
  }
  let cur = st.manifest_seq;
  for key in store.list(&manifest_prefix(&prefix))? {
    if let Some(seq) = parse_tail_seq(&key)
      && seq != cur
    {
      let _ = store.delete(&key);
    }
  }
  Ok(())
}

/// Detach the pending group-commit buffer for an out-of-lock upload.
///
/// The caller holds the global state lock only while taking the buffer and
/// reserving its end sequence. The remote object PUT happens afterwards
/// without blocking commits; `finish_flushed_journal` records the object
/// after the upload succeeds.
fn take_journal_pending(st: &mut EngineState) -> Option<(String, Vec<u8>)> {
  if st.pending.is_empty() || st.journal_flushing {
    return None;
  }
  let end = st.journal_seq;
  st.journal_flushing = true;
  Some((
    journal_key_epoch(&st.cfg.prefix, end, st.fence_epoch),
    std::mem::take(&mut st.pending),
  ))
}

/// Record a successfully uploaded detached journal object.
fn finish_flushed_journal(st: &mut EngineState, key: &str, bytes_len: u64) {
  st.journal_flushing = false;
  if let Some(end) = parse_tail_seq(key) {
    st.journal_seqs.insert(end);
    st.journal_sizes.insert(end, bytes_len);
  }
  st.pending_lo = 0;
}

/// Abort a detached upload and restore its buffer for a later flush.
fn abort_journal_flush(st: &mut EngineState, buffer: Vec<u8>) {
  st.journal_flushing = false;
  if buffer.is_empty() {
    return;
  }
  // Preserve the detached (failed) groups in front of any groups appended
  // while the upload was in flight; dropping them would lose acknowledged but
  // not-yet-durable writes.
  if st.pending.is_empty() {
    st.pending = buffer;
  } else {
    let mut restored = buffer;
    restored.extend_from_slice(&st.pending);
    st.pending = restored;
  }
}

/// Flush the pending group-commit buffer into one journal object covering
/// groups `pending_lo..=journal_seq`.
fn flush_journal_pending(store: &dyn Store, st: &mut EngineState) -> Result<()> {
  let Some((key, buffer)) = take_journal_pending(st) else {
    return Ok(());
  };
  let bytes = buffer.len() as u64;
  if let Err(e) = store.put(&key, &buffer) {
    abort_journal_flush(st, buffer);
    return Err(e);
  }
  finish_flushed_journal(st, &key, bytes);
  Ok(())
}

/// Background flusher for windowed group-commit mode: every `ms` it turns the
/// pending buffer into one journal object (and garbage-collects folded ones).
fn spawn_journal_flusher(inner: Arc<Inner>) {
  let Some(ms) = inner.state.read().cfg.journal_window_ms else {
    return;
  };
  let weak = Arc::downgrade(&inner);
  let stop = inner.journal_stop.clone();
  thread::Builder::new()
    .name("objectlsm-journal".into())
    .spawn(move || {
      loop {
        if stop.load(Ordering::SeqCst) {
          return;
        }
        thread::sleep(Duration::from_millis(ms));
        if stop.load(Ordering::SeqCst) {
          return;
        }
        let Some(inner) = weak.upgrade() else {
          return;
        };
        if let Some((key, buffer)) = {
          let mut st = inner.state.write();
          take_journal_pending(&mut st)
        } {
          match inner.store.put(&key, &buffer) {
            Ok(()) => {
              let bytes = buffer.len() as u64;
              let mut st = inner.state.write();
              finish_flushed_journal(&mut st, &key, bytes);
              gc_journal(&*inner.store, &mut st);
            }
            Err(_) => {
              let mut st = inner.state.write();
              abort_journal_flush(&mut st, buffer);
            }
          }
        }
      }
    })
    .ok();
}

fn spawn_background_flusher(inner: Arc<Inner>) {
  let weak = Arc::downgrade(&inner);
  thread::Builder::new()
    .name("objectlsm-flush".into())
    .spawn(move || {
      loop {
        thread::sleep(Duration::from_millis(10));
        let Some(inner) = weak.upgrade() else {
          return;
        };
        if !inner.state.read().cfg.background_flush {
          continue;
        }
        let parts: Vec<String> = {
          let st = inner.state.read();
          st.pending_flushes.keys().cloned().collect()
        };
        for part in parts {
          let _ = finish_partition_flush(&inner, &part);
        }
      }
    })
    .ok();
}

impl Inner {
  /// Best-effort fencing: reject mutations once the held writer lease has
  /// been marked lost (e.g. heartbeat renewal failed after expiry).
  fn ensure_writer(&self) -> Result<()> {
    let lost = self
      .lease_lost
      .lock()
      .as_ref()
      .map(|flag| flag.load(Ordering::SeqCst))
      .unwrap_or(false);
    if lost {
      return Err(Error::store("writer lease lost"));
    }
    Ok(())
  }

  pub(crate) fn partition_lock(&self, name: &str) -> Option<PartitionLock> {
    self.partitions.get(name)
  }

  /// Reject mutations on a read-only follower, then apply lease fencing for
  /// writers. Every store-mutating entry point calls this instead of
  /// [`Self::ensure_writer`] so a follower can never write journals, segments
  /// or manifests into the shared bucket.
  fn ensure_writable(&self) -> Result<()> {
    if self.read_only.load(Ordering::SeqCst) {
      return Err(Error::store("engine is a read-only follower"));
    }
    self.ensure_writer()
  }

  /// Wait for any in-flight detached compaction on `name` to finish.
  fn wait_compaction_finished(&self, name: &str) {
    loop {
      let Some(lock) = self.partitions.get(name) else {
        return;
      };
      if !lock.read().compacting {
        return;
      }
      drop(lock);
      std::thread::sleep(std::time::Duration::from_millis(1));
    }
  }

  fn ensure_partitions(&self, names: &[String]) {
    for name in names {
      self.partitions.create(name);
    }
    let mut st = self.state.write();
    for name in names {
      st.ensure_meta(name);
    }
  }

  fn partition_locks(&self, names: &[String]) -> BTreeMap<String, PartitionLock> {
    names
      .iter()
      .filter_map(|name| self.partitions.get(name).map(|lock| (name.clone(), lock)))
      .collect()
  }

  /// Atomically commit an op group (journal PUT first, then apply).
  ///
  /// Locks for every involved partition are acquired in sorted-name order,
  /// then the global metadata lock is taken only for seq allocation / journal
  /// buffering / flush-compact-GC. Different partitions therefore commit
  /// concurrently and only briefly share the global lock.
  pub(crate) fn commit_ops(&self, ops: Vec<Op>) -> Result<()> {
    if ops.is_empty() {
      return Ok(());
    }
    self.ensure_writable()?;
    self.metrics.bump_commit();
    let mut names: Vec<String> = ops.iter().map(|op| op.part.clone()).collect();
    names.sort();
    names.dedup();

    self.ensure_partitions(&names);
    let locks = self.partition_locks(&names);
    let mut guards = BTreeMap::new();
    for name in &names {
      guards.insert(
        name.clone(),
        locks.get(name).expect("partition lock").write(),
      );
    }

    // Allocate the journal seq and (in windowed mode) append to the pending
    // buffer under the global lock. In strict mode the group is serialized
    // here but its object PUT happens afterwards outside the global lock, so a
    // network write no longer blocks unrelated partition reads/writes.
    let (group, strict_put) = {
      let mut st = self.state.write();
      let seq = st.journal_seq + 1;
      st.journal_seq = seq;
      let group = Group {
        seq,
        epoch: st.fence_epoch,
        ops,
      };
      let bytes = encode_group(&group)?;
      let strict_put = if st.cfg.journal_window_ms.is_none() {
        Some((
          journal_key_epoch(&st.cfg.prefix, seq, st.fence_epoch),
          bytes,
        ))
      } else {
        if st.pending.is_empty() {
          st.pending_lo = seq;
        }
        st.pending.extend_from_slice(&bytes);
        if st.pending.len() as u64 >= st.cfg.journal_max_buffer_bytes {
          flush_journal_pending(&*self.store, &mut st)?;
        }
        None
      };
      (group, strict_put)
    };

    if let Some((jkey, bytes)) = strict_put {
      // Durability before visibility: write the journal object first, then
      // apply. The journal lock only serializes journal PUTs.
      {
        let _guard = self.journal_lock.lock();
        if let Err(e) = self.store.put(&jkey, &bytes) {
          self.metrics.bump_commit_failure();
          return Err(e);
        }
      }
      let mut st = self.state.write();
      st.journal_seqs.insert(group.seq);
      st.journal_sizes.insert(group.seq, bytes.len() as u64);
    }

    let mut put_ops = 0u64;
    let mut delete_ops = 0u64;
    for op in &group.ops {
      if op.value.is_some() {
        put_ops += 1;
      } else {
        delete_ops += 1;
      }
    }
    self.metrics.bump_puts(put_ops);
    self.metrics.bump_deletes(delete_ops);

    for op in &group.ops {
      let ps = guards.get_mut(&op.part).expect("partition guard");
      if group.seq <= ps.meta.watermark {
        continue;
      }
      ps.apply(&op.key, op.value.as_deref());
    }

    let mut to_compact: Vec<String> = Vec::new();
    {
      let mut st = self.state.write();
      let mem_limit = st.cfg.max_memtable_bytes;
      let over: Vec<String> = names
        .iter()
        .filter(|name| {
          guards
            .get(*name)
            .map(|ps| !ps.meta.dropped && ps.mem_bytes > mem_limit)
            .unwrap_or(false)
        })
        .cloned()
        .collect();
      let background_flush = st.cfg.background_flush;
      for name in over {
        if background_flush {
          let _ = take_partition_flush(&mut st, guards.get_mut(&name).expect("guard"));
        } else {
          // Maintenance after the durability point must not turn an already
          // committed batch into an error: leave the memtable for a later flush.
          let _ = flush_partition(&*self.store, &mut st, guards.get_mut(&name).expect("guard"));
        }
      }

      // Do not run compaction while holding the partition/global locks. Only
      // mark partitions that need it and run the detached compaction after the
      // commit guards are released.
      let seg_limit = st.cfg.max_segments_before_compact;
      for name in &names {
        let ps = guards.get_mut(name).expect("guard");
        if !ps.meta.dropped && !ps.compacting && ps.meta.segments.len() >= seg_limit {
          to_compact.push(name.clone());
        }
      }
      gc_journal(&*self.store, &mut st);
    }
    drop(guards);
    for name in to_compact {
      self.compact_partition_detached(&name)?;
    }
    Ok(())
  }

  /// Ensure a partition exists and is not marked dropped (re-create clears the
  /// dropped flag, keeping its watermark to avoid stale journal replays).
  pub(crate) fn touch_partition(&self, name: &str) -> Result<()> {
    if self.read_only.load(Ordering::SeqCst) {
      // Followers never create or resurrect partitions: a handle is only
      // handed out for a partition the leader currently publishes (present in
      // the local refreshed view and not dropped). Anything else errors so a
      // follower can never publish a manifest into the shared bucket.
      let Some(lock) = self.partitions.get(name) else {
        return Err(Error::store(format!(
          "read-only follower: partition {name} is not published"
        )));
      };
      let ps = lock.read();
      if ps.meta.dropped {
        return Err(Error::store(format!(
          "read-only follower: partition {name} was removed by the leader"
        )));
      }
      return Ok(());
    }
    self.ensure_writer()?;
    let lock = self.partitions.create(name);
    let mut ps = lock.write();
    if ps.meta.dropped {
      ps.meta.dropped = false;
      let mut st = self.state.write();
      sync_partition_meta(&mut st, &ps);
      publish_manifest(&*self.store, &mut st)?;
    } else {
      let mut st = self.state.write();
      st.ensure_meta(name);
    }
    Ok(())
  }

  /// Load (and cache) the block index of a segment via tail + index Range GETs.
  pub(crate) fn load_index(
    &self,
    prefix: &str,
    part: &str,
    seg: &SegmentMeta,
  ) -> Result<Arc<SegmentIndex>> {
    if let Some(idx) = self.index_cache.get(seg.id) {
      return Ok(idx);
    }
    if let Some(idx) = &seg.index {
      let idx = Arc::new(idx.clone());
      self.index_cache.insert(seg.id, idx.clone());
      return Ok(idx);
    }
    let key = segment_key(prefix, part, seg.id);
    let tail_raw = self
      .store
      .get_range(
        &key,
        seg.bytes.saturating_sub(TAIL_LEN as u64),
        TAIL_LEN as u64,
      )?
      .ok_or_else(|| Error::Corrupt(format!("segment {} tail missing", seg.id)))?;
    let tail = parse_tail(&tail_raw)?;
    let idx_raw = self
      .store
      .get_range(&key, tail.index_offset as u64, tail.index_len as u64)?
      .ok_or_else(|| Error::Corrupt(format!("segment {} index missing", seg.id)))?;
    let index = Arc::new(decode_index(&idx_raw)?);
    self.index_cache.insert(seg.id, index.clone());
    Ok(index)
  }

  /// Load (and cache) one decoded block of a segment via a Range GET.
  pub(crate) fn load_block(
    &self,
    prefix: &str,
    part: &str,
    seg: &SegmentMeta,
    block: &BlockMeta,
  ) -> Result<Arc<SegmentEntries>> {
    if let Some(entries) = self.block_cache.get(seg.id, block.offset) {
      return Ok(entries);
    }
    let key = segment_key(prefix, part, seg.id);
    let raw = self
      .store
      .get_range(&key, block.offset as u64, block.len as u64)?
      .ok_or_else(|| {
        Error::Corrupt(format!(
          "segment {} block @{} missing",
          seg.id, block.offset
        ))
      })?;
    if raw.len() < BLOCK_HEADER_LEN {
      return Err(Error::Corrupt("block shorter than header".into()));
    }
    let entries = Arc::new(decode_block(&raw)?);
    self
      .block_cache
      .insert(seg.id, block.offset, entries.clone(), block.len as u64);
    Ok(entries)
  }

  /// Point lookup: memtable, then segments newest -> oldest using the block
  /// index to fetch only the candidate block.
  pub(crate) fn lookup(&self, name: &str, key: &[u8]) -> Result<Option<Vec<u8>>> {
    self.metrics.bump_get();
    // Reader token prevents segment-object deletion while this lookup runs.
    let _gate = self.readers.enter();
    let prefix = self.state.read().cfg.prefix.clone();
    let Some(lock) = self.partitions.get(name) else {
      return Ok(None);
    };
    let segments = {
      let ps = lock.read();
      if let Some(e) = ps.mem.get(key) {
        return Ok(match e {
          MemEntry::Value(v) => Some(v.clone()),
          MemEntry::Tombstone => None,
        });
      }
      ps.meta.segments.clone()
    };
    if let Some(pending) = self.state.read().pending_flushes.get(name)
      && let Some(e) = pending.entries.iter().find(|(k, _)| k.as_slice() == key)
    {
      return Ok(e.1.clone());
    }

    for seg in segments.iter().rev() {
      if seg.first.as_slice() > key || seg.last.as_slice() < key {
        continue;
      }
      let index = self.load_index(&prefix, name, seg)?;
      let Some(bi) = find_block(&index, key) else {
        continue;
      };
      let block = &index.blocks[bi];
      let entries = self.load_block(&prefix, name, seg, block)?;
      let idx = entries.partition_point(|(k, _)| k.as_slice() < key);
      if let Some((k, v)) = entries.get(idx)
        && k.as_slice() == key
      {
        // A tombstone here shadows any older segment value.
        return Ok(v.clone());
      }
    }
    Ok(None)
  }

  pub(crate) fn clear_partition(&self, name: &str) -> Result<()> {
    self.ensure_writable()?;
    // Gate deletes against readers (before taking the partition lock), and
    // publish the cleared manifest before deleting old objects: a crash only
    // leaves orphan objects, never a manifest pointing at deleted segments.
    let _del = self.readers.exclusive();
    self.wait_compaction_finished(name);
    let Some(lock) = self.partition_lock(name) else {
      return Ok(());
    };
    let mut ps = lock.write();
    let mut st = self.state.write();
    let prefix = st.cfg.prefix.clone();
    let eager = st.cfg.eager_object_delete;
    let tail_seq = st.journal_seq;
    let old = std::mem::take(&mut ps.meta.segments);
    ps.mem.clear();
    ps.mem_bytes = 0;
    ps.meta.watermark = tail_seq;
    st.pending_flushes.remove(name);
    ps.meta.dropped = false;
    sync_partition_meta(&mut st, &ps);
    publish_manifest(&*self.store, &mut st)?;
    gc_journal(&*self.store, &mut st);
    for seg in old {
      if eager {
        let _ = self.store.delete(&segment_key(&prefix, name, seg.id));
      }
      self.index_cache.remove(seg.id);
    }
    Ok(())
  }

  pub(crate) fn rm_partition(&self, name: &str) -> Result<()> {
    self.ensure_writable()?;
    let _del = self.readers.exclusive();
    self.wait_compaction_finished(name);
    let Some(lock) = self.partition_lock(name) else {
      return Ok(());
    };
    let mut ps = lock.write();
    let mut st = self.state.write();
    let prefix = st.cfg.prefix.clone();
    let eager = st.cfg.eager_object_delete;
    let tail_seq = st.journal_seq;
    let old = std::mem::take(&mut ps.meta.segments);
    ps.mem.clear();
    ps.mem_bytes = 0;
    ps.meta.watermark = tail_seq;
    st.pending_flushes.remove(name);
    ps.meta.dropped = true;
    sync_partition_meta(&mut st, &ps);
    publish_manifest(&*self.store, &mut st)?;
    gc_journal(&*self.store, &mut st);
    for seg in old {
      if eager {
        let _ = self.store.delete(&segment_key(&prefix, name, seg.id));
      }
      self.index_cache.remove(seg.id);
    }
    Ok(())
  }

  /// Flush + merge-compact a single partition.
  pub(crate) fn compact_partition(&self, name: &str) -> Result<()> {
    self.ensure_writable()?;
    self.compact_partition_detached(name)
  }

  /// Flush + merge-compact every non-dropped partition.
  pub(crate) fn compact_all(&self) -> Result<()> {
    self.ensure_writable()?;
    let pending: Vec<String> = self.state.read().pending_flushes.keys().cloned().collect();
    for name in pending {
      finish_partition_flush(self, &name)?;
    }
    let names = self.partitions.names();
    for name in names {
      self.compact_partition_detached(&name)?;
    }
    Ok(())
  }

  /// Flush a partition memtable using the detached upload pipeline so the
  /// segment PUT does not happen while any partition/global lock is held.
  fn flush_partition_detached(&self, part: &str) -> Result<()> {
    if self.state.read().pending_flushes.contains_key(part) {
      finish_partition_flush(self, part)?;
    }
    loop {
      let pending = {
        let Some(lock) = self.partition_lock(part) else {
          return Ok(());
        };
        let mut ps = lock.write();
        if ps.mem.is_empty() {
          return Ok(());
        }
        let mut st = self.state.write();
        let entries: SegmentEntries = ps
          .mem
          .iter()
          .map(|(k, e)| {
            let v = match e {
              MemEntry::Value(v) => Some(v.clone()),
              MemEntry::Tombstone => None,
            };
            (k.clone(), v)
          })
          .collect();
        let encoded = encode_segment(&entries, st.cfg.block_size as usize)?;
        let segment_id = st.next_segment_id;
        st.next_segment_id += 1;
        let pending = PendingFlush {
          partition: ps.name.clone(),
          watermark: ps.meta.watermark.max(st.journal_seq),
          segment_id,
          encoded,
          entries,
        };
        st.pending_flushes.insert(part.to_string(), pending.clone());
        ps.mem.clear();
        ps.mem_bytes = 0;
        Some(pending)
      };
      if let Some(pending) = pending {
        finish_partition_flush(self, &pending.partition)?;
      }
    }
  }

  /// Compact one partition without holding the partition/global lock during
  /// the remote reads or the merged-segment upload. The compaction is marked
  /// in-flight under a brief write lock, runs remote I/O outside, then applies
  /// the result under a brief write lock.
  pub(crate) fn compact_partition_detached(&self, part: &str) -> Result<()> {
    self.flush_partition_detached(part)?;
    let Some(begin) = ({
      let Some(lock) = self.partition_lock(part) else {
        return Ok(());
      };
      let mut ps = lock.write();
      if ps.compacting || ps.meta.dropped || ps.meta.segments.len() < 2 {
        return Ok(());
      }
      let mut st = self.state.write();
      let segs = ps.meta.segments.clone();
      let begin = DetachedCompact {
        prefix: st.cfg.prefix.clone(),
        segs,
        segment_id: st.next_segment_id,
        embed_index: st.cfg.manifest_embed_index,
        block_size: st.cfg.block_size as usize,
      };
      st.next_segment_id += 1;
      ps.compacting = true;
      Some(begin)
    }) else {
      return Ok(());
    };

    // Remote I/O while no partition/global lock is held.
    let remote_result: Result<Option<SegmentMeta>> = (|| {
      let all = read_segments_parallel(&*self.store, &begin.prefix, part, &begin.segs)?;
      let out = merge_entries(all);
      if out.is_empty() {
        return Ok(None);
      }
      let encoded = encode_segment(&out, begin.block_size)?;
      let key = segment_key(&begin.prefix, part, begin.segment_id);
      self.store.put(&key, &encoded)?;
      Ok(Some(build_segment_meta(
        begin.segment_id,
        begin.segs.last().map(|s| s.seq).unwrap_or(0),
        &encoded,
        &out,
        begin.embed_index,
      )?))
    })();
    let merged = match remote_result {
      Ok(m) => m,
      Err(e) => {
        if let Some(lock) = self.partition_lock(part) {
          lock.write().compacting = false;
        }
        return Err(e);
      }
    };

    let old_ids: std::collections::BTreeSet<u64> = begin.segs.iter().map(|s| s.id).collect();
    let removed = {
      let Some(lock) = self.partition_lock(part) else {
        if let Some(m) = &merged {
          let _ = self.store.delete(&segment_key(&begin.prefix, part, m.id));
        }
        return Ok(());
      };
      let mut ps = lock.write();
      if !ps.compacting {
        if let Some(m) = &merged {
          let _ = self.store.delete(&segment_key(&begin.prefix, part, m.id));
        }
        return Ok(());
      }
      ps.compacting = false;
      let current_ids: std::collections::BTreeSet<u64> =
        ps.meta.segments.iter().map(|s| s.id).collect();
      if !old_ids.is_subset(&current_ids) {
        if let Some(m) = &merged {
          let _ = self.store.delete(&segment_key(&begin.prefix, part, m.id));
        }
        return Ok(());
      }
      let removed: Vec<SegmentMeta> = ps
        .meta
        .segments
        .iter()
        .filter(|s| old_ids.contains(&s.id))
        .cloned()
        .collect();
      ps.meta.segments.retain(|s| !old_ids.contains(&s.id));
      if let Some(m) = merged {
        ps.meta.segments.insert(0, m);
      }
      let mut st = self.state.write();
      sync_partition_meta(&mut st, &ps);
      publish_manifest(&*self.store, &mut st)?;
      gc_journal(&*self.store, &mut st);
      st.compactions_completed += 1;
      removed
    };
    self.delete_old_segments(part, removed);
    Ok(())
  }

  /// Delete old segment objects after the partition lock has been released,
  /// waiting for any in-flight readers to drain first.
  fn delete_old_segments(&self, part: &str, old: Vec<SegmentMeta>) {
    if old.is_empty() {
      return;
    }
    let _del = self.readers.exclusive();
    let (eager, prefix) = {
      let st = self.state.read();
      (st.cfg.eager_object_delete, st.cfg.prefix.clone())
    };
    for seg in old {
      if eager {
        let _ = self.store.delete(&segment_key(&prefix, part, seg.id));
      }
      self.index_cache.remove(seg.id);
    }
  }
}

impl Engine for ObjectLsm {
  type Error = Error;
  type Partition = ObjectLsmPartition;
  type Batch = ObjectLsmBatch;

  fn partition(&self, name: &str) -> Result<Self::Partition> {
    self.inner.touch_partition(name)?;
    Ok(ObjectLsmPartition {
      name: name.to_string(),
      inner: self.inner.clone(),
    })
  }

  fn partition_exists(&self, name: &str) -> bool {
    self
      .inner
      .partitions
      .get(name)
      .map(|lock| !lock.read().meta.dropped)
      .unwrap_or(false)
  }

  fn list_partitions(&self) -> Result<Vec<String>> {
    Ok(
      self
        .inner
        .partitions
        .snapshot()
        .iter()
        .filter(|lock| !lock.read().meta.dropped)
        .map(|lock| lock.read().name.clone())
        .collect(),
    )
  }

  fn rm_partition(&self, partition: &Self::Partition) -> Result<()> {
    self.inner.rm_partition(&partition.name)
  }

  fn write_buffer_size(&self) -> u64 {
    self
      .inner
      .partitions
      .snapshot()
      .iter()
      .map(|lock| lock.read().mem_bytes)
      .sum()
  }

  fn journal_count(&self) -> usize {
    self.inner.state.read().journal_seqs.len()
  }

  fn journal_disk_space(&self) -> Result<u64> {
    let st = self.inner.state.read();
    Ok(st.journal_sizes.values().copied().sum())
  }

  fn cache_size(&self) -> u64 {
    self.inner.block_cache.used()
  }

  fn cache_capacity(&self) -> u64 {
    self.inner.block_cache.capacity()
  }

  fn compactions_completed(&self) -> usize {
    self.inner.state.read().compactions_completed as usize
  }

  fn batch(&self) -> Self::Batch {
    ObjectLsmBatch::new(self.inner.clone())
  }

  fn batch_with_capacity(&self, capacity: usize) -> Self::Batch {
    ObjectLsmBatch::with_capacity(self.inner.clone(), capacity)
  }

  fn persist(&self) -> Result<()> {
    self.inner.ensure_writable()?;
    // Strict mode: every committed group is already a durable journal object.
    // Windowed mode: force a synchronous flush so this call only returns once
    // everything acknowledged so far is durable.
    if self.inner.state.read().cfg.journal_window_ms.is_some() {
      let mut st = self.inner.state.write();
      flush_journal_pending(&*self.inner.store, &mut st)?;
      gc_journal(&*self.inner.store, &mut st);
    }
    // Background memtable uploads are durable through journal objects even
    // before their segments are published, but persist() also drains any
    // already-detached segment snapshots to reach the fully folded state.
    let parts: Vec<String> = self
      .inner
      .state
      .read()
      .pending_flushes
      .keys()
      .cloned()
      .collect();
    for part in parts {
      finish_partition_flush(&self.inner, &part)?;
    }
    Ok(())
  }

  fn disk_space(&self) -> Result<u64> {
    let segments_mem: u64 = self
      .inner
      .partitions
      .snapshot()
      .iter()
      .map(|lock| {
        let ps = lock.read();
        ps.meta.segments.iter().map(|s| s.bytes).sum::<u64>() + ps.mem_bytes
      })
      .sum();
    let st = self.inner.state.read();
    Ok(segments_mem + st.journal_sizes.values().copied().sum::<u64>() + st.manifest_bytes)
  }

  fn compact(&self) -> Result<()> {
    self.inner.compact_all()
  }
}
