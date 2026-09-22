window.BENCHMARK_DATA = {
  "lastUpdate": 1790054063657,
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
          "id": "cba514aa3b9fe09c9b547aa2a51f1f030d6093ca",
          "message": "perf: optimize outlier search, delta reduction tree, deduplicate capi and kernels",
          "timestamp": "2026-09-22T12:01:50+08:00",
          "tree_id": "cbff063210cf56358bda16713ca6648e26ff7a1a",
          "url": "https://github.com/webc-site/fastalp/commit/cba514aa3b9fe09c9b547aa2a51f1f030d6093ca"
        },
        "date": 1790049796539,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "bench_compress_f32_large_batch_cached",
            "value": 29070,
            "unit": "ns",
            "extra": "9.02 GB/s"
          },
          {
            "name": "bench_compress_f32_large_batch_sampled",
            "value": 61240,
            "unit": "ns",
            "extra": "4.28 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_cached_1024",
            "value": 1025,
            "unit": "ns",
            "extra": "4 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_sampled_1024",
            "value": 1912,
            "unit": "ns",
            "extra": "2.14 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_cached_1024",
            "value": 510.6,
            "unit": "ns",
            "extra": "8.02 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_sampled_1024",
            "value": 2028,
            "unit": "ns",
            "extra": "2.02 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_cached_1024",
            "value": 351.5,
            "unit": "ns",
            "extra": "23.31 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_sampled_1024",
            "value": 2028,
            "unit": "ns",
            "extra": "4.04 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_cached",
            "value": 99750,
            "unit": "ns",
            "extra": "5.26 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_sampled",
            "value": 132400,
            "unit": "ns",
            "extra": "3.96 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_cached_1024",
            "value": 3025,
            "unit": "ns",
            "extra": "2.71 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_sampled_1024",
            "value": 4377,
            "unit": "ns",
            "extra": "1.87 GB/s"
          },
          {
            "name": "bench_compress_f64_random_cached_1024",
            "value": 4858,
            "unit": "ns",
            "extra": "1.69 GB/s"
          },
          {
            "name": "bench_compress_f64_random_sampled_1024",
            "value": 37220,
            "unit": "ns",
            "extra": "0.22 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_cached_1024",
            "value": 2889,
            "unit": "ns",
            "extra": "2.84 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_sampled_1024",
            "value": 5589,
            "unit": "ns",
            "extra": "1.47 GB/s"
          },
          {
            "name": "bench_decompress_f32_large_batch",
            "value": 22680,
            "unit": "ns",
            "extra": "11.56 GB/s"
          },
          {
            "name": "bench_decompress_f32_ramp_1024",
            "value": 709.8,
            "unit": "ns",
            "extra": "5.77 GB/s"
          },
          {
            "name": "bench_decompress_f32_sensor_1024",
            "value": 372.8,
            "unit": "ns",
            "extra": "10.99 GB/s"
          },
          {
            "name": "bench_decompress_f64_identical_1024",
            "value": 235.7,
            "unit": "ns",
            "extra": "34.76 GB/s"
          },
          {
            "name": "bench_decompress_f64_large_batch",
            "value": 50290,
            "unit": "ns",
            "extra": "10.43 GB/s"
          },
          {
            "name": "bench_decompress_f64_ramp_1024",
            "value": 743.4,
            "unit": "ns",
            "extra": "11.02 GB/s"
          },
          {
            "name": "bench_decompress_f64_random_1024",
            "value": 803.6,
            "unit": "ns",
            "extra": "10.19 GB/s"
          },
          {
            "name": "bench_decompress_f64_sensor_1024",
            "value": 806.4,
            "unit": "ns",
            "extra": "10.16 GB/s"
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
          "id": "fdca0558e40b0fdf0f9a1bed9d987fe614a8d85c",
          "message": "fix: guard zero cap and empty delta, harden kernel slices and hist overflow",
          "timestamp": "2026-09-22T12:11:22+08:00",
          "tree_id": "1431d4a716ccaad367fa57ac4be2c67f5a17d31c",
          "url": "https://github.com/webc-site/fastalp/commit/fdca0558e40b0fdf0f9a1bed9d987fe614a8d85c"
        },
        "date": 1790050353969,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "bench_compress_f32_large_batch_cached",
            "value": 51550,
            "unit": "ns",
            "extra": "5.09 GB/s"
          },
          {
            "name": "bench_compress_f32_large_batch_sampled",
            "value": 116000,
            "unit": "ns",
            "extra": "2.26 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_cached_1024",
            "value": 1221,
            "unit": "ns",
            "extra": "3.35 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_sampled_1024",
            "value": 2453,
            "unit": "ns",
            "extra": "1.67 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_cached_1024",
            "value": 918.5,
            "unit": "ns",
            "extra": "4.46 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_sampled_1024",
            "value": 3645,
            "unit": "ns",
            "extra": "1.12 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_cached_1024",
            "value": 680.5,
            "unit": "ns",
            "extra": "12.04 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_sampled_1024",
            "value": 4598,
            "unit": "ns",
            "extra": "1.78 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_cached",
            "value": 177100,
            "unit": "ns",
            "extra": "2.96 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_sampled",
            "value": 246000,
            "unit": "ns",
            "extra": "2.13 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_cached_1024",
            "value": 3340,
            "unit": "ns",
            "extra": "2.45 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_sampled_1024",
            "value": 4602,
            "unit": "ns",
            "extra": "1.78 GB/s"
          },
          {
            "name": "bench_compress_f64_random_cached_1024",
            "value": 4888,
            "unit": "ns",
            "extra": "1.68 GB/s"
          },
          {
            "name": "bench_compress_f64_random_sampled_1024",
            "value": 35570,
            "unit": "ns",
            "extra": "0.23 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_cached_1024",
            "value": 2979,
            "unit": "ns",
            "extra": "2.75 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_sampled_1024",
            "value": 5619,
            "unit": "ns",
            "extra": "1.46 GB/s"
          },
          {
            "name": "bench_decompress_f32_large_batch",
            "value": 22910,
            "unit": "ns",
            "extra": "11.44 GB/s"
          },
          {
            "name": "bench_decompress_f32_ramp_1024",
            "value": 843.4,
            "unit": "ns",
            "extra": "4.86 GB/s"
          },
          {
            "name": "bench_decompress_f32_sensor_1024",
            "value": 398.2,
            "unit": "ns",
            "extra": "10.29 GB/s"
          },
          {
            "name": "bench_decompress_f64_identical_1024",
            "value": 217.2,
            "unit": "ns",
            "extra": "37.72 GB/s"
          },
          {
            "name": "bench_decompress_f64_large_batch",
            "value": 64710,
            "unit": "ns",
            "extra": "8.1 GB/s"
          },
          {
            "name": "bench_decompress_f64_ramp_1024",
            "value": 808.2,
            "unit": "ns",
            "extra": "10.14 GB/s"
          },
          {
            "name": "bench_decompress_f64_random_1024",
            "value": 1391,
            "unit": "ns",
            "extra": "5.89 GB/s"
          },
          {
            "name": "bench_decompress_f64_sensor_1024",
            "value": 1068,
            "unit": "ns",
            "extra": "7.67 GB/s"
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
          "id": "b6a977d3f18bd040d5f5d9eaf797f988e278b9e2",
          "message": "merge: perf-opt (2-way ILP unpack, const pack dispatch, macro deduplication)",
          "timestamp": "2026-09-22T12:30:58+08:00",
          "tree_id": "ad57601eeec05217dcfa0ba3c3110e2798b209b8",
          "url": "https://github.com/webc-site/fastalp/commit/b6a977d3f18bd040d5f5d9eaf797f988e278b9e2"
        },
        "date": 1790051597936,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "bench_compress_f32_large_batch_cached",
            "value": 29030,
            "unit": "ns",
            "extra": "9.03 GB/s"
          },
          {
            "name": "bench_compress_f32_large_batch_sampled",
            "value": 61860,
            "unit": "ns",
            "extra": "4.24 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_cached_1024",
            "value": 1026,
            "unit": "ns",
            "extra": "3.99 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_sampled_1024",
            "value": 2514,
            "unit": "ns",
            "extra": "1.63 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_cached_1024",
            "value": 903.6,
            "unit": "ns",
            "extra": "4.53 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_sampled_1024",
            "value": 3681,
            "unit": "ns",
            "extra": "1.11 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_cached_1024",
            "value": 730.6,
            "unit": "ns",
            "extra": "11.21 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_sampled_1024",
            "value": 3650,
            "unit": "ns",
            "extra": "2.24 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_cached",
            "value": 100100,
            "unit": "ns",
            "extra": "5.24 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_sampled",
            "value": 130000,
            "unit": "ns",
            "extra": "4.03 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_cached_1024",
            "value": 3040,
            "unit": "ns",
            "extra": "2.69 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_sampled_1024",
            "value": 3630,
            "unit": "ns",
            "extra": "2.26 GB/s"
          },
          {
            "name": "bench_compress_f64_random_cached_1024",
            "value": 3395,
            "unit": "ns",
            "extra": "2.41 GB/s"
          },
          {
            "name": "bench_compress_f64_random_sampled_1024",
            "value": 18840,
            "unit": "ns",
            "extra": "0.43 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_cached_1024",
            "value": 2614,
            "unit": "ns",
            "extra": "3.13 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_sampled_1024",
            "value": 4172,
            "unit": "ns",
            "extra": "1.96 GB/s"
          },
          {
            "name": "bench_decompress_f32_large_batch",
            "value": 15580,
            "unit": "ns",
            "extra": "16.83 GB/s"
          },
          {
            "name": "bench_decompress_f32_ramp_1024",
            "value": 626.8,
            "unit": "ns",
            "extra": "6.53 GB/s"
          },
          {
            "name": "bench_decompress_f32_sensor_1024",
            "value": 259.8,
            "unit": "ns",
            "extra": "15.77 GB/s"
          },
          {
            "name": "bench_decompress_f64_identical_1024",
            "value": 269.2,
            "unit": "ns",
            "extra": "30.43 GB/s"
          },
          {
            "name": "bench_decompress_f64_large_batch",
            "value": 50410,
            "unit": "ns",
            "extra": "10.4 GB/s"
          },
          {
            "name": "bench_decompress_f64_ramp_1024",
            "value": 803.3,
            "unit": "ns",
            "extra": "10.2 GB/s"
          },
          {
            "name": "bench_decompress_f64_random_1024",
            "value": 800.8,
            "unit": "ns",
            "extra": "10.23 GB/s"
          },
          {
            "name": "bench_decompress_f64_sensor_1024",
            "value": 1036,
            "unit": "ns",
            "extra": "7.91 GB/s"
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
          "id": "b44112cd3d57269a9cb6d27243846c0c73047616",
          "message": "docs: prune legacy changelog entries",
          "timestamp": "2026-09-22T13:00:10+08:00",
          "tree_id": "b3e055b9cb8f05a62ec76db176f0d5e309109fdf",
          "url": "https://github.com/webc-site/fastalp/commit/b44112cd3d57269a9cb6d27243846c0c73047616"
        },
        "date": 1790053277355,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "bench_compress_f32_large_batch_cached",
            "value": 29120,
            "unit": "ns",
            "extra": "9 GB/s"
          },
          {
            "name": "bench_compress_f32_large_batch_sampled",
            "value": 73520,
            "unit": "ns",
            "extra": "3.57 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_cached_1024",
            "value": 783.3,
            "unit": "ns",
            "extra": "5.23 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_sampled_1024",
            "value": 2383,
            "unit": "ns",
            "extra": "1.72 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_cached_1024",
            "value": 876.1,
            "unit": "ns",
            "extra": "4.68 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_sampled_1024",
            "value": 3706,
            "unit": "ns",
            "extra": "1.11 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_cached_1024",
            "value": 706.8,
            "unit": "ns",
            "extra": "11.59 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_sampled_1024",
            "value": 3936,
            "unit": "ns",
            "extra": "2.08 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_cached",
            "value": 146900,
            "unit": "ns",
            "extra": "3.57 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_sampled",
            "value": 130500,
            "unit": "ns",
            "extra": "4.02 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_cached_1024",
            "value": 3010,
            "unit": "ns",
            "extra": "2.72 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_sampled_1024",
            "value": 4482,
            "unit": "ns",
            "extra": "1.83 GB/s"
          },
          {
            "name": "bench_compress_f64_random_cached_1024",
            "value": 4958,
            "unit": "ns",
            "extra": "1.65 GB/s"
          },
          {
            "name": "bench_compress_f64_random_sampled_1024",
            "value": 35700,
            "unit": "ns",
            "extra": "0.23 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_cached_1024",
            "value": 2844,
            "unit": "ns",
            "extra": "2.88 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_sampled_1024",
            "value": 5529,
            "unit": "ns",
            "extra": "1.48 GB/s"
          },
          {
            "name": "bench_decompress_f32_large_batch",
            "value": 22760,
            "unit": "ns",
            "extra": "11.52 GB/s"
          },
          {
            "name": "bench_decompress_f32_ramp_1024",
            "value": 759.4,
            "unit": "ns",
            "extra": "5.39 GB/s"
          },
          {
            "name": "bench_decompress_f32_sensor_1024",
            "value": 398.2,
            "unit": "ns",
            "extra": "10.29 GB/s"
          },
          {
            "name": "bench_decompress_f64_identical_1024",
            "value": 408.2,
            "unit": "ns",
            "extra": "20.07 GB/s"
          },
          {
            "name": "bench_decompress_f64_large_batch",
            "value": 51330,
            "unit": "ns",
            "extra": "10.21 GB/s"
          },
          {
            "name": "bench_decompress_f64_ramp_1024",
            "value": 743.2,
            "unit": "ns",
            "extra": "11.02 GB/s"
          },
          {
            "name": "bench_decompress_f64_random_1024",
            "value": 799.6,
            "unit": "ns",
            "extra": "10.25 GB/s"
          },
          {
            "name": "bench_decompress_f64_sensor_1024",
            "value": 800.8,
            "unit": "ns",
            "extra": "10.23 GB/s"
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
          "id": "e8e8db5acaa3133ecee5f13f284b006815781cb4",
          "message": "chore(release): v0.1.43 (4-step recurrence tree, 16-way ILP unrolling, zero-alloc RD decode)",
          "timestamp": "2026-09-22T13:12:56+08:00",
          "tree_id": "3377864a2b14982fee3611ef1052fb223c2fdee9",
          "url": "https://github.com/webc-site/fastalp/commit/e8e8db5acaa3133ecee5f13f284b006815781cb4"
        },
        "date": 1790054037188,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "bench_compress_f32_large_batch_cached",
            "value": 30490,
            "unit": "ns",
            "extra": "8.6 GB/s"
          },
          {
            "name": "bench_compress_f32_large_batch_sampled",
            "value": 102200,
            "unit": "ns",
            "extra": "2.57 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_cached_1024",
            "value": 1120,
            "unit": "ns",
            "extra": "3.66 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_sampled_1024",
            "value": 2513,
            "unit": "ns",
            "extra": "1.63 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_cached_1024",
            "value": 949.6,
            "unit": "ns",
            "extra": "4.31 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_sampled_1024",
            "value": 3790,
            "unit": "ns",
            "extra": "1.08 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_cached_1024",
            "value": 756.9,
            "unit": "ns",
            "extra": "10.82 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_sampled_1024",
            "value": 3890,
            "unit": "ns",
            "extra": "2.11 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_cached",
            "value": 164100,
            "unit": "ns",
            "extra": "3.19 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_sampled",
            "value": 152300,
            "unit": "ns",
            "extra": "3.44 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_cached_1024",
            "value": 3003,
            "unit": "ns",
            "extra": "2.73 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_sampled_1024",
            "value": 4761,
            "unit": "ns",
            "extra": "1.72 GB/s"
          },
          {
            "name": "bench_compress_f64_random_cached_1024",
            "value": 5312,
            "unit": "ns",
            "extra": "1.54 GB/s"
          },
          {
            "name": "bench_compress_f64_random_sampled_1024",
            "value": 34860,
            "unit": "ns",
            "extra": "0.23 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_cached_1024",
            "value": 3163,
            "unit": "ns",
            "extra": "2.59 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_sampled_1024",
            "value": 5758,
            "unit": "ns",
            "extra": "1.42 GB/s"
          },
          {
            "name": "bench_decompress_f32_large_batch",
            "value": 15460,
            "unit": "ns",
            "extra": "16.96 GB/s"
          },
          {
            "name": "bench_decompress_f32_ramp_1024",
            "value": 349.9,
            "unit": "ns",
            "extra": "11.71 GB/s"
          },
          {
            "name": "bench_decompress_f32_sensor_1024",
            "value": 286.6,
            "unit": "ns",
            "extra": "14.29 GB/s"
          },
          {
            "name": "bench_decompress_f64_identical_1024",
            "value": 230.1,
            "unit": "ns",
            "extra": "35.6 GB/s"
          },
          {
            "name": "bench_decompress_f64_large_batch",
            "value": 54220,
            "unit": "ns",
            "extra": "9.67 GB/s"
          },
          {
            "name": "bench_decompress_f64_ramp_1024",
            "value": 930.9,
            "unit": "ns",
            "extra": "8.8 GB/s"
          },
          {
            "name": "bench_decompress_f64_random_1024",
            "value": 1426,
            "unit": "ns",
            "extra": "5.74 GB/s"
          },
          {
            "name": "bench_decompress_f64_sensor_1024",
            "value": 839.4,
            "unit": "ns",
            "extra": "9.76 GB/s"
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
          "id": "85ec8c78dc7cc36688f6b3e469178b9d5fe5e9a5",
          "message": "docs: update benchmark chart image in intro.md and README.md",
          "timestamp": "2026-09-22T13:13:24+08:00",
          "tree_id": "0f9b3c764e5557ab5621a7d7218a0c810bfd3be3",
          "url": "https://github.com/webc-site/fastalp/commit/85ec8c78dc7cc36688f6b3e469178b9d5fe5e9a5"
        },
        "date": 1790054063145,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "bench_compress_f32_large_batch_cached",
            "value": 52420,
            "unit": "ns",
            "extra": "5 GB/s"
          },
          {
            "name": "bench_compress_f32_large_batch_sampled",
            "value": 119500,
            "unit": "ns",
            "extra": "2.19 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_cached_1024",
            "value": 1336,
            "unit": "ns",
            "extra": "3.07 GB/s"
          },
          {
            "name": "bench_compress_f32_ramp_sampled_1024",
            "value": 2554,
            "unit": "ns",
            "extra": "1.6 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_cached_1024",
            "value": 931.1,
            "unit": "ns",
            "extra": "4.4 GB/s"
          },
          {
            "name": "bench_compress_f32_sensor_sampled_1024",
            "value": 3836,
            "unit": "ns",
            "extra": "1.07 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_cached_1024",
            "value": 683.1,
            "unit": "ns",
            "extra": "11.99 GB/s"
          },
          {
            "name": "bench_compress_f64_identical_sampled_1024",
            "value": 3976,
            "unit": "ns",
            "extra": "2.06 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_cached",
            "value": 178500,
            "unit": "ns",
            "extra": "2.94 GB/s"
          },
          {
            "name": "bench_compress_f64_large_batch_sampled",
            "value": 241200,
            "unit": "ns",
            "extra": "2.17 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_cached_1024",
            "value": 3425,
            "unit": "ns",
            "extra": "2.39 GB/s"
          },
          {
            "name": "bench_compress_f64_ramp_sampled_1024",
            "value": 4688,
            "unit": "ns",
            "extra": "1.75 GB/s"
          },
          {
            "name": "bench_compress_f64_random_cached_1024",
            "value": 4953,
            "unit": "ns",
            "extra": "1.65 GB/s"
          },
          {
            "name": "bench_compress_f64_random_sampled_1024",
            "value": 34250,
            "unit": "ns",
            "extra": "0.24 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_cached_1024",
            "value": 2910,
            "unit": "ns",
            "extra": "2.82 GB/s"
          },
          {
            "name": "bench_compress_f64_sensor_sampled_1024",
            "value": 5539,
            "unit": "ns",
            "extra": "1.48 GB/s"
          },
          {
            "name": "bench_decompress_f32_large_batch",
            "value": 14030,
            "unit": "ns",
            "extra": "18.68 GB/s"
          },
          {
            "name": "bench_decompress_f32_ramp_1024",
            "value": 306.1,
            "unit": "ns",
            "extra": "13.38 GB/s"
          },
          {
            "name": "bench_decompress_f32_sensor_1024",
            "value": 254.8,
            "unit": "ns",
            "extra": "16.08 GB/s"
          },
          {
            "name": "bench_decompress_f64_identical_1024",
            "value": 227.6,
            "unit": "ns",
            "extra": "35.99 GB/s"
          },
          {
            "name": "bench_decompress_f64_large_batch",
            "value": 52380,
            "unit": "ns",
            "extra": "10.01 GB/s"
          },
          {
            "name": "bench_decompress_f64_ramp_1024",
            "value": 850.8,
            "unit": "ns",
            "extra": "9.63 GB/s"
          },
          {
            "name": "bench_decompress_f64_random_1024",
            "value": 1291,
            "unit": "ns",
            "extra": "6.35 GB/s"
          },
          {
            "name": "bench_decompress_f64_sensor_1024",
            "value": 860.8,
            "unit": "ns",
            "extra": "9.52 GB/s"
          }
        ]
      }
    ]
  }
}