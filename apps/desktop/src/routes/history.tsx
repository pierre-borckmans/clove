import { Copy, Gauge, History } from "lucide-react";
import { PaneHeader } from "@/components/app-shell";
import { Empty } from "@/components/empty";
import { RepoMark } from "@/components/sidebar";
import { Skeleton } from "@/components/ui/skeleton";
import { since } from "@/lib/format";
import { copyText, useHistory, useStats } from "@/lib/queries";
import type { Created, Removed, RepoStats } from "@/lib/types";
import { cn } from "@/lib/utils";

const BY: Record<Removed["by"], string> = {
  hand: "Removed",
  prune: "Pruned",
  "auto-prune": "Auto-pruned",
  "claude-hook": "Removed by Claude Code",
};

/** `clove log --all-repos`: what clove removed, when, by what, why, and how to get the branch back. */
export function HistoryPage() {
  const { data, isLoading } = useHistory();
  return (
    <>
      <PaneHeader>
        <h1 className="text-[14px] font-semibold tracking-tight">History</h1>
        <span className="text-[12.5px] text-muted-foreground">
          How fast clones come, and what clove removed, in every repository
        </span>
        <div data-tauri-drag-region className="h-full flex-1" />
      </PaneHeader>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-[860px] flex-col gap-4 px-6 pt-5 pb-16">
          <Timings />
          <h2 className="mt-2 text-[13px] font-medium">Removed</h2>
          {isLoading ? (
            <Skeleton className="h-40 rounded-xl" />
          ) : !data?.length ? (
            <div className="rounded-xl border border-dashed">
              <Empty icon={<History />} title="Nothing removed yet">
                Removals by hand, by cleanup, by auto-prune and by Claude Code show up here, with the command that
                brings a deleted branch back.
              </Empty>
            </div>
          ) : (
            <section className="overflow-hidden rounded-xl border bg-card">
              <RemovalList entries={data} />
            </section>
          )}
          <p className="text-[12px] text-faint">
            Same as <span className="font-mono">clove stats --all-repos</span> and{" "}
            <span className="font-mono">clove log --all-repos</span>. Agents read it with{" "}
            <span className="font-mono">clove log --json</span>.
          </p>
        </div>
      </div>
    </>
  );
}

export function RemovalList({ entries }: { entries: Removed[] }) {
  return (
    <ul>
      {entries.map((e) => (
        <li
          key={`${e.root}-${e.at}-${e.name}`}
          className="flex items-center gap-3 border-b px-4 py-2.5 text-[13px] last:border-b-0"
        >
          <span className={cn("size-1.5 shrink-0 rounded-full", e.by === "auto-prune" ? "bg-amber" : "bg-faint")} />
          <span className="w-[72px] shrink-0 text-[12px] text-faint">{since(e.at)}</span>
          <RepoMark name={e.repo} />
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="truncate">
              <span className={e.by === "auto-prune" ? "text-amber" : "text-muted-foreground"}>{BY[e.by]} </span>
              <span className="font-medium">{e.name}</span>
              {e.kind === "worktree" && <span className="text-faint"> (git worktree)</span>}
              <span className="text-faint"> · {e.repo}</span>
            </span>
            <span className="truncate text-[12px] text-muted-foreground">
              {[e.reason, e.branch ?? "detached"].filter(Boolean).join(" · ")}
            </span>
          </span>
          {e.restore ? (
            <button
              type="button"
              title="Copy the command that brings the branch back"
              onClick={() => void copyText(e.restore as string, "Command copied")}
              className="flex shrink-0 items-center gap-1.5 rounded-md px-2 py-1 font-mono text-[11.5px] text-faint transition-colors hover:bg-foreground/[0.045] hover:text-foreground"
            >
              {e.restore}
              <Copy className="size-3" />
            </button>
          ) : e.branch && !e.branch_deleted ? (
            <span className="shrink-0 text-[12px] text-faint">branch kept</span>
          ) : null}
        </li>
      ))}
    </ul>
  );
}

const FAST_MS = 5000;
const RERAN_MS = 1000;

function secs(ms: number): string {
  if (ms < 1000) return `${(ms / 1000).toFixed(2)}s`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m${String(Math.floor((ms % 60_000) / 1000)).padStart(2, "0")}s`;
}

const setupMs = (c: Created) => c.timings.filter((t) => t.step.startsWith("setup ")).reduce((n, t) => n + t.ms, 0);
const stepMs = (c: Created, step: string) => c.timings.find((t) => t.step === step)?.ms ?? 0;

/** `clove stats`: per repo with clones, how long they take and where the time goes. */
function Timings() {
  const { data, isLoading } = useStats();
  const repos = data?.filter((r) => r.stats.clones > 0) ?? [];
  if (isLoading) return <Skeleton className="h-56 rounded-xl" />;
  return (
    <>
      <h2 className="text-[13px] font-medium">Clone timings</h2>
      {repos.length === 0 ? (
        <div className="rounded-xl border border-dashed">
          <Empty icon={<Gauge />} title="No clones made yet">
            Every new clone's timings land here: how long it took, and which step took it.
          </Empty>
        </div>
      ) : (
        repos.map((r) => <RepoTimings key={r.root} data={r} />)
      )}
    </>
  );
}

function RepoTimings({ data: { repo, stats: s, created } }: { data: RepoStats }) {
  // One scale for the whole card: a clone that reran nothing. Setup reruns
  // overflow it, which is the point.
  const scale = Math.max(1, s.clean_median_ms ?? s.median_ms);
  const steps = s.steps.filter((x) => x.max_ms >= 5);
  return (
    <section className="overflow-hidden rounded-xl border bg-card">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 border-b px-4 py-3">
        <span className="flex items-center gap-2 self-center">
          <RepoMark name={repo} />
          <span className="text-[13.5px] font-medium">{repo}</span>
        </span>
        <span className="text-[12.5px] text-muted-foreground tabular-nums">
          {s.clones} clone{s.clones === 1 ? "" : "s"} · median{" "}
          <span className="font-medium text-foreground">{secs(s.median_ms)}</span> · p90 {secs(s.p90_ms)} · {s.fast} of{" "}
          {s.clones} under {secs(FAST_MS)}
        </span>
        <div className="flex-1" />
        {s.clean_median_ms !== null && (
          <span className="text-[12.5px] tabular-nums">
            <span className="text-sage">clove itself </span>
            <span className="font-medium">{secs(s.clean_median_ms)}</span>
          </span>
        )}
      </div>
      {s.reran > 0 && (
        <p className="border-b px-4 py-2 text-[12px] text-muted-foreground">
          <span className="text-amber">
            Setup reran in {s.reran} of {s.clones}
          </span>{" "}
          (over {secs(RERAN_MS)}): those branches changed what the golden image had set up, so a step ran again in the
          clone.
        </p>
      )}
      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.15fr)] gap-x-6 px-4 py-3 max-md:grid-cols-1">
        <div>
          <h3 className="mb-1.5 text-[11.5px] text-faint">Where the time goes · median when the step ran</h3>
          <ul className="flex flex-col gap-1">
            {steps.map((x) => {
              const over = x.median_ms > scale;
              const setup = x.step.startsWith("setup ");
              return (
                <li key={x.step} className="grid grid-cols-[112px_44px_minmax(0,1fr)] items-center gap-2 text-[12px]">
                  <span className="truncate text-muted-foreground" title={x.step}>
                    {x.step}
                  </span>
                  <span className="text-right font-mono tabular-nums">{secs(x.median_ms)}</span>
                  <span
                    className="flex items-center gap-1.5"
                    title={`p90 ${secs(x.p90_ms)} · ran in ${x.runs} of ${s.clones}`}
                  >
                    <span
                      className={cn(
                        "h-1.5 rounded-full",
                        setup ? "bg-amber/70" : "bg-bottle/70",
                        over && "rounded-r-none",
                      )}
                      style={{ width: `${Math.min(100, (x.median_ms / scale) * 100)}%`, minWidth: 3 }}
                    />
                    {over && <span className="text-[10px] text-amber">▸</span>}
                    {x.runs < s.clones && (
                      <span className="shrink-0 text-[10.5px] text-faint">
                        {x.runs}/{s.clones}
                      </span>
                    )}
                  </span>
                </li>
              );
            })}
          </ul>
        </div>
        <div className="max-md:mt-3">
          <h3 className="mb-1.5 text-[11.5px] text-faint">Recent</h3>
          <ul className="flex flex-col gap-1">
            {created.map((c) => (
              <CreatedRow key={`${c.at}-${c.name}`} c={c} scale={scale} />
            ))}
          </ul>
          <div className="mt-2 flex gap-3 text-[10.5px] text-faint">
            {[
              ["bg-bottle", "mount"],
              ["bg-sage/60", "checkout"],
              ["bg-amber/80", "setup"],
              ["bg-foreground/25", "other"],
            ].map(([c, l]) => (
              <span key={l} className="flex items-center gap-1">
                <span className={cn("size-2 rounded-sm", c)} />
                {l}
              </span>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}

/** One creation: its total, and a bar split into mount, checkout, setup and the rest. */
function CreatedRow({ c, scale }: { c: Created; scale: number }) {
  const mount = stepMs(c, "mount");
  const checkout = stepMs(c, "checkout");
  const setup = setupMs(c);
  const other = Math.max(0, c.total_ms - mount - checkout - setup);
  // Twice the scale fits the row; past it the bar is cut and says so.
  const room = scale * 2;
  const part = (ms: number) => `${(Math.min(ms, room) / room) * 100}%`;
  const cut = c.total_ms > room;
  const tone = c.total_ms < FAST_MS ? "text-sage" : setup > RERAN_MS ? "text-amber" : "text-destructive";
  return (
    <li
      className="grid grid-cols-[64px_minmax(0,1fr)_46px_minmax(0,1fr)] items-center gap-2 text-[12px]"
      title={c.timings.map((t) => `${t.step} ${secs(t.ms)}`).join(" · ")}
    >
      <span className="text-[11.5px] text-faint">{since(c.at)}</span>
      <span className="truncate font-medium">{c.name}</span>
      <span className={cn("text-right font-mono tabular-nums", tone)}>{secs(c.total_ms)}</span>
      <span className="flex h-2 items-center overflow-hidden rounded-full bg-foreground/[0.05]">
        <span className="h-full bg-bottle" style={{ width: part(mount) }} />
        <span className="h-full bg-sage/60" style={{ width: part(checkout) }} />
        <span className="h-full bg-amber/80" style={{ width: part(setup) }} />
        <span className="h-full bg-foreground/25" style={{ width: part(other) }} />
        {cut && <span className="ml-auto pr-0.5 text-[9px] leading-none text-amber">▸</span>}
      </span>
    </li>
  );
}
