#!/usr/bin/env -S bun
import { resolve } from "node:path";
import { $ } from "bun";
import { parse as yamlParse } from "yaml";
import { systemEnvByLang } from "./lib/data.js";
import { uploadSvg } from "./lib/upload.js";

const BENCHES_DIR = import.meta.dirname,
  ROOT_DIR = resolve(BENCHES_DIR, "../.."),
  FASTALP_DIR = resolve(BENCHES_DIR, ".."),
  I18N_DIR = resolve(BENCHES_DIR, "i18n"),
  IMG_DIR = resolve(BENCHES_DIR, "img"),
  LANG_LI = ["zh", "en"];

const i18nLoad = async (lang = "zh") => {
  const yml_path = resolve(I18N_DIR, `${lang}.yml`),
    content = await Bun.file(yml_path).text();
  return yamlParse(content);
};

export const benchCdnPublish = async () => {
  const url_map = {};

  for (const lang of LANG_LI) {
    const i18n = await i18nLoad(lang),
      local_svg = resolve(IMG_DIR, `${lang}/bench.svg`),
      file = Bun.file(local_svg);

    if (!(await file.exists())) {
      console.error(`Error: ${local_svg} does not exist. Run benches/benchSvgGen.js first!`);
      continue;
    }

    let cdn_url = "";
    try {
      console.log(`\n1. Uploading ${lang} SVG to GitHub CDN...`);
      const svg_buf = await file.bytes();
      cdn_url = await uploadSvg(svg_buf);
      url_map[lang] = cdn_url;
      console.log(`  -> CDN URL [${lang}]: ${cdn_url}`);
    } catch (err) {
      console.warn(`  -> Upload CDN warning: ${err.message || err}`);
      cdn_url = `https://raw.githubusercontent.com/webc-site/fastalp/main/fastalp/benches/img/${lang}/bench.svg`;
    }

    // Update fastalp/readme/{lang}/intro.md
    console.log(`2. Updating fastalp/readme/${lang}/intro.md hero image...`);
    const readme_path = resolve(FASTALP_DIR, `readme/${lang}/intro.md`),
      md_file = Bun.file(readme_path);

    if (await md_file.exists()) {
      let content = await md_file.text();
      const sys_env = systemEnvByLang(lang === "zh"),
        hero_block = `<p align="center">\n  <img src="${cdn_url}" alt="${i18n.hero_alt}" width="100%">\n  <br>\n  <sub><b>${i18n.env_title}</b>: ${sys_env.cpu} ｜ ${sys_env.toolchain}</sub>\n</p>`;

      const hero_regex = /<p align="center">[\s\S]*?alt="fastalp[^"]*"[\s\S]*?<\/p>/;
      if (hero_regex.test(content)) {
        content = content.replace(hero_regex, hero_block);
      } else {
        const marker = "\n\n---\n",
          idx = content.indexOf(marker);
        if (idx !== -1) {
          content = content.slice(0, idx) + `\n\n${hero_block}` + content.slice(idx);
        }
      }

      await Bun.write(readme_path, content);
      console.log(`  -> Updated ${readme_path}`);
    }
  }

  // Compile mdt to update fastalp/README.md
  console.log("\n3. Compiling fastalp/README.md with mdt...");
  try {
    await $`bun x mdt fastalp`.cwd(ROOT_DIR);
    console.log("  -> Compiled fastalp/README.md successfully!");
  } catch (err) {
    console.warn(`  -> mdt compile warning: ${err.message || err}`);
  }

  console.log("\nAll CDN publish and markdown sync done!");
  return url_map;
};

if (import.meta.main) {
  await benchCdnPublish();
}

export default benchCdnPublish;
