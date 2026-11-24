import { useMutation, useQueries, useQueryClient } from "@tanstack/react-query";
import { BrushCleaning, CircleCheck, Loader } from "lucide-react";
import { toast } from "sonner";
import { PaneHeader } from "@/components/app-shell";
import { Empty } from "@/components/empty";
import { RepoMark } from "@/components/sidebar";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { api } from "@/lib/api";
import { keys, useRepos } from "@/lib/queries";
import type { RepoSummary } from "@/lib/types";

export function CleanupPage() {
  const { data: repos = [] } = useRepos();
  const results = useQueries({
    queries: repos.map((r) => ({ queryKey: keys.gc(r.root), queryFn: () => api.gc(r.root, false) })),
  });
  const loading = results.some((q) => q.isLoading);
  const total = results.reduce((a, q) => a + (q.data?.length ?? 0), 0);

  return (
    <>
      <PaneHeader>
        <h1 className="text-[14px] font-semibold tracking-tight">Cleanup</h1>
        <span className="text-[12.5px] text-muted-foreground">
          Leftovers from interrupted operations and old golden generations
        </span>
        <div data-tauri-drag-region className="h-full flex-1" />
      </PaneHeader>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-[860px] flex-col gap-6 px-6 pt-5 pb-16">
          {loading ? (
            <Skeleton className="h-28 rounded-xl" />
          ) : total === 0 ? (
            <div className="rounded-xl border border-dashed">
              <Empty icon={<CircleCheck className="text-sage" />} title="Nothing to clean up">
                No half-created clones, stray mount points or old golden images in{" "}
                {repos.length === 1 ? repos[0].name : `${repos.length} repositories`}.
              </Empty>
            </div>
          ) : (
            repos.map((r, i) =>
              results[i]?.data?.length ? <RepoLeftovers key={r.id} repo={r} actions={results[i].data} /> : null,
            )
          )}
        </div>
      </div>
    </>
  );
}

function RepoLeftovers({ repo, actions }: { repo: RepoSummary; actions: string[] }) {
  const qc = useQueryClient();
  const apply = useMutation({
    mutationFn: () => api.gc(repo.root, true),
    onSuccess: (done) =>
      toast.success(`Cleaned up ${repo.name}`, {
        description: `${done.length} item${done.length === 1 ? "" : "s"} removed`,
      }),
    onError: (e) => toast.error(`Cleanup of ${repo.name} failed`, { description: String(e) }),
    onSettled: () => qc.invalidateQueries(),
  });
  return (
    <section className="overflow-hidden rounded-xl border bg-card">
      <div className="flex items-center gap-2.5 border-b px-4 py-2.5">
        <RepoMark name={repo.name} />
        <span className="text-[13px] font-medium">{repo.name}</span>
        <span className="text-[12px] text-muted-foreground">
          {actions.length} item{actions.length === 1 ? "" : "s"}
        </span>
        <div className="flex-1" />
        <Button
          size="sm"
          className="h-7 gap-1.5 text-[12.5px]"
          disabled={apply.isPending}
          onClick={() => apply.mutate()}
        >
          {apply.isPending ? <Loader className="size-3.5 animate-spin" /> : <BrushCleaning className="size-3.5" />}
          Clean up
        </Button>
      </div>
      <ul>
        {actions.map((a) => (
          <li key={a} className="flex items-center gap-3 border-b px-4 py-2.5 text-[13px] last:border-b-0">
            <span className="size-1.5 shrink-0 rounded-full bg-amber" />
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
    </section>
  );
}
