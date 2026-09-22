//! Microbenchmark suite for fastalp compression and decompression algorithms.
//! fastalp 浮点压缩与解压算法微基准测试套件。

use divan::{Bencher, black_box};
use fastalp::{Encoder, compress, compress_into, decompress_into, max_compressed_size};

/// Global high-performance memory allocator (mimalloc).
/// 全局高性能内存分配器（mimalloc）。
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Single block standard vector element count (1024 floats).
/// 单块标准向量元素数（1024 浮点数）。
const BLOCK_SIZE: usize = 1024;

/// Large batch throughput evaluation vector size (65536 floats, f64 is 512 KB, f32 is 256 KB).
/// 大批量吞吐评测向量元素数（65536 浮点数，对应 f64 512 KB，f32 256 KB）。
const LARGE_BATCH_SIZE: usize = 65536;

/// Divan benchmark runner main entry point.
/// Divan 基准测试运行器主入口函数。
fn main() {
  divan::main();
}

/// Generate simulated sensor decimal fractional f64 time-series data.
/// 生成模拟传感器十进制小数 f64 时序数据。
fn generate_sensor_data(count: usize) -> Vec<f64> {
  (0..count).map(|i| (200 + (i % 150)) as f64 * 0.1).collect()
}

/// Generate simulated sensor decimal fractional f32 time-series data.
/// 生成模拟传感器十进制小数 f32 时序数据。
fn generate_sensor_data_f32(count: usize) -> Vec<f32> {
  (0..count)
    .map(|i| (200 + (i % 150)) as f32 * 0.1f32)
    .collect()
}

/// Generate smooth monotonically increasing f64 time-series data.
/// 生成平滑线性递增 f64 时序数据。
fn generate_ramp_data(count: usize) -> Vec<f64> {
  (0..count).map(|i| 100.0 + i as f64 * 0.05).collect()
}

/// Generate smooth monotonically increasing f32 time-series data.
/// 生成平滑线性递增 f32 时序数据。
fn generate_ramp_data_f32(count: usize) -> Vec<f32> {
  (0..count).map(|i| 100.0f32 + i as f32 * 0.05f32).collect()
}

/// Generate deterministic pseudo-random f64 data with fixed seed.
/// 生成带固定随机种子的确定性伪随机 f64 数据。
fn generate_random_data(count: usize) -> Vec<f64> {
  fastrand::seed(42);
  (0..count)
    .map(|_| {
      let base = fastrand::i32(-1000..1000) as f64;
      let dec = fastrand::u32(0..1000) as f64 * 0.01;
      base + dec
    })
    .collect()
}

/// Macro defining sampled compression, cached compression, and decompression benchmarks.
/// 宏定义冷启动动态参数采样压缩、热状态内核参数复用压缩与解压基准测试三联组。
macro_rules! def_bench {
  ($bench_sampled:ident, $bench_cached:ident, $bench_dec:ident, $ty:ty, $expr:expr, $cap:expr) => {
    #[divan::bench]
    fn $bench_sampled(bencher: Bencher) {
      let data: Vec<$ty> = $expr;
      let mut dst = Vec::with_capacity(max_compressed_size::<$ty>(data.len()));
      bencher.bench_local(|| {
        dst.clear();
        compress_into(&data, &mut dst);
        black_box(&dst);
      });
    }

    #[divan::bench]
    fn $bench_cached(bencher: Bencher) {
      let data: Vec<$ty> = $expr;
      let mut dst = Vec::with_capacity(max_compressed_size::<$ty>(data.len()));
      let mut encoder = Encoder::new();
      encoder.compress_into(&data, &mut dst);
      bencher.bench_local(|| {
        dst.clear();
        encoder.compress_into(&data, &mut dst);
        black_box(&dst);
      });
    }

    #[divan::bench]
    fn $bench_dec(bencher: Bencher) {
      let data: Vec<$ty> = $expr;
      let compressed = compress(&data[..]);
      let mut dst: Vec<$ty> = Vec::with_capacity($cap);
      bencher.bench_local(|| {
        dst.clear();
        // SAFETY: data was compressed successfully by fastalp, decompression is guaranteed to succeed.
        unsafe {
          decompress_into(&compressed, &mut dst).unwrap_unchecked();
        }
        black_box(&dst);
      });
    }
  };
}

// ───────────────────────────────────────────────
// 1. f64 1024 浮点基准测试
// ───────────────────────────────────────────────
def_bench!(
  bench_compress_f64_sensor_sampled_1024,
  bench_compress_f64_sensor_cached_1024,
  bench_decompress_f64_sensor_1024,
  f64,
  generate_sensor_data(BLOCK_SIZE),
  BLOCK_SIZE
);

def_bench!(
  bench_compress_f64_ramp_sampled_1024,
  bench_compress_f64_ramp_cached_1024,
  bench_decompress_f64_ramp_1024,
  f64,
  generate_ramp_data(BLOCK_SIZE),
  BLOCK_SIZE
);

def_bench!(
  bench_compress_f64_random_sampled_1024,
  bench_compress_f64_random_cached_1024,
  bench_decompress_f64_random_1024,
  f64,
  generate_random_data(BLOCK_SIZE),
  BLOCK_SIZE
);

def_bench!(
  bench_compress_f64_identical_sampled_1024,
  bench_compress_f64_identical_cached_1024,
  bench_decompress_f64_identical_1024,
  f64,
  vec![98.6f64; BLOCK_SIZE],
  BLOCK_SIZE
);

// ───────────────────────────────────────────────
// 2. f32 1024 浮点基准测试
// ───────────────────────────────────────────────
def_bench!(
  bench_compress_f32_sensor_sampled_1024,
  bench_compress_f32_sensor_cached_1024,
  bench_decompress_f32_sensor_1024,
  f32,
  generate_sensor_data_f32(BLOCK_SIZE),
  BLOCK_SIZE
);

def_bench!(
  bench_compress_f32_ramp_sampled_1024,
  bench_compress_f32_ramp_cached_1024,
  bench_decompress_f32_ramp_1024,
  f32,
  generate_ramp_data_f32(BLOCK_SIZE),
  BLOCK_SIZE
);

// ───────────────────────────────────────────────
// 3. 大块批量吞吐基准测试 (65536 浮点数)
// ───────────────────────────────────────────────
def_bench!(
  bench_compress_f64_large_batch_sampled,
  bench_compress_f64_large_batch_cached,
  bench_decompress_f64_large_batch,
  f64,
  generate_sensor_data(LARGE_BATCH_SIZE),
  LARGE_BATCH_SIZE
);

def_bench!(
  bench_compress_f32_large_batch_sampled,
  bench_compress_f32_large_batch_cached,
  bench_decompress_f32_large_batch,
  f32,
  generate_sensor_data_f32(LARGE_BATCH_SIZE),
  LARGE_BATCH_SIZE
);
