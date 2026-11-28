import { ChevronsDownUp, ChevronsUpDown, Minus, Plus, Scan } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { BulbArt } from "@/components/map/bulb";
import { type DrawerItem, MapDrawer } from "@/components/map/drawer";
import { ageOf, type LabelSpot } from "@/components/map/geometry";
import { ringsLayout } from "@/components/map/layout";
import { forest, forksOf, isFolded, lineageOf } from "@/components/map/lineage";
import {
  ClovePiece,
  cloneLabelSize,
  type MapCtx,
  type MenuTarget,
  type Selection,
  WorktreeShelf,
} from "@/components/map/nodes";
import { type Pull, usePull } from "@/components/map/pull";
import { CloneRadialMenu, ImageRadialMenu } from "@/components/map/radial-menu";
import { Scene } from "@/components/map/scene";
import { bulbSeed, type MapScene, type SceneNode, SceneView } from "@/components/map/svg-map";
import { Button } from "@/components/ui/button";
import { type Box, useMapCamera } from "@/hooks/use-map-camera";
import { startOp, useOps } from "@/lib/ops";
import { usePrefs } from "@/lib/prefs";
import { openIn, usePrCheck, usePrs } from "@/lib/queries";
import type { CloneInfo, OtherWorktree, RepoStatus } from "@/lib/types";
import { setBulbs, setUi, useUi } from "@/lib/ui";
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
  const onMap = useMemo(() => clones.filter((c) => !ripped.has(c.name)), [clones, ripped]);
  /** The menu a press just closed, so clicking its node again toggles it shut. */
  const justClosed = useRef<{ key: string; at: number } | null>(null);
  const clearSelection = useCallback(() => {
    setSelection(null);
  }, []);
  /** The open radial menu: what it is on. It follows that node's animated place. */
  const [radial, setRadial] = useState<{
    target: MenuTarget;
    keyboard: boolean;
    /** Animating out; the menu calls back once it is gone. */
    closing?: boolean;
  } | null>(null);
  /** The map's one clock: every clove, stem and label moves on it (map/scene.ts). */
  const scene: MapScene = useMemo(() => new Scene<SceneNode>(), []);
  useEffect(() => () => scene.stop(), [scene]);
  /** Close the menu with its animation (the menu removes itself when done). */
  const dismissRadial = useCallback(() => setRadial((r) => (r && !r.closing ? { ...r, closing: true } : r)), []);
  const prefs = usePrefs();
  const { openBulbs: allOpen, closedBulbs: allClosed } = useUi();
  const openList = allOpen[status.root];
  const closedList = allClosed[status.root];
  const openBulbs = useMemo(() => new Set(openList ?? []), [openList]);
  const closedBulbs = useMemo(() => new Set(closedList ?? []), [closedList]);
  const { data: prs } = usePrs(status.root);
  usePrCheck(status.root);
  const ops = useOps();
  const refreshing = ops.some(
    (o) => o.status === "running" && o.req.root === status.root && (o.req.kind === "refresh" || o.req.kind === "init"),
  );
  const refreshOp = ops.find(
    (o) => o.status === "running" && o.req.root === status.root && (o.req.kind === "refresh" || o.req.kind === "init"),
  );
  const refreshStep = refreshOp?.steps[refreshOp.steps.length - 1]?.name ?? null;
  // A refresh that just failed: said under the bulb for a few seconds.
  const failedOp = ops.find(
    (o) =>
      o.status === "error" &&
      o.req.root === status.root &&
      (o.req.kind === "refresh" || o.req.kind === "init") &&
      Date.now() - (o.finishedAt ?? 0) < 4000,
  );
  const [failShown, setFailShown] = useState<string | null>(null);
  const failedId = failedOp?.id;
  const failedAt = failedOp?.finishedAt ?? 0;
  useEffect(() => {
    if (!failedId) return;
    setFailShown(failedId);
    const t = window.setTimeout(() => setFailShown(null), Math.max(0, 4000 - (Date.now() - failedAt)));
    return () => window.clearTimeout(t);
  }, [failedId, failedAt]);
  const refreshFailed = !!failShown;
  const current = status.golden?.generation;
  const newest = current ?? Math.max(0, ...clones.map((c) => c.golden_generation));

  const picked = selection?.kind === "clone" ? clones.find((c) => c.name === selection.name) : undefined;
  const pickedWt = selection?.kind === "worktree" ? worktrees.find((w) => w.path === selection.path) : undefined;
  const generation = picked?.golden_generation ?? (selection?.kind === "image" ? selection.generation : current);

  const peelKey = ops
    .filter((o) => o.req.kind === "rewind" && o.req.root === status.root && o.status === "running")
    .map((o) => (o.req.kind === "rewind" ? o.req.name : ""))
    .join("\n");
  const peeling = useMemo(() => new Set(peelKey ? peelKey.split("\n") : []), [peelKey]);

  const ctx: MapCtx = {
    newest,
    current,
    prOf: (name) => prs?.get(name)?.pr,
    picked: picked?.name,
    pickedWorktree: pickedWt?.path,
    // Only a picked image looks picked on the map: at rest, nothing does.
    imageGeneration: selection?.kind === "image" ? selection.generation : undefined,
    // With an image picked, its clones stay lit and the rest dim.
    focusAge: !picked && selection?.kind === "image" ? ageOf(newest, selection.generation) : null,
    hovered,
    setHovered,
    select: (next) => setSelection(next),
    open: (path) => void openIn(path, prefs.editor),
    deepestAge: 0,
    repoName: status.name,
    base: status.golden?.base ?? status.base,
    behind: status.golden?.behind ?? null,
    refreshing,
    refreshStep,
    refreshFailed,
    pull: null,
    startPull: () => {},
    toggleBulb: (name) => setBulbs(status.root, [name], !layoutRef.current?.bulbs.get(name)?.open),
    peeling,
    menuFor: radial && !radial.closing ? (radial.target.kind === "clone" ? radial.target.name : "image") : undefined,
    // While closing, nothing is "open": a clone's label drops back as the ring fades.
    menu: (target, keyboard) => {
      const closed = justClosed.current;
      justClosed.current = null;
      if (closed && closed.key === menuKey(target) && performance.now() - closed.at < 600) return;
      setRadial({ target, keyboard });
    },
  };

  /** The last layout's label spots (relative to their cloves), so the next keeps them where it can. */
  const prevLabels = useRef(new Map<string, LabelSpot>());
  /** The current layout, for handlers built before it (the bulb toggle). */
  const layoutRef = useRef<ReturnType<typeof ringsLayout> | null>(null);
  const layout = useMemo(() => {
    const labelOf = (c: CloneInfo) => cloneLabelSize(c, (n) => prs?.get(n)?.pr);
    const next = ringsLayout(onMap, newest, labelOf, {
      open: openBulbs,
      closed: closedBulbs,
      prev: prevLabels.current,
    });
    prevLabels.current = new Map(
      next.nodes.map((n) => [n.clone.name, { ...n.label, x: n.label.x - n.x, y: n.label.y - n.y }]),
    );
    return next;
  }, [onMap, newest, prs, openBulbs, closedBulbs]);
  layoutRef.current = layout;
  ctx.deepestAge = Math.max(0, ...layout.layers.map((l) => l.age));

  const { viewport, camera, dragging, hand, fit, zoomBy, setDrawer, keepVisible, bindings } = useMapCamera(
    layout.width,
    layout.height,
    worktrees.length ? SHELF_INSET : 0,
  );

  // The details drawer: up over the map's bottom while something is picked.
  const drawerItem: DrawerItem | null = picked
    ? { kind: "clone", clone: picked }
    : pickedWt
      ? { kind: "worktree", worktree: pickedWt }
      : selection?.kind === "image" && status.golden
        ? selection.generation === current
          ? { kind: "image" }
          : { kind: "layer", generation: selection.generation }
        : null;
  const legendBar = useRef<HTMLDivElement>(null);
  const [mapHeight, setMapHeight] = useState(600);
  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setMapHeight(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, [viewport]);
  // At most ~45% of the map.
  const drawerMax = Math.round(mapHeight * 0.45);
  const selectionRef = useRef(selection);
  selectionRef.current = selection;
  const radialRef = useRef(radial);
  radialRef.current = radial;
  /**
   * What to keep in view, in viewport px: an open radial menu's whole extent
   * (ring, items, the label lifted over it), else the picked clove and its
   * label, or the bulb. A plain worktree lives on the shelf, always in view.
   */
  const keepBox = useCallback((): Box | undefined => {
    const vp = viewport.current;
    const sel = selectionRef.current;
    if (!vp) return;
    const menu = vp.closest(".repo-map")?.querySelector(".map-radial");
    const name = radialRef.current?.target.kind === "clone" ? radialRef.current.target.name : null;
    const els: (Element | null)[] = menu
      ? [...menu.querySelectorAll("*"), name ? vp.querySelector(`g.map-label[data-for="${CSS.escape(name)}"]`) : null]
      : sel?.kind === "clone"
        ? [
            vp.querySelector(`g.map-clone[aria-label="Clone ${CSS.escape(sel.name)}"]`),
            vp.querySelector(`g.map-label[data-for="${CSS.escape(sel.name)}"]`),
          ]
        : sel?.kind === "image"
          ? [vp.querySelector(".map-image")]
          : [];
    const rects = els
      .filter((e): e is Element => !!e)
      .map((e) => e.getBoundingClientRect())
      .filter((r) => r.width || r.height);
    if (!rects.length) return;
    const at = vp.getBoundingClientRect();
    return {
      left: Math.min(...rects.map((r) => r.left)) - at.left,
      top: Math.min(...rects.map((r) => r.top)) - at.top,
      right: Math.max(...rects.map((r) => r.right)) - at.left,
      bottom: Math.max(...rects.map((r) => r.bottom)) - at.top,
    };
  }, [viewport]);
  const onDrawerHeight = useCallback(
    (px: number) => {
      const covered = px;
      // The worktree shelf rides up with the drawer, so it stays in reach.
      viewport.current?.style.setProperty("--drawer-covered", `${covered}px`);
      setDrawer(covered, keepBox());
    },
    [setDrawer, viewport, keepBox],
  );
  // A menu opening near an edge (or over the drawer): glide so its whole ring
  // shows, once it has grown to full size.
  const radialKey =
    radial && !radial.closing
      ? `${radial.target.kind}:${radial.target.kind === "clone" ? radial.target.name : ""}`
      : null;
  useEffect(() => {
    if (!radialKey) return;
    const t = window.setTimeout(() => {
      const box = keepBox();
      if (box) keepVisible(box);
    }, 260);
    return () => window.clearTimeout(t);
  }, [radialKey, keepBox, keepVisible]);

  // Expand or fold every bulb at once: the same shared state as the list's
  // header toggle and its `*` key, so the two stay in step.
  const lineage = useMemo(() => forest(clones), [clones]);
  const forkParents = useMemo(() => clones.filter((c) => (lineage.kids.get(c.name) ?? []).length), [clones, lineage]);
  const allForksOpen = forkParents.every((c) => !isFolded(lineage, c.name, openBulbs, closedBulbs));
  /** The map's size before an expand/fold all, to glide-fit once it settles if that changed it a lot. */
  const sizeBeforeToggle = useRef<{ w: number; h: number; at: number } | null>(null);
  const setAllForks = (open: boolean) => {
    sizeBeforeToggle.current = { w: layout.width, h: layout.height, at: performance.now() };
    setBulbs(
      status.root,
      forkParents.map((c) => c.name),
      open,
    );
  };
  useEffect(() => {
    const before = sizeBeforeToggle.current;
    sizeBeforeToggle.current = null;
    // Only the size change that toggle caused, not a later one.
    if (!before || performance.now() - before.at > 1000) return;
    const change = Math.max(Math.abs(Math.log(layout.width / before.w)), Math.abs(Math.log(layout.height / before.h)));
    // A quarter bigger or smaller: refit, once the bulbs have mostly unfolded.
    if (change < Math.log(1.25)) return;
    const t = window.setTimeout(fit, 250);
    return () => window.clearTimeout(t);
  }, [layout.width, layout.height, fit]);

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
      // Unfold its lineage, so the new fork is seen landing.
      setBulbs(status.root, lineageOf(clones, from), true);
      setForking(true);
      void startOp({ kind: "fork", root: status.root, from })
        .catch((e) => {
          scene.cancelPending();
          toast.error(`Could not fork ${from}`, { description: String(e) });
        })
        .finally(() => setForking(false));
      return;
    }
    if (p.target.kind === "image") {
      // A new clone on a generated name and branch, from the base: no questions.
      setSprouting(true);
      void startOp({ kind: "new", root: status.root, branch: null, detach: false })
        .catch((e) => {
          scene.cancelPending();
          toast.error("Could not start a new clone", { description: String(e) });
        })
        .finally(() => setSprouting(false));
      return;
    }
    const name = p.target.name;
    const clone = clones.find((c) => c.name === name);
    if (!clone) return;
    // Its forks go with it, folded or not: the whole branch, deepest first.
    const f = forest(clones);
    const branch: { clone: CloneInfo; depth: number }[] = [];
    const walk = (c: CloneInfo, depth: number) => {
      branch.push({ clone: c, depth });
      for (const k of f.kids.get(c.name) ?? []) walk(k, depth + 1);
    };
    walk(clone, 0);
    const all = branch.map((b) => b.clone);
    // Uncommitted work would be lost: that still takes the dialog's say-so.
    // (It already springs home: the scene took it from where it was dropped.)
    if (all.some((c) => c.dirty))
      return setUi({ remove: { root: status.root, clones: [clone], forks: forksOf(clones, name) } });
    // They fly off together from where it was let go.
    scene.ripGroup(name);
    setRipped((r) => new Set([...r, ...all.map((c) => c.name)]));
    let undone = false;
    let done = false;
    const go = async () => {
      if (undone || done) return;
      done = true;
      for (const { clone: c } of [...branch].sort((a, b) => b.depth - a.depth)) {
        await startOp({ kind: "remove", root: status.root, name: c.name, delete_branch: true }).catch((e) => {
          unrip(c.name);
          toast.error(`Could not remove ${c.name}`, { description: String(e) });
        });
      }
    };
    const forks = all.length - 1;
    toast(forks ? `Removing ${name} and ${forks} fork${forks === 1 ? "" : "s"}` : `Removing ${name}`, {
      description: clone.branch ? `Branches go too, unless they have commits the base lacks.` : undefined,
      duration: 5000,
      action: {
        label: "Undo",
        onClick: () => {
          undone = true;
          for (const c of all) unrip(c.name);
        },
      },
      onAutoClose: () => void go(),
      onDismiss: () => void go(),
    });
  };
  const { pull, start, swallowClick, setPull } = usePull({
    scale: camera.scale,
    toWorld: (sx, sy) => {
      const box = viewport.current?.getBoundingClientRect();
      if (!box) return undefined;
      return { x: (sx - box.left - camera.x) / camera.scale, y: (sy - box.top - camera.y) / camera.scale };
    },
    onTear,
    onRelease: (p, short) => {
      // The scene takes over from where it was dropped, on the same springs:
      // a rip carries on (home, or off), a short copy shrinks back.
      if (p.target.kind === "clone" && !p.fork) {
        scene.absorb(p.target.name, p.dx, p.dy);
        scene.setPull(null);
      } else if (short) scene.setPull(null, true);
    },
  });
  useLayoutEffect(() => scene.setPull(pull), [pull, scene]);
  ctx.pull = pull;
  // Outward is a bigger radius from the image: rips tear and forks arm only that way.
  ctx.startPull = (target, event) => {
    const box = viewport.current?.getBoundingClientRect();
    if (target.kind !== "clone" || !box) return start(target, event);
    start(target, event, {
      x: box.left + camera.x + scene.core.x.v * camera.scale,
      y: box.top + camera.y + scene.core.y.v * camera.scale,
    });
  };
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

  // A new clone or fork that failed: its dropped copy shrinks back home.
  const createFailed = ops.some(
    (o) =>
      o.status === "error" &&
      o.req.root === status.root &&
      (o.req.kind === "new" || o.req.kind === "fork") &&
      Date.now() - (o.finishedAt ?? 0) < 1500,
  );
  useEffect(() => {
    if (createFailed) scene.cancelPending();
  }, [createFailed, scene]);

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
    ? Math.max(
        MENU_HOLE,
        (radial.target.kind === "image" ? 72 : 17) * camera.scale + (radial.target.kind === "image" ? 16 : 6),
      )
    : 0;
  // While its menu is open, a clone's own label rises to sit above the ring
  // (its outer edge plus a gap, in world units), on the scene's springs.
  const liftFor = radial && !radial.closing && radial.target.kind === "clone" ? radial.target.name : null;
  useLayoutEffect(() => {
    const n = liftFor ? layout.nodes.find((x) => x.clone.name === liftFor) : undefined;
    if (!liftFor || !n) return scene.setLift(null);
    const outer = (menuHole + 50 + 8) / camera.scale;
    const rel = { x: n.label.x - n.x, y: n.label.y - n.y };
    scene.setLift(liftFor, -(rel.x + n.label.w / 2), -outer - n.label.h - rel.y);
  }, [liftFor, layout, menuHole, camera.scale, scene]);
  // Keyboard focus shows only after keyboard navigation (Tab, arrows), never
  // after a click or a pull, whatever the engine thinks :focus-visible means.
  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Tab" || e.key.startsWith("Arrow")) el.setAttribute("data-kbd", "");
    };
    const onPointer = () => el.removeAttribute("data-kbd");
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onPointer, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onPointer, true);
    };
  }, [viewport]);
  // Escape closes an open menu (the menu handles that); with none open, it picks nothing.
  const radialOpen = useRef(false);
  radialOpen.current = !!radial;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented || radialOpen.current) return;
      const t = event.target as Element | null;
      if (t?.closest("input, textarea, [contenteditable], [role=dialog], [role=alertdialog]")) return;
      if (document.querySelector('[role="dialog"], [role="alertdialog"]')) return;
      clearSelection();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [clearSelection]);
  const closeRadial = (refocus: boolean) => {
    const target = radial?.target;
    dismissRadial();
    if (!refocus || !target) return;
    const label = target.kind === "clone" ? `Clone ${target.name}` : `Bulb (golden image), generation ${newest}`;
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
        onClickCapture={(event) => {
          // The click ending a pull, or a pan (the camera's), isn't a click.
          swallowClick(event);
          if (!event.isPropagationStopped()) bindings.onClickCapture(event);
        }}
        onClick={(event) => {
          // A click on empty canvas (not a pan or a pull: those swallow theirs) picks nothing.
          if ((event.target as Element).closest('button, [role="button"], [role="menu"], fieldset')) return;
          clearSelection();
        }}
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
        className="map-viewport relative min-h-0 flex-1 overflow-clip outline-none"
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
            <div className="flex h-full flex-col items-center justify-center gap-4 text-[13px] text-muted-foreground">
              {/* No image yet: the bulb's faint outline. */}
              <svg
                width="160"
                height="160"
                viewBox="-80 -80 160 160"
                className="map-scene map-bulb-empty"
                aria-hidden="true"
              >
                <BulbArt seed={bulbSeed(status.name)} />
              </svg>
              Build a golden image to start branching.
              <Button size="sm" disabled={refreshing} onClick={() => void startOp({ kind: "init", root: status.root })}>
                Build golden image
              </Button>
            </div>
          ) : (
            <SceneView layout={layout} ctx={ctx} scene={scene} />
          )}
        </div>
        <WorktreeShelf worktrees={worktrees} ctx={ctx} />
        {radial && (
          <Anchored scene={scene} target={radial.target} camera={camera}>
            {radialClone && (
              <CloneRadialMenu
                key={radialClone.name}
                clone={radialClone}
                root={status.root}
                bulb={layout.bulbs.get(radialClone.name)}
                lineage={lineageOf(clones, radialClone.name)}
                x={0}
                y={0}
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
                x={0}
                y={0}
                hole={menuHole}
                keyboard={radial.keyboard}
                onClose={closeRadial}
                closing={!!radial.closing}
                onClosed={() => setRadial(null)}
              />
            )}
          </Anchored>
        )}
      </section>
      {/* One panel at the map's foot: the legend row, with the details drawer growing up from it. */}
      <div className="relative shrink-0">
        <MapDrawer
          status={status}
          clones={clones}
          item={drawerItem}
          maxHeight={drawerMax}
          onHeight={onDrawerHeight}
          refreshing={refreshing}
        />
        <div
          ref={legendBar}
          className={cn(
            "flex h-9 shrink-0 items-center gap-2 bg-card px-4 text-[10.5px] text-muted-foreground",
            // One panel: the drawer's border tops it while it's up.
            !drawerItem && "border-t",
          )}
        >
          <LegendItem swatch={<Swatch skin="map-skin-peeled" />}>mounted</LegendItem>
          <LegendItem swatch={<Swatch skin="map-skin-unmounted" />}>unmounted</LegendItem>
          <LegendItem swatch={<Swatch skin="map-skin-peeled map-skin-modified" />}>modified</LegendItem>
          <LegendItem swatch={<Swatch skin="map-skin-bruised" />}>needs attention</LegendItem>
          <LegendItem swatch={<Swatch skin="map-skin-peeled map-skin-merged" />}>merged</LegendItem>
          <LegendItem
            swatch={
              // Just an older image's dashed stem, as wide as the stem-and-clove it replaced.
              <span className="flex h-4 w-6 items-center">
                <span className="w-full border-t border-dashed border-faint" />
              </span>
            }
          >
            older image
          </LegendItem>
          <span className="ml-2 hidden text-faint min-[1400px]:inline">
            Drag to pan · pinch or ⌘/Ctrl scroll to zoom
          </span>
          <div className="flex-1" />
          {forkParents.length > 0 && (
            <Button
              variant="ghost"
              size="sm"
              className="mr-1 h-6 gap-1.5 px-2 text-[11px] text-muted-foreground"
              onClick={() => setAllForks(!allForksOpen)}
            >
              {allForksOpen ? <ChevronsDownUp className="size-3.5" /> : <ChevronsUpDown className="size-3.5" />}
              {allForksOpen ? "Fold forks" : "Expand forks"}
            </Button>
          )}
          <Button
            variant="ghost"
            size="icon-sm"
            className="size-6"
            aria-label="Zoom out"
            onClick={() => zoomBy(1 / 1.2)}
          >
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
      </div>
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

/** Legend cloves lean like they sit on the map, all at one angle; the box keeps its size. */
const SWATCH_TILT = 32;
const SWATCH_LEN = 15;

function Swatch({ skin }: { skin: string }) {
  return (
    <span className={cn("map-piece relative flex h-4 w-3 items-center justify-center", skin)}>
      <ClovePiece length={SWATCH_LEN} angle={SWATCH_TILT} />
    </span>
  );
}

/** Screen-space overlay pinned to a node's animated place on the map (the scene's clock moves it). */
function Anchored({
  scene,
  target,
  camera,
  children,
}: {
  scene: MapScene;
  target: MenuTarget;
  camera: { x: number; y: number; scale: number };
  children: ReactNode;
}) {
  const el = useRef<HTMLDivElement>(null);
  const cam = useRef(camera);
  cam.current = camera;
  useLayoutEffect(
    () =>
      scene.onFrame(() => {
        const at = target.kind === "clone" ? scene.at(target.name) : { x: scene.core.x.v, y: scene.core.y.v };
        if (!at || !el.current) return;
        const { x, y, scale } = cam.current;
        el.current.style.transform = `translate(${x + at.x * scale}px, ${y + at.y * scale}px)`;
      }),
    [scene, target],
  );
  // The camera moves without the clock: follow it too.
  useLayoutEffect(() => {
    const at = target.kind === "clone" ? scene.at(target.name) : { x: scene.core.x.v, y: scene.core.y.v };
    if (at && el.current)
      el.current.style.transform = `translate(${camera.x + at.x * camera.scale}px, ${camera.y + at.y * camera.scale}px)`;
  });
  return (
    <div ref={el} className="absolute top-0 left-0 z-20">
      {children}
    </div>
  );
}
