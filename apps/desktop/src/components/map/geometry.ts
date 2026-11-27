import type { CloneInfo, OtherWorktree, Pr } from "@/lib/types";

/** A world-space rectangle; x/y is the top-left corner. */
export type Box = { x: number; y: number; w: number; h: number };
export type Align = "left" | "right" | "center";
/** A label's box and which edge its text hugs. */
export type LabelSpot = Box & { align: Align };

/** Generations past this many images ago share the oldest group. */
export const MAX_AGE = 3;
export const ageOf = (newest: number, generation: number) => Math.min(Math.max(0, newest - generation), MAX_AGE);

export const LABEL_H = 40;
const LABEL_MAX = 200;
const SANS = '-apple-system, BlinkMacSystemFont, "SF Pro Text", "Inter", "Helvetica Neue", system-ui, sans-serif';
const MONO = 'ui-monospace, "SF Mono", "JetBrains Mono", Menlo, monospace';

let measure: CanvasRenderingContext2D | null | undefined;
function textWidth(text: string, font: string) {
  measure ??= typeof document === "undefined" ? null : document.createElement("canvas").getContext("2d");
  if (!measure) return text.length * 7;
  measure.font = font;
  return measure.measureText(text).width;
}

export type StatePart = { text: string; tone?: "amber" | "sage" | "rose"; arrow?: "up" | "down" };

/** The small line under a clone's name: modified, ↑↓, #PR. */
export function cloneState(c: CloneInfo, pr: Pr | null | undefined): StatePart[] {
  if (c.failed_steps.length || c.issue || c.operation) {
    const text = c.operation ? `${c.operation}: conflicts` : c.issue || `${c.failed_steps.join(", ")} failed`;
    return [{ text, tone: "amber" }];
  }
  if (!c.mounted) return [{ text: "unmounted" }];
  const parts: StatePart[] = [];
  if (pr?.state === "merged") parts.push({ text: "merged", tone: "sage" });
  if (c.dirty) parts.push({ text: "modified", tone: "amber" });
  else if (!c.ahead && pr?.state !== "merged") parts.push({ text: "clean" });
  if (c.ahead) parts.push({ text: String(c.ahead), arrow: "up" });
  if (c.behind) parts.push({ text: String(c.behind), arrow: "down" });
  if (pr?.state === "open") parts.push({ text: `#${pr.number}` });
  return parts;
}

export function worktreeState(w: OtherWorktree): StatePart[] {
  if (w.missing) return [{ text: "folder missing" }];
  if (w.locked) return [{ text: "locked" }];
  if (w.dirty) return [{ text: "modified", tone: "amber" }];
  return [{ text: "not a clone" }];
}

/** How much room a name and its state line take, measured in the map's fonts. */
export function labelSize(name: string, state: StatePart[], tag?: string) {
  const nameW = textWidth(name, `500 12px ${SANS}`) + 12 + (tag ? textWidth(tag, `10px ${MONO}`) + 18 : 0);
  const stateW =
    state.reduce((n, p) => n + textWidth(p.text, `10.5px ${SANS}`) + (p.arrow ? 10 : 0), 0) +
    8 * (state.length - 1) +
    6;
  return { w: Math.min(LABEL_MAX, Math.ceil(Math.max(nameW, stateW)) + 2), h: LABEL_H };
}

/** A clove's length from the data only it holds: 22 to 42 px, 30 when unknown. */
export function cloveLength(c: CloneInfo) {
  if (c.own_bytes === undefined) return 30;
  const mb = Math.max(8, c.own_bytes / 2 ** 20);
  const t = Math.min(1, (Math.log2(mb) - 3) / 9);
  return Math.round(22 + t * 20);
}

export const overlaps = (a: Box, b: Box, pad = 4) =>
  a.x < b.x + b.w + pad && b.x < a.x + a.w + pad && a.y < b.y + b.h + pad && b.y < a.y + a.h + pad;

/** The first candidate clear of every obstacle, else the last one tried. */
export function firstFree<T extends Box>(candidates: T[], obstacles: Box[]): T {
  return candidates.find((c) => !obstacles.some((o) => overlaps(c, o))) ?? candidates[candidates.length - 1];
}

/** A label box hung off point (x, y) towards direction (ux, uy). */
export function spotToward(x: number, y: number, ux: number, uy: number, w: number, h: number): LabelSpot {
  if (ux > 0.4) return { x, y: y - h / 2, w, h, align: "left" };
  if (ux < -0.4) return { x: x - w, y: y - h / 2, w, h, align: "right" };
  return { x: x - w / 2, y: uy < 0 ? y - h : y, w, h, align: "center" };
}

/** Candidate label spots around a node, nearest and most outward first. */
export function spotsAround(x: number, y: number, ux: number, uy: number, start: number, w: number, h: number) {
  const dirs: [number, number][] = [
    [ux, uy],
    [1, 0],
    [-1, 0],
    [0, uy < 0 ? -1 : 1],
    [0, uy < 0 ? 1 : -1],
  ];
  const spots: LabelSpot[] = [];
  for (let step = 0; step < 9; step++) {
    const d = start + step * 14;
    for (const [dx, dy] of dirs) spots.push(spotToward(x + dx * d, y + dy * d, dx, dy, w, h));
  }
  return spots;
}

export const centerBox = (x: number, y: number, w: number, h = w): Box => ({ x: x - w / 2, y: y - h / 2, w, h });

export function bounds(boxes: Box[]) {
  const x0 = Math.min(...boxes.map((b) => b.x));
  const y0 = Math.min(...boxes.map((b) => b.y));
  const x1 = Math.max(...boxes.map((b) => b.x + b.w));
  const y1 = Math.max(...boxes.map((b) => b.y + b.h));
  return { x0, y0, x1, y1 };
}

/** The nearest point of a box to (x, y), for leader lines. */
export function nearestOn(box: Box, x: number, y: number) {
  return { x: Math.min(box.x + box.w, Math.max(box.x, x)), y: Math.min(box.y + box.h, Math.max(box.y, y)) };
}
