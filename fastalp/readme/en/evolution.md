## Architectural Evolution & Novel Optimizations

`fastalp` is an engineered reimagining of the ALP paradigm for modern superscalar architectures and columnar time-series storage engines.

### Foundations Inherited from Original ALP

- **Two-Level Adaptive Sampling**:
  Derives optimal decimal scaling parameters `(exp, fac)` that minimize combined bit-width and exception penalties through two-phase coarse and fine sampling.

- **Hardware-Native Round-Ties-Even (Upgraded from Magic Number)**:
  Original ALP utilized IEEE 754 bias constants (`0x0018000000000000` / `12582912.0`) inside floating-point units. `fastalp` investigates its $[-2^{51}, 2^{51}]$ range boundary limitations and upgrades it to hardware-native round-to-nearest-even instructions (x86 `ROUNDSD` / ARM64 `FRINTN`), eliminating range overflow risks while maintaining branchless latency.

- **FOR Frame-of-Reference Subtraction**:
  Subtracts the frame-wide minimum value to shift signed ranges into compact non-negative domains, reducing encoded bit-widths.

---

### Proprietary Algorithmic & Performance Breakthroughs

- **Adaptive Delta-ALP**:
  Smooth sensor physical waveforms often have large absolute spans but tiny step differences. `fastalp` implements first-order difference encoding with 16-sample mathematical short-circuit pruning, narrowing bit-widths by 15% ~ 38%.

- **Decimal Exact Division Reconstruction (`use_div`)**:
  Eliminates spurious exception inflation caused by IEEE 754 binary truncation in multiplication (e.g. `* 0.1`). Reduces stored byte volume by 20% ~ 38%.

- **Intelligent Outlier Pruning & 0-bit Sparse Encoding**:
  For datasets where 99% of values are constant with rare isolated pulses, `fastalp` strips outliers into the exception dictionary, allowing the main bitstream to drop to 0-bit. Delivers compression ratios exceeding 150x ~ 744x.

- **Dual-Stage Dynamic Outlier Pruning & Budget Relaxation (16 to 32)**:
  Solves the trade-off between shielding Delta candidates and maximizing FOR compression. Pre-pruning enforces a strict 16-exception budget to preserve Delta eligibility. Once FOR mode is chosen, the exception budget relaxes to 32 to aggressively narrow long-tail spike bit-widths. Atomically resets exception locations to the base value prior to FOR pruning to purge Delta backfill artifacts, preventing histogram double counting.

- **Previous-Value Exception Backfilling**:
  Backfills exceptions with preceding integers to prevent artificial gradient steps that corrupt delta difference bit-widths.

- **Four-Step Recurrence Delta Tree & 1-Cycle Dependency Decoupling**:
  Precomputes minimal delta step vectors to decompose sequential prefix-sum dependencies into isomorphic four-element balanced binary trees. Decouples the running accumulator from the 8-element delta sum, slashing cross-batch loop dependency latency to a single clock cycle and accelerating delta decompression to 18 ~ 28 GB/s.

- **16-Element Wide Word Loading & Instruction-Level Parallelism**:
  Extends narrow unpacking kernels (1, 2, 4 bits) with 16-element dual unrolling (`unroll_16!`, `write_16!`), reading 16 elements per 64-bit load to saturate modern superscalar execution units.

- **Branch-Free Repeat Expansion**:
  Applies bitwise state transition invariants to eliminate all conditional branches and pipeline stalls during repeat expansion; triggers 64-element native SIMD copies/broadcasts on full-zero/full-one words, boosting repeat-dense decompression throughput significantly.

- **2-bit Self-Describing Headers & Arbitrary Length Support**:
  Employs a 2-bit length tag: standard 1024-element frames require only 3 bytes of header, while RAW fallback frames require 1 byte. Automatically scales to 32-bit offsets for arrays exceeding 65,535 elements.

- **12.5% Exception Bound & Single-Byte RAW Fallback**:
  Guarantees zero negative compression inflation on high-entropy data by falling back to a 1-byte header RAW stream whenever exceptions exceed 12.5% or encoded bytes exceed raw size.

- **Single-Comparison Fast Path for Equi-Value Sequences**:
  Checks `slice[1] == slice[0]` on block entry; non-constant streams exit in 1 CPU cycle, while constant sequences encode 1024 elements into 11 bytes (744x ratio).

- **Three-Stage Microarchitectural Pruning Pipeline**:
  Replaces unpruned parameter searches with a 3-tier cascade (pure decimal early return, 4/16-sample short-circuiting, and non-decimal abort), boosting end-to-end compression throughput from 0.8 GB/s to **1.9 GB/s** (2.41x faster than C++ ALP); pure encoding kernel throughput reaches **7.1 GB/s (1.35x faster than C++ ALP 5.3 GB/s)**; streaming throughput reaches **15~24+ GB/s** with cached parameters.

- **Pure Register SIMD Decompression**:
  Vectorizes common bit-widths (8, 16, 32, 64) into branchless register pipelines, achieving **27.0 GB/s** geometric mean decompression throughput (surpassing C++ ALP's 20.0 GB/s, 1.35x faster).

- **256-Entry L1D Stack-Allocated Lookup Tables**:
  Eliminates costly division latency by maintaining stack-resident tables that fit entirely in L1D cache.

- **Fused 8-Way Register-Level Delta Bitpacker**:
  Merges difference calculation, base subtraction, and bitpacking into a unified single-pass 128-bit register pipeline, eliminating intermediate memory roundtrips.

- **Mathematical Short-Circuit Delta Filter**:
  Proves mathematically that if the first 16 samples' delta range exceeds the FOR span, full delta encoding cannot be optimal, avoiding redundant scans for 90% of irregular series.

- **Branchless 4-Way Unrolled Encoding Loop**:
  Unrolls core scalar loops into 4-way parallel ALU streams, reaching **4.4~6.8 GB/s** encoding speeds.

- **Zero-Allocation Streaming Pipeline**:
  Provides `compress_into` and `decompress_into` interfaces, allowing applications to reuse buffers without GC or heap allocation churn.

- **Zero-Cost Generic Trait Abstraction**:
  Unifies `f64` and `f32` operations under `AlpFloat` with precomputed static power tables and compile-time inlining.

- **Compile-Time Const-Generic 64-Way 8-Element Periodic Bit-Unpacking**:
  Eliminates 128-bit variable-shift instruction bloat and register spills. Based on the mathematical invariant: for any $BW \in [1, 64]$, every 8 elements span exactly $BW$ bytes ($8 \times BW / 8 = BW$). Monomorphized across all 64 bit-widths: small widths (1, 2, 4) unroll via L1D tables; regular widths (8, 16, 32, 64) read native integers directly; and widths $BW \le 56$ fold all shifts into compile-time immediate constants within single 64-bit integer loads. Boosts decompression to **28.1+ GB/s**, reaching **47 ~ 91 GB/s** on smooth/regular sequences.

- **Zero-Copy 1024-Block Streaming ALP-RD Real Doubles Decoder**:
  Eliminates micro-batch chunking and double intermediate buffering in Real Doubles mode. Stream-unpacks `right_parts` (high-entropy mantissa) directly into raw destination pointer memory, unpacks 1-3 bit dictionary indices into an 8KB stack buffer, and fuses them via in-place bitwise OR (`dst[i] |= shifted_dict[...]`). Supercharges RD decompression from 3.7 GB/s by nearly 3x to **11.6+ GB/s**.

- **Branch-Free Word- and Byte-Level Run-Length Expansion (`expand_repeats`)**:
  Replaces branch-heavy 64-bit word scanning and variable `trailing_ones` loops with a two-tier unrolled state machine. Pure-zero and pure-one words trigger full 64-element SIMD copies or broadcasts. Mixed words expand byte-by-byte with branchless 8-element unrolled stores, accelerating repeat-heavy datasets (`food_prices`, `nyc29`) by 50% ~ 70%.

- **Zero-Overhead Strongly Typed `ChunkType` Enum**:
  Refactors wire format type byte into `#[repr(u8)] pub enum ChunkType`, eliminating string comparisons and redundant branches while ensuring compile-time exhaustive match verification.

- **Unified Consumer Paradigm (`AlpConsumer`) & Fused Single-Pass Delta Decompression**:
  Overhauls traditional two-pass delta decompression (unpacking integer deltas into an 8KB stack buffer, followed by a separate prefix-sum and floating-point conversion pass). Fastalp introduces the monomorphized `AlpConsumer` pipeline architecture: within the bit-unpacking kernel loop, each decoded batch of 8 delta offsets is immediately accumulated into prefix sums, base-offset, and converted to IEEE 754 floats directly in CPU registers before writing to destination pointer memory. Entirely eliminates 8KB intermediate stack buffer allocation, cache writes, and re-reads. Throughput on `neon_air_pressure` surged from 10.42 GB/s to 22.11 GB/s (2.12x speedup), boosting all 11 delta datasets to 18 ~ 28 GB/s and pushing global 31-dataset arithmetic mean decompression throughput to **30.34 GB/s**.

- **Decoupled Modular Bit-Unpacking Engine & Direct Specialized Dispatch**:
  Breaks apart the monolithic 1442-line unpacking file into clean, single-responsibility modules: `consumer.rs`, `decoder.rs`, `kernel.rs`, and safe top-level dispatchers. Dispatches inner macros directly to dedicated non-inlined subkernels (`unpack_1`, `unpack_2`, `unpack_4`, `unpack_8`, `unpack_16`, `unpack_32`, `unpack_64`, `unpack_le16`, `unpack_17_to_32`, `unpack_33_to_64`), while marking outer dispatchers with controlled `#[inline]`. Shrinks caller stack frames in unoptimized debug builds from megabytes down to under 100 bytes, completely eliminating stack overflow hazards on default 512KB test runner threads while retaining peak release performance.

- **Global Unrolling, Array Construction, and Bit-Width Dispatch Macros (`arr_8!`, `unroll_8!`, `write_8!`, `write_4!`, `match_pack_23!`)**:
  Eliminates repetitive manual index offset sequences and boilerplate match blocks. The unified macro suite provides compile-time 8-element loop unrolling (`arr_8!`, `unroll_8!`), pre-binds destination pointers to eliminate duplicate expression evaluations (`write_8!`, `write_4!`), and collapses 23-arm bit-width matches into single clean calls (`match_pack_23!`), removing 120+ lines of duplicate code while preserving full compiler inlining.

- **Kernel Single-Instruction Wide Loads (16-bit / 32-bit / 128-bit Loads)**:
  Eliminates per-element branch decisions and memory load contention in `unpack_2`, `unpack_4`, and `unpack_16`. `unpack_2` loads all 8 2-bit values via a single `u16` load and pure bitshifts; `unpack_4` loads all 8 4-bit values via a single `u32` load without slice creation overhead; `unpack_16` merges 8 separate 16-bit reads into a single 128-bit wide load (`u128`), slashing load port pressure by 87.5% and accelerating decompression throughput to **30.34 GB/s**.

- **Vectorized 0-bit Constant Block Memory Expansion**:
  Replaces element-by-element pointer writes in `ForConsumer::consume_zeros` with an 8-way unrolled store loop (`write_8!`), allowing compilers to emit native AVX2 / NEON broadcast store instructions and elevating constant dataset decompression (e.g. `gov30`, `gov31`, `gov40`) to **90 ~ 93 GB/s**.

- **Delta Prefix-Sum Critical Path Dependency Chain Reduction**:
  Decouples the running accumulator `curr` from internal 8-element delta sum reduction in `AlpDeltaConsumer`, cutting loop-carried dependency chain latency from 2 cycles down to 1 cycle. Maximizes instruction-level parallelism (ILP) and keeps delta decompression throughput at a steady **18 ~ 27 GB/s**.

- **Zero-Width Frame Short-Circuit & Zero-Payload Recurrence**:
  Eliminates dispatch and tree reduction overhead for 0-bit width blocks inspired by graupel bitpacking architecture. When differential bit-width is 0: if the base delta is 0 (steady constant run), broadcasts the scalar value directly to memory; if base delta is non-zero (stationary linear sequence), evaluates direct register additions, completely bypassing bit-unpack dispatch and prefix-sum trees to saturate memory bus bandwidth at **80+ GB/s**.

- **Zero-Base Delta Specialized Consumer & Fill-Forward Smoothing**:
  Eliminates redundant step-vector loads for monotonic sequences and removes delta spike explosions. Specializes `AlpDeltaZeroMinConsumer` for `min_delta == 0` streams, removing all step maintenance and up to 10 intermediate integer additions per 8 elements. During encoding rescan, applies fill-forward replacement so exceptions inherit the prior valid integer, ensuring differential residuals are strictly 0 and preventing artificial jump spikes.

- **Raw Pointer Uninitialized Memory Soundness & UB Elimination**:
  Switches entirely to raw pointer reservation and in-place writes in `decompress_into`, `bitunpack_u64_raw`, and `expand_repeats`. Safely updates buffer lengths only after elements are initialized, strictly eliminating undefined behavior (UB) from constructing uninitialized slice references (`&mut [T]`) and passing all strict modern Rust memory soundness audits.
