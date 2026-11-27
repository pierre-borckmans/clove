import type { CodeViewItem, FileDiffMetadata } from "@pierre/diffs";
import {
  CodeView,
  type CodeViewHandle,
  type CodeViewReactOptions,
  WorkerPoolContextProvider,
} from "@pierre/diffs/react";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import {
  ArrowDownWideNarrow,
  ArrowLeft,
  Boxes,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Code,
  FileDiff as FileDiffIcon,
  Folder,
  GitBranch,
  LoaderCircle,
  Package as PackageIcon,
  TriangleAlert,
  WrapText,
} from "lucide-react";
import { memo, type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { PaneHeader } from "@/components/app-shell";
import { Empty } from "@/components/empty";
import { Shortcut } from "@/components/shortcut";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useHotkey } from "@/hooks/use-hotkey";
import { useTheme } from "@/hooks/use-theme";
import { api } from "@/lib/api";
import { isTauri } from "@/lib/bridge";
import { highlightWorker, useParsedPatch } from "@/lib/diff-workers";
import { APP_LABELS, setPrefs, usePrefs } from "@/lib/prefs";
import { openIn, useCloneDiff, useClones, useRepo } from "@/lib/queries";
import type { CloneDiff, FileAttrs, Package } from "@/lib/types";
import { cn } from "@/lib/utils";

/** Why a file starts folded; binary files have nothing to unfold. */
type Hidden = "binary" | "generated" | "lockfile" | "large";

type Entry = {
  id: string;
  file: FileDiffMetadata;
  added: number;
  removed: number;
  attrs?: FileAttrs;
  pkg?: Package;
  hidden?: Hidden;
};
type Group = {
  /** A directory: the package's, or a folder for files with no package of their own. */
  key: string;
  /** The manifest's name when it says more than the directory does. */
  name?: string;
  pkg?: Package;
  entries: Entry[];
  added: number;
  removed: number;
};

const THEME = { dark: "vitesse-dark", light: "vitesse-light" } as const;
/** The theme sets its background; the app's card colour wins instead. */
const DIFF_CSS = `* { --diffs-bg: var(--card) !important; --diffs-light-bg: var(--card) !important; --diffs-dark-bg: var(--card) !important; }`;
/** Folded until asked for, like GitHub: reading them rarely helps a review. */
const LOCKFILE =
  /(^|\/)(pnpm-lock\.yaml|package-lock\.json|npm-shrinkwrap\.json|yarn\.lock|bun\.lockb?|Cargo\.lock|go\.sum|poetry\.lock|uv\.lock|Pipfile\.lock|Gemfile\.lock|composer\.lock|flake\.lock|mix\.lock)$/;
/** Changed lines past which a file starts folded. */
const LARGE = 800;
const TREE_MIN = 180;
const TREE_MAX = 520;

const POOL = {
  workerFactory: highlightWorker,
  poolSize: Math.min(4, Math.max(2, (navigator.hardwareConcurrency || 4) - 2)),
};
const HIGHLIGHTER = { theme: THEME };

const KIND: Record<FileDiffMetadata["type"], { letter: string; className: string; label: string }> = {
  new: { letter: "A", className: "text-sage", label: "added" },
  deleted: { letter: "D", className: "text-rose", label: "deleted" },
  change: { letter: "M", className: "text-amber", label: "modified" },
  "rename-pure": { letter: "R", className: "text-sky-600 dark:text-sky-400", label: "renamed" },
  "rename-changed": { letter: "R", className: "text-sky-600 dark:text-sky-400", label: "renamed and modified" },
};

const HIDDEN_LABEL: Record<Hidden, string> = {
  binary: "binary",
  generated: "generated",
  lockfile: "lockfile",
  large: "large diff",
};

export function DiffPage() {
  const { repoId, clone: name } = useParams({ from: "/shell/repo/$repoId/diff/$clone" });
  const navigate = useNavigate();
  const { repo } = useRepo(repoId);
  const clone = useClones(repo?.root).data?.find((c) => c.name === name);
  const diff = useCloneDiff(repo?.root, name);
  const parsed = useParsedPatch(diff.data?.patch);
  const prefs = usePrefs();

  const entries = useMemo(() => toEntries(parsed.files, diff.data), [parsed.files, diff.data]);
  const groups = useMemo(
    () => toGroups(entries, prefs.diffSort, prefs.diffGroup),
    [entries, prefs.diffSort, prefs.diffGroup],
  );
  const ordered = useMemo(() => groups.flatMap((g) => g.entries), [groups]);
  const totals = useMemo(
    () =>
      entries.reduce((t, e) => ({ added: t.added + e.added, removed: t.removed + e.removed }), {
        added: 0,
        removed: 0,
      }),
    [entries],
  );

  const [activeId, setActiveId] = useState<string>();
  const view = useRef<CodeViewHandle<undefined, undefined>>(null);
  const jump = useCallback((id: string) => {
    view.current?.scrollTo({ type: "item", id, align: "start", behavior: "smooth-auto" });
    setActiveId(id);
  }, []);
  const step = (by: number) => {
    const i = ordered.findIndex((e) => e.id === activeId);
    const next = ordered[Math.min(ordered.length - 1, Math.max(0, Math.max(i, 0) + by))];
    if (next) jump(next.id);
  };

  const back = () => void navigate({ to: "/repo/$repoId", params: { repoId } });
  const ready = ordered.length > 0;
  useHotkey({ key: "Escape" }, back);
  useHotkey({ key: "j" }, () => step(1), ready);
  useHotkey({ key: "k" }, () => step(-1), ready);
  useHotkey({ key: "s" }, () => setPrefs({ diffStyle: prefs.diffStyle === "split" ? "unified" : "split" }));
  useHotkey({ key: "w" }, () => setPrefs({ diffWrap: !prefs.diffWrap }));
  useHotkey({ key: "o" }, () => setPrefs({ diffSort: prefs.diffSort === "path" ? "changes" : "path" }));
  useHotkey({ key: "g" }, () => setPrefs({ diffGroup: !prefs.diffGroup }));

  // Two phases worth showing: git computing the patch, then the worker parsing it.
  const loading = !repo || diff.isLoading || (parsed.parsing && parsed.files === undefined);

  return (
    <>
      <PaneHeader>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon-sm" className="-ml-1.5 size-7 text-muted-foreground" asChild>
              <Link to="/repo/$repoId" params={{ repoId }} aria-label="Back to clones">
                <ArrowLeft className="size-4" />
              </Link>
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            Back <Shortcut keys="escape" className="ml-1" />
          </TooltipContent>
        </Tooltip>
        <h1 className="truncate text-[14px] font-semibold tracking-tight">{name}</h1>
        {diff.data && (
          <span className="flex min-w-0 items-center gap-1.5 font-mono text-[12px] text-muted-foreground">
            <GitBranch className="size-3 shrink-0 text-faint" />
            <span className="truncate">{clone?.branch ?? name}</span>
            <span className="text-faint">vs</span>
            <span className="truncate">{diff.data.base}</span>
          </span>
        )}
        <div data-tauri-drag-region className="h-full flex-1" />
        {(diff.isFetching || parsed.parsing) && !loading && (
          <LoaderCircle className="size-3.5 animate-spin text-faint" aria-label="Refreshing" />
        )}
        {ready && (
          <span className="inline-flex h-6 items-center gap-2 rounded-full border border-border/80 bg-card px-2.5 font-mono text-[11.5px] tabular-nums shadow-2xs">
            <span className="font-medium text-sage">+{totals.added}</span>
            <span className="font-medium text-rose">−{totals.removed}</span>
            <span className="text-border">·</span>
            <span className="font-sans text-muted-foreground">
              {entries.length} {entries.length === 1 ? "file" : "files"}
            </span>
          </span>
        )}
        <Segmented
          value={prefs.diffStyle}
          onChange={(v) => setPrefs({ diffStyle: v })}
          options={[
            ["split", "Split"],
            ["unified", "Unified"],
          ]}
        />
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-pressed={prefs.diffWrap}
              onClick={() => setPrefs({ diffWrap: !prefs.diffWrap })}
              className={cn("size-7 text-muted-foreground", prefs.diffWrap && "bg-accent text-foreground")}
              aria-label="Wrap long lines"
            >
              <WrapText className="size-4" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            Wrap long lines <Shortcut keys="w" className="ml-1" />
          </TooltipContent>
        </Tooltip>
        {clone && (
          <Button
            variant="outline"
            size="sm"
            className="h-7 gap-1.5 text-[12.5px]"
            onClick={() => void openIn(clone.path, prefs.editor)}
          >
            <Code className="size-3.5" />
            {APP_LABELS[prefs.editor]}
          </Button>
        )}
      </PaneHeader>

      {diff.error || parsed.error ? (
        <Empty icon={<TriangleAlert />} title="Can't show the changes" className="flex-1">
          {String(diff.error ?? parsed.error)}
        </Empty>
      ) : loading ? (
        <Loading phase={diff.isLoading || !repo ? "git" : "parse"} bytes={diff.data?.patch.length} />
      ) : !ready || !repo || !diff.data ? (
        <Empty icon={<FileDiffIcon />} title="No changes" className="flex-1">
          {name} matches {diff.data?.base}: nothing committed, staged or modified.
        </Empty>
      ) : (
        <div className="flex min-h-0 flex-1">
          <FileList groups={groups} grouped={prefs.diffGroup} activeId={activeId} onPick={jump} />
          <WorkerPoolContextProvider poolOptions={POOL} highlighterOptions={HIGHLIGHTER}>
            <Diffs
              view={view}
              root={repo.root}
              name={name}
              diff={diff.data}
              ordered={ordered}
              generation={parsed.generation}
              onActive={setActiveId}
            />
          </WorkerPoolContextProvider>
        </div>
      )}
    </>
  );
}

/** The scroll view: virtualized by CodeView, highlighted in the worker pool. */
const Diffs = memo(function Diffs({
  view,
  root,
  name,
  diff,
  ordered,
  generation,
  onActive,
}: {
  view: React.Ref<CodeViewHandle<undefined, undefined>>;
  root: string;
  name: string;
  diff: CloneDiff;
  ordered: Entry[];
  generation: number;
  onActive: (id: string) => void;
}) {
  const theme = useTheme();
  const prefs = usePrefs();
  const [opened, setOpened] = useState<Set<string>>(new Set());
  const byId = useMemo(() => new Map(ordered.map((e) => [e.id, e])), [ordered]);

  // One version per item: new content (generation) or a fold toggled.
  const items = useMemo<CodeViewItem<undefined>[]>(
    () =>
      ordered.map((e) => {
        const folded = e.hidden === "binary" || (!!e.hidden && !opened.has(e.id));
        return {
          id: e.id,
          type: "diff",
          fileDiff: e.file,
          collapsed: folded,
          version: generation * 2 + (folded ? 1 : 0),
        };
      }),
    [ordered, opened, generation],
  );

  const mergeBase = diff.merge_base;
  const options = useMemo<CodeViewReactOptions<undefined, undefined>>(
    () => ({
      theme: THEME,
      themeType: theme,
      unsafeCSS: DIFF_CSS,
      diffStyle: prefs.diffStyle,
      overflow: prefs.diffWrap ? "wrap" : "scroll",
      lineDiffType: "word-alt",
      hunkSeparators: "line-info",
      stickyHeaders: true,
      layout: { paddingTop: 12, paddingBottom: 96, gap: 12 },
      // Expanding unchanged lines needs both whole files. Files git shows
      // through a filter or textconv are not their bytes on disk: no context.
      loadDiffFiles: isTauri
        ? async (f) => {
            if (byId.get(f.name)?.attrs?.converted) throw new Error("shown through a filter or textconv");
            const old = f.prevName ?? f.name;
            const [a, b] = await Promise.all([
              api.cloneFile(root, name, mergeBase, old),
              api.cloneFile(root, name, null, f.name),
            ]);
            const newFile = { name: f.name, contents: b };
            return f.type === "rename-pure"
              ? { oldFile: null, newFile }
              : { oldFile: { name: old, contents: a }, newFile };
          }
        : undefined,
    }),
    [theme, prefs.diffStyle, prefs.diffWrap, root, name, mergeBase, byId],
  );

  const toggle = useCallback(
    (id: string) =>
      setOpened((s) => {
        const n = new Set(s);
        n.has(id) ? n.delete(id) : n.add(id);
        return n;
      }),
    [],
  );

  const renderMeta = useCallback(
    (item: CodeViewItem<undefined>) => {
      const e = byId.get(item.id);
      if (!e?.hidden) return null;
      if (e.hidden === "binary") return <Badge>binary · not shown</Badge>;
      const folded = !opened.has(e.id);
      return (
        <button
          type="button"
          onClick={() => toggle(e.id)}
          className="flex items-center gap-1.5 text-[11.5px] text-muted-foreground hover:text-foreground"
          title={
            e.hidden === "generated"
              ? "Marked linguist-generated or linguist-vendored in .gitattributes"
              : e.hidden === "large"
                ? `Over ${LARGE} changed lines`
                : undefined
          }
        >
          <Badge>{HIDDEN_LABEL[e.hidden]}</Badge>
          {folded ? "Show" : "Hide"}
          <ChevronRight className={cn("size-3 transition-transform", !folded && "rotate-90")} />
        </button>
      );
    },
    [byId, opened, toggle],
  );

  // The file list follows the item at the top, at most once a frame.
  const frame = useRef(0);
  const scroller = useRef<HTMLDivElement | null>(null);
  const onScroll = useCallback(
    (top: number, viewer: { getScrollAnchor(t: number): { id: string } | undefined; getScrollHeight(): number }) => {
      if (frame.current) return;
      frame.current = requestAnimationFrame(() => {
        frame.current = 0;
        const el = scroller.current;
        const atEnd = el && top + el.clientHeight >= viewer.getScrollHeight() - 2;
        const id = atEnd ? ordered[ordered.length - 1]?.id : viewer.getScrollAnchor(top)?.id;
        if (id) onActive(id);
      });
    },
    [ordered, onActive],
  );
  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  const header = useCallback(
    () =>
      diff.truncated ? (
        <Notice>This diff is too big to show in full. The files after the cut are left out.</Notice>
      ) : null,
    [diff.truncated],
  );
  const footer = useCallback(
    () =>
      diff.skipped.length > 0 ? (
        <Notice>
          Not shown, untracked and over 1 MB: <span className="font-mono">{diff.skipped.join(", ")}</span>
        </Notice>
      ) : null,
    [diff.skipped],
  );

  return (
    <CodeView
      ref={view}
      items={items}
      options={options}
      onScroll={onScroll}
      renderHeaderMetadata={renderMeta}
      renderCodeViewHeader={header}
      renderCodeViewFooter={footer}
      className="diff-pane min-w-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain px-4"
      containerRef={scroller}
    />
  );
});

function toEntries(files: FileDiffMetadata[] | undefined, diff: CloneDiff | undefined): Entry[] {
  if (!files || !diff) return [];
  const attrs = new Map(diff.attrs.map((a) => [a.path, a]));
  // Innermost first, so the first prefix match is the file's own package.
  const pkgOf = (path: string) => diff.packages.find((p) => p.dir === "" || path.startsWith(`${p.dir}/`));
  const seen = new Set<string>();
  return files.map((file) => {
    const a = attrs.get(file.name);
    let added = 0;
    let removed = 0;
    for (const h of file.hunks) {
      added += h.additionLines;
      removed += h.deletionLines;
    }
    // A changed file without hunks is git's "Binary files … differ".
    const binary = !!a?.binary || (file.hunks.length === 0 && file.type !== "rename-pure");
    const hidden: Hidden | undefined = binary
      ? "binary"
      : a?.generated
        ? "generated"
        : LOCKFILE.test(file.name)
          ? "lockfile"
          : added + removed > LARGE
            ? "large"
            : undefined;
    // Ids must be unique; a path shows up twice only in odd patches.
    let id = file.name;
    while (seen.has(id)) id += "#";
    seen.add(id);
    return { id, file, added, removed, attrs: a, pkg: pkgOf(file.name), hidden };
  });
}

/**
 * Where a file is grouped: its package, or for files only the repo root's
 * manifest covers (proto/, gen/typescript/…), their top two folders.
 */
function groupOf(e: Entry): { key: string; pkg?: Package } {
  if (e.pkg?.dir) return { key: e.pkg.dir, pkg: e.pkg };
  const dirs = e.file.name.split("/").slice(0, -1);
  if (dirs.length === 0) return { key: "", pkg: e.pkg };
  return { key: dirs.slice(0, 2).join("/") };
}

/** Go module paths and names equal to the folder repeat the directory. */
function nameOf(pkg: Package | undefined, key: string): string | undefined {
  if (!pkg || pkg.manifest === "go.mod") return undefined;
  const base = key.split("/").pop();
  return pkg.name && pkg.name !== base && pkg.name !== key ? pkg.name : undefined;
}

function toGroups(entries: Entry[], sort: "path" | "changes", group: boolean): Group[] {
  const weight = (e: { added: number; removed: number }) => e.added + e.removed;
  const sorted = sort === "changes" ? [...entries].sort((a, b) => weight(b) - weight(a)) : entries;
  if (!group) return [{ key: "*", entries: sorted, added: 0, removed: 0 }];
  const by = new Map<string, Group>();
  for (const e of sorted) {
    const { key: k, pkg } = groupOf(e);
    const g = by.get(k) ?? { key: k, pkg, name: nameOf(pkg, k), entries: [], added: 0, removed: 0 };
    g.entries.push(e);
    g.added += e.added;
    g.removed += e.removed;
    by.set(k, g);
  }
  const list = [...by.values()];
  return sort === "changes"
    ? list.sort((a, b) => weight(b) - weight(a))
    : list.sort((a, b) => a.key.localeCompare(b.key));
}

function Loading({ phase, bytes }: { phase: "git" | "parse"; bytes?: number }) {
  const prefs = usePrefs();
  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex shrink-0 flex-col gap-2 border-r bg-muted/30 p-3" style={{ width: prefs.diffTree }}>
        {[70, 55, 80, 45, 65, 50].map((w) => (
          <Skeleton key={w} className="h-7 rounded-md" style={{ width: `${w}%` }} />
        ))}
      </div>
      <div className="flex flex-1 flex-col gap-3 p-4">
        <div className="flex items-center gap-2 text-[12.5px] text-muted-foreground">
          <LoaderCircle className="size-3.5 animate-spin" />
          {phase === "git"
            ? "Reading changes from git…"
            : `Parsing ${bytes ? `${Math.max(1, Math.round(bytes / 1024))} KB of ` : ""}patch…`}
        </div>
        <Skeleton className="h-40 rounded-lg" />
        <Skeleton className="h-64 rounded-lg" />
      </div>
    </div>
  );
}

const FileList = memo(function FileList({
  groups,
  grouped,
  activeId,
  onPick,
}: {
  groups: Group[];
  grouped: boolean;
  activeId?: string;
  onPick: (id: string) => void;
}) {
  const prefs = usePrefs();
  const list = useRef<HTMLDivElement>(null);
  const nav = useRef<HTMLElement>(null);
  // Groups start folded: the list opens as the packages the branch touches.
  const [open, setOpen] = useState<Set<string>>(new Set());
  const allOpen = groups.every((g) => open.has(g.key));
  const toggleGroup = (g: Group) =>
    setOpen((s) => {
      const n = new Set(s);
      if (n.has(g.key)) n.delete(g.key);
      else {
        n.add(g.key);
        if (g.entries[0]) onPick(g.entries[0].id);
      }
      return n;
    });
  const packages = groups.filter((g) => g.pkg).length;
  const folders = groups.length - packages;
  // Keep the current file in view as the diff scrolls.
  useEffect(() => {
    if (!activeId) return;
    list.current?.querySelector(`[data-id="${CSS.escape(activeId)}"]`)?.scrollIntoView({ block: "nearest" });
  }, [activeId]);
  return (
    <nav ref={nav} className="relative flex shrink-0 flex-col border-r bg-muted/30" style={{ width: prefs.diffTree }}>
      <div className="flex h-9 shrink-0 items-center gap-1 border-b px-3">
        <span className="flex-1 text-[11.5px] text-muted-foreground">
          {grouped
            ? [plural(packages, "package"), folders ? plural(folders, "folder") : ""].filter(Boolean).join(" · ")
            : "Files"}
        </span>
        {grouped && (
          <ListToggle
            on={false}
            onClick={() => setOpen(allOpen ? new Set() : new Set(groups.map((g) => g.key)))}
            label={allOpen ? "Collapse all" : "Expand all"}
          >
            {allOpen ? <ChevronsDownUp /> : <ChevronsUpDown />}
          </ListToggle>
        )}
        <ListToggle
          on={prefs.diffSort === "changes"}
          onClick={() => setPrefs({ diffSort: prefs.diffSort === "path" ? "changes" : "path" })}
          label="Sort by most changed"
          keys="o"
        >
          <ArrowDownWideNarrow />
        </ListToggle>
        <ListToggle
          on={prefs.diffGroup}
          onClick={() => setPrefs({ diffGroup: !prefs.diffGroup })}
          label="Group by package"
          keys="g"
        >
          <Boxes />
        </ListToggle>
      </div>
      <div ref={list} className="min-h-0 flex-1 overflow-y-auto py-1.5">
        {groups.map((g) => {
          const shown = !grouped || open.has(g.key);
          return (
            <div key={g.key} className={cn(grouped && shown && "mb-1")}>
              {grouped && (
                <GroupRow
                  group={g}
                  open={shown}
                  current={!shown && g.entries.some((e) => e.id === activeId)}
                  onToggle={() => toggleGroup(g)}
                />
              )}
              {shown &&
                g.entries.map((e) => (
                  <FileRow
                    key={e.id}
                    entry={e}
                    active={e.id === activeId}
                    prefix={grouped ? g.key : ""}
                    onPick={onPick}
                  />
                ))}
            </div>
          );
        })}
      </div>
      <div className="flex flex-wrap gap-x-3 gap-y-1 border-t px-3 py-2 text-[11px] text-faint">
        <Hint keys="j" label="next" />
        <Hint keys="k" label="previous" />
        <Hint keys="s" label="split" />
        <Hint keys="w" label="wrap" />
        <Hint keys="o" label="sort" />
        <Hint keys="g" label="group" />
      </div>
      <ResizeHandle target={nav} />
    </nav>
  );
});

/**
 * Drags the tree's width. It sets the style directly while dragging and saves
 * the width once on release, so nothing re-renders mid-drag.
 */
function ResizeHandle({ target }: { target: React.RefObject<HTMLElement | null> }) {
  const [dragging, setDragging] = useState(false);
  const onPointerDown = (e: React.PointerEvent) => {
    const el = target.current;
    if (!el) return;
    e.preventDefault();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startW = el.offsetWidth;
    let w = startW;
    setDragging(true);
    const move = (ev: PointerEvent) => {
      w = Math.min(TREE_MAX, Math.max(TREE_MIN, startW + ev.clientX - startX));
      el.style.width = `${w}px`;
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setDragging(false);
      setPrefs({ diffTree: Math.round(w) });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  return (
    <div
      onPointerDown={onPointerDown}
      onDoubleClick={() => {
        if (target.current) target.current.style.width = "256px";
        setPrefs({ diffTree: 256 });
      }}
      title="Drag to resize, double-click to reset"
      className={cn(
        "absolute inset-y-0 -right-[3px] z-10 w-[6px] cursor-col-resize transition-colors",
        dragging ? "bg-bottle/40" : "hover:bg-bottle/25",
      )}
    />
  );
}

function plural(n: number, word: string) {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

function GroupRow({
  group: g,
  open,
  current,
  onToggle,
}: {
  group: Group;
  open: boolean;
  current: boolean;
  onToggle: () => void;
}) {
  const Icon = g.pkg ? PackageIcon : Folder;
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-expanded={open}
      title={g.pkg ? `${g.pkg.dir || "repository root"} · ${g.pkg.manifest}` : `${g.key}: no package manifest`}
      className={cn(
        "mx-1.5 flex w-[calc(100%-12px)] items-center gap-1.5 rounded-md px-1.5 py-1.5 text-left transition-colors hover:bg-foreground/[0.04]",
        current && "bg-foreground/[0.07]",
      )}
    >
      <ChevronRight className={cn("size-3 shrink-0 text-faint transition-transform", open && "rotate-90")} />
      <Icon className={cn("size-3.5 shrink-0", g.pkg ? "text-bottle" : "text-faint")} />
      <span className="min-w-0 flex-1">
        <span className="block truncate font-mono text-[12px] text-foreground/90">{g.key || "repository root"}</span>
        <span className="block truncate text-[10.5px] text-faint">
          {plural(g.entries.length, "file")}
          {g.name && ` · ${g.name}`}
        </span>
      </span>
      <span className="flex shrink-0 flex-col items-end font-mono text-[10.5px] leading-tight tabular-nums">
        {g.added > 0 && <span className="text-sage">+{g.added}</span>}
        {g.removed > 0 && <span className="text-rose">−{g.removed}</span>}
      </span>
    </button>
  );
}

const FileRow = memo(function FileRow({
  entry: e,
  active,
  prefix,
  onPick,
}: {
  entry: Entry;
  active: boolean;
  /** The group's directory, left out of the path under its heading. */
  prefix: string;
  onPick: (id: string) => void;
}) {
  const kind = KIND[e.file.type];
  const slash = e.file.name.lastIndexOf("/");
  const base = e.file.name.slice(slash + 1);
  const full = slash >= 0 ? e.file.name.slice(0, slash) : "";
  const dir = prefix && full.startsWith(prefix) ? full.slice(prefix.length + 1) : full;
  const note = e.hidden ? `${HIDDEN_LABEL[e.hidden]} · ` : "";
  return (
    <button
      type="button"
      data-id={e.id}
      onClick={() => onPick(e.id)}
      title={e.file.prevName ? `${e.file.prevName} → ${e.file.name}` : e.file.name}
      className={cn(
        "relative mx-1.5 flex w-[calc(100%-12px)] items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors",
        prefix !== "" && "pl-6",
        active
          ? "bg-bottle/[0.08] before:absolute before:inset-y-1 before:left-0 before:w-[2.5px] before:rounded-r-full before:bg-bottle dark:bg-bottle/[0.12]"
          : "hover:bg-foreground/[0.04]",
      )}
    >
      <span className={cn("w-3 shrink-0 font-mono text-[11px] font-semibold", kind.className)} title={kind.label}>
        {kind.letter}
      </span>
      <span className="min-w-0 flex-1">
        <span className={cn("block truncate text-[12.5px]", active ? "text-foreground" : "text-foreground/85")}>
          {base}
        </span>
        {(dir || note) && (
          <span className="block truncate font-mono text-[10.5px] text-faint">
            {note}
            {dir}
          </span>
        )}
      </span>
      <span className="flex shrink-0 flex-col items-end font-mono text-[10.5px] leading-tight tabular-nums">
        {e.added > 0 && <span className="text-sage">+{e.added}</span>}
        {e.removed > 0 && <span className="text-rose">−{e.removed}</span>}
      </span>
    </button>
  );
});

function ListToggle({
  on,
  onClick,
  label,
  keys,
  children,
}: {
  on: boolean;
  onClick: () => void;
  label: string;
  keys?: string;
  children: ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-pressed={on}
          aria-label={label}
          onClick={onClick}
          className={cn(
            "size-6 text-muted-foreground [&_svg]:size-3.5",
            on && "bg-bottle-soft text-bottle hover:bg-bottle-soft hover:text-bottle",
          )}
        >
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {label} {keys && <Shortcut keys={keys} className="ml-1" />}
      </TooltipContent>
    </Tooltip>
  );
}

function Segmented<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (v: T) => void;
  options: [T, string][];
}) {
  return (
    <div className="flex h-7 items-center rounded-lg border border-border/85 bg-muted/60 p-0.5 text-[12px]">
      {options.map(([v, label]) => (
        <button
          key={v}
          type="button"
          onClick={() => onChange(v)}
          className={cn(
            "h-full rounded-[5.5px] px-2.5 font-medium transition-all",
            v === value
              ? "bg-card text-foreground shadow-[var(--btn-outline-shadow)]"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

function Badge({ children }: { children: ReactNode }) {
  return (
    <span className="rounded-full bg-foreground/[0.07] px-1.5 font-sans text-[10.5px] leading-[17px] text-muted-foreground">
      {children}
    </span>
  );
}

function Hint({ keys, label }: { keys: string; label: string }) {
  return (
    <span className="flex items-center gap-1">
      <Shortcut keys={keys} className="[&_kbd]:h-4 [&_kbd]:min-w-4 [&_kbd]:text-[10px]" />
      {label}
    </span>
  );
}

function Notice({ children }: { children: ReactNode }) {
  return (
    <div className="my-3 flex items-start gap-2 rounded-lg border border-amber/30 bg-amber/8 px-3 py-2 text-[12.5px] text-foreground/85">
      <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-amber" />
      <span className="min-w-0">{children}</span>
    </div>
  );
}
