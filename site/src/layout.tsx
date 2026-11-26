// What every page shares: the video's backdrop, the nav and the footer links.
import type { ReactNode } from "react";
import { REPO } from "./components";
import { BookIcon, GithubIcon } from "./icons";
import { Mark } from "./visuals";

export function Backdrop() {
  return (
    <div className="backdrop" aria-hidden="true">
      <div className="blob b1" />
      <div className="blob b2" />
      <div className="blob b3" />
      <div className="grid" />
      <div className="grain" />
      <div className="vignette" />
    </div>
  );
}

function IconLink({
  href,
  icon,
  children,
}: {
  href: string;
  icon: ReactNode;
  children: ReactNode;
}) {
  return (
    <a
      className="inline-flex items-center gap-2 leading-none text-muted transition-colors hover:text-ink"
      href={href}
    >
      {/* Martian Mono sits low in its line box; nudge the icon to its glyphs' centre. */}
      <span className="flex translate-y-[1px] opacity-80">{icon}</span>
      {children}
    </a>
  );
}

export function DocsLink() {
  return (
    <IconLink href="/docs/" icon={<BookIcon />}>
      docs
    </IconLink>
  );
}

export function GithubLink({ label = "github" }: { label?: string }) {
  return (
    <IconLink href={REPO} icon={<GithubIcon />}>
      {label}
    </IconLink>
  );
}

export function Nav({ wide = false }: { wide?: boolean }) {
  return (
    <>
      <nav
        data-topnav
        className={`mx-auto flex items-center justify-between px-5 py-6 sm:px-8 ${wide ? "max-w-[84rem]" : "max-w-[76rem]"}`}
      >
        <a href="/" className="flex items-center gap-2.5 text-[1.6rem] text-ink no-underline">
          <Mark size={30} /> clove
        </a>
        <div className="flex items-center gap-7 font-mono text-[0.8rem]">
          <DocsLink />
          <GithubLink />
        </div>
      </nav>
      <PillNav />
    </>
  );
}

/** Once the top nav scrolls away, a small pill drops in at the top centre. */
function PillNav() {
  return (
    <nav className="pill-nav" aria-label="Quick links">
      <a href="/" className="flex items-center gap-2 text-ink" aria-label="clove home">
        <Mark size={20} />
        <span className="text-[1rem]">clove</span>
      </a>
      <span className="h-4 w-px bg-[rgba(233,238,232,0.15)]" />
      <a href="/docs/" className="text-muted hover:text-ink" aria-label="docs">
        <BookIcon />
      </a>
      <a href={REPO} className="text-muted hover:text-ink" aria-label="github">
        <GithubIcon />
      </a>
    </nav>
  );
}

export function FooterLinks() {
  return (
    <div className="flex flex-wrap justify-center gap-8 font-mono text-[0.8rem]">
      <GithubLink label="github.com/pierre-borckmans/clove" />
      <DocsLink />
      <span className="text-dim">MIT</span>
    </div>
  );
}
