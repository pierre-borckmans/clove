import {
  Code,
  Copy,
  CopyPlus,
  Ellipsis,
  FolderOpen,
  GitBranch,
  GitCommitHorizontal,
  Lock,
  SquareTerminal,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { PickBox } from "@/components/clone-list";
import { PointerMenu } from "@/components/pointer-menu";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { tildify } from "@/lib/format";
import { useOps } from "@/lib/ops";
import { FILE_MANAGER } from "@/lib/platform";
import { APP_LABELS, usePrefs } from "@/lib/prefs";
import { copyPath, copyText, openIn } from "@/lib/queries";
import type { OtherWorktree } from "@/lib/types";
import { setUi } from "@/lib/ui";
import { cn } from "@/lib/utils";

/**
 * The repo's plain git worktrees, below the clones and quieter than them:
 * ones made before clove, by agents, or by other tools.
 */
export function WorktreeList({ root, worktrees }: { root: string; worktrees: OtherWorktree[] }) {
  const ops = useOps();
  // Worktrees picked for a bulk remove, by path.
  const [picked, setPicked] = useState<ReadonlySet<string>>(new Set());
  useEffect(() => {
    setPicked((p) => {
      const live = new Set(worktrees.map((w) => w.path));
      return [...p].every((x) => live.has(x)) ? p : new Set([...p].filter((x) => live.has(x)));
    });
  }, [worktrees]);
  if (worktrees.length === 0) return null;
  const busy = new Set(
    ops
      .filter(
        (o) =>
          o.status === "running" && o.req.root === root && (o.req.kind === "remove_worktree" || o.req.kind === "adopt"),
      )
      .map((o) => (o.req.kind === "remove_worktree" || o.req.kind === "adopt" ? o.req.path : "")),
  );
  const pickedWts = worktrees.filter((w) => picked.has(w.path) && !busy.has(w.path));
  const toggle = (path: string) =>
    setPicked((p) => {
      const n = new Set(p);
      if (!n.delete(path)) n.add(path);
      return n;
    });
  return (
    <section>
      <div className="mb-2 flex items-baseline gap-3">
        <h2 className="text-[13px] font-medium text-muted-foreground">Other git worktrees</h2>
        <span className="text-[12px] text-faint">
          {worktrees.length} not made by clove · make one a clone to get warm deps
        </span>
        <div className="flex-1" />
        {pickedWts.length > 0 && (
          <div className="flex items-center gap-1.5 self-center">
            <Button variant="ghost" size="sm" className="h-7 text-[12.5px]" onClick={() => setPicked(new Set())}>
              Clear
            </Button>
            <Button
              variant="destructive"
              size="sm"
              className="h-7 gap-1.5 text-[12.5px]"
              onClick={() => setUi({ removeWorktree: { root, worktrees: pickedWts } })}
            >
              <Trash2 className="size-3.5" /> Remove {pickedWts.length}
            </Button>
          </div>
        )}
      </div>
      <div className="overflow-hidden rounded-xl border border-dashed">
        {worktrees.map((w) => (
          <WorktreeRow
            key={w.path}
            worktree={w}
            root={root}
            busy={busy.has(w.path)}
            picked={picked.has(w.path)}
            picking={picked.size > 0}
            onPick={() => toggle(w.path)}
          />
        ))}
      </div>
    </section>
  );
}

function WorktreeRow({
  worktree: w,
  root,
  busy,
  picked,
  picking,
  onPick,
}: {
  worktree: OtherWorktree;
  root: string;
  busy: boolean;
  picked: boolean;
  picking: boolean;
  onPick: () => void;
}) {
  const prefs = usePrefs();
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  return (
    <div
      onDoubleClick={() => !w.missing && void openIn(w.path, prefs.editor)}
      onMouseDown={(e) => (e.metaKey || e.ctrlKey) && onPick()}
      onContextMenu={(e) => {
        e.preventDefault();
        setMenuAt({ x: e.clientX, y: e.clientY });
      }}
      className={cn(
        "group grid h-[44px] grid-cols-[16px_14px_minmax(0,1fr)_minmax(0,220px)_auto] items-center gap-x-4 border-b px-4 transition-colors last:border-b-0",
        picked ? "bg-primary/[0.06]" : "hover:bg-foreground/[0.025]",
        busy && "opacity-55",
      )}
    >
      {menuAt && (
        <PointerMenu at={menuAt} onClose={() => setMenuAt(null)}>
          <WorktreeMenuItems worktree={w} root={root} />
        </PointerMenu>
      )}
      <PickBox checked={picked} visible={picking} label={`Select ${w.name}`} onToggle={onPick} />
      <span className="size-2 rotate-45 border-[1.5px] border-faint" aria-hidden="true" />
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="truncate text-[12.5px] text-foreground/80">{w.name}</span>
          <span className="flex min-w-0 items-center gap-1 font-mono text-[11.5px] text-muted-foreground">
            {w.branch ? (
              <>
                <GitBranch className="size-3 shrink-0 text-faint" />
                <span className="truncate">{w.branch}</span>
              </>
            ) : (
              <>
                <GitCommitHorizontal className="size-3 shrink-0 text-faint" />
                <span className="truncate">detached at {w.head?.slice(0, 8)}</span>
              </>
            )}
          </span>
        </div>
        <div className="truncate font-mono text-[11px] text-faint">{tildify(w.path)}</div>
      </div>
      <State worktree={w} busy={busy} />
      <div className="flex items-center justify-end">
        <WorktreeMenu worktree={w} root={root}>
          <Button
            variant="ghost"
            size="icon-sm"
            className="size-7 text-muted-foreground data-[state=open]:bg-accent"
            aria-label="Worktree actions"
          >
            <Ellipsis className="size-4" />
          </Button>
        </WorktreeMenu>
      </div>
    </div>
  );
}

function State({ worktree: w, busy }: { worktree: OtherWorktree; busy: boolean }) {
  const parts: ReactNode[] = [];
  if (busy) parts.push(<span key="b">working…</span>);
  if (w.pr?.state === "merged")
    parts.push(
      <span key="m" className="rounded-full bg-sage-soft px-1.5 text-[11px] leading-[17px] text-sage">
        merged
      </span>,
    );
  if (w.missing)
    parts.push(
      <span key="g" className="flex items-center gap-1 text-amber">
        <TriangleAlert className="size-3" /> gone
      </span>,
    );
  if (w.locked)
    parts.push(
      <Tooltip key="l">
        <TooltipTrigger asChild>
          <span className="flex items-center gap-1">
            <Lock className="size-3" /> locked
          </span>
        </TooltipTrigger>
        <TooltipContent>{w.lock_reason ?? "Locked with git worktree lock"}</TooltipContent>
      </Tooltip>,
    );
  if (w.dirty)
    parts.push(
      <span key="d" className="flex items-center gap-1 text-amber">
        <span className="size-1.5 rounded-full bg-amber" />
        modified
      </span>,
    );
  if (w.detached_commits)
    parts.push(
      <span key="o" className="text-amber" title="Commits on no branch: removing it would orphan them">
        {w.detached_commits} on no branch
      </span>,
    );
  if (w.pr?.state === "open")
    parts.push(
      <span key="p" className="font-mono" title={`PR #${w.pr.number} open`}>
        #{w.pr.number}
      </span>,
    );
  if (parts.length === 0) parts.push(<span key="c">clean</span>);
  return (
    <span className="flex min-w-0 items-center gap-2.5 overflow-hidden text-[12px] text-muted-foreground">{parts}</span>
  );
}

export function WorktreeMenu({
  worktree: w,
  root,
  children,
}: {
  worktree: OtherWorktree;
  root: string;
  children: ReactNode;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-60">
        <WorktreeMenuItems worktree={w} root={root} />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** A worktree's menu items, shared by its ••• button and its right-click menu. */
function WorktreeMenuItems({ worktree: w, root }: { worktree: OtherWorktree; root: string }) {
  const prefs = usePrefs();
  return (
    <>
      <DropdownMenuItem disabled={w.missing} onSelect={() => void openIn(w.path, prefs.editor)}>
        <Code /> Open in {APP_LABELS[prefs.editor]}
      </DropdownMenuItem>
      <DropdownMenuItem disabled={w.missing} onSelect={() => void openIn(w.path, prefs.terminal)}>
        <SquareTerminal /> Open in {APP_LABELS[prefs.terminal]}
      </DropdownMenuItem>
      <DropdownMenuItem disabled={w.missing} onSelect={() => void openIn(w.path, "finder")}>
        <FolderOpen /> Reveal in {FILE_MANAGER}
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem onSelect={() => void copyPath(w.path)}>
        <Copy /> Copy path
      </DropdownMenuItem>
      {w.branch && (
        <DropdownMenuItem onSelect={() => void copyText(w.branch as string, "Branch copied")}>
          <GitBranch /> Copy branch name
        </DropdownMenuItem>
      )}
      <DropdownMenuSeparator />
      <DropdownMenuItem onSelect={() => setUi({ adopt: { root, worktree: w } })}>
        <CopyPlus /> Make it a clone…
      </DropdownMenuItem>
      <DropdownMenuItem variant="destructive" onSelect={() => setUi({ removeWorktree: { root, worktrees: [w] } })}>
        <Trash2 /> Remove worktree…
      </DropdownMenuItem>
    </>
  );
}
