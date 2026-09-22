#[path = "common/mod.rs"]
mod common;

use std::time::Instant;

use fastalp::{Encoder, decompress_into};

fn main() {
  let data = common::load_sample("neon_air_pressure");
  if data.is_empty() {
    eprintln!(
      "Error: neon_air_pressure dataset not found at {:?}",
      common::sample_path("neon_air_pressure")
    );
    return;
  }

  let mut enc = Encoder::<f64>::with_capacity(1024);
  let mut comp_buf = Vec::new();
  enc.compress_into(&data, &mut comp_buf);

  println!("Comp buf len: {}", comp_buf.len());
  let mut dec_buf = Vec::with_capacity(1024);

  // Measure decompression
  let iters = 10000;
  let start = Instant::now();
  for _ in 0..iters {
    dec_buf.clear();
    unsafe {
      decompress_into::<f64>(&comp_buf, &mut dec_buf).unwrap_unchecked();
    }
  }
  let dur = start.elapsed();
  let gb_s = (data.len() * 8 * iters) as f64 / (dur.as_secs_f64() * 1e9);
  println!("Decomp speed: {:.2} GB/s", gb_s);
}
