#[path = "common/mod.rs"]
mod common;

use fastalp::Encoder;

fn main() {
  let test_cases = [
    "gov26",
    "usgs_river_discharge",
    "isd_air_temperature",
    "usgs_gage_height",
    "noaa_water_level",
  ];
  let mut enc = Encoder::<f64>::with_capacity(1024);
  let mut dst = Vec::new();
  for name in test_cases {
    let data = common::load_sample(name);
    if data.is_empty() {
      eprintln!("Warning: sample {name} not found");
      continue;
    }
    enc.reset();
    dst.clear();
    enc.compress_into(&data, &mut dst);
    let hdr = fastalp::read_header(&dst).unwrap();
    println!(
      "{name}: scheme={:?}, type_byte={}, len={}",
      enc.cached_scheme,
      hdr.type_byte,
      dst.len()
    );
  }
}
