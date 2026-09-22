use std::{
  env, fs,
  hint::black_box,
  io::{BufRead, BufReader},
  path::{Path, PathBuf},
  process::exit,
  time::{Duration, Instant},
};

use fastalp::{ChunkType, Encoder, compress_into, decompress_into, max_compressed_size};
use serde::Serialize;

#[inline]
fn round2(v: f64) -> f64 {
  (v * 100.0).round() / 100.0
}

#[inline]
fn round4(v: f64) -> f64 {
  (v * 10000.0).round() / 10000.0
}

#[derive(Debug, Serialize)]
struct DatasetItem {
  name: String,
  raw_bytes: usize,
  compressed_bytes: usize,
  ratio: f64,
  bits_per_val: f64,
  enc_gb_s: f64,
  enc_sampled_gb_s: f64,
  enc_kernel_gb_s: f64,
  dec_gb_s: f64,
}

#[derive(Debug, Serialize)]
struct Paper31Section {
  total_raw_bytes: usize,
  total_compressed_bytes: usize,
  ratio: f64,
  bits_per_val: f64,
  avg_enc_gb_s: f64,
  avg_enc_sampled_gb_s: f64,
  avg_enc_kernel_gb_s: f64,
  avg_dec_gb_s: f64,
  datasets: Vec<DatasetItem>,
}

#[derive(Debug, Serialize)]
struct MicroBenchmarkResult {
  raw_bytes: usize,
  compressed_bytes: usize,
  ratio: f64,
  bits_per_val: f64,
  enc_gb_s: f64,
  enc_sampled_gb_s: f64,
  enc_kernel_gb_s: f64,
  dec_gb_s: f64,
}

#[derive(Debug, Serialize)]
struct MicroBenchmarksSection {
  sensor_1024: MicroBenchmarkResult,
  ramp_1024: MicroBenchmarkResult,
  constant_1024: MicroBenchmarkResult,
  random_1024: MicroBenchmarkResult,
}

#[derive(Debug, Serialize)]
struct CodecJsonReport {
  algorithm: &'static str,
  display_name: &'static str,
  category: &'static str,
  paper_31: Paper31Section,
  micro_benchmarks: MicroBenchmarksSection,
}

fn load_csv(path: &Path) -> Vec<f64> {
  let Ok(f) = fs::File::open(path) else {
    return Vec::new();
  };
  BufReader::new(f)
    .lines()
    .map_while(Result::ok)
    .filter_map(|line| {
      let s = line.trim();
      if s.is_empty() || s.starts_with('#') || s.starts_with("column") {
        None
      } else {
        s.parse::<f64>().ok()
      }
    })
    .collect()
}

fn bench_micro(data: &[f64]) -> MicroBenchmarkResult {
  let iters = 1000;
  let mut comp_buf = Vec::with_capacity(max_compressed_size::<f64>(data.len()));
  let mut dec_buf: Vec<f64> = Vec::with_capacity(data.len());
  let mut encoder = Encoder::new();

  // Warmup
  for _ in 0..50 {
    comp_buf.clear();
    encoder.compress_into(data, &mut comp_buf);
    dec_buf.clear();
    let _ = decompress_into(&comp_buf, &mut dec_buf);
  }

  // Sampled
  let t0 = Instant::now();
  for _ in 0..iters {
    comp_buf.clear();
    compress_into(data, &mut comp_buf);
    black_box(&comp_buf);
  }
  let enc_sampled_dt = t0.elapsed().as_secs_f64() / iters as f64;

  // Cached kernel
  encoder.reset();
  comp_buf.clear();
  encoder.compress_into(data, &mut comp_buf);
  let t1 = Instant::now();
  for _ in 0..iters {
    comp_buf.clear();
    encoder.compress_into(data, &mut comp_buf);
    black_box(&comp_buf);
  }
  let enc_kernel_dt = t1.elapsed().as_secs_f64() / iters as f64;

  // Decompress
  let t2 = Instant::now();
  for _ in 0..iters {
    dec_buf.clear();
    unsafe {
      decompress_into(&comp_buf, &mut dec_buf).unwrap_unchecked();
    }
    black_box(&dec_buf);
  }
  let dec_dt = t2.elapsed().as_secs_f64() / iters as f64;

  let raw_bytes = data.len() * 8;
  let comp_bytes = comp_buf.len();
  let ratio = raw_bytes as f64 / comp_bytes as f64;
  let bpv = (comp_bytes * 8) as f64 / data.len() as f64;
  let enc_sampled_gb_s = (raw_bytes as f64 / enc_sampled_dt) / 1e9;
  let enc_kernel_gb_s = (raw_bytes as f64 / enc_kernel_dt) / 1e9;
  let dec_gb_s = (raw_bytes as f64 / dec_dt) / 1e9;

  MicroBenchmarkResult {
    raw_bytes,
    compressed_bytes: comp_bytes,
    ratio: round4(ratio),
    bits_per_val: round2(bpv),
    enc_gb_s: round2(enc_sampled_gb_s),
    enc_sampled_gb_s: round2(enc_sampled_gb_s),
    enc_kernel_gb_s: round2(enc_kernel_gb_s),
    dec_gb_s: round2(dec_gb_s),
  }
}

fn main() {
  let alp_dir = env::var("ALP_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ALP"));

  let candidates = [
    alp_dir.join("data/samples"),
    PathBuf::from("../ALP/data/samples"),
    PathBuf::from("../../ALP/data/samples"),
    PathBuf::from("ALP/data/samples"),
  ];

  let Some(samples_dir) = candidates.into_iter().find(|p| p.exists()) else {
    eprintln!(
      "Error: samples directory not found. Please set ALP_DIR or place ALP repository beside workspace."
    );
    exit(1);
  };

  let Ok(dir_entries) = fs::read_dir(&samples_dir) else {
    eprintln!("Error: failed to read samples directory: {:?}", samples_dir);
    exit(1);
  };

  let mut entries: Vec<_> = dir_entries
    .filter_map(|r| r.ok())
    .filter(|e| e.path().extension().is_some_and(|ext| ext == "csv"))
    .collect();
  entries.sort_by_key(|a| a.file_name());

  if entries.is_empty() {
    eprintln!(
      "Error: Found samples dir {:?} but 0 valid CSV datasets.",
      samples_dir
    );
    exit(1);
  }

  println!(
    "Running fastalp benchmark across all {} datasets (fair zero-alloc pipeline)...",
    entries.len()
  );

  let mut dataset_items = Vec::new();
  let mut total_raw_bytes = 0;
  let mut total_comp_bytes = 0;
  let mut sum_enc = 0.0;
  let mut sum_enc_kern = 0.0;
  let mut sum_dec = 0.0;

  let mut comp_buf = Vec::with_capacity(65536);
  let mut dec_buf = Vec::with_capacity(1024);
  let mut encoder = Encoder::<f64>::with_capacity(1024);

  for entry in &entries {
    let path = entry.path();
    let name = path.file_stem().unwrap().to_string_lossy().to_string();
    let data = load_csv(&path);
    if data.is_empty() {
      continue;
    }

    let raw_bytes = data.len() * 8;
    total_raw_bytes += raw_bytes;

    // Warmup
    encoder.reset();
    for _ in 0..100 {
      comp_buf.clear();
      compress_into(&data, &mut comp_buf);
      dec_buf.clear();
      let _ = decompress_into::<f64>(&comp_buf, &mut dec_buf);
    }

    // Measure End-to-End Compression with Dynamic Sampling (min of 3 rounds of 1000 iters)
    let comp_iters = 1000;
    let mut best_enc_dur = Duration::MAX;
    for _ in 0..3 {
      let start_enc = Instant::now();
      for _ in 0..comp_iters {
        comp_buf.clear();
        compress_into(&data, &mut comp_buf);
        black_box(&comp_buf);
      }
      best_enc_dur = best_enc_dur.min(start_enc.elapsed());
    }
    let enc_gb_s = (raw_bytes as f64 * comp_iters as f64) / (best_enc_dur.as_secs_f64() * 1e9);

    // Measure Pure Encoding Kernel Without Sampling (Reusing Cached Parameters, min of 3 rounds of 1000 iters)
    encoder.reset();
    comp_buf.clear();
    encoder.compress_into(&data, &mut comp_buf); // First pass establishes cached parameters
    let mut best_enc_kern_dur = Duration::MAX;
    for _ in 0..3 {
      let start_enc_kern = Instant::now();
      for _ in 0..comp_iters {
        comp_buf.clear();
        encoder.compress_into(&data, &mut comp_buf);
        black_box(&comp_buf);
      }
      best_enc_kern_dur = best_enc_kern_dur.min(start_enc_kern.elapsed());
    }
    let enc_kernel_gb_s =
      (raw_bytes as f64 * comp_iters as f64) / (best_enc_kern_dur.as_secs_f64() * 1e9);

    // Measure Decompression (min of 3 rounds of 1000 iters - exact match with C++ ALP benchmark)
    let dec_iters = 1000;
    let mut best_dec_dur = Duration::MAX;
    for _ in 0..3 {
      let start_dec = Instant::now();
      for _ in 0..dec_iters {
        dec_buf.clear();
        unsafe {
          decompress_into::<f64>(&comp_buf, &mut dec_buf).unwrap_unchecked();
        }
        black_box(&dec_buf);
      }
      best_dec_dur = best_dec_dur.min(start_dec.elapsed());
    }
    let dec_gb_s = (raw_bytes as f64 * dec_iters as f64) / (best_dec_dur.as_secs_f64() * 1e9);

    let comp_bytes = comp_buf.len();
    total_comp_bytes += comp_bytes;
    let ratio = raw_bytes as f64 / comp_bytes as f64;
    let bits_per_val = (comp_bytes * 8) as f64 / data.len() as f64;

    sum_enc += enc_gb_s;
    sum_enc_kern += enc_kernel_gb_s;
    sum_dec += dec_gb_s;

    let header = fastalp::read_header(&comp_buf).unwrap();
    let chunk_type = header.chunk_type().unwrap_or(ChunkType::F64);
    let bw = header.params.map(|p| p.bit_width).unwrap_or(0);
    println!(
      "{:<24} | {:<12?} (bw={:>2}, rep={}) | Ratio: {:>6.2}x | Enc(samp): {:>5.2} GB/s | Enc(kern): {:>5.2} GB/s | Dec: {:>5.2} GB/s",
      name, chunk_type, bw, header.has_repeat, ratio, enc_gb_s, enc_kernel_gb_s, dec_gb_s
    );

    dataset_items.push(DatasetItem {
      name,
      raw_bytes,
      compressed_bytes: comp_bytes,
      ratio: round4(ratio),
      bits_per_val: round2(bits_per_val),
      enc_gb_s: round2(enc_gb_s),
      enc_sampled_gb_s: round2(enc_gb_s),
      enc_kernel_gb_s: round2(enc_kernel_gb_s),
      dec_gb_s: round2(dec_gb_s),
    });
  }

  let n = entries.len() as f64;
  let avg_enc = sum_enc / n;
  let avg_enc_kern = sum_enc_kern / n;
  let avg_dec = sum_dec / n;
  let total_ratio = total_raw_bytes as f64 / total_comp_bytes as f64;
  let total_bpv = (total_comp_bytes * 8) as f64 / (total_raw_bytes as f64 / 8.0);

  // Measure microbenchmarks
  let micro_len = 1024;
  let sensor_data: Vec<f64> = (0..micro_len)
    .map(|i| (200 + (i % 150)) as f64 * 0.1)
    .collect();
  let ramp_data: Vec<f64> = (0..micro_len).map(|i| 100.0 + i as f64 * 0.05).collect();
  let constant_data: Vec<f64> = vec![98.6; micro_len];
  let random_noise: Vec<f64> = {
    fastrand::seed(42);
    (0..micro_len)
      .map(|_| f64::from_bits(fastrand::u64(..)))
      .collect()
  };

  let report = CodecJsonReport {
    algorithm: "fastalp",
    display_name: "fastalp (Rust)",
    category: "specialized_float",
    paper_31: Paper31Section {
      total_raw_bytes,
      total_compressed_bytes: total_comp_bytes,
      ratio: round4(total_ratio),
      bits_per_val: round2(total_bpv),
      avg_enc_gb_s: round2(avg_enc),
      avg_enc_sampled_gb_s: round2(avg_enc),
      avg_enc_kernel_gb_s: round2(avg_enc_kern),
      avg_dec_gb_s: round2(avg_dec),
      datasets: dataset_items,
    },
    micro_benchmarks: MicroBenchmarksSection {
      sensor_1024: bench_micro(&sensor_data),
      ramp_1024: bench_micro(&ramp_data),
      constant_1024: bench_micro(&constant_data),
      random_1024: bench_micro(&random_noise),
    },
  };

  let json_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/json/fastalp.json");
  if let Some(parent) = json_path.parent() {
    let _ = fs::create_dir_all(parent);
  }
  let full_json =
    serde_json::to_string_pretty(&report).expect("Failed to serialize fastalp report to JSON");
  fs::write(&json_path, full_json).expect("Failed to write fastalp.json");
  println!("\nSuccessfully updated {}", json_path.display());
}
