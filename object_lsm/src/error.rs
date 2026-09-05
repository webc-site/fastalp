//! Error type for wedb_object_lsm.

use std::fmt::Display;

use thiserror::Error;

/// Unified error type.
#[derive(Debug, Clone, Error)]
pub enum Error {
  /// Underlying object store failure.
  #[error("object store error: {0}")]
  Store(String),
  /// Corrupted / truncated object payload.
  #[error("corrupted object data: {0}")]
  Corrupt(String),
  /// I/O error.
  #[error("i/o error: {0}")]
  Io(String),
  /// Encoding / decoding failure.
  #[error("encoding error: {0}")]
  Encode(String),
}

impl Error {
  /// Construct a store error from a displayable source.
  pub fn store(err: impl Display) -> Self {
    Error::Store(err.to_string())
  }
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
  fn from(e: std::io::Error) -> Self {
    Error::Io(e.to_string())
  }
}
