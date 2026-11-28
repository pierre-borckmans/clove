import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import {
  ArrowDown,
  ArrowUp,
  Check,
  Code,
  Copy,
  Ellipsis,
  FileDiff,
  FolderOpen,
  GitBranch,
  GitCommitHorizontal,
  GitFork,
  GitPullRequestArrow,
  HardDrive,
  History,
  Plus,
  Sparkles,
  SquareTerminal,
  Trash2,
  TriangleAlert,
  Undo2,
  Unplug,
} from "lucide-react";
import { type KeyboardEvent, type MouseEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useHotkey } from "@/hooks/use-hotkey";
import { bytes, since } from "@/lib/format";
import { resolveInTerminal, resolverName, startOp, useOps } from "@/lib/ops";
import { FILE_MANAGER, isMac } from "@/lib/platform";
import { APP_LABELS, usePrefs } from "@/lib/prefs";
import {
  checkpointNow,
  copyPath,
  copyText,
  forkClone,
  openIn,
  useMountToggle,
  usePrCheck,
  usePrs,
  useRepoConfig,
  useRepoList,
} from "@/lib/queries";
import type { CloneInfo, MergeCheck } from "@/lib/types";
import { setUi } from "@/lib/ui";
import { cn } from "@/lib/utils";
import { Empty } from "./empty";
import { PointerMenu } from "./pointer-menu";
import { Shortcut } from "./shortcut";
import { StatusDot } from "./status-dot";

// Columns by pane width. Narrow: no hover actions (the ••• menu, the keys and
// the map's ring have them), so the name keeps its room. Medium: actions on
// hover. Wide: actions inline, with Update too. The name takes what's left.
const COLS = cn(
  "grid-cols-[16px_14px_minmax(0,1fr)_128px_60px_64px_60px]",
  "@min-[1000px]:grid-cols-[16px_14px_minmax(0,1fr)_140px_64px_72px_180px]",
  "@min-[1300px]:grid-cols-[16px_14px_minmax(0,1fr)_176px_72px_80px_auto]",
);

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
  // Clones picked for a bulk remove (checkbox, mod-click, shift-click, space).
  const [picked, setPicked] = useState<ReadonlySet<string>>(new Set());
  const anchor = useRef<number | null>(null);
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

  // Picks of clones that went away go too.
  useEffect(() => {
    setPicked((p) => {
      const live = new Set(clones.map((c) => c.name));
      return [...p].every((n) => live.has(n)) ? p : new Set([...p].filter((n) => live.has(n)));
    });
  }, [clones]);

  const removing = new Set(
    ops
      .filter((o) => o.status === "running" && o.req.kind === "remove" && o.req.root === root)
      .map((o) => (o.req.kind === "remove" ? o.req.name : "")),
  );

  const pickedClones = clones.filter((c) => picked.has(c.name) && !removing.has(c.name));
  const togglePick = (i: number) => {
    const c = shown[i];
    if (!c) return;
    anchor.current = i;
    setPicked((p) => {
      const n = new Set(p);
      if (!n.delete(c.name)) n.add(c.name);
      return n;
    });
  };
  // Shift-click: picks every row between the last one picked and this one.
  const pickRange = (i: number) => {
    const from = anchor.current ?? selected;
    const [a, b] = from < i ? [from, i] : [i, from];
    setPicked((p) => new Set([...p, ...shown.slice(a, b + 1).map((c) => c.name)]));
    anchor.current = i;
  };
  const removePicked = () => setUi({ remove: { root, clones: pickedClones } });

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
        return pickedClones.length ? removePicked() : setUi({ remove: { root, clones: [c] } });
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
    else if (c && k === " ") togglePick(selected);
    else if (k === "escape" && picked.size > 0) setPicked(new Set());
    else return;
    e.preventDefault();
  };

  if (clones.length === 0) {
    return (
      <div className="rounded-xl border border-dashed">
        <Empty
          icon={<GitBranch />}
          title="No clones yet"
          action={
            <Button size="sm" onClick={() => setUi({ newClone: { root } })} className="gap-2">
              <Plus className="size-3.5" /> New clone
              <Shortcut keys="mod+n" className="ml-1 [&_kbd]:border-white/25 [&_kbd]:bg-white/15 [&_kbd]:text-white" />
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
      <div className="mb-2 flex items-center gap-3">
        <h2 className="text-[13px] font-medium">Clones</h2>
        <span className="text-[12px] text-muted-foreground tabular-nums">
          {clones.length} · {clones.filter((c) => c.mounted).length} mounted
        </span>
        <div className="flex-1" />
        {pickedClones.length > 0 && (
          <div className="flex items-center gap-1.5">
            <Button variant="ghost" size="sm" className="h-7 text-[12.5px]" onClick={() => setPicked(new Set())}>
              Clear
            </Button>
            <Button variant="destructive" size="sm" className="h-7 gap-1.5 text-[12.5px]" onClick={removePicked}>
              <Trash2 className="size-3.5" /> Remove {pickedClones.length}
            </Button>
          </div>
        )}
        <div className="relative">
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
            placeholder="Filter"
            className="h-7 w-44 rounded-md border bg-background px-2.5 pr-7 text-[12.5px] outline-none transition-colors placeholder:text-faint focus:border-ring focus:ring-[3px] focus:ring-ring/20"
          />
          <span className="pointer-events-none absolute top-1/2 right-1.5 -translate-y-1/2">
            <Shortcut keys="/" />
          </span>
        </div>
      </div>

      <div className="overflow-hidden rounded-xl border bg-card">
        <div
          className={cn(
            "grid items-center gap-x-4 border-b bg-muted/40 px-4 py-1.5 text-[11.5px] text-muted-foreground",
            COLS,
          )}
        >
          <span />
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
              picked={picked.has(c.name)}
              picking={picked.size > 0}
              busy={removing.has(c.name)}
              stale={goldenGeneration !== null && c.golden_generation < goldenGeneration}
              check={prs?.get(c.name)}
              onPick={(range) => (range ? pickRange(i) : togglePick(i))}
              onSelect={(e) => {
                if (e.metaKey || e.ctrlKey) togglePick(i);
                else if (e.shiftKey) pickRange(i);
                else anchor.current = i;
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
        <Hint keys="space" label="select" />
        <Hint keys="mod+backspace" label={pickedClones.length ? `remove ${pickedClones.length}` : "remove"} />
      </div>
    </section>
  );
}

/** The list's frame while clones load: their status runs git in every clone. */
export function CloneListSkeleton({ rows = 3 }: { rows?: number }) {
  return (
    <section aria-busy>
      <div className="mb-2 flex h-7 items-center gap-3">
        <h2 className="text-[13px] font-medium">Clones</h2>
        <Skeleton className="h-3 w-20" />
      </div>
      <div className="overflow-hidden rounded-xl border bg-card">
        <div
          className={cn(
            "grid items-center gap-x-4 border-b bg-muted/40 px-4 py-1.5 text-[11.5px] text-muted-foreground",
            COLS,
          )}
        >
          <span />
          <span />
          <span>Name</span>
          <span>Changes</span>
          <span>Created</span>
          <span className="text-right">Size</span>
          <span />
        </div>
        {Array.from({ length: rows }, (_, i) => (
          <div key={i} className={cn("grid h-[52px] items-center gap-x-4 border-b px-4 last:border-b-0", COLS)}>
            <span />
            <Skeleton className="size-2 rounded-full" />
            <div className="flex flex-col gap-1.5">
              <Skeleton className="h-3.5 w-40" />
              <Skeleton className="h-3 w-28" />
            </div>
            <Skeleton className="h-3 w-24" />
            <Skeleton className="h-3 w-14" />
            <Skeleton className="ml-auto h-3 w-12" />
            <span />
          </div>
        ))}
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

/** A row's pick box: shown on hover, on the selected row, or once any is picked. */
export function PickBox({
  checked,
  visible,
  label,
  onToggle,
}: {
  checked: boolean;
  visible: boolean;
  label: string;
  onToggle: (range: boolean) => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={checked}
      aria-label={label}
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => {
        e.stopPropagation();
        onToggle(e.shiftKey);
      }}
      className={cn(
        "flex size-4 items-center justify-center rounded-[4px] border transition-opacity focus-visible:opacity-100",
        checked ? "border-primary bg-primary text-primary-foreground" : "border-foreground/25 bg-background",
        checked || visible ? "opacity-100" : "opacity-0 group-hover:opacity-100",
      )}
    >
      {checked && <Check className="size-3" strokeWidth={3} />}
    </button>
  );
}

function CloneRow({
  clone: c,
  root,
  selected,
  picked,
  picking,
  busy,
  stale,
  check,
  onPick,
  onSelect,
}: {
  clone: CloneInfo;
  root: string;
  selected: boolean;
  picked: boolean;
  picking: boolean;
  busy: boolean;
  stale: boolean;
  check?: MergeCheck;
  onPick: (range: boolean) => void;
  onSelect: (e: MouseEvent) => void;
}) {
  const prefs = usePrefs();
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
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
      onContextMenu={(e) => {
        // Right-click: the ••• menu's items, at the pointer (the mouse-down selected the row).
        e.preventDefault();
        setMenuAt({ x: e.clientX, y: e.clientY });
      }}
      onDoubleClick={() => void openIn(c.path, prefs.editor)}
      className={cn(
        "group grid h-[52px] items-center gap-x-4 border-b px-4 transition-colors last:border-b-0",
        COLS,
        picked ? "bg-primary/[0.06]" : selected ? "bg-foreground/[0.04]" : "hover:bg-foreground/[0.025]",
        busy && "opacity-55",
      )}
    >
      {menuAt && (
        <PointerMenu at={menuAt} onClose={() => setMenuAt(null)}>
          <CloneMenuItems clone={c} root={root} />
        </PointerMenu>
      )}
      <PickBox checked={picked} visible={selected || picking} label={`Select ${c.name}`} onToggle={onPick} />
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
          {!!c.checkpoints && (
            <button
              type="button"
              title={`${c.checkpoints} checkpoint${c.checkpoints === 1 ? "" : "s"}: rewind…`}
              onMouseDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                setUi({ rewind: { root, clone: c } });
              }}
              className="flex shrink-0 items-center gap-1 rounded-full bg-bottle-soft px-1.5 text-[11px] leading-[18px] text-bottle hover:brightness-95"
            >
              <History className="size-3" />
              {c.checkpoints}
            </button>
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
          {c.forked_from && (
            <span className="flex shrink-0 items-center gap-1 font-sans text-[11px] text-faint">
              · <GitFork className="size-3" /> forked from {c.forked_from}
            </span>
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

      <div className="flex items-center justify-end gap-0.5">
        <div
          className={cn(
            "hidden items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100 @min-[1000px]:flex @min-[1300px]:opacity-100",
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
          {c.branch && !c.operation && (
            <RowAction
              label="Update to latest base"
              className="hidden @min-[1300px]:inline-flex"
              onClick={() => void startOp({ kind: "update", root, name: c.name })}
            >
              <GitPullRequestArrow />
            </RowAction>
          )}
          <RowAction label={`Reveal in ${FILE_MANAGER}`} onClick={() => void openIn(c.path, "finder")}>
            <FolderOpen />
          </RowAction>
          <RowAction label="Copy path" onClick={() => void copyPath(c.path)}>
            <Copy />
          </RowAction>
        </div>
        {/* Remove stays on every row, at every width. */}
        <RowAction
          label="Remove clone…"
          className="hover:bg-destructive/10 hover:text-destructive"
          onClick={() => setUi({ remove: { root, clones: [c] } })}
        >
          <Trash2 />
        </RowAction>
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
        className="rounded-full bg-sage-soft px-1.5 text-[11px] leading-[17px] text-sage"
        title={check?.reason ?? `PR #${pr.number} merged`}
      >
        merged
      </span>,
    );
  const openPr =
    pr?.state === "open" ? (
      <span key="o" className="font-mono text-[11.5px] text-muted-foreground" title={`PR #${pr.number} open`}>
        #{pr.number}
      </span>
    ) : null;
  if (!c.mounted)
    return (
      <span className="flex items-center gap-2 text-[12px] text-faint">
        {prParts}unmounted{openPr}
      </span>
    );
  const parts: ReactNode[] = [...prParts];
  if (c.ahead)
    parts.push(
      <span
        key="a"
        className="flex items-center gap-0.5 font-mono text-foreground/80"
        title={`${c.ahead} commits ahead of base`}
      >
        <ArrowUp className="size-3" />
        {c.ahead}
      </span>,
    );
  if (c.behind)
    parts.push(
      <span
        key="b"
        className="flex items-center gap-0.5 font-mono text-muted-foreground"
        title={`${c.behind} commits behind base`}
      >
        <ArrowDown className="size-3" />
        {c.behind}
      </span>,
    );
  // "modified" gets its own line under the commits and PR, so the column stays narrow.
  const modified = c.dirty && (
    <span className="flex items-center gap-1 text-amber">
      <span className="size-1.5 rounded-full bg-amber" />
      modified
    </span>
  );
  if (parts.length === 0 && !modified)
    return <span className="flex items-center gap-2.5 text-[12px] text-faint">clean{openPr}</span>;
  const commits = parts.length > 0 || openPr;
  return (
    <span className="flex min-w-0 flex-col justify-center gap-0.5 overflow-hidden text-[12px] tabular-nums">
      {commits && (
        <span className="flex min-w-0 items-center gap-2.5">
          {parts}
          {openPr}
        </span>
      )}
      {modified}
    </span>
  );
}

function RowAction({
  label,
  onClick,
  className,
  children,
}: {
  label: string;
  onClick: () => void;
  className?: string;
  children: ReactNode;
}) {
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
          className={cn("size-7 text-muted-foreground hover:text-foreground [&_svg]:size-[15px]", className)}
        >
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export function CloneMenu({ clone: c, root, children }: { clone: CloneInfo; root: string; children: ReactNode }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-60">
        <CloneMenuItems clone={c} root={root} />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** A clone's menu items, shared by its ••• button and its right-click menu. */
export function CloneMenuItems({ clone: c, root }: { clone: CloneInfo; root: string }) {
  const prefs = usePrefs();
  const mount = useMountToggle(root);
  const openDiff = useOpenDiff(root);
  const resolver = useRepoConfig(c.operation ? root : undefined).data?.config.update?.resolver;
  const qc = useQueryClient();
  return (
    <>
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
      <DropdownMenuSeparator />
      <DropdownMenuItem onSelect={() => void checkpointNow(qc, root, c.name)}>
        <History /> Checkpoint
      </DropdownMenuItem>
      <DropdownMenuItem disabled={!c.checkpoints} onSelect={() => setUi({ rewind: { root, clone: c } })}>
        <Undo2 /> Rewind…
        {!!c.checkpoints && <DropdownMenuShortcut>{c.checkpoints}</DropdownMenuShortcut>}
      </DropdownMenuItem>
      <DropdownMenuItem onSelect={() => forkClone(root, c.name)}>
        <GitFork /> Fork
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem onSelect={() => mount.mutate({ name: c.name, mounted: c.mounted })}>
        {c.mounted ? <Unplug /> : <HardDrive />}
        {c.mounted ? "Unmount" : "Mount"}
        <DropdownMenuShortcut>M</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuItem variant="destructive" onSelect={() => setUi({ remove: { root, clones: [c] } })}>
        <Trash2 /> Remove clone…
        <DropdownMenuShortcut>{isMac ? "⌘⌫" : "Ctrl+⌫"}</DropdownMenuShortcut>
      </DropdownMenuItem>
    </>
  );
}

/** Opens a clone's changes in the diff view. */
export function useOpenDiff(root: string) {
  const navigate = useNavigate();
  const repoId = useRepoList().data?.find((r) => r.root === root)?.id;
  return (c: CloneInfo) => {
    if (repoId && c.mounted) void navigate({ to: "/repo/$repoId/diff/$clone", params: { repoId, clone: c.name } });
  };
}
