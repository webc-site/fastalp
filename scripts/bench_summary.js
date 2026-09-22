#!/usr/bin/env node
import fs from "node:fs";

const input_file = process.argv[2] || "bench_raw.txt",
  output_json_file = process.argv[3] || "bench_output.json",
  output_md_file = process.argv[4] || "bench_summary.md";

if (!fs.existsSync(input_file)) {
  console.error(`Input file ${input_file} does not exist`);
  process.exit(1);
}

const raw_text = fs.readFileSync(input_file, "utf8").replaceAll(/\x1b\[[0-9;]*m/g, "");

const unit_map = {
  ns: 1,
  "µs": 1e3,
  us: 1e3,
  ms: 1e6,
  s: 1e9,
};

const byteCount = (name) => {
  const is_f32 = name.includes("f32"),
    elem_bytes = is_f32 ? 4 : 8;
  if (name.includes("large_batch")) {
    return 65536 * elem_bytes;
  }
  return 1024 * elem_bytes;
};

const item_li = [],
  line_li = raw_text.split("\n");

for (const line of line_li) {
  const m = line.match(
    /[├╰]─\s+([^\s]+)\s+([0-9.]+)\s+([^\s]+)\s+│\s+([0-9.]+)\s+([^\s]+)\s+│\s+([0-9.]+)\s+([^\s]+)/
  );
  if (!m) continue;

  const name = m[1],
    fastest_val = parseFloat(m[2]),
    fastest_unit = m[3],
    median_val = parseFloat(m[6]),
    median_unit = m[7],
    median_ns = median_val * (unit_map[median_unit] || 1),
    bytes = byteCount(name),
    throughput_gb = (bytes / median_ns).toFixed(2);

  item_li.push({
    name,
    fastest: `${fastest_val} ${fastest_unit}`,
    median: `${median_val} ${median_unit}`,
    median_ns: Math.round(median_ns * 10) / 10,
    throughput_gb: parseFloat(throughput_gb),
  });
}

// 1. Write github-action-benchmark custom format JSON
const bench_data = item_li.map((item) => ({
  name: item.name,
  unit: "ns",
  value: item.median_ns,
  extra: `${item.throughput_gb} GB/s`,
}));

fs.writeFileSync(output_json_file, JSON.stringify(bench_data, null, 2), "utf8");
console.log(`Generated benchmark JSON for ${bench_data.length} items -> ${output_json_file}`);

// 2. Generate Markdown summary
let max_dec_throughput = 0,
  max_enc_throughput = 0;

for (const item of item_li) {
  if (item.name.includes("decompress") || item.name.includes("_dec")) {
    if (item.throughput_gb > max_dec_throughput) max_dec_throughput = item.throughput_gb;
  } else {
    if (item.throughput_gb > max_enc_throughput) max_enc_throughput = item.throughput_gb;
  }
}

const formatMode = (name) => {
  if (name.includes("decompress") || name.includes("_dec")) return "Decompress";
  if (name.includes("cached")) return "Warm Kernel";
  if (name.includes("sampled")) return "Cold Sampled";
  return "Default";
};

let md = "## FastALP Performance Benchmark Summary\n\n";
md += `> **Peak Decompression Throughput**: **\`${max_dec_throughput} GB/s\`** | **Peak Compression (Warm)**: **\`${max_enc_throughput} GB/s\`**\n\n`;
md += "### Benchmark Metrics\n\n";
md += "| Benchmark Case | Median Latency | Throughput (GB/s) | Mode |\n";
md += "| :--- | :---: | :---: | :---: |\n";

for (const item of item_li) {
  md += `| \`${item.name}\` | ${item.median} | **${item.throughput_gb} GB/s** | ${formatMode(item.name)} |\n`;
}

md += "\n> **[View Interactive Continuous Benchmark History & Regression Chart](https://webc-site.github.io/fastalp/dev/bench/)**\n";

fs.writeFileSync(output_md_file, md, "utf8");
console.log(`Generated Step Summary Markdown -> ${output_md_file}`);
