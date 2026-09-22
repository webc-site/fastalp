//! Project-wide unrolling, chunk manipulation, and dispatch helper macros.
//! 全局通用的循环展开、数组构造、内存写入与分发辅助宏

/// Constructs an 8-element array by applying an expression to index 0..8.
/// 通过对索引 0..8 分别求值构造 8 元素数组（完全内联，消除手动 +1, +2, +3）
#[macro_export]
macro_rules! arr_8 {
  ($idx:ident => $expr:expr) => {
    [
      {
        let $idx = 0;
        $expr
      },
      {
        let $idx = 1;
        $expr
      },
      {
        let $idx = 2;
        $expr
      },
      {
        let $idx = 3;
        $expr
      },
      {
        let $idx = 4;
        $expr
      },
      {
        let $idx = 5;
        $expr
      },
      {
        let $idx = 6;
        $expr
      },
      {
        let $idx = 7;
        $expr
      },
    ]
  };
}

/// Unrolls a block 8 times with `$idx` bound to 0..8.
/// 将逻辑按索引 0..8 重复展开 8 次顺序执行
#[macro_export]
macro_rules! unroll_8 {
  ($idx:ident => $expr:expr) => {{
    let $idx = 0;
    $expr;
    let $idx = 1;
    $expr;
    let $idx = 2;
    $expr;
    let $idx = 3;
    $expr;
    let $idx = 4;
    $expr;
    let $idx = 5;
    $expr;
    let $idx = 6;
    $expr;
    let $idx = 7;
    $expr;
  }};
}

/// Unrolls a block 16 times with `$idx` bound to 0..16.
/// 将逻辑按索引 0..16 重复展开 16 次顺序执行
#[macro_export]
macro_rules! unroll_16 {
  ($idx:ident => $expr:expr) => {{
    $crate::unroll_8!(k => {
      let $idx = k;
      $expr;
    });
    $crate::unroll_8!(k => {
      let $idx = k + 8;
      $expr;
    });
  }};
}

/// Writes 8 evaluated elements to consecutive raw pointer memory `*($dst).add(k) = expr(k)`.
/// 向连续裸指针内存顺序写入 8 个计算结果（局部绑定 base 指针，杜绝表达式重复求值）
#[macro_export]
macro_rules! write_8 {
  ($dst:expr, $idx:ident => $expr:expr) => {{
    let dst = $dst;
    $crate::unroll_8!($idx => {
      *dst.add($idx) = $expr;
    });
  }};
}

/// Writes 16 evaluated elements to consecutive raw pointer memory `*($dst).add(k) = expr(k)`.
/// 向连续裸指针内存顺序写入 16 个计算结果（局部绑定 base 指针）
#[macro_export]
macro_rules! write_16 {
  ($dst:expr, $idx:ident => $expr:expr) => {{
    let dst = $dst;
    $crate::unroll_16!($idx => {
      *dst.add($idx) = $expr;
    });
  }};
}

/// Unrolls a block 4 times with `$idx` bound to 0..4.
/// 将逻辑按索引 0..4 重复展开 4 次顺序执行
#[macro_export]
macro_rules! unroll_4 {
  ($idx:ident => $expr:expr) => {{
    let $idx = 0;
    $expr;
    let $idx = 1;
    $expr;
    let $idx = 2;
    $expr;
    let $idx = 3;
    $expr;
  }};
}

/// Writes 4 evaluated elements to consecutive raw pointer memory `*($dst).add(k) = expr(k)`.
/// 向连续裸指针内存顺序写入 4 个计算结果（局部绑定 base 指针）
#[macro_export]
macro_rules! write_4 {
  ($dst:expr, $idx:ident => $expr:expr) => {{
    let dst = $dst;
    $crate::unroll_4!($idx => {
      *dst.add($idx) = $expr;
    });
  }};
}

/// Generic case generator for monomorphized bit-width dispatches.
/// 编译期单态化分发通用模式生成宏（避免手动手写 30+ 冗余分支）
#[macro_export]
macro_rules! match_pack_cases {
  ($bw:expr, fallback => $fallback:expr, |$w:ident| $arm:expr, [$($val:literal),* $(,)?]) => {
    match $bw {
      $(
        $val => {
          const $w: u8 = $val;
          $arm
        }
      )*
      _ => $fallback,
    }
  };
}

/// Dispatches bit-width 1..=32 to monomorphized chunk packing function.
/// 统一分发 1..=32 位宽至单态化 8 元素块打包内核（宏元编程展开，零重复代码）
#[macro_export]
macro_rules! match_pack_32 {
  ($bw:expr, fallback => $fallback:expr, |$w:ident| $arm:expr) => {
    $crate::match_pack_cases!(
      $bw,
      fallback => $fallback,
      |$w| $arm,
      [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32
      ]
    )
  };
}
