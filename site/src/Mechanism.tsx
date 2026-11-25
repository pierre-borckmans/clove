import type { ReactNode } from "react";

/** One stage of the pipeline: a labelled box drawn like a technical figure. */
function Stage({
  n,
  name,
  path,
  children,
  strong = false,
}: {
  n: string;
  name: string;
  path: string;
  children: ReactNode;
  strong?: boolean;
}) {
  return (
    <div
      className={`relative min-w-0 rounded-md border bg-paper p-4 ${strong ? "border-ink" : "border-rule"}`}
    >
      <div className="flex items-baseline justify-between gap-2 font-mono text-[0.7rem] text-muted">
        <span>{n}</span>
      </div>
      <h3 className="mt-1 text-[1.35rem] leading-tight font-medium">{name}</h3>
      <p className="mt-1 font-mono text-[0.7rem] text-muted">
        {path.split("/").map((part, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: static path segments
          <span key={i}>
            {i > 0 && "/"}
            {i > 0 && <wbr />}
            {part}
          </span>
        ))}
      </p>
      <div className="mt-3 text-[0.95rem] leading-snug text-ink-2">{children}</div>
    </div>
  );
}

/** Connector between stages: horizontal on wide screens, vertical when stacked. */
function Edge({ op, time }: { op: string; time: string }) {
  return (
    <div className="flex items-center gap-3 py-1 pl-6 lg:flex-col lg:justify-center lg:gap-1 lg:py-0 lg:pl-0">
      <svg
        className="h-10 w-4 shrink-0 text-bottle lg:hidden"
        viewBox="0 0 16 40"
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        <line
          x1="8"
          y1="0"
          x2="8"
          y2="33"
          stroke="currentColor"
          strokeWidth="1.5"
          vectorEffect="non-scaling-stroke"
        />
        <path d="M3 32 L8 40 L13 32 Z" fill="currentColor" />
      </svg>
      <svg
        className="hidden h-3 w-full text-bottle lg:block"
        viewBox="0 0 100 12"
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        <line
          x1="0"
          y1="6"
          x2="92"
          y2="6"
          stroke="currentColor"
          strokeWidth="1.5"
          vectorEffect="non-scaling-stroke"
        />
        <path d="M90 1 L100 6 L90 11 Z" fill="currentColor" />
      </svg>
      <div className="font-mono text-[0.72rem] leading-tight lg:text-center">
        <div className="text-ink">{op}</div>
        <div className="text-bottle">{time}</div>
      </div>
    </div>
  );
}

export function Pipeline() {
  return (
    <figure className="mt-2 xl:-ml-[13rem]">
      <div className="grid grid-cols-[minmax(0,1fr)] gap-0 lg:grid-cols-[minmax(0,1fr)_5rem_minmax(0,1fr)_5rem_minmax(0,1fr)_5rem_minmax(0,1fr)] lg:items-stretch">
        <Stage n="built once" name="Golden image" path="~/.clove/…/golden/3/image.asif" strong>
          <ul className="space-y-1">
            <li>base branch checked out</li>
            <li>
              <span className="font-mono text-[0.8rem]">node_modules</span>, 4,470 packages
            </li>
            <li>nix shell built by direnv</li>
            <li>build caches</li>
            <li className="font-mono text-[0.8rem] text-muted">3.8G allocated</li>
          </ul>
        </Stage>
        <Edge op="clonefile()" time="0.00s" />
        <Stage n="per worktree" name="Clone image" path="clones/feat-login/image.asif">
          One file. It points at the golden image's blocks and gets its own blocks only for what you
          write. A fresh one owns 6.1M.
        </Stage>
        <Edge op="mount" time="0.70s" />
        <Stage n="per worktree" name="Mounted volume" path="~/src/mono.clove/feat-login/">
          An APFS volume with the whole warmed checkout in it, deps included.
        </Stage>
        <Edge op="checkout" time="0.21s" />
        <Stage n="per worktree" name="Git worktree" path="feat-login/mono on feat/login">
          Registered with your repo. The index is copied from golden, so git doesn't rehash 35k
          files.
        </Stage>
      </div>
      <figcaption className="mt-4 text-[0.9rem] text-muted">
        Times from a release build on a monorepo with 35k tracked files.
      </figcaption>
    </figure>
  );
}

const CELLS = 16;

function Strip({ label, written }: { label: string; written: number[] }) {
  const golden = written.length === 0;
  return (
    <div className="grid grid-cols-[6.5rem_minmax(0,1fr)] items-center gap-3 sm:grid-cols-[8rem_minmax(0,1fr)]">
      <span className="truncate font-mono text-[0.72rem] text-ink-2">{label}</span>
      <div className="grid grid-cols-[repeat(16,minmax(0,1fr))] gap-1">
        {Array.from({ length: CELLS }, (_, i) => {
          const own = written.includes(i);
          const cls = golden
            ? "bg-bottle"
            : own
              ? "bg-peel"
              : "border border-dashed border-bottle/60 bg-transparent";
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed-size figure
          return <span key={i} className={`aspect-square rounded-[3px] ${cls}`} />;
        })}
      </div>
    </div>
  );
}

/** Copy-on-write in one picture: clones reference golden blocks until they write. */
export function Blocks() {
  return (
    <figure className="rounded-md border border-rule bg-paper-2/60 p-4 sm:p-6">
      <div className="space-y-2.5">
        <Strip label="golden gen 3" written={[]} />
        <Strip label="feat-login" written={[4, 5, 12]} />
        <Strip label="fix-auth" written={[9]} />
        <Strip label="agent-parser" written={[1, 2, 3, 13, 14]} />
      </div>
      <figcaption className="mt-5 flex flex-wrap gap-x-6 gap-y-2 font-mono text-[0.7rem] text-muted">
        <span className="flex items-center gap-2">
          <span className="inline-block size-3 rounded-[2px] bg-bottle" /> block on disk
        </span>
        <span className="flex items-center gap-2">
          <span className="inline-block size-3 rounded-[2px] border border-dashed border-bottle" />{" "}
          shared with golden, no extra disk
        </span>
        <span className="flex items-center gap-2">
          <span className="inline-block size-3 rounded-[2px] bg-peel" /> written in this clone
        </span>
      </figcaption>
    </figure>
  );
}
