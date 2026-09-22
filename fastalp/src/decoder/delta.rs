use core::{mem::MaybeUninit, slice::from_raw_parts_mut};

use crate::{
  bitpack::{
    AlpDecoder, AlpDeltaConsumer, AlpDeltaZeroMinConsumer, bitunpack_core_consumer,
    packed_byte_size,
  },
  error::{Error, Result},
  float::AlpFloat,
  params::AlpParams,
};

/// Decodes an ALP Delta differential compressed block directly to raw pointer.
/// 解压 ALP Delta 一阶差分压缩数据块至裸指针内存 (src 为头部之后的有效载荷，零堆分配)
///
/// # Safety
///
/// `dst_ptr` must point to valid memory for at least `count` continuous writable `F` elements.
/// `dst_ptr` 必须指向至少具备 `count` 个连续可写 `F` 元素的有效内存。
#[inline]
pub unsafe fn decode_delta_raw<F: AlpFloat>(
  src: &[u8],
  count: usize,
  params: AlpParams,
  dst_ptr: *mut F,
) -> Result<()> {
  if count == 0 {
    return Ok(());
  }
  let mut cursor = 0;

  if src.len() < cursor + F::BASE_SIZE * 2 {
    return Err(Error::UnexpectedEof {
      needed: cursor + F::BASE_SIZE * 2,
      available: src.len(),
    });
  }

  let first = F::read_base(&src[cursor..cursor + F::BASE_SIZE]);
  cursor += F::BASE_SIZE;

  let min_delta = F::read_base(&src[cursor..cursor + F::BASE_SIZE]);
  cursor += F::BASE_SIZE;

  let payload = &src[cursor..];
  dispatch_decoder!(params, first, F, decoder => {
    // SAFETY: Valid byte count verified above, and caller guarantees dst_ptr has count space
    // SAFETY: 上方已校验有效字节数，且调用方保证 dst_ptr 具有 count 空间
    unsafe {
      decode_delta_inner(payload, count, params, decoder, first, min_delta, dst_ptr)?;
    }
  });

  if params.bit_width > 0 && count > 1 {
    let packed_len = packed_byte_size(count - 1, params.bit_width);
    cursor += packed_len;
  }

  // Restore exceptions (patch dictionary)
  // 恢复异常值（Patch 字典）
  unsafe {
    super::patch_exceptions(&src[cursor..], count, dst_ptr)?;
  }

  Ok(())
}

#[inline(always)]
unsafe fn decode_delta_inner<F: AlpFloat, D: AlpDecoder<F>>(
  payload: &[u8],
  count: usize,
  params: AlpParams,
  decoder: D,
  first: F::Int,
  min_delta: F::Int,
  dst_ptr: *mut F,
) -> Result<()> {
  let first_val = decoder.decode_int(first);
  unsafe {
    *dst_ptr = first_val;
  }
  if count == 1 {
    return Ok(());
  }

  let rest_count = count - 1;

  // 借鉴 graupel 思想：0 位宽零存储等差/恒定极速短路（吞吐直达总线极限 80+ GB/s）
  if params.bit_width == 0 {
    if min_delta == F::ZERO_INT {
      // SAFETY: 调用方保证 dst_ptr 具有至少 count 个槽位；采用 MaybeUninit 严守 Rust 内存安全模型
      unsafe {
        from_raw_parts_mut(dst_ptr.add(1).cast::<MaybeUninit<F>>(), rest_count)
          .fill(MaybeUninit::new(first_val));
      }
    } else {
      let m1 = min_delta;
      let m2 = F::int_add(m1, m1);
      let m3 = F::int_add(m2, m1);
      let m4 = F::int_add(m2, m2);
      let full_8 = rest_count / 8;
      let mut base_curr = first;
      for g in 0..full_8 {
        let ptr = unsafe { dst_ptr.add(1 + g * 8) };
        let c0 = F::int_add(base_curr, m1);
        let c1 = F::int_add(base_curr, m2);
        let c2 = F::int_add(base_curr, m3);
        let c3 = F::int_add(base_curr, m4);
        let c4 = F::int_add(c3, m1);
        let c5 = F::int_add(c3, m2);
        let c6 = F::int_add(c3, m3);
        let c7 = F::int_add(c3, m4);
        base_curr = c7;
        let c = [c0, c1, c2, c3, c4, c5, c6, c7];
        unsafe {
          write_8!(ptr, k => decoder.decode_int(c[k]));
        }
      }
      let mut curr = base_curr;
      for i in (1 + full_8 * 8)..count {
        curr = F::int_add(curr, m1);
        unsafe {
          *dst_ptr.add(i) = decoder.decode_int(curr);
        }
      }
    }
    return Ok(());
  }

  let packed_len = packed_byte_size(rest_count, params.bit_width);
  if payload.len() < packed_len {
    return Err(Error::UnexpectedEof {
      needed: packed_len,
      available: payload.len(),
    });
  }

  // SAFETY: dst_ptr has count slots guaranteed by caller; unpack and reconstruct in a single fused pass
  // 根据 min_delta 是否为 0 分发到零公差极速特化单态化消费者
  unsafe {
    if min_delta == F::ZERO_INT {
      let consumer = AlpDeltaZeroMinConsumer::new(first, decoder);
      bitunpack_core_consumer(
        &payload[..packed_len],
        rest_count,
        params.bit_width,
        consumer,
        dst_ptr.add(1),
      );
    } else {
      let consumer = AlpDeltaConsumer::new(first, min_delta, decoder);
      bitunpack_core_consumer(
        &payload[..packed_len],
        rest_count,
        params.bit_width,
        consumer,
        dst_ptr.add(1),
      );
    }
  }
  Ok(())
}
