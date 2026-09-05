//! Cloudflare R2 (S3-compatible) [`Store`] backend, feature `r2`.
//!
//! Wraps `object_store::aws::AmazonS3` and bridges its async API into the
//! synchronous [`Store`] trait with an internal multi-thread tokio runtime.

use std::{
  ops::Range,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  time::Duration,
};

use md5::{Digest, Md5};
use object_store::{
  BackoffConfig, ObjectStore, PutMode, PutOptions, PutPayload, RetryConfig, UpdateVersion,
  aws::{AmazonS3Builder, S3ConditionalPut},
  path::Path as ObjPath,
};
use tokio::runtime::Runtime;

use crate::{
  error::{Error, Result},
  store::Store,
};

/// R2 connection parameters (usually sourced from the environment).
#[derive(Clone, Debug)]
pub struct R2Config {
  pub bucket: String,
  pub endpoint: String,
  pub access_key_id: String,
  pub secret_access_key: String,
}

impl R2Config {
  /// Build from env: `R2_BUCKET`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`,
  /// and either `R2_ENDPOINT` or `R2_ACCOUNT_ID`.
  pub fn from_env() -> Result<Self> {
    fn get(name: &str) -> Result<String> {
      std::env::var(name).map_err(|_| Error::store(format!("missing env {name}")))
    }
    let bucket = get("R2_BUCKET")?;
    let endpoint = match std::env::var("R2_ENDPOINT") {
      Ok(e) if !e.is_empty() => e,
      _ => {
        let account = get("R2_ACCOUNT_ID")?;
        format!("https://{account}.r2.cloudflarestorage.com")
      }
    };
    Ok(Self {
      bucket,
      endpoint,
      access_key_id: get("R2_ACCESS_KEY_ID")?,
      secret_access_key: get("R2_SECRET_ACCESS_KEY")?,
    })
  }
}

/// Sync [`Store`] backed by Cloudflare R2.
///
/// The async `object_store` client is bridged with an internal tokio runtime
/// via `Runtime::block_on`, so `R2Store` methods must be called from a plain
/// (non-async) thread. Calling them from inside a tokio async context will
/// panic.
pub struct R2Store {
  inner: Arc<dyn ObjectStore>,
  rt: Runtime,
  put_failures: AtomicU64,
  get_failures: AtomicU64,
  other_failures: AtomicU64,
}

impl R2Store {
  pub fn new(cfg: R2Config) -> Result<Self> {
    let max_retries = env_usize("OBJLSM_R2_MAX_RETRIES", 30);
    let retry_timeout = Duration::from_secs(env_u64("OBJLSM_R2_RETRY_TIMEOUT_SECS", 300));
    let retry = RetryConfig {
      backoff: BackoffConfig {
        init_backoff: Duration::from_millis(200),
        max_backoff: Duration::from_secs(10),
        base: 2.0,
      },
      max_retries,
      retry_timeout,
    };
    let builder = AmazonS3Builder::new()
      .with_bucket_name(&cfg.bucket)
      .with_region("auto")
      .with_endpoint(&cfg.endpoint)
      .with_access_key_id(&cfg.access_key_id)
      .with_secret_access_key(&cfg.secret_access_key)
      .with_virtual_hosted_style_request(false)
      // Enable If-None-Match / If-Match conditional writes (required by the
      // writer lease's create-if-absent and compare-and-swap on R2).
      .with_conditional_put(S3ConditionalPut::ETagMatch)
      .with_retry(retry);
    let inner = builder.build().map_err(Error::store)?;
    let rt = Runtime::new().map_err(|e| Error::store(format!("tokio runtime: {e}")))?;
    Ok(Self {
      inner: Arc::new(inner),
      rt,
      put_failures: AtomicU64::new(0),
      get_failures: AtomicU64::new(0),
      other_failures: AtomicU64::new(0),
    })
  }

  /// Build directly from environment variables.
  pub fn from_env() -> Result<Self> {
    Self::new(R2Config::from_env()?)
  }

  fn block<T>(&self, fut: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    self.rt.block_on(fut)
  }

  fn obj_err(e: object_store::Error) -> Error {
    Error::store(e.to_string())
  }

  /// Number of object PUTs that ultimately failed (after retries).
  pub fn put_failures(&self) -> u64 {
    self.put_failures.load(Ordering::Relaxed)
  }

  /// Number of object GETs that ultimately failed (after retries).
  pub fn get_failures(&self) -> u64 {
    self.get_failures.load(Ordering::Relaxed)
  }

  /// Number of other object-store calls (delete/list/…) that failed.
  pub fn other_failures(&self) -> u64 {
    self.other_failures.load(Ordering::Relaxed)
  }
}

fn env_u64(name: &str, default: u64) -> u64 {
  std::env::var(name)
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
  std::env::var(name)
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(default)
}

fn path(key: &str) -> Result<ObjPath> {
  ObjPath::from_url_path(key).map_err(|e| Error::store(format!("invalid object key {key:?}: {e}")))
}

impl Store for R2Store {
  fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    let p = path(key)?;
    let inner = self.inner.clone();
    let res = self.block(async move {
      let r = match inner.get(&p).await {
        Ok(r) => r,
        Err(object_store::Error::NotFound { .. }) => return Ok(None),
        Err(e) => return Err(Self::obj_err(e)),
      };
      let b = r.bytes().await.map_err(Self::obj_err)?;
      Ok(Some(b.to_vec()))
    });
    if res.is_err() {
      self.get_failures.fetch_add(1, Ordering::Relaxed);
    }
    res
  }

  fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Option<Vec<u8>>> {
    let p = path(key)?;
    let start =
      usize::try_from(offset).map_err(|_| Error::store("range offset exceeds platform usize"))?;
    let end = usize::try_from(offset.saturating_add(len))
      .map_err(|_| Error::store("range end exceeds platform usize"))?;
    let range = Range { start, end };
    let inner = self.inner.clone();
    let res = self.block(async move {
      match inner.get_range(&p, range).await {
        Ok(b) => Ok(Some(b.to_vec())),
        Err(object_store::Error::NotFound { .. }) => Ok(None),
        Err(e) => Err(Self::obj_err(e)),
      }
    });
    if res.is_err() {
      self.get_failures.fetch_add(1, Ordering::Relaxed);
    }
    res
  }

  fn put(&self, key: &str, data: &[u8]) -> Result<()> {
    let p = path(key)?;
    let inner = self.inner.clone();
    let payload = PutPayload::from(data.to_vec());
    let res = self.block(async move {
      inner
        .put(&p, payload)
        .await
        .map(|_| ())
        .map_err(Self::obj_err)
    });
    if res.is_err() {
      self.put_failures.fetch_add(1, Ordering::Relaxed);
    }
    res
  }

  fn put_if_matches(&self, key: &str, expected: &[u8], new: &[u8]) -> Result<bool> {
    // R2/S3 single-part object ETag is the quoted MD5 of the payload, so a
    // byte-level compare-and-swap maps to If-Match.
    let etag = format!("{:x}", Md5::digest(expected));
    let mode = PutMode::Update(UpdateVersion {
      e_tag: Some(etag),
      version: None,
    });
    let p = path(key)?;
    let inner = self.inner.clone();
    let payload = PutPayload::from(new.to_vec());
    let res = self.block(async move {
      match inner
        .put_opts(
          &p,
          payload,
          PutOptions {
            mode,
            ..Default::default()
          },
        )
        .await
      {
        Ok(_) => Ok(true),
        Err(object_store::Error::Precondition { .. }) => Ok(false),
        Err(e) => Err(Self::obj_err(e)),
      }
    });
    if res.is_err() {
      self.put_failures.fetch_add(1, Ordering::Relaxed);
    }
    res
  }

  fn create(&self, key: &str, data: &[u8]) -> Result<bool> {
    let p = path(key)?;
    let inner = self.inner.clone();
    let payload = PutPayload::from(data.to_vec());
    let res = self.block(async move {
      match inner
        .put_opts(
          &p,
          payload,
          PutOptions {
            mode: PutMode::Create,
            ..Default::default()
          },
        )
        .await
      {
        Ok(_) => Ok(true),
        Err(object_store::Error::AlreadyExists { .. }) => Ok(false),
        Err(e) => Err(Self::obj_err(e)),
      }
    });
    if res.is_err() {
      self.put_failures.fetch_add(1, Ordering::Relaxed);
    }
    res
  }

  fn delete(&self, key: &str) -> Result<()> {
    let p = path(key)?;
    let inner = self.inner.clone();
    let res = self.block(async move {
      match inner.delete(&p).await {
        Ok(()) => Ok(()),
        Err(object_store::Error::NotFound { .. }) => Ok(()),
        Err(e) => Err(Self::obj_err(e)),
      }
    });
    if res.is_err() {
      self.other_failures.fetch_add(1, Ordering::Relaxed);
    }
    res
  }

  fn list(&self, prefix: &str) -> Result<Vec<String>> {
    let p = ObjPath::from(prefix);
    let inner = self.inner.clone();
    let res = self.block(async move {
      let mut out = Vec::new();
      let mut stream = inner.list(Some(&p));
      use futures_util::StreamExt;
      while let Some(meta) = stream.next().await {
        let meta = meta.map_err(Self::obj_err)?;
        out.push(meta.location.to_string());
      }
      Ok(out)
    });
    if res.is_err() {
      self.other_failures.fetch_add(1, Ordering::Relaxed);
    }
    res
  }
}
