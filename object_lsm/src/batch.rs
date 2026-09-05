//! [`Batch`] trait implementation.

use std::sync::Arc;

use wedb_embed_engine::Batch;

use crate::{
  engine::Inner,
  error::{Error, Result},
  journal::Op,
  partition::ObjectLsmPartition,
};

/// Accumulates mutations (possibly across partitions) and commits them as one
/// atomic journal group.
pub struct ObjectLsmBatch {
  pub(crate) inner: Arc<Inner>,
  pub(crate) ops: Vec<Op>,
}

impl ObjectLsmBatch {
  pub(crate) fn new(inner: Arc<Inner>) -> Self {
    Self {
      inner,
      ops: Vec::new(),
    }
  }

  pub(crate) fn with_capacity(inner: Arc<Inner>, capacity: usize) -> Self {
    Self {
      inner,
      ops: Vec::with_capacity(capacity),
    }
  }
}

impl Batch for ObjectLsmBatch {
  type Error = Error;
  type Partition = ObjectLsmPartition;

  fn insert(&mut self, partition: &Self::Partition, key: &[u8], value: &[u8]) {
    self.ops.push(Op::put(
      partition.name.clone(),
      key.to_vec(),
      value.to_vec(),
    ));
  }

  fn rm(&mut self, partition: &Self::Partition, key: &[u8]) {
    self
      .ops
      .push(Op::delete(partition.name.clone(), key.to_vec()));
  }

  fn len(&self) -> usize {
    self.ops.len()
  }

  fn commit(self) -> Result<()> {
    self.inner.commit_ops(self.ops)
  }
}
