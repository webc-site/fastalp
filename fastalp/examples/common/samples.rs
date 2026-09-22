use std::fs::read_dir;

#[path = "mod.rs"]
pub mod base;
pub use base::*;

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
