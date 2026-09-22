#[path = "common/mod.rs"]
mod common;

use fastalp::Encoder;

fn main() {
  let test_cases = [
    "gov26",
    "scene_ramp",
    "scene_sensor",
    "scene_geo",
    "scene_steady",
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
    println!("{name}: scheme={:?}", enc.cached_scheme);
  }
}
