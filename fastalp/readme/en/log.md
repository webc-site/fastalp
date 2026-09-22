## Changelog

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