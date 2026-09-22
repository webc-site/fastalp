## 更新日志

### v0.1.42

- **独立仓库迁移与工作区标准化**：<br>
  从主项目解耦迁移至独立仓库 `webc-site/fastalp`；规范包元数据、文档模板与社区链接；统一 workspace 配置。

### v0.1.40

- **位解包架构解耦与单趟消费器融合**：<br>
  将巨型位解包逻辑重构解耦为 `consumer.rs`、`decoder.rs`、`kernel.rs` 与顶层安全调度；抽象 `AlpConsumer` 消费流范式，实现 Delta 一阶差分寄存器树状前缀和累加与浮点重构单趟流式输出，省去 8KB 临时栈缓冲往返拷贝与双重遍历。
- **打包与解包内核宽位加载与指令流水线加速**：<br>
  在 `unpack_2`、`unpack_4` 与 `unpack_16` 中淘汰逐元素分支与切片读取开销，分别升级为单次 `u16`、`u32` 与 `u128` 宽位加载与纯位移展开；在 `consume_zeros` 中实现基于 `write_8!` 的常数块向量化展开；在 `AlpDeltaConsumer` 中解耦累加器关键路径，将循环依赖延迟由 2 周期缩短至 1 周期。
- **全局展开与位宽分发宏体系**：<br>
  引入 `src/macros.rs` 全局宏体系（`arr_8!`、`unroll_8!`、`write_8!`、`write_4!`、`match_pack_23!`），精简 23 分支打包样板代码，预绑定裸指针消除重复计算，消除 120+ 行冗余代码。
- **未初始化内存 Soundness 与 Clippy 零告警保障**：<br>
  在 `decompress_into`、`bitunpack_u64_raw` 与 `expand_repeats` 中全面采用裸指针预留与原地写入，安全更新长度，完全消除在未初始化内存上构造切片引用的未定义行为（UB）隐患；规范全部绝对路径导入，确保 `-W clippy::absolute_paths` 零告警通过。