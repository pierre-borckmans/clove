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
import { CloneNode, ClovePiece, isDimmed, type MapCtx, MergedBadge, menuTrigger, skinClasses } from "./nodes";
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
    clones.filter((c) => ringOf(c.golden_generation) === age).sort((a, b) => b.created_at - a.created_at),
  );

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
  let cursor = -Math.PI / 2 + gap / 2;
  const placed = groups.flatMap((group, ring) => {
    const r = radiusOf(ring);
    const step = SPACING / r + (clones.length ? spare / clones.length : 0);
    return group.map((clone) => {
      const angle = cursor + step / 2;
      cursor += step;
      return { clone, angle, ring, x: cx + Math.cos(angle) * r * STRETCH, y: cy + Math.sin(angle) * r };
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
        {nodes.map(({ id, values: [x, y], data: node, presence, leaving }) => {
          // A leaving clove lets go of its edge as it flies off.
          if (leaving) return null;
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
          return (
            <path
              key={id}
              d={`M ${cx + ux * CORE_RADIUS} ${cy + uy * CORE_RADIUS} Q ${cx + ux * mid - uy * bend} ${cy + uy * mid + ux * bend} ${x - ux * 18} ${y - uy * 18}`}
              fill="none"
              style={{ "--appear": presence } as CSSProperties}
              className={cn(
                "map-edge",
                node.ring > 0 && "map-edge-old",
                lit && "map-edge-active",
                isDimmed(ctx, node.clone.golden_generation) && "map-dim",
              )}
            />
          );
        })}
        {nodes.map(
          ({ id, values, data: node, presence, leaving }) =>
            node.leader &&
            !leaving && (
              <line
                key={`leader-${id}`}
                x1={values[4]}
                y1={values[5]}
                x2={values[6]}
                y2={values[7]}
                style={{ "--appear": presence } as CSSProperties}
                className={cn("map-leader", isDimmed(ctx, node.clone.golden_generation) && "map-dim")}
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
        )}
        style={{ left: cx - 72, top: cy - 72 }}
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
      {nodes.map(({ id, values: [x0, y0, lx, ly], data: node, presence, leaving }) => {
        const pr = ctx.prOf(node.clone.name);
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
            className={cn(node.ring > 0 && "map-clone-old", skinClasses(node.clone, pr))}
          >
            <span
              className="map-clone-symbol flex size-[34px] items-center justify-center rounded-full"
              style={{
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
