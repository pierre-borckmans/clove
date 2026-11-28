import { type CSSProperties, useMemo } from "react";
import { Glyph } from "@/components/glyph";
import type { CloneInfo } from "@/lib/types";
import { cn } from "@/lib/utils";
import {
  ageOf,
  type Box,
  bounds,
  centerBox,
  cloveLength,
  firstFree,
  type LabelSpot,
  MAX_AGE,
  nearestOn,
  spotToward,
} from "./geometry";
import {
  CheckpointSkins,
  CloneNode,
  ClovePiece,
  forkPullOn,
  isDimmed,
  type MapCtx,
  MergedBadge,
  menuTrigger,
  offsetOf,
  pullOn,
  skinClasses,
} from "./nodes";
import type { Pull } from "./pull";
import { useMotion } from "./use-motion";

const RING_GAP = 46;
/** The smallest ring, and how far apart clones sit along any ring. */
const MIN_INNER = 118;
const SPACING = 46;
/** Orbits are ellipses, wider than tall, like the pane they sit in. */
const STRETCH = 1.6;
/** Where the core's rim is, for edges to start at. */
const CORE_RADIUS = 78;
const PAD = 30;
/** How far a fork sits outside its parent's ring, per level (two at most). */
const FORK_OUT = 24;

/**
 * A ring's clones, newest first, with each fork right after the clone it was
 * forked from (oldest fork first), so lineages sit side by side on the ring.
 * A fork whose parent isn't in the group is a root of its own.
 */
function lineageOrder(group: CloneInfo[]) {
  const names = new Set(group.map((c) => c.name));
  const kids = new Map<string, CloneInfo[]>();
  const roots: CloneInfo[] = [];
  for (const c of group) {
    const p = c.forked_from;
    if (p && p !== c.name && names.has(p)) kids.set(p, [...(kids.get(p) ?? []), c]);
    else roots.push(c);
  }
  const out: { clone: CloneInfo; depth: number }[] = [];
  const seen = new Set<string>();
  const visit = (c: CloneInfo, depth: number) => {
    if (seen.has(c.name)) return;
    seen.add(c.name);
    out.push({ clone: c, depth });
    for (const k of (kids.get(c.name) ?? []).sort((a, b) => a.created_at - b.created_at)) visit(k, depth + 1);
  };
  for (const r of roots.sort((a, b) => b.created_at - a.created_at)) visit(r, 0);
  // A loop of forks (can't happen, but names get reused): still placed.
  for (const c of group) visit(c, 0);
  return out;
}

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

/** Rings mean age: clones of the current image sit on the outer ring, each older generation one ring in. */
export function ringsLayout(clones: CloneInfo[], newest: number, labelOf: (c: CloneInfo) => { w: number; h: number }) {
  const ringOf = (generation: number) => ageOf(newest, generation);
  const ringCount = clones.length ? Math.max(...clones.map((c) => ringOf(c.golden_generation))) + 1 : 1;
  // Like tree rings, the newest growth is outermost: the current image's
  // clones take the outer ring, each older generation one ring in.
  const slotOf = (age: number) => ringCount - 1 - age;
  const groups = Array.from({ length: ringCount }, (_, age) =>
    lineageOrder(clones.filter((c) => ringOf(c.golden_generation) === age)),
  );
  const onMap = new Set(clones.map((c) => c.name));

  // Each generation's arc is the room its clones need at its ring's radius:
  // SPACING px apart along the ring (measured on the short axis, the tighter
  // one). When the circle can't hold that, every ring grows together.
  let scale = 1;
  const radiusOf = (age: number) => (MIN_INNER + slotOf(age) * RING_GAP) * scale;
  // Every ring has its label pill at the top: keep clones clear of it.
  const topGap = () => 2 * Math.asin(Math.min(1, 100 / (Math.min(...groups.map((_, a) => radiusOf(a))) * STRETCH)));
  const needed = () => groups.reduce((sum, g, age) => sum + (g.length * SPACING) / radiusOf(age), 0);
  for (let i = 0; i < 6 && needed() > Math.PI * 2 - topGap(); i++) {
    scale *= (needed() / (Math.PI * 2 - topGap())) * 1.02;
  }
  const gap = topGap();
  const spare = Math.max(0, Math.PI * 2 - gap - needed());
  const outer = radiusOf(0);
  const cx = 0;
  const cy = 0;
  // Generations follow each other clockwise from the top, newest first; spare
  // angle is shared out by clone count so no arc is left empty.
  // Forks hug their parent: spare angle goes only before the clones they came off.
  const roots = groups.reduce((n, g) => n + g.filter((x) => x.depth === 0).length, 0);
  let cursor = -Math.PI / 2 + gap / 2 - (roots ? spare / roots / 2 : 0);
  const placed = groups.flatMap((group, ring) => {
    const r = radiusOf(ring);
    const tight = SPACING / r;
    return group.map(({ clone, depth }) => {
      // A clone's share of the spare angle goes before it, so its forks follow it closely.
      if (depth === 0 && roots) cursor += spare / roots;
      const angle = cursor + tight / 2;
      cursor += tight;
      // A fork steps out past its parent, so its stem reads as a branch.
      const out = Math.min(depth, 2) * FORK_OUT;
      const parent = clone.forked_from && onMap.has(clone.forked_from) ? clone.forked_from : null;
      return {
        clone,
        angle,
        ring,
        parent,
        x: cx + Math.cos(angle) * (r * STRETCH + out),
        y: cy + Math.sin(angle) * (r + out),
      };
    });
  });
  const rings = Array.from({ length: ringCount }, (_, ring) => {
    const gens = [...new Set(placed.filter((n) => n.ring === ring).map((n) => n.clone.golden_generation))];
    // A ring with no clones still stands for one image: label it with that generation.
    if (!gens.length) gens.push(newest - ring);
    return { ring, r: radiusOf(ring), generations: gens.sort((a, b) => b - a) };
  });

  // Every label hangs off its clove along the same ray as its edge, so names
  // read outward all round; crowded ones slide or move out with a leader.
  const obstacles: Box[] = [
    centerBox(cx, cy, 150),
    ...placed.map((n) => centerBox(n.x, n.y, 34)),
    ...rings.map((r) => centerBox(cx, cy - r.r, 170, 20)),
  ];
  const nodes = placed.map((n) => {
    const dx = n.x - cx;
    const dy = n.y - cy;
    const len = Math.hypot(dx, dy) || 1;
    const ux = dx / len;
    const uy = dy / len;
    const size = labelOf(n.clone);
    const label = firstFree(outwardSpots(n.x, n.y, ux, uy, size.w, size.h), obstacles);
    obstacles.push(label);
    const end = nearestOn(label, n.x, n.y);
    const far = Math.hypot(end.x - n.x, end.y - n.y) > 30;
    return { ...n, label, leader: far ? ([n.x + ux * 17, n.y + uy * 17, end.x, end.y] as const) : null };
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
    width: box.x1 - box.x0 + PAD * 2,
    height: box.y1 - box.y0 + PAD * 2,
    cx: cx + ox,
    cy: cy + oy,
  };
}

type RingsLayout = ReturnType<typeof ringsLayout>;

/** Spin and sideways sway for a clove flying off, `q` from 0 (start) to 1 (gone). */
function leafFlight(node: { x: number; y: number; angle: number; clone: { name: string } }, q: number) {
  // Neighbours turn opposite ways, so a batch of removals doesn't spin in step.
  const turn = [...node.clone.name].reduce((n, ch) => n + ch.charCodeAt(0), 0) % 2 ? 1 : -1;
  const sway = Math.sin(q * Math.PI * 3) * 26 * (1 - q * 0.4);
  // Across the clove's ray, which points along its angle from the core.
  return {
    spin: turn * 480 * q,
    sway: { x: -Math.sin(node.angle) * sway * turn, y: Math.cos(node.angle) * sway * turn },
  };
}
type RingNode = RingsLayout["nodes"][number];

/** How far a removed clove flies outward, enough to leave the view at a fitted zoom. */
const FLY = 900;

/** Motion tracks for a layout: clones grow out of the core and fly off outward; rings grow from the centre. */
function tracks(layout: RingsLayout) {
  const { cx, cy } = layout;
  return {
    core: [{ id: "core", values: [cx, cy], origin: [cx, cy], data: null }],
    rings: layout.rings.map((ring) => ({ id: `ring-${ring.ring}`, values: [ring.r], origin: [0], data: ring })),
    nodes: layout.nodes.map((node) => {
      const leader = node.leader ?? [node.x, node.y, node.x, node.y];
      const values = [node.x, node.y, node.label.x, node.label.y, ...leader];
      const len = Math.hypot(node.x - cx, node.y - cy) || 1;
      const dx = ((node.x - cx) / len) * FLY;
      const dy = ((node.y - cy) / len) * FLY;
      return {
        id: node.clone.name,
        values,
        origin: [cx, cy, cx, cy, cx, cy, cx, cy],
        exit: values.map((v, i) => v + (i % 2 ? dy : dx)),
        data: node,
      };
    }),
  };
}

export function RingsLayer({ layout, ctx }: { layout: RingsLayout; ctx: MapCtx }) {
  const focusRing = ctx.focusAge;
  const target = useMemo(() => tracks(layout), [layout]);
  const [core] = useMotion(target.core);
  const rings = useMotion(target.rings);
  const nodes = useMotion<RingNode>(target.nodes);
  const [cx, cy] = core.values;
  // Where each clove is this frame, for forks' stems to their parents.
  const at = new Map(nodes.filter((n) => !n.leaving).map((n) => [n.id, n.values] as const));
  const forkPull = ctx.pull?.fork && ctx.pull.target.kind === "clone" ? ctx.pull : null;
  return (
    <>
      <svg className="pointer-events-none absolute inset-0 size-full overflow-visible" aria-hidden="true">
        {rings.map(({ id, values: [r], data: { ring }, presence }) => (
          <ellipse
            key={id}
            className={cn("map-orbit", ring > 0 && "map-orbit-old", focusRing === ring && "map-orbit-focus")}
            style={{ opacity: presence }}
            cx={cx}
            cy={cy}
            rx={r * STRETCH}
            ry={r}
          />
        ))}
        {nodes.map(({ id, values: [x0, y0], data: node, presence, leaving }) => {
          // A leaving clove lets go of its edge as it flies off, and a torn one has.
          const pull = pullOn(ctx, node.clone.name);
          if (leaving || pull?.torn) return null;
          // Pulled but holding: the edge stretches with it and thins.
          const x = x0 + (pull?.dx ?? 0);
          const y = y0 + (pull?.dy ?? 0);
          const dx = x - cx;
          const dy = y - cy;
          const len = Math.hypot(dx, dy);
          // Still inside the core: nothing to draw yet.
          if (len < CORE_RADIUS + 18) return null;
          const ux = dx / len;
          const uy = dy / len;
          // A slight sweep, so edges read as growth rather than spokes.
          const bend = 8 + node.ring * 5;
          const mid = (CORE_RADIUS + len - 18) / 2;
          const lit = ctx.picked === node.clone.name || ctx.hovered === node.clone.name;
          // A fork grows from its parent clove, not from the image.
          const from = node.parent ? at.get(node.parent) : undefined;
          const d = from
            ? stemPath(from[0], from[1], x, y)
            : `M ${cx + ux * CORE_RADIUS} ${cy + uy * CORE_RADIUS} Q ${cx + ux * mid - uy * bend} ${cy + uy * mid + ux * bend} ${x - ux * 18} ${y - uy * 18}`;
          return (
            <path
              key={id}
              d={d}
              fill="none"
              style={{ "--appear": presence, "--strain": pull?.strain ?? 0 } as CSSProperties}
              className={cn(
                "map-edge",
                from && "map-stem",
                pull && "map-edge-strained",
                node.ring > 0 && "map-edge-old",
                lit && "map-edge-active",
                isDimmed(ctx, node.clone.golden_generation, node.clone.name) && "map-dim",
              )}
            />
          );
        })}
        {nodes.map(
          ({ id, values, data: node, presence, leaving }) =>
            node.leader &&
            !leaving &&
            !pullOn(ctx, node.clone.name) && (
              <line
                key={`leader-${id}`}
                x1={values[4]}
                y1={values[5]}
                x2={values[6]}
                y2={values[7]}
                style={{ "--appear": presence } as CSSProperties}
                className={cn("map-leader", isDimmed(ctx, node.clone.golden_generation, node.clone.name) && "map-dim")}
              />
            ),
        )}
      </svg>
      {rings.map(({ id, values: [r], data: { ring, generations }, presence, leaving }) => {
        // Ring 0 is the current image's own ring.
        const active =
          ring === 0
            ? ctx.imageGeneration === ctx.current
            : generations.includes(ctx.imageGeneration ?? -1) && ctx.imageGeneration !== ctx.current;
        const label =
          ring === MAX_AGE && generations.length > 1
            ? `gen ${generations[generations.length - 1]}–${generations[0]}`
            : `gen ${generations.join(", ")}`;
        return (
          <button
            type="button"
            key={id}
            inert={leaving}
            aria-label={ring === 0 ? `Clones from the current image, ${label}` : `Clones from older image, ${label}`}
            aria-pressed={active}
            className={cn("map-ring-label absolute -translate-x-1/2 -translate-y-1/2", active && "map-node-selected")}
            style={{ left: cx, top: cy - r, opacity: presence }}
            onClick={() =>
              ctx.select({ kind: "image", generation: ring === 0 ? (ctx.current ?? generations[0]) : generations[0] })
            }
          >
            {label}
            <span className="text-faint">
              {" "}
              · {ring === 0 ? "current image" : ring === 1 ? "previous image" : `${ring} images ago`}
            </span>
          </button>
        );
      })}
      <button
        type="button"
        aria-label={`Golden image, generation ${ctx.newest}`}
        aria-pressed={ctx.imageGeneration === ctx.current}
        className={cn(
          "map-image map-image-current absolute flex size-[144px] flex-col items-center justify-center rounded-full text-center",
          ctx.imageGeneration === ctx.current && "map-node-selected",
          ctx.pull?.target.kind === "image" && "map-image-pulled",
        )}
        style={{ left: cx - 72, top: cy - 72 }}
        onPointerDown={(event) => ctx.current !== undefined && ctx.startPull({ kind: "image" }, event)}
        {...menuTrigger(
          ctx,
          { kind: "image" },
          () => ctx.current !== undefined && ctx.select({ kind: "image", generation: ctx.current }),
        )}
      >
        <span className="map-image-symbol flex size-10 items-center justify-center rounded-full">
          <Glyph mono className="size-8" />
        </span>
        <span className="mt-2 text-[13px] font-medium">Golden image</span>
        <span className="mt-1 font-mono text-[10px] text-muted-foreground">generation {ctx.newest}</span>
      </button>
      <Sprout ctx={ctx} cx={cx} cy={cy} />
      {forkPull && forkPull.target.kind === "clone" && at.get(forkPull.target.name) && (
        <ForkCopy
          pull={forkPull}
          home={at.get(forkPull.target.name) as readonly number[]}
          angle={
            nodes.find((n) => n.id === (forkPull.target.kind === "clone" ? forkPull.target.name : ""))?.data.angle ?? 0
          }
        />
      )}
      {nodes.map(({ id, values: [px, py, plx, ply], data: node, presence, leaving }) => {
        const pr = ctx.prOf(node.clone.name);
        // Pulled: the clove, and its name with it, follow the pointer.
        const off = offsetOf(ctx, node.clone.name);
        const [x0, y0, lx, ly] = off ? [px + off.dx, py + off.dy, plx + off.dx, ply + off.dy] : [px, py, plx, ply];
        // A leaving clove tumbles off like a leaf: it spins and sways across
        // its path as it flies out.
        const flight = leaving ? leafFlight(node, 1 - presence) : null;
        const x = x0 + (flight?.sway.x ?? 0);
        const y = y0 + (flight?.sway.y ?? 0);
        return (
          <CloneNode
            key={id}
            clone={node.clone}
            ctx={ctx}
            x={x}
            y={y}
            size={34}
            label={{ ...node.label, x: lx, y: ly }}
            leaving={leaving}
            style={
              // While it animates, the per-frame fade replaces the CSS transition.
              { "--appear": presence, transition: presence < 1 ? "none" : undefined } as CSSProperties
            }
            className={cn(
              node.ring > 0 && "map-clone-old",
              skinClasses(node.clone, pr),
              ctx.peeling.has(node.clone.name) && "map-peeling",
              forkPullOn(ctx, node.clone.name) && "map-forking",
            )}
          >
            <CheckpointSkins count={node.clone.checkpoints ?? 0} />
            <span
              className="map-clone-symbol map-clone-symbol-wrap flex size-[34px] items-center justify-center rounded-full"
              style={{
                // Rose while pulled, and still rose as a ripped one flies off.
                ...(off && pullTint(pullOn(ctx, node.clone.name)?.strain ?? 1)),
                transform: flight
                  ? `rotate(${flight.spin}deg)`
                  : presence < 1
                    ? `scale(${0.25 + 0.75 * presence})`
                    : undefined,
              }}
            >
              {/* Size follows the clone's own data, kept inside the 34px target. */}
              <ClovePiece
                length={Math.max(26, Math.min(32, cloveLength(node.clone)))}
                angle={(node.angle * 180) / Math.PI + 90}
                cut={!!node.clone.dirty && node.clone.mounted}
              />
            </span>
            {pr?.state === "merged" && <MergedBadge className="top-0 right-0" />}
          </CloneNode>
        );
      })}
    </>
  );
}

/** A pulled clove turns rose as it strains. Inline, so no skin rule outranks it. */
function pullTint(strain: number): CSSProperties {
  const p = Math.round(strain * 100);
  return {
    color: `color-mix(in oklch, var(--rose) ${p}%, var(--bottle))`,
    background: `color-mix(in oklch, var(--rose) ${Math.round(strain * 22)}%, var(--bottle-soft))`,
    boxShadow: `0 0 0 ${4 + strain * 4}px color-mix(in oklch, var(--rose) ${Math.round(strain * 30)}%, transparent)`,
  };
}

/**
 * A new clove being pulled out of the golden image: it grows from the image's
 * edge as the pull strains, and an edge ties it to the image.
 */
function Sprout({ ctx, cx, cy }: { ctx: MapCtx; cx: number; cy: number }) {
  const pull = ctx.pull;
  if (pull?.target.kind !== "image") return null;
  const x = pull.from.x + pull.dx;
  const y = pull.from.y + pull.dy;
  const dx = x - cx;
  const dy = y - cy;
  const len = Math.hypot(dx, dy) || 1;
  const ux = dx / len;
  const uy = dy / len;
  // It sits at least on the image's rim, so it's seen coming out of it.
  const rim = CORE_RADIUS - 8;
  const out = Math.max(len, rim);
  const sx = cx + ux * out;
  const sy = cy + uy * out;
  const grown = 0.35 + 0.65 * pull.strain;
  return (
    <>
      <svg className="pointer-events-none absolute inset-0 size-full overflow-visible" aria-hidden="true">
        {out > CORE_RADIUS + 4 && (
          // The same sweep as a clone's edge (see the edges above), on the current ring's bend.
          <path
            className={cn("map-sprout-edge", pull.torn && "map-sprout-edge-free")}
            fill="none"
            d={`M ${cx + ux * CORE_RADIUS} ${cy + uy * CORE_RADIUS} Q ${cx + ux * ((CORE_RADIUS + out - 14) / 2) - uy * 8} ${cy + uy * ((CORE_RADIUS + out - 14) / 2) + ux * 8} ${sx - ux * 14} ${sy - uy * 14}`}
          />
        )}
      </svg>
      <span
        className={cn("map-sprout map-piece map-skin-peeled absolute", pull.torn && "map-sprout-free")}
        style={{ left: sx - 17, top: sy - 17, transform: `scale(${grown})` }}
        aria-hidden="true"
      >
        <span className="map-sprout-symbol flex size-[34px] items-center justify-center rounded-full">
          <ClovePiece length={30} angle={(Math.atan2(uy, ux) * 180) / Math.PI + 90} />
        </span>
        {pull.torn && <span className="map-sprout-hint">{pull.held ? "growing…" : "let go for a new clone"}</span>}
      </span>
    </>
  );
}

/** A fork's stem from its parent clove: the image edges' gentle sweep, trimmed clear of both cloves. */
function stemPath(px: number, py: number, x: number, y: number) {
  const dx = x - px;
  const dy = y - py;
  const len = Math.hypot(dx, dy) || 1;
  const ux = dx / len;
  const uy = dy / len;
  const trim = Math.min(18, len / 3);
  const bend = Math.min(14, len / 4);
  const mx = (px + x) / 2 - uy * bend;
  const my = (py + y) / 2 + ux * bend;
  return `M ${px + ux * trim} ${py + uy * trim} Q ${mx} ${my} ${x - ux * trim} ${y - uy * trim}`;
}

/**
 * A pale copy of a clove, pulled off it with Option/Alt held: it follows the
 * pointer on a stem back to its clone, which stays put. Let go past tearing
 * and it waits there for its fork to exist.
 */
function ForkCopy({ pull, home, angle }: { pull: Pull; home: readonly number[]; angle: number }) {
  const [hx, hy] = home;
  const x = hx + pull.dx;
  const y = hy + pull.dy;
  const grown = 0.55 + 0.45 * pull.strain;
  return (
    <>
      <svg className="pointer-events-none absolute inset-0 size-full overflow-visible" aria-hidden="true">
        {Math.hypot(pull.dx, pull.dy) > 20 && (
          <path
            className={cn("map-sprout-edge map-fork-stem", pull.torn && "map-sprout-edge-free")}
            fill="none"
            d={stemPath(hx, hy, x, y)}
          />
        )}
      </svg>
      <span
        className={cn("map-sprout map-fork-copy map-piece map-skin-peeled absolute", pull.torn && "map-sprout-free")}
        style={{ left: x - 17, top: y - 17, transform: `scale(${grown})` }}
        aria-hidden="true"
      >
        <span className="map-sprout-symbol flex size-[34px] items-center justify-center rounded-full">
          <ClovePiece length={30} angle={(angle * 180) / Math.PI + 90} />
        </span>
        <span className="map-sprout-hint">
          {pull.held ? "forking…" : pull.torn ? "let go to fork" : "⌥ pull to fork"}
        </span>
      </span>
    </>
  );
}
