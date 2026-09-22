use fearless_simd::{Level, Simd, dispatch, prelude::*};

use crate::{constants::MAX_EXCEPTIONS, encoder::exception::Exception, float::AlpFloat};

macro_rules! define_fearless_kernel {
  (
    $kernel_fn:ident,
    $simd_fn:ident,
    $F:ty,
    $I:ty,
    $U:ty,
    $floats:ident,
    $ints:ident,
    $uints:ident
  ) => {
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    unsafe fn $kernel_fn<S: Simd>(
      simd: S,
      slice: &[$F],
      enc_ptr: *mut $I,
      exp_factor: $F,
      fac_int: i64,
      frac_exp: $F,
      use_div: bool,
      exceptions: &mut Vec<Exception<$U>>,
    ) -> ($I, $I) {
      let n = S::$floats::N;
      let stride = n * 4;
      let exp_v = S::$floats::splat(simd, exp_factor);
      let frac_v = S::$floats::splat(simd, frac_exp);
      let fac_v = S::$ints::splat(simd, fac_int as $I);

      let mut min_v0 = S::$floats::splat(simd, <$F>::INFINITY);
      let mut min_v1 = S::$floats::splat(simd, <$F>::INFINITY);
      let mut max_v0 = S::$floats::splat(simd, <$F>::NEG_INFINITY);
      let mut max_v1 = S::$floats::splat(simd, <$F>::NEG_INFINITY);
      let mut any_diff_v = S::$uints::splat(simd, 0);

      let full_chunks_len = slice.len() / stride * stride;
      let (full_slice, rem_slice) = slice.split_at(full_chunks_len);

      macro_rules! run_loop {
        ($decode:expr) => {
          for (chunk_idx, c) in full_slice.chunks_exact(stride).enumerate() {
            let base_out = chunk_idx * stride;
            let v0 = S::$floats::from_slice(simd, &c[0..n]);
            let v1 = S::$floats::from_slice(simd, &c[n..n * 2]);
            let v2 = S::$floats::from_slice(simd, &c[n * 2..n * 3]);
            let v3 = S::$floats::from_slice(simd, &c[n * 3..n * 4]);

            let r0 = (v0 * exp_v).round_ties_even();
            let r1 = (v1 * exp_v).round_ties_even();
            let r2 = (v2 * exp_v).round_ties_even();
            let r3 = (v3 * exp_v).round_ties_even();

            let i0 = r0.to_int::<S::$ints>();
            let i1 = r1.to_int::<S::$ints>();
            let i2 = r2.to_int::<S::$ints>();
            let i3 = r3.to_int::<S::$ints>();

            unsafe {
              i0.store_slice(core::slice::from_raw_parts_mut(enc_ptr.add(base_out), n));
              i1.store_slice(core::slice::from_raw_parts_mut(enc_ptr.add(base_out + n), n));
              i2.store_slice(core::slice::from_raw_parts_mut(enc_ptr.add(base_out + n * 2), n));
              i3.store_slice(core::slice::from_raw_parts_mut(enc_ptr.add(base_out + n * 3), n));
            }

            let d0 = $decode(i0);
            let d1 = $decode(i1);
            let d2 = $decode(i2);
            let d3 = $decode(i3);

            let diff0 = d0.bitcast::<S::$uints>() ^ v0.bitcast::<S::$uints>();
            let diff1 = d1.bitcast::<S::$uints>() ^ v1.bitcast::<S::$uints>();
            let diff2 = d2.bitcast::<S::$uints>() ^ v2.bitcast::<S::$uints>();
            let diff3 = d3.bitcast::<S::$uints>() ^ v3.bitcast::<S::$uints>();

            any_diff_v |= diff0 | diff1 | diff2 | diff3;

            min_v0 = min_v0.min(r0);
            min_v1 = min_v1.min(r1);
            min_v0 = min_v0.min(r2);
            min_v1 = min_v1.min(r3);

            max_v0 = max_v0.max(r0);
            max_v1 = max_v1.max(r1);
            max_v0 = max_v0.max(r2);
            max_v1 = max_v1.max(r3);
          }
        };
      }

      if use_div {
        run_loop!(|i: S::$ints| i.to_float::<S::$floats>() / exp_v);
      } else if fac_int == 1 {
        run_loop!(|i: S::$ints| i.to_float::<S::$floats>() * frac_v);
      } else {
        run_loop!(|i: S::$ints| (i * fac_v).to_float::<S::$floats>() * frac_v);
      }

      let min_v = min_v0.min(min_v1);
      let max_v = max_v0.max(max_v1);

      let mut min_arr = [0.0 as $F; 16];
      let mut max_arr = [0.0 as $F; 16];
      let mut diff_arr = [0 as $U; 16];
      min_v.store_slice(&mut min_arr[..n]);
      max_v.store_slice(&mut max_arr[..n]);
      any_diff_v.store_slice(&mut diff_arr[..n]);

      let mut any_diff: $U = 0;
      let (mut min_int, mut max_int) = if full_chunks_len > 0 {
        let mut min_val = <$F>::INFINITY;
        let mut max_val = <$F>::NEG_INFINITY;
        for ((&min_e, &max_e), &diff_e) in min_arr[..n]
          .iter()
          .zip(&max_arr[..n])
          .zip(&diff_arr[..n])
        {
          min_val = min_val.min(min_e);
          max_val = max_val.max(max_e);
          any_diff |= diff_e;
        }
        (min_val as $I, max_val as $I)
      } else {
        (<$I>::MAX, <$I>::MIN)
      };

      // SAFETY: Caller guarantees enc_ptr has valid writable memory for at least slice.len() elements.
      let enc_slice = unsafe { core::slice::from_raw_parts_mut(enc_ptr, slice.len()) };

      if !rem_slice.is_empty() {
        let rem_start = full_chunks_len;
        for (&v, enc_ref) in rem_slice.iter().zip(&mut enc_slice[rem_start..]) {
          let enc = (v * exp_factor).round_ties_even() as $I;
          *enc_ref = enc;
          let d = if use_div {
            (enc as $F) / exp_factor
          } else if fac_int == 1 {
            (enc as $F) * frac_exp
          } else {
            ((enc.wrapping_mul(fac_int as $I)) as $F) * frac_exp
          };
          if d.to_bits() == v.to_bits() {
            min_int = min_int.min(enc);
            max_int = max_int.max(enc);
          } else {
            any_diff |= 1;
          }
        }
      }

      if any_diff == 0 {
        return (min_int, max_int);
      }

      let mut min_int_rescanned = <$I>::MAX;
      let mut max_int_rescanned = <$I>::MIN;

      macro_rules! run_rescan {
        ($decode:expr) => {
          for (idx, (&v, enc_ref)) in slice.iter().zip(enc_slice.iter_mut()).enumerate() {
            let enc = *enc_ref;
            let d = $decode(enc);
            if d.to_bits() == v.to_bits() {
              min_int_rescanned = min_int_rescanned.min(enc);
              max_int_rescanned = max_int_rescanned.max(enc);
            } else {
              *enc_ref = 0;
              exceptions.push(Exception {
                pos: idx,
                bits: v.to_bits(),
              });
              if exceptions.len() > MAX_EXCEPTIONS {
                return (<$I>::MAX, <$I>::MIN);
              }
            }
          }
        };
      }

      if use_div {
        run_rescan!(|enc: $I| (enc as $F) / exp_factor);
      } else if fac_int == 1 {
        run_rescan!(|enc: $I| (enc as $F) * frac_exp);
      } else {
        run_rescan!(|enc: $I| (enc.wrapping_mul(fac_int as $I) as $F) * frac_exp);
      }

      (min_int_rescanned, max_int_rescanned)
    }

    pub(crate) unsafe fn $simd_fn(
      slice: &[$F],
      enc_ptr: *mut $I,
      exp_factor: $F,
      fac_int: i64,
      frac_exp: $F,
      use_div: bool,
      exceptions: &mut Vec<Exception<$U>>,
    ) -> ($I, $I) {
      let level = Level::new();
      dispatch!(level, simd => {
        unsafe {
          $kernel_fn(simd, slice, enc_ptr, exp_factor, fac_int, frac_exp, use_div, exceptions)
        }
      })
    }
  };
}

define_fearless_kernel!(
  encode_fearless_f64_kernel,
  encode_simd_f64,
  f64,
  i64,
  u64,
  f64s,
  i64s,
  u64s
);

define_fearless_kernel!(
  encode_fearless_f32_kernel,
  encode_simd_f32,
  f32,
  i32,
  u32,
  f32s,
  i32s,
  u32s
);

/// Dispatches to optimal vectorized encoding kernel based on float type.
/// 统一分发至最优的向量化编码内核
#[inline(always)]
pub(crate) unsafe fn encode_slice<F: AlpFloat>(
  slice: &[F],
  enc_ptr: *mut F::Int,
  exp_factor: F,
  fac_int: i64,
  frac_exp: F,
  use_div: bool,
  exceptions: &mut Vec<Exception<F::RawBits>>,
) -> (F::Int, F::Int) {
  // SAFETY: Caller guarantees slice is continuous and enc_ptr has space for at least slice.len() elements.
  unsafe {
    F::encode_simd(
      slice, enc_ptr, exp_factor, fac_int, frac_exp, use_div, exceptions,
    )
  }
}
