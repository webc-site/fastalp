mod common;

use std::{
  env::var,
  fs,
  path::{Path, PathBuf},
  str::FromStr,
};

use aok::Result;
use common::{verify_roundtrip, verify_roundtrip_delta};
use fastalp::AlpFloat;

#[ctor::ctor(unsafe)]
fn _log_init() {
  log_init::init();
}

fn get_alp_data_dir() -> Option<PathBuf> {
  if let Ok(p) = var("ALP_DATASET_DIR") {
    let pb = PathBuf::from(p);
    if pb.exists() {
      return Some(pb);
    }
  }
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let candidates = [
    manifest_dir.join("../ALP/data"),
    manifest_dir.join("../../ALP/data"),
    PathBuf::from("/Users/z/git/db/ALP/data"),
  ];
  candidates.into_iter().find(|p| p.exists())
}

fn load_csv<F: AlpFloat + FromStr>(path: &Path) -> Vec<F> {
  let content = fs::read_to_string(path).expect("Failed to read CSV");
  content
    .lines()
    .filter_map(|line| line.trim().parse::<F>().ok())
    .collect()
}

#[test]
fn test_alp_paper_datasets_roundtrip_and_ratio() -> Result<()> {
  let Some(data_dir) = get_alp_data_dir() else {
    println!("ALP dataset directory not found, skipping.");
    return Ok(());
  };

  let samples_dir = data_dir.join("samples");
  if !samples_dir.exists() {
    return Ok(());
  }

  let mut entries: Vec<_> = fs::read_dir(&samples_dir)?
    .filter_map(|r| r.ok())
    .filter(|e| e.path().extension().is_some_and(|ext| ext == "csv"))
    .collect();
  entries.sort_by_key(|a| a.file_name());

  println!("\n{:=<88}", "");
  println!(
    "{:<28} | {:>8} | {:>10} | {:>12} | {:>10}",
    "Dataset Name", "Count", "Raw (B)", "Comp (B)", "Ratio"
  );
  println!("{:-<88}", "");

  let mut total_raw = 0usize;
  let mut total_comp = 0usize;

  for entry in entries {
    let path = entry.path();
    let file_stem = path
      .file_stem()
      .and_then(|s| s.to_str())
      .unwrap_or("unknown");
    let data = load_csv::<f64>(&path);
    if data.is_empty() {
      continue;
    }

    let raw_bytes = data.len() * 8;
    // 使用 verify_roundtrip 进行多重深度校验（compress, count, max_size, decompress, decompress_into_slice, decompress_into_raw）
    let compressed = verify_roundtrip(&data)?;

    let comp_bytes = compressed.len();
    let ratio = raw_bytes as f64 / comp_bytes as f64;
    total_raw += raw_bytes;
    total_comp += comp_bytes;

    println!(
      "{:<28} | {:>8} | {:>10} | {:>12} | {:>9.2}x",
      file_stem,
      data.len(),
      raw_bytes,
      comp_bytes,
      ratio
    );
  }

  let total_ratio = total_raw as f64 / total_comp as f64;
  println!("{:-<88}", "");
  println!(
    "{:<28} | {:>8} | {:>10} | {:>12} | {:>9.2}x",
    "TOTAL / AVERAGE", "-", total_raw, total_comp, total_ratio
  );
  println!("{:=<88}\n", "");

  Ok(())
}

#[test]
fn test_alp_edge_case_and_float_datasets() -> Result<()> {
  let Some(data_dir) = get_alp_data_dir() else {
    return Ok(());
  };

  // 1. Edge cases
  let edge_dir = data_dir.join("edge_case");
  if edge_dir.exists() {
    let edge_csv = edge_dir.join("edge_case.csv");
    if edge_csv.exists() {
      let data = load_csv::<f64>(&edge_csv);
      if !data.is_empty() {
        verify_roundtrip(&data)?;
      }
    }

    // avx512dq.csv 在 ALP 原项目中对标 float edge case (192 异常)
    let avx_csv = edge_dir.join("avx512dq.csv");
    if avx_csv.exists() {
      let data = load_csv::<f32>(&avx_csv);
      if !data.is_empty() {
        verify_roundtrip(&data)?;
      }
    }
  }

  // 2. Float datasets (float/test_0.csv .. test_3.csv)
  let float_dir = data_dir.join("float");
  if float_dir.exists() {
    for entry in fs::read_dir(float_dir)?.filter_map(|r| r.ok()) {
      if entry.path().extension().is_some_and(|e| e == "csv") {
        let data = load_csv::<f32>(&entry.path());
        if !data.is_empty() {
          verify_roundtrip(&data)?;
        }
      }
    }
  }

  // 3. Double datasets (double/test_0.csv)
  let double_dir = data_dir.join("double");
  if double_dir.exists() {
    for entry in fs::read_dir(double_dir)?.filter_map(|r| r.ok()) {
      if entry.path().extension().is_some_and(|e| e == "csv") {
        let data = load_csv::<f64>(&entry.path());
        if !data.is_empty() {
          verify_roundtrip(&data)?;
        }
      }
    }
  }

  Ok(())
}

#[test]
fn test_alp_issue_and_generated_datasets() -> Result<()> {
  let Some(data_dir) = get_alp_data_dir() else {
    return Ok(());
  };

  // 1. Issue datasets (ShapesAll_TEST.csv, issue_24_1024_values.csv)
  let issue_dir = data_dir.join("issue");
  if issue_dir.exists() {
    let issue_small = issue_dir.join("issue_24_1024_values.csv");
    if issue_small.exists() {
      let data = load_csv::<f64>(&issue_small);
      if !data.is_empty() {
        verify_roundtrip(&data)?;
        verify_roundtrip_delta(&data)?;
      }
    }

    let shapes_csv = issue_dir.join("ShapesAll_TEST.csv");
    if shapes_csv.exists() {
      let data = load_csv::<f64>(&shapes_csv);
      if !data.is_empty() {
        verify_roundtrip(&data)?;
      }
    }
  }

  // 2. Generated bit-width benchmark columns (generated_doubles_bw*.csv)
  let gen_dir = data_dir.join("generated");
  if gen_dir.exists() {
    // 覆盖不同代表性位宽 (bw0, bw1, bw5, bw16, bw32, bw48, bw64)
    for bw in [0, 1, 5, 16, 32, 48, 64] {
      let file = gen_dir.join(format!("generated_doubles_bw{bw}.csv"));
      if file.exists() {
        let data = load_csv::<f64>(&file);
        if !data.is_empty() {
          verify_roundtrip(&data)?;
        }
      }
    }
  }

  Ok(())
}
