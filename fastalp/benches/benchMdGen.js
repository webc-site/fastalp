#!/usr/bin/env -S bun
import { resolve } from "node:path";
import { $ } from "bun";
import { loadBenchData } from "./lib/data.js";
import { renderMd } from "./lib/renderMd.js";

const BENCHES_DIR = import.meta.dirname,
  ROOT_DIR = resolve(BENCHES_DIR, ".."),
  LANG_LI = ["zh", "en"];

export const benchMdGen = async () => {
  console.log("1. Loading benchmark JSONs...");
  const bench_data = await loadBenchData();

  for (const lang of LANG_LI) {
    console.log(`2. Generating readme/${lang}/bench.md from JSON...`);
    const md = renderMd(bench_data, lang),
      target_file = resolve(ROOT_DIR, `readme/${lang}/bench.md`);
    await Bun.write(target_file, md);
    console.log(`  -> Saved ${target_file}`);
  }

  console.log("3. Compiling fastalp/README.md with mdt...");
  try {
    await $`bun x mdt .`.cwd(ROOT_DIR);
    console.log("  -> fastalp/README.md updated successfully!");
  } catch (err) {
    console.warn(`  -> mdt compile warning: ${err.message || err}`);
  }

  console.log("\nBenchmark documentation generation complete!");
};

if (import.meta.main) {
  await benchMdGen();
}

export default benchMdGen;
