#!/usr/bin/env -S bun
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { parse as yamlParse } from "yaml";
import { loadBenchData } from "./lib/data.js";
import { renderSvg } from "./lib/render.js";
import { optimizeSvg, renderJpg } from "./lib/upload.js";
import { benchMdGen } from "./benchMdGen.js";

const BENCHES_DIR = import.meta.dirname,
  I18N_DIR = resolve(BENCHES_DIR, "i18n"),
  IMG_DIR = resolve(BENCHES_DIR, "img"),
  LANG_LI = ["zh", "en"];

const i18nLoad = async (lang = "zh") => {
  const yml_path = resolve(I18N_DIR, `${lang}.yml`),
    content = await Bun.file(yml_path).text();
  return yamlParse(content);
};

export const benchSvgGen = async () => {
  console.log("1. Loading benchmark dataset JSONs...");
  const bench_data = await loadBenchData();

  for (const lang of LANG_LI) {
    console.log(`\n2. Generating SVG and JPG for [${lang}] (Mobile Layout)...`);
    const i18n = await i18nLoad(lang),
      raw_svg = renderSvg(bench_data, i18n, lang),
      svg = optimizeSvg(raw_svg),
      out_dir = resolve(IMG_DIR, lang);

    await mkdir(out_dir, { recursive: true });

    const local_svg = resolve(out_dir, "bench.svg"),
      local_jpg = resolve(out_dir, "bench.jpg");

    await Bun.write(local_svg, svg);
    console.log(`  -> Saved local SVG: ${local_svg}`);

    try {
      const jpg_bytes = await renderJpg(svg, 1440);
      await Bun.write(local_jpg, jpg_bytes);
      console.log(`  -> Saved local JPG: ${local_jpg}`);
    } catch (err) {
      console.warn(`  -> Resvg render JPG warning: ${err.message || err}`);
    }
  }

  console.log("\n3. Generating synchronized Markdown documentation (readme/{zh,en}/bench.md)...");
  await benchMdGen();

  console.log("\nLocal SVG, JPG & Markdown generation complete!");
};

if (import.meta.main) {
  await benchSvgGen();
}

export default benchSvgGen;
