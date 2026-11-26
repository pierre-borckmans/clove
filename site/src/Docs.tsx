import { REPO } from "./components";
import { Backdrop, FooterLinks, Nav } from "./layout";
import type { DocPage } from "./pages";
import { pages } from "./pages";

const href = (slug: string) => (slug ? `/docs/${slug}/` : "/docs/");

function Sidebar({ current }: { current: string }) {
  return (
    <nav aria-label="Docs" className="font-mono text-[0.82rem]">
      <p className="kicker mb-4 !text-[0.66rem]">docs</p>
      <ul className="m-0 grid list-none gap-1 p-0">
        {pages.map((p) => {
          const on = p.slug === current;
          return (
            <li key={p.slug}>
              <a
                href={href(p.slug)}
                aria-current={on ? "page" : undefined}
                className={`block rounded-md border px-3 py-1.5 transition-colors ${on ? "border-bottle/40 bg-bottle/10 text-ink" : "border-transparent text-muted hover:text-ink"}`}
              >
                {p.label}
              </a>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

function OnThisPage({ page }: { page: DocPage }) {
  const hs = page.headings.filter((h) => h.depth === 2);
  if (hs.length < 2) return null;
  return (
    <aside className="hidden font-mono text-[0.75rem] xl:block">
      <div className="sticky top-20">
        <p className="mb-3 text-dim">on this page</p>
        <ul className="m-0 grid list-none gap-2 p-0">
          {hs.map((h) => (
            <li key={h.id}>
              <a href={`#${h.id}`} data-toc={h.id} className="toc-link text-muted hover:text-ink">
                {h.text}
              </a>
            </li>
          ))}
        </ul>
      </div>
    </aside>
  );
}

export function Docs({ page }: { page: DocPage }) {
  const i = pages.indexOf(page);
  const prev = pages[i - 1];
  const next = pages[i + 1];
  const source = page.slug ? `docs/${page.slug}.md` : "README.md";
  return (
    <>
      <Backdrop />
      <Nav wide />
      <div className="mx-auto grid max-w-[84rem] gap-10 px-5 pt-6 pb-24 sm:px-8 lg:grid-cols-[13rem_minmax(0,1fr)] xl:grid-cols-[13rem_minmax(0,1fr)_12rem]">
        <div className="lg:sticky lg:top-20 lg:self-start">
          <Sidebar current={page.slug} />
        </div>
        <main className="min-w-0">
          <h1 className="m-0 text-[2.8rem] leading-[1.05] font-normal tracking-[-0.02em] sm:text-[3.6rem]">
            {page.title}
          </h1>
          {/* Rendered from the repo's own Markdown at build time. */}
          {/* biome-ignore lint/security/noDangerouslySetInnerHtml: our own docs, rendered at build time */}
          <article className="doc mt-8" dangerouslySetInnerHTML={{ __html: page.html }} />
          <div className="mt-16 flex flex-wrap items-center justify-between gap-4 border-t border-rule pt-6 font-mono text-[0.8rem]">
            {prev ? (
              <a className="text-muted hover:text-ink" href={href(prev.slug)}>
                ← {prev.label}
              </a>
            ) : (
              <span />
            )}
            <a className="text-dim hover:text-ink" href={`${REPO}/edit/main/${source}`}>
              edit this page
            </a>
            {next ? (
              <a className="text-muted hover:text-ink" href={href(next.slug)}>
                {next.label} →
              </a>
            ) : (
              <span />
            )}
          </div>
        </main>
        <OnThisPage page={page} />
      </div>
      <footer className="pb-14">
        <FooterLinks />
      </footer>
    </>
  );
}
