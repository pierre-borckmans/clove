import { useParams } from "@tanstack/react-router";
import { Copy, List, Network, Plus } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { PaneHeader } from "@/components/app-shell";
import { CloneList, CloneListSkeleton } from "@/components/clone-list";
import { GoldenCard, GoldenCardSkeleton } from "@/components/golden-card";
import { RepoGraph } from "@/components/repo-graph";
import { Shortcut } from "@/components/shortcut";
import { RepoMark } from "@/components/sidebar";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { WorktreeList } from "@/components/worktree-list";
import { api } from "@/lib/api";
import { tildify } from "@/lib/format";
import { copyPath, useClones, useRepo, useStatus, useWorktrees } from "@/lib/queries";
import { setUi } from "@/lib/ui";

export function RepoPage() {
  const [view, setView] = useState<"map" | "list">("map");
  const { repoId } = useParams({ from: "/shell/repo/$repoId" });
  const { repo, isLoading, data: summaries } = useRepo(repoId);
  // As many placeholder rows as the repo has clones, once `repos` knows.
  const rows = Math.min(summaries?.find((r) => r.id === repoId)?.clones || 3, 8);
  const status = useStatus(repo?.root);
  const clones = useClones(repo?.root);
  const worktrees = useWorktrees(repo?.root);

  useEffect(() => {
    const refetch = () => {
      // Ask git again rather than reuse what the backend remembers.
      void api.forgetCache().then(() => {
        void status.refetch();
        void clones.refetch();
        void worktrees.refetch();
      });
    };
    window.addEventListener("clove:refetch", refetch);
    return () => window.removeEventListener("clove:refetch", refetch);
  }, [status, clones, worktrees]);

  if (!repo) {
    return (
      <>
        <PaneHeader>
          {isLoading ? (
            <>
              <Skeleton className="size-5 rounded-[5px]" />
              <Skeleton className="h-4 w-32" />
            </>
          ) : (
            <span>Repository not found</span>
          )}
        </PaneHeader>
        {isLoading ? (
          <Body golden={<GoldenCardSkeleton />} clones={<CloneListSkeleton rows={rows} />} />
        ) : (
          <div className="flex-1" />
        )}
      </>
    );
  }

  return (
    <>
      <PaneHeader>
        <RepoMark name={repo.name} active className="size-5 text-[11px]" />
        <h1 className="text-[14px] font-semibold tracking-tight">{repo.name}</h1>
        <button
          type="button"
          onClick={() => void copyPath(repo.root)}
          className="group flex min-w-0 items-center gap-1.5 rounded px-1 py-0.5 font-mono text-[12px] text-muted-foreground hover:bg-accent hover:text-foreground"
          title="Copy path"
        >
          <span className="truncate">{tildify(repo.root)}</span>
          <Copy className="size-3 opacity-0 group-hover:opacity-70" />
        </button>
        <div data-tauri-drag-region className="h-full flex-1" />
        <fieldset
          className="flex shrink-0 items-center gap-0.5 rounded-md border bg-muted/50 p-0.5"
          aria-label="Repository view"
        >
          <Button
            variant="ghost"
            size="sm"
            aria-pressed={view === "list"}
            className="h-6 gap-1.5 px-2 text-[11.5px] aria-pressed:bg-bottle-soft aria-pressed:font-medium aria-pressed:text-bottle aria-pressed:shadow-sm aria-pressed:ring-1 aria-pressed:ring-bottle/30 aria-pressed:hover:bg-bottle-soft aria-pressed:hover:text-bottle"
            onClick={() => setView("list")}
          >
            <List className="size-3.5" />
            List
          </Button>
          <Button
            variant="ghost"
            size="sm"
            aria-pressed={view === "map"}
            className="h-6 gap-1.5 px-2 text-[11.5px] aria-pressed:bg-bottle-soft aria-pressed:font-medium aria-pressed:text-bottle aria-pressed:shadow-sm aria-pressed:ring-1 aria-pressed:ring-bottle/30 aria-pressed:hover:bg-bottle-soft aria-pressed:hover:text-bottle"
            onClick={() => setView("map")}
          >
            <Network className="size-3.5" />
            Map
          </Button>
        </fieldset>
        <Button
          size="sm"
          className="h-7 gap-1.5 pr-1.5 pl-2.5 text-[12.5px]"
          onClick={() => setUi({ newClone: { root: repo.root } })}
        >
          <Plus className="size-3.5" />
          New clone
          <Shortcut keys="mod+n" className="ml-0.5 [&_kbd]:border-white/20 [&_kbd]:bg-white/15 [&_kbd]:text-white/90" />
        </Button>
      </PaneHeader>
      {view === "map" && status.data && clones.data ? (
        <RepoGraph key={repo.id} status={status.data} clones={clones.data} worktrees={worktrees.data ?? []} />
      ) : (
        <Body
          golden={status.data ? <GoldenCard status={status.data} /> : <GoldenCardSkeleton />}
          clones={
            clones.data ? (
              <CloneList
                root={repo.root}
                clones={clones.data}
                goldenGeneration={status.data?.golden?.generation ?? null}
              />
            ) : (
              <CloneListSkeleton rows={rows} />
            )
          }
          worktrees={worktrees.data && <WorktreeList root={repo.root} worktrees={worktrees.data} />}
        />
      )}
    </>
  );
}

function Body({ golden, clones, worktrees }: { golden: ReactNode; clones: ReactNode; worktrees?: ReactNode }) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto flex max-w-[1080px] flex-col gap-6 px-6 pt-5 pb-24">
        {golden}
        {clones}
        {worktrees}
      </div>
    </div>
  );
}
