//! Object-storage abstraction + in-memory implementation for tests.

use std::{
  collections::BTreeMap,
  sync::{Arc, Mutex},
};

use crate::error::{Error, Result};

/// Minimal object-storage interface required by the engine.
///
/// All methods are synchronous; a remote implementation (AWS S3 / Cloudflare R2 /
/// MinIO via `object_store`) bridges async SDK calls with an internal runtime.
pub trait Store: Send + Sync + 'static {
  /// Fetch an object; `None` when the key does not exist.
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;

  /// Fetch a byte range `[offset, offset + len)` of an object; `None` when the
  /// key does not exist. Reads past the end return the available tail bytes
  /// (matching S3 Range GET semantics).
  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>>;

  /// Atomically create or overwrite an object.
  fn put(&self, key: &str, data: &[u8]) -> Result<()>;

  /// Delete an object; deleting a missing key is a no-op.
  fn delete(&self, key: &str) -> Result<()>;

  /// Atomically create an object only if it does not exist yet.
  ///
  /// Returns `true` when the object was created, `false` when it already
  /// exists (used by the writer lease; default backend reports unsupported).
  fn create(&self, _key: &str, _data: &[u8]) -> Result<bool> {
    Err(Error::store("create-if-absent not supported by this store"))
  }

  /// Atomically replace an object only if its current content equals
  /// `expected`; returns whether the replacement happened (compare-and-swap).
  fn put_if_matches(&self, _key: &str, _expected: &[u8], _new: &[u8]) -> Result<bool> {
    Err(Error::store("compare-and-swap not supported by this store"))
  }

  /// List object keys under `prefix` in lexicographic order.
  fn list(&self, prefix: &str) -> Result<Vec<String>>;
}

#[derive(Default)]
struct MemoryInner {
  objects: BTreeMap<String, Vec<u8>>,
}

/// In-memory [`Store`] used for unit tests and offline development.
#[derive(Clone, Default)]
pub struct MemoryStore {
  inner: Arc<Mutex<MemoryInner>>,
}

impl MemoryStore {
  /// Create an empty in-memory store.
  pub fn new() -> Self {
    Self::default()
  }
}

impl Store for MemoryStore {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    Ok(self.inner.lock().unwrap().objects.get(key).cloned())
  }

  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
    let g = self.inner.lock().unwrap();
    let Some(obj) = g.objects.get(key) else {
      return Ok(None);
    };
    let start = (offset as usize).min(obj.len());
    let end = start.saturating_add(len as usize).min(obj.len());
    Ok(Some(obj[start..end].to_vec()))
  }

  fn put(&self, key: &str, data: &[u8]) -> Result<()> {
    self
      .inner
      .lock()
      .unwrap()
      .objects
      .insert(key.to_string(), data.to_vec());
    Ok(())
  }

  fn delete(&self, key: &str) -> Result<()> {
    self.inner.lock().unwrap().objects.remove(key);
    Ok(())
  }

  fn create(&self, key: &str, data: &[u8]) -> Result<bool> {
    let mut g = self.inner.lock().unwrap();
    if g.objects.contains_key(key) {
      Ok(false)
    } else {
      g.objects.insert(key.to_string(), data.to_vec());
      Ok(true)
    }
  }

  fn put_if_matches(&self, key: &str, expected: &[u8], new: &[u8]) -> Result<bool> {
    let mut g = self.inner.lock().unwrap();
    if g
      .objects
      .get(key)
      .map(|v| v.as_slice() == expected)
      .unwrap_or(false)
    {
      g.objects.insert(key.to_string(), new.to_vec());
      Ok(true)
    } else {
      Ok(false)
    }
  }

  fn list(&self, prefix: &str) -> Result<Vec<String>> {
    Ok(
      self
        .inner
        .lock()
        .unwrap()
        .objects
        .range(prefix.to_string()..)
        .take_while(|(k, _)| k.starts_with(prefix))
        .map(|(k, _)| k.clone())
        .collect(),
    )
  }
}
