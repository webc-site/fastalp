//! Persisted engine manifest.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
  error::{Error, Result},
  segment::SegmentMeta,
};

/// A manifest snapshot: the durable description of live segments and the
/// per-partition journal watermark.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
  /// Manifest sequence number (monotonic).
  pub seq: u64,
  /// Next segment id to hand out.
  pub next_segment_id: u64,
  /// Last journal group sequence assigned at write time.
  pub next_journal_seq: u64,
  /// Per-partition durable metadata.
  pub partitions: BTreeMap<String, PartitionMeta>,
  /// Fencing epoch of the writer (0 = unfenced).
  #[serde(default)]
  pub fence_epoch: u128,
}

impl Manifest {
  pub fn encode(&self) -> Result<Vec<u8>> {
    serde_json::to_vec(self).map_err(|e| Error::Encode(format!("manifest encode: {e}")))
  }

  pub fn decode(buf: &[u8]) -> Result<Self> {
    serde_json::from_slice(buf).map_err(|e| Error::Corrupt(format!("manifest decode: {e}")))
  }
}

/// Durable per-partition state.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionMeta {
  /// Immutable segments, oldest first.
  pub segments: Vec<SegmentMeta>,
  /// Highest journal seq already folded into durable state (segments or an
  /// intentional discard via clear / rm_partition).
  pub watermark: u64,
  /// Whether the partition was dropped; kept so the watermark survives
  /// recreation and stale journal groups are never resurrected.
  pub dropped: bool,
}
