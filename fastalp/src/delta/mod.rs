//! High-performance differential (Delta) encoding utilities for consecutive time series float streams.
//! 针对连续时序浮点数流的高性能一阶差分（Delta）编码与前缀和还原模块

use crate::float::AlpFloat;

#[inline(always)]
fn scan_deltas<F: AlpFloat>(
  slice: &[F::Int],
  prev: &mut F::Int,
  min_delta: &mut F::Int,
  max_delta: &mut F::Int,
) {
  let (chunks, rem) = slice.as_chunks::<16>();
  for chunk in chunks {
    let d0 = F::int_sub(chunk[0], *prev);
    let d1 = F::int_sub(chunk[1], chunk[0]);
    let d2 = F::int_sub(chunk[2], chunk[1]);
    let d3 = F::int_sub(chunk[3], chunk[2]);
    let d4 = F::int_sub(chunk[4], chunk[3]);
    let d5 = F::int_sub(chunk[5], chunk[4]);
    let d6 = F::int_sub(chunk[6], chunk[5]);
    let d7 = F::int_sub(chunk[7], chunk[6]);
    let d8 = F::int_sub(chunk[8], chunk[7]);
    let d9 = F::int_sub(chunk[9], chunk[8]);
    let d10 = F::int_sub(chunk[10], chunk[9]);
    let d11 = F::int_sub(chunk[11], chunk[10]);
    let d12 = F::int_sub(chunk[12], chunk[11]);
    let d13 = F::int_sub(chunk[13], chunk[12]);
    let d14 = F::int_sub(chunk[14], chunk[13]);
    let d15 = F::int_sub(chunk[15], chunk[14]);
    *prev = chunk[15];

    let min01 = d0.min(d1);
    let min23 = d2.min(d3);
    let min45 = d4.min(d5);
    let min67 = d6.min(d7);
    let min89 = d8.min(d9);
    let min1011 = d10.min(d11);
    let min1213 = d12.min(d13);
    let min1415 = d14.min(d15);

    let min0123 = min01.min(min23);
    let min4567 = min45.min(min67);
    let min891011 = min89.min(min1011);
    let min12131415 = min1213.min(min1415);

    let min_lo = min0123.min(min4567);
    let min_hi = min891011.min(min12131415);
    let l_min = min_lo.min(min_hi);

    let max01 = d0.max(d1);
    let max23 = d2.max(d3);
    let max45 = d4.max(d5);
    let max67 = d6.max(d7);
    let max89 = d8.max(d9);
    let max1011 = d10.max(d11);
    let max1213 = d12.max(d13);
    let max1415 = d14.max(d15);

    let max0123 = max01.max(max23);
    let max4567 = max45.max(max67);
    let max891011 = max89.max(max1011);
    let max12131415 = max1213.max(max1415);

    let max_lo = max0123.max(max4567);
    let max_hi = max891011.max(max12131415);
    let l_max = max_lo.max(max_hi);

    *min_delta = (*min_delta).min(l_min);
    *max_delta = (*max_delta).max(l_max);
  }

  for &curr in rem {
    let delta = F::int_sub(curr, *prev);
    *min_delta = (*min_delta).min(delta);
    *max_delta = (*max_delta).max(delta);
    *prev = curr;
  }
}

/// Calculates the min delta and required bit width for adjacent first-order differences.
/// 计算相邻一阶差分的极小值与所需比特位宽（16 元素二叉平衡规约流水线计算）
#[inline(always)]
pub(crate) fn delta_range<F: AlpFloat>(first: F::Int, rest: &[F::Int]) -> (F::Int, u8) {
  if rest.is_empty() {
    return (F::ZERO_INT, 0);
  }
  let mut min_delta = F::MAX_INT;
  let mut max_delta = F::MIN_INT;
  let mut prev = first;
  scan_deltas::<F>(rest, &mut prev, &mut min_delta, &mut max_delta);
  let delta_bit_width = F::bits_needed(F::calc_range(min_delta, max_delta));
  (min_delta, delta_bit_width)
}

/// Evaluates whether delta encoding yields a smaller bit width than standard Frame-of-Reference (FOR).
/// 评估一阶差分编码相比直接 FOR 基准值对齐是否具有更窄的比特位宽优势（前置 16 采样快筛，无缝续扫零冗余内存访问）
#[inline(always)]
pub(crate) fn eval_delta_benefit<F: AlpFloat>(
  first: F::Int,
  rest: &[F::Int],
  for_bit_width: u8,
) -> Option<(F::Int, u8)> {
  if rest.is_empty() {
    return None;
  }

  // Mathematical property fast pre-filter: sub-interval extremum range <= full range.
  // If first 16 elements already have delta bit-width >= for_bit_width, abort early.
  // 数学性质快筛：子区间的极值跨度恒 <= 全量区间极值跨度。
  // 若前 16 个元素的差分位宽已 >= for_bit_width，则全集差分位宽绝不可能小于 for_bit_width，立即短路早停。
  let pre_n = rest.len().min(16);
  let mut min_delta = F::MAX_INT;
  let mut max_delta = F::MIN_INT;
  let mut prev = first;
  scan_deltas::<F>(&rest[..pre_n], &mut prev, &mut min_delta, &mut max_delta);

  let pre_bw = F::bits_needed(F::calc_range(min_delta, max_delta));
  if pre_bw >= for_bit_width {
    return None;
  }

  if pre_n < rest.len() {
    scan_deltas::<F>(&rest[pre_n..], &mut prev, &mut min_delta, &mut max_delta);
  }

  let delta_bit_width = F::bits_needed(F::calc_range(min_delta, max_delta));
  if delta_bit_width < for_bit_width {
    Some((min_delta, delta_bit_width))
  } else {
    None
  }
}
