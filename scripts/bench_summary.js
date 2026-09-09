#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const inputFile = process.argv[2] || "bench_raw.txt";
const outputJsonFile = process.argv[3] || "bench_output.json";
const outputMdFile = process.argv[4] || "bench_summary.md";

if (!fs.existsSync(inputFile)) {
  console.error(`Input file ${inputFile} does not exist`);
  process.exit(1);
}

const rawText = fs.readFileSync(inputFile, "utf8");

const unitMap = {
  ns: 1,
  "µs": 1e3,
  us: 1e3,
  ms: 1e6,
  s: 1e9,
};

function getBytes(name) {
  const isF32 = name.includes("f32");
  const elemBytes = isF32 ? 4 : 8;
  if (name.includes("large_batch")) {
    return 65536 * elemBytes;
  }
  return 1024 * elemBytes;
}

const items = [];
const lines = rawText.split("\n");

for (const line of lines) {
  const m = line.match(
    /[├╰]─\s+([^\s]+)\s+([0-9.]+)\s+([^\s]+)\s+│\s+([0-9.]+)\s+([^\s]+)\s+│\s+([0-9.]+)\s+([^\s]+)/
  );
  if (!m) continue;

  const name = m[1];
  const fastestVal = parseFloat(m[2]);
  const fastestUnit = m[3];
  const medianVal = parseFloat(m[6]);
  const medianUnit = m[7];

  const medianNs = medianVal * (unitMap[medianUnit] || 1);
  const bytes = getBytes(name);
  const throughputGb = (bytes / medianNs).toFixed(2);

  items.push({
    name,
    fastest: `${fastestVal} ${fastestUnit}`,
    median: `${medianVal} ${medianUnit}`,
    medianNs: Math.round(medianNs * 10) / 10,
    throughputGb: parseFloat(throughputGb),
  });
}

// 1. Write github-action-benchmark custom format JSON
const benchmarkData = items.map((item) => ({
  name: item.name,
  unit: "ns",
  value: item.medianNs,
  extra: `${item.throughputGb} GB/s`,
}));

fs.writeFileSync(outputJsonFile, JSON.stringify(benchmarkData, null, 2), "utf8");
console.log(`Generated benchmark JSON for ${benchmarkData.length} items -> ${outputJsonFile}`);

// 2. Generate Markdown summary
let maxDecThroughput = 0;
let maxEncThroughput = 0;

for (const item of items) {
  if (item.name.includes("decompress")) {
    if (item.throughputGb > maxDecThroughput) maxDecThroughput = item.throughputGb;
  } else {
    if (item.throughputGb > maxEncThroughput) maxEncThroughput = item.throughputGb;
  }
}

const formatMode = (name) => {
  if (name.includes("decompress")) return "⚡ Decompress";
  if (name.includes("cached")) return "🔥 Warm Kernel";
  if (name.includes("sampled")) return "❄️ Cold Sampled";
  return "⚡ Default";
};

let md = `## 🚀 FastALP Performance Benchmark Summary\n\n`;
md += `> **Peak Decompression Throughput**: **\`${maxDecThroughput} GB/s\`** | **Peak Compression (Warm)**: **\`${maxEncThroughput} GB/s\`**\n\n`;

md += `### 📊 Benchmark Metrics\n\n`;
md += `| Benchmark Case | Median Latency | Throughput (GB/s) | Mode |\n`;
md += `| :--- | :---: | :---: | :---: |\n`;

for (const item of items) {
  md += `| \`${item.name}\` | ${item.median} | **${item.throughputGb} GB/s** | ${formatMode(item.name)} |\n`;
}

md += `\n> 📈 **[View Interactive Continuous Benchmark History & Regression Chart](https://webc-site.github.io/fastalp/dev/bench/)**\n`;

fs.writeFileSync(outputMdFile, md, "utf8");
console.log(`Generated Step Summary Markdown -> ${outputMdFile}`);
