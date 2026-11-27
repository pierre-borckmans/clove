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
  spotsAround,
} from "./geometry";
import { CloneNode, ClovePiece, isDimmed, type MapCtx, MergedBadge, skinClasses } from "./nodes";

/** Head radius by age: the current image is the biggest, older ones shrink. */
const HEAD_R = [60, 40, 33, 29];
const GROUP_GAP = 96;
const PAD = 36;
/** Cloves fan above and below a head, leaving its sides to the braid. */
const ARCS: [number, number][] = [
  [-162, -18],
  [18, 162],
];
const rad = (deg: number) => (deg * Math.PI) / 180;

/** A head's silhouette radius at angle θ: a pointed neck on top, a flatter base. */
function rim(r: number, theta: number) {
  const s = Math.sin(theta);
  return s < 0 ? r * (1 + 0.28 * Math.abs(s) ** 3) : r * (0.98 - 0.06 * s * s);
}

type Piece = {
  clone: CloneInfo;
  x: number;
  y: number;
  angle: number;
  length: number;
  label: LabelSpot;
  leader: [number, number, number, number] | null;
};

/** Each image generation is a garlic head on a braid, oldest left, current image right. */
export function braidLayout(clones: CloneInfo[], newest: number, labelOf: (c: CloneInfo) => { w: number; h: number }) {
  const ages = [...new Set([0, ...clones.map((c) => ageOf(newest, c.golden_generation))])].sort((a, b) => b - a);
  let cursor = 0;
  let previousRight = 0;
  const heads = ages.map((age, index) => {
    const r = HEAD_R[age];
    const members = clones
      .filter((c) => ageOf(newest, c.golden_generation) === age)
      .sort((a, b) => b.created_at - a.created_at);
    const generations = [...new Set(members.map((c) => c.golden_generation))].sort((a, b) => b - a);
    const longest = Math.max(30, ...members.map(cloveLength));
    // Newest clone on top, then alternate below and above; fill a row, then the next one out.
    const sides = [members.filter((_, i) => i % 2 === 0), members.filter((_, i) => i % 2 === 1)];
    const pieces: Omit<Piece, "label" | "leader">[] = [];
    sides.forEach((list, side) => {
      const [a0, a1] = ARCS[side];
      let row = 0;
      let placed = 0;
      while (placed < list.length) {
        const reach = r + 6 + longest / 2 + row * (longest + 6);
        const capacity = Math.max(1, Math.floor((rad(a1 - a0) * reach) / (longest * 0.6 + 9)));
        const batch = list.slice(placed, placed + capacity);
        // A short row clusters around the middle rather than spreading to the stem.
        const span = Math.min(a1 - a0, (batch.length * (longest * 0.6 + 12) * 180) / (Math.PI * reach));
        const mid = (a0 + a1) / 2;
        batch.forEach((clone, j) => {
          const theta = rad(mid - span / 2 + (span * (j + 0.5)) / batch.length);
          const dist = rim(r, theta) + 6 + longest / 2 + row * (longest + 6);
          pieces.push({
            clone,
            angle: theta,
            length: cloveLength(clone),
            x: Math.cos(theta) * dist,
            y: Math.sin(theta) * dist,
          });
        });
        placed += batch.length;
        row++;
      }
    });
    const left = Math.max(r * 1.05, ...pieces.map((p) => -p.x + p.length / 2));
    const right = Math.max(r * 1.05, ...pieces.map((p) => p.x + p.length / 2));
    cursor += index === 0 ? 0 : previousRight + GROUP_GAP + left;
    previousRight = right;
    const cx = cursor;
    return {
      age,
      r,
      cx,
      cy: 0,
      generations: generations.length ? generations : [newest],
      pieces: pieces.map((p) => ({ ...p, x: p.x + cx })),
    };
  });

  // Labels go where nothing else is: heads and cloves first, then labels as they land.
  const obstacles: Box[] = [
    ...heads.map((h) => ({ x: h.cx - h.r * 1.02, y: -h.r * 1.3, w: h.r * 2.04, h: h.r * 2.38 })),
    ...heads.flatMap((h) => h.pieces.map((p) => centerBox(p.x, p.y, p.length * 0.8))),
  ];
  const pieces: Piece[] = [];
  for (const head of [...heads].sort((a, b) => a.age - b.age)) {
    for (const p of head.pieces) {
      const size = labelOf(p.clone);
      const ux = Math.cos(p.angle);
      const uy = Math.sin(p.angle);
      const label = firstFree(spotsAround(p.x, p.y, ux, uy, p.length / 2 + 2, size.w, size.h), obstacles);
      obstacles.push(label);
      const end = nearestOn(label, p.x, p.y);
      const far = Math.hypot(end.x - p.x, end.y - p.y) > p.length / 2 + 14;
      pieces.push({
        ...p,
        label,
        leader: far ? [p.x + ux * (p.length / 2), p.y + uy * (p.length / 2), end.x, end.y] : null,
      });
    }
  }

  const oldest = heads[0];
  const tail = oldest.cx - oldest.r - 70;
  const box = bounds([...obstacles, { x: tail, y: -10, w: 1, h: 20 }]);
  const dx = PAD - box.x0;
  const dy = PAD - box.y0;
  const contentWidth = box.x1 - box.x0 + PAD * 2;
  const width = contentWidth;
  const height = box.y1 - box.y0 + PAD * 2;
  const shift = 0;
  const move = <T extends { x: number; y: number }>(p: T): T => ({ ...p, x: p.x + dx + shift, y: p.y + dy });
  return {
    kind: "braid" as const,
    width,
    height,
    stem: { x0: tail + dx + shift, x1: heads[heads.length - 1].cx + dx + shift, y: dy },
    heads: heads.map((h) => ({ ...h, cx: h.cx + dx + shift, cy: dy, pieces: undefined })),
    pieces: pieces.map((p) => ({
      ...move(p),
      label: move(p.label),
      leader:
        p.leader && ([p.leader[0] + dx + shift, p.leader[1] + dy, p.leader[2] + dx + shift, p.leader[3] + dy] as const),
    })),
  };
}

/** Three strands twisting along the stem, tapering at the old end. */
function strands(x0: number, x1: number, y: number) {
  const period = 58;
  const amp = 5.5;
  return [0, 1, 2].map((k) => {
    let d = "";
    for (let x = x0; x <= x1; x += 3) {
      const taper = Math.min(1, (x - x0) / 50);
      const yy = y + amp * taper * Math.sin((2 * Math.PI * (x - x0)) / period + (k * 2 * Math.PI) / 3);
      d += `${d ? "L" : "M"}${x.toFixed(1)} ${yy.toFixed(1)}`;
    }
    return d;
  });
}

// A garlic head in unit space: pointed neck, round body, a flat basal plate with roots.
const HEAD_PATH =
  "M0 -1.3 C0.06 -1.06 0.22 -0.93 0.55 -0.75 C0.98 -0.5 1.06 0.2 0.86 0.55 C0.68 0.86 0.32 0.93 0 0.93 C-0.32 0.93 -0.68 0.86 -0.86 0.55 C-1.06 0.2 -0.98 -0.5 -0.55 -0.75 C-0.22 -0.93 -0.06 -1.06 0 -1.3Z";
const HEAD_SEAMS =
  "M0 -1.12 C-0.72 -0.78 -0.88 0.3 -0.48 0.9 M0 -1.12 C0.72 -0.78 0.88 0.3 0.48 0.9 M0 -1.12 C-0.98 -0.62 -1.02 0.36 -0.74 0.7 M0 -1.12 C0.98 -0.62 1.02 0.36 0.74 0.7";
const HEAD_ROOTS = "M-0.28 0.92 l-0.09 0.15 M-0.1 0.94 l-0.03 0.17 M0.1 0.94 l0.03 0.17 M0.28 0.92 l0.09 0.15";

export function BraidLayer({ layout, ctx }: { layout: ReturnType<typeof braidLayout>; ctx: MapCtx }) {
  return (
    <>
      <svg className="pointer-events-none absolute inset-0 size-full overflow-visible" aria-hidden="true">
        {strands(layout.stem.x0, layout.stem.x1, layout.stem.y).map((d, k) => (
          <path key={k} d={d} className={cn("map-braid", k === 1 && "map-braid-back")} fill="none" />
        ))}
        {layout.pieces.map(
          (p) =>
            p.leader && (
              <line
                key={p.clone.name}
                x1={p.leader[0]}
                y1={p.leader[1]}
                x2={p.leader[2]}
                y2={p.leader[3]}
                className={cn("map-leader", isDimmed(ctx, p.clone.golden_generation) && "map-dim")}
              />
            ),
        )}
      </svg>
      {layout.heads.map((h) => {
        const current = h.age === 0;
        const gens = h.generations;
        const label =
          h.age === MAX_AGE && gens.length > 1 ? `gen ${gens[gens.length - 1]}–${gens[0]}` : `gen ${gens.join(", ")}`;
        const generation = current ? (ctx.current ?? ctx.newest) : gens[0];
        const active = current
          ? ctx.imageGeneration === ctx.current
          : gens.includes(ctx.imageGeneration ?? -1) && ctx.imageGeneration !== ctx.current;
        const w = h.r * 2.2;
        const tall = h.r * 2.5;
        return (
          <button
            type="button"
            key={h.age}
            aria-label={current ? `Golden image, generation ${ctx.newest}` : `Clones from older image, ${label}`}
            aria-pressed={active}
            title={current ? "Current golden image" : h.age === 1 ? "Previous image" : `${h.age} images ago`}
            onClick={() => ctx.select({ kind: "image", generation })}
            className={cn(
              "map-head absolute flex flex-col items-center justify-center text-center",
              !current && "map-head-old",
              active && "map-node-selected",
              ctx.focusAge !== null && ctx.focusAge !== h.age && "map-dim",
            )}
            style={{ left: h.cx - w / 2, top: h.cy - h.r * 1.32, width: w, height: tall }}
          >
            <svg
              viewBox="-1.1 -1.32 2.2 2.5"
              className="absolute inset-0 size-full overflow-visible"
              aria-hidden="true"
            >
              <path className="map-head-shape" d={HEAD_PATH} vectorEffect="non-scaling-stroke" />
              <path className="map-head-seam" d={HEAD_SEAMS} fill="none" vectorEffect="non-scaling-stroke" />
              <path className="map-head-seam" d={HEAD_ROOTS} fill="none" vectorEffect="non-scaling-stroke" />
            </svg>
            <span className="relative mt-[6%] flex flex-col items-center">
              {current && <span className="text-[12px] leading-tight font-medium">Golden image</span>}
              <span className={cn("font-mono text-muted-foreground", current ? "mt-0.5 text-[10px]" : "text-[10px]")}>
                {current ? `generation ${ctx.newest}` : label}
              </span>
            </span>
          </button>
        );
      })}
      {layout.pieces.map((p) => {
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
