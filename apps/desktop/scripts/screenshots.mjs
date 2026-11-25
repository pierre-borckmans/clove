// Screenshots of the browser build (mock backend), light and dark.
// Start `pnpm dev` first, then: node scripts/screenshots.mjs [baseUrl] [only]
import { mkdirSync } from "node:fs";
import { chromium } from "playwright";

const base = process.argv[2] ?? "http://localhost:1420";
const only = process.argv[3];
const out = new URL("../screenshots/", import.meta.url).pathname;
mkdirSync(out, { recursive: true });

const MONO = "#/repo/mono-3fa81c02";
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const shots = {
  repo: async (p) => {
    await p.goto(`${base}/?theme=${p.theme}${MONO}`);
    await p.waitForSelector("text=Golden image");
    await wait(400);
  },
  "new-clone": async (p) => {
    await shots.repo(p);
    await p.keyboard.press("Meta+n");
    await wait(250);
    await p.keyboard.type("feat/search", { delay: 20 });
    await wait(300);
  },
  palette: async (p) => {
    await shots.repo(p);
    await p.keyboard.press("Meta+k");
    await wait(200);
    await p.keyboard.type("bill", { delay: 20 });
    await wait(250);
  },
  progress: async (p) => {
    await shots.repo(p);
    await p.getByRole("button", { name: "Refresh", exact: true }).click();
    await wait(5200);
  },
  "new-clone-done": async (p) => {
    await shots.repo(p);
    await p.keyboard.press("Meta+n");
    await wait(200);
    await p.keyboard.type("chore/bump-deps", { delay: 10 });
    await p.keyboard.press("Enter");
    await wait(1500);
  },
  remove: async (p) => {
    await shots.repo(p);
    await p.getByText("feat-billing-v2", { exact: true }).click();
    await p.keyboard.press("Meta+Backspace");
    await wait(300);
  },
  "row-menu": async (p) => {
    await shots.repo(p);
    await p.getByText("fix-cache-stampede", { exact: true }).click();
    await p.getByRole("option", { selected: true }).getByRole("button", { name: "More actions" }).click();
    await wait(250);
  },
  empty: async (p) => {
    await p.goto(`${base}/?theme=${p.theme}#/repo/site-0b7e55a1`);
    await p.waitForSelector("text=No clones yet");
    await wait(300);
  },
  doctor: async (p) => {
    await p.goto(`${base}/?theme=${p.theme}#/doctor`);
    await p.waitForSelector("text=This Mac");
    await wait(500);
  },
  cleanup: async (p) => {
    await p.goto(`${base}/?theme=${p.theme}#/cleanup`);
    await p.waitForSelector('text="Merged clones"');
    await wait(400);
  },
  settings: async (p) => {
    await shots.repo(p);
    await p.getByRole("link", { name: /^Settings/ }).click();
    await p.waitForSelector("text=~/.config/clove/repos/mono.toml");
    await wait(400);
  },
  "add-repo": async (p) => {
    await shots.repo(p);
    await p.keyboard.press("Meta+o");
    await p.getByRole("button", { name: "Build golden image" }).click();
    await wait(4200);
  },
  popover: async (p) => {
    await p.setViewportSize({ width: 360, height: 520 });
    await p.goto(`${base}/?theme=${p.theme}#/popover`);
    await p.waitForSelector("text=fix-cache-stampede");
    await p.getByText("fix-cache-stampede", { exact: true }).hover();
    await wait(300);
  },
};

const browser = await chromium.launch();
for (const theme of ["light", "dark"]) {
  const ctx = await browser.newContext({
    viewport: { width: 1180, height: 760 },
    deviceScaleFactor: 2,
    colorScheme: theme,
  });
  for (const [name, run] of Object.entries(shots)) {
    if (only && !only.split(",").includes(name)) continue;
    const page = await ctx.newPage();
    page.theme = theme;
    page.on("pageerror", (e) => console.error(`[${name}/${theme}]`, e.message));
    await run(page);
    await page.screenshot({ path: `${out}${name}-${theme}.png` });
    console.log(`${name}-${theme}.png`);
    await page.close();
  }
  await ctx.close();
}
await browser.close();
