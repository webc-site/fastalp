## Performance & Comparative Benchmarks

### Test Environment and Compiler Setup

All benchmarks were evaluated on identical hardware under equivalent conditions:

- **CPU: Apple M2 Max (12 Cores)**<br>
- **OS: macOS 26.5.1 ｜ Toolchain: Rust 1.100.0-nightly / Clang (-O3)**<br>
- **Memory Allocator**: `mimalloc 0.1.52`<br>
- **Benchmark Suite**: Rust `divan 0.1.21` micro-benchmark harness vs C++ `std::chrono::high_resolution_clock` (median steady-state sampling)

### Cross-Algorithm Benchmark Comparison

Tested against standard floating-point and time-series codecs across all 37 datasets on identical hardware (measured via Geometric Mean, fully consistent with the visual infographic):

| Codec | Category | Decomp Throughput (GeoMean) | vs C++ Decomp | End-to-End Comp (GeoMean) | Pure Kernel (GeoMean) | vs C++ Pure Kernel | GeoMean Ratio |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **fastalp (Rust)** | Specialized Float | **27.2 GB/s** | **1.43x vs C++** | **1.9 GB/s (2.45x faster)** | **6.9 GB/s** | **1.34x vs C++** | **9.65x** |
| **C++ ALP** (Paper Reference) | Specialized Float | **19.1 GB/s** | Baseline (1.0x) | **0.8 GB/s** | **5.1 GB/s** | Baseline (1.0x) | **5.93x** |
| Pcodec (pco) | Specialized Float | **1.8 GB/s** | 0.10x (10.4x slower) | **0.2 GB/s** | — | — | **8.81x** |
| Zstd (level 3) | General Byte | **1.4 GB/s** | 0.07x (13.4x slower) | **0.5 GB/s** | — | — | **6.07x** |
| LZ4 (lz4_flex) | General Byte | **5.0 GB/s** | 0.26x (3.8x slower) | **2.0 GB/s** | — | — | **3.89x** |
| Snappy (snap) | General Byte | **4.6 GB/s** | 0.24x (4.1x slower) | **2.5 GB/s** | — | — | **3.05x** |
| Chimp128 (ts+val) | Specialized Float | **1.0 GB/s** | 0.05x (19.4x slower) | **1.3 GB/s** | — | — | **5.05x** |
| Gorilla (ts+val) | Specialized Float | **1.2 GB/s** | 0.06x (16.0x slower) | **1.9 GB/s** | — | — | **4.41x** |

---

### Pure Encoding & Streaming Cache Throughput Deep Dive

In floating-point and time-series compression benchmarks, advanced modes offer specialized throughput profiles:

- **Pure Encoding (No Sampling)**:<br>
  As measured in the original C++ ALP paper benchmark (`ALP/publication/source_code/bench_speed/bench_alp_encode.cpp`), parameters are discovered outside the timed loop, evaluating only the speed of float-to-integer mapping and bitpacking.
- **Stateful Streaming Cache**:<br>
  For stationary continuous time series, reuses derived model parameters across 1024-element blocks, skipping repeated sampling.

Comprehensive 37-dataset side-by-side evaluation on identical hardware (providing both Geometric Mean and Arithmetic Mean calibrations):

| Benchmark Metric / Operational Mode | fastalp (Rust) | C++ ALP (Reference) | Speedup vs C++ | Measurement Methodology & Scope |
| :--- | :---: | :---: | :---: | :--- |
| **Benchmark Decompression Throughput** | GeoMean **27.2 GB/s**<br>ArithMean **32.19 GB/s** | GeoMean 19.1 GB/s<br>ArithMean 19.32 GB/s | GeoMean **1.43x vs C++**<br>ArithMean **1.67x vs C++** | Evaluated across all 37 datasets with SIMD fusion and wide unaligned loads |
| **Pure Encoding Throughput (No Sampling)** | GeoMean **6.9 GB/s**<br>ArithMean **8.06 GB/s** | GeoMean 5.1 GB/s<br>ArithMean 5.53 GB/s | GeoMean **1.34x vs C++**<br>ArithMean **1.46x vs C++** | Bypasses parameter sampling; tests pure float-to-int transform and dense bitpacking (Paper benchmark scope) |
| **End-to-End Compression (w/ Sampling)** | GeoMean **1.9 GB/s**<br>ArithMean **2.80 GB/s** | GeoMean 0.8 GB/s<br>ArithMean 0.77 GB/s | GeoMean **2.45x vs C++**<br>ArithMean **3.64x vs C++** | Real-world ingestion pipeline; 3-tier cascade pruning eliminates exhaustive search overhead |
| **Stateful Streaming Cache (Parameter Reuse)** | **15 ~ 24+ GB/s** | — | **Steady-State Stream** | Caches derived `(exp, fac)` models across consecutive 1024-element blocks via `Encoder` |
| **Compression Ratio** | GeoMean **9.65x**<br>Total Bytes **3.74x** | GeoMean 5.93x<br>Total Bytes 2.89x | GeoMean **+63% higher**<br>Total Bytes **+29% higher** | Evaluated across all 37 datasets; Delta-ALP and division reconstruction significantly reduce dynamic bit-widths |

---

### Industrial Scenario Micro-Benchmarks

| Business Scenario Slice | Dataset Scale | fastalp<br>(Decomp / Comp / Ratio) | C++ ALP<br>(Decomp / Comp / Ratio) | Pcodec<br>(Decomp / Comp / Ratio) | Baseline Codec<br>(Decomp / Comp / Ratio) |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Decimal Environmental & Hydrology IoT** | 11 sets (11,264 pts) | **24.3 GB/s**<br>**2.9 GB/s**<br>**3.46x** | 19.4 GB/s<br>0.8 GB/s<br>3.16x | 1.65 GB/s<br>0.2 GB/s<br>3.30x | LZ4:<br>7.4 GB/s<br>1.8 GB/s<br>1.78x |
| **Quantitative Trading & Asset Quotes** | 7 sets (7,168 pts) | **21.2 GB/s**<br>**3.0 GB/s**<br>**4.86x** | 19.8 GB/s<br>0.8 GB/s<br>3.85x | 1.56 GB/s<br>0.2 GB/s<br>4.17x | Snappy:<br>14.0 GB/s<br>3.9 GB/s<br>2.22x |
| **Geospatial & GPS Trajectory Tracking** | 5 sets (5,120 pts) | **22.0 GB/s**<br>**2.1 GB/s**<br>**2.18x** | 16.9 GB/s<br>0.7 GB/s<br>1.73x | 2.01 GB/s<br>0.2 GB/s<br>2.27x | Snappy:<br>31.9 GB/s<br>8.2 GB/s<br>1.40x |
| **Healthcare Claims & Pharma Pricing** | 5 sets (5,120 pts) | **24.6 GB/s**<br>**1.6 GB/s**<br>**2.15x** | 19.6 GB/s<br>0.8 GB/s<br>2.19x | 2.04 GB/s<br>0.2 GB/s<br>2.16x | Zstd:<br>1.0 GB/s<br>0.4 GB/s<br>1.99x |
| **Public Demographics & Civic Economics** | 6 sets (6,144 pts) | **68.4 GB/s**<br>**2.4 GB/s**<br>**10.82x** | 20.2 GB/s<br>0.8 GB/s<br>4.64x | 2.70 GB/s<br>0.3 GB/s<br>10.07x | Zstd:<br>5.9 GB/s<br>2.1 GB/s<br>13.16x |
| **Monotonic Ramp, Storage & Steady Waves** | 3 sets (3,072 pts) | **44.1 GB/s**<br>**5.5 GB/s**<br>**27.40x** | 19.4 GB/s<br>0.8 GB/s<br>2.90x | 2.50 GB/s<br>0.3 GB/s<br>21.04x | Zstd:<br>2.1 GB/s<br>1.2 GB/s<br>10.21x |

### C++ ALP Benchmark Methodology & Calibration

- **Official C++ ALP Benchmark Code**: [cwida/ALP (bench_alp_encode.cpp)](https://github.com/cwida/ALP/blob/main/publication/source_code/bench_speed/bench_alp_encode.cpp)
- **Evaluation Fork Repository**: [github.com/x-at-01/ALP](https://github.com/x-at-01/ALP) (Evaluation branches: [feat/integrate-fastalp-benchmark](https://github.com/x-at-01/ALP/tree/feat/integrate-fastalp-benchmark) / [bench/self-eval](https://github.com/x-at-01/ALP/tree/bench/self-eval))
- **Unified Methodology Notes**:
  - **100% Unaltered Core Logic**: The fork maintains the original core algorithm (`include/` directory) without modification, preserving the authors' SIMD and inverse mapping logic.
  - **End-to-End Pipeline vs Pure Kernel Throughput**:
    - **Pure Kernel (Paper methodology, C++ 5.1 GB/s vs fastalp 6.9 GB/s)**:<br>
      C++ ALP official benchmark calls model initialization outside the measurement loop, assuming optimal exponents and factors are known beforehand, achieving **5.1 GB/s** geometric mean throughput (arithmetic mean 5.53 GB/s); under the exact same benchmark conditions, fastalp achieves **6.9 GB/s** pure encoding throughput (**1.34x speedup vs C++**; arithmetic mean **8.06 GB/s**, **1.46x vs C++**).
    - **End-to-End Compression (Real-world metric, C++ 0.8 GB/s vs fastalp 1.9 GB/s)**:<br>
      In real-world time-series ingestion, incoming blocks require adaptive parameter sampling. When sampling is measured within the timing loop, C++ ALP unpruned exhaustive search accounts for >80% of execution time, yielding an end-to-end throughput of **0.8 GB/s** (arithmetic mean 0.77 GB/s); fastalp performs complete end-to-end compression including adaptive parameter sampling from scratch, achieving **1.9 GB/s** geometric mean end-to-end throughput (**2.45x faster than C++ ALP**; arithmetic mean **2.80 GB/s**, **3.64x vs C++**); when hitting stateful parameter cache, pure kernel throughput reaches **15 ~ 24+ GB/s**.
    - **Decompression Throughput (GeoMean 27.2 GB/s vs 19.1 GB/s)**:<br>
      Utilizing branchless SIMD register pipelines and L1D stack LUTs, fastalp attains **27.2 GB/s** geometric mean decompression throughput, outperforming C++ ALP **19.1 GB/s** (**1.43x faster**; arithmetic mean **32.19 GB/s** vs **19.32 GB/s**, **1.67x faster**).
  - **Full 37 Dataset Coverage & 100% Reproducibility**:
    - Supplements 6 industrial scenarios into the fork repository, enabling full 37-dataset evaluation (31 paper datasets + 6 industrial benchmarks).
    - Anyone can clone [x-at-01/ALP](https://github.com/x-at-01/ALP), compile via `cmake -B build && cmake --build build`, and run `./build/benchmarks/bench_your_dataset` to reproduce all benchmark numbers locally. Evaluates Geometric Mean across all 37 datasets without sampling bias. fastalp achieves an overall geometric mean compression ratio of **9.65x** (compared to C++ ALP **5.93x**).

### Comprehensive Dataset Coverage & Sources

Evaluated on all 31 public datasets from the original ALP paper plus 6 representative industrial benchmarks across 6 domains:

- **IoT & Environmental Sensors (11 datasets)**: `neon_pm10_dust`, `neon_dew_point_temp`, `neon_air_pressure`, `neon_wind_dir`, `neon_bio_temp_c`, `basel_temp_f`, `basel_wind_f`, `city_temperature_f`, `air_sensor_f`, `arade4`, `scene_sensor`.
- **Quantitative Finance & Trading (7 datasets)**: `stocks_usa_c`, `stocks_de`, `stocks_uk`, `bitcoin_f`, `bitcoin_transactions_f`, `food_prices`, `scene_finance`.
- **Geographic Mapping & Trajectories (5 datasets)**: `poi_lat`, `poi_lon`, `bird_migration_f`, `nyc29`, `scene_geo`.
- **Healthcare & Public Assistance (5 datasets)**: `medicare1`, `medicare9`, `cms1`, `cms9`, `cms25`.
- **Government & Macroeconomics (6 datasets)**: `gov10`, `gov26`, `gov30`, `gov31`, `gov40`, `scene_macro`.
- **Hardware Storage & Physical Waveforms (3 datasets)**: `ssd_hdd_benchmarks_f`, `scene_ramp`, `scene_steady`.
