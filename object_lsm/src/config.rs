//! Engine configuration.

/// Default per-partition memtable budget before it is flushed to a segment.
pub const DEFAULT_MAX_MEMTABLE_BYTES: u64 = 16 * 1024 * 1024;
/// Default target data-block payload size inside segment objects.
pub const DEFAULT_BLOCK_SIZE: u32 = 32 * 1024;
/// Default block cache capacity.
pub const DEFAULT_CACHE_CAPACITY: u64 = 64 * 1024 * 1024;
/// Segments per partition allowed before an automatic merge compaction.
pub const DEFAULT_MAX_SEGMENTS_BEFORE_COMPACT: usize = 16;
/// Default object-key prefix.
pub const DEFAULT_PREFIX: &str = "wedb/objectlsm";

/// Engine configuration.
#[derive(Clone, Debug)]
pub struct Config {
  /// Object key prefix that isolates this engine instance inside a bucket.
  pub prefix: String,
  /// Flush a partition's memtable to an immutable segment once its estimated
  /// byte size exceeds this budget.
  pub max_memtable_bytes: u64,
  /// Target data-block payload size inside segment objects (bytes).
  pub block_size: u32,
  /// Block cache capacity in bytes (0 disables the cache).
  pub cache_capacity: u64,
  /// Merge-compact a partition once it holds this many segments.
  pub max_segments_before_compact: usize,
  /// Group-commit journal window in ms. `None` keeps the strict per-commit
  /// durable PUT; `Some(ms)` batches concurrent/queued commits into one
  /// journal object flushed every `ms` (or when the buffer fills), which cuts
  /// object-store write amplification dramatically. Acknowledged commits in
  /// this mode are durable after the next flush (`persist()` forces one).
  pub journal_window_ms: Option<u64>,
  /// Upper bound of the in-memory pending journal buffer (bytes) before a
  /// synchronous flush is forced.
  pub journal_max_buffer_bytes: u64,
  /// Flush full memtables on a background worker instead of inside the commit
  /// path. Commits remain durable through journal objects; the worker uploads
  /// immutable segment objects and publishes the manifest later. Explicit
  /// `persist()` / `compact()` still perform synchronous maintenance.
  pub background_flush: bool,
  /// Embed each segment's block index in the manifest. `true` (default) makes
  /// a cold point read a single block Range GET; `false` shrinks the manifest
  /// at the cost of tail+index Range GETs on cold reads.
  pub manifest_embed_index: bool,
  /// When `true` (default), replaced segment objects are deleted eagerly after
  /// compaction, `clear_partition`, `rm_partition`, and startup GC. When
  /// `false`, orphaned segment objects are intentionally left behind so an
  /// external object-store lifecycle rule (for example Cloudflare R2 bucket
  /// versioning + lifecycle) can reclaim them later. Reads and recovery are
  /// unaffected; journal GC keeps its own separate policy.
  pub eager_object_delete: bool,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      prefix: DEFAULT_PREFIX.into(),
      max_memtable_bytes: DEFAULT_MAX_MEMTABLE_BYTES,
      block_size: DEFAULT_BLOCK_SIZE,
      cache_capacity: DEFAULT_CACHE_CAPACITY,
      max_segments_before_compact: DEFAULT_MAX_SEGMENTS_BEFORE_COMPACT,
      journal_window_ms: None,
      journal_max_buffer_bytes: 1024 * 1024,
      background_flush: false,
      eager_object_delete: true,
      manifest_embed_index: true,
    }
  }
}

impl Config {
  /// Config with a custom object-key prefix.
  pub fn new(prefix: impl Into<String>) -> Self {
    Self {
      prefix: prefix.into(),
      ..Self::default()
    }
  }

  /// Set the per-partition memtable flush budget.
  pub fn max_memtable_bytes(mut self, bytes: u64) -> Self {
    self.max_memtable_bytes = bytes;
    self
  }

  /// Set the target segment data-block payload size.
  pub fn block_size(mut self, bytes: u32) -> Self {
    self.block_size = bytes;
    self
  }

  /// Set the block cache capacity in bytes.
  pub fn cache_capacity(mut self, bytes: u64) -> Self {
    self.cache_capacity = bytes;
    self
  }

  /// Set the segment count that triggers automatic merge compaction.
  pub fn max_segments_before_compact(mut self, n: usize) -> Self {
    self.max_segments_before_compact = n;
    self
  }

  /// Set the group-commit journal window (None = strict per-commit PUT).
  pub fn journal_window_ms(mut self, ms: Option<u64>) -> Self {
    self.journal_window_ms = ms;
    self
  }

  /// Upper bound of the pending journal buffer before a synchronous flush.
  pub fn journal_max_buffer_bytes(mut self, bytes: u64) -> Self {
    self.journal_max_buffer_bytes = bytes;
    self
  }

  /// Move memtable segment uploads off the commit path.
  pub fn background_flush(mut self, enabled: bool) -> Self {
    self.background_flush = enabled;
    self
  }

  /// Control whether segment block indexes are embedded in the manifest.
  pub fn manifest_embed_index(mut self, embed: bool) -> Self {
    self.manifest_embed_index = embed;
    self
  }

  /// Control whether replaced segment objects are deleted eagerly.
  ///
  /// Disable this when object-store lifecycle rules are responsible for
  /// reclaiming orphaned segment objects; the engine then skips the per-object
  /// `DELETE` calls after compaction / clear / rm / startup GC.
  pub fn eager_object_delete(mut self, enabled: bool) -> Self {
    self.eager_object_delete = enabled;
    self
  }

  /// Config for one shard of a shared bucket layout: each shard owns a
  /// disjoint `<base>/shard-<id>` prefix, so separate writer instances can
  /// run concurrently without colliding.
  pub fn for_shard(base: impl Into<String>, shard: u64) -> Self {
    Self::new(format!("{}/shard-{shard}", base.into()))
  }
}
