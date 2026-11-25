// Renders the brand SVGs to PNGs, a preview sheet, and the terminal bitmap.
// Usage (from apps/desktop, which has playwright): node ../../brand/render.mjs
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// Resolve playwright from the working directory (apps/desktop has it).
const { chromium } = createRequire(join(process.cwd(), "noop.js"))("playwright");
const dir = dirname(fileURLToPath(import.meta.url));
const out = join(dir, "png");
mkdirSync(out, { recursive: true });
const svg = (f) => readFileSync(join(dir, f), "utf8");

const browser = await chromium.launch();
const page = await browser.newPage({ deviceScaleFactor: 1 });

async function png(file, size, name, bg = "transparent") {
  await page.setViewportSize({ width: size, height: size });
  await page.setContent(
    `<style>html,body{margin:0;background:${bg}}svg{display:block;width:${size}px;height:${size}px}</style>${svg(file)}`,
  );
  await page.screenshot({ path: join(out, name), omitBackground: bg === "transparent" });
}

await png("icon.svg", 1024, "icon-1024.png");
await png("favicon.svg", 180, "apple-touch-icon.png");
await png("favicon.svg", 32, "favicon-32.png");
await png("tray.svg", 22, "tray.png");
await png("tray.svg", 44, "tray@2x.png");
await png("mark.svg", 512, "mark-512.png");
await png("mark-on-dark.svg", 512, "mark-on-dark-512.png");

// Terminal bitmap: rasterise the mark small and classify each pixel as crescent
// (#), clove (R) or empty (.), so the CLI and TUI draw the same shape.
const W = Number(process.env.BITMAP ?? 16);
await page.setViewportSize({ width: 400, height: 400 });
await page.setContent(`<canvas id="c" width="${W}" height="${W}"></canvas>`);
const markSvg = svg("mark.svg");
const rows = await page.evaluate(
  async ({ src, W }) => {
    const img = new Image();
    img.src = `data:image/svg+xml;base64,${btoa(src)}`;
    await img.decode();
    const ctx = document.getElementById("c").getContext("2d");
    ctx.drawImage(img, 0, 0, W, W);
    const d = ctx.getImageData(0, 0, W, W).data;
    const lines = [];
    for (let y = 0; y < W; y++) {
      let l = "";
      for (let x = 0; x < W; x++) {
        const [r, g, b, a] = d.slice((y * W + x) * 4, (y * W + x) * 4 + 4);
        if (a < 110) l += ".";
        else if (g > r + 30) l += "R";
        else l += "#";
      }
      lines.push(l);
    }
    return lines;
  },
  { src: markSvg, W },
);
writeFileSync(join(out, `bitmap-${W}.txt`), `${rows.join("\n")}\n`);
console.log(rows.join("\n"));

// Preview sheet
const sheet = `<style>body{margin:0;display:flex;gap:28px;padding:28px;background:#e7eae1;font:12px monospace;align-items:end;flex-wrap:wrap}
.d{background:#16201b;padding:20px;border-radius:12px}.l{background:#f1f2ec;padding:20px;border-radius:12px}</style>
<div class="l"><div style="width:180px">${svg("mark.svg")}</div></div>
<div class="d"><div style="width:180px">${svg("mark-on-dark.svg")}</div></div>
<div><img src="data:image/png;base64,${readFileSync(join(out, "icon-1024.png")).toString("base64")}" width="256"></div>
<div><img src="data:image/png;base64,${readFileSync(join(out, "icon-1024.png")).toString("base64")}" width="64"></div>
<div><img src="data:image/png;base64,${readFileSync(join(out, "icon-1024.png")).toString("base64")}" width="32"></div>
<div><img src="data:image/png;base64,${readFileSync(join(out, "favicon-32.png")).toString("base64")}" width="16"></div>
<div class="l"><img src="data:image/png;base64,${readFileSync(join(out, "tray@2x.png")).toString("base64")}" width="22"></div>`;
await page.setViewportSize({ width: 1000, height: 320 });
await page.setContent(sheet);
await page.screenshot({ path: join(out, "preview.png") });
await browser.close();
