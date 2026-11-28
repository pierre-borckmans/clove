// The golden image as a garlic bulb seen from below, in line art: a lobed
// outline (one bulge per clove pressing the skin), skin lines running from
// the root plate to the rim along each lobe, a knotty root plate in the
// middle, and faint contours for the image's generations. Everything is
// seeded, so it never jitters between renders.

import { useMemo } from "react";
import { C_PATH, CLOVE_PATH } from "@/components/glyph";
import { cn } from "@/lib/utils";
import { MAX_AGE } from "./geometry";

/** The bulb's rim, at its widest. */
export const BULB_R = 72;
const LOBES = 9;
/** The clove mark at the centre: its size, the room it keeps from the innermost contour, and below what it hides. */
const MARK = 38;
const MARK_MARGIN = 6;
const MARK_MIN = 14;
/** The innermost a generation's contour goes. */
const PLATE_R = 15;

/** A small deterministic random sequence. */
export function seeded(seed: number) {
  let a = (seed * 2654435761) >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** The bulb's shape for a seed: the rim's radius at any angle, and where its lobes are. */
export function bulbShape(seed: number) {
  const rnd = seeded(seed);
  const turn = rnd() * Math.PI * 2;
  const amp = Array.from({ length: LOBES }, () => 0.8 + rnd() * 0.4);
  const width = (Math.PI * 2) / LOBES;
  /** Which lobe an angle falls in, and how far across it (0 to 1). */
  const lobeAt = (theta: number) => {
    const t = (((theta - turn) % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2);
    const k = Math.floor(t / width);
    return { k, u: t / width - k };
  };
  const rim = (theta: number) => {
    const { k, u } = lobeAt(theta);
    // A bulge mid-lobe, a crease where two lobes meet.
    const bulge = Math.sin(Math.PI * u) ** 0.65;
    return BULB_R * (0.86 + 0.14 * bulge * amp[k]);
  };
  return { turn, width, rim, lobeAt };
}
export type BulbShape = ReturnType<typeof bulbShape>;

/**
 * Where a generation's contour sits along `theta`: the newest at the rim,
 * each older one deeper toward the root plate (MAX_AGE deepest).
 */
export function contourRadius(shape: BulbShape, age: number, theta: number) {
  const f = 1 - Math.min(age, MAX_AGE) * 0.2;
  return PLATE_R + (shape.rim(theta) - PLATE_R) * f;
}

/** A generation's contour as a path: the rim itself for the current image. */
export function contourPath(shape: BulbShape, age: number) {
  return closed((t) => contourRadius(shape, age, t));
}

const fmt = (n: number) => n.toFixed(2);
function closed(r: (theta: number) => number, n = 216) {
  const pts = Array.from({ length: n }, (_, i) => {
    const a = (i / n) * Math.PI * 2;
    return `${fmt(Math.cos(a) * r(a))} ${fmt(Math.sin(a) * r(a))}`;
  });
  return `M ${pts.join(" L ")} Z`;
}

/** Skin lines: two per lobe (its crease, and one across it), only over the outer part, bowing with the lobe. */
function meridians(shape: BulbShape, seed: number) {
  const rnd = seeded(seed * 31 + 7);
  const out: string[] = [];
  for (let k = 0; k < LOBES; k++) {
    for (const u of [0, 0.3 + rnd() * 0.4]) {
      const a = shape.turn + (k + u) * shape.width;
      const mid = shape.turn + (k + 0.5) * shape.width;
      const r1 = shape.rim(a) - 1.2;
      const r0 = r1 * (0.6 + rnd() * 0.08);
      const bow = u === 0 ? 0 : (mid - a) * 0.15;
      const rm = (r0 + r1) / 2;
      const am = a + bow;
      out.push(
        `M ${fmt(Math.cos(a) * r0)} ${fmt(Math.sin(a) * r0)} Q ${fmt(Math.cos(am) * rm)} ${fmt(Math.sin(am) * rm)} ${fmt(Math.cos(a) * r1)} ${fmt(Math.sin(a) * r1)}`,
      );
    }
  }
  return out.join(" ");
}

/**
 * The drawing, centred on 0,0: the lobed outline and quiet skin lines. The
 * generations' contours are drawn and made interactive by the map (svg-map's
 * LayerButtons). `breathRef` wraps what breathes while the image refreshes.
 */
export function BulbArt({
  seed,
  deepest = 0,
  breathRef,
  className,
}: {
  seed: number;
  /** The oldest generation drawn inside: the mark shrinks to fit within its contour. */
  deepest?: number;
  breathRef?: React.RefObject<SVGGElement | null>;
  className?: string;
}) {
  const art = useMemo(() => {
    const shape = bulbShape(seed);
    return { shape, outline: closed(shape.rim), lines: meridians(shape, seed) };
  }, [seed]);
  // The mark fits inside the innermost contour with a margin; too small, it goes.
  const inner = Math.min(
    ...Array.from({ length: 72 }, (_, i) => contourRadius(art.shape, deepest, (i / 72) * Math.PI * 2)),
  );
  const mark = Math.min(MARK, 2 * inner - 2 * MARK_MARGIN);
  return (
    <g className={cn("map-bulb-art", className)}>
      <g ref={breathRef}>
        <path className="map-image-disc map-bulb-outline" d={art.outline} />
        <path className="map-bulb-line" d={art.lines} />
        {mark >= MARK_MIN && (
          // The clove mark at the heart of the bulb: part of its button, not one of its own.
          <g className="map-bulb-mark" transform={`scale(${mark / 100}) translate(-50 -50)`}>
            <path d={C_PATH} fill="none" strokeWidth="19" strokeLinecap="round" />
            <path d={CLOVE_PATH} transform="translate(53 51) rotate(16) scale(0.33) translate(-50 -49)" />
          </g>
        )}
      </g>
    </g>
  );
}

/** The bulb's rim as a path, for the skin that forms round it while the image refreshes. */
export const rimPath = (seed: number) => closed(bulbShape(seed).rim);
