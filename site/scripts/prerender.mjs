// Renders the page to static HTML at build time. The client gets only the small
// enhancement script, so the page reads fine with JavaScript off.
import { readFile, rm, writeFile } from "node:fs/promises";

const root = new URL("..", import.meta.url);
const { render } = await import(new URL(".ssr/entry-server.js", root).href);
const file = new URL("dist/index.html", root);
const html = await readFile(file, "utf8");
if (!html.includes("<!--app-->")) throw new Error("dist/index.html has no <!--app--> marker");
await writeFile(file, html.replace("<!--app-->", render()));
await rm(new URL(".ssr", root), { recursive: true, force: true });
console.log("prerendered dist/index.html");
