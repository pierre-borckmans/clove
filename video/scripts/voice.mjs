// Speaks each line of src/script.json with macOS `say` into public/vo/<id>.wav
// and writes the measured lengths to src/durations.json, which sets each
// scene's length. VOICE=... picks the voice (default: Samantha).
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";

const voice = process.env.VOICE ?? "Samantha";
const rate = process.env.RATE ?? "185";
const lines = JSON.parse(readFileSync("src/script.json", "utf8"));
mkdirSync("public/vo", { recursive: true });
const durations = {};
for (const { id, say } of lines) {
  const aiff = `/tmp/clove-vo-${id}.aiff`;
  execFileSync("say", ["-v", voice, "-r", rate, "-o", aiff, say]);
  const wav = `public/vo/${id}.wav`;
  execFileSync("ffmpeg", ["-y", "-loglevel", "error", "-i", aiff, "-ar", "48000", "-ac", "1", wav]);
  const secs = Number(
    execFileSync("ffprobe", ["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", wav])
      .toString()
      .trim(),
  );
  durations[id] = secs;
  console.log(`${id.padEnd(9)} ${secs.toFixed(2)}s`);
}
writeFileSync("src/durations.json", JSON.stringify({ voice, durations }, null, 2) + "\n");
