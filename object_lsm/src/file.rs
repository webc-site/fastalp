//! Local filesystem [`Store`] implementation.
//!
//! Objects map to files under `root/key`. This is a simple, test-oriented
//! backend (and a local-disk reference backend for benchmarks); it is not
//! guaranteed to be crash-atomic at the object level.

use std::{
  fs,
  io::Write as _,
  path::{Path, PathBuf},
  sync::Arc,
};

use crate::{
  error::{Error, Result},
  store::Store,
};

/// Local directory-backed [`Store`].
#[derive(Clone)]
pub struct FileStore {
  root: Arc<PathBuf>,
}

impl FileStore {
  pub fn new(root: impl AsRef<Path>) -> Result<Self> {
    fs::create_dir_all(root.as_ref()).map_err(Error::from)?;
    Ok(Self {
      root: Arc::new(root.as_ref().to_path_buf()),
    })
  }

  fn path_of(&self, key: &str) -> Result<PathBuf> {
    let mut path = (*self.root).clone();
    for comp in key.split('/') {
      if comp.is_empty() || comp == "." || comp == ".." || comp.contains('\\') {
        return Err(Error::store(format!("unsafe object key {key:?}")));
      }
      path.push(comp);
    }
    Ok(path)
  }

  fn key_of(&self, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(&*self.root).ok()?;
    let parts: Vec<&str> = rel
      .components()
      .filter_map(|c| match c {
        std::path::Component::Normal(s) => s.to_str(),
        _ => None,
      })
      .collect();
    Some(parts.join("/"))
  }

  fn hook_abort(&self, env: &str, key: &str) {
    if std::env::var_os(env).is_some() && key.contains("/seg/") {
      std::process::abort();
    }
  }
}

impl Store for FileStore {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    match fs::read(self.path_of(key)?) {
      Ok(b) => Ok(Some(b)),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
      Err(e) => Err(Error::from(e)),
    }
  }

  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
    let Some(bytes) = self.get(key)? else {
      return Ok(None);
    };
    let start = (offset as usize).min(bytes.len());
    let end = start.saturating_add(len as usize).min(bytes.len());
    Ok(Some(bytes[start..end].to_vec()))
  }

  fn put(&self, key: &str, data: &[u8]) -> Result<()> {
    self.hook_abort("WEDB_FS_ABORT_ON_PUT_SEGMENT", key);
    let path = self.path_of(key)?;
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).map_err(Error::from)?;
    }
    let mut f = fs::File::create(&path).map_err(Error::from)?;
    f.write_all(data).map_err(Error::from)?;
    f.sync_all().ok();
    Ok(())
  }

  fn delete(&self, key: &str) -> Result<()> {
    self.hook_abort("WEDB_FS_ABORT_ON_DELETE_SEGMENT", key);
    match fs::remove_file(self.path_of(key)?) {
      Ok(()) => Ok(()),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
      Err(e) => Err(Error::from(e)),
    }
  }

  fn create(&self, key: &str, data: &[u8]) -> Result<bool> {
    let path = self.path_of(key)?;
    if path.exists() {
      return Ok(false);
    }
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).map_err(Error::from)?;
    }
    match fs::OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(&path)
    {
      Ok(mut f) => {
        f.write_all(data).map_err(Error::from)?;
        Ok(true)
      }
      Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
      Err(e) => Err(Error::from(e)),
    }
  }

  fn put_if_matches(&self, key: &str, expected: &[u8], new: &[u8]) -> Result<bool> {
    if self.get(key)?.as_deref() == Some(expected) {
      self.put(key, new)?;
      Ok(true)
    } else {
      Ok(false)
    }
  }

  fn list(&self, prefix: &str) -> Result<Vec<String>> {
    fn collect(
      dir: &Path,
      store: &FileStore,
      prefix: &str,
      keys: &mut Vec<String>,
    ) -> std::io::Result<()> {
      for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
          collect(&path, store, prefix, keys)?;
        } else if let Some(key) = store.key_of(&path)
          && key.starts_with(prefix)
        {
          keys.push(key);
        }
      }
      Ok(())
    }
    let mut keys = Vec::new();
    collect(&self.root, self, prefix, &mut keys).map_err(Error::from)?;
    keys.sort();
    Ok(keys)
  }
}
