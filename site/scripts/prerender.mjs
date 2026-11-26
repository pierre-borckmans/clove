// Renders every page to static HTML at build time: the landing page and one
// per docs page. The client gets only the small enhancement script, so pages
// read fine with JavaScript off.
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";

const root = new URL("..", import.meta.url);
const { render, routes } = await import(new URL(".ssr/entry-server.js", root).href);
const shell = await readFile(new URL("dist/index.html", root), "utf8");
if (!shell.includes("<!--app-->")) throw new Error("dist/index.html has no <!--app--> marker");

for (const { path, title } of routes) {
  const dir = new URL(`dist${path}`, root);
  await mkdir(dir, { recursive: true });
  const html = shell
    .replace("<!--app-->", render(path))
    .replace(/<title>[^<]*<\/title>/, `<title>${title}</title>`)
    .replace(
      /<link rel="canonical" href="[^"]*"/,
      `<link rel="canonical" href="https://clove.fast${path}"`,
    );
  await writeFile(new URL("index.html", dir), html);
  console.log(`prerendered ${path}`);
}
await rm(new URL(".ssr", root), { recursive: true, force: true });
