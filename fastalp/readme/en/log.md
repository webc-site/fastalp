## Changelog

### v0.1.48

- **Non-Dependent Parallel LUT Repeat Expansion & 2KB Compile-Time Table**:
  Precomputes a 256*8 byte relative offset lookup table (`REPEAT_OFFSETS_LUT`, exactly 2KB, 100% resident in L1 D-Cache) via `const fn`. Breaks the 8-stage serial shift-and-add data dependency chain in time-series run-length expansion, dispatching concurrent memory loads across superscalar CPU execution ports. Completely eliminates `prev` state tracking and provides native SIMD fast paths for all-zero and all-one words, dramatically accelerating decompression for repeat-dense datasets.
- **Zero-Width Constant Broadcast Soundness Hardening & Vectorized In-Place Writes**:
  Replaces manual store loops for `bit_width == 0` blocks with `MaybeUninit` slice `fill`, strictly complying with the Rust Tree Borrows and Stacked Borrows memory models while enabling LLVM to emit native SIMD broadcast stores (ARM64 `st1` / x86 `vmovups`) that saturate memory bus bandwidth at 80~95 GB/s.
- **4-Run Contiguous Span Sampling & High-Risk Exception Penalty**:
  Restructures sample extraction into 4 contiguous runs distributed across 0%, 33%, 66%, and 100% intervals of the block, capturing local differential smoothness while eliminating global bias at an O(1) constant budget. Integrates `calc_exc_cost` with a 4x exponential penalty on high exception rates (`exceptions >= 3`), preventing parameters from exceeding the 128-exception threshold and falling back to RAW mode. Propels `cms1` compression ratio by +39% with decompression speed doubling to 29.5 GB/s, while securing stable decimal encoding for `medicare1`.
- **Fill-Forward Exception Smoothing & Single-Step Index Probe**:
  Ensures exceptions inherit predecessor integers so differential residuals are strictly 0, preventing artificial jump spikes. Employs standard library `position` iterators over sorted exceptions to find initial valid elements in O(K) time, replacing repetitive binary searches across the entire block.

### v0.1.47

- **Streamlined Example Codebase & Unified Relative Dataset Paths**:
  Refactored all benchmark and verification binaries in `examples/` (`bench_all_codecs`, `bench_all_37`, `check_dec`, `check_schemes`, `profile_steps`). Extracted shared `examples/common` module to consolidate relative dataset resolution and CSV ingestion, eliminating repetitive PathBuf candidate searching across binaries.
- **Zero `#[allow(...)]` Across Codebase & Modern Rust Idioms**:
  Completely eradicated all `#[allow(...)]` lint suppressions across the repository. Refactored kernel/engine signatures using clean parameter objects and slice iterators, passing strict nightly Clippy checks with zero warnings.
- **Comprehensive Boundary & Fault-Tolerance Test Coverage**:
  Added comprehensive tests covering IEEE 754 edge cases (+0/-0, NaN, Inf), slice lengths from 0 to 2049, length tag transitions, exception thresholds, and defensive parsing against corrupted bitstreams.

### v0.1.46

- **Benchmark Suite Upgraded with Real-World Industrial Datasets**:
  Completely removed synthetic mock data generators in favor of 6 built-in representative real-world industrial datasets (temperature, stock prices, barometric pressure, food price index, volatile cryptocurrency, and air quality). Fully exercises FOR, Delta, Run-Length Repeat, Outlier Pruning, and ALP-RD decoding kernels.
- **Zero-Allocation Benchmark Inner Loops**:
  Eliminated hidden heap allocations across all three benchmark modes (sampled compression, warm-kernel compression, and decompression). Reusable scratch buffers and encoder instances isolate pure superscalar CPU execution throughput from memory allocator noise.
- **Generic Ingestion & CI Regression Tracking Hardening**:
  Unified generic CSV parsing and data tiling logic, eliminating code redundancy while enhancing benchmark report aggregation script compatibility.

### v0.1.45

- **Two-Stage Dynamic Outlier Budget for FOR Mode**:
  Decoupled outlier pruning between preliminary screening and FOR-exclusive optimization. Constrained the pre-pruning budget to 16 exceptions to protect Delta differential encoding from excessive outlier overhead; once Delta is bypassed, expanded the FOR exception budget to 32, unlocking deeper bit-width reduction while strictly adhering to cost monotonicity. Datasets with isolated spikes achieve tighter packing and faster decoding (e.g. `medicare9` bit-width dropped from 9 to 8, boosting ratio to 6.73x and decoding throughput up to 40.96 GB/s).
- **Delta Patch Artifact Purge & Zero-Overhead State Machine Transition**:
  Purged predecessor patch artifacts at exception locations in FOR mode, eliminating double-counting in outlier histograms and premature aborts. Refined cached pruning guards in the state machine, enabling multi-stage exploration on initial blocks while preserving instant O(1) cache hits on subsequent chunks.
- **Defensive Bit-Width Bounds & Soundness Hardening**:
  Hardened target bit-width mask calculation with defensive bounds against potential shifts >= 64 bits. Cleaned up redundant buffer size calculations on unpruned branches, passing rigorous safety and architectural review with zero warnings.

### v0.1.44

- **Branch-Free Repeat Run-Length Expansion**:
  Derived and applied the bitmap state transition identity, eliminating all bit-by-bit conditional branches in repeat expansion in favor of single-instruction arithmetic step updates and direct pointer writes; coupled with 8-element unrolled write macros to eradicate pipeline stalls, boosting decompression throughput on repeat-heavy datasets by 15% ~ 24% (e.g. `food_prices` up to 20.8 GB/s, `nyc29` up to 18.2 GB/s).
- **16-Element Wide-Load Unpacking for Low Bit-Widths**:
  Fully rolled out 16-element instruction-level parallelism (ILP) 2-way unrolling across 1, 2, and 4-bit unpacking kernels; leveraged single-instruction 16-bit, 32-bit, and 64-bit wide-word loads to cut load instructions in half and saturate superscalar ALU execution ports.
- **Global Unrolling Macro Framework Expansion**:
  Introduced `unroll_16!` and `write_16!` macros in `src/macros.rs`, eliminating repetitive manual unrolling boilerplate and removing unused constants and slice dead code.

### v0.1.43

- **4-Step Linear Recurrence Prefix Tree**:
  In `AlpDeltaConsumer`, leveraged the associative and commutative algebraic ring properties of two's complement arithmetic to precompute minimum-delta step vectors, decoupling the serial accumulator into isomorphic 4-tuple balanced binary addition trees and eliminating redundant per-element additions while reducing loop-carried dependency latency to a single instruction cycle.
- **16-Element Instruction-Level Parallelism Unrolling**:
  Expanded 8-bit unpacking (`unpack_8`) and delta scanning (`scan_deltas`) into 16-element dual-path loads with stepped fallbacks, saturating multi-issue execution ports; compressed the delta extremum reduction tree depth down to 4 levels, slashing critical-path latency by over 70%.
- **Zero-Allocation Real Doubles Decoding**:
  Replaced 16KB per-block stack zeroing in `decode_rd_raw` with uninitialized scratch buffers and in-place 16-element bitwise OR unrolling, achieving zero-copy in-place reconstruction.
- **Pure Mathematical Division Pruning & Early Abort**:
  In parameter sampling, instantly pruned high-latency floating-point division branches when pre-checks proved non-decimal characteristics, and introduced 4-sample anomaly fast aborts to eliminate wasted cycles on non-decimal sequences.
- **Macro Metaprogramming & Redundant Code Elimination**:
  Removed all backward-compatible macro aliases, designed unified metaprogramming macros generating monomorphized jump tables across all bit widths 1..=32, dramatically shrinking binary footprint and boilerplate duplication.

### v0.1.42

- **Standalone Repository Migration & Workspace Standardization**:<br>
  Migrated to standalone repository `webc-site/fastalp`; standardized package metadata, documentation templates, and community links; unified workspace configurations.

### v0.1.40

- **Bit-Unpacking Architectural Decoupling & Fused Single-Pass Consumer**:
  Refactored the monolithic bit-unpacking engine into modular subcomponents: `consumer.rs`, `decoder.rs`, `kernel.rs`, and safe top-level dispatchers. Abstracted the `AlpConsumer` pipeline paradigm, fusing Delta first-order difference prefix sums and floating-point reconstruction directly in CPU registers, eliminating 8KB intermediate stack buffers and double iterations.
- **Instruction Pipeline Wide Loads & Loop Accelerations**:
  Replaced branch-heavy element-by-element loops and slice reads in `unpack_2`, `unpack_4`, and `unpack_16` with single-instruction `u16`, `u32`, and `u128` wide loads and pure bitshift extractions; vectorized 0-bit constant block filling via 8-way unrolling (`write_8!`) in `consume_zeros`; decoupled the running accumulator in `AlpDeltaConsumer`, cutting cycle dependency latency from 2 cycles down to 1 cycle.
- **Global Unrolling & Dispatch Macro Framework**:
  Introduced `src/macros.rs` (`arr_8!`, `unroll_8!`, `write_8!`, `write_4!`, `match_pack_23!`), collapsing 23-arm packing match boilerplate and pre-binding pointers to eliminate duplicate expression evaluations and remove 120+ lines of redundant code.
- **Uninitialized Memory Soundness & Zero Clippy Warnings**:
  Adopted raw pointer reservation and in-place writes in `decompress_into`, `bitunpack_u64_raw`, and `expand_repeats`, strictly eliminating undefined behavior (UB) from constructing uninitialized slice references; cleaned up all absolute path references to achieve zero warnings under `-W clippy::absolute_paths`.