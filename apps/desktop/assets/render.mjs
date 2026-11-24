// Renders the app icon and the tray template icon from glyph.svg.
import { readFileSync } from "node:fs";
import { chromium } from "playwright";

const dir = new URL(".", import.meta.url).pathname;
const glyph = readFileSync(`${dir}glyph.svg`, "utf8").replace(/<svg[^>]*>|<\/svg>/g, "");
const browser = await chromium.launch();
const page = await browser.newPage({ deviceScaleFactor: 1 });

async function shot(html, size, out) {
  await page.setViewportSize({ width: size, height: size });
  await page.setContent(`<html><body style="margin:0;background:transparent">${html}</body></html>`);
  await page.screenshot({ path: out, omitBackground: true });
}

const app = `
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#c25f34"/><stop offset="1" stop-color="#9e4521"/>
    </linearGradient>
  </defs>
  <rect x="100" y="100" width="824" height="824" rx="185" fill="url(#bg)"/>
  <rect x="100.5" y="100.5" width="823" height="823" rx="185" fill="none" stroke="#fff" stroke-opacity=".12"/>
  <g transform="translate(212 212) scale(6)" color="#f6ead9">${glyph}</g>
</svg>`;
await shot(app, 1024, `${dir}icon.png`);

const tray = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="36" height="36" color="#000">${glyph}</svg>`;
await shot(tray, 36, `${dir}../src-tauri/icons/tray.png`);
await browser.close();
