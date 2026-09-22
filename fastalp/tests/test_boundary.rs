mod common;

use common::{verify_roundtrip, verify_roundtrip_delta};
use fastalp::{
  CHUNK_SIZE_1024, Error, Result, count, decompress, decompress_into_slice,
  header::{
    FLAG_REPEAT, LEN_TAG_1024, LEN_TAG_U8, LEN_TAG_U16, LEN_TAG_U32, TYPE_F32_RAW, TYPE_F32_RD,
    TYPE_F64_RAW, TYPE_F64_RD, read_header,
  },
};

#[ctor::ctor(unsafe)]
fn _log_init() {
  log_init::init();
}

const EDGE_LENGTHS: [usize; 22] = [
  0, 1, 2, 3, 4, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256, 1023, 1024, 1025, 2048, 2049,
];

#[test]
fn test_boundary_slice_lengths_f64() -> Result<()> {
  for &len in &EDGE_LENGTHS {
    let data: Vec<f64> = (0..len).map(|i| 100.0 + (i as f64) * 0.125).collect();
    verify_roundtrip(&data)?;
    verify_roundtrip_delta(&data)?;
  }
  Ok(())
}

#[test]
fn test_boundary_slice_lengths_f32() -> Result<()> {
  for &len in &EDGE_LENGTHS {
    let data: Vec<f32> = (0..len).map(|i| 50.0f32 + (i as f32) * 0.25f32).collect();
    verify_roundtrip(&data)?;
    verify_roundtrip_delta(&data)?;
  }
  Ok(())
}

#[test]
fn test_boundary_length_tag_transitions() -> Result<()> {
  // 测试长度分档临界值以及对应 Header 中的 len_tag
  // LEN_TAG_U8: <= 255
  let d_255: Vec<f64> = (0..255).map(|i| (i as f64) * 0.5).collect();
  let c_255 = verify_roundtrip(&d_255)?;
  let h_255 = read_header(&c_255)?;
  assert_eq!(h_255.len_tag, LEN_TAG_U8);
  assert_eq!(h_255.count, 255);

  // LEN_TAG_U16: 256
  let d_256: Vec<f64> = (0..256).map(|i| (i as f64) * 0.5).collect();
  let c_256 = verify_roundtrip(&d_256)?;
  let h_256 = read_header(&c_256)?;
  assert_eq!(h_256.len_tag, LEN_TAG_U16);
  assert_eq!(h_256.count, 256);

  // LEN_TAG_1024: 刚好 1024
  let d_1024: Vec<f64> = (0..CHUNK_SIZE_1024).map(|i| (i as f64) * 0.5).collect();
  let c_1024 = verify_roundtrip(&d_1024)?;
  let h_1024 = read_header(&c_1024)?;
  assert_eq!(h_1024.len_tag, LEN_TAG_1024);
  assert_eq!(h_1024.count, 1024);

  // LEN_TAG_U16: 65535
  let d_65535: Vec<f32> = (0..65535).map(|i| (i as f32) * 0.25f32).collect();
  let c_65535 = verify_roundtrip(&d_65535)?;
  let h_65535 = read_header(&c_65535)?;
  assert_eq!(h_65535.len_tag, LEN_TAG_U16);
  assert_eq!(h_65535.count, 65535);

  // LEN_TAG_U32: 65536
  let d_65536: Vec<f32> = (0..65536).map(|i| (i as f32) * 0.25f32).collect();
  let c_65536 = verify_roundtrip(&d_65536)?;
  let h_65536 = read_header(&c_65536)?;
  assert_eq!(h_65536.len_tag, LEN_TAG_U32);
  assert_eq!(h_65536.count, 65536);

  Ok(())
}

#[test]
fn test_boundary_ieee754_special_values_f64() -> Result<()> {
  // 1. 全 0.0 与 全 -0.0
  let all_zero = vec![0.0f64; 128];
  verify_roundtrip(&all_zero)?;
  verify_roundtrip_delta(&all_zero)?;

  let all_neg_zero = vec![-0.0f64; 128];
  verify_roundtrip(&all_neg_zero)?;
  verify_roundtrip_delta(&all_neg_zero)?;

  // 2. 正零与负零交替（严格验证符号位保留）
  let alt_zeros: Vec<f64> = (0..100)
    .map(|i| if i % 2 == 0 { 0.0f64 } else { -0.0f64 })
    .collect();
  verify_roundtrip(&alt_zeros)?;
  verify_roundtrip_delta(&alt_zeros)?;

  // 3. 全 NaN 与 全 Infinity
  let all_nan = vec![f64::NAN; 64];
  verify_roundtrip(&all_nan)?;
  verify_roundtrip_delta(&all_nan)?;

  let all_inf = vec![f64::INFINITY; 64];
  verify_roundtrip(&all_inf)?;
  verify_roundtrip_delta(&all_inf)?;

  let all_neg_inf = vec![f64::NEG_INFINITY; 64];
  verify_roundtrip(&all_neg_inf)?;
  verify_roundtrip_delta(&all_neg_inf)?;

  // 4. 极端数值：次正规数、最小正数、最大有限值、最小有限值
  let extremes = vec![
    f64::MIN_POSITIVE,
    -f64::MIN_POSITIVE,
    f64::MAX,
    f64::MIN,
    f64::EPSILON,
    1e-300,
    -1e-300,
    1e-315,            // 次正规数 subnormal
    f64::from_bits(1), // 最小非零位
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    0.0,
    -0.0,
  ];
  verify_roundtrip(&extremes)?;
  verify_roundtrip_delta(&extremes)?;

  Ok(())
}

#[test]
fn test_boundary_ieee754_special_values_f32() -> Result<()> {
  let extremes_f32 = vec![
    f32::MIN_POSITIVE,
    -f32::MIN_POSITIVE,
    f32::MAX,
    f32::MIN,
    f32::EPSILON,
    1e-40f32,
    f32::from_bits(1),
    f32::NAN,
    f32::INFINITY,
    f32::NEG_INFINITY,
    0.0f32,
    -0.0f32,
  ];
  verify_roundtrip(&extremes_f32)?;
  verify_roundtrip_delta(&extremes_f32)?;
  Ok(())
}

#[test]
fn test_boundary_exception_count_thresholds() -> Result<()> {
  // ALP 在单个 1024 向量中最多容忍 128 个异常（12.5%）
  // 1. 刚好 0 个异常
  let d0: Vec<f64> = (0..1024).map(|i| (i as f64) * 0.1).collect();
  verify_roundtrip(&d0)?;

  // 2. 刚好 1 个异常
  let mut d1 = d0.clone();
  d1[500] = f64::NAN;
  verify_roundtrip(&d1)?;

  // 3. 刚好 128 个异常（处于容忍阈值上限，应维持 ALP 压缩）
  let mut d128 = d0.clone();
  for k in 0..128 {
    d128[k * 8] = 99999999.12345678;
  }
  verify_roundtrip(&d128)?;

  // 4. 刚好 129 个异常：超过标准 ALP 128 异常上限后，由于高位离散度低，自动触发 ALP-RD 模式
  let mut d129 = d0.clone();
  for k in 0..129 {
    d129[k * 7] = 99999999.12345678;
  }
  let comp129 = verify_roundtrip(&d129)?;
  let hdr129 = read_header(&comp129)?;
  assert_eq!(hdr129.type_byte, TYPE_F64_RD);

  // 5. f32 对应的 129 个异常：触发 f32 ALP-RD 模式
  let mut d129_f32: Vec<f32> = (0..1024).map(|i| (i as f32) * 0.1f32).collect();
  for k in 0..129 {
    d129_f32[k * 7] = 888_888.9_f32;
  }
  let comp129_f32 = verify_roundtrip(&d129_f32)?;
  let hdr129_f32 = read_header(&comp129_f32)?;
  assert_eq!(hdr129_f32.type_byte, TYPE_F32_RD);

  // 6. 完全随机高熵比特（ALP 与 ALP-RD 均无法压缩）必触发保底 RAW 模式
  fastrand::seed(42);
  let raw_data_f64: Vec<f64> = (0..1024)
    .map(|_| f64::from_bits(fastrand::u64(..)))
    .collect();
  let comp_raw = verify_roundtrip(&raw_data_f64)?;
  let hdr_raw = read_header(&comp_raw)?;
  assert_eq!(hdr_raw.type_byte, TYPE_F64_RAW);

  let raw_data_f32: Vec<f32> = (0..1024)
    .map(|_| f32::from_bits(fastrand::u32(..)))
    .collect();
  let comp_raw_f32 = verify_roundtrip(&raw_data_f32)?;
  let hdr_raw_f32 = read_header(&comp_raw_f32)?;
  assert_eq!(hdr_raw_f32.type_byte, TYPE_F32_RAW);

  Ok(())
}

#[test]
fn test_boundary_corrupted_input_defense() -> Result<()> {
  // 1. 0 字节空切片解压
  assert!(matches!(
    decompress::<f64>(&[]),
    Err(Error::UnexpectedEof { .. })
  ));
  assert!(matches!(count(&[]), Err(Error::UnexpectedEof { .. })));

  // 2. 截断数据（仅有 1 字节描述符，缺乏 count 和参数）
  assert!(matches!(
    decompress::<f64>(&[0x01]),
    Err(Error::UnexpectedEof { .. })
  ));

  // 3. 非法类型字节（type_byte = 0 或 > MAX_TYPE_BYTE）
  assert!(matches!(
    decompress::<f64>(&[0x00, 0x01]),
    Err(Error::InvalidHeader)
  ));
  assert!(matches!(
    decompress::<f64>(&[0x0F, 0x01]),
    Err(Error::InvalidHeader)
  ));

  // 4. decompress_into_slice 切片容量不足防护
  let data = vec![1.25f64, 2.5, 3.75, 5.0];
  let comp = fastalp::compress(&data);
  let mut insufficient_buf = [0.0f64; 3];
  assert!(matches!(
    decompress_into_slice(&comp, &mut insufficient_buf),
    Err(Error::BufferTooSmall {
      needed: 4,
      available: 3
    })
  ));

  // 5. 损坏 repeat bitmap 防御：第 0 位非法设为 1
  let rep_data = vec![123.456f64; 64];
  let mut rep_comp = fastalp::compress(&rep_data);
  if (rep_comp[0] & FLAG_REPEAT) != 0 {
    let hdr = read_header(&rep_comp)?;
    rep_comp[hdr.cursor] |= 1;
    let res: Result<Vec<f64>> = decompress(&rep_comp);
    assert!(matches!(res, Err(Error::InvalidHeader)));
  }

  // 6. 截断 payload 导致 UnexpectedEof
  if rep_comp.len() > 8 {
    let truncated = &rep_comp[..rep_comp.len() - 4];
    assert!(decompress::<f64>(truncated).is_err());
  }

  Ok(())
}
