use std::{
  env::var,
  fs,
  path::{Path, PathBuf},
};

pub const SAMPLES_DIR: &str = "../../ALP/data/samples";

pub fn samples_dir() -> PathBuf {
  if let Ok(dir) = var("ALP_DATASET_DIR") {
    let p = PathBuf::from(dir);
    if p.exists() {
      return p;
    }
  }
  let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SAMPLES_DIR);
  if manifest_path.exists() {
    manifest_path
  } else {
    PathBuf::from(SAMPLES_DIR)
  }
}

pub fn sample_path(name: &str) -> PathBuf {
  samples_dir().join(format!("{name}.csv"))
}

pub fn load_csv(path: impl AsRef<Path>) -> Vec<f64> {
  let Ok(content) = fs::read_to_string(path) else {
    return Vec::new();
  };
  content
    .lines()
    .filter_map(|l| {
      let s = l.trim();
      if s.is_empty() || s.starts_with('#') || s.starts_with("column") {
        None
      } else {
        s.parse::<f64>().ok()
      }
    })
    .collect()
}

pub fn load_sample(name: &str) -> Vec<f64> {
  load_csv(sample_path(name))
}
