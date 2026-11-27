import type { CloneInfo } from "@/lib/types";
import { cn } from "@/lib/utils";
import {
  ageOf,
  type Box,
  bounds,
  centerBox,
  cloveLength,
  firstFree,
  LABEL_H,
  type LabelSpot,
  nearestOn,
  spotsAround,
} from "./geometry";
import { CloneNode, ClovePiece, isDimmed, type MapCtx, MergedBadge, skinClasses } from "./nodes";

const CORE = 52;
const PAD = 36;
const GAP = 4;
type Point = { x: number; y: number };
const polar = (cx: number, cy: number, r: number, a: number) => ({ x: cx + Math.cos(a) * r, y: cy + Math.sin(a) * r });

/** One clove's slice of the bulb, inset so neighbours keep a gap. */
function wedgePath(cx: number, cy: number, ri: number, ro: number, a: number, b: number) {
  const inset = (r: number) => Math.asin(GAP / 2 / r);
  const p1 = polar(cx, cy, ri, a + inset(ri));
  const p2 = polar(cx, cy, ro, a + inset(ro));
  const p3 = polar(cx, cy, ro, b - inset(ro));
  const p4 = polar(cx, cy, ri, b - inset(ri));
  const large = b - a > Math.PI ? 1 : 0;
  return `M${p1.x} ${p1.y} L${p2.x} ${p2.y} A${ro} ${ro} 0 ${large} 1 ${p3.x} ${p3.y} L${p4.x} ${p4.y} A${ri} ${ri} 0 ${large} 0 ${p1.x} ${p1.y}Z`;
}

/** Spreads a column of labels so none overlap, staying near where each wants to be. */
function stack(wanted: number[]) {
  const ys = [...wanted];
  for (let i = 1; i < ys.length; i++) ys[i] = Math.max(ys[i], ys[i - 1] + LABEL_H + 4);
  const overflow = ys.length ? ys[ys.length - 1] - wanted[wanted.length - 1] : 0;
  return ys.map((y) => y - overflow / 2);
}

/** The current image as a sliced bulb; clones of older images lie loose around it. */
export function bulbLayout(
  clones: CloneInfo[],
  newest: number,
  labelOf: (c: CloneInfo, tag?: string) => { w: number; h: number },
) {
  const inner = clones
    .filter((c) => ageOf(newest, c.golden_generation) === 0)
    .sort((a, b) => b.created_at - a.created_at);
  const loose = clones
    .filter((c) => ageOf(newest, c.golden_generation) > 0)
    .sort((a, b) => b.golden_generation - a.golden_generation || b.created_at - a.created_at);
  const R = Math.min(210, 132 + Math.max(0, inner.length - 4) * 9);
  const cx = 0;
  const cy = 0;

  // Slices are sized by each clone's own data, newest first, clockwise from the top.
  const weights = inner.map(cloveLength);
  const total = weights.reduce((n, w) => n + w, 0) || 1;
  let start = -Math.PI / 2;
  const wedges = inner.map((clone, i) => {
    const sweep = (weights[i] / total) * Math.PI * 2;
    const a = start;
    start += sweep;
    return { clone, a, b: a + Math.min(sweep, Math.PI * 2 - 0.02), mid: a + sweep / 2 };
  });

  // Labels in two columns beside the bulb, joined by leader lines.
  const columnGap = 34;
  const labelled = wedges.map((w) => {
    const size = labelOf(w.clone);
    const right = Math.cos(w.mid) > 1e-6 || (Math.abs(Math.cos(w.mid)) <= 1e-6 && Math.sin(w.mid) < 0);
    return { ...w, size, right, want: cy + Math.sin(w.mid) * (R + 18) - LABEL_H / 2 };
  });
  const placedWedges: ((typeof labelled)[number] & { label: LabelSpot; leader: Point[] })[] = [];
  for (const right of [true, false]) {
    const column = labelled.filter((w) => w.right === right).sort((a, b) => a.want - b.want);
    const ys = stack(column.map((w) => w.want));
    column.forEach((w, i) => {
      const x = right ? cx + R + columnGap : cx - R - columnGap - w.size.w;
      const label: LabelSpot = { x, y: ys[i], w: w.size.w, h: LABEL_H, align: right ? "left" : "right" };
      const rim = polar(cx, cy, R + 3, w.mid);
      const out = polar(cx, cy, R + 12, w.mid);
      const elbowX = right ? Math.max(out.x, cx + R + 18) : Math.min(out.x, cx - R - 18);
      const ly = ys[i] + 10;
      const lx = right ? x - 3 : x + w.size.w + 3;
      placedWedges.push({ ...w, label, leader: [rim, out, { x: elbowX, y: ly }, { x: lx, y: ly }] });
    });
  }

  // Older clones scatter above and below, further out the older they are.
  const pieces = loose.map((clone, i) => ({ clone, age: ageOf(newest, clone.golden_generation), top: i % 2 === 0 }));
  const rows = new Map<string, typeof pieces>();
  for (const p of pieces) {
    const key = `${p.top}-${p.age}`;
    rows.set(key, [...(rows.get(key) ?? []), p]);
  }
  const scattered: { clone: CloneInfo; age: number; x: number; y: number; angle: number; length: number }[] = [];
  for (const row of rows.values()) {
    const { top, age } = row[0];
    const reach = R + 34 + (age - 1) * 50;
    const spread = Math.min(R * 0.8, (row.length - 1) * 70);
    row.forEach((p, j) => {
      const x = row.length === 1 ? (age % 2 ? -0.32 : 0.32) * R : -spread + (2 * spread * j) / (row.length - 1);
      const y = (top ? -1 : 1) * Math.sqrt(Math.max(0, reach * reach - x * x));
      scattered.push({ ...p, x: cx + x, y: cy + y, angle: Math.atan2(y, x), length: cloveLength(p.clone) });
    });
  }

  const obstacles: Box[] = [
    centerBox(cx, cy, R * 2 + 8),
    ...placedWedges.map((w) => w.label),
    ...scattered.map((p) => centerBox(p.x, p.y, p.length * 0.8)),
  ];
  const loosePieces = scattered.map((p) => {
    const tag = `gen ${p.clone.golden_generation}`;
    const size = labelOf(p.clone, tag);
    const ux = Math.cos(p.angle);
    const uy = Math.sin(p.angle);
    // Beside the clove first, so the scatter stays low.
    const side = ux >= 0 ? 1 : -1;
    const spots = [
      ...spotsAround(p.x, p.y, side, 0, p.length / 2 + 2, size.w, size.h).slice(0, 2),
      ...spotsAround(p.x, p.y, ux, uy, p.length / 2 + 2, size.w, size.h),
    ];
    const label = firstFree(spots, obstacles);
    obstacles.push(label);
    const end = nearestOn(label, p.x, p.y);
    const far = Math.hypot(end.x - p.x, end.y - p.y) > p.length / 2 + 14;
    const tip = { x: p.x + ux * (p.length / 2), y: p.y + uy * (p.length / 2) };
    return { ...p, tag, label, leader: far ? [tip, end] : null };
  });

  const box = bounds(obstacles);
  const dx = PAD - box.x0;
  const dy = PAD - box.y0;
  const width = box.x1 - box.x0 + PAD * 2;
  const height = box.y1 - box.y0 + PAD * 2;
  const move = <T extends Point>(p: T): T => ({ ...p, x: p.x + dx, y: p.y + dy });
  return {
    kind: "bulb" as const,
    width,
    height,
    cx: cx + dx,
    cy: cy + dy,
    R,
    wedges: placedWedges.map((w) => ({ ...w, label: move(w.label), leader: w.leader.map(move) })),
    loose: loosePieces.map((p) => ({ ...move(p), label: move(p.label), leader: p.leader?.map(move) ?? null })),
  };
}

const polyline = (points: Point[]) => points.map((p, i) => `${i ? "L" : "M"}${p.x} ${p.y}`).join(" ");

export function BulbLayer({ layout, ctx }: { layout: ReturnType<typeof bulbLayout>; ctx: MapCtx }) {
  const { cx, cy, R } = layout;
  const ri = CORE + 8;
  const frame = R + 10;
  const currentActive = ctx.imageGeneration === ctx.current;
  return (
    <>
      <svg className="pointer-events-none absolute inset-0 size-full overflow-visible" aria-hidden="true">
        <circle className="map-bulb-skin" cx={cx} cy={cy} r={R + 7} />
        {layout.wedges.map((w) => (
          <path
            key={w.clone.name}
            d={polyline(w.leader)}
            fill="none"
            className={cn(
              "map-leader",
              (ctx.picked === w.clone.name || ctx.hovered === w.clone.name) && "map-leader-active",
              isDimmed(ctx, w.clone.golden_generation) && "map-dim",
            )}
          />
        ))}
        {layout.loose.map(
          (p) =>
            p.leader && (
              <path
                key={p.clone.name}
                d={polyline(p.leader)}
                fill="none"
                className={cn("map-leader", isDimmed(ctx, p.clone.golden_generation) && "map-dim")}
              />
            ),
        )}
      </svg>
      {layout.wedges.map((w) => {
        const pr = ctx.prOf(w.clone.name);
        const vein = [polar(cx, cy, ri + 10, w.mid), polar(cx, cy, ri + (R - ri) * 0.42, w.mid)];
        const cut = polar(cx, cy, R - 16, w.mid);
        const tangent = w.mid + Math.PI / 2;
        const badge = polar(cx, cy, R - 15, w.mid + Math.min(0.5, (w.b - w.a) / 3));
        return (
          <CloneNode
            key={w.clone.name}
            clone={w.clone}
            ctx={ctx}
            x={cx}
            y={cy}
            size={frame * 2}
            label={w.label}
            className={cn("map-wedge", skinClasses(w.clone, pr))}
          >
            <svg
              viewBox={`${cx - frame} ${cy - frame} ${frame * 2} ${frame * 2}`}
              className="absolute inset-0 size-full overflow-visible"
              aria-hidden="true"
            >
              <path className="clove-skin" d={wedgePath(cx, cy, ri, R, w.a, w.b)} strokeLinejoin="round" />
              <path
                className="clove-vein"
                d={`M${vein[0].x} ${vein[0].y} L${vein[1].x} ${vein[1].y}`}
                fill="none"
                strokeLinecap="round"
              />
              {w.clone.dirty && w.clone.mounted && (
                <path
                  className="clove-cut"
                  d={`M${cut.x - Math.cos(tangent) * 7} ${cut.y - Math.sin(tangent) * 7} L${cut.x + Math.cos(tangent) * 7} ${cut.y + Math.sin(tangent) * 7}`}
                  strokeLinecap="round"
                />
              )}
            </svg>
            {pr?.state === "merged" && (
              <MergedBadge
                className="-translate-x-1/2 -translate-y-1/2"
                style={{ left: badge.x - cx + frame, top: badge.y - cy + frame }}
              />
            )}
          </CloneNode>
        );
      })}
      <button
        type="button"
        aria-label={`Golden image, generation ${ctx.newest}`}
        aria-pressed={currentActive}
        className={cn(
          "map-bulb-core absolute flex flex-col items-center justify-center rounded-full text-center",
          currentActive && "map-node-selected",
          ctx.focusAge !== null && ctx.focusAge !== 0 && "map-dim",
        )}
        style={{ left: cx - CORE, top: cy - CORE, width: CORE * 2, height: CORE * 2 }}
        onClick={() => ctx.current !== undefined && ctx.select({ kind: "image", generation: ctx.current })}
      >
        <span className="text-[12px] leading-tight font-medium">Golden image</span>
        <span className="mt-0.5 font-mono text-[10px] text-muted-foreground">generation {ctx.newest}</span>
      </button>
      {layout.loose.map((p) => {
        const pr = ctx.prOf(p.clone.name);
        return (
          <CloneNode
            key={p.clone.name}
            clone={p.clone}
            ctx={ctx}
            x={p.x}
            y={p.y}
            size={p.length}
            label={p.label}
            tag={p.tag}
            className={skinClasses(p.clone, pr)}
          >
            <span className="flex size-full items-center justify-center">
              <ClovePiece
                length={p.length}
                angle={(p.angle * 180) / Math.PI + 90}
                cut={!!p.clone.dirty && p.clone.mounted}
              />
            </span>
            {pr?.state === "merged" && <MergedBadge className="-top-1 -right-1" />}
          </CloneNode>
        );
      })}
    </>
  );
}
