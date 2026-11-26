// Renders one still near the end of each scene, plus a contact sheet: out/qa/.
//   node scripts/qa.mjs [scene ...]
import { bundle } from "@remotion/bundler";
import { renderStill, selectComposition } from "@remotion/renderer";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync } from "node:fs";

const script = JSON.parse(readFileSync("src/script.json", "utf8"));
const { durations } = JSON.parse(readFileSync("src/durations.json", "utf8"));
const root = readFileSync("src/Root.tsx", "utf8");
const TAIL = Object.fromEntries([...root.matchAll(/^\s+(\w+): ([\d.]+),$/gm)].map((m) => [m[1], +m[2]]));
const LEAD = 10, XFADE = 14, FPS = 30;
const only = process.argv.slice(2);

mkdirSync("out/qa", { recursive: true });
const serveUrl = await bundle({ entryPoint: "src/index.ts" });
const composition = await selectComposition({ serveUrl, id: "Intro" });
let start = 0;
const files = [];
for (const { id } of script) {
  const frames = LEAD + Math.round(((durations[id] ?? 3) + (TAIL[id] ?? 1.4)) * FPS) + XFADE;
  const frame = start + frames - XFADE - 12;
  if (!only.length || only.includes(id)) {
    const output = `out/qa/${String(files.length).padStart(2, "0")}-${id}.png`;
    await renderStill({ serveUrl, composition, frame, output });
    files.push(output);
    console.log(id, frame);
  }
  start += frames - XFADE;
}
if (files.length > 1) {
  const cols = 4, rows = Math.ceil(files.length / cols);
  execFileSync("ffmpeg", ["-y", "-loglevel", "error", ...files.flatMap((f) => ["-i", f]),
    "-filter_complex", files.map((_, i) => `[${i}]scale=640:360[s${i}]`).join(";") + ";" + files.map((_, i) => `[s${i}]`).join("") +
    `xstack=inputs=${files.length}:layout=${files.map((_, i) => `${(i % cols) * 640}_${Math.floor(i / cols) * 360}`).join("|")}:fill=black`,
    "out/qa/sheet.png"]);
}
