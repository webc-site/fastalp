import { optimize } from "svgo";
import { Resvg } from "@resvg/resvg-js";
import cdnUpload from "@1-/github_cdn";

const FONT_FILE_LI = [
  "/System/Library/Fonts/Hiragino Sans GB.ttc",
  "/System/Library/Fonts/STHeiti Medium.ttc",
  "/System/Library/Fonts/STHeiti Light.ttc",
  "/System/Library/Fonts/Supplemental/Songti.ttc",
  "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
  "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
  "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
  "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
  "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
].filter((f) => Bun.file(f).size > 0);

const tokenByEnv = () => {
  const t = process.env.GH_TOKEN || process.env.GITHUB_TOKEN;
  if (!t) {
    throw new Error("缺少 GH_TOKEN 或 GITHUB_TOKEN 环境变量，无法上传 SVG 至 CDN");
  }
  return t;
};

export const optimizeSvg = (raw_svg) => {
  return optimize(raw_svg, {
    multipass: true,
  }).data;
};

export const renderJpg = async (svg_str, target_width = 1440) => {
  const resvg = new Resvg(svg_str, {
    fitTo: { mode: "width", value: target_width },
    background: "#070b14",
    font: {
      loadSystemFonts: false,
      fontFiles: FONT_FILE_LI,
      defaultFontFamily: "Hiragino Sans GB",
    },
  });
  const png_buf = resvg.render().asPng();
  if (typeof Bun !== "undefined" && typeof Bun.Image === "function") {
    const img = new Bun.Image(png_buf);
    return await img.jpeg({ quality: 95 }).bytes();
  }
  return png_buf;
};

export const uploadSvg = async (svg_buf) => {
  const gh_token = tokenByEnv(),
    upload = cdnUpload(gh_token, "webc-fs/-"),
    raw_url = await upload(svg_buf, "svg"),
    { pathname } = new URL(raw_url.startsWith("//") ? "https:" + raw_url : raw_url);
  return "https://fastly.jsdelivr.net" + pathname;
};
