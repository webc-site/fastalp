import { resolve } from "node:path";

export const loadCppAlpResult = async () => {
  const alp_dir = process.env.ALP_DIR || resolve(import.meta.dirname, "../../../../ALP"),
    csv_path = resolve(alp_dir, "benchmarks/your_own_dataset_result.csv"),
    file = Bun.file(csv_path);

  if (!(await file.exists())) return null;

  const content = await file.text(),
    line_li = content.trim().split("\n");

  if (line_li.length <= 1) return null;

  const dataset_li = [];
  let total_raw = 0,
    total_compressed = 0,
    sum_dec = 0,
    sum_enc = 0,
    sum_enc_kern = 0;

  for (let i = 1; i < line_li.length; ++i) {
    const part_li = line_li[i].split(",");
    if (part_li.length < 10) continue;

    const name = part_li[1],
      bits_per_val = parseFloat(part_li[3]),
      enc_sampled_gb_s = parseFloat(part_li[5]),
      enc_kernel_gb_s = parseFloat(part_li[7]),
      dec_gb_s = parseFloat(part_li[9]),
      raw_bytes = 1024 * 8,
      comp_bytes = Math.round((1024 * bits_per_val) / 8),
      ratio = raw_bytes / comp_bytes;

    total_raw += raw_bytes;
    total_compressed += comp_bytes;
    sum_dec += dec_gb_s;
    sum_enc += enc_sampled_gb_s;
    sum_enc_kern += enc_kernel_gb_s;

    dataset_li.push({
      name,
      raw_bytes,
      compressed_bytes: comp_bytes,
      ratio,
      bits_per_val,
      enc_gb_s: enc_sampled_gb_s,
      enc_sampled_gb_s,
      enc_kernel_gb_s,
      dec_gb_s,
    });
  }

  const n = dataset_li.length,
    sensor_sc = dataset_li.find((d) => d.name === "scene_sensor"),
    ramp_sc = dataset_li.find((d) => d.name === "scene_ramp"),
    steady_sc = dataset_li.find((d) => d.name === "scene_steady");

  const result_obj = {
    algorithm: "cpp_alp",
    display_name: "C++ ALP",
    category: "specialized_float",
    paper_31: {
      total_raw_bytes: total_raw,
      total_compressed_bytes: total_compressed,
      ratio: total_raw / total_compressed,
      bits_per_val: (total_compressed * 8) / (total_raw / 8),
      avg_enc_gb_s: sum_enc / n,
      avg_enc_kernel_gb_s: sum_enc_kern / n,
      avg_dec_gb_s: sum_dec / n,
      datasets: dataset_li,
    },
    micro_benchmarks: {
      sensor_1024: sensor_sc
        ? {
            raw_bytes: 8192,
            compressed_bytes: sensor_sc.compressed_bytes,
            ratio: sensor_sc.ratio,
            bits_per_val: sensor_sc.bits_per_val,
            enc_gb_s: sensor_sc.enc_gb_s,
            enc_kernel_gb_s: sensor_sc.enc_kernel_gb_s,
            dec_gb_s: sensor_sc.dec_gb_s,
          }
        : null,
      ramp_1024: ramp_sc
        ? {
            raw_bytes: 8192,
            compressed_bytes: ramp_sc.compressed_bytes,
            ratio: ramp_sc.ratio,
            bits_per_val: ramp_sc.bits_per_val,
            enc_gb_s: ramp_sc.enc_gb_s,
            enc_kernel_gb_s: ramp_sc.enc_kernel_gb_s,
            dec_gb_s: ramp_sc.dec_gb_s,
          }
        : null,
      constant_1024: steady_sc
        ? {
            raw_bytes: 8192,
            compressed_bytes: steady_sc.compressed_bytes,
            ratio: steady_sc.ratio,
            bits_per_val: steady_sc.bits_per_val,
            enc_gb_s: steady_sc.enc_gb_s,
            enc_kernel_gb_s: steady_sc.enc_kernel_gb_s,
            dec_gb_s: steady_sc.dec_gb_s,
          }
        : null,
    },
  };

  // 自动将 C++ 官方 Fork 跑出的真实结果同步固化到 benches/json/cpp_alp.json
  const json_dst = resolve(import.meta.dirname, "../json/cpp_alp.json");
  await Bun.write(json_dst, JSON.stringify(result_obj, null, 2) + "\n");

  return result_obj;
};

export default loadCppAlpResult;
