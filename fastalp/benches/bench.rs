//! Real-world time-series microbenchmark suite for fastalp compression and decompression algorithms.
//! fastalp 浮点压缩与解压真实时序数据集性能回归基准测试套件。

use std::str::FromStr;

use divan::{Bencher, black_box};
use fastalp::{Encoder, compress, decompress_into, max_compressed_size};

/// Global high-performance memory allocator (mimalloc).
/// 全局高性能内存分配器（mimalloc）。
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Divan benchmark runner main entry point.
/// Divan 基准测试运行器主入口函数。
fn main() {
  divan::main();
}

// ───────────────────────────────────────────────
// 内置真实工业公开测试集 (来自 ALP 标准评测体系)
// ───────────────────────────────────────────────
const RAW_CSV_CITY_TEMPERATURE: &str = include_str!("real_data/city_temperature_f.csv");
const RAW_CSV_STOCKS_DE: &str = include_str!("real_data/stocks_de.csv");
const RAW_CSV_NEON_AIR_PRESSURE: &str = include_str!("real_data/neon_air_pressure.csv");
const RAW_CSV_FOOD_PRICES: &str = include_str!("real_data/food_prices.csv");
const RAW_CSV_BITCOIN: &str = include_str!("real_data/bitcoin_f.csv");
const RAW_CSV_AIR_SENSOR: &str = include_str!("real_data/air_sensor_f.csv");

#[inline]
fn parse_csv<T: FromStr>(raw: &str) -> Vec<T> {
  raw
    .lines()
    .map(str::trim)
    .filter(|s| !s.is_empty() && !s.starts_with('#') && !s.starts_with("column"))
    .filter_map(|s| s.parse::<T>().ok())
    .collect()
}

#[inline]
fn tile<T: Copy>(base: &[T], target_len: usize) -> Vec<T> {
  base.iter().copied().cycle().take(target_len).collect()
}

/// Macro defining sampled compression, cached compression, and decompression benchmarks.
/// 宏定义冷启动动态参数重采样压缩、热状态内核参数复用压缩与解压基准测试三联组。
macro_rules! def_bench {
  ($bench_sampled:ident, $bench_cached:ident, $bench_decompress:ident, $ty:ty, $expr:expr) => {
    #[divan::bench]
    fn $bench_sampled(bencher: Bencher) {
      let data: Vec<$ty> = $expr;
      let mut dst = Vec::with_capacity(max_compressed_size::<$ty>(data.len()));
      let mut encoder = Encoder::with_capacity(data.len());
      bencher.bench_local(|| {
        dst.clear();
        encoder.reset();
        encoder.compress_into(&data, &mut dst);
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
    fn $bench_decompress(bencher: Bencher) {
      let data: Vec<$ty> = $expr;
      let compressed = compress(&data[..]);
      let mut dst: Vec<$ty> = Vec::with_capacity(data.len());
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
// 1. 真实时序气象温度数据集 (city_temperature_f, 标准十进制 FOR 模式)
// ───────────────────────────────────────────────
def_bench!(
  bench_city_temperature_f64_sampled,
  bench_city_temperature_f64_cached,
  bench_city_temperature_f64_decompress,
  f64,
  parse_csv(RAW_CSV_CITY_TEMPERATURE)
);

// ───────────────────────────────────────────────
// 2. 真实金融股票价格时序数据集 (stocks_de, 时序一阶差分 Delta 模式)
// ───────────────────────────────────────────────
def_bench!(
  bench_stocks_de_f64_sampled,
  bench_stocks_de_f64_cached,
  bench_stocks_de_f64_decompress,
  f64,
  parse_csv(RAW_CSV_STOCKS_DE)
);

// ───────────────────────────────────────────────
// 3. 真实环境传感器气压时序数据集 (neon_air_pressure, 平滑缓变 Delta 模式)
// ───────────────────────────────────────────────
def_bench!(
  bench_neon_air_pressure_f64_sampled,
  bench_neon_air_pressure_f64_cached,
  bench_neon_air_pressure_f64_decompress,
  f64,
  parse_csv(RAW_CSV_NEON_AIR_PRESSURE)
);

// ───────────────────────────────────────────────
// 4. 真实食品物价指数时序数据集 (food_prices, 时序连续行程 Repeat 模式)
// ───────────────────────────────────────────────
def_bench!(
  bench_food_prices_f64_sampled,
  bench_food_prices_f64_cached,
  bench_food_prices_f64_decompress,
  f64,
  parse_csv(RAW_CSV_FOOD_PRICES)
);

// ───────────────────────────────────────────────
// 5. 真实加密货币时序数据集 (bitcoin_f, 包含极端插针尖峰 Outlier 剪枝模式)
// ───────────────────────────────────────────────
def_bench!(
  bench_bitcoin_f64_sampled,
  bench_bitcoin_f64_cached,
  bench_bitcoin_f64_decompress,
  f64,
  parse_csv(RAW_CSV_BITCOIN)
);

// ───────────────────────────────────────────────
// 6. 真实空气质量传感器数据集 (air_sensor_f, 真实双精度二进制浮点 ALP-RD 解耦模式)
// ───────────────────────────────────────────────
def_bench!(
  bench_air_sensor_f64_sampled,
  bench_air_sensor_f64_cached,
  bench_air_sensor_f64_decompress,
  f64,
  parse_csv(RAW_CSV_AIR_SENSOR)
);

// ───────────────────────────────────────────────
// 7. 单精度 f32 真实时序数据集 (city_temperature & stocks_de)
// ───────────────────────────────────────────────
def_bench!(
  bench_city_temperature_f32_sampled,
  bench_city_temperature_f32_cached,
  bench_city_temperature_f32_decompress,
  f32,
  parse_csv(RAW_CSV_CITY_TEMPERATURE)
);

def_bench!(
  bench_stocks_de_f32_sampled,
  bench_stocks_de_f32_cached,
  bench_stocks_de_f32_decompress,
  f32,
  parse_csv(RAW_CSV_STOCKS_DE)
);

// ───────────────────────────────────────────────
// 8. 真实时序大块批量吞吐基准测试 (65536 浮点数，512 KB 块大小)
// ───────────────────────────────────────────────
def_bench!(
  bench_large_batch_f64_sampled,
  bench_large_batch_f64_cached,
  bench_large_batch_f64_decompress,
  f64,
  tile(&parse_csv::<f64>(RAW_CSV_CITY_TEMPERATURE), 65536)
);

def_bench!(
  bench_large_batch_f32_sampled,
  bench_large_batch_f32_cached,
  bench_large_batch_f32_decompress,
  f32,
  tile(&parse_csv::<f32>(RAW_CSV_CITY_TEMPERATURE), 65536)
);
