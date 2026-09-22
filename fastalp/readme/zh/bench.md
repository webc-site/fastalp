## 性能评测与多算法对比

### 测试环境与编译配置

所有基准测试均在同一物理机上执行并进行同机对比测试：

- **芯片: Apple M2 Max (12 核)**<br>
- **环境: macOS 26.5.1 ｜ 工具链: Rust 1.100.0-nightly / Clang (-O3)**<br>
- **内存分配器**: `mimalloc 0.1.52`<br>
- **基准测试框架**: Rust `divan 0.1.21` 微基准套件 vs C++ `std::chrono::high_resolution_clock`（稳态中位数采样）

### 主流浮点与时序压缩算法同机横向对比

在完全相同的测试硬件与全量 37 项数据负载下，同机全量对比业界主流浮点与时序压缩库（统一采用全部 37 项数据集实测几何均值，与评测图表完全一致）：

| 算法名称 | 算法分类 | 解压吞吐 (几何均值) | 相对 C++ 解压 | 端到端压缩 (几何均值) | 压缩纯编码吞吐 (几何均值) | 相对 C++ 纯编码 | 几何平均压缩比 |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **fastalp (Rust)** | 浮点专用 | **24.8 GB/s** | **较 C++ 快 1.35x** | **1.6 GB/s (快 2.07x)** | **6.4 GB/s** | **较 C++ 快 1.20x** | **7.18x** |
| **C++ ALP** (原版实现) | 浮点专用 | **18.3 GB/s** | 基准 (1.0x) | **0.8 GB/s** | **5.3 GB/s** | 基准 (1.0x) | **5.65x** |
| Pcodec (pco) | 浮点专用 | **1.8 GB/s** | 0.10x (慢 10.0x) | **0.2 GB/s** | — | — | **8.81x** |
| Zstd (level 3) | 通用字节 | **1.4 GB/s** | 0.08x (慢 12.8x) | **0.5 GB/s** | — | — | **6.07x** |
| LZ4 (lz4_flex) | 通用字节 | **5.0 GB/s** | 0.27x (慢 3.7x) | **2.0 GB/s** | — | — | **3.89x** |
| Snappy (snap) | 通用字节 | **4.6 GB/s** | 0.25x (慢 4.0x) | **2.5 GB/s** | — | — | **3.05x** |
| Chimp128 (ts+val) | 浮点专用 | **1.0 GB/s** | 0.05x (慢 18.6x) | **1.3 GB/s** | — | — | **5.05x** |
| Gorilla (ts+val) | 浮点专用 | **1.2 GB/s** | 0.07x (慢 15.3x) | **1.9 GB/s** | — | — | **4.41x** |

---

### 压缩纯编码与流式参数复用进阶对比

在时序浮点压缩评测中，针对特定运行形态与写入模式提供进阶吞吐评测：

- **压缩纯编码（不含采样）**：<br>
  原论文官方测试代码（`bench_alp_encode.cpp`）在计时循环外部预先执行 `init`，假设已获知最佳指数与因子，仅测量跳过采样后的纯浮点变换与密集位打包内核速度。
- **状态化流式参数缓存**：<br>
  在平稳连续时序流写入时，跨 1024 满块复用已推导的模型参数，跳过重复采样开销。

同机 37 项全量数据集实测对照（提供几何均值与算术均值双口径详细对比）：

| 评测维度 / 运行模式 | fastalp (Rust) | C++ ALP (官方原版) | 相对 C++ 提升幅度 | 评测机制与工业场景说明 |
| :--- | :---: | :---: | :---: | :--- |
| **全量基准解压吞吐** | 几何均值 **24.8 GB/s**<br>算术均值 **29.45 GB/s** | 几何均值 18.3 GB/s<br>算术均值 18.74 GB/s | 几何均值 **快 1.35x**<br>算术均值 **快 1.57x** | 37 项全量数据集实测，单趟差分融合与宽位加载加速 |
| **压缩纯编码吞吐 (不含采样)** | 几何均值 **6.4 GB/s**<br>算术均值 **7.31 GB/s** | 几何均值 5.3 GB/s<br>算术均值 5.70 GB/s | 几何均值 **快 1.20x**<br>算术均值 **快 1.28x** | 预置或缓存模型参数，跳过采样探测，纯浮点整型变换与位打包内核（原论文测试代码口径） |
| **端到端压缩吞吐 (含自适应采样)** | 几何均值 **1.6 GB/s**<br>算术均值 **2.17 GB/s** | 几何均值 0.8 GB/s<br>算术均值 0.76 GB/s | 几何均值 **快 2.07x**<br>算术均值 **快 2.85x** | 真实时序全流程写入口径，三级级联剪枝规避暴力穷举开销 |
| **状态化连续流式吞吐 (参数缓存)** | **15 ~ 24+ GB/s** | — | **平稳流式写入** | 跨 1024 满块复用已推导的模型参数，平稳时序跳过采样直接推导 |
| **综合压缩比** | 几何均值 **7.18x**<br>总字节加权 **3.65x** | 几何均值 5.65x<br>总字节加权 3.14x | 几何均值 **领先 27%**<br>总字节加权 **领先 16%** | 37 项公开与工业基准实测，Delta 差分与除法重构有效收窄动态位宽 |

---

### 典型工业场景微基准细分实测

| 业务场景切片 | 样本规模 | fastalp<br>(解压 / 压缩 / 压缩比) | C++ ALP<br>(解压 / 压缩 / 压缩比) | Pcodec<br>(解压 / 压缩 / 压缩比) | 对照算法<br>(解压 / 压缩 / 压缩比) |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **十进制环境与气象水文传感** | 11 组 (11,264 点) | **22.7 GB/s**<br>**2.9 GB/s**<br>**3.48x** | 17.7 GB/s<br>0.8 GB/s<br>3.16x | 1.65 GB/s<br>0.2 GB/s<br>3.30x | LZ4:<br>7.4 GB/s<br>1.8 GB/s<br>1.78x |
| **高频量化金融交易与资产行情** | 7 组 (7,168 点) | **21.4 GB/s**<br>**2.9 GB/s**<br>**4.86x** | 19.9 GB/s<br>0.8 GB/s<br>3.82x | 1.56 GB/s<br>0.2 GB/s<br>4.17x | Snappy:<br>14.0 GB/s<br>3.9 GB/s<br>2.22x |
| **地理空间高精测绘与轨迹跟踪** | 5 组 (5,120 点) | **17.4 GB/s**<br>**0.8 GB/s**<br>**2.12x** | 15.3 GB/s<br>0.7 GB/s<br>1.78x | 2.01 GB/s<br>0.2 GB/s<br>2.27x | Snappy:<br>31.9 GB/s<br>8.2 GB/s<br>1.40x |
| **医疗社保理赔与公共卫生处方** | 5 组 (5,120 点) | **25.0 GB/s**<br>**1.7 GB/s**<br>**2.15x** | 18.5 GB/s<br>0.8 GB/s<br>2.19x | 2.04 GB/s<br>0.2 GB/s<br>2.16x | Zstd:<br>1.0 GB/s<br>0.4 GB/s<br>1.99x |
| **公共政务民生与宏观统计普查** | 6 组 (6,144 点) | **75.7 GB/s**<br>**2.1 GB/s**<br>**10.95x** | 21.6 GB/s<br>0.8 GB/s<br>9.50x | 2.81 GB/s<br>0.3 GB/s<br>8.57x | Zstd:<br>3.2 GB/s<br>2.1 GB/s<br>11.24x |
| **连续河流流量、海洋潮位与存储指标** | 3 组 (3,072 点) | **24.9 GB/s**<br>**1.3 GB/s**<br>**10.54x** | 20.4 GB/s<br>0.8 GB/s<br>4.65x | 2.41 GB/s<br>0.3 GB/s<br>25.86x | Zstd:<br>6.4 GB/s<br>1.5 GB/s<br>13.13x |

### C++ ALP 测试机制与统计口径说明

- **C++ ALP 官方算法实现**：[cwida/ALP](https://github.com/cwida/ALP)
- **C++ ALP 官方原版测试代码**：[cwida/ALP (bench_alp_encode.cpp)](https://github.com/cwida/ALP/blob/main/publication/source_code/bench_speed/bench_alp_encode.cpp)
- **统计口径统一与测试机制说明**：
  - **核心算法保持官方原貌**：完全基于 C++ ALP 官方核心算法（`include/` 目录），保留官方实现的向量化与十进制反向映射逻辑。
  - **端到端全流程与纯编码内核的双重口径统一**：
    - **压缩纯编码（不含采样，原论文测试口径，C++ 5.3 GB/s vs fastalp 6.4 GB/s）**：<br>
      C++ ALP 官方原版测试代码在测速计时循环外部调用了模型初始化，假设已预先获知最佳指数与因子，仅测量跳过采样后的纯浮点变换与位打包内核速度，在同机测得几何平均吞吐为 **5.3 GB/s**（算术均值 5.70 GB/s）；在此相同基准下，fastalp 压缩纯编码吞吐（不含采样）几何均值达到 **6.4 GB/s**（较 C++ 快 **1.20x**；算术均值达到 **7.31 GB/s**，较 C++ 快 **1.28x**）。
    - **端到端全量流水线（真实写入口径，C++ 0.8 GB/s vs fastalp 1.6 GB/s）**：<br>
      在真实时序写入时，新数据块无法预知模型参数，必须经历采样分析。为了公平衡量工程实际性能，我们将采样分析纳入计时循环。由于 C++ ALP 采用无剪枝的暴力穷举，采样阶段占用了 80% 以上的时间，其实际端到端几何平均吞吐测得为 **0.8 GB/s**（算术均值 0.76 GB/s）；fastalp 凭借三级级联剪枝机制（纯十进制早停、4/16 样本快筛、高熵早停），端到端压缩几何平均吞吐达到 **1.6 GB/s**（较 C++ 提速 **2.07x**；算术均值达到 **2.17 GB/s**，较 C++ 提速 **2.85x**）；在平稳流式命中状态化参数缓存时，纯编码吞吐可达 **15 ~ 24+ GB/s**。
    - **解压性能（几何均值 24.8 GB/s vs 18.3 GB/s）**：<br>
      得益于纯寄存器 SIMD 展开与 L1D 局部查表，fastalp 解压几何平均吞吐达到 **24.8 GB/s**，较 C++ ALP 的 **18.3 GB/s** 提速 **1.35x**（算术均值达到 **29.45 GB/s**，较 C++ 的 **18.74 GB/s** 提速 **1.57x**）。
  - **37 项数据集全量无偏实测与一键复现**：
    - 评测涵盖 C++ ALP 官方论文收录的全部 31 项时序与列存数据集，并补充 6 项来自物理观测真实公开时序（NOAA 潮位、USGS 水文与全球气象观测），共 37 项全真实时序基准。
    - 所有算法统一采用全量 37 项评测数据计算几何平均值，杜绝采样偏倚。fastalp 综合几何平均压缩比达到 **7.18x**（C++ ALP 为 **5.65x**）。

### 评测数据集全景与公开数据源

本评测采用 ALP 官方论文收录的全部 31 个公开时序与列存测试集，以及来自真实物理观测的 6 项公开时序观测样本（共 37 项基准，100% 真实观测时序），覆盖 6 大业务领域：

- **物联网与环境传感（11 项）**
  - `neon_pm10_dust`：PM10 悬浮微粒粉尘浓度传感（μg/m³）· [NEON 官方生态观测网络](https://doi.org/10.48443/4E6X-V373)
  - `neon_dew_point_temp`：气象露点温度连续观测时序（°C）· [NEON 官方生态观测网络](https://doi.org/10.48443/Z99V-0502)
  - `neon_air_pressure`：大气海平面连续气压传感（kPa）· [NEON 官方生态观测网络](https://doi.org/10.48443/RXR7-PP32)
  - `neon_wind_dir`：超声波气象风向角度传感（0-360°）· [NEON 官方生态观测网络](https://doi.org/10.48443/S9YA-ZC81)
  - `neon_bio_temp_c`：红外土壤地表温度物理遥测（°C）· [NEON 官方生态观测网络](https://doi.org/10.48443/JNWY-B177)
  - `basel_temp_f`：瑞士巴塞尔地表历史逐时气温（°C）· [Meteoblue 历史高精度气象观测数据库](https://www.meteoblue.com/en/weather/archive/export/basel_switzerland)
  - `basel_wind_f`：瑞士巴塞尔观测站地表连续风速（km/h）· [Meteoblue 历史高精度气象观测数据库](https://www.meteoblue.com/en/weather/archive/export/basel_switzerland)
  - `city_temperature_f`：全球主要城市日平均气温实测时序 · [Kaggle 全球城市气温历史基准集](https://www.kaggle.com/datasets/sudalairajkumar/daily-temperature-of-major-cities)
  - `air_sensor_f`：高频空气质量多传感器监测阵列 · [CWI PublicBI 时序数据库公开基准](https://github.com/cwida/public_bi_benchmark)
  - `arade4`：葡萄牙 Arade 水文站水尺高度监控 · [CWI PublicBI Arade 水文站观测数据](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/Arade/)
  - `isd_air_temperature`：英国伦敦希思罗站逐时地表气温（1024 点）· [NOAA NCEI 全球地表逐时气象观测数据库 (ISD-Lite)](https://www.ncei.noaa.gov/pub/data/noaa/isd-lite/)

- **量化金融与资产行情（7 项）**
  - `stocks_usa_c`：美股微秒级高频订单簿成交价时序 · [Zenodo 全球金融量化交易公开集](https://zenodo.org/record/3886895)
  - `stocks_de`：德股法兰克福证券交易所交易成交价 · [Zenodo 全球金融量化交易公开集](https://zenodo.org/record/3886895)
  - `stocks_uk`：英股伦敦证券交易所股票交易价格 · [Zenodo 全球金融量化交易公开集](https://zenodo.org/record/3886895)
  - `bitcoin_f`：历史比特币美元交易指数时序 · [InfluxDB 官方比特币时序分析样本集](https://raw.githubusercontent.com/influxdata/influxdb2-sample-data/master/bitcoin-price-data/bitcoin-historical-annotated.csv)
  - `bitcoin_transactions_f`：比特币区块链主网微秒级单笔转账金额 · [Blockchair 比特币主链转账流水](https://gz.blockchair.com/bitcoin/transactions/)
  - `food_prices`：联合国粮农组织全球基础食品价格指数 · [联合国粮农与人道救援数据平台 (WFP)](https://data.humdata.org/dataset/wfp-food-prices)
  - `isd_sea_pressure`：英国伦敦希思罗站海平面气压实测（1024 点）· [NOAA NCEI 全球地表逐时气象观测数据库 (ISD-Lite)](https://www.ncei.noaa.gov/pub/data/noaa/isd-lite/)

- **地理测绘与轨迹跟踪（5 项）**
  - `poi_lat`：全球兴趣点高精度地理纬度坐标 · [Kaggle POI 全球地理空间数据库](https://www.kaggle.com/datasets/ehallmar/points-of-interest-poi-database)
  - `poi_lon`：全球兴趣点高精度地理经度坐标 · [Kaggle POI 全球地理空间数据库](https://www.kaggle.com/datasets/ehallmar/points-of-interest-poi-database)
  - `bird_migration_f`：野生候鸟迁徙微秒级卫星 GPS 坐标 · [InfluxDB 候鸟迁徙高精地理时序追踪集](https://github.com/influxdata/influxdb2-sample-data/blob/master/bird-migration-data/bird-migration.csv)
  - `nyc29`：纽约出租车连续营运 GPS 轨迹与计程 · [CWI PublicBI NYC 出租车地理时序数据库](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/NYC/)
  - `usgs_gage_height`：密苏里河赫曼站水尺测深高程（1024 点）· [USGS 国家水文信息系统 (NWIS Site 06934500)](https://waterdata.usgs.gov/monitoring-location/06934500/)

- **医疗社保与公共卫生（5 项）**
  - `medicare1`：门诊医疗保险理赔结算账单流水 · [CWI PublicBI Medicare 医疗卫生统计集](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/Medicare3/)
  - `medicare9`：专科就诊补贴与报销费用时序 · [CWI PublicBI Medicare 医疗卫生统计集](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/Medicare3/)
  - `cms1`：医疗保险供应商结算明细记录 · [CWI PublicBI CMSProvider 医疗保险数据库](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CMSprovider/)
  - `cms9`：专科处方药品报销结算价格流水 · [CWI PublicBI CMSProvider 医疗保险数据库](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CMSprovider/)
  - `cms25`：医疗设备使用与专科诊疗收费项目 · [CWI PublicBI CMSProvider 医疗保险数据库](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CMSprovider/)

- **公共政务与宏观经济（5 项）**
  - `gov10`：财政预算与公共支出明细统计指标 · [CWI PublicBI CommonGovernment 统计集](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CommonGovernment/)
  - `gov26`：国家人口普查低熵常数序列流 · [CWI PublicBI CommonGovernment 统计集](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CommonGovernment/)
  - `gov30`：宏观经济运行指标与财政综合统计 · [CWI PublicBI CommonGovernment 统计集](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CommonGovernment/)
  - `gov31`：财政转移支付与地区扶持资金时序 · [CWI PublicBI CommonGovernment 统计集](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CommonGovernment/)
  - `gov40`：市政公用管网工程高精测绘与统计 · [CWI PublicBI CommonGovernment 统计集](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CommonGovernment/)

- **硬件存储与连续水文波形（4 项）**
  - `ssd_hdd_benchmarks_f`：固态硬盘与机械硬盘连续 I/O 吞吐基准 · [Kaggle 存储设备吞吐实测数据库](https://www.kaggle.com/datasets/alanjo/ssd-and-hdd-benchmarks)
  - `usgs_river_discharge`：波托马克河华盛顿站连续河流流量（1024 点）· [USGS 国家水文信息系统 (NWIS Site 01646500)](https://waterdata.usgs.gov/monitoring-location/01646500/)
  - `noaa_water_level`：波士顿港 6 分钟连续水尺水位观测（1024 点）· [NOAA 潮汐与水流观测数据系统 (CO-OPS 8443970)](https://tidesandcurrents.noaa.gov/waterlevels.html?id=8443970)
  - `noaa_water_sigma`：基韦斯特潮位观测误差标准差（1024 点）· [NOAA 潮汐与水流观测数据系统 (CO-OPS 8724580)](https://tidesandcurrents.noaa.gov/waterlevels.html?id=8724580)
