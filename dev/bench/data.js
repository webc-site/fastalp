window.BENCHMARK_DATA = {
  "lastUpdate": 1790048366115,
  "repoUrl": "https://github.com/webc-site/fastalp",
  "entries": {
    "FastALP Microbenchmarks": [
      {
        "commit": {
          "author": {
            "name": "x-at-01",
            "username": "x-at-01",
            "email": "x-at-01@googlegroups.com"
          },
          "committer": {
            "name": "x-at-01",
            "username": "x-at-01",
            "email": "x-at-01@googlegroups.com"
          },
          "id": "79267556508a462bd0f258450481cd44bf5ad4fb",
          "message": "ci: add benchmark workflow, step summary, and continuous regression tracking",
          "timestamp": "2026-09-09T03:37:39Z",
          "url": "https://github.com/webc-site/fastalp/commit/79267556508a462bd0f258450481cd44bf5ad4fb"
        },
        "date": 1788925779337,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "bench_compress_f32_large_batch_cached",
            "value": 22240,
            "unit": "ns",
            "extra": "11.79 GB/s"
          },
          {
            "name": "bench_compress_f32_large_batch_sampled",
            "value": 50520,
            "unit": "ns",
            "extra": "5.19 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_cached_1024",
            "value": 848.1,
            "unit": "ns",
            "extra": "4.83 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_sampled_1024",
            "value": 1490,
            "unit": "ns",
            "extra": "2.75 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_cached_1024",
            "value": 432.4,
            "unit": "ns",
            "extra": "9.47 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_sampled_1024",
            "value": 2645,
            "unit": "ns",
            "extra": "1.55 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_cached_1024",
            "value": 494.4,
            "unit": "ns",
            "extra": "16.57 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_sampled_1024",
            "value": 2434,
            "unit": "ns",
            "extra": "3.37 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_cached",
            "value": 49530,
            "unit": "ns",
            "extra": "10.59 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_sampled",
            "value": 77950,
            "unit": "ns",
            "extra": "6.73 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_cached_1024",
            "value": 1250,
            "unit": "ns",
            "extra": "6.55 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_sampled_1024",
            "value": 2550,
            "unit": "ns",
            "extra": "3.21 GB/s"
          },
          {
            "name": "bench_compress_f64_random_cached_1024",
            "value": 3067,
            "unit": "ns",
            "extra": "2.67 GB/s"
          },
          {
            "name": "bench_compress_f64_random_sampled_1024",
            "value": 27130,
            "unit": "ns",
            "extra": "0.3 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_cached_1024",
            "value": 622.3,
            "unit": "ns",
            "extra": "13.16 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_sampled_1024",
            "value": 1610,
            "unit": "ns",
            "extra": "5.09 GB/s"
          },
          {
            "name": "bench_decompress_f32_large_batch",
            "value": 14600,
            "unit": "ns",
            "extra": "17.96 GB/s"
          },
          {
            "name": "bench_decompress_f32_ramp_1024",
            "value": 349.5,
            "unit": "ns",
            "extra": "11.72 GB/s"
          },
          {
            "name": "bench_decompress_f32_sensor_1024",
            "value": 261.4,
            "unit": "ns",
            "extra": "15.67 GB/s"
          },
          {
            "name": "bench_decompress_f64_identical_1024",
            "value": 103.6,
            "unit": "ns",
            "extra": "79.07 GB/s"
          },
          {
            "name": "bench_decompress_f64_large_batch",
            "value": 50870,
            "unit": "ns",
            "extra": "10.31 GB/s"
          },
          {
            "name": "bench_decompress_f64_ramp_1024",
            "value": 727.3,
            "unit": "ns",
            "extra": "11.26 GB/s"
          },
          {
            "name": "bench_decompress_f64_random_1024",
            "value": 1027,
            "unit": "ns",
            "extra": "7.98 GB/s"
          },
          {
            "name": "bench_decompress_f64_sensor_1024",
            "value": 925.6,
            "unit": "ns",
            "extra": "8.85 GB/s"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "x-at-01@googlegroups.com",
            "name": "x-at-01",
            "username": "x-at-01"
          },
          "committer": {
            "email": "x-at-01@googlegroups.com",
            "name": "x-at-01",
            "username": "x-at-01"
          },
          "distinct": false,
          "id": "bf84ce73726ba9b20d154db07d7328a5ec9a0867",
          "message": "refactor: consolidate bench macros, simplify env docs, and enable serde",
          "timestamp": "2026-09-22T11:37:53+08:00",
          "tree_id": "6715a4f86ffe06ae38fee4a45fdcbd5227cd9f5b",
          "url": "https://github.com/webc-site/fastalp/commit/bf84ce73726ba9b20d154db07d7328a5ec9a0867"
        },
        "date": 1790048365145,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "bench_compress_f32_large_batch_cached",
            "value": 29160,
            "unit": "ns",
            "extra": "8.99 GB/s"
          },
          {
            "name": "bench_compress_f32_large_batch_sampled",
            "value": 81000,
            "unit": "ns",
            "extra": "3.24 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_cached_1024",
            "value": 1069,
            "unit": "ns",
            "extra": "3.83 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_sampled_1024",
            "value": 2554,
            "unit": "ns",
            "extra": "1.6 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_cached_1024",
            "value": 917.6,
            "unit": "ns",
            "extra": "4.46 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_sampled_1024",
            "value": 3646,
            "unit": "ns",
            "extra": "1.12 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_cached_1024",
            "value": 713.6,
            "unit": "ns",
            "extra": "11.48 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_sampled_1024",
            "value": 3591,
            "unit": "ns",
            "extra": "2.28 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_cached",
            "value": 126400,
            "unit": "ns",
            "extra": "4.15 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_sampled",
            "value": 132500,
            "unit": "ns",
            "extra": "3.96 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_cached_1024",
            "value": 3110,
            "unit": "ns",
            "extra": "2.63 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_sampled_1024",
            "value": 4563,
            "unit": "ns",
            "extra": "1.8 GB/s"
          },
          {
            "name": "bench_compress_f64_random_cached_1024",
            "value": 4928,
            "unit": "ns",
            "extra": "1.66 GB/s"
          },
          {
            "name": "bench_compress_f64_random_sampled_1024",
            "value": 34450,
            "unit": "ns",
            "extra": "0.24 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_cached_1024",
            "value": 2559,
            "unit": "ns",
            "extra": "3.2 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_sampled_1024",
            "value": 5054,
            "unit": "ns",
            "extra": "1.62 GB/s"
          },
          {
            "name": "bench_decompress_f32_large_batch",
            "value": 19000,
            "unit": "ns",
            "extra": "13.8 GB/s"
          },
          {
            "name": "bench_decompress_f32_ramp_1024",
            "value": 723.5,
            "unit": "ns",
            "extra": "5.66 GB/s"
          },
          {
            "name": "bench_decompress_f32_sensor_1024",
            "value": 365.1,
            "unit": "ns",
            "extra": "11.22 GB/s"
          },
          {
            "name": "bench_decompress_f64_identical_1024",
            "value": 228.8,
            "unit": "ns",
            "extra": "35.8 GB/s"
          },
          {
            "name": "bench_decompress_f64_large_batch",
            "value": 60600,
            "unit": "ns",
            "extra": "8.65 GB/s"
          },
          {
            "name": "bench_decompress_f64_ramp_1024",
            "value": 773.6,
            "unit": "ns",
            "extra": "10.59 GB/s"
          },
          {
            "name": "bench_decompress_f64_random_1024",
            "value": 1262,
            "unit": "ns",
            "extra": "6.49 GB/s"
          },
          {
            "name": "bench_decompress_f64_sensor_1024",
            "value": 996.4,
            "unit": "ns",
            "extra": "8.22 GB/s"
          }
        ]
      }
    ]
  }
}