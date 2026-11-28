import { Minus, Plus, Scan, X } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { GoldenCard } from "@/components/golden-card";
import { ageOf } from "@/components/map/geometry";
import { MapInspector } from "@/components/map/inspector";
import {
  ClovePiece,
  cloneLabelSize,
  type MapCtx,
  type MenuTarget,
  MergedBadge,
  type Selection,
  WorktreeShelf,
} from "@/components/map/nodes";
import { type Pull, usePull } from "@/components/map/pull";
import { CloneRadialMenu, ImageRadialMenu } from "@/components/map/radial-menu";
import { RingsLayer, ringsLayout } from "@/components/map/rings";
import { Button } from "@/components/ui/button";
import { useMapCamera } from "@/hooks/use-map-camera";
import { startOp, useOps } from "@/lib/ops";
import { usePrefs } from "@/lib/prefs";
import { openIn, usePrCheck, usePrs } from "@/lib/queries";
import type { CloneInfo, OtherWorktree, RepoStatus } from "@/lib/types";
import { setUi } from "@/lib/ui";
import { cn } from "@/lib/utils";

/** Room the worktree shelf takes at the bottom of the viewport. */
const SHELF_INSET = 44;
/** The smallest hole a radial menu leaves: a clove and its halo. */
const MENU_HOLE = 34;
const menuKey = (target: MenuTarget) => (target.kind === "clone" ? `clone:${target.name}` : "image");

/** An image-to-clone map. Placement shows provenance, not inferred Git ancestry. */
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
  const [panel, setPanel] = useState<"image" | null>(null);
  /**
   * Clones ripped off the map. They leave it at once (flying off from where
   * they were dropped) while the undo toast runs, and stay off until removed;
   * Undo, or a removal that fails, brings them back.
   */
  const [ripped, setRipped] = useState<ReadonlySet<string>>(new Set());
  const unrip = useCallback(
    (name: string) =>
      setRipped((r) => {
        if (!r.has(name)) return r;
        const next = new Set(r);
        next.delete(name);
        return next;
      }),
    [],
  );
  /** Where ripped cloves were let go, while they fly off from there. */
  const [flights, setFlights] = useState<ReadonlyMap<string, { dx: number; dy: number }>>(new Map());
  const dropFlight = useCallback(
    (name: string) =>
      setFlights((f) => {
        if (!f.has(name)) return f;
        const next = new Map(f);
        next.delete(name);
        return next;
      }),
    [],
  );
  const onMap = useMemo(() => clones.filter((c) => !ripped.has(c.name)), [clones, ripped]);
  /** The menu a press just closed, so clicking its node again toggles it shut. */
  const justClosed = useRef<{ key: string; at: number } | null>(null);
  /** The open radial menu: what it is on, anchored in world coordinates. */
  const [radial, setRadial] = useState<{
    target: MenuTarget;
    x: number;
    y: number;
    /** The node's radius in world units, so the ring follows the zoom. */
    radius: number;
    keyboard: boolean;
    /** The clone's name on the map when it opened (centre and height, world units): its label rises from there. */
    name?: { x: number; y: number; h: number };
    /** Animating out; the menu calls back once it is gone. */
    closing?: boolean;
  } | null>(null);
  /** Close the menu with its animation (the menu removes itself when done). */
  const dismissRadial = useCallback(() => setRadial((r) => (r && !r.closing ? { ...r, closing: true } : r)), []);
  const prefs = usePrefs();
  const { data: prs } = usePrs(status.root);
  usePrCheck(status.root);
  const ops = useOps();
  const refreshing = ops.some(
    (o) => o.status === "running" && o.req.root === status.root && (o.req.kind === "refresh" || o.req.kind === "init"),
  );
  const current = status.golden?.generation;
  const newest = current ?? Math.max(0, ...clones.map((c) => c.golden_generation));

  const picked = selection?.kind === "clone" ? clones.find((c) => c.name === selection.name) : undefined;
  const pickedWt = selection?.kind === "worktree" ? worktrees.find((w) => w.path === selection.path) : undefined;
  const generation = picked?.golden_generation ?? (selection?.kind === "image" ? selection.generation : current);

  const ctx: MapCtx = {
    newest,
    current,
    prOf: (name) => prs?.get(name)?.pr,
    picked: picked?.name,
    pickedWorktree: pickedWt?.path,
    imageGeneration: picked || pickedWt ? undefined : generation,
    // With an image picked, its clones stay lit and the rest dim.
    focusAge: !picked && selection?.kind === "image" ? ageOf(newest, selection.generation) : null,
    hovered,
    setHovered,
    select: (next) => {
      setSelection(next);
      setPanel(null);
    },
    open: (path) => void openIn(path, prefs.editor),
    // While closing, nothing is "open": a clone's label drops back as the ring fades.
    pull: null,
    returning: null,
    flights: new Map(),
    startPull: () => {},
    peeling: new Set(
      ops
        .filter((o) => o.req.kind === "rewind" && o.req.root === status.root && o.status === "running")
        .map((o) => (o.req.kind === "rewind" ? o.req.name : "")),
    ),
    menuFor: radial && !radial.closing ? (radial.target.kind === "clone" ? radial.target.name : "image") : undefined,
    menuLift: undefined,
    menu: (target, at, keyboard) => {
      const closed = justClosed.current;
      justClosed.current = null;
      if (closed && closed.key === menuKey(target) && performance.now() - closed.at < 600) return;
      const box = viewport.current?.getBoundingClientRect();
      if (!box) return;
      const { x, y, scale } = camera;
      const toWorld = (sx: number, sy: number) => ({ x: (sx - box.left - x) / scale, y: (sy - box.top - y) / scale });
      const t = at.title;
      setRadial({
        target,
        keyboard,
        radius: at.radius / scale,
        name: t && { ...toWorld(t.x + t.w / 2, t.y + t.h / 2), h: t.h / scale },
        ...toWorld(at.x, at.y),
      });
    },
  };

  const layout = useMemo(() => {
    const labelOf = (c: CloneInfo) => cloneLabelSize(c, (n) => prs?.get(n)?.pr);
    return ringsLayout(onMap, newest, labelOf);
  }, [onMap, newest, prs]);

  const { viewport, camera, dragging, hand, fit, zoomBy, bindings } = useMapCamera(
    layout.width,
    layout.height,
    worktrees.length ? SHELF_INSET : 0,
  );

  // Pull a clove off a clone to remove it, or out of the image for a new one.
  /** A clove pulled out of the image, until its new-clone op is under way. */
  const [sprouting, setSprouting] = useState(false);
  /** A fork's copy let go, until its fork op is under way. */
  const [forking, setForking] = useState(false);
  const onTear = (p: Pull) => {
    dismissRadial();
    if (p.fork && p.target.kind === "clone") {
      // Option-drag: the copy that tore free becomes a fork, on a generated name.
      const from = p.target.name;
      setForking(true);
      void startOp({ kind: "fork", root: status.root, from })
        .catch((e) => toast.error(`Could not fork ${from}`, { description: String(e) }))
        .finally(() => setForking(false));
      return;
    }
    if (p.target.kind === "image") {
      // A new clone on a generated name and branch, from the base: no questions.
      setSprouting(true);
      void startOp({ kind: "new", root: status.root, branch: null, detach: false })
        .catch((e) => toast.error("Could not start a new clone", { description: String(e) }))
        .finally(() => setSprouting(false));
      return;
    }
    const name = p.target.name;
    const clone = clones.find((c) => c.name === name);
    if (!clone) return;
    // Uncommitted work would be lost: that still takes the dialog's say-so.
    if (clone.dirty) {
      springBack(p);
      return setUi({ remove: { root: status.root, clones: [clone] } });
    }
    // It flies off from where it was let go.
    setFlights((f) => new Map(f).set(name, { dx: p.dx, dy: p.dy }));
    window.setTimeout(() => dropFlight(name), 1500);
    setRipped((r) => new Set(r).add(name));
    let undone = false;
    let done = false;
    const go = () => {
      if (undone || done) return;
      done = true;
      void startOp({ kind: "remove", root: status.root, name, delete_branch: true }).catch((e) => {
        unrip(name);
        toast.error(`Could not remove ${name}`, { description: String(e) });
      });
    };
    toast(`Removing ${name}`, {
      description: clone.branch ? `Its branch goes too, unless it has commits the base lacks.` : undefined,
      duration: 5000,
      action: {
        label: "Undo",
        onClick: () => {
          undone = true;
          dropFlight(name);
          unrip(name);
        },
      },
      onAutoClose: go,
      onDismiss: go,
    });
  };
  const { pull, returning, start, swallowClick, springBack, setPull } = usePull({
    scale: camera.scale,
    toWorld: (sx, sy) => {
      const box = viewport.current?.getBoundingClientRect();
      if (!box) return undefined;
      return { x: (sx - box.left - camera.x) / camera.scale, y: (sy - box.top - camera.y) / camera.scale };
    },
    onTear,
  });
  ctx.pull = pull;
  ctx.returning = returning;
  ctx.flights = flights;
  ctx.startPull = start;
  // A dropped clove waits for its dialog. Cancelled, it springs back; removed,
  // it flies off from where it was dropped; a new clone grows from the image.
  // A clove pulled out of the image waits for its new clone's op to start and
  // finish, then goes as the real clone grows out of the image.
  const heldImage = !!pull?.held && pull.target.kind === "image";
  const growing = ops.some((o) => o.status === "running" && o.req.root === status.root && o.req.kind === "new");
  useEffect(() => {
    if (heldImage && !sprouting && !growing) setPull(null);
  }, [heldImage, sprouting, growing, setPull]);
  // A fork's copy waits the same way, for its fork op.
  const heldFork = !!pull?.held && !!pull.fork;
  const forkRunning = ops.some((o) => o.status === "running" && o.req.root === status.root && o.req.kind === "fork");
  useEffect(() => {
    if (heldFork && !forking && !forkRunning) setPull(null);
  }, [heldFork, forking, forkRunning, setPull]);

  // A ripped clone stays off the map until it's gone; if its removal failed, it comes back.
  useEffect(() => {
    for (const name of ripped) {
      const gone = !clones.some((c) => c.name === name);
      const failed = ops.some(
        (o) => o.req.kind === "remove" && o.req.root === status.root && o.req.name === name && o.status === "error",
      );
      if (gone || failed) unrip(name);
    }
  }, [ripped, clones, ops, status.root, unrip]);

  const empty = current === undefined && !clones.length;
  const menuTarget = radial?.target;
  const radialClone = menuTarget?.kind === "clone" ? clones.find((c) => c.name === menuTarget.name) : undefined;
  // A clone's menu goes when the clone does or something else is picked; the
  // image's when a clone or worktree is picked.
  useEffect(() => {
    if (!radial) return;
    // Its clone is gone: nothing to glide back to, close at once.
    if (radial.target.kind === "clone" && !radialClone) return setRadial(null);
    const stale =
      radial.target.kind === "clone"
        ? picked?.name !== radial.target.name
        : !!picked || !!pickedWt || generation !== current;
    if (stale) dismissRadial();
  }, [radial, radialClone, picked, pickedWt, generation, current, dismissRadial]);
  // The ring's hole tracks the zoom: the node's radius on screen plus a gap
  // (more around the image), never smaller than a clove's ring.
  const menuHole = radial
    ? Math.max(MENU_HOLE, radial.radius * camera.scale + (radial.target.kind === "image" ? 16 : 6))
    : 0;
  // While its menu is open, a clone's own label rises to sit above the ring
  // (the ring's outer edge plus a gap, converted to world units): one element
  // throughout, moved by a CSS transition.
  if (radial?.name && !radial.closing) {
    const outer = menuHole + 50;
    ctx.menuLift = {
      dx: radial.x - radial.name.x,
      dy: radial.y - (outer + 8) / camera.scale - radial.name.h / 2 - radial.name.y,
    };
  }
  const closeRadial = (refocus: boolean) => {
    const target = radial?.target;
    dismissRadial();
    if (!refocus || !target) return;
    const label = target.kind === "clone" ? `Clone ${target.name}` : `Golden image, generation ${newest}`;
    viewport.current?.querySelector<HTMLElement>(`[aria-label="${CSS.escape(label)}"]`)?.focus();
  };

  return (
    <section className="repo-map flex min-h-0 flex-1 flex-col" aria-label="Repository map">
      <p id="map-navigation-help" className="sr-only">
        Placement shows image origin, not Git ancestry. Drag or scroll to pan. Pinch or hold Command or Control while
        scrolling to zoom. Focus the map and use arrow keys to pan, plus and minus to zoom, or zero to fit. Hold Space
        to drag over a node. Click a clone or the golden image, or press period on it, for its actions.
      </p>
      <section
        ref={viewport}
        {...bindings}
        onClickCapture={swallowClick}
        onPointerDown={(event) => {
          // Any press outside the ring closes it; remember which, so the click
          // that follows on the same node closes rather than reopens.
          if (radial) {
            justClosed.current = { key: menuKey(radial.target), at: event.timeStamp };
            dismissRadial();
          }
          bindings.onPointerDown(event);
        }}
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
          {empty ? (
            <div className="flex h-full items-center justify-center text-[13px] text-muted-foreground">
              Build a golden image to start branching.
            </div>
          ) : (
            <RingsLayer layout={layout} ctx={ctx} />
          )}
        </div>
        <WorktreeShelf worktrees={worktrees} ctx={ctx} />
        {radial && (
          <>
            {radialClone && (
              <CloneRadialMenu
                key={radialClone.name}
                clone={radialClone}
                root={status.root}
                x={camera.x + radial.x * camera.scale}
                y={camera.y + radial.y * camera.scale}
                hole={menuHole}
                keyboard={radial.keyboard}
                onClose={closeRadial}
                closing={!!radial.closing}
                onClosed={() => setRadial(null)}
              />
            )}
            {radial.target.kind === "image" && (
              <ImageRadialMenu
                status={status}
                refreshing={refreshing}
                showDetails={() => setPanel("image")}
                x={camera.x + radial.x * camera.scale}
                y={camera.y + radial.y * camera.scale}
                hole={menuHole}
                keyboard={radial.keyboard}
                onClose={closeRadial}
                closing={!!radial.closing}
                onClosed={() => setRadial(null)}
              />
            )}
          </>
        )}
      </section>
      <div className="flex h-9 shrink-0 items-center gap-2 border-t px-4 text-[10.5px] text-muted-foreground">
        <LegendItem swatch={<Swatch skin="map-skin-peeled" />}>mounted</LegendItem>
        <LegendItem swatch={<Swatch skin="map-skin-unmounted" />}>unmounted</LegendItem>
        <LegendItem swatch={<Swatch skin="map-skin-peeled map-skin-modified" cut />}>modified</LegendItem>
        <LegendItem swatch={<Swatch skin="map-skin-bruised" />}>needs attention</LegendItem>
        <LegendItem swatch={<Swatch skin="map-skin-peeled map-skin-merged" merged />}>merged</LegendItem>
        <LegendItem swatch={<span className="w-4 border-t border-dashed border-faint" />}>
          older image, one ring in
        </LegendItem>
        <span className="ml-2 hidden text-faint min-[1400px]:inline">Drag to pan · pinch or ⌘/Ctrl scroll to zoom</span>
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
      <MapInspector
        status={status}
        clones={clones}
        picked={picked}
        pickedWt={pickedWt}
        generation={generation}
        panel={panel}
        setPanel={setPanel}
        refreshing={refreshing}
      />
      {panel && (
        <div className="map-details absolute inset-x-4 bottom-[106px] z-10 max-h-[70%] overflow-y-auto rounded-xl border bg-background p-3 shadow-xl">
          <div className="mb-2 flex items-center justify-between px-1 text-[12px] font-medium">
            <span>Image details</span>
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
          <GoldenCard status={status} />
        </div>
      )}
    </section>
  );
}

function LegendItem({ swatch, children }: { swatch: ReactNode; children: ReactNode }) {
  return (
    <span className="mr-2 flex items-center gap-1.5 whitespace-nowrap">
      {swatch}
      {children}
    </span>
  );
}

function Swatch({ skin, cut, merged }: { skin: string; cut?: boolean; merged?: boolean }) {
  return (
    <span className={cn("map-piece relative flex h-4 w-3 items-center justify-center", merged && "mr-1.5", skin)}>
      <ClovePiece length={15} angle={0} cut={cut} />
      {merged && <MergedBadge className="-top-0.5 -right-1.5 size-2.5 [&_svg]:size-2" />}
    </span>
  );
}
