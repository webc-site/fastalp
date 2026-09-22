use std::{
  fs::{self, read_dir},
  path::{Path, PathBuf},
};

pub const SAMPLES_DIR: &str = "../../ALP/data/samples";

pub fn samples_dir() -> PathBuf {
  if Path::new(SAMPLES_DIR).exists() {
    PathBuf::from(SAMPLES_DIR)
  } else {
    PathBuf::from("../ALP/data/samples")
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

pub fn load_paper_samples() -> Vec<(String, Vec<f64>)> {
  let dir = samples_dir();
  let mut list = Vec::new();
  if let Ok(entries) = read_dir(&dir) {
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
      if p.extension().is_some_and(|e| e == "csv") {
        let name = p
          .file_stem()
          .and_then(|s| s.to_str())
          .unwrap_or("unknown")
          .to_string();
        let data = load_sample(&name);
        if !data.is_empty() {
          list.push((name, data));
        }
      }
    }
  }
  list
}
