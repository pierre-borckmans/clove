import type { CloneInfo } from "@/lib/types";
import { ageOf, type Box, bounds, centerBox, type LabelSpot, nearestOn, overlaps, spotToward } from "./geometry";
import { isFolded as folded, forest } from "./lineage";

/** How far apart clones sit along the ring. */
const SPACING = 46;
/** The base ring's radius (short axis), before it grows to fit. */
const BASE_R = 164;
/** Orbits are ellipses, wider than tall, like the pane they sit in. */
export const STRETCH = 1.6;
/** Where the core's rim is, for edges to start at. */
export const CORE_RADIUS = 78;
/** The repo's name and image, two lines centred under the bulb: their middle, and their box. */
export const REPO_LABEL_Y = 100;
export const REPO_LABEL_BOX = { w: 180, h: 32 };
const PAD = 30;
/** How far out each generation of forks grows past the clone it came off. */
const FORK_STEP = 62;

/** Where a label may go: out along the clove's ray, sliding a little along the ring before moving further out. */
function outwardSpots(x: number, y: number, ux: number, uy: number, w: number, h: number) {
  const spots: LabelSpot[] = [];
  for (let step = 0; step < 12; step++) {
    const d = 22 + step * 12;
    for (const slide of [0, 12, -12, 24, -24]) {
      spots.push(spotToward(x + ux * d - uy * slide, y + uy * d + ux * slide, ux, uy, w, h));
    }
  }
  return spots;
}

type Placed = {
  clone: CloneInfo;
  angle: number;
  /** How many images old its golden image is (0: the current one), up to MAX_AGE. */
  ring: number;
  /** The clone it was forked from, when that is drawn too. */
  parent: string | null;
  /** Forks of forks out from its root clone. */
  depth: number;
  x: number;
  y: number;
  /** Forks folded into this clove's bulb (0: none). */
  folded: number;
};

/**
 * The map's layout. Radius means lineage only: every clone that isn't a fork
 * sits on one base ring, in a wedge of the circle sized by its shown forks,
 * and its forks grow outward from it, one FORK_STEP per generation of forks,
 * fanned across the wedge so stems never cross. Big lineages fold into their
 * parent clove (a bulb) unless `open`. A clone's image generation shows as
 * the bulb layer its stem roots in, and its skin, not its radius.
 */
export function ringsLayout(
  clones: CloneInfo[],
  newest: number,
  labelOf: (c: CloneInfo) => { w: number; h: number },
  {
    open = new Set<string>(),
    closed = new Set<string>(),
    prev,
  }: {
    /** Lineages opened or closed by hand (see ui.setBulbs). */
    open?: ReadonlySet<string>;
    closed?: ReadonlySet<string>;
    /** Last layout's label spots, relative to their cloves: kept when still free, so labels don't hop. */
    prev?: ReadonlyMap<string, LabelSpot>;
  } = {},
) {
  const ageOfClone = (c: CloneInfo) => ageOf(newest, c.golden_generation);
  const f = forest(clones);
  const isFolded = (c: CloneInfo) => folded(f, c.name, open, closed);
  const shown = (c: CloneInfo) => (isFolded(c) ? [] : (f.kids.get(c.name) ?? []));
  const leaves = (c: CloneInfo): number => {
    const k = shown(c);
    return k.length ? k.reduce((n, x) => n + leaves(x), 0) : 1;
  };
  // Lineage roots on the base ring, grouped by image age, newest first.
  const roots = clones.filter(f.isRoot).sort((a, b) => ageOfClone(a) - ageOfClone(b) || b.created_at - a.created_at);

  // Each root needs SPACING px along the ring (on the short axis, the tighter
  // one), or the room its shown forks take further out. When the circle can't
  // hold that, the ring grows.
  let r = BASE_R;
  const need = (c: CloneInfo, depth: number): number =>
    Math.max(
      SPACING / (r + depth * FORK_STEP),
      shown(c).reduce((n, k) => n + need(k, depth + 1), 0),
    );
  const needed = () => roots.reduce((n, c) => n + need(c, 0), 0);
  for (let i = 0; i < 6 && needed() > Math.PI * 2; i++) r *= (needed() / (Math.PI * 2)) * 1.02;
  const spare = Math.max(0, Math.PI * 2 - needed());
  const cx = 0;
  const cy = 0;
  const at = (angle: number, out: number) => ({
    x: cx + Math.cos(angle) * (r * STRETCH + out),
    y: cy + Math.sin(angle) * (r + out),
  });
  const placed: Placed[] = [];
  /** Clones folded away, and the clove whose bulb holds them. */
  const hiddenInto = new Map<string, string>();
  const hide = (c: CloneInfo, into: string) => {
    for (const k of f.kids.get(c.name) ?? []) {
      hiddenInto.set(k.name, into);
      hide(k, into);
    }
  };
  // Each root owns a wedge sized by its shown leaves and sits mid-wedge; its
  // forks fan out across the wedge, one FORK_STEP further out per generation.
  const totalLeaves = roots.reduce((n, c) => n + leaves(c), 0) || 1;
  const place = (c: CloneInfo, a0: number, w: number, depth: number, parent: string | null) => {
    const angle = a0 + w / 2;
    const folded = isFolded(c) ? f.descendants(c.name) : 0;
    if (folded) hide(c, c.name);
    placed.push({ clone: c, angle, ring: ageOfClone(c), parent, depth, folded, ...at(angle, depth * FORK_STEP) });
    const kids = shown(c);
    if (!kids.length) return;
    const needs = kids.map((k) => need(k, depth + 1));
    const total = needs.reduce((n, x) => n + x, 0);
    // Forks keep near their parent: a wide wedge isn't all theirs.
    const span = Math.min(w, total * 1.6);
    const extra = span - total;
    const kidLeaves = kids.reduce((n, k) => n + leaves(k), 0);
    let cursor = angle - span / 2;
    kids.forEach((k, i) => {
      const kw = needs[i] + (extra * leaves(k)) / kidLeaves;
      place(k, cursor, kw, depth + 1, c.name);
      cursor += kw;
    });
  };
  let cursor = -Math.PI / 2;
  for (const c of roots) {
    const w = need(c, 0) + (spare * leaves(c)) / totalLeaves;
    place(c, cursor, w, 0, null);
    cursor += w;
  }
  /** Lineages that fold into a bulb, open or not. */
  const bulbs = new Map<string, { forks: number; open: boolean }>();
  for (const c of clones)
    if (!hiddenInto.has(c.name) && f.descendants(c.name) > 0)
      bulbs.set(c.name, { forks: f.descendants(c.name), open: !isFolded(c) });
  /** The image's generations, as layers in the bulb: the current one (the rim) and each older one in use. */
  const layers = [...new Set([0, ...clones.map(ageOfClone)])]
    .sort((a, b) => a - b)
    .map((age) => {
      const gens = [...new Set(clones.filter((c) => ageOfClone(c) === age).map((c) => c.golden_generation))];
      if (!gens.length) gens.push(newest - age);
      return { age, generations: gens.sort((a, b) => b - a) };
    });
  const rings = [{ ring: 0, r }];
  const outer = r;

  // Every label hugs its own clove: outward first, then either side along
  // the ring, then inward, clear of other cloves, labels, edges and stems.
  // A clove with forks showing keeps its name on the side away from them.
  // Only when nothing near is free does a label move out, on a short dotted
  // leader that crosses no stem or clove.
  const byName = new Map(placed.map((n) => [n.clone.name, n]));
  const lines: Box[] = placed.flatMap((n) => {
    const p = n.parent ? byName.get(n.parent) : undefined;
    const g = p ? stemGeom(p.x, p.y, n.x, n.y) : edgeGeom(cx, cy, n.x, n.y, n.ring);
    return g ? quadBoxes(g) : [];
  });
  const cloves = placed.map((n) => centerBox(n.x, n.y, n.folded ? 50 : 34));
  const obstacles: Box[] = [
    centerBox(cx, cy, 150),
    // The repo's name and image, under the bulb.
    centerBox(cx, cy + REPO_LABEL_Y, REPO_LABEL_BOX.w, REPO_LABEL_BOX.h),
    // A bulb's huddle of cloves and its count sit beside the clove.
    ...cloves,
    ...lines,
    // A lineage's fold badge sits beside its clove, across the ray.
    ...placed
      .filter((n) => bulbs.has(n.clone.name))
      .map((n) => centerBox(n.x - Math.sin(n.angle) * 24, n.y + Math.cos(n.angle) * 24, 26, 18)),
  ];
  // Covering a clove is far worse than crossing a line.
  const cost = (b: Box, own: Box) =>
    cloves.reduce((n, o) => n + (o !== own && overlaps(b, o) ? 20 : 0), 0) +
    obstacles.reduce((n, o) => n + (overlaps(b, o) ? 1 : 0), 0);
  const nodes = placed.map((n) => {
    const len = Math.hypot(n.x - cx, n.y - cy) || 1;
    const ux = (n.x - cx) / len;
    const uy = (n.y - cy) / len;
    const size = labelOf(n.clone);
    // Outward, then round both sides to inward; away from its forks first.
    const turn = (a: number): [number, number] => [
      ux * Math.cos(a) - uy * Math.sin(a),
      ux * Math.sin(a) + uy * Math.cos(a),
    ];
    let dirs: [number, number][] = [0, 0.8, -0.8, Math.PI / 2, -Math.PI / 2, 2.3, -2.3, Math.PI].map(turn);
    const kids = placed.filter((k) => k.parent === n.clone.name);
    if (kids.length) {
      const kx = kids.reduce((a, k) => a + (k.x - n.x) / (Math.hypot(k.x - n.x, k.y - n.y) || 1), 0);
      const ky = kids.reduce((a, k) => a + (k.y - n.y) / (Math.hypot(k.x - n.x, k.y - n.y) || 1), 0);
      dirs = dirs
        .map((d, i) => ({ d, i, toward: d[0] * kx + d[1] * ky }))
        .sort((a, b) => a.toward - b.toward || a.i - b.i)
        .filter((x) => x.toward < kids.length * 0.6)
        .map((x) => x.d);
    }
    const own = cloves[placed.indexOf(n)];
    const near = dirs.flatMap(([dx, dy]) =>
      [21, 28, 36].flatMap((d) =>
        [0, 7, -7, 14, -14, 21, -21].map((slide) =>
          spotToward(n.x + dx * d - dy * slide, n.y + dy * d + dx * slide, dx, dy, size.w, size.h),
        ),
      ),
    );
    // Its own clove doesn't count against it: the spots already sit clear of it.
    const others = obstacles.filter((o) => o !== own);
    const free = (b: Box) => !others.some((o) => overlaps(b, o));
    // Where it was last time, if that's still clear: labels stay put when they can.
    const before = prev?.get(n.clone.name);
    const kept = before && { ...before, x: n.x + before.x, y: n.y + before.y, w: size.w, h: size.h };
    let label = kept && free(kept) && near.some((b) => b.align === kept.align) ? kept : near.find(free);
    let leader: readonly [number, number, number, number] | null = null;
    if (!label) {
      // Last resort: out along the ray, at most 70px, on a leader that crosses nothing.
      for (const spot of outwardSpots(n.x, n.y, ux, uy, size.w, size.h).filter((b) => {
        const e = nearestOn(b, n.x, n.y);
        return Math.hypot(e.x - n.x, e.y - n.y) <= 70;
      })) {
        if (!free(spot)) continue;
        const e = nearestOn(spot, n.x, n.y);
        const sx = n.x + ux * 17;
        const sy = n.y + uy * 17;
        const steps = Math.max(2, Math.ceil(Math.hypot(e.x - sx, e.y - sy) / 6));
        const crosses = Array.from({ length: steps + 1 }, (_, k) =>
          centerBox(sx + ((e.x - sx) * k) / steps, sy + ((e.y - sy) * k) / steps, 3),
        ).some((b) => [...lines, ...cloves].some((o) => o !== own && overlaps(b, o, 0)));
        if (crosses) continue;
        label = spot;
        leader = [sx, sy, e.x, e.y];
        break;
      }
    }
    // Nothing clean: the near spot with the fewest overlaps, still beside its clove.
    label ??= near.reduce((best, b) => (cost(b, own) < cost(best, own) ? b : best), near[0]);
    obstacles.push(label);
    return { ...n, label, leader };
  });

  // The world hugs the rings and their labels, so the map stays centred.
  const box = bounds([...obstacles, centerBox(cx, cy, outer * STRETCH * 2, outer * 2)]);
  const ox = PAD - box.x0;
  const oy = PAD - box.y0;
  const shift = <T extends { x: number; y: number }>(p: T): T => ({ ...p, x: p.x + ox, y: p.y + oy });
  return {
    kind: "rings" as const,
    nodes: nodes.map((n) => ({
      ...shift(n),
      label: shift(n.label),
      leader: n.leader && ([n.leader[0] + ox, n.leader[1] + oy, n.leader[2] + ox, n.leader[3] + oy] as const),
    })),
    rings,
    layers,
    hiddenInto,
    bulbs,
    width: box.x1 - box.x0 + PAD * 2,
    height: box.y1 - box.y0 + PAD * 2,
    cx: cx + ox,
    cy: cy + oy,
  };
}

export type RingsLayout = ReturnType<typeof ringsLayout>;
export type RingNode = RingsLayout["nodes"][number];

/** A fork's stem from its parent clove: the image edges' gentle sweep, trimmed clear of both cloves. */
export function stemGeom(px: number, py: number, x: number, y: number): Quad {
  const dx = x - px;
  const dy = y - py;
  const len = Math.hypot(dx, dy) || 1;
  const ux = dx / len;
  const uy = dy / len;
  const trim = Math.min(18, len / 3);
  const bend = Math.min(10, len / 7);
  const mx = (px + x) / 2 - uy * bend;
  const my = (py + y) / 2 + ux * bend;
  return [px + ux * trim, py + uy * trim, mx, my, x - ux * trim, y - uy * trim];
}
export const stemPath = (px: number, py: number, x: number, y: number) => quadPath(stemGeom(px, py, x, y));

/** A clone's edge from the image's rim, with a slight sweep so edges read as growth rather than spokes. */
export function edgeGeom(cx: number, cy: number, x: number, y: number, ring: number): Quad | null {
  const dx = x - cx;
  const dy = y - cy;
  const len = Math.hypot(dx, dy);
  // Still inside the core: nothing to draw yet.
  if (len < CORE_RADIUS + 18) return null;
  const ux = dx / len;
  const uy = dy / len;
  const bend = 8 + ring * 5;
  const mid = (CORE_RADIUS + len - 18) / 2;
  return [
    cx + ux * CORE_RADIUS,
    cy + uy * CORE_RADIUS,
    cx + ux * mid - uy * bend,
    cy + uy * mid + ux * bend,
    x - ux * 18,
    y - uy * 18,
  ];
}

/** A quadratic curve: start, control, end. */
export type Quad = readonly [number, number, number, number, number, number];
export const quadPath = ([x0, y0, qx, qy, x1, y1]: Quad) => `M ${x0} ${y0} Q ${qx} ${qy} ${x1} ${y1}`;
/** Small boxes along a curve, every few pixels, so labels can steer clear of it. */
function quadBoxes([x0, y0, qx, qy, x1, y1]: Quad, every = 8): Box[] {
  const n = Math.max(2, Math.ceil(Math.hypot(x1 - x0, y1 - y0) / every));
  return Array.from({ length: n + 1 }, (_, i) => {
    const t = i / n;
    const a = (1 - t) ** 2;
    const b = 2 * (1 - t) * t;
    const c = t * t;
    return centerBox(a * x0 + b * qx + c * x1, a * y0 + b * qy + c * y1, 6);
  });
}
