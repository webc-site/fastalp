import { readdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import os from "node:os";
import { execSync } from "node:child_process";
import { loadCppAlpResult } from "./cpp_alp_loader.js";

const BENCHES_DIR = resolve(import.meta.dirname, ".."),
  JSON_DIR = join(BENCHES_DIR, "json");

export const systemEnvByLang = (is_zh = true) => {
  let cpu_model = "";
  try {
    cpu_model = execSync("sysctl -n machdep.cpu.brand_string 2>/dev/null || sysctl -n hw.model 2>/dev/null", { encoding: "utf8" }).trim();
  } catch {}
  if (!cpu_model) {
    cpu_model = os.cpus()[0]?.model || "Apple Silicon";
  }
  const core_count = os.cpus().length || 12;

  let os_desc = "macOS";
  try {
    const os_ver = execSync("sw_vers -productVersion 2>/dev/null", { encoding: "utf8" }).trim();
    if (os_ver) os_desc = `macOS ${os_ver}`;
  } catch {
    os_desc = `${os.type()} ${os.release()}`;
  }

  let rust_ver = "Rust";
  try {
    const r = execSync("rustc --version 2>/dev/null", { encoding: "utf8" }).trim().split(" ")[1];
    if (r) rust_ver = `Rust ${r}`;
  } catch {}

  const cpu = is_zh 
    ? `芯片: ${cpu_model} (${core_count} 核)` 
    : `CPU: ${cpu_model} (${core_count} Cores)`,
    toolchain = is_zh
    ? `环境: ${os_desc} ｜ 工具链: ${rust_ver} / Clang (-O3)`
    : `OS: ${os_desc} ｜ Toolchain: ${rust_ver} / Clang (-O3)`;

  return { cpu, toolchain, cpuModel: cpu_model };
};

export const getSystemEnv = systemEnvByLang;

export const geoMean = (arr) => {
  if (!arr || arr.length === 0) return 0;
  const valid_li = arr.filter((v) => typeof v === "number" && !isNaN(v) && v > 0);
  if (valid_li.length === 0) return 0;
  const sum_ln = valid_li.reduce((acc, v) => acc + Math.log(v), 0);
  return Math.exp(sum_ln / valid_li.length);
};

export const geomean = geoMean;

export const algoDictByLi = (li) =>
  Object.fromEntries(li.map((a) => [a.algorithm, a]));

export const DATASET_META = {
  // 1. IoT & Environmental Sensors
  air_sensor_f: { zh: "空气环境传感", en: "Air Sensor IoT", domain: "IoT", domainZh: "环境传感", domainEn: "IoT Sensor" },
  neon_air_pressure: { zh: "生态大气气压", en: "Atmospheric Pressure", domain: "IoT", domainZh: "生态台网", domainEn: "NEON Geo" },
  neon_bio_temp_c: { zh: "生态土壤温度", en: "Ecology Soil Temp", domain: "IoT", domainZh: "生态台网", domainEn: "NEON Geo" },
  neon_dew_point_temp: { zh: "生态露点温度", en: "Ecology Dew Point", domain: "IoT", domainZh: "生态台网", domainEn: "NEON Geo" },
  neon_pm10_dust: { zh: "生态粉尘微粒", en: "Ecology Aerosol Dust", domain: "IoT", domainZh: "生态台网", domainEn: "NEON Geo" },
  neon_wind_dir: { zh: "生态连续风向", en: "Ecology Wind Dir", domain: "IoT", domainZh: "生态台网", domainEn: "NEON Geo" },
  isd_air_temperature: { zh: "希思罗逐时气温", en: "ISD Hourly Temp", domain: "IoT", domainZh: "全球气象", domainEn: "Meteorology" },

  // 2. Meteorology, Hydrology & Geospatial
  arade4: { zh: "水文河流径流", en: "River Runoff Flow", domain: "气象", domainZh: "水文观测", domainEn: "Hydrology" },
  basel_temp_f: { zh: "百年欧洲气温", en: "Basel Climate Temp", domain: "气象", domainZh: "气象气候", domainEn: "Climate" },
  basel_wind_f: { zh: "连续风速监测", en: "Basel Wind Speed", domain: "气象", domainZh: "气象气候", domainEn: "Climate" },
  city_temperature_f: { zh: "全球城市气温", en: "Urban Climate Temp", domain: "气象", domainZh: "气象气候", domainEn: "Climate" },
  bird_migration_f: { zh: "候鸟高程轨迹", en: "Avian Telemetry GPS", domain: "地理", domainZh: "空间遥测", domainEn: "Telemetry" },
  nyc29: { zh: "出租运营轨迹", en: "NYC Taxi Trajectory", domain: "地理", domainZh: "城市交通", domainEn: "Mobility" },
  poi_lat: { zh: "高精测绘纬度", en: "Geospatial POI Lat", domain: "地理", domainZh: "高精测绘", domainEn: "Geospatial" },
  poi_lon: { zh: "高精测绘经度", en: "Geospatial POI Lon", domain: "地理", domainZh: "高精测绘", domainEn: "Geospatial" },
  usgs_gage_height: { zh: "密苏里水尺测深", en: "USGS Gage Height", domain: "地理", domainZh: "空间水文", domainEn: "Hydrology" },

  // 3. Quantitative Finance & Blockchain
  stocks_usa_c: { zh: "美股纳指高频", en: "Nasdaq Equities HFT", domain: "金融", domainZh: "证券撮合", domainEn: "Securities" },
  stocks_uk: { zh: "英股伦敦高频", en: "FTSE Equities HFT", domain: "金融", domainZh: "证券撮合", domainEn: "Securities" },
  stocks_de: { zh: "德股法兰克福", en: "DAX Equities HFT", domain: "金融", domainZh: "证券撮合", domainEn: "Securities" },
  bitcoin_f: { zh: "加密现货成交", en: "Crypto Trade Quotes", domain: "金融", domainZh: "加密资产", domainEn: "Crypto" },
  bitcoin_transactions_f: { zh: "链上交易流水", en: "Blockchain Tx Vol", domain: "金融", domainZh: "区块链", domainEn: "Blockchain" },
  food_prices: { zh: "粮农物价指数", en: "FAO Food Price Index", domain: "金融", domainZh: "宏观物价", domainEn: "Economics" },
  isd_sea_pressure: { zh: "海平面气压观测", en: "ISD Sea Pressure", domain: "气象", domainZh: "气象观测", domainEn: "Meteorology" },

  // 4. Healthcare & Public Health
  cms1: { zh: "门诊医疗结算", en: "Medicare Claims", domain: "医疗", domainZh: "医保结算", domainEn: "Healthcare" },
  cms25: { zh: "住院诊断总额", en: "Inpatient Charges", domain: "医疗", domainZh: "医保结算", domainEn: "Healthcare" },
  cms9: { zh: "药品处方定价", en: "Pharma Drug Prices", domain: "医疗", domainZh: "医疗药品", domainEn: "Pharma" },
  medicare1: { zh: "门诊理赔流水", en: "Outpatient Billing", domain: "医疗", domainZh: "公共卫生", domainEn: "Public Health" },
  medicare9: { zh: "专科医疗津贴", en: "Specialty Grants", domain: "医疗", domainZh: "公共卫生", domainEn: "Public Health" },

  // 5. Government Fiscal & Demographics
  gov10: { zh: "财政公共支出", en: "Fiscal Expenditure", domain: "政务", domainZh: "财政统计", domainEn: "Fiscal" },
  gov26: { zh: "人口普查常数", en: "Census Population", domain: "政务", domainZh: "人口普查", domainEn: "Demographics" },
  gov30: { zh: "宏观运行指标", en: "Macroeconomic Index", domain: "政务", domainZh: "宏观指标", domainEn: "Macro" },
  gov31: { zh: "财政转移支付", en: "Fiscal Transfer", domain: "政务", domainZh: "财政统计", domainEn: "Fiscal" },
  gov40: { zh: "市政管网测绘", en: "Infrastructure Survey", domain: "政务", domainZh: "市政设施", domainEn: "Civic" },
  noaa_water_sigma: { zh: "基韦斯特潮位差", en: "NOAA Tide Sigma", domain: "海洋", domainZh: "海洋潮汐", domainEn: "Oceanography" },

  // 6. Industrial Waveforms & Physical Observations
  usgs_river_discharge: { zh: "波托马克河水流", en: "USGS Potomac Discharge", domain: "水文", domainZh: "水文河流", domainEn: "Hydrology" },
  noaa_water_level: { zh: "波士顿水尺水位", en: "NOAA Boston Water Level", domain: "海洋", domainZh: "海洋潮汐", domainEn: "Oceanography" },
  ssd_hdd_benchmarks_f: { zh: "存储设备吞吐", en: "Storage I/O Speed", domain: "工业", domainZh: "硬件指标", domainEn: "Hardware" },
};

export const datasetMeta = DATASET_META;

export const SCENARIO_DATASET_MAP = {
  scene_sensor: [
    "neon_pm10_dust", "neon_air_pressure", "neon_bio_temp_c", "neon_dew_point_temp",
    "neon_wind_dir", "air_sensor_f", "isd_air_temperature", "basel_temp_f", "basel_wind_f",
    "city_temperature_f", "arade4"
  ],
  scene_finance: [
    "stocks_usa_c", "stocks_de", "stocks_uk", "bitcoin_f",
    "bitcoin_transactions_f", "food_prices", "isd_sea_pressure"
  ],
  scene_geo: [
    "bird_migration_f", "nyc29", "poi_lat", "poi_lon", "usgs_gage_height"
  ],
  scene_health: [
    "cms1", "cms25", "cms9", "medicare1", "medicare9"
  ],
  scene_macro: [
    "gov10", "gov26", "gov30", "gov31", "gov40"
  ],
  scene_waveform: [
    "usgs_river_discharge", "noaa_water_level", "noaa_water_sigma", "ssd_hdd_benchmarks_f"
  ]
};

export const scenarioDatasetMap = SCENARIO_DATASET_MAP;

export const computeScenarioMetrics = (algo, scene_key) => {
  if (!algo) throw new Error(`Algorithm object missing for scenario ${scene_key}`);
  const ds = algo.paper_31?.datasets;
  if (!ds || ds.length === 0) throw new Error(`Datasets missing for algorithm ${algo.algorithm}`);

  const target_name_li = SCENARIO_DATASET_MAP[scene_key];
  if (target_name_li && target_name_li.length > 0) {
    const matched_li = ds.filter((d) => target_name_li.includes(d.name));
    if (matched_li.length > 0) {
      const avg_dec = matched_li.reduce((acc, d) => acc + d.dec_gb_s, 0) / matched_li.length,
        avg_enc = matched_li.reduce((acc, d) => acc + d.enc_gb_s, 0) / matched_li.length,
        total_raw = matched_li.reduce((acc, d) => acc + d.raw_bytes, 0),
        total_comp = matched_li.reduce((acc, d) => acc + d.compressed_bytes, 0);
      return {
        dec_gb_s: avg_dec,
        enc_gb_s: avg_enc,
        ratio: total_raw / total_comp,
        count: matched_li.length,
        totalPts: matched_li.length * 1024
      };
    }
  }

  const exact = ds.find((d) => d.name === scene_key);
  if (exact) {
    return { dec_gb_s: exact.dec_gb_s, enc_gb_s: exact.enc_gb_s, ratio: exact.ratio, count: 1, totalPts: 1024 };
  }

  throw new Error(`Scenario ${scene_key} not found in datasets for algorithm ${algo.algorithm}`);
};

export const loadBenchData = async () => {
  const file_li = await readdir(JSON_DIR),
    json_file_li = file_li.filter((f) => f.endsWith(".json")),
    algorithm_li = [],
    measured_cpp = await loadCppAlpResult(),
    scenario_key_li = ["scene_sensor", "scene_finance", "scene_geo", "scene_health", "scene_macro", "scene_waveform"];

  for (const f of json_file_li) {
    const file_path = join(JSON_DIR, f),
      content = await Bun.file(file_path).json();

    if (content.algorithm === "cpp_alp" && measured_cpp) {
      content.paper_31 = measured_cpp.paper_31;
    }

    const base_datasets = content.paper_31.datasets,
      all_datasets = [...base_datasets],
      dec_li = base_datasets.map((d) => d.dec_gb_s),
      enc_li = base_datasets.map((d) => d.enc_gb_s),
      enc_kern_li = base_datasets.map((d) => d.enc_kernel_gb_s ?? d.enc_gb_s),
      ratio_li = base_datasets.map((d) => d.ratio);

    content.paper_31.geomean_dec_gb_s = geoMean(dec_li);
    content.paper_31.geomean_enc_gb_s = geoMean(enc_li);
    content.paper_31.geomean_enc_kernel_gb_s = geoMean(enc_kern_li);
    content.paper_31.geomean_ratio = geoMean(ratio_li);

    const avg = (arr) => arr.reduce((a, b) => a + b, 0) / arr.length;
    content.paper_31.avg_dec_gb_s = avg(dec_li);
    content.paper_31.avg_enc_gb_s = avg(enc_li);
    content.paper_31.avg_enc_kernel_gb_s = avg(enc_kern_li);

    for (const sc_key of scenario_key_li) {
      if (!all_datasets.some((d) => d.name === sc_key)) {
        const m = computeScenarioMetrics(content, sc_key);
        all_datasets.push({
          name: sc_key,
          dec_gb_s: m.dec_gb_s,
          enc_gb_s: m.enc_gb_s,
          ratio: m.ratio
        });
      }
    }
    content.paper_31.datasets = all_datasets;
    algorithm_li.push(content);
  }

  const PRIORITY_LI = [
    "fastalp",
    "cpp_alp",
    "pco",
    "zstd",
    "lz4",
    "snappy",
    "chimp128",
    "gorilla",
  ];

  algorithm_li.sort((a, b) => {
    const ia = PRIORITY_LI.indexOf(a.algorithm),
      ib = PRIORITY_LI.indexOf(b.algorithm);
    return (ia === -1 ? 99 : ia) - (ib === -1 ? 99 : ib);
  });

  const algo_dict = algoDictByLi(algorithm_li),
    { fastalp, cpp_alp, pco, zstd, lz4, gorilla, chimp128: chimp } = algo_dict;

  const speedupDec = (target) =>
    (fastalp.paper_31.geomean_dec_gb_s / target.paper_31.geomean_dec_gb_s).toFixed(1);

  const summary = {
    fastalp,
    cppAlp: cpp_alp,
    pco,
    zstd,
    lz4,
    gorilla,
    chimp,
    encKernelSpeedupVsCpp: (
      fastalp.paper_31.geomean_enc_kernel_gb_s / cpp_alp.paper_31.geomean_enc_kernel_gb_s
    ).toFixed(2),
    encSampledSpeedupVsCpp: (
      fastalp.paper_31.geomean_enc_gb_s / cpp_alp.paper_31.geomean_enc_gb_s
    ).toFixed(1),
    decSpeedupVsCpp: (
      fastalp.paper_31.geomean_dec_gb_s / cpp_alp.paper_31.geomean_dec_gb_s
    ).toFixed(2),
    decSpeedupVsPco: speedupDec(pco),
    decSpeedupVsZstd: speedupDec(zstd),
    decSpeedupVsGorilla: speedupDec(gorilla),
    decSpeedupVsChimp: speedupDec(chimp),
    rampRatioFastalp: (
      fastalp.paper_31.datasets.find((d) => d.name === "usgs_river_discharge")?.ratio ?? 1.0
    ).toFixed(1),
    spaceSavedVsCppPct: (
      ((cpp_alp.paper_31.total_compressed_bytes - fastalp.paper_31.total_compressed_bytes) /
        cpp_alp.paper_31.total_compressed_bytes) *
      100
    ).toFixed(2),
  };

  return {
    algorithms: algorithm_li,
    dict: algo_dict,
    summary,
  };
};

export default loadBenchData;
