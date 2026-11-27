import { useNavigate } from "@tanstack/react-router";
import {
  ArrowDown,
  ArrowUp,
  Code,
  Copy,
  Ellipsis,
  FileDiff,
  FolderOpen,
  GitBranch,
  GitCommitHorizontal,
  GitPullRequestArrow,
  HardDrive,
  Plus,
  Search,
  Sparkles,
  SquareTerminal,
  Trash2,
  TriangleAlert,
  Undo2,
  Unplug,
} from "lucide-react";
import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useHotkey } from "@/hooks/use-hotkey";
import { bytes, since } from "@/lib/format";
import { resolveInTerminal, resolverName, startOp, useOps } from "@/lib/ops";
import { FILE_MANAGER, isMac } from "@/lib/platform";
import { APP_LABELS, usePrefs } from "@/lib/prefs";
import { copyPath, copyText, openIn, useMountToggle, usePrCheck, usePrs, useRepoConfig, useRepos } from "@/lib/queries";
import type { CloneInfo, MergeCheck } from "@/lib/types";
import { setUi } from "@/lib/ui";
import { cn } from "@/lib/utils";
import { Empty } from "./empty";
import { Shortcut } from "./shortcut";
import { StatusDot } from "./status-dot";

const COLS = "grid-cols-[14px_minmax(0,1fr)_176px_84px_72px_148px]";

export function CloneList({
  root,
  clones,
  goldenGeneration,
}: {
  root: string;
  clones: CloneInfo[];
  goldenGeneration: number | null;
}) {
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const filterRef = useRef<HTMLInputElement>(null);
  const prefs = usePrefs();
  const mount = useMountToggle(root);
  const ops = useOps();
  const openDiff = useOpenDiff(root);
  const { data: prs } = usePrs(root);
  usePrCheck(root);

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return q ? clones.filter((c) => c.name.toLowerCase().includes(q) || c.branch?.toLowerCase().includes(q)) : clones;
  }, [clones, filter]);

  useEffect(() => {
    if (selected >= shown.length) setSelected(Math.max(0, shown.length - 1));
  }, [shown.length, selected]);

  useHotkey({ key: "/" }, () => filterRef.current?.focus());

  const removing = new Set(
    ops
      .filter((o) => o.status === "running" && o.req.kind === "remove" && o.req.root === root)
      .map((o) => (o.req.kind === "remove" ? o.req.name : "")),
  );

  const act = (c: CloneInfo, key: string) => {
    switch (key) {
      case "enter":
      case "e":
        return void openIn(c.path, prefs.editor);
      case "t":
        return void openIn(c.path, prefs.terminal);
      case "f":
        return void openIn(c.path, "finder");
      case "c":
        return void copyPath(c.path);
      case "d":
        return openDiff(c);
      case "m":
        return mount.mutate({ name: c.name, mounted: c.mounted });
      case "backspace":
        return setUi({ remove: { root, clone: c } });
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if ((e.target as HTMLElement).closest("button, input, [role=menu]")) return;
    const c = shown[selected];
    const k = e.key.toLowerCase();
    if (k === "arrowdown" || k === "j") setSelected((s) => Math.min(shown.length - 1, s + 1));
    else if (k === "arrowup" || k === "k") setSelected((s) => Math.max(0, s - 1));
    else if (c && !(e.metaKey || e.ctrlKey) && ["enter", "e", "t", "f", "c", "d", "m"].includes(k)) act(c, k);
    else if (c && k === "backspace" && (e.metaKey || e.ctrlKey)) act(c, "backspace");
    else return;
    e.preventDefault();
  };

  if (clones.length === 0) {
    return (
      <div className="overflow-hidden rounded-xl border border-dashed border-border/90 bg-card/50">
        <Empty
          icon={<GitBranch />}
          title="No clones yet"
          action={
            <Button size="sm" onClick={() => setUi({ newClone: { root } })} className="gap-2">
              <Plus className="size-3.5" /> New clone
              <Shortcut
                keys="mod+n"
                className="ml-1 [&_kbd]:border-white/25 [&_kbd]:bg-white/15 [&_kbd]:text-white [&_kbd]:shadow-none"
              />
            </Button>
          }
        >
          A clone is a full worktree with dependencies installed, copied from the golden image in about a second.
        </Empty>
      </div>
    );
  }

  return (
    <section>
      <div className="mb-2.5 flex items-center gap-2.5">
        <h2 className="text-[13.5px] font-semibold tracking-tight">Clones</h2>
        <span className="inline-flex h-5 items-center gap-1 rounded-full border border-border/80 bg-card px-2 text-[11px] font-medium text-muted-foreground tabular-nums shadow-2xs">
          <span className="text-foreground/90">{clones.length}</span>
          <span className="text-faint">·</span>
          <span>{clones.filter((c) => c.mounted).length} mounted</span>
        </span>
        <div className="flex-1" />
        <div className="relative">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-faint" />
          <input
            ref={filterRef}
            value={filter}
            onChange={(e) => {
              setFilter(e.target.value);
              setSelected(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setFilter("");
                listRef.current?.focus();
              }
              if (e.key === "ArrowDown" || e.key === "Enter") {
                e.preventDefault();
                listRef.current?.focus();
              }
            }}
            placeholder="Filter clones"
            className="h-7 w-48 rounded-[7px] border border-border bg-card pr-7 pl-7.5 text-[12.5px] shadow-2xs outline-none transition-all placeholder:text-faint focus:w-56 focus:border-ring focus:ring-[3px] focus:ring-ring/20"
          />
          <span className="pointer-events-none absolute top-1/2 right-1.5 -translate-y-1/2">
            <Shortcut keys="/" />
          </span>
        </div>
      </div>

      <div className="clove-card overflow-hidden">
        <div
          className={cn("clove-label grid items-center gap-x-4 border-b border-border/75 bg-muted/40 px-4 py-2", COLS)}
        >
          <span />
          <span>Name</span>
          <span>Changes</span>
          <span>Created</span>
          <span className="text-right">Size</span>
          <span />
        </div>
        <div
          ref={listRef}
          role="listbox"
          tabIndex={0}
          aria-label="Clones"
          onKeyDown={onKeyDown}
          className="outline-none [&:focus-visible_[data-selected=true]]:ring-1 [&:focus-visible_[data-selected=true]]:ring-ring/40 [&:focus-visible_[data-selected=true]]:ring-inset"
        >
          {shown.map((c, i) => (
            <CloneRow
              key={c.name}
              clone={c}
              root={root}
              selected={i === selected}
              busy={removing.has(c.name)}
              stale={goldenGeneration !== null && c.golden_generation < goldenGeneration}
              check={prs?.get(c.name)}
              onSelect={() => {
                setSelected(i);
                listRef.current?.focus();
              }}
            />
          ))}
          {shown.length === 0 && (
            <div className="px-4 py-6 text-center text-[12.5px] text-muted-foreground">
              No clone matches “{filter}”.
            </div>
          )}
        </div>
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 px-1 text-[11.5px] text-faint">
        <Hint keys="up+down" label="move" />
        <Hint keys="enter" label={APP_LABELS[prefs.editor]} />
        <Hint keys="t" label={APP_LABELS[prefs.terminal]} />
        <Hint keys="f" label={FILE_MANAGER} />
        <Hint keys="d" label="changes" />
        <Hint keys="c" label="copy path" />
        <Hint keys="m" label="mount" />
        <Hint keys="mod+backspace" label="remove" />
      </div>
    </section>
  );
}

function Hint({ keys, label }: { keys: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <Shortcut keys={keys} className="[&_kbd]:h-4 [&_kbd]:min-w-4 [&_kbd]:text-[10px]" />
      {label}
    </span>
  );
}

function CloneRow({
  clone: c,
  root,
  selected,
  busy,
  stale,
  check,
  onSelect,
}: {
  clone: CloneInfo;
  root: string;
  selected: boolean;
  busy: boolean;
  stale: boolean;
  check?: MergeCheck;
  onSelect: () => void;
}) {
  const prefs = usePrefs();
  const openDiff = useOpenDiff(root);
  const failed = c.failed_steps.length > 0;
  const state = busy ? "busy" : c.issue ? "warn" : !c.mounted ? "unmounted" : failed ? "warn" : "mounted";
  return (
    <div
      role="option"
      tabIndex={-1}
      aria-selected={selected}
      data-selected={selected}
      onMouseDown={onSelect}
      onDoubleClick={() => void openIn(c.path, prefs.editor)}
      className={cn(
        "group relative grid h-[56px] items-center gap-x-4 border-b border-border/65 px-4 transition-colors last:border-b-0",
        COLS,
        selected
          ? "bg-bottle/[0.05] before:absolute before:inset-y-1.5 before:left-0 before:w-[3px] before:rounded-r-full before:bg-bottle dark:bg-bottle/[0.085]"
          : "hover:bg-foreground/[0.024]",
        busy && "opacity-55",
      )}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="flex items-center">
            <StatusDot state={state} />
          </span>
        </TooltipTrigger>
        <TooltipContent side="right">
          {busy
            ? "Removing…"
            : c.issue
              ? `${c.issue}. Cleanup finishes removing it.`
              : !c.mounted
                ? "Unmounted. Mount it to use the worktree."
                : failed
                  ? `Setup failed: ${c.failed_steps.join(", ")}`
                  : "Mounted"}
        </TooltipContent>
      </Tooltip>

      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className={cn("truncate text-[13px] font-medium", !c.mounted && "text-muted-foreground")}>
            {c.name}
          </span>
          {c.operation && (
            <span
              title="Fix the conflicts and git add them, then Continue update from the menu (or Abort)"
              className="flex shrink-0 items-center gap-1 rounded-full bg-amber/12 px-1.5 text-[11px] leading-[18px] text-amber"
            >
              <TriangleAlert className="size-3" />
              {c.operation}: conflicts
            </span>
          )}
          {c.removing_when_free && (
            <span className="shrink-0 rounded-full bg-foreground/[0.06] px-1.5 text-[11px] leading-[18px] text-muted-foreground">
              removing when free
            </span>
          )}
          {c.issue && (
            <span className="flex shrink-0 items-center gap-1 rounded-full bg-amber/12 px-1.5 text-[11px] leading-[18px] text-amber">
              <TriangleAlert className="size-3" />
              {c.issue}
            </span>
          )}
          {failed && (
            <span className="flex shrink-0 items-center gap-1 rounded-full bg-amber/12 px-1.5 text-[11px] leading-[18px] text-amber">
              <TriangleAlert className="size-3" />
              {c.failed_steps.join(", ")} failed
            </span>
          )}
        </div>
        <div className="mt-0.5 flex min-w-0 items-center gap-1.5 font-mono text-[11.5px] text-muted-foreground">
          {c.branch ? (
            <>
              <GitBranch className="size-3 shrink-0 text-faint" />
              <span className="truncate">{c.branch}</span>
            </>
          ) : (
            <>
              <GitCommitHorizontal className="size-3 shrink-0 text-faint" />
              <span className="truncate">detached at {c.head?.slice(0, 8)}</span>
            </>
          )}
          {stale && (
            <Tooltip>
              <TooltipTrigger asChild>
                <span className="shrink-0 font-sans text-[11px] text-faint">· gen {c.golden_generation}</span>
              </TooltipTrigger>
              <TooltipContent>Made from an older golden image</TooltipContent>
            </Tooltip>
          )}
        </div>
      </div>

      <button
        type="button"
        disabled={!c.mounted}
        onClick={(e) => {
          e.stopPropagation();
          openDiff(c);
        }}
        title={c.mounted ? "View changes (D)" : undefined}
        className="-mx-1.5 flex min-w-0 items-center self-stretch rounded-md px-1.5 text-left enabled:hover:bg-foreground/[0.04] disabled:cursor-default"
      >
        <Changes clone={c} check={check} />
      </button>

      <span
        className="text-[12.5px] text-muted-foreground tabular-nums"
        title={new Date(c.created_at * 1000).toLocaleString()}
      >
        {since(c.created_at)}
      </span>
      <span className="text-right font-mono text-[12px] text-muted-foreground tabular-nums">
        {bytes(c.own_bytes ?? c.image_bytes)}
      </span>

      <div className="flex items-center justify-end gap-1">
        <div
          className={cn(
            "flex items-center gap-0.5 rounded-lg border border-border/75 bg-background/95 p-0.5 opacity-0 shadow-2xs transition-opacity group-hover:opacity-100",
            selected && "opacity-100",
          )}
        >
          <RowAction
            label={`Open in ${APP_LABELS[prefs.terminal]}`}
            onClick={() => void openIn(c.path, prefs.terminal)}
          >
            <SquareTerminal />
          </RowAction>
          <RowAction label={`Open in ${APP_LABELS[prefs.editor]}`} onClick={() => void openIn(c.path, prefs.editor)}>
            <Code />
          </RowAction>
          {c.mounted && (
            <RowAction label="View changes" onClick={() => openDiff(c)}>
              <FileDiff />
            </RowAction>
          )}
          <RowAction label={`Reveal in ${FILE_MANAGER}`} onClick={() => void openIn(c.path, "finder")}>
            <FolderOpen />
          </RowAction>
          <RowAction label="Copy path" onClick={() => void copyPath(c.path)}>
            <Copy />
          </RowAction>
        </div>
        <CloneMenu clone={c} root={root}>
          <Button
            variant="ghost"
            size="icon-sm"
            className="size-7 text-muted-foreground data-[state=open]:bg-accent"
            aria-label="More actions"
          >
            <Ellipsis className="size-4" />
          </Button>
        </CloneMenu>
      </div>
    </div>
  );
}

function Changes({ clone: c, check }: { clone: CloneInfo; check?: MergeCheck }) {
  // The last PR check: merged leads, an open PR's number trails.
  const pr = check?.pr;
  const prParts: ReactNode[] = [];
  if (pr?.state === "merged")
    prParts.push(
      <span
        key="m"
        className="inline-flex h-[19px] items-center rounded-full border border-sage/25 bg-sage-soft px-1.5 text-[10.5px] font-medium text-sage"
        title={check?.reason ?? `PR #${pr.number} merged`}
      >
        merged
      </span>,
    );
  const openPr =
    pr?.state === "open" ? (
      <span
        key="o"
        className="inline-flex h-[19px] items-center rounded-full border border-border/80 bg-muted/60 px-1.5 font-mono text-[10.5px] text-muted-foreground"
        title={`PR #${pr.number} open`}
      >
        #{pr.number}
      </span>
    ) : null;
  if (!c.mounted)
    return (
      <span className="flex items-center gap-1.5 text-[11.5px] text-faint">
        {prParts}
        <span className="rounded-full border border-dashed border-border px-1.5 py-px text-[10.5px]">unmounted</span>
        {openPr}
      </span>
    );
  const parts: ReactNode[] = [...prParts];
  if (c.dirty)
    parts.push(
      <span
        key="d"
        className="inline-flex h-[19px] items-center gap-1 rounded-full border border-amber/25 bg-amber/10 px-1.5 text-[10.5px] font-medium text-amber"
      >
        <span className="size-1.5 rounded-full bg-amber" />
        modified
      </span>,
    );
  if (c.ahead)
    parts.push(
      <span
        key="a"
        className="inline-flex items-center gap-0.5 font-mono text-[11.5px] font-medium text-foreground/85"
        title={`${c.ahead} commits ahead of base`}
      >
        <ArrowUp className="size-3 text-sage" />
        {c.ahead}
      </span>,
    );
  if (c.behind)
    parts.push(
      <span
        key="b"
        className="inline-flex items-center gap-0.5 font-mono text-[11.5px] text-muted-foreground"
        title={`${c.behind} commits behind base`}
      >
        <ArrowDown className="size-3 text-faint" />
        {c.behind}
      </span>,
    );
  if (parts.length === 0)
    return (
      <span className="flex items-center gap-2 text-[11.5px] text-faint">
        <span>clean</span>
        {openPr}
      </span>
    );
  return (
    <span className="flex min-w-0 items-center gap-1.5 overflow-hidden text-[12px] tabular-nums">
      {parts}
      {openPr}
    </span>
  );
}

function RowAction({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={label}
          onClick={(e) => {
            e.stopPropagation();
            onClick();
          }}
          className="size-6 rounded-[5px] text-muted-foreground hover:bg-bottle-soft hover:text-bottle [&_svg]:size-[14px]"
        >
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export function CloneMenu({ clone: c, root, children }: { clone: CloneInfo; root: string; children: ReactNode }) {
  const prefs = usePrefs();
  const mount = useMountToggle(root);
  const openDiff = useOpenDiff(root);
  const resolver = useRepoConfig(c.operation ? root : undefined).data?.config.update?.resolver;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-60">
        <DropdownMenuItem onSelect={() => void openIn(c.path, prefs.editor)}>
          <Code /> Open in {APP_LABELS[prefs.editor]}
          <DropdownMenuShortcut>↵</DropdownMenuShortcut>
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => void openIn(c.path, prefs.terminal)}>
          <SquareTerminal /> Open in {APP_LABELS[prefs.terminal]}
          <DropdownMenuShortcut>T</DropdownMenuShortcut>
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => void openIn(c.path, "finder")}>
          <FolderOpen /> Reveal in {FILE_MANAGER}
          <DropdownMenuShortcut>F</DropdownMenuShortcut>
        </DropdownMenuItem>
        {c.mounted && (
          <DropdownMenuItem onSelect={() => openDiff(c)}>
            <FileDiff /> View changes
            <DropdownMenuShortcut>D</DropdownMenuShortcut>
          </DropdownMenuItem>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => void copyPath(c.path)}>
          <Copy /> Copy path
          <DropdownMenuShortcut>C</DropdownMenuShortcut>
        </DropdownMenuItem>
        {c.branch && (
          <DropdownMenuItem onSelect={() => void copyText(c.branch as string, "Branch copied")}>
            <GitBranch /> Copy branch name
          </DropdownMenuItem>
        )}
        <DropdownMenuSeparator />
        {c.operation ? (
          <>
            {resolver && (
              <DropdownMenuItem onSelect={() => resolveInTerminal(root, c.name)}>
                <Sparkles /> Resolve with {resolverName(resolver)}
              </DropdownMenuItem>
            )}
            <DropdownMenuItem onSelect={() => void startOp({ kind: "update", root, name: c.name, step: "continue" })}>
              <GitPullRequestArrow /> Continue update
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => void startOp({ kind: "update", root, name: c.name, step: "abort" })}>
              <Undo2 /> Abort update
            </DropdownMenuItem>
          </>
        ) : (
          c.branch && (
            <DropdownMenuItem onSelect={() => void startOp({ kind: "update", root, name: c.name })}>
              <GitPullRequestArrow /> Update to latest base
              <DropdownMenuShortcut>U</DropdownMenuShortcut>
            </DropdownMenuItem>
          )
        )}
        <DropdownMenuItem onSelect={() => mount.mutate({ name: c.name, mounted: c.mounted })}>
          {c.mounted ? <Unplug /> : <HardDrive />}
          {c.mounted ? "Unmount" : "Mount"}
          <DropdownMenuShortcut>M</DropdownMenuShortcut>
        </DropdownMenuItem>
        <DropdownMenuItem variant="destructive" onSelect={() => setUi({ remove: { root, clone: c } })}>
          <Trash2 /> Remove clone…
          <DropdownMenuShortcut>{isMac ? "⌘⌫" : "Ctrl+⌫"}</DropdownMenuShortcut>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Opens a clone's changes in the diff view. */
export function useOpenDiff(root: string) {
  const navigate = useNavigate();
  const repoId = useRepos().data?.find((r) => r.root === root)?.id;
  return (c: CloneInfo) => {
    if (repoId && c.mounted) void navigate({ to: "/repo/$repoId/diff/$clone", params: { repoId, clone: c.name } });
  };
}
