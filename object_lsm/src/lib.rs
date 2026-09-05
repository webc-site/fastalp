//! wedb_object_lsm —— Object-storage-backed LSM storage engine.
//!
//! Implements the [`wedb_embed_engine`] [`Engine`](wedb_embed_engine::Engine) /
//! [`Partition`](wedb_embed_engine::Partition) / [`Batch`](wedb_embed_engine::Batch)
//! traits on top of S3-compatible object storage (AWS S3 / Cloudflare R2 / MinIO).
//!
//! Data model:
//! - committed writes first land in a per-instance **journal** (immutable objects),
//!   giving cross-partition atomic batches + crash-safe replay;
//! - per-partition **memtables** absorb recent writes for µs-scale hot reads;
//! - full memtables are flushed into immutable **block-indexed segment** objects;
//! - a small **manifest** object records live segments + per-partition watermark.
//!
//! M2 done: block-indexed segments, byte-range reads, block/index caches and
//! ordered streaming iteration. M3 done: merge compaction, journal GC and
//! orphan GC at open. M4 done: wedb_embed parity harness + benchmarks.
//! R2/S3 remote Store backend (2 feature) done; live-verified on R2.

pub mod batch;
pub mod cache;
pub mod codec;
pub mod config;
pub mod engine;
pub mod error;
pub mod file;
pub mod journal;
pub mod keys;
pub mod lease;
pub mod manifest;
pub mod metrics;
pub mod partition;
#[cfg(feature = "r2")]
pub mod r2;
mod scan;
pub mod segment;
pub mod state;
pub mod store;

pub use batch::ObjectLsmBatch;
pub use cache::{BlockCache, IndexCache};
pub use config::Config;
pub use engine::ObjectLsm;

impl ObjectLsm {
  /// Shared object-store handle used by this engine instance.
  pub fn store(&self) -> &std::sync::Arc<dyn Store> {
    &self.inner.store
  }
}

pub use error::{Error, Result};
pub use metrics::MetricsSnapshot;
pub use partition::{ObjectLsmEntry, ObjectLsmIter, ObjectLsmPartition};
pub use store::{MemoryStore, Store};

/// Bridge so `wedb_embed` can use this engine as a backend
/// (`wedb_embed::Error: From<E::Error>` bound on its `Db` methods).
#[cfg(feature = "wedb")]
impl From<Error> for wedb_embed::Error {
  fn from(e: Error) -> Self {
    wedb_embed::Error::engine(e.to_string())
  }
}

pub use file::FileStore;
pub use lease::{Lease, LeaseOptions};
#[cfg(feature = "r2")]
pub use r2::{R2Config, R2Store};
