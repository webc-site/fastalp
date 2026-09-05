#![allow(dead_code)]
//! Cross-platform Redis transport + process RSS helpers shared by the
//! `redis_vs_wedb` and `footprint` benchmarks.
//!
//! On Unix the benchmarks talk to `redis-server` over a Unix domain socket
//! (`/tmp/wedb_redis_bench.sock`); on Windows they fall back to TCP on
//! `127.0.0.1:6379`. RSS measurement uses `getrusage`/`/proc` on Unix and
//! `GetProcessMemoryInfo` (working set) on Windows.

#[cfg(windows)]
use std::net::TcpStream;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::{
  io::{self, Read, Write},
  path::PathBuf,
  time::Duration,
};

#[cfg(unix)]
pub const REDIS_SOCK: &str = "/tmp/wedb_redis_bench.sock";
#[cfg(windows)]
pub const REDIS_ADDR: &str = "127.0.0.1:6379";

/// A Redis connection over the platform's native transport.
pub enum RedisConn {
  #[cfg(unix)]
  Unix(UnixStream),
  #[cfg(windows)]
  Tcp(TcpStream),
}

impl RedisConn {
  pub fn connect() -> io::Result<Self> {
    Self::connect_with_timeout(Duration::from_millis(1000), Duration::from_millis(1000))
  }

  pub fn connect_with_timeout(read_timeout: Duration, write_timeout: Duration) -> io::Result<Self> {
    #[cfg(unix)]
    {
      let s = UnixStream::connect(REDIS_SOCK)?;
      s.set_read_timeout(Some(read_timeout))?;
      s.set_write_timeout(Some(write_timeout))?;
      Ok(Self::Unix(s))
    }
    #[cfg(windows)]
    {
      let s = TcpStream::connect(REDIS_ADDR)?;
      s.set_read_timeout(Some(read_timeout))?;
      s.set_write_timeout(Some(write_timeout))?;
      let _ = s.set_nodelay(true);
      Ok(Self::Tcp(s))
    }
  }

  pub fn set_read_timeout(&self, d: Option<Duration>) -> io::Result<()> {
    match self {
      #[cfg(unix)]
      Self::Unix(s) => s.set_read_timeout(d),
      #[cfg(windows)]
      Self::Tcp(s) => s.set_read_timeout(d),
    }
  }

  pub fn set_write_timeout(&self, d: Option<Duration>) -> io::Result<()> {
    match self {
      #[cfg(unix)]
      Self::Unix(s) => s.set_write_timeout(d),
      #[cfg(windows)]
      Self::Tcp(s) => s.set_write_timeout(d),
    }
  }
}

impl Read for RedisConn {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    match self {
      #[cfg(unix)]
      Self::Unix(s) => s.read(buf),
      #[cfg(windows)]
      Self::Tcp(s) => s.read(buf),
    }
  }
}

impl Write for RedisConn {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    match self {
      #[cfg(unix)]
      Self::Unix(s) => s.write(buf),
      #[cfg(windows)]
      Self::Tcp(s) => s.write(buf),
    }
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

/// Base `redis-cli` connection arguments (socket on Unix, TCP on Windows).
pub fn redis_cli_base_args() -> Vec<&'static str> {
  #[cfg(unix)]
  {
    vec!["-s", REDIS_SOCK]
  }
  #[cfg(windows)]
  {
    vec!["-h", "127.0.0.1", "-p", "6379"]
  }
}

/// Redis server data directory.
pub fn redis_data_dir() -> PathBuf {
  #[cfg(unix)]
  {
    PathBuf::from("/tmp/wedb_redis_bench_data")
  }
  #[cfg(windows)]
  {
    std::env::temp_dir().join("wedb_redis_bench_data")
  }
}

/// Reusable WeDb benchmark data directory.
pub fn wedb_bench_dir() -> PathBuf {
  #[cfg(unix)]
  {
    PathBuf::from("/tmp/wedb_bench_data_5gb")
  }
  #[cfg(windows)]
  {
    std::env::temp_dir().join("wedb_bench_data_5gb")
  }
}

/// Resident set size (physical memory) of the current process, in bytes.
pub fn rss_bytes() -> u64 {
  #[cfg(unix)]
  {
    unix_rss_bytes()
  }
  #[cfg(windows)]
  {
    windows_rss_bytes()
  }
  #[cfg(not(any(unix, windows)))]
  {
    0
  }
}

#[cfg(unix)]
fn unix_rss_bytes() -> u64 {
  use std::mem::MaybeUninit;

  let mut usage = MaybeUninit::<libc::rusage>::uninit();
  let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
  if ret != 0 {
    return 0;
  }
  let usage = unsafe { usage.assume_init() };

  #[cfg(target_os = "macos")]
  {
    usage.ru_maxrss as u64
  }
  #[cfg(target_os = "linux")]
  {
    if let Ok(s) = std::fs::read_to_string("/proc/self/statm") {
      if let Some(pages) = s
        .split_whitespace()
        .nth(1)
        .and_then(|p| p.parse::<u64>().ok())
      {
        return pages * 4096;
      }
    }
    (usage.ru_maxrss as u64) * 1024
  }
  #[cfg(not(any(target_os = "macos", target_os = "linux")))]
  {
    usage.ru_maxrss as u64
  }
}

#[cfg(windows)]
#[allow(dead_code)]
fn windows_rss_bytes() -> u64 {
  use std::os::raw::c_void;

  #[repr(C)]
  struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
  }

  #[link(name = "kernel32")]
  unsafe extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
  }
  #[link(name = "psapi")]
  unsafe extern "system" {
    fn GetProcessMemoryInfo(
      process: *mut c_void,
      counters: *mut ProcessMemoryCounters,
      cb: u32,
    ) -> i32;
  }

  let mut pmc = ProcessMemoryCounters {
    cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
    page_fault_count: 0,
    peak_working_set_size: 0,
    working_set_size: 0,
    quota_peak_paged_pool_usage: 0,
    quota_paged_pool_usage: 0,
    quota_peak_non_paged_pool_usage: 0,
    quota_non_paged_pool_usage: 0,
    pagefile_usage: 0,
    peak_pagefile_usage: 0,
  };

  unsafe {
    if GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) != 0 {
      pmc.working_set_size as u64
    } else {
      0
    }
  }
}
