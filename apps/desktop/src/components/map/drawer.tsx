import { ArrowDown, ArrowUp, Copy, FileDiff, GitFork, History, Lock, TriangleAlert } from "lucide-react";
import { type ReactNode, useLayoutEffect, useRef } from "react";
import { useOpenDiff } from "@/components/clone-list";
import { GoldenSteps } from "@/components/golden-card";
import { MapInspector } from "@/components/map/inspector";
import { Button } from "@/components/ui/button";
import { barsFromTimings, Waterfall } from "@/components/waterfall";
import { ago, bytes, ms, shortSha, since } from "@/lib/format";
import { copyText, useCheckpoints, useCloneDiff, usePrs } from "@/lib/queries";
import type { CloneInfo, OtherWorktree, RepoStatus } from "@/lib/types";
import { setBulbs, setUi } from "@/lib/ui";
import { cn } from "@/lib/utils";

/** What the drawer shows: the bulb (current image), a clone, or a plain worktree. */
export type DrawerItem =
  | { kind: "image" }
  | { kind: "layer"; generation: number }
  | { kind: "clone"; clone: CloneInfo }
  | { kind: "worktree"; worktree: OtherWorktree };

export const drawerKey = (item: DrawerItem) =>
  item.kind === "image"
    ? "image"
    : item.kind === "layer"
      ? `g:${item.generation}`
      : item.kind === "clone"
        ? `c:${item.clone.name}`
        : `w:${item.worktree.path}`;

/**
 * Details over the map's bottom edge, above the inspector bar: slides up when
 * something is picked, back down when nothing is. It overlays the map, so the
 * map's size never changes; `onHeight` reports how much of it is covered.
 */
export function MapDrawer({
  status,
  clones,
  item,
  maxHeight,
  onHeight,
  refreshing,
}: {
  status: RepoStatus;
  clones: CloneInfo[];
  refreshing: boolean;
  /** The picked item, or null to slide down. */
  item: DrawerItem | null;
  maxHeight: number;
  onHeight: (px: number) => void;
}) {
  // Keep the last item while sliding down, so the content doesn't vanish first.
  const last = useRef<DrawerItem | null>(item);
  if (item) last.current = item;
  const shown = item ?? last.current;
  const panel = useRef<HTMLDivElement>(null);
  const open = !!item;
  const key = shown ? drawerKey(shown) : "";

  // Report the covered height when it opens, changes size, or closes.
  useLayoutEffect(() => {
    const el = panel.current;
    if (!el || !open) {
      onHeight(0);
      return;
    }
    onHeight(el.offsetHeight);
    const ro = new ResizeObserver(() => onHeight(el.offsetHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, [open, onHeight]);

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-full z-10 overflow-hidden">
      <section
        ref={panel}
        aria-label="Details"
        aria-hidden={!open}
        inert={!open}
        className={cn(
          "map-drawer pointer-events-auto overflow-y-auto border-t bg-card",
          open ? "map-drawer-open" : "map-drawer-closed",
        )}
        style={{ maxHeight }}
      >
        {shown && (
          <div key={key} className="map-drawer-content flex flex-col gap-3 px-5 pt-3 pb-3">
            {/* What it is and its quick actions, then its details. */}
            <MapInspector
              status={status}
              clones={clones}
              picked={shown.kind === "clone" ? shown.clone : undefined}
              pickedWt={shown.kind === "worktree" ? shown.worktree : undefined}
              generation={
                shown.kind === "layer"
                  ? shown.generation
                  : shown.kind === "clone"
                    ? shown.clone.golden_generation
                    : status.golden?.generation
              }
              refreshing={refreshing}
            />
            {shown.kind === "layer" ? null : shown.kind === "image" ? (
              <ImageDetails status={status} />
            ) : shown.kind === "clone" ? (
              <CloneDetails status={status} clone={shown.clone} clones={clones} />
            ) : (
              <WorktreeDetails worktree={shown.worktree} />
            )}
          </div>
        )}
      </section>
    </div>
  );
}

function Col({ title, children, className }: { title: string; children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex min-w-0 flex-col gap-1.5", className)}>
      <h3 className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">{title}</h3>
      {children}
    </div>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 items-baseline gap-2 text-[12.5px]">
      <span className="w-24 shrink-0 text-muted-foreground">{label}</span>
      <div className="flex min-w-0 flex-1 items-baseline gap-1.5">{children}</div>
    </div>
  );
}

function CopyPath({ path }: { path: string }) {
  return (
    <span className="flex min-w-0 items-center gap-1">
      <span className="truncate font-mono text-[11.5px]">{path}</span>
      <Button
        variant="ghost"
        size="icon-sm"
        className="size-5 shrink-0"
        aria-label="Copy path"
        onClick={() => void copyText(path, "Path copied")}
      >
        <Copy className="size-3" />
      </Button>
    </span>
  );
}

/** The bulb: what the golden image card shows, laid out wide and short. */
function ImageDetails({ status }: { status: RepoStatus }) {
  const g = status.golden;
  if (!g) return <p className="text-[12.5px] text-muted-foreground">No golden image yet.</p>;
  const behind = g.behind ?? 0;
  return (
    <div className="grid grid-cols-[minmax(220px,1fr)_minmax(0,2fr)] gap-x-8 gap-y-3">
      <Col title="Golden image">
        <Row label="Generation">
          <span className="font-mono">gen {g.generation}</span>
          {g.dirty && (
            <span className="flex items-center gap-1 text-amber">
              <TriangleAlert className="size-3.5" /> dirty
            </span>
          )}
        </Row>
        <Row label="Commit">
          <button
            type="button"
            className="font-mono hover:text-bottle"
            aria-label="Copy commit"
            onClick={() => void copyText(g.commit, "Commit copied")}
          >
            {shortSha(g.commit, 10)}
          </button>
        </Row>
        <Row label="Base">
          <span className="truncate font-mono">{g.base}</span>
          {behind > 0 ? (
            <span className="flex shrink-0 items-center gap-0.5 text-bottle">
              <ArrowDown className="size-3" />
              {behind} behind
            </span>
          ) : g.behind === null ? null : (
            <span className="shrink-0 text-sage">up to date</span>
          )}
        </Row>
        <Row label="Built">{ago(g.age_secs)}</Row>
        <Row label="Size">
          <span className="font-mono tabular-nums">{bytes(g.image_bytes)}</span>
          {status.backend && <span className="text-[11px] text-faint uppercase">{status.backend}</span>}
        </Row>
      </Col>
      <Col title="Warm steps">
        <GoldenSteps status={status} />
      </Col>
    </div>
  );
}

function CloneDetails({ status, clone: c, clones }: { status: RepoStatus; clone: CloneInfo; clones: CloneInfo[] }) {
  const { data: prs } = usePrs(status.root);
  const pr = prs?.get(c.name)?.pr;
  const openDiff = useOpenDiff(status.root);
  const diff = useCloneDiff(c.mounted ? status.root : undefined, c.name);
  const files = diff.data ? (diff.data.patch.match(/^diff --git /gm) ?? []).length : null;
  const checkpoints = useCheckpoints(status.root, c.name);
  const forks = clones.filter((x) => x.forked_from === c.name);
  const total = c.total_ms ?? c.timings.reduce((n, t) => n + t.ms, 0);
  // Nothing about checkpoints or forks unless the clone has some.
  const saved = checkpoints.data ?? [];
  const lineage = saved.length > 0 || !!c.forked_from || forks.length > 0;
  return (
    <div className={cn("grid gap-x-8 gap-y-3", lineage ? "grid-cols-3" : "grid-cols-2")}>
      <Col title="Branch">
        <Row label="Branch">
          <span className="truncate font-mono">{c.branch ?? `detached at ${shortSha(c.head)}`}</span>
        </Row>
        <Row label="PR">
          {pr ? (
            <>
              <span className={cn(pr.state === "merged" ? "text-sage" : "text-foreground")}>
                #{pr.number} · {pr.state}
              </span>
              {pr.url && (
                <Button
                  variant="ghost"
                  size="icon-sm"
                  className="size-5"
                  aria-label="Copy PR link"
                  onClick={() => void copyText(pr.url, "PR link copied")}
                >
                  <Copy className="size-3" />
                </Button>
              )}
            </>
          ) : (
            <span className="text-faint">none</span>
          )}
        </Row>
        <Row label="Changes">
          {c.dirty ? <span className="text-amber">uncommitted</span> : <span className="text-faint">clean</span>}
          {files !== null && files > 0 && <span className="text-muted-foreground">{files} files vs base</span>}
        </Row>
        <Row label="Ahead">
          <span className="flex items-center gap-2 font-mono tabular-nums">
            <span className="flex items-center">
              <ArrowUp className="size-3" />
              {c.ahead ?? 0}
            </span>
            <span className="flex items-center">
              <ArrowDown className="size-3" />
              {c.behind ?? 0}
            </span>
          </span>
        </Row>
        {c.mounted && (
          <div>
            <Button variant="outline" size="sm" className="h-6 text-[11.5px]" onClick={() => openDiff(c)}>
              <FileDiff className="size-3" /> View changes
            </Button>
          </div>
        )}
      </Col>
      {lineage && (
        <Col
          title={saved.length ? (forks.length || c.forked_from ? "Checkpoints & lineage" : "Checkpoints") : "Lineage"}
        >
          {saved.length > 0 && (
            <>
              <ul className="flex flex-col gap-0.5" aria-label="Checkpoints">
                {saved.slice(0, 5).map((k) => (
                  <li key={k.id} className="flex items-baseline gap-2 text-[12px]">
                    <History className="size-3 shrink-0 self-center text-faint" />
                    <span className="truncate font-mono">{k.id}</span>
                    <span className="shrink-0 text-muted-foreground">{since(k.created_at)}</span>
                    <span className="ml-auto shrink-0 font-mono text-[11px] text-faint tabular-nums">
                      {bytes(k.own_bytes)}
                    </span>
                  </li>
                ))}
                {saved.length > 5 && <li className="text-[11.5px] text-faint">and {saved.length - 5} more</li>}
              </ul>
              <div>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-6 text-[11.5px]"
                  onClick={() => setUi({ rewind: { root: status.root, clone: c } })}
                >
                  Rewind…
                </Button>
              </div>
            </>
          )}
          {c.forked_from && (
            <Row label="Forked from">
              <span className="truncate font-mono">{c.forked_from}</span>
            </Row>
          )}
          {forks.length > 0 && (
            <Row label="Forks">
              <span>{forks.length}</span>
              <button
                type="button"
                className="flex items-center gap-1 text-bottle hover:underline"
                onClick={() => setBulbs(status.root, [c.name], true)}
              >
                <GitFork className="size-3" /> show forks
              </button>
            </Row>
          )}
        </Col>
      )}
      <Col title="Setup & disk">
        {c.timings.length > 0 ? (
          <Waterfall bars={barsFromTimings(c.timings)} total={total} dense />
        ) : (
          <span className="text-[12px] text-faint">No setup timings.</span>
        )}
        {total > 0 && <Row label="Ready in">{ms(total)}</Row>}
        <Row label="Disk">
          <span className="font-mono tabular-nums">
            {c.own_bytes === undefined ? "?" : bytes(c.own_bytes)} own · {bytes(c.image_bytes)} image
          </span>
        </Row>
        <Row label="Path">
          <CopyPath path={c.path} />
        </Row>
      </Col>
    </div>
  );
}

function WorktreeDetails({ worktree: w }: { worktree: OtherWorktree }) {
  return (
    <div className="grid grid-cols-3 gap-x-8 gap-y-3">
      <Col title="Worktree">
        <Row label="Branch">
          <span className="truncate font-mono">{w.branch ?? `detached at ${shortSha(w.head)}`}</span>
        </Row>
        <Row label="PR">{w.pr ? `#${w.pr.number} · ${w.pr.state}` : <span className="text-faint">none</span>}</Row>
        <Row label="Path">
          <CopyPath path={w.path} />
        </Row>
        {w.locked && (
          <Row label="Locked">
            <Lock className="size-3 self-center" />
            <span className="truncate">{w.lock_reason ?? "yes"}</span>
          </Row>
        )}
        {!!w.detached_commits && <Row label="Unsaved">{w.detached_commits} commits on no branch</Row>}
      </Col>
      <Col title="Changes" className="col-span-2">
        {w.changes?.length ? (
          <ul className="grid grid-cols-2 gap-x-6 font-mono text-[11.5px]">
            {w.changes.map((l) => (
              <li key={l} className="truncate">
                {l}
              </li>
            ))}
            {(w.changes_total ?? 0) > w.changes.length && (
              <li className="text-faint">and {(w.changes_total ?? 0) - w.changes.length} more</li>
            )}
          </ul>
        ) : (
          <span className="text-[12px] text-faint">{w.missing ? "Folder missing." : "No uncommitted changes."}</span>
        )}
        <p className="text-[11.5px] text-faint">
          {w.adopt_blocker ?? "Not from the golden image: making it a clone moves it onto copy-on-write storage."}
        </p>
      </Col>
    </div>
  );
}
