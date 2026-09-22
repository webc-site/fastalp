use core::fmt::Debug;

use fastalp::{
  AlpFloat, count, decompress, decompress_into_raw, decompress_into_slice, max_compressed_size,
  read_count,
};

/// 基于二进制位严格断言两浮点数完全一致（区分 +0.0 与 -0.0，且支持 NaN）
#[inline]
pub fn assert_float_eq<F: AlpFloat + Debug>(orig: F, dec: F, idx: usize) {
  if orig.is_nan() {
    assert!(dec.is_nan(), "At index {idx}: expected NaN, got {dec:?}");
  } else {
    assert!(
      orig.is_exact_same(dec),
      "At index {idx}: bit mismatch (orig={orig:?}, dec={dec:?})"
    );
  }
}

/// 严格断言两浮点数切片完全一致
#[inline]
pub fn assert_slice_eq<F: AlpFloat + Debug>(orig: &[F], dec: &[F]) {
  assert_eq!(
    orig.len(),
    dec.len(),
    "Slice length mismatch: expected {}, got {}",
    orig.len(),
    dec.len()
  );
  for (i, (&a, &b)) in orig.iter().zip(dec).enumerate() {
    assert_float_eq(a, b, i);
  }
}

/// 深度验证已压缩数据的合法性：
/// 1. count() 校验
/// 2. read_count() 校验
/// 3. max_compressed_size() 上限校验
/// 4. decompress() 还原校验
/// 5. decompress_into_slice() 零分配还原校验
/// 6. decompress_into_raw() 底层裸指针还原校验
pub fn verify_compressed<F: AlpFloat + Debug>(
  data: &[F],
  compressed: &[u8],
) -> fastalp::Result<()> {
  let cnt = count(compressed)?;
  assert_eq!(cnt, data.len(), "count() mismatch");

  let read_cnt = read_count(compressed)?;
  assert_eq!(read_cnt, data.len(), "read_count() mismatch");

  let max_sz = max_compressed_size::<F>(data.len());
  assert!(
    compressed.len() <= max_sz,
    "compressed len {} exceeds max_compressed_size {}",
    compressed.len(),
    max_sz
  );

  let decompressed: Vec<F> = decompress(compressed)?;
  assert_slice_eq(data, &decompressed);

  let mut slice_buf = vec![F::default(); data.len()];
  let written = decompress_into_slice(compressed, &mut slice_buf)?;
  assert_eq!(
    written,
    data.len(),
    "decompress_into_slice written length mismatch"
  );
  assert_slice_eq(data, &slice_buf);

  if !data.is_empty() {
    let mut raw_buf = vec![F::default(); data.len()];
    let raw_written = unsafe { decompress_into_raw(compressed, raw_buf.as_mut_ptr(), data.len())? };
    assert_eq!(
      raw_written,
      data.len(),
      "decompress_into_raw written length mismatch"
    );
    assert_slice_eq(data, &raw_buf);
  }

  Ok(())
}

/// 通用常规压缩全流程往返验证
pub fn verify_roundtrip<F: AlpFloat + Debug>(data: &[F]) -> fastalp::Result<Vec<u8>> {
  let compressed = fastalp::compress(data);
  verify_compressed(data, &compressed)?;
  Ok(compressed)
}

/// 通用 Delta 一阶差分压缩全流程往返验证
pub fn verify_roundtrip_delta<F: AlpFloat + Debug>(data: &[F]) -> fastalp::Result<Vec<u8>> {
  let compressed = fastalp::compress_delta(data);
  verify_compressed(data, &compressed)?;
  Ok(compressed)
}
