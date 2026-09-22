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
| **fastalp (Rust)** | Specialized Float | **23.8 GB/s** | **1.26x vs C++** | **1.6 GB/s (2.16x faster)** | **6.2 GB/s** | **1.22x vs C++** | **7.18x** |
| **C++ ALP** (Paper Reference) | Specialized Float | **18.8 GB/s** | Baseline (1.0x) | **0.7 GB/s** | **5.1 GB/s** | Baseline (1.0x) | **5.65x** |
| Pcodec (pco) | Specialized Float | **1.8 GB/s** | 0.10x (10.3x slower) | **0.2 GB/s** | — | — | **8.81x** |
| Zstd (level 3) | General Byte | **1.4 GB/s** | 0.08x (13.2x slower) | **0.5 GB/s** | — | — | **6.07x** |
| LZ4 (lz4_flex) | General Byte | **5.0 GB/s** | 0.26x (3.8x slower) | **2.0 GB/s** | — | — | **3.89x** |
| Snappy (snap) | General Byte | **4.6 GB/s** | 0.25x (4.1x slower) | **2.5 GB/s** | — | — | **3.05x** |
| Chimp128 (ts+val) | Specialized Float | **1.0 GB/s** | 0.05x (19.1x slower) | **1.3 GB/s** | — | — | **5.05x** |
| Gorilla (ts+val) | Specialized Float | **1.2 GB/s** | 0.06x (15.7x slower) | **1.9 GB/s** | — | — | **4.41x** |

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
| **Benchmark Decompression Throughput** | GeoMean **23.8 GB/s**<br>ArithMean **28.62 GB/s** | GeoMean 18.8 GB/s<br>ArithMean 18.96 GB/s | GeoMean **1.26x vs C++**<br>ArithMean **1.51x vs C++** | Evaluated across all 37 datasets with SIMD fusion and wide unaligned loads |
| **Pure Encoding Throughput (No Sampling)** | GeoMean **6.2 GB/s**<br>ArithMean **7.06 GB/s** | GeoMean 5.1 GB/s<br>ArithMean 5.46 GB/s | GeoMean **1.22x vs C++**<br>ArithMean **1.29x vs C++** | Bypasses parameter sampling; tests pure float-to-int transform and dense bitpacking (Paper benchmark scope) |
| **End-to-End Compression (w/ Sampling)** | GeoMean **1.6 GB/s**<br>ArithMean **2.21 GB/s** | GeoMean 0.7 GB/s<br>ArithMean 0.73 GB/s | GeoMean **2.16x vs C++**<br>ArithMean **3.02x vs C++** | Real-world ingestion pipeline; 3-tier cascade pruning eliminates exhaustive search overhead |
| **Stateful Streaming Cache (Parameter Reuse)** | **15 ~ 24+ GB/s** | — | **Steady-State Stream** | Caches derived `(exp, fac)` models across consecutive 1024-element blocks via `Encoder` |
| **Compression Ratio** | GeoMean **7.18x**<br>Total Bytes **3.65x** | GeoMean 5.65x<br>Total Bytes 3.14x | GeoMean **+27% higher**<br>Total Bytes **+16% higher** | Evaluated across all 37 datasets; Delta-ALP and division reconstruction significantly reduce dynamic bit-widths |

---

### Industrial Scenario Micro-Benchmarks

| Business Scenario Slice | Dataset Scale | fastalp<br>(Decomp / Comp / Ratio) | C++ ALP<br>(Decomp / Comp / Ratio) | Pcodec<br>(Decomp / Comp / Ratio) | Baseline Codec<br>(Decomp / Comp / Ratio) |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Decimal Environmental & Hydrology IoT** | 11 sets (11,264 pts) | **21.6 GB/s**<br>**2.9 GB/s**<br>**3.48x** | 19.5 GB/s<br>0.7 GB/s<br>3.16x | 1.65 GB/s<br>0.2 GB/s<br>3.30x | LZ4:<br>7.4 GB/s<br>1.8 GB/s<br>1.78x |
| **Quantitative Trading & Asset Quotes** | 7 sets (7,168 pts) | **20.6 GB/s**<br>**2.9 GB/s**<br>**4.86x** | 19.1 GB/s<br>0.8 GB/s<br>3.82x | 1.56 GB/s<br>0.2 GB/s<br>4.17x | Snappy:<br>14.0 GB/s<br>3.9 GB/s<br>2.22x |
| **Geospatial & GPS Trajectory Tracking** | 5 sets (5,120 pts) | **16.9 GB/s**<br>**0.8 GB/s**<br>**2.12x** | 16.2 GB/s<br>0.7 GB/s<br>1.78x | 2.01 GB/s<br>0.2 GB/s<br>2.27x | Snappy:<br>31.9 GB/s<br>8.2 GB/s<br>1.40x |
| **Healthcare Claims & Pharma Pricing** | 5 sets (5,120 pts) | **23.1 GB/s**<br>**1.6 GB/s**<br>**2.15x** | 18.7 GB/s<br>0.8 GB/s<br>2.19x | 2.04 GB/s<br>0.2 GB/s<br>2.16x | Zstd:<br>1.0 GB/s<br>0.4 GB/s<br>1.99x |
| **Public Demographics & Civic Economics** | 6 sets (6,144 pts) | **67.7 GB/s**<br>**2.1 GB/s**<br>**11.04x** | 20.6 GB/s<br>0.7 GB/s<br>8.39x | 2.70 GB/s<br>0.3 GB/s<br>10.07x | Zstd:<br>5.9 GB/s<br>2.1 GB/s<br>13.16x |
| **River Discharge, Marine Tides & Storage** | 3 sets (3,072 pts) | **23.5 GB/s**<br>**1.5 GB/s**<br>**10.26x** | 18.7 GB/s<br>0.8 GB/s<br>4.47x | 2.50 GB/s<br>0.3 GB/s<br>21.04x | Zstd:<br>2.1 GB/s<br>1.2 GB/s<br>10.21x |

### C++ ALP Benchmark Methodology & Calibration

- **Official C++ ALP Implementation**: [cwida/ALP](https://github.com/cwida/ALP)
- **Official C++ ALP Benchmark Code**: [cwida/ALP (bench_alp_encode.cpp)](https://github.com/cwida/ALP/blob/main/publication/source_code/bench_speed/bench_alp_encode.cpp)
- **Unified Methodology Notes**:
  - **100% Unaltered Core Logic**: Evaluated directly against the original core algorithm (`include/` directory) without modification, preserving the authors' SIMD and inverse mapping logic.
  - **End-to-End Pipeline vs Pure Kernel Throughput**:
    - **Pure Kernel (Paper methodology, C++ 5.1 GB/s vs fastalp 6.2 GB/s)**:<br>
      C++ ALP official benchmark calls model initialization outside the measurement loop, assuming optimal exponents and factors are known beforehand, achieving **5.1 GB/s** geometric mean throughput (arithmetic mean 5.46 GB/s); under the exact same benchmark conditions, fastalp achieves **6.2 GB/s** pure encoding throughput (**1.22x speedup vs C++**; arithmetic mean **7.06 GB/s**, **1.29x vs C++**).
    - **End-to-End Compression (Real-world metric, C++ 0.7 GB/s vs fastalp 1.6 GB/s)**:<br>
      In real-world time-series ingestion, incoming blocks require adaptive parameter sampling. When sampling is measured within the timing loop, C++ ALP unpruned exhaustive search accounts for >80% of execution time, yielding an end-to-end throughput of **0.7 GB/s** (arithmetic mean 0.73 GB/s); fastalp performs complete end-to-end compression including adaptive parameter sampling from scratch, achieving **1.6 GB/s** geometric mean end-to-end throughput (**2.16x faster than C++ ALP**; arithmetic mean **2.21 GB/s**, **3.02x vs C++**); when hitting stateful parameter cache, pure kernel throughput reaches **15 ~ 24+ GB/s**.
    - **Decompression Throughput (GeoMean 23.8 GB/s vs 18.8 GB/s)**:<br>
      Utilizing branchless SIMD register pipelines and L1D stack LUTs, fastalp attains **23.8 GB/s** geometric mean decompression throughput, outperforming C++ ALP **18.8 GB/s** (**1.26x faster**; arithmetic mean **28.62 GB/s** vs **18.96 GB/s**, **1.51x faster**).
  - **Full 37 Dataset Coverage & 100% Reproducibility**:
    - Evaluated across all 31 public datasets from the original C++ ALP paper plus 6 real-world physical observation time-series (NOAA tides, USGS hydrology and global meteorology), totaling 37 real benchmarks.
    - All algorithms are evaluated across all 37 datasets using Geometric Mean without sampling bias. fastalp achieves an overall geometric mean compression ratio of **7.18x** (compared to C++ ALP **5.65x**).

### Comprehensive Dataset Coverage & Sources

Evaluated on all 31 public datasets from the original ALP paper plus 6 real-world physical observation time-series (37 benchmarks in total, 100% real physical observations across 6 domains):

- **IoT & Environmental Sensors (11 datasets)**: [`neon_pm10_dust`](https://doi.org/10.48443/4E6X-V373), [`neon_dew_point_temp`](https://doi.org/10.48443/Z99V-0502), [`neon_air_pressure`](https://doi.org/10.48443/RXR7-PP32), [`neon_wind_dir`](https://doi.org/10.48443/S9YA-ZC81), [`neon_bio_temp_c`](https://doi.org/10.48443/JNWY-B177), [`basel_temp_f`](https://www.meteoblue.com/en/weather/archive/export/basel_switzerland), [`basel_wind_f`](https://www.meteoblue.com/en/weather/archive/export/basel_switzerland), [`city_temperature_f`](https://www.kaggle.com/datasets/sudalairajkumar/daily-temperature-of-major-cities), [`air_sensor_f`](https://github.com/cwida/public_bi_benchmark), [`arade4`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/Arade/), [`isd_air_temperature`](https://www.ncei.noaa.gov/pub/data/noaa/isd-lite/).
- **Quantitative Finance & Trading (7 datasets)**: [`stocks_usa_c`](https://zenodo.org/record/3886895), [`stocks_de`](https://zenodo.org/record/3886895), [`stocks_uk`](https://zenodo.org/record/3886895), [`bitcoin_f`](https://raw.githubusercontent.com/influxdata/influxdb2-sample-data/master/bitcoin-price-data/bitcoin-historical-annotated.csv), [`bitcoin_transactions_f`](https://gz.blockchair.com/bitcoin/transactions/), [`food_prices`](https://data.humdata.org/dataset/wfp-food-prices), [`isd_sea_pressure`](https://www.ncei.noaa.gov/pub/data/noaa/isd-lite/).
- **Geographic Mapping & Trajectories (5 datasets)**: [`poi_lat`](https://www.kaggle.com/datasets/ehallmar/points-of-interest-poi-database), [`poi_lon`](https://www.kaggle.com/datasets/ehallmar/points-of-interest-poi-database), [`bird_migration_f`](https://github.com/influxdata/influxdb2-sample-data/blob/master/bird-migration-data/bird-migration.csv), [`nyc29`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/NYC/), [`usgs_gage_height`](https://waterdata.usgs.gov/monitoring-location/06934500/).
- **Healthcare & Public Assistance (5 datasets)**: [`medicare1`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/Medicare3/), [`medicare9`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/Medicare3/), [`cms1`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CMSprovider/), [`cms9`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CMSprovider/), [`cms25`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CMSprovider/).
- **Government & Macroeconomics (6 datasets)**: [`gov10`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CommonGovernment/), [`gov26`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CommonGovernment/), [`gov30`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CommonGovernment/), [`gov31`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CommonGovernment/), [`gov40`](https://homepages.cwi.nl/~boncz/PublicBIbenchmark/CommonGovernment/), [`noaa_water_sigma`](https://tidesandcurrents.noaa.gov/waterlevels.html?id=8724580).
- **Hardware Storage & Continuous Hydrology (3 datasets)**: [`ssd_hdd_benchmarks_f`](https://www.kaggle.com/datasets/alanjo/ssd-and-hdd-benchmarks), [`usgs_river_discharge`](https://waterdata.usgs.gov/monitoring-location/01646500/), [`noaa_water_level`](https://tidesandcurrents.noaa.gov/waterlevels.html?id=8443970).
