import { Copy, History } from "lucide-react";
import { PaneHeader } from "@/components/app-shell";
import { Empty } from "@/components/empty";
import { RepoMark } from "@/components/sidebar";
import { Skeleton } from "@/components/ui/skeleton";
import { since } from "@/lib/format";
import { copyText, useHistory } from "@/lib/queries";
import type { Removed } from "@/lib/types";
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
          Clones and git worktrees clove removed, in every repository
        </span>
        <div data-tauri-drag-region className="h-full flex-1" />
      </PaneHeader>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-[860px] flex-col gap-4 px-6 pt-5 pb-16">
          {isLoading ? (
            <Skeleton className="h-40 rounded-xl" />
          ) : !data?.length ? (
            <div className="overflow-hidden rounded-xl border border-dashed border-border/90 bg-card/50">
              <Empty icon={<History />} title="Nothing removed yet">
                Removals by hand, by cleanup, by auto-prune and by Claude Code show up here, with the command that
                brings a deleted branch back.
              </Empty>
            </div>
          ) : (
            <section className="clove-card overflow-hidden">
              <RemovalList entries={data} />
            </section>
          )}
          <p className="text-[12px] text-faint">
            Same as <span className="font-mono">clove log --all-repos</span>. Agents read it with{" "}
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
          className="flex items-center gap-3 border-b border-border/65 px-4 py-3 text-[13px] transition-colors last:border-b-0 hover:bg-foreground/[0.018]"
        >
          <span
            className={cn(
              "size-2 shrink-0 rounded-full",
              e.by === "auto-prune"
                ? "bg-amber shadow-[0_0_0_3px_color-mix(in_oklab,var(--amber)_16%,transparent)]"
                : "bg-faint/70",
            )}
          />
          <span className="w-[72px] shrink-0 text-[12px] text-faint tabular-nums">{since(e.at)}</span>
          <RepoMark name={e.repo} />
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="truncate">
              <span className={e.by === "auto-prune" ? "font-medium text-amber" : "text-muted-foreground"}>
                {BY[e.by]}{" "}
              </span>
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
              className="flex shrink-0 items-center gap-1.5 rounded-md border border-border/75 bg-muted/40 px-2 py-1 font-mono text-[11px] text-muted-foreground transition-colors hover:border-bottle/30 hover:bg-bottle-soft hover:text-bottle"
            >
              {e.restore}
              <Copy className="size-3" />
            </button>
          ) : e.branch && !e.branch_deleted ? (
            <span className="shrink-0 rounded-full border border-border/75 px-2 py-0.5 text-[11px] text-faint">
              branch kept
            </span>
          ) : null}
        </li>
      ))}
    </ul>
  );
}
