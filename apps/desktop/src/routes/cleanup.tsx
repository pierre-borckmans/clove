import { useMutation, useQueries, useQueryClient } from "@tanstack/react-query";
import { BrushCleaning, CircleCheck, GitMerge, Loader, TriangleAlert } from "lucide-react";
import { toast } from "sonner";
import { PaneHeader } from "@/components/app-shell";
import { Empty } from "@/components/empty";
import { RepoMark } from "@/components/sidebar";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { api } from "@/lib/api";
import { bytes } from "@/lib/format";
import { keys, useRepos } from "@/lib/queries";
import type { Cleanup, MergeCheck, RepoSummary } from "@/lib/types";
import { cn } from "@/lib/utils";

/** Something cleanup can remove: a leftover or a provably merged clone. */
const hasWork = (c: Cleanup) => c.leftovers.length > 0 || c.checks.some((m) => m.verdict === "prune");
/** Clones worth showing: merged ones, ready to go or needing a look. */
const merged = (c: Cleanup) => c.checks.filter((m) => m.verdict !== "keep");

export function CleanupPage() {
  const { data: repos = [] } = useRepos();
  const results = useQueries({
    queries: repos.map((r) => ({
      queryKey: keys.cleanup(r.root),
      queryFn: () => api.cleanup(r.root, false),
      staleTime: 30_000,
    })),
  });
  const loading = results.some((q) => q.isLoading);
  const shown = repos
    .map((r, i) => ({ repo: r, data: results[i]?.data, error: results[i]?.error }))
    .filter((x) => x.error || (x.data && (merged(x.data).length > 0 || x.data.leftovers.length > 0 || x.data.warning)));

  return (
    <>
      <PaneHeader>
        <h1 className="text-[14px] font-semibold tracking-tight">Cleanup</h1>
        <span className="text-[12.5px] text-muted-foreground">
          Clones whose pull request is merged, and leftovers from interrupted runs
        </span>
        <div data-tauri-drag-region className="h-full flex-1" />
      </PaneHeader>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-[860px] flex-col gap-6 px-6 pt-5 pb-16">
          {loading ? (
            <>
              <div className="flex items-center gap-2 text-[12.5px] text-muted-foreground">
                <Loader className="size-3.5 animate-spin" />
                Asking GitHub about pull requests…
              </div>
              <Skeleton className="h-28 rounded-xl" />
            </>
          ) : shown.length === 0 ? (
            <div className="rounded-xl border border-dashed">
              <Empty icon={<CircleCheck className="text-sage" />} title="Nothing to clean up">
                No merged clones, half-created clones, stray mount points or old golden images in{" "}
                {repos.length === 1 ? repos[0].name : `${repos.length} repositories`}.
              </Empty>
            </div>
          ) : (
            shown.map(({ repo, data, error }) =>
              error ? (
                <section
                  key={repo.id}
                  className="rounded-xl border bg-card px-4 py-3 font-mono text-[12px] text-destructive"
                >
                  {repo.name}: {String(error)}
                </section>
              ) : data ? (
                <RepoCleanup key={repo.id} repo={repo} data={data} />
              ) : null,
            )
          )}
        </div>
      </div>
    </>
  );
}

function RepoCleanup({ repo, data }: { repo: RepoSummary; data: Cleanup }) {
  const qc = useQueryClient();
  const apply = useMutation({
    mutationFn: () => api.cleanup(repo.root, true),
    onSuccess: (done) => {
      qc.setQueryData(keys.cleanup(repo.root), done);
      const parts = [];
      if (done.removed.length)
        parts.push(
          `${done.removed.length} merged clone${done.removed.length === 1 ? "" : "s"} and branch${done.removed.length === 1 ? "" : "es"}`,
        );
      if (done.leftovers.length)
        parts.push(`${done.leftovers.length} leftover${done.leftovers.length === 1 ? "" : "s"}`);
      toast.success(`Cleaned up ${repo.name}`, {
        description: parts.length ? `Removed ${parts.join(" and ")}` : "Nothing left to remove",
      });
      for (const k of done.kept) toast.warning("Kept a merged clone", { description: k });
    },
    onError: (e) => toast.error(`Cleanup of ${repo.name} failed`, { description: String(e) }),
    onSettled: () => qc.invalidateQueries({ predicate: (q) => q.queryKey[0] !== "cleanup" }),
  });
  const result = apply.data ?? data;
  const clones = merged(result);
  const ready = clones.filter((m) => m.verdict === "prune" && !result.removed.includes(m.clone));
  const kept = result.checks.length - clones.length;
  const work = !apply.data && hasWork(result);

  return (
    <section className="overflow-hidden rounded-xl border bg-card">
      <div className="flex items-center gap-2.5 border-b px-4 py-2.5">
        <RepoMark name={repo.name} />
        <span className="text-[13px] font-medium">{repo.name}</span>
        <span className="text-[12px] text-muted-foreground">
          {[
            ready.length && `${ready.length} merged`,
            result.leftovers.length && `${result.leftovers.length} leftover${result.leftovers.length === 1 ? "" : "s"}`,
          ]
            .filter(Boolean)
            .join(" · ")}
        </span>
        <div className="flex-1" />
        {work && (
          <Button
            size="sm"
            className="h-7 gap-1.5 text-[12.5px]"
            disabled={apply.isPending}
            onClick={() => apply.mutate()}
          >
            {apply.isPending ? <Loader className="size-3.5 animate-spin" /> : <BrushCleaning className="size-3.5" />}
            Remove
          </Button>
        )}
      </div>
      {result.warning && (
        <div className="flex items-start gap-2 border-b bg-amber/5 px-4 py-2.5 text-[12.5px] text-amber">
          <TriangleAlert className="mt-px size-3.5 shrink-0" />
          <span className="selectable">{result.warning}</span>
        </div>
      )}
      {clones.length > 0 && (
        <>
          <Heading>Merged clones</Heading>
          <ul>
            {clones.map((m) => (
              <MergedRow key={m.clone} check={m} removed={result.removed.includes(m.clone)} />
            ))}
          </ul>
        </>
      )}
      {kept > 0 && (
        <p className="border-b px-4 py-2 text-[12px] text-faint">
          {kept} other clone{kept === 1 ? "" : "s"} kept: open PR, no PR, or not merged
        </p>
      )}
      {result.leftovers.length > 0 && (
        <>
          <Heading>Leftovers</Heading>
          <ul>
            {result.leftovers.map((a) => (
              <li key={a} className="flex items-center gap-3 border-b px-4 py-2.5 text-[13px] last:border-b-0">
                <span className={cn("size-1.5 shrink-0 rounded-full", apply.data ? "bg-sage" : "bg-amber")} />
                <span className="selectable min-w-0 text-muted-foreground">
                  {a.split(/(\/\S+)/).map((part, i) =>
                    part.startsWith("/") ? (
                      <span key={`${i}-${part}`} className="font-mono text-[12px] text-foreground/80">
                        {part}
                      </span>
                    ) : (
                      part
                    ),
                  )}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

function Heading({ children }: { children: string }) {
  return (
    <div className="border-b bg-muted/40 px-4 py-1.5 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
      {children}
    </div>
  );
}

function MergedRow({ check: m, removed }: { check: MergeCheck; removed: boolean }) {
  const ok = m.verdict === "prune" || removed;
  return (
    <li className="grid grid-cols-[16px_minmax(0,180px)_minmax(0,1fr)_64px] items-center gap-3 border-b px-4 py-2.5 last:border-b-0">
      {ok ? <GitMerge className="size-3.5 text-sage" /> : <TriangleAlert className="size-3.5 text-amber" />}
      <span className={cn("truncate text-[13px] font-medium", removed && "text-muted-foreground line-through")}>
        {m.clone}
      </span>
      <span className={cn("truncate text-[12.5px]", ok ? "text-sage" : "text-amber")}>
        {removed ? "removed with its branch" : m.reason}
        {m.verdict === "review" && !removed && <span className="text-faint"> · kept, review by hand</span>}
      </span>
      <span className="text-right font-mono text-[12px] tabular-nums text-muted-foreground">{bytes(m.own_bytes)}</span>
    </li>
  );
}
