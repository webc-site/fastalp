#[path = "common/mod.rs"]
mod common;

use std::time::Instant;

use fastalp::Encoder;

fn main() {
  let test_cases = [
    "basel_wind_f",
    "bird_migration_f",
    "bitcoin_f",
    "gov40",
    "poi_lat",
    "air_sensor_f",
    "neon_air_pressure",
    "neon_wind_dir",
  ];

  let mut comp_buf = Vec::with_capacity(65536);
  let mut encoder = Encoder::<f64>::with_capacity(1024);

  for name in test_cases {
    let data = common::load_sample(name);
    if data.is_empty() {
      eprintln!("Warning: sample {name} not found");
      continue;
    }
    let raw_bytes = data.len() * 8;

    encoder.reset();
    comp_buf.clear();
    encoder.compress_into(&data, &mut comp_buf);

    let iters = 10000;
    let start = Instant::now();
    for _ in 0..iters {
      comp_buf.clear();
      encoder.compress_into(&data, &mut comp_buf);
    }
    let dur = start.elapsed();
    let ns_per_iter = dur.as_nanos() as f64 / iters as f64;
    let gb_s = (raw_bytes as f64 * iters as f64) / (dur.as_secs_f64() * 1e9);

    println!(
      "{name:<20} | {ns_per_iter:>7.1} ns/iter | {gb_s:>5.2} GB/s | scheme: {:?} | target_bw: {:?}",
      encoder.cached_scheme, encoder.cached_target_bw
    );
    fastalp::profile_compress_breakdown(&data);
  }
}
