import {
  ArrowDown,
  ArrowUp,
  Code,
  CopyPlus,
  Ellipsis,
  FileDiff,
  GitBranch,
  List,
  Lock,
  Minus,
  Plus,
  RefreshCw,
  Scan,
  SquareTerminal,
  X,
} from "lucide-react";
import { useMemo, useState } from "react";
import { CloneMenu, useOpenDiff } from "@/components/clone-list";
import { CloveGlyph, Glyph } from "@/components/glyph";
import { GoldenCard } from "@/components/golden-card";
import { Button } from "@/components/ui/button";
import { WorktreeList, WorktreeMenu } from "@/components/worktree-list";
import { useMapCamera } from "@/hooks/use-map-camera";
import { ago, bytes, shortSha, since } from "@/lib/format";
import { startOp, useOps } from "@/lib/ops";
import { APP_LABELS, usePrefs } from "@/lib/prefs";
import { openIn, useMountToggle, usePrCheck, usePrs } from "@/lib/queries";
import type { CloneInfo, OtherWorktree, RepoStatus } from "@/lib/types";
import { setUi } from "@/lib/ui";
import { cn } from "@/lib/utils";

type Selection =
  | { kind: "image"; generation: number }
  | { kind: "clone"; name: string }
  | { kind: "worktree"; path: string };

/** Rings past the inner one; clones older than this share the outermost. */
const MAX_RING = 3;
const RING_GAP = 46;
/** Orbits are ellipses, wider than tall, like the pane they sit in. */
const STRETCH = 1.6;
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
/** The tray of plain worktrees, beside the rings: they have no image. */
const TRAY = { gap: 120, width: 250, head: 58, row: 54, pad: 16 };
/** Where the core's rim is, for edges to start at. */
const CORE_RADIUS = 78;

/** An image-to-clone map. The edges are provenance, not inferred Git ancestry. */
export function RepoGraph({
  status,
  clones,
  worktrees,
}: {
  status: RepoStatus;
  clones: CloneInfo[];
  worktrees: OtherWorktree[];
}) {
  const [selection, setSelection] = useState<Selection | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [panel, setPanel] = useState<"image" | "worktrees" | null>(null);
  const prefs = usePrefs();
  const openDiff = useOpenDiff(status.root);
  const { data: prs } = usePrs(status.root);
  usePrCheck(status.root);
  const mount = useMountToggle(status.root);
  const ops = useOps();
  const refreshing = ops.some(
    (o) => o.status === "running" && o.req.root === status.root && (o.req.kind === "refresh" || o.req.kind === "init"),
  );
  const current = status.golden?.generation;

  // Distance means age: clones of the current image sit on the inner ring,
  // each older generation one ring out (the last ring holds everything older).
  const layout = useMemo(() => {
    const newest = current ?? Math.max(0, ...clones.map((c) => c.golden_generation));
    const ringOf = (generation: number) => Math.min(Math.max(0, newest - generation), MAX_RING);
    const ringCount = clones.length ? Math.max(...clones.map((c) => ringOf(c.golden_generation))) + 1 : 1;
    const inner = Math.max(118, clones.length * 6);
    const radiusOf = (ring: number) => inner + ring * RING_GAP;
    const outer = radiusOf(ringCount - 1);
    const orbitWidth = outer * STRETCH * 2 + 460;
    // Plain worktrees sit apart, in a tray right of the rings' labels.
    const trayHeight = worktrees.length ? TRAY.head + worktrees.length * TRAY.row + TRAY.pad : 0;
    const width = orbitWidth + (worktrees.length ? TRAY.width + TRAY.gap - 120 : 0);
    const height = Math.max(outer * 2 + 150, trayHeight + 80);
    const cx = orbitWidth / 2;
    const cy = height / 2;
    // Newest first, clockwise from the top; the top stays free for ring labels.
    const ordered = [...clones].sort((a, b) => b.created_at - a.created_at);
    const step = (Math.PI * 2) / Math.max(1, ordered.length);
    const nodes = ordered.map((clone, i) => {
      const angle = -Math.PI / 2 + step * (i + 0.5);
      const ring = ringOf(clone.golden_generation);
      const r = radiusOf(ring);
      const cos = Math.cos(angle);
      const sin = Math.sin(angle);
      const side = cos > 0.35 ? "right" : cos < -0.35 ? "left" : sin < 0 ? "top" : "bottom";
      return { clone, angle, ring, side, x: cx + cos * r * STRETCH, y: cy + sin * r };
    });
    const tray = worktrees.length
      ? { x: orbitWidth - 120 + TRAY.gap, y: cy - trayHeight / 2, width: TRAY.width, height: trayHeight }
      : null;
    const looseNodes = tray
      ? worktrees.map((worktree, i) => ({
          worktree,
          side: "right" as const,
          x: tray.x + 30,
          y: tray.y + TRAY.head + TRAY.row * i + 10,
        }))
      : [];
    const rings = Array.from({ length: ringCount }, (_, ring) => {
      const gens = [...new Set(nodes.filter((n) => n.ring === ring).map((n) => n.clone.golden_generation))];
      return { ring, r: radiusOf(ring), generations: gens.sort((a, b) => b - a) };
    });
    return { nodes, rings, looseNodes, tray, width, height, cx, cy, newest };
  }, [clones, current, worktrees]);

  const { viewport, camera, dragging, hand, fit, zoomBy, bindings } = useMapCamera(layout.width, layout.height);
  const picked = selection?.kind === "clone" ? clones.find((c) => c.name === selection.name) : undefined;
  const generation = picked?.golden_generation ?? (selection?.kind === "image" ? selection.generation : current);
  const currentImage = current !== undefined && generation === current;
  const selectedPr = picked && prs?.get(picked.name)?.pr;
  const pickedWt = selection?.kind === "worktree" ? worktrees.find((w) => w.path === selection.path) : undefined;
  // With an image or ring picked, its clones stay lit and the rest dim.
  const focusRing =
    !picked && selection?.kind === "image"
      ? (layout.rings.find((r) => r.generations.includes(selection.generation))?.ring ?? 0)
      : null;
  const dimmed = (ring: number) => focusRing !== null && ring !== focusRing;

  return (
    <section className="repo-map flex min-h-0 flex-1 flex-col" aria-label="Repository map">
      <div className="flex h-12 shrink-0 items-center gap-3 border-b px-5">
        <h2 className="text-[13px] font-medium">Image lineage</h2>
        <span className="text-[12px] text-muted-foreground">
          {clones.length} clones · {new Set(clones.map((c) => c.golden_generation)).size} image generations
        </span>
        <div className="flex-1" />
        {worktrees.length > 0 && (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 text-[12px]"
            onClick={() => setPanel(panel === "worktrees" ? null : "worktrees")}
          >
            <List className="size-3.5" /> Other worktrees <span className="text-faint">{worktrees.length}</span>
          </Button>
        )}
      </div>
      <p id="map-navigation-help" className="sr-only">
        Connections show image origin, not Git ancestry. Drag or scroll to pan. Pinch or hold Command or Control while
        scrolling to zoom. Focus the map and use arrow keys to pan, plus and minus to zoom, or zero to fit. Hold Space
        to drag over a node.
      </p>
      <section
        ref={viewport}
        {...bindings}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: This canvas implements keyboard pan, zoom, and fit.
        tabIndex={0}
        role="application"
        aria-label="Interactive image map"
        aria-describedby="map-navigation-help"
        data-panning={dragging || undefined}
        data-hand={hand || undefined}
        className="map-viewport relative min-h-0 flex-1 overflow-clip outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50"
        style={{
          backgroundPosition: `${camera.x}px ${camera.y}px`,
          backgroundSize: `${18 * camera.scale}px ${18 * camera.scale}px`,
        }}
      >
        <div
          className="map-world absolute top-0 left-0"
          style={{
            width: layout.width,
            height: layout.height,
            transform: `translate(${camera.x}px, ${camera.y}px) scale(${camera.scale})`,
            transformOrigin: "top left",
          }}
        >
          <svg className="pointer-events-none absolute inset-0 size-full overflow-visible" aria-hidden="true">
            {layout.rings.map(({ ring, r }) => (
              <ellipse
                key={ring}
                className={cn("map-orbit", ring > 0 && "map-orbit-old", focusRing === ring && "map-orbit-focus")}
                cx={layout.cx}
                cy={layout.cy}
                rx={r * STRETCH}
                ry={r}
              />
            ))}
            {layout.nodes.map((node) => {
              const old = node.ring > 0;
              const dx = node.x - layout.cx;
              const dy = node.y - layout.cy;
              const len = Math.hypot(dx, dy);
              const ux = dx / len;
              const uy = dy / len;
              const sx = layout.cx + ux * CORE_RADIUS;
              const sy = layout.cy + uy * CORE_RADIUS;
              const ex = node.x - ux * 18;
              const ey = node.y - uy * 18;
              // A slight sweep, so edges read as growth rather than spokes.
              const bend = 8 + node.ring * 5;
              const mid = (CORE_RADIUS + len - 18) / 2;
              const mx = layout.cx + ux * mid - uy * bend;
              const my = layout.cy + uy * mid + ux * bend;
              const lit = picked?.name === node.clone.name || hovered === node.clone.name;
              return (
                <path
                  key={node.clone.name}
                  d={`M ${sx} ${sy} Q ${mx} ${my} ${ex} ${ey}`}
                  fill="none"
                  className={cn(
                    "map-edge",
                    old && "map-edge-old",
                    lit && "map-edge-active",
                    dimmed(node.ring) && "map-dim",
                  )}
                />
              );
            })}
          </svg>
          {layout.rings
            .filter((ring) => ring.ring > 0)
            .map(({ ring, r, generations }) => {
              const active = !picked && generations.includes(generation ?? -1) && generation !== current;
              const label =
                ring === MAX_RING && generations.length > 1
                  ? `gen ${generations[generations.length - 1]}–${generations[0]}`
                  : `gen ${generations.join(", ")}`;
              return (
                <button
                  type="button"
                  key={ring}
                  aria-label={`Clones from older image, ${label}`}
                  aria-pressed={active}
                  className={cn(
                    "map-ring-label absolute -translate-x-1/2 -translate-y-1/2",
                    active && "map-node-selected",
                  )}
                  style={{ left: layout.cx, top: layout.cy - r }}
                  onClick={() => {
                    setSelection({ kind: "image", generation: generations[0] });
                    setPanel(null);
                  }}
                >
                  {label}
                  <span className="text-faint"> · {ring === 1 ? "previous image" : `${ring} images ago`}</span>
                </button>
              );
            })}
          {(current !== undefined || layout.nodes.length > 0) && (
            <button
              type="button"
              aria-label={`Golden image, generation ${layout.newest}`}
              aria-pressed={!picked && generation === current}
              className={cn(
                "map-image map-image-current absolute flex size-[144px] flex-col items-center justify-center rounded-full text-center",
                !picked && generation === current && "map-node-selected",
              )}
              style={{ left: layout.cx - 72, top: layout.cy - 72 }}
              onClick={() => {
                if (current !== undefined) setSelection({ kind: "image", generation: current });
                setPanel(null);
              }}
            >
              <span className="map-image-symbol flex size-10 items-center justify-center rounded-full">
                <Glyph mono className="size-8" />
              </span>
              <span className="mt-2 text-[13px] font-medium">Golden image</span>
              <span className="mt-1 font-mono text-[10px] text-muted-foreground">generation {layout.newest}</span>
            </button>
          )}
          {layout.nodes.map((node) => {
            const c = node.clone;
            const warning = !!(c.failed_steps.length || c.issue || c.operation);
            const pr = prs?.get(c.name)?.pr;
            const active = picked?.name === c.name;
            return (
              <button
                type="button"
                key={c.name}
                aria-label={`Clone ${c.name}`}
                aria-pressed={active}
                title={c.branch ?? c.name}
                onClick={() => {
                  setSelection({ kind: "clone", name: c.name });
                  setPanel(null);
                }}
                onPointerEnter={() => setHovered(c.name)}
                onPointerLeave={() => setHovered((h) => (h === c.name ? null : h))}
                onDoubleClick={() => c.mounted && void openIn(c.path, prefs.editor)}
                className={cn(
                  "map-clone absolute size-[34px]",
                  `map-clone-label-${node.side}`,
                  active && "map-node-selected",
                  node.ring > 0 && "map-clone-old",
                  dimmed(node.ring) && "map-dim",
                  !c.mounted && "map-clone-unmounted",
                  warning && "map-clone-warning",
                )}
                style={{ left: node.x - 17, top: node.y - 17 }}
              >
                <span className="map-clone-symbol flex size-[34px] items-center justify-center rounded-full">
                  <CloveGlyph angle={(node.angle * 180) / Math.PI + 90} />
                </span>
                <span className="map-leaf-label absolute flex w-[218px] flex-col">
                  <span className="map-clone-name max-w-full truncate rounded px-1.5 py-0.5 text-[12px] font-medium">
                    {c.name}
                  </span>
                  <span className="map-clone-state mt-0.5 flex h-4 items-center justify-center gap-2 text-[10.5px] text-muted-foreground">
                    {warning ? (
                      <span className="text-amber">
                        {c.operation ? `${c.operation}: conflicts` : c.issue || `${c.failed_steps.join(", ")} failed`}
                      </span>
                    ) : !c.mounted ? (
                      "unmounted"
                    ) : (
                      <>
                        {pr?.state === "merged" && <span className="text-sage">merged</span>}
                        {c.dirty ? (
                          <span className="text-amber">modified</span>
                        ) : !c.ahead && pr?.state !== "merged" ? (
                          "clean"
                        ) : null}
                        {!!c.ahead && (
                          <span className="flex items-center">
                            <ArrowUp className="size-2.5" />
                            {c.ahead}
                          </span>
                        )}
                        {!!c.behind && (
                          <span className="flex items-center">
                            <ArrowDown className="size-2.5" />
                            {c.behind}
                          </span>
                        )}
                        {pr?.state === "open" && <span>#{pr.number}</span>}
                      </>
                    )}
                  </span>
                </span>
              </button>
            );
          })}
          {layout.tray && (
            <div
              className="map-tray absolute rounded-2xl"
              style={{
                left: layout.tray.x,
                top: layout.tray.y,
                width: layout.tray.width,
                height: layout.tray.height,
              }}
            >
              <div className="px-4 pt-3 text-[11.5px] font-medium">Not clones yet</div>
              <div className="px-4 text-[10.5px] text-muted-foreground">Plain git worktrees, outside the image</div>
            </div>
          )}
          {layout.looseNodes.map(({ worktree: w, side, x, y }) => {
            const active = pickedWt?.path === w.path;
            return (
              <button
                type="button"
                key={w.path}
                aria-label={`Plain worktree ${w.name}`}
                aria-pressed={active}
                title={w.path}
                onClick={() => {
                  setSelection({ kind: "worktree", path: w.path });
                  setPanel(null);
                }}
                onDoubleClick={() => !w.missing && void openIn(w.path, prefs.editor)}
                className={cn(
                  "map-clone map-loose absolute size-[34px]",
                  `map-clone-label-${side}`,
                  active && "map-node-selected",
                  focusRing !== null && "map-dim",
                  w.dirty && "map-loose-dirty",
                )}
                style={{ left: x - 17, top: y - 17 }}
              >
                <span className="map-loose-symbol flex size-[34px] items-center justify-center rounded-full">
                  {w.locked ? <Lock className="size-3.5" /> : <GitBranch className="size-3.5" />}
                </span>
                <span className="map-leaf-label absolute flex w-[218px] flex-col">
                  <span className="map-clone-name max-w-full truncate rounded px-1.5 py-0.5 text-[12px] font-medium">
                    {w.name}
                  </span>
                  <span className="map-clone-state mt-0.5 flex h-4 items-center justify-center gap-2 text-[10.5px] text-muted-foreground">
                    {w.missing ? (
                      "folder missing"
                    ) : w.locked ? (
                      "locked"
                    ) : w.dirty ? (
                      <span className="text-amber">modified</span>
                    ) : (
                      "not a clone"
                    )}
                  </span>
                </span>
              </button>
            );
          })}
          {current === undefined && !layout.nodes.length && (
            <div className="flex h-full items-center justify-center text-[13px] text-muted-foreground">
              Build a golden image to start branching.
            </div>
          )}
        </div>
      </section>
      <div className="flex h-9 shrink-0 items-center gap-2 border-t px-4 text-[10.5px] text-muted-foreground">
        <span className="flex items-center gap-1.5">
          <span className="h-px w-4 bg-bottle/60" />
          from the current image
        </span>
        <span className="ml-2 flex items-center gap-1.5">
          <span className="w-4 border-t border-dashed border-faint" />
          from an older image, one ring per generation
        </span>
        {worktrees.length > 0 && (
          <span className="ml-2 flex items-center gap-1.5">
            <GitBranch className="size-3" />
            plain worktree, in the tray
          </span>
        )}
        <span className="ml-2 hidden text-faint min-[1300px]:inline">Drag to pan · pinch or ⌘/Ctrl scroll to zoom</span>
        <div className="flex-1" />
        <Button variant="ghost" size="icon-sm" className="size-6" aria-label="Zoom out" onClick={() => zoomBy(1 / 1.2)}>
          <Minus className="size-3" />
        </Button>
        <span className="w-9 text-center font-mono">{Math.round(camera.scale * 100)}%</span>
        <Button variant="ghost" size="icon-sm" className="size-6" aria-label="Zoom in" onClick={() => zoomBy(1.2)}>
          <Plus className="size-3" />
        </Button>
        <Button variant="ghost" size="icon-sm" className="size-6" aria-label="Fit map" onClick={fit}>
          <Scan className="size-3.5" />
        </Button>
      </div>
      <div
        className="map-inspector flex min-h-[90px] shrink-0 flex-wrap items-center gap-x-5 gap-y-2 border-t bg-card px-5 py-3"
        aria-live="polite"
      >
        {pickedWt ? (
          <>
            <div className="min-w-0 flex-1 basis-[230px]">
              <div className="flex min-w-0 items-center gap-2 text-[13px] font-medium">
                <GitBranch className="size-4 shrink-0 text-muted-foreground" />
                <span className="truncate">{pickedWt.name}</span>
                <span className="shrink-0 text-[11px] font-normal text-muted-foreground">plain git worktree</span>
              </div>
              <div className="mt-1.5 truncate font-mono text-[11px] text-muted-foreground">
                {pickedWt.branch ?? `detached at ${shortSha(pickedWt.head)}`} · {pickedWt.path}
              </div>
              <div className="mt-1 text-[11px] text-faint">
                {pickedWt.adopt_blocker ??
                  "Not from the golden image: making it a clone moves it onto copy-on-write storage, changes included."}
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
              <Button
                size="sm"
                className="h-7 text-[12px]"
                disabled={!!pickedWt.adopt_blocker}
                onClick={() => setUi({ adopt: { root: status.root, worktree: pickedWt } })}
              >
                <CopyPlus className="size-3.5" />
                Make it a clone…
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-[12px]"
                disabled={pickedWt.missing}
                onClick={() => void openIn(pickedWt.path, prefs.editor)}
              >
                <Code className="size-3.5" />
                {APP_LABELS[prefs.editor]}
              </Button>
              <WorktreeMenu root={status.root} worktree={pickedWt}>
                <Button variant="ghost" size="icon-sm" className="size-7" aria-label="Worktree actions">
                  <Ellipsis className="size-4" />
                </Button>
              </WorktreeMenu>
            </div>
          </>
        ) : (
          <>
            <div className="min-w-0 flex-1 basis-[230px]">
              <div className="flex min-w-0 items-center gap-2 text-[13px] font-medium">
                {picked ? (
                  <CloveGlyph className="h-4 w-3 shrink-0 text-bottle" />
                ) : (
                  <Glyph className="size-4 shrink-0" />
                )}
                <span className="truncate">
                  {picked?.name ??
                    (currentImage
                      ? "Golden image"
                      : generation !== undefined
                        ? `Generation ${generation}`
                        : "No golden image")}
                </span>
                {selectedPr && (
                  <span className="shrink-0 text-[11px] font-normal text-muted-foreground">
                    #{selectedPr.number} · {selectedPr.state}
                  </span>
                )}
              </div>
              <div className="mt-1.5 truncate font-mono text-[11px] text-muted-foreground">
                {picked
                  ? `${picked.branch ?? `detached at ${shortSha(picked.head)}`} · gen ${picked.golden_generation}`
                  : currentImage && status.golden
                    ? `${status.golden.base} · ${shortSha(status.golden.commit)} · ${bytes(status.golden.image_bytes)}`
                    : plural(clones.filter((c) => c.golden_generation === generation).length, "clone") +
                      " created from this image"}
              </div>
              <div className="mt-1 text-[11px] text-faint">
                {picked
                  ? `Created ${since(picked.created_at)} · ${bytes(picked.own_bytes ?? picked.image_bytes)} ${picked.own_bytes === undefined ? "image size" : "of unique data"}`
                  : currentImage && status.golden
                    ? `Built ${ago(status.golden.age_secs)}${status.golden.behind ? ` · ${status.golden.behind} commits behind base` : status.golden.behind === null ? "" : " · up to date"}`
                    : generation === undefined
                      ? "Build an image once, then create ready worktrees from it."
                      : "The original image has been replaced; its clones remain independent."}
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
              {picked ? (
                <>
                  {picked.mounted ? (
                    <>
                      <Button
                        size="sm"
                        className="h-7 text-[12px]"
                        onClick={() => void openIn(picked.path, prefs.editor)}
                      >
                        <Code className="size-3.5" />
                        {APP_LABELS[prefs.editor]}
                      </Button>
                      <Button
                        variant="outline"
                        size="icon-sm"
                        className="size-7"
                        aria-label="Open terminal"
                        onClick={() => void openIn(picked.path, prefs.terminal)}
                      >
                        <SquareTerminal className="size-3.5" />
                      </Button>
                      <Button variant="outline" size="sm" className="h-7 text-[12px]" onClick={() => openDiff(picked)}>
                        <FileDiff className="size-3.5" />
                        Changes
                      </Button>
                    </>
                  ) : (
                    <Button
                      size="sm"
                      disabled={mount.isPending}
                      onClick={() => mount.mutate({ name: picked.name, mounted: false })}
                    >
                      Mount clone
                    </Button>
                  )}
                  <CloneMenu root={status.root} clone={picked}>
                    <Button variant="ghost" size="icon-sm" className="size-7" aria-label="Clone actions">
                      <Ellipsis className="size-4" />
                    </Button>
                  </CloneMenu>
                </>
              ) : currentImage ? (
                <>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-[12px]"
                    onClick={() => setPanel(panel === "image" ? null : "image")}
                  >
                    Image details
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-[12px]"
                    disabled={refreshing}
                    onClick={() => void startOp({ kind: "refresh", root: status.root })}
                  >
                    <RefreshCw className={cn("size-3.5", refreshing && "animate-spin")} />
                    {refreshing ? "Refreshing…" : "Refresh"}
                  </Button>
                </>
              ) : !status.golden && generation === undefined ? (
                <Button
                  size="sm"
                  disabled={refreshing}
                  onClick={() => void startOp({ kind: "init", root: status.root })}
                >
                  Build golden image
                </Button>
              ) : null}
            </div>
          </>
        )}
      </div>
      {panel && (
        <div className="map-details absolute inset-x-4 bottom-[106px] z-10 max-h-[70%] overflow-y-auto rounded-xl border bg-background p-3 shadow-xl">
          <div className="mb-2 flex items-center justify-between px-1 text-[12px] font-medium">
            <span>{panel === "image" ? "Image details" : "Other worktrees"}</span>
            <Button
              variant="ghost"
              size="icon-sm"
              className="size-6"
              aria-label="Close details"
              onClick={() => setPanel(null)}
            >
              <X className="size-3.5" />
            </Button>
          </div>
          {panel === "image" ? (
            <GoldenCard status={status} />
          ) : (
            <WorktreeList root={status.root} worktrees={worktrees} />
          )}
        </div>
      )}
    </section>
  );
}
