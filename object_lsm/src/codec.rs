//! Little-endian binary codec helpers shared by journal/segment formats.

use crate::error::{Error, Result};

pub fn put_u32(out: &mut Vec<u8>, v: u32) {
  out.extend_from_slice(&v.to_le_bytes());
}

pub fn put_u64(out: &mut Vec<u8>, v: u64) {
  out.extend_from_slice(&v.to_le_bytes());
}

pub fn put_bytes(out: &mut Vec<u8>, b: &[u8]) -> Result<()> {
  let len = u32::try_from(b.len()).map_err(|_| Error::Encode("byte slice too long".into()))?;
  put_u32(out, len);
  out.extend_from_slice(b);
  Ok(())
}

pub fn put_str(out: &mut Vec<u8>, s: &str) -> Result<()> {
  put_bytes(out, s.as_bytes())
}

/// Sequential reader over a byte slice with bounds checking.
pub struct Reader<'a> {
  buf: &'a [u8],
  pos: usize,
}

impl<'a> Reader<'a> {
  pub fn new(buf: &'a [u8]) -> Self {
    Self { buf, pos: 0 }
  }

  pub fn remaining(&self) -> usize {
    self.buf.len() - self.pos
  }

  fn take(&mut self, n: usize) -> Result<&'a [u8]> {
    if self.remaining() < n {
      return Err(Error::Corrupt("unexpected end of payload".into()));
    }
    let s = &self.buf[self.pos..self.pos + n];
    self.pos += n;
    Ok(s)
  }

  pub fn u8(&mut self) -> Result<u8> {
    Ok(self.take(1)?[0])
  }

  pub fn u32(&mut self) -> Result<u32> {
    let s = self.take(4)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
  }

  pub fn u64(&mut self) -> Result<u64> {
    let s = self.take(8)?;
    Ok(u64::from_le_bytes([
      s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
  }

  pub fn bytes(&mut self) -> Result<Vec<u8>> {
    let n = self.u32()? as usize;
    Ok(self.take(n)?.to_vec())
  }

  pub fn str(&mut self) -> Result<String> {
    String::from_utf8(self.bytes()?).map_err(|e| Error::Corrupt(format!("invalid utf-8: {e}")))
  }
}
