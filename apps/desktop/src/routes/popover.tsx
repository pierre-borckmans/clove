import { useQueries } from "@tanstack/react-query";
import {
  AppWindow,
  ArrowDown,
  Code,
  Copy,
  FolderOpen,
  FolderPlus,
  GitBranch,
  GitCommitHorizontal,
  Plus,
  Power,
  SquareTerminal,
  Stethoscope,
} from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { Glyph } from "@/components/glyph";
import { StatusDot } from "@/components/status-dot";
import { Toaster } from "@/components/ui/sonner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useNow } from "@/hooks/use-now";
import { api } from "@/lib/api";
import { listen } from "@/lib/bridge";
import { ms } from "@/lib/format";
import { type Op, startOp, useOps } from "@/lib/ops";
import { APP_LABELS, usePrefs } from "@/lib/prefs";
import { copyPath, keys, openIn, POLL_MS, useRepoChanges, useRepos } from "@/lib/queries";
import type { CloneInfo, RepoSummary } from "@/lib/types";
import { cn } from "@/lib/utils";

/** The menu-bar popover: every repo and clone, one click from a terminal or editor. */
export function PopoverPage() {
  useRepoChanges();
  const { data: repos = [], isLoading } = useRepos();
  const ops = useOps();
  const running = ops.filter((o) => o.status === "running" || (o.finishedAt && Date.now() - o.finishedAt < 4000));
  const clones = useQueries({
    queries: repos.map((r) => ({
      queryKey: keys.clones(r.root),
      queryFn: () => api.clones(r.root, true),
      refetchInterval: POLL_MS,
    })),
  });
  const [adding, setAdding] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !adding) void api.hidePopover();
    };
    window.addEventListener("keydown", onKey);
    const un = listen("popover-shown", () => setAdding(null));
    return () => {
      window.removeEventListener("keydown", onKey);
      void un.then((f) => f());
    };
  }, [adding]);

  return (
    <div className="flex h-full flex-col overflow-hidden bg-[var(--popover-surface)] text-[13px] in-data-[shell=web]:rounded-xl in-data-[shell=web]:border in-data-[shell=web]:shadow-2xl">
      <header className="flex h-11 shrink-0 items-center gap-2 px-3.5">
        <Glyph className="size-4" />
        <span className="font-semibold tracking-tight">clove</span>
        <div className="flex-1" />
        <IconButton label="Open clove" onClick={() => void api.openMain()}>
          <AppWindow />
        </IconButton>
      </header>

      {running.length > 0 && (
        <div className="mx-2 mb-1 flex flex-col gap-1">
          {running.slice(0, 3).map((o) => (
            <MiniOp key={o.id} op={o} />
          ))}
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
        {!isLoading && repos.length === 0 && (
          <div className="flex flex-col items-center px-6 py-12 text-center">
            <p className="text-muted-foreground">No repositories yet.</p>
            <button
              type="button"
              onClick={() => void api.openMain(undefined, "add-repo")}
              className="mt-3 rounded-md bg-primary px-3 py-1.5 text-[12.5px] font-medium text-primary-foreground hover:bg-primary/90"
            >
              Add repository…
            </button>
          </div>
        )}
        {repos.map((r, i) => (
          <RepoBlock
            key={r.id}
            repo={r}
            clones={clones[i]?.data}
            adding={adding === r.root}
            setAdding={(on) => setAdding(on ? r.root : null)}
          />
        ))}
      </div>

      <footer className="flex h-10 shrink-0 items-center gap-0.5 border-t border-foreground/[0.07] px-1.5">
        <FooterButton onClick={() => void api.openMain(undefined, "add-repo")}>
          <FolderPlus /> Add repo
        </FooterButton>
        <FooterButton onClick={() => void api.openMain("/doctor")}>
          <Stethoscope /> Doctor
        </FooterButton>
        <div className="flex-1" />
        <FooterButton onClick={() => void api.quit()}>
          <Power /> Quit
        </FooterButton>
      </footer>
      <Toaster position="bottom-center" offset={48} />
    </div>
  );
}

function RepoBlock({
  repo,
  clones,
  adding,
  setAdding,
}: {
  repo: RepoSummary;
  clones?: CloneInfo[];
  adding: boolean;
  setAdding: (on: boolean) => void;
}) {
  return (
    <section className="mt-1.5">
      <div className="group flex h-7 items-center gap-2 px-2">
        <button
          type="button"
          onClick={() => void api.openMain(`/repo/${repo.id}`)}
          className="truncate text-[12px] font-semibold text-muted-foreground hover:text-foreground"
        >
          {repo.name}
        </button>
        {repo.generation !== null && <span className="font-mono text-[11px] text-faint">gen {repo.generation}</span>}
        {!!repo.behind && (
          <span
            className="flex items-center font-mono text-[11px] text-bottle"
            title={`${repo.behind} commits behind base`}
          >
            <ArrowDown className="size-3" />
            {repo.behind}
          </span>
        )}
        <div className="flex-1" />
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              aria-label={`New clone in ${repo.name}`}
              onClick={() => setAdding(!adding)}
              className={cn(
                "flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/[0.07] hover:text-foreground",
                adding && "bg-foreground/[0.07] text-foreground",
              )}
            >
              <Plus className="size-3.5" />
            </button>
          </TooltipTrigger>
          <TooltipContent>New clone</TooltipContent>
        </Tooltip>
      </div>
      {adding && <QuickNew root={repo.root} done={() => setAdding(false)} />}
      {clones?.length === 0 && !adding && (
        <button
          type="button"
          onClick={() => setAdding(true)}
          className="mx-1 flex w-[calc(100%-0.5rem)] items-center gap-2 rounded-md px-2 py-1.5 text-[12.5px] text-faint hover:bg-foreground/[0.05] hover:text-muted-foreground"
        >
          No clones. Make one…
        </button>
      )}
      {clones?.map((c) => (
        <CloneRow key={c.name} clone={c} />
      ))}
    </section>
  );
}

function QuickNew({ root, done }: { root: string; done: () => void }) {
  const [branch, setBranch] = useState("");
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => ref.current?.focus(), []);
  return (
    <form
      className="mx-1 mb-1 flex items-center gap-2 rounded-md border border-ring/50 bg-background/80 px-2 ring-[3px] ring-ring/15"
      onSubmit={(e) => {
        e.preventDefault();
        void startOp({ kind: "new", root, branch: branch.trim() || null });
        done();
      }}
    >
      <GitBranch className="size-3.5 text-muted-foreground" />
      <input
        ref={ref}
        value={branch}
        onChange={(e) => setBranch(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            done();
          }
        }}
        placeholder="Branch, or empty for a generated name"
        className="h-8 min-w-0 flex-1 bg-transparent font-mono text-[12.5px] outline-none placeholder:font-sans placeholder:text-faint"
      />
      <span className="text-[11px] text-faint">↵</span>
    </form>
  );
}

function CloneRow({ clone: c }: { clone: CloneInfo }) {
  const prefs = usePrefs();
  return (
    <div className="group flex h-10 items-center gap-2.5 rounded-md px-2 transition-colors hover:bg-foreground/[0.055]">
      <StatusDot state={!c.mounted ? "unmounted" : c.failed_steps.length ? "warn" : "mounted"} />
      <button
        type="button"
        onClick={() => void openIn(c.path, prefs.editor)}
        className="min-w-0 flex-1 text-left"
        title={`Open in ${APP_LABELS[prefs.editor]}`}
      >
        <div className={cn("truncate text-[13px] leading-tight font-medium", !c.mounted && "text-muted-foreground")}>
          {c.name}
          {c.dirty && <span className="ml-1.5 inline-block size-1.5 rounded-full bg-amber align-middle" />}
        </div>
        <div className="flex items-center gap-1 truncate font-mono text-[11px] leading-tight text-muted-foreground">
          {c.branch ?? (
            <>
              <GitCommitHorizontal className="size-3" /> {c.head?.slice(0, 8)}
            </>
          )}
          {!c.mounted && <span className="text-faint">· unmounted</span>}
        </div>
      </button>
      <div className="flex items-center opacity-0 transition-opacity group-hover:opacity-100">
        <IconButton label={APP_LABELS[prefs.terminal]} onClick={() => void openIn(c.path, prefs.terminal)}>
          <SquareTerminal />
        </IconButton>
        <IconButton label={APP_LABELS[prefs.editor]} onClick={() => void openIn(c.path, prefs.editor)}>
          <Code />
        </IconButton>
        <IconButton label="Finder" onClick={() => void openIn(c.path, "finder")}>
          <FolderOpen />
        </IconButton>
        <IconButton label="Copy path" onClick={() => void copyPath(c.path)}>
          <Copy />
        </IconButton>
      </div>
    </div>
  );
}

function MiniOp({ op }: { op: Op }) {
  const now = useNow(op.status === "running");
  const current = [...op.steps].reverse().find((s) => s.end === undefined);
  const done = op.steps.filter((s) => s.end !== undefined).length;
  const pct = op.status === "running" ? Math.min(92, 8 + done * 14) : 100;
  return (
    <div className="rounded-lg border border-foreground/[0.07] bg-background/70 px-2.5 py-2 animate-in fade-in-0 slide-in-from-top-1">
      <div className="flex items-center gap-2 text-[12px]">
        <span className="font-medium">{op.title}</span>
        <span className="truncate font-mono text-[11.5px] text-muted-foreground">{op.subject}</span>
        <span className="ml-auto font-mono text-[11px] text-muted-foreground tabular-nums">
          {op.status === "running" ? (current?.name ?? "…") : op.status === "ok" ? "done" : "failed"}{" "}
          {ms((op.finishedAt ?? now) - op.startedAt)}
        </span>
      </div>
      <div className="mt-1.5 h-[3px] overflow-hidden rounded-full bg-foreground/[0.07]">
        <div
          className={cn(
            "h-full rounded-full transition-[width] duration-500",
            op.status === "error" ? "bg-destructive" : op.status === "ok" ? "bg-sage" : "bg-bottle",
          )}
          style={{ width: `${pct}%` }}
        />
      </div>
      {op.error && <div className="mt-1.5 line-clamp-2 font-mono text-[11px] text-destructive">{op.error}</div>}
    </div>
  );
}

function IconButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          onClick={onClick}
          className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/[0.08] hover:text-foreground [&_svg]:size-[15px]"
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

function FooterButton({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] text-muted-foreground transition-colors hover:bg-foreground/[0.07] hover:text-foreground [&_svg]:size-3.5"
    >
      {children}
    </button>
  );
}
