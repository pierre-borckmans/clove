import type { ReactNode } from "react";

export const REPO = "https://github.com/pierre-borckmans/clove";
export const INSTALL = "cargo install --git https://github.com/pierre-borckmans/clove clove-cli";

/** Formats milliseconds like the CLI does (`secs` in render.rs). */
export function secs(ms: number): string {
  if (ms < 1000) return `${(ms / 1000).toFixed(2)}s`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m${String(Math.floor((ms % 60_000) / 1000)).padStart(2, "0")}s`;
}

export function Section({
  id,
  label,
  title,
  children,
}: {
  id: string;
  label: string;
  title: ReactNode;
  children: ReactNode;
}) {
  return (
    <section id={id} className="scroll-mt-8 border-t border-rule">
      <div className="mx-auto grid grid-cols-[minmax(0,1fr)] max-w-[74rem] gap-x-12 px-4 py-16 sm:px-8 sm:py-24 lg:grid-cols-[10rem_minmax(0,1fr)]">
        <p className="mb-3 font-mono text-[0.75rem] tracking-wide text-rust lg:mt-3 lg:mb-0">
          {label}
        </p>
        <div className="min-w-0">
          <h2 className="max-w-[22ch] text-[2.1rem] leading-[1.08] font-medium tracking-[-0.015em] text-balance sm:text-[2.75rem]">
            {title}
          </h2>
          <div className="mt-8">{children}</div>
        </div>
      </div>
    </section>
  );
}

export function Prose({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <div className={`prose-body max-w-[38rem] text-ink-2 ${className}`}>{children}</div>;
}

/** A command with a copy button. The button only appears once JS runs. */
export function CopyCommand({
  cmd,
  wrap = false,
  className = "",
}: {
  cmd: string;
  wrap?: boolean;
  className?: string;
}) {
  return (
    <div
      className={`flex items-stretch overflow-hidden rounded-md border border-rule bg-paper-2 font-mono text-[0.8rem] ${className}`}
    >
      <span className="select-none py-3 pr-2 pl-4 text-rust" aria-hidden="true">
        $
      </span>
      <code
        className={`raw min-w-0 flex-1 py-3 pr-4 ${wrap ? "[overflow-wrap:anywhere]" : "overflow-x-auto whitespace-nowrap"}`}
      >
        {cmd}
      </code>
      <button
        type="button"
        data-copy={cmd}
        hidden
        className="cursor-pointer border-l border-rule px-3 text-muted hover:text-ink"
      >
        copy
      </button>
    </div>
  );
}

/** Dark terminal panel. `title` is the cwd shown in the header strip. */
export function Term({
  title,
  children,
  aside,
  className = "",
  ...rest
}: {
  title: string;
  children: ReactNode;
  aside?: ReactNode;
  className?: string;
  [data: `data-${string}`]: string;
}) {
  return (
    <figure
      className={`overflow-hidden rounded-lg border border-term-rule bg-term text-term-ink shadow-[0_1px_0_rgba(0,0,0,0.04),0_18px_40px_-24px_rgba(26,23,20,0.45)] ${className}`}
      {...rest}
    >
      <div className="flex items-center justify-between gap-4 border-b border-term-rule px-4 py-2 font-mono text-[0.7rem] text-term-dim">
        <span className="truncate">{title}</span>
        {aside}
      </div>
      <pre className="overflow-x-auto px-3 py-4 font-mono text-[0.68rem] leading-[1.7] sm:px-4 sm:text-[0.8rem]">
        {children}
      </pre>
    </figure>
  );
}

// Terminal colors, matching the CLI's styles (green, dim, bold, magenta).
export const ok = "text-[#9cc2a2]";
export const dim = "text-term-dim";
export const accent = "text-[#e8936a]";

export function Prompt({ cwd }: { cwd?: string }) {
  return (
    <>
      {cwd && <span className={dim}>{cwd} </span>}
      <span className={accent}>$ </span>
    </>
  );
}

/** One `✓ step  0.70s` line, padded like the CLI's `{:<24}`. */
export function StepLine({ step, ms }: { step: string; ms: number }) {
  return (
    <span className="term-line" data-r="step" data-step={step} data-ms={ms}>
      {"  "}
      <span className={ok}>✓</span> {step.padEnd(24)} <span className={dim}>{secs(ms)}</span>
      {"\n"}
    </span>
  );
}

export function SkipLine({ step, reason }: { step: string; reason: string }) {
  return (
    <span className={`term-line ${dim}`} data-r="show">
      {"  · "}
      {step.padEnd(24)} {reason}
      {"\n"}
    </span>
  );
}

/** Minimal TOML highlighting, done at build time. */
export function Toml({ src }: { src: string }) {
  return (
    <>
      {src.split("\n").map((line, i) => {
        const key = `${i}:${line}`;
        if (line.startsWith("#")) {
          return (
            <span key={key} className={dim}>
              {line}
              {"\n"}
            </span>
          );
        }
        if (line.startsWith("[")) {
          return (
            <span key={key} className={accent}>
              {line}
              {"\n"}
            </span>
          );
        }
        const m = line.match(/^(\w+)(\s*=\s*)(.*)$/);
        if (!m) return `${line}\n`;
        return (
          <span key={key}>
            {m[1]}
            <span className={dim}>{m[2]}</span>
            <span className={ok}>{m[3]}</span>
            {"\n"}
          </span>
        );
      })}
    </>
  );
}

/** Minimal JSON highlighting: keys plain, strings green, the rest dim. */
export function Json({ value }: { value: unknown }) {
  const text = JSON.stringify(value, null, 2);
  const parts = text.split(/("(?:[^"\\]|\\.)*"(?:\s*:)?)/g);
  return (
    <>
      {parts.map((p, i) => {
        const key = `${i}`;
        if (!p.startsWith('"')) {
          return (
            <span key={key} className={dim}>
              {p}
            </span>
          );
        }
        if (p.endsWith(":")) {
          return (
            <span key={key}>
              {p.slice(0, -1)}
              <span className={dim}>:</span>
            </span>
          );
        }
        return (
          <span key={key} className={ok}>
            {p}
          </span>
        );
      })}
    </>
  );
}
