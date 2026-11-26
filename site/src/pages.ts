// The docs pages, rendered from the repo's own Markdown (README.md and
// docs/*.md) at build time, so the site never drifts from the repo.
import { Marked, type Tokens } from "marked";
import readme from "../../README.md?raw";
import { REPO } from "./components";

const files = import.meta.glob("../../docs/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** Sidebar order; the slug is the file name, "" for the README overview. */
const ORDER = [
  "",
  "commands",
  "configuration",
  "freshness",
  "cleanup",
  "agents",
  "internals",
  "limits",
];
const LABELS: Record<string, string> = { "": "Overview" };

export type Heading = { depth: number; text: string; id: string };
export type DocPage = {
  slug: string;
  title: string;
  label: string;
  html: string;
  headings: Heading[];
};

export function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/<[^>]+>/g, "")
    .replace(/[^\p{L}\p{N}\s-]/gu, "")
    .trim()
    .replace(/\s+/g, "-");
}

/** Repo-relative Markdown links become site URLs; anything else in the repo goes to GitHub. */
function rewrite(href: string, fromDocs: boolean): string {
  if (/^[a-z]+:|^#|^\//i.test(href)) return href;
  const [path, hash] = href.split("#");
  const anchor = hash ? `#${hash}` : "";
  const clean = fromDocs ? path.replace(/^\.\//, "") : path;
  if (/^(\.\.\/)?README\.md$/.test(clean)) return `/docs/${anchor}`;
  const m = clean.match(/^(?:docs\/)?([\w-]+)\.md$/);
  if (m && (fromDocs || clean.startsWith("docs/"))) return `/docs/${m[1]}/${anchor}`;
  const repoPath = fromDocs ? `docs/${clean}`.replace(/^docs\/\.\.\//, "") : clean;
  return `${REPO}/blob/main/${repoPath}${anchor}`;
}

/** Text content only needs <, > and & escaped; quotes stay as typed. */
const escText = (t: string) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const esc = (t: string) =>
  t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** A shell command: `clove` and its subcommand in green, flags in gold. */
function hlCmd(cmd: string): string {
  return escText(cmd)
    .replace(/^(\$ )/, '<span class="t-dim">$1</span>')
    .replace(/(^|\s|<\/span>)(clove)( [a-z][\w-]*)?/, '$1<span class="t-cmd">$2$3</span>')
    .replace(/(\s)(--?[\w-]+)/g, '$1<span class="t-flag">$2</span>');
}

/** Blocks made of `command   # what it does` lines, under `# section` headers. */
function cmdRef(text: string): string | null {
  const lines = text.split("\n").filter((l) => l.trim());
  const rows = lines.filter((l) => /^\S.*?\s{2,}# /.test(l));
  if (rows.length < 3 || rows.length + lines.filter((l) => /^# /.test(l)).length < lines.length)
    return null;
  const inline = (t: string) => esc(t).replace(/`([^`]+)`/g, "<code>$1</code>");
  return `<div class="cmdref">${lines
    .map((l) => {
      if (/^# /.test(l)) return `<div class="cmdref-sec">${esc(l.slice(2))}</div>`;
      const m = l.match(/^(.*?)\s{2,}# (.*)$/) as RegExpMatchArray;
      return `<div class="cmdref-row"><code>${hlCmd(m[1].trim())}</code><span>${inline(m[2])}</span></div>`;
    })
    .join("")}</div>`;
}

/** A TOML value: strings in gold, numbers and booleans in red. */
function hlValue(v: string): string {
  return v
    .split(/("[^"]*")/)
    .map((part) =>
      part.startsWith('"')
        ? `<span class="t-str">${escText(part)}</span>`
        : escText(part).replace(/\b(true|false|\d+)\b/g, '<span class="t-num">$1</span>'),
    )
    .join("");
}

function hlToml(text: string): string {
  return text
    .split("\n")
    .map((l) => {
      const ci = l.search(/(^|\s)# /);
      const code = ci >= 0 ? l.slice(0, ci) : l;
      const comment = ci >= 0 ? `<span class="t-dim">${escText(l.slice(ci))}</span>` : "";
      if (/^\s*\[/.test(code)) return `<span class="t-dim">${escText(code)}</span>${comment}`;
      const m = code.match(/^(\s*)([\w.-]+)(\s*=\s*)(.*)$/);
      if (!m) return escText(code) + comment;
      return `${m[1]}<span class="t-key">${m[2]}</span>${m[3]}${hlValue(m[4])}${comment}`;
    })
    .join("\n");
}

function hlShell(text: string): string {
  return text
    .split("\n")
    .map((l) => {
      const ci = l.search(/(^|\s{2,})# /);
      const code = ci >= 0 ? l.slice(0, ci) : l;
      const comment = ci >= 0 ? l.slice(ci) : "";
      let c: string;
      if (/^\s*✓/.test(code)) c = `<span class="t-ok">${escText(code)}</span>`;
      else if (/^\s*[·+]/.test(code)) c = `<span class="t-dim">${escText(code)}</span>`;
      else if (/^\s*●/.test(code)) c = `<span class="t-cmd">${escText(code)}</span>`;
      else c = hlCmd(code);
      return c + (comment ? `<span class="t-dim">${escText(comment)}</span>` : "");
    })
    .join("\n");
}

/** Copies the commands, without prompts or output. */
function copyText(text: string): string {
  const lines = text.split("\n");
  const prompted = lines.filter((l) => l.startsWith("$ ")).map((l) => l.slice(2));
  return prompted.length ? prompted.join("\n") : text;
}

function codeBlock(text: string, lang: string | undefined): string {
  const ref = !lang && cmdRef(text);
  if (ref) return ref;
  const label = lang || (text.startsWith("$ ") ? "terminal" : "shell");
  const body = lang === "toml" ? hlToml(text) : lang ? escText(text) : hlShell(text);
  return `<figure class="code"><div class="code-head"><span>${label}</span><button type="button" data-copy="${esc(copyText(text))}" hidden>copy</button></div><pre><code>${body}</code></pre></figure>\n`;
}

function render(md: string, fromDocs: boolean): { html: string; headings: Heading[] } {
  const headings: Heading[] = [];
  const marked = new Marked({ gfm: true });
  marked.use({
    walkTokens(token) {
      if (token.type === "link")
        (token as Tokens.Link).href = rewrite((token as Tokens.Link).href, fromDocs);
    },
    renderer: {
      code({ text, lang }) {
        return codeBlock(text, lang || undefined);
      },
      heading({ tokens, depth, text }) {
        const id = slugify(text);
        const inner = this.parser.parseInline(tokens);
        if (depth <= 3) headings.push({ depth, text: inner.replace(/<[^>]+>/g, ""), id });
        return `<h${depth} id="${id}"><a class="anchor" href="#${id}" aria-hidden="true">#</a>${inner}</h${depth}>\n`;
      },
    },
  });
  let html = marked.parse(md, { async: false }) as string;
  // A list item's bold lead-in becomes its title, without the trailing colon or comma.
  // Not when the sentence carries on after it ("**…**, so the cost…").
  html = html.replace(
    /<li>(<p>)?<strong>((?:(?!<\/strong>)[\s\S])+?)[:,.]?<\/strong>(?![,;])\s*/g,
    '<li>$1<strong class="lead">$2</strong>',
  );
  // "→ [Page](url)" becomes a small "Page →" link.
  html = html.replace(
    /\s*→\s*<a href="([^"]+)">([^<]+)<\/a>/g,
    ' <a class="more" href="$1">$2&nbsp;→</a>',
  );
  return { html, headings };
}

/** Drops what only makes sense on GitHub: the logo, badges and back links. */
function tidy(md: string): { title: string; body: string } {
  const lines = md
    .replace(/<picture>[\s\S]*?<\/picture>\s*/g, "")
    .split("\n")
    .filter((l) => !/^\[!\[/.test(l) && !/^\[← README\]/.test(l));
  const i = lines.findIndex((l) => l.startsWith("# "));
  const title = i >= 0 ? lines[i].slice(2).trim() : "";
  if (i >= 0) lines.splice(i, 1);
  return { title, body: lines.join("\n").trim() };
}

export const pages: DocPage[] = ORDER.map((slug) => {
  const raw = slug === "" ? readme : files[`../../docs/${slug}.md`];
  if (!raw) throw new Error(`no docs/${slug}.md`);
  const { title, body } = tidy(raw);
  const { html, headings } = render(body, slug !== "");
  const t = slug === "" ? "Overview" : title;
  return { slug, title: t, label: LABELS[slug] ?? t, html, headings };
});

export function pageFor(path: string): DocPage | undefined {
  const m = path.match(/^\/docs\/?([\w-]*)\/?$/);
  if (!m) return undefined;
  return pages.find((p) => p.slug === m[1]);
}
